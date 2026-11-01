import QtQuick
import QtQuick.Controls
import QtCore
import Reelhouse 1.0
import "ReaderThemes.js" as RT

// The reader of Text mode: one chapter as paragraphs and pictures. The position is kept as a permille of the chapter,
// the chapter counts as read when the end is reached. "Aa" changes how the text looks, the translate button writes the chapter in another language.
Rectangle {
    id: rp
    focus: visible
    readonly property var g: Core.prefs.general || ({})
    readonly property var r: (Core.manga.novels || ({})).reader || ({})
    readonly property var paras: r.paras || []
    readonly property var pt: Core.manga.pagetools || ({})
    property bool restored: false
    property bool finishedSent: false
    property int lastSent: -1
    property bool editing: false
    property bool showOriginal: false
    property int trId: 0
    property string trKey: ""
    function tr(s) { return Core.tr(Core.uiLang, s) }
    // one of the macaw's funny lines (src/mascot_lines.rs), random and never the last one
    function quip(m) { const r = Core.state("mascot:" + Core.uiLang + ":" + m); return r && r.text ? r.text : "" }
    function setting(key, value) {
        if (Theme.mobile && key === "text_pages") mobStore.textPages = value ? "1" : "0"
        Core.cmd({cmd: "set_setting", path: "general." + key, value: value})
    }

    // ---- study in the reader (docs/study-in-readers.md): the core works out, per chapter, the learned words to mix
    // into a text in a language the user knows and the fully known sentences for quiz cards in a text in the study
    // language; the switches of the preferences (study_*, off by default) decide what is shown ----
    readonly property var study: r.study || ({})
    // everything hangs on the main switch of Preferences > Study (off by default), then each kind has its own
    readonly property bool studyOn: g.study_in_player === true
    readonly property bool cuesOn: studyOn && g.study_mascot_cues !== false
    readonly property var mix: studyOn && g.study_wordmix !== false ? (study.mix || []) : []
    readonly property var quiz: studyOn && g.study_quiz_cards !== false ? (study.quiz || []) : []
    readonly property bool readingOn: g.study_wordmix_reading !== false
    property var quizClosed: ({})          // paragraph -> true: the card was answered or closed
    function quizAt(i) {
        for (let k = 0; k < quiz.length; ++k) if (quiz[k].para === i) return quiz[k]
        return null
    }
    function esc(t) { return String(t).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }
    // A paragraph with swapped words as rich text: the word of the study language on a soft highlight, a link that
    // carries the original (and the reading) for the tooltip.
    function mixHtml(pieces, indent) {
        const hl = Qt.rgba(Theme.c.accent.r, Theme.c.accent.g, Theme.c.accent.b, 0.22)
        const bg = "#" + [hl.r, hl.g, hl.b].map(function(v) { return ("0" + Math.round(v * 255).toString(16)).slice(-2) }).join("")
        let h = " ".repeat(indent)
        for (let i = 0; i < pieces.length; ++i) {
            const x = pieces[i]
            if (x.w === undefined) { h += esc(x.t || ""); continue }
            const tip = encodeURIComponent((x.o || "") + "\n" + (x.r || ""))
            h += "<a href=\"w:" + tip + "\" style=\"text-decoration:none\"><span style=\"background-color:" + bg + ";color:" + Theme.c.accentText + "\">" + esc(x.w) + "</span></a>"
            // the reading, small after the word (Japanese), when the preferences ask for it
            if (readingOn && x.r && x.r !== x.w) h += "<span style=\"font-size:small;color:" + Theme.c.accentText + "\">(" + esc(x.r) + ")</span>"
        }
        return "<span style=\"white-space:pre-wrap\">" + h + "</span>"
    }
    function tipOf(link) {
        if (link.indexOf("w:") !== 0) return ""
        const parts = decodeURIComponent(link.substring(2)).split("\n")
        return parts[0] + (parts[1] ? "  (" + parts[1] + ")" : "")
    }
    // the macaw peeks with its magnifier at a swapped word now and then (not at every hover)
    property double lastPeek: 0
    function peek() {
        if (!cuesOn || typeof Core.mascotCue !== "function" || Date.now() - lastPeek < 60000) return
        lastPeek = Date.now()
        Core.mascotCue({cue: "peek", corner: "bottom-right", ms: 2500})
    }
    function cheer() {
        if (cuesOn && typeof Core.mascotCue === "function") Core.mascotCue({cue: "cheer", corner: "bottom-right", ms: 5000, text: quip("chapter_done") || tr("Chapter finished!")})
    }

    // ---- the look: a theme of the "Themes & Settings" sheet (its night variant while the app is dark), or colours
    // chosen by hand when no theme is set ----
    readonly property bool night: g.text_theme_night !== false && Theme.scheme === "dark"
    readonly property var look: RT.textLook(g.text_theme || "", night)
    readonly property string bgName: g.text_bg || "app"
    readonly property var preset: ({
        "white": ["#ffffff", "#1c1c1c"], "sepia": ["#fbf0d9", "#3b2f1e"], "gray": ["#e4e4e4", "#222222"],
        "dark": ["#1f1f23", "#d8d8dc"], "black": ["#000000", "#c8c8c8"], "custom": [g.text_bg_color || "#fbf0d9", ""]
    })
    color: look ? look.bg : (preset[bgName] ? preset[bgName][0] : Theme.c.window)
    readonly property color fg: look ? look.fg : (g.text_fg ? g.text_fg : (preset[bgName] && preset[bgName][1] !== "" ? preset[bgName][1] : Theme.c.text))
    readonly property int size: g.novel_size || 19
    readonly property string family: g.text_font ? g.text_font : (look && look.family !== "" ? look.family : Theme.type.family)
    readonly property bool heavy: !!g.text_bold || (look ? look.heavy : false)
    // The "Themes & Settings" panel is docked on the left: the text moves right by its width instead of being covered.
    readonly property real sideInset: lookSheet.inset
    readonly property int colW: Math.min(g.text_width || 760, width - 32 - sideInset)
    readonly property int indentChars: Math.round((g.text_indent || 0) / Math.max(8, size))
    readonly property bool dimBar: look ? RT.isLight(look.bg) : (bgName === "white" || bgName === "sepia" || bgName === "gray")

    // ---- pages instead of scrolling: the view moves a screen at a time, cut between two lines ----
    // a phone reads in pages (a swipe or a tap on a side turns them) unless scrolling was chosen on this device
    Settings { id: mobStore; category: "mobileReader"; property string textPages: "" }
    readonly property bool paged: Theme.mobile ? (mobStore.textPages !== "" ? mobStore.textPages === "1" : true) : !!g.text_pages
    readonly property real linePx: size * (g.text_line || 170) / 100
    readonly property real pageStep: Math.max(100, Math.floor((flick.height - 56) / Math.max(8, linePx)) * linePx)
    readonly property int pageCount: Math.max(1, Math.ceil(Math.max(0, flick.contentHeight - flick.height) / pageStep) + 1)
    readonly property int pageNow: Math.min(pageCount, Math.round(flick.contentY / pageStep) + 1)
    // the controller's right stick (PadNavigator, `dt` seconds since the last frame): up / down scroll the text
    // (with pages: turn them), left / right make the text smaller / bigger
    property real padSizeWait: 0
    property real padTurnWait: 0
    function padStick(x, y, dt) {
        if (paged) {
            if (Math.abs(y) <= 0.5) padTurnWait = 0
            else if ((padTurnWait -= dt) <= 0) { turnPage(y < 0 ? 1 : -1); padTurnWait = 0.6 }
        } else if (y !== 0) {
            flick.contentY = Math.max(0, Math.min(Math.max(0, flick.contentHeight - flick.height), flick.contentY - y * 1200 * dt))
        }
        if (g.pad_zoom !== "stick" || Math.abs(x) <= 0.6) { padSizeWait = 0; return }
        if ((padSizeWait -= dt) <= 0) { padSize(x > 0 ? 1 : -1); padSizeWait = 0.18 }
    }
    // the text a size bigger or smaller (L2 / R2, or the right stick when Preferences > Controller says so)
    function padSize(step) { setting("novel_size", Math.max(12, Math.min(40, (g.novel_size || 19) + step))) }
    // a page forward or back, with the turn chosen in the sheet; at the end of the chapter the next one opens
    function turnPage(delta) {
        const maxY = Math.max(0, flick.contentHeight - flick.height)
        if (delta > 0 && flick.contentY >= maxY - 2) { if (r.hasNext) Core.cmd({cmd: "novel_next", delta: 1}); return }
        if (delta < 0 && flick.contentY <= 2) { if (r.hasPrev) Core.cmd({cmd: "novel_next", delta: -1}); return }
        const to = Math.max(0, Math.min(maxY, Math.round(flick.contentY / pageStep + delta) * pageStep))
        turner.turn(function() { flick.contentY = to }, delta > 0)
    }

    // ---- the translation of the chapter ----
    readonly property string trTarget: g.novel_translate === "app" ? Core.uiLang : (g.novel_translate || "")
    readonly property var trTexts: {
        const t = pt.translations && trId > 0 ? pt.translations[String(trId)] : null
        return t && t.texts ? t.texts : null
    }
    readonly property bool trBusy: trTarget !== "" && trId > 0 && !trTexts && !(pt.translations && pt.translations[String(trId)] && pt.translations[String(trId)].error)
    readonly property string trError: pt.translations && trId > 0 && pt.translations[String(trId)] && pt.translations[String(trId)].error ? pt.translations[String(trId)].error : ""
    property int nextId: 100000
    function askTranslation() {
        const key = chapterKey + "|" + trTarget
        if (trTarget === "" || paras.length === 0 || trKey === key) return
        trKey = key
        trId = nextId++
        Core.cmd({cmd: "translate_texts", kind: "novel", id: trId, source: "auto", target: trTarget, texts: paras.map(function(p) { return p.image ? "" : p.text })})
    }
    onTrTargetChanged: { trKey = ""; trId = 0; Qt.callLater(askTranslation) }
    onParasChanged: Qt.callLater(askTranslation)

    function atEnd() {
        return flick.contentHeight > flick.height && flick.contentY >= flick.contentHeight - flick.height - 4
    }
    function permille() {
        const span = Math.max(1, flick.contentHeight - flick.height)
        return Math.round(Math.max(0, Math.min(1, flick.contentY / span)) * 1000)
    }
    function restore() {
        if (rp.restored || paras.length === 0 || flick.contentHeight <= flick.height) return
        rp.restored = true
        flick.contentY = (rp.r.resume || 0) / 1000 * Math.max(0, flick.contentHeight - flick.height)
    }
    // When the column gets narrower or wider (the side panel, the window) the text flows again and gets taller or
    // shorter: the reader stays at the same part of the chapter.
    property int keepFrac: 0
    property bool reflowing: false
    onColWChanged: if (restored) { reflowing = true; Qt.callLater(rp.restoreFrac) }
    function restoreFrac() {
        reflowing = false
        const span = Math.max(0, flick.contentHeight - flick.height)
        let y = keepFrac / 1000 * span
        if (paged) y = Math.round(y / pageStep) * pageStep
        flick.contentY = Math.max(0, Math.min(span, y))
    }
    function scrollBy(px) { flick.contentY = Math.max(0, Math.min(Math.max(0, flick.contentHeight - flick.height), flick.contentY + px)) }

    // In the editor, Escape only leaves the editor (unsaved changes are dropped, like Cancel) and the reading keys are off.
    Keys.onEscapePressed: function(e) { padBack(); e.accepted = true }
    // ○ / B of a controller does the same (Main.qml): the sheet, then the editor, then the reader
    function padBack() {
        if (lookSheet.opened && !editing) lookSheet.close()
        else if (editing) editing = false
        else Core.cmd({cmd: "novel_close_reader"})
    }
    // with pages, the arrows turn the pages (the chapter changes at its ends); scrolling, they change the chapter
    Keys.onLeftPressed: function(e) { if (!editing) { if (paged) turnPage(-1); else if (r.hasPrev) Core.cmd({cmd: "novel_next", delta: -1}) } e.accepted = !editing }
    Keys.onRightPressed: function(e) { if (!editing) { if (paged) turnPage(1); else if (r.hasNext) Core.cmd({cmd: "novel_next", delta: 1}) } e.accepted = !editing }
    Keys.onPressed: function(e) {
        if (editing) return
        if (paged && (e.key === Qt.Key_Down || e.key === Qt.Key_PageDown || e.key === Qt.Key_Space)) turnPage(1)
        else if (paged && (e.key === Qt.Key_Up || e.key === Qt.Key_PageUp)) turnPage(-1)
        else if (e.key === Qt.Key_Down) scrollBy(80)
        else if (e.key === Qt.Key_Up) scrollBy(-80)
        else if (e.key === Qt.Key_PageDown || e.key === Qt.Key_Space) scrollBy(flick.height * 0.9)
        else if (e.key === Qt.Key_PageUp) scrollBy(-flick.height * 0.9)
        else return
        e.accepted = true
    }
    onVisibleChanged: { if (visible) forceActiveFocus(); attachSide() }

    // ---- the buttons in the left sidebar ----
    // ReaderSidebar draws the reader's buttons itself (under the chapter's name) and calls the side* functions below;
    // the reader only tells the window it is the one open (win.activeReader). The buttons in the header bar (acts)
    // are only for a window without a sidebar.
    readonly property bool sideOn: { const w = rp.Window.window; return !!w && !!w.readerSideSlot }
    function attachSide() {
        const w = rp.Window.window
        if (!w || w.activeReader === undefined) return
        if (rp.visible) { w.activeReader = rp; console.log("[reader] buttons in the sidebar for the", rp.readerKind, "reader (sidebar shown:", rp.sideOn + ")") }
        else if (w.activeReader === rp) w.activeReader = null
    }
    Component.onCompleted: Qt.callLater(attachSide)
    Component.onDestruction: { const w = rp.Window.window; if (w && w.activeReader === rp) w.activeReader = null }
    readonly property string readerKind: "novel"
    function sideEdit(btn) { editing = true }
    function sideTranslate(btn) { trPop.openRight(btn) }
    function sideLook(btn) { lookSheet.openFrom(btn) }
    // back from the editor: the keys return to the reader
    onEditingChanged: if (!editing) forceActiveFocus()
    // the state is rebuilt on every change, so the chapter itself decides when a new one starts
    readonly property string chapterKey: (r.seriesId || "") + "|" + (r.chapterId || "")
    onChapterKeyChanged: { restored = false; finishedSent = false; lastSent = -1; trKey = ""; trId = 0; quizClosed = ({}); Qt.callLater(restore) }

    // the grain of the Paper theme
    PaperGrain { anchors.fill: parent; visible: !!(rp.look && rp.look.grain); speck: rp.fg }

    Flickable {
        id: flick
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom
        anchors.leftMargin: rp.sideInset
        contentWidth: width
        // with pages, the last page is filled out so it can sit at the top like the others
        contentHeight: rp.paged ? Math.ceil((body.height + 120) / Math.max(1, rp.pageStep)) * rp.pageStep + Math.max(0, height - rp.pageStep) : body.height + 120
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: !rp.paged
        ScrollBar.vertical: ScrollBar { visible: !rp.paged }

        onContentYChanged: {
            if (!rp.reflowing) rp.keepFrac = rp.permille()
            if (rp.paras.length === 0) return
            const p = rp.permille()
            if (Math.abs(p - rp.lastSent) >= 5 || (p === 0 && rp.lastSent !== 0)) {
                rp.lastSent = p
                Core.cmd({cmd: "novel_progress", permille: p})
            }
            if (rp.atEnd() && !rp.finishedSent) {
                rp.finishedSent = true
                Core.cmd({cmd: "novel_finished"})
                rp.cheer()
            }
        }
        onContentHeightChanged: rp.restore()
        // a tap on the page gives the keys back to the reader (after a header button or a popover)
        TapHandler {
            onTapped: function(ev) {
                rp.forceActiveFocus()
                // with pages, the sides of the screen turn them
                if (!rp.paged) return
                const x = ev.position.x
                if (x < flick.width * 0.3) rp.turnPage(-1)
                else if (x > flick.width * 0.7) rp.turnPage(1)
            }
        }
        WheelHandler {
            enabled: rp.paged
            onWheel: function(e) { if (!turner.busy && e.angleDelta.y !== 0) rp.turnPage(e.angleDelta.y < 0 ? 1 : -1) }
        }
        // a phone: a swipe sideways (or up / down) turns the page
        DragHandler {
            enabled: rp.paged && Theme.touch
            target: null
            onActiveChanged: {
                if (active || turner.busy) return
                const dx = translation.x, dy = translation.y
                if (Math.abs(dx) < 40 && Math.abs(dy) < 40) return
                if (Math.abs(dx) >= Math.abs(dy)) rp.turnPage(dx < 0 ? 1 : -1); else rp.turnPage(dy < 0 ? 1 : -1)
            }
        }

        Column {
            id: body
            x: (flick.width - rp.colW) / 2
            // no bar above the reader (its buttons are in the sidebar): the window buttons float at the top right,
            // so a column that reaches under them starts a little lower
            y: rp.inSide && x < 110 ? 50 : 28
            width: rp.colW
            spacing: g.text_para_gap !== undefined ? g.text_para_gap : 14
            Repeater {
                model: rp.paras.length
                delegate: Item {
                    id: paraItem
                    required property int index
                    readonly property var p: rp.paras[index] || ({})
                    readonly property bool translatedNow: !rp.showOriginal && !!rp.trTexts && !!rp.trTexts[index]
                    readonly property string shown: translatedNow ? rp.trTexts[index] : (p.text || "")
                    // the learned words mixed in (only over the original text, never over a translation)
                    readonly property var pieces: !translatedNow && !p.heading && !p.image ? (rp.mix[index] || []) : []
                    readonly property var quizHere: !p.image && !rp.quizClosed[index] ? rp.quizAt(index) : null
                    width: body.width
                    height: (p.image ? pic.height : txt.implicitHeight) + (card.visible ? card.height + 14 : 0)
                    PosterImage {
                        id: pic
                        visible: !!p.image
                        width: parent.width
                        height: status === Image.Ready && implicitWidth > 0 ? width * implicitHeight / implicitWidth : (visible ? width * 0.6 : 0)
                        url: p.image || ""
                        fillMode: Image.PreserveAspectFit
                    }
                    Text {
                        id: txt
                        visible: !p.image
                        width: parent.width
                        text: pieces.length > 0 ? rp.mixHtml(pieces, p.heading ? 0 : Math.max(0, rp.indentChars)) : (p.heading || rp.indentChars <= 0 ? "" : " ".repeat(rp.indentChars)) + shown
                        textFormat: pieces.length > 0 ? Text.RichText : Text.PlainText
                        // the original word of a swapped one, on hover
                        ToolTip.visible: hoveredLink !== ""
                        ToolTip.text: rp.tipOf(hoveredLink)
                        onLinkHovered: function(link) { if (link !== "") rp.peek() }
                        wrapMode: Text.Wrap
                        color: p.heading && rp.g.text_head_color ? rp.g.text_head_color : rp.fg
                        font.pixelSize: p.heading ? Math.round(rp.size * (rp.g.text_head_size || 135) / 100) : rp.size
                        font.bold: !!p.heading || rp.heavy
                        font.family: rp.family
                        font.wordSpacing: rp.g.text_word || 0
                        font.letterSpacing: (rp.g.text_letter || 0) / 10
                        lineHeight: (rp.g.text_line || 170) / 100
                        horizontalAlignment: p.heading && rp.g.text_head_center ? Text.AlignHCenter : ({"center": Text.AlignHCenter, "justify": Text.AlignJustify, "end": Text.AlignRight}[rp.g.text_align] || Text.AlignLeft)
                    }
                    // a quiz card after the paragraph that holds a fully known sentence
                    ReaderQuizCard {
                        id: card
                        visible: paraItem.quizHere !== null
                        y: (p.image ? pic.height : txt.implicitHeight) + 14
                        width: Math.min(parent.width, 520)
                        x: (parent.width - width) / 2
                        text: paraItem.quizHere ? paraItem.quizHere.text : ""
                        lang: rp.study.lang || ""
                        ink: rp.fg
                        paper: Qt.tint(rp.color, Qt.rgba(rp.fg.r, rp.fg.g, rp.fg.b, 0.06))
                        // on screen: inside the part of the chapter being shown
                        shown: visible && body.y + paraItem.y + y < flick.contentY + flick.height && body.y + paraItem.y + y + height > flick.contentY
                        onClosed: rp.quizClosed = Object.assign({}, rp.quizClosed, {[paraItem.index]: true})
                        onDone: function(ok) { closeLater.start() }
                        Timer { id: closeLater; interval: 2500; onTriggered: card.closed() }
                    }
                }
            }
        }
    }

    // the page turn (pages only): a picture of the old page goes away over the new one
    PageTurn {
        id: turner
        anchors.fill: flick
        z: 0.5
        target: flick
        backColor: rp.color
        style: rp.g.text_turn || "slide"
    }
    // which page of the chapter, at the bottom (pages only): the same bar as the Images reader; a click or a drag on
    // it goes to a page
    ReaderProgress {
        visible: rp.paged && rp.paras.length > 0
        anchors { bottom: parent.bottom; bottomMargin: 6 }
        x: flick.x + body.x + (body.width - width) / 2          // under the column of text
        z: 2
        width: Math.max(120, Math.min(rp.colW, 640))
        current: rp.pageNow
        count: rp.pageCount
        ink: rp.fg
        onSeek: function(page) {
            const to = Math.max(0, Math.min(Math.max(0, flick.contentHeight - flick.height), (page - 1) * rp.pageStep))
            turner.turn(function() { flick.contentY = to }, page > rp.pageNow)
            rp.forceActiveFocus()
        }
    }

    // the red light: a tint over the page, for the evening
    Rectangle { anchors.fill: parent; z: 1; color: "#ff6a00"; opacity: Math.min(0.6, (g.red_light || 0) / 100 * 0.6); enabled: false }

    // the arrows that move the page
    Column {
        anchors.right: parent.right; anchors.rightMargin: 16; anchors.bottom: parent.bottom; anchors.bottomMargin: 20
        z: 3
        spacing: 10
        Repeater {
            model: [{glyph: "up", step: -1}, {glyph: "down", step: 1}]
            delegate: ModeButton {
                required property var modelData
                glyph: modelData.glyph
                onClicked: if (rp.paged) rp.turnPage(modelData.step); else rp.scrollBy(modelData.step * flick.height * 0.85)
            }
        }
    }

    // ================= actions =================
    // While reading they sit in the app's left sidebar (ReaderSidebar, reached through the window's readerSideSlot), one
    // per line with its name, so the page keeps all of its height for the text. Without a sidebar (a narrow Android
    // window) they stay in the header bar, grouped on the right. The keys (arrows, Escape) work either way.
    property Item slot: null
    property Item sideSlot: null            // set by ModeBrowser from Main's readerSideSlot
    readonly property bool inSide: false   // the sidebar draws its own buttons now (ReaderSidebar)
    readonly property bool sideLabels: inSide && sideSlot.width >= 150
    component ActBtn: FlatButton {
        property string label: ""
        focusPolicy: Qt.NoFocus   // the arrow keys must keep turning the pages
        width: rp.inSide ? rp.sideSlot.width : implicitWidth
        implicitWidth: rp.sideLabels ? contentItem.implicitWidth + 36 : 40
        implicitHeight: rp.inSide ? 36 : 40
        alignStart: rp.sideLabels
        text: rp.sideLabels ? label : ""
        ToolTip.visible: hovered && !rp.sideLabels
        ToolTip.text: label
    }
        Grid {
            id: acts
            parent: rp.slot ? rp.slot : rp
            width: implicitWidth
            visible: rp.visible && !rp.sideOn
            columns: 99
            rowSpacing: 2
            columnSpacing: 2
            ActBtn { kind: "flat"; glyph: "edit"; label: rp.tr("Edit the text of this chapter"); onClicked: rp.editing = true }
            ActBtn {
                id: trBtn
                kind: rp.trTarget !== "" ? "suggested" : "flat"
                glyph: "translate"
                label: rp.tr("Translate the chapter")
                ToolTip.visible: hovered && !trPop.visible && !rp.sideLabels
                onClicked: if (rp.inSide) trPop.openRight(trBtn); else trPop.openBelow(trBtn, true)
            }
            FlatButton {
                id: aaBtn
                focusPolicy: Qt.NoFocus
                kind: lookSheet.opened ? "plain" : "flat"
                width: rp.inSide ? rp.sideSlot.width : implicitWidth
                implicitWidth: aaRow.implicitWidth + 20
                implicitHeight: rp.inSide ? 36 : 40
                ToolTip.visible: hovered && !lookSheet.visible && !rp.sideLabels
                ToolTip.text: rp.tr("How the text looks")
                contentItem: Item {
                    implicitWidth: aaRow.implicitWidth
                    Row {
                        id: aaRow
                        spacing: rp.sideLabels ? 8 : 4
                        anchors.verticalCenter: parent.verticalCenter
                        x: rp.sideLabels ? 10 : (parent.width - width) / 2
                        Lbl { text: "Aa"; bold: true; size: 16; anchors.verticalCenter: parent.verticalCenter }
                        Icon { visible: !rp.inSide; name: "down"; size: 16; anchors.verticalCenter: parent.verticalCenter }
                        Text {
                            visible: rp.sideLabels
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, aaBtn.width - 60); elide: Text.ElideRight
                            text: rp.tr("How the text looks")
                            color: Theme.c.text
                            font.pixelSize: 15; font.weight: Theme.type.buttonWeight !== undefined ? Theme.type.buttonWeight : Font.Bold; font.family: Theme.type.family
                        }
                    }
                }
                onClicked: lookSheet.openFrom(aaBtn)
            }
            ActBtn { kind: "flat"; glyph: "prev"; enabled: !!rp.r.hasPrev; label: rp.tr("Previous chapter (Left)"); onClicked: Core.cmd({cmd: "novel_next", delta: -1}) }
            ActBtn { kind: "flat"; glyph: "next"; enabled: !!rp.r.hasNext; label: rp.tr("Next chapter (Right)"); onClicked: Core.cmd({cmd: "novel_next", delta: 1}) }
        }

    // ---- translate ----
    Popover {
        id: trPop
        contentItem: Column {
            width: 350
            Lbl { x: 14; topPadding: 6; text: rp.tr("Translate the chapter"); bold: true; size: 15 }
            Lbl { x: 14; width: parent.width - 28; text: rp.tr("Writes the chapter in another language; the original stays one switch away."); dim: true; size: 13; wrapMode: Text.Wrap; bottomPadding: 6 }
            PrefCombo {
                title: rp.tr("Translate into"); first: false
                readonly property var codes: ["", "app", "en", "pt", "es", "fr", "de", "it", "nl", "sv", "pl", "ru", "tr", "ar", "hi", "id", "vi", "th", "ja", "ko", "zh"]
                options: [rp.tr("Off"), rp.tr("System language"), "English", "Português", "Español", "Français", "Deutsch", "Italiano", "Nederlands", "Svenska", "Polski", "Русский", "Türkçe", "العربية", "हिन्दी", "Bahasa Indonesia", "Tiếng Việt", "ไทย", "日本語", "한국어", "中文"]
                current: Math.max(0, codes.indexOf(rp.g.novel_translate || ""))
                onPicked: function(i) { rp.setting("novel_translate", codes[i]) }
            }
            PrefSwitch { title: rp.tr("Show the original"); first: false; checked: rp.showOriginal; onToggled: function(on) { rp.showOriginal = on } }
            ProgressRow {
                width: parent.width
                visible: rp.trTarget !== ""
                label: rp.trBusy ? rp.tr("Translating the chapter") : rp.tr("Translation")
                readonly property var pg: rp.pt.trProgress && rp.trId > 0 ? rp.pt.trProgress[String(rp.trId)] : null
                total: rp.trBusy ? (pg ? pg[1] : 0) : 1
                done: rp.trBusy ? (pg ? pg[0] : 0) : (rp.trError !== "" ? 0 : 1)
                busy: rp.trBusy
                error: rp.trError
                detail: rp.trBusy ? rp.tr("Several parts at the same time; what is already translated is kept for next time.") : (rp.trError === "" ? rp.tr("Done") : "")
            }
            FlatButton { visible: rp.trError !== ""; x: 14; kind: "plain"; implicitHeight: 30; text: rp.tr("Try again"); onClicked: { rp.trKey = ""; rp.askTranslation() } }
        }
    }

    // ---- how the text looks: the "Themes & Settings" sheet ----
    TextLookSheet { id: lookSheet; rd: rp }

    // the editor of the text (books the person wrote or imported; a site's chapter can be edited too)
    Rectangle {
        id: editor
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom
        visible: rp.editing
        color: Theme.c.window
        z: 5
        onVisibleChanged: if (visible) { editArea.text = rp.r.markup || ""; editArea.forceActiveFocus() }
        Column {
            anchors.fill: parent
            anchors.margins: 16
            spacing: 10
            Lbl { width: parent.width; text: rp.tr("Edit the text of this chapter, then save."); dim: true; wrapMode: Text.Wrap }
            ScrollView {
                width: parent.width
                height: parent.height - 110
                TextArea {
                    id: editArea
                    wrapMode: TextArea.Wrap
                    color: Theme.c.text
                    selectionColor: Theme.c.accent
                    selectedTextColor: Theme.c.accentFg
                    selectByMouse: true
                    font.pixelSize: 16
                    font.family: Theme.type.family
                    // the theme's text field: its view colour, a hair line, the accent while typing
                    background: Rectangle {
                        radius: Theme.shape.control || 8
                        color: Theme.c.view
                        border.width: editArea.activeFocus ? 2 : 1
                        border.color: editArea.activeFocus ? Theme.c.accent : Theme.c.border
                    }
                }
            }
            Row {
                spacing: 8
                FlatButton { kind: "suggested"; text: rp.tr("Save"); onClicked: { Core.cmd({cmd: "novel_save_edit", text: editArea.text}); rp.editing = false } }
                FlatButton { kind: "flat"; text: rp.tr("Cancel"); onClicked: rp.editing = false }
                FlatButton { visible: !!rp.r.edited; kind: "plain"; text: rp.tr("Restore original"); onClicked: { Core.cmd({cmd: "novel_reset_edit"}); rp.editing = false } }
            }
        }
    }

    // loading and errors
    Column {
        anchors.centerIn: parent
        spacing: 10
        visible: rp.paras.length === 0
        BusyIndicator { visible: !rp.r.error; running: visible; anchors.horizontalCenter: parent.horizontalCenter }
        Lbl {
            anchors.horizontalCenter: parent.horizontalCenter
            text: rp.r.error ? rp.tr(rp.r.error) : rp.tr("Loading text…")
            color: rp.r.error ? Theme.c.error : rp.fg
            wrapMode: Text.Wrap
            width: Math.min(560, rp.width - 40)
            horizontalAlignment: Text.AlignHCenter
            size: 16
        }
    }
}
