import QtQuick
import Reelhouse 1.0

// The big button of the Learn area. Playful themes (Android, Apple) give it a darker bottom edge that it sinks into
// when pressed, like a game button; GNOME and Windows keep it flat with the theme's corner.
Item {
    id: b
    property string text: ""
    property string glyph: ""
    property color tone: Theme.c.accent
    property color fg: Theme.c.accentFg
    // a light card-like button with a border instead of a filled one
    property bool outline: false
    property int fontSize: 15
    readonly property bool playful: Theme.look === "android" || Theme.look === "apple"
    readonly property int depth: playful ? 4 : 0
    readonly property real rad: playful ? 16 : Math.max(6, Theme.shape.control)
    readonly property bool down: ma.pressed
    signal clicked()
    implicitHeight: 46 + depth
    implicitWidth: Math.max(110, row.implicitWidth + 40)
    opacity: enabled ? 1 : 0.45
    activeFocusOnTab: true
    Keys.onSpacePressed: b.clicked()

    // the bottom edge (playful themes)
    Rectangle {
        visible: b.depth > 0
        anchors.fill: parent
        radius: b.rad
        color: b.outline ? Theme.c.border : Qt.darker(b.tone, 1.3)
    }
    Rectangle {
        id: face
        width: parent.width
        height: parent.height - b.depth
        y: b.down ? b.depth : 0
        radius: b.rad
        color: b.outline ? (ma.containsMouse ? Theme.c.cardHover : Theme.c.card) : (ma.containsMouse && b.enabled ? Qt.lighter(b.tone, 1.07) : b.tone)
        border.width: b.outline ? 2 : (b.activeFocus ? 2 : 0)
        border.color: b.activeFocus ? Theme.c.text : Theme.c.border
        Behavior on y { NumberAnimation { duration: 60 } }
        Row {
            id: row
            anchors.centerIn: parent
            spacing: 8
            LearnGlyph { visible: b.glyph !== ""; name: b.glyph; size: 18; color: b.outline ? Theme.c.text : b.fg; anchors.verticalCenter: parent.verticalCenter }
            Text {
                text: b.text
                color: b.outline ? Theme.c.text : b.fg
                font.pixelSize: b.fontSize
                font.weight: Font.Bold
                font.family: Theme.type.family
                font.capitalization: b.playful ? Font.AllUppercase : Font.MixedCase
                font.letterSpacing: b.playful ? 0.6 : 0
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
    MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; enabled: b.enabled; cursorShape: Qt.PointingHandCursor; onClicked: b.clicked() }
}
