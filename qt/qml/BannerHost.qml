import QtQuick
import QtQuick.Effects
import Reelhouse 1.0

// Phone-style pop-ups at the top of the window with the macaw (study reminders, the Pomodoro timer, greetings).
// Main.qml puts one above everything (the dialogs and the player too) and feeds it Core's `banner` signal; QML can also
// call Core.showBanner({kind, title, body, mood, action}). See docs/mascot-contract.md.
//
// One banner at a time, the others wait in a queue. The macaw brings it: it drops in from the top, flying, grabs the
// banner (still above the window) by its lower edge and pulls it down into view, then hops into its place on the
// banner; when the banner leaves, it flies off upwards with it. The banner stays ~5 s (longer while the pointer is
// on it), and goes away with ×, a swipe up, or a click (which first sends `action` with Core.cmd). With `calm`
// (reduced motion) the banner only slides, as plain notifications do.
// It is drawn in the style of the theme: Apple notification (glass, radius 18), GNOME libadwaita card, Windows toast
// (card with an accent bar), Android heads-up notification (radius 28).
Item {
    id: host
    // Set by Main while the player is on screen: the glass of the Apple theme has nothing to blur there.
    property bool overVideo: false
    // How long a banner stays (ms).
    property int stay: 5200
    property var queue: []
    property var current: null
    readonly property string look: Theme.look === "apple" || Theme.look === "windows" || Theme.look === "android" ? Theme.look : "gnome"
    // reduced motion: no flying macaw, the banner slides in and out
    property bool calm: false

    // ---- the macaw's delivery, driven by two numbers so every frame follows from them ----
    // t: 0..1 it flies down to the edge of the window, 1..2 it pulls the banner down, 2..3 it hops into its place
    // u: 0..1 it flies away upwards, the banner going up after it
    property real t: 0
    property real u: 0
    readonly property real flyer: 84                 // the size of the flying bird
    readonly property real slotSize: 64              // the size of the bird on the banner
    readonly property real grip: 0.26                // where the wings hold the edge, down the pull picture
    function ease(x) { return 1 - Math.pow(1 - Math.max(0, Math.min(1, x)), 3) }
    function easeInOut(x) { x = Math.max(0, Math.min(1, x)); return x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2 }
    function easeIn(x) { x = Math.max(0, Math.min(1, x)); return x * x * x }
    function lerp(a, b, k) { return a + (b - a) * k }
    // how far the banner is in (0 above the window, 1 in place)
    readonly property real slide: calm ? card.calmSlide : (u > 0 ? 1 - easeIn(u) : easeInOut(t - 1))
    // just above the window (the calm slide starts further up, so the shadow is not seen either); the macaw grabs the
    // lower edge there, its body already in view under it
    readonly property real hiddenY: calm ? -card.height - 30 : -card.height - 2
    // where the bird sits on the banner (once it is in place)
    readonly property real slotX: card.x + content.x
    readonly property real slotY: card.topGap + (card.height - slotSize) / 2
    // under the banner, holding its lower edge: the same spot whatever the banner's height
    readonly property real pullX: slotX + slotSize / 2 - flyer / 2
    function pullY(cardY) { return cardY + card.height - flyer * grip }

    // Only the banner itself takes the mouse; the rest of the window stays usable.
    enabled: current !== null

    function show(b) {
        if (!b || (!b.title && !b.body)) return
        const q = queue.slice()
        // the same message twice in a row (two timers, a double event) is shown once
        const last = q.length > 0 ? q[q.length - 1] : current
        if (last && last.title === b.title && last.body === b.body) return
        q.push(b)
        // a long queue only keeps the newest few
        queue = q.slice(-4)
        if (current === null) next()
    }
    function next() {
        if (queue.length === 0) { current = null; return }
        const q = queue.slice()
        current = q.shift()
        queue = q
        // the daily greeting: the macaw also waves from the corner
        if (current.kind === "info" && current.mood === "wave") corner.greet()
        card.dragY = 0
        card.shown = true
        exitAnim.stop()
        u = 0
        t = 0
        if (!calm) enterAnim.restart()
        else t = 3
        hideTimer.restart()
    }
    function dismiss() {
        if (!card.shown) return
        hideTimer.stop()
        card.shown = false
        if (calm) { afterHide.restart(); return }
        // the bird flies away (from wherever it was in its delivery), then the next banner
        enterAnim.stop()
        t = 3
        exitAnim.restart()
    }
    function press() {
        const a = current ? current.action : null
        if (a && a.cmd) Core.cmd(a)
        dismiss()
    }

    // The window is up: tell the core the time zone and language (for the reminders), and let it greet once a day.
    function hello() { Core.cmd({cmd: "banner_hello", tz: -new Date().getTimezoneOffset(), lang: Core.uiLang}) }
    Timer { interval: 1500; running: true; onTriggered: host.hello() }
    Connections { target: Core; function onUiLangChanged() { host.hello() } }

    Timer {
        id: hideTimer
        interval: host.stay
        onTriggered: if (hover.containsMouse) restart(); else host.dismiss()
    }
    Timer { id: afterHide; interval: (Theme.motion.normal || 250) + 60; onTriggered: host.next() }
    SequentialAnimation {
        id: enterAnim
        NumberAnimation { target: host; property: "t"; from: 0; to: 1; duration: 420 }
        NumberAnimation { target: host; property: "t"; from: 1; to: 2; duration: 560 }
        NumberAnimation { target: host; property: "t"; from: 2; to: 3; duration: 320 }
    }
    SequentialAnimation {
        id: exitAnim
        NumberAnimation { target: host; property: "u"; from: 0; to: 1; duration: 720 }
        ScriptAction { script: { host.u = 0; host.t = 0; host.next() } }
    }

    Item {
        id: card
        property bool shown: false
        property real dragY: 0
        // the slide of the calm mode (animated by the Behavior below)
        property real calmSlide: shown ? 1 : 0
        Behavior on calmSlide { enabled: host.calm; NumberAnimation { duration: Theme.motion.normal || 250; easing.type: Easing.OutCubic } }
        readonly property real topGap: host.look === "android" ? 10 : (host.look === "windows" ? 14 : 12)
        readonly property real radius: host.look === "apple" ? 18 : (host.look === "android" ? 28 : (host.look === "windows" ? 8 : 12))
        width: Math.min(host.width - 24, host.look === "android" ? 420 : (host.look === "windows" ? 380 : 400))
        height: Math.max(84, content.implicitHeight + (host.look === "android" ? 28 : 24))
        anchors.horizontalCenter: parent.horizontalCenter
        y: host.lerp(host.hiddenY, topGap, host.slide) + Math.min(0, dragY)
        opacity: host.calm ? calmSlide : 1
        visible: host.slide > 0.001 && host.current !== null

        // ---- the background in the style of the theme ----
        // a soft shadow under it (all themes float the banner)
        Rectangle {
            anchors.fill: parent
            radius: card.radius
            color: Theme.c.popover
            layer.enabled: card.visible
            layer.effect: MultiEffect {
                shadowEnabled: true
                shadowColor: Theme.elevation.shadowColor || "#59000000"
                shadowBlur: 1.0
                shadowVerticalOffset: Theme.elevation.shadowOffsetY !== undefined ? Theme.elevation.shadowOffsetY : 8
                autoPaddingEnabled: true
            }
        }
        // Apple: glass (over the video, a dark see-through card instead, there is nothing to blur)
        Glass {
            visible: host.look === "apple" && !host.overVideo
            anchors.fill: parent
            radius: card.radius
            level: "regular"
            solid: Theme.c.popover
        }
        Rectangle {
            visible: host.look !== "apple" || host.overVideo
            anchors.fill: parent
            radius: card.radius
            color: host.look === "apple" ? (Theme.scheme === "light" ? "#eef5f5f7" : "#e62c2c2e")
                 : host.look === "android" ? (Theme.c.surface2 || Theme.c.popover)
                 : Theme.c.popover
            border.width: host.look === "android" ? 0 : 1
            border.color: Theme.c.border
        }
        // Windows: the accent bar along the left edge of the toast
        Rectangle {
            visible: host.look === "windows"
            x: 6; width: 3; radius: 1.5
            anchors { top: parent.top; bottom: parent.bottom; topMargin: 14; bottomMargin: 14 }
            color: Theme.c.accent
        }

        // ---- the content: macaw, (app line), title, text, × ----
        Row {
            id: content
            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: host.look === "windows" ? 16 : 12; rightMargin: 40 }
            spacing: 10
            Mascot {
                id: bird
                anchors.verticalCenter: parent.verticalCenter
                size: host.slotSize
                mood: host.current && host.current.mood ? host.current.mood : "idle"
                animate: card.shown
                // the flying bird stands in for this one while it brings the banner and while it leaves
                opacity: host.calm || (host.t >= 3 && host.u === 0) ? 1 : 0
            }
            Column {
                width: content.width - bird.width - content.spacing
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                // The line with the app name, as phone notifications have (Apple, Android, Windows).
                Row {
                    visible: host.look !== "gnome"
                    spacing: 6
                    Text {
                        text: host.look === "apple" ? Core.tr(Core.uiLang, "Reelhouse").toUpperCase() : Core.tr(Core.uiLang, "Reelhouse")
                        color: Theme.c.textDim
                        font.family: Theme.type.family
                        font.pixelSize: 11
                        font.weight: host.look === "android" ? Font.Medium : Font.Normal
                        font.letterSpacing: host.look === "apple" ? 0.4 : 0
                    }
                    Text { text: "·"; color: Theme.c.textDim; font.pixelSize: 11; visible: host.look === "android" }
                    Text {
                        text: Core.tr(Core.uiLang, "now")
                        color: Theme.c.textDim
                        font.family: Theme.type.family
                        font.pixelSize: 11
                        visible: host.look !== "windows"
                    }
                }
                Text {
                    width: parent.width
                    text: host.current ? (host.current.title || "") : ""
                    visible: text !== ""
                    color: Theme.c.text
                    font.family: Theme.type.family
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                    maximumLineCount: 2
                    wrapMode: Text.WordWrap
                }
                Text {
                    width: parent.width
                    text: host.current ? (host.current.body || "") : ""
                    visible: text !== ""
                    color: host.look === "apple" ? Theme.c.text : Theme.c.textDim
                    font.family: Theme.type.family
                    font.pixelSize: 13
                    wrapMode: Text.WordWrap
                    maximumLineCount: 3
                    elide: Text.ElideRight
                    lineHeight: 1.1
                }
            }
        }

        // click = the action; a swipe up = away (one MouseArea, so a press never reaches the page under the banner)
        MouseArea {
            id: hover
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: host.current && host.current.action ? Qt.PointingHandCursor : Qt.ArrowCursor
            property real startY: 0
            property bool moved: false
            onPressed: function(m) { startY = m.y; moved = false }
            onPositionChanged: function(m) {
                if (!pressed) return
                const d = m.y - startY
                if (Math.abs(d) > 6) moved = true
                card.dragY = d
            }
            onReleased: {
                if (card.dragY < -24) host.dismiss()
                else if (!moved) host.press()
                card.dragY = 0
            }
            onCanceled: card.dragY = 0
        }

        // ×
        Rectangle {
            id: closeBtn
            anchors { right: parent.right; top: parent.top; rightMargin: 10; topMargin: 10 }
            width: 24; height: 24; radius: 12
            color: closeHover.containsMouse ? Theme.c.hover || "#22808080" : "transparent"
            Icon { anchors.centerIn: parent; name: "close"; size: 14; color: Theme.c.textDim }
            MouseArea { id: closeHover; anchors.fill: parent; hoverEnabled: true; onClicked: host.dismiss() }
        }
    }

    // ---- the macaw that brings the banner and takes it away ----
    Mascot {
        id: courier
        readonly property real pulledY: host.pullY(card.y)
        visible: !host.calm && host.current !== null && (host.t < 3 || host.u > 0)
        size: host.u > 0 ? host.lerp(host.slotSize, host.flyer, host.ease(host.u * 2))
             : host.t <= 2 ? host.flyer : host.lerp(host.flyer, host.slotSize, host.ease(host.t - 2))
        // flying down, pulling, flying up into its place (its own mood only as it lands), flying away
        mood: host.u > 0 || host.t < 1 ? "fly" : (host.t < 2.05 ? "pull" : (host.t < 2.8 ? "fly" : (host.current && host.current.mood ? host.current.mood : "idle")))
        animate: visible
        x: host.u > 0 ? host.slotX + host.u * 40
           : host.t <= 2 ? host.pullX
           : host.lerp(host.pullX, host.slotX, host.ease(host.t - 2))
        y: host.u > 0 ? host.lerp(host.slotY, -host.flyer - 80, host.easeIn(host.u * 1.25))
           : host.t < 1 ? host.lerp(-host.flyer - 40, host.pullY(host.hiddenY), host.ease(host.t))
           : host.t <= 2 ? pulledY
           // the hop: up into the slot with a little arc
           : host.lerp(host.pullY(card.y), host.slotY, host.ease(host.t - 2)) - Math.sin(Math.PI * Math.min(1, host.t - 2)) * 18
    }

    // ---- the small macaw that waves from the bottom right corner with the greeting ----
    Mascot {
        id: corner
        property bool up: false
        function greet() { up = true; cornerTimer.restart() }
        size: 92
        mood: "wave"
        animate: up
        say: up ? Core.tr(Core.uiLang, "Hi!") : ""
        bubbleSide: "left"
        x: host.width - size - 18
        y: up ? host.height - size - 14 : host.height + 20
        visible: y < host.height
        Behavior on y { NumberAnimation { duration: Theme.motion.slow || 400; easing.type: Easing.OutBack } }
        Timer { id: cornerTimer; interval: 4500; onTriggered: corner.up = false }
        MouseArea { anchors.fill: parent; enabled: corner.up; onClicked: corner.up = false }
    }
}
