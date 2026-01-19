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
    width: 1000
    height: outputExpanded ? 1200 : 770
    title: "NixOS Update Checker"
    flags: Qt.Dialog | Qt.WindowStaysOnTopHint

    property bool outputExpanded: false
    property bool packageListExpanded: false

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
            trayIcon.icon.source = checker.get_icon_path()
            trayIcon.tooltip = checker.tooltip_text
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
        tooltip: "NixOS Update Checker"

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
                    var scriptPath = checker.get_update_script_path()
                    if (scriptPath) {
                        outputExpanded = true
                        terminal.clear()
                        terminal.runScript(scriptPath)
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
                text: qsTr("Quit")
                onTriggered: checker.quit_app()
            }
        }
    }

    // Main content
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 20
        spacing: 15

        Label {
            text: "NixOS Update Checker"
            font.pixelSize: 18
            font.bold: true
            Layout.alignment: Qt.AlignHCenter
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
                    onEditingFinished: autosaveSettings()
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
        }

        // Status area
        GroupBox {
            title: "Status"
            Layout.fillWidth: true

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
                        text: checker.has_updates
                            ? checker.update_count + " package(s) to update"
                            : "No updates available"
                        font.bold: checker.has_updates
                        color: checker.has_updates ? "#2196F3" : palette.text
                    }
                }

                // Expandable package list
                ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 150
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
                                var oldHash = modelData.old_hash_short || ""
                                var newHash = modelData.hash_short || ""
                                if (oldHash && newHash) {
                                    return "• " + name + " (" + oldHash + " → " + newHash + ")"
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
            }
        }

        // Output panel with embedded terminal
        GroupBox {
            title: "Update Output"
            Layout.fillWidth: true
            Layout.fillHeight: outputExpanded

            ColumnLayout {
                anchors.fill: parent
                spacing: 5

                // Status line (always visible)
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
                        text: terminal.running ? "Running update..." : (checker.update_status_line || "")
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
                    Layout.minimumHeight: 400
                    visible: outputExpanded

                    // Dark background for terminal area
                    Rectangle {
                        anchors.fill: parent
                        color: "#1e1e1e"
                        z: -1
                    }
                }
            }
        }

        // Terminal widget instance
        TerminalWidget {
            id: terminal

            onFinished: function(exitCode) {
                checker.set_update_running(false)
                checker.set_update_status_line(exitCode === 0 ? "Update completed successfully" : "Update failed with exit code " + exitCode)
                // Clear updates since we just applied them
                if (exitCode === 0) {
                    checker.set_has_updates(false)
                    checker.set_update_count(0)
                    checker.set_updates_json("[]")
                }
                checker.update_completed()
            }

            onRunningChanged: {
                checker.set_update_running(terminal.running)
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
                    // Get the script path from checker and run in terminal
                    var scriptPath = checker.get_update_script_path()
                    if (scriptPath) {
                        outputExpanded = true
                        terminal.clear()
                        terminal.runScript(scriptPath)
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
}
