import QtQuick
import Reelhouse 1.0

// Turns a page of a reader with an animation, the way Apple Books offers it:
//   "slide" - the old page slides away sideways, showing the new one under it;
//   "curl"  - the old page folds over its spine (a rotation in perspective) with a shading that deepens as it lifts
//             and a soft shadow on the page underneath;
//   "none"  - the page changes at once.
// How: a picture of the reader is taken first (grabToImage), then the page is changed under it, then the picture is
// animated away. Place it over the reader's view (same size, above it) and call turn(change, toLeft).
Item {
    id: pt
    property Item target: null
    property string style: "slide"
    property bool busy: anim.running || waiting
    property bool waiting: false
    property bool toLeft: true        // the old page leaves to the left (forward in a left-to-right book)
    property var shotResult: null     // keeps the grabbed picture alive while it is shown
    property real t: 0                // 0 → 1 while the page goes away
    property color backColor: "transparent"   // the reader's background, under the picture (the view itself is see-through)
    property bool upward: false       // vertical readers: the slide goes up or down instead of sideways
    enabled: false                    // never takes the pointer from the reader

    function turn(change, leftward) {
        if (style === "none" || !target || !target.visible || waiting || target.width < 2 || target.height < 2) { change(); return }
        if (anim.running) { anim.complete() }
        waiting = true
        const ok = target.grabToImage(function(res) {
            pt.waiting = false
            pt.shotResult = res
            shot.source = res.url
            pt.toLeft = leftward
            change()
            pt.t = 0
            anim.restart()
        })
        if (!ok) { waiting = false; change() }
    }

    // the shadow the lifting page casts on the new page, near the spine
    Rectangle {
        visible: anim.running && pt.style === "curl"
        width: parent.width * 0.35 * (1 - pt.t)
        height: parent.height
        x: pt.toLeft ? 0 : parent.width - width
        opacity: 0.55 * (1 - pt.t)
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: pt.toLeft ? "#80000000" : "#00000000" }
            GradientStop { position: 1; color: pt.toLeft ? "#00000000" : "#80000000" }
        }
    }

    Item {
        id: leaf
        visible: anim.running
        width: pt.width
        height: pt.height
        // slide: straight out to the side (or up and down in a vertical reader)
        x: pt.style === "slide" && !pt.upward ? (pt.toLeft ? -1 : 1) * pt.width * pt.t : 0
        y: pt.style === "slide" && pt.upward ? (pt.toLeft ? -1 : 1) * pt.height * pt.t : 0
        Rectangle { anchors.fill: parent; color: pt.backColor }
        Image { id: shot; anchors.fill: parent; cache: false; smooth: true }
        transform: Rotation {
            // curl: the page folds round its spine, the far edge first
            origin.x: pt.toLeft ? 0 : leaf.width
            origin.y: leaf.height / 2
            axis { x: 0; y: 1; z: 0 }
            angle: pt.style === "curl" ? (pt.toLeft ? 1 : -1) * 92 * pt.t : 0
        }
        // the slide drags a thin shadow at its trailing edge
        Rectangle {
            visible: pt.style === "slide" && !pt.upward
            width: 24
            height: parent.height
            x: pt.toLeft ? parent.width : -width
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: pt.toLeft ? "#40000000" : "#00000000" }
                GradientStop { position: 1; color: pt.toLeft ? "#00000000" : "#40000000" }
            }
        }
        // curl: the lifting page darkens towards its fold, a light band runs along the bend
        Rectangle {
            visible: pt.style === "curl"
            anchors.fill: parent
            opacity: Math.min(1, pt.t * 1.6)
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: pt.toLeft ? "#59000000" : "#00000000" }
                GradientStop { position: 0.55; color: "#14ffffff" }
                GradientStop { position: 1.0; color: pt.toLeft ? "#00000000" : "#59000000" }
            }
        }
    }

    NumberAnimation {
        id: anim
        target: pt
        property: "t"
        from: 0
        to: 1
        duration: pt.style === "curl" ? 520 : 300
        easing.type: pt.style === "curl" ? Easing.InOutSine : Easing.OutCubic
        onStopped: { shot.source = ""; pt.shotResult = null }
    }
}
