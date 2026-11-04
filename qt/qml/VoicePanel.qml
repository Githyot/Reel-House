import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The voice of Conversation (the core: voice.rs, controller_voice.rs, whisper_local.rs): who writes down what the
// learner says (a cloud key, or Whisper on this computer: private and offline after one download), the Whisper model
// and its download, who reads the tutor aloud (Google online, or a voice of this computer) and the pause that ends a
// turn in a voice call, and the microphone (which one, and a test with a level bar that writes down a few seconds).
// Self-contained like OllamaPanel: `VoicePanel { width: parent.width }`. Reads Core.state("voice"); calls voice_setup /
// voice_status / voice_stop / voice_test / voice_hear_test and set_setting general.voice_*.
Rectangle {
    id: vp
    property var v: ({})
    readonly property var w: vp.v.whisper || ({})
    readonly property string phase: vp.w.phase || "idle"
    readonly property bool busy: vp.w.busy === true
    readonly property bool isReady: vp.phase === "ready"
    readonly property bool failed: vp.phase === "error"
    readonly property real pct: Number(vp.w.percent || 0)
    readonly property bool hasBytes: Number(vp.w.total || 0) > 0
    readonly property var model: vp.v.model || ({})
    readonly property var local: vp.v.local || ({})
    // the voices of this computer for the language studied
    readonly property var myVoices: (vp.local.voices || []).filter(function(x) { return String(x.locale || "").split(/[_-]/)[0].toLowerCase() === (vp.v.lang || "") })
    readonly property bool showWhisper: vp.v.available === true && (vp.v.input === "local" || vp.w.inUse === true || vp.w.installed === true || vp.busy || vp.isReady)
    function tr(t) { return Core.tr(Core.uiLang, t) }
    function refresh() { vp.v = Core.state("voice") || ({}) }
    function set(key, val) { Core.cmd({cmd: "set_setting", path: "general." + key, value: val}); vp.refresh() }
    function mb(b) { return Math.round(Number(b || 0) / 1048576) + " MB" }
    // ---- the microphone: which one, and a test (a few seconds written down by the engines above) ----
    // MicRecorder.qml by URL: where Qt Multimedia is missing the panel still works, without this part
    Loader { id: testLoader; source: "MicRecorder.qml"; active: vp.visible }
    readonly property var testMic: testLoader.status === Loader.Ready ? testLoader.item : null
    property var mics: []
    property bool testing: false
    property real testPeak: 0
    property string testError: ""
    readonly property var test: vp.v.test || ({})
    readonly property string micName: {
        const id = vp.v.mic || ""
        if (id === "") return vp.tr("Automatic (the system's default)")
        const m = vp.mics.filter(function(x) { return x.id === id })
        return m.length ? m[0].name : vp.tr("Automatic (the system's default)")
    }
    function listMics() { vp.mics = vp.testMic ? vp.testMic.inputs() : [] }
    function startTest() {
        if (!vp.testMic) return
        vp.testError = ""
        vp.testPeak = 0
        vp.testing = true
        vp.testMic.start(vp.v.testFile || "", {auto: false, device: vp.v.mic || ""})
        testTimer.restart()
    }
    function stopTest() { testTimer.stop(); if (vp.testMic && vp.testMic.recording) vp.testMic.stop() }
    Timer { id: testTimer; interval: 4000; onTriggered: vp.stopTest() }
    Connections {
        target: vp.testMic
        ignoreUnknownSignals: true
        function onFinished(url) {
            vp.testing = false
            Core.cmd({cmd: "voice_hear_test", path: url})
            vp.refresh()
        }
        function onFailed(text) { vp.testing = false; testTimer.stop(); vp.testError = vp.tr(text) }
        function onLevelChanged() { if (vp.testing && vp.testMic.level > vp.testPeak) vp.testPeak = vp.testMic.level }
        function onRecordingChanged() { if (!vp.testMic.recording && vp.testing && !testTimer.running) vp.testing = false }
    }
    onTestMicChanged: vp.listMics()
    function phaseText() {
        if (vp.failed) return vp.w.message || vp.tr("Whisper could not be started.")
        if (vp.phase === "idle") {
            if (vp.w.installed === true) return vp.tr("Downloaded. It starts by itself when you speak.")
            return vp.tr("Writes down what you say on this computer: free, private and offline. The first time, Reelhouse downloads it (%1).").replace("%1", (vp.model.mib || 0) + " MB")
        }
        return vp.tr(vp.w.message || "")
    }

    implicitHeight: col.implicitHeight + 28
    radius: Theme.shape.card
    color: Theme.c.card
    border.color: Theme.c.border
    border.width: 1

    Connections { target: Core; function onStudyChanged() { if (vp.visible) vp.refresh() } }
    Connections { target: Core; ignoreUnknownSignals: true; function onPrefsChanged() { if (vp.visible) vp.refresh() } }
    Timer { interval: 1000; repeat: true; running: vp.visible && vp.busy; onTriggered: vp.refresh() }
    onVisibleChanged: if (visible) { Core.cmd({cmd: "voice_status"}); refresh() }
    Component.onCompleted: { if (visible) Core.cmd({cmd: "voice_status"}); refresh() }

    Column {
        id: col
        x: 14
        y: 14
        width: vp.width - 28
        spacing: 10

        Row {
            width: parent.width
            spacing: 10
            Rectangle {
                width: 10; height: 10; radius: 5
                anchors.verticalCenter: parent.verticalCenter
                color: vp.v.canHear === false ? Theme.c.warning : vp.failed ? Theme.c.error : vp.busy ? Theme.c.warning : Theme.c.success
            }
            Lbl { text: vp.tr("Voice"); bold: true; width: parent.width - 20; elide: Text.ElideRight }
        }

        // ---- hearing ----
        Lbl { text: vp.tr("What you say is written down by"); size: 13; dim: true }
        Segmented {
            fitWidth: col.width
            model: [{id: "auto", label: vp.tr("Automatic")}, {id: "gemini", label: "Gemini"}, {id: "openai", label: "OpenAI"}, {id: "local", label: vp.tr("This computer")}]
            current: vp.v.input || "auto"
            onPicked: function(id) { vp.set("voice_input", id) }
        }
        Lbl {
            width: parent.width
            wrapMode: Text.Wrap
            size: 13
            color: vp.v.canHear === false ? Theme.c.warningText : Theme.c.textDim
            text: vp.v.canHear === false ? vp.tr(vp.v.hearMissing || "") : (vp.v.plan || []).map(function(p) { return vp.tr(p) }).join("  →  ")
        }

        // ---- the microphone ----
        Column {
            width: parent.width
            spacing: 8
            visible: vp.testMic !== null
            Lbl { text: vp.tr("Microphone"); size: 13; dim: true }
            Flow {
                width: parent.width
                spacing: 8
                FlatButton {
                    id: micBtn
                    kind: "flat"; glyph: "mic"; implicitHeight: 30
                    text: vp.micName
                    enabled: !vp.testing
                    ToolTip.visible: hovered; ToolTip.text: vp.tr("Choose the microphone")
                    onClicked: { vp.listMics(); micMenu.openBelow(micBtn, false) }
                }
                FlatButton {
                    kind: vp.testing ? "suggested" : "flat"; implicitHeight: 30
                    text: vp.testing ? vp.tr("Stop") : vp.tr("Test the microphone")
                    onClicked: vp.testing ? vp.stopTest() : vp.startTest()
                }
            }
            // how loud the voice comes in while testing
            Rectangle {
                visible: vp.testing
                width: parent.width; height: 8; radius: 4
                color: Theme.c.fill3
                Rectangle {
                    width: parent.width * Math.max(0.02, Math.min(1, vp.testMic ? vp.testMic.level : 0)); height: parent.height; radius: parent.radius
                    color: vp.testMic && vp.testMic.level > 0.3 ? Theme.c.success : Theme.c.warning
                    Behavior on width { NumberAnimation { duration: 80 } }
                }
            }
            Lbl {
                width: parent.width
                wrapMode: Text.Wrap
                size: 13
                visible: text !== ""
                readonly property bool bad: !vp.testing && (vp.testError !== "" || (vp.test.error || "") !== "")
                color: bad ? Theme.c.warningText : Theme.c.textDim
                text: {
                    if (vp.testing) return vp.tr("Speak now: say a short sentence in the language you study.")
                    if (vp.testError !== "") return vp.testError
                    if (vp.test.busy === true) return vp.tr("Writing down what you said…")
                    if ((vp.test.error || "") !== "") return vp.tr(vp.test.error)
                    if ((vp.test.text || "") !== "") return vp.tr("Heard: %1").replace("%1", vp.test.text)
                    return ""
                }
            }
            Lbl {
                width: parent.width
                wrapMode: Text.Wrap
                size: 12
                color: Theme.c.warningText
                visible: !vp.testing && vp.testError === "" && vp.testPeak > 0 && vp.testPeak < 0.3
                text: vp.tr("The microphone is very quiet: raise its input volume (Windows: Settings > System > Sound > Input).")
            }
        }

        // ---- Whisper on this computer ----
        Column {
            width: parent.width
            spacing: 8
            visible: vp.showWhisper
            Row {
                spacing: 8
                Lbl { text: vp.tr("Whisper on this computer"); bold: true; size: 14; anchors.verticalCenter: parent.verticalCenter }
                FlatButton {
                    id: modelBtn
                    kind: "flat"
                    text: vp.tr(vp.model.label || "") + "  ·  " + (vp.model.mib || 0) + " MB"
                    implicitHeight: 30
                    enabled: !vp.busy
                    ToolTip.visible: hovered; ToolTip.text: vp.tr("Recognition model")
                    onClicked: modelMenu.openBelow(modelBtn, false)
                }
            }
            Lbl { width: parent.width; wrapMode: Text.Wrap; size: 12; dim: true; text: vp.tr(vp.model.note || "") }
            Lbl {
                width: parent.width
                wrapMode: Text.Wrap
                size: 13
                text: vp.phaseText()
                dim: !vp.failed
                color: vp.failed ? Theme.c.errorText : Theme.c.textDim
            }
            // the download (the program, then the model)
            Column {
                width: parent.width
                spacing: 4
                visible: vp.busy && (vp.phase === "downloading_runtime" || vp.phase === "downloading_model")
                Rectangle {
                    width: parent.width; height: 8; radius: 4
                    color: Theme.c.fill3
                    Rectangle {
                        visible: vp.hasBytes
                        width: parent.width * Math.max(0, Math.min(1, vp.pct / 100)); height: parent.height; radius: parent.radius
                        color: Theme.c.accent
                        Behavior on width { NumberAnimation { duration: 250 } }
                    }
                    Rectangle {
                        visible: !vp.hasBytes
                        width: parent.width * 0.25; height: parent.height; radius: parent.radius
                        color: Theme.c.accent
                        SequentialAnimation on x {
                            running: vp.visible && !vp.hasBytes && vp.busy
                            loops: Animation.Infinite
                            NumberAnimation { from: 0; to: col.width * 0.75; duration: 900 }
                            NumberAnimation { from: col.width * 0.75; to: 0; duration: 900 }
                        }
                    }
                }
                Lbl { visible: vp.hasBytes; text: vp.pct.toFixed(1) + "%  ·  " + vp.mb(vp.w.done) + " / " + vp.mb(vp.w.total); dim: true; size: 12 }
            }
            Row {
                spacing: 8
                visible: !vp.busy
                FlatButton {
                    visible: !vp.isReady
                    kind: "suggested"
                    text: vp.failed ? vp.tr("Try again") : (vp.w.installed === true ? vp.tr("Start Whisper") : vp.tr("Download and turn on Whisper"))
                    onClicked: { Core.cmd({cmd: "voice_setup"}); vp.refresh() }
                }
                FlatButton {
                    visible: vp.isReady && vp.w.ours === true
                    text: vp.tr("Stop Whisper")
                    onClicked: { Core.cmd({cmd: "voice_stop"}); vp.refresh() }
                }
            }
            Lbl {
                visible: !!vp.w.dir
                width: parent.width
                text: vp.tr("Files in %1").replace("%1", vp.w.dir || "")
                dim: true
                size: 12
                wrapMode: Text.WrapAnywhere
            }
        }

        // ---- the pause that ends a turn (every turn of Conversation; finer steps in Settings > Study > Microphone) ----
        Lbl { text: vp.tr("Your turn ends after a pause of"); size: 13; dim: true }
        Segmented {
            fitWidth: col.width
            model: [{id: "0", label: vp.tr("Automatic")}, {id: "1000", label: "1 s"}, {id: "2000", label: "2 s"}, {id: "3000", label: "3 s"}]
            current: String(vp.v.pause || 0)
            onPicked: function(id) { vp.set("voice_pause", Number(id)) }
        }

        // ---- speaking ----
        Lbl { text: vp.tr("The tutor's voice"); size: 13; dim: true }
        Segmented {
            fitWidth: col.width
            model: [{id: "auto", label: vp.tr("Automatic")}, {id: "google", label: vp.tr("Google (online)")}, {id: "local", label: vp.tr("This computer")}]
            current: vp.v.output || "auto"
            onPicked: function(id) { vp.set("voice_output", id) }
        }
        Row {
            spacing: 8
            width: parent.width
            FlatButton {
                id: sysVoiceBtn
                visible: vp.local.available === true && vp.myVoices.length > 0
                kind: "flat"
                implicitHeight: 30
                text: vp.tr("Voice of this computer") + ": " + ((vp.v.localVoice || "") !== "" ? vp.v.localVoice : vp.tr("Automatic"))
                onClicked: voiceMenu.openBelow(sysVoiceBtn, false)
            }
            FlatButton {
                kind: "flat"; glyph: "volume"; implicitHeight: 30
                text: vp.tr("Test")
                onClicked: Core.cmd({cmd: "voice_test", engine: ""})
            }
            FlatButton {
                visible: vp.local.available === true && vp.myVoices.length > 0 && (vp.v.output || "auto") !== "local"
                kind: "flat"; implicitHeight: 30
                text: vp.tr("Test the voice of this computer")
                onClicked: Core.cmd({cmd: "voice_test", engine: "local"})
            }
        }
        Lbl {
            visible: vp.local.available !== undefined && (vp.local.available !== true || vp.myVoices.length === 0)
            width: parent.width
            wrapMode: Text.Wrap
            size: 12
            color: Theme.c.warningText
            text: vp.local.available !== true
                ? vp.tr("This version has no voices of the computer (Qt TextToSpeech is missing): only Google's online voice can read.")
                : vp.tr("No voice of this computer speaks the language studied. On Windows: Settings > Time & language > Speech > Add voices.")
        }
    }

    PopMenu {
        id: modelMenu
        menuWidth: 330
        readonly property var ids: ["auto"].concat((vp.v.models || []).map(function(m) { return m.id }))
        items: [{label: vp.tr("Automatic (the best for this language)"), checked: (vp.v.modelChoice || "auto") === "auto"}].concat((vp.v.models || []).map(function(m) {
            return {label: vp.tr(m.label) + "  ·  " + m.mib + " MB", checked: vp.v.modelChoice === m.id}
        }))
        onPicked: function(i) { vp.set("voice_model", modelMenu.ids[i]) }
    }
    PopMenu {
        id: micMenu
        menuWidth: 330
        items: [{label: vp.tr("Automatic (the system's default)"), checked: (vp.v.mic || "") === ""}].concat(vp.mics.map(function(m) {
            return {label: m.name + (m.isDefault ? "  ·  " + vp.tr("default") : ""), checked: vp.v.mic === m.id}
        }))
        onPicked: function(i) { vp.set("voice_mic", i === 0 ? "" : vp.mics[i - 1].id) }
    }
    PopMenu {
        id: voiceMenu
        menuWidth: 330
        items: [{label: vp.tr("Automatic"), checked: (vp.v.localVoice || "") === ""}].concat(vp.myVoices.map(function(x) { return {label: x.name, checked: vp.v.localVoice === x.name} }))
        onPicked: function(i) { vp.set("voice_local", i === 0 ? "" : vp.myVoices[i - 1].name) }
    }
}
