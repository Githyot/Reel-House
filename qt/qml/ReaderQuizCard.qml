import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A small "do you understand this?" card of the readers (docs/study-in-readers.md): a sentence of the study
// language whose words are all known (a paragraph of a novel, a speech bubble of a manga). The meaning is shown on
// request (the bubble's translation, else asked of the translator), and the answer goes to the knowledge file.
Rectangle {
    id: qc
    property string text: ""            // the sentence, in the study language
    property string lang: ""            // its 3-letter code; "" = the study language
    property string meaning: ""         // a translation already known (a translated bubble); else asked for
    property color ink: Theme.c.text
    property color paper: Theme.c.card
    property bool answered: false
    property bool revealed: false
    property int trId: 0
    // the card is on screen (set by the reader): the macaw comes to think with the reader once
    property bool shown: true
    property bool cued: false
    signal done(bool correct)
    signal closed()

    readonly property var g: Core.prefs.general || ({})
    readonly property bool cuesOn: g.study_in_player === true && g.study_mascot_cues !== false
    readonly property var pt: Core.manga.pagetools || ({})
    readonly property var trRes: trId > 0 && pt.translations ? pt.translations[String(trId)] : null
    readonly property string shownMeaning: meaning !== "" ? meaning : (trRes && trRes.texts ? (trRes.texts[0] || "") : "")
    readonly property string trError: trRes && trRes.error ? trRes.error : ""
    function tr(s) { return Core.tr(Core.uiLang, s) }
    // one of the macaw's funny lines (src/mascot_lines.rs), random and never the last one
    function quip(m) { const r = Core.state("mascot:" + Core.uiLang + ":" + m); return r && r.text ? r.text : "" }
    // the meaning is written in the first language the user knows, else in the language of the app
    function target() { const k = g.known_langs || []; return k.length > 0 ? k[0] : Core.uiLang }
    function cue(c, text) {
        if (!cuesOn || typeof Core.mascotCue !== "function") return
        if (text) Core.mascotCue({cue: c, corner: "bottom-right", ms: 4500, text: text})
        else Core.mascotCue({cue: c, corner: "bottom-right", ms: 2200})
    }
    function reveal() {
        revealed = true
        if (meaning === "" && trId === 0) {
            // numbers far from the ones the readers use for their pages and chapters
            trId = 700000 + Math.floor(Math.random() * 200000)
            Core.cmd({cmd: "translate_texts", kind: "novel", id: trId, source: "auto", target: target(), texts: [text]})
        }
    }
    function answer(ok) {
        if (answered) return
        answered = true
        revealed = true
        Core.cmd({cmd: "reader_study_answer", text: text, lang: lang, correct: ok})
        // with a funny line now and then (shown a little longer, time to read it)
        cue(ok ? "happy" : "sad", Math.random() < 0.5 ? quip(ok ? "quiz_right" : "quiz_wrong") : "")
        done(ok)
    }
    function cueOnce() { if (shown && visible && !cued) { cued = true; cue("think") } }
    onShownChanged: cueOnce()
    onVisibleChanged: cueOnce()
    Component.onCompleted: cueOnce()

    implicitWidth: 360
    implicitHeight: col.implicitHeight + 28
    radius: Theme.shape.card || 12
    color: paper
    border.width: 1
    border.color: Qt.rgba(ink.r, ink.g, ink.b, 0.18)

    Column {
        id: col
        x: 14; y: 14
        width: parent.width - 28
        spacing: 10
        Row {
            width: parent.width
            spacing: 10
            Mascot { id: bird; mood: qc.answered ? "happy" : "think"; size: 44; animate: qc.visible }
            Column {
                width: parent.width - bird.width - closeBtn.width - 20
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Lbl { text: qc.tr("Quiz"); bold: true; size: 15; color: qc.ink }
                Lbl { width: parent.width; text: qc.tr("Every word here is one you know. Do you understand it?"); size: 13; color: qc.ink; opacity: 0.7; wrapMode: Text.Wrap }
            }
            FlatButton { id: closeBtn; kind: "flat"; glyph: "close"; implicitWidth: 32; implicitHeight: 32; focusPolicy: Qt.NoFocus; ToolTip.visible: hovered; ToolTip.text: qc.tr("Close"); onClicked: qc.closed() }
        }
        Text {
            width: parent.width
            text: qc.text
            wrapMode: Text.Wrap
            color: qc.ink
            font.pixelSize: 20
            font.family: Theme.type.family
            textFormat: Text.PlainText
        }
        // the meaning, once asked for
        Text {
            visible: qc.revealed
            width: parent.width
            text: qc.shownMeaning !== "" ? qc.shownMeaning : (qc.trError !== "" ? qc.trError : qc.tr("Translating…"))
            wrapMode: Text.Wrap
            color: qc.trError !== "" && qc.shownMeaning === "" ? Theme.c.error : qc.ink
            opacity: 0.8
            font.pixelSize: 15
            font.italic: true
            font.family: Theme.type.family
        }
        Flow {
            width: parent.width
            spacing: 8
            visible: !qc.answered
            FlatButton { visible: !qc.revealed; kind: "flat"; glyph: "eye"; text: qc.tr("Show the meaning"); focusPolicy: Qt.NoFocus; onClicked: qc.reveal() }
            FlatButton { kind: "suggested"; glyph: "check"; text: qc.tr("I understood"); focusPolicy: Qt.NoFocus; onClicked: qc.answer(true) }
            FlatButton { kind: "flat"; text: qc.tr("Not yet"); focusPolicy: Qt.NoFocus; onClicked: qc.answer(false) }
        }
        Lbl { visible: qc.answered; text: qc.tr("Saved to your knowledge."); size: 13; color: qc.ink; opacity: 0.7 }
    }
}
