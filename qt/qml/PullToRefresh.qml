import QtQuick
import Reelhouse 1.0

// Pull to refresh, on a phone: the list is dragged down past its top and let go (Material "swipe to refresh", the
// iOS refresh control). Put it beside a Flickable: `PullToRefresh { target: flick; onRefresh: ... }`, and let that
// Flickable go past its top while on (`boundsBehavior: Theme.mobile ? Flickable.DragOverBounds : ...`).
// The badge with the arrow appears as the list is pulled, turns with the pull and spins while the page reloads.
Rectangle {
    id: ptr
    property Flickable target: null
    readonly property bool on: Theme.mobile === true && target !== null
    readonly property real threshold: 72
    readonly property real pull: on ? Math.max(0, -target.verticalOvershoot) : 0
    property bool busy: false
    signal refresh()
    anchors.horizontalCenter: target ? target.horizontalCenter : undefined
    y: target ? target.y + 10 + Math.min(pull, threshold) * 0.4 : 0
    z: 5
    width: 40; height: 40; radius: 20
    color: Theme.c.popover
    border.width: 1
    border.color: Theme.c.border
    visible: on && (pull > 4 || busy)
    opacity: busy ? 1 : Math.min(1, pull / threshold)
    Icon {
        id: glyph
        anchors.centerIn: parent
        name: "refresh"; size: 20
        color: ptr.pull >= ptr.threshold || ptr.busy ? Theme.c.accent : Theme.c.textDim
        rotation: ptr.busy ? 0 : ptr.pull * 2.5
        RotationAnimation on rotation { running: ptr.busy; loops: Animation.Infinite; from: 0; to: 360; duration: 900 }
    }
    Connections {
        target: ptr.target
        function onDraggingChanged() {
            if (!ptr.on || ptr.target.dragging || ptr.busy) return
            if (ptr.pull >= ptr.threshold) { ptr.busy = true; ptr.refresh(); done.restart() }
        }
    }
    Timer { id: done; interval: 1400; onTriggered: ptr.busy = false }
}
