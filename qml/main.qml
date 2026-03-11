import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.platform as Platform
import NixosUpdateChecker
import Terminal 1.0

ApplicationWindow {
    id: root
    visible: false  // Start hidden - show via tray menu
    width: 975
    height: 700
    minimumWidth: 900
    minimumHeight: 400
    title: "NixOS Update Checker"
    flags: Qt.Dialog | Qt.WindowStaysOnTopHint

    property bool outputExpanded: false
    property bool packageListExpanded: false
    property bool updateJustCompleted: false  // Track if update just finished successfully
    property string lastLine: ""  // Last non-empty line from terminal output

    // Progress tracking for update process
    QtObject {
        id: progressInfo
        property real progress: 0.0
        property string statusText: ""
        property string progressText: ""

        function parseProgress(line) {
            if (!line) {
                return
            }

            // Parse nix build progress patterns from progress-bar.cc:
            // Format: [running/done/expected built] when builds are running
            //         [done/expected built] when no running builds
            //         [done built] when complete
            // Copied: "256 copied" or "256 copied (123.4/500.0 MiB)"
            // Download: "1115.9 MiB DL"
            var builtMatch = line.match(/\[(\d+)(?:\/(\d+))?(?:\/(\d+))?\s+built/)
            var copiedMatch = line.match(/(\d+)\s+copied/)
            var copiedSizeMatch = line.match(/copied\s+\(([\d.]+)\/([\d.]+)\s*MiB\)/)
            var dlMatch = line.match(/([\d.]+)\s*MiB\s+DL/)

            if (builtMatch) {
                // Parse the built numbers based on how many are present
                // 3 numbers: running/done/expected -> done=match[2], expected=match[3]
                // 2 numbers: done/expected -> done=match[1], expected=match[2]
                // 1 number: done -> done=match[1], expected=done (complete)
                var done, expected
                if (builtMatch[3]) {
                    // 3 numbers: running/done/expected
                    done = parseInt(builtMatch[2])
                    expected = parseInt(builtMatch[3])
                } else if (builtMatch[2]) {
                    // 2 numbers: done/expected
                    done = parseInt(builtMatch[1])
                    expected = parseInt(builtMatch[2])
                } else {
                    // 1 number: done (complete)
                    done = parseInt(builtMatch[1])
                    expected = done
                }

                var copied = copiedMatch ? parseInt(copiedMatch[1]) : 0
                var copiedSize = 0, copiedTotal = 0
                if (copiedSizeMatch) {
                    copiedSize = parseFloat(copiedSizeMatch[1])
                    copiedTotal = parseFloat(copiedSizeMatch[2])
                }

                // Calculate combined progress (build phase is 70%, copy/download phase is 30%)
                var buildProgress = expected > 0 ? done / expected : 1.0
                var copyProgress = copiedTotal > 0 ? copiedSize / copiedTotal : 0

                if (copiedTotal > 0) {
                    progress = (buildProgress * 0.7) + (copyProgress * 0.3)
                } else {
                    progress = buildProgress * 0.7
                }

                var pct = (progress * 100).toFixed(1)
                var builtText = done + "/" + expected + " built"
                var copiedText = copied > 0 ? ", " + copied + " copied" : ""
                if (copiedTotal > 0) {
                    copiedText += " (" + copiedSize.toFixed(1) + "/" + copiedTotal.toFixed(1) + " MiB)"
                }
                var dlText = dlMatch ? ", " + dlMatch[1] + " MiB DL" : ""
                progressText = pct + "%"
                statusText = builtText + copiedText + dlText
            }
            // Parse "copying path" lines
            else if (line.indexOf("copying path") >= 0) {
                var pathMatch = line.match(/copying path '[^']*-([^-']+)-[^']*'/)
                if (pathMatch) {
                    statusText = "Copying: " + pathMatch[1]
                } else {
                    statusText = "Copying paths..."
                }
            }
            // Parse "building" lines
            else if (line.indexOf("building '/nix/store") >= 0) {
                var buildingMatch = line.match(/building '[^']*-([^-']+)-[^']*\.drv'/)
                if (buildingMatch) {
                    statusText = "Building: " + buildingMatch[1]
                }
            }
            // Parse nixos-rebuild phases
            else if (line.indexOf("building the system configuration") >= 0) {
                statusText = "Building system configuration..."
            }
            else if (line.indexOf("activating the configuration") >= 0) {
                progress = 0.9
                progressText = "90%"
                statusText = "Activating configuration..."
            }
            else if (line.indexOf("setting up") >= 0 || line.indexOf("reloading") >= 0) {
                progress = 0.95
                progressText = "95%"
                statusText = "Finalizing..."
            }
        }

        function reset() {
            progress = 0.0
            statusText = "Starting update..."
            progressText = "0%"
            root.updateJustCompleted = false
        }

        function complete(success) {
            progress = 1.0
            progressText = "100%"
            statusText = success ? "Update complete!" : "Update failed"
        }
    }

    // Autosave function
    function autosaveSettings() {
        if (flakePathField.text.length > 0) {
            var unit = unitCombo.model[unitCombo.currentIndex].value
            checker.save_config(flakePathField.text, intervalSpinBox.value, unit)
        }
    }

    // The Rust backend
    UpdateChecker {
        id: checker

        Component.onCompleted: {
            checker.load_config()
            // Start initial check after a short delay
            initialCheckTimer.start()
        }

        onUpdates_changed: {
            // Don't overwrite green icon during updates
            if (!checker.update_running) {
                trayIcon.icon.source = checker.get_icon_path()
            }
            // Reset "Update complete!" status and progress bar when new updates are found
            if (checker.has_updates) {
                root.updateJustCompleted = false
                progressInfo.progress = 0.0
                progressInfo.progressText = ""
                progressInfo.statusText = ""
            }
        }

        onCheckingChanged: {
            // Don't overwrite green icon during updates
            if (!checker.update_running) {
                trayIcon.icon.source = checker.get_icon_path()
            }
            if (checker.checking) {
                root.lastLine = ""
                checker.update_status_line = ""
            }
        }

        onUpdate_runningChanged: {
            if (checker.update_running) {
                trayIcon.icon.source = "qrc:/icons/nix-flake-updating.svg"
                checker.tooltip_text = "NixOS Update Checker\nUpdate in progress..."
            } else {
                trayIcon.icon.source = checker.get_icon_path()
            }
        }

        onConfig_loaded: {
            flakePathField.text = checker.flake_path
            intervalSpinBox.value = checker.check_interval
            // Set the unit combo box
            var unit = checker.check_interval_unit
            if (unit === "days") {
                unitCombo.currentIndex = 1
            } else if (unit === "weeks") {
                unitCombo.currentIndex = 2
            } else {
                unitCombo.currentIndex = 0  // hours
            }
            // Calculate interval in milliseconds based on unit
            var minutes = checker.check_interval
            if (unit === "days") {
                minutes = checker.check_interval * 24 * 60
            } else if (unit === "weeks") {
                minutes = checker.check_interval * 7 * 24 * 60
            } else {
                minutes = checker.check_interval * 60
            }
            checkTimer.interval = minutes * 60 * 1000
            checkTimer.start()
        }

        onConfig_saved: {
            // Calculate interval in milliseconds based on unit
            var unit = checker.check_interval_unit
            var minutes = checker.check_interval
            if (unit === "days") {
                minutes = checker.check_interval * 24 * 60
            } else if (unit === "weeks") {
                minutes = checker.check_interval * 7 * 24 * 60
            } else {
                minutes = checker.check_interval * 60
            }
            checkTimer.interval = minutes * 60 * 1000
        }
    }

    // Timer for initial check
    Timer {
        id: initialCheckTimer
        interval: 2000
        repeat: false
        onTriggered: checker.check_now()
    }

    // Timer for periodic checks
    Timer {
        id: checkTimer
        interval: 60 * 60 * 1000 // Default 1 hour
        repeat: true
        running: false
        onTriggered: checker.check_now()
    }

    // Timer to poll for check completion (async check)
    Timer {
        id: pollTimer
        interval: 100
        repeat: true
        running: checker.checking
        onTriggered: checker.poll_check_result()
    }

    // Timer to detect external system updates (every 30 seconds)
    // If system changed and we had updates showing, triggers a full recheck
    Timer {
        id: systemChangeTimer
        interval: 30 * 1000
        repeat: true
        running: true
        onTriggered: checker.check_system_changed()
    }

    // Timer to refresh the "last checked" display (every minute)
    Timer {
        id: refreshTimeTimer
        interval: 60 * 1000
        repeat: true
        running: true
        onTriggered: checker.refresh_last_check_time()
    }

    // Timer to poll terminal output during updates (200ms)
    Timer {
        id: terminalPollTimer
        interval: 200
        repeat: true
        running: terminal.running
        onTriggered: {
            var text = terminal.getAllText()
            if (!text) return

            // Update last line for progress display
            var line = checker.find_last_line(text)
            if (line && line !== root.lastLine) {
                root.lastLine = line
                progressInfo.parseProgress(line)

                // Update tray tooltip with progress
                if (progressInfo.progressText) {
                    checker.tooltip_text = "NixOS Update Checker\nUpdate in progress (" + progressInfo.progressText + ")"
                }
            }

            // Check for completion
            if (checker.check_update_complete(text)) {
                var exitCode = checker.get_update_exit_code(text)
                terminal.running = false

                checker.update_running = false
                checker.update_status_line = exitCode === 0 ? "Update completed successfully" : "Update failed with exit code " + exitCode
                progressInfo.complete(exitCode === 0)
                terminal.show()

                // Check for reboot-requiring packages before clearing
                var needsReboot = exitCode === 0 && checker.check_reboot_required()
                var rebootPkgs = needsReboot ? checker.get_reboot_packages() : ""

                if (exitCode === 0) {
                    root.updateJustCompleted = true
                    checker.clear_cached_updates()

                    if (needsReboot) {
                        rebootDialog.rebootPackages = rebootPkgs
                        rebootDialog.open()
                    }
                }
                checker.update_completed()
            }
        }
    }

    // Folder dialog for flake path selection
    Platform.FolderDialog {
        id: folderDialog
        title: "Select Flake Directory"
        folder: flakePathField.text ? "file://" + flakePathField.text : Platform.StandardPaths.writableLocation(Platform.StandardPaths.HomeLocation)
        onAccepted: {
            // Convert file:// URL to path
            var path = folder.toString().replace("file://", "")
            flakePathField.text = path
            autosaveSettings()
        }
    }

    // System tray icon
    Platform.SystemTrayIcon {
        id: trayIcon
        visible: true
        icon.source: "qrc:/icons/nix-flake.svg"
        tooltip: checker.tooltip_text || "NixOS Update Checker"

        onActivated: function(reason) {
            if (reason === Platform.SystemTrayIcon.Trigger) {
                // Left click - show window
                root.show()
                root.raise()
                root.requestActivate()
            }
        }

        menu: Platform.Menu {
            Platform.MenuItem {
                text: checker.has_updates
                    ? qsTr("Run Update (%1 available)").arg(checker.update_count)
                    : qsTr("No updates available")
                enabled: checker.has_updates && !terminal.running
                onTriggered: {
                    root.show()
                    var flakePath = checker.get_update_script_path()
                    var commitMsg = checker.get_update_commit_message()
                    if (flakePath) {
                        terminal.clear()
                        root.lastLine = ""
                        progressInfo.reset()
                        var cmd = checker.build_update_command(flakePath, commitMsg, checker.commit_and_push, checker.run_gc_after_update)
                        terminal.runCommand(cmd)
                    }
                }
            }

            Platform.MenuSeparator {}

            Platform.MenuItem {
                text: checker.checking ? qsTr("Checking...") : qsTr("Check Now")
                enabled: !checker.checking
                onTriggered: checker.check_now()
            }

            Platform.MenuItem {
                text: qsTr("Settings...")
                onTriggered: {
                    root.show()
                    root.raise()
                    root.requestActivate()
                }
            }

            Platform.MenuSeparator {}

            Platform.MenuItem {
                text: checker.build_info || "NixOS Update Checker"
                enabled: false
            }

            Platform.MenuItem {
                text: qsTr("Quit")
                onTriggered: checker.quit_app()
            }
        }
    }

    // Main content
    ColumnLayout {
        id: mainContent
        anchors.fill: parent
        anchors.margins: 15
        spacing: 10

        Label {
            text: "NixOS Update Checker"
            font.pixelSize: 18
            font.bold: true
            Layout.alignment: Qt.AlignHCenter
        }

        Label {
            text: checker.build_info || ""
            font.pixelSize: 10
            color: "#888"
            Layout.alignment: Qt.AlignHCenter
            visible: checker.build_info ? true : false
        }

        // Settings
        GridLayout {
            columns: 2
            rowSpacing: 10
            columnSpacing: 10
            Layout.fillWidth: true

            Label { text: "Flake Path:" }
            RowLayout {
                Layout.fillWidth: true
                spacing: 5

                TextField {
                    id: flakePathField
                    Layout.fillWidth: true
                    placeholderText: "/etc/nixos"
                    readOnly: true
                }

                Button {
                    text: "Browse..."
                    onClicked: folderDialog.open()
                }
            }

            Label { text: "Check Interval:" }
            RowLayout {
                spacing: 10
                SpinBox {
                    id: intervalSpinBox
                    from: 1
                    to: 99
                    value: 1
                    editable: true
                    onValueModified: autosaveSettings()
                }
                ComboBox {
                    id: unitCombo
                    model: [
                        { text: "Hours", value: "hours" },
                        { text: "Days", value: "days" },
                        { text: "Weeks", value: "weeks" }
                    ]
                    textRole: "text"
                    currentIndex: 0
                    onActivated: autosaveSettings()
                }
            }

            Label { text: "" }  // empty cell for grid alignment
            CheckBox {
                id: commitAndPushCheckbox
                text: "Commit and push after update"
                checked: checker.commit_and_push
                onToggled: {
                    checker.commit_and_push = checked
                    autosaveSettings()
                }
            }

            Label { text: "" }  // empty cell for grid alignment
            CheckBox {
                id: runGcCheckbox
                text: "Run garbage collection after update"
                checked: checker.run_gc_after_update
                onToggled: {
                    checker.run_gc_after_update = checked
                    autosaveSettings()
                }
            }
        }

        // Status area
        GroupBox {
            title: "Status"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: font.pixelSize * 5 * 1.6 + 40
            clip: true

            ColumnLayout {
                anchors.fill: parent
                spacing: 5

                Label {
                    text: "Last check: " + (checker.last_check_time || "Never")
                }

                Label {
                    text: checker.status_message || "Ready"
                    color: checker.status_message && checker.status_message.startsWith("Error") ? "red" : palette.text
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 5

                    Button {
                        text: packageListExpanded ? "▼" : "▶"
                        flat: true
                        implicitWidth: 30
                        visible: checker.has_updates
                        onClicked: packageListExpanded = !packageListExpanded
                    }

                    Label {
                        text: "Updates"
                        font.bold: true
                        visible: checker.has_updates
                    }

                    Label {
                        text: root.updateJustCompleted
                            ? "Update complete!"
                            : (checker.has_updates
                                ? checker.update_count + (checker.update_count === 1 ? " package" : " packages") + " available"
                                    + (checker.download_size && checker.unpacked_size
                                        ? " (" + checker.download_size + " download, " + checker.unpacked_size + " unpacked)"
                                        : checker.download_size ? " (" + checker.download_size + " download)" : "")
                                : "No updates available")
                        color: root.updateJustCompleted ? "#4CAF50" : (checker.has_updates ? "#2196F3" : palette.text)
                    }
                }

                // Expandable package list
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: packageListExpanded && checker.has_updates
                    Layout.minimumHeight: packageListExpanded && checker.has_updates ? 80 : 0
                    visible: packageListExpanded && checker.has_updates

                    ListView {
                        id: packageListView
                        clip: true
                        model: {
                            try {
                                return checker.updates_json ? JSON.parse(checker.updates_json) : []
                            } catch (e) {
                                return []
                            }
                        }
                        delegate: Label {
                            text: {
                                var name = modelData.package_name
                                var oldName = modelData.old_package_name || ""
                                var oldHash = modelData.old_hash_short || ""
                                var newHash = modelData.hash_short || ""

                                // Extract version from package name (e.g., "firefox-120.0" -> "120.0")
                                function extractVersion(pkgName) {
                                    var lastDash = pkgName.lastIndexOf('-')
                                    if (lastDash > 0) {
                                        var after = pkgName.substring(lastDash + 1)
                                        if (after.length > 0 && /^\d/.test(after)) {
                                            return after
                                        }
                                    }
                                    return null
                                }

                                // Extract base name (without version)
                                function baseName(pkgName) {
                                    var lastDash = pkgName.lastIndexOf('-')
                                    if (lastDash > 0) {
                                        var after = pkgName.substring(lastDash + 1)
                                        if (after.length > 0 && /^\d/.test(after)) {
                                            return pkgName.substring(0, lastDash)
                                        }
                                    }
                                    return pkgName
                                }

                                var newVersion = extractVersion(name)
                                var oldVersion = oldName ? extractVersion(oldName) : null
                                var base = baseName(name)

                                // Prefer version display over hash display
                                if (oldVersion && newVersion && oldVersion !== newVersion) {
                                    return "• " + base + " (" + oldVersion + " → " + newVersion + ")"
                                } else if (oldVersion && newVersion && oldVersion === newVersion && oldHash && newHash) {
                                    return "• " + base + " (" + oldVersion + " " + oldHash + " → " + newHash + ")"
                                } else if (!oldVersion && newVersion && oldHash) {
                                    return "• " + base + " (" + oldHash + " → " + newVersion + ")"
                                } else if (oldVersion && !newVersion && newHash) {
                                    return "• " + base + " (" + oldVersion + " → " + newHash + ")"
                                } else if (oldHash && newHash) {
                                    return "• " + base + " (" + oldHash + " → " + newHash + ")"
                                } else if (newHash) {
                                    return "• " + name + " (new: " + newHash + ")"
                                } else {
                                    return "• " + name
                                }
                            }
                            font.family: "monospace"
                            padding: 2
                            width: packageListView.width
                        }
                    }
                }

                // Spacer to push content to top
                Item { Layout.fillHeight: true }
            }
        }

        // Progress bar for update progress (always visible when running or complete)
        ColumnLayout {
            Layout.fillWidth: true
            visible: terminal.running || progressBar.value >= 1.0
            spacing: 2

            RowLayout {
                Layout.fillWidth: true
                spacing: 10

                Label {
                    id: progressLabel
                    text: progressInfo.statusText
                    elide: Text.ElideMiddle
                    Layout.fillWidth: true
                    font.family: "monospace"
                    font.pointSize: 9
                }

                Label {
                    text: progressInfo.progressText
                    font.family: "monospace"
                    font.pointSize: 9
                    font.bold: true
                }
            }

            ProgressBar {
                id: progressBar
                Layout.fillWidth: true
                from: 0
                to: 1
                value: progressInfo.progress

                // Custom styling for better visibility
                background: Rectangle {
                    implicitHeight: 6
                    color: "#e0e0e0"
                    radius: 3
                }

                contentItem: Item {
                    implicitHeight: 6
                    Rectangle {
                        width: progressBar.visualPosition * parent.width
                        height: parent.height
                        radius: 3
                        color: progressBar.value >= 1.0 ? "#4CAF50" : "#2196F3"
                    }
                }
            }
        }

        // Output panel with embedded terminal
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: outputExpanded
            spacing: 5

            // Header row with expand button and title
            RowLayout {
                Layout.fillWidth: true
                spacing: 5

                Button {
                    text: outputExpanded ? "▼" : "▶"
                    flat: true
                    implicitWidth: 30
                    onClicked: outputExpanded = !outputExpanded
                }

                Label {
                    text: "Update Output"
                    font.bold: true
                }

                Label {
                    text: {
                        if (terminal.running) {
                            return root.lastLine || "Running update..."
                        } else if (root.lastLine) {
                            return root.lastLine
                        } else {
                            return checker.update_status_line || ""
                        }
                    }
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                    font.family: "monospace"
                    color: terminal.running ? "#2196F3" : palette.text
                }
            }

            // Embedded terminal using WindowContainer
            WindowContainer {
                id: terminalContainer
                window: terminal.window
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.minimumHeight: 300
                Layout.preferredHeight: 350
                visible: outputExpanded

                // Dark background for terminal area
                Rectangle {
                    anchors.fill: parent
                    color: "#1e1e1e"
                    z: -1
                }
            }
        }

        // Terminal widget instance
        TerminalWidget {
            id: terminal

            onRunningChanged: {
                checker.update_running = terminal.running
            }
        }

        // Buttons
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Button {
                text: checker.checking ? "Checking..." : "Check Now"
                enabled: !checker.checking && flakePathField.text.length > 0
                onClicked: checker.check_now()
            }

            Button {
                text: terminal.running ? "Updating..." : "Run Update"
                enabled: checker.has_updates && !terminal.running
                onClicked: {
                    var flakePath = checker.get_update_script_path()
                    var commitMsg = checker.get_update_commit_message()
                    if (flakePath) {
                        terminal.clear()
                        root.lastLine = ""
                        progressInfo.reset()
                        var cmd = checker.build_update_command(flakePath, commitMsg, checker.commit_and_push, checker.run_gc_after_update)
                        terminal.runCommand(cmd)
                    }
                }
            }

            Item { Layout.fillWidth: true }

            Button {
                text: "Close"
                onClicked: root.hide()
            }
        }
    }

    // Handle window close - hide instead of quit
    onClosing: function(close) {
        close.accepted = false
        root.hide()
    }

    // Reboot confirmation dialog
    Dialog {
        id: rebootDialog
        title: "Reboot Required"
        modal: true
        anchors.centerIn: parent
        width: 450
        height: 320

        property string rebootPackages: ""

        contentItem: ColumnLayout {
            spacing: 10

            Label {
                text: "The following packages were updated and may require a reboot to take full effect:"
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.minimumHeight: 80

                ListView {
                    clip: true
                    model: rebootDialog.rebootPackages ? rebootDialog.rebootPackages.split(", ") : []
                    delegate: Label {
                        text: "  \u2022 " + modelData
                        font.bold: true
                        font.family: "monospace"
                        width: ListView.view.width
                        padding: 2
                    }
                }
            }

            Label {
                text: "Would you like to reboot now?"
                Layout.fillWidth: true
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                Item { Layout.fillWidth: true }
                Button {
                    text: "Yes"
                    onClicked: {
                        rebootDialog.close()
                        Qt.callLater(function() {
                            checker.reboot_system()
                        })
                    }
                }
                Button {
                    text: "No"
                    onClicked: rebootDialog.close()
                }
            }
        }
    }
}
