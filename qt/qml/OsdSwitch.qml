import QtQuick
import Reelhouse 1.0
import "Osd.js" as Osd

// The on/off switch of the player menus, drawn like the theme's own: libadwaita, Fluent, Material 3 or iOS.
// It only asks (`toggled`); the state that comes back from the player sets `checked`.
Item {
    id: sw
    property bool checked: false
    signal toggled(bool on)
    readonly property string kind: Osd.t(Theme.layout.player).sw
    readonly property int dur: Theme.motion.fast || 120

    implicitWidth: kind === "m3" ? 52 : (kind === "fluent" ? 40 : 46)
    implicitHeight: kind === "m3" ? 32 : (kind === "fluent" ? 20 : 26)
    width: implicitWidth
    height: implicitHeight

    Rectangle {
        id: track
        anchors.fill: parent
        radius: height / 2
        color: {
            if (sw.checked) return sw.kind === "ios" ? "#30d158" : Theme.c.accent
            if (sw.kind === "m3") return "#36343b"
            if (sw.kind === "fluent") return area.containsMouse ? "#0fffffff" : "#00000000"
            return area.containsMouse ? "#4dffffff" : "#3dffffff"
        }
        // Fluent and Material draw an outline round the "off" track
        border.width: sw.checked ? 0 : (sw.kind === "fluent" ? 1 : (sw.kind === "m3" ? 2 : 0))
        border.color: sw.kind === "m3" ? "#938f99" : "#c5ffffff"
        Behavior on color { ColorAnimation { duration: sw.dur } }
    }
    Rectangle {
        id: knob
        // the knob: white (GNOME, Apple), a dot that grows when on (Windows, Material)
        readonly property real d: {
            if (sw.kind === "m3") return sw.checked ? 24 : 16
            if (sw.kind === "fluent") return area.pressed ? 14 : (area.containsMouse ? 14 : 12)
            return sw.height - 6
        }
        width: area.pressed && sw.kind !== "fluent" && sw.kind !== "m3" ? d + 4 : d
        height: d
        radius: d / 2
        anchors.verticalCenter: parent.verticalCenter
        x: sw.checked ? sw.width - width - (sw.height - d) / 2 : (sw.height - d) / 2
        color: {
            if (sw.kind === "m3") return sw.checked ? Theme.c.accentFg : "#938f99"
            if (sw.kind === "fluent") return sw.checked ? Theme.c.accentFg : "#c5ffffff"
            return "#ffffff"
        }
        Behavior on x { NumberAnimation { duration: sw.dur; easing.type: Easing.OutCubic } }
        Behavior on width { NumberAnimation { duration: sw.dur } }
        Behavior on height { NumberAnimation { duration: sw.dur } }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: sw.toggled(!sw.checked)
    }
    Accessible.role: Accessible.CheckBox
    Accessible.checked: checked
}
