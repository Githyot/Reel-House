import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Reelhouse 1.0

ApplicationWindow {
    id: win

    // "Size in general" (Preferences), live: the pages and the popup layer (dialogs, menus, tips) are drawn scaled from
    // their top left corner (120 % is the natural size) and laid out in the window size divided by the scale, so they
    // still fill the window. Text is drawn as distance fields, so it stays sharp at any scale.
    readonly property real uiScaleF: Math.max(0.5, Math.min(2, ((Core.prefs.general && Core.prefs.general.ui_scale) || 120) / 120))
    // ApplicationWindow and the Overlay give themselves the window size again on every resize (Bindings lost that race,
    // which is why the size seemed to do nothing or cut the right side off), so the size is put back after each change.
    function fitScale() {
        const s = win.uiScaleF, w = win.width / s, h = win.height / s
        const layers = [win.contentItem, win.Overlay.overlay]
        for (var i = 0; i < layers.length; ++i) {
            const it = layers[i]
            if (!it) continue
            it.transformOrigin = Item.TopLeft
            if (it.scale !== s) it.scale = s
            if (Math.abs(it.width - w) > 0.5) it.width = w
            if (Math.abs(it.height - h) > 0.5) it.height = h
        }
    }
    onUiScaleFChanged: fitScale()
    onWidthChanged: Qt.callLater(fitScale)
    onHeightChanged: Qt.callLater(fitScale)
    Connections { target: win.contentItem; function onWidthChanged() { win.fitScale() } function onHeightChanged() { win.fitScale() } }
    Connections { target: win.Overlay.overlay; function onWidthChanged() { win.fitScale() } function onHeightChanged() { win.fitScale() } }
    width: 1280
    height: 800
    // a window may be made as narrow as a phone (under 600 px the pages take the phone layout, Theme.mobile)
    minimumWidth: 360
    minimumHeight: 520
    // ---- mobile (Material 3 / iOS HIG) ----
    // A phone or tablet build, or a window narrower than 600 px (the laid-out width, so "Size in general" counts):
    // the pages lay themselves out for a phone (Theme.mobile, read everywhere). A Platform singleton of the Android /
    // iOS builds can feed Theme.mobile, Theme.touch and Theme.safe (the safe area) later; the window width keeps
    // working on a desktop, so the phone layout can be tried by making the window narrow.
    readonly property bool phoneOs: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property bool narrow: width / uiScaleF < 600
    Binding { target: Theme; property: "mobile"; value: win.phoneOs || win.narrow }
    readonly property var safe: Theme.safe !== undefined ? Theme.safe : ({top: 0, bottom: 0, left: 0, right: 0})
    visible: true
    title: Core.tr(Core.uiLang, "Reelhouse")
    color: Theme.c.window
    flags: Qt.Window | Qt.FramelessWindowHint
    // Controls (Label, TextField, ComboBox...) inherit the window font: the theme's text font, switched live.
    font.family: Theme.type.family
    // The stock controls (attached ToolTips, BusyIndicator, text selection, sliders, popups) draw with the palette:
    // without this they take the system one, and a light system under a dark theme shows pale/white boxes.
    palette.window: Theme.c.window
    palette.windowText: Theme.c.text
    palette.base: Theme.c.view
    palette.alternateBase: Theme.c.surface
    palette.text: Theme.c.text
    palette.placeholderText: Theme.c.textDim
    palette.button: Theme.c.button
    palette.buttonText: Theme.c.text
    palette.brightText: Theme.c.accentFg
    palette.highlight: Theme.c.accent
    palette.highlightedText: Theme.c.accentFg
    palette.accent: Theme.c.accent
    palette.link: Theme.c.accentText
    palette.toolTipBase: Theme.c.popover
    palette.toolTipText: Theme.c.text
    palette.light: Theme.c.surface2
    palette.midlight: Theme.c.surface2
    palette.mid: Theme.c.divider
    palette.dark: Theme.c.textDim
    palette.shadow: "#66000000"

    property string tab: "discover"      // discover | library | calendar | search
    property bool searchOpen: false
    property string mtab: "library"      // Images and Text modes: library | explore | history
    // Images / Text: the list pages (0), a series (1) or the reader (2), what the header shows and its way back
    readonly property var mst: images ? Core.manga : (Core.manga.novels || ({}))
    readonly property int modeDepth: !(images || texts) ? 0 : (mst.reader ? 2 : (mst.view ? 1 : 0))
    readonly property string modeSub: {
        if (modeDepth === 2) {
            const r = mst.reader
            if (images) return (r.chapter && r.chapter.label) ? r.chapter.label : ""
            return (r.chapter ? Core.tr(Core.uiLang, "Chapter") + " " + r.chapter : "") + (r.chapterTitle ? (r.chapter ? " · " : "") + r.chapterTitle : "")
        }
        return ""
    }
    function modeClose() { Core.cmd({cmd: (images ? "manga_" : "novel_") + (modeDepth === 2 ? "close_reader" : "close")}) }
    property int modeSearch: 0           // the magnifier in those modes
    readonly property bool inDetail: Core.detail.open === true
    readonly property bool inPlayer: Core.player.active === true
    readonly property bool images: (Core.manga.mode || "video") === "images"   // Images mode replaces the video pages
    readonly property bool study: (Core.manga.mode || "video") === "study"      // Study mode (flashcards)
    readonly property bool studySession: study && Core.study.session !== undefined && Core.study.session !== null
    readonly property bool texts: (Core.manga.mode || "video") === "text"       // Text mode (novels) replaces the video pages
    readonly property bool research: (Core.manga.mode || "video") === "research" // Research mode (the built-in browser)
    // The player's quick search hands a page over to Research mode.
    function researchOpen(url) { win.needResearch().openUrl(url); showToast(Core.tr(Core.uiLang, "Opened in Research mode")) }
    // the browser page, for the browser part of the sidebar (ResearchSidebar.qml finds it here)
    readonly property var researchView: researchPage
    // Research (a whole browser) and Study are built the first time they are needed, not at start (see their Loaders)
    readonly property var researchPage: researchHost.item
    readonly property var studyPage: studyHost.item
    function needResearch() { if (researchHost.status !== Loader.Ready) { researchHost.asynchronous = false; researchHost.active = true } return researchHost.item }
    function needStudy() { if (studyHost.status !== Loader.Ready) { studyHost.asynchronous = false; studyHost.active = true } return studyHost.item }
    property string platform: ""         // Catalog > Platforms ("" = everything)
    property real nightFilter: Core.settings.nightFilter !== undefined ? Core.settings.nightFilter : 0

    // ---- pages above the start page (GTK NavigationView): "genre", "detail", "info" ----
    property var stack: []
    readonly property string top: stack.length > 0 ? stack[stack.length - 1] : "home"
    property string shownDetailId: ""
    function push(p) { var s = stack.slice(); s.push(p); stack = s }
    function without(names) { return stack.filter(function(x) { return names.indexOf(x) < 0 }) }
    // Ctrl+ / Ctrl- change the size of the interface in steps of 10% (50% to 200%), Ctrl+0 goes back to 100%.
    function zoom(step) {
        const cur = (Core.prefs.general && Core.prefs.general.ui_scale) || 120
        Core.cmd({cmd: "set_setting", path: "general.ui_scale", value: Math.max(50, Math.min(200, step === 0 ? 120 : cur + step))})
    }
    function back() {
        if (top === "info") { var s = stack.slice(); s.pop(); stack = s }
        else if (top === "detail") Core.cmd({cmd: "close_detail"})
        else if (top === "genre") {
            var t = stack.slice(); t.pop(); stack = t
            if (stack.indexOf("genre") < 0) Core.cmd({cmd: "close_genre"})
        }
    }
    // Forward: reopens the title page that was left last (GTK go_forward).
    function forward() { if (!win.inPlayer) Core.cmd({cmd: "go_forward"}) }
    function home() {
        if (Core.detail.open === true) Core.cmd({cmd: "close_detail", all: true})
        if (stack.indexOf("genre") >= 0) Core.cmd({cmd: "close_genre"})
        stack = []
    }
    function openGenre(id, name, kind) {
        // Only one category page at a time: a category opened from a category page replaces it.
        var s = stack.slice()
        var at = s.indexOf("genre")
        if (at >= 0) s = s.slice(0, at)
        stack = s
        Core.cmd({cmd: "open_genre", id: id, name: name, kind: kind})
        push("genre")
    }
    function openInfo() { if (top !== "info") push("info") }
    function testOpen(what) { if (what === "player:pin") { playerItem.pinned = true; playerItem.shown = true } else if (what.indexOf("player:editor") === 0) { playerItem.pinned = true; playerItem.shown = true; var q = what.split(":"); if (q.length >= 4) playerItem.openEditor(parseFloat(q[2]), parseFloat(q[3])); else playerItem.openEditor() } else if (what === "study:more") win.needStudy().openMore(); else if (what.indexOf("study:") === 0) win.needStudy().go(what.substring(6)); else if (what === "about") about.open(); else if (what === "studio") studioTest.open(); else if (what === "picker") pickerTest.pick("#3584e4"); else if (what.indexOf("prefs") === 0) openPrefs(what.length > 6 ? what.substring(6) : "general") }
    function openPrefs(name) { prefs.show(name ? name : "general") }
    // Game controllers plugged in now: [{kind, name}]
    property var pads: []
    // Controller mode: a controller is plugged in. Then the focus blip plays, the orbs of time circle behind the
    // pages (Preferences > General > Controller mode) and a bar shows what the buttons do.
    readonly property bool padMode: pads.length > 0
    readonly property string padKind: pads.length > 0 ? pads[pads.length - 1].kind : "xbox"
    readonly property var padGen: Core.prefs.general || ({})
    property real padPressedAt: 0
    onActiveFocusItemChanged: {
        // only focus moved by the controller or by Tab (Qt 6.7+ tells the reason), not by the mouse
        const byPad = padMode && Date.now() - padPressedAt <= 400
        const it = activeFocusItem
        const byTab = !!it && (it.focusReason === Qt.TabFocusReason || it.focusReason === Qt.BacktabFocusReason)
        if (!byPad && !byTab) return
        if (byPad) padNav.ensureVisible(it)
        if (padGen.pad_sound !== false && uiSounds.item && (byPad || !!padGen.sound_nav)) uiSounds.item.nav()
        if (byPad && padGen.pad_rumble === true) Core.padRumble(110, 0, 45)
    }
    // the controller's spatial navigation, repeat and scrolling (PadNavigator.qml)
    PadNavigator { id: padNav; window: win; repeatMs: win.padGen.pad_repeat || 90; player: playerItem; reader: win.padReader }
    // Preferences > Controller to the core: stick deadzone, and for PlayStation controllers over HID the light bar
    // (the theme's accent unless a colour of its own was picked, live when the theme or accent changes), the touchpad
    // as a mouse and its speed. HID only runs in controller mode.
    readonly property var padCfg: ({
        on: win.padMode,
        deadzone: (win.padGen.pad_deadzone || 30) / 100,
        light: win.padGen.pad_light_accent === false && (win.padGen.pad_light_color || "") !== "" ? win.padGen.pad_light_color : "" + Theme.c.accent,
        brightness: win.padGen.pad_light_brightness !== undefined ? win.padGen.pad_light_brightness : 100,
        touchMouse: win.padGen.pad_touch_mouse !== false,
        touchSpeed: (win.padGen.pad_touch_speed || 100) / 100
    })
    onPadCfgChanged: Core.padConfig(padCfg)
    // the on-screen keyboard of controller mode (✕/A on a text box)
    PadKeyboard { id: padKeyboard }
    // a page that takes the face buttons itself while it shows (StudyReview: the grades; KaiwaPage: talk, replay…);
    // it sets and clears this. A page with a box or buttons along the bottom says `padHintsTop: true` to get the hint
    // bar at the top right, and may give `padHintsY` (window y) to put it under its own bar.
    property Item padPage: null
    // the height of the hint bar, for a page that keeps room for it (KaiwaPage)
    readonly property real padHintHeight: padHintBar.height
    // the hint bar fades after a few seconds without a press and comes back with the next one
    property bool padIdle: false
    Timer { id: padIdleTimer; interval: 4000; onTriggered: win.padIdle = true }
    function padWake() { padIdle = false; padIdleTimer.restart() }
    // the reader open now (Images or Text), for the page and chapter buttons
    readonly property Item padReader: (images || texts) && modeDepth === 2 && activeReader && activeReader.visible ? activeReader : null
    readonly property bool padReaderManga: padReader !== null && padReader.readerKind === "manga"
    // the interface sounds (UiSounds.qml): focus blip, select, and the background music. They play the files picked in
    // Preferences > General > Sounds and music (none ship with the app). The music plays whenever a music file is
    // picked (with or without a controller) and stops in the player, in Study (Learn, reviews with audio,
    // Conversation), in Preferences (voice and TTS tests) and when the window is not focused or is minimized.
    Loader {
        id: uiSounds
        source: "UiSounds.qml"
        active: win.padMode || !!(win.padGen.sound_nav || win.padGen.sound_select || win.padGen.sound_music)
        onLoaded: {
            item.gen = Qt.binding(function() { return win.padGen })
            item.dataDir = Qt.binding(function() { return (Core.prefs.extra && Core.prefs.extra.dataDir) || "" })
            item.musicWanted = Qt.binding(function() {
                return win.active && win.visibility !== Window.Minimized && !win.inPlayer && !win.study && !prefs.visible
            })
        }
    }
    // a click on a control (a button, a row, a card: anything with a clicked signal) plays the select sound, like
    // ✕ / A does, when a select sound is picked or in controller mode. The handler only watches (a passive grab), so
    // the click still goes to the control under it; popups and menus above it are left out.
    Item {
        id: clickSound
        objectName: "padPassive"
        anchors.fill: parent
        z: 100000
        enabled: !!uiSounds.item && win.padGen.pad_sound !== false && (win.padMode || !!win.padGen.sound_select)
        function clickableAt(x, y) {
            var it = win.contentItem, p = {x: x, y: y}, found = null
            for (var n = 0; it && n < 80; ++n) {
                if (it !== win.contentItem && it.enabled && typeof it.clicked === "function") found = it
                var next = null
                for (var i = it.children.length - 1; i >= 0; --i) {
                    const c = it.children[i]
                    if (c === clickSound || !c.visible || !c.enabled) continue
                    const q = it.mapToItem(c, p.x, p.y)
                    if (q.x >= 0 && q.y >= 0 && q.x < c.width && q.y < c.height) { next = c; p = q; break }
                }
                it = next
            }
            return found
        }
        TapHandler {
            gesturePolicy: TapHandler.DragThreshold
            acceptedButtons: Qt.LeftButton
            onTapped: function(point) {
                if (uiSounds.item && clickSound.clickableAt(point.position.x, point.position.y)) uiSounds.item.select()
            }
        }
    }
    // R1 / L1 (RB / LB): the next or previous tab or section of the mode — Preferences' tabs while it is open, the
    // pages of Study, Library / Explore / History in Images and Text, the next chapter in a reader, the video tabs
    function padSection(step) {
        if (prefs.visible) { prefs.padTab(step); return }
        if (padReader) {
            if (padReaderManga) padReader.chapter(step)
            else if (step > 0 ? padReader.r.hasNext : padReader.r.hasPrev) Core.cmd({cmd: "novel_next", delta: step})
            return
        }
        if (win.study) {
            const s = win.needStudy()
            if (!s) return
            const ids = s.talkOk ? ["decks", "learn", "add", "browse", "stats", "time", "talk"] : ["decks", "learn", "add", "browse", "stats", "time"]
            const i = Math.max(0, ids.indexOf(s.view))
            if (win.studySession) Core.cmd({cmd: "study_close"})
            s.go(ids[(i + step + ids.length) % ids.length])
            return
        }
        if (win.images || win.texts) {
            if (win.modeDepth > 0) return
            const m = ["library", "explore", "history"]
            win.mtab = m[(Math.max(0, m.indexOf(win.mtab)) + step + m.length) % m.length]
            return
        }
        padTab(step)
    }
    // R2 / L2 (RT / LT): a page — the next or previous page in a reader, else about a screen down or up
    function padPageTurn(step) {
        if (padReader && !padInPopup) {
            if (padReaderManga) padReader.go(step)
            else padReader.turnPage(step)
            return
        }
        padNav.page(step)
    }
    // ✕ / A on the focused control. HP: a synthetic Space did not press some buttons ("Continue" of a title), so a
    // button is clicked directly; a control of its own kind says how with padActivate(); Space is the last resort.
    function padActivate(f) {
        if (f && typeof f.padActivate === "function") f.padActivate()
        else if (f && typeof f.click === "function" && f.checkable !== undefined) f.click()
        else Core.sendKey(win, Qt.Key_Space)
    }
    // ○ / B: closes the open popup or menu (Escape), then steps back one level: the open reader (its sheets and
    // editor first, padBack()), the series page of Images / Text, the search, the title pages of Video
    function padBack() {
        if (win.activeFocusItem && padNav.isIn(win.activeFocusItem, Overlay.overlay)) Core.sendKey(win, Qt.Key_Escape)
        else if (win.padReader && typeof win.padReader.padBack === "function") win.padReader.padBack()
        else if (win.modeDepth > 0) win.modeClose()
        else if (win.searchOpen && win.top === "home") win.searchOpen = false
        else if (win.top !== "home") win.back()
    }
    // L2 / R2 zoom in a reader when Preferences > Controller says so ("Zoom with": triggers, the default; the right
    // stick zooms instead when picked). True when it zoomed.
    readonly property bool padZoomStick: win.padGen.pad_zoom === "stick"
    function padReaderZoom(step) {
        if (padZoomStick || !padReader || padInPopup) return false
        if (padReaderManga) padReader.zoom(step * padReader.zoomStep())
        else if (typeof padReader.padSize === "function") padReader.padSize(step)
        else return false
        return true
    }
    // the tools of the open reader: □/X editor, △/Y themes, R3 translation, Share/Select study card, L3 upscaling
    function padReaderTool(button) {
        const r = padReader
        if (!r) return false
        switch (button) {
        case "X": r.sideEdit(r); return true
        case "Y": r.sideLook(r); return true
        case "R3": r.sideTranslate(r); return true
        case "Select": if (padReaderManga) { r.sideCard(); return true } return false
        case "L3": if (padReaderManga) { r.sideUpscale(r); return true } return false
        }
        return false
    }
    // L1/R1 (LB/RB): the tabs of the video pages, in the order of the navigation
    function padTab(step) {
        if (win.images || win.texts || win.study || win.research) return
        const ids = ["discover", "library", "calendar"]
        const i = Math.max(0, ids.indexOf(win.tab))
        win.home(); win.searchOpen = false
        win.tab = ids[(i + step + ids.length) % ids.length]
    }
    // the focused card, or one of its parents, with a ⋮ menu (padMenu()), for △/Y
    function padMenuItem(it) {
        for (var i = 0; it && i < 6; ++i, it = it.parent) if (typeof it.padMenu === "function") return it
        return null
    }
    // the focus is in a popup, menu or dialog (they live in the overlay)
    readonly property bool padInPopup: !!activeFocusItem && padNav.isIn(activeFocusItem, Overlay.overlay)
    // the legend shows in the fixed sidebar on normal pages; elsewhere (popups, readers, Study, no sidebar) it floats
    readonly property bool padLegendInSidebar: sidebar.visible && !padInPopup && !study && sidebar.readerSlot === null
    // what the buttons do here. A page or popup can declare its own `controllerHints` ([{button, text}]); the
    // focused control's nearest one wins, so an open popup's list replaces the page's.
    function padDeclared(it) {
        for (var i = 0; it && i < 40; ++i, it = it.parent) if (it.controllerHints !== undefined && it.controllerHints !== null) return it.controllerHints
        return null
    }
    readonly property var playerHints: {
        const t = function(x) { return Core.tr(Core.uiLang, x) }
        return [{button: "A", text: t("Play / pause")}, {button: "B", text: t("Exit")}, {button: "Pad", text: t("Seek")},
                {button: "LB", text: t("Previous / next episode")}, {button: "RS", text: t("Seek / volume")}, {button: "Y", text: t("Subtitles and audio")},
                {button: "X", text: "X-Ray"}, {button: "Start", text: t("Settings")}]
    }
    readonly property var readerHints: {
        const t = function(x) { return Core.tr(Core.uiLang, x) }
        var h = [{button: "Pad", text: t("Turn / scroll")}, {button: "LS", text: t("Tools")},
                 {button: "RT", text: padZoomStick ? t("Page") : (padReaderManga ? t("Zoom") : t("Text size"))}, {button: "RB", text: t("Chapter")},
                 {button: "RS", text: padZoomStick ? (padReaderManga ? t("Scroll / zoom") : t("Scroll / size")) : t("Scroll")},
                 {button: "X", text: t("Editor")}, {button: "Y", text: t("Themes")}, {button: "R3", text: t("Translation")}]
        if (padReaderManga) { h.push({button: "Select", text: t("Study card")}); h.push({button: "L3", text: "Upscaling"}) }
        h.push({button: "B", text: t("Back")})
        return h
    }
    readonly property var padHints: {
        const t = function(x) { return Core.tr(Core.uiLang, x) }
        if (padKeyboard.opened) return padKeyboard.controllerHints
        if (prefs.visible && !padDeclared(activeFocusItem))
            return [{button: "LS", text: t("Move")}, {button: "A", text: t("Select")}, {button: "LB", text: t("Tabs")}, {button: "B", text: t("Close")}]
        const own = padDeclared(activeFocusItem)
        if (own) return own
        if (padPage && padPage.visible && padPage.padHints && !padInPopup) return padPage.padHints
        if (padReader && !padInPopup) return readerHints
        if (padInPopup) return [{button: "LS", text: t("Move")}, {button: "A", text: t("Select")}, {button: "B", text: t("Close")}]
        var h = [{button: "LS", text: t("Move")}, {button: "Pad", text: t("Scroll")}, {button: "A", text: t("Select")}]
        if (win.top !== "home" || win.modeDepth > 0) h.push({button: "B", text: t("Back")})
        if (win.padMenuItem(win.activeFocusItem)) h.push({button: "Y", text: t("Options")})
        if (!win.research) h.push({button: "RB", text: t("Switch tab")})
        if (!(win.images || win.texts || win.study || win.research) && !win.padMenuItem(win.activeFocusItem)) h.push({button: "Y", text: t("Search")})
        h.push({button: "RT", text: t("Page")})
        h.push({button: "Start", text: t("Settings")})
        return h
    }
    function showToast(text) { toastBox.msg = text; toastBox.opacity = 1; toastTimer.restart() }
    Connections {
        target: Core
        // A title page opened (from a card, a category, a notification…) goes on top; closing it takes it (and INFO) away.
        function onDetailChanged() {
            var d = Core.detail
            if (d.open === true) {
                var id = d.meta !== undefined ? d.meta.id : ""
                if (win.top !== "detail" && win.top !== "info") { win.stack = win.without(["detail", "info"]); win.push("detail") }
                else if (id !== win.shownDetailId && win.top === "info") { win.stack = win.without(["info"]) }
                win.shownDetailId = id
            } else if (win.stack.indexOf("detail") >= 0) {
                win.stack = win.without(["detail", "info"])
                win.shownDetailId = ""
            }
        }
    }
    Component.onCompleted: { Core.refreshPrefs(); applyTheme(); fitScale() }
    // The look of the app (Preferences > Appearance > Style): the theme file is read again when it changes.
    readonly property string wantTheme: Core.settings.theme !== undefined && Core.settings.theme !== "" ? Core.settings.theme : Theme.systemId
    onWantThemeChanged: applyTheme()
    // Apple: the sidebar floats at the left over the content; the header and the pages start after it.
    // (Research mode keeps it too: the browser's controls are a section of it, ResearchSidebar.qml)
    readonly property bool sideNav: Theme.sidebarNav && !inPlayer && !Theme.mobile
    readonly property bool rail: sideNav && Theme.layout.paneStyle === "rail"
    readonly property bool docked: sideNav && Theme.layout.paneStyle === "docked"
    // Windows: the pane is open on wide windows and compact (icons) on narrow ones; the menu button of the title bar flips it.
    property int paneMode: 0     // 0 = by width, 1 = open, 2 = compact
    // (the widths are those of the laid-out pages, so a bigger "Size in general" switches earlier)
    readonly property bool compactPane: docked && (paneMode === 2 || (paneMode === 0 && width / uiScaleF < 1000))
    // A phone (any theme), and Android under 600 px: the navigation is a bar along the bottom (BottomNav)
    readonly property bool bottomBar: Theme.mobile === true
    readonly property real sideW: !sideNav || bottomBar ? 0 : (compactPane ? Theme.spacing.paneCompactW : Theme.spacing.sidebarW + Theme.spacing.sidebarInset * 2)
    readonly property real headX: docked ? 0 : sideW
    // Text and Images readers: their buttons go into the left sidebar or rail (ReaderSidebar); null when neither is shown
    // (narrow Android: they stay in the header). It is handed down explicitly (TextPage / ImagesPage -> ModeBrowser ->
    // NovelReaderPage / MangaReaderPage `sideSlot`), so the header hides its bar exactly when the buttons are there.
    // where the Library's categories go (the sidebar under the places), null when there is no sidebar
    readonly property Item librarySideSlot: !(texts || images) || inPlayer ? null : (sidebar.librarySlot || null)
    // the reader that is open (MangaReaderPage / NovelReaderPage set it): the sidebar draws its buttons and calls it
    property Item activeReader: null
    readonly property Item readerSideSlot: !(texts || images) || modeDepth !== 2 || inPlayer ? null : (sidebar.readerSlot || railNav.readerSlot || null)
    // No bar over the content (the owner's "gain vertical space"): a title page, a series page, the Text reader with
    // its buttons in the sidebar, and the Study pages of the themes with large titles (the title shares its line with
    // the window buttons). The back / cover buttons then float over the page (AppHeader.floating).
    readonly property bool chromeFloats: !inPlayer && (
        (top === "detail" && inDetail && !images && !texts && !study && !research)
        || ((images || texts) && modeDepth === 1)
        || ((texts || images) && modeDepth === 2 && readerSideSlot !== null)
        || (study && body.largeTitle && !Theme.mobile)
        || (research && Theme.sidebarNav && Theme.layout.windowControls !== "native-right"))   // Research: the page takes the whole height
    // A tighter large title than the theme's nominal height (less empty space above the pages), tighter still in Study.
    readonly property real titleH: Math.max(0, (Theme.mobile && !(Theme.spacing.largeTitleH > 0) ? 56 : Theme.spacing.largeTitleH) - (study ? 14 : 8))
    // Study mode: in a Study page a click in the sidebar opens that page (Decks, Learn, Time) and leaves a review.
    function modeTab(id) {
        if (win.study) { Core.cmd({cmd: "study_close"}); win.needStudy().go(id) }
        else win.mtab = id
    }
    function applyTheme() {
        if (Theme.id !== wantTheme) Theme.load(wantTheme)
        Theme.apply(Core.settings.colorScheme !== undefined ? Core.settings.colorScheme : "system", Core.settings.accent !== undefined ? Core.settings.accent : "")
    }

    // The video lives under every page. The pages cover it; the player page lets it show.
    // It is drawn at its own pixel size, so it is kept out of "Size in general" (scaled up, the picture would blur).
    VideoItem { width: parent.width * win.uiScaleF; height: parent.height * win.uiScaleF; scale: 1 / win.uiScaleF; transformOrigin: Item.TopLeft; z: -2 }
    // everything under the floating panels: the window colour, the soft ambient colours (Apple) and the pages
    Item {
        id: stage
        anchors.fill: parent
        z: -1
        visible: !win.inPlayer
        Component.onCompleted: Theme.backdrop = stage
        Rectangle { anchors.fill: parent; color: Theme.c.window }
        Aurora {
            anchors.fill: parent
            visible: Theme.glass && Theme.layout.aurora === true
            opacity: 0.7
            enabledLook: visible
            colours: ["#0a84ff", "#5e5ce6", "#bf5af2", "#30d5c8"]
            motion: Core.prefs.general !== undefined && Core.prefs.general.aurora_motion === true
        }
        // controller mode: the orbs of time (PS2) behind the pages
        OrbsBackground {
            anchors.fill: parent
            visible: win.padMode && win.padGen.pad_orbs !== false && !win.inPlayer
        }

    // ---- pages ----
        // Windows: the content sits on a layer with a rounded top-left corner next to the pane
        Rectangle {
            visible: win.docked && Theme.layout.contentLayer === true
            anchors { left: parent.left; leftMargin: win.sideW; right: parent.right; bottom: parent.bottom; top: parent.top; topMargin: win.chromeFloats ? 0 : header.height }
            color: Theme.c.layer
            topLeftRadius: Theme.shape.large
            border.width: 1
            border.color: Theme.c.border
        }
        Item {
            id: body
            anchors { left: parent.left; leftMargin: win.sideW + (win.bottomBar ? (win.safe.left || 0) : 0); right: parent.right; rightMargin: win.bottomBar ? (win.safe.right || 0) : 0; bottom: parent.bottom; bottomMargin: win.bottomBar ? bottomNav.height : 0; top: parent.top; topMargin: (win.chromeFloats ? (win.bottomBar ? (win.safe.top || 0) : 0) : header.height) + (searchBar.visible ? searchBar.height : 0) }
            // Apple: a large title over the pages of the sidebar (Discover, Library, Calendar; Library, Explore, History).
            // A phone: every theme has the large title (Material "top app bar, large", iOS large title).
            readonly property bool largeTitle: (Theme.layout.largeTitles === true || Theme.mobile) && win.top === "home" && win.modeDepth === 0 && win.tab !== "search" && !win.studySession && !win.research
            readonly property string sectionTitle: {
                const t = Core.uiLang
                if (win.study) return Core.tr(t, "Study")
                if (win.images || win.texts) return ({library: Core.tr(t, "Library"), explore: Core.tr(t, "Explore"), history: Core.tr(t, "History")})[win.mtab] || ""
                return ({discover: Core.tr(t, "Discover"), library: Core.tr(t, "Library"), calendar: Core.tr(t, "Calendar")})[win.tab] || ""
            }
            Text {
                visible: body.largeTitle
                x: Theme.mobile ? 16 : Theme.spacing.page
                y: 0
                height: win.titleH
                // when the bar floats, the window buttons sit at the right end of this line
                width: parent.width - x - (win.chromeFloats && header.captions ? 150 : Theme.spacing.page)
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
                text: body.sectionTitle
                color: Theme.c.text
                font.pixelSize: Theme.mobile && Theme.type.mobile !== undefined && Theme.type.mobile.largeTitle !== undefined ? Theme.type.mobile.largeTitle : (Theme.type.largeTitle !== undefined ? Theme.type.largeTitle.size : 30)
                font.weight: Theme.type.largeTitle !== undefined ? Theme.type.largeTitle.weight : Font.Bold
                font.letterSpacing: Theme.type.largeTitle !== undefined ? (Theme.type.largeTitle.letterSpacing || 0) : 0
                font.family: Theme.type.display || Theme.type.family
            }
            // A page that is not the start page is built the first time it is shown and then kept (Lazy): a phone has
            // little memory for all of them at once, and the start is quicker everywhere. Discover and the Library are
            // built at once (the start page and the one beside it); Study and Research keep their ids (Main calls them).
            component Lazy: Loader {
                property bool shown: false
                anchors.fill: parent
                visible: shown
                active: false
                onShownChanged: if (shown && !active) active = true
                Component.onCompleted: if (shown) active = true
            }
            readonly property bool videoHome: !win.images && !win.texts && !win.study && !win.research
            Item {
                id: host
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom; top: parent.top; topMargin: body.largeTitle ? win.titleH : 0 }
                DiscoverPage { anchors.fill: parent; visible: win.tab === "discover" && win.top === "home" && body.videoHome; platform: win.platform }
                LibraryPage { anchors.fill: parent; visible: win.tab === "library" && win.top === "home" && body.videoHome }
                Lazy { shown: win.tab === "calendar" && win.top === "home" && body.videoHome; sourceComponent: CalendarPage { } }
                Lazy { shown: win.tab === "search" && win.top === "home" && body.videoHome; sourceComponent: SearchPage { } }
                Lazy { shown: win.top === "genre" && body.videoHome; sourceComponent: GenrePage { } }
                Lazy { shown: win.top === "detail" && win.inDetail && body.videoHome; sourceComponent: DetailPage { } }
                Lazy { shown: win.top === "info" && win.inDetail && body.videoHome; sourceComponent: InfoPage { } }
                // Research (the browser) and Study: built the first time their mode opens, over a few frames
                // (asynchronous, with a light placeholder meanwhile), then kept. win.needResearch / needStudy build
                // them at once when something calls into them before that (a link sent to Research, a test).
                Loader {
                    id: researchHost
                    anchors.fill: parent
                    visible: win.research
                    active: false
                    asynchronous: true
                    readonly property bool want: win.research
                    onWantChanged: if (want) active = true
                    Component.onCompleted: if (want) active = true
                    sourceComponent: ResearchPage { }
                }
                Loader {
                    id: studyHost
                    anchors.fill: parent
                    visible: win.study
                    active: false
                    asynchronous: true
                    readonly property bool want: win.study
                    onWantChanged: if (want) active = true
                    Component.onCompleted: if (want) active = true
                    sourceComponent: StudyPage { }
                }
                BusyIndicator {
                    anchors.centerIn: parent
                    visible: running
                    running: (win.research && researchHost.status !== Loader.Ready) || (win.study && studyHost.status !== Loader.Ready)
                }
                Lazy { shown: win.images; sourceComponent: ImagesPage { mtab: win.mtab; searchTick: win.modeSearch; slot: header.actionSlot; sideSlot: win.images ? win.readerSideSlot : null; catSlot: win.images ? win.librarySideSlot : null } }
                Lazy { shown: win.texts; sourceComponent: TextPage { mtab: win.mtab; searchTick: win.modeSearch; slot: header.actionSlot; sideSlot: win.texts ? win.readerSideSlot : null; catSlot: win.texts ? win.librarySideSlot : null } }
            }
        }
    }

    AppHeader {
        id: header
        visible: !win.inPlayer
        anchors { left: parent.left; leftMargin: win.headX; right: parent.right; top: parent.top }
        z: 5
        window: win
        floating: win.chromeFloats
        // the Text reader has its way back in the sidebar
        floatBack: !((win.texts && win.modeDepth === 2) || (win.images && win.modeDepth === 2 && win.readerSideSlot !== null))
        paneW: win.docked ? win.sideW : 0
        // GNOME has no large titles: in Study the bar names the mode (the pages then start right under it)
        barTitle: win.study && Theme.layout.largeTitles !== true ? Core.tr(Core.uiLang, "Study") : ""
        tab: win.tab === "search" ? "discover" : win.tab
        mtab: win.mtab
        depth: win.modeDepth
        subTitle: win.modeSub
        onModeBack: win.modeClose()
        onModeTabSelected: function(id) { win.modeTab(id) }
        page: win.top
        title: win.modeDepth === 2 ? (mst.reader.title || "") : (win.modeDepth === 1 ? ((mst.view.series || ({})).title || "") : (Core.detail.meta !== undefined ? Core.detail.meta.name : ""))
        platform: win.platform
        platforms: {
            var rows = Core.home.rows !== undefined ? Core.home.rows : []
            var names = []
            for (var i = 0; i < rows.length; ++i) if (names.indexOf(rows[i].platform) < 0) names.push(rows[i].platform)
            if (win.platform !== "" && names.indexOf(win.platform) < 0) names.push(win.platform)
            return names.sort()
        }
        onTabSelected: function(id) { win.home(); win.tab = id; win.searchOpen = false }
        onBack: win.back()
        onPaneToggle: win.paneMode = win.compactPane ? 1 : 2
        onToggleSearch: {
            if (win.images || win.texts) { win.modeSearch++; return }
            win.searchOpen = !win.searchOpen
            if (win.searchOpen) searchField.forceActiveFocus()
        }
        onSearchRequested: function(text) { win.home(); win.tab = "search"; win.searchOpen = true; searchField.text = text; Core.cmd({cmd: "search", q: text}) }
        onPlatformChosen: function(name) { win.home(); win.platform = name; win.tab = "discover" }
        onCategoryChosen: function(name, kind) { win.openGenre(0, name, kind) }
        onPrefsRequested: win.openPrefs("general")
        onAboutRequested: about.open()
        onArtRequested: artDialog.open()
    }

    // search bar that slides under the header (Ctrl+F)
    Rectangle {
        id: searchBar
        visible: win.searchOpen && win.top === "home" && !win.inPlayer && !win.images && !win.texts && !win.study && !win.research
        anchors { left: parent.left; leftMargin: win.headX; right: parent.right; top: header.bottom }
        height: Theme.mobile ? 60 : 52
        color: Theme.c.headerbar
        z: 4
        TextField {
            id: searchField
            // Android: no word suggestions in search fields. The keyboard's "composing" word lost its place in the field
            // (typing "pokemon" gave "okemonp"); with suggestions off each letter goes straight in.
            inputMethodHints: Qt.platform.os === "android" ? Qt.ImhNoPredictiveText : Qt.ImhNone
            anchors.centerIn: parent
            width: Math.min(640, parent.width - (Theme.mobile ? 32 : 60))
            height: Theme.mobile ? 44 : 36
            placeholderText: Core.tr(Core.uiLang, "Search movies, series and anime")
            color: Theme.c.text
            placeholderTextColor: Theme.c.textDim
            font.pixelSize: 16
            font.family: Theme.type.family
            selectByMouse: true
            leftPadding: 38
            background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: searchField.activeFocus ? 2 : 0; border.color: Theme.c.accent }
            Icon { name: "search"; size: 16; color: Theme.c.textDim; anchors { left: parent.left; leftMargin: 11; verticalCenter: parent.verticalCenter } }
            onAccepted: if (text.trim() !== "") { win.tab = "search"; Core.cmd({cmd: "search", q: text.trim()}) }
            Keys.onEscapePressed: win.searchOpen = false
        }
    }
    Shortcut { sequence: "Ctrl+F"; onActivated: { if (win.research && !win.inPlayer) { win.needResearch().focusAddress(); return } if (win.images || win.texts) { win.modeSearch++; return } if (win.rail) { win.searchOpen = true; searchField.forceActiveFocus(); return } if (win.sideNav) { sidebar.focusSearch(); return } win.searchOpen = true; searchField.forceActiveFocus() } }

    // A phone (and a narrow Android window): the navigation bar along the bottom (BottomNav.qml)
    BottomNav {
        id: bottomNav
        visible: win.bottomBar && !win.inPlayer
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        z: 6
        tab: win.tab === "search" ? "discover" : win.tab
        mtab: win.mtab
        platform: win.platform
        platforms: header.platforms
        studyView: win.studyPage ? win.studyPage.view : "decks"
        onTabSelected: function(id) { win.home(); win.tab = id; win.searchOpen = false }
        onModeTabSelected: function(id) { win.modeTab(id) }
        onSearchToggled: { win.searchOpen = !win.searchOpen; if (win.searchOpen) searchField.forceActiveFocus() }
        onPlatformChosen: function(name) { win.home(); win.platform = name; win.tab = "discover" }
        onCategoryChosen: function(name, kind) { win.openGenre(0, name, kind) }
        onPrefsRequested: win.openPrefs("general")
        onAboutRequested: about.open()
    }

    // Android: the navigation rail
    RailNav {
        id: railNav
        visible: (win.rail && !win.bottomBar) || win.compactPane
        x: 0
        y: win.compactPane ? header.height : 0
        width: win.compactPane ? Theme.spacing.paneCompactW : Theme.spacing.sidebarW
        height: parent.height - y
        z: 6
        window: win
        tab: win.tab === "search" ? "discover" : win.tab
        mtab: win.mtab
        platform: win.platform
        platforms: header.platforms
        studyView: win.studyPage ? win.studyPage.view : "decks"
        onTabSelected: function(id) { win.home(); win.tab = id; win.searchOpen = false }
        onModeTabSelected: function(id) { win.modeTab(id) }
        onSearchToggled: { win.searchOpen = !win.searchOpen; if (win.searchOpen) searchField.forceActiveFocus() }
        onPlatformChosen: function(name) { win.home(); win.platform = name; win.tab = "discover" }
        onCategoryChosen: function(name, kind) { win.openGenre(0, name, kind) }
        onPrefsRequested: win.openPrefs("general")
        onAboutRequested: about.open()
    }

    SidebarNav {
        id: sidebar
        visible: win.sideNav && !win.rail && !win.compactPane
        x: Theme.spacing.sidebarInset
        y: win.docked ? header.height : Theme.spacing.sidebarInset
        width: Theme.spacing.sidebarW
        height: parent.height - y - Theme.spacing.sidebarInset
        z: 6
        window: win
        tab: win.tab === "search" ? "discover" : win.tab
        mtab: win.mtab
        platform: win.platform
        platforms: header.platforms
        studyView: win.studyPage ? win.studyPage.view : "decks"
        onTabSelected: function(id) { win.home(); win.tab = id; win.searchOpen = false }
        onModeTabSelected: function(id) { win.modeTab(id) }
        onSearchRequested: function(text) { win.home(); win.tab = "search"; win.searchOpen = false; Core.cmd({cmd: "search", q: text}) }
        onPlatformChosen: function(name) { win.home(); win.platform = name; win.tab = "discover" }
        onCategoryChosen: function(name, kind) { win.openGenre(0, name, kind) }
        onPrefsRequested: win.openPrefs("general")
        onAboutRequested: about.open()
    }

    PlayerPage { id: playerItem; anchors.fill: parent; visible: win.inPlayer; z: 6; window: win }
    // The feedback note, one for the whole app: opened by the button in the fixed sidebar (SidebarNav), in the rail
    // and in the player's bar (PlayerPage); the X-Ray header keeps its own. `extra` is added to where the person was.
    function openFeedback(near, extra) {
        if (feedbackBox.opened) { feedbackBox.close(); return }
        feedbackBox.near = near
        feedbackBox.place = (win.inPlayer ? "player" : (Core.manga.mode || "video") + " / " + win.top + (win.modeDepth > 0 ? " / depth " + win.modeDepth : "")) + (extra ? " / " + extra : "")
        feedbackBox.open()
    }
    FeedbackBox { id: feedbackBox }
    // No internet, or the user's "Offline mode": a pill at the top; downloaded content keeps opening (OfflinePill.qml)
    OfflinePill {
        anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: (header.visible ? header.height : 0) + 8 }
        z: 8
        visible: shown && !win.inPlayer
    }
    readonly property bool feedbackOpen: feedbackBox.opened
    // The macaw's cues (Core.mascotCue: shh, peek, cheer…), above the pages and the player; it keeps clear of the
    // player's controls at the bottom.
    MascotStage { anchors.fill: parent; z: 9; bottomInset: win.inPlayer ? 110 : 24 + (win.bottomBar ? bottomNav.height : 0) }

    // Work in the background (GTK tasks.rs), bottom left; hidden in the player.
    TaskPanel {
        anchors { left: parent.left; bottom: parent.bottom; margins: 14; bottomMargin: 14 + (win.bottomBar ? bottomNav.height : 0) }
        z: 8
        visible: items.length > 0 && !win.inPlayer
    }

    PrefsDialog { id: prefs }
    AboutDialog { id: about }
    KaraokeStudio { id: studioTest }
    ColorPickerAdapter { id: pickerTest; withAlpha: true }
    ArtDialog { id: artDialog }

    // The night filter (GTK look::red_layer): a warm layer over everything that lets clicks through.
    Rectangle { anchors.fill: parent; z: 50; visible: win.nightFilter > 0; color: Qt.rgba(1.0, 0.55, 0.08, win.nightFilter / 100 * 0.40); enabled: false }

    // ---- resize edges of the frameless window ----
    // In the overlay layer, so nothing (header, sidebar, video, open dialogs) can sit on top of them. A DragHandler hands
    // the drag to the system (Qt's pattern for frameless windows): no stuck press, works on Windows and Wayland.
    Item {
        id: resizeLayer
        parent: Overlay.overlay
        anchors.fill: parent
        z: 10000
        readonly property int e: 6       // edge thickness
        readonly property int k: 14      // corner size
        readonly property bool on: win.visibility !== Window.Maximized && win.visibility !== Window.FullScreen && !win.phoneOs
        Repeater {
            model: [
                {e: Qt.LeftEdge, x: 0, y: 1, w: 0, h: -2, c: Qt.SizeHorCursor},
                {e: Qt.RightEdge, x: -1, y: 1, w: 0, h: -2, c: Qt.SizeHorCursor},
                {e: Qt.TopEdge, x: 1, y: 0, w: -2, h: 0, c: Qt.SizeVerCursor},
                {e: Qt.BottomEdge, x: 1, y: -1, w: -2, h: 0, c: Qt.SizeVerCursor},
                {e: Qt.LeftEdge | Qt.TopEdge, x: 0, y: 0, w: 1, h: 1, c: Qt.SizeFDiagCursor},
                {e: Qt.RightEdge | Qt.BottomEdge, x: -1, y: -1, w: 1, h: 1, c: Qt.SizeFDiagCursor},
                {e: Qt.RightEdge | Qt.TopEdge, x: -1, y: 0, w: 1, h: 1, c: Qt.SizeBDiagCursor},
                {e: Qt.LeftEdge | Qt.BottomEdge, x: 0, y: -1, w: 1, h: 1, c: Qt.SizeBDiagCursor}
            ]
            // w/h: 0 = edge thickness, 1 = corner size, -2 = the length between the corners. x/y: -1 = at the far side.
            delegate: Item {
                required property var modelData
                visible: resizeLayer.on
                readonly property real ww: modelData.w === 0 ? resizeLayer.e : (modelData.w === 1 ? resizeLayer.k : resizeLayer.width - 2 * resizeLayer.k)
                readonly property real hh: modelData.h === 0 ? resizeLayer.e : (modelData.h === 1 ? resizeLayer.k : resizeLayer.height - 2 * resizeLayer.k)
                width: ww; height: hh
                x: modelData.x < 0 ? resizeLayer.width - ww : (modelData.x === 1 ? resizeLayer.k : 0)
                y: modelData.y < 0 ? resizeLayer.height - hh : (modelData.y === 1 ? resizeLayer.k : 0)
                HoverHandler { cursorShape: modelData.c }
                DragHandler {
                    target: null
                    grabPermissions: PointerHandler.CanTakeOverFromAnything
                    onActiveChanged: if (active) win.startSystemResize(modelData.e)
                }
                // A plain press also starts it at once (no drag threshold), as most desktops do.
                TapHandler {
                    gesturePolicy: TapHandler.DragThreshold
                    onPressedChanged: if (pressed) win.startSystemResize(modelData.e)
                }
            }
        }
    }

    // ---- messages from the core ----
    Rectangle {
        id: toastBox
        property string msg: ""
        opacity: 0
        // Above the dialogs too (a key already in use, "Copied"…).
        parent: Overlay.overlay
        z: 1000
        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 40 + (win.bottomBar && !win.inPlayer ? bottomNav.height : (win.safe.bottom || 0)) }
        width: Math.min(parent.width - 60, toastText.implicitWidth + 72)
        height: 46
        radius: 23
        color: "#e6303034"
        border.color: Theme.c.border
        Text { id: toastText; anchors.centerIn: parent; width: parent.width - 60; elide: Text.ElideRight; text: toastBox.msg; color: "#ffffff"; font.pixelSize: 15; font.family: Theme.type.family; horizontalAlignment: Text.AlignHCenter }
        Behavior on opacity { NumberAnimation { duration: Theme.motion.normal } }
        Timer { id: toastTimer; interval: 5000; onTriggered: toastBox.opacity = 0 }
    }
    // Phone-style pop-ups with the macaw (study reminders, Pomodoro…), above the dialogs and the player too.
    BannerHost {
        id: bannerHost
        parent: Overlay.overlay
        z: 1001
        anchors.fill: parent
        overVideo: win.inPlayer
    }
    Connections {
        target: Core
        // Every banner (from the core or born in QML, like the Pomodoro's) is also kept in the bell, with its macaw,
        // time and click action, so one that went by unseen can still be read.
        function onBanner(b) {
            bannerHost.show(b)
            Core.cmd({cmd: "note_banner", kind: b.kind || "info", title: b.title || "", body: b.body || "", mood: b.mood || "idle", action: b.action || null})
        }
    }
    Connections {
        target: Core
        function onToast(text) { win.showToast(Core.tr(Core.uiLang, text)) }
        // A second start of Reelhouse, or a link to a title, brings the window up (GTK Event::Raise).
        function onRaiseWindow() { win.show(); win.raise(); win.requestActivate() }
    }
    Shortcut {
        sequence: "Escape"
        // In the player, Escape is the configurable "back" key; in a study review StudyPage closes the session with it.
        enabled: !win.inPlayer && !win.studySession && !(win.studyPage && win.studyPage.learnLesson)
        onActivated: {
            if (win.visibility === Window.FullScreen) win.showNormal()
            else if (win.inPlayer) Core.cmd({cmd: "close_player"})
            else if (win.top !== "home") win.back()
            else win.searchOpen = false
        }
    }
    Shortcut { sequence: "F11"; onActivated: win.visibility === Window.FullScreen ? win.showNormal() : win.showFullScreen() }
    Shortcut { sequence: "Ctrl+Q"; onActivated: Qt.quit() }
    // Game controller (GTK gui/gamepad.rs): in the player the buttons do the player actions; elsewhere the pad moves
    // the focus (Tab / Shift+Tab), presses the focused control (A) and goes back (B), like the GTK version.
    Connections {
        target: Core
        function onPadConnection(connected, kind, name) {
            const list = win.pads.filter(function(p) { return p.name !== name })
            if (connected) list.push({kind: kind, name: name})
            win.pads = list
            // a stick value left from before the change must not keep scrolling or zooming
            padNav.rsX = 0; padNav.rsY = 0; padNav.dpadX = 0; padNav.dpadY = 0
            padToast.show(connected, kind, name)
        }
        function onPadPressed(button) {
            // the right stick ("RS:<y>", "RSX:<x>"): PadNavigator scrolls, seeks / sets the volume in the player, and
            // scrolls / zooms the open reader
            if (button.indexOf("RS") === 0) { padNav.stick(button); if (!win.inPlayer && button.slice(-5) !== ":0.00") win.padWake(); return }
            // HP: the left stick ("LSLeft" …) moves the focus through menus and buttons (the sidebar, the modes), the
            // D-pad moves the page's content. Where arrows only have one meaning (the player, Preferences, a popup, the
            // on-screen keyboard) the stick is the same as the D-pad.
            const lsm = /^(~?)LS(Left|Right|Up|Down)$/.exec(button)
            const stick = !!lsm
            if (lsm) button = lsm[1] + lsm[2]
            if (win.inPlayer) {
                // ✕/A play-pause, ○/B exit, ←/→ seek, ↑/↓ volume, L1/R1 episodes, △/Y subtitles and audio, □/X X-Ray,
                // L2/R2 one minute, Options/Menu settings (PlayerPage shows these with the OSD)
                if (button.charAt(0) === "~") return
                // Preferences over the player: ○/B and Options close it (○/B must not leave the video)
                if (prefs.visible) {
                    if (button === "B" || button === "Start") prefs.close()
                    else if (button === "LB" || button === "RB") prefs.padTab(button === "RB" ? 1 : -1)
                    else if (button === "A") Core.sendKey(win, Qt.Key_Space)
                    else if (["Left", "Right", "Up", "Down"].indexOf(button) >= 0) padNav.move(button)
                    return
                }
                if (button === "Start") { win.openPrefs("general"); return }
                if (button === "A" && win.padGen.pad_sound !== false && uiSounds.item) uiSounds.item.select()
                var acts = {A: "play_pause", B: "back", X: "xray", Y: "tracks_menu", Left: "seek_back", Right: "seek_forward",
                            Up: "volume_up", Down: "volume_down", LB: "prev_episode", RB: "next_episode",
                            LT: "seek_back_big", RT: "seek_forward_big", Select: "mute"}
                if (acts[button] !== undefined) playerItem.action(acts[button])
                return
            }
            win.padPressedAt = Date.now()
            if (button.charAt(0) === "~") { padNav.release(button.slice(1)); padNav.contentRelease(button.slice(1)); return }
            win.padWake()
            // the on-screen keyboard first, then Preferences, a page with its own buttons (the grades of a review), the
            // open reader, and the rest of the app
            if (padKeyboard.opened && padKeyboard.padButton(button)) return
            if (button === "Start") { if (prefs.visible) prefs.close(); else win.openPrefs("general"); return }
            // ○/B in Preferences: closes it (HP: its close did nothing), or first a list or dialog open over it
            if (prefs.visible && button === "B") {
                const fi = win.activeFocusItem
                if (fi && win.padInPopup && !padNav.isIn(fi, prefs.contentItem) && fi !== prefs.contentItem.parent) Core.sendKey(win, Qt.Key_Escape)
                else prefs.close()
                return
            }
            if (!prefs.visible && !win.padInPopup && win.padPage && win.padPage.visible && win.padPage.padButton(button)) {
                if (win.padGen.pad_sound !== false && uiSounds.item) uiSounds.item.select()
                return
            }
            if (win.padReader && !win.padInPopup) {
                if (win.padReaderTool(button)) return
                const onTool = win.activeFocusItem && win.activeFocusItem.padSideTool === true
                if ((button === "Left" || button === "Right") && !onTool && !stick) {
                    if (win.padReaderManga) win.padReader.side(button === "Left")
                    else win.padReader.turnPage(button === "Right" ? 1 : -1)
                    return
                }
            }
            switch (button) {
            case "LT": if (!win.padReaderZoom(-1)) win.padPageTurn(-1); break
            case "RT": if (!win.padReaderZoom(1)) win.padPageTurn(1); break
            case "LB": win.padSection(-1); break
            case "RB": win.padSection(1); break
            case "Y":
                if (win.padMenuItem(win.activeFocusItem)) { win.padMenuItem(win.activeFocusItem).padMenu(); break }
                if (!(win.images || win.texts || win.study || win.research)) {
                    win.home(); win.searchOpen = true; searchField.forceActiveFocus()
                }
                break
            case "Left": case "Up": case "Right": case "Down":
                if (stick || win.padInPopup || prefs.visible) padNav.press(button)
                else padNav.contentPress(button)
                break
            case "A": {
                if (win.padGen.pad_sound !== false && uiSounds.item) uiSounds.item.select()
                const f = win.activeFocusItem
                // a text box: the on-screen keyboard (PadKeyboard) to type in it
                if (f && f.cursorPosition !== undefined && f.insert !== undefined && f.readOnly !== true) { padKeyboard.openFor(f); break }
                win.padActivate(f)
                break
            }
            case "B": win.padBack(); break
            }
        }
    }
    Shortcut { sequence: "Ctrl+,"; onActivated: win.openPrefs("general") }
    Shortcut { sequences: ["Ctrl+=", "Ctrl++"]; onActivated: win.zoom(10) }
    Shortcut { sequence: "Ctrl+-"; onActivated: win.zoom(-10) }
    Shortcut { sequence: "Ctrl+0"; onActivated: win.zoom(0) }
    // The back button of the mouse goes back, like in the GTK version.
    MouseArea {
        anchors.fill: parent
        z: 40
        acceptedButtons: Qt.BackButton | Qt.ForwardButton
        enabled: !win.inPlayer && !win.research   // Research mode: back and forward of the browser (ResearchPage)
        onClicked: function(m) { if (m.button === Qt.ForwardButton) win.forward(); else if (win.top !== "home") win.back() }
    }
    // Back and forward keys (browser keys of keyboards and mice) and Alt+arrows.
    Shortcut { sequences: ["Alt+Left", "Back"]; enabled: !win.inPlayer && !win.research; onActivated: if (win.top !== "home") win.back() }
    Shortcut { sequences: ["Alt+Right", "Forward"]; enabled: !win.inPlayer && !win.research; onActivated: win.forward() }

    // ---- controller mode: what the buttons do here (✕ ○ □ △ or A B X Y) ----
    // Pages with buttons or content along the bottom (a review with its grades, a reader, the on-screen keyboard, a
    // page that asks with `padHintsTop`, like Conversation) get it at the top right, under the header; elsewhere it sits at the bottom centre. It fades after a few seconds
    // without a press.
    PadHints {
        id: padHintBar
        parent: Overlay.overlay
        z: 8999
        // the fixed sidebar shows the same list at its bottom
        visible: win.padMode && !win.inPlayer && (!win.padLegendInSidebar || padKeyboard.opened)
        kind: win.padKind
        hints: win.padHints
        idle: win.padIdle
        readonly property bool pageTop: win.padPage !== null && win.padPage.visible && win.padPage.padHintsTop === true && !win.padInPopup
        readonly property bool atTop: win.studySession || win.padReader !== null || padKeyboard.opened || pageTop
        readonly property real topY: pageTop && win.padPage.padHintsY > 0 ? win.padPage.padHintsY : (win.chromeFloats ? 12 : header.height + 8)
        columns: parent && implicitWidth > parent.width - 48 ? Math.ceil(hints.length / 2) : 0
        x: parent ? (atTop ? parent.width - width - 16 : (parent.width - width) / 2) : 0
        y: parent ? (atTop ? topY : parent.height - height - 16 - (win.bottomBar ? bottomNav.height : 0)) : 0
    }

    // ---- "Connected" card of a game controller, with its model ----
    Rectangle {
        id: padToast
        property bool connected: true
        property string kind: "pad"
        property string name: ""
        function show(c, k, n) { connected = c; kind = k; name = n; opacity = 1; padTimer.restart() }
        parent: Overlay.overlay
        z: 9000
        opacity: 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: Theme.motion.normal || 200 } }
        anchors.horizontalCenter: parent ? parent.horizontalCenter : undefined
        y: 18 + (1 - opacity) * -12
        width: padRow.implicitWidth + 32
        height: 64
        radius: Theme.layout.dialog === "sheet" ? 20 : (Theme.shape.card || 12)
        color: Theme.c.popover
        border.width: 1
        border.color: Theme.c.border
        Row {
            id: padRow
            anchors.centerIn: parent
            spacing: 12
            PadBadge { kind: padToast.kind; compact: true; scale: 1.4; anchors.verticalCenter: parent.verticalCenter }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text { text: padToast.connected ? Core.tr(Core.uiLang, "Connected") : Core.tr(Core.uiLang, "Disconnected"); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
                Text { text: padToast.name; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family; elide: Text.ElideRight; width: Math.min(implicitWidth, 320) }
            }
            PadBadge { kind: padToast.kind; anchors.verticalCenter: parent.verticalCenter }
        }
        Timer { id: padTimer; interval: 3500; onTriggered: padToast.opacity = 0 }
    }

    // The loading screen of the start (the macaw with popcorn): over everything until the core has sent its first data.
    LoadingScreen { parent: Overlay.overlay; anchors.fill: parent; z: 100000 }
}
