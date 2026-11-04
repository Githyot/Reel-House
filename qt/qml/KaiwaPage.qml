import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "KanaHover.js" as Kana

// Study: Conversation (会話), a tutor to talk with in the language studied (the core: kaiwa.rs, controller_kaiwa.rs).
// Home: a fixed header (the language, the level, the tutor and three tabs) over the body of the tab, the only one
// built. "Start": free conversation, a voice call, a reading story, a scene of your own, the role-play scenes by theme
// (a ListView of themes: only what is on screen is built) and the guided lessons. "History": the past sessions with
// their reports. "Progress": days in a row, minutes, messages, the kinds of mistakes and the words learned.
// A session: the tutor's messages on the left (a click on a word explains it; the reading in kana, romaji and the
// translation can be shown under each, and the voice reads it), the learner's on the right with the check of each
// one under it; a hint gives three replies to pick; "End" writes the report.
//
// Kept light on purpose (the owner felt lag when scrolling, 2026-10-15): flat cards (no glass, no shadows, no colour
// animations), the lists of the state cached and only replaced when their content changes (a new array makes a
// Repeater or a ListView build every card again), the chat as a ListModel that is only appended to or patched.
Item {
    id: kp
    property var k: ({})
    property string tab: "start"
    readonly property var s: k.session ? k.session : null
    readonly property var lines: s && s.lines ? s.lines : []
    readonly property var g: Core.prefs.general || ({})
    readonly property bool ja: (k.lang || "") === "ja"
    readonly property real cw: Math.min(980, width - Theme.spacing.page * 2)
    readonly property real x0: (width - cw) / 2
    readonly property bool wide: cw >= 760
    readonly property var loc: Qt.locale(String(Core.uiLang).replace("-", "_"))
    function tr(t) { return Core.tr(Core.uiLang, t) }
    function set(key, v) { Core.cmd({cmd: "set_setting", path: "general." + key, value: v}) }

    // ---- controller: while a session is open this page is Main.qml's `padPage`, which hands it the buttons first.
    // ✕/A talk (again: send now), □/X the tutor's last message again, △/Y a hint, ○/B back (or drop the recording),
    // L1/LB romaji (Japanese) or the translation, R1/RB the translation, Share/View end, ↑/↓ scroll; Options stays
    // the app's settings. The glyphs show on the buttons, and the hint bar goes to the top right under the session's
    // bar (`padHintsTop`/`padHintsY`), where it covers neither the box nor the chat (the chat starts below it).
    readonly property var padWin: Window.window
    readonly property bool padOn: !!(padWin && padWin.padMode)
    readonly property bool padPs: kp.padOn && (padWin.padKind === "ps4" || padWin.padKind === "ps5")
    readonly property bool padActive: kp.padOn && kp.visible && kp.s !== null && !kp.liveSession && !wordPop.opened
    readonly property bool padHintsTop: true
    readonly property real padHintsY: { kp.width; kp.height; return bar.mapToItem(null, 0, bar.height).y + 8 }
    readonly property real padReserve: kp.padActive && padWin.padHintHeight !== undefined ? padWin.padHintHeight + 14 : 0
    function padRegister() {
        const w = kp.padWin
        if (!w || w.padPage === undefined) return
        if (kp.padActive) w.padPage = kp
        else if (w.padPage === kp) w.padPage = null
    }
    onPadActiveChanged: padRegister()
    Component.onDestruction: { const w = kp.padWin; if (w && w.padPage === kp) w.padPage = null }
    function padButton(b) {
        if (!kp.padActive) return false
        const ended = kp.s.ended > 0
        switch (b) {
        case "A":
            if (ended) return false
            if (!kp.recording && (kp.k.listening === true || (kp.callMode && kp.k.busy === true))) return true
            kp.toggleMic(); return true
        case "X": kp.replay(); return true
        case "Y": if (!ended && kp.lines.length > 0 && kp.k.hintsBusy !== true) Core.cmd({cmd: "kaiwa_hint"}); return true
        case "B":
            if (kp.recording && kp.mic) { kp.hfArmed = false; kp.mic.cancel(); return true }
            Core.cmd({cmd: "kaiwa_open", id: 0}); return true
        case "LB": kp.set(kp.ja ? "kaiwa_romaji" : "kaiwa_translation", kp.ja ? kp.g.kaiwa_romaji !== true : kp.g.kaiwa_translation !== true); return true
        case "RB":
            kp.set("kaiwa_translation", kp.g.kaiwa_translation !== true)
            if (kp.g.kaiwa_translation !== true && kp.liveSession) Core.cmd({cmd: "kaiwa_annotate"})
            return true
        case "Select": if (!ended) kp.endSession(); return true
        case "Up": case "Down": {
            const step = Math.max(80, list.height * 0.4) * (b === "Up" ? -1 : 1)
            const top = list.originY - list.topMargin
            const end = Math.max(top, list.originY + list.contentHeight - list.height)
            list.contentY = Math.max(top, Math.min(end, list.contentY + step))
            return true
        }
        }
        return false
    }
    readonly property var padHints: {
        const ended = kp.s !== null && kp.s.ended > 0
        var h = []
        if (!ended) h.push({button: "A", text: kp.recording ? kp.tr("Send now") : kp.tr("Talk")})
        h.push({button: "X", text: kp.tr("Replay")})
        if (!ended) h.push({button: "Y", text: kp.tr("Hint")})
        h.push({button: "B", text: kp.recording ? kp.tr("Cancel") : kp.tr("Back")})
        if (kp.ja) h.push({button: "LB", text: "Romaji"})
        h.push({button: "RB", text: kp.tr("Translation")})
        if (!ended) h.push({button: "Select", text: kp.tr("End")})
        h.push({button: "Pad", text: kp.tr("Scroll")})
        h.push({button: "Start", text: kp.tr("Settings")})
        return h
    }
    // a controller glyph in the corner of a button (each use sets `visible: kp.padActive` and `ps: kp.padPs`: an
    // inline component does not see the ids of this file)
    component PadMark: PadGlyph {
        size: 16
        z: 5
        anchors { right: parent.right; bottom: parent.bottom; rightMargin: -4; bottomMargin: -4 }
    }

    // ---- the state, read once per change and cached ----
    // The lists below keep their array (and so their cards) until their content really changes.
    property var kLevels: []
    property var kScenes: []        // [{cat, items: [scene]}]
    property var kLessons: []
    property var kHistory: []
    property var kProgress: ({})
    property var kMistakes: []
    property var kWords: []
    property var sigs: ({})
    function keep(name, v) {
        const sig = JSON.stringify(v)
        if (kp.sigs[name] === sig) return
        kp.sigs[name] = sig
        kp[name] = v
    }
    function refresh() {
        const k = Core.state("kaiwa") || ({})
        kp.k = k
        kp.keep("kLevels", (k.levels || []).map(function(l) { return {id: l, label: l} }))
        const groups = [], at = {}
        for (const sc of (k.scenes || [])) {
            if (at[sc.cat] === undefined) { at[sc.cat] = groups.length; groups.push({cat: sc.cat, items: []}) }
            groups[at[sc.cat]].items.push(sc)
        }
        kp.keep("kScenes", groups)
        kp.keep("kLessons", k.lessons || [])
        kp.keep("kHistory", k.history || [])
        kp.keep("kProgress", k.progress || ({}))
        kp.keep("kMistakes", k.mistakes || [])
        kp.keep("kWords", k.words || [])
        kp.syncLines()
    }
    function start(o) { Core.cmd(Object.assign({cmd: "kaiwa_start"}, o)); refresh() }
    Connections { target: Core; function onStudyChanged() { if (kp.visible) kp.refresh() } }
    // ---- Conversation by voice (Gemini Live: KaiwaVoiceCard, KaiwaLiveBar) ----
    // the options and whether a call runs (kaiwa_json().live); the call itself is the light state "kaiwa_live", read
    // several times a second only while a call runs
    readonly property var lv: kp.k.live || ({})
    readonly property bool liveSession: kp.s !== null && kp.s.live === true
    property var liveNow: ({})
    Timer {
        interval: 120; repeat: true
        running: kp.visible && kp.lv.running === true
        onRunningChanged: if (!running) kp.liveNow = Core.state("kaiwa_live") || ({})
        onTriggered: kp.liveNow = Core.state("kaiwa_live") || ({})
    }
    readonly property bool liveHere: kp.liveSession && kp.liveNow.running === true && kp.liveNow.sid === kp.s.id
    // ---- the microphone (MicRecorder.qml; absent where Qt Multimedia is not installed) ----
    readonly property bool callMode: kp.s !== null && kp.s.mode === "call" && !kp.liveSession
    Loader { id: micLoader; source: "MicRecorder.qml"; active: kp.visible }
    readonly property var mic: micLoader.status === Loader.Ready ? micLoader.item : null
    readonly property bool recording: kp.mic !== null && kp.mic.recording
    property string micError: ""
    property bool micNeedsKey: false
    // nobody spoke in the turn that just ended (MicRecorder.silent comes right before its failed)
    property bool silentTurn: false
    Connections {
        target: kp.mic
        ignoreUnknownSignals: true
        function onFinished(url) {
            Core.cmd({cmd: "kaiwa_listen", path: url})
            kp.hfAwaitHear = true
            kp.hfSawListening = false
            kp.refresh()
        }
        function onSilent() { kp.silentTurn = true }
        function onFailed(text) {
            const silent = kp.silentTurn
            kp.silentTurn = false
            // hands-free: a turn without speech is no error, it only counts towards the pause of the loop
            if (silent && kp.handsFree) { kp.emptyTurn(); return }
            kp.micError = kp.tr(text)
            kp.hfArmed = false
        }
    }
    // the microphone button (or ✕/A): a press by hand also takes hands-free out of its pause
    function toggleMic() {
        if (!kp.recording) { kp.hfPaused = false; kp.hfEmpty = 0 }
        kp.hfArmed = false
        kp.listen(true)
    }
    // starts or stops a turn. `byHand`: a press of the button (it stops the tutor's voice first: the microphone never
    // records while the tutor speaks, or it would hear the app); otherwise hands-free, which waits for the voice to end.
    function listen(byHand) {
        kp.micError = ""
        kp.micNeedsKey = false
        if (!kp.mic) { kp.micError = kp.tr("The microphone is not available in this version."); return }
        if (kp.recording) { kp.mic.stop(); return }
        if (!byHand && (kp.speakingNow || kp.k.listening === true || kp.k.busy === true)) return
        if (kp.k.canHear !== true) {
            // nothing can hear: a free Gemini key does (whatever the tutor is), or Whisper on this computer (VoicePanel)
            kp.micError = kp.tr(kp.k.hearMissing || "Speaking needs a free Gemini key: make one at aistudio.google.com/apikey and paste it in Conversation > Conversation by voice, or in Preferences > Addons & accounts > Gemini.")
            kp.micNeedsKey = true
            return
        }
        if (byHand) Core.cmd({cmd: "kaiwa_speak", text: ""})
        // every turn ends by itself after the learner's pause (VoiceInput + the core's detector; the pause and the
        // sensitivity are in Settings > Study > Microphone); nothing said in 8 s stops without sending. (Before, only
        // a voice call passed `auto`: a free conversation recorded until the button was pressed again.)
        const voice = Core.state("voice") || ({})
        console.log("[voice] Conversation: microphone open " + (byHand ? "by hand" : "by itself (hands-free)"))
        kp.mic.start(kp.k.micFile || "", Object.assign({}, voice.vad || ({}), {auto: true, device: voice.mic || ""}))
    }

    // ---- hands-free: after the tutor's answer (and its voice) the microphone opens again by itself ----
    // (general.kaiwa_handsfree, the switch in the session's bar). It stops when the page is left, at "End", and after
    // two turns in a row without speech (hfPaused; a press of the microphone goes on).
    readonly property bool handsFree: kp.g.kaiwa_handsfree !== false
    property bool hfPaused: false
    property int hfEmpty: 0
    // an answer came: waiting for its voice to start and end before listening
    property bool hfArmed: false
    property real hfArmedAt: 0
    property bool hfSawSpeech: false
    property real hfQuietSince: 0
    // a recording went to be written down: waiting for the text (none = an empty turn)
    property bool hfAwaitHear: false
    property bool hfSawListening: false
    // the tutor's lines already seen in this session (a new one arms the loop)
    property int hfTutorSeen: 0
    readonly property bool hfOn: kp.handsFree && !kp.hfPaused && kp.visible && kp.s !== null && !(kp.s.ended > 0)
                                 && !kp.liveSession && kp.mic !== null && kp.k.canHear === true
    function hfArm() {
        if (!kp.hfOn) return
        kp.hfArmed = true
        kp.hfArmedAt = Date.now()
        kp.hfSawSpeech = false
        kp.hfQuietSince = 0
    }
    function emptyTurn() {
        kp.hfEmpty += 1
        console.log("[voice] Conversation: a turn without speech (" + kp.hfEmpty + " in a row)")
        if (kp.hfEmpty >= 2) { kp.hfPaused = true; kp.hfArmed = false; console.log("[voice] Conversation: hands-free paused"); return }
        // one more chance
        kp.hfArm()
    }
    Timer {
        interval: 200; repeat: true
        running: kp.hfArmed
        onTriggered: {
            if (!kp.hfOn) { kp.hfArmed = false; return }
            if (kp.recording || kp.k.listening === true || kp.k.busy === true) return
            if (kp.speakingNow) { kp.hfSawSpeech = true; kp.hfQuietSince = 0; return }
            const now = Date.now()
            // the tutor's voice may take a few seconds to start (an online voice is fetched first)
            const voiceOn = kp.g.kaiwa_voice === true || kp.callMode
            if (voiceOn && !kp.hfSawSpeech && now - kp.hfArmedAt < 8000) return
            if (kp.hfQuietSince === 0) { kp.hfQuietSince = now; return }
            if (now - kp.hfQuietSince < (kp.hfSawSpeech ? 600 : 300)) return
            kp.hfArmed = false
            kp.listen(false)
        }
    }
    // the tutor's voice started while the microphone was open (a late answer, a word read aloud): the recording is
    // dropped so the app never hears itself, and hands-free listens again after the voice. (Not in the first second:
    // a press of the microphone stops the voice, and its flag is read only twice a second.)
    onSpeakingNowChanged: {
        if (kp.speakingNow && kp.recording && kp.mic && Date.now() - kp.recStart > 1200) {
            console.log("[voice] Conversation: the tutor's voice started, recording dropped")
            kp.mic.cancel()
            kp.hfArm()
        }
    }
    // a new session (or none): the loop starts fresh and a recording of the old one is dropped
    readonly property real sid: kp.s ? kp.s.id : -1
    onSidChanged: {
        if (kp.recording && kp.mic) kp.mic.cancel()
        kp.hfArmed = false; kp.hfPaused = false; kp.hfEmpty = 0; kp.hfAwaitHear = false
    }
    function endSession() {
        if (kp.recording && kp.mic) kp.mic.cancel()
        kp.hfArmed = false
        Core.cmd({cmd: "kaiwa_end"})
    }
    // □/X: the tutor's last message again (a recording is dropped first; hands-free listens after it)
    function replay() {
        for (let i = kp.lines.length - 1; i >= 0; --i) {
            const l = kp.lines[i] || ({})
            if (l.role === "user") continue
            const wasRec = kp.recording
            if (wasRec && kp.mic) kp.mic.cancel()
            Core.cmd({cmd: "kaiwa_speak", text: String(l.text || "")})
            kp.watchUntil = Date.now() + 15000
            if (wasRec || kp.hfArmed) kp.hfArm()
            return
        }
    }
    // the page's side of the voice: the system voice, the [voice] log lines (VoiceHost.qml; kept once made)
    Loader { id: voiceHost; source: "VoiceHost.qml" }
    // ---- what is happening now, with the seconds waited (Core.state("kaiwa_status"), read twice a second while
    // something happens): listening, writing down the voice, the tutor thinking / writing, the voice speaking ----
    property var st: ({})
    property real recStart: 0
    property real nowMs: 0
    // a few seconds after an answer the voice may start: keep looking
    property real watchUntil: 0
    onRecordingChanged: if (recording) kp.recStart = Date.now()
    readonly property bool sysTalking: voiceHost.item !== null && voiceHost.item.talking === true
    readonly property bool speakingNow: kp.st.speaking === true || kp.sysTalking
    readonly property string stage: kp.recording ? "recording" : ((kp.st.stage || "") !== "" ? kp.st.stage : (kp.speakingNow ? "speaking" : ""))
    readonly property int waited: kp.stage === "recording" ? Math.max(0, Math.round((kp.nowMs - kp.recStart) / 1000))
                                  : Math.max(0, Math.round(((kp.st.now || 0) - (kp.st.since || 0)) / 1000))
    // "Ollama (qwen2.5:3b-instruct)" → "Ollama · qwen2.5:3b-instruct"
    readonly property string aiName: String(kp.st.ai || kp.k.ai || "").replace(/\s*\((.*)\)$/, " · $1")
    function stageText() {
        switch (kp.stage) {
        case "recording": return kp.tr("Listening…")
        case "hearing": return kp.tr("Writing down what you said…")
        case "preparing": return kp.tr("Loading the local AI (%1)…").replace("%1", kp.aiName)
        case "thinking": return kp.tr("The tutor is thinking (%1)…").replace("%1", kp.aiName)
        case "writing": return kp.tr("Writing…")
        case "speaking": return kp.tr("Speaking…")
        }
        return ""
    }
    function pollStatus() {
        kp.nowMs = Date.now()
        kp.st = Core.state("kaiwa_status") || ({})
    }
    Timer {
        interval: 500; repeat: true; triggeredOnStart: true
        running: kp.visible && (kp.recording || kp.k.busy === true || kp.k.listening === true || kp.speakingNow || kp.hfArmed || kp.nowMs < kp.watchUntil)
        onTriggered: kp.pollStatus()
    }
    function cancelWait() {
        if (kp.recording && kp.mic) kp.mic.cancel()
        if (kp.stage === "speaking") Core.cmd({cmd: "kaiwa_speak", text: ""})
        Core.cmd({cmd: "kaiwa_cancel"})
        kp.refresh()
    }
    // a new text heard: sent at once (general.kaiwa_autosend, on by default, and always during a call), else put in
    // the box to read it before sending
    property real heardSeen: -1
    readonly property bool autoSend: kp.callMode || kp.g.kaiwa_autosend !== false
    onKChanged: {
        // the voice may start a moment after an answer: keep looking for a while after any change
        if (kp.s !== null) kp.watchUntil = Date.now() + 15000
        if (kp.visible) kp.pollStatus()
        const h = kp.k.heard || ({})
        const fresh = h.seq !== undefined && kp.heardSeen >= 0 && h.seq !== kp.heardSeen
        if (h.seq !== undefined) kp.heardSeen = h.seq
        if (kp.hfAwaitHear) {
            if (kp.k.listening === true) kp.hfSawListening = true
            if (fresh) { kp.hfAwaitHear = false; kp.hfEmpty = 0 }
            // written down, but nothing came out of it: an empty turn
            else if (kp.hfSawListening && kp.k.listening !== true) { kp.hfAwaitHear = false; if (kp.handsFree) kp.emptyTurn() }
        }
        if (!fresh) return
        const typed = input.text.trim()
        if (kp.autoSend) {
            const text = (typed !== "" ? typed + " " : "") + h.text
            console.log("[voice] Conversation: sent what was heard (" + String(h.text).length + " characters)")
            Core.cmd({cmd: "kaiwa_send", text: text})
            input.text = ""
        } else {
            input.text = (typed !== "" ? input.text + " " : "") + h.text; input.forceActiveFocus(); input.cursorPosition = input.length
        }
    }
    Connections { target: Core; function onPrefsChanged() { if (kp.visible) kp.refresh() } ignoreUnknownSignals: true }
    onVisibleChanged: {
        if (visible) { Core.cmd({cmd: "kaiwa_open", id: kp.s ? kp.s.id : 0}); refresh() }
        // leaving Conversation hangs up a Gemini Live call (it can be called again from the session), and lets go
        // of the local AI and Whisper so the rest of the app stays light (they load again when needed here)
        else {
            if (kp.lv.running === true) Core.cmd({cmd: "kaiwa_live_stop"})
            kp.hfArmed = false
            if (kp.recording && kp.mic) kp.mic.cancel()
            Core.cmd({cmd: "kaiwa_release"})
        }
    }
    Component.onCompleted: if (visible) { Core.cmd({cmd: "kaiwa_open", id: 0}); refresh() }

    // ---- the lines of the open session, as a model that only changes where the session changed ----
    ListModel { id: lineModel }
    property real lineSession: -1
    function syncLines() {
        const ls = kp.lines
        const sid = kp.s ? kp.s.id : -1
        const newSession = sid !== kp.lineSession
        if (newSession) { lineModel.clear(); kp.lineSession = sid }
        while (lineModel.count > ls.length) lineModel.remove(lineModel.count - 1)
        for (let i = 0; i < ls.length; ++i) {
            const l = ls[i] || ({})
            const o = {
                mine: l.role === "user",
                msg: String(l.text || ""),
                reading: String(l.reading || ""),
                romaji: String(l.romaji || ""),
                translation: String(l.translation || ""),
                chk: l.check ? JSON.stringify(l.check) : ""
            }
            if (i < lineModel.count) {
                const c = lineModel.get(i)
                for (const key in o) if (c[key] !== o[key]) lineModel.setProperty(i, key, o[key])
            } else {
                lineModel.append(o)
            }
        }
        // hands-free: a new message of the tutor (not the ones of a session opened again) opens the loop
        const tutor = ls.filter(function(l) { return l && l.role !== "user" }).length
        if (newSession) kp.hfTutorSeen = tutor
        else if (tutor > kp.hfTutorSeen) { kp.hfTutorSeen = tutor; kp.hfArm() }
    }

    function esc(t) { return String(t || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }
    // every word of a text becomes a quiet link (a click explains it)
    function linked(t, color) {
        const col = "" + (color !== undefined ? color : Theme.c.text)
        if (kp.ja) return Kana.linkify(esc(t), col, null)
        // (the entities of esc() stay as they are)
        return esc(t).replace(/(&[a-z]+;)|([A-Za-zÀ-ɏͰ-ϿЀ-ӿ֐-ۿ฀-๿ᄀ-ᇿ぀-ヿ㐀-鿿가-힯'’-]+)/g, function(m, ent, w) {
            if (ent) return ent
            return '<a href="w:' + encodeURIComponent(w) + '" style="text-decoration:none"><span style="color:' + col + '; text-decoration:none">' + w + '</span></a>'
        })
    }
    function wordOf(link) { return decodeURIComponent(link.substring(link.indexOf(":") + 1)) }
    function when(t) { return new Date(t * 1000).toLocaleString(kp.loc, "d MMM, hh:mm") }

    // A flat card in the theme's shape (the corners and the hair line of StudySurface, without its glass, shadows
    // and colour animation: many of them scroll at once here).
    component Card: Rectangle {
        radius: Theme.shape.card
        color: Theme.c.card
        border.width: Theme.id === "windows" || Theme.id === "gnome" ? 1 : 0
        border.color: Theme.id === "windows" ? Theme.c.border : Theme.c.divider
    }
    // A card that is a button.
    component ClickCard: Card {
        id: cc
        signal activated()
        readonly property bool hot: cca.containsMouse
        color: hot ? Theme.c.cardHover : Theme.c.card
        MouseArea { id: cca; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: cc.activated() }
    }
    // A text field of the page.
    component Field: TextField {
        id: fld
        color: Theme.c.text; placeholderTextColor: Theme.c.textDim; font.family: Theme.type.family; font.pixelSize: 13
        background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: fld.activeFocus ? 2 : 0; border.color: Theme.c.accent }
    }

    // ================= home =================
    Item {
        id: home
        visible: kp.s === null
        anchors.fill: parent

        // ---- the header: the language, the level, the tutor, the tabs (it does not scroll) ----
        Column {
            id: head
            x: kp.x0; y: 16; width: kp.cw
            spacing: 10
            Row {
                width: parent.width
                spacing: 12
                Lbl { text: "会話"; size: 26; bold: true; color: Theme.c.accentText; anchors.verticalCenter: parent.verticalCenter }
                Column {
                    width: parent.width - 80
                    anchors.verticalCenter: parent.verticalCenter
                    Lbl { text: kp.tr("Conversation"); size: 20; bold: true }
                    Lbl { width: parent.width; elide: Text.ElideRight; text: kp.k.langName ? kp.tr("Talk with your tutor in %1").replace("%1", kp.k.langName) : kp.tr("Choose the language you study in Preferences > Languages I know."); dim: true; size: 13 }
                }
            }
            Flow {
                width: parent.width
                spacing: 10
                Segmented {
                    model: [{id: "start", label: kp.tr("Start"), icon: "play"}, {id: "history", label: kp.tr("History"), icon: "history"}, {id: "progress", label: kp.tr("Progress"), icon: "stats"}]
                    current: kp.tab
                    onPicked: function(id) { kp.tab = id }
                }
                // the level
                Segmented {
                    visible: (kp.k.lang || "") !== ""
                    model: kp.kLevels
                    current: kp.k.level || ""
                    onPicked: function(id) { kp.set("kaiwa_level", id) }
                }
                // the tutor (the AI behind it)
                Rectangle {
                    visible: (kp.k.lang || "") !== ""
                    height: 34; radius: 17
                    width: aiRow.implicitWidth + 24
                    color: (kp.k.aiMissing || "") !== "" ? Qt.alpha(Theme.c.warning, 0.18) : (aiArea.containsMouse ? Theme.c.buttonHover : Theme.c.fill)
                    Row {
                        id: aiRow
                        anchors.centerIn: parent
                        spacing: 6
                        Icon { name: "message"; size: 16; color: Theme.c.textDim; anchors.verticalCenter: parent.verticalCenter }
                        Lbl { text: kp.tr("Tutor") + ": " + (kp.k.ai || ""); size: 13; anchors.verticalCenter: parent.verticalCenter }
                        Icon { name: "down"; size: 14; color: Theme.c.textDim; anchors.verticalCenter: parent.verticalCenter }
                    }
                    MouseArea { id: aiArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: aiMenu.openBelow(parent, false) }
                    ToolTip.visible: aiArea.containsMouse && (kp.k.aiMissing || "") !== ""
                    ToolTip.text: kp.tr(kp.k.aiMissing || "")
                }
                Field {
                    visible: (kp.k.ai || "").indexOf("Ollama") === 0
                    width: 240; height: 34
                    text: kp.g.kaiwa_ollama || ""
                    placeholderText: kp.tr("Ollama model (qwen2.5:3b-instruct)")
                    onEditingFinished: kp.set("kaiwa_ollama", text)
                }
            }
            Lbl { visible: (kp.k.aiMissing || "") !== ""; width: parent.width; wrapMode: Text.Wrap; text: kp.tr(kp.k.aiMissing || ""); color: Theme.c.warningText; size: 13 }
            Lbl { visible: (kp.k.error || "") !== "" && kp.s === null; width: parent.width; wrapMode: Text.Wrap; text: kp.tr(kp.k.error || ""); color: Theme.c.errorText; size: 13 }
        }
        Rectangle { id: headLine; anchors { top: head.bottom; topMargin: 12; left: parent.left; right: parent.right } height: 1; color: Theme.c.divider }

        // ---- the body: only the open tab is built ----
        Loader {
            id: body
            anchors { top: headLine.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
            sourceComponent: kp.tab === "history" ? historyTab : (kp.tab === "progress" ? progressTab : startTab)
        }
    }

    // what was typed in the Start tab (kept while another tab is open)
    property string topicText: ""
    property int scriptIndex: 0
    property var ownScene: ["", "", ""]

    // ---------- start ----------
    Component {
        id: startTab
        ListView {
            id: startList
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            reuseItems: true
            cacheBuffer: 320
            ScrollBar.vertical: ScrollBar {}
            model: kp.kScenes
            header: Column {
                x: kp.x0; width: kp.cw
                spacing: 12
                topPadding: 16; bottomPadding: 10
                // ===== OllamaPanel (the local AI setup, qt/qml/OllamaPanel.qml): shown while the tutor is Ollama or automatic =====
                OllamaPanel {
                    width: parent.width
                    visible: ["auto", "ollama"].indexOf(kp.g.kaiwa_ai || "auto") >= 0
                }
                // ===== Conversation by voice (Gemini Live, qt/qml/KaiwaVoiceCard.qml): the free key and the options =====
                KaiwaVoiceCard {
                    width: parent.width
                    info: kp.lv
                }
                // ===== VoicePanel (hearing and speaking: cloud or this computer, qt/qml/VoicePanel.qml) =====
                VoicePanel { width: parent.width }
                Flow {
                    width: parent.width
                    spacing: 12
                    readonly property real cardW: kp.wide ? (kp.cw - 36) / 4 : kp.cw
                    // free conversation
                    ClickCard {
                        width: parent.cardW; height: 140
                        onActivated: kp.start({mode: "chat"})
                        Column {
                            x: 16; y: 14; width: parent.width - 32; spacing: 5
                            Lbl { text: "💬"; size: 24 }
                            Lbl { text: kp.tr("Free conversation"); bold: true; size: 16 }
                            Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; size: 13; text: kp.tr("Talk about anything, at your level. Each message is checked.") }
                        }
                    }
                    // a voice call: speak and listen, no typing
                    ClickCard {
                        width: parent.cardW; height: 140
                        onActivated: kp.start({mode: "call"})
                        Column {
                            x: 16; y: 14; width: parent.width - 32; spacing: 5
                            Lbl { text: "📞"; size: 24 }
                            Lbl { text: kp.tr("Voice call"); bold: true; size: 16 }
                            Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; size: 13; text: kp.tr("Talk out loud with the microphone; the tutor answers with its voice.") }
                        }
                    }
                    // a reading story with three questions
                    Card {
                        width: parent.cardW; height: 140
                        Column {
                            x: 16; y: 12; width: parent.width - 32; spacing: 6
                            Row { spacing: 8; Lbl { text: "📖"; size: 20 } Lbl { text: kp.tr("Story time"); bold: true; size: 16; anchors.verticalCenter: parent.verticalCenter } }
                            Field {
                                id: topic
                                width: parent.width
                                placeholderText: kp.tr("Topic (optional)")
                                Component.onCompleted: text = kp.topicText
                                onTextChanged: kp.topicText = text
                            }
                            Row {
                                spacing: 6
                                ComboBox {
                                    id: script
                                    visible: kp.ja
                                    width: 150; height: 32
                                    model: [kp.tr("Normal kanji"), kp.tr("Hiragana only"), kp.tr("Katakana words"), kp.tr("Harder kanji")]
                                    font.pixelSize: 12
                                    Component.onCompleted: currentIndex = kp.scriptIndex
                                    onActivated: function(i) { kp.scriptIndex = i }
                                }
                                FlatButton {
                                    kind: "suggested"; glyph: "play"; text: kp.tr("Read"); implicitHeight: 32
                                    onClicked: kp.start({mode: "story", topic: topic.text, script: ["normal", "hiragana", "katakana", "stretch"][script.currentIndex]})
                                }
                            }
                        }
                    }
                    // a scene of your own
                    Card {
                        width: parent.cardW; height: kp.wide ? 140 : 180
                        Column {
                            x: 16; y: 12; width: parent.width - 32; spacing: 5
                            Row { spacing: 8; Lbl { text: "🎭"; size: 20 } Lbl { text: kp.tr("Your own scene"); bold: true; size: 16; anchors.verticalCenter: parent.verticalCenter } }
                            Repeater {
                                model: [kp.tr("Where, what happens"), kp.tr("The tutor plays…"), kp.tr("You play…")]
                                delegate: Field {
                                    required property var modelData
                                    required property int index
                                    width: parent.width; height: 26
                                    font.pixelSize: 12
                                    placeholderText: modelData
                                    Component.onCompleted: text = kp.ownScene[index] || ""
                                    onTextChanged: kp.ownScene[index] = text
                                }
                            }
                        }
                        FlatButton {
                            anchors { right: parent.right; top: parent.top; margins: 10 }
                            kind: "suggested"; glyph: "play"; implicitWidth: 34; implicitHeight: 30
                            ToolTip.visible: hovered; ToolTip.text: kp.tr("Start the scene")
                            onClicked: kp.start({mode: "custom", custom: kp.ownScene.slice(0, 3)})
                        }
                    }
                }
                Lbl { visible: kp.kScenes.length > 0; text: kp.tr("Role-play"); bold: true; size: 18; topPadding: 6 }
            }
            // one theme of role-play scenes
            delegate: Column {
                id: catCol
                required property var modelData
                x: kp.x0; width: kp.cw
                spacing: 8
                bottomPadding: 14
                Lbl { text: kp.tr(catCol.modelData.cat); dim: true; size: 13 }
                Flow {
                    width: parent.width
                    spacing: 10
                    Repeater {
                        model: catCol.modelData.items
                        delegate: ClickCard {
                            id: scCard
                            required property var modelData
                            width: kp.wide ? (kp.cw - 30) / 4 : (kp.cw - 10) / 2
                            height: 80
                            onActivated: kp.start({mode: "roleplay", scene: scCard.modelData.id})
                            Lbl { x: 12; y: 12; text: scCard.modelData.icon; size: 22 }
                            Column {
                                x: 48; y: 12; width: parent.width - 60
                                Lbl { width: parent.width; elide: Text.ElideRight; text: kp.tr(scCard.modelData.title); bold: true; size: 14 }
                                Lbl { width: parent.width; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight; text: kp.tr(scCard.modelData.goal); dim: true; size: 11 }
                            }
                        }
                    }
                }
            }
            footer: Column {
                x: kp.x0; width: kp.cw
                spacing: 12
                bottomPadding: 28
                Lbl { visible: kp.kLessons.length > 0; text: kp.tr("Guided lessons"); bold: true; size: 18; topPadding: 4 }
                Flow {
                    width: parent.width
                    spacing: 10
                    Repeater {
                        model: kp.kLessons
                        delegate: ClickCard {
                            id: lsCard
                            required property var modelData
                            required property int index
                            width: kp.wide ? (kp.cw - 40) / 5 : (kp.cw - 10) / 2
                            height: 88
                            onActivated: kp.start({mode: "lesson", scene: lsCard.modelData.id})
                            Column {
                                x: 12; y: 10; width: parent.width - 24; spacing: 3
                                Row { spacing: 6; Lbl { text: lsCard.modelData.icon; size: 18 } Lbl { text: (lsCard.index + 1) + "."; dim: true; size: 12; anchors.verticalCenter: parent.verticalCenter } }
                                Lbl { width: parent.width; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight; text: kp.tr(lsCard.modelData.title); bold: true; size: 13 }
                            }
                            ToolTip.visible: lsCard.hot; ToolTip.delay: 500; ToolTip.text: kp.tr(lsCard.modelData.goal)
                        }
                    }
                }
                // about you, for the tutor
                Card {
                    width: parent.width
                    height: aboutCol.implicitHeight + 24
                    Column {
                        id: aboutCol
                        x: 16; y: 12; width: parent.width - 32; spacing: 8
                        Lbl { text: kp.tr("About you (for the tutor)"); bold: true; size: 14 }
                        Flow {
                            width: parent.width; spacing: 10
                            Field {
                                width: 200
                                text: kp.g.kaiwa_name || ""
                                placeholderText: kp.tr("Your name")
                                onEditingFinished: kp.set("kaiwa_name", text)
                            }
                            Field {
                                width: Math.min(460, aboutCol.width - 210)
                                text: kp.g.kaiwa_interests || ""
                                placeholderText: kp.tr("What you like (anime, cooking, games…)")
                                onEditingFinished: kp.set("kaiwa_interests", text)
                            }
                        }
                    }
                }
            }
        }
    }

    // ---------- history ----------
    Component {
        id: historyTab
        ListView {
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            reuseItems: true
            spacing: 8
            ScrollBar.vertical: ScrollBar {}
            model: kp.kHistory
            header: Item {
                width: kp.cw; height: kp.kHistory.length === 0 ? 60 : 16
                Lbl { x: kp.x0; y: 20; visible: kp.kHistory.length === 0; text: kp.tr("No conversations yet."); dim: true }
            }
            footer: Item { width: 1; height: 24 }
            delegate: ClickCard {
                id: hCard
                required property var modelData
                x: kp.x0; width: kp.cw
                height: hCol.implicitHeight + 20
                onActivated: Core.cmd({cmd: "kaiwa_open", id: hCard.modelData.id})
                Column {
                    id: hCol
                    x: 14; y: 10; width: parent.width - 70; spacing: 3
                    Lbl { width: parent.width; elide: Text.ElideRight; text: kp.tr(hCard.modelData.title); bold: true; size: 14 }
                    Lbl { text: kp.when(hCard.modelData.started) + "  ·  " + hCard.modelData.minutes + " min  ·  " + hCard.modelData.lines + " " + kp.tr("messages") + (hCard.modelData.ended > 0 ? "" : "  ·  " + kp.tr("open")); dim: true; size: 12 }
                    Lbl { visible: text !== ""; width: parent.width; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight; text: hCard.modelData.report && hCard.modelData.report.summary ? hCard.modelData.report.summary : ""; size: 12 }
                }
                FlatButton {
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter; rightMargin: 10 }
                    kind: "flat"; glyph: "trash"; implicitWidth: 36; implicitHeight: 34
                    ToolTip.visible: hovered; ToolTip.text: kp.tr("Delete this conversation")
                    onClicked: Core.cmd({cmd: "kaiwa_delete", id: hCard.modelData.id})
                }
            }
        }
    }

    // ---------- progress ----------
    Component {
        id: progressTab
        Flickable {
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            contentHeight: prog.implicitHeight + 44
            ScrollBar.vertical: ScrollBar {}
            Column {
                id: prog
                x: kp.x0; y: 16; width: kp.cw
                spacing: 12
                readonly property var p: kp.kProgress
                readonly property real peak: Math.max(1, ((prog.p.kinds || [])[0] || {count: 1}).count)
                Flow {
                    width: parent.width
                    spacing: 10
                    Repeater {
                        model: [
                            {icon: "🔥", n: prog.p.streak || 0, t: kp.tr("days in a row")},
                            {icon: "⏱️", n: prog.p.minutes || 0, t: kp.tr("minutes talking")},
                            {icon: "💬", n: prog.p.sessions || 0, t: kp.tr("conversations")},
                            {icon: "✍️", n: prog.p.written || 0, t: kp.tr("messages written")}
                        ]
                        delegate: Card {
                            id: stat
                            required property var modelData
                            width: kp.wide ? (kp.cw - 30) / 4 : (kp.cw - 10) / 2
                            height: 82
                            Column {
                                x: 14; y: 12; spacing: 2
                                Row { spacing: 6; Lbl { text: stat.modelData.icon; size: 20 } Lbl { text: "" + stat.modelData.n; bold: true; size: 24 } }
                                Lbl { text: stat.modelData.t; dim: true; size: 12 }
                            }
                        }
                    }
                }
                Lbl { visible: (prog.p.kinds || []).length > 0; text: kp.tr("Kinds of mistakes"); bold: true; size: 16 }
                Repeater {
                    model: prog.p.kinds || []
                    delegate: Row {
                        id: kindRow
                        required property var modelData
                        spacing: 10
                        Lbl { width: 120; text: kp.tr(kindRow.modelData.kind); size: 13; anchors.verticalCenter: parent.verticalCenter }
                        Rectangle { width: Math.max(6, (kp.cw - 200) * kindRow.modelData.count / prog.peak); height: 12; radius: 6; color: Theme.c.accent; anchors.verticalCenter: parent.verticalCenter }
                        Lbl { text: "" + kindRow.modelData.count; dim: true; size: 12; anchors.verticalCenter: parent.verticalCenter }
                    }
                }
                Lbl { visible: kp.kMistakes.length > 0; text: kp.tr("Recent mistakes"); bold: true; size: 16 }
                Repeater {
                    model: kp.kMistakes
                    delegate: Column {
                        id: mis
                        required property var modelData
                        width: kp.cw
                        Lbl { width: parent.width; wrapMode: Text.Wrap; textFormat: Text.StyledText; size: 14; text: "<s>" + kp.esc(mis.modelData.wrong) + "</s>  →  <b>" + kp.esc(mis.modelData.right) + "</b>" }
                        Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; size: 12; text: mis.modelData.why || "" }
                    }
                }
                Lbl { visible: kp.kWords.length > 0; text: kp.tr("Words learned"); bold: true; size: 16 }
                Flow {
                    width: parent.width; spacing: 8
                    Repeater {
                        model: kp.kWords
                        delegate: Rectangle {
                            id: wchip
                            required property var modelData
                            height: 30; radius: 15; width: wl.implicitWidth + 20
                            color: Theme.c.fill
                            Lbl { id: wl; anchors.centerIn: parent; size: 13; text: wchip.modelData.word + (wchip.modelData.meaning ? "  ·  " + wchip.modelData.meaning : "") }
                        }
                    }
                }
            }
        }
    }

    readonly property var aiIds: ["auto", "claude", "gemini", "openai", "ollama"]
    PopMenu {
        id: aiMenu
        menuWidth: 260
        items: [kp.tr("Automatic"), "Claude", "Gemini", "GPT (OpenAI)", kp.tr("Ollama (this computer)")].map(function(t, i) { return {label: t, checked: (kp.g.kaiwa_ai || "auto") === kp.aiIds[i]} })
        onPicked: function(i) { kp.set("kaiwa_ai", kp.aiIds[i]) }
    }

    // ================= a session =================
    Item {
        id: talk
        visible: kp.s !== null
        anchors.fill: parent
        readonly property bool ended: kp.s !== null && kp.s.ended > 0
        // top: back, the title, what shows under the tutor's messages, the voice, end
        Item {
            id: bar
            x: kp.x0; width: kp.cw; height: 52
            FlatButton {
                id: backBtn
                anchors.verticalCenter: parent.verticalCenter
                kind: "flat"; glyph: "left"; implicitWidth: 38; implicitHeight: 36
                ToolTip.visible: hovered; ToolTip.text: kp.tr("Back to Conversation")
                onClicked: Core.cmd({cmd: "kaiwa_open", id: 0})
                PadMark { visible: kp.padActive; ps: kp.padPs; button: "B" }
            }
            Column {
                anchors { left: backBtn.right; leftMargin: 8; right: tools.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                Lbl { width: parent.width; elide: Text.ElideRight; text: kp.s ? kp.tr(kp.s.title) : ""; bold: true; size: 16 }
                Lbl { text: kp.s ? (kp.k.langName || "") + " · " + (kp.s.level || "") + " · " + (kp.liveSession ? "Gemini Live" : (kp.k.ai || "")) : ""; dim: true; size: 12 }
            }
            Row {
                id: tools
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                spacing: 4
                Repeater {
                    model: (kp.ja ? [{k: "kaiwa_reading", g: "translate", t: "Reading in kana", l: "ふ"}, {k: "kaiwa_romaji", g: "", t: "Romaji", l: "Ro"}] : []).concat([{k: "kaiwa_translation", g: "translate", t: "Translation", l: ""}])
                    delegate: FlatButton {
                        id: optBtn
                        required property var modelData
                        readonly property bool on: kp.g[optBtn.modelData.k] === true
                        kind: on ? "suggested" : "flat"
                        glyph: optBtn.modelData.l === "" ? optBtn.modelData.g : ""
                        text: optBtn.modelData.l
                        implicitWidth: 38; implicitHeight: 34
                        ToolTip.visible: hovered; ToolTip.text: kp.tr(optBtn.modelData.t)
                        onClicked: {
                            kp.set(optBtn.modelData.k, !on)
                            // a spoken session: the lines heard get their reading / translation now
                            if (!on && kp.liveSession) Core.cmd({cmd: "kaiwa_annotate"})
                        }
                        PadMark {
                            ps: kp.padPs
                            button: optBtn.modelData.k === "kaiwa_translation" ? "RB" : "LB"
                            visible: kp.padActive && (optBtn.modelData.k === "kaiwa_translation" || optBtn.modelData.k === "kaiwa_romaji")
                        }
                    }
                }
                // hands-free: the microphone opens again by itself after each answer (and its voice)
                FlatButton {
                    visible: !kp.liveSession
                    readonly property bool on: kp.handsFree
                    kind: on ? "suggested" : "flat"; glyph: "repeat"
                    text: kp.wide ? kp.tr("Hands-free") : ""
                    implicitHeight: 34
                    ToolTip.visible: hovered; ToolTip.text: kp.tr("Hands-free: after each answer the microphone opens again by itself")
                    onClicked: { kp.set("kaiwa_handsfree", !on); kp.hfPaused = false; kp.hfEmpty = 0; if (on) kp.hfArmed = false }
                }
                FlatButton {
                    visible: !kp.liveSession
                    readonly property bool on: kp.g.kaiwa_voice === true
                    kind: on ? "suggested" : "flat"; glyph: on ? "volume" : "mute"
                    implicitWidth: 38; implicitHeight: 34
                    ToolTip.visible: hovered; ToolTip.text: kp.tr("Read the tutor's messages aloud")
                    onClicked: kp.set("kaiwa_voice", !on)
                }
                FlatButton {
                    visible: !kp.liveSession
                    readonly property bool on: kp.g.kaiwa_check !== false
                    kind: on ? "suggested" : "flat"; glyph: "check"
                    implicitWidth: 38; implicitHeight: 34
                    ToolTip.visible: hovered; ToolTip.text: kp.tr("Check each of my messages")
                    onClicked: kp.set("kaiwa_check", !on)
                }
                FlatButton {
                    visible: !talk.ended
                    kind: "flat"; glyph: "close"; text: kp.wide ? kp.tr("End") : ""
                    implicitHeight: 34
                    ToolTip.visible: hovered && !kp.wide; ToolTip.text: kp.tr("End and see the report")
                    onClicked: kp.endSession()
                    PadMark { visible: kp.padActive; ps: kp.padPs; button: "Select" }
                }
            }
        }
        Rectangle { anchors { top: bar.bottom; left: parent.left; right: parent.right } height: 1; color: Theme.c.divider }

        ListView {
            id: list
            anchors { top: bar.bottom; topMargin: 1 + kp.padReserve; bottom: composer.top; left: parent.left; right: parent.right }
            clip: true
            spacing: 10
            reuseItems: true
            boundsBehavior: Flickable.StopAtBounds
            model: lineModel
            onCountChanged: Qt.callLater(function() { list.positionViewAtEnd() })
            ScrollBar.vertical: ScrollBar {}
            header: Item { width: 1; height: 14 }
            footer: Column {
                x: kp.x0; width: kp.cw
                spacing: 10
                topPadding: 4; bottomPadding: 16
                // what is happening now: the stage, the seconds waited, and a way out
                Rectangle {
                    id: statusBox
                    visible: kp.stage !== "" && !kp.liveSession
                    width: Math.min(kp.cw, stCol.implicitWidth + 24 + stCancel.implicitWidth + 12)
                    height: Math.max(stCol.implicitHeight, stCancel.implicitHeight) + 16
                    radius: 14
                    color: Theme.c.fill3
                    Row {
                        id: stDots
                        x: 12; anchors.verticalCenter: parent.verticalCenter
                        spacing: 4
                        Repeater {
                            model: 3
                            delegate: Rectangle {
                                id: dot
                                required property int index
                                width: 6; height: 6; radius: 3; color: kp.stage === "recording" ? Theme.c.error : Theme.c.accent
                                SequentialAnimation on opacity { running: statusBox.visible; loops: Animation.Infinite; PauseAnimation { duration: dot.index * 160 } NumberAnimation { from: 0.25; to: 1; duration: 320 } NumberAnimation { from: 1; to: 0.25; duration: 320 } }
                            }
                        }
                    }
                    Column {
                        id: stCol
                        anchors { left: stDots.right; leftMargin: 10; verticalCenter: parent.verticalCenter }
                        width: Math.min(implicitWidth, kp.cw - 24 - stDots.width - stCancel.implicitWidth - 22)
                        spacing: 2
                        Lbl {
                            width: Math.min(implicitWidth, kp.cw - 140); wrapMode: Text.Wrap; size: 13
                            text: kp.stageText() + (kp.stage !== "speaking" ? "  " + kp.waited + " s" : "")
                        }
                        Lbl {
                            visible: (kp.stage === "thinking" || kp.stage === "preparing") && kp.waited >= 30
                            width: Math.min(implicitWidth, kp.cw - 140); wrapMode: Text.Wrap; size: 12; dim: true
                            text: String(kp.st.ai || "").indexOf("Ollama") === 0
                                ? kp.tr("A model on this computer can take a minute, mostly the first time (it is loaded into memory).")
                                : kp.tr("The answer is taking longer than usual.")
                        }
                    }
                    FlatButton {
                        id: stCancel
                        anchors { right: parent.right; rightMargin: 6; verticalCenter: parent.verticalCenter }
                        kind: "flat"; glyph: "close"; implicitHeight: 30
                        text: kp.stage === "speaking" ? kp.tr("Stop") : kp.tr("Cancel")
                        onClicked: kp.cancelWait()
                    }
                }
                // hands-free stopped after two turns without speech
                Rectangle {
                    visible: kp.handsFree && kp.hfPaused && kp.s !== null && !(kp.s.ended > 0) && !kp.liveSession
                    width: parent.width; height: hfRow.implicitHeight + 16; radius: 12
                    color: Theme.c.fill3
                    Row {
                        id: hfRow
                        x: 12; y: 8; width: parent.width - 24; spacing: 8
                        Icon { name: "mic"; size: 16; color: Theme.c.textDim; anchors.verticalCenter: parent.verticalCenter }
                        Lbl {
                            width: parent.width - 24; wrapMode: Text.Wrap; size: 13; dim: true
                            text: kp.padActive ? kp.tr("Hands-free paused: no speech in two turns. Press ✕/A to talk again.")
                                               : kp.tr("Hands-free paused: no speech in two turns. Tap the microphone to talk again.")
                        }
                    }
                }
                HeardStrip { width: parent.width; sid: kp.s ? kp.s.id : -1; ja: kp.ja; showRomaji: kp.g.kaiwa_romaji === true }
                Lbl { visible: kp.micError !== ""; width: parent.width; wrapMode: Text.Wrap; text: kp.micError; color: Theme.c.errorText; size: 13 }
                FlatButton {
                    visible: kp.micNeedsKey || (kp.liveSession && !kp.liveHere && kp.lv.hasKey !== true)
                    kind: "suggested"; glyph: "external"; text: kp.tr("Get a free key"); implicitHeight: 32
                    onClicked: Qt.openUrlExternally(kp.lv.keyUrl || "https://aistudio.google.com/apikey")
                }
                // a call: the words of the turn in progress (they become lines when the turn ends)
                Rectangle {
                    visible: kp.liveHere && (kp.liveNow.tutor || "") !== ""
                    width: Math.min(kp.cw * 0.82, ptT.implicitWidth + 28); height: ptT.implicitHeight + 20
                    radius: 16; color: Theme.c.card; border.width: 1; border.color: Theme.c.divider
                    opacity: 0.85
                    Text { id: ptT; x: 14; y: 10; width: Math.min(kp.cw * 0.82 - 28, implicitWidth); wrapMode: Text.Wrap; text: kp.liveNow.tutor || ""; color: Theme.c.text; font.pixelSize: 17; font.family: Theme.type.family }
                }
                Rectangle {
                    visible: kp.liveHere && (kp.liveNow.user || "") !== ""
                    x: kp.cw - width
                    width: Math.min(kp.cw * 0.82, puT.implicitWidth + 28); height: puT.implicitHeight + 20
                    radius: 16; color: Theme.c.accent
                    opacity: 0.7
                    Text { id: puT; x: 14; y: 10; width: Math.min(kp.cw * 0.82 - 28, implicitWidth); wrapMode: Text.Wrap; text: kp.liveNow.user || ""; color: Theme.c.accentFg; font.pixelSize: 17; font.family: Theme.type.family }
                }
                Row {
                    visible: (kp.k.error || "") !== ""
                    spacing: 8
                    Column {
                        width: Math.min(kp.cw - 120, Math.max(errMain.implicitWidth, errDetail.visible ? errDetail.implicitWidth : 0))
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2
                        Lbl { id: errMain; width: Math.min(kp.cw - 120, implicitWidth); wrapMode: Text.Wrap; text: kp.tr(kp.k.error || ""); color: Theme.c.errorText; size: 13 }
                        // the technical side (model, seconds, the raw error, the microphone's level): for the log and for help
                        Lbl { id: errDetail; visible: (kp.k.errorDetail || "") !== ""; width: Math.min(kp.cw - 120, implicitWidth); wrapMode: Text.Wrap; text: kp.k.errorDetail || ""; dim: true; size: 11 }
                    }
                    FlatButton { kind: "flat"; glyph: "refresh"; text: kp.tr("Try again"); implicitHeight: 32; anchors.verticalCenter: parent.verticalCenter; onClicked: Core.cmd({cmd: "kaiwa_retry"}) }
                }
                // the report
                Loader {
                    width: parent.width
                    active: talk.ended
                    visible: active
                    sourceComponent: Card {
                        id: repCard
                        height: rep.implicitHeight + 28
                        readonly property var r: kp.s && kp.s.report ? kp.s.report : null
                        Column {
                            id: rep
                            x: 16; y: 14; width: parent.width - 32; spacing: 8
                            Lbl { text: kp.tr("Report"); bold: true; size: 18 }
                            Lbl { visible: kp.k.reporting === true; text: kp.tr("Writing the report…"); dim: true }
                            Lbl { visible: !!repCard.r; width: parent.width; wrapMode: Text.Wrap; text: repCard.r ? repCard.r.summary || "" : "" }
                            Repeater {
                                model: repCard.r ? (repCard.r.strengths || []).map(function(x) { return "✅  " + x }).concat((repCard.r.improve || []).map(function(x) { return "🎯  " + x })) : []
                                delegate: Lbl { required property var modelData; width: rep.width; wrapMode: Text.Wrap; size: 13; text: modelData }
                            }
                            Flow {
                                width: parent.width; spacing: 8
                                Repeater {
                                    model: repCard.r ? (repCard.r.words || []) : []
                                    delegate: Rectangle {
                                        id: rwChip
                                        required property var modelData
                                        height: 32; radius: 16; width: rw.implicitWidth + 44
                                        color: Theme.c.fill
                                        Lbl { id: rw; x: 12; anchors.verticalCenter: parent.verticalCenter; size: 13; text: rwChip.modelData.word + (rwChip.modelData.reading ? " (" + rwChip.modelData.reading + ")" : "") + "  ·  " + (rwChip.modelData.meaning || "") }
                                        FlatButton {
                                            anchors { right: parent.right; verticalCenter: parent.verticalCenter; rightMargin: 2 }
                                            kind: "flat"; glyph: "plus"; implicitWidth: 28; implicitHeight: 28
                                            ToolTip.visible: hovered; ToolTip.text: kp.tr("Make a flashcard")
                                            onClicked: Core.cmd({cmd: "kaiwa_card", word: rwChip.modelData.word, reading: rwChip.modelData.reading || "", meaning: rwChip.modelData.meaning || ""})
                                        }
                                    }
                                }
                            }
                            FlatButton { kind: "suggested"; glyph: "plus"; text: kp.tr("New conversation"); onClicked: Core.cmd({cmd: "kaiwa_open", id: 0}) }
                        }
                    }
                }
            }
            delegate: Item {
                id: row
                required property bool mine
                required property string msg
                required property string reading
                required property string romaji
                required property string translation
                required property string chk
                // parsed only when the check of this line changes
                readonly property var verdict: row.chk !== "" ? JSON.parse(row.chk) : null
                // the tutor's text with every word a link, made again only when the text changes (and the learner's
                // in a spoken session: what Gemini heard, with its reading)
                readonly property bool rich: !row.mine || kp.liveSession
                readonly property string html: !row.rich ? "" : (row.mine ? kp.linked(row.msg, Theme.c.accentFg) : kp.linked(row.msg))
                readonly property real maxW: kp.cw * 0.82
                width: list.width
                height: bubble.height + (chkLoader.active ? chkLoader.height + 6 : 0)
                Rectangle {
                    id: bubble
                    x: row.mine ? kp.x0 + kp.cw - width : kp.x0
                    width: Math.min(row.maxW, Math.max(bCol.implicitWidth, 60) + 28)
                    height: bCol.implicitHeight + 20
                    radius: 16
                    color: row.mine ? Theme.c.accent : Theme.c.card
                    border.width: row.mine ? 0 : 1
                    border.color: Theme.c.divider
                    Column {
                        id: bCol
                        x: 14; y: 10
                        width: Math.min(row.maxW - 28, implicitWidth)
                        spacing: 3
                        Text {
                            id: msgText
                            width: Math.min(row.maxW - 28 - (row.mine ? 0 : 30), implicitWidth)
                            wrapMode: Text.Wrap
                            textFormat: row.rich ? Text.RichText : Text.PlainText
                            text: row.rich ? row.html : row.msg
                            color: row.mine ? Theme.c.accentFg : Theme.c.text
                            font.pixelSize: 17; font.family: Theme.type.family
                            lineHeight: 1.15
                            onLinkActivated: function(link) { Core.cmd({cmd: "kaiwa_word", word: kp.wordOf(link), sentence: row.msg}); wordPop.open() }
                            HoverHandler { enabled: row.rich; cursorShape: msgText.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.IBeamCursor }
                        }
                        Lbl { visible: row.rich && kp.ja && kp.g.kaiwa_reading === true && row.reading !== ""; width: msgText.width; wrapMode: Text.Wrap; text: row.reading; color: row.mine ? Theme.c.accentFg : Theme.c.textDim; opacity: row.mine ? 0.85 : 1; size: 13 }
                        Lbl { visible: row.rich && kp.ja && kp.g.kaiwa_romaji === true && row.romaji !== ""; width: msgText.width; wrapMode: Text.Wrap; text: row.romaji; color: row.mine ? Theme.c.accentFg : Theme.c.textDim; opacity: row.mine ? 0.85 : 1; size: 13; font.italic: true }
                        Lbl { visible: row.rich && kp.g.kaiwa_translation === true && row.translation !== ""; width: msgText.width; wrapMode: Text.Wrap; text: row.translation; color: row.mine ? Theme.c.accentFg : Theme.c.accentText; opacity: row.mine ? 0.85 : 1; size: 13 }
                    }
                    // listen (the tutor's messages only)
                    Loader {
                        active: !row.mine
                        anchors { right: parent.right; top: parent.top; margins: 4 }
                        sourceComponent: FlatButton {
                            kind: "flat"; glyph: "volume"; iconSize: 14; implicitWidth: 28; implicitHeight: 28
                            ToolTip.visible: hovered; ToolTip.text: kp.tr("Listen")
                            onClicked: Core.cmd({cmd: "kaiwa_speak", text: row.msg})
                        }
                    }
                }
                // the check of the learner's message (built only where there is one)
                Loader {
                    id: chkLoader
                    active: row.mine && row.verdict !== null && (row.verdict.ok !== undefined || row.verdict.error !== undefined)
                    anchors.top: bubble.bottom; anchors.topMargin: 6
                    x: kp.x0 + kp.cw - width
                    sourceComponent: Rectangle {
                        id: chkBox
                        readonly property var v: row.verdict || ({})
                        readonly property bool ok: v.ok === true || (v.errors || []).length === 0
                        width: Math.min(row.maxW, chkCol.implicitWidth + 24)
                        height: chkCol.implicitHeight + 16
                        radius: 12
                        color: Qt.alpha(ok ? Theme.c.success : Theme.c.warning, 0.14)
                        Column {
                            id: chkCol
                            x: 12; y: 8
                            width: Math.min(row.maxW - 24, implicitWidth)
                            spacing: 4
                            Lbl { visible: chkBox.v.error !== undefined; text: chkBox.v.error ? kp.tr(chkBox.v.error) : ""; dim: true; size: 12 }
                            Lbl {
                                visible: !chkBox.ok && (chkBox.v.corrected || "") !== ""
                                width: Math.min(row.maxW - 24, implicitWidth); wrapMode: Text.Wrap
                                text: "✏️  " + (chkBox.v.corrected || ""); size: 15
                            }
                            Repeater {
                                model: chkBox.v.errors || []
                                delegate: Lbl {
                                    id: errLine
                                    required property var modelData
                                    width: Math.min(row.maxW - 24, implicitWidth); wrapMode: Text.Wrap
                                    textFormat: Text.StyledText; size: 13
                                    text: "<s>" + kp.esc(errLine.modelData.wrong) + "</s> → <b>" + kp.esc(errLine.modelData.right) + "</b>  " + kp.esc(errLine.modelData.why || "")
                                }
                            }
                            Lbl {
                                visible: (chkBox.v.praise || "") !== ""
                                width: Math.min(row.maxW - 24, implicitWidth); wrapMode: Text.Wrap
                                text: (chkBox.ok ? "✅  " : "💡  ") + (chkBox.v.praise || ""); dim: true; size: 12
                            }
                        }
                    }
                }
            }
        }

        // the hints, over the box where the learner writes
        Rectangle {
            id: hintsBox
            readonly property var h: Array.isArray(kp.k.hints) ? kp.k.hints : []
            visible: !talk.ended && (h.length > 0 || kp.k.hintsBusy === true || (!!kp.k.hints && kp.k.hints.error !== undefined))
            anchors { bottom: composer.top; bottomMargin: 6 }
            x: kp.x0; width: kp.cw
            height: hCol2.implicitHeight + 16
            radius: Theme.shape.popover
            color: Theme.c.popover
            border.width: 1; border.color: Theme.c.border
            Column {
                id: hCol2
                x: 10; y: 8; width: parent.width - 20; spacing: 4
                Lbl { visible: kp.k.hintsBusy === true; text: kp.tr("Thinking of replies…"); dim: true; size: 12 }
                Lbl { visible: !!(kp.k.hints && kp.k.hints.error); text: kp.k.hints && kp.k.hints.error ? kp.tr(kp.k.hints.error) : ""; color: Theme.c.errorText; size: 12 }
                Repeater {
                    model: hintsBox.visible ? hintsBox.h : []
                    delegate: Rectangle {
                        id: hint
                        required property var modelData
                        width: hCol2.width; height: hc.implicitHeight + 10; radius: 8
                        color: ha.containsMouse ? Theme.c.hover : "transparent"
                        Column {
                            id: hc
                            x: 8; y: 5; width: parent.width - 16
                            Lbl { width: parent.width; wrapMode: Text.Wrap; text: hint.modelData.text || ""; size: 15 }
                            Lbl { visible: kp.ja && (hint.modelData.reading || "") !== ""; width: parent.width; wrapMode: Text.Wrap; text: hint.modelData.reading || ""; dim: true; size: 12 }
                            Lbl { width: parent.width; wrapMode: Text.Wrap; text: hint.modelData.translation || ""; color: Theme.c.accentText; size: 12 }
                        }
                        MouseArea { id: ha; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { if (kp.liveSession) return; input.text = hint.modelData.text || ""; input.forceActiveFocus(); input.cursorPosition = input.length } }
                    }
                }
            }
        }

        // where the learner writes
        Item {
            id: composer
            anchors.bottom: parent.bottom
            x: kp.x0; width: kp.cw
            height: talk.ended ? 0 : (kp.liveSession ? liveBar.implicitHeight : (kp.callMode ? 120 : Math.min(160, input.implicitHeight + 8) + 24))
            visible: !talk.ended
            // a spoken session (Gemini Live): the bar of the call instead of the box
            KaiwaLiveBar {
                id: liveBar
                visible: kp.liveSession
                width: parent.width
                live: kp.liveNow
                info: kp.lv
                sess: kp.s
            }
            FlatButton {
                id: hintBtn
                visible: !kp.callMode && !kp.liveSession
                anchors { left: parent.left; verticalCenter: inputBox.verticalCenter }
                kind: "flat"; glyph: "lightbulb"; implicitWidth: 40; implicitHeight: 40
                enabled: kp.lines.length > 0 && kp.k.hintsBusy !== true
                ToolTip.visible: hovered; ToolTip.text: kp.tr("Hint: three replies you could send")
                onClicked: Core.cmd({cmd: "kaiwa_hint"})
                PadMark { visible: kp.padActive; ps: kp.padPs; button: "Y" }
            }
            FlatButton {
                id: micBtn
                visible: !kp.callMode && !kp.liveSession
                anchors { left: hintBtn.right; leftMargin: 2; verticalCenter: inputBox.verticalCenter }
                kind: kp.recording ? "suggested" : "flat"; glyph: "mic"; implicitWidth: 40; implicitHeight: 40
                enabled: kp.k.listening !== true
                ToolTip.visible: hovered; ToolTip.text: kp.recording ? kp.tr("Stop and write what I said") : kp.tr("Speak (it stops by itself when you pause)")
                onClicked: kp.toggleMic()
                PadMark { visible: kp.padActive; ps: kp.padPs; button: "A" }
            }
            // a call: one big button, the voice goes straight to the tutor
            Column {
                visible: kp.callMode
                anchors.centerIn: parent
                spacing: 6
                Item {
                    width: 72; height: 72
                    anchors.horizontalCenter: parent.horizontalCenter
                    Rectangle {
                        anchors.centerIn: parent
                        width: 72 + (kp.mic ? kp.mic.level * 18 : 0); height: width; radius: width / 2
                        visible: kp.recording
                        color: Qt.alpha(Theme.c.accent, 0.25)
                    }
                    Rectangle {
                        anchors.fill: parent; radius: 36
                        color: kp.recording ? Theme.c.error : Theme.c.accent
                        opacity: callMic.enabled ? 1 : 0.5
                        Icon { anchors.centerIn: parent; name: kp.recording ? "pause" : "mic"; size: 30; color: kp.recording ? Theme.c.errorFg : Theme.c.accentFg }
                        MouseArea { id: callMic; anchors.fill: parent; enabled: kp.k.listening !== true && kp.k.busy !== true; cursorShape: Qt.PointingHandCursor; onClicked: kp.toggleMic() }
                    }
                    PadMark { visible: kp.padActive; ps: kp.padPs; button: "A"; size: 20 }
                }
                Lbl {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: kp.recording ? kp.tr("Listening… click to send") : (kp.k.listening === true ? kp.tr("Writing down what you said…") : (kp.k.busy === true ? kp.tr("The tutor is answering…") : kp.tr("Click and speak")))
                    dim: true; size: 13
                }
            }
            ScrollView {
                id: inputBox
                visible: !kp.callMode && !kp.liveSession
                anchors { left: micBtn.right; leftMargin: 6; right: sendBtn.left; rightMargin: 6; top: parent.top; topMargin: 8; bottom: parent.bottom; bottomMargin: 16 }
                TextArea {
                    id: input
                    wrapMode: TextEdit.Wrap
                    color: Theme.c.text
                    placeholderText: kp.tr("Write in %1… (Enter sends, Shift+Enter breaks the line)").replace("%1", kp.k.langName || "")
                    placeholderTextColor: Theme.c.textDim
                    font.family: Theme.type.family; font.pixelSize: 16
                    selectByMouse: true
                    background: Rectangle { radius: 18; color: Theme.c.fill3; border.width: input.activeFocus ? 2 : 0; border.color: Theme.c.accent }
                    leftPadding: 14; rightPadding: 14; topPadding: 9; bottomPadding: 9
                    Keys.onPressed: function(e) {
                        if ((e.key === Qt.Key_Return || e.key === Qt.Key_Enter) && !(e.modifiers & Qt.ShiftModifier)) { sendBtn.clicked(); e.accepted = true }
                    }
                }
            }
            FlatButton {
                id: sendBtn
                visible: !kp.callMode && !kp.liveSession
                anchors { right: parent.right; verticalCenter: inputBox.verticalCenter }
                kind: "suggested"; glyph: "send"; implicitWidth: 44; implicitHeight: 40
                enabled: input.text.trim() !== "" && kp.k.busy !== true
                ToolTip.visible: hovered; ToolTip.text: kp.tr("Send")
                onClicked: { if (!enabled) return; Core.cmd({cmd: "kaiwa_send", text: input.text}); input.text = "" }
            }
        }
    }

    // ================= a word, explained =================
    Popup {
        id: wordPop
        readonly property var w: kp.k.word || ({})
        anchors.centerIn: parent
        width: Math.min(420, kp.width - 32)
        padding: 18
        modal: false
        background: Rectangle { radius: Theme.shape.popover; color: Theme.c.popover; border.width: 1; border.color: Theme.c.border }
        contentItem: Column {
            spacing: 8
            Row {
                spacing: 10
                Lbl { text: wordPop.w.word || ""; bold: true; size: 24 }
                Lbl { visible: (wordPop.w.reading || "") !== ""; text: wordPop.w.reading || ""; dim: true; size: 15; anchors.verticalCenter: parent.verticalCenter }
                FlatButton { kind: "flat"; glyph: "volume"; implicitWidth: 32; implicitHeight: 30; onClicked: Core.cmd({cmd: "kaiwa_speak", text: wordPop.w.word || ""}) }
            }
            Lbl { visible: wordPop.w.loading === true; text: kp.tr("Looking it up…"); dim: true }
            Lbl { visible: !!wordPop.w.error; width: parent.width; wrapMode: Text.Wrap; text: kp.tr(wordPop.w.error || ""); color: Theme.c.errorText; size: 13 }
            Lbl { visible: text !== ""; width: parent.width; wrapMode: Text.Wrap; text: wordPop.w.meaning || ""; size: 15 }
            Lbl { visible: text !== ""; width: parent.width; wrapMode: Text.Wrap; text: wordPop.w.note || ""; dim: true; size: 13 }
            Lbl { visible: text !== ""; width: parent.width; wrapMode: Text.Wrap; text: wordPop.w.example || ""; size: 14; topPadding: 4 }
            Lbl { visible: text !== ""; width: parent.width; wrapMode: Text.Wrap; text: wordPop.w.example_tr || ""; color: Theme.c.accentText; size: 12 }
            Row {
                spacing: 8
                FlatButton {
                    kind: "suggested"; glyph: "plus"; text: kp.tr("Make a flashcard")
                    enabled: (wordPop.w.meaning || "") !== ""
                    onClicked: { Core.cmd({cmd: "kaiwa_card", word: wordPop.w.word || "", reading: wordPop.w.reading || "", meaning: wordPop.w.meaning || "", example: wordPop.w.example || "", example_tr: wordPop.w.example_tr || ""}); wordPop.close() }
                }
                FlatButton { kind: "flat"; text: kp.tr("Close"); onClicked: wordPop.close() }
            }
        }
    }
}
