import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.platform as Platform
import NixosUpdateChecker

ApplicationWindow {
    id: root
    visible: false
    width: 450
    height: 350
    title: "NixOS Update Checker - Settings"
    flags: Qt.Dialog | Qt.WindowStaysOnTopHint

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
            trayIcon.toolTip = checker.tooltip_text
        }

        onConfig_loaded: {
            flakePathField.text = checker.flake_path
            intervalSpinBox.value = checker.check_interval
            terminalField.text = checker.terminal
            // Start periodic checking
            checkTimer.interval = checker.check_interval * 60 * 1000
            checkTimer.start()
        }

        onConfig_saved: {
            checkTimer.interval = checker.check_interval * 60 * 1000
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

    // System tray icon
    Platform.SystemTrayIcon {
        id: trayIcon
        visible: true
        icon.source: "qrc:/icons/nix-flake.svg"
        toolTip: "NixOS Update Checker"

        onActivated: function(reason) {
            if (reason === Platform.SystemTrayIcon.Trigger) {
                // Left click - run update if updates available, otherwise check
                if (checker.has_updates) {
                    checker.run_update()
                } else {
                    checker.check_now()
                }
            } else if (reason === Platform.SystemTrayIcon.Context) {
                // Right click - show menu (handled automatically)
            }
        }

        menu: Platform.Menu {
            Platform.MenuItem {
                text: checker.has_updates
                    ? qsTr("Run Update (%1 available)").arg(checker.update_count)
                    : qsTr("No updates available")
                enabled: checker.has_updates
                onTriggered: checker.run_update()
            }

            Platform.MenuSeparator {}

            Platform.MenuItem {
                text: checker.checking ? qsTr("Checking...") : qsTr("Check Now")
                enabled: !checker.checking
                onTriggered: checker.check_now()
            }

            Platform.MenuItem {
                text: qsTr("Settings...")
                onTriggered: root.show()
            }

            Platform.MenuSeparator {}

            Platform.MenuItem {
                text: qsTr("Quit")
                onTriggered: checker.quit_app()
            }
        }
    }

    // Settings dialog content
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 20
        spacing: 15

        Label {
            text: "NixOS Update Checker Settings"
            font.pixelSize: 18
            font.bold: true
            Layout.alignment: Qt.AlignHCenter
        }

        GridLayout {
            columns: 2
            rowSpacing: 10
            columnSpacing: 10
            Layout.fillWidth: true

            Label { text: "Flake Path:" }
            TextField {
                id: flakePathField
                Layout.fillWidth: true
                placeholderText: "/path/to/your/nixos-config"
            }

            Label { text: "Check Interval (minutes):" }
            SpinBox {
                id: intervalSpinBox
                from: 5
                to: 1440
                value: 60
                editable: true
            }

            Label { text: "Terminal:" }
            TextField {
                id: terminalField
                Layout.fillWidth: true
                placeholderText: "ghostty"
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
                    color: checker.status_message.startsWith("Error") ? "red" : "inherit"
                }

                Label {
                    text: checker.has_updates
                        ? checker.update_count + " update(s) available"
                        : "No updates available"
                    font.bold: checker.has_updates
                    color: checker.has_updates ? "#2196F3" : "inherit"
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
                onClicked: {
                    // Save first if changed
                    if (flakePathField.text !== checker.flake_path ||
                        intervalSpinBox.value !== checker.check_interval ||
                        terminalField.text !== checker.terminal) {
                        checker.save_config(flakePathField.text, intervalSpinBox.value, terminalField.text)
                    }
                    checker.check_now()
                }
            }

            Item { Layout.fillWidth: true }

            Button {
                text: "Save"
                onClicked: {
                    checker.save_config(flakePathField.text, intervalSpinBox.value, terminalField.text)
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
