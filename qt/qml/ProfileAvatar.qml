import QtQuick
import QtQuick.Effects
import Reelhouse 1.0

// The round picture of a profile: one of the twelve that come with the app (a colour gradient with a white symbol,
// drawn here, so no picture files are needed) or the person's own photo, a 256 x 256 PNG the core keeps in the
// profile's folder (Core.profiles: `preset`, `photo`).
Item {
    id: av
    property int preset: 0
    property string photo: ""        // the photo on disk; "" = the ready-made picture
    property int size: 32
    property bool ring: false        // a ring in the accent colour around it (the profile in use, the picture chosen)
    implicitWidth: size
    implicitHeight: size
    // [top colour, bottom colour, symbol of MaterialIcons.js] of each ready-made picture (src/profiles.rs PRESETS)
    readonly property var looks: [
        ["#42A5F5", "#1565C0", "person"],
        ["#FFCA28", "#FF8F00", "star"],
        ["#EF5350", "#C62828", "heart"],
        ["#7986CB", "#3949AB", "music"],
        ["#A1887F", "#6D4C41", "paw"],
        ["#BA68C8", "#7B1FA2", "gamepad"],
        ["#66BB6A", "#2E7D32", "puzzle"],
        ["#F06292", "#C2185B", "brush"],
        ["#4DB6AC", "#00796B", "globe"],
        ["#5C6BC0", "#1A237E", "moon"],
        ["#90A4AE", "#455A64", "bolt"],
        ["#FFA726", "#E65100", "smile"]
    ]
    readonly property var look: looks[((preset % looks.length) + looks.length) % looks.length]
    // a photo that cannot be read (deleted, broken) leaves the ready-made picture
    readonly property bool showPhoto: photo !== "" && pic.status === Image.Ready

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        visible: !av.showPhoto
        gradient: Gradient {
            GradientStop { position: 0; color: av.look[0] }
            GradientStop { position: 1; color: av.look[1] }
        }
        Icon { anchors.centerIn: parent; name: av.look[2]; solid: true; size: Math.max(10, Math.round(av.size * 0.56)); color: "#ffffff" }
    }
    Image {
        id: pic
        anchors.fill: parent
        visible: av.showPhoto
        source: av.photo !== "" ? Core.fileUrl(av.photo) : ""
        sourceSize.width: 256
        sourceSize.height: 256
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        smooth: true
        mipmap: true
        // cut round (the RoundedPicture.qml way)
        layer.enabled: av.showPhoto
        layer.effect: MultiEffect {
            maskEnabled: true
            autoPaddingEnabled: false
            maskSource: roundMask
            maskThresholdMin: 0.5
            maskSpreadAtMin: 1.0
        }
    }
    Item {
        id: roundMask
        anchors.fill: parent
        visible: false
        layer.enabled: true
        Rectangle { anchors.fill: parent; radius: width / 2; color: "black" }
    }
    Rectangle {
        visible: av.ring
        anchors.centerIn: parent
        width: parent.width + 6
        height: width
        radius: width / 2
        color: "transparent"
        border.width: 2
        border.color: Theme.c.accent
    }
}
