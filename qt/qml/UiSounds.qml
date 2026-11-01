import QtQuick
import QtMultimedia
import Reelhouse 1.0

// The interface sounds: a blip when the focus moves, another on select, and background music in a loop. The app ships
// no recordings: each slot plays the file the person picked in Preferences > Controller > Sounds and music (copied to
// <data>/sounds, see src/uisounds.rs: nav.*, select.*, music.* there are used by themselves), in any format Qt Multimedia reads (mp3, ogg, wav, flac, m4a…).
// A picked file plays through a MediaPlayer, because SoundEffect only reads plain PCM WAV (an mp3 there is silent);
// with no file the blips are the small ones made by the core (reelhouse::orbs) and there is no music. Loaded by a
// Loader in Main.qml, so the app still works where Qt Multimedia is missing.
Item {
    id: us
    property var gen: ({})
    property string dataDir: ""
    // Main.qml: the music may play now (window in front, not in the player, Study or Preferences)
    property bool musicWanted: false
    readonly property real vol: (gen.sound_volume !== undefined ? gen.sound_volume : 60) / 100

    function fileUrl(rel) {
        if (!rel || !dataDir) return ""
        const path = (dataDir + "/" + rel).replace(/\\/g, "/")
        // Windows: file:///C:/…; elsewhere the path already starts with /
        return encodeURI(path.charAt(0) === "/" ? "file://" + path : "file:///" + path)
    }
    function nav() { navSound.play() }
    function select() { selectSound.play() }

    // one interface sound: the picked file through a MediaPlayer, else the core's blip through a SoundEffect
    // (an inline component sees no ids of this file, so it gets everything through its properties)
    component Blip: Item {
        id: blip
        property url picked: ""           // the picked file, "" = built-in
        property url builtIn: ""
        property real volume: 0.6
        function play() {
            if (picked.toString() !== "") {
                if (player.playbackState === MediaPlayer.PlayingState) player.position = 0
                else player.play()
            } else fx.play()
        }
        SoundEffect { id: fx; source: blip.picked.toString() === "" ? blip.builtIn : ""; volume: blip.volume }
        MediaPlayer {
            id: player
            source: blip.picked
            audioOutput: AudioOutput { volume: blip.volume }
            onErrorOccurred: function(error, text) { console.warn("[sound] cannot play " + blip.picked + ": " + (text || player.errorString) + " (error " + error + ")") }
        }
    }
    Blip { id: navSound; picked: us.fileUrl(us.gen.sound_nav || ""); builtIn: Core.navBlipUrl(0); volume: us.vol }
    Blip { id: selectSound; picked: us.fileUrl(us.gen.sound_select || ""); builtIn: Core.navBlipUrl(1); volume: us.vol }

    // ---- background music: fades in and out over 300 ms ----
    readonly property url musicUrl: fileUrl(gen.sound_music || "")
    readonly property bool musicOn: musicWanted && gen.pad_music !== false && musicUrl.toString() !== ""
    property real fade: 0
    Behavior on fade { NumberAnimation { duration: 300 } }
    onMusicOnChanged: { console.log("[sound] music " + (musicOn ? "on" : "off") + " (wanted " + musicWanted + ", switch " + (gen.pad_music !== false) + ", file " + (musicUrl.toString() || "none") + ")"); sync() }
    Component.onCompleted: sync()
    onDataDirChanged: console.log("[sound] files: nav " + (navSound.picked.toString() || "built-in") + ", select " + (selectSound.picked.toString() || "built-in") + ", music " + (musicUrl.toString() || "none") + ", volume " + Math.round(vol * 100) + ", music switch " + (gen.pad_music !== false))
    function sync() {
        if (musicOn) { if (music.playbackState !== MediaPlayer.PlayingState) music.play(); fade = 1 }
        else fade = 0
    }
    onFadeChanged: if (fade === 0 && !musicOn) music.pause()
    MediaPlayer {
        id: music
        source: us.musicUrl
        loops: MediaPlayer.Infinite
        audioOutput: AudioOutput { volume: us.vol * us.fade }
        onSourceChanged: { console.log("[sound] music file " + (source.toString() || "none")); Qt.callLater(us.sync) }
        onErrorOccurred: function(error, text) { console.warn("[sound] cannot play the music " + us.musicUrl + ": " + (text || music.errorString) + " (error " + error + ")") }
        onPlaybackStateChanged: console.log("[sound] music " + ["stopped", "playing", "paused"][playbackState] + " (" + us.musicUrl + ")")
        onMediaStatusChanged: {
            const names = ["no media", "loading", "loaded", "stalled", "buffering", "buffered", "end of media", "invalid media"]
            console.log("[sound] music status: " + (names[mediaStatus] || mediaStatus) + (mediaStatus === MediaPlayer.InvalidMedia ? " " + music.errorString : ""))
            // a loop that ended by itself (a backend that ignores `loops`) starts again
            if (mediaStatus === MediaPlayer.EndOfMedia && us.musicOn) music.play()
        }
    }
}
