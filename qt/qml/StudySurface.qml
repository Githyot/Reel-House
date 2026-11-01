import QtQuick
import QtQuick.Effects
import Reelhouse 1.0

// A surface of the Study mode, drawn the way of the theme:
// GNOME a 12 px card with a hair line (boxed list), Windows (Fluent) an 8 px card with a 1 px stroke,
// Material 3 a tonal surface with large corners (16, 28 when raised), Apple an inset-grouped card or glass.
// `raised` adds a soft shadow (the card of the review) so it reads over the wallpaper / aurora.
Item {
    id: sf
    property bool raised: false
    property bool hovered: false
    default property alias content: inner.data
    readonly property string tid: Theme.look
    readonly property real corner: tid === "android" ? (raised ? 28 : 16)
                                 : tid === "apple" ? (raised ? Theme.shape.large : 12)
                                 : tid === "windows" ? 8
                                 : 12
    readonly property color fill: hovered ? Theme.c.cardHover : Theme.c.card
    readonly property bool stroke: tid === "windows" || tid === "gnome"

    // soft shadow under raised surfaces: stacked translucent rounded rectangles, not an effect layer (a layer that
    // changes size, as the review card does, showed green boxes on some graphics cards)
    Repeater {
        model: sf.raised ? 4 : 0
        delegate: Rectangle {
            required property int index
            anchors.fill: parent
            anchors.margins: -index * 3
            anchors.topMargin: -index * 3 + 8
            anchors.bottomMargin: -index * 3 - 8
            radius: sf.corner + index * 3
            color: "black"
            opacity: (Theme.scheme === "light" ? 0.07 : 0.16) / (1 + index * 0.4)
        }
    }
    // Apple: the thick material over the wallpaper (falls back to the solid card)
    Glass {
        visible: Theme.glass && sf.tid === "apple"
        anchors.fill: parent
        radius: sf.corner
        level: "thick"
        solid: sf.fill
    }
    Rectangle {
        visible: !(Theme.glass && sf.tid === "apple")
        anchors.fill: parent
        radius: sf.corner
        color: sf.fill
        border.width: sf.stroke ? 1 : 0
        border.color: sf.tid === "windows" ? Theme.c.border : Theme.c.divider
        Behavior on color { ColorAnimation { duration: Theme.motion.fast } }
    }
    Item { id: inner; anchors.fill: parent }
}
