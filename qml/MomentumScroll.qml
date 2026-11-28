import QtQuick

// Touchpad momentum for a Flickable. Flickable follows a touchpad's pixel
// deltas 1:1 and stops dead when the fingers lift: only macOS sends
// momentum phases (Qt.ScrollMomentum), and on Linux neither libinput nor
// the compositor coasts. This watches the touchpad's scroll events on the
// way to the Flickable (it never takes them, so scrolling itself is
// unchanged), tracks their velocity, and flicks the Flickable with it when
// the gesture ends. Mouse wheels pass straight through; Flickable already
// animates those.
//
// Put it over the Flickable as a sibling (a child would be in its content):
//     MomentumScroll { flickable: list; anchors.fill: list }
MouseArea {
    id: momentum
    required property Flickable flickable
    // Fingers that stop longer than this before lifting mean "stop here".
    property int stillMs: 60

    acceptedButtons: Qt.NoButton
    z: 1

    property real velocity: 0   // px/s; positive scrolls toward the end
    property real lastTime: 0
    property bool tracking: false

    function release() {
        if (!tracking) return
        tracking = false
        const v = Date.now() - lastTime > stillMs ? 0 : velocity
        velocity = 0
        // Flickable handles the gesture's end first; flick after it.
        // flick() takes the content's velocity, the opposite sign.
        if (Math.abs(v) > 50) Qt.callLater(() => flickable.flick(0, -v))
    }

    // Touchpads that send no phases (X11): a pause ends the gesture.
    Timer { id: idle; interval: momentum.stillMs; onTriggered: momentum.release() }

    onWheel: (wheel) => {
        wheel.accepted = false
        const dy = wheel.pixelDelta.y
        if (wheel.phase === Qt.ScrollEnd) { idle.stop(); release(); return }
        if (wheel.phase === Qt.ScrollBegin) flickable.cancelFlick()
        if (dy === 0) return   // a mouse wheel, or a phase with no motion

        const now = Date.now()
        const dt = now - lastTime
        if (!tracking || dt > 100) {
            flickable.cancelFlick()
            tracking = true
            velocity = 0
        }
        if (dt > 0) {
            // Smooth over a few events; they arrive unevenly.
            const v = -dy * 1000 / Math.min(dt, 100)
            const w = velocity === 0 ? 1 : Math.min(1, dt / 40)
            velocity += (v - velocity) * w
        }
        lastTime = now
        if (wheel.phase === Qt.NoScrollPhase) idle.restart()
    }
}
