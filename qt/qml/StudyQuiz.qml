import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Reelhouse 1.0
import "Osd.js" as Osd

// Study while watching (studyplay.rs): the question shown over the paused video, Duolingo style. The core sends it as
// `player.studyQuiz` ({id, kind, challenge, text, word, options, bank, joiner, cardFront, cardBack, result, ...}):
//   translate / word  choose one of the options      build  tap the words in order      type  type what was heard
//   card              a flashcard of the user, shown, turned and graded by the user
// Check sends study_quiz_answer; the answer sheet (green or red) has Continue (study_quiz_skip plays on). The notice
// before a part without subtitles (`player.studyNotice`) shows at the top. The player is always dark: Osd tokens only.
// With the macaw on (`quiz.mascot`, setting general.study_mascot_cues) it pulls the card in from the side and pushes it
// out after the answer (docs/mascot-contract.md); its other cues over the video (`player.studyCue`: shh during a part
// without subtitles, a peek at a mixed word) are passed on to Core.mascotCue.
Item {
    id: sq
    property var quiz: null            // Core.player.studyQuiz
    property string notice: ""         // Core.player.studyNotice
    property var cue: null             // Core.player.studyCue ({n, cue, ms})
    readonly property int cueN: cue ? cue.n : 0
    onCueNChanged: if (cue && cueN > 0 && typeof Core.mascotCue === "function") Core.mascotCue({cue: cue.cue, ms: cue.ms})
    readonly property bool busy: quiz !== null && quiz !== undefined
    readonly property real qid: busy ? quiz.id : -1
    readonly property var result: busy && quiz.result ? quiz.result : null
    readonly property string challenge: busy ? quiz.challenge : ""
    readonly property var o: Osd.t(Theme.layout.player)
    readonly property color ink: Osd.readable(Theme.c.accent)
    readonly property color good: "#58cc02"
    readonly property color bad: "#ff4b4b"
    readonly property bool mascotOn: busy && quiz.mascot === true
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function arr(v) { return v ? v : [] }

    // what the user chose so far (reset for every question)
    property int picked: -1
    property var built: []              // indexes into quiz.bank, in the order tapped
    property bool revealed: false       // card: the back is shown
    property bool useBank: false        // type: answer with the word bank instead of the keyboard
    onQidChanged: {
        picked = -1; built = []; revealed = false; useBank = false; typed.text = ""
        if (busy && challenge === "type") typed.forceActiveFocus()
        // the macaw pulls the new card in from the side
        pushAnim.stop()
        if (busy && quiz.mascot === true) pullAnim.restart(); else { pullAnim.stop(); slide.x = 0 }
    }

    function builtText() {
        const b = arr(quiz.bank)
        return built.map(function(i) { return b[i] }).join(quiz.joiner || "")
    }
    readonly property bool canCheck: {
        if (!busy || result) return false
        if (challenge === "translate" || challenge === "word") return picked >= 0
        if (challenge === "build") return built.length > 0
        if (challenge === "type") return useBank ? built.length > 0 : typed.text.trim() !== ""
        return false
    }
    function check() {
        if (!canCheck) return
        var a = ""
        if (challenge === "translate" || challenge === "word") a = String(picked)
        else if (challenge === "build" || (challenge === "type" && useBank)) a = builtText()
        else a = typed.text
        Core.cmd({cmd: "study_quiz_answer", id: quiz.id, answer: a})
    }
    function grade(right) { Core.cmd({cmd: "study_quiz_answer", id: quiz.id, answer: right ? "right" : "wrong"}) }
    function next() {
        if (!busy || pushAnim.running) return
        // the macaw pushes the card out first; the episode plays on when it is gone
        if (mascotOn) { pushId = quiz.id; pushAnim.restart() }
        else Core.cmd({cmd: "study_quiz_skip", id: quiz.id})
    }
    property real pushId: -1
    function replay() { Core.cmd({cmd: "study_quiz_replay", id: quiz.id}) }

    // the question takes the clicks and the keys while it shows (the video behind does not toggle)
    MouseArea { anchors.fill: parent; enabled: sq.busy; visible: enabled; hoverEnabled: true; onWheel: function(w) { w.accepted = true } }
    Rectangle {
        anchors.fill: parent
        visible: sq.busy
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#00000000" }
            GradientStop { position: 0.55; color: "#73000000" }
            GradientStop { position: 1.0; color: "#b3000000" }
        }
    }
    Keys.onReturnPressed: sq.result ? sq.next() : sq.check()
    Keys.onEscapePressed: if (sq.busy) sq.next()

    // ---- the notice before a part without subtitles ----
    Rectangle {
        visible: sq.notice !== "" && !sq.busy
        anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: 84 }
        width: noticeRow.implicitWidth + 32
        height: 40
        radius: Osd.r(sq.o.ctlR, height)
        color: sq.o.panel
        border.width: sq.o.strokeW
        border.color: sq.o.stroke
        Row {
            id: noticeRow
            anchors.centerIn: parent
            spacing: 10
            Icon { name: "subtitles"; size: 18; color: sq.ink; anchors.verticalCenter: parent.verticalCenter }
            Text { text: sq.tr(sq.notice); color: sq.o.fg; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
        }
    }

    // ---- the card ----
    // it slides in from the right (pulled by the macaw) and out to the right (pushed)
    NumberAnimation { id: pullAnim; target: slide; property: "x"; from: (sq.width + card.width) / 2 + 140; to: 0; duration: 650; easing.type: Easing.OutBack; easing.overshoot: 0.8 }
    SequentialAnimation {
        id: pushAnim
        PauseAnimation { duration: 120 }   // the macaw leans in
        NumberAnimation { target: slide; property: "x"; to: (sq.width + card.width) / 2 + 160; duration: 420; easing.type: Easing.InBack }
        ScriptAction { script: { Core.cmd({cmd: "study_quiz_skip", id: sq.pushId}); pushReset.restart() } }
    }
    // the question did not close (it changed on the core's side): the card comes back
    Timer { id: pushReset; interval: 800; onTriggered: if (sq.busy && !pullAnim.running) slide.x = 0 }
    Rectangle {
        id: card
        visible: sq.busy
        transform: Translate { id: slide }
        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: Math.max(24, parent.height * 0.08) }
        width: Math.min(640, parent.width - 32)
        height: body.implicitHeight + 40
        radius: Math.max(16, sq.o.cardR)
        color: sq.o.panel
        border.width: sq.o.strokeW
        border.color: sq.o.stroke
        clip: true

        ColumnLayout {
            id: body
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
            spacing: 14

            // the kind of question
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Icon { name: sq.challenge === "type" || sq.challenge === "build" ? "audio" : (sq.challenge === "card" ? "study" : "translate"); size: 18; color: sq.ink }
                Text {
                    Layout.fillWidth: true
                    text: sq.quiz && sq.quiz.kind === "listen" ? sq.tr("Listening practice") : sq.tr("Study while watching")
                    color: sq.ink
                    font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family
                    font.capitalization: Font.AllUppercase
                }
                OsdButton { kind: "round"; glyph: "repeat"; visible: sq.challenge !== "card"; enabled: sq.busy && !sq.quiz.replaying; ToolTip.visible: hovered; ToolTip.text: sq.tr("Play the line again"); onClicked: sq.replay() }
            }
            Text {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: {
                    switch (sq.challenge) {
                    case "translate": return sq.tr("What does this line mean?")
                    case "word": return sq.tr("What does this word mean?")
                    case "build": return sq.tr("Put the words of the line you heard in order")
                    case "type": return sq.tr("Type what you heard")
                    case "card": return sq.tr("Do you remember this card?")
                    }
                    return ""
                }
                color: sq.o.fg
                font.pixelSize: 20; font.weight: Font.Bold; font.family: Theme.type.family
            }
            // the line (when it is not the answer), the asked word in the accent
            Text {
                Layout.fillWidth: true
                visible: sq.busy && sq.quiz.text !== "" && sq.challenge !== "card"
                wrapMode: Text.Wrap
                textFormat: Text.StyledText
                text: {
                    if (!sq.busy) return ""
                    const esc = function(s) { return s.replace(/&/g, "&amp;").replace(/</g, "&lt;") }
                    const t = esc(sq.quiz.text || "")
                    const w = esc(sq.quiz.word || "")
                    if (sq.challenge === "word" && w !== "" && t.indexOf(w) >= 0)
                        return t.replace(w, "<b><font color='" + sq.ink + "'>" + w + "</font></b>")
                    return t
                }
                color: sq.o.fg
                font.pixelSize: 22; font.family: Theme.type.family
            }

            // ---- choose one ----
            Column {
                Layout.fillWidth: true
                visible: sq.challenge === "translate" || sq.challenge === "word"
                spacing: 8
                Repeater {
                    model: sq.busy ? sq.arr(sq.quiz.options).length : 0
                    delegate: Rectangle {
                        required property int index
                        readonly property bool sel: sq.picked === index
                        readonly property bool isRight: sq.result !== null && sq.quiz.answerIdx === index
                        readonly property bool wrongPick: sq.result !== null && sel && !isRight
                        width: parent.width
                        height: Math.max(44, optText.implicitHeight + 20)
                        radius: Osd.r(Math.max(10, sq.o.rowR), height)
                        color: isRight ? Qt.alpha(sq.good, 0.22) : (wrongPick ? Qt.alpha(sq.bad, 0.22) : (sel ? Qt.alpha(sq.ink, 0.18) : (optArea.containsMouse ? sq.o.hover : "transparent")))
                        border.width: 2
                        border.color: isRight ? sq.good : (wrongPick ? sq.bad : (sel ? sq.ink : sq.o.sep))
                        Text {
                            id: optText
                            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: 14; rightMargin: 14 }
                            text: sq.arr(sq.quiz.options)[index] || ""
                            wrapMode: Text.Wrap
                            color: sq.o.fg
                            font.pixelSize: 15; font.family: Theme.type.family
                        }
                        MouseArea { id: optArea; anchors.fill: parent; hoverEnabled: true; enabled: sq.result === null; cursorShape: Qt.PointingHandCursor; onClicked: sq.picked = index }
                    }
                }
            }

            // ---- word bank: the answer line, then the words left ----
            Rectangle {
                Layout.fillWidth: true
                visible: sq.challenge === "build" || (sq.challenge === "type" && sq.useBank)
                implicitHeight: Math.max(52, answerFlow.implicitHeight + 16)
                color: "transparent"
                Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: sq.o.sep }
                Flow {
                    id: answerFlow
                    anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
                    spacing: 6
                    Repeater {
                        model: sq.built.length
                        delegate: BankWord {
                            required property int index
                            label: sq.busy ? (sq.arr(sq.quiz.bank)[sq.built[index]] || "") : ""
                            onTapped: if (sq.result === null) { const b = sq.built.slice(); b.splice(index, 1); sq.built = b }
                        }
                    }
                }
            }
            Flow {
                Layout.fillWidth: true
                visible: sq.challenge === "build" || (sq.challenge === "type" && sq.useBank)
                spacing: 6
                Repeater {
                    model: sq.busy ? sq.arr(sq.quiz.bank).length : 0
                    delegate: BankWord {
                        required property int index
                        readonly property bool used: sq.built.indexOf(index) >= 0
                        label: sq.arr(sq.quiz.bank)[index] || ""
                        opacity: used ? 0.25 : 1
                        onTapped: if (!used && sq.result === null) sq.built = sq.built.concat([index])
                    }
                }
            }

            // ---- type what was heard ----
            TextField {
                id: typed
                Layout.fillWidth: true
                visible: sq.challenge === "type" && !sq.useBank
                enabled: sq.result === null
                placeholderText: sq.tr("Type what you heard")
                placeholderTextColor: sq.o.muted
                color: sq.o.fg
                selectionColor: Theme.c.accent
                selectedTextColor: Theme.c.accentFg
                font.pixelSize: 18; font.family: Theme.type.family
                background: Rectangle {
                    radius: Osd.r(Math.max(8, sq.o.ctlR), height)
                    color: sq.o.field
                    border.width: typed.activeFocus ? 2 : 1
                    border.color: typed.activeFocus ? sq.ink : sq.o.sep
                }
                onAccepted: sq.check()
            }
            Text {
                visible: sq.challenge === "type" && sq.result === null
                text: sq.useBank ? sq.tr("Use the keyboard") : sq.tr("Use the word bank")
                color: sq.ink
                font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { sq.useBank = !sq.useBank; sq.built = [] } }
            }

            // ---- a flashcard of the user ----
            Rectangle {
                Layout.fillWidth: true
                visible: sq.challenge === "card"
                implicitHeight: cardCol.implicitHeight + 24
                radius: Math.max(10, sq.o.rowR)
                color: sq.o.btn
                Column {
                    id: cardCol
                    anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: 14 }
                    spacing: 8
                    Text { width: parent.width; visible: sq.busy && (sq.quiz.cardDeck || "") !== ""; text: sq.busy ? sq.quiz.cardDeck || "" : ""; color: sq.o.muted; font.pixelSize: 12; font.family: Theme.type.family; elide: Text.ElideRight }
                    Text { width: parent.width; text: sq.busy ? sq.quiz.cardFront || "" : ""; wrapMode: Text.Wrap; color: sq.o.fg; font.pixelSize: 22; font.family: Theme.type.family }
                    Rectangle { visible: sq.revealed || sq.result !== null; width: parent.width; height: 1; color: sq.o.sep }
                    Text { width: parent.width; visible: sq.revealed || sq.result !== null; text: sq.busy ? sq.quiz.cardBack || "" : ""; wrapMode: Text.Wrap; color: sq.o.fg; font.pixelSize: 16; font.family: Theme.type.family }
                    Text { width: parent.width; visible: (sq.revealed || sq.result !== null) && sq.busy && (sq.quiz.translation || "") !== ""; text: sq.busy ? sq.quiz.translation || "" : ""; wrapMode: Text.Wrap; color: sq.o.dim; font.pixelSize: 14; font.italic: true; font.family: Theme.type.family }
                }
            }

            // ---- buttons ----
            RowLayout {
                Layout.fillWidth: true
                visible: sq.result === null
                spacing: 10
                OsdButton { kind: "tonal"; text: sq.tr("Skip"); onClicked: sq.next() }
                Item { Layout.fillWidth: true }
                OsdButton { visible: sq.challenge === "card" && !sq.revealed; kind: "accent"; text: sq.tr("Show answer"); onClicked: sq.revealed = true }
                OsdButton { visible: sq.challenge === "card" && sq.revealed; kind: "tonal"; glyph: "close"; text: sq.tr("I forgot"); onClicked: sq.grade(false) }
                OsdButton { visible: sq.challenge === "card" && sq.revealed; kind: "accent"; glyph: "check"; text: sq.tr("I knew it"); onClicked: sq.grade(true) }
                OsdButton { visible: sq.challenge !== "card"; kind: "accent"; enabled: sq.canCheck; text: sq.tr("Check"); onClicked: sq.check() }
            }
            Item { Layout.fillWidth: true; visible: sq.result !== null; implicitHeight: sheet.height - 8 }
        }

        // ---- the answer sheet: green when right, red when wrong ----
        Rectangle {
            id: sheet
            visible: sq.result !== null
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: sheetCol.implicitHeight + 32
            color: sq.result && sq.result.correct ? Qt.rgba(0.345, 0.8, 0.008, 0.2) : Qt.rgba(1, 0.294, 0.294, 0.2)
            Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 2; color: sq.result && sq.result.correct ? sq.good : sq.bad }
            ColumnLayout {
                id: sheetCol
                anchors { left: parent.left; right: parent.right; top: parent.top; margins: 16 }
                spacing: 8
                RowLayout {
                    spacing: 10
                    Rectangle {
                        width: 30; height: 30; radius: 15
                        color: sq.result && sq.result.correct ? sq.good : sq.bad
                        Icon { anchors.centerIn: parent; name: sq.result && sq.result.correct ? "check" : "close"; size: 18; color: "#ffffff" }
                    }
                    Text {
                        text: sq.result && sq.result.correct ? sq.tr("Nicely done!") : sq.tr("Correct answer:")
                        color: sq.result && sq.result.correct ? "#a6f06a" : "#ffb3b3"
                        font.pixelSize: 18; font.weight: Font.Bold; font.family: Theme.type.family
                    }
                }
                // the expected text, the parts missing from the answer underlined in red
                Text {
                    Layout.fillWidth: true
                    visible: sq.result !== null && (sq.result.expected || "") !== "" && sq.challenge !== "card"
                    wrapMode: Text.Wrap
                    textFormat: Text.StyledText
                    text: {
                        if (!sq.result) return ""
                        const esc = function(s) { return s.replace(/&/g, "&amp;").replace(/</g, "&lt;") }
                        const d = sq.arr(sq.result.diff)
                        if (d.length === 0) return esc(sq.result.expected)
                        return d.map(function(p) { return p.ok ? esc(p.t) : "<u><font color='#ff8a8a'>" + esc(p.t) + "</font></u>" }).join("")
                    }
                    color: sq.o.fg
                    font.pixelSize: 17; font.family: Theme.type.family
                }
                Text {
                    Layout.fillWidth: true
                    visible: sq.result !== null && sq.busy && sq.quiz.line !== "" && sq.quiz.line !== sq.result.expected && sq.challenge !== "translate"
                    wrapMode: Text.Wrap
                    text: sq.busy ? sq.quiz.line || "" : ""
                    color: sq.o.dim
                    font.pixelSize: 14; font.family: Theme.type.family
                }
                Text {
                    Layout.fillWidth: true
                    visible: sq.result !== null && sq.busy && (sq.quiz.translation || "") !== "" && sq.challenge !== "translate"
                    wrapMode: Text.Wrap
                    text: sq.busy ? sq.quiz.translation || "" : ""
                    color: sq.o.dim
                    font.pixelSize: 14; font.italic: true; font.family: Theme.type.family
                }
                RowLayout {
                    Layout.fillWidth: true
                    Item { Layout.fillWidth: true }
                    OsdButton { kind: "accent"; text: sq.tr("Continue"); onClicked: sq.next() }
                }
            }
        }
    }

    // ---- the macaw holding the top-left corner of the card: pulling it in, watching, pushing it out ----
    Mascot {
        id: buddy
        visible: sq.mascotOn && card.visible
        size: Math.max(64, Math.min(96, sq.height * 0.12))
        x: card.x + slide.x - width * 0.62
        y: card.y - height * 0.58
        z: 1
        mood: pushAnim.running ? "push" : (pullAnim.running ? "pull" : (sq.result ? (sq.result.correct ? "happy" : "sad") : "think"))
    }

    // a word of the bank
    component BankWord: Rectangle {
        id: chip
        property string label: ""
        signal tapped()
        width: chipText.implicitWidth + 24
        height: 38
        radius: Osd.r(Math.max(10, sq.o.rowR), height)
        color: chipArea.containsMouse ? sq.o.btnHover : sq.o.btn
        border.width: 1
        border.color: sq.o.sep
        Text { id: chipText; anchors.centerIn: parent; text: chip.label; color: sq.o.fg; font.pixelSize: 16; font.family: Theme.type.family }
        MouseArea { id: chipArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: chip.tapped() }
    }
}
