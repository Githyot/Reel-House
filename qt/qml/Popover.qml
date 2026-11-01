import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A libadwaita popover: drawn inside the window (rounded, see-through corners), opened under its button.
// On a phone (Theme.mobile) every popover is a bottom sheet instead (Material 3 "bottom sheet", iOS sheet with a
// grabber): full width, rising from the bottom edge over a scrim, its rows centred, clear of the home indicator.
Popup {
    id: pop
    property real anchorX: 0
    property real anchorY: 0
    property bool alignRight: false
    readonly property bool sheet: Theme.mobile === true
    readonly property real safeBottom: Theme.safe !== undefined && Theme.safe.bottom !== undefined ? Theme.safe.bottom : 0
    readonly property bool grabber: sheet && Theme.mob !== undefined && Theme.mob.grabber === true
    // HyperOS: the sheet floats clear of the screen edges (Theme.mob.sheetMargin), every corner rounded
    readonly property bool floating: sheet && Theme.mob !== undefined && Theme.mob.floatingSheet === true
    readonly property real sheetMargin: floating ? (Theme.mob.sheetMargin !== undefined ? Theme.mob.sheetMargin : 12) : 0
    readonly property real ovW: Overlay.overlay ? Overlay.overlay.width : 1000
    readonly property real ovH: Overlay.overlay ? Overlay.overlay.height : 800
    parent: Overlay.overlay
    padding: 8
    topPadding: grabber ? 24 : (sheet ? 12 : 8)
    bottomPadding: sheet ? (floating ? 16 : 12 + safeBottom) : 8
    // the sheet is as wide as the window; its content (menus are built at a fixed width) sits in the middle
    leftPadding: sheet ? Math.max(8, (width - contentWidth) / 2) : 8
    rightPadding: sheet ? Math.max(8, (width - contentWidth) / 2) : 8
    width: sheet ? ovW - 2 * sheetMargin : implicitWidth
    focus: true
    modal: sheet
    dim: sheet
    Overlay.modal: Rectangle { color: Theme.c.scrim }
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    x: sheet ? sheetMargin : Math.max(8, Math.min(alignRight ? anchorX - width : anchorX, ovW - width - 8))
    // the popup never leaves the window (a button at the bottom of a sidebar opens it upward)
    y: sheet ? ovH - height - (floating ? sheetMargin + safeBottom : 0) : Math.max(8, Math.min(anchorY, ovH - height - 8))
    function openBelow(btn, right) {
        const pt = btn.mapToItem(Overlay.overlay, 0, btn.height + 6)
        alignRight = !!right
        anchorX = alignRight ? pt.x + btn.width : pt.x
        anchorY = pt.y
        open()
    }
    // beside a button of the sidebar: to its right, top edges aligned
    function openRight(btn) {
        const pt = btn.mapToItem(Overlay.overlay, btn.width + 8, 0)
        alignRight = false
        anchorX = pt.x
        anchorY = pt.y
        open()
    }
    function openAt(item, px, py) {
        const pt = item.mapToItem(Overlay.overlay, px, py)
        alignRight = false
        anchorX = pt.x
        anchorY = pt.y
        open()
    }
    // GNOME: a solid popover. Apple: a glass panel that blurs what is behind it (see Glass.qml). Phone: the sheet.
    background: Item {
        Glass { visible: !pop.sheet; anchors.fill: parent; radius: Theme.shape.popover !== undefined ? Theme.shape.popover : 12; level: "regular"; shadow: true }
        Rectangle {
            visible: pop.sheet
            anchors.fill: parent
            color: Theme.c.popover
            topLeftRadius: Theme.shape.sheet !== undefined ? Theme.shape.sheet : 16
            topRightRadius: Theme.shape.sheet !== undefined ? Theme.shape.sheet : 16
            bottomLeftRadius: pop.floating ? topLeftRadius : 0
            bottomRightRadius: pop.floating ? topRightRadius : 0
            border.width: Theme.glass ? 1 : 0
            border.color: Theme.c.glassBorder
            // iOS: the grabber (a 36x5 pill) that says the sheet can be dragged away
            Rectangle { visible: pop.grabber; anchors { top: parent.top; topMargin: 6; horizontalCenter: parent.horizontalCenter } width: 36; height: 5; radius: 2.5; color: Theme.c.grabber !== undefined ? Theme.c.grabber : Theme.c.textDim }
        }
    }
    // a swipe down on the sheet closes it
    DragHandler {
        enabled: pop.sheet
        target: null
        yAxis.enabled: true
        xAxis.enabled: false
        onActiveChanged: if (!active && translation.y > 60) pop.close()
    }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.motion.fast } }
}
