import QtQuick
import Reelhouse 1.0

// A row of a reader's sheet that does something (save, open, show a file): an icon, its name and an arrow, in the
// shape of "More options" / "Customize" of "Themes & Settings".
Rectangle {
    id: sl
    property string glyph: ""
    property string text: ""
    signal clicked()
    width: parent ? parent.width : 360
    height: 48
    radius: Theme.look === "apple" ? 12 : (Theme.look === "android" ? 24 : Theme.shape.control)
    color: area.containsMouse ? Theme.c.cardHover : Theme.c.card
    border.width: Theme.look === "windows" || Theme.look === "gnome" ? 1 : 0
    border.color: Theme.c.border
    Row {
        anchors { left: parent.left; leftMargin: 14; right: chev.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
        spacing: 10
        Icon { id: ic; name: sl.glyph; size: 18; anchors.verticalCenter: parent.verticalCenter }
        Lbl { text: sl.text; size: 15; width: parent.width - ic.width - 10; elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter }
    }
    Icon { id: chev; anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter } name: "right"; size: 18; color: Theme.c.textDim }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sl.clicked() }
}
