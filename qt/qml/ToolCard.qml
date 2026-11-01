import QtQuick
import Reelhouse 1.0

// One tool of a reader's sheet (the Images reader's "Editor"): its icon and its name on a card, in the shape of the
// theme cards of "Themes & Settings" (ThemeCard.qml). The tool in use is marked the way each theme marks a choice:
// Apple a ring around it, the others an accent outline (thicker on Material), and the accent on the icon.
Item {
    id: tc
    property string name: ""
    property string glyph: ""
    property bool selected: false
    signal clicked()
    readonly property string tid: Theme.look
    readonly property real r: tid === "apple" ? 14 : (tid === "android" ? 16 : (tid === "windows" ? 6 : 12))
    implicitWidth: 120
    implicitHeight: tid === "windows" ? 92 : 84

    // the selection ring (Apple): around the card, with a gap
    Rectangle {
        visible: tc.tid === "apple" && tc.selected
        anchors.fill: card
        anchors.margins: -4
        radius: tc.r + 4
        color: "transparent"
        border.width: 2.5
        border.color: Theme.c.accent
    }
    Rectangle {
        id: card
        anchors.fill: parent
        anchors.margins: tc.tid === "apple" ? 4 : 0
        radius: tc.r
        color: tc.selected && tc.tid !== "apple" ? Theme.c.accent20 : (area.containsMouse ? Theme.c.cardHover : Theme.c.card)
        border.width: tc.tid === "apple" ? 0.5 : (tc.selected ? (tc.tid === "android" ? 3 : 2) : 1)
        border.color: tc.selected && tc.tid !== "apple" ? Theme.c.accent : (Theme.scheme === "dark" ? "#33ffffff" : "#26000000")
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            y: (parent.height - height) / 2 - 11
            name: tc.glyph
            size: 26
            color: tc.selected ? Theme.c.accent : Theme.c.text
        }
        Text {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 6; bottomMargin: 9 }
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            text: tc.name
            color: Theme.c.text
            font.pixelSize: 13
            font.weight: tc.selected ? Font.DemiBold : Font.Normal
            font.family: Theme.type.family
        }
    }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: tc.clicked() }
}
