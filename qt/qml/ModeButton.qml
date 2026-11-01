import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The floating button of the Images, Text and Study readers, in the shape of the active theme:
// GNOME round OSD button, Windows (Fluent) rounded square with a hair line, Android (Material) tonal FAB,
// Apple a translucent circle. "on" uses the theme accent.
Rectangle {
    id: mb
    property string glyph: ""
    property string tip: ""
    property bool on: false
    property bool enabledBtn: true
    signal clicked()
    readonly property string ps: Theme.layout.player || "adw"
    // the floating back / cover buttons over a page are a little smaller than the reader's
    property int diameter: ps === "m3" ? 48 : 44
    implicitWidth: diameter
    implicitHeight: implicitWidth
    width: implicitWidth; height: implicitHeight
    radius: ps === "fluent" ? (Theme.shape.control || 4) + 2 : (ps === "m3" ? 16 : width / 2)
    color: !enabledBtn ? "#55000000"
         : on ? Theme.c.accent
         : ps === "m3" ? (ma.containsMouse ? Qt.lighter(Theme.c.surface2 || "#2b2930", 1.25) : (Theme.c.surface2 || "#2b2930"))
         : ps === "glass" ? (ma.containsMouse ? "#b31c1c1e" : "#801c1c1e")
         : ps === "fluent" ? (ma.containsMouse ? "#e62c2c2c" : "#cc202020")
         : (ma.containsMouse ? "#cc000000" : "#99000000")
    border.width: ps === "fluent" || ps === "glass" ? 1 : 0
    border.color: "#26ffffff"
    opacity: enabledBtn ? 1 : 0.5
    scale: ma.pressed ? 0.94 : 1
    Behavior on color { ColorAnimation { duration: 100 } }
    Behavior on scale { NumberAnimation { duration: 90 } }
    Icon { anchors.centerIn: parent; name: mb.glyph; size: Math.round(mb.diameter / 2); color: mb.on ? (Theme.c.accentFg || "#ffffff") : (mb.ps === "m3" ? Theme.c.text : "#ffffff") }
    MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; enabled: mb.enabledBtn; cursorShape: Qt.PointingHandCursor; onClicked: mb.clicked(); onPressAndHold: if (Theme.touch && mb.tip !== "") mb.ToolTip.show(mb.tip, 2500) }
    ToolTip.visible: ma.containsMouse && tip !== ""
    ToolTip.text: tip
}
