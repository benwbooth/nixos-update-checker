import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import QtQuick.Effects
import Qt.labs.platform as Platform
import NixosUpdateChecker
import Terminal 1.0
import GlobalMenu 1.0

// Software Update, macOS style, in hearth's look: an opaque window with
// liquid-glass cards and buttons (sheen, rim, soft shadow): a status card
// (up to date / updates available / installing / restart), the pending
// NixOS and Flatpak updates, and the Automatic Updates sheet.
// New updates also post an "Updates Available" notification with Install
// Now and Later (in an hour, tonight, tomorrow).
ApplicationWindow {
    id: root
    visible: false  // Start hidden - show via tray menu or the notification
    // A fixed size that never follows the content: a resize is a round trip
    // to the compositor, so it would show the new layout in the old size for
    // a frame. Long lists scroll; details swap with them in the same space.
    width: 700
    height: 660
    minimumWidth: 600
    minimumHeight: 540
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
    onOutputExpandedChanged: if (outputExpanded) terminal.focusTerminal()

    // --- look (hearth's Theme, from the system palette) ---
    QtObject {
        id: theme
        readonly property bool dark: palette.window.hslLightness < 0.5
        readonly property color bg: palette.window
        readonly property color fg: palette.windowText
        readonly property color fgDim: Qt.rgba(fg.r, fg.g, fg.b, 0.58)
        readonly property color fgFaint: Qt.rgba(fg.r, fg.g, fg.b, 0.35)
        readonly property color accent: palette.highlight
        readonly property color accentText: "#ffffff"
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

    // macOS check box: a small rounded square, accent-filled with a white
    // check when on.
    component CheckBox_: MouseArea {
        id: cb
        property bool checked: false
        signal toggled(bool checked)
        implicitWidth: 17
        implicitHeight: 17
        cursorShape: Qt.PointingHandCursor
        onClicked: toggled(!checked)
        opacity: enabled ? 1 : 0.4
        Rectangle {
            anchors.fill: parent
            radius: 4.5
            color: cb.checked ? theme.accent : (theme.dark ? Qt.rgba(1, 1, 1, 0.10) : "#ffffff")
            border.color: cb.checked ? Qt.rgba(1, 1, 1, 0.3) : (theme.dark ? Qt.rgba(1, 1, 1, 0.28) : Qt.rgba(0, 0, 0, 0.25))
            Rectangle {
                anchors.fill: parent
                anchors.margins: 1
                radius: 3.5
                visible: cb.checked
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0.25) }
                    GradientStop { position: 0.6; color: Qt.rgba(1, 1, 1, 0) }
                }
            }
            Canvas {
                anchors.fill: parent
                visible: cb.checked
                onPaint: {
                    const c = getContext("2d");
                    c.reset();
                    c.strokeStyle = "#ffffff";
                    c.lineWidth = 2;
                    c.lineCap = "round";
                    c.lineJoin = "round";
                    c.beginPath();
                    c.moveTo(width * 0.26, height * 0.53);
                    c.lineTo(width * 0.44, height * 0.71);
                    c.lineTo(width * 0.75, height * 0.32);
                    c.stroke();
                }
            }
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

    // Install the ticked items (`all`: everything, e.g. Install Now in the
    // notification).
    function startUpdate(all) {
        if (terminal.running || !checker.has_updates) return
        if (!all && checkedCount === 0) return
        page = 0
        showWindow()
        partialInstall = !all && checkedCount < rows.filter(function(r) { return r.selectable }).length
        terminal.clear()
        root.lastLine = ""
        progressInfo.reset()
        terminal.runCommand(checker.build_update_command(all ? "" : selectionJson()))
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
            if (answer === "install") root.startUpdate(true)
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

                // Check for reboot-requiring packages before clearing
                var needsReboot = exitCode === 0 && checker.check_reboot_required()
                root.rebootPackages = needsReboot ? checker.get_reboot_packages() : ""

                if (exitCode === 0) {
                    root.updateJustCompleted = true
                    // Unticked items are still pending: check again instead.
                    if (root.partialInstall) checker.check_now()
                    else checker.clear_cached_updates()
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
                onTriggered: root.startUpdate(true)
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
    // Classic Software Update: a header saying what's available, a table of
    // updates to tick, the selected update's description (or the update's
    // terminal) below it, and Install N Items. macOS colours and glass from
    // hearth's look.
    // The menus, defined once: exported to the desktop's global menu bar
    // (GlobalMenuBar: hearth's top bar, Plasma's Global Menu) and shown by
    // the hamburger button in the window.
    readonly property var menuModel: [
        { title: "Software Update", items: [
            { id: "check", text: checker.checking ? "Checking…" : "Check for Updates",
              enabled: !checker.checking && !terminal.running },
            { id: "install", text: root.installLabel(), enabled: root.canInstall },
            { id: "restart", text: "Restart Now", enabled: root.phase === "restart" },
            { id: "later", text: "Remind Me Later", enabled: checker.has_updates && !terminal.running, items: [
                { id: "later-hour", text: "In an Hour" },
                { id: "later-tonight", text: "Tonight" },
                { id: "later-tomorrow", text: "Tomorrow" } ] },
            { separator: true },
            { id: "options", text: "Options…" },
            { separator: true },
            { id: "close", text: "Close Window" },
            { id: "quit", text: "Quit Software Update" } ] },
        { title: "Edit", items: [
            { id: "copy", text: "Copy", enabled: root.outputExpanded },
            { id: "select-text", text: "Select All Output", enabled: root.outputExpanded },
            { separator: true },
            { id: "select-updates", text: "Select All Updates", enabled: root.phase === "available" },
            { id: "deselect-updates", text: "Deselect All Updates", enabled: root.phase === "available" } ] },
        { title: "View", items: [
            { id: "details", text: root.outputExpanded ? "Hide Details" : "Show Details" } ] },
        { title: "Help", items: [
            { id: "release-notes", text: "NixOS Release Notes" },
            { id: "upgrading", text: "Upgrading NixOS" },
            { id: "flathub", text: "Flathub" } ] }
    ]

    function menuAction(id) {
        switch (id) {
        case "check": checker.check_now(); break
        case "install": root.startUpdate(false); break
        case "restart": Qt.callLater(checker.reboot_system); break
        case "later-hour": checker.snooze("hour"); break
        case "later-tonight": checker.snooze("tonight"); break
        case "later-tomorrow": checker.snooze("tomorrow"); break
        case "options": root.page = 1; root.showWindow(); break
        case "close": root.hide(); break
        case "quit": checker.quit_app(); break
        case "copy": terminal.copySelection(); break
        case "select-text": terminal.selectAll(); break
        case "select-updates": root.setAllChecked(true); break
        case "deselect-updates": root.setAllChecked(false); break
        case "details": root.outputExpanded = !root.outputExpanded; break
        case "release-notes": Qt.openUrlExternally("https://nixos.org/manual/nixos/stable/release-notes"); break
        case "upgrading": Qt.openUrlExternally("https://nixos.org/manual/nixos/stable/#sec-upgrading"); break
        case "flathub": Qt.openUrlExternally("https://flathub.org"); break
        }
    }

    GlobalMenuBar {
        id: globalMenu
        window: root
        model: root.menuModel
        onTriggered: function(id) { root.menuAction(id) }
    }
    // The menu's address goes on the window's surface, which exists only
    // while the window is shown.
    onVisibleChanged: if (visible) Qt.callLater(globalMenu.attach)

    // The hamburger's menu: the same model, flattened (one group per menu,
    // submenus kept), rebuilt each time it opens so it shows the current
    // texts and states.
    Menu { id: hamburgerMenu }
    Component { id: hamburgerItem; MenuItem { property string actionId; onTriggered: root.menuAction(actionId) } }
    Component { id: hamburgerSeparator; MenuSeparator {} }
    Component { id: hamburgerSubmenu; Menu {} }
    property var hamburgerObjects: []
    function fillMenu(menu, items) {
        for (var i = 0; i < items.length; i++) {
            var it = items[i]
            var obj
            if (it.separator) {
                obj = hamburgerSeparator.createObject(null)
                menu.addItem(obj)
            } else if (it.items) {
                obj = hamburgerSubmenu.createObject(null, { title: it.text, enabled: it.enabled !== false })
                fillMenu(obj, it.items)
                menu.addMenu(obj)
            } else {
                obj = hamburgerItem.createObject(null, { text: it.text, actionId: it.id, enabled: it.enabled !== false })
                menu.addItem(obj)
            }
            hamburgerObjects.push(obj)
        }
    }
    function openHamburger(button) {
        while (hamburgerMenu.count > 0) hamburgerMenu.takeItem(0)
        for (var i = 0; i < hamburgerObjects.length; i++) hamburgerObjects[i].destroy()
        hamburgerObjects = []
        for (var m = 0; m < menuModel.length; m++) {
            if (m > 0) {
                var sep = hamburgerSeparator.createObject(null)
                hamburgerMenu.addItem(sep)
                hamburgerObjects.push(sep)
            }
            fillMenu(hamburgerMenu, menuModel[m].items)
        }
        hamburgerMenu.popup(button, button.width - hamburgerMenu.implicitWidth, button.height + 4)
    }

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

    // State of the window.
    readonly property string phase: terminal.running ? "installing"
        : updateFailed ? "failed"
        : (updateJustCompleted && rebootPackages) ? "restart"
        : updateJustCompleted ? "done"
        : checker.checking && !checker.has_updates ? "checking"
        : checker.has_updates ? "available"
        : "current"

    // --- the update table ---
    // Rows: the NixOS system (one item: a flake update can't be split), each
    // Flatpak, and a newer NixOS release (information only: it takes an edit
    // to flake.nix). Keys are stable across checks so ticks survive them.
    readonly property var nixUpdates: {
        try { return checker.updates_json ? JSON.parse(checker.updates_json) : [] } catch (e) { return [] }
    }
    readonly property var flatpakUpdates: {
        if (!checker.include_flatpak) return []
        try { return checker.flatpak_updates_json ? JSON.parse(checker.flatpak_updates_json) : [] } catch (e) { return [] }
    }
    // "26.05" from "NixOS 26.05 (Yarara)".
    readonly property string systemRelease: (checker.system_version.match(/^NixOS ([^ ]+)/) || ["", ""])[1]
    readonly property var rows: {
        var r = []
        if (nixUpdates.length > 0)
            r.push({ key: "nixos", kind: "nixos", selectable: true,
                     name: "NixOS " + (checker.followed_release || systemRelease) + " System Update",
                     version: systemRelease,
                     size: checker.download_size || "" })
        for (var i = 0; i < flatpakUpdates.length; i++) {
            var f = flatpakUpdates[i]
            r.push({ key: f.installation + ":" + f.full_ref, kind: "flatpak", selectable: true, data: f,
                     name: f.name + (f.installation === "user" ? " (yours)" : ""),
                     version: f.new_version || f.branch, size: f.download_size || "" })
        }
        if (checker.new_release)
            r.push({ key: "release", kind: "release", selectable: false,
                     name: "NixOS " + checker.new_release + " (new release)",
                     version: checker.new_release, size: "" })
        return r
    }
    // Keys unticked by hand; everything else is ticked.
    property var unchecked: ({})
    function isChecked(row) { return row.selectable && !unchecked[row.key] }
    function setChecked(row, on) {
        var u = Object.assign({}, unchecked)
        if (on) delete u[row.key]; else u[row.key] = true
        unchecked = u
    }
    function setAllChecked(on) {
        var u = {}
        if (!on) for (var i = 0; i < rows.length; i++) if (rows[i].selectable) u[rows[i].key] = true
        unchecked = u
    }
    readonly property int checkedCount: {
        var n = 0
        for (var i = 0; i < rows.length; i++) if (isChecked(rows[i])) n++
        return n
    }
    readonly property bool canInstall: phase === "available" && checkedCount > 0
    function installLabel() {
        return checkedCount === 1 ? "Install 1 Item" : "Install " + checkedCount + " Items"
    }
    // What the ticks ask for, for build_update_command.
    function selectionJson() {
        var sel = { system: false, flatpaks: [] }
        for (var i = 0; i < rows.length; i++) {
            if (!isChecked(rows[i])) continue
            if (rows[i].kind === "nixos") sel.system = true
            else if (rows[i].kind === "flatpak") sel.flatpaks.push(rows[i].data.full_ref)
        }
        return JSON.stringify(sel)
    }
    property string selectedKey: ""
    readonly property var selectedRow: {
        for (var i = 0; i < rows.length; i++) if (rows[i].key === selectedKey) return rows[i]
        return rows.length ? rows[0] : null
    }
    // Some items were left unticked: after installing, check again rather
    // than clearing the list.
    property bool partialInstall: false

    StackLayout {
        anchors.fill: parent
        anchors.margins: 20
        currentIndex: root.page

        // ===== Software Update =====
        ColumnLayout {
            id: updatePage
            spacing: 14

            // Header: emblem, what's going on, and what to do.
            RowLayout {
                Layout.fillWidth: true
                spacing: 16

                // A glass disc with the snowflake (turning while busy) and
                // a badge: the count, a check mark, ! or ↻.
                Item {
                    implicitWidth: 60
                    implicitHeight: 60
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
                            sourceSize.width: 38
                            sourceSize.height: 38
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
                        visible: root.phase !== "checking" && root.phase !== "installing"
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: -2
                        width: Math.max(22, badgeText.implicitWidth + 12)
                        height: 22
                        radius: 11
                        color: root.phase === "available" || root.phase === "failed" ? theme.bad
                             : root.phase === "restart" ? theme.warn : theme.good
                        border.color: theme.dark ? "#1e1f22" : "#ffffff"
                        border.width: 2
                        Text_ {
                            id: badgeText
                            anchors.centerIn: parent
                            color: "#ffffff"
                            font.weight: Font.Bold
                            font.pointSize: 8.5
                            text: root.phase === "available" ? checker.total_count
                                : root.phase === "failed" ? "!"
                                : root.phase === "restart" ? "↻" : "✓"
                        }
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 3
                    Text_ {
                        Layout.fillWidth: true
                        font.family: theme.displayFont
                        font.pointSize: 14
                        font.weight: Font.Bold
                        text: {
                            switch (root.phase) {
                            case "installing": return "Installing software…"
                            case "failed": return "The update didn’t finish."
                            case "restart": return "Restart to finish updating."
                            case "done": return "Your software is up to date."
                            case "checking": return "Checking for new software…"
                            case "available": return "New software is available."
                            default: return "Your software is up to date."
                            }
                        }
                    }
                    Text_ {
                        Layout.fillWidth: true
                        color: theme.fgDim
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        text: {
                            switch (root.phase) {
                            case "installing": return progressInfo.statusText || "Preparing…"
                            case "failed": return "Nothing else was changed. The details show what went wrong."
                            case "restart": return "Updated: " + root.rebootPackages
                            case "done": return "Updates were installed. " + checker.system_version
                            case "checking": return "Looking at " + checker.flake_path + (checker.include_flatpak ? " and Flatpak apps" : "")
                            case "available": return "Select the items you want, then click Install."
                            default: return checker.system_version + (checker.last_check_time ? " · checked " + checker.last_check_time : "")
                            }
                        }
                    }
                    // Progress, while installing and after a failure.
                    Item {
                        Layout.fillWidth: true
                        Layout.topMargin: 4
                        implicitHeight: 6
                        visible: root.phase === "installing" || root.phase === "failed"
                        Rectangle { anchors.fill: parent; radius: 3; color: theme.control }
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
                }

                // The menus (also in the desktop's global menu bar).
                MouseArea {
                    id: hamburger
                    Layout.alignment: Qt.AlignTop
                    implicitWidth: 32
                    implicitHeight: 32
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.openHamburger(hamburger)
                    Rectangle {
                        anchors.fill: parent
                        radius: height / 2
                        color: hamburger.pressed || hamburgerMenu.visible ? theme.controlHover
                             : hamburger.containsMouse ? theme.control : "transparent"
                        border.color: hamburger.containsMouse || hamburgerMenu.visible ? theme.rim : "transparent"
                    }
                    Column {
                        anchors.centerIn: parent
                        spacing: 3
                        Repeater {
                            model: 3
                            Rectangle { width: 14; height: 1.6; radius: 0.8; color: theme.fg }
                        }
                    }
                }
            }

            // The update table
            GlassCard {
                id: tableCard
                Layout.fillHeight: true
                Layout.preferredHeight: 200
                padding: 0
                Item {
                    width: parent.width
                    height: tableCard.height

                    // Column header
                    Item {
                        id: tableHeader
                        width: parent.width
                        height: 30
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 14
                            anchors.rightMargin: 24
                            spacing: 10
                            Text_ { text: "Install"; Layout.preferredWidth: 44; color: theme.fgDim; font.pointSize: theme.smallSize; font.weight: Font.DemiBold }
                            Text_ { text: "Name"; Layout.fillWidth: true; color: theme.fgDim; font.pointSize: theme.smallSize; font.weight: Font.DemiBold }
                            Text_ { text: "Version"; Layout.preferredWidth: 130; color: theme.fgDim; font.pointSize: theme.smallSize; font.weight: Font.DemiBold }
                            Text_ { text: "Size"; Layout.preferredWidth: 86; horizontalAlignment: Text.AlignRight; color: theme.fgDim; font.pointSize: theme.smallSize; font.weight: Font.DemiBold }
                        }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: theme.separator }
                    }

                    ListView {
                        id: table
                        anchors.top: tableHeader.bottom
                        anchors.bottom: parent.bottom
                        width: parent.width
                        anchors.topMargin: 4
                        anchors.bottomMargin: 4
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                        model: root.rows
                        enabled: root.phase === "available"
                        delegate: MouseArea {
                            id: rowArea
                            required property var modelData
                            required property int index
                            readonly property bool selected: root.selectedRow && root.selectedRow.key === modelData.key
                            width: table.width
                            height: 32
                            onClicked: root.selectedKey = modelData.key
                            onDoubleClicked: if (modelData.selectable) root.setChecked(modelData, !root.isChecked(modelData))
                            Rectangle {
                                // Banding, and the selection: an accent
                                // capsule inset from the edges, macOS style.
                                anchors.fill: parent
                                anchors.leftMargin: 6
                                anchors.rightMargin: 6
                                radius: 7
                                color: rowArea.selected ? theme.accent
                                     : rowArea.index % 2 === 1 ? (theme.dark ? Qt.rgba(1, 1, 1, 0.035) : Qt.rgba(0, 0, 0, 0.03))
                                     : "transparent"
                            }
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 14
                                anchors.rightMargin: 24
                                spacing: 10
                                Item {
                                    Layout.preferredWidth: 44
                                    implicitHeight: 18
                                    CheckBox_ {
                                        visible: rowArea.modelData.selectable
                                        anchors.verticalCenter: parent.verticalCenter
                                        x: 4
                                        checked: root.isChecked(rowArea.modelData)
                                        onToggled: function(on) { root.selectedKey = rowArea.modelData.key; root.setChecked(rowArea.modelData, on) }
                                    }
                                }
                                Rectangle {
                                    // Small icon tile
                                    implicitWidth: 20
                                    implicitHeight: 20
                                    radius: 5
                                    gradient: Gradient {
                                        GradientStop { position: 0; color: rowArea.modelData.kind === "flatpak" ? "#5a9bff" : rowArea.modelData.kind === "release" ? "#ffb340" : (theme.dark ? "#3a4a6b" : "#dfe8f7") }
                                        GradientStop { position: 1; color: rowArea.modelData.kind === "flatpak" ? "#2f6fdc" : rowArea.modelData.kind === "release" ? "#f08a00" : (theme.dark ? "#26324a" : "#c4d3ee") }
                                    }
                                    border.color: Qt.rgba(1, 1, 1, 0.22)
                                    Image {
                                        visible: rowArea.modelData.kind === "nixos"
                                        anchors.centerIn: parent
                                        source: "qrc:/icons/nix-flake.svg"
                                        sourceSize.width: 14
                                        sourceSize.height: 14
                                    }
                                    Text_ {
                                        visible: rowArea.modelData.kind !== "nixos"
                                        anchors.centerIn: parent
                                        text: rowArea.modelData.kind === "release" ? "!" : "▣"
                                        color: "#ffffff"
                                        font.pointSize: 8
                                        font.weight: Font.Bold
                                    }
                                }
                                Text_ {
                                    Layout.fillWidth: true
                                    text: rowArea.modelData.name
                                    color: rowArea.selected ? theme.accentText : theme.fg
                                }
                                Text_ {
                                    Layout.preferredWidth: 130
                                    text: rowArea.modelData.version
                                    color: rowArea.selected ? theme.accentText : theme.fgDim
                                    font.family: theme.mono
                                    font.pointSize: theme.smallSize
                                    elide: Text.ElideMiddle
                                }
                                Text_ {
                                    Layout.preferredWidth: 86
                                    horizontalAlignment: Text.AlignRight
                                    text: rowArea.modelData.size || "—"
                                    color: rowArea.selected ? theme.accentText : theme.fgDim
                                    font.pointSize: theme.smallSize
                                }
                            }
                        }
                    }

                    // Empty table
                    Column {
                        anchors.centerIn: table
                        visible: root.rows.length === 0
                        spacing: 4
                        Text_ {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: root.phase === "checking" ? "Checking…" : "No updates available"
                            color: theme.fgDim
                            font.pointSize: theme.fontSize + 1
                        }
                        Text_ {
                            anchors.horizontalCenter: parent.horizontalCenter
                            visible: !!checker.last_check_time && root.phase !== "checking"
                            text: "Last checked " + checker.last_check_time
                            color: theme.fgFaint
                            font.pointSize: theme.smallSize
                        }
                    }
                }
            }

            // The selected update's description, or the update's terminal
            // (drawn in the scene: see terminal_widget.h). Same space, swapped
            // in one frame.
            GlassCard {
                id: paneCard
                Layout.fillHeight: true
                Layout.preferredHeight: 240
                padding: 0
                Item {
                    width: parent.width
                    height: paneCard.height

                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: 1
                        radius: theme.radius - 1
                        visible: root.outputExpanded
                        color: "#000000"
                        clip: true
                        TerminalWidget {
                            id: terminal
                            anchors.fill: parent
                            anchors.margins: 10
                            onRunningChanged: checker.update_running = terminal.running
                        }
                    }

                    ScrollView {
                        id: descScroll
                        anchors.fill: parent
                        anchors.margins: 16
                        visible: !root.outputExpanded
                        contentWidth: availableWidth
                        clip: true

                        ColumnLayout {
                            width: descScroll.availableWidth
                            spacing: 8
                            readonly property var row: root.selectedRow

                            // Nothing to install: the system as it is.
                            ColumnLayout {
                                visible: !parent.row
                                Layout.fillWidth: true
                                spacing: 6
                                Text_ { text: checker.system_version; font.weight: Font.DemiBold; font.pointSize: theme.fontSize + 1 }
                                Text_ {
                                    Layout.fillWidth: true
                                    wrapMode: Text.Wrap
                                    elide: Text.ElideNone
                                    color: theme.fgDim
                                    text: "Updates come from the flake at " + checker.flake_path
                                        + (checker.include_flatpak ? " and from Flatpak (your apps and system-wide ones)." : ".")
                                        + " Checks run " + root.intervalText()
                                        + (checker.notify_updates ? ", and a notification asks before anything is installed." : ".")
                                }
                                Text_ {
                                    Layout.fillWidth: true
                                    visible: checker.status_message.startsWith("Error")
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 3
                                    elide: Text.ElideRight
                                    color: theme.bad
                                    text: checker.status_message
                                }
                            }

                            // NixOS system update
                            ColumnLayout {
                                visible: !!parent.row && parent.row.kind === "nixos"
                                Layout.fillWidth: true
                                spacing: 6
                                Text_ { text: parent.parent.row ? parent.parent.row.name : ""; font.weight: Font.DemiBold; font.pointSize: theme.fontSize + 1 }
                                Text_ {
                                    Layout.fillWidth: true
                                    wrapMode: Text.Wrap
                                    elide: Text.ElideNone
                                    color: theme.fgDim
                                    text: root.nixUpdates.length + (root.nixUpdates.length === 1 ? " package changes" : " packages change")
                                        + (root.sizeText() ? " (" + root.sizeText() + ")" : "") + ". Updates the inputs of "
                                        + checker.flake_path + " and rebuilds the system"
                                        + (checker.commit_and_push ? ", then commits and pushes flake.lock." : ".")
                                }
                                Text_ {
                                    Layout.fillWidth: true
                                    visible: checker.status_message.startsWith("Error")
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 3
                                    elide: Text.ElideRight
                                    color: theme.bad
                                    text: checker.status_message
                                }
                                Repeater {
                                    model: parent.visible ? root.nixUpdates : []
                                    delegate: UpdateRow {
                                        required property var modelData
                                        required property int index
                                        Layout.fillWidth: true
                                        readonly property var info: root.nixVersions(modelData)
                                        name: info.name
                                        from: info.from
                                        to: info.to
                                        tag: info.isNew ? "new" : info.rebuilt ? "rebuilt" : ""
                                    }
                                }
                            }

                            // A Flatpak
                            ColumnLayout {
                                visible: !!parent.row && parent.row.kind === "flatpak"
                                Layout.fillWidth: true
                                spacing: 6
                                readonly property var f: parent.row && parent.row.kind === "flatpak" ? parent.row.data : null
                                Text_ { text: parent.f ? parent.f.name : ""; font.weight: Font.DemiBold; font.pointSize: theme.fontSize + 1 }
                                Text_ {
                                    Layout.fillWidth: true
                                    visible: !!parent.f && !!parent.f.summary
                                    wrapMode: Text.Wrap
                                    elide: Text.ElideNone
                                    color: theme.fgDim
                                    text: parent.f ? parent.f.summary : ""
                                }
                                GridLayout {
                                    columns: 2
                                    columnSpacing: 14
                                    rowSpacing: 4
                                    Layout.topMargin: 4
                                    readonly property var f: parent.f
                                    Text_ { text: "Version"; color: theme.fgDim; font.pointSize: theme.smallSize }
                                    Text_ {
                                        font.family: theme.mono
                                        font.pointSize: theme.smallSize
                                        text: !parent.f ? "" : (parent.f.old_version && parent.f.old_version !== parent.f.new_version
                                            ? parent.f.old_version + " → " + parent.f.new_version : (parent.f.new_version || parent.f.branch))
                                    }
                                    Text_ { text: "Download"; color: theme.fgDim; font.pointSize: theme.smallSize }
                                    Text_ { font.pointSize: theme.smallSize; text: parent.f && parent.f.download_size ? parent.f.download_size : "Unknown" }
                                    Text_ { text: "Installed for"; color: theme.fgDim; font.pointSize: theme.smallSize }
                                    Text_ { font.pointSize: theme.smallSize; text: !parent.f ? "" : (parent.f.installation === "user" ? "You" : "Everyone (system-wide)") + ", from " + parent.f.origin }
                                    Text_ { text: "Kind"; color: theme.fgDim; font.pointSize: theme.smallSize }
                                    Text_ { font.pointSize: theme.smallSize; text: parent.f && parent.f.kind === "runtime" ? "Runtime (shared by apps)" : "App" }
                                    Text_ { text: "Ref"; color: theme.fgDim; font.pointSize: theme.smallSize }
                                    Text_ { font.family: theme.mono; font.pointSize: theme.smallSize - 0.5; color: theme.fgDim; text: parent.f ? parent.f.full_ref : "" }
                                }
                                Text_ {
                                    Layout.fillWidth: true
                                    Layout.topMargin: 6
                                    visible: !!parent.f && !!parent.f.subject
                                    wrapMode: Text.Wrap
                                    elide: Text.ElideNone
                                    color: theme.fgDim
                                    font.pointSize: theme.smallSize
                                    text: parent.f ? "Latest change: " + parent.f.subject + (parent.f.date ? " (" + parent.f.date.split(" ")[0] + ")" : "") : ""
                                }
                            }

                            // A newer NixOS release
                            ColumnLayout {
                                visible: !!parent.row && parent.row.kind === "release"
                                Layout.fillWidth: true
                                spacing: 8
                                Text_ { text: "NixOS " + checker.new_release + " is available"; font.weight: Font.DemiBold; font.pointSize: theme.fontSize + 1 }
                                Text_ {
                                    Layout.fillWidth: true
                                    wrapMode: Text.Wrap
                                    elide: Text.ElideNone
                                    color: theme.fgDim
                                    text: "Your flake follows " + (checker.followed_release ? "NixOS " + checker.followed_release : "an older release")
                                        + ". Updates stay on that release, so moving to " + checker.new_release
                                        + " can’t be installed from here: change the nixpkgs input (and any other release-pinned inputs, such as home-manager) in flake.nix, then update. Read the release notes first for changes that need attention."
                                }
                                GlassButton {
                                    text: "Release Notes"
                                    onClicked: Qt.openUrlExternally("https://nixos.org/manual/nixos/stable/release-notes")
                                }
                            }
                        }
                    }
                }
            }

            // Footer: status note and options on the left, actions on the right.
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 1
                    Text_ {
                        Layout.fillWidth: true
                        color: theme.fgFaint
                        font.pointSize: theme.smallSize
                        text: (checker.last_check_time ? "Last checked " + checker.last_check_time : "Not checked yet")
                            + (checker.snooze_text && root.phase === "available" ? " · " + checker.snooze_text : "")
                    }
                    RowLayout {
                        spacing: 14
                        LinkButton { text: "Options…"; onClicked: root.page = 1 }
                        LinkButton {
                            visible: root.phase === "installing" || root.phase === "failed" || root.phase === "done"
                                     || root.phase === "restart" || root.lastLine !== ""
                            text: root.outputExpanded ? "Hide Details" : "Show Details"
                            onClicked: root.outputExpanded = !root.outputExpanded
                        }
                    }
                }
                GlassButton {
                    id: laterButton
                    visible: root.phase === "available"
                    text: "Later ▾"
                    onClicked: laterMenu.popup(laterButton, 0, -laterMenu.implicitHeight - 4)
                    Menu {
                        id: laterMenu
                        MenuItem { text: "Remind Me in an Hour"; onTriggered: checker.snooze("hour") }
                        MenuItem { text: "Remind Me Tonight"; onTriggered: checker.snooze("tonight") }
                        MenuItem { text: "Remind Me Tomorrow"; onTriggered: checker.snooze("tomorrow") }
                    }
                }
                GlassButton {
                    visible: root.phase !== "available" && root.phase !== "restart"
                    text: root.phase === "checking" ? "Checking…" : root.phase === "installing" ? "Installing…" : "Check Now"
                    enabled: root.phase !== "checking" && root.phase !== "installing" && flakePathField.text.length > 0
                    onClicked: checker.check_now()
                }
                GlassButton {
                    visible: root.phase === "available"
                    primary: true
                    text: root.installLabel()
                    enabled: root.canInstall
                    onClicked: root.startUpdate(false)
                }
                GlassButton {
                    visible: root.phase === "restart"
                    primary: true
                    text: "Restart Now"
                    onClicked: Qt.callLater(checker.reboot_system)
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

    // `--show` opens the window at start (`--show=options` the sheet;
    // `--expand` opens both update lists and `--demo-details` shows the
    // terminal with a harmless command's output, for screenshots).
    Component.onCompleted: {
        var args = Qt.application.arguments
        for (var i = 0; i < args.length; i++) {
            if (args[i] === "--show" || args[i] === "--show=options") {
                if (args[i] === "--show=options") page = 1
                showWindow()
            }
            if (args[i] === "--expand") nixListExpanded = flatpakListExpanded = true
            // Screenshots of the details view: a harmless command's output.
            if (args[i] === "--demo-details") {
                outputExpanded = true
                terminal.runCommand("printf '\\033[1;36mSoftware Update\\033[0m\\n'; ls --color=always /; printf 'Type here: '; read -r x; echo \"you typed: $x\"; echo Update complete!")
            }
        }
    }

    // Handle window close - hide instead of quit
    onClosing: function(close) {
        close.accepted = false
        root.hide()
    }
}
