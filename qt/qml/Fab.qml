import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The floating action button of a page on a phone (Material 3 FAB: 56 dp, 16 dp corners, the primary container colour;
// extended with a label when `label` is set). Shown only where the theme wants FABs (Theme.mob.fab): the iOS family
// keeps the primary action in the bar instead. Pages put it at the bottom right, above the navigation bar.
Rectangle {
    id: fab
    property string glyph: "plus"
    property string label: ""
    property string tip: ""
    property bool enabledBtn: true
    signal clicked()
    readonly property bool wanted: Theme.mobile === true && Theme.mob !== undefined && Theme.mob.fab === true
    readonly property int sz: Theme.mob !== undefined && Theme.mob.fabSize !== undefined ? Theme.mob.fabSize : 56
    visible: wanted
    width: label !== "" ? row.implicitWidth + 40 : sz
    height: sz
    radius: Theme.mob !== undefined && Theme.mob.fabRadius !== undefined ? Theme.mob.fabRadius : 16
    color: Theme.c.fab !== undefined ? Theme.c.fab : Theme.c.accent
    opacity: enabledBtn ? 1 : 0.5
    scale: area.pressed ? 0.95 : 1
    Behavior on scale { NumberAnimation { duration: 90 } }
    z: 20
    // a soft shadow under it (M3 level 3), drawn as a blurred-looking stack of rectangles: no effect layer on a phone
    Repeater {
        model: 3
        delegate: Rectangle {
            required property int index
            z: -1
            anchors.fill: parent
            anchors.margins: -index * 1.5
            anchors.topMargin: -index * 1.5 + 2 + index
            radius: fab.radius + index * 1.5
            color: "#000000"
            opacity: 0.07
        }
    }
    // the state layer of the hover and press (8 % / 12 % of the icon colour)
    Rectangle { anchors.fill: parent; radius: parent.radius; color: fab.ink; opacity: area.pressed ? 0.12 : (area.containsMouse ? 0.08 : 0) }
    readonly property color ink: Theme.c.onFab !== undefined ? Theme.c.onFab : Theme.c.accentFg
    Row {
        id: row
        anchors.centerIn: parent
        spacing: 12
        Icon { name: fab.glyph; size: 24; color: fab.ink; anchors.verticalCenter: parent.verticalCenter }
        Text { visible: fab.label !== ""; text: fab.label; color: fab.ink; font.pixelSize: 14; font.weight: Font.Medium; font.letterSpacing: 0.1; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        enabled: fab.enabledBtn
        onClicked: fab.clicked()
        onPressAndHold: if (fab.tip !== "") fab.ToolTip.show(fab.tip, 2500)
    }
    ToolTip.text: tip
    Accessible.role: Accessible.Button
    Accessible.name: label !== "" ? label : tip
}
