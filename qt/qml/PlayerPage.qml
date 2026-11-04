import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQml
import QtQuick.Effects
import Reelhouse 1.0
import "Accel.js" as Accel
import "Osd.js" as Osd
import QtQuick.Shapes
import QtQuick.Dialogs

// Over the video (VideoItem in Main.qml), like the GTK player: the preparing / error panel, the top bar, the control bar
// with its menus (quality, audio, subtitles, speed, Anime4K, video scale), the skip-opening button and the next-episode card.
Item {
    id: page
    property var window
    readonly property var p: Core.player
    readonly property string stage: p.stage ? p.stage : ""
    readonly property bool playing: stage === "playing"
    // The look of everything over the video, per theme (Osd.js): GNOME adw, Windows fluent, Android m3, Apple glass.
    readonly property string ps: Theme.layout.player || "adw"
    readonly property var o: Osd.t(ps)
    readonly property color accentInk: Osd.readable(Theme.c.accent)   // the accent as text on the dark panels
    property bool shown: true
    function openEditor(s, e) {
        if (s !== undefined && e !== undefined) { page.clipStart = s; page.clipEnd = e } else page.clipAroundPlayhead()
        editorPop.openAbove(edBtn)
    }
    // The part the Editor opens on, where the playhead is: the subtitle line on screen when there is one, otherwise
    // from 2 s before the playhead to 4 s after it. A part already chosen that holds the playhead is kept (closing and
    // opening the Editor again must not lose it).
    function clipAroundPlayhead() {
        const t = page.p.pos || 0
        const d = page.p.dur || 0
        if (page.clipStart >= 0 && page.clipEnd > page.clipStart && t >= page.clipStart - 1 && t <= page.clipEnd + 1) return
        const ss = page.p.subStart !== undefined ? page.p.subStart : -1
        const se = page.p.subEnd !== undefined ? page.p.subEnd : -1
        if (ss >= 0 && se > ss + 0.2 && t >= ss - 0.5 && t <= se + 0.5) { page.clipStart = ss; page.clipEnd = se; return }
        page.clipStart = Math.max(0, t - 2)
        page.clipEnd = d > 0 ? Math.min(d, t + 4) : t + 4
    }
    property bool pinned: false          // test aid: keep the controls on screen
    property bool remaining: false
    // The menus and popovers open right now, read from the popups themselves. (A counter stepped by their opened /
    // closed signals went wrong: a popup closed during its fade-in fires closed without opened, the count went below
    // zero and the controls never hid again.)
    readonly property int menusOpen: (qMenu.visible ? 1 : 0) + (aMenu.visible ? 1 : 0) + (uMenu.visible ? 1 : 0) + (vsMenu.visible ? 1 : 0) + (moreMenu.visible ? 1 : 0) + (page.window && page.window.feedbackOpen ? 1 : 0)
        + (picker.visible ? 1 : 0) + (subsMenu.visible ? 1 : 0) + (editorPop.visible ? 1 : 0) + (speedPop.visible ? 1 : 0) + (statsPop.visible ? 1 : 0)
    onMenusOpenChanged: poke()
    property bool sideOpen: false
    readonly property bool paused: p.paused === true
    // playing again (by the button, the key or a click on the video) starts the hiding of the controls afresh
    onPausedChanged: poke()
    onVisibleChanged: if (!visible) {
        sideOpen = false; thumbReset()   // a new video starts with an empty picture cache
        // leaving the player closes every menu / popover it opened
        qMenu.close(); aMenu.close(); uMenu.close(); vsMenu.close(); picker.close()
        subsMenu.close(); editorPop.close(); speedPop.close(); statsPop.close()
    }
    // controls auto-hide once playback starts
    onPlayingChanged: if (playing) poke()
    function fmt(s) {
        s = Math.max(0, Math.floor(s || 0))
        var h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60), x = s % 60
        return (h > 0 ? h + ":" + (m < 10 ? "0" : "") : "") + m + ":" + (x < 10 ? "0" : "") + x
    }
    function poke() { page.shown = true; hide.restart() }
    // a real move of the pointer (2 px or more) shows the bar; Qt Quick also sends hover events while the picture
    // changes under a still pointer, and those kept the bar up (HP: it never hid)
    property point lastPointer: Qt.point(-1, -1)
    function pointerAt(item, x, y) {
        const p = item.mapToItem(page, x, y)
        if (Math.abs(p.x - lastPointer.x) < 2 && Math.abs(p.y - lastPointer.y) < 2) return
        lastPointer = Qt.point(p.x, p.y)
        poke()
    }
    function toggleFullscreen() { page.window.visibility === Window.FullScreen ? page.window.showNormal() : page.window.showFullScreen() }
    function arr(v) { return v ? v : [] }
    onStageChanged: console.log("player stage:", stage, p.error ? p.error : "", p.message ? p.message : "")

    Timer {
        id: hide
        // HP: the bar and the pointer go 2 s after the mouse stops, even with the pointer resting on the bar. An open
        // menu or side panel holds them, and the timer checks again instead of leaving the bar up until the next move.
        interval: 2000
        onTriggered: {
            if (!page.playing || page.paused || page.pinned) return
            if (page.menusOpen > 0 || page.sideOpen) { hide.restart(); return }
            page.shown = false
        }
    }
    MouseArea {
        id: surface
        anchors.fill: parent
        hoverEnabled: true
        // a phone: sliding a finger up / down on the left half dims the picture, on the right half changes the
        // volume (idea from NuvioMobile's PlayerGestureOverlay, written anew). Not from the very top or bottom, where
        // Android's own gestures live.
        property string gesture: ""
        property bool slid: false
        property real startX: 0
        property real startY: 0
        property real startValue: 0
        preventStealing: gesture !== ""
        onPressed: function(m) { gesture = ""; slid = false; startX = m.x; startY = m.y }
        onReleased: gesture = ""
        onPositionChanged: function(m) {
            page.pointerAt(parent, m.x, m.y)
            if (!Theme.mobile || !pressed) return
            const dy = startY - m.y
            if (gesture === "") {
                if (Math.abs(dy) < 24 || Math.abs(dy) < Math.abs(m.x - startX)) return
                if (startY < 48 || startY > height - 72) return
                gesture = startX < width / 2 ? "light" : "volume"
                slid = true
                startValue = gesture === "light" ? page.light : Math.min(100, page.p.volume !== undefined ? page.p.volume : 100)
            }
            const v = Math.round(Math.max(gesture === "light" ? 20 : 0, Math.min(100, startValue + dy / (height * 0.7) * 100)))
            if (gesture === "light") page.light = v
            else Core.cmd({cmd: "volume", value: v})
            gestureFlash.show(gesture, v)
        }
        // a phone: a tap shows or hides the controls (play / pause is the big button in the middle); a double tap on
        // a side seeks that way (Theme.mob.seekStep seconds), in the middle it plays / pauses
        onClicked: function(m) {
            if (slid) { slid = false; return }
            if (!Theme.mobile) { page.togglePlay(); return }
            if (page.shown && page.playing && !page.p.paused) page.shown = false; else page.poke()
        }
        onDoubleClicked: function(m) {
            if (!Theme.mobile) { page.toggleFullscreen(); return }
            const third = width / 3
            const step = Theme.mob !== undefined && Theme.mob.seekStep !== undefined ? Theme.mob.seekStep : 10
            if (m.x < third) { Core.cmd({cmd: "seek_relative", secs: -step}); seekFlash.show(-1, step) }
            else if (m.x > 2 * third) { Core.cmd({cmd: "seek_relative", secs: step}); seekFlash.show(1, step) }
            else page.togglePlay()
            page.poke()
        }
        cursorShape: page.shown || !page.playing ? Qt.ArrowCursor : Qt.BlankCursor
        onWheel: function(w) { Core.cmd({cmd: "volume", value: Math.max(0, Math.min(150, (page.p.volume || 0) + (w.angleDelta.y > 0 ? 5 : -5)))}); page.poke() }
    }

    // a phone: how bright the picture is (20 to 100 %), set by sliding on the left half; a dark veil over the video
    property int light: 100
    Rectangle { anchors.fill: parent; z: -1; color: "#000000"; opacity: (100 - page.light) / 100; visible: page.light < 100 }
    // the "☀ 70 %" / "🔊 45 %" pill while sliding
    Rectangle {
        id: gestureFlash
        property string kind: "volume"
        property int value: 0
        function show(k, v) { kind = k; value = v; opacity = 1; gestureFlashTimer.restart() }
        visible: opacity > 0
        opacity: 0
        z: 4
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height * 0.18
        width: 132; height: 44; radius: 22
        color: "#80000000"
        Text {
            anchors.centerIn: parent
            text: (gestureFlash.kind === "light" ? "☀ " + Core.tr(Core.uiLang, "Brightness") : "🔊 " + Core.tr(Core.uiLang, "Volume")) + " " + gestureFlash.value + "%"
            color: "#ffffff"; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family
        }
        Behavior on opacity { NumberAnimation { duration: 160 } }
        Timer { id: gestureFlashTimer; interval: 700; onTriggered: gestureFlash.opacity = 0 }
    }

    // the "« 10 s" / "10 s »" flash of a double tap on a phone
    Rectangle {
        id: seekFlash
        property int dir: 1
        property int secs: 10
        function show(d, s) { dir = d; secs = s; opacity = 1; seekFlashTimer.restart() }
        visible: opacity > 0
        opacity: 0
        z: 4
        anchors.verticalCenter: parent.verticalCenter
        x: dir < 0 ? parent.width * 0.14 : parent.width * 0.86 - width
        width: 104; height: 48; radius: 24
        color: "#80000000"
        Text { anchors.centerIn: parent; text: (seekFlash.dir < 0 ? "« " : "") + seekFlash.secs + " s" + (seekFlash.dir < 0 ? "" : " »"); color: "#ffffff"; font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family }
        Behavior on opacity { NumberAnimation { duration: 160 } }
        Timer { id: seekFlashTimer; interval: 650; onTriggered: seekFlash.opacity = 0 }
    }
    // a phone: the big play / pause in the middle of the picture while the controls are shown (Material / YouTube)
    OsdButton {
        visible: Theme.mobile && page.shown && page.playing && !page.sideOpen && !page.editing
        anchors.centerIn: parent
        z: 4
        kind: "tonal"; pill: true
        glyph: page.p.paused ? "play" : "pause"; iconSize: 40
        implicitWidth: 80; implicitHeight: 80
        opacity: 0.92
        onClicked: page.togglePlay()
    }

    // ---- loading screen (GTK: the title's own wallpaper and logo, like Stremio) and error panel ----
    // The wallpaper covers the whole page (cropped, never letterboxed), blurred and dimmed by a scrim in the theme's
    // tone; the logo, the title and the state stay in the middle.
    Rectangle {
        id: loading
        anchors.fill: parent
        color: "#0c0c10"
        clip: true
        visible: !page.playing
        readonly property bool failed: page.stage === "failed"
        // the first torrent: the P2P notice waits for an answer
        readonly property bool consent: page.stage === "consent"
        Image {
            id: loadBg
            // a little bigger than the page: the blur fades the borders, and they fall outside
            anchors { fill: parent; margins: -40 }
            source: page.p.loadingBg ? "image://web/" + encodeURIComponent(page.p.loadingBg) : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
            opacity: status === Image.Ready ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.motion.slow || 350 } }
            // blurred through its own layer: no hidden source, no padding, so the picture covers every pixel of the page
            layer.enabled: status === Image.Ready
            layer.effect: MultiEffect {
                autoPaddingEnabled: false
                blurEnabled: true
                blurMax: 48
                blur: page.ps === "glass" ? 1.0 : (page.ps === "fluent" ? 0.8 : 0.55)
                saturation: page.ps === "glass" ? 0.3 : 0
            }
        }
        // the scrim: an even dim in the theme's dark tone, darker at the bottom where the buttons are
        Rectangle {
            anchors.fill: parent
            color: page.ps === "m3" ? "#b3141218" : (page.ps === "fluent" ? "#b81c1c1c" : (page.ps === "glass" ? "#80000000" : "#a6000000"))
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.0; color: "#59000000" }
                GradientStop { position: 0.35; color: "#00000000" }
                GradientStop { position: 0.7; color: "#00000000" }
                GradientStop { position: 1.0; color: "#99000000" }
            }
        }
        ColumnLayout {
            anchors.centerIn: parent
            width: Math.min(parent.width - 80, 760)
            spacing: 12
            Image {
                id: logo
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: Math.min(360, loading.width - 80)
                Layout.preferredHeight: 130
                visible: !!page.p.loadingLogo && status === Image.Ready
                source: page.p.loadingLogo ? "image://web/" + encodeURIComponent(page.p.loadingLogo) : ""
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                SequentialAnimation on opacity {
                    running: logo.visible && !loading.failed
                    loops: Animation.Infinite
                    NumberAnimation { from: 0.45; to: 1; duration: 900; easing.type: Easing.InOutQuad }
                    NumberAnimation { from: 1; to: 0.45; duration: 900; easing.type: Easing.InOutQuad }
                }
            }
            Spinner { Layout.alignment: Qt.AlignHCenter; Layout.topMargin: 4; running: !loading.failed && !loading.consent; visible: running }
            Text {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: page.p.title ? page.p.title : ""
                color: "#ffffff"
                // smaller under a logo, which already shows the name
                font.pixelSize: logo.visible ? 22 : (page.ps === "fluent" ? 28 : 30)
                font.weight: page.ps === "m3" ? 400 : (page.ps === "fluent" ? 600 : 800)
                font.family: Theme.type.family
            }
            Text {
                visible: !loading.failed && text !== ""
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: page.p.statusLine ? page.p.statusLine : ""
                color: "#ffffff"
                font.pixelSize: 17
                font.weight: page.o.titleWeight
                font.family: Theme.type.family
            }
            Text {
                visible: !loading.failed && text !== ""
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: page.p.statusSub ? page.p.statusSub : ""
                color: "#b3ffffff"
                font.pixelSize: 14
                font.family: Theme.type.family
            }
            // the P2P notice, shown once before the first torrent plays
            Text {
                visible: loading.consent
                Layout.fillWidth: true
                Layout.maximumWidth: 640
                Layout.alignment: Qt.AlignHCenter
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: Core.tr(Core.uiLang, "This video comes from a torrent (P2P). While it plays, the app downloads pieces from other people and also sends pieces to them, and your IP address is visible to them. Only play what you have the right to watch. With a debrid account in Preferences nothing is shared.")
                color: "#ffffff"
                font.pixelSize: 16
                font.family: Theme.type.family
            }
            // the error, on a panel of the theme so a bright wallpaper never washes it out
            Rectangle {
                visible: loading.failed
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: Math.min(640, errText.implicitWidth + 40)
                Layout.preferredHeight: errText.implicitHeight + 28
                radius: page.o.cardR
                color: page.o.panel
                border.width: page.o.strokeW
                border.color: page.o.stroke
                Text {
                    id: errText
                    anchors.centerIn: parent
                    width: Math.min(600, implicitWidth)
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    maximumLineCount: 8
                    elide: Text.ElideRight
                    text: page.p.error ? page.p.error : Core.tr(Core.uiLang, "This stream could not be played.")
                    color: "#ff8a7a"
                    font.pixelSize: 15
                    font.family: Theme.type.family
                }
            }
            Row {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 8
                spacing: 10
                OsdButton { visible: loading.consent; kind: "accent"; pill: page.ps !== "fluent"; glyph: "play"; text: Core.tr(Core.uiLang, "I understand, play"); onClicked: Core.cmd({cmd: "p2p_accept"}) }
                OsdButton { visible: loading.failed; kind: "accent"; pill: page.ps !== "fluent"; glyph: "refresh"; text: Core.tr(Core.uiLang, "Try again"); onClicked: Core.cmd({cmd: "retry_stream"}) }
                OsdButton { visible: loading.failed && page.arr(page.p.streams).length > 1; kind: "tonal"; pill: page.ps !== "fluent"; text: Core.tr(Core.uiLang, "Choose another torrent"); onClicked: picker.open() }
                OsdButton { kind: "tonal"; pill: page.ps !== "fluent"; text: loading.failed ? Core.tr(Core.uiLang, "Back") : Core.tr(Core.uiLang, "Cancel"); onClicked: Core.cmd({cmd: "close_player"}) }
            }
        }
    }

    // A spinner in the theme's shape: an accent arc (Windows progress ring, Material circular indicator), a white
    // arc (GNOME, Apple). Drawn with a Shape so it needs no style of Qt Quick Controls.
    component Spinner: Item {
        id: spn
        property bool running: true
        property int size: page.ps === "m3" ? 44 : 40
        implicitWidth: size
        implicitHeight: size
        readonly property real stroke: page.ps === "m3" ? 4 : (page.ps === "fluent" ? 3 : 3.5)
        Shape {
            id: spnShape
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            // the faint full ring under the arc (Windows and Material show one)
            ShapePath {
                strokeColor: page.ps === "fluent" || page.ps === "m3" ? "#26ffffff" : "transparent"
                strokeWidth: spn.stroke
                fillColor: "transparent"
                PathAngleArc { centerX: spn.size / 2; centerY: spn.size / 2; radiusX: spn.size / 2 - spn.stroke; radiusY: radiusX; startAngle: 0; sweepAngle: 360 }
            }
            ShapePath {
                strokeColor: page.ps === "fluent" || page.ps === "m3" ? Theme.c.accent : "#ffffff"
                strokeWidth: spn.stroke
                capStyle: ShapePath.RoundCap
                fillColor: "transparent"
                PathAngleArc { id: spnArc; centerX: spn.size / 2; centerY: spn.size / 2; radiusX: spn.size / 2 - spn.stroke; radiusY: radiusX; startAngle: -90; sweepAngle: 100 }
            }
            RotationAnimator on rotation { running: spn.running && spn.visible; from: 0; to: 360; duration: 1000; loops: Animation.Infinite }
        }
        // the arc grows and shrinks while it turns
        SequentialAnimation {
            running: spn.running && spn.visible
            loops: Animation.Infinite
            NumberAnimation { target: spnArc; property: "sweepAngle"; from: 40; to: 260; duration: 700; easing.type: Easing.InOutQuad }
            NumberAnimation { target: spnArc; property: "sweepAngle"; from: 260; to: 40; duration: 700; easing.type: Easing.InOutQuad }
        }
    }

    // ---- top bar ----
    Rectangle {
        id: topBar
        opacity: page.shown || !page.playing ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 180 } }
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: 64
        // the look of the controls follows the theme: adw (GNOME), glass (Apple), fluent (Windows), m3 (Android)
        readonly property string ps: Theme.layout.player || "adw"
        gradient: Gradient {
            GradientStop { position: 0; color: topBar.ps === "adw" ? "#bf000000" : (topBar.ps === "fluent" ? "#cc000000" : (topBar.ps === "m3" ? "#99000000" : "#80000000")) }
            GradientStop { position: 1; color: "#00000000" }
        }
        HoverHandler { id: topHover }
        MouseArea {
            anchors.fill: parent
            onDoubleClicked: page.toggleFullscreen()
            DragHandler { target: null; onActiveChanged: if (active && page.window.visibility !== Window.FullScreen) page.window.startSystemMove() }
        }
        // Apple: the coloured traffic lights at the top left, like the main window, beside the title capsule. In the
        // player their red light closes the player (not the app), like the close button of the other themes.
        OsdWindowButtons {
            id: topLights
            visible: Theme.look === "apple"
            traffic: true; closesPlayer: true; window: page.window
            anchors { left: parent.left; leftMargin: 18; verticalCenter: parent.verticalCenter }
        }
        // The back button and the title in one capsule as wide as they are: it grows with the title and the title
        // elides only past the room the bar has left (the window buttons keep theirs). Apple draws it as glass; the
        // other themes keep the bare row on the bar's shade, laid out the same way.
        Rectangle {
            id: titlePill
            readonly property bool glass: topBar.ps === "glass"
            readonly property real padL: glass ? 4 : 0
            readonly property real padR: glass ? 16 : 0
            readonly property real maxW: Math.max(120, topBar.width - x - (topCaption.visible ? topCaption.width + 24 : 16))
            anchors { left: topLights.visible ? topLights.right : parent.left; leftMargin: topLights.visible ? 14 : 10; verticalCenter: parent.verticalCenter }
            height: glass ? 44 : 40
            width: Math.min(maxW, padL + titleRow.implicitWidth + padR)
            radius: height / 2
            color: glass ? page.o.bar : "transparent"
            border.width: glass ? 1 : 0
            border.color: page.o.stroke
            Rectangle {
                visible: titlePill.glass
                anchors { fill: parent; margins: 1 }
                radius: height / 2
                gradient: Gradient { GradientStop { position: 0; color: "#17ffffff" } GradientStop { position: 0.5; color: "#00ffffff" } }
            }
            Row {
                id: titleRow
                anchors { left: parent.left; leftMargin: titlePill.padL; verticalCenter: parent.verticalCenter }
                spacing: titlePill.glass ? 4 : 10
                OsdButton {
                    id: backBtn
                    anchors.verticalCenter: parent.verticalCenter
                    kind: "osd"; glyph: "left"; iconSize: 20
                    implicitHeight: titlePill.glass ? 36 : 40; implicitWidth: implicitHeight
                    ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Back (Esc)")
                    onClicked: Core.cmd({cmd: "close_player"})
                }
                Text {
                    id: titleText
                    anchors.verticalCenter: parent.verticalCenter
                    // its own width up to what the capsule can hold
                    width: Math.max(0, Math.min(implicitWidth, titlePill.maxW - titlePill.padL - titlePill.padR - backBtn.width - titleRow.spacing))
                    text: page.p.title ? page.p.title : ""
                    color: "#ffffff"
                    font.pixelSize: topBar.ps === "m3" ? 22 : (topBar.ps === "fluent" ? 14 : (topBar.ps === "glass" ? 17 : 16))
                    font.weight: topBar.ps === "m3" ? 400 : (topBar.ps === "fluent" ? 600 : (topBar.ps === "glass" ? 600 : 700))
                    font.family: Theme.type.family
                    elide: Text.ElideRight
                    maximumLineCount: 1
                }
            }
        }
        // the other themes: their caption buttons, with light symbols (the player is dark in every theme); close
        // closes the player only
        OsdWindowButtons {
            id: topCaption
            visible: Theme.look !== "apple"
            closesPlayer: true; window: page.window
            anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
        }
    }

    // ---- skip opening / ending ----
    OsdButton {
        visible: page.playing && !!page.p.skip
        // above the next-episode card when both show
        anchors { right: parent.right; rightMargin: 24; bottom: parent.bottom; bottomMargin: 130 + (nextCard.visible ? nextCard.height + 12 : 0) }
        // the accent everywhere but on Apple, where it is a glass capsule like the other controls
        kind: page.ps === "glass" ? "tonal" : "accent"
        pill: page.ps !== "fluent"
        glyph: "next"
        text: page.p.skip ? page.p.skip.label : ""
        implicitHeight: page.ps === "m3" ? 48 : 42
        onClicked: Core.cmd({cmd: "skip_segment"})
    }

    // ---- while the video plays with the bar hidden: no pointer anywhere on the player (the X-Ray layer, the edge
    // buttons and the overlays above the video set their own cursors, so this sits on top). A move shows the bar
    // again and this layer goes away.
    MouseArea {
        z: 1000
        anchors.fill: parent
        visible: !Theme.mobile && !page.shown && page.playing && page.p.paused !== true && !quickSearch.opened
        acceptedButtons: Qt.NoButton
        hoverEnabled: true
        cursorShape: Qt.BlankCursor
        onPositionChanged: function(m) { page.pointerAt(parent, m.x, m.y) }
    }

    // ---- X-Ray: cast, music, scene and trivia of the current time (opens on pause, or from its tab / button) ----
    XRayPanel {
        id: xray
        anchors.fill: parent
        z: 14
        visible: page.playing
        // the Editor plays and pauses the part being cut (and a study question pauses the episode): those pauses are
        // not the viewer stopping to look, so the X-Ray stays closed (also just after the Editor closes)
        holdAuto: editorPop.visible || editorHold.running || studyQuiz.busy
        // the globe of the quick search sits above the X-Ray tab (on the left edge), not in the bottom bar
        searchOpen: quickSearch.opened
        // HP: with the bar hidden while it plays the X-Ray tab and the globe go too; with the bar shown (a mouse move)
        // or on pause they are there
        edgeShown: page.shown || !page.playing || page.p.paused === true || quickSearch.opened
        onSearchAsked: function(toggle) { if (toggle) quickSearch.toggle(); else quickSearch.open(); page.poke() }
    }
    Timer { id: editorHold; interval: 1500 }

    // ---- study while watching: the question over the paused video, the notice before a part without subtitles ----
    StudyQuiz {
        id: studyQuiz
        anchors.fill: parent
        z: 40
        visible: page.playing
        quiz: page.p.studyQuiz || null
        notice: page.p.studyNotice || ""
        cue: page.p.studyCue || null   // the macaw's cues (shh, peek): passed on to Core.mascotCue
        // the X-Ray may have opened on the pause of the question just before the question arrived
        onBusyChanged: if (busy) { xray.autoOpened = false; xray.hoverOpen = false; page.poke() }
    }
    // The study mark under the pointer on the seek bar ("" when none), for the preview label.
    function studyMarkAt(t) {
        const ms = page.arr(page.p.studyMarks)
        const tol = Math.max(2, (page.p.dur || 0) * 0.006)
        for (let i = 0; i < ms.length; i++) {
            const m = ms[i]
            if (t >= m.t - tol && t <= m.end + tol)
                return m.kind === "listen" ? Core.tr(Core.uiLang, "Listening practice (no subtitles)") : Core.tr(Core.uiLang, "Study question")
        }
        return ""
    }

    // ---- opening / ending card: the song of MyAnimeList, its video on YouTube (yt-dlp); a click pauses the episode ----
    // The lookup runs in the background: the card shows only once the song and its video are found and can play
    // (the core sends no card while it searches, nor when the search failed).
    Rectangle {
        id: themeCard
        readonly property var t: page.p.theme || null
        readonly property bool ready: t !== null && !t.loading && !(t.error && t.error !== "") && !!t.url
        visible: page.playing && ready && page.shown && !xray.open
        anchors { left: parent.left; leftMargin: 24; bottom: parent.bottom; bottomMargin: 130 }
        width: Math.max(0, Math.min(400, parent.width - 48))
        height: 104
        // a card of the theme (Osd.js); the hover lights the edge in the accent (Material: a state layer instead)
        radius: page.o.cardR
        color: page.ps === "m3" && themeArea.containsMouse ? "#f7363440" : page.o.panel
        border.width: page.ps === "m3" ? 0 : 1
        border.color: themeArea.containsMouse ? Theme.c.accent : page.o.stroke
        Behavior on border.color { ColorAnimation { duration: 120 } }
        Behavior on color { ColorAnimation { duration: 120 } }
        Item {
            id: themePic
            x: 12; anchors.verticalCenter: parent.verticalCenter
            width: 142; height: 80
            RoundedPicture { anchors.fill: parent; radius: Math.max(4, page.o.cardR - 4); url: themeCard.t && themeCard.t.thumbnail ? themeCard.t.thumbnail : ""; visible: !!(themeCard.t && themeCard.t.thumbnail) }
            Rectangle { anchors.fill: parent; radius: Math.max(4, page.o.cardR - 4); color: "#33000000"; visible: !!(themeCard.t && themeCard.t.thumbnail) }
            Rectangle {
                anchors.centerIn: parent
                width: 40; height: 28; radius: 8
                color: themeArea.containsMouse ? "#ff0000" : "#e6212121"
                visible: !!(themeCard.t && themeCard.t.url)
                Icon { anchors.centerIn: parent; name: "play"; size: 18; color: "#ffffff" }
            }
        }
        Column {
            anchors { left: themePic.right; leftMargin: 12; right: themeClose.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
            spacing: 3
            Text {
                text: themeCard.t ? (themeCard.t.kind === "op" ? Core.tr(Core.uiLang, "Opening") : Core.tr(Core.uiLang, "Ending")) + (themeCard.t.number > 0 ? " " + themeCard.t.number : "") : ""
                color: page.accentInk; font.pixelSize: 12; font.weight: page.o.headWeight; font.family: Theme.type.family
            }
            Text {
                width: parent.width; elide: Text.ElideRight
                text: themeCard.t ? (themeCard.t.song || "") : ""
                color: page.o.fg; font.pixelSize: 15; font.weight: page.o.titleWeight; font.family: Theme.type.family
            }
            Text { width: parent.width; elide: Text.ElideRight; visible: text !== ""; text: themeCard.t && themeCard.t.artist ? themeCard.t.artist : ""; color: page.o.dim; font.pixelSize: 13; font.family: Theme.type.family }
            Text { width: parent.width; elide: Text.ElideRight; visible: text !== ""; text: themeCard.t && themeCard.t.title ? "YouTube · " + themeCard.t.title : ""; color: page.o.muted; font.pixelSize: 11; font.family: Theme.type.family }
        }
        MouseArea {
            id: themeArea
            anchors.fill: parent
            hoverEnabled: true
            enabled: !!(themeCard.t && themeCard.t.url)
            cursorShape: Qt.PointingHandCursor
            // the video plays inside the X-Ray, which opens on it
            onClicked: xray.openTheme(themeCard.t)
            ToolTip.visible: containsMouse
            ToolTip.text: Core.tr(Core.uiLang, "Pause the episode and play this video")
        }
        OsdButton {
            id: themeClose
            anchors { right: parent.right; rightMargin: 6; top: parent.top; topMargin: 6 }
            kind: "round"; glyph: "close"; iconSize: 16
            onClicked: Core.cmd({cmd: "theme_dismiss"})
        }
    }

    // ---- next episode card ----
    // A 16:9 picture (the episode's own, else the series' backdrop, else a play glyph: never an empty grey box), the
    // episode code on the picture, the name on up to two lines, the countdown in a light grey that reads on the dark
    // panel (the accent could be too dark there) and a bar that empties until the next episode starts.
    Rectangle {
        id: nextCard
        readonly property var n: page.p.next || null
        readonly property real secs: n ? Math.max(0, n.secondsLeft) : 0
        readonly property string pic: n ? (n.thumbnail || n.fallback || "") : ""
        visible: page.playing && n !== null
        anchors { right: parent.right; rightMargin: Theme.mobile ? 16 : 24; bottom: parent.bottom; bottomMargin: 130 }
        width: Math.max(0, Math.min(Theme.mobile ? 420 : 460, parent.width - (Theme.mobile ? 32 : 48)))
        height: Theme.mobile ? 108 : 124
        radius: page.o.cardR
        color: page.o.panel
        border.width: page.o.strokeW
        border.color: page.o.stroke
        // the picture (click = play now)
        Item {
            id: nextPic
            anchors { left: parent.left; leftMargin: 10; top: parent.top; topMargin: 10; bottom: parent.bottom; bottomMargin: 13 }
            width: Math.round(height * 16 / 9)
            Rectangle { anchors.fill: parent; radius: Math.max(4, page.o.cardR - 4); color: "#26ffffff" }
            Icon { anchors.centerIn: parent; name: "play"; size: 28; color: "#99ffffff"; visible: nextPicImg.status !== Image.Ready }
            RoundedPicture {
                id: nextPicImg
                anchors.fill: parent
                radius: Math.max(4, page.o.cardR - 4)
                placeholder: "transparent"
                url: nextCard.pic
                fallback: nextCard.n && nextCard.n.thumbnail && nextCard.n.fallback ? nextCard.n.fallback : ""
                visible: nextCard.pic !== ""
            }
            // hover: darken and show the play glyph
            Rectangle {
                anchors.fill: parent; radius: Math.max(4, page.o.cardR - 4)
                color: "#73000000"
                visible: nextPicArea.containsMouse && nextPicImg.status === Image.Ready
                Icon { anchors.centerIn: parent; name: "play"; size: 30; color: "#ffffff" }
            }
            // the episode code on the picture
            Rectangle {
                visible: !!(nextCard.n && nextCard.n.label)
                anchors { left: parent.left; leftMargin: 6; bottom: parent.bottom; bottomMargin: 6 }
                width: codeText.implicitWidth + 12; height: codeText.implicitHeight + 4
                radius: 4
                color: "#cc000000"
                Text { id: codeText; anchors.centerIn: parent; text: nextCard.n ? nextCard.n.label : ""; color: "#ffffff"; font.pixelSize: 11; font.weight: Font.DemiBold; font.family: Theme.type.family }
            }
            MouseArea {
                id: nextPicArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: Core.cmd({cmd: "next_episode"})
            }
        }
        ColumnLayout {
            anchors { left: nextPic.right; leftMargin: 12; right: nextClose.left; rightMargin: 4; top: parent.top; topMargin: 10; bottom: parent.bottom; bottomMargin: 13 }
            spacing: 2
            Text {
                Layout.fillWidth: true
                text: Core.tr(Core.uiLang, "Next episode") + " · " + Math.ceil(nextCard.secs) + " s"
                color: page.o.dim; font.pixelSize: 12; font.weight: page.o.headWeight; font.family: Theme.type.family
                elide: Text.ElideRight
            }
            Text {
                Layout.fillWidth: true
                Layout.fillHeight: true
                verticalAlignment: Text.AlignTop
                // the name alone (the code is on the picture); the full title when the episode has no name
                text: nextCard.n ? (nextCard.n.name || nextCard.n.title) : ""
                color: page.o.fg; font.pixelSize: Theme.mobile ? 14 : 15; font.weight: page.o.titleWeight; font.family: Theme.type.family
                wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                lineHeight: 1.05
            }
            OsdButton {
                kind: "accent"; pill: page.ps !== "fluent"; glyph: "play"
                text: Core.tr(Core.uiLang, "Play now")
                onClicked: Core.cmd({cmd: "next_episode"})
            }
        }
        OsdButton {
            id: nextClose
            anchors { right: parent.right; rightMargin: 6; top: parent.top; topMargin: 6 }
            kind: "round"; glyph: "close"; iconSize: 16
            onClicked: Core.cmd({cmd: "dismiss_next"})
        }
        // the countdown: a thin line along the bottom that empties until the next episode starts
        Rectangle {
            id: nextTrack
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: Math.max(10, page.o.cardR); rightMargin: Math.max(10, page.o.cardR); bottomMargin: 5 }
            height: 3; radius: 1.5
            color: "#26ffffff"
            Rectangle {
                height: parent.height; radius: parent.radius
                width: nextCard.n && nextCard.n.total > 0 ? parent.width * Math.min(1, nextCard.secs / nextCard.n.total) : 0
                color: Theme.c.accent
                Behavior on width { NumberAnimation { duration: 400 } }
            }
        }
    }

    // ---- control bar ----
    Rectangle {
        id: bottomBar
        // a phone or tablet with room (landscape, 640 px and up): speed joins audio and quality on the bar
        readonly property bool mobWide: Theme.mobile && width >= 640
        objectName: "osdBar"
        // top edge of the panel behind the controls: the menus open above it, not over the seek bar (Osd.above)
        readonly property real panelTop: barCol.y - 12
        opacity: page.shown && page.playing ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 180 } }
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        readonly property string ps: Theme.layout.player || "adw"
        height: ps === "adw" ? 150 : (ps === "fluent" ? 112 : 150)
        gradient: Gradient {
            GradientStop { position: 0; color: "#00000000" }
            GradientStop { position: 1; color: bottomBar.ps === "adw" ? "#d9000000" : (bottomBar.ps === "fluent" ? "#00000000" : "#99000000") }
        }
        HoverHandler { id: barHover }
        // the panel behind the controls: a libadwaita OSD box (GNOME), an acrylic strip (Windows), a tonal sheet
        // (Android), a glass capsule (Apple); colours in Osd.js
        Rectangle {
            anchors { fill: barCol; leftMargin: bottomBar.ps === "fluent" ? -16 : -16; rightMargin: bottomBar.ps === "fluent" ? -16 : -16; topMargin: bottomBar.ps === "fluent" ? -8 : -10; bottomMargin: bottomBar.ps === "fluent" ? -8 : -8 }
            radius: page.o.barR
            color: page.o.bar
            border.width: bottomBar.ps === "fluent" || bottomBar.ps === "m3" ? 0 : 1
            border.color: page.o.stroke
            Rectangle { visible: bottomBar.ps === "fluent"; anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: "#1fffffff" }
            Rectangle {
                visible: page.o.glossy
                anchors { fill: parent; margins: 1 }
                radius: Math.max(0, parent.radius - 1)
                gradient: Gradient { GradientStop { position: 0; color: "#14ffffff" } GradientStop { position: 0.5; color: "#00ffffff" } }
            }
        }
        ColumnLayout {
            id: barCol
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: Theme.mobile ? 10 : (bottomBar.ps === "fluent" ? 16 : (bottomBar.ps === "adw" ? 28 : 34)); rightMargin: Theme.mobile ? 10 : (bottomBar.ps === "fluent" ? 16 : (bottomBar.ps === "adw" ? 28 : 34)); bottomMargin: (bottomBar.ps === "fluent" ? 8 : (bottomBar.ps === "adw" ? 20 : 24)) + (Theme.mobile && Theme.safe !== undefined && Theme.safe.bottom !== undefined ? Theme.safe.bottom : 0) }
            spacing: 4
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                Text { Layout.preferredWidth: 64; horizontalAlignment: Text.AlignRight; text: page.fmt(page.p.pos); color: "#ffffff"; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: Theme.type.family }
                Slider {
                    id: seek
                    Layout.fillWidth: true
                    from: 0
                    to: Math.max(1, page.p.dur || 1)
                    value: pressed ? value : (page.p.pos || 0)
                    onPressedChanged: if (!pressed) Core.cmd({cmd: "seek", pos: value}); else page.thumbHover(seek.leftPadding + seek.visualPosition * seek.availableWidth)
                    // dragging the bar (a finger on a phone, like YouTube; the mouse too) shows the picture of where it is
                    onMoved: page.thumbHover(seek.leftPadding + seek.visualPosition * seek.availableWidth)
                    // Preview picture above the seek bar. A MouseArea that lets the clicks through: it only follows the pointer
                    // over the bar (a HoverHandler inside a Slider did not fire).
                    MouseArea {
                        id: seekHover
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.NoButton
                        z: 5
                        readonly property bool hovered: containsMouse
                        // without the picture preview, the study mark under the pointer says what it is
                        ToolTip.visible: hovered && !page.p.thumbnails && page.studyMarkAt(seek.hoverT) !== ""
                        ToolTip.text: page.studyMarkAt(seek.hoverT)
                        onPositionChanged: function(m) { page.thumbHover(m.x) }
                        onEntered: page.thumbHover(mouseX)
                    }
                    property real hoverX: 0
                    property real hoverT: 0
                    background: Rectangle {
                        x: seek.leftPadding
                        y: seek.topPadding + seek.availableHeight / 2 - height / 2
                        width: seek.availableWidth
                        height: bottomBar.ps === "m3" ? 6 : (bottomBar.ps === "fluent" ? (seekHover.hovered || seek.pressed ? 5 : 3) : (bottomBar.ps === "glass" ? 4 : 5))
                        Behavior on height { NumberAnimation { duration: 100 } }
                        radius: height / 2
                        color: bottomBar.ps === "m3" ? "#4a4458" : "#40ffffff"
                        Rectangle {
                            // what is already in the cache
                            width: Math.min(1, ((page.p.pos || 0) + (page.p.cache || 0)) / Math.max(1, page.p.dur || 1)) * parent.width
                            height: parent.height
                            radius: height / 2
                            color: "#40ffffff"
                        }
                        // played: white on Apple (media controls), the accent elsewhere
                        Rectangle { width: seek.visualPosition * parent.width; height: parent.height; radius: height / 2; color: bottomBar.ps === "glass" ? "#ffffff" : Theme.c.accent }
                        Repeater {
                            model: page.arr(page.p.marks).length
                            delegate: Rectangle {
                                required property int index
                                readonly property real modelData: page.arr(page.p.marks)[index] || 0
                                x: modelData / Math.max(1, page.p.dur || 1) * parent.width - 1
                                y: -3
                                width: 2
                                height: parent.height + 6
                                color: "#ffffff"
                                opacity: 0.8
                            }
                        }
                        // study while watching: a dot for a question about a line, a band for a part without subtitles
                        Repeater {
                            model: page.arr(page.p.studyMarks).length
                            delegate: Rectangle {
                                required property int index
                                readonly property var m: page.arr(page.p.studyMarks)[index] || ({t: 0, end: 0, kind: "card"})
                                readonly property bool band: m.kind === "listen"
                                readonly property real dur: Math.max(1, page.p.dur || 1)
                                x: band ? m.t / dur * parent.width : m.t / dur * parent.width - width / 2
                                y: band ? -2 : (parent.height - height) / 2
                                width: band ? Math.max(4, (m.end - m.t) / dur * parent.width) : 8
                                height: band ? parent.height + 4 : 8
                                radius: height / 2
                                color: band ? Qt.alpha(page.accentInk, 0.45) : page.accentInk
                                border.width: band ? 0 : 1
                                border.color: "#99000000"
                                opacity: m.done ? 0.45 : 1
                            }
                        }
                    }
                    handle: Rectangle {
                        x: seek.leftPadding + seek.visualPosition * (seek.availableWidth - width)
                        y: seek.topPadding + seek.availableHeight / 2 - height / 2
                        // Apple: a small white ball that grows on hover; Windows: the Fluent thumb (dark, accent dot) on hover;
                        // Android: an accent bar; GNOME: a white knob
                        width: bottomBar.ps === "m3" ? 4 : (bottomBar.ps === "glass" ? (seekHover.hovered || seek.pressed ? 14 : 10) : (bottomBar.ps === "fluent" ? 18 : 14))
                        height: bottomBar.ps === "m3" ? 24 : width
                        radius: bottomBar.ps === "m3" ? 2 : width / 2
                        color: bottomBar.ps === "m3" ? Theme.c.accent : (bottomBar.ps === "fluent" ? "#454545" : "#ffffff")
                        border.width: bottomBar.ps === "fluent" ? 1 : 0
                        border.color: "#17ffffff"
                        opacity: bottomBar.ps === "fluent" ? (seekHover.hovered || seek.pressed ? 1 : 0) : 1
                        Behavior on opacity { NumberAnimation { duration: 100 } }
                        Behavior on width { NumberAnimation { duration: 100 } }
                        Rectangle {
                            visible: bottomBar.ps === "fluent"
                            anchors.centerIn: parent
                            width: seek.pressed ? 8 : 10; height: width; radius: width / 2
                            color: Theme.c.accent
                        }
                    }
                }
                Text {
                    Layout.preferredWidth: 64
                    text: page.remaining ? "-" + page.fmt((page.p.dur || 0) - (page.p.pos || 0)) : page.fmt(page.p.dur)
                    color: "#ffffff"
                    font.pixelSize: 14
                    font.weight: Font.DemiBold
                    font.family: Theme.type.family
                    MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; ToolTip.visible: containsMouse; ToolTip.text: Core.tr(Core.uiLang, "Click to switch between total and remaining time"); onClicked: page.remaining = !page.remaining }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 4
                // play / pause, the main button: the accent circle on Windows and Android, a glass circle on Apple, flat on GNOME
                OsdButton {
                    id: playBtn
                    kind: bottomBar.ps === "m3" || bottomBar.ps === "fluent" ? "accent" : (bottomBar.ps === "glass" ? "tonal" : "osd")
                    pill: true
                    glyph: page.p.paused ? "play" : "pause"
                    iconSize: bottomBar.ps === "adw" ? 26 : 22
                    implicitWidth: 48
                    implicitHeight: 48
                    ToolTip.visible: hovered
                    ToolTip.text: Core.tr(Core.uiLang, "Play / Pause (Space)")
                    onClicked: page.togglePlay()
                }
                OsdButton { visible: !!page.p.hasNext; kind: "osd"; glyph: "next"; iconSize: 24; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Next episode (N)"); onClicked: Core.cmd({cmd: "next_episode"}) }
                OsdButton { visible: !Theme.mobile; kind: "osd"; glyph: page.p.muted ? "mute" : "volume"; iconSize: 22; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Mute (M)"); onClicked: Core.cmd({cmd: "toggle_mute"}) }
                OsdSlider {
                    id: vol
                    visible: !Theme.mobile   // a phone has its own volume keys
                    from: 0
                    to: 150
                    stepSize: 1
                    implicitWidth: 120
                    boostFrom: 100   // past 100 % the sound is amplified: shown in orange, with a tick at 100
                    // follows the player, except while dragged (the state arriving 4×/s would snap it back)
                    Binding on value { when: !vol.pressed; restoreMode: Binding.RestoreNone; value: page.p.volume !== undefined ? page.p.volume : 100 }
                    ToolTip.visible: hovered
                    ToolTip.text: Core.tr(Core.uiLang, "Volume (scroll the mouse wheel to change)") + " · " + Math.round(value) + "%"
                    onMoved: Core.cmd({cmd: "volume", value: value})
                }
                // ---- skip group: one pill holding [» 80s] and the round auto-skip switch [A] ----
                Rectangle {
                    id: skipGroup
                    Layout.leftMargin: 6
                    readonly property string ps: bottomBar.ps
                    // Shape and neutral tones of the theme: Fluent is a rounded rectangle, the others a full pill.
                    readonly property real h: ps === "m3" ? 40 : 34
                    readonly property real r: ps === "fluent" ? (Theme.shape.control || 4) + 2 : h / 2
                    readonly property color neutral: ps === "m3" ? "#33ffffff" : (ps === "glass" ? "#2effffff" : (ps === "fluent" ? "#1fffffff" : "#33ffffff"))
                    readonly property color neutralHover: ps === "fluent" ? "#2effffff" : "#4dffffff"
                    implicitHeight: h
                    implicitWidth: skipRow.implicitWidth + 6
                    radius: r
                    color: neutral
                    border.width: ps === "fluent" || ps === "glass" ? 1 : 0
                    border.color: "#1affffff"

                    Row {
                        id: skipRow
                        anchors { left: parent.left; verticalCenter: parent.verticalCenter; leftMargin: 0 }
                        spacing: 2

                        // [» 80s]: jump ahead by the configured seconds
                        Item {
                            id: skipFwd
                            width: skipFwdContent.implicitWidth + 24
                            height: skipGroup.h
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: 3
                                radius: skipGroup.ps === "fluent" ? skipGroup.r - 2 : height / 2
                                color: skipFwdArea.pressed ? "#33ffffff" : (skipFwdArea.containsMouse ? "#1fffffff" : "transparent")
                                Behavior on color { ColorAnimation { duration: 100 } }
                            }
                            Row {
                                id: skipFwdContent
                                anchors.centerIn: parent
                                spacing: 6
                                Icon { name: "forward"; size: 16; color: "#ffffff"; anchors.verticalCenter: parent.verticalCenter }
                                Text { text: (page.p.skipSeconds || 90) + "s"; color: "#ffffff"; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                            }
                            MouseArea {
                                id: skipFwdArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Core.cmd({cmd: "skip_forward"})
                                ToolTip.visible: containsMouse
                                ToolTip.text: Core.tr(Core.uiLang, "Skip ahead") + " " + (page.p.skipSeconds || 90) + "s"
                            }
                        }

                        // [A]: openings and endings skipped by themselves. Off: neutral grey; on: theme accent.
                        Rectangle {
                            id: autoSkip
                            readonly property bool on: !!page.p.autoSkip
                            width: skipGroup.h - 6
                            height: width
                            anchors.verticalCenter: parent.verticalCenter
                            radius: width / 2
                            color: on ? (autoArea.containsMouse ? Qt.lighter(Theme.c.accent, 1.12) : Theme.c.accent)
                                      : (autoArea.containsMouse ? skipGroup.neutralHover : skipGroup.neutral)
                            border.width: !on && (skipGroup.ps === "fluent" || skipGroup.ps === "glass") ? 1 : 0
                            border.color: "#26ffffff"
                            scale: autoArea.pressed ? 0.92 : 1
                            Behavior on color { ColorAnimation { duration: Theme.motion && Theme.motion.fast ? Theme.motion.fast : 120 } }
                            Behavior on scale { NumberAnimation { duration: 90 } }
                            Text {
                                anchors.centerIn: parent
                                text: "A"
                                color: autoSkip.on ? (Theme.c.accentFg || "#ffffff") : "#ffffff"
                                font.pixelSize: 14
                                font.weight: Font.Bold
                                font.family: Theme.type.family
                            }
                            MouseArea {
                                id: autoArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Core.cmd({cmd: "set_setting", path: "anime_skip.auto_skip", value: !autoSkip.on})
                                ToolTip.visible: containsMouse
                                ToolTip.text: Core.tr(Core.uiLang, "Skip openings and endings by themselves") + (autoSkip.on ? " (" + Core.tr(Core.uiLang, "on") + ")" : " (" + Core.tr(Core.uiLang, "off") + ")")
                            }
                            Accessible.role: Accessible.CheckBox
                            Accessible.checked: on
                            Accessible.name: Core.tr(Core.uiLang, "Auto skip")
                        }
                    }
                }
                Item { Layout.fillWidth: true }
                // the macaw's button: the learned words it swapped into the subtitle this episode, and flashcards of them
                OsdButton {
                    id: swBtn
                    visible: page.arr(page.p.studySwaps).length > 0
                    kind: "osd"; active: swapsPop.visible
                    ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Words Arara swapped in this episode")
                    onClicked: swapsPop.openAbove(swBtn)
                    Mascot { anchors.centerIn: parent; size: 28; mood: swapsPop.visible ? "happy" : "peek"; animate: false; tappable: false }
                    Rectangle {
                        anchors { right: parent.right; top: parent.top; rightMargin: 2; topMargin: 2 }
                        width: Math.max(16, swCount.implicitWidth + 8); height: 16; radius: 8
                        color: Theme.c.accent
                        Text { id: swCount; anchors.centerIn: parent; text: page.arr(page.p.studySwaps).length; color: Theme.c.accentFg; font.pixelSize: 10; font.weight: Font.Bold; font.family: Theme.type.family }
                    }
                }
                OsdButton { visible: xray.hasData && !Theme.mobile; kind: "osd"; active: xray.open; text: "X-Ray"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Cast, music and trivia of this moment"); onClicked: { xray.pinned = !xray.open; if (!xray.pinned) { xray.autoOpened = false; xray.hoverOpen = false } } }
                OsdButton { id: qBtn; kind: "osd"; active: qMenu.visible; glyph: "quality"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Video quality and torrent"); onClicked: { qMenu.entries = page.qualityEntries(); qMenu.openAbove(qBtn) } }
                OsdButton { id: aBtn; kind: "osd"; active: aMenu.visible; glyph: "audio"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Audio track"); onClicked: { aMenu.entries = page.audioEntries(); aMenu.openAbove(aBtn) } }
                OsdButton { id: sBtn; kind: "osd"; active: subsMenu.visible; glyph: "subtitles"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Subtitles"); onClicked: subsMenu.openAbove(sBtn) }
                OsdButton { id: spBtn; visible: !Theme.mobile || bottomBar.mobWide; kind: "osd"; active: speedPop.visible; glyph: "speed"; text: (Math.round((page.p.speed || 1) * 100) / 100) + "×"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Playback speed"); onClicked: speedPop.openAbove(spBtn) }
                OsdButton { id: uBtn; visible: !Theme.mobile; kind: "osd"; active: uMenu.visible; glyph: "upscale"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Anime4K upscaling"); onClicked: { uMenu.entries = page.upscaleEntries(); uMenu.openAbove(uBtn) } }
                OsdButton { id: vsBtn; visible: !Theme.mobile; kind: "osd"; active: vsMenu.visible; glyph: "scale"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Video scale"); onClicked: { vsMenu.entries = page.scaleEntries(); vsMenu.openAbove(vsBtn) } }
                OsdButton { id: edBtn; visible: !Theme.mobile; kind: "osd"; active: editorPop.visible; glyph: "scissors"; text: Core.tr(Core.uiLang, "Editor"); ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Cut a part of the video and save it to an Anki deck"); onClicked: page.openEditor() }
                OsdButton { id: stBtn; visible: !Theme.mobile; kind: "osd"; active: statsPop.visible; glyph: "stats"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Statistics"); onClicked: { Core.cmd({cmd: "refresh_stats"}); statsPop.openAbove(stBtn) } }
                // the feedback note (the same FeedbackBox as the sidebar's button, Main.openFeedback)
                OsdButton { id: fbBtn; visible: !Theme.mobile && (!Core.prefs.general || Core.prefs.general.feedback_button !== false); kind: "osd"; glyph: "message"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Send feedback"); onClicked: if (page.window && page.window.openFeedback) page.window.openFeedback(fbBtn, "") }
                // a phone: the rest of the bar in a bottom sheet
                OsdButton {
                    id: moreBtn
                    visible: Theme.mobile
                    kind: "osd"; glyph: "more"; active: moreMenu.visible
                    ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "More")
                    onClicked: {
                        const T = function(x) { return Core.tr(Core.uiLang, x) }
                        var e = [{header: T("More")}]
                        // audio and quality are always on the bar; speed too when the bar is wide
                        if (!bottomBar.mobWide) e.push({text: T("Playback speed") + "  " + (Math.round((page.p.speed || 1) * 100) / 100) + "×", key: "sp"})
                        if (xray.hasData) e.push({text: "X-Ray", key: "xray", checked: xray.open})
                        e.push({text: T("Anime4K upscaling"), key: "up"})
                        e.push({text: T("Video scale"), key: "vs"})
                        e.push({text: T("Editor"), key: "ed"})
                        e.push({text: T("Statistics"), key: "st"})
                        e.push({text: page.p.muted ? T("Unmute") : T("Mute (M)"), key: "mute", checked: page.p.muted === true})
                        moreMenu.entries = e
                        moreMenu.openAbove(moreBtn)
                    }
                }
                // quick search (Research mode): text or Google Lens about what is on screen, results over the video
                OsdButton { kind: "osd"; glyph: page.window && page.window.visibility === Window.FullScreen ? "unfullscreen" : "fullscreen"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Fullscreen (F)"); onClicked: page.toggleFullscreen() }
            }
        }
    }

    // ---- menus ----
    function qualityEntries() {
        var e = [{header: Core.tr(Core.uiLang, "Quality")}]
        page.arr(page.p.qualities).forEach(function(q) { e.push({text: q.label, checked: q.current, key: "q", index: q.index}) })
        if (page.arr(page.p.streams).length > 1) { e.push({sep: true}); e.push({text: Core.tr(Core.uiLang, "Choose another torrent…"), key: "picker"}) }
        return e
    }
    // `delay` = the audio delay just stepped to (the player's state comes a moment later)
    function audioEntries(delay) {
        const T = function(x) { return Core.tr(Core.uiLang, x) }
        var e = [{header: T("Audio")}]
        var t = page.arr(page.p.audio)
        if (t.length === 0) e.push({text: T("No audio tracks"), key: ""})
        t.forEach(function(a) { e.push({text: (a.external ? "⤓ " : "") + a.label, checked: a.selected, key: "aid", id: a.id}) })
        e.push({sep: true})
        e.push({text: T("Load an audio track…"), key: "aload", chevron: true})
        // a dub from the episode's other torrents (only the addons the user added are read)
        e.push({text: T("Look for a dub in other torrents"), key: "autodub", checked: page.p.autoDub === true})
        // with where it is (the torrent's peers / speed / downloaded, or the seconds mpv has been reading), so a slow
        // stream reads as working
        const prog = page.p.dubProgress ? "  ·  " + page.p.dubProgress : ""
        if (page.p.dubBusy) e.push({text: T("Opening another stream…") + (page.p.dubOpening ? " " + page.p.dubOpening : "") + prog, key: "", note: true})
        else if (page.p.dubAdding) e.push({text: T("Adding the tracks of another stream…") + prog, key: "", note: true})
        else if (page.p.dubNote) e.push({text: T(page.p.dubNote), key: "", note: true})
        // every audio language the other streams of the episode say they have: one click adds that stream's tracks
        const dc = page.arr(page.p.audioSources)
        if (dc.length > 0) {
            e.push({header: T("Audio from other streams")})
            dc.forEach(function(x) { e.push({text: x.label, key: "dubtry", index: x.index, lang: x.lang}) })
        }
        // a dub loaded from a file: its delay (+ = the sound comes later), the automatic sync, taking it out
        const ext = t.some(function(a) { return a.selected && a.external })
        const d = delay !== undefined ? delay : (page.p.audioDelay || 0)
        if (ext || Math.abs(d) > 0.0005) {
            e.push({header: T("Audio delay")})
            e.push({stepper: true, coarse: true, key: "adelay", num: d, value: (d >= 0 ? "+" : "") + d.toFixed(2) + " s"})
        }
        if (ext) {
            if (page.p.audioSyncBusy) e.push({text: T("Syncing the audio with the original…"), key: ""})
            else e.push({text: T("Sync with the original audio"), key: "async"})
            if (page.p.audioSyncNote) e.push({text: T(page.p.audioSyncNote), key: "", note: true})
            e.push({text: T("Remove this audio track"), key: "aremove"})
        }
        return e
    }
    function subEntries() {
        var e = [{header: Core.tr(Core.uiLang, "Subtitles")}]
        var t = page.arr(page.p.subs)
        var any = t.some(function(s) { return s.selected })
        e.push({text: Core.tr(Core.uiLang, "Off"), checked: !any, key: "sid", id: 0})
        t.forEach(function(s) { e.push({text: s.label, checked: s.selected, key: "sid", id: s.id}) })
        var c = page.arr(page.p.subCands)
        if (c.length > 0) {
            e.push({header: Core.tr(Core.uiLang, "From the addons")})
            c.slice(0, 40).forEach(function(x) { e.push({text: (x.lang || "?") + " · " + x.addon + " #" + (x.index + 1), key: "cand", index: x.index}) })
        }
        e.push({header: Core.tr(Core.uiLang, "Delay")})
        e.push({stepper: true, key: "delay", value: (page.p.subDelay || 0).toFixed(1) + " s"})
        return e
    }
    function speedEntries() {
        var e = [{header: Core.tr(Core.uiLang, "Playback speed")}]
        ;[0.5, 0.75, 1, 1.25, 1.5, 1.75, 2].forEach(function(v) { e.push({text: v + "×", checked: Math.abs((page.p.speed || 1) - v) < 0.01, key: "speed", value: v}) })
        return e
    }
    function upscaleEntries() {
        var e = [{header: Core.tr(Core.uiLang, "Anime4K upscaling")}, {text: Core.tr(Core.uiLang, "Off"), checked: !page.p.upscaling, key: "up", mode: ""}]
        page.arr(page.p.upscaleModes).forEach(function(m) { e.push({text: Core.tr(Core.uiLang, "Mode") + " " + m, checked: !!page.p.upscaling && page.p.upscaleMode === m, key: "up", mode: m}) })
        e.push({sep: true})
        e.push({text: Core.tr(Core.uiLang, "Upscaling settings…"), key: "upPrefs"})
        return e
    }
    function scaleEntries() {
        var e = [{header: Core.tr(Core.uiLang, "Video scale")}]
        ;[["fit", "Fit"], ["crop", "Crop"], ["stretch", "Stretch"]].forEach(function(x) { e.push({text: Core.tr(Core.uiLang, x[1]), checked: page.p.videoScale === x[0], key: "scale", mode: x[0]}) })
        return e
    }
    function pick(en) {
        switch (en.key) {
        case "q": Core.cmd({cmd: "switch_stream", index: en.index}); break
        case "picker": picker.open(); break
        case "aid": Core.cmd({cmd: "set_audio", id: en.id}); break
        case "aload": audioDlg.open(); break
        case "async": Core.cmd({cmd: "audio_sync"}); break
        case "aremove": Core.cmd({cmd: "audio_remove"}); break
        case "autodub": Core.cmd({cmd: "auto_dub", on: page.p.autoDub !== true}); break
        case "dubtry": Core.cmd({cmd: "dub_try", index: en.index, lang: en.lang || ""}); break
        case "sid": Core.cmd({cmd: "set_sub", id: en.id}); break
        case "cand": Core.cmd({cmd: "load_sub_cand", index: en.index}); break
        case "speed": Core.cmd({cmd: "speed", value: en.value}); break
        case "up": Core.cmd({cmd: "upscale", mode: en.mode}); break
        case "scale": Core.cmd({cmd: "video_scale", mode: en.mode}); break
        case "upPrefs": page.window.openPrefs("general"); break
        }
    }
    OsdMenu { id: qMenu; onPicked: function(en) { page.pick(en) } }
    // the "More" sheet of a phone: what the bar has no room for
    OsdMenu {
        id: moreMenu
        onPicked: function(en) {
            switch (en.key) {
            case "q": qMenu.entries = page.qualityEntries(); qMenu.openAbove(moreBtn); break
            case "a": aMenu.entries = page.audioEntries(); aMenu.openAbove(moreBtn); break
            case "sp": speedPop.openAbove(moreBtn); break
            case "xray": xray.pinned = !xray.open; if (!xray.pinned) { xray.autoOpened = false; xray.hoverOpen = false } break
            case "up": uMenu.entries = page.upscaleEntries(); uMenu.openAbove(moreBtn); break
            case "vs": vsMenu.entries = page.scaleEntries(); vsMenu.openAbove(moreBtn); break
            case "ed": page.openEditor(); break
            case "st": Core.cmd({cmd: "refresh_stats"}); statsPop.openAbove(moreBtn); break
            case "mute": Core.cmd({cmd: "toggle_mute"}); break
            }
        }
    }
    OsdMenu {
        id: aMenu
        menuWidth: 370
        onPicked: function(en) { page.pick(en) }
        // ± 0.1 s (dir ±1) or ± 1 s (dir ±10); 0 = back to none
        onStepped: function(en, dir) {
            const v = dir === 0 ? 0 : Math.round(((en.num || 0) + dir * 0.1) * 1000) / 1000
            Core.cmd({cmd: "audio_delay", value: v})
            aMenu.entries = page.audioEntries(v)
        }
    }
    // the menu follows the sync while it is open (busy -> the result)
    property string audioSyncWas: ""
    Connections {
        target: Core
        function onPlayerChanged() {
            const b = (page.p.audioSyncBusy === true) + "/" + (page.p.dubBusy === true) + "/" + (page.p.dubNote || "") + "/" + (page.p.dubProgress || "") + "/" + (page.p.dubAdding === true)
            if (b === page.audioSyncWas) return
            page.audioSyncWas = b
            if (aMenu.visible) aMenu.entries = page.audioEntries()
        }
    }
    // an audio file (a dub) for this video; on Android the picked document is copied into the app first
    FileDialog {
        id: audioDlg
        title: Core.tr(Core.uiLang, "Choose an audio track")
        nameFilters: Qt.platform.os === "android" ? [] : [Core.tr(Core.uiLang, "Audio") + " (*.mka *.mp3 *.aac *.m4a *.ac3 *.eac3 *.dts *.flac *.ogg *.oga *.opus *.wav *.wma *.mp2 *.mkv *.mp4 *.avi *.webm)", Core.tr(Core.uiLang, "All files") + " (*)"]
        onAccepted: {
            const path = Core.importFile(selectedFile.toString(), "external-audio")
            if (path === "") Core.toast(Core.tr(Core.uiLang, "The audio file was not found."))
            else Core.cmd({cmd: "audio_add", path: path})
        }
    }
    OsdMenu { id: uMenu; onPicked: function(en) { page.pick(en) } }
    OsdMenu { id: vsMenu; menuWidth: 200; onPicked: function(en) { page.pick(en) } }

    // ---- every torrent of this title ----
    Popup {
        id: picker
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(620, page.width - 40)
        height: Math.min(640, page.height - 60)
        modal: true
        padding: 0
        // nearly opaque: the error card under it ("could not play…", its buttons) showed through on a phone
        background: OsdPanel { id: pickerBg; sheet: true; color: String(pickerBg.o.panel).length === 9 ? "#fa" + String(pickerBg.o.panel).substring(3) : pickerBg.o.panel }
        // the dim behind the sheet
        Overlay.modal: Rectangle { color: "#80000000" }
        enter: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.motion.fast || 120 }
            NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.motion.normal || 200; easing.type: Easing.OutCubic }
        }
        exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Theme.motion.fast || 120 } }
        contentItem: ColumnLayout {
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: page.ps === "m3" ? 20 : 14
                Layout.bottomMargin: 8
                Text { Layout.fillWidth: true; text: Core.tr(Core.uiLang, "Choose a torrent"); color: page.o.fg; font.pixelSize: page.ps === "m3" ? 22 : (page.ps === "fluent" ? 20 : 18); font.weight: page.ps === "m3" ? 400 : page.o.titleWeight; font.family: Theme.type.family }
                OsdButton { kind: "round"; glyph: "close"; iconSize: 16; onClicked: picker.close() }
            }
            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: page.o.sep; visible: page.ps === "fluent" || page.ps === "adw" }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: page.ps === "m3" ? 6 : 4
                topMargin: 6
                leftMargin: 10
                rightMargin: 10
                bottomMargin: 10
                model: page.arr(page.p.streams).length
                ScrollBar.vertical: ScrollBar {}
                // a row per torrent; the one playing is marked the theme's way (see OsdRow): accent check (GNOME),
                // accent pill (Windows), secondary container (Android), white check (Apple)
                delegate: Rectangle {
                    id: tRow
                    required property int index
                    readonly property var modelData: page.arr(page.p.streams)[index] || ({})
                    readonly property bool cur: index === page.p.cur
                    readonly property bool filled: cur && page.o.mark === "fill"
                    width: ListView.view.width - 20
                    height: col2.implicitHeight + 18
                    radius: Math.max(page.o.rowR, page.ps === "m3" ? 16 : (page.ps === "glass" ? 14 : page.o.rowR + 2))
                    color: ma.pressed ? page.o.press
                         : tRow.filled ? (ma.containsMouse ? page.o.selHover : page.o.sel)
                         : (ma.containsMouse ? page.o.hover : (tRow.cur ? page.o.sel : "#0affffff"))
                    Behavior on color { ColorAnimation { duration: Theme.motion.fast || 120 } }
                    Rectangle {
                        visible: page.o.mark === "bar" && tRow.cur
                        width: 3; height: 18; radius: 1.5; color: Theme.c.accent
                        anchors { left: parent.left; leftMargin: 1; verticalCenter: parent.verticalCenter }
                    }
                    Icon {
                        id: tCheck
                        visible: tRow.cur && page.o.mark !== "bar"
                        name: "check"; size: 18
                        color: tRow.filled ? page.o.selFg : (page.o.markAccent ? page.accentInk : page.o.fg)
                        anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                    }
                    Column {
                        id: col2
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: 14; rightMargin: tCheck.visible ? 38 : 14 }
                        spacing: 2
                        Text { width: parent.width; text: tRow.modelData.name || ""; color: tRow.filled ? page.o.selFg : page.o.fg; font.pixelSize: 14; font.weight: page.o.titleWeight; font.family: Theme.type.family; elide: Text.ElideRight }
                        Text { width: parent.width; text: tRow.modelData.desc || ""; color: tRow.filled ? page.o.selFg : page.o.dim; opacity: tRow.filled ? 0.85 : 1; font.pixelSize: 13; font.family: Theme.type.family; wrapMode: Text.Wrap; maximumLineCount: 3; elide: Text.ElideRight }
                        Text {
                            text: [tRow.modelData.addon, tRow.modelData.seeders > 0 ? tRow.modelData.seeders + " " + Core.tr(Core.uiLang, "seeders") : "", tRow.modelData.sizeMb > 0 ? (tRow.modelData.sizeMb / 1024).toFixed(1) + " GB" + (tRow.modelData.pack ? " (pack)" : "") : ""].filter(function(x) { return x !== "" && x !== undefined }).join(" · ")
                            color: tRow.filled ? page.o.selFg : page.accentInk
                            font.pixelSize: 12
                            font.weight: 600
                            font.family: Theme.type.family
                        }
                    }
                    MouseArea {
                        id: ma
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { Core.cmd({cmd: "switch_stream", index: tRow.index}); picker.close() }
                    }
                }
            }
        }
    }

    // ---- Subtitles menu: three columns, translation and karaoke pages ----
    SubsMenu {
        id: subsMenu
        p: page.p
        onPrefsRequested: function(name) { page.window.openPrefs(name) }
    }

    // A popover of the player opened above its button (speed, statistics).
    component OsdPop: Popup {
        id: op
        parent: Overlay.overlay
        padding: page.o.pad + 8
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        property real anchorX: 0
        property real anchorY: 0
        x: Math.max(8, Math.min(anchorX - width / 2, (Overlay.overlay ? Overlay.overlay.width : 1000) - width - 8))
        y: Math.max(8, anchorY - height - 10)
        function openAbove(btn) { const pt = btn.mapToItem(Overlay.overlay, btn.width / 2, 0); anchorX = pt.x; anchorY = Osd.above(btn, pt.y); open() }
        background: OsdPanel { }
        enter: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.motion.fast || 120 }
            NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: Theme.motion.normal || 200; easing.type: Easing.OutCubic }
        }
        exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Theme.motion.fast || 120 } }
    }
    // the dim help text of a popover
    component PopNote: Text {
        wrapMode: Text.Wrap
        color: page.o.muted
        font.pixelSize: 12
        font.family: Theme.type.family
        lineHeight: 1.1
    }

    // ---- a word of the subtitle that is in a Study deck: a small flash card for 5 s, shown with the subtitle ----
    Rectangle {
        id: flash
        readonly property var f: page.p.flash || null
        property var last: ({})
        onFChanged: if (f) last = f
        z: 20
        anchors { right: parent.right; bottom: parent.bottom; rightMargin: 28; bottomMargin: 120 }
        width: Math.min(300, parent.width - 56)
        height: col.implicitHeight + 24
        radius: page.o.cardR
        color: page.o.panel
        border.width: page.o.strokeW
        border.color: page.o.stroke
        opacity: f ? 1 : 0
        visible: opacity > 0.01
        Behavior on opacity { NumberAnimation { duration: 220 } }
        Column {
            id: col
            x: 16; y: 12
            width: parent.width - 32
            spacing: 4
            Row {
                spacing: 8
                Icon { name: "study"; size: 14; color: page.accentInk; anchors.verticalCenter: parent.verticalCenter }
                Text { text: "Anki"; color: page.accentInk; font.pixelSize: 11; font.weight: 600; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
            }
            Text { width: parent.width; text: flash.last.term || ""; color: page.o.fg; font.pixelSize: 30; font.weight: page.o.titleWeight; font.family: Theme.type.family; elide: Text.ElideRight }
            Text { width: parent.width; visible: !!flash.last.reading; text: flash.last.reading || ""; color: page.o.dim; font.pixelSize: 15; font.family: Theme.type.family; elide: Text.ElideRight }
            Text { width: parent.width; visible: !!flash.last.meaning; text: flash.last.meaning || ""; color: page.o.fg; font.pixelSize: 15; wrapMode: Text.Wrap; maximumLineCount: 3; elide: Text.ElideRight; font.family: Theme.type.family }
        }
    }

    // ---- editor: cut a part of the video, with its subtitle, kanji and readings, into an Anki deck of the Study mode ----
    property real clipStart: -1
    property real clipEnd: -1

    // ---- editor <-> mpv sync ----
    readonly property bool editing: editorPop.visible && clipStart >= 0 && clipEnd > clipStart
    property bool clipLoop: false        // at the end: false = pause on the end frame, true = back to the start (mpv A-B loop)
    property bool clipGuard: false       // playback started from the editor: watch time-pos against clipEnd
    property real seekTarget: -1

    // Frame-exact seeks while a handle is dragged, coalesced so mpv gets at most one seek every 40 ms (the last one wins).
    function clipSeek(t) { page.seekTarget = t; if (!seekPump.running) { seekPump.flush(); seekPump.start() } }
    Timer {
        id: seekPump
        interval: 40
        function flush() { if (page.seekTarget >= 0) { Core.cmd({cmd: "seek", pos: page.seekTarget, exact: true}); page.seekTarget = -1 } }
        onTriggered: flush()
    }
    function clipArmLoop() {
        if (page.editing && page.clipGuard && page.clipLoop) Core.cmd({cmd: "ab_loop", a: page.clipStart, b: page.clipEnd})
        else Core.cmd({cmd: "ab_loop"})
    }
    // "Preview": from the start frame, play up to the end.
    function clipPlay() {
        if (!page.editing) return
        seekPump.stop(); page.seekTarget = -1
        Core.cmd({cmd: "seek", pos: page.clipStart, exact: true})
        page.clipGuard = true
        page.clipArmLoop()
        Core.cmd({cmd: "set_pause", value: false})
    }
    function clipEditMoved(t) {
        if (page.clipGuard || !page.p.paused) { Core.cmd({cmd: "set_pause", value: true}); page.clipStop() }
        page.clipSeek(t)
    }
    function clipStop() {
        page.clipGuard = false
        Core.cmd({cmd: "ab_loop"})
    }
    // Space / play button / click on the video. In the editor: playing stops; paused plays inside the part
    // (from the start when the playhead is outside it or already on the end).
    function togglePlay() {
        if (!page.editing) { Core.cmd({cmd: "pause"}); return }
        if (!page.p.paused) { Core.cmd({cmd: "set_pause", value: true}); page.clipStop(); return }
        const t = page.p.pos || 0
        if (t < page.clipStart - 0.05 || t >= page.clipEnd - 0.05) { page.clipPlay(); return }
        page.clipGuard = true
        page.clipArmLoop()
        Core.cmd({cmd: "set_pause", value: false})
    }
    onClipLoopChanged: clipArmLoop()
    onEditingChanged: if (!editing) clipStop()
    // time-pos watch (the player state arrives every 50 ms): stop on the end frame. The loop needs no watch, mpv does it.
    Connections {
        target: Core
        enabled: page.clipGuard && !page.clipLoop
        function onPlayerChanged() {
            if (!page.editing) { page.clipStop(); return }
            if (page.p.paused) return
            if ((page.p.pos || 0) >= page.clipEnd) {
                Core.cmd({cmd: "set_pause", value: true})
                Core.cmd({cmd: "seek", pos: page.clipEnd, exact: true})
                page.clipStop()
            }
        }
    }
    OsdPop {
        id: editorPop
        contentWidth: Math.min(540, page.width - 48)   // the Column alone would size the popover to its widest child
        onClosed: editorHold.restart()
        // Safety net (the Editor button already chose a part round the playhead, see clipAroundPlayhead).
        onAboutToShow: { if (page.clipStart < 0 || page.clipEnd <= page.clipStart) page.clipAroundPlayhead(); trim.recenter() }
        contentItem: Column {
            width: Math.min(540, page.width - 48)
            spacing: 10
            // title, and the length of the part as a chip on the right
            Item {
                width: parent.width
                height: 30
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    Icon { name: "scissors"; size: 18; color: "#ffffff"; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: Core.tr(Core.uiLang, "Editor"); color: page.o.fg; font.pixelSize: page.o.titleSize; font.weight: page.o.titleWeight; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                }
                Rectangle {
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    visible: page.editing
                    height: 24
                    width: lenText.implicitWidth + 18
                    radius: page.ps === "fluent" ? 4 : height / 2
                    color: page.ps === "m3" ? "#4a4458" : Qt.alpha(Theme.c.accent, 0.22)
                    Text {
                        id: lenText
                        anchors.centerIn: parent
                        text: page.fmt(page.clipStart) + " – " + page.fmt(page.clipEnd) + "  ·  " + (page.clipEnd - page.clipStart).toFixed(1) + " s"
                        color: "#ffffff"
                        font.pixelSize: 12; font.weight: 600; font.family: Theme.type.family
                    }
                }
            }
            PopNote {
                width: parent.width
                text: Core.tr(Core.uiLang, "Drag the two round handles to mark where the part starts and ends; the bar zooms in by itself for the fine adjustment. The card gets the video, the subtitle of that time, the kanji with hiragana / katakana, and a description found on the internet.")
            }
            // the bar on a field of the theme, so its marks read on any video behind
            Rectangle {
                width: parent.width
                height: trim.implicitHeight + 16
                radius: Math.min(page.o.cardR, 16)
                color: "#26000000"
                border.width: page.ps === "m3" ? 0 : 1
                border.color: page.o.sep
                TrimTimeline {
                    id: trim
                    x: 8; y: 8
                    width: parent.width - 16
                    duration: Math.max(1, page.p.dur || 1)
                    position: page.p.pos || 0
                    playing: !page.p.paused
                    startTime: page.clipStart
                    endTime: page.clipEnd
                    // Each handle shows its own frame while it moves; editing a bound while the part plays stops the playback.
                    onStartEdited: function(t) { page.clipStart = t; page.clipEditMoved(t) }
                    onEndEdited: function(t) { page.clipEnd = t; page.clipEditMoved(t) }
                    onSeekRequested: function(t) { page.clipSeek(t) }
                }
            }
            Row {
                spacing: 8
                OsdButton { kind: "tonal"; glyph: "left"; iconSize: 16; text: Core.tr(Core.uiLang, "Start here"); implicitHeight: 32; onClicked: { const t = page.p.pos || 0; page.clipStart = t; if (page.clipEnd <= t + 0.05) page.clipEnd = Math.min(page.p.dur || t + 5, t + 5) } }
                OsdButton { kind: "tonal"; glyph: "right"; iconSize: 16; text: Core.tr(Core.uiLang, "End here"); implicitHeight: 32; onClicked: { const t = page.p.pos || 0; if (page.clipStart < 0 || t > page.clipStart + 0.05) page.clipEnd = t } }
                OsdButton { kind: "tonal"; glyph: "subtitles"; iconSize: 16; implicitHeight: 32; visible: page.p.subStart !== undefined && page.p.subStart >= 0 && page.p.subEnd > page.p.subStart; text: Core.tr(Core.uiLang, "This line"); ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Start and end of the subtitle on screen"); onClicked: { page.clipStart = page.p.subStart; page.clipEnd = page.p.subEnd } }
            }
            // The subtitle of the part, editable before saving; it follows the player until the user types in it.
            Rectangle {
                width: parent.width
                height: Math.max(64, Math.min(140, clipText.implicitHeight + 16))
                radius: page.ps === "m3" ? 8 : Osd.r(page.o.ctlR >= 999 ? 12 : page.o.ctlR + 2, height)
                color: page.o.field
                border.width: clipText.activeFocus ? 2 : (page.o.fieldStroke !== "#00000000" ? 1 : 0)
                border.color: clipText.activeFocus ? Theme.c.accent : page.o.fieldStroke
                // Windows: the accent line under a focused text box
                Rectangle { visible: page.ps === "fluent" && clipText.activeFocus; anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 1 } height: 2; color: Theme.c.accent }
                ScrollView {
                    anchors.fill: parent
                    anchors.margins: 4
                    TextArea {
                        id: clipText
                        property bool edited: false
                        wrapMode: TextArea.Wrap
                        color: page.o.fg
                        selectionColor: Theme.c.accent
                        selectedTextColor: Theme.c.accentFg
                        placeholderText: Core.tr(Core.uiLang, "Subtitle of this part")
                        placeholderTextColor: page.o.muted
                        font.pixelSize: 14
                        font.family: Theme.type.family
                        selectByMouse: true
                        background: null
                        onTextChanged: if (activeFocus) edited = true
                        Connections {
                            target: page
                            function onPChanged() { if (!clipText.edited && !clipText.activeFocus && clipText.text !== (page.p.subText || "")) clipText.text = page.p.subText || "" }
                        }
                        Connections {
                            target: editorPop
                            function onAboutToShow() { clipText.edited = false; clipText.text = page.p.subText || "" }
                        }
                    }
                }
            }
            Item {
                width: parent.width
                height: saveBtn.height
                Row {
                    spacing: 8
                    anchors.verticalCenter: parent.verticalCenter
                    OsdButton { kind: "tonal"; glyph: page.clipGuard && !page.p.paused ? "pause" : "play"; iconSize: 16; text: Core.tr(Core.uiLang, "Preview"); enabled: page.editing; onClicked: page.clipGuard && !page.p.paused ? page.togglePlay() : page.clipPlay() }
                    OsdButton { kind: page.clipLoop ? "accent" : "tonal"; glyph: "repeat"; iconSize: 16; text: Core.tr(Core.uiLang, "Loop"); ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "At the end: back to the start instead of pausing"); onClicked: page.clipLoop = !page.clipLoop }
                    OsdButton { kind: "tonal"; glyph: "star"; iconSize: 16; text: "Autocard"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Picture, sentence audio and context of the line on screen to the last Anki card (AnkiConnect)") + " · Ctrl+M"; onClicked: Core.cmd({cmd: "autocard"}) }
                }
                OsdButton {
                    id: saveBtn
                    anchors.right: parent.right
                    kind: "accent"
                    pill: page.ps !== "fluent"
                    text: page.p.clipBusy ? Core.tr(Core.uiLang, "Saving…") : Core.tr(Core.uiLang, "Save to studies")
                    enabled: !page.p.clipBusy && page.clipStart >= 0 && page.clipEnd > page.clipStart + 0.2
                    onClicked: Core.cmd({cmd: "clip_save", start: page.clipStart, end: page.clipEnd, text: clipText.text.trim()})
                }
            }
        }
    }

    // ---- speed (GTK: a slider from 0.25 to 4 and presets) ----
    OsdPop {
        id: speedPop
        contentWidth: 360   // the Column alone would size the popover to its widest child
        contentItem: Column {
            width: 360
            spacing: 10
            Item {
                width: parent.width
                height: 30
                Text { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Playback speed"); color: page.o.fg; font.pixelSize: page.o.titleSize; font.weight: page.o.titleWeight; font.family: Theme.type.family }
                Text { anchors { right: parent.right; verticalCenter: parent.verticalCenter } text: (Math.round((page.p.speed || 1) * 100) / 100) + "×"; color: page.accentInk; font.pixelSize: 20; font.weight: 600; font.family: Theme.type.family }
            }
            OsdSlider {
                id: spd
                width: parent.width
                from: 0.25; to: 4
                // follows the player, except while dragged
                Binding on value { when: !spd.pressed; restoreMode: Binding.RestoreNone; value: page.p.speed || 1 }
                onMoved: Core.cmd({cmd: "speed", value: Math.round(value * 20) / 20})
            }
            OsdSegmented {
                width: parent.width
                translate: false
                options: [["0.5", "0.5×"], ["1", "1×"], ["1.25", "1.25×"], ["1.5", "1.5×"], ["1.75", "1.75×"], ["2", "2×"], ["4", "4×"]]
                current: { const v = Math.round((page.p.speed || 1) * 100) / 100; return [0.5, 1, 1.25, 1.5, 1.75, 2, 4].indexOf(v) >= 0 ? String(v) : "" }
                onChosen: function(id) { Core.cmd({cmd: "speed", value: parseFloat(id)}) }
            }
        }
    }

    // ---- statistics (GTK fill_stats) ----
    OsdPop {
        id: statsPop
        contentWidth: 340   // the Column alone would size the popover to its widest child
        contentItem: Column {
            width: 340
            spacing: 8
            Text { text: Core.tr(Core.uiLang, "Statistics"); color: page.o.fg; font.pixelSize: page.o.titleSize; font.weight: page.o.titleWeight; font.family: Theme.type.family; bottomPadding: 2 }
            // the figures on a field, label on the left (dim) and value on the right
            Rectangle {
                width: parent.width
                height: statGrid.implicitHeight + 16
                radius: Math.min(page.o.cardR, 14)
                color: "#1fffffff"
                Grid {
                    id: statGrid
                    x: 12; y: 8
                    width: parent.width - 24
                    columns: 2
                    columnSpacing: 18
                    rowSpacing: 6
                    Repeater {
                        // two cells per row (label, value); a count model so the 4×/s state refresh does not rebuild them
                        model: page.arr(page.p.stats).length * 2
                        delegate: TextEdit {
                            required property int index
                            readonly property var row: page.arr(page.p.stats)[Math.floor(index / 2)] || []
                            readonly property var modelData: ({t: row[index % 2] !== undefined ? String(row[index % 2]) : "", dim: index % 2 === 0})
                            readOnly: true; selectByMouse: !modelData.dim; text: modelData.t; color: modelData.dim ? page.o.dim : page.o.fg; font.pixelSize: 13; font.weight: modelData.dim ? 400 : 600; font.family: Theme.type.family
                            selectionColor: Theme.c.accent; selectedTextColor: Theme.c.accentFg
                        }
                    }
                }
            }
            Text { visible: !!page.p.infoHash; topPadding: 4; text: Core.tr(Core.uiLang, "Info hash"); color: page.o.headColor; font.pixelSize: page.o.headSize; font.weight: page.o.headWeight; font.family: Theme.type.family }
            Row {
                visible: !!page.p.infoHash
                width: parent.width
                spacing: 8
                TextEdit { id: hashText; width: parent.width - copyBtn.width - 8; readOnly: true; selectByMouse: true; wrapMode: TextEdit.WrapAnywhere; text: page.p.infoHash || ""; color: page.o.fg; selectionColor: Theme.c.accent; selectedTextColor: Theme.c.accentFg; font.pixelSize: 12; font.family: Theme.type.mono; anchors.verticalCenter: parent.verticalCenter }
                OsdButton { id: copyBtn; kind: "tonal"; glyph: "copy"; iconSize: 16; text: Core.tr(Core.uiLang, "Copy"); implicitHeight: 32; anchors.verticalCenter: parent.verticalCenter; onClicked: { hashText.selectAll(); hashText.copy(); hashText.deselect(); Core.toast(Core.tr(Core.uiLang, "Copied")) } }
            }
        }
    }

    // ---- the words the macaw swapped into the subtitle this episode (studyplay.rs `player.studySwaps`) ----
    OsdPop {
        id: swapsPop
        contentWidth: 360
        contentItem: Column {
            width: 360
            spacing: 10
            Row {
                spacing: 10
                Mascot { size: 44; mood: "happy"; animate: false; tappable: false }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 300
                    Text { width: parent.width; wrapMode: Text.Wrap; text: Core.tr(Core.uiLang, "Words I swapped in this episode"); color: page.o.fg; font.pixelSize: page.o.titleSize; font.weight: page.o.titleWeight; font.family: Theme.type.family }
                    Text { width: parent.width; wrapMode: Text.Wrap; text: Core.tr(Core.uiLang, "I put the words you learned into the subtitle. At the credits I ask you about them."); color: page.o.dim; font.pixelSize: 12; font.family: Theme.type.family }
                }
            }
            ListView {
                id: swList
                width: parent.width
                height: Math.min(contentHeight, 300)
                clip: true
                spacing: 6
                model: page.arr(page.p.studySwaps).length
                boundsBehavior: Flickable.StopAtBounds
                delegate: Rectangle {
                    required property int index
                    readonly property var w: page.arr(page.p.studySwaps)[index] || ({})
                    width: swList.width
                    height: swCol.implicitHeight + 14
                    radius: Math.min(page.o.cardR, 12)
                    color: "#1fffffff"
                    Column {
                        id: swCol
                        x: 12; y: 7
                        width: parent.width - 24
                        spacing: 2
                        Text {
                            width: parent.width; wrapMode: Text.Wrap; textFormat: Text.StyledText
                            text: "<b>" + (w.word || "") + "</b>" + (w.reading ? " <font color='#b0ffffff'>" + w.reading + "</font>" : "") + "  →  " + (w.original || "") + (w.base ? " <font color='#b0ffffff'>(" + Core.tr(Core.uiLang, "form of") + " " + w.base + ")</font>" : "")
                            color: page.o.fg; font.pixelSize: 15; font.family: Theme.type.family
                        }
                        Text { width: parent.width; wrapMode: Text.Wrap; text: w.plain || ""; color: page.o.dim; font.pixelSize: 12; font.italic: true; font.family: Theme.type.family }
                    }
                }
            }
            Row {
                spacing: 8
                OsdButton { kind: "accent"; pill: page.ps !== "fluent"; glyph: "study"; text: Core.tr(Core.uiLang, "Make flashcards"); onClicked: Core.cmd({cmd: "study_swaps_cards"}) }
                OsdButton { kind: "tonal"; pill: page.ps !== "fluent"; text: Core.tr(Core.uiLang, "Close"); onClicked: swapsPop.close() }
            }
        }
    }

    // ---- preview picture above the seek bar ----
    // Pointer x on the bar -> exact time (`hoverT`). The picture shown is the nearest one already made (own cache, so it
    // follows a fast slide with no wait); the worker is asked for the exact slot, at most every 50 ms plus a last
    // ask when the pointer stops, so the final place is never skipped.
    property var thumbTimes: []          // slot times with a picture, sorted
    property var thumbUrls: ({})         // slot time -> file URL
    // the preview shows while the pointer is over the bar or the bar is being dragged (a finger on a phone)
    readonly property bool seekPeek: seekHover.hovered || seek.pressed
    property string thumbGen: ""
    property real thumbWant: -1
    property real thumbAsked: -1
    function thumbHover(mx) {
        const d = page.p.dur || 0
        seek.hoverX = Math.max(seek.leftPadding, Math.min(seek.leftPadding + seek.availableWidth, mx))
        seek.hoverT = Math.max(0, Math.min(1, (mx - seek.leftPadding) / Math.max(1, seek.availableWidth))) * d
        if (!page.p.thumbnails || !(d > 0)) return
        thumbBox.place()
        thumbBox.show(page.thumbNearest(seek.hoverT))
        page.thumbWant = seek.hoverT
        if (!thumbAsk.running) { thumbAsk.ask(); thumbAsk.start() }
    }
    Timer {
        id: thumbAsk
        interval: 50
        function ask() {
            if (page.thumbWant < 0 || Math.abs(page.thumbWant - page.thumbAsked) < 0.5) return
            page.thumbAsked = page.thumbWant
            Core.cmd({cmd: "request_thumb", t: page.thumbWant})
        }
        onTriggered: { ask(); if (page.seekPeek && Math.abs(page.thumbWant - page.thumbAsked) >= 0.5) restart() }
    }
    function thumbNearest(t) {
        const a = page.thumbTimes
        if (a.length === 0) return ""
        let lo = 0, hi = a.length - 1
        while (lo < hi) { const mid = (lo + hi) >> 1; if (a[mid] < t) lo = mid + 1; else hi = mid }
        const k = lo > 0 && Math.abs(a[lo - 1] - t) <= Math.abs(a[lo] - t) ? lo - 1 : lo
        return page.thumbUrls[a[k]]
    }
    function thumbReset() { page.thumbTimes = []; page.thumbUrls = ({}); page.thumbAsked = -1; thumbBox.url = "" }
    Connections {
        target: Core
        function onThumb(t, url) {
            const g = (url.match(/\/g(\d+)_s-?\d+\.jpg$/) || [])[1] || ""
            if (g !== page.thumbGen) { page.thumbReset(); page.thumbGen = g }
            if (page.thumbUrls[t] === undefined) {
                const a = page.thumbTimes
                let i = a.length
                while (i > 0 && a[i - 1] > t) i--
                a.splice(i, 0, t)
                page.thumbTimes = a
            }
            page.thumbUrls[t] = url
            if (page.seekPeek) thumbBox.show(page.thumbNearest(seek.hoverT))
        }
    }
    Rectangle {
        id: thumbBox
        property string url: ""
        function show(u) { if (u && u !== url) url = u }
        readonly property bool want: page.seekPeek && !!page.p.thumbnails && (page.p.dur || 0) > 0 && bottomBar.visible
        visible: want
        parent: Overlay.overlay
        width: 172
        height: thumbCol.implicitHeight + 12
        // a small card of the theme (Osd.js), the picture's corners concentric with it
        radius: Math.min(page.o.cardR, 14)
        color: page.o.panel
        border.width: page.o.strokeW
        border.color: page.o.stroke
        z: 20
        // Centred on the pointer, kept inside the window. Placed at each move: mapToItem is not a live binding, and the bar
        // slides in from below, so a position worked out once pointed far above the window.
        function place() {
            const pt = seek.mapToItem(Overlay.overlay, seek.hoverX, 0)
            x = Math.max(8, Math.min(page.width - width - 8, pt.x - width / 2))
            y = pt.y - height - 8
        }
        Column {
            id: thumbCol
            anchors.centerIn: parent
            spacing: 4
            Rectangle {
                width: 160; height: 90; radius: Math.max(3, thumbBox.radius - 6); color: "#000000"; clip: true
                // Small local JPEGs: decoded at once (no blank frame between two pictures) and kept in Qt's pixmap cache,
                // so places seen before come back instantly. File names never repeat, so the cache cannot be stale.
                Image {
                    id: thumbImg
                    anchors.fill: parent
                    source: thumbBox.url
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: false
                    cache: true
                    sourceSize.width: 320
                    onStatusChanged: if (status === Image.Error) console.log("[thumbs] could not read", source)
                }
                // Before the first picture of this video arrives
                Spinner { anchors.centerIn: parent; size: 26; running: thumbBox.visible && thumbBox.url === ""; visible: running }
            }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: page.fmt(seek.hoverT); color: page.o.fg; font.pixelSize: 13; font.weight: 600; font.family: Theme.type.family }
            Text { anchors.horizontalCenter: parent.horizontalCenter; width: 160; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; visible: text !== ""; text: page.studyMarkAt(seek.hoverT); color: page.accentInk; font.pixelSize: 12; font.weight: 600; font.family: Theme.type.family }
        }
    }

    // ---- episodes: Stremio-style tab on the right edge + drawer (hover opens, leaving the dock closes) ----
    // Look per theme. The player is always dark, so the colours are fixed dark OSD tones; the accent comes from the theme.
    readonly property var epLook: ({
        gnome:   { tabW: 28, tabH: 96, tabR: 12, tabBg: "#e6303030", tabHov: "#f23a3a3a", edge: "#1affffff", panelBg: "#f2303030", panelR: 0,  inset: 0,  rowR: 6,  rowHov: "#1affffff", rowCur: "#14ffffff", bar: false, shadow: false },
        windows: { tabW: 24, tabH: 80, tabR: 4,  tabBg: "#e62c2c2c", tabHov: "#f23a3a3a", edge: "#14ffffff", panelBg: "#f22c2c2c", panelR: 0,  inset: 0,  rowR: 4,  rowHov: "#0fffffff", rowCur: "#0affffff", bar: true,  shadow: true  },
        android: { tabW: 32, tabH: 72, tabR: 16, tabBg: "#ff2b2930", tabHov: "#ff36343b", edge: "transparent", panelBg: "#ff211f26", panelR: 28, inset: 0, rowR: 16, rowHov: "#14e6e0e9", rowCur: "#4a4458", bar: false, shadow: true  },
        apple:   { tabW: 26, tabH: 88, tabR: 14, tabBg: "#b31c1c1e", tabHov: "#cc2c2c2e", edge: "#33ffffff", panelBg: "#d91c1c1e", panelR: 22, inset: 10, rowR: 10, rowHov: "#14ffffff", rowCur: "#1fffffff", bar: false, shadow: true  }
    })
    readonly property var ek: epLook[Theme.look] || epLook.gnome
    readonly property int epMotion: Theme.motion && Theme.motion.normal ? Theme.motion.normal : 200
    readonly property int epEasing: Theme.motion && Theme.motion.easing === "OutQuint" ? Easing.OutQuint : Easing.OutCubic
    readonly property bool hasEpisodes: (!!page.p.hasNext || !!page.p.isSeries) && page.arr(page.p.episodes).length > 0

    Item {
        id: epDock
        z: 15
        visible: page.hasEpisodes
        readonly property int panelW: 340
        anchors { top: parent.top; bottom: parent.bottom; right: parent.right }
        // The strip that holds the tab and the drawer: leaving it (back to the video) closes the drawer.
        // Fixed width so nothing jumps while the drawer slides; HoverHandler and empty Items let clicks reach the video.
        width: page.ek.tabW + panelW + page.ek.inset * 2
        readonly property real openX: width - panelW - page.ek.inset

        HoverHandler {
            id: dockHover
            onHoveredChanged: hovered ? epLeave.stop() : epLeave.restart()
        }
        // Short debounce so a quick slip of the pointer does not close the drawer.
        Timer {
            id: epLeave
            interval: 180
            onTriggered: if (!dockHover.hovered && page.sideOpen) { page.sideOpen = false; page.poke() }
        }

        // ---- drawer ----
        Item {
            id: side
            width: epDock.panelW
            anchors { top: parent.top; bottom: parent.bottom; topMargin: page.ek.inset; bottomMargin: page.ek.inset }
            x: page.sideOpen ? epDock.openX : epDock.width + 24
            visible: page.sideOpen || xAnim.running
            Behavior on x { NumberAnimation { id: xAnim; duration: page.epMotion; easing.type: page.epEasing } }
            opacity: page.sideOpen ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: page.epMotion } }

            // Background: floating card (apple) or flush sheet with the leading corners rounded (android) / square (gnome, windows).
            Item {
                anchors.fill: parent
                clip: page.ek.inset === 0
                layer.enabled: page.ek.shadow
                layer.effect: MultiEffect { shadowEnabled: true; shadowColor: "#73000000"; shadowBlur: 1.0; shadowHorizontalOffset: -4; autoPaddingEnabled: true }
                Rectangle {
                    width: parent.width + (page.ek.inset === 0 ? page.ek.panelR : 0)
                    height: parent.height
                    radius: page.ek.panelR
                    color: page.ek.panelBg
                    border.width: page.ek.edge === "transparent" ? 0 : 1
                    border.color: page.ek.edge
                }
            }
            MouseArea { anchors.fill: parent; hoverEnabled: true; onPositionChanged: function(m) { page.pointerAt(parent, m.x, m.y) } }

            Row {
                id: sideHead
                x: 12; y: 12
                spacing: 8
                OsdButton { kind: "round"; glyph: "right"; anchors.verticalCenter: parent.verticalCenter; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Hide the episode list"); onClicked: { epLeave.stop(); page.sideOpen = false } }
                Text { text: Core.tr(Core.uiLang, "Episodes"); color: page.o.fg; font.pixelSize: page.ps === "glass" || page.ps === "m3" ? 22 : 20; font.weight: page.ps === "m3" ? 400 : (page.ps === "fluent" ? 600 : 700); font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
            }
            ListView {
                id: epList
                anchors { left: parent.left; right: parent.right; top: sideHead.bottom; bottom: parent.bottom; topMargin: 8; bottomMargin: 8; leftMargin: 6; rightMargin: 6 }
                clip: true
                spacing: page.ek.rowR >= 10 ? 4 : 0
                model: page.arr(page.p.episodes).length
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                delegate: Rectangle {
                    id: epRow
                    required property int index
                    readonly property var modelData: page.arr(page.p.episodes)[index] || ({})
                    readonly property bool seen: modelData.watched || modelData.current
                    width: ListView.view.width
                    height: 70
                    radius: page.ek.rowR
                    color: epA.containsMouse ? page.ek.rowHov
                         : epRow.modelData.current && page.ek.rowCur !== "" ? page.ek.rowCur
                         : (seen ? "transparent" : "#4d000000")
                    opacity: seen || epA.containsMouse ? 1 : 0.55
                    Behavior on color { ColorAnimation { duration: 120 } }
                    // M3: current episode on a tonal accent layer
                    Rectangle {
                        visible: page.ek.rowCur === "" && epRow.modelData.current && !epA.containsMouse
                        anchors.fill: parent; radius: parent.radius
                        color: Theme.c.accent; opacity: 0.24
                    }
                    // Fluent list selection indicator
                    Rectangle {
                        visible: page.ek.bar && epRow.modelData.current
                        width: 3; height: 18; radius: 1.5
                        color: Theme.c.accent
                        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                    }
                    RoundedPicture {
                        id: epPic
                        visible: !!epRow.modelData.thumbnail || !!page.p.episodeFallback
                        x: 10
                        anchors.verticalCenter: parent.verticalCenter
                        width: 96; height: 54; radius: Math.max(4, page.ek.rowR - 2)
                        url: epRow.modelData.thumbnail || ""
                        fallback: page.p.episodeFallback || ""
                        blur: !epRow.seen && page.p.blurUnwatched ? 0.6 : 0
                    }
                    Text {
                        anchors { left: epPic.visible ? epPic.right : parent.left; leftMargin: epPic.visible ? 10 : 14; right: epMark.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                        text: (epRow.modelData.episode || 0) + ".  " + (epRow.modelData.title || (Core.tr(Core.uiLang, "Episode") + " " + (epRow.modelData.episode || 0)))
                        color: "#ffffff"; font.pixelSize: 14; font.family: Theme.type.family
                        font.weight: epRow.modelData.current ? Font.DemiBold : Font.Normal
                        wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
                    }
                    Item {
                        id: epMark
                        anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                        width: epRow.modelData.current ? 18 : (epRow.modelData.watched ? wb.width : 0)
                        height: 20
                        Icon { visible: epRow.modelData.current; name: "play"; size: 18; color: Theme.look === "gnome" ? "#ffffff" : page.accentInk; anchors.centerIn: parent }
                        Rectangle {
                            id: wb
                            visible: !epRow.modelData.current && epRow.modelData.watched
                            width: wbt.implicitWidth + 12; height: 20; radius: Theme.look === "android" || Theme.look === "apple" ? 10 : 4; color: "#2ec27e"
                            Text { id: wbt; anchors.centerIn: parent; text: Core.tr(Core.uiLang, "WATCHED"); color: "#04210f"; font.pixelSize: 10; font.weight: Font.ExtraBold; font.family: Theme.type.family }
                        }
                    }
                    MouseArea {
                        id: epA
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            epLeave.stop()
                            page.sideOpen = false
                            function two(v) { return (v < 10 ? "0" : "") + v }
                            Core.cmd({cmd: "play_episode", id: epRow.modelData.id, label: "S" + two(epRow.modelData.season || 0) + "E" + two(epRow.modelData.episode || 0)})
                        }
                    }
                }
            }
            onVisibleChanged: {
                if (!visible) return
                const eps = page.arr(page.p.episodes)
                for (let i = 0; i < eps.length; i++) if (eps[i].current) { epList.positionViewAtIndex(i, ListView.Center); break }
            }
        }

        // ---- tab (trigger): glued to the right edge, rides on the drawer's left edge when open ----
        Item {
            id: epTab
            width: page.ek.tabW
            height: page.ek.tabH
            anchors.verticalCenter: parent.verticalCenter
            x: page.sideOpen ? epDock.openX - width - (page.ek.inset > 0 ? 6 : 0) : epDock.width - width
            Behavior on x { NumberAnimation { duration: page.epMotion; easing.type: page.epEasing } }
            opacity: page.shown || page.sideOpen || tabArea.containsMouse ? 1 : 0
            enabled: opacity > 0
            Behavior on opacity { NumberAnimation { duration: Theme.motion && Theme.motion.fast ? Theme.motion.fast : 120 } }

            // Rounded on the left only when glued to the edge; fully rounded when it floats beside an open apple card.
            Item {
                anchors.fill: parent
                clip: true
                layer.enabled: page.ek.shadow
                layer.effect: MultiEffect { shadowEnabled: true; shadowColor: "#59000000"; shadowBlur: 0.6; shadowHorizontalOffset: -2; autoPaddingEnabled: true }
                Rectangle {
                    readonly property bool glued: !(page.sideOpen && page.ek.inset > 0)
                    width: parent.width + (glued ? page.ek.tabR : 0)
                    height: parent.height
                    radius: page.ek.tabR
                    color: tabArea.containsMouse || page.sideOpen ? page.ek.tabHov : page.ek.tabBg
                    border.width: page.ek.edge === "transparent" ? 0 : 1
                    border.color: page.ek.edge
                    Behavior on color { ColorAnimation { duration: 120 } }
                }
            }
            // Fluent: accent pill on the tab while hovered or open; M3: accent state colour on the icon.
            Rectangle {
                visible: page.ek.bar && (tabArea.containsMouse || page.sideOpen)
                width: 3; height: 16; radius: 1.5; color: Theme.c.accent
                anchors { left: parent.left; leftMargin: 3; verticalCenter: parent.verticalCenter }
            }
            Column {
                anchors.centerIn: parent
                spacing: 6
                Icon { name: "episodes"; size: 18; anchors.horizontalCenter: parent.horizontalCenter; color: Theme.look === "android" && (tabArea.containsMouse || page.sideOpen) ? page.accentInk : "#ffffff" }
                Icon { name: page.sideOpen ? "right" : "left"; size: 16; anchors.horizontalCenter: parent.horizontalCenter; color: "#b3ffffff" }
            }
            MouseArea {
                id: tabArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                // Hover opens at once; click toggles (touch screens and pinning back).
                property double openedAt: 0
                onEntered: { epLeave.stop(); if (!page.sideOpen) { page.sideOpen = true; openedAt = Date.now() } page.poke() }
                // A tap right after the hover-open (touch sends both) must not close it again.
                onClicked: { epLeave.stop(); if (Date.now() - openedAt > 350) page.sideOpen = !page.sideOpen }
                ToolTip.visible: containsMouse && !page.sideOpen
                ToolTip.text: Core.tr(Core.uiLang, "Episodes")
            }
        }
    }

    // The globe button's search bar and results window (QuickSearch.qml), above the controls.
    QuickSearch {
        id: quickSearch
        anchors.fill: parent
        z: 60
        onOpenInResearch: function(url) { if (page.window && page.window.researchOpen) page.window.researchOpen(url) }
    }

    // ---- keys: the configurable shortcuts of Preferences > Shortcuts (GTK do_action) ----
    function action(id) {
        var v
        switch (id) {
        case "back": if (page.window.visibility === Window.FullScreen) page.window.showNormal(); else Core.cmd({cmd: "close_player"}); break
        case "play_pause": page.togglePlay(); break
        case "seek_back": Core.cmd({cmd: "seek_relative", secs: -10}); break
        case "seek_forward": Core.cmd({cmd: "seek_relative", secs: 10}); break
        case "seek_back_big": Core.cmd({cmd: "seek_relative", secs: -60}); break
        case "seek_forward_big": Core.cmd({cmd: "seek_relative", secs: 60}); break
        case "skip": Core.cmd({cmd: "skip_forward"}); break
        case "volume_up": Core.cmd({cmd: "volume", value: Math.min(150, (page.p.volume || 0) + 5)}); break
        case "volume_down": Core.cmd({cmd: "volume", value: Math.max(0, (page.p.volume || 0) - 5)}); break
        case "mute": Core.cmd({cmd: "toggle_mute"}); break
        case "fullscreen": page.toggleFullscreen(); break
        case "speed_up": case "speed_down": case "speed_reset":
            v = id === "speed_reset" ? 1 : Math.max(0.25, Math.min(4, (page.p.speed || 1) + (id === "speed_up" ? 0.25 : -0.25)))
            Core.cmd({cmd: "speed", value: v})
            Core.toast(Core.tr(Core.uiLang, "Speed") + " " + Math.round(v * 100) / 100 + "×")
            break
        case "sub_delay_less": case "sub_delay_more":
            v = (page.p.subDelay || 0) + (id === "sub_delay_more" ? 0.1 : -0.1)
            Core.cmd({cmd: "sub_delay", value: v})
            Core.toast(Core.tr(Core.uiLang, "Subtitle delay") + " " + (v >= 0 ? "+" : "") + v.toFixed(1) + " s")
            break
        case "next_episode": Core.cmd({cmd: "next_episode"}); break
        case "prev_episode": Core.cmd({cmd: "prev_episode"}); break
        case "tracks_menu": subsMenu.openAbove(sBtn); break
        case "xray": if (xray.hasData) { xray.pinned = !xray.open; if (!xray.pinned) { xray.autoOpened = false; xray.hoverOpen = false } } break
        case "autocard": Core.cmd({cmd: "autocard"}); break
        }
        page.poke()
    }
    // controller mode: what the buttons do in the player, a dark pill at the lower right just above the control bar
    // (HP: at the top it covered the title of the episode); it hides with the OSD
    PadHints {
        visible: !!(page.window && page.window.padMode) && page.shown && bottomBar.visible
        kind: page.window ? page.window.padKind : "xbox"
        hints: page.window ? page.window.playerHints : []
        color: "#cc000000"
        // two rows when the window is too narrow for one
        columns: page.width < 1100 ? Math.ceil(hints.length / 2) : 0
        x: parent.width - width - 24
        y: bottomBar.y + bottomBar.panelTop - height - 12
        z: 50
    }
    Instantiator {
        model: Core.prefs.extra !== undefined && Core.prefs.extra.shortcuts !== undefined ? Core.prefs.extra.shortcuts : []
        delegate: Shortcut {
            required property var modelData
            enabled: page.visible && !studyQuiz.busy   // the keys belong to the question while it shows
            sequence: Accel.toSequence(modelData.key)
            onActivated: page.action(modelData.id)
        }
    }
    // Ctrl+0 turns Anime4K off, Ctrl+1 to Ctrl+6 pick a mode.
    Instantiator {
        model: 7
        delegate: Shortcut {
            required property int index
            enabled: page.visible
            sequence: "Ctrl+" + index
            onActivated: Core.cmd({cmd: "upscale_mode", n: index})
        }
    }
}
