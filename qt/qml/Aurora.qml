import QtQuick
import QtQuick.Effects
import Reelhouse 1.0

// A moving background behind the title of a page (GTK aurora.rs): soft blobs of colour taken from the cover that
// glide to a new cover and, when "Moving background" is on, drift all the time. Fades out toward the bottom.
Item {
    id: au
    property var colours: []          // four "#rrggbb" strings (from Core.moodReady)
    property bool enabledLook: true
    property bool motion: false
    readonly property bool dark: Theme.scheme !== "light"
    visible: enabledLook
    clip: true

    // One blob per colour: a blurred disc that glides between colours and, while moving, between spots.
    // The disc is drawn small (a 192 px texture) and scaled up: blurring a disc the size of the window, four
    // times, every frame while the colours move was most of the cost of this background.
    Repeater {
        model: 4
        delegate: Item {
            id: blob
            required property int index
            readonly property real reach: Math.max(au.width, au.height)
            readonly property real r: reach * (0.40 + 0.04 * index)
            width: 2 * r
            height: 2 * r
            // Rest spot, then the drift loop around it.
            x: au.width * (0.5 + 0.42 * Math.sin(index * 1.7)) - r
            y: au.height * (0.42 + 0.38 * Math.cos(index * 2.3)) - r
            Rectangle {
                id: disc
                readonly property int px: 192
                anchors.centerIn: parent
                width: px
                height: px
                radius: px / 2
                scale: blob.r > 0 ? (2 * blob.r) / px : 1
                color: au.colours.length > blob.index ? au.colours[blob.index] : "#6070c8"
                opacity: au.dark ? 0.5 : 0.36
                Behavior on color { ColorAnimation { duration: 1400; easing.type: Easing.InOutQuad } }
                layer.enabled: true
                layer.smooth: true
                // the same soft edge (96 px on screen) whatever the size of the window
                layer.effect: MultiEffect { blurEnabled: true; blur: 1.0; blurMax: Math.min(64, Math.max(4, Math.round(96 / disc.scale))) }
            }

            SequentialAnimation on x {
                running: au.motion && au.enabledLook
                loops: Animation.Infinite
                NumberAnimation { to: au.width * (0.5 + 0.42 * Math.sin(blob.index * 1.7 + 2.0)) - blob.r; duration: 16000 + blob.index * 3000; easing.type: Easing.InOutSine }
                NumberAnimation { to: au.width * (0.5 + 0.42 * Math.sin(blob.index * 1.7)) - blob.r; duration: 16000 + blob.index * 3000; easing.type: Easing.InOutSine }
            }
            SequentialAnimation on y {
                running: au.motion && au.enabledLook
                loops: Animation.Infinite
                NumberAnimation { to: au.height * (0.42 + 0.38 * Math.cos(blob.index * 2.3 + 2.0)) - blob.r; duration: 19000 + blob.index * 2500; easing.type: Easing.InOutSine }
                NumberAnimation { to: au.height * (0.42 + 0.38 * Math.cos(blob.index * 2.3)) - blob.r; duration: 19000 + blob.index * 2500; easing.type: Easing.InOutSine }
            }
        }
    }

    // Fades the colours into the page toward the bottom.
    Rectangle {
        anchors.fill: parent
        z: 10
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.alpha(Theme.c.window, 0.0) }
            GradientStop { position: 0.45; color: Qt.alpha(Theme.c.window, 0.35) }
            GradientStop { position: 1.0; color: Theme.c.window }
        }
    }
}
