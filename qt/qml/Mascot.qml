import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// "Arara", the blue macaw of Reelhouse (a hyacinth macaw, a bit sleepy, with a top hat, a medal and an energy drink).
// The drawings are qt/mascot/arara-<mood>.svg (made by tools/mascot.py); see docs/mascot-contract.md.
//
//     Mascot { mood: "happy"; size: 120; say: "Nice!"; bubbleSide: "right" }
//
// Besides the moods there are the poses of the animations: fly (wings flapping), shh (a wing tip on the beak), peek
// (a magnifying glass on one eye), pull (wings up, holding an edge above, the grip line ~20% down the picture) and
// push (leaning to the right, wings out). `mirror` turns the bird to face the other way (the bubble stays readable).
//
// `size` is the height of the drawing (the drawing is square). The speech bubble sits outside that box, on
// `bubbleSide`, so leave room for it. With `animate` the bird bobs gently, blinks now and then, and flaps for
// "cheer" and "wave".
Item {
    id: root
    property string mood: "idle"
    property real size: 120
    property string say: ""
    property string bubbleSide: "right"   // right | left | top
    property bool animate: true
    property bool mirror: false
    // A click on the bird: it hops, flaps and answers with one of its funny lines (src/mascot_lines.rs, moment
    // "tap") for a few seconds, over whatever it was saying. `tappable: false` turns that off.
    property bool tappable: true
    property string pokeSay: ""
    property string pokeMood: "surprised"
    readonly property bool poked: pokeSay !== ""
    function poke() {
        const r = Core.state("mascot:" + Core.uiLang + ":tap")
        pokeSay = r && r.text ? r.text : (pokeSay !== "" ? pokeSay : "!")
        pokeMood = "surprised"
        hopAnim.restart()
        pokeMoodWait.restart()
        pokeEnd.restart()
    }
    // Moods that have drawings; anything else shows the idle bird.
    readonly property var moods: ["idle", "happy", "cheer", "sad", "tired", "sleep", "think", "wave", "surprised", "fly", "shh", "peek", "pull", "push"]
    readonly property string shown: poked ? pokeMood : (moods.indexOf(mood) >= 0 ? mood : "idle")
    // the poses with a second frame (-b): cheer and wave flap in bursts, the others all the time (fly fast, pull and
    // push as slow tugs)
    readonly property bool flaps: shown === "cheer" || shown === "wave" || shown === "fly" || shown === "pull" || shown === "push"
    readonly property bool bursts: shown === "cheer" || shown === "wave"
    readonly property int flapEvery: shown === "fly" ? 120 : (shown === "pull" || shown === "push" ? 300 : 210)
    readonly property bool blinks: shown !== "happy" && shown !== "sleep"
    // The bubble, so a parent can make room for it.
    readonly property alias bubble: bubble

    implicitWidth: size
    implicitHeight: size

    // 0 = the mood, 1 = eyes shut (blink), 2 = the other wing position (flap)
    property int frame: 0
    // SVG, or the WebP copies when this Qt cannot read SVG (no image plugin for it): decided once, by the first image.
    property bool noSvg: false
    function file(suffix) {
        return root.noSvg ? "qrc:/mascot/webp/arara-" + root.shown + suffix + ".webp" : "qrc:/mascot/arara-" + root.shown + suffix + ".svg"
    }

    Item {
        id: bird
        width: root.size
        height: root.size
        // breathing for the sleepy bird, a small bob for the others (from the feet, so it stays on the ground)
        transformOrigin: Item.Bottom
        // squash: the hop's stretch (wider and shorter when landing, taller when leaving the ground); tilt: a wobble
        property real squash: 0
        property real tilt: 0
        property real hop: 0
        transform: [
            Scale { origin.x: root.size / 2; xScale: root.mirror ? -1 : 1 },
            Scale { origin.x: root.size / 2; origin.y: root.size; xScale: 1 + bird.squash; yScale: 1 - bird.squash },
            Rotation { origin.x: root.size / 2; origin.y: root.size; angle: bird.tilt }
        ]
        property real bob: 0
        y: -bob - hop

        // the click: a hop with a stretch, a wobble, and a line in the bubble (see `poke`)
        MouseArea {
            anchors.fill: parent
            enabled: root.tappable
            cursorShape: Qt.PointingHandCursor
            onClicked: root.poke()
        }
        SequentialAnimation {
            id: hopAnim
            ParallelAnimation {
                SequentialAnimation {
                    NumberAnimation { target: bird; property: "squash"; to: 0.14; duration: 90; easing.type: Easing.OutQuad }
                    NumberAnimation { target: bird; property: "squash"; to: -0.1; duration: 120; easing.type: Easing.OutQuad }
                    NumberAnimation { target: bird; property: "hop"; to: root.size * 0.22; duration: 220; easing.type: Easing.OutQuad }
                    NumberAnimation { target: bird; property: "hop"; to: 0; duration: 260; easing.type: Easing.InQuad }
                    NumberAnimation { target: bird; property: "squash"; to: 0.12; duration: 80; easing.type: Easing.OutQuad }
                    NumberAnimation { target: bird; property: "squash"; to: 0; duration: 260; easing.type: Easing.OutElastic; easing.amplitude: 1.4 }
                }
                SequentialAnimation {
                    PauseAnimation { duration: 180 }
                    NumberAnimation { target: bird; property: "tilt"; to: -9; duration: 120; easing.type: Easing.InOutSine }
                    NumberAnimation { target: bird; property: "tilt"; to: 8; duration: 160; easing.type: Easing.InOutSine }
                    NumberAnimation { target: bird; property: "tilt"; to: -5; duration: 160; easing.type: Easing.InOutSine }
                    NumberAnimation { target: bird; property: "tilt"; to: 0; duration: 180; easing.type: Easing.OutBack }
                }
            }
        }
        // surprised for the hop, then a cheer (flapping) while the line shows, then back to the mood it had
        Timer { id: pokeMoodWait; interval: 520; onTriggered: if (root.poked) root.pokeMood = "cheer" }
        Timer { id: pokeEnd; interval: 4500; onTriggered: { root.pokeSay = ""; root.pokeMood = "surprised" } }
        Repeater {
            // All frames stay loaded and only the visible one changes, so swapping never flickers.
            model: root.flaps ? 3 : (root.blinks ? 2 : 1)
            Image {
                required property int index
                anchors.fill: parent
                source: root.file(index === 0 ? "" : (index === 1 ? "-blink" : "-b"))
                sourceSize: Qt.size(Math.ceil(root.size * 2), Math.ceil(root.size * 2))
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
                visible: index === root.frame || (index === 0 && status !== Image.Ready)
                onStatusChanged: if (status === Image.Error && !root.noSvg) root.noSvg = true
            }
        }

        SequentialAnimation on bob {
            running: root.animate && root.visible && root.shown !== "sleep"
            loops: Animation.Infinite
            NumberAnimation { from: 0; to: root.size * 0.025; duration: 900; easing.type: Easing.InOutSine }
            NumberAnimation { from: root.size * 0.025; to: 0; duration: 900; easing.type: Easing.InOutSine }
        }
        SequentialAnimation on scale {
            running: root.animate && root.visible && root.shown === "sleep"
            loops: Animation.Infinite
            NumberAnimation { from: 1; to: 1.035; duration: 1600; easing.type: Easing.InOutSine }
            NumberAnimation { from: 1.035; to: 1; duration: 1800; easing.type: Easing.InOutSine }
        }
    }
    onShownChanged: { frame = 0; flapCount = 0 }
    onAnimateChanged: if (!animate) { frame = 0; bird.bob = 0; bird.scale = 1 }
    onVisibleChanged: if (!visible && poked) { hopAnim.stop(); bird.squash = 0; bird.tilt = 0; bird.hop = 0; pokeSay = ""; pokeMood = "surprised" }

    // Blink every few seconds (a random pause, so several macaws on screen do not blink together).
    Timer {
        id: blinkWait
        running: root.animate && root.visible && root.blinks && !root.flaps
        interval: 2600 + Math.random() * 3000
        repeat: true
        onTriggered: { interval = 2600 + Math.random() * 3000; root.frame = 1; blinkEnd.restart() }
    }
    Timer { id: blinkEnd; interval: 140; onTriggered: if (root.frame === 1) root.frame = 0 }

    // Cheer and wave: a burst of flaps, a rest, again. Fly, pull, push: the two frames one after the other.
    property int flapCount: 0
    Timer {
        running: root.animate && root.visible && root.flaps
        interval: !root.bursts ? root.flapEvery : (root.flapCount < 8 ? 210 : 1600)
        repeat: true
        onTriggered: {
            if (!root.bursts) { root.frame = root.frame === 2 ? 0 : 2; return }
            if (root.flapCount >= 8) { root.flapCount = 0; root.frame = 2; return }
            root.frame = root.frame === 2 ? 0 : 2
            root.flapCount++
            if (root.flapCount >= 8) root.frame = 0
        }
    }

    // ---- the speech bubble ----
    // Its colour is solid (the theme's popover laid over the window colour): a see-through popover let the page show
    // through and the line could not be read.
    readonly property color bubbleColor: Qt.tint(Theme.c.window !== undefined ? Theme.c.window : "#202020", Theme.c.popover)
    // The bubble of `say` sits in the page (the page leaves room for it). A line from a click with nothing being said
    // opens over everything instead (a popup), since the page left no room there and it would go under its neighbours.
    readonly property bool pokeFloats: poked && say === ""
    Popup {
        id: pokePop
        parent: root
        visible: root.pokeFloats && root.visible
        x: bubble.px
        y: bubble.py
        width: bubble.width
        height: bubble.height
        padding: 0
        modal: false
        focus: false
        closePolicy: Popup.NoAutoClose
        background: null
        contentItem: Item {}
        onClosed: if (root.pokeFloats) { root.pokeSay = ""; root.pokeMood = "surprised" }
    }
    Item {
        id: bubble
        readonly property bool open: root.say !== "" || root.poked
        // over everything (in the popup) for a click's line, in the page otherwise
        parent: root.pokeFloats && pokePop.visible ? pokePop.contentItem : root
        readonly property real px: root.bubbleSide === "left" ? -width - tail + root.size * 0.06
           : root.bubbleSide === "top" ? (root.size - width) / 2
           : root.size * 0.94 + tail
        readonly property real py: root.bubbleSide === "top" ? -height - tail : Math.max(-height * 0.3, root.size * 0.22 - height / 2)
        readonly property real maxW: Math.max(140, Math.min(300, root.size * 2.4))
        readonly property real tail: 9
        width: Math.min(maxW, words.implicitWidth + 28)
        height: words.implicitHeight + 20
        visible: opacity > 0.01
        opacity: open ? 1 : 0
        scale: open ? 1 : 0.85
        Behavior on opacity { NumberAnimation { duration: Theme.motion.fast || 150 } }
        Behavior on scale { NumberAnimation { duration: Theme.motion.normal || 200; easing.type: Easing.OutBack } }
        transformOrigin: root.bubbleSide === "left" ? Item.Right : (root.bubbleSide === "top" ? Item.Bottom : Item.Left)
        x: parent === root ? px : 0
        y: parent === root ? py : 0

        // the pointer towards the bird (a turned square under the box)
        Rectangle {
            width: bubble.tail * 1.6
            height: width
            rotation: 45
            color: box.color
            border.width: 1
            border.color: box.border.color
            x: root.bubbleSide === "left" ? bubble.width - width / 2 - 1
               : root.bubbleSide === "top" ? bubble.width / 2 - width / 2
               : -width / 2 + 1
            y: root.bubbleSide === "top" ? bubble.height - width / 2 - 1 : Math.min(bubble.height - width - 6, Math.max(6, bubble.height / 2 - width / 2))
        }
        Rectangle {
            id: box
            anchors.fill: parent
            radius: Math.min(height / 2, Theme.shape.card !== undefined ? Theme.shape.card + 4 : 14)
            color: root.bubbleColor
            border.width: 1
            border.color: Theme.c.border
        }
        Text {
            id: words
            anchors.centerIn: parent
            width: Math.min(implicitWidth, bubble.maxW - 28)
            text: root.poked ? root.pokeSay : root.say
            wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter
            color: Theme.c.text
            font.family: Theme.type.family
            font.pixelSize: Math.max(12, Math.min(15, root.size * 0.13))
            font.weight: Font.Medium
        }
    }
}
