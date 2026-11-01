import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "Osd.js" as Osd

// X-Ray (like Prime Video) over the left side of the video. It follows the playback clock that mpv reports
// (Core.playbackEvent: time-pos and pause): it opens by itself on pause and closes on play, and opens while the
// pointer is over its tab. It shows what covers the current time: the cast and characters (with voice actors for
// anime), the song playing, the scene (chapter) and trivia; "All" lists the whole cast.
// The header picks where the cast comes from (automatic, TMDB, MyAnimeList, IMDb) and fetches it again.
// The openings, endings and trailers play inside the panel (a second mpv in the core; ThemeVideo shows its pictures)
// while the episode waits. Trailers are never listed as songs: they have a small row of their own under "All".
// "Who's in this scene?" sends the frame on screen to Google Lens (LensProbe.qml) and the core matches its answer
// with the cast (xray_lens): the people found head "In this scene". Only on that button, or by itself on open when
// the viewer turned that on (general.xray_lens_auto): the frame leaves the computer.
// Above the X-Ray tab sits the globe of the quick search: hovering it a moment (or clicking it) opens the search bar.
// The look follows the theme's player style (Theme.layout.player: adw, glass, fluent, m3) on dark tones, like the
// player's bottom bar: the theme gives the accent, the radii and the font.
Item {
    id: xr
    readonly property var x0: Core.xray || ({})
    readonly property string status: x0.status || "none"
    readonly property var items: x0.data && x0.data.items ? x0.data.items : []
    property real pos: 0
    property bool paused: false
    // the player hides the edge buttons (the X-Ray tab and the globe) while it plays with its bar hidden
    property bool edgeShown: true
    property bool pinned: false            // opened with the tab or the button: stays until closed
    property bool autoOpened: false
    property string tab: "scene"           // scene | cast | music | trivia
    readonly property bool open: pinned || autoOpened || hoverOpen
    property bool hoverOpen: false
    property bool autoOnPause: true
    // the quick search (QuickSearch.qml in the player): its globe sits above the X-Ray tab
    property bool searchOpen: false
    property bool searchEnabled: true
    signal searchAsked(bool toggle)
    // Something else paused the video on purpose (the clip Editor, a study question): no opening on that pause.
    property bool holdAuto: false
    readonly property bool hasData: status === "ready" && items.length > 0
    // the source chosen in the header; the core keeps it in the settings (general.xray_source)
    readonly property string source: x0.source || "auto"
    property string pickedSource: ""
    readonly property string wantSource: pickedSource !== "" ? pickedSource : source
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function fmt(t) { t = Math.max(0, Math.floor(t)); const m = Math.floor(t / 60), s = t % 60; return m + ":" + (s < 10 ? "0" : "") + s }
    // (a song with no known place in the episode, end_time 1e9, is listed under Music but never "playing now")
    function at(kind) { return items.filter(function(i) { return i.type === kind && xr.pos >= i.start_time && xr.pos < i.end_time && !(kind === "music" && i.end_time >= 1e8) }) }
    function all(kind) { return items.filter(function(i) { return i.type === kind }) }
    // The lists shown are replaced only when what they hold changes: the clock and the X-Ray state arrive several
    // times a second, and a new array every time would rebuild the rows (lost hover, scroll jumps).
    property var nowCast: []
    property var nowMusic: []
    property var nowScene: []
    property var nowTrivia: []
    property var allCast: []
    property var allMusic: []
    property var allTrivia: []
    property var allTrailers: []
    property var lensHits: []
    function keep(name, v) { if (JSON.stringify(v) !== JSON.stringify(xr[name])) xr[name] = v }
    function refreshNow() { keep("nowCast", at("cast")); keep("nowMusic", at("music")); keep("nowScene", at("scene")); keep("nowTrivia", at("trivia")); refreshLens() }
    function refreshAll() { keep("allCast", all("cast")); keep("allMusic", all("music")); keep("allTrivia", all("trivia")); keep("allTrailers", all("trailer")) }
    onPosChanged: refreshNow()
    onItemsChanged: { refreshNow(); refreshAll() }
    onX0Changed: refreshLens()
    onSourceChanged: pickedSource = ""

    function fetchNow() { Core.cmd({cmd: "xray_fetch", source: xr.wantSource }); xr.pickedSource = "" }

    // ---- the video of the openings / endings (state read from the core while the panel is open) ----
    property var tv: ({})
    readonly property bool tvActive: (tv.status || "") !== ""
    function refreshTv() { const v = Core.state("theme_video") || ({}); if (JSON.stringify(v) !== JSON.stringify(xr.tv)) xr.tv = v }
    // The card of the player (the opening / ending playing now) opens the panel on its song and plays it here.
    function openTheme(t) {
        xr.pinned = true
        xr.tab = "music"
        const c = {cmd: "theme_play"}
        if (t) { c.kind = t.kind || ""; c.url = t.url || ""; c.song = t.song || ""; c.artist = t.artist || ""; c.number = t.number || 0; c.title = t.title || "" }
        startVideo(c)
    }
    // A row of the Music list: an opening / ending of MyAnimeList is searched on YouTube; one found on YouTube already
    // (and a trailer) has its address in `text`.
    function playSong(it) {
        const tag = it.tag || ""
        const url = String(it.text || "")
        if (it.type === "trailer" || tag === "Trailer") { startVideo({cmd: "theme_play", url: url, song: it.name || "", label: "Trailer"}); return }
        const parts = tag.split(" ")
        const kind = parts[0] === "Opening" ? "op" : (parts[0] === "Ending" ? "ed" : "")
        if (url.indexOf("http") === 0) { startVideo({cmd: "theme_play", kind: kind, url: url, title: it.name || "", song: it.name || "", label: tag}); return }
        startVideo({cmd: "theme_play", kind: kind, song: it.name || "", artist: it.role || "", number: parseInt(parts[1]) || 0})
    }
    // "Born: 1964-09-02" from the core, its label in the language of the interface
    function factLine(f) { const i = String(f).indexOf(": "); return i > 0 ? tr(f.substring(0, i)) + f.substring(i) : String(f) }
    function startVideo(c) { tvView.clearFrame(); Core.cmd(c); refreshTv() }
    // "Opening 2" / "Trailer" from the core, in the language of the interface
    function tlabel(l) { if (!l) return ""; const p = String(l).split(" "); return tr(p[0]) + (p.length > 1 ? " " + p.slice(1).join(" ") : "") }
    // ---- Google Lens: who is in the frame on screen ----
    // the core keeps one answer per stretch of the episode (15 s); the one of the stretch playing is shown
    readonly property var lensFrames: x0.lens && x0.lens.frames ? x0.lens.frames : []
    readonly property real lensSecs: x0.lens && x0.lens.bucketSecs ? x0.lens.bucketSecs : 15
    property var lensNow: ({})
    readonly property bool lensDone: lensNow.bucket !== undefined
    function refreshLens() {
        const b = Math.floor(Math.max(0, xr.pos) / xr.lensSecs)
        let f = ({})
        for (let i = 0; i < lensFrames.length; i++) if (lensFrames[i].bucket === b) f = lensFrames[i]
        keep("lensNow", f)
        keep("lensHits", f.hits || [])
    }
    // the switch shows the viewer's choice at once (the X-Ray state only comes again with its next change)
    property var lensAutoPick: undefined
    readonly property bool lensAuto: lensAutoPick !== undefined ? lensAutoPick : x0.lensAuto === true
    property bool lensBusy: false
    property string lensError: ""
    property int lensReq: 0
    property var lensObj: null
    // LensProbe.qml (the page view that asks Google Lens): a singleton when it is one, else made here once.
    // A build without it leaves the button disabled with a word why.
    function lensProbe() {
        try { if (typeof LensProbe !== "undefined" && LensProbe && typeof LensProbe.describe === "function") return LensProbe } catch (e) {}
        if (xr.lensObj) return xr.lensObj
        const c = Qt.createComponent("LensProbe.qml")
        if (c.status !== Component.Ready) { console.log("xray: LensProbe unavailable:", c.errorString()); return null }
        xr.lensObj = c.createObject(xr)
        return xr.lensObj && typeof xr.lensObj.describe === "function" ? xr.lensObj : null
    }
    function askLens() {
        if (xr.lensBusy) return
        const probe = lensProbe()
        if (!probe) { xr.lensError = tr("Google Lens is not available in this version."); return }
        // the frame of the video itself (mpv, without subtitles or controls), as the quick search does: taken on a
        // worker thread of the core, it comes back as Core.lensFrame (the player keeps running meanwhile)
        console.log("[lens] X-Ray: asking for the frame")
        const r = Core.cmd({cmd: "browser_grab_frame_async"})
        if (r.ok === false) { xr.lensError = tr(r.error || "The picture could not be taken."); return }
        xr.lensError = ""
        xr.lensBusy = true
        xr.lensGrab = ++xr.lensReq
        xr.lensAt = xr.pos
        lensTimeout.restart()
    }
    property int lensGrab: 0
    property real lensAt: 0
    Connections {
        target: Core
        ignoreUnknownSignals: true
        function onLensFrame(f) {
            if (xr.lensGrab === 0 || xr.lensGrab !== xr.lensReq) return
            xr.lensGrab = 0
            if (!f.frame) { lensTimeout.stop(); xr.lensBusy = false; xr.lensError = xr.tr(f.error || "The picture could not be taken."); return }
            xr.describeFrame(f.frame)
        }
    }
    function describeFrame(frame) {
        console.log("[lens] X-Ray: frame " + frame + ", starting the hidden Lens page")
        const probe = lensProbe()
        if (!probe) { lensTimeout.stop(); xr.lensBusy = false; xr.lensError = tr("Google Lens is not available in this version."); return }
        const req = xr.lensReq
        const at = xr.lensAt
        probe.describe(frame, function(a, b) {
            if (req !== xr.lensReq) return   // timed out, or asked again meanwhile
            lensTimeout.stop()
            xr.lensBusy = false
            // (result) or (error, result): both are taken
            const res = a && typeof a === "object" ? a : (b && typeof b === "object" ? b : {error: String(a || "")})
            if (res.error) { xr.lensError = String(res.error); return }
            Core.cmd({cmd: "xray_lens", pos: at, text: String(res.text || ""), titles: res.titles || []})
        })
    }
    Timer { id: lensTimeout; interval: 60000; onTriggered: { xr.lensReq++; xr.lensBusy = false; xr.lensError = xr.tr("Google Lens did not answer.") } }
    // by itself on open, when the viewer turned it on and this stretch was not looked at yet
    onOpenChanged: { if (open && xr.lensAuto && xr.hasData && !xr.lensDone) Qt.callLater(xr.askLens); xr.askTrivia() }
    // the trivia of the episode (Google's AI Overview, or the AI of Preferences > Conversation): asked once per
    // episode, when the Trivia tab is shown; the core runs it off the interface thread and keeps it on disk
    readonly property var epTrivia: x0.episodeTrivia || ({})
    readonly property string epTriviaStatus: epTrivia.status || "none"
    readonly property var epTriviaItems: epTrivia.status === "ready" && epTrivia.data && epTrivia.data.items ? epTrivia.data.items : []
    readonly property string epTriviaSource: epTrivia.status === "ready" && epTrivia.data ? (epTrivia.data.source || "") : ""
    readonly property string epTriviaError: epTrivia.status === "error" ? String(epTrivia.data || "") : ""
    function askTrivia() { if (xr.open && xr.tab === "trivia" && xr.epTriviaStatus === "none" && xr.status !== "none") Core.cmd({cmd: "xray_trivia"}) }
    onTabChanged: askTrivia()
    onEpTriviaStatusChanged: Qt.callLater(askTrivia)

    function srcName(id) { return id === "tmdb" ? "TMDB" : (id === "mal" ? "MyAnimeList" : (id === "imdb" ? "IMDb" : tr("Automatic"))) }
    Timer { interval: 250; repeat: true; running: panel.visible; triggeredOnStart: true; onTriggered: xr.refreshTv() }
    Connections { target: Core; function onXrayChanged() { if (panel.visible) xr.refreshTv() } }

    // ---- the look of the theme's player ----
    readonly property string ps: Theme.layout.player || "adw"
    readonly property string family: Theme.type.family
    readonly property color accent: Theme.c.accent
    readonly property color accentFg: Theme.c.accentFg || "#ffffff"
    // Material on a dark surface reads its primary colour lighter
    readonly property color accentText: ps === "m3" ? Osd.readable(Qt.lighter(Theme.c.accent, 1.35)) : "#ffffff"   // light enough on the dark panel in light app themes too
    readonly property color panelColor: ps === "glass" ? "#e81c1c22" : (ps === "m3" ? "#f21d1b20" : (ps === "fluent" ? "#f2202020" : "#f0242424"))
    readonly property color panelBorder: ps === "glass" ? "#33ffffff" : (ps === "fluent" ? "#26ffffff" : "#1fffffff")
    readonly property real panelRadius: ps === "glass" ? 26 : (ps === "m3" ? 28 : (ps === "fluent" ? 8 : (Theme.shape.sheet || 14)))
    readonly property real rowRadius: ps === "glass" ? 14 : (ps === "m3" ? 16 : (ps === "fluent" ? 4 : 10))
    readonly property real cardRadius: ps === "glass" ? 18 : (ps === "m3" ? 16 : (ps === "fluent" ? 6 : (Theme.shape.card || 12)))
    readonly property color rowHover: ps === "m3" ? "#14ffffff" : "#1affffff"
    readonly property color cardColor: ps === "glass" ? "#1fffffff" : (ps === "m3" ? "#2b2930" : (ps === "fluent" ? "#0fffffff" : "#14ffffff"))
    readonly property int titleSize: ps === "glass" || ps === "m3" ? 22 : 20
    readonly property int titleWeight: ps === "m3" ? Font.Normal : (ps === "fluent" ? Font.DemiBold : Font.Bold)

    // ---- the playback events of mpv ----
    Connections {
        target: Core
        function onPlaybackEvent(st) {
            xr.pos = st.pos || 0
            const p = st.paused === true
            if (p !== xr.paused) {
                xr.paused = p
                if (p && xr.autoOnPause && !xr.holdAuto && xr.hasData && !xr.pinned) xr.autoOpened = true
                if (!p) { xr.autoOpened = false; xr.pinned = false }
            }
        }
    }
    Component.onCompleted: { const st = Core.playback || ({}); pos = st.pos || 0; paused = st.paused === true; refreshNow(); refreshAll() }

    // ---- the tab on the left edge: the pointer over it opens the panel ----
    Rectangle {
        id: tabBtn
        visible: (xr.hasData || xr.tvActive) && !xr.open && xr.edgeShown
        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
        width: 30; height: 92
        radius: xr.ps === "fluent" ? 4 : (xr.ps === "adw" ? (Theme.shape.control || 8) : 15)
        color: tabHover.hovered ? "#f2303030" : (xr.ps === "glass" ? "#b31c1c22" : "#d9202024")
        border.width: 1; border.color: xr.tvActive ? xr.accent : xr.panelBorder
        Text {
            anchors.centerIn: parent
            rotation: -90
            text: "X-Ray"
            color: "#ffffff"; font.pixelSize: 13; font.weight: Font.Bold; font.family: xr.family
        }
        // Opening by hover also arms the close timer: if the pointer never reaches the panel, it closes again.
        HoverHandler { id: tabHover; onHoveredChanged: if (hovered) { xr.hoverOpen = true; leave.interval = 700; leave.restart() } }
        TapHandler { onTapped: xr.pinned = true }
    }

    // ---- the globe of the quick search, above the tab: a short hover (or a click) opens the search bar on top ----
    Rectangle {
        id: globeBtn
        visible: xr.searchEnabled && !xr.open && xr.edgeShown
        anchors { left: parent.left; leftMargin: 0; bottom: tabBtn.top; bottomMargin: 10 }
        width: 30; height: 30; radius: 15
        color: globeHover.hovered || xr.searchOpen ? "#f2303030" : (xr.ps === "glass" ? "#b31c1c22" : "#d9202024")
        border.width: 1; border.color: xr.searchOpen ? xr.accent : xr.panelBorder
        Icon { anchors.centerIn: parent; name: "globe"; size: 16; color: "#ffffff" }
        ToolTip.visible: globeHover.hovered && !xr.searchOpen; ToolTip.delay: 900; ToolTip.text: xr.tr("Quick search and Google Lens")
        HoverHandler { id: globeHover; cursorShape: Qt.PointingHandCursor; onHoveredChanged: if (hovered && !xr.searchOpen) globeDelay.restart(); else globeDelay.stop() }
        // a pointer only passing by does not open it
        Timer { id: globeDelay; interval: 350; onTriggered: if (globeHover.hovered && !xr.searchOpen) xr.searchAsked(false) }
        TapHandler { onTapped: { globeDelay.stop(); xr.searchAsked(true) } }
    }

    // ---- the panel ----
    Rectangle {
        id: panel
        width: Math.min(400, xr.width * 0.42)
        anchors { top: parent.top; bottom: parent.bottom; topMargin: 56; bottomMargin: 120 }
        x: xr.open ? 16 : -width - 20
        Behavior on x { NumberAnimation { duration: Theme.motion && Theme.motion.normal ? Theme.motion.normal : 200; easing.type: Easing.OutCubic } }
        visible: x > -width - 10
        radius: xr.panelRadius
        color: xr.panelColor
        border.width: xr.ps === "m3" ? 0 : 1
        border.color: xr.panelBorder
        clip: true
        HoverHandler { id: panelHover; onHoveredChanged: if (!hovered && xr.hoverOpen) { leave.interval = 220; leave.restart() } else leave.stop() }
        Timer { id: leave; interval: 220; onTriggered: if (!panelHover.hovered && !tabHover.hovered) xr.hoverOpen = false }
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onWheel: function(w) { w.accepted = false } }
        // Windows: the light top edge of an acrylic flyout
        Rectangle { visible: xr.ps === "fluent"; anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: parent.radius; rightMargin: parent.radius } height: 1; color: "#33ffffff" }

        Column {
            id: head
            x: xr.ps === "m3" ? 20 : 16; y: xr.ps === "m3" ? 18 : 14
            width: parent.width - 2 * x
            spacing: 10
            Item {
                width: parent.width; height: 32
                Text { anchors.verticalCenter: parent.verticalCenter; text: "X-Ray"; color: "#ffffff"; font.pixelSize: xr.titleSize; font.weight: xr.titleWeight; font.family: xr.family }
                Text { anchors { verticalCenter: parent.verticalCenter; right: feedbackBtn.visible ? feedbackBtn.left : closeBtn.left; rightMargin: 8 } text: xr.fmt(xr.pos) + (xr.paused ? "  ⏸" : ""); color: "#99ffffff"; font.pixelSize: 13; font.family: xr.family }
                // the feedback note (FeedbackBox): in the player it lives here, not as the floating button of the app
                FlatButton {
                    id: feedbackBtn
                    visible: !Core.prefs.general || Core.prefs.general.feedback_button !== false
                    anchors { right: closeBtn.left; rightMargin: 2; verticalCenter: parent.verticalCenter }
                    kind: "osd"; glyph: "message"; iconSize: 17; implicitHeight: 34; implicitWidth: 34
                    active: feedbackBox.opened
                    ToolTip.visible: hovered && !feedbackBox.opened; ToolTip.delay: 600; ToolTip.text: xr.tr("Send feedback")
                    onClicked: { if (feedbackBox.opened) feedbackBox.close(); else feedbackBox.open() }
                }
                // writing the note pins the panel: opened by hover only, it would slide away as the pointer leaves it
                FeedbackBox { id: feedbackBox; near: feedbackBtn; place: "player / X-Ray " + xr.tab; onOpened: xr.pinned = true }
                FlatButton { id: closeBtn; anchors { right: parent.right; verticalCenter: parent.verticalCenter } kind: "osd"; glyph: "close"; iconSize: 18; implicitHeight: 34; implicitWidth: 34; onClicked: { xr.pinned = false; xr.autoOpened = false; xr.hoverOpen = false } }
            }
            // where the cast comes from, and Fetch
            Item {
                width: parent.width; height: 34
                Rectangle {
                    id: srcBox
                    anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                    height: 32; width: srcRow.implicitWidth + 4
                    radius: xr.ps === "fluent" ? 4 : (xr.ps === "adw" ? (Theme.shape.control || 6) + 2 : height / 2)
                    color: "#1fffffff"
                    border.width: xr.ps === "m3" ? 1 : 0; border.color: "#8fffffff"
                    ToolTip.visible: srcHover.hovered; ToolTip.delay: 600; ToolTip.text: xr.tr("Pick where the cast and trivia come from")
                    HoverHandler { id: srcHover }
                    Row {
                        id: srcRow
                        anchors.centerIn: parent
                        spacing: 0
                        Repeater {
                            model: [{k: "auto", t: xr.tr("Auto")}, {k: "tmdb", t: "TMDB"}, {k: "mal", t: "MAL"}, {k: "imdb", t: "IMDb"}]
                            delegate: Rectangle {
                                required property var modelData
                                readonly property bool sel: xr.wantSource === modelData.k
                                height: 28; width: sl.implicitWidth + 18
                                radius: srcBox.radius - 2
                                color: sel ? (xr.ps === "glass" ? "#59ffffff" : xr.accent) : (sa.containsMouse ? "#1affffff" : "transparent")
                                Behavior on color { ColorAnimation { duration: 120 } }
                                Text { id: sl; anchors.centerIn: parent; text: modelData.t; color: parent.sel && xr.ps !== "glass" ? xr.accentFg : "#ffffff"; font.pixelSize: 12; font.weight: parent.sel ? Font.DemiBold : Font.Normal; font.family: xr.family }
                                MouseArea { id: sa; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: xr.pickedSource = modelData.k }
                            }
                        }
                    }
                }
                BusyIndicator { anchors { right: fetchBtn.left; rightMargin: 4; verticalCenter: parent.verticalCenter } width: 24; height: 24; running: xr.status === "loading"; visible: running }
                FlatButton {
                    id: fetchBtn
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    kind: xr.pickedSource !== "" && xr.pickedSource !== xr.source ? "suggested" : "osd"
                    glyph: "refresh"; iconSize: 16
                    text: xr.tr("Fetch")
                    implicitHeight: 32
                    enabled: xr.status !== "loading"
                    onClicked: xr.fetchNow()
                }
            }
            // the tabs
            Item {
                width: parent.width; height: 34
                // Apple: one capsule with a sliding thumb (segmented control)
                Rectangle { visible: xr.ps === "glass"; anchors.fill: tabRow; anchors.margins: -2; radius: height / 2; color: "#1fffffff" }
                Row {
                    id: tabRow
                    anchors.verticalCenter: parent.verticalCenter
                    x: xr.ps === "glass" ? 2 : 0
                    spacing: xr.ps === "glass" ? 0 : (xr.ps === "fluent" ? 2 : 6)
                    Repeater {
                        model: [{k: "scene", t: xr.tr("In this scene")}, {k: "cast", t: xr.tr("All")}, {k: "music", t: xr.tr("Music")}, {k: "trivia", t: xr.tr("Trivia")}]
                        delegate: Rectangle {
                            required property var modelData
                            id: tabD
                            readonly property bool sel: xr.tab === modelData.k
                            height: xr.ps === "fluent" ? 34 : 30
                            width: tl.implicitWidth + (xr.ps === "m3" && sel ? 40 : 22)
                            radius: xr.ps === "m3" ? 8 : (xr.ps === "fluent" ? 4 : 15)
                            color: {
                                if (xr.ps === "fluent") return ta.containsMouse ? "#14ffffff" : "transparent"
                                if (xr.ps === "glass") return sel ? "#59ffffff" : "transparent"
                                if (xr.ps === "m3") return sel ? Qt.rgba(xr.accent.r, xr.accent.g, xr.accent.b, 0.32) : (ta.containsMouse ? "#14ffffff" : "transparent")
                                return sel ? xr.accent : (ta.containsMouse ? "#33ffffff" : "#1fffffff")
                            }
                            border.width: xr.ps === "m3" && !sel ? 1 : 0
                            border.color: "#8fffffff"
                            Behavior on color { ColorAnimation { duration: 120 } }
                            Row {
                                anchors.centerIn: parent
                                spacing: 6
                                Icon { visible: xr.ps === "m3" && tabD.sel; name: "check"; size: 16; color: "#ffffff"; anchors.verticalCenter: parent.verticalCenter }
                                Text {
                                    id: tl
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.t
                                    color: tabD.sel ? (xr.ps === "adw" ? xr.accentFg : "#ffffff") : (xr.ps === "fluent" ? "#b3ffffff" : "#ffffff")
                                    font.pixelSize: 13; font.weight: tabD.sel || xr.ps === "adw" ? Font.DemiBold : Font.Normal; font.family: xr.family
                                }
                            }
                            // Windows: the selected tab is marked by a short accent line under it
                            Rectangle { visible: xr.ps === "fluent" && parent.sel; anchors { bottom: parent.bottom; horizontalCenter: parent.horizontalCenter } width: 16; height: 3; radius: 1.5; color: xr.accent }
                            MouseArea { id: ta; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: xr.tab = modelData.k }
                        }
                    }
                }
            }
        }

        // ---- the video of the opening / ending, above the lists while it is open ----
        Rectangle {
            id: tvCard
            visible: xr.tvActive
            anchors { left: parent.left; right: parent.right; top: head.bottom; margins: 12; topMargin: 12 }
            height: visible ? tvCol.implicitHeight + 20 : 0
            radius: xr.cardRadius
            color: xr.cardColor
            border.width: xr.ps === "glass" ? 1 : 0; border.color: "#26ffffff"
            Column {
                id: tvCol
                x: 10; y: 10; width: parent.width - 20
                spacing: 8
                Item {
                    id: screen
                    width: parent.width; height: Math.round(width * 9 / 16)
                    Rectangle { anchors.fill: parent; radius: xr.cardRadius - 4; color: "#000000" }
                    RoundedPicture { anchors.fill: parent; radius: xr.cardRadius - 4; url: xr.tv.thumbnail || ""; visible: !tvView.hasFrame && (xr.tv.thumbnail || "") !== ""; opacity: 0.6 }
                    ThemeVideo {
                        id: tvView
                        anchors.fill: parent
                        radius: xr.cardRadius - 4
                        running: xr.tv.status === "playing" && panel.visible
                    }
                    BusyIndicator { anchors.centerIn: parent; width: 40; height: 40; running: xr.tv.status === "loading" || (xr.tv.status === "playing" && xr.tv.buffering === true); visible: running }
                    Column {
                        visible: xr.tv.status === "error"
                        anchors.centerIn: parent; width: parent.width - 32; spacing: 4
                        Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: xr.tr("The video could not be played."); color: "#ffffff"; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: xr.family }
                        Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; maximumLineCount: 3; elide: Text.ElideRight; text: xr.tv.error || ""; color: "#b3ffffff"; font.pixelSize: 11; font.family: xr.family }
                    }
                    Text {
                        visible: xr.tv.status === "loading"
                        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 10 }
                        text: xr.tr("Looking for the video on YouTube…"); color: "#e6ffffff"; font.pixelSize: 12; font.family: xr.family
                    }
                    // a click on the picture plays / pauses
                    MouseArea { anchors.fill: parent; enabled: xr.tv.status === "playing"; cursorShape: Qt.PointingHandCursor; onClicked: { Core.cmd({cmd: "theme_pause"}); xr.refreshTv() } }
                    Rectangle {
                        visible: xr.tv.status === "playing" && xr.tv.paused === true
                        anchors.centerIn: parent; width: 52; height: 52; radius: 26; color: "#99000000"
                        Icon { anchors.centerIn: parent; name: "play"; size: 26; color: "#ffffff" }
                    }
                }
                // what plays
                Column {
                    width: parent.width; spacing: 1
                    Text { width: parent.width; elide: Text.ElideRight; text: xr.tlabel(xr.tv.label) + (xr.tv.artist ? "  ·  " + xr.tv.artist : ""); color: xr.ps === "m3" ? xr.accentText : "#99ffffff"; font.pixelSize: 11; font.weight: Font.DemiBold; font.family: xr.family }
                    Text { width: parent.width; elide: Text.ElideRight; text: xr.tv.song || xr.tv.title || ""; color: "#ffffff"; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: xr.family }
                }
                // the seek line
                Item {
                    id: seekLine
                    width: parent.width; height: 18
                    readonly property real dur: xr.tv.dur || 0
                    readonly property real frac: dragging ? dragFrac : (dur > 0 ? Math.min(1, (xr.tv.pos || 0) / dur) : 0)
                    property bool dragging: false
                    property real dragFrac: 0
                    Rectangle {
                        id: track
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
                        height: xr.ps === "m3" ? 6 : (xr.ps === "glass" ? 4 : (seekMouse.containsMouse ? 5 : 3))
                        radius: height / 2
                        color: "#40ffffff"
                        Rectangle { width: parent.width * seekLine.frac; height: parent.height; radius: parent.radius; color: xr.ps === "glass" ? "#ffffff" : xr.accent }
                    }
                    Rectangle {
                        visible: seekLine.dur > 0 && (seekMouse.containsMouse || seekLine.dragging || xr.ps === "m3")
                        x: track.width * seekLine.frac - width / 2
                        anchors.verticalCenter: parent.verticalCenter
                        width: xr.ps === "m3" ? 4 : 12; height: xr.ps === "m3" ? 18 : 12
                        radius: xr.ps === "m3" ? 2 : 6
                        color: xr.ps === "m3" ? xr.accent : "#ffffff"
                    }
                    MouseArea {
                        id: seekMouse
                        anchors.fill: parent
                        enabled: xr.tv.status === "playing" && seekLine.dur > 0
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        function fracAt(mx) { return Math.max(0, Math.min(1, mx / width)) }
                        onPressed: function(m) { seekLine.dragging = true; seekLine.dragFrac = fracAt(m.x) }
                        onPositionChanged: function(m) { if (seekLine.dragging) seekLine.dragFrac = fracAt(m.x) }
                        onReleased: function(m) { seekLine.dragging = false; Core.cmd({cmd: "theme_seek", pos: fracAt(m.x) * seekLine.dur}); xr.refreshTv() }
                    }
                }
                // the controls
                Item {
                    width: parent.width; height: 34
                    Row {
                        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                        spacing: 2
                        FlatButton {
                            kind: "osd"; iconSize: 18; implicitHeight: 34; implicitWidth: 34
                            glyph: xr.tv.status === "playing" && xr.tv.paused !== true && xr.tv.ended !== true ? "pause" : "play"
                            enabled: xr.tv.status === "playing"
                            onClicked: { Core.cmd({cmd: "theme_pause"}); xr.refreshTv() }
                        }
                        FlatButton {
                            kind: "osd"; iconSize: 18; implicitHeight: 34; implicitWidth: 34
                            glyph: xr.tv.muted === true || (xr.tv.volume || 0) <= 0 ? "mute" : "volume"
                            ToolTip.visible: hovered; ToolTip.text: xr.tr("Mute")
                            onClicked: { Core.cmd({cmd: "theme_volume", mute: !(xr.tv.muted === true)}); xr.refreshTv() }
                        }
                        // volume: a short line, the wheel works too
                        Item {
                            id: volBox
                            width: 64; height: 34
                            readonly property real v: (xr.tv.volume !== undefined ? xr.tv.volume : 100) / 100
                            Rectangle {
                                anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
                                height: 3; radius: 1.5; color: "#40ffffff"
                                Rectangle { width: parent.width * volBox.v; height: parent.height; radius: parent.radius; color: xr.tv.muted === true ? "#80ffffff" : "#ffffff" }
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                function setAt(mx) { Core.cmd({cmd: "theme_volume", volume: Math.round(Math.max(0, Math.min(1, mx / width)) * 100), mute: false}); xr.refreshTv() }
                                onPressed: function(m) { setAt(m.x) }
                                onPositionChanged: function(m) { if (pressed) setAt(m.x) }
                                onWheel: function(w) { Core.cmd({cmd: "theme_volume", volume: Math.max(0, Math.min(100, (xr.tv.volume || 0) + (w.angleDelta.y > 0 ? 5 : -5)))}); xr.refreshTv() }
                            }
                            ToolTip.visible: volHover.hovered; ToolTip.text: xr.tr("Volume")
                            HoverHandler { id: volHover }
                        }
                        Text { anchors.verticalCenter: parent.verticalCenter; leftPadding: 8; text: xr.fmt(seekLine.dragging ? seekLine.dragFrac * seekLine.dur : (xr.tv.pos || 0)) + " / " + xr.fmt(seekLine.dur); color: "#b3ffffff"; font.pixelSize: 11; font.family: xr.family; font.features: ({ "tnum": 1 }) }
                    }
                    FlatButton {
                        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                        kind: "osd"; glyph: "close"; iconSize: 18; implicitHeight: 34; implicitWidth: 34
                        ToolTip.visible: hovered; ToolTip.text: xr.tr("Close the video")
                        onClicked: { Core.cmd({cmd: "theme_stop"}); tvView.clearFrame(); xr.refreshTv() }
                    }
                }
                FlatButton {
                    width: parent.width
                    kind: "suggested"; glyph: "play"; iconSize: 16
                    implicitHeight: 34
                    text: xr.tr("Resume episode")
                    onClicked: { Core.cmd({cmd: "theme_stop", resume: true}); tvView.clearFrame(); xr.refreshTv() }
                }
            }
        }

        component Head: Text {
            width: parent ? parent.width : 300
            color: xr.ps === "m3" ? xr.accentText : (xr.ps === "glass" ? "#8cffffff" : (xr.ps === "fluent" ? "#ffffff" : "#d9ffffff"))
            font.pixelSize: xr.ps === "glass" ? 12 : (xr.ps === "adw" ? 13 : 14)
            font.weight: xr.ps === "m3" ? Font.Medium : (xr.ps === "glass" ? Font.Normal : (xr.ps === "fluent" ? Font.DemiBold : Font.Bold))
            font.capitalization: xr.ps === "glass" ? Font.AllUppercase : Font.MixedCase
            font.letterSpacing: xr.ps === "glass" ? 0.6 : 0
            font.family: xr.family
            topPadding: 10; bottomPadding: 2
            leftPadding: xr.ps === "glass" ? 8 : 4
        }
        // A person: a click opens what the sources know of them (the character on MyAnimeList, the actor on TMDB,
        // or Wikipedia: xray_person), looked up once and kept; a second click closes it.
        component Person: Rectangle {
            id: pr
            property var it
            property bool open: false
            readonly property string ref: it && it.ref ? it.ref : ""
            readonly property var info: ref !== "" && xr.x0.people ? (xr.x0.people[ref] || null) : null
            width: parent ? parent.width : 300
            height: open ? 64 + more.implicitHeight + 10 : 64
            radius: xr.rowRadius
            color: pa.containsMouse || open ? xr.rowHover : "transparent"
            clip: true
            // round photos (a letter on a tint of the accent when there is none)
            Item {
                id: avatar
                x: 8; y: 8
                width: 48; height: 48
                RoundedPicture { anchors.fill: parent; radius: 24; url: it.image || ""; visible: (it.image || "") !== ""; placeholder: "#26ffffff" }
                Rectangle {
                    visible: (it.image || "") === ""
                    anchors.fill: parent; radius: 24
                    color: Qt.rgba(xr.accent.r, xr.accent.g, xr.accent.b, 0.35)
                    Text { anchors.centerIn: parent; text: (it.name || "?").charAt(0); color: "#ffffff"; font.pixelSize: 20; font.bold: true; font.family: xr.family }
                }
            }
            Column {
                anchors { left: avatar.right; leftMargin: 12; right: parent.right; rightMargin: 28 }
                y: 32 - height / 2
                spacing: 2
                Text { width: parent.width; elide: Text.ElideRight; text: it.name || ""; color: "#ffffff"; font.pixelSize: 14; font.weight: xr.ps === "m3" ? Font.Medium : Font.DemiBold; font.family: xr.family }
                Text { width: parent.width; elide: Text.ElideRight; visible: text !== ""; text: it.role ? (it.tag === "guest" ? xr.tr("Guest") + " · " : "") + it.role : ""; color: "#b3ffffff"; font.pixelSize: 12; font.family: xr.family }
                Text { width: parent.width; elide: Text.ElideRight; visible: text !== ""; text: it.voice ? "🎙 " + it.voice : ""; color: "#99ffffff"; font.pixelSize: 11; font.family: xr.family }
            }
            Icon { visible: pr.ref !== ""; x: parent.width - 24; y: 24; name: pr.open ? "up" : "down"; size: 16; color: "#80ffffff" }
            MouseArea {
                id: pa
                anchors.fill: parent; hoverEnabled: true
                cursorShape: pr.ref !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: { if (pr.ref === "") return; pr.open = !pr.open; if (pr.open && (!pr.info || pr.info.error)) Core.cmd({cmd: "xray_person", ref: pr.ref}) }
            }
            Column {
                id: more
                visible: pr.open
                x: 12; y: 66; width: parent.width - 24
                spacing: 4
                Text { visible: pr.info === null || pr.info.loading === true; text: xr.tr("Looking it up…"); color: "#99ffffff"; font.pixelSize: 12; font.family: xr.family }
                Text { visible: !!(pr.info && pr.info.error); width: parent.width; wrapMode: Text.Wrap; text: pr.info && pr.info.error ? xr.tr(pr.info.error) : ""; color: "#ffb4ab"; font.pixelSize: 12; font.family: xr.family }
                Text { visible: text !== ""; width: parent.width; wrapMode: Text.Wrap; text: pr.info && pr.info.about ? pr.info.about : ""; color: "#e6ffffff"; font.pixelSize: 12; lineHeight: 1.1; font.family: xr.family }
                Repeater {
                    model: pr.info && pr.info.facts ? pr.info.facts : []
                    delegate: Text { required property var modelData; width: more.width; wrapMode: Text.Wrap; text: "• " + xr.factLine(modelData); color: "#b3ffffff"; font.pixelSize: 11; font.family: xr.family }
                }
                Text {
                    visible: !!(pr.info && pr.info.url)
                    text: "<a href=\"" + (pr.info && pr.info.url ? pr.info.url : "") + "\">" + xr.tr("Open the page") + (pr.info && pr.info.source ? " (" + pr.info.source + ")" : "") + "</a>"
                    textFormat: Text.StyledText; linkColor: xr.accentText; font.pixelSize: 12; font.family: xr.family
                    onLinkActivated: function(l) { Qt.openUrlExternally(l) }
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                }
            }
        }
        component Song: Rectangle {
            property var it
            readonly property bool current: xr.tvActive && (xr.tv.song || "") === (it.name || "")
            width: parent ? parent.width : 300
            height: 60
            radius: xr.rowRadius
            color: sm.containsMouse ? Qt.tint(xr.cardColor, "#14ffffff") : xr.cardColor
            border.width: current ? 1 : 0; border.color: xr.accent
            Rectangle {
                id: songIcon
                x: 10; anchors.verticalCenter: parent.verticalCenter
                width: 40; height: 40; radius: xr.ps === "fluent" ? 4 : (xr.ps === "m3" ? 12 : 20)
                color: xr.accent
                Icon { anchors.centerIn: parent; name: sm.containsMouse ? "play" : "audio"; size: 18; color: xr.accentFg }
            }
            Column {
                anchors { left: songIcon.right; leftMargin: 12; right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                spacing: 2
                Text { width: parent.width; elide: Text.ElideRight; text: it.name || ""; color: "#ffffff"; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: xr.family }
                Text { width: parent.width; elide: Text.ElideRight; text: (it.role || "") + (it.tag ? "  ·  " + xr.tlabel(it.tag) : "") + (it.end_time < 1e8 ? "  ·  " + xr.fmt(it.start_time) + "–" + xr.fmt(it.end_time) : ""); color: "#b3ffffff"; font.pixelSize: 12; font.family: xr.family }
            }
            MouseArea {
                id: sm
                anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                onClicked: xr.playSong(it)
                ToolTip.visible: containsMouse; ToolTip.delay: 600; ToolTip.text: xr.tr("Play in the X-Ray")
            }
        }
        component Fact: Rectangle {
            property var it
            width: parent ? parent.width : 300
            height: ft.implicitHeight + 20
            radius: xr.rowRadius
            color: xr.cardColor
            Column {
                id: ft
                x: 12; y: 10; width: parent.width - 24; spacing: 4
                Text { width: parent.width; wrapMode: Text.Wrap; text: it.name || ""; color: "#ffffff"; font.pixelSize: 13; font.weight: (it.text || "") !== "" ? Font.DemiBold : Font.Normal; font.family: xr.family }
                Text { width: parent.width; wrapMode: Text.Wrap; visible: text !== ""; text: it.text || ""; color: "#ccffffff"; font.pixelSize: 12; font.family: xr.family }
                Text { visible: text !== ""; text: it.role || ""; color: "#80ffffff"; font.pixelSize: 10; font.family: xr.family }
            }
        }

        Flickable {
            anchors { left: parent.left; right: parent.right; top: tvCard.visible ? tvCard.bottom : head.bottom; bottom: parent.bottom; margins: 12; topMargin: 12 }
            contentHeight: body.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Column {
                id: body
                width: parent.width
                spacing: 6
                // loading / nothing / error
                Row {
                    visible: !xr.hasData && (xr.status === "loading" || xr.status === "waiting")
                    width: parent.width; spacing: 10
                    BusyIndicator { width: 28; height: 28; running: parent.visible }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 40; wrapMode: Text.Wrap
                        text: xr.tr("Gathering the cast, music and trivia…") + (xr.source !== "auto" ? " (" + xr.srcName(xr.source) + ")" : "")
                        color: "#b3ffffff"; font.pixelSize: 13; font.family: xr.family
                    }
                }
                Rectangle {
                    visible: !xr.hasData && xr.status !== "loading" && xr.status !== "waiting"
                    width: parent.width; height: errCol.implicitHeight + 24
                    radius: xr.rowRadius; color: xr.cardColor
                    Column {
                        id: errCol
                        x: 12; y: 12; width: parent.width - 24; spacing: 6
                        Row {
                            spacing: 8
                            Icon { name: "info"; size: 18; color: "#ccffffff"; anchors.verticalCenter: parent.verticalCenter }
                            Text { anchors.verticalCenter: parent.verticalCenter; text: xr.status === "error" ? xr.tr("Nothing came from this source.") : xr.tr("Nothing for this video."); color: "#ffffff"; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: xr.family }
                        }
                        Text { width: parent.width; wrapMode: Text.Wrap; visible: xr.status === "error" && text !== ""; text: String(xr.x0.data || ""); color: "#99ffffff"; font.pixelSize: 11; font.family: xr.family }
                        Text { width: parent.width; wrapMode: Text.Wrap; visible: xr.status === "error"; text: xr.tr("Try another source above, or Fetch again."); color: "#ccffffff"; font.pixelSize: 12; font.family: xr.family }
                    }
                }
                // in this scene: first who Google Lens recognised in the frame (on the button, or by itself if set)
                Item {
                    visible: xr.tab === "scene" && xr.hasData
                    width: parent.width; height: visible ? 40 : 0
                    FlatButton {
                        id: lensBtn
                        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                        kind: xr.lensDone ? "osd" : "suggested"
                        glyph: "lens"; iconSize: 16; implicitHeight: 34
                        text: xr.lensBusy ? xr.tr("Asking Google Lens…") : (xr.lensDone ? xr.tr("Look again") : xr.tr("Who's in this scene?"))
                        enabled: !xr.lensBusy
                        ToolTip.visible: hovered; ToolTip.delay: 600; ToolTip.text: xr.tr("Sends the frame on screen to Google Lens and finds the characters and actors it names")
                        onClicked: xr.askLens()
                    }
                    BusyIndicator { anchors { left: lensBtn.right; leftMargin: 6; verticalCenter: parent.verticalCenter } width: 22; height: 22; running: xr.lensBusy; visible: running }
                    // ask by itself whenever the X-Ray opens (off by default: the frame goes to Google)
                    Row {
                        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                        spacing: 6
                        Text { anchors.verticalCenter: parent.verticalCenter; text: xr.tr("On open"); color: "#b3ffffff"; font.pixelSize: 11; font.family: xr.family }
                        OsdSwitch {
                            anchors.verticalCenter: parent.verticalCenter
                            checked: xr.lensAuto
                            onToggled: function(on) { xr.lensAutoPick = on; Core.cmd({cmd: "set_setting", path: "general.xray_lens_auto", value: on}) }
                            ToolTip.visible: autoHover.hovered; ToolTip.delay: 600; ToolTip.text: xr.tr("Ask Google Lens by itself each time the X-Ray opens")
                            HoverHandler { id: autoHover }
                        }
                    }
                }
                Text { visible: xr.tab === "scene" && xr.lensError !== ""; width: parent.width; leftPadding: 4; wrapMode: Text.Wrap; text: xr.lensError; color: "#ffb4ab"; font.pixelSize: 12; font.family: xr.family }
                Head { visible: xr.tab === "scene" && xr.lensDone; text: xr.tr("Recognised in this frame") }
                Repeater { model: xr.tab === "scene" ? xr.lensHits : []; delegate: Person { required property var modelData; it: modelData } }
                Text {
                    visible: xr.tab === "scene" && xr.lensDone && xr.lensHits.length === 0
                    width: parent.width; leftPadding: 4; wrapMode: Text.Wrap
                    text: xr.tr("Google Lens named no one of the cast in this frame.")
                    color: "#b3ffffff"; font.pixelSize: 12; font.family: xr.family
                }
                // what Lens said of the picture (three lines; a click shows it all)
                Rectangle {
                    id: lensSay
                    property bool expanded: false
                    visible: xr.tab === "scene" && xr.lensDone && (xr.lensNow.summary || "") !== ""
                    width: parent.width; height: visible ? sayCol.implicitHeight + 16 : 0
                    radius: xr.rowRadius; color: xr.cardColor
                    Column {
                        id: sayCol
                        x: 10; y: 8; width: parent.width - 20; spacing: 2
                        Text { text: xr.tr("What Google Lens says"); color: "#80ffffff"; font.pixelSize: 10; font.family: xr.family }
                        Text { width: parent.width; wrapMode: Text.Wrap; maximumLineCount: lensSay.expanded ? 40 : 3; elide: Text.ElideRight; text: xr.lensNow.summary || ""; color: "#ccffffff"; font.pixelSize: 12; font.family: xr.family }
                    }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: lensSay.expanded = !lensSay.expanded }
                }
                Head { visible: xr.tab === "scene" && xr.nowScene.length > 0; text: xr.tr("Scene") }
                Repeater { model: xr.tab === "scene" ? xr.nowScene : []; delegate: Text { required property var modelData; width: body.width; leftPadding: 4; wrapMode: Text.Wrap; text: modelData.name + "  ·  " + xr.fmt(modelData.start_time) + (modelData.end_time < 1e8 ? "–" + xr.fmt(modelData.end_time) : ""); color: "#ffffff"; font.pixelSize: 14; font.family: xr.family } }
                Head { visible: xr.tab === "scene" && xr.nowMusic.length > 0; text: xr.tr("Playing now") }
                Repeater { model: xr.tab === "scene" ? xr.nowMusic : []; delegate: Song { required property var modelData; it: modelData } }
                Head { visible: xr.tab === "scene" && xr.nowCast.length > 0; text: xr.tr("Cast") }
                Repeater { model: xr.tab === "scene" ? xr.nowCast.filter(function(c) { return c.tag !== "supporting" }).slice(0, 12) : []; delegate: Person { required property var modelData; it: modelData } }
                Head { visible: xr.tab === "scene" && xr.nowTrivia.length > 0; text: xr.tr("Trivia") }
                Repeater { model: xr.tab === "scene" ? xr.nowTrivia.slice(0, 2) : []; delegate: Fact { required property var modelData; it: modelData } }
                // all the cast
                Repeater { model: xr.tab === "cast" ? xr.allCast : []; delegate: Person { required property var modelData; it: modelData } }
                // the trailers (IMDb): a small row of their own, never with the songs
                Head { visible: xr.tab === "cast" && xr.allTrailers.length > 0; text: xr.tr("Trailers") }
                Flow {
                    visible: xr.tab === "cast" && xr.allTrailers.length > 0
                    width: parent.width; spacing: 6
                    Repeater {
                        model: xr.tab === "cast" ? xr.allTrailers.length : 0
                        delegate: FlatButton {
                            required property int index
                            readonly property var it: xr.allTrailers[index] || ({})
                            kind: "osd"; glyph: "play"; iconSize: 14; implicitHeight: 30
                            text: xr.tr("Trailer") + " " + (index + 1)
                            ToolTip.visible: hovered; ToolTip.delay: 600; ToolTip.text: it.name || ""
                            onClicked: xr.playSong(it)
                        }
                    }
                }
                // the openings and endings
                Head { visible: xr.tab === "music" && xr.allMusic.length > 0; text: xr.tr("Opening & ending") }
                Text { visible: xr.tab === "music" && xr.hasData && xr.allMusic.length === 0; width: parent.width; leftPadding: 4; topPadding: 6; wrapMode: Text.Wrap; text: xr.tr("No opening or ending was found for this title."); color: "#b3ffffff"; font.pixelSize: 12; font.family: xr.family }
                Repeater { model: xr.tab === "music" ? xr.allMusic : []; delegate: Song { required property var modelData; it: modelData } }
                // the trivia: first the episode's (Google / AI), then the facts of the title (IMDb, TMDB, MyAnimeList)
                Head { visible: xr.tab === "trivia" && xr.epTriviaStatus !== "none"; text: xr.tr("Episode trivia") }
                Row {
                    visible: xr.tab === "trivia" && xr.epTriviaStatus === "loading"
                    width: parent.width; spacing: 10
                    BusyIndicator { width: 24; height: 24; running: parent.visible }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 34; wrapMode: Text.Wrap
                        text: xr.tr("Asking Google and the AI about this episode…")
                        color: "#b3ffffff"; font.pixelSize: 12; font.family: xr.family
                    }
                }
                Repeater { model: xr.tab === "trivia" ? xr.epTriviaItems : []; delegate: Fact { required property var modelData; it: ({name: modelData}) } }
                Text {
                    visible: xr.tab === "trivia" && xr.epTriviaSource !== ""
                    width: parent.width; leftPadding: 4; wrapMode: Text.Wrap
                    text: xr.tr("Source") + ": " + xr.epTriviaSource + "  ·  " + xr.tr("Answers made by AI can be wrong.")
                    color: "#80ffffff"; font.pixelSize: 10; font.family: xr.family
                }
                Item {
                    visible: xr.tab === "trivia" && xr.epTriviaStatus === "error"
                    width: parent.width; height: visible ? Math.max(errTxt.implicitHeight, againBtn.implicitHeight) + 4 : 0
                    Text {
                        id: errTxt
                        anchors { left: parent.left; right: againBtn.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                        leftPadding: 4; wrapMode: Text.Wrap
                        text: xr.epTriviaError !== "" ? xr.tr(xr.epTriviaError) : xr.tr("No trivia was found for this episode.")
                        color: "#b3ffffff"; font.pixelSize: 12; font.family: xr.family
                    }
                    FlatButton {
                        id: againBtn
                        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                        kind: "osd"; implicitHeight: 30
                        text: xr.tr("Try again")
                        onClicked: Core.cmd({cmd: "xray_trivia", again: true})
                    }
                }
                Head { visible: xr.tab === "trivia" && xr.epTriviaStatus !== "none" && xr.allTrivia.length > 0; text: xr.tr("About the title") }
                Repeater { model: xr.tab === "trivia" ? xr.allTrivia : []; delegate: Fact { required property var modelData; it: modelData } }
                Text {
                    visible: xr.hasData && xr.x0.data && xr.x0.data.sources && xr.x0.data.sources.length > 0
                    width: parent.width; wrapMode: Text.Wrap; topPadding: 10; leftPadding: 4
                    text: xr.tr("Sources") + ": " + (xr.x0.data && xr.x0.data.sources ? xr.x0.data.sources.join(", ") : "") + "  ·  " + xr.tr("Source") + ": " + xr.srcName(xr.source)
                    color: "#66ffffff"; font.pixelSize: 10; font.family: xr.family
                }
            }
        }
    }
}
