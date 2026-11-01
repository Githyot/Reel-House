import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import Reelhouse 1.0
import "KanaHover.js" as Kana

// Study: the review of a deck, in the way of Anki's reviewer but laid out as one card in the middle of the page.
// Top: back, the deck, the progress of the session, the three counts (the queue of the card underlined), undo, edit, menu.
// Middle: the card; the answer appears under a line in the same card. Bottom: "Show answer" (Space), then
// Again / Hard / Good / Easy (1-4) with the time until the card comes back above each. The end of the deck shows what was done.
// The game (shared with Learn): a ring with the progress of the session, the combo of right answers in a row (a flame),
// XP for each right answer going to the same daily goal as the lessons, and the macaw in the corner reacting to the
// answers. None of it waits for an animation: the keys work at once.
Item {
    id: rv
    property var session: null
    property bool pageVisible: true
    property color cNew: "#3584e4"
    property color cLearn: Theme.c.error
    property color cDue: Theme.c.success
    property color cHard: Theme.c.warning
    readonly property var card: session && session.card ? session.card : null
    readonly property bool shown: session !== null && session.showAnswer === true
    readonly property real cardId: card ? card.id : 0
    readonly property bool narrow: width < 620
    readonly property real cw: Math.min(900, width - Theme.spacing.page * 2)
    readonly property real cardW: Math.min(760, width - (narrow ? 24 : Theme.spacing.page * 2))
    readonly property bool busyPopup: editPop.opened || moreMenu.opened
    readonly property bool keysOn: pageVisible && visible && !busyPopup
    function tr(s) { return Core.tr(Core.uiLang, s) }
    // ---- controller: each grade is a button (HP). Before the answer: ✕/A shows it, □/X replays the sound. After:
    // □/X Again, △/Y Hard, ✕/A Good, ○/B Easy (Xbox X / Y / A / B). Main.qml hands the buttons here first while
    // this page is registered as its `padPage`.
    readonly property var padWin: Window.window
    readonly property bool padOn: !!(padWin && padWin.padMode)
    readonly property string padKind: padWin && padWin.padKind ? padWin.padKind : "xbox"
    readonly property bool padActive: keysOn && card !== null
    readonly property var gradeButtons: ["X", "Y", "A", "B"]
    function padRegister() {
        const w = padWin
        if (!w || w.padPage === undefined) return
        if (padActive) w.padPage = rv
        else if (w.padPage === rv) w.padPage = null
    }
    onPadActiveChanged: padRegister()
    Component.onDestruction: { const w = padWin; if (w && w.padPage === rv) w.padPage = null }
    function padButton(b) {
        if (!padActive) return false
        if (!shown) {
            if (b === "A") { reveal(); return true }
            if (b === "X") { playSide(true); return true }
            return false
        }
        const i = gradeButtons.indexOf(b)
        if (i < 0) return false
        answer(i + 1)
        return true
    }
    readonly property var padHints: shown
        ? [{button: "X", text: tr("Again")}, {button: "Y", text: tr("Hard")}, {button: "A", text: tr("Good")}, {button: "B", text: tr("Easy")}]
        : [{button: "A", text: tr("Show answer")}, {button: "X", text: tr("Replay")}, {button: "LB", text: tr("Sections")}]
    // one of the macaw's funny lines (src/mascot_lines.rs), random and never the last one
    function quip(m) { const r = Core.state("mascot:" + Core.uiLang + ":" + m); return r && r.text ? r.text : "" }
    // XP, goal, streak, and the session's combo (controller_study.rs study_game_json)
    readonly property var game: Core.study.game !== undefined && Core.study.game !== null ? Core.study.game : ({})
    readonly property int combo: game.combo || 0
    readonly property color cFlame: "#ff9600"
    readonly property color cXp: "#f2b01e"

    // ---- the session's own tally (for the end screen) ----
    readonly property real sid: session ? session.id : 0
    property var tally: [0, 0, 0, 0]
    property real startedAt: 0
    property real endedAt: 0
    Component.onCompleted: { startedAt = Date.now(); lastCombo = game.combo || 0; lastXp = game.xp || 0; padRegister() }
    onSidChanged: { tally = [0, 0, 0, 0]; startedAt = Date.now(); endedAt = 0; lastCombo = game.combo || 0; lastXp = game.xp || 0 }
    onCardChanged: if (card === null && session !== null && endedAt === 0) endedAt = Date.now()
    function answer(e) {
        if (!card || !shown) return
        const t = tally.slice(); t[e - 1]++; tally = t
        buddy.react(e)
        Core.cmd({cmd: "study_answer", ease: e})
    }
    function reveal() { if (card && !shown) Core.cmd({cmd: "study_show"}) }
    function openMore() { if (card) moreMenu.openBelow(moreBtn, true) }
    function mmss(ms) { const s = Math.max(0, Math.round(ms / 1000)); const m = Math.floor(s / 60); return m + ":" + ("0" + (s % 60)).slice(-2) }

    // ---- the parts of the answer: Anki's "{{FrontSide}}<hr id=answer>Back" shows the back under a line ----
    readonly property int hrAt: card && card.a ? card.a.search(/<hr[^>]*id\s*=\s*["']?answer["']?[^>]*>/i) : -1
    // A clip card shows its video in the card itself (the stage below), so the picture and the "play" link leave the text:
    // the question is the video (when asked for), the answer is the subtitle.
    function bare(html) {
        if (!rv.hasClip) return html
        return html.replace(/<img[^>]*>/gi, "").replace(/<a[^>]*href="clip:[^"]*"[^>]*>[\s\S]*?<\/a>/gi, "").replace(/^(\s*<br\s*\/?>)+/i, "")
    }
    readonly property string posterSrc: { const m = card && card.q ? /<img[^>]*\ssrc="([^"]+)"/i.exec(card.q) : null; return m ? m[1] : "" }
    readonly property string frontHtml: !card ? "" : bare(shown && hrAt < 0 ? card.a : card.q)
    readonly property string backHtml: card && hrAt >= 0 ? bare(card.a.substring(hrAt).replace(/^<hr[^>]*>/i, "")) : ""
    // the clip inside the card: nothing plays until the viewer asks
    property bool clipOn: false
    property var clipSt: ({})
    function playClip() { if (!card || !card.clip) return; clipOn = true; Core.cmd({cmd: "study_play_clip", name: card.clip}) }
    function toggleClip() { if (!clipOn || clipSt.ended === true) playClip(); else Core.cmd({cmd: "study_clip_pause"}) }
    Timer { interval: 250; repeat: true; running: rv.clipOn && rv.pageVisible; onTriggered: rv.clipSt = Core.state("study_clip") }
    onShownChanged: { kanaLink = ""; if (shown && hrAt >= 0) backIn.restart(); else if (shown) frontIn.restart() }
    onCardIdChanged: { kanaLink = ""; clipOn = false; clipSt = ({}); Core.cmd({cmd: "study_stop_clip"}); if (card) frontIn.restart(); textFl.contentY = 0 }

    // ---- sounds: they play by themselves when a side shows (Anki autoplay); R plays them again ----
    property string playedKey: ""
    function playSide(force) {
        const c = rv.card
        if (!c) { if (rv.playedKey !== "") { rv.playedKey = ""; Core.cmd({cmd: "study_stop_sounds"}) } return }
        const key = c.id + (rv.shown ? "a" : "q")
        if (!force && key === rv.playedKey) return
        rv.playedKey = key
        const list = rv.shown ? (c.aSounds || []) : (c.qSounds || [])
        if (list.length > 0 || force) Core.cmd({cmd: "study_play_sounds", names: rv.shown && force ? (c.qSounds || []).concat(c.aSounds || []) : list})
        else Core.cmd({cmd: "study_stop_sounds"})
    }
    function openLink(l) {
        if (l.indexOf("jp:") === 0) return   // a word of the kana reader: it only shows its reading on hover
        if (l.indexOf("sound:") === 0) { Core.cmd({cmd: "study_play_sounds", names: [decodeURIComponent(l.substring(6))]}); return }
        // a clip of the Editor: played in mpv, not in the system player
        if (/\.(mp4|webm|mkv|mov|m4v)$/i.test(l)) { rv.clipOn = true; Core.cmd({cmd: "study_play_clip", name: decodeURIComponent(l.split("/").pop())}); return }
        Qt.openUrlExternally(l)
    }
    Connections { target: Core; function onStudyChanged() { if (rv.pageVisible) rv.playSide(false) } }
    onSessionChanged: if (session === null) playSide(false)
    readonly property bool hasSound: card !== null && ((card.qSounds || []).length + (card.aSounds || []).length) > 0
    readonly property bool hasClip: card !== null && !!card.clip
    // ---- the hiragana / katakana reader: hovering a Japanese word of the card shows its kana and romaji (KanaHover.js) ----
    readonly property var kanaDict: card ? Kana.dict((card.a || "") + (card.q || "")) : ({})
    property string kanaLink: ""
    property Item kanaFrom: null
    property point kanaAt: Qt.point(0, 0)
    function kanaHover(item, link, pos) {
        if (link.indexOf("jp:") !== 0) { if (kanaFrom === item) kanaLink = ""; return }
        kanaFrom = item; kanaLink = link; kanaAt = item.mapToItem(rv, pos.x, pos.y)
    }

    readonly property int total: session ? (session.done || 0) + (session.new || 0) + (session.learn || 0) + (session.review || 0) : 0
    readonly property real progress: total > 0 ? (session.done || 0) / total : 0

    // ================= top bar =================
    Item {
        id: hdr
        x: (rv.width - rv.cw) / 2
        y: Theme.layout.largeTitles === true ? 4 : 12
        width: rv.cw
        height: 48
        FlatButton {
            id: backBtn
            anchors { left: parent.left; verticalCenter: parent.verticalCenter }
            kind: Theme.look === "android" || Theme.look === "apple" ? "flatround" : "flat"
            glyph: "left"; iconSize: 22; implicitWidth: 40; implicitHeight: 40
            ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: rv.tr("Back to the decks") + " (Esc)"
            onClicked: Core.cmd({cmd: "study_close"})
        }
        Column {
            anchors { left: backBtn.right; leftMargin: 8; right: rightSide.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
            spacing: 0
            readonly property var parts: rv.session ? rv.session.name.split("::") : [""]
            Text {
                width: parent.width; elide: Text.ElideRight
                text: parent.parts[parent.parts.length - 1]
                color: Theme.c.text; font.pixelSize: 17; font.weight: Font.Bold; font.family: Theme.type.family
            }
            Text {
                visible: parent.parts.length > 1
                width: parent.width; elide: Text.ElideMiddle
                text: parent.parts.slice(0, -1).join("  ›  ")
                color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
            }
        }
        Row {
            id: rightSide
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: rv.narrow ? 2 : 6
            // the three counts, the queue of the card on screen underlined (Anki)
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: rv.narrow ? 8 : 12
                rightPadding: rv.narrow ? 4 : 10
                Repeater {
                    model: 3
                    delegate: Item {
                        required property int index
                        readonly property string q: ["new", "learn", "review"][index]
                        readonly property color tone: [rv.cNew, rv.cLearn, rv.cDue][index]
                        readonly property bool cur: rv.card !== null && rv.card.kind === q
                        width: Math.max(18, num.implicitWidth); height: 30
                        Text {
                            id: num
                            anchors.centerIn: parent
                            text: rv.session ? "" + (rv.session[parent.q] || 0) : ""
                            color: parent.tone
                            font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family
                        }
                        Rectangle {
                            visible: parent.cur
                            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom }
                            width: num.implicitWidth + 4; height: 2; radius: 1
                            color: parent.tone
                        }
                        ToolTip.visible: hov.hovered; ToolTip.delay: 400
                        ToolTip.text: [rv.tr("New"), rv.tr("Learning"), rv.tr("To review")][index]
                        HoverHandler { id: hov }
                    }
                }
            }
            FlatButton {
                visible: rv.hasSound && !rv.narrow
                anchors.verticalCenter: parent.verticalCenter
                kind: "flat"; glyph: "audio"; iconSize: 20; implicitWidth: 38; implicitHeight: 38
                ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: rv.tr("Play the sound again") + " (R)"
                onClicked: rv.playSide(true)
            }
            FlatButton {
                anchors.verticalCenter: parent.verticalCenter
                kind: "flat"; glyph: "undo"; iconSize: 20; implicitWidth: 38; implicitHeight: 38
                enabled: rv.session !== null && rv.session.canUndo === true
                ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: rv.tr("Undo") + " (Ctrl+Z)"
                onClicked: Core.cmd({cmd: "study_undo"})
            }
            FlatButton {
                anchors.verticalCenter: parent.verticalCenter
                kind: "flat"; glyph: "edit"; iconSize: 20; implicitWidth: 38; implicitHeight: 38
                enabled: rv.card !== null && rv.card.nid !== undefined
                ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: rv.tr("Edit this card") + " (E)"
                onClicked: editPop.open()
            }
            FlatButton {
                id: moreBtn
                anchors.verticalCenter: parent.verticalCenter
                kind: "flat"; glyph: "more"; iconSize: 20; implicitWidth: 38; implicitHeight: 38
                enabled: rv.card !== null
                ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: rv.tr("More")
                onClicked: rv.openMore()
            }
        }
    }
    // ================= the game: progress ring, combo, XP and the daily goal =================
    component Ring: Item {
        id: ring
        property real frac: 0
        property color tone: Theme.c.accent
        property real thick: 4
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: Theme.c.fill3; strokeWidth: ring.thick; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                PathAngleArc { centerX: ring.width / 2; centerY: ring.height / 2; radiusX: ring.width / 2 - ring.thick / 2; radiusY: radiusX; startAngle: 0; sweepAngle: 360 }
            }
            ShapePath {
                strokeColor: ring.tone; strokeWidth: ring.thick; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                PathAngleArc { centerX: ring.width / 2; centerY: ring.height / 2; radiusX: ring.width / 2 - ring.thick / 2; radiusY: radiusX; startAngle: -90; sweepAngle: 360 * Math.max(0.001, Math.min(1, ring.frac)) }
            }
        }
    }
    Item {
        id: bar
        visible: rv.card !== null
        x: hdr.x; width: hdr.width
        anchors { top: hdr.bottom; topMargin: 4 }
        height: 34
        // the session: answered / (answered + still to do)
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Ring {
                width: 30; height: 30
                anchors.verticalCenter: parent.verticalCenter
                frac: rv.progress
                Behavior on frac { NumberAnimation { duration: Theme.motion.normal; easing.type: Easing.OutCubic } }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: (rv.session ? rv.session.done : 0) + " / " + rv.total
                color: Theme.c.textDim; font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family
            }
            // the combo: a flame with the right answers in a row; it pops at each one and burns brighter from 5
            Rectangle {
                id: comboChip
                visible: rv.combo >= 2
                anchors.verticalCenter: parent.verticalCenter
                width: comboRow.implicitWidth + 16; height: 26; radius: 13
                color: rv.combo >= (rv.game.fire || 10) ? "#ff4b4b" : (rv.combo >= 5 ? rv.cFlame : Qt.rgba(1, 0.59, 0, 0.18))
                Row {
                    id: comboRow
                    anchors.centerIn: parent
                    spacing: 4
                    LearnGlyph { name: "flame"; size: 16; color: rv.combo >= 5 ? "#ffffff" : rv.cFlame; filled: true; fillColor: rv.combo >= 5 ? "#ffd27a" : Qt.rgba(1, 0.59, 0, 0.35); anchors.verticalCenter: parent.verticalCenter }
                    Text { text: rv.combo + " " + rv.tr("in a row"); color: rv.combo >= 5 ? "#ffffff" : rv.cFlame; font.pixelSize: 12; font.weight: Font.Black; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                }
                ToolTip.visible: comboHov.hovered; ToolTip.delay: 400
                ToolTip.text: rv.tr("From %1 in a row each right answer gives double XP").arg(rv.game.fire || 10)
                HoverHandler { id: comboHov }
            }
        }
        // today's XP towards the daily goal (the same goal as the Learn lessons), and this session's XP
        Row {
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: 8
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 3
                LearnGlyph { name: "zap"; size: 16; color: rv.cXp; filled: true; fillColor: Qt.rgba(0.95, 0.69, 0.12, 0.35); anchors.verticalCenter: parent.verticalCenter }
                Text {
                    id: sessionXp
                    anchors.verticalCenter: parent.verticalCenter
                    text: "+" + (rv.game.xp || 0) + " XP"
                    color: rv.cXp; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family
                }
            }
            Rectangle {
                visible: !rv.narrow
                anchors.verticalCenter: parent.verticalCenter
                width: 96; height: 8; radius: 4
                color: Theme.c.fill3
                Rectangle {
                    readonly property real f: Math.min(1, (rv.game.xpToday || 0) / Math.max(1, rv.game.goal || 20))
                    width: Math.max(f > 0 ? 8 : 0, parent.width * f); height: parent.height; radius: 4
                    color: f >= 1 ? Theme.c.success : rv.cXp
                    Behavior on width { NumberAnimation { duration: Theme.motion.normal; easing.type: Easing.OutCubic } }
                }
                ToolTip.visible: goalHov.hovered; ToolTip.delay: 300
                ToolTip.text: rv.tr("Daily goal") + ": " + (rv.game.xpToday || 0) + " / " + (rv.game.goal || 20) + " XP"
                HoverHandler { id: goalHov }
            }
            Text {
                visible: !rv.narrow
                anchors.verticalCenter: parent.verticalCenter
                text: (rv.game.xpToday || 0) + "/" + (rv.game.goal || 20)
                color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
            }
        }
    }
    // the combo pops at each right answer; "+XP" floats up from the answer buttons
    property int lastCombo: 0
    property int lastXp: 0
    onGameChanged: {
        const c = game.combo || 0, x = game.xp || 0
        if (c > lastCombo && c >= 2) comboPop.restart()
        if (x > lastXp) { xpFloat.text = "+" + (x - lastXp) + " XP"; floatUp.restart() }
        if (c > lastCombo && (c === 5 || c === 10 || (c >= 25 && c % 25 === 0))) buddy.milestone(c)
        lastCombo = c; lastXp = x
    }
    SequentialAnimation {
        id: comboPop
        NumberAnimation { target: comboChip; property: "scale"; from: 1; to: 1.25; duration: 120; easing.type: Easing.OutQuad }
        NumberAnimation { target: comboChip; property: "scale"; to: 1; duration: 220; easing.type: Easing.OutBack }
    }
    Text {
        id: xpFloat
        z: 5
        opacity: 0
        x: (rv.width - width) / 2
        y: bottomBar.y - 10
        color: rv.cXp
        font.pixelSize: 18; font.weight: Font.Black; font.family: Theme.type.family
        style: Text.Outline; styleColor: Qt.rgba(0, 0, 0, 0.25)
        ParallelAnimation {
            id: floatUp
            NumberAnimation { target: xpFloat; property: "y"; from: bottomBar.y - 4; to: bottomBar.y - 54; duration: 750; easing.type: Easing.OutCubic }
            SequentialAnimation {
                NumberAnimation { target: xpFloat; property: "opacity"; from: 0; to: 1; duration: 120 }
                PauseAnimation { duration: 360 }
                NumberAnimation { target: xpFloat; property: "opacity"; to: 0; duration: 270 }
            }
        }
    }

    // ================= the card =================
    Item {
        id: stage
        visible: rv.card !== null
        anchors { left: parent.left; right: parent.right; top: bar.bottom; topMargin: 16; bottom: bottomBar.top; bottomMargin: 16 }

        StudySurface {
            id: face
            raised: true
            width: rv.cardW
            height: Math.min(stage.height, Math.max(Math.min(stage.height, 260), col.implicitHeight + pad * 2))
            readonly property real pad: rv.narrow ? 22 : 36
            anchors.centerIn: parent
            // no animation of the height: the card is a glass / shadow layer, and a layer that changes size every frame
            // showed green boxes on some graphics cards

            // marked / flag in the corner
            Row {
                z: 2
                anchors { top: parent.top; right: parent.right; margins: 14 }
                spacing: 6
                Icon { visible: rv.card !== null && rv.card.marked === true; name: "star"; size: 18; color: Theme.c.rating }
                Rectangle {
                    visible: rv.card !== null && rv.card.flag > 0
                    width: 14; height: 14; radius: 7
                    anchors.verticalCenter: parent.verticalCenter
                    color: rv.card && rv.card.flag > 0 ? moreMenu.flags[rv.card.flag - 1] : "transparent"
                }
            }

            Flickable {
                id: textFl
                anchors.fill: parent
                anchors.margins: face.pad
                contentWidth: width
                contentHeight: Math.max(height, col.implicitHeight)
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                Column {
                    id: col
                    width: textFl.width
                    y: Math.max(0, (textFl.height - implicitHeight) / 2)
                    spacing: 0
                    // the clip of a clip card, played here (the pictures come from a second mpv of the core)
                    Item {
                        id: clipStage
                        visible: rv.hasClip
                        width: parent.width
                        height: visible ? clipBox.height + 12 : 0
                        Rectangle {
                            id: clipBox
                            width: Math.min(parent.width, 380)
                            height: Math.round(width * 9 / 16)
                            anchors.horizontalCenter: parent.horizontalCenter
                            radius: Theme.shape.card !== undefined ? Theme.shape.card : 12
                            color: "#000000"
                            clip: true
                            Image {
                                anchors.fill: parent
                                visible: !clipView.hasFrame && source !== ""
                                source: rv.posterSrc
                                fillMode: Image.PreserveAspectFit
                                asynchronous: true
                            }
                            ThemeVideo {
                                id: clipView
                                anchors.fill: parent
                                source: "clip"
                                radius: clipBox.radius
                                running: rv.clipOn && rv.pageVisible
                            }
                            // play / pause / again, on the picture
                            Rectangle {
                                anchors.centerIn: parent
                                width: 56; height: 56; radius: 28
                                color: "#99000000"
                                visible: !rv.clipOn || rv.clipSt.paused === true || rv.clipSt.ended === true || clipArea.containsMouse
                                Icon { anchors.centerIn: parent; name: rv.clipOn && rv.clipSt.paused !== true && rv.clipSt.ended !== true ? "pause" : "play"; size: 26; color: "#ffffff" }
                            }
                            MouseArea { id: clipArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: rv.toggleClip() }
                            // where it is
                            Rectangle {
                                visible: rv.clipOn && (rv.clipSt.duration || 0) > 0
                                anchors { left: parent.left; bottom: parent.bottom }
                                height: 3
                                width: parent.width * Math.min(1, (rv.clipSt.pos || 0) / Math.max(0.1, rv.clipSt.duration || 1))
                                color: Theme.c.accent
                            }
                        }
                    }
                    Text {
                        id: frontText
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.RichText
                        text: Kana.linkify(rv.frontHtml, "" + Theme.c.text, rv.kanaDict)
                        onLinkHovered: function(l) { rv.kanaHover(frontText, l, frontHov.point.position) }
                        HoverHandler { id: frontHov; cursorShape: frontText.hoveredLink.indexOf("jp:") === 0 ? Qt.WhatsThisCursor : Qt.ArrowCursor }
                        color: Theme.c.text
                        linkColor: Theme.c.accentText
                        onLinkActivated: function(l) { rv.openLink(l) }
                        font.pixelSize: rv.narrow ? 22 : 26
                        font.family: Theme.type.family
                        lineHeight: 1.15
                        NumberAnimation on opacity { id: frontIn; from: 0.0; to: 1.0; duration: Theme.motion.normal; running: false }
                    }
                    Item {
                        id: answerPart
                        visible: rv.shown && rv.backHtml !== ""
                        width: parent.width
                        height: visible ? backText.implicitHeight + 30 : 0
                        Rectangle {
                            y: 14
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: parent.width; height: 1
                            color: Theme.c.divider
                        }
                        Text {
                            id: backText
                            y: 30
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            textFormat: Text.RichText
                            text: Kana.linkify(rv.backHtml, "" + Theme.c.text, rv.kanaDict)
                            onLinkHovered: function(l) { rv.kanaHover(backText, l, backHov.point.position) }
                            HoverHandler { id: backHov; cursorShape: backText.hoveredLink.indexOf("jp:") === 0 ? Qt.WhatsThisCursor : Qt.ArrowCursor }
                            color: Theme.c.text
                            linkColor: Theme.c.accentText
                            onLinkActivated: function(l) { rv.openLink(l) }
                            font.pixelSize: rv.narrow ? 16 : 18
                            font.family: Theme.type.family
                            lineHeight: 1.1
                        }
                        ParallelAnimation {
                            id: backIn
                            NumberAnimation { target: answerPart; property: "opacity"; from: 0; to: 1; duration: Theme.motion.slow; easing.type: Easing.OutCubic }
                            NumberAnimation { target: backText; property: "y"; from: 42; to: 30; duration: Theme.motion.slow; easing.type: Easing.OutCubic }
                        }
                    }
                    // the media of the card, tidy under it: sound again, the video clip
                    Item {
                        visible: rv.hasSound
                        width: parent.width; height: visible ? 56 : 0
                        Row {
                            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom }
                            spacing: 8
                            FlatButton {
                                visible: rv.hasSound
                                kind: "plain"; pill: true; glyph: "audio"; iconSize: 16; implicitHeight: 34
                                text: rv.narrow ? "" : rv.tr("Replay")
                                ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: rv.tr("Play the sound again") + " (R)"
                                onClicked: rv.playSide(true)
                            }
                        }
                    }
                }
            }
        }
    }

    // ================= the bottom bar =================
    component AnswerButton: Item {
        id: ab
        property string label: ""
        property string interval: ""
        property string hint: ""
        property color tone: "gray"
        property bool showHint: true
        // controller mode: the button that gives this grade, drawn in place of the key number
        property string padButton: ""
        signal activated()
        implicitHeight: 74
        Text {
            id: ivl
            anchors.horizontalCenter: parent.horizontalCenter
            text: ab.interval
            color: Theme.c.textDim
            font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family
        }
        Rectangle {
            id: bg
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 50
            radius: Theme.layout.buttons === "fluent" ? 4 : (Theme.layout.buttons === "capsule" ? height / 2 : Theme.shape.medium)
            color: Qt.rgba(ab.tone.r, ab.tone.g, ab.tone.b, ma.pressed ? 0.34 : (ma.containsMouse ? 0.26 : 0.16))
            border.width: Theme.layout.buttons === "fluent" ? 1 : 0
            border.color: Qt.rgba(ab.tone.r, ab.tone.g, ab.tone.b, 0.45)
            Behavior on color { ColorAnimation { duration: Theme.motion.fast } }
            Text {
                anchors.centerIn: parent
                text: ab.label
                color: ab.tone
                font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family
            }
            PadGlyph {
                visible: ab.padButton !== ""
                anchors { right: parent.right; rightMargin: Theme.layout.buttons === "capsule" ? 14 : 8; verticalCenter: parent.verticalCenter }
                button: ab.padButton
                ps: rv.padKind === "ps4" || rv.padKind === "ps5"
                size: 22
            }
            Text {
                visible: ab.showHint && ab.padButton === ""
                anchors { right: parent.right; rightMargin: Theme.layout.buttons === "capsule" ? 16 : 10; verticalCenter: parent.verticalCenter }
                text: ab.hint
                color: Theme.c.textDim
                font.pixelSize: 11; font.weight: Font.DemiBold; font.family: Theme.type.family
            }
            MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: ab.activated() }
        }
    }

    Item {
        id: bottomBar
        visible: rv.card !== null
        x: (rv.width - width) / 2
        width: Math.min(rv.cardW, 680)
        anchors { bottom: parent.bottom; bottomMargin: 20 }
        height: 74

        Column {
            visible: !rv.shown
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom }
            spacing: 6
            FlatButton {
                anchors.horizontalCenter: parent.horizontalCenter
                kind: "suggested"; pill: true
                text: rv.tr("Show answer")
                implicitWidth: Math.min(420, bottomBar.width)
                implicitHeight: 50
                onClicked: rv.reveal()
                PadGlyph {
                    visible: rv.padOn
                    anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
                    button: "A"
                    ps: rv.padKind === "ps4" || rv.padKind === "ps5"
                    size: 22
                }
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !rv.narrow && !rv.padOn
                text: rv.tr("Space")
                color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family
            }
        }
        Row {
            visible: rv.shown
            anchors.fill: parent
            spacing: rv.narrow ? 6 : 10
            Repeater {
                model: 4
                delegate: AnswerButton {
                    required property int index
                    width: (bottomBar.width - parent.spacing * 3) / 4
                    height: bottomBar.height
                    label: [rv.tr("Again"), rv.tr("Hard"), rv.tr("Good"), rv.tr("Easy")][index]
                    interval: rv.card && rv.card.labels ? rv.card.labels[index] : ""
                    hint: "" + (index + 1)
                    showHint: !rv.narrow
                    tone: [rv.cLearn, rv.cHard, rv.cDue, rv.cNew][index]
                    padButton: rv.padOn ? rv.gradeButtons[index] : ""
                    onActivated: rv.answer(index + 1)
                }
            }
        }
    }

    // ================= the end of the deck =================
    StudySurface {
        id: endCard
        visible: rv.session !== null && rv.card === null
        width: Math.min(480, rv.width - 32)
        height: endCol.implicitHeight + 56
        anchors.centerIn: parent
        raised: true
        Column {
            id: endCol
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 28 }
            spacing: 14
            // the macaw celebrates (or cheers you up after a hard deck)
            Mascot {
                anchors.horizontalCenter: parent.horizontalCenter
                size: 120
                mood: (rv.game.answered || 0) === 0 ? "happy" : ((rv.game.accuracy || 0) >= 60 ? "cheer" : "happy")
                animate: endCard.visible
            }
            Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: rv.tr("Congratulations!"); bold: true; size: 22 }
            Lbl {
                width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; dim: true; size: 14
                text: rv.session && (rv.session.learn || 0) > 0 ? rv.tr("Cards you are still learning come back in a few minutes.") : rv.tr("You have finished this deck for now.")
            }
            Lbl {
                visible: rv.session !== null && (rv.session.done || 0) > 0
                width: parent.width; horizontalAlignment: Text.AlignHCenter; dim: true; size: 13
                text: rv.tr("%1 cards reviewed").arg(rv.session ? rv.session.done : 0) + ((rv.game.streak || 0) > 0 ? "  ·  " + rv.tr("Streak: %1 days").arg(rv.game.streak) : "")
            }
            Row {
                visible: rv.session !== null && (rv.session.done || 0) > 0
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 10
                Repeater {
                    model: [
                        {v: "+" + (rv.game.xp || 0), t: rv.tr("XP earned"), c: rv.cXp},
                        {v: (rv.game.answered || 0) > 0 ? (rv.game.accuracy || 0) + "%" : "–", t: rv.tr("Accuracy"), c: rv.cDue},
                        {v: "" + (rv.game.bestCombo || 0), t: rv.tr("Best combo"), c: rv.cFlame},
                        {v: rv.mmss((rv.endedAt > 0 ? rv.endedAt : Date.now()) - rv.startedAt), t: rv.tr("Time"), c: Theme.c.text}
                    ]
                    delegate: Rectangle {
                        required property var modelData
                        width: Math.max(78, (endCol.width - 30) / 4); height: 64
                        radius: Theme.shape.medium
                        color: Theme.c.fill3
                        Column {
                            anchors.centerIn: parent
                            spacing: 2
                            Lbl { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.v; bold: true; size: 20; color: modelData.c }
                            Lbl { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.t; dim: true; size: 12 }
                        }
                    }
                }
            }
            Item { width: 1; height: 4 }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 8
                FlatButton { id: backDecks; kind: "suggested"; pill: true; glyph: "left"; text: rv.tr("Back to decks"); onClicked: Core.cmd({cmd: "study_close"}) }
                FlatButton {
                    id: unb
                    visible: rv.session !== null && (rv.session.buried || 0) > 0
                    kind: "plain"; text: rv.tr("Unbury") + " (" + (rv.session ? rv.session.buried : 0) + ")"
                    onClicked: Core.cmd({cmd: "study_unbury"})
                }
            }
        }
    }

    // ================= the macaw in the corner =================
    // It thinks while the question is shown, looks at the answer, is happy with Good / Easy and sad with Again, and
    // cheers at combo milestones. Its reactions never hold the keys back.
    Mascot {
        id: buddy
        // room beside the card on a wide window; a small one over the corner of the card otherwise
        readonly property bool roomy: (rv.width - rv.cardW) / 2 >= 128
        property string reaction: ""
        property string line: ""
        function react(e) {
            reaction = e === 1 ? "sad" : (e === 2 ? "idle" : "happy")
            // now and then a funny line (more often after Again, to cheer up); Hard gets none
            line = e === 1 && Math.random() < 0.6 ? rv.quip("review_wrong") : (e >= 3 && Math.random() < 0.3 ? rv.quip("review_right") : "")
            reactEnd.restart()
        }
        function milestone(n) {
            reaction = "cheer"
            line = rv.tr("%1 in a row!").arg(n)
            reactEnd.restart()
        }
        visible: rv.card !== null
        z: 4
        size: roomy ? 104 : 56
        x: roomy ? 18 : bottomBar.x
        y: roomy ? rv.height - size - 18 : bottomBar.y - size - 2
        mood: reaction !== "" ? reaction : (rv.shown ? "idle" : "think")
        say: roomy ? line : ""
        bubbleSide: "right"
        animate: visible && rv.pageVisible
        Timer { id: reactEnd; interval: buddy.line !== "" && buddy.reaction !== "cheer" ? 2600 : (buddy.reaction === "cheer" ? 1800 : 1100); onTriggered: { buddy.reaction = ""; buddy.line = "" } }
    }

    // ================= the card menu =================
    Popover {
        id: moreMenu
        readonly property var flags: ["#e01b24", "#ff7800", "#2ec27e", "#3584e4", "#e66eb4", "#14b8a6", "#9141ac"]
        readonly property var rows: [
            {t: "Suspend card", k: "suspend", key: "@"}, {t: "Suspend note", k: "suspend_note", key: ""},
            {t: "Bury card", k: "bury", key: "-"}, {t: "Bury note", k: "bury_note", key: ""},
            {t: "Mark note", k: "mark", key: "*"}, {t: "Reset to new", k: "reset", key: ""},
            {t: "Due today", k: "set_due", arg: 0, key: ""}, {t: "Due tomorrow", k: "set_due", arg: 1, key: ""}
        ]
        contentItem: Column {
            width: 280
            spacing: 2
            Repeater {
                model: moreMenu.rows.length
                delegate: Item {
                    required property int index
                    readonly property var row: moreMenu.rows[index]
                    width: 280
                    height: 36
                    Rectangle { anchors.fill: parent; radius: Theme.shape.small; color: mma.containsMouse ? Theme.c.hover : "transparent" }
                    Text { anchors { left: parent.left; leftMargin: 12; verticalCenter: parent.verticalCenter } text: rv.tr(parent.row.t); color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family }
                    Text { visible: parent.row.key !== ""; anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter } text: parent.row.key; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
                    MouseArea { id: mma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { Core.cmd({cmd: "study_card", what: parent.row.k, arg: parent.row.arg !== undefined ? parent.row.arg : 0}); moreMenu.close() } }
                }
            }
            Rectangle { width: 280; height: 1; color: Theme.c.divider }
            Item {
                width: 280; height: 40
                Text { anchors { left: parent.left; leftMargin: 12; verticalCenter: parent.verticalCenter } text: rv.tr("Flag"); color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family }
                Row {
                    anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                    spacing: 7
                    Repeater {
                        model: moreMenu.flags.length
                        delegate: Rectangle {
                            required property int index
                            width: 20; height: 20; radius: 10
                            color: moreMenu.flags[index]
                            border.width: rv.card && rv.card.flag === index + 1 ? 3 : 0
                            border.color: Theme.c.text
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { Core.cmd({cmd: "study_card", what: "flag", arg: rv.card && rv.card.flag === index + 1 ? 0 : index + 1}); moreMenu.close() } }
                        }
                    }
                }
            }
            Item {
                visible: rv.session !== null && (rv.session.buried || 0) > 0
                width: 280; height: 36
                Rectangle { anchors.fill: parent; radius: Theme.shape.small; color: uma.containsMouse ? Theme.c.hover : "transparent" }
                Text { anchors { left: parent.left; leftMargin: 12; verticalCenter: parent.verticalCenter } text: rv.tr("Unbury") + " (" + (rv.session ? rv.session.buried : 0) + ")"; color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family }
                MouseArea { id: uma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { Core.cmd({cmd: "study_unbury"}); moreMenu.close() } }
            }
        }
    }

    // ================= edit the note of the card =================
    Popup {
        id: editPop
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(620, (parent ? parent.width : 700) - 40)
        height: Math.min(implicitHeight, (parent ? parent.height : 700) - 60)
        padding: 22
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        Overlay.modal: Rectangle { color: Theme.c.scrim }
        background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
        // the fields are read when the popup opens, so a refresh of the state does not wipe what is typed
        property var names: []
        property var values: []
        property real nid: 0
        onAboutToShow: {
            const c = rv.card
            if (!c) return
            nid = c.nid
            values = (c.fields || []).slice()
            const ms = Core.study.models || []
            let n = []
            for (let i = 0; i < ms.length; ++i) if (ms[i].id === c.mid) n = ms[i].fields
            names = n.length > 0 ? n : values.map(function(v, i) { return rv.tr("Field") + " " + (i + 1) })
        }
        contentItem: Flickable {
            implicitHeight: ecol.implicitHeight
            contentWidth: width
            contentHeight: ecol.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Column {
                id: ecol
                width: parent.width
                spacing: 12
                Lbl { text: rv.tr("Edit this card"); bold: true; size: 18 }
                Repeater {
                    id: eds
                    model: editPop.values.length
                    delegate: Column {
                        required property int index
                        property alias value: ta.text
                        width: ecol.width
                        spacing: 6
                        Lbl { text: editPop.names[index] || ""; dim: true; bold: true; size: 13 }
                        TextArea {
                            id: ta
                            width: parent.width
                            height: Math.max(64, Math.min(160, implicitHeight))
                            text: editPop.values[index] || ""
                            wrapMode: TextArea.Wrap
                            selectByMouse: true
                            color: Theme.c.text
                            font.pixelSize: 15
                            font.family: Theme.type.family
                            leftPadding: 12; rightPadding: 12; topPadding: 10; bottomPadding: 10
                            background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: ta.activeFocus ? 2 : 1; border.color: ta.activeFocus ? Theme.c.accent : Theme.c.border }
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 8
                    FlatButton { kind: "plain"; text: rv.tr("Cancel"); onClicked: editPop.close() }
                    FlatButton {
                        kind: "suggested"; text: rv.tr("Save")
                        onClicked: {
                            const vals = []
                            for (let i = 0; i < eds.count; ++i) vals.push(eds.itemAt(i).value)
                            Core.cmd({cmd: "study_edit_note", nid: editPop.nid, fields: vals})
                            editPop.close()
                        }
                    }
                }
            }
        }
    }

    // ================= keys =================
    Shortcut { sequences: ["Space", "Return", "Enter"]; enabled: rv.keysOn && rv.card !== null; onActivated: rv.shown ? rv.answer(3) : rv.reveal() }
    Repeater {
        model: 4
        delegate: Item {
            required property int index
            Shortcut { sequence: "" + (index + 1); enabled: rv.keysOn && rv.shown; onActivated: rv.answer(index + 1) }
        }
    }
    Shortcut { sequence: "Ctrl+Z"; enabled: rv.keysOn; onActivated: Core.cmd({cmd: "study_undo"}) }
    Shortcut { sequences: ["R", "F5"]; enabled: rv.keysOn && rv.card !== null; onActivated: rv.playSide(true) }
    Shortcut { sequence: "E"; enabled: rv.keysOn && rv.card !== null; onActivated: editPop.open() }
    Shortcut { sequence: "@"; enabled: rv.keysOn && rv.card !== null; onActivated: Core.cmd({cmd: "study_card", what: "suspend"}) }
    Shortcut { sequence: "-"; enabled: rv.keysOn && rv.card !== null; onActivated: Core.cmd({cmd: "study_card", what: "bury"}) }
    Shortcut { sequence: "*"; enabled: rv.keysOn && rv.card !== null; onActivated: Core.cmd({cmd: "study_card", what: "mark"}) }
    Shortcut { sequence: "Escape"; enabled: rv.keysOn; onActivated: Core.cmd({cmd: "study_close"}) }
    // the box of the kana reader, beside the word under the mouse
    Rectangle {
        id: kanaTip
        z: 100
        visible: rv.kanaLink !== "" && rv.kanaFrom !== null && rv.kanaFrom.hoveredLink === rv.kanaLink
        readonly property real gap: 18
        width: Math.min(320, kanaTxt.implicitWidth + 24)
        height: kanaTxt.implicitHeight + 18
        x: Math.max(8, Math.min(rv.width - width - 8, rv.kanaAt.x - width / 2))
        y: rv.kanaAt.y + gap + height > rv.height - 8 ? rv.kanaAt.y - gap - height : rv.kanaAt.y + gap
        radius: Theme.shape.small
        color: Theme.c.popover
        border.width: 1
        border.color: Theme.c.divider
        Text {
            id: kanaTxt
            x: 12; y: 9
            width: Math.min(296, implicitWidth)
            wrapMode: Text.Wrap
            textFormat: Text.RichText
            text: rv.kanaLink !== "" ? Kana.tip(rv.kanaLink, rv.kanaDict) : ""
            color: Theme.c.text
            font.pixelSize: 15
            font.family: Theme.type.family
            lineHeight: 1.1
        }
    }
}
