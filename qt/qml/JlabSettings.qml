import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The settings of Jlab mode (core: jlab.rs): the deck on or off, reading help per kind of card, the kana and kanji
// trainers, the days before a card moves on. Opened from the menu of a deck and from the review of a Jlab card.
//
//     JlabSettings { id: jlabSet }   jlabSet.openFor(did, name)
Popup {
    id: js
    property real did: 0
    property string name: ""
    readonly property var st: Core.study || ({})
    readonly property var j: st.jlab || ({})
    readonly property bool deckOn: (j.decks || []).indexOf(did) >= 0
    readonly property var levels: [tr("Romaji"), tr("Kana"), tr("Kana + known kanji"), tr("Kanji + furigana")]
    readonly property var kanaKeys: ["none", "hiragana", "all", "learned"]
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function set(k, v) { Core.cmd({cmd: "jlab_set", key: k, value: v}) }
    function openFor(d, n) { did = d; name = n || ""; open() }
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: 20
    width: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 32)
    height: Math.min(body.implicitHeight + 40, (Overlay.overlay ? Overlay.overlay.height : 700) - 48)
    Overlay.modal: Rectangle { color: Theme.c.scrim }
    background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
    Flickable {
        anchors.fill: parent
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        Column {
            id: body
            width: parent.width
            spacing: 16
            Item {
                width: parent.width
                height: ttl.implicitHeight
                Lbl { id: ttl; width: parent.width - 44; wrapMode: Text.Wrap; bold: true; size: 20; text: js.tr("Jlab mode") + (js.name !== "" ? " · " + js.name : "") }
                FlatButton { anchors.right: parent.right; kind: "plain"; glyph: "close"; iconSize: 16; implicitHeight: 32; onClicked: js.close() }
            }
            Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; text: js.tr("Study Japanese sentences in two steps: first you listen, then you read. Tap a word to look it up; it becomes the gap of the reading card.") }
            PrefGroup {
                visible: js.did !== 0
                PrefSwitch {
                    first: true
                    title: js.tr("Use Jlab mode in this deck")
                    checked: js.deckOn
                    onToggled: function(v) { Core.cmd({cmd: "jlab_deck", id: js.did, on: v}) }
                }
            }
            PrefGroup {
                PrefCombo { first: true; title: js.tr("Reading help on listening cards"); options: js.levels; current: js.j.listen || 0; onPicked: function(i) { js.set("listen", i) } }
                PrefSwitch {
                    title: js.tr("Sentence on the front of listening cards")
                    subtitle: js.tr("Off: only the audio until you show the answer")
                    checked: js.j.listenText !== false
                    onToggled: function(v) { js.set("listen_text", v) }
                }
                PrefCombo { title: js.tr("Reading help on reading cards"); options: js.levels; current: js.j.read !== undefined ? js.j.read : 3; onPicked: function(i) { js.set("read", i) } }
                PrefCombo { title: js.tr("Reading help in the dictionary"); options: js.levels; current: js.j.dict !== undefined ? js.j.dict : 1; onPicked: function(i) { js.set("dict", i) } }
            }
            PrefGroup {
                PrefCombo {
                    first: true
                    title: js.tr("Kana trainer")
                    subtitle: js.tr("Kana that show as kana inside the romaji")
                    options: [js.tr("None"), js.tr("Hiragana"), js.tr("Hiragana and katakana"), js.tr("Those I learned in Learn")]
                    current: Math.max(0, js.kanaKeys.indexOf(js.j.kana || "none"))
                    onPicked: function(i) { js.set("kana", js.kanaKeys[i]) }
                }
                PrefSpin {
                    title: js.tr("Kanji I know")
                    subtitle: js.tr("Counted from JLPT N5 up to N1, fewer strokes first. Known kanji show without furigana.")
                    from: 0; to: js.j.kanjiMax || 3000; step: 10
                    value: js.j.kanji || 0
                    onMoved: function(v) { js.set("kanji", Math.round(v)) }
                }
                PrefSpin {
                    title: js.tr("Days to move on")
                    subtitle: js.tr("A listening card remembered for this many days becomes a reading card; a reading card remembered as long retires.")
                    from: 1; to: 365; step: 1
                    value: js.j.retireDays || 10
                    onMoved: function(v) { js.set("retire_days", Math.round(v)) }
                }
            }
        }
    }
}
