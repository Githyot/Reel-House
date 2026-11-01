import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "KanaHover.js" as Kana

// What the microphone heard last in this session (Core.state("voice").last), under the conversation, so the learner
// can check what was understood: who wrote it down and how long it took, then the text with its Japanese words
// hoverable like the study cards (KanaHover.js: hiragana, katakana and romaji of kana words; the kanji of a word
// with their meaning and on / kun readings from the app's kanji table, sent by the core), and the romaji line when
// the page's "Romaji" option is on. `sid` is the open session: a text heard in another session is not shown.
Column {
    id: hs
    property real sid: -1
    property bool ja: false
    property bool showRomaji: false
    property var last: ({})
    // the newest text heard and the session it was heard in
    property real knownSeq: -1
    property real ownerSid: -2
    readonly property string heard: String(hs.last.text || "")
    readonly property var dict: hs.last.kanji && hs.last.kanji.kanji ? hs.last.kanji : ({reading: {}, meaning: {}, kanji: {}})
    visible: hs.heard !== "" && hs.ownerSid === hs.sid
    spacing: 3
    function tr(t) { return Core.tr(Core.uiLang, t) }
    function esc(t) { return String(t || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }
    function refresh(first) {
        const l = (Core.state("voice") || ({})).last || ({})
        const seq = Number(l.seq || 0)
        if (seq === hs.knownSeq) return
        hs.knownSeq = seq
        hs.last = l
        // a text from before this page opened belongs to no session shown here
        hs.ownerSid = first ? -2 : hs.sid
    }
    Connections { target: Core; function onStudyChanged() { hs.refresh(false) } }
    Component.onCompleted: hs.refresh(true)

    Lbl {
        width: parent.width
        elide: Text.ElideRight
        size: 12
        dim: true
        text: hs.tr("Heard by %1 in %2 s").replace("%1", hs.tr(hs.last.engine || "")).replace("%2", (Number(hs.last.ms || 0) / 1000).toFixed(1))
              + ((hs.last.note || "") !== "" ? "  ·  " + hs.tr("another engine failed first") : "")
    }
    Text {
        id: heardText
        width: Math.min(parent.width, implicitWidth)
        wrapMode: Text.Wrap
        textFormat: Text.RichText
        text: hs.ja ? Kana.linkify(hs.esc(hs.heard), "" + Theme.c.text, hs.dict) : hs.esc(hs.heard)
        color: Theme.c.text
        font.pixelSize: 15
        font.family: Theme.type.family
        HoverHandler { cursorShape: heardText.hoveredLink.indexOf("jp:") === 0 ? Qt.WhatsThisCursor : Qt.ArrowCursor }
        ToolTip.visible: heardText.hoveredLink.indexOf("jp:") === 0
        ToolTip.text: heardText.hoveredLink.indexOf("jp:") === 0 ? Kana.tip(heardText.hoveredLink, hs.dict) : ""
    }
    Lbl {
        visible: hs.ja && hs.showRomaji
        width: parent.width
        wrapMode: Text.Wrap
        size: 13
        dim: true
        font.italic: true
        text: Kana.romaji(hs.heard)
    }
}
