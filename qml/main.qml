import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.platform as Platform
import NixosUpdateChecker

ApplicationWindow {
    id: root
    visible: false  // Start hidden - show via tray menu
    width: 1000
    height: outputExpanded ? 1200 : 770
    title: "NixOS Update Checker"
    flags: Qt.Dialog | Qt.WindowStaysOnTopHint

    property bool outputExpanded: false

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
            checkTimer.interval = checker.check_interval * 60 * 1000
        }

        onOutput_changed: {
            // Auto-scroll to bottom
            outputArea.cursorPosition = outputArea.text.length
        }

        onUpdate_completed: {
            // Expand output panel to show results
            outputExpanded = true
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

    // Timer to poll for update progress (async update)
    Timer {
        id: updatePollTimer
        interval: 100
        repeat: true
        running: checker.update_running
        onTriggered: checker.poll_update_result()
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
                enabled: checker.has_updates && !checker.update_running
                onTriggered: {
                    root.show()
                    checker.run_update()
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
            TextField {
                id: flakePathField
                Layout.fillWidth: true
                placeholderText: "/etc/nixos"
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

                Label {
                    text: checker.has_updates
                        ? checker.update_count + " update(s) available"
                        : "No updates available"
                    font.bold: checker.has_updates
                    color: checker.has_updates ? "#2196F3" : palette.text
                }
            }
        }

        // Output panel with status line
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

                    Label {
                        text: checker.update_status_line || "No output"
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                        font.family: "monospace"
                        color: checker.update_running ? "#2196F3" : palette.text
                    }

                    Button {
                        text: outputExpanded ? "▼ Collapse" : "▶ Expand"
                        flat: true
                        onClicked: outputExpanded = !outputExpanded
                    }
                }

                // Expandable output area
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: outputExpanded

                    TextArea {
                        id: outputArea
                        text: checker.update_output
                        readOnly: true
                        font.family: "monospace"
                        font.pixelSize: 11
                        wrapMode: TextArea.Wrap
                        background: Rectangle {
                            color: palette.base
                            border.color: palette.mid
                            border.width: 1
                        }
                    }
                }
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
                text: checker.update_running ? "Updating..." : "Run Update"
                enabled: checker.has_updates && !checker.update_running
                onClicked: checker.run_update()
            }

            Item { Layout.fillWidth: true }

            Button {
                text: "Save"
                onClicked: {
                    var unit = unitCombo.model[unitCombo.currentIndex].value
                    checker.save_config(flakePathField.text, intervalSpinBox.value, unit)
                }
            }

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
