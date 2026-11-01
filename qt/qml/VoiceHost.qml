import QtQuick
import Reelhouse 1.0

// Made with Conversation and kept with it (KaiwaPage loads it once): the page's side of the voice
// (controller_voice.rs).
// - Speaks the texts the core sends to the system voice (`Core.state("voice").say`, a new `seq` each time; an
//   empty text stops), through LocalTts.qml (Qt TextToSpeech, loaded by URL: missing Qt Speech breaks nothing).
// - Tells the core which voices this computer has (`voice_local_info`), so it can warn when the language has none.
// - Writes the core's new `[voice]` lines to the log (reelhouse-qt.log).
Item {
    id: vh
    property real saySeen: -1
    property real logSeen: -1
    Loader {
        id: ttsLoader
        source: "LocalTts.qml"
        onStatusChanged: {
            // the reason in the log (a build without Qt Speech, or its plugin missing next to the program)
            if (status === Loader.Error) console.log("[voice] system voices unavailable: LocalTts.qml could not load (QtTextToSpeech QML module or its plugin missing)")
            vh.report()
        }
    }
    readonly property var tts: ttsLoader.status === Loader.Ready ? ttsLoader.item : null
    // a system voice is reading now (Conversation's "Speaking…")
    readonly property bool talking: vh.tts !== null && vh.tts.talking === true
    Connections {
        target: vh.tts
        ignoreUnknownSignals: true
        function onProblem(text) { Core.cmd({cmd: "voice_local_failed", why: String(text)}) }
    }

    function report() {
        if (ttsLoader.status === Loader.Loading || ttsLoader.status === Loader.Null) return
        if (vh.tts) Core.cmd({cmd: "voice_local_info", available: true, engine: vh.tts.engineName, voices: vh.tts.voices("")})
        else Core.cmd({cmd: "voice_local_info", available: false, engine: "", voices: []})
    }
    function check() {
        const v = Core.state("voice") || ({})
        const lines = v.log || []
        for (let i = 0; i < lines.length; ++i) {
            if (lines[i].seq > vh.logSeen) { console.log(lines[i].line); vh.logSeen = lines[i].seq }
        }
        const s = v.say || ({})
        if (s.seq === undefined || s.seq === vh.saySeen) return
        const first = vh.saySeen < 0
        vh.saySeen = s.seq
        // a request older than this page is not spoken again
        if (first && s.text) return
        if (!s.text) { if (vh.tts) vh.tts.stop(); return }
        if (!vh.tts) { Core.cmd({cmd: "voice_local_failed", why: "Qt TextToSpeech is not installed"}); return }
        if (!vh.tts.say(String(s.text), String(s.lang || ""), String(s.voice || "")))
            Core.cmd({cmd: "voice_local_failed", why: "no system voice for " + (s.lang || "?")})
    }
    Connections { target: Core; function onStudyChanged() { vh.check() } }
    Component.onCompleted: vh.check()
}
