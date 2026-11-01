import QtQuick
import Reelhouse 1.0

// Window button. GNOME: a round button. Windows (Fluent): a 46x32 caption button, the close one turns red.
Rectangle {
    id: wb
    property string icon: "close"
    readonly property bool fluent: Theme.layout.captionButtons === "fluent"
    signal clicked()
    // reachable with a controller or Tab (close of Preferences and dialogs): Space / Enter press it, a ring shows it
    activeFocusOnTab: true
    Keys.onSpacePressed: wb.clicked()
    Keys.onReturnPressed: wb.clicked()
    Keys.onEnterPressed: wb.clicked()
    border.width: activeFocus ? 2 : 0
    border.color: Theme.c.accent
    readonly property bool m3: Theme.layout.captionButtons === "m3"
    width: fluent ? 46 : (m3 ? 40 : 26)
    height: fluent ? 32 : (m3 ? 40 : 26)
    radius: fluent ? 0 : (m3 ? 20 : 13)
    color: m3 ? (area.containsMouse ? Theme.c.hover : "transparent") : fluent
        ? (area.containsMouse ? (icon === "close" ? "#c42b1c" : Theme.c.fill) : "transparent")
        : (area.containsMouse ? Theme.c.fill : Theme.c.hover)
    Icon { anchors.centerIn: parent; name: wb.icon; size: wb.fluent ? 12 : (wb.m3 ? 18 : 14); color: wb.fluent && wb.icon === "close" && area.containsMouse ? "#ffffff" : Theme.c.text }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; onClicked: wb.clicked() }
}
