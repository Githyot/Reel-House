import QtQuick
import Reelhouse 1.0

// A category as a button (GTK .genre-chip): accent-tinted pill; a click opens the page of that category.
Rectangle {
    id: g
    property alias text: t.text
    property bool clickable: true
    signal clicked()
    radius: height / 2
    color: clickable && area.containsMouse ? Theme.c.accent50 : Theme.c.accent25
    implicitWidth: t.implicitWidth + 22
    implicitHeight: 26
    Text { id: t; anchors.centerIn: parent; color: Theme.c.text; font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family }
    MouseArea { id: area; anchors.fill: parent; enabled: g.clickable; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: g.clicked() }
}
