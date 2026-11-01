import QtQuick
import Reelhouse 1.0

// One card of the readers' theme grid: "Aa" (or a page sample) in the theme's colours and font, its name under it.
// The chosen card is marked the way each theme marks a choice: Apple a ring around it, GNOME a check badge,
// Windows a radio circle (radio-card), Material a thick outline with a check.
Item {
    id: tc
    property string name: ""
    property color bg: "#ffffff"
    property color fg: "#000000"
    property string family: ""
    property bool heavy: false
    property bool grain: false
    property bool selected: false
    property string sample: "Aa"
    property bool pageSample: false     // a small page with panels instead of "Aa" (the Images reader)
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
        color: tc.bg
        clip: true
        border.width: tc.tid === "apple" ? 0.5 : (tc.selected && tc.tid !== "apple" ? (tc.tid === "android" ? 3 : 2) : 1)
        border.color: tc.selected && tc.tid !== "apple" ? Theme.c.accent : (Theme.scheme === "dark" ? "#33ffffff" : "#26000000")
        PaperGrain { visible: tc.grain; anchors.fill: parent; speck: tc.fg }
        // a tiny comic page: white paper with three panels
        Rectangle {
            visible: tc.pageSample
            anchors.horizontalCenter: parent.horizontalCenter
            y: (parent.height - height) / 2 - 9
            width: 26; height: 36; radius: 2
            color: "#fafafa"
            border.width: 1; border.color: "#33000000"
            Column {
                x: 3; y: 3; spacing: 2
                Rectangle { width: 20; height: 12; color: "#c9c9c9" }
                Row { spacing: 2; Rectangle { width: 9; height: 14; color: "#dadada" } Rectangle { width: 9; height: 14; color: "#b9b9b9" } }
            }
        }
        Text {
            visible: !tc.pageSample
            anchors.horizontalCenter: parent.horizontalCenter
            y: (parent.height - height) / 2 - 9
            text: tc.sample
            color: tc.fg
            font.family: tc.family !== "" ? tc.family : Theme.type.family
            font.pixelSize: 26
            font.weight: tc.heavy ? Font.Bold : Font.Normal
        }
        Text {
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 9 }
            width: Math.min(implicitWidth, parent.width - 12)
            elide: Text.ElideRight
            text: tc.name
            color: tc.fg
            opacity: 0.85
            font.family: Theme.type.family
            font.pixelSize: 12
            font.weight: tc.selected ? Font.DemiBold : Font.Normal
        }
        // Windows: the radio circle of a radio card
        Rectangle {
            visible: tc.tid === "windows"
            anchors { right: parent.right; top: parent.top; margins: 7 }
            width: 16; height: 16; radius: 8
            color: tc.selected ? Theme.c.accent : "transparent"
            border.width: tc.selected ? 0 : 1
            border.color: tc.fg
            opacity: tc.selected ? 1 : 0.6
            Rectangle { visible: tc.selected; anchors.centerIn: parent; width: 6; height: 6; radius: 3; color: Theme.c.accentFg }
        }
        // GNOME and Material: a check badge on the chosen card
        Rectangle {
            visible: (tc.tid === "gnome" || tc.tid === "android") && tc.selected
            anchors { right: parent.right; top: parent.top; margins: 6 }
            width: 20; height: 20; radius: 10
            color: Theme.c.accent
            Icon { anchors.centerIn: parent; name: "check"; size: 14; color: Theme.c.accentFg }
        }
        // the pointer over the card
        Rectangle { anchors.fill: parent; radius: parent.radius; color: tc.fg; opacity: area.containsMouse ? 0.06 : 0 }
    }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: tc.clicked() }
}
