import QtQuick
import QtQuick.Effects
import Reelhouse 1.0

// A picture cut to a rounded rectangle.
Item {
    id: rp
    property string url: ""
    property real radius: Theme.shape.poster
    property color placeholder: Theme.c.placeholder
    property alias status: pic.status
    property alias fillMode: pic.fillMode
    property real zoom: 1             // the picture grows inside its frame (GTK: .poster-card:hover .poster-pic scale 1.12)
    property string fallback: ""
    property string title: ""         // a cover can be found by the name when the address does not work
    Behavior on zoom { NumberAnimation { duration: Theme.motion.fast; easing.type: Easing.OutCubic } }
    property real blur: 0             // 0..1 (episodes not watched yet are blurred so they do not spoil the story)
    Rectangle { anchors.fill: parent; radius: rp.radius; color: rp.placeholder }
    PosterImage {
        id: pic
        anchors.centerIn: parent
        width: rp.width * rp.zoom
        height: rp.height * rp.zoom
        url: rp.url
        fallback: rp.fallback
        title: rp.title
        sourceSize.width: Math.round(rp.width * (Theme.mobile ? 1.5 : 2))
        layer.enabled: pic.status === Image.Ready
        layer.effect: MultiEffect {
            maskEnabled: true
            blurEnabled: rp.blur > 0
            blur: rp.blur
            blurMax: 32
            autoPaddingEnabled: false
            maskSource: maskItem
            maskThresholdMin: 0.5
            maskSpreadAtMin: 1.0
        }
    }
    Item {
        id: maskItem
        anchors.fill: pic
        visible: false
        layer.enabled: true
        Rectangle { anchors.centerIn: parent; width: rp.width; height: rp.height; radius: rp.radius; color: "black" }
    }
}
