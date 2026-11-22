import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import QtQuick.Effects
import Qt.labs.platform as Platform
import NixosUpdateChecker
import Terminal 1.0

// Software Update, macOS style, in hearth's look: an opaque window with
// liquid-glass cards and buttons (sheen, rim, soft shadow): a status card
// (up to date / updates available / installing / restart), the pending
// NixOS and Flatpak updates, and the Automatic Updates sheet.
// New updates also post an "Updates Available" notification with Install
// Now and Later (in an hour, tonight, tomorrow).
ApplicationWindow {
    id: root
    visible: false  // Start hidden - show via tray menu or the notification
    width: 700
    height: 560
    minimumWidth: 600
    // Grow with the content (expanded lists, the terminal) instead of
    // squeezing it; the sheet scrolls, so it doesn't count.
    minimumHeight: Math.min(page === 0 ? updatePage.implicitHeight + 44 : 420,
                            Screen.desktopAvailableHeight - 40)
    title: "Software Update"
    // Keep text off exact half device pixels at 133% (glyphs smear there).
    contentItem.transform: Translate { x: 0.001; y: 0.001 }
    color: theme.bg

    property bool outputExpanded: false
    property bool nixListExpanded: false
    property bool flatpakListExpanded: false
    property bool updateJustCompleted: false  // Track if update just finished successfully
    property bool updateFailed: false
    property string lastLine: ""  // Last non-empty line from terminal output
    property string rebootPackages: ""
    property int page: 0  // 0 = Software Update, 1 = Automatic Updates sheet
    onOutputExpandedChanged: Qt.callLater(function() {
        height = outputExpanded ? Math.max(height, minimumHeight + 280) : Math.max(minimumHeight, 560)
    })

    // --- look (hearth's Theme, from the system palette) ---
    QtObject {
        id: theme
        readonly property bool dark: palette.window.hslLightness < 0.5
        readonly property color bg: palette.window
        readonly property color fg: palette.windowText
        readonly property color fgDim: Qt.rgba(fg.r, fg.g, fg.b, 0.58)
        readonly property color fgFaint: Qt.rgba(fg.r, fg.g, fg.b, 0.35)
        readonly property color accent: palette.highlight
        readonly property color good: "#30d158"
        readonly property color bad: "#ff453a"
        readonly property color warn: "#ff9f0a"
        // Cards: a lighter layer of glass on the window.
        readonly property color card: dark ? Qt.rgba(1, 1, 1, 0.05) : Qt.rgba(1, 1, 1, 0.75)
        readonly property color rim: dark ? Qt.rgba(1, 1, 1, 0.13) : Qt.rgba(1, 1, 1, 0.9)
        readonly property color edge: dark ? Qt.rgba(0, 0, 0, 0.38) : Qt.rgba(0, 0, 0, 0.10)
        readonly property real sheen: dark ? 0.06 : 0.35
        readonly property color separator: dark ? Qt.rgba(1, 1, 1, 0.09) : Qt.rgba(0, 0, 0, 0.08)
        readonly property color control: dark ? Qt.rgba(1, 1, 1, 0.10) : Qt.rgba(0, 0, 0, 0.05)
        readonly property color controlHover: dark ? Qt.rgba(1, 1, 1, 0.16) : Qt.rgba(0, 0, 0, 0.09)
        readonly property int radius: 18
        // SF Pro where installed, else Inter (as hearth), else the system font.
        readonly property var families: Qt.fontFamilies()
        readonly property string font: families.indexOf("SF Pro") >= 0 ? "SF Pro"
            : families.indexOf("SF Pro Text") >= 0 ? "SF Pro Text"
            : families.indexOf("Inter") >= 0 ? "Inter"
            : families.indexOf("Inter Variable") >= 0 ? "Inter Variable"
            : Qt.application.font.family
        readonly property string displayFont: families.indexOf("SF Pro Display") >= 0 ? "SF Pro Display"
            : families.indexOf("Inter Display") >= 0 ? "Inter Display" : font
        readonly property real fontSize: 10.5
        readonly property real smallSize: 9
        readonly property string mono: families.indexOf("Hack") >= 0 ? "Hack" : "monospace"
    }

    font.family: theme.font
    font.pointSize: theme.fontSize

    // --- building blocks ---

    // A pane of liquid glass: translucent tint, a sheen in its upper part,
    // a bright rim inside a darker hairline, and a soft shadow.
    component GlassCard: Item {
        id: card
        default property alias content: inner.data
        property real padding: 18
        Layout.fillWidth: true
        implicitHeight: inner.childrenRect.height + 2 * padding

        RectangularShadow {
            anchors.fill: bgRect
            radius: bgRect.radius
            blur: 22
            offset.y: 6
            spread: -4
            color: Qt.rgba(0, 0, 0, theme.dark ? 0.32 : 0.10)
        }
        Rectangle {
            id: bgRect
            anchors.fill: parent
            radius: theme.radius
            color: theme.card
            border.color: theme.edge
            border.width: 1
            Rectangle {
                // Sheen: light falling on the top of the glass.
                anchors.fill: parent
                anchors.margins: 1
                radius: parent.radius - 1
                gradient: Gradient {
                    GradientStop { position: 0.0; color: Qt.rgba(1, 1, 1, theme.sheen) }
                    GradientStop { position: 0.45; color: Qt.rgba(1, 1, 1, 0) }
                }
                border.color: theme.rim
                border.width: 1
            }
        }
        Item {
            id: inner
            x: card.padding
            y: card.padding
            width: card.width - 2 * card.padding
            height: childrenRect.height
        }
    }

    component Text_: Text {
        color: theme.fg
        font.family: theme.font
        font.pointSize: theme.fontSize
        renderType: Text.NativeRendering
        elide: Text.ElideRight
    }

    // Capsule button. `primary` is the accent-filled default action, with a
    // glassy highlight on top.
    component GlassButton: MouseArea {
        id: btn
        property string text
        property bool primary: false
        implicitWidth: Math.max(primary ? 120 : 90, lbl.implicitWidth + 36)
        implicitHeight: 30
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        opacity: enabled ? 1 : 0.4
        Rectangle {
            id: face
            anchors.fill: parent
            radius: height / 2
            color: btn.primary
                ? (btn.pressed ? Qt.darker(theme.accent, 1.2) : btn.containsMouse ? Qt.lighter(theme.accent, 1.08) : theme.accent)
                : (btn.pressed ? theme.controlHover : btn.containsMouse ? theme.controlHover : theme.control)
            border.color: btn.primary ? Qt.rgba(1, 1, 1, 0.28) : theme.rim
            border.width: 1
            Rectangle {
                anchors.fill: parent
                anchors.margins: 1
                radius: height / 2
                gradient: Gradient {
                    GradientStop { position: 0.0; color: Qt.rgba(1, 1, 1, btn.primary ? 0.22 : theme.sheen) }
                    GradientStop { position: 0.55; color: Qt.rgba(1, 1, 1, 0) }
                }
            }
        }
        RectangularShadow {
            visible: btn.primary && btn.enabled
            anchors.fill: face
            z: -1
            radius: face.radius
            blur: 10
            offset.y: 3
            spread: -3
            color: Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.45)
        }
        Text_ {
            id: lbl
            anchors.centerIn: parent
            text: btn.text
            color: btn.primary ? "#ffffff" : theme.fg
            font.weight: btn.primary ? Font.DemiBold : Font.Medium
        }
    }

    // A text link ("More Info…").
    component LinkButton: MouseArea {
        id: link
        property string text
        implicitWidth: linkText.implicitWidth
        implicitHeight: linkText.implicitHeight
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        Text_ {
            id: linkText
            text: link.text
            color: theme.accent
            font.underline: link.containsMouse
        }
    }

    // macOS switch.
    component Toggle: MouseArea {
        id: sw
        property bool checked: false
        signal toggled(bool checked)
        implicitWidth: 40
        implicitHeight: 24
        cursorShape: Qt.PointingHandCursor
        onClicked: toggled(!checked)
        opacity: enabled ? 1 : 0.4
        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: sw.checked ? theme.accent : (theme.dark ? Qt.rgba(1, 1, 1, 0.18) : Qt.rgba(0, 0, 0, 0.14))
            border.color: theme.rim
            Behavior on color { ColorAnimation { duration: 140 } }
            Rectangle {
                width: parent.height - 4
                height: width
                radius: width / 2
                y: 2
                x: sw.checked ? parent.width - width - 2 : 2
                color: "#ffffff"
                Behavior on x { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
            }
        }
    }

    // A row in a settings group: label, note under it, control on the right.
    component SettingRow: Item {
        id: row
        property string label
        property string note: ""
        default property alias control: holder.data
        Layout.fillWidth: true
        implicitHeight: Math.max(46, texts.implicitHeight + 18)
        opacity: enabled ? 1 : 0.45
        Column {
            id: texts
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - holder.width - 16
            Text_ { text: row.label; width: parent.width }
            Text_ {
                visible: row.note !== ""
                text: row.note
                width: parent.width
                color: theme.fgDim
                font.pointSize: theme.smallSize
                wrapMode: Text.Wrap
                elide: Text.ElideNone
            }
        }
        Row {
            id: holder
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
        }
    }

    component Separator: Rectangle {
        Layout.fillWidth: true
        implicitHeight: 1
        color: theme.separator
    }

    // One pending update in a More Info list: the name, a tag ("new",
    // "user", "runtime"), then old → new version, right-aligned in mono.
    component UpdateRow: Item {
        id: ur
        property string name
        property string from: ""
        property string to: ""
        property string tag: ""
        implicitHeight: 26
        Rectangle {
            // Faint banding, like macOS list rows.
            anchors.fill: parent
            radius: 6
            color: (typeof index !== "undefined" && index % 2 === 1)
                ? (theme.dark ? Qt.rgba(1, 1, 1, 0.035) : Qt.rgba(0, 0, 0, 0.03)) : "transparent"
        }
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            spacing: 8
            Text_ {
                text: ur.name
                font.pointSize: theme.smallSize + 0.5
                Layout.fillWidth: true
                Layout.minimumWidth: 80
            }
            Rectangle {
                visible: ur.tag !== ""
                implicitWidth: tagText.implicitWidth + 10
                implicitHeight: 16
                radius: 4
                color: theme.control
                Text_ {
                    id: tagText
                    anchors.centerIn: parent
                    text: ur.tag
                    color: theme.fgDim
                    font.pointSize: theme.smallSize - 1.5
                }
            }
            Text_ {
                visible: ur.from !== ""
                text: ur.from
                color: theme.fgFaint
                font.family: theme.mono
                font.pointSize: theme.smallSize - 0.5
                Layout.maximumWidth: 150
                elide: Text.ElideMiddle
            }
            Text_ {
                visible: ur.from !== ""
                text: "→"
                color: theme.fgFaint
                font.pointSize: theme.smallSize
            }
            Text_ {
                text: ur.to
                color: theme.fg
                font.family: theme.mono
                font.pointSize: theme.smallSize - 0.5
                Layout.maximumWidth: 150
                elide: Text.ElideMiddle
            }
        }
    }

    // A rounded tile holding a glyph, like the app icons in macOS lists.
    component IconTile: Rectangle {
        property alias source: img.source
        property bool flatpakGlyph: false
        implicitWidth: 34
        implicitHeight: 34
        radius: 9
        gradient: Gradient {
            GradientStop { position: 0; color: flatpakGlyph ? "#5a9bff" : (theme.dark ? "#3a4a6b" : "#dfe8f7") }
            GradientStop { position: 1; color: flatpakGlyph ? "#2f6fdc" : (theme.dark ? "#26324a" : "#c4d3ee") }
        }
        border.color: Qt.rgba(1, 1, 1, 0.22)
        Image {
            id: img
            anchors.centerIn: parent
            visible: !flatpakGlyph
            sourceSize.width: 24
            sourceSize.height: 24
        }
        Canvas {
            // Flatpak-ish box.
            visible: flatpakGlyph
            anchors.centerIn: parent
            width: 22
            height: 22
            onPaint: {
                const c = getContext("2d");
                c.reset();
                c.strokeStyle = "#ffffff";
                c.lineWidth = 1.8;
                c.lineJoin = "round";
                c.beginPath();
                c.moveTo(11, 2); c.lineTo(19.5, 6.5); c.lineTo(19.5, 15.5);
                c.lineTo(11, 20); c.lineTo(2.5, 15.5); c.lineTo(2.5, 6.5); c.closePath();
                c.moveTo(2.5, 6.5); c.lineTo(11, 11); c.lineTo(19.5, 6.5);
                c.moveTo(11, 11); c.lineTo(11, 20);
                c.stroke();
            }
        }
    }

    // --- progress parsing (unchanged behaviour) ---
    QtObject {
        id: progressInfo
        property real progress: 0.0
        property string statusText: ""
        property string progressText: ""
        property bool completed: false
        property bool succeeded: false

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
            var flatpakMatch = line.match(/Updating\s+(\d+)\/(\d+)/)

            if (builtMatch) {
                var done, expected
                if (builtMatch[3]) {
                    done = parseInt(builtMatch[2])
                    expected = parseInt(builtMatch[3])
                } else if (builtMatch[2]) {
                    done = parseInt(builtMatch[1])
                    expected = parseInt(builtMatch[2])
                } else {
                    done = parseInt(builtMatch[1])
                    expected = done
                }

                var copied = copiedMatch ? parseInt(copiedMatch[1]) : 0
                var copiedSize = 0, copiedTotal = 0
                if (copiedSizeMatch) {
                    copiedSize = parseFloat(copiedSizeMatch[1])
                    copiedTotal = parseFloat(copiedSizeMatch[2])
                }

                // Combined progress (build phase is 70%, copy/download phase is 30%)
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
            else if (flatpakMatch) {
                statusText = "Updating Flatpak " + flatpakMatch[1] + " of " + flatpakMatch[2] + "…"
            }
            else if (line.indexOf("Updating Flatpak apps") >= 0) {
                statusText = "Updating Flatpak apps…"
            }
            else if (line.indexOf("Updating flake inputs") >= 0) {
                statusText = "Updating flake inputs…"
            }
            else if (line.indexOf("copying path") >= 0) {
                var pathMatch = line.match(/copying path '[^']*-([^-']+)-[^']*'/)
                statusText = pathMatch ? "Copying " + pathMatch[1] : "Copying paths…"
            }
            else if (line.indexOf("building '/nix/store") >= 0) {
                var buildingMatch = line.match(/building '[^']*-([^-']+)-[^']*\.drv'/)
                if (buildingMatch) {
                    statusText = "Building " + buildingMatch[1]
                }
            }
            else if (line.indexOf("building the system configuration") >= 0) {
                statusText = "Building system configuration…"
            }
            else if (line.indexOf("activating the configuration") >= 0) {
                progress = 0.9
                progressText = "90%"
                statusText = "Activating configuration…"
            }
            else if (line.indexOf("setting up") >= 0 || line.indexOf("reloading") >= 0) {
                progress = 0.95
                progressText = "95%"
                statusText = "Finalizing…"
            }
            else if (line.indexOf("Committing changes") >= 0) {
                statusText = "Committing flake.lock…"
            }
            else if (line.indexOf("garbage collection") >= 0) {
                statusText = "Collecting garbage…"
            }
        }

        function reset() {
            progress = 0.0
            statusText = "Preparing…"
            progressText = ""
            completed = false
            succeeded = false
            root.updateJustCompleted = false
            root.updateFailed = false
            root.rebootPackages = ""
        }

        function complete(success) {
            progress = 1.0
            progressText = ""
            statusText = success ? "Update complete" : "Update failed"
            completed = true
            succeeded = success
        }
    }

    function intervalText() {
        var n = checker.check_interval
        var unit = checker.check_interval_unit || "hours"
        var word = unit === "days" ? "day" : unit === "weeks" ? "week" : "hour"
        return n === 1 ? "every " + word : "every " + n + " " + word + "s"
    }

    // Splits a NixOS store name diff into name and versions: "firefox",
    // "145.0.2" → "146.0". Without a version change it falls back to the
    // store hashes (a rebuild of the same version).
    function nixVersions(d) {
        function split(pkg) {
            // The version starts at the first "-" followed by a digit.
            var m = pkg.match(/^(.*?)-(\d.*)$/)
            return m ? { base: m[1], version: m[2] } : { base: pkg, version: "" }
        }
        var n = split(d.package_name || "")
        var o = d.old_package_name ? split(d.old_package_name) : { base: "", version: "" }
        var oldHash = d.old_hash_short || "", newHash = d.hash_short || ""
        if (o.version && n.version && o.version !== n.version)
            return { name: n.base, from: o.version, to: n.version }
        if (n.version && (!o.version || o.version === n.version))
            return { name: n.base, from: "", to: n.version, rebuilt: true }
        if (!d.old_package_name)
            return { name: n.base, from: "", to: n.version || newHash, isNew: true }
        if (oldHash && newHash)
            return { name: n.base, from: oldHash, to: newHash }
        return { name: n.base, from: "", to: n.version || newHash }
    }

    function sizeText() {
        if (checker.download_size && checker.unpacked_size)
            return checker.download_size + " download, " + checker.unpacked_size + " on disk"
        return checker.download_size ? checker.download_size + " download" : ""
    }

    function showWindow() {
        root.show()
        root.raise()
        root.requestActivate()
    }

    function startUpdate() {
        if (terminal.running || !checker.has_updates) return
        page = 0
        showWindow()
        terminal.clear()
        root.lastLine = ""
        progressInfo.reset()
        terminal.runCommand(checker.build_update_command())
        if (outputExpanded) {
            Qt.callLater(terminal.focusTerminal)
        }
    }

    // Autosave function
    function autosaveSettings() {
        if (flakePathField.text.length > 0) {
            var unit = unitCombo.model[unitCombo.currentIndex].value
            checker.save_config(flakePathField.text, intervalSpinBox.value, unit)
        }
    }

    function intervalMs() {
        var unit = checker.check_interval_unit
        var minutes = checker.check_interval * 60
        if (unit === "days") minutes = checker.check_interval * 24 * 60
        else if (unit === "weeks") minutes = checker.check_interval * 7 * 24 * 60
        return minutes * 60 * 1000
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
            // Reset "Update complete" and the progress bar when new updates are found
            if (checker.has_updates && !checker.update_running) {
                root.updateJustCompleted = false
                root.updateFailed = false
                progressInfo.progress = 0.0
                progressInfo.progressText = ""
                progressInfo.statusText = ""
                progressInfo.completed = false
                progressInfo.succeeded = false
            }
        }

        onCheckingChanged: {
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
                checker.tooltip_text = "Software Update\nInstalling updates…"
            } else {
                trayIcon.icon.source = checker.get_icon_path()
            }
        }

        onConfig_loaded: {
            flakePathField.text = checker.flake_path
            intervalSpinBox.value = checker.check_interval
            generationPruneModeCombo.currentIndex = checker.generation_prune_mode === "older-than" ? 1 : 0
            generationPruneSpinBox.value = checker.generation_prune_value
            var unit = checker.check_interval_unit
            unitCombo.currentIndex = unit === "days" ? 1 : unit === "weeks" ? 2 : 0
            checkTimer.interval = intervalMs()
            checkTimer.start()
        }

        onConfig_saved: checkTimer.interval = intervalMs()
    }

    Timer {
        id: initialCheckTimer
        interval: 2000
        repeat: false
        onTriggered: checker.check_now()
    }

    // Periodic checks
    Timer {
        id: checkTimer
        interval: 60 * 60 * 1000
        repeat: true
        running: false
        onTriggered: checker.check_now()
    }

    // Poll for check completion (async check)
    Timer {
        interval: 100
        repeat: true
        running: checker.checking
        onTriggered: checker.poll_check_result()
    }

    // Detect external system updates (every 30 seconds)
    Timer {
        interval: 30 * 1000
        repeat: true
        running: true
        onTriggered: checker.check_system_changed()
    }

    // Refresh "last checked", and post the reminder once a Later runs out.
    Timer {
        interval: 60 * 1000
        repeat: true
        running: true
        onTriggered: {
            checker.refresh_last_check_time()
            checker.maybe_notify()
        }
    }

    // The notification's answer (Install Now / Later / Details).
    Timer {
        interval: 250
        repeat: true
        running: checker.notification_pending
        onTriggered: {
            var answer = checker.poll_notification()
            if (answer === "install") root.startUpdate()
            else if (answer === "details") root.showWindow()
        }
    }

    // Poll terminal output during updates
    Timer {
        interval: 200
        repeat: true
        running: terminal.running
        onTriggered: {
            var text = terminal.getAllText()
            if (!text) return

            var line = checker.find_last_line(text)
            if (line && line !== root.lastLine) {
                root.lastLine = line
                progressInfo.parseProgress(line)
                if (progressInfo.progressText) {
                    checker.tooltip_text = "Software Update\nInstalling updates (" + progressInfo.progressText + ")"
                }
            }

            if (checker.check_update_complete(text)) {
                var exitCode = checker.get_update_exit_code(text)

                checker.update_status_line = exitCode === 0 ? "Update completed successfully" : "Update failed with exit code " + exitCode
                progressInfo.complete(exitCode === 0)
                terminal.show()

                // Check for reboot-requiring packages before clearing
                var needsReboot = exitCode === 0 && checker.check_reboot_required()
                root.rebootPackages = needsReboot ? checker.get_reboot_packages() : ""

                if (exitCode === 0) {
                    root.updateJustCompleted = true
                    checker.clear_cached_updates()
                    if (needsReboot) root.showWindow()
                } else {
                    root.updateFailed = true
                    root.outputExpanded = true
                }
                terminal.running = false
                checker.update_completed()
            }
        }
    }

    Platform.FolderDialog {
        id: folderDialog
        title: "Choose Flake Folder"
        folder: flakePathField.text ? "file://" + flakePathField.text : Platform.StandardPaths.writableLocation(Platform.StandardPaths.HomeLocation)
        onAccepted: {
            var path = folder.toString().replace("file://", "")
            flakePathField.text = path
            autosaveSettings()
        }
    }

    Platform.SystemTrayIcon {
        id: trayIcon
        visible: true
        icon.source: "qrc:/icons/nix-flake.svg"
        tooltip: checker.tooltip_text || "Software Update"

        onActivated: function(reason) {
            if (reason === Platform.SystemTrayIcon.Trigger) {
                root.page = 0
                root.showWindow()
            }
        }

        menu: Platform.Menu {
            Platform.MenuItem {
                text: terminal.running ? qsTr("Installing Updates…")
                    : checker.has_updates
                    ? qsTr("Update Now (%1 available)").arg(checker.total_count)
                    : qsTr("Your system is up to date")
                enabled: checker.has_updates && !terminal.running
                onTriggered: root.startUpdate()
            }

            Platform.Menu {
                title: qsTr("Remind Me Later")
                enabled: checker.has_updates && !terminal.running
                Platform.MenuItem { text: qsTr("In an Hour"); onTriggered: checker.snooze("hour") }
                Platform.MenuItem { text: qsTr("Tonight"); onTriggered: checker.snooze("tonight") }
                Platform.MenuItem { text: qsTr("Tomorrow"); onTriggered: checker.snooze("tomorrow") }
            }

            Platform.MenuSeparator {}

            Platform.MenuItem {
                text: checker.checking ? qsTr("Checking…") : qsTr("Check for Updates")
                enabled: !checker.checking
                onTriggered: checker.check_now()
            }

            Platform.MenuItem {
                text: qsTr("Software Update…")
                onTriggered: { root.page = 0; root.showWindow() }
            }

            Platform.MenuItem {
                text: qsTr("Automatic Updates…")
                onTriggered: { root.page = 1; root.showWindow() }
            }

            Platform.MenuSeparator {}

            Platform.MenuItem {
                text: checker.build_info || "Software Update"
                enabled: false
            }

            Platform.MenuItem {
                text: qsTr("Quit")
                onTriggered: checker.quit_app()
            }
        }
    }

    // --- the window ---
    background: Rectangle {
        color: theme.bg
        // A faint wash of light from the top, as on macOS windows.
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(1, 1, 1, theme.dark ? 0.04 : 0.18) }
                GradientStop { position: 0.4; color: Qt.rgba(1, 1, 1, 0) }
            }
        }
    }

    // State of the status card.
    readonly property string phase: terminal.running ? "installing"
        : updateFailed ? "failed"
        : (updateJustCompleted && rebootPackages) ? "restart"
        : updateJustCompleted ? "done"
        : checker.checking ? "checking"
        : checker.has_updates ? "available"
        : "current"

    StackLayout {
        anchors.fill: parent
        anchors.margins: 22
        currentIndex: root.page

        // ===== Software Update =====
        ColumnLayout {
            id: updatePage
            spacing: 14

            RowLayout {
                Layout.fillWidth: true
                Text_ {
                    text: "Software Update"
                    font.family: theme.displayFont
                    font.pointSize: 17
                    font.weight: Font.Bold
                    Layout.fillWidth: true
                }
                Text_ {
                    text: checker.system_version
                    color: theme.fgDim
                    font.pointSize: theme.smallSize
                }
            }

            // Status card
            GlassCard {
                ColumnLayout {
                    width: parent.width
                    spacing: 14

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 16

                        // Status emblem: a glass disc with the snowflake, a
                        // check mark when up to date, a badge with the count.
                        Item {
                            implicitWidth: 64
                            implicitHeight: 64
                            Rectangle {
                                anchors.fill: parent
                                radius: width / 2
                                gradient: Gradient {
                                    GradientStop { position: 0; color: theme.dark ? "#3b4d72" : "#e6eefb" }
                                    GradientStop { position: 1; color: theme.dark ? "#202a40" : "#c8d7f0" }
                                }
                                border.color: Qt.rgba(1, 1, 1, theme.dark ? 0.18 : 0.9)
                                Image {
                                    anchors.centerIn: parent
                                    source: "qrc:/icons/nix-flake.svg"
                                    sourceSize.width: 40
                                    sourceSize.height: 40
                                    RotationAnimator on rotation {
                                        running: root.phase === "checking" || root.phase === "installing"
                                        from: 0; to: 360
                                        duration: 2400
                                        loops: Animation.Infinite
                                        onRunningChanged: if (!running) parent.rotation = 0
                                    }
                                }
                            }
                            Rectangle {
                                // Badge
                                visible: root.phase === "current" || root.phase === "done"
                                         || root.phase === "available" || root.phase === "failed"
                                         || root.phase === "restart"
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                anchors.margins: -2
                                width: Math.max(24, badgeText.implicitWidth + 12)
                                height: 24
                                radius: 12
                                color: root.phase === "available" ? theme.bad
                                     : root.phase === "failed" ? theme.bad
                                     : root.phase === "restart" ? theme.warn
                                     : theme.good
                                border.color: theme.dark ? "#1e1f22" : "#ffffff"
                                border.width: 2
                                Text_ {
                                    id: badgeText
                                    anchors.centerIn: parent
                                    color: "#ffffff"
                                    font.weight: Font.Bold
                                    font.pointSize: 9
                                    text: root.phase === "available" ? checker.total_count
                                        : root.phase === "failed" ? "!"
                                        : root.phase === "restart" ? "↻"
                                        : "✓"
                                }
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 3
                            Text_ {
                                Layout.fillWidth: true
                                font.pointSize: 13
                                font.weight: Font.DemiBold
                                text: {
                                    switch (root.phase) {
                                    case "installing": return "Installing Updates…"
                                    case "failed": return "The update didn’t finish"
                                    case "restart": return "Restart to finish updating"
                                    case "done": return "Your system is up to date"
                                    case "checking": return "Checking for updates…"
                                    case "available": return "Updates Available"
                                    default: return "Your system is up to date"
                                    }
                                }
                            }
                            Text_ {
                                Layout.fillWidth: true
                                color: theme.fgDim
                                wrapMode: Text.Wrap
                                maximumLineCount: 3
                                text: {
                                    switch (root.phase) {
                                    case "installing": return progressInfo.statusText || "Preparing…"
                                    case "failed": return "See the details below. Nothing else was changed."
                                    case "restart": return "Updated: " + root.rebootPackages
                                    case "done": return "Updates were installed. " + checker.system_version
                                    case "checking": return checker.flake_path
                                    case "available": {
                                        var parts = []
                                        if (checker.update_count > 0)
                                            parts.push(checker.update_count + (checker.update_count === 1 ? " NixOS package" : " NixOS packages"))
                                        if (checker.include_flatpak && checker.flatpak_count > 0)
                                            parts.push(checker.flatpak_count + (checker.flatpak_count === 1 ? " Flatpak update" : " Flatpak updates"))
                                        var s = parts.join(" and ")
                                        if (root.sizeText()) s += " · " + root.sizeText()
                                        return s
                                    }
                                    default: return checker.system_version
                                    }
                                }
                            }
                            Text_ {
                                Layout.fillWidth: true
                                visible: root.phase !== "installing"
                                color: checker.status_message && checker.status_message.startsWith("Error") ? theme.bad : theme.fgFaint
                                font.pointSize: theme.smallSize
                                wrapMode: Text.Wrap
                                maximumLineCount: 2
                                text: checker.status_message && checker.status_message.startsWith("Error")
                                    ? checker.status_message
                                    : "Last checked " + (checker.last_check_time || "never")
                                      + (checker.snooze_text && root.phase === "available" ? " · " + checker.snooze_text : "")
                            }
                        }

                        ColumnLayout {
                            spacing: 6
                            Layout.alignment: Qt.AlignVCenter
                            GlassButton {
                                Layout.alignment: Qt.AlignRight
                                primary: root.phase === "available" || root.phase === "restart"
                                visible: root.phase !== "installing"
                                text: root.phase === "available" ? "Update Now"
                                    : root.phase === "restart" ? "Restart Now"
                                    : root.phase === "checking" ? "Checking…"
                                    : "Check Now"
                                enabled: root.phase !== "checking" && flakePathField.text.length > 0
                                onClicked: {
                                    if (root.phase === "available") root.startUpdate()
                                    else if (root.phase === "restart") Qt.callLater(checker.reboot_system)
                                    else checker.check_now()
                                }
                            }
                            // Later: put off the reminder, like the
                            // notification's Later buttons.
                            GlassButton {
                                id: laterButton
                                Layout.alignment: Qt.AlignRight
                                visible: root.phase === "available"
                                text: "Later ▾"
                                onClicked: laterMenu.popup(laterButton, 0, laterButton.height + 4)
                                Menu {
                                    id: laterMenu
                                    MenuItem { text: "Remind Me in an Hour"; onTriggered: checker.snooze("hour") }
                                    MenuItem { text: "Remind Me Tonight"; onTriggered: checker.snooze("tonight") }
                                    MenuItem { text: "Remind Me Tomorrow"; onTriggered: checker.snooze("tomorrow") }
                                }
                            }
                        }
                    }

                    // Progress (while installing and after)
                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: root.phase === "installing" || root.phase === "failed"
                        spacing: 6
                        Item {
                            Layout.fillWidth: true
                            implicitHeight: 6
                            Rectangle {
                                anchors.fill: parent
                                radius: 3
                                color: theme.control
                            }
                            Rectangle {
                                height: parent.height
                                radius: 3
                                width: Math.max(height, parent.width * progressInfo.progress)
                                color: progressInfo.completed ? (progressInfo.succeeded ? theme.good : theme.bad) : theme.accent
                                Behavior on width { NumberAnimation { duration: 250; easing.type: Easing.OutCubic } }
                                Rectangle {
                                    anchors.fill: parent
                                    radius: parent.radius
                                    gradient: Gradient {
                                        GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0.35) }
                                        GradientStop { position: 0.6; color: Qt.rgba(1, 1, 1, 0) }
                                    }
                                }
                            }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            Text_ {
                                Layout.fillWidth: true
                                text: root.lastLine || checker.update_status_line || ""
                                color: theme.fgFaint
                                font.family: theme.mono
                                font.pointSize: theme.smallSize - 0.5
                                elide: Text.ElideMiddle
                            }
                            Text_ {
                                text: progressInfo.progressText
                                color: theme.fgDim
                                font.pointSize: theme.smallSize
                                font.weight: Font.DemiBold
                            }
                        }
                    }

                    // Details disclosure: the embedded terminal
                    LinkButton {
                        visible: root.phase === "installing" || root.phase === "failed"
                                 || root.phase === "done" || root.phase === "restart" || root.lastLine !== ""
                        text: root.outputExpanded ? "Hide Details" : "Show Details"
                        onClicked: {
                            root.outputExpanded = !root.outputExpanded
                            if (root.outputExpanded) Qt.callLater(terminal.focusTerminal)
                        }
                    }
                }
            }

            // A new major release: flake updates stay on the release the
            // flake follows, so moving on takes an edit to flake.nix.
            GlassCard {
                visible: checker.new_release !== "" && !root.outputExpanded
                padding: 14
                RowLayout {
                    width: parent.width
                    spacing: 12
                    Rectangle {
                        implicitWidth: 34
                        implicitHeight: 34
                        radius: 9
                        gradient: Gradient {
                            GradientStop { position: 0; color: "#ffb340" }
                            GradientStop { position: 1; color: "#f08a00" }
                        }
                        border.color: Qt.rgba(1, 1, 1, 0.25)
                        Text_ {
                            anchors.centerIn: parent
                            text: "!"
                            color: "#ffffff"
                            font.weight: Font.Bold
                            font.pointSize: 13
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 1
                        Text_ {
                            Layout.fillWidth: true
                            text: "NixOS " + checker.new_release + " is available"
                            font.weight: Font.DemiBold
                        }
                        Text_ {
                            Layout.fillWidth: true
                            color: theme.fgDim
                            font.pointSize: theme.smallSize
                            wrapMode: Text.Wrap
                            elide: Text.ElideNone
                            text: "Your flake follows " + (checker.followed_release ? "NixOS " + checker.followed_release : "an older release")
                                + ", which updates won’t move past. To upgrade, change the nixpkgs input (and any release-pinned inputs) in flake.nix."
                        }
                    }
                    GlassButton {
                        text: "Release Notes"
                        onClicked: Qt.openUrlExternally("https://nixos.org/manual/nixos/stable/release-notes")
                    }
                }
            }

            // The terminal (a native child window, so it lives outside any
            // scroll area and above nothing that pops up).
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.minimumHeight: 160
                visible: root.outputExpanded
                radius: 12
                color: "#1e1e1e"
                border.color: theme.edge
                WindowContainer {
                    id: terminalContainer
                    anchors.fill: parent
                    anchors.margins: 8
                    window: terminal.window
                    visible: root.outputExpanded && root.page === 0
                    focus: true
                    activeFocusOnTab: true
                    onVisibleChanged: if (visible) Qt.callLater(terminal.focusTerminal)
                    onActiveFocusChanged: if (activeFocus) terminal.focusTerminal()
                }
            }

            // Pending updates
            GlassCard {
                id: updatesCard
                visible: !root.outputExpanded && checker.has_updates && root.phase !== "installing"
                padding: 14
                ColumnLayout {
                    width: parent.width
                    spacing: 10

                    // NixOS
                    RowLayout {
                        Layout.fillWidth: true
                        visible: checker.update_count > 0
                        spacing: 12
                        IconTile { source: "qrc:/icons/nix-flake.svg" }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 1
                            Text_ { text: "NixOS"; font.weight: Font.DemiBold; Layout.fillWidth: true }
                            Text_ {
                                Layout.fillWidth: true
                                color: theme.fgDim
                                font.pointSize: theme.smallSize
                                text: checker.update_count + (checker.update_count === 1 ? " package" : " packages")
                                    + (root.sizeText() ? " · " + root.sizeText() : "")
                            }
                        }
                        LinkButton {
                            text: root.nixListExpanded ? "Less Info" : "More Info…"
                            onClicked: root.nixListExpanded = !root.nixListExpanded
                        }
                    }
                    ListView {
                        id: nixList
                        Layout.fillWidth: true
                        Layout.leftMargin: 46
                        visible: root.nixListExpanded && checker.update_count > 0
                        implicitHeight: Math.min(contentHeight, 180)
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                        model: {
                            try { return checker.updates_json ? JSON.parse(checker.updates_json) : [] }
                            catch (e) { return [] }
                        }
                        delegate: UpdateRow {
                            width: nixList.width - 12
                            readonly property var info: root.nixVersions(modelData)
                            name: info.name
                            from: info.from
                            to: info.to
                            tag: info.isNew ? "new" : info.rebuilt ? "rebuilt" : ""
                        }
                    }

                    Separator {
                        visible: checker.update_count > 0 && checker.include_flatpak && checker.flatpak_count > 0
                    }

                    // Flatpak
                    RowLayout {
                        Layout.fillWidth: true
                        visible: checker.include_flatpak && checker.flatpak_count > 0
                        spacing: 12
                        IconTile { flatpakGlyph: true }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 1
                            Text_ { text: "Flatpak Apps"; font.weight: Font.DemiBold; Layout.fillWidth: true }
                            Text_ {
                                Layout.fillWidth: true
                                color: theme.fgDim
                                font.pointSize: theme.smallSize
                                text: {
                                    var list = flatpakList.model
                                    var apps = 0, sys = 0, user = 0
                                    for (var i = 0; i < list.length; i++) {
                                        if (list[i].kind === "app") apps++
                                        if (list[i].installation === "system") sys++; else user++
                                    }
                                    var s = checker.flatpak_count + (checker.flatpak_count === 1 ? " update" : " updates")
                                    if (apps !== list.length) s += " (" + apps + (apps === 1 ? " app, " : " apps, ") + (list.length - apps) + " runtime" + (list.length - apps === 1 ? ")" : "s)")
                                    if (sys && user) s += " · system and user"
                                    else if (user) s += " · your apps"
                                    return s
                                }
                            }
                        }
                        LinkButton {
                            text: root.flatpakListExpanded ? "Less Info" : "More Info…"
                            onClicked: root.flatpakListExpanded = !root.flatpakListExpanded
                        }
                    }
                    ListView {
                        id: flatpakList
                        Layout.fillWidth: true
                        Layout.leftMargin: 46
                        visible: root.flatpakListExpanded && checker.include_flatpak && checker.flatpak_count > 0
                        implicitHeight: Math.min(contentHeight, 180)
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                        model: {
                            try { return checker.flatpak_updates_json ? JSON.parse(checker.flatpak_updates_json) : [] }
                            catch (e) { return [] }
                        }
                        delegate: UpdateRow {
                            width: flatpakList.width - 12
                            name: modelData.name
                            from: modelData.old_version !== modelData.new_version ? modelData.old_version : ""
                            to: modelData.new_version || modelData.branch
                            tag: modelData.installation === "user" ? "user" : modelData.kind === "runtime" ? "runtime" : ""
                        }
                    }
                }
            }

            // Automatic updates summary, opens the sheet
            GlassCard {
                visible: !root.outputExpanded
                padding: 6
                SettingRow {
                    width: parent.width - 24
                    x: 12
                    label: "Automatic updates"
                    note: "Check " + root.intervalText()
                        + (checker.notify_updates ? ", notify me" : "")
                        + (checker.include_flatpak ? ", include Flatpak apps" : "")
                    GlassButton {
                        text: "Options…"
                        onClicked: root.page = 1
                    }
                }
            }

            Item { Layout.fillHeight: true; visible: !root.outputExpanded }

            RowLayout {
                Layout.fillWidth: true
                Text_ {
                    Layout.fillWidth: true
                    text: checker.build_info || ""
                    color: theme.fgFaint
                    font.pointSize: theme.smallSize - 0.5
                }
                GlassButton {
                    text: "Close"
                    onClicked: root.hide()
                }
            }
        }

        // ===== Automatic Updates sheet =====
        ColumnLayout {
            spacing: 14

            Text_ {
                text: "Automatic Updates"
                font.family: theme.displayFont
                font.pointSize: 17
                font.weight: Font.Bold
            }

            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true

                ColumnLayout {
                    width: parent.width
                    spacing: 14

                    Text_ { text: "Checking"; color: theme.fgDim; font.weight: Font.DemiBold; Layout.leftMargin: 6 }
                    GlassCard {
                        padding: 4
                        ColumnLayout {
                            x: 12
                            width: parent.width - 24
                            spacing: 0
                            SettingRow {
                                label: "Check for updates"
                                note: "Fetches the flake’s inputs and compares them with this system."
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
                            Separator {}
                            SettingRow {
                                label: "Notify me when updates are available"
                                note: "A notification with Install Now and Later (in an hour, tonight or tomorrow)."
                                Toggle {
                                    checked: checker.notify_updates
                                    onToggled: function(on) {
                                        checker.notify_updates = on
                                        autosaveSettings()
                                        if (on) checker.maybe_notify()
                                    }
                                }
                            }
                            Separator {}
                            SettingRow {
                                label: "Include Flatpak apps"
                                note: "Also checks and updates Flatpak apps and runtimes, system-wide and your own. Masked apps are left alone."
                                Toggle {
                                    checked: checker.include_flatpak
                                    onToggled: function(on) {
                                        checker.include_flatpak = on
                                        autosaveSettings()
                                        checker.check_now()
                                    }
                                }
                            }
                        }
                    }

                    Text_ { text: "System flake"; color: theme.fgDim; font.weight: Font.DemiBold; Layout.leftMargin: 6 }
                    GlassCard {
                        padding: 4
                        ColumnLayout {
                            x: 12
                            width: parent.width - 24
                            spacing: 0
                            SettingRow {
                                label: "Flake folder"
                                note: flakePathField.text || "/etc/nixos"
                                TextField {
                                    // Holds the path; shown in the note above.
                                    id: flakePathField
                                    visible: false
                                    readOnly: true
                                }
                                GlassButton {
                                    text: "Choose…"
                                    onClicked: folderDialog.open()
                                }
                            }
                        }
                    }

                    Text_ { text: "After updating"; color: theme.fgDim; font.weight: Font.DemiBold; Layout.leftMargin: 6 }
                    GlassCard {
                        padding: 4
                        ColumnLayout {
                            x: 12
                            width: parent.width - 24
                            spacing: 0
                            SettingRow {
                                label: "Commit and push flake.lock"
                                note: "Commits only flake.lock, as the flake’s owner, and pushes it."
                                Toggle {
                                    checked: checker.commit_and_push
                                    onToggled: function(on) {
                                        checker.commit_and_push = on
                                        autosaveSettings()
                                    }
                                }
                            }
                            Separator {}
                            SettingRow {
                                label: "Remove old system generations"
                                note: "Frees disk space once the update succeeds."
                                Toggle {
                                    id: pruneToggle
                                    checked: checker.prune_generations
                                    onToggled: function(on) {
                                        checker.prune_generations = on
                                        autosaveSettings()
                                    }
                                }
                            }
                            SettingRow {
                                enabled: checker.prune_generations
                                label: "    Which to remove"
                                ComboBox {
                                    id: generationPruneModeCombo
                                    model: [
                                        { text: "Keep newest", value: "keep-latest" },
                                        { text: "Older than", value: "older-than" }
                                    ]
                                    textRole: "text"
                                    onActivated: {
                                        checker.generation_prune_mode = model[currentIndex].value
                                        autosaveSettings()
                                    }
                                }
                                SpinBox {
                                    id: generationPruneSpinBox
                                    from: 1
                                    to: 999
                                    value: 10
                                    editable: true
                                    onValueModified: {
                                        checker.generation_prune_value = value
                                        autosaveSettings()
                                    }
                                }
                                Text_ {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: generationPruneModeCombo.currentIndex === 1 ? "days" : "generations"
                                    color: theme.fgDim
                                }
                            }
                            Separator {}
                            SettingRow {
                                label: "Collect garbage"
                                note: "Runs nix-collect-garbage once the update succeeds."
                                Toggle {
                                    checked: checker.run_gc_after_update
                                    onToggled: function(on) {
                                        checker.run_gc_after_update = on
                                        autosaveSettings()
                                    }
                                }
                            }
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                GlassButton {
                    text: "Done"
                    primary: true
                    onClicked: root.page = 0
                }
            }
        }
    }

    TerminalWidget {
        id: terminal
        onRunningChanged: checker.update_running = terminal.running
    }

    // `--show` opens the window at start (`--show=options` the sheet;
    // `--expand` opens both update lists, for screenshots).
    Component.onCompleted: {
        var args = Qt.application.arguments
        for (var i = 0; i < args.length; i++) {
            if (args[i] === "--show" || args[i] === "--show=options") {
                if (args[i] === "--show=options") page = 1
                showWindow()
            }
            if (args[i] === "--expand") nixListExpanded = flatpakListExpanded = true
        }
    }

    // Handle window close - hide instead of quit
    onClosing: function(close) {
        close.accepted = false
        root.hide()
    }
}
