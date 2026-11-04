import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtCore
import Reelhouse 1.0
import "ReaderThemes.js" as RT

// The reader of Images mode: pages one at a time (right to left, left to right, vertical) or a long strip (webtoon),
// zoom and fit, speech-bubble text (OCR), translation and AI upscaling, all chosen from the bar.
Rectangle {
    id: rp
    focus: visible
    readonly property var g: Core.prefs.general || ({})
    readonly property var r: Core.manga.reader || ({})
    readonly property var pages: r.pages || []
    readonly property var pt: Core.manga.pagetools || ({})
    // A phone reads in portrait: the long strip (webtoon) unless another way was chosen on this device (kept on the
    // device, QSettings, so the desktop's choice of pages right to left stays what it is).
    Settings { id: mobStore; category: "mobileReader"; property string mangaMode: "" }
    readonly property string mode: Theme.mobile ? (mobStore.mangaMode !== "" ? mobStore.mangaMode : "webtoon") : (g.manga_mode || "rtl")
    readonly property bool strip: mode === "webtoon"
    readonly property bool rtl: mode === "rtl"
    readonly property bool vertical: mode === "vertical"
    readonly property string fitMode: g.reader_fit || "screen"
    readonly property real zoomF: (g.reader_zoom || 100) / 100
    // the colour around the pages: a theme of the "Themes & Settings" sheet ("theme" follows the app)
    readonly property string bgName: g.reader_bg || "dark"
    readonly property var bgLook: RT.pages[bgName] || RT.pages.dark
    color: bgLook.bg !== "" ? bgLook.bg : Theme.c.window
    readonly property bool light: RT.isLight(String(color))
    // two pages side by side (an open book): from the second page on, both upright; the cover and wide pages stay alone
    readonly property bool spreadWanted: !!g.reader_spread && !strip && !vertical
    readonly property bool spreadOn: spreadWanted && curPage > 0 && curPage + 1 < pages.length && page.ratio >= 1 && page2.ratio >= 1
    readonly property bool trOn: !!g.reader_translate
    readonly property bool ocrOn: !!g.reader_ocr || trOn || translateAll
    readonly property bool upOn: !!g.reader_upscale
    property bool showOriginal: false
    property bool barsOn: true            // the middle of the page shows or hides the bars
    property bool translateAll: false
    property bool upAll: false
    // ---- blocks of text chosen on the pages (click), the sheet that translates them, copy ----
    property var sel: ({})

    // ---- hand corrections (translation file of the chapter, .md): text boxes and picture edits ----
    property string editMode: ""          // "" reading, "text" boxes, "paint" picture tools
    property var active: null             // the bubble shown in the side panel
    // card mode: the part of a page chosen for a study card ({page, url, x, y, w, h}, 0..1 of the original page),
    // the text and translation found inside it, and the deck used last
    property var cardSel: null
    property var cardAuto: null
    property int lastDeck: 0
    function setCardSel(sel) {
        cardSel = sel
        // the text of the page is needed for the card: read it when it was not read yet
        if (sel && !(pt.text && pt.text[sel.url]) && !reqPages[sel.url + "|o"]) {
            reqPages = Object.assign({}, reqPages, {[sel.url + "|o"]: true})
            Core.readPage(sel.url)
        }
    }
    property string paintTool: "brush"
    property string paintColor: "#ffffff"
    property int paintSize: 18
    property real paintAlpha: 1.0
    property int fillTol: 40
    readonly property var db: Core.manga.pagedb || ({})
    onEditModeChanged: { if (editMode === "paint" || editMode === "card") active = null; if (editMode !== "card") { cardSel = null; cardAuto = null } }
    function pageDb(n) {
        const ps = db.pages || []
        for (let i = 0; i < ps.length; ++i) if (ps[i].page === n) return ps[i]
        return null
    }
    function activate(it) { active = it; if (it) barsOn = true }
    function focusEditor() { Qt.callLater(function() { bubblePanel.focusEditor() }) }
    function saveBubble(page, it, patch) {
        const v = Object.assign({}, it, patch)
        Core.cmd({cmd: "pagedb_bubble", page: page, index: it.dbIndex, ocr: it.ocr, x: v.x, y: v.y, w: v.w, h: v.h,
                  original: v.original || "", html: v.html || "", reviewed: v.reviewed === true,
                  font: v.font || "", size: v.size || 100, color: v.color || "", highlight: v.highlight || ""})
        // the first save of an OCR block gives it a place in the file: the panel follows it there
        if (active && active.page === page && active.dbIndex === it.dbIndex && active.ocr === it.ocr) {
            const n = Object.assign({}, active, patch)
            if (n.dbIndex < 0) { const p = pageDb(page); n.dbIndex = p && p.bubbles ? p.bubbles.length : 0; const pb = p && p.bubbles ? p.bubbles : []; for (let i = 0; i < pb.length; ++i) if (pb[i].ocr === it.ocr && it.ocr >= 0) n.dbIndex = i }
            active = n
        }
    }
    function addBubble(page, url, x, y, w, h) {
        const p = pageDb(page)
        const it = {dbIndex: -1, ocr: -1, x: x, y: y, w: w, h: h, original: "", html: "", reviewed: false, mt: ""}
        Core.cmd({cmd: "pagedb_bubble", page: page, index: -1, ocr: -1, x: x, y: y, w: w, h: h, original: "", html: "", reviewed: false})
        active = Object.assign({page: page, url: url}, it, {dbIndex: p && p.bubbles ? p.bubbles.length : 0})
        focusEditor()
    }
    function removeBubble(page, it) {
        if (it.dbIndex >= 0) Core.cmd({cmd: "pagedb_remove_bubble", page: page, index: it.dbIndex})
        else if (it.ocr >= 0) {
            // an OCR block never saved: saved once, then removed (it stays hidden)
            Core.cmd({cmd: "pagedb_bubble", page: page, index: -1, ocr: it.ocr, x: it.x, y: it.y, w: it.w, h: it.h, original: it.original || "", html: "", reviewed: false})
            const p = pageDb(page)
            Core.cmd({cmd: "pagedb_remove_bubble", page: page, index: p && p.bubbles ? p.bubbles.length : 0})
        }
        if (active && active.page === page && active.dbIndex === it.dbIndex && active.ocr === it.ocr) active = null
    }
    // picture edits: the page's operation list, kept in the .md
    property var opsUndo: []
    function addOp(page, url, opsText, op) {
        let list = []
        try { list = JSON.parse(opsText || "[]") } catch (e) { list = [] }
        list.push(op)
        const t = JSON.stringify(list)
        Core.setEditOps(url, t)
        Core.cmd({cmd: "pagedb_ops", page: page, ops: t})
        opsUndo = opsUndo.concat([{page: page, url: url}])
    }
    function undoOp() {
        if (opsUndo.length === 0) return
        const last = opsUndo[opsUndo.length - 1]
        opsUndo = opsUndo.slice(0, opsUndo.length - 1)
        const p = pageDb(last.page)
        let list = []
        try { list = JSON.parse(p && p.ops ? p.ops : "[]") } catch (e) { list = [] }
        list.pop()
        const t = JSON.stringify(list)
        Core.setEditOps(last.url, t)
        Core.cmd({cmd: "pagedb_ops", page: last.page, ops: t})
    }
    property bool sheetOn: false
    property int sheetId: 0
    property string sheetText: ""
    function toggleSel(key, text, only) {
        const n = Object.assign({}, sel)
        if (only) n[key] = text
        else if (n[key] !== undefined) delete n[key]
        else n[key] = text
        sel = n
    }
    function clearSel() { sel = ({}) }
    function selectionText() {
        const keys = Object.keys(sel)
        return keys.map(function(k) { return sel[k] }).join("\n")
    }
    function copySelection() { clip.text = selectionText(); clip.selectAll(); clip.copy(); clip.text = "" }
    function openSheet() {
        sheetText = selectionText()
        if (sheetText === "") return
        sheetOn = true
        sheetId = nextTrId++
        Core.cmd({cmd: "translate_texts", kind: "manga", id: sheetId, texts: sheetText.split("\n"), target: targetLang() || Core.uiLang})
    }
    readonly property var sheetRes: pt.translations && sheetId > 0 ? pt.translations[String(sheetId)] : null
    TextEdit { id: clip; visible: false }
    property int curPage: 0
    property int lastSent: -1
    property var reqPages: ({})
    property var trIds: ({})
    property int nextTrId: 1
    function tr(s) { return Core.tr(Core.uiLang, s) }
    // one of the macaw's funny lines (src/mascot_lines.rs), random and never the last one
    function quip(m) { const r = Core.state("mascot:" + Core.uiLang + ":" + m); return r && r.text ? r.text : "" }
    function setting(key, value) {
        if (Theme.mobile && key === "manga_mode") mobStore.mangaMode = value
        Core.cmd({cmd: "set_setting", path: "general." + key, value: value})
    }
    function targetLang() { return g.reader_translate === "app" ? Core.uiLang : g.reader_translate }
    function zoomStep() { const z = g.reader_zoom || 100; return z <= 30 ? 5 : 10 }
    function zoom(step) { setting("reader_zoom", Math.max(10, Math.min(400, Math.round((g.reader_zoom || 100) + step)))) }

    // ---- study in the reader (docs/study-in-readers.md): a speech bubble of the study language whose words are all
    // known offers a quiz card; the macaw cheers at the end of a chapter (the study_* switches, off by default) ----
    readonly property bool quizOn: g.study_in_player === true && g.study_quiz_cards !== false
    readonly property bool cuesOn: g.study_in_player === true && g.study_mascot_cues !== false
    property var quizBubble: null
    function openQuiz(text, meaning) {
        if (String(text).trim() === "") return
        // a new card each time (its answer state starts again)
        quizBubble = null
        Qt.callLater(function() { rp.quizBubble = {text: String(text), meaning: String(meaning || "")} })
    }
    property string cheeredFor: ""
    function finished() {
        Core.cmd({cmd: "manga_finished"})
        if (cheeredFor === pagesKey) return
        cheeredFor = pagesKey
        if (cuesOn && typeof Core.mascotCue === "function") Core.mascotCue({cue: "cheer", corner: "bottom-right", ms: 5000, text: quip("chapter_done") || tr("Chapter finished!")})
    }

    // ---- what is asked of the core: once for every page ----
    function want(url) {
        if (!url) return
        if (ocrOn && !reqPages[url + "|o"]) {
            reqPages = Object.assign({}, reqPages, {[url + "|o"]: true})
            Core.readPage(url)
        }
        if (upOn && pt.engine && !reqPages[url + "|u"]) {
            reqPages = Object.assign({}, reqPages, {[url + "|u"]: true})
            Core.cmd({cmd: "upscale_page", url: url})
        }
    }
    function wantVisible() {
        if (!ocrOn && !upOn) return
        if (translateAll || upAll) { for (let i = 0; i < pages.length; ++i) want(pages[i]); return }
        if (strip) {
            for (let i = 0; i < list.count; ++i) {
                const it = list.itemAtIndex(i)
                if (it && it.y < list.contentY + list.height * 2 && it.y + it.height > list.contentY - list.height) want(pages[i])
            }
        } else {
            for (let d = -1; d <= 2; ++d) want(pages[curPage + d])
        }
    }
    function maybeTranslate(url) {
        if (!trOn || !url || trIds[url] !== undefined) return
        const t = pt.text && pt.text[url]
        if (!t || !t.blocks || t.blocks.length === 0) return
        const id = nextTrId++
        trIds = Object.assign({}, trIds, {[url]: id})
        Core.cmd({cmd: "translate_texts", kind: "manga", id: id, texts: t.blocks.map(function(b) { return b.text }), target: targetLang()})
    }
    // a page is "done" when its answer came, good or bad; failures are counted apart so the chapter does not hang
    function readCount() {
        let n = 0
        for (let i = 0; i < pages.length; ++i) if (pt.text && pt.text[pages[i]]) n++
        return n
    }
    function readFailed() {
        let n = 0
        for (let i = 0; i < pages.length; ++i) { const t = pt.text && pt.text[pages[i]]; if (t && t.error) n++ }
        return n
    }
    function firstReadError() {
        for (let i = 0; i < pages.length; ++i) { const t = pt.text && pt.text[pages[i]]; if (t && t.error) return t.error }
        return ""
    }
    function translatedCount() {
        let n = 0
        for (let i = 0; i < pages.length; ++i) {
            const t = pt.text && pt.text[pages[i]]
            if (t && (t.error || !t.blocks || t.blocks.length === 0)) { n++; continue }   // nothing to translate
            const id = trIds[pages[i]]
            if (id !== undefined && pt.translations && pt.translations[String(id)]) n++
        }
        return n
    }
    function translateFailed() {
        let n = 0
        for (let i = 0; i < pages.length; ++i) {
            const id = trIds[pages[i]]
            const r = id !== undefined && pt.translations ? pt.translations[String(id)] : null
            if (r && r.error) n++
        }
        return n
    }
    function firstTranslateError() {
        for (let i = 0; i < pages.length; ++i) {
            const id = trIds[pages[i]]
            const r = id !== undefined && pt.translations ? pt.translations[String(id)] : null
            if (r && r.error) return r.error
        }
        return ""
    }
    // asks again for what failed
    function retryFailed() {
        const rq = Object.assign({}, reqPages), ti = Object.assign({}, trIds)
        for (let i = 0; i < pages.length; ++i) {
            const u = pages[i]
            const t = pt.text && pt.text[u]
            if (t && t.error) { delete rq[u + "|o"]; Core.cmd({cmd: "forget_page_text", url: u}) }
            const id = ti[u]
            const r = id !== undefined && pt.translations ? pt.translations[String(id)] : null
            if (r && r.error) delete ti[u]
        }
        reqPages = rq; trIds = ti
        wantVisible()
        for (let i = 0; i < pages.length; ++i) maybeTranslate(pages[i])
    }
    onOcrOnChanged: wantVisible()
    onUpOnChanged: wantVisible()
    onTrOnChanged: {
        trIds = ({})
        for (let i = 0; i < pages.length; ++i) maybeTranslate(pages[i])
    }
    onTranslateAllChanged: wantVisible()
    onUpAllChanged: wantVisible()
    // translating the whole chapter: the pages that are not on screen have no ReaderPage to ask, so it is asked here
    onPtChanged: if (trOn && translateAll) for (let i = 0; i < pages.length; ++i) maybeTranslate(pages[i])
    function upCount() {
        let n = 0
        for (let i = 0; i < pages.length; ++i) if (pt.upscaled && pt.upscaled[pages[i]]) n++
        return n
    }
    // The reader state is rebuilt at every change of the core (a translation that arrives, a page read…): `pages` is then
    // a new array with the same addresses. Only a real change of chapter resets the reader, otherwise every answer
    // would ask for every translation again (an endless loop of requests).
    property string pagesKey: ""
    onPagesChanged: {
        const k = pages.join("\n")
        if (k === pagesKey) return
        pagesKey = k
        lastSent = -1
        trIds = ({})
        // the core forgets the text/upscaling of the chapter that was left: coming back to it asks again (from the disk)
        reqPages = ({})
        translateAll = false
        curPage = Math.max(0, Math.min(r.resume || 0, pages.length - 1))
        Qt.callLater(wantVisible)
        if (strip && pages.length > 0 && r.resume > 0) Qt.callLater(function() { list.positionViewAtIndex(Math.min(r.resume, pages.length - 1), ListView.Beginning) })
    }
    onModeChanged: Qt.callLater(function() { if (strip) list.positionViewAtIndex(curPage, ListView.Beginning) })

    // ---- moving ----
    function chapter(delta) {
        if (delta > 0 ? r.hasNext : r.hasPrev) Core.cmd({cmd: "manga_next", delta: delta})
    }
    // delta in the reading direction: +1 goes forward
    function go(delta) {
        if (strip) { list.contentY = Math.max(0, Math.min(list.contentHeight - list.height, list.contentY + delta * list.height * 0.9)); return }
        let to = curPage + delta
        // spreads move two pages at a time; the cover (page 1) is alone
        if (spreadWanted) to = delta > 0 ? (curPage === 0 ? 1 : curPage + (spreadOn ? 2 : 1)) : (curPage <= 1 ? curPage - 1 : curPage - 2)
        if (to < 0) { chapter(-1); return }
        if (to >= pages.length) { chapter(1); return }
        // the old page goes away with the turn chosen in the sheet: forward leaves to the left in a left-to-right book,
        // to the right in a manga, upwards in the vertical mode
        turner.turn(function() {
            curPage = to
            pager.contentX = 0
            pager.contentY = 0
        }, rtl ? delta < 0 : delta > 0)
    }
    // the sides: with the book on the right the left side is forward
    // the controller's right stick (PadNavigator, `dt` seconds since the last frame): up / down scroll the strip or
    // pan a page taller than the window, left / right zoom out / in a step at a time
    property real padZoomWait: 0
    function padStick(x, y, dt) {
        if (y !== 0) {
            const f = rp.strip ? list : pager
            const top = f.originY, bottom = f.originY + f.contentHeight - f.height
            if (bottom > top) f.contentY = Math.max(top, Math.min(bottom, f.contentY - y * 1400 * dt))
        }
        // the stick zooms only when Preferences > Controller says so (by default L2 / R2 zoom: HP's reader kept
        // zooming out with a stick that rests off centre)
        if (g.pad_zoom !== "stick" || Math.abs(x) <= 0.6) { padZoomWait = 0; return }
        padZoomWait -= dt
        if (padZoomWait <= 0) { zoom(x > 0 ? zoomStep() : -zoomStep()); padZoomWait = 0.2 }
    }
    function side(left) { go((rtl ? left : !left) ? 1 : -1) }
    onCurPageChanged: {
        wantVisible()
        if (pages.length === 0 || strip) return
        if (curPage !== lastSent) {
            lastSent = curPage
            Core.cmd({cmd: "manga_page", page: curPage})
            if (curPage === pages.length - 1 || (spreadWanted && curPage > 0 && curPage === pages.length - 2)) finished()
        }
    }

    // Escape steps back one level: the chosen bubble, then the edit mode, then the translation sheet, then the reader.
    // (A key-specific handler runs before Keys.onPressed, so every case is decided here.)
    Keys.onEscapePressed: function(e) { padBack(); e.accepted = true }
    // ○ / B of a controller does the same (Main.qml)
    function padBack() {
        if (quizBubble) quizBubble = null
        else if (rp.anySheet()) rp.closeSheets()
        else if (editMode === "card" && cardSel) cardSel = null
        else if (editMode === "text" && active) active = null
        else if (editMode !== "") { editMode = ""; active = null }
        else if (sheetOn) sheetOn = false
        else Core.cmd({cmd: "manga_close_reader"})
    }
    Keys.onPressed: function(e) {
        if (editMode === "paint" && !(e.modifiers & Qt.ControlModifier)) {
            const tools = {[Qt.Key_B]: "brush", [Qt.Key_E]: "eraser", [Qt.Key_G]: "fill", [Qt.Key_I]: "picker", [Qt.Key_C]: "crop"}
            if (tools[e.key] !== undefined) { paintTool = tools[e.key]; e.accepted = true; return }
        }
        if (editMode === "paint" && (e.modifiers & Qt.ControlModifier) && e.key === Qt.Key_Z) { undoOp(); e.accepted = true; return }
        if (e.key === Qt.Key_Plus || e.key === Qt.Key_Equal) zoom(zoomStep())
        else if (e.key === Qt.Key_Minus) zoom(-zoomStep())
        else if (e.key === Qt.Key_0) setting("reader_zoom", 100)
        else if (e.key === Qt.Key_Left) { if (strip || vertical) chapter(-1); else side(true) }
        else if (e.key === Qt.Key_Right) { if (strip || vertical) chapter(1); else side(false) }
        else if (e.key === Qt.Key_Down && vertical) go(1)
        else if (e.key === Qt.Key_Up && vertical) go(-1)
        else if (e.key === Qt.Key_PageDown || e.key === Qt.Key_Space) go(1)
        else if (e.key === Qt.Key_PageUp) go(-1)
        else if (e.key === Qt.Key_Home && !strip) curPage = 0
        else if (e.key === Qt.Key_End && !strip) curPage = Math.max(0, pages.length - 1)
        else return
        e.accepted = true
    }
    onVisibleChanged: { if (visible) { lastSent = -1; barsOn = true; forceActiveFocus() } attachSide() }

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
    readonly property string readerKind: "manga"
    function sideUpscale(btn) { toggleSheet(upPop) }
    function sideTranslate(btn) { toggleSheet(trPop) }
    function sideEdit(btn) { toggleSheet(editSheet) }
    function sideCard() { editMode = editMode === "card" ? "" : "card" }
    // HP: the text boxes over the speech bubbles can be locked while reading, so a swipe on a phone scrolls or turns
    // the page instead of opening a box; they stay drawn, and the editor still reaches them
    readonly property bool bubblesLocked: g.reader_bubble_taps === false
    function sideBubbleLock() { setting("reader_bubble_taps", bubblesLocked) }
    function sideLook(btn) { toggleSheet(lookSheet) }
    // The reader's panels (Upscaling, Translation, Editor, Themes & Settings) open on the left side, one at a time,
    // drawn the way the theme draws "Themes & Settings" (ReaderSheet.qml).
    function sheets() { return [lookSheet, editSheet, trPop, upPop] }
    function toggleSheet(sh) {
        const all = sheets()
        for (let i = 0; i < all.length; ++i) if (all[i] !== sh) all[i].close()
        if (sh.opened) sh.close(); else sh.open()
    }
    function anySheet() { return sheets().some(function(x) { return x.opened }) }
    function closeSheets() { sheets().forEach(function(x) { x.close() }) }

    // the grain of the Paper background
    PaperGrain { anchors.fill: parent; visible: rp.bgLook.grain; speck: "#4a463f" }

    // The "Themes & Settings" panel is docked on the left: the pages move right by its width instead of being covered.
    readonly property real sideInset: Math.max(lookSheet.inset, editSheet.inset, trPop.inset, upPop.inset)
    // The strip keeps its place while it gets narrower or wider (its pages change height): the page in the middle of
    // the screen and how far into it, recorded while it scrolls and put back after the width changes.
    property var stripKeep: null
    property bool stripResizing: false
    function keepStrip() {
        if (!strip || stripResizing || list.count === 0) return
        const idx = list.indexAt(list.contentX + list.width / 2, list.contentY + list.height / 2)
        const it = idx >= 0 ? list.itemAtIndex(idx) : null
        if (it && it.height > 0) stripKeep = {idx: idx, off: (list.contentY + list.height / 2 - it.y) / it.height}
    }
    function restoreStrip() {
        stripResizing = false
        const k = stripKeep
        if (!strip || !k) return
        list.positionViewAtIndex(k.idx, ListView.Beginning)
        const it = list.itemAtIndex(k.idx)
        if (it) list.contentY = Math.max(0, Math.min(list.contentHeight - list.height, it.y + k.off * it.height - list.height / 2))
    }

    // ================= webtoon: a long strip =================
    ListView {
        id: list
        visible: rp.strip
        anchors.fill: parent
        anchors.leftMargin: rp.sideInset
        onWidthChanged: if (rp.strip && rp.stripKeep) { rp.stripResizing = true; Qt.callLater(rp.restoreStrip) }
        spacing: rp.g.reader_gap || 0
        model: rp.strip ? rp.pages.length : 0
        cacheBuffer: Math.max(0, height * 2)
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        contentWidth: width * rp.zoomF
        flickableDirection: Flickable.AutoFlickIfNeeded
        ScrollBar.vertical: ScrollBar {}
        delegate: ReaderPage {
            required property int index
            rd: rp
            url: rp.pages[index] || ""
            pageNo: index + 1
            width: list.width * rp.zoomF
            x: Math.max(0, (list.width - width) / 2)
            height: ready ? width * ratio : width * 1.45
        }
        onContentYChanged: {
            if (!rp.strip || count === 0) return
            rp.keepStrip()
            rp.wantVisible()
            const idx = indexAt(contentX + width / 2, contentY + height / 2)
            if (idx >= 0 && idx !== rp.lastSent) {
                rp.lastSent = idx
                rp.curPage = idx
                Core.cmd({cmd: "manga_page", page: idx})
            }
        }
        TapHandler { onTapped: { rp.forceActiveFocus(); rp.barsOn = !rp.barsOn } }
        onAtYEndChanged: if (rp.strip && atYEnd && count > 0 && contentHeight > height) rp.finished()
    }
    // automatic scrolling of the strip
    Timer {
        interval: 16
        repeat: true
        running: rp.visible && rp.strip && (rp.g.reader_scroll || 0) > 0 && !list.moving && !list.dragging && !list.atYEnd
        onTriggered: list.contentY = Math.min(list.contentHeight - list.height, list.contentY + (rp.g.reader_scroll || 0) / 25)
    }

    // ================= pages one at a time =================
    Flickable {
        id: pager
        visible: !rp.strip
        anchors.fill: parent
        anchors.leftMargin: rp.sideInset
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        // with a spread the two pages sit side by side (in a manga the second one on the left)
        readonly property real pairW: page.width + (rp.spreadOn ? page2.width : 0)
        readonly property real pairX: Math.max(0, (contentWidth - pairW) / 2)
        contentWidth: Math.max(width, pairW)
        contentHeight: Math.max(height, page.height, rp.spreadOn ? page2.height : 0)
        // a zoomed page opens centred, not stuck to the left edge
        onContentWidthChanged: contentX = Math.max(0, (contentWidth - width) / 2)
        ReaderPage {
            id: page
            rd: rp
            url: rp.pages[rp.curPage] || ""
            pageNo: rp.curPage + 1
            readonly property real boxW: rp.fitBox(ratio, natW)
            width: boxW
            height: boxW * ratio
            x: pager.pairX + (rp.spreadOn && rp.rtl ? page2.width : 0)
            y: Math.max(0, (pager.contentHeight - height) / 2)
        }
        // the second page of a spread
        ReaderPage {
            id: page2
            rd: rp
            visible: rp.spreadOn
            url: rp.spreadWanted && rp.curPage > 0 ? (rp.pages[rp.curPage + 1] || "") : ""
            pageNo: rp.curPage + 2
            width: rp.fitBox(ratio, natW)
            height: width * ratio
            x: pager.pairX + (rp.rtl ? 0 : page.width)
            y: Math.max(0, (pager.contentHeight - height) / 2)
        }
        PosterImage { visible: false; width: 1; height: 1; url: rp.spreadWanted ? (rp.pages[rp.curPage + 2] || "") : "" }
        // the neighbours are loaded ahead
        PosterImage { visible: false; width: 1; height: 1; url: rp.pages[rp.curPage + 1] || "" }
        PosterImage { visible: false; width: 1; height: 1; url: rp.pages[rp.curPage - 1] || "" }
        MouseArea {
            // tap zones: the sides turn the page
            anchors.fill: parent
            z: -1
            onClicked: function(m) {
                rp.forceActiveFocus()   // the keys come back to the reader after a header button or a popover
                const x = m.x - pager.contentX
                if (rp.g.reader_taps && x < pager.width * 0.3) rp.side(true)
                else if (rp.g.reader_taps && x > pager.width * 0.7) rp.side(false)
                else rp.barsOn = !rp.barsOn
            }
        }
        WheelHandler {
            acceptedModifiers: Qt.ControlModifier
            onWheel: function(e) { rp.zoom(e.angleDelta.y > 0 ? rp.zoomStep() : -rp.zoomStep()) }
        }
    }

    // The width of a page in the page-by-page view, for the chosen fit; a spread shares the width between two pages.
    function fitBox(ratio, natW) {
        const vw = spreadOn ? pager.width / 2 : pager.width, vh = pager.height, z = zoomF
        if (fitMode === "width") return vw * z
        if (fitMode === "height") return vh * z / ratio
        if (fitMode === "original") return (natW > 0 ? natW : vw) * z
        return Math.min(vw, vh / ratio) * z
    }
    // the page turn of the page-by-page view
    PageTurn {
        id: turner
        anchors.fill: pager
        z: 0.5
        target: pager
        backColor: rp.color
        upward: rp.vertical
        style: rp.g.reader_turn || "slide"
    }

    // ---- where the reader is in the chapter: a thin bar with "page / pages" at the bottom, as in the Text reader;
    // a click or a drag goes to a page ----
    function seekPage(n) {
        const to = Math.max(0, Math.min(pages.length - 1, n))
        if (strip) { list.positionViewAtIndex(to, ListView.Beginning); return }
        // a spread starts on an even page (the cover is alone)
        curPage = spreadWanted && to > 0 && to % 2 === 0 ? to - 1 : to
        pager.contentX = 0
        pager.contentY = 0
    }
    ReaderProgress {
        id: progress
        visible: rp.barsOn && rp.pages.length > 1 && !rp.sheetOn && rp.editMode === ""
        z: 3
        anchors { bottom: parent.bottom; bottomMargin: 6; horizontalCenter: pager.horizontalCenter }
        width: Math.max(120, Math.min(640, pager.width - 160))
        current: rp.curPage + 1
        count: rp.pages.length
        ink: rp.light ? "#222222" : "#dddddd"
        onSeek: function(page) { rp.seekPage(page - 1); rp.forceActiveFocus() }
    }

    // ================= buttons on the screen =================
    component Round: ModeButton { }
    Column {
        visible: !!rp.g.reader_buttons && rp.barsOn
        anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
        spacing: 10
        Round { glyph: "plus"; tip: rp.tr("Zoom in (+)"); onClicked: rp.zoom(rp.zoomStep()) }
        Round { glyph: "minus"; tip: rp.tr("Zoom out (-)"); onClicked: rp.zoom(-rp.zoomStep()) }
        Round { glyph: "expand"; tip: rp.tr("Fit the page (0)"); onClicked: rp.setting("reader_zoom", 100) }
        Round { glyph: "prev"; tip: rp.tr("Previous chapter"); enabledBtn: !!rp.r.hasPrev; onClicked: rp.chapter(-1) }
        Round { glyph: "next"; tip: rp.tr("Next chapter"); enabledBtn: !!rp.r.hasNext; onClicked: rp.chapter(1) }
    }
    Round {
        visible: !!rp.g.reader_buttons && rp.barsOn && !rp.strip
        anchors { left: parent.left; leftMargin: 14 + rp.sideInset; verticalCenter: parent.verticalCenter }
        glyph: "left"
        tip: rp.tr("Previous page")
        onClicked: rp.side(true)
    }
    Round {
        visible: !!rp.g.reader_buttons && rp.barsOn && !rp.strip
        anchors { right: parent.right; rightMargin: 14; top: parent.verticalCenter; topMargin: 190 }
        glyph: "right"
        tip: rp.tr("Next page")
        onClicked: rp.side(false)
    }

    // ================= actions =================
    // While reading they sit in the app's left sidebar (ReaderSidebar, through the window's readerSideSlot), one per
    // line with its name, so the pages keep the whole height. Without a sidebar they stay in the header bar on the right.
    property Item slot: null
    property Item sideSlot: null            // set by ModeBrowser from Main's readerSideSlot
    readonly property bool inSide: false   // the sidebar draws its own buttons now (ReaderSidebar)
    readonly property bool sideLabels: inSide && sideSlot.width >= 150
    // a popover beside the sidebar button, or under the header button
    function openAt(pop, btn) { if (rp.sideOn) pop.openRight(btn); else pop.openBelow(btn, true) }
    component ActBtn: FlatButton {
        property string label: ""
        property bool popOpen: false
        focusPolicy: Qt.NoFocus   // the arrow keys must keep turning the pages
        width: rp.inSide ? rp.sideSlot.width : implicitWidth
        implicitWidth: rp.sideLabels ? contentItem.implicitWidth + 36 : 40
        implicitHeight: rp.inSide ? 36 : 40
        alignStart: rp.sideLabels
        text: rp.sideLabels ? label : ""
        ToolTip.visible: hovered && !rp.sideLabels && !popOpen
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
        ActBtn {
            id: upBtn
            kind: rp.upOn ? "suggested" : "flat"
            glyph: "upscale"
            label: rp.tr("Upscale the pages with AI")
            popOpen: upPop.opened
            onClicked: rp.toggleSheet(upPop)
        }
        ActBtn {
            id: trBtn
            kind: rp.trOn || rp.ocrOn ? "suggested" : "flat"
            glyph: "translate"
            label: rp.tr("Text of the pages and translation")
            popOpen: trPop.opened
            onClicked: rp.toggleSheet(trPop)
        }
        ActBtn {
            id: editBtn
            kind: rp.editMode !== "" ? "suggested" : "flat"
            glyph: "edit"
            label: rp.tr("Edit the text boxes and the pictures")
            popOpen: editSheet.opened
            onClicked: rp.toggleSheet(editSheet)
        }
        ActBtn {
            visible: rp.pages.length > 0
            kind: rp.bubblesLocked ? "suggested" : "flat"
            glyph: "lock"
            label: rp.bubblesLocked ? rp.tr("Text boxes locked: a tap goes to the page") : rp.tr("Lock the text boxes (a tap goes to the page)")
            onClicked: rp.sideBubbleLock()
        }
        ActBtn {
            id: cardBtn
            kind: rp.editMode === "card" ? "suggested" : "flat"
            glyph: "study"
            label: rp.tr("Study card from a part of the page")
            onClicked: rp.editMode = rp.editMode === "card" ? "" : "card"
        }
        FlatButton {
            id: setBtn
            focusPolicy: Qt.NoFocus   // the arrow keys must keep turning the pages
            kind: "flat"
            width: rp.inSide ? rp.sideSlot.width : implicitWidth
            implicitWidth: aaRow.implicitWidth + 20
            implicitHeight: rp.inSide ? 36 : 40
            ToolTip.visible: hovered && !lookSheet.visible && !rp.sideLabels
            ToolTip.text: rp.tr("Themes & Settings")
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
                        width: Math.min(implicitWidth, setBtn.width - 60); elide: Text.ElideRight
                        text: rp.tr("Themes & Settings")
                        color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family
                    }
                }
            }
            onClicked: rp.toggleSheet(lookSheet)
        }
        Item { visible: rp.pages.length > 0; implicitWidth: cnt.implicitWidth + 16; implicitHeight: rp.inSide ? 30 : 40; width: rp.inSide ? rp.sideSlot.width : implicitWidth; height: rp.inSide ? 30 : 40; Lbl { id: cnt; anchors.centerIn: parent; text: (rp.curPage + 1) + (rp.spreadOn ? "–" + (rp.curPage + 2) : "") + " / " + rp.pages.length; bold: true; size: 14 } }
    }

    // ---- reading options: the "Themes & Settings" sheet ----
    PageLookSheet { id: lookSheet; rd: rp }

    // the quiz card of a speech bubble, at the bottom in the middle, over the pages
    Loader {
        active: rp.quizBubble !== null
        z: 20
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 56
        width: Math.min(440, rp.width - 32)
        sourceComponent: ReaderQuizCard {
            text: rp.quizBubble ? rp.quizBubble.text : ""
            meaning: rp.quizBubble ? rp.quizBubble.meaning : ""
            lang: ""
            onClosed: rp.quizBubble = null
            onDone: function(ok) { quizClose.start() }
            Timer { id: quizClose; interval: 2500; onTriggered: rp.quizBubble = null }
        }
    }

    // ---- text of the pages and translation: the "Translation" panel ----
    ReaderSheet {
        id: trPop
        title: rp.tr("Translation")
        sheetWidth: 400
        Column {
            width: parent.width
            spacing: 4
            Lbl { text: rp.tr("Text of the pages"); dim: true; size: 12 }
            Lbl { width: parent.width; text: rp.tr("Click a speech bubble to switch between the original and the translation."); dim: true; size: 13; wrapMode: Text.Wrap }
        }
        PrefGroup {
            PrefSwitch { title: rp.tr("Read the text as the pages appear"); checked: !!rp.g.reader_ocr; onToggled: function(on) { rp.setting("reader_ocr", on) } }
        }
        Column {
            width: parent.width
            spacing: 4
            Lbl { text: rp.tr("Translate the chapter"); dim: true; size: 12 }
            Lbl { width: parent.width; text: rp.tr("Reads every page, finds out the language by itself and writes the translation over the speech bubbles."); dim: true; size: 13; wrapMode: Text.Wrap }
        }
        PrefGroup {
            PrefCombo {
                title: rp.tr("Translate into")
                readonly property var codes: ["", "app", "en", "pt", "es", "fr", "de", "it", "nl", "sv", "pl", "ru", "tr", "ar", "hi", "id", "vi", "th", "ja", "ko", "zh"]
                options: [rp.tr("Off"), rp.tr("System language"), "English", "Português", "Español", "Français", "Deutsch", "Italiano", "Nederlands", "Svenska", "Polski", "Русский", "Türkçe", "العربية", "हिन्दी", "Bahasa Indonesia", "Tiếng Việt", "ไทย", "日本語", "한국어", "中文"]
                current: Math.max(0, codes.indexOf(rp.g.reader_translate || ""))
                onPicked: function(i) { rp.setting("reader_translate", codes[i]); if (codes[i] === "") rp.translateAll = false }
            }
            PrefSwitch { title: rp.tr("Show the original"); checked: rp.showOriginal; onToggled: function(on) { rp.showOriginal = on } }
        }
        FlatButton {
            kind: "suggested"
            enabled: rp.trOn
            text: rp.tr("Translate the chapter")
            onClicked: rp.translateAll = true
        }
        ProgressRow {
            width: parent.width
            visible: rp.translateAll
            label: rp.tr("Reading the text of the pages")
            total: rp.pages.length
            done: rp.readCount()
            busy: true
            error: rp.readFailed() > 0 && done >= total ? rp.readFailed() + " " + rp.tr("pages could not be read") + ": " + rp.firstReadError() : ""
            detail: done < total ? rp.tr("Finding the speech bubbles on every page.") + (rp.readFailed() > 0 ? "  (" + rp.readFailed() + " " + rp.tr("failed") + ")" : "") : ""
        }
        ProgressRow {
            width: parent.width
            visible: rp.translateAll
            label: rp.tr("Translating the pages")
            total: rp.pages.length
            done: rp.translatedCount()
            busy: true
            error: rp.translateFailed() > 0 ? rp.translateFailed() + " " + rp.tr("pages could not be translated") + ": " + rp.firstTranslateError() : ""
            detail: done < total ? rp.tr("A page at a time, with the whole page as context, so the dialogue reads naturally.") : rp.tr("Done")
        }
        FlatButton {
            visible: rp.translateAll && (rp.readFailed() > 0 || rp.translateFailed() > 0)
            kind: "flat"
            text: rp.tr("Try again")
            onClicked: rp.retryFailed()
        }
    }

    // ---- upscaling: the "Upscaling" panel ----
    ReaderSheet {
        id: upPop
        title: "Upscaling"
        sheetWidth: 400
        PrefGroup {
            PrefSwitch { title: rp.tr("Upscale the pages"); subtitle: rp.tr("Enlarges the pictures with AI. The engine (about 40 MB) is downloaded the first time."); checked: rp.upOn; onToggled: function(on) { rp.setting("reader_upscale", on) } }
            PrefCombo {
                visible: (rp.pt.models || []).length > 0
                title: rp.tr("Model")
                options: rp.pt.models || []
                current: Math.max(0, (rp.pt.models || []).indexOf(rp.pt.model))
                onPicked: function(i) { rp.setting("up_model", rp.pt.models[i]); rp.reqPages = ({}) ; rp.wantVisible() }
            }
            PrefRow {
                title: rp.tr("Size")
                subtitle: rp.tr("At most 2× the original page: sharper, without filling the memory of the graphics card. Pages already done are kept.")
            }
        }
        FlatButton {
            visible: rp.upOn && !!rp.pt.engine
            kind: "suggested"
            text: rp.tr("Sharpen the whole chapter")
            enabled: !rp.upAll || rp.upCount() >= rp.pages.length
            onClicked: rp.upAll = true
        }
        ProgressRow {
            width: parent.width
            visible: rp.upOn && !!rp.pt.engine && rp.upAll
            label: rp.tr("Sharpening the pages")
            total: rp.pages.length
            done: rp.upCount()
            busy: true
            detail: done < total ? rp.tr("The page keeps its size; only the detail gets sharper.") : rp.tr("Done")
        }
        FlatButton {
            visible: rp.upOn && !rp.pt.engine
            kind: "suggested"; glyph: "download"
            text: rp.tr("Install upscaler")
            onClicked: Core.cmd({cmd: "upscale_install"})
        }
    }

    // ---- the sheet at the bottom: the chosen text and its translation ----
    Rectangle {
        id: sheet
        visible: rp.sheetOn
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: Math.min(parent.height * 0.5, sheetCol.implicitHeight + 28)
        color: Theme.c.popover
        border.width: 1; border.color: Theme.c.border
        z: 20
        MouseArea { anchors.fill: parent }
        Column {
            id: sheetCol
            x: 18; y: 14
            width: parent.width - 36
            spacing: 8
            Item {
                width: parent.width; height: 34
                Lbl { text: rp.tr("Translation of the selected text"); bold: true; size: 15; anchors.verticalCenter: parent.verticalCenter }
                Row {
                    anchors.right: parent.right; spacing: 4
                    FlatButton { kind: "flat"; implicitHeight: 32; text: rp.tr("Copy the translation"); enabled: !!(rp.sheetRes && rp.sheetRes.texts); onClicked: { clip.text = rp.sheetRes.texts.join("\n"); clip.selectAll(); clip.copy(); clip.text = "" } }
                    FlatButton { kind: "flat"; implicitHeight: 32; text: rp.tr("Try again"); visible: !!(rp.sheetRes && rp.sheetRes.error); onClicked: rp.openSheet() }
                    FlatButton { kind: "circular"; glyph: "close"; implicitWidth: 32; implicitHeight: 32; onClicked: rp.sheetOn = false }
                }
            }
            Row {
                width: parent.width; spacing: 16
                Column {
                    width: (parent.width - 16) / 2; spacing: 4
                    Lbl { text: rp.tr("Original"); dim: true; size: 12 }
                    ScrollView { width: parent.width; height: Math.min(180, Math.max(48, orig.implicitHeight)); TextArea { id: orig; readOnly: true; selectByMouse: true; wrapMode: TextArea.Wrap; text: rp.sheetText; color: Theme.c.text; selectionColor: Theme.c.accent; selectedTextColor: Theme.c.accentFg; font.pixelSize: 15; font.family: Theme.type.family; background: null } }
                }
                Column {
                    width: (parent.width - 16) / 2; spacing: 4
                    Lbl { text: rp.targetLang() || Core.uiLang; dim: true; size: 12 }
                    Row {
                        spacing: 8; visible: rp.sheetOn && !rp.sheetRes
                        BusyIndicator { running: parent.visible; width: 24; height: 24 }
                        Lbl { text: rp.tr("Translating…"); dim: true }
                    }
                    ScrollView { visible: !!rp.sheetRes; width: parent.width; height: Math.min(180, Math.max(48, tro.implicitHeight)); TextArea { id: tro; readOnly: true; selectByMouse: true; wrapMode: TextArea.Wrap; selectionColor: Theme.c.accent; selectedTextColor: Theme.c.accentFg; text: rp.sheetRes ? (rp.sheetRes.error ? rp.sheetRes.error : (rp.sheetRes.texts || []).join("\n")) : ""; color: rp.sheetRes && rp.sheetRes.error ? Theme.c.errorText : Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family; background: null } }
                }
            }
        }
    }

    // loading and errors
    Column {
        anchors.centerIn: parent
        spacing: 10
        visible: rp.pages.length === 0
        BusyIndicator { visible: !rp.r.error; running: visible; anchors.horizontalCenter: parent.horizontalCenter }
        Lbl {
            anchors.horizontalCenter: parent.horizontalCenter
            text: rp.r.error ? rp.tr(rp.r.error) : rp.tr("Loading pages…")
            color: rp.r.error ? "#ff7b72" : (rp.light ? "#333333" : "#cccccc")
            wrapMode: Text.Wrap
            width: Math.min(560, rp.width - 40)
            horizontalAlignment: Text.AlignHCenter
            size: 16
        }
    }

    // ================= hand corrections =================
    // The "Editor" panel of the pencil button: the tools that work right on the page (text boxes, picture, study card)
    // as cards like the themes of "Themes & Settings", and the chapter's translation file. Picking a tool closes the
    // panel when it lies over the page; beside the page (a wide window) it stays open.
    ReaderSheet {
        id: editSheet
        title: rp.tr("Editor")
        sheetWidth: 400
        Column {
            width: parent.width
            spacing: 8
            Lbl { text: rp.tr("Tools"); dim: true; size: 12 }
            Grid {
                width: parent.width
                columns: 3
                spacing: 10
                Repeater {
                    model: [{id: "text", glyph: "edit", name: rp.tr("Text boxes")}, {id: "paint", glyph: "brush", name: rp.tr("Picture")}, {id: "card", glyph: "study", name: rp.tr("Study card")}]
                    delegate: ToolCard {
                        required property var modelData
                        width: (parent.width - 20) / 3
                        name: modelData.name
                        glyph: modelData.glyph
                        selected: rp.editMode === modelData.id
                        onClicked: {
                            rp.editMode = rp.editMode === modelData.id ? "" : modelData.id
                            rp.active = null
                            if (rp.editMode !== "" && !editSheet.docked) editSheet.close()
                        }
                    }
                }
            }
            Lbl {
                width: parent.width
                wrapMode: Text.Wrap
                dim: true
                size: 13
                text: rp.editMode === "text" ? rp.tr("Text boxes: click one to correct it, drag it to move, drag on the page to draw a new one")
                    : rp.editMode === "card" ? rp.tr("Study card: drag a rectangle over the part of the page to keep")
                    : rp.editMode === "paint" ? rp.tr("Picture: paint, fill, erase or crop. Saved in the chapter's translation file")
                    : rp.tr("Pick a tool and work right on the page.")
            }
        }
        Column {
            width: parent.width
            spacing: 8
            Lbl { text: rp.tr("Translation file (.md)"); dim: true; size: 12 }
            SheetLink { glyph: "download"; text: rp.tr("Save the translation (.md)…"); onClicked: mdSave.open() }
            SheetLink { glyph: "folder"; text: rp.tr("Open a translation (.md)…"); onClicked: mdOpen.open() }
            SheetLink {
                visible: (rp.db.file || "") !== ""
                glyph: "external"
                text: rp.tr("Show the translation file")
                onClicked: Qt.openUrlExternally(Core.fileUrl((rp.db.file || "").replace(/[\\/][^\\/]*$/, "")))
            }
        }
    }
    FileDialog {
        id: mdSave
        title: rp.tr("Save the translation")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "md"
        nameFilters: ["Markdown (*.md)"]
        onAccepted: Core.cmd({cmd: "pagedb_export", path: Core.localPath(selectedFile.toString())})
    }
    FileDialog {
        id: mdOpen
        title: rp.tr("Open a translation")
        nameFilters: ["Markdown (*.md)", "All files (*)"]
        onAccepted: Core.cmd({cmd: "pagedb_import", path: Core.localPath(selectedFile.toString())})
    }
    // what the edit mode does: a tool, so on the left side at the top (the middle is the page)
    Rectangle {
        visible: rp.editMode !== ""
        anchors { left: parent.left; leftMargin: 16 + rp.sideInset; top: parent.top; topMargin: 12 }
        z: 21
        // never wider than the window: the hint is cut with "…" on small windows
        width: Math.max(0, Math.min(modeHint.implicitWidth + modeDone.implicitWidth + modeRow.spacing + 28, rp.width - 32 - rp.sideInset, 620)); height: 40; radius: 20
        color: Theme.c.popover; border.width: 1; border.color: Theme.c.border
        Row {
            id: modeRow
            anchors.centerIn: parent
            spacing: 10
            Lbl { id: modeHint; anchors.verticalCenter: parent.verticalCenter; width: Math.max(0, modeRow.parent.width - 28 - modeDone.implicitWidth - modeRow.spacing); elide: Text.ElideRight; size: 13; text: rp.editMode === "text" ? rp.tr("Text boxes: click one to correct it, drag it to move, drag on the page to draw a new one") : (rp.editMode === "card" ? rp.tr("Study card: drag a rectangle over the part of the page to keep") : rp.tr("Picture: paint, fill, erase or crop. Saved in the chapter's translation file")) }
            FlatButton { id: modeDone; anchors.verticalCenter: parent.verticalCenter; kind: "suggested"; implicitHeight: 30; text: rp.tr("Done"); onClicked: { rp.editMode = ""; rp.active = null; rp.forceActiveFocus() } }
        }
    }
    // the chosen bubble: beside the page when there is room, over it when not
    BubbleEditPanel {
        id: bubblePanel
        rd: rp
        z: 22
        readonly property real pageW: rp.strip ? list.width * rp.zoomF : page.width
        readonly property real margin: Math.max(0, ((rp.strip ? list.width : pager.width) - pageW) / 2)
        readonly property bool beside: margin >= 300
        width: beside ? Math.min(420, margin - 24) : Math.min(380, rp.width - 32)
        x: beside ? rp.width - margin + 12 : rp.width - width - 16
        // small windows: the panel scrolls inside instead of running off the bottom
        height: Math.max(0, Math.min(implicitHeight, rp.height - 80))
        y: Math.max(64, Math.min(rp.height - height - 16, (rp.height - height) / 2))
    }
    // the study card of the chosen part of a page: placed like the bubble panel
    PageCardPanel {
        rd: rp
        z: 22
        readonly property real pageW: rp.strip ? list.width * rp.zoomF : pager.pairW
        readonly property real margin: Math.max(0, ((rp.strip ? list.width : pager.width) - pageW) / 2)
        readonly property bool beside: margin >= 300
        width: beside ? Math.min(420, margin - 24) : Math.min(380, rp.width - 32)
        x: beside ? rp.width - margin + 12 : rp.width - width - 16
        height: Math.max(0, Math.min(implicitHeight, rp.height - 80))
        y: Math.max(64, Math.min(rp.height - height - 16, (rp.height - height) / 2))
    }
    PaintTools {
        visible: rp.editMode === "paint"
        rd: rp
        z: 22
        x: 16 + rp.sideInset
        height: Math.max(0, Math.min(implicitHeight, rp.height - 80))
        y: Math.max(64, Math.min(rp.height - height - 16, (rp.height - height) / 2))
    }
}
