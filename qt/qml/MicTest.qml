import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Settings > Study > Microphone: a test of the microphone with fallback. "Test the microphone" records a few
// seconds (VoiceInput moves by itself to another format or another microphone when the chosen one sends only
// silence, and Qt's own recorder is the last way) and writes down what was heard; "Find a working microphone"
// listens to each input for a moment and keeps the first one that gives sound (general.voice_mic).
// MicRecorder.qml by URL: where Qt Multimedia is missing this row says so.
Column {
    id: mt
    width: parent ? parent.width : 600
    spacing: 8
    property var v: ({})
    function refresh() { mt.v = Core.state("voice") || ({}) }
    function tr(s) { return Core.tr(Core.uiLang, s) }
    Loader { id: ld; source: "MicRecorder.qml"; active: mt.visible }
    readonly property var mic: ld.status === Loader.Ready ? ld.item : null
    readonly property var test: mt.v.test || ({})
    property bool testing: false
    property real peak: 0
    // the microphone the last test recorded from
    property string lastDevice: ""
    property string error: ""
    property string note: ""
    property var found: []

    function start() {
        if (!mt.mic) return
        mt.refresh()
        mt.error = ""; mt.note = ""; mt.peak = 0; mt.lastDevice = ""
        mt.testing = true
        mt.mic.start(mt.v.testFile || "", {auto: false, device: mt.v.mic || ""})
        tm.restart()
    }
    function stop() { tm.stop(); if (mt.mic && mt.mic.recording) mt.mic.stop() }
    function scan() {
        if (!mt.mic) return
        mt.error = ""; mt.note = ""; mt.found = []
        mt.mic.retryDevice()
        mt.mic.scan()
    }
    Timer { id: tm; interval: 4000; onTriggered: mt.stop() }
    // while the words are being written down
    Timer { interval: 700; repeat: true; running: mt.visible && mt.test.busy === true; onTriggered: mt.refresh() }
    Connections {
        target: mt.mic
        ignoreUnknownSignals: true
        function onFinished(url) {
            mt.testing = false
            Core.cmd({cmd: "voice_hear_test", path: url})
            mt.refresh()
        }
        function onFailed(text) { mt.testing = false; tm.stop(); mt.error = mt.tr(text) }
        function onSwitched(name) { mt.note = mt.tr("The chosen microphone sent only silence; now trying: %1").replace("%1", name) }
        function onLevelChanged() {
            if (mt.testing && mt.mic.level > mt.peak) mt.peak = mt.mic.level
            if (mt.testing && mt.mic.device !== "") mt.lastDevice = mt.mic.device
        }
        function onScanned(results) {
            mt.found = results
            const live = results.filter(function(r) { return r.live })
            if (live.length === 0) {
                mt.error = mt.tr("No microphone sent sound. On Windows: Settings > Privacy & security > Microphone, let desktop apps use it; check that the microphone is not muted (Settings > System > Sound > Input).")
                return
            }
            // the chosen one if it works, else the default if it works, else the first that works
            const cur = mt.v.mic || ""
            let pick = live.filter(function(r) { return r.id === cur })[0]
                    || live.filter(function(r) { return r.isDefault })[0] || live[0]
            if (cur === "" && pick.isDefault) pick = null
            if (pick) Core.cmd({cmd: "set_setting", path: "general.voice_mic", value: pick.id})
            mt.refresh()
            mt.note = mt.tr("Working microphone: %1").replace("%1", pick ? pick.name : live[0].name)
        }
    }
    Component.onCompleted: mt.refresh()

    Lbl {
        width: parent.width
        visible: !mt.mic
        wrapMode: Text.Wrap; size: 13; dim: true
        text: mt.tr("The microphone needs Qt Multimedia, which this build does not have.")
    }
    Flow {
        width: parent.width
        spacing: 8
        visible: mt.mic !== null
        FlatButton {
            kind: mt.testing ? "suggested" : "flat"; glyph: "mic"; implicitHeight: 30
            enabled: !(mt.mic && mt.mic.scanning)
            text: mt.testing ? mt.tr("Stop") : mt.tr("Test the microphone")
            onClicked: mt.testing ? mt.stop() : mt.start()
        }
        FlatButton {
            kind: "flat"; implicitHeight: 30
            enabled: !mt.testing && !(mt.mic && mt.mic.scanning)
            text: mt.mic && mt.mic.scanning ? mt.tr("Listening to each microphone…") : mt.tr("Find a working microphone")
            onClicked: mt.scan()
        }
    }
    // how loud the voice comes in, and from which microphone
    Rectangle {
        visible: mt.testing
        width: parent.width; height: 8; radius: 4
        color: Theme.c.fill3
        Rectangle {
            width: parent.width * Math.max(0.02, Math.min(1, mt.mic ? mt.mic.level : 0)); height: parent.height; radius: parent.radius
            color: mt.mic && mt.mic.level > 0.3 ? Theme.c.success : Theme.c.warning
            Behavior on width { NumberAnimation { duration: 80 } }
        }
    }
    Lbl {
        width: parent.width; wrapMode: Text.Wrap; size: 12; dim: true
        visible: mt.testing && mt.mic && mt.mic.device !== ""
        text: mt.tr("Recording from: %1").replace("%1", mt.mic ? mt.mic.device : "")
    }
    Lbl {
        width: parent.width; wrapMode: Text.Wrap; size: 13
        visible: text !== ""
        readonly property bool bad: !mt.testing && (mt.error !== "" || (mt.test.error || "") !== "")
        color: bad ? Theme.c.warningText : Theme.c.textDim
        text: {
            if (mt.testing) return mt.tr("Speak now: say a short sentence in the language you study.")
            if (mt.error !== "") return mt.error
            if (mt.test.busy === true) return mt.tr("Writing down what you said…")
            if ((mt.test.error || "") !== "") return mt.tr(mt.test.error)
            if ((mt.test.text || "") !== "") return mt.tr("Heard: %1").replace("%1", mt.test.text)
            return ""
        }
    }
    // what was measured and what the transcriber gave back (also in the log as "[voice] recording: …")
    Lbl {
        width: parent.width; wrapMode: Text.Wrap; size: 12; dim: true
        visible: !mt.testing && mt.test.busy !== true && (mt.test.note || "") !== ""
        text: mt.tr("Measured: %1").replace("%1", (mt.lastDevice !== "" ? mt.lastDevice + " · " : "") + (mt.test.note || ""))
    }
    Lbl {
        width: parent.width; wrapMode: Text.Wrap; size: 12; color: Theme.c.accent
        visible: mt.note !== ""
        text: mt.note
    }
    Lbl {
        width: parent.width; wrapMode: Text.Wrap; size: 12; color: Theme.c.warningText
        visible: !mt.testing && mt.error === "" && mt.peak > 0 && mt.peak < 0.3
        text: mt.tr("The microphone is very quiet: raise its input volume (Windows: Settings > System > Sound > Input).")
    }
    // the scan's answer, one line per microphone
    Repeater {
        model: mt.found
        Lbl {
            width: mt.width; size: 12
            color: modelData.live ? Theme.c.successText : Theme.c.textDim
            text: (modelData.live ? "✓ " : "✗ ") + modelData.name + (modelData.isDefault ? "  (" + mt.tr("default") + ")" : "")
                  + "  ·  " + (modelData.live ? mt.tr("sends sound") : mt.tr("only silence"))
        }
    }
}
