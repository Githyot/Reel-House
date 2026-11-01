import QtQuick
import Reelhouse 1.0

// Where the reader is in the chapter, at the bottom of the Images and Text readers: "page / pages" over a thin bar.
// A click or a drag on the bar goes to that page (the page under the pointer is shown while dragging).
Item {
    id: pb
    property int current: 1                 // 1 based
    property int count: 1
    property color ink: Theme.c.text        // the text colour of the reader, so it reads on any page colour
    signal seek(int page)                   // 1 based
    implicitHeight: 36
    readonly property bool active: area.containsMouse || area.pressed
    readonly property int shown: area.pressed ? area.target : current
    readonly property real frac: count <= 1 ? 1 : Math.max(0, Math.min(1, (shown - 1) / (count - 1)))

    Text {
        anchors { horizontalCenter: parent.horizontalCenter; bottom: track.top; bottomMargin: 6 }
        text: pb.shown + " / " + pb.count
        color: pb.ink
        opacity: pb.active ? 0.9 : 0.55
        font.family: Theme.type.family
        font.pixelSize: 12
        font.weight: area.pressed ? Font.DemiBold : Font.Normal
    }
    Rectangle {
        id: track
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; bottomMargin: 6 }
        height: pb.active ? 6 : 3
        radius: height / 2
        color: Qt.rgba(pb.ink.r, pb.ink.g, pb.ink.b, 0.18)
        Behavior on height { NumberAnimation { duration: Theme.motion.fast } }
        Rectangle {
            width: Math.max(parent.height, parent.width * pb.frac)
            height: parent.height
            radius: parent.radius
            color: Theme.c.accent
        }
        // the handle, while the pointer is on the bar
        Rectangle {
            visible: pb.active
            width: 14; height: 14; radius: 7
            x: parent.width * pb.frac - width / 2
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.c.accent
            border.width: 2
            border.color: "#ffffff"
        }
    }
    MouseArea {
        id: area
        // taller than the bar, so it is easy to hit
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: 22
        hoverEnabled: true
        preventStealing: true
        cursorShape: Qt.PointingHandCursor
        property int target: 1
        function pageAt(x) { return pb.count <= 1 ? 1 : 1 + Math.round(Math.max(0, Math.min(1, x / Math.max(1, width))) * (pb.count - 1)) }
        onPressed: function(m) { target = pageAt(m.x) }
        onPositionChanged: function(m) { if (pressed) target = pageAt(m.x) }
        onReleased: if (target !== pb.current) pb.seek(target)
    }
}
