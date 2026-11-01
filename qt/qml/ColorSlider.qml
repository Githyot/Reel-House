import QtQuick
import Reelhouse 1.0

// A slider whose track shows the colours it chooses between. `kind` is the look: adw (Libadwaita), fluent, m3, apple.
Item {
    id: sl
    property real from: 0
    property real to: 1
    property real value: 0
    property string kind: "adw"
    property bool rainbow: false
    property bool checker: false
    property color c0: "#000000"
    property color c1: "#ffffff"
    signal moved(real v)

    readonly property var rain: ["#ff0000", "#ffff00", "#00ff00", "#00ffff", "#0000ff", "#ff00ff", "#ff0000"]
    function stopColor(i) {
        if (rainbow) return rain[i]
        var t = i / 6
        return Qt.rgba(c0.r + (c1.r - c0.r) * t, c0.g + (c1.g - c0.g) * t, c0.b + (c1.b - c0.b) * t, c0.a + (c1.a - c0.a) * t)
    }
    readonly property real trackH: kind === "m3" ? 16 : (kind === "apple" ? 14 : (kind === "fluent" ? 10 : 12))
    readonly property real hw: kind === "m3" ? 6 : (kind === "apple" ? 26 : (kind === "fluent" ? 20 : 20))
    readonly property real frac: to > from ? Math.max(0, Math.min(1, (value - from) / (to - from))) : 0
    readonly property color mid: stopColor(3)

    implicitHeight: kind === "m3" ? 40 : (kind === "apple" ? 30 : 26)
    implicitWidth: 200
    activeFocusOnTab: true
    Keys.onLeftPressed: moved(Math.max(from, value - (to - from) / 50))
    Keys.onRightPressed: moved(Math.min(to, value + (to - from) / 50))

    function at(x) {
        var t = Math.max(0, Math.min(1, (x - hw / 2) / Math.max(1, width - hw)))
        moved(from + t * (to - from))
    }

    Item {
        id: trk
        width: parent.width
        height: sl.trackH
        anchors.verticalCenter: parent.verticalCenter
        Grid {
            visible: sl.checker
            anchors.fill: parent
            columns: Math.ceil(parent.width / 7)
            clip: true
            Repeater {
                model: sl.checker ? Math.ceil(parent.width / 7) * Math.ceil(sl.trackH / 7) : 0
                delegate: Rectangle { width: 7; height: 7; color: ((index % Math.ceil(trk.width / 7)) + Math.floor(index / Math.ceil(trk.width / 7))) % 2 === 0 ? "#bbbbbb" : "#ffffff" }
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: height / 2
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0 / 6; color: sl.stopColor(0) }
                GradientStop { position: 1 / 6; color: sl.stopColor(1) }
                GradientStop { position: 2 / 6; color: sl.stopColor(2) }
                GradientStop { position: 3 / 6; color: sl.stopColor(3) }
                GradientStop { position: 4 / 6; color: sl.stopColor(4) }
                GradientStop { position: 5 / 6; color: sl.stopColor(5) }
                GradientStop { position: 6 / 6; color: sl.stopColor(6) }
            }
            border.width: 1
            border.color: Theme.c.border
        }
        // The checker shows through the see-through end of an alpha slider; round its ends too.
        Rectangle { visible: sl.checker; anchors.fill: parent; radius: height / 2; color: "transparent"; border.width: 3; border.color: Theme.c.window }
    }

    // The handle.
    Rectangle {
        id: knob
        x: sl.frac * (sl.width - sl.hw)
        width: sl.hw
        height: sl.kind === "m3" ? 40 : sl.hw
        anchors.verticalCenter: parent.verticalCenter
        radius: sl.kind === "m3" ? 3 : width / 2
        color: sl.kind === "m3" ? Theme.c.accent : (sl.kind === "fluent" ? Theme.c.card : "#ffffff")
        border.width: sl.kind === "fluent" ? 2 : (sl.kind === "adw" ? 2 : (sl.kind === "apple" ? 0 : 0))
        border.color: sl.kind === "fluent" ? "#ffffff" : (sl.activeFocus ? Theme.c.accent : "#ffffff")
        Rectangle { visible: sl.kind === "m3"; x: -4; width: parent.width + 8; height: parent.height; radius: 5; color: "transparent"; border.width: 4; border.color: Theme.c.window; z: -1 }
        Rectangle { visible: sl.kind === "fluent"; anchors.centerIn: parent; width: parent.width - 8; height: width; radius: width / 2; color: sl.mid }
        Rectangle { visible: sl.kind === "adw" || sl.kind === "apple"; anchors.centerIn: parent; width: parent.width - (sl.kind === "apple" ? 8 : 8); height: width; radius: width / 2; color: sl.mid; opacity: sl.kind === "apple" ? 1 : 0.0 }
        Rectangle { visible: sl.kind === "apple"; anchors.fill: parent; radius: width / 2; color: "transparent"; border.width: 1; border.color: "#33000000"; z: -1 }
    }
    // A soft shadow under round handles.
    Rectangle { visible: sl.kind === "apple" || sl.kind === "adw"; x: knob.x; y: knob.y + 1; width: knob.width; height: knob.height; radius: knob.radius; color: "#40000000"; z: -1 }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onPressed: function(m) { sl.forceActiveFocus(); sl.at(m.x) }
        onPositionChanged: function(m) { if (pressed) sl.at(m.x) }
    }
}
