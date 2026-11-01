import QtQuick
import Reelhouse 1.0

// The macaw's own stage over the whole window (the pages and the player): it comes in at a corner at the right
// moment and goes away again. Main.qml hosts one and feeds it Core's `mascotCue` signal; anything in QML can call
// Core.mascotCue({cue, corner, ms, text}). See docs/mascot-contract.md.
//
//   cue "shh"    hops in and holds a wing tip on its beak, until `ms` runs out or a "hide" cue (default: stays)
//   cue "peek"   only the magnifying glass and the big eye poke in from the side edge, discreetly (default 4.5 s)
//   cue "cheer" | "wave" | "happy" | "sad" | "think"   pops in at the corner, with `text` in a speech bubble (3.5 s)
//   cue "hide"   flies away upwards (the peek slips back out of the edge instead)
// corner: "bottom-right" (default) | "bottom-left" | "top-right" | "top-left". A click on the bird sends it away.
// Only the bird takes the mouse; the rest of the window stays usable.
Item {
    id: stage
    // room kept free at the bottom (the controls of the player) and at the sides
    property real bottomInset: 24
    property real topInset: 16
    property real sideInset: 20
    // reduced motion: the bird fades in and out instead of hopping and flying
    property bool calm: false

    // the cue on screen ("" = nobody)
    property string cue: ""
    property string corner: "bottom-right"
    property string text: ""
    property string mood: "idle"
    // true once the bird stands in its place (the speech bubble only shows then)
    property bool settled: false
    property bool leaving: false

    readonly property bool onRight: corner.indexOf("left") < 0
    readonly property bool onTop: corner.indexOf("top") === 0
    readonly property real size: cue === "peek" ? 124 : 108
    readonly property real homeX: onRight ? width - size - sideInset : sideInset
    readonly property real homeY: onTop ? topInset : height - size - bottomInset
    // the peek: the bird leans in from the side edge at the very corner, so only its head (glass and eye) is inside
    // the window (the body stays below the bottom edge)
    readonly property real peekX: onRight ? width - size * 0.46 : -size * 0.54
    readonly property real peekY: onTop ? -size * 0.2 : height - size * 0.64
    readonly property var cues: ["shh", "peek", "cheer", "wave", "happy", "sad", "think"]

    function play(c) {
        const k = c && c.cue ? String(c.cue) : ""
        if (k === "hide") { leave(); return }
        if (cues.indexOf(k) < 0) return
        const where = c.corner ? String(c.corner) : "bottom-right"
        const ms = c.ms !== undefined && c.ms !== null ? Number(c.ms) : (k === "shh" ? 0 : (k === "peek" ? 4500 : 3500))
        stay.stop()
        // the bird is already there, in the same kind of place: it only changes its pose and words
        const same = cue !== "" && !leaving && settled && (k === "peek") === (cue === "peek") && where === corner
        corner = where
        text = c.text ? String(c.text) : ""
        cue = k
        mood = k
        if (same) bounce.restart()
        else enter()
        if (ms > 0) { stay.interval = ms; stay.restart() }
    }

    function stopAll() { hopIn.stop(); popIn.stop(); peekIn.stop(); flyAway.stop(); slipOut.stop(); fadeOut.stop(); bounce.stop() }

    function enter() {
        stopAll()
        leaving = false
        settled = false
        actor.scale = 1
        actor.rotation = 0
        actor.opacity = 1
        if (calm) {
            actor.x = cue === "peek" ? peekX : homeX
            actor.y = cue === "peek" ? peekY : homeY
            actor.rotation = cue === "peek" ? (onRight ? -14 : 14) : 0
            actor.opacity = 0
            fadeIn.restart()
            return
        }
        if (cue === "peek") {
            actor.x = onRight ? width + 10 : -size - 10
            actor.y = peekY
            actor.rotation = onRight ? -14 : 14
            peekIn.restart()
        } else if (cue === "shh") {
            actor.x = homeX
            actor.y = onTop ? -size - 20 : height + 20
            hopIn.restart()
        } else {
            actor.x = homeX
            actor.y = homeY
            actor.scale = 0.2
            actor.opacity = 0
            popIn.restart()
        }
    }

    function leave() {
        stay.stop()
        if (cue === "" || leaving) return
        stopAll()
        leaving = true
        settled = false
        if (calm) { fadeOut.restart(); return }
        if (cue === "peek") { slipOut.restart(); return }
        mood = "fly"
        flyAway.restart()
    }
    function gone() { cue = ""; leaving = false; settled = false; actor.opacity = 1 }

    Timer { id: stay; onTriggered: stage.leave() }

    Item {
        id: actor
        visible: stage.cue !== ""
        width: stage.size
        height: stage.size
        transformOrigin: Item.Bottom
        Mascot {
            id: bird
            anchors.fill: parent
            size: stage.size
            mood: stage.mood
            // the bird faces the middle of the window from a left corner too
            mirror: !stage.onRight
            animate: actor.visible
            say: stage.settled && stage.cue !== "peek" ? stage.text : ""
            bubbleSide: stage.onRight ? "left" : "right"
        }
        MouseArea {
            anchors.fill: parent
            enabled: actor.visible && !stage.leaving
            cursorShape: Qt.PointingHandCursor
            onClicked: stage.leave()
        }
    }

    // ---- the ways in ----
    // shh: a hop up from the edge, a small bounce, then it holds
    SequentialAnimation {
        id: hopIn
        NumberAnimation { target: actor; property: "y"; to: stage.homeY - (stage.onTop ? -26 : 34); duration: 300; easing.type: Easing.OutQuad }
        NumberAnimation { target: actor; property: "y"; to: stage.homeY; duration: 220; easing.type: Easing.InQuad }
        NumberAnimation { target: actor; property: "scale"; from: 1; to: 0.94; duration: 70 }
        NumberAnimation { target: actor; property: "scale"; to: 1; duration: 180; easing.type: Easing.OutBack }
        ScriptAction { script: stage.settled = true }
    }
    // the moods: a pop at the corner
    SequentialAnimation {
        id: popIn
        ParallelAnimation {
            NumberAnimation { target: actor; property: "scale"; to: 1; duration: 380; easing.type: Easing.OutBack; easing.overshoot: 2.2 }
            NumberAnimation { target: actor; property: "opacity"; to: 1; duration: 160 }
        }
        ScriptAction { script: stage.settled = true }
    }
    // peek: slides in from the side edge, slowly, like someone trying not to be seen
    SequentialAnimation {
        id: peekIn
        NumberAnimation { target: actor; property: "x"; to: stage.peekX; duration: 700; easing.type: Easing.OutCubic }
        ScriptAction { script: stage.settled = true }
    }
    // a new pose while it is already there: a little jump
    SequentialAnimation {
        id: bounce
        NumberAnimation { target: actor; property: "scale"; to: 1.12; duration: 110; easing.type: Easing.OutQuad }
        NumberAnimation { target: actor; property: "scale"; to: 1; duration: 240; easing.type: Easing.OutBack }
    }
    NumberAnimation { id: fadeIn; target: actor; property: "opacity"; to: 1; duration: 200; onFinished: stage.settled = true }

    // ---- the ways out ----
    // flies away upwards, drifting towards the middle
    SequentialAnimation {
        id: flyAway
        PauseAnimation { duration: 140 }
        ParallelAnimation {
            NumberAnimation { target: actor; property: "y"; to: -stage.size - 80; duration: 900; easing.type: Easing.InQuad }
            NumberAnimation { target: actor; property: "x"; to: actor.x + (stage.onRight ? -90 : 90); duration: 900; easing.type: Easing.InOutSine }
            NumberAnimation { target: actor; property: "rotation"; to: stage.onRight ? -10 : 10; duration: 500 }
        }
        ScriptAction { script: stage.gone() }
    }
    SequentialAnimation {
        id: slipOut
        NumberAnimation { target: actor; property: "x"; to: stage.onRight ? stage.width + 10 : -stage.size - 10; duration: 420; easing.type: Easing.InCubic }
        ScriptAction { script: stage.gone() }
    }
    SequentialAnimation {
        id: fadeOut
        NumberAnimation { target: actor; property: "opacity"; to: 0; duration: 220 }
        ScriptAction { script: stage.gone() }
    }

    Connections {
        target: Core
        function onMascotCued(c) { stage.play(c) }
    }
}
