import QtQuick
import QtTextToSpeech

// The voices of this computer (Qt TextToSpeech: SAPI on Windows, where adding a language under Settings > Time &
// language > Speech brings its voice, e.g. "Microsoft Haruka Desktop" for Japanese; speech-dispatcher on Linux;
// the system voices on macOS). Offline and free. Loaded by VoiceHost.qml through a Loader, so a build without
// Qt Speech still works (the core is then told there is no system voice).
// voices(lang) lists [{name, locale}] ("" = all), say(text, lang, name) speaks with the voice named (or the first
// of the language) and answers false when there is none, stop() stops.
Item {
    id: lt
    readonly property string engineName: tts.engine
    readonly property bool talking: tts.state === TextToSpeech.Speaking
    signal problem(string text)

    TextToSpeech {
        id: tts
        onErrorOccurred: function(reason, text) { lt.problem(text || "the system voice failed") }
    }

    function base(localeName) { return String(localeName || "").split(/[_-]/)[0].toLowerCase() }
    function voices(lang) {
        const out = []
        const keep = tts.locale
        const locs = tts.availableLocales()
        for (let i = 0; i < locs.length; ++i) {
            if (lang && lt.base(locs[i].name) !== lang) continue
            tts.locale = locs[i]
            const vs = tts.availableVoices()
            for (let j = 0; j < vs.length; ++j) out.push({name: String(vs[j].name), locale: String(locs[i].name)})
        }
        tts.locale = keep
        return out
    }
    function say(text, lang, name) {
        const locs = tts.availableLocales()
        let picked = null, pickedLoc = null
        for (let i = 0; i < locs.length && !(picked && picked.name === name); ++i) {
            if (lt.base(locs[i].name) !== lang) continue
            tts.locale = locs[i]
            const vs = tts.availableVoices()
            for (let j = 0; j < vs.length; ++j) {
                if (!picked || vs[j].name === name) { picked = vs[j]; pickedLoc = locs[i] }
                if (vs[j].name === name) break
            }
        }
        if (!picked) return false
        tts.locale = pickedLoc
        tts.voice = picked
        tts.say(text)
        console.log("[voice] system voice: " + picked.name + " (" + pickedLoc.name + ", engine " + tts.engine + ")")
        return true
    }
    function stop() { tts.stop() }
}
