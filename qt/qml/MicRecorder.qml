import QtQuick
import QtMultimedia
import Reelhouse 1.0

// The microphone of Conversation: records the learner's voice to a WAV file (mono, 16 kHz) for the engine that writes
// it down (controller_voice.rs). Loaded by a Loader, so the page still works where Qt Multimedia is missing.
// VoiceInput (qt/src/voiceinput.cpp) reads the device and the core watches the learner's turn (reelhouse::vad):
// it gives the real loudness for the ring and, with `auto` (every turn of Conversation), ends the recording by itself
// after the learner's pause and keeps only the speech. Where it cannot open the device, Qt's own MediaRecorder
// records instead (no detector: a pulse for the ring, and it stops at the longest turn, 30 s).
// start(path, vad) begins (`vad`: Core.state("voice").vad with `auto` and `device`, the microphone chosen in
// Conversation > Voice), stop() ends and sends, cancel() drops it;
// `finished(url)` gives the file, `failed(text)` an error (no microphone, denied, nothing heard…); `silent()` comes
// just before the failure when nobody spoke (Conversation's hands-free counts those turns).
Item {
    id: mic
    signal finished(string url)
    signal failed(string text)
    signal silent()
    // the chosen microphone gave only silence and the recording moved to another one (or another format)
    signal switched(string name)
    // scan(): [{id, name, isDefault, live, peakDb}]
    signal scanned(var results)
    readonly property bool scanning: vin.scanning
    // the microphone recording now
    readonly property string device: vin.recording ? vin.device : (recorder.recorderState === MediaRecorder.RecordingState ? input.device.description : "")
    readonly property bool recording: vin.recording || recorder.recorderState === MediaRecorder.RecordingState
    // how loud the voice is now (0-1), for the ring around the button
    readonly property real level: vin.recording ? vin.level : mic.pulse
    // the learner is talking now (VoiceInput only)
    readonly property bool speaking: vin.speaking
    readonly property bool hasInput: devices.audioInputs.length > 0
    property bool wanted: false
    property real pulse: 0
    // VoiceInput could not open the device once: Qt's recorder from then on
    property bool useRecorder: false

    VoiceInput {
        id: vin
        onFinished: function(url) { mic.finished(url) }
        onFailed: function(text) {
            // every input gave zeros through VoiceInput: Qt's own recorder (another Windows path) next time
            if (text.indexOf("only silence") >= 0) {
                mic.useRecorder = true
                console.log("[voice] microphone: only silence everywhere, Qt's recorder next time")
            }
            mic.failed(text)
        }
        onSwitched: function(name) { mic.switched(name) }
        onScanned: function(results) { mic.scanned(results) }
        // the call waited and no voice came, or (push to talk) almost nothing was recorded; the level is in the log
        onNothing: { mic.silent(); mic.failed("No voice was found in the recording. Speak a little louder or closer to the microphone; the level measured is in the log.") }
    }

    MediaDevices { id: devices }
    CaptureSession {
        id: session
        audioInput: AudioInput { id: input }
        recorder: MediaRecorder {
            id: recorder
            mediaFormat {
                fileFormat: MediaFormat.Wave
                audioCodec: MediaFormat.AudioCodec.Wave
            }
            audioSampleRate: 16000
            audioChannelCount: 1
            quality: MediaRecorder.NormalQuality
            onRecorderStateChanged: {
                if (recorderState === MediaRecorder.StoppedState && mic.wanted) {
                    mic.wanted = false
                    mic.finished(String(recorder.actualLocation))
                }
            }
            onErrorOccurred: function(err, text) { mic.wanted = false; mic.failed(text || "The microphone could not record.") }
        }
    }
    // Qt's recorder has no detector: it stops (and sends) at the longest turn
    Timer {
        interval: 30000
        running: recorder.recorderState === MediaRecorder.RecordingState
        onTriggered: { console.log("[voice] microphone: Qt's recorder reached the longest turn (30 s), sent"); recorder.stop() }
    }
    // a gentle pulse while Qt's recorder records (it gives no input level to QML)
    SequentialAnimation on pulse {
        running: recorder.recorderState === MediaRecorder.RecordingState
        loops: Animation.Infinite
        NumberAnimation { from: 0.2; to: 1; duration: 600; easing.type: Easing.InOutSine }
        NumberAnimation { from: 1; to: 0.2; duration: 600; easing.type: Easing.InOutSine }
    }

    function fileUrl(path) {
        return path.indexOf("file:") === 0 ? path : "file:///" + String(path).replace(/\\/g, "/").replace(/^\/+/, "")
    }
    // the microphones of this computer for a picker: [{id, name, isDefault}] (`device` in start's options)
    function inputs() { return vin.inputs() }
    // listens to each microphone for a moment and tells which give sound (scanned)
    function scan() { if (!mic.recording) vin.scan() }
    // back to VoiceInput (after Qt's recorder was used as the fallback)
    function retryDevice() { mic.useRecorder = false }
    function start(path, vad) {
        if (!mic.hasInput) { mic.failed("No microphone was found."); return }
        if (!mic.useRecorder) {
            if (vin.start(String(path), JSON.stringify(vad || ({auto: false})))) return
            mic.useRecorder = true
            console.log("[voice] microphone: VoiceInput could not open the device, Qt's recorder instead (no automatic end of turn)")
        }
        input.device = devices.defaultAudioInput
        var want = (vad && vad.device) ? String(vad.device) : ""
        for (var i = 0; want !== "" && i < devices.audioInputs.length; ++i)
            if (String(devices.audioInputs[i].id) === want) input.device = devices.audioInputs[i]
        recorder.outputLocation = mic.fileUrl(path)
        mic.wanted = true
        recorder.record()
    }
    function stop() {
        if (vin.recording) vin.stop()
        else if (recorder.recorderState === MediaRecorder.RecordingState) recorder.stop()
    }
    function cancel() {
        mic.wanted = false
        if (vin.recording) vin.cancel()
        if (recorder.recorderState === MediaRecorder.RecordingState) recorder.stop()
    }
}
