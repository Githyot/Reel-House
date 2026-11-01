import QtQuick
import Reelhouse 1.0

// Rounded label: `blue` for genres, plain for the year and length.
Rectangle {
    id: chip
    property alias text: t.text
    property bool blue: false
    property color fg: blue ? Theme.c.chipBlueText : Theme.c.text
    signal clicked()
    radius: Theme.shape.pill
    color: blue ? Theme.c.chipBlue : Theme.c.pillDim
    implicitWidth: t.implicitWidth + 30
    implicitHeight: 28
    Text {
        id: t
        anchors.centerIn: parent
        color: chip.fg
        font.pixelSize: 14
        font.weight: Font.DemiBold
        font.family: Theme.type.family
    }
    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: chip.clicked() }
}
