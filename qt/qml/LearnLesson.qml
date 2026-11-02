import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A Learn lesson on the whole page, in the way of Duolingo: the progress bar and hearts on top, one challenge at a
// time with big option cards or a word bank, Check at the bottom, then a green or red sheet with the answer and
// Continue. Missed challenges come back at the end; the finish screen shows XP, accuracy and time.
// Keys: 1–9 pick an option (or a word of the bank; in "match" 1–5 the left column and 6–0 the right one),
// Enter checks and continues, Backspace takes the last word back, Escape asks to leave.
// The macaw (Mascot) reacts in the feedback sheet, when the hearts run out and on the finish screen. Characters
// lessons (Japanese kana) add their own challenges: kana → romaji, romaji → kana, listen → kana and tracing.
Item {
    id: ll
    property var s: null
    property var ln: ({})
    property bool pageVisible: true
    signal command(var o)
    function tr(t) { return Core.tr(Core.uiLang, t) }
    // one of the macaw's funny lines (src/mascot_lines.rs), random and never the last one
    function quip(m) { const r = Core.state("mascot:" + Core.uiLang + ":" + m); return r && r.text ? r.text : "" }

    readonly property var ch: s && s.challenge ? s.challenge : null
    readonly property string status: s ? s.status : ""
    // the macaw's line on the finish screen, picked once when the lesson ends (a binding would pick again on each update)
    property string finishQuip: ""
    onStatusChanged: if (status === "done") finishQuip = quip(s && s.mistakes === 0 ? "lesson_perfect" : "lesson_finish")
    readonly property bool answered: status === "right" || status === "wrong"
    readonly property string kind: ch ? ch.kind : ""
    readonly property var options: ch && ch.options ? ch.options : []
    readonly property var readings: ch && ch.option_readings ? ch.option_readings : []
    readonly property var bank: ch && ch.bank ? ch.bank : []
    readonly property var pairs: ch && ch.pairs ? ch.pairs : []
    readonly property var verdict: s && s.verdict ? s.verdict : null
    readonly property bool playful: Theme.look === "android" || Theme.look === "apple"
    readonly property real rad: playful ? 16 : Math.max(6, Theme.shape.control)
    readonly property real cw: Math.min(660, width - 40)
    readonly property color cHeart: "#ff4b4b"
    readonly property color cGood: Theme.c.success
    readonly property color cBad: Theme.c.error
    // the challenge on screen; when it changes the answer is cleared
    readonly property string chKey: s ? (s.started + ":" + s.pos + ":" + kind + ":" + (ch ? ch.prompt + ch.audio + bank.length : "")) : ""

    // what the user is answering
    property int picked: -1
    property var chosen: []
    property string typed: ""
    property bool useBank: false
    property int leftSel: -1
    property int rightSel: -1
    property var matchedL: []
    property var matchedR: []
    property var missed: []
    property int flashL: -1
    property int flashR: -1

    onChKeyChanged: reset()
    readonly property bool kanaChoice: kind === "kana_romaji" || kind === "romaji_kana" || kind === "kana_listen"
    // the tracing card: whether something was drawn (the drawing itself lives in KanaTrace)
    property bool traced: false
    function reset() {
        picked = -1; chosen = []; typed = ""; traced = false
        leftSel = -1; rightSel = -1; matchedL = []; matchedR = []; missed = []; flashL = -1; flashR = -1
        // Japanese is hard to type without an input method: the word bank first
        useBank = ch !== null && ch.answer_lang === "jpn"
        // the sentence is heard at once (later: this runs while the new state is being read)
        if (ch && ch.audio && (kind === "listen" || kind === "assist" || kind === "kana_listen" || ((kind === "kana_romaji" || kind === "select") && ch.new_word === true)) && visible) Qt.callLater(ll.play, false)
        if (visible) keys.forceActiveFocus()
    }
    onVisibleChanged: if (visible) { reset(); keys.forceActiveFocus() }
    function play(slow) { if (ch && ch.audio) command({cmd: "learn_play", url: ch.audio, slow: slow}) }

    function answerText() {
        if (kind === "select" || kind === "assist" || kind === "fill" || kanaChoice) return picked >= 0 ? options[picked] : ""
        if (kind === "arrange" || (kind === "listen" && useBank)) return chosen.map(function(i) { return bank[i] }).join(" ")
        if (kind === "listen") return typed
        return ""
    }
    readonly property bool canCheck: !answered && status === "question" && (kind === "match" ? false : (kind === "kana_trace" ? traced : answerText().trim() !== ""))
    // the tracing card is judged here (how much of the character the strokes cover), then sent like any answer
    property var tracer: null
    function check() {
        if (!canCheck) return
        if (kind === "kana_trace") command({cmd: "learn_answer", answer: tracer ? tracer.verdict() : "no"})
        else command({cmd: "learn_answer", answer: answerText()})
    }
    function next() { command({cmd: "learn_continue"}) }
    function skip() { if (status === "question") command({cmd: "learn_answer", answer: ""}) }
    function tapBank(i) {
        if (answered || i < 0 || i >= bank.length || chosen.indexOf(i) >= 0) return
        chosen = chosen.concat([i])
    }
    function untap(pos) {
        if (answered) return
        const c = chosen.slice(); c.splice(pos, 1); chosen = c
    }
    // match: a word on the left and a meaning on the right; a right pair locks, a wrong one flashes red
    function tapLeft(i) { if (answered || matchedL.indexOf(i) >= 0) return; leftSel = i; tryPair() }
    function tapRight(j) { if (answered || matchedR.indexOf(j) >= 0) return; rightSel = j; tryPair() }
    function tryPair() {
        if (leftSel < 0 || rightSel < 0) return
        const w = pairs[leftSel][0]
        if (pairs[leftSel][1] === options[rightSel]) {
            matchedL = matchedL.concat([leftSel]); matchedR = matchedR.concat([rightSel])
            if (matchedL.length === pairs.length) command({cmd: "learn_answer", answer: "", missed: missed})
        } else {
            if (missed.indexOf(w) < 0) missed = missed.concat([w])
            flashL = leftSel; flashR = rightSel
            flashTimer.restart()
        }
        leftSel = -1; rightSel = -1
    }
    Timer { id: flashTimer; interval: 450; onTriggered: { ll.flashL = -1; ll.flashR = -1 } }

    Rectangle { anchors.fill: parent; color: Theme.c.window }

    // keys of the lesson (a text field takes them while typing)
    Item {
        id: keys
        focus: true
        Keys.onPressed: function(e) {
            if (!ll.visible || !ll.s) return
            if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) {
                if (ll.status === "done" || ll.answered) ll.next(); else ll.check()
                e.accepted = true
            } else if (e.key === Qt.Key_Escape) {
                if (ll.status === "done") ll.next(); else quitPop.open()
                e.accepted = true
            } else if (e.key === Qt.Key_Backspace && ll.chosen.length > 0) {
                ll.untap(ll.chosen.length - 1); e.accepted = true
            } else if (e.key >= Qt.Key_0 && e.key <= Qt.Key_9 && ll.status === "question") {
                const d = e.key === Qt.Key_0 ? 10 : e.key - Qt.Key_0
                if (ll.kind === "match") { if (d <= 5) ll.tapLeft(d - 1); else ll.tapRight(d - 6) }
                else if (ll.kind === "arrange" || (ll.kind === "listen" && ll.useBank)) ll.tapBank(d - 1)
                else if (d - 1 < ll.options.length) ll.picked = d - 1
                e.accepted = true
            }
        }
    }

    // ---------- top: leave, progress, hearts ----------
    Item {
        id: topRow
        visible: ll.status !== "done"
        x: (parent.width - Math.min(900, parent.width - 32)) / 2
        width: Math.min(900, parent.width - 32); height: 64
        FlatButton {
            id: closeBtn
            anchors { left: parent.left; verticalCenter: parent.verticalCenter }
            kind: "flatround"; glyph: "close"; iconSize: 22
            implicitWidth: 40; implicitHeight: 40
            ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: ll.tr("Leave the lesson") + " (Esc)"
            onClicked: quitPop.open()
        }
        Rectangle {
            id: bar
            anchors { left: closeBtn.right; leftMargin: 14; right: hearts.left; rightMargin: 16; verticalCenter: parent.verticalCenter }
            height: ll.playful ? 16 : 10
            radius: height / 2
            color: Theme.c.fill
            Rectangle {
                width: Math.max(parent.height, parent.width * (ll.s ? ll.s.progress : 0))
                height: parent.height; radius: parent.radius
                color: ll.cGood
                Behavior on width { NumberAnimation { duration: 350; easing.type: Easing.OutCubic } }
                // the shine of the playful bar
                Rectangle { visible: ll.playful; x: 8; y: 4; width: Math.max(0, parent.width - 16); height: 4; radius: 2; color: "#40ffffff" }
            }
        }
        // the combo: a flame with the answers right in a row; it pops at each one and burns brighter from 5
        Rectangle {
            id: combo
            readonly property int n: ll.s ? ll.s.combo : 0
            visible: n >= 2
            anchors { right: bar.right; bottom: bar.top; bottomMargin: 4 }
            width: comboRow.implicitWidth + 16; height: 26; radius: 13
            color: n >= 10 ? "#ff4b4b" : (n >= 5 ? "#ff9600" : Qt.rgba(1, 0.59, 0, 0.18))
            Row {
                id: comboRow
                anchors.centerIn: parent
                spacing: 4
                LearnGlyph { name: "flame"; size: 16; color: combo.n >= 5 ? "#ffffff" : "#ff9600"; filled: true; fillColor: combo.n >= 5 ? "#ffd27a" : Qt.rgba(1, 0.59, 0, 0.35); anchors.verticalCenter: parent.verticalCenter }
                Text { text: combo.n + " " + ll.tr("in a row"); color: combo.n >= 5 ? "#ffffff" : "#ff9600"; font.pixelSize: 12; font.weight: Font.Black; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
            }
            onNChanged: if (n >= 2) pop.restart()
            SequentialAnimation {
                id: pop
                NumberAnimation { target: combo; property: "scale"; from: 1; to: 1.25; duration: 120; easing.type: Easing.OutQuad }
                NumberAnimation { target: combo; property: "scale"; to: 1; duration: 220; easing.type: Easing.OutBack }
            }
        }
        Row {
            id: hearts
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: 6
            LearnGlyph { name: "heart"; size: 24; color: ll.cHeart; filled: true; fillColor: ll.cHeart; anchors.verticalCenter: parent.verticalCenter }
            Text { text: ll.ln.unlimited === true || (ll.s && ll.s.kind !== "lesson" && ll.s.kind !== "legendary") ? "∞" : String(ll.s ? ll.s.hearts : 0); color: ll.cHeart; font.pixelSize: 18; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
        }
    }

    // ---------- the challenge ----------
    Flickable {
        id: body
        visible: ll.status !== "done"
        anchors { top: topRow.bottom; bottom: foot.top; left: parent.left; right: parent.right }
        contentWidth: width
        contentHeight: Math.max(height, view.implicitHeight + 48)
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        Column {
            id: view
            x: (parent.width - ll.cw) / 2
            y: Math.max(16, (body.height - implicitHeight) / 2)
            width: ll.cw
            spacing: 22
            // "New word"
            Row {
                visible: ll.ch !== null && ll.ch.new_word === true
                spacing: 6
                LearnGlyph { name: "sparkles"; size: 18; color: "#ce82ff"; anchors.verticalCenter: parent.verticalCenter }
                Text { text: ll.tr("New word"); color: "#ce82ff"; font.pixelSize: 14; font.weight: Font.Black; font.family: Theme.type.family; font.capitalization: ll.playful ? Font.AllUppercase : Font.MixedCase; anchors.verticalCenter: parent.verticalCenter }
            }
            Text {
                width: parent.width
                wrapMode: Text.Wrap
                text: ll.title()
                color: Theme.c.text
                font.pixelSize: 24; font.weight: Font.Bold; font.family: Theme.type.display || Theme.type.family
            }
            Loader {
                width: parent.width
                sourceComponent: ll.kind === "select" ? selectView : ll.kind === "assist" ? assistView : ll.kind === "fill" ? fillView : ll.kind === "arrange" ? arrangeView : ll.kind === "listen" ? listenView : ll.kind === "match" ? matchView : ll.kanaChoice ? kanaView : ll.kind === "kana_trace" ? traceView : null
            }
        }
    }
    function title() {
        if (!ch) return ""
        switch (kind) {
        case "select": return ch.answer_lang === ln.course ? tr("Which one of these is “%1”?").arg(ch.prompt) : tr("Select the correct meaning")
        case "assist": return tr("Choose the correct translation")
        case "fill": return tr("Fill in the blank")
        case "arrange": return tr("Translate this sentence")
        case "listen": return useBank ? tr("Tap what you hear") : tr("Type what you hear")
        case "match": return tr("Tap the matching pairs")
        case "kana_romaji": return ch.new_word === true ? tr("New character! How does it sound?") : tr("How is this character read?")
        case "romaji_kana": return tr("Which character is “%1”?").arg(ch.prompt)
        case "kana_listen": return tr("Which character do you hear?")
        case "kana_trace": return tr("Trace the character")
        }
        return ""
    }

    // ---------- pieces ----------
    // a big answer card, numbered, green or red once checked
    component OptionCard: Item {
        id: oc
        property string num: ""
        property string label: ""
        property string reading: ""
        property bool selected: false
        property string mark: ""
        property bool big: false
        property bool faded: false
        signal tapped()
        readonly property int depth: ll.playful ? 4 : 0
        readonly property color edge: mark === "right" ? ll.cGood : (mark === "wrong" ? ll.cBad : (selected ? Theme.c.accent : Theme.c.border))
        implicitHeight: Math.max(big ? 64 : 54, ocCol.implicitHeight + 24) + depth
        opacity: faded ? 0.4 : 1
        Rectangle { visible: oc.depth > 0; anchors.fill: parent; radius: ll.rad; color: oc.edge }
        Rectangle {
            width: parent.width; height: parent.height - oc.depth
            y: ocArea.pressed ? oc.depth : 0
            radius: ll.rad
            color: oc.mark !== "" || oc.selected ? Qt.tint(Theme.c.card, Qt.rgba(oc.edge.r, oc.edge.g, oc.edge.b, 0.14)) : (ocArea.containsMouse ? Theme.c.cardHover : Theme.c.card)
            border.width: 2; border.color: oc.edge
            Rectangle {
                id: num
                visible: oc.num !== ""
                anchors { left: parent.left; leftMargin: 14; verticalCenter: parent.verticalCenter }
                width: 26; height: 26; radius: 7
                color: "transparent"; border.width: 2; border.color: oc.selected || oc.mark !== "" ? oc.edge : Theme.c.border
                Text { anchors.centerIn: parent; text: oc.num; color: oc.selected || oc.mark !== "" ? oc.edge : Theme.c.textDim; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family }
            }
            Column {
                id: ocCol
                anchors { left: num.visible ? num.right : parent.left; leftMargin: 14; right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
                spacing: 2
                Text { visible: oc.reading !== ""; width: parent.width; horizontalAlignment: num.visible ? Text.AlignLeft : Text.AlignHCenter; text: oc.reading; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                Text { width: parent.width; horizontalAlignment: num.visible ? Text.AlignLeft : Text.AlignHCenter; wrapMode: Text.Wrap; text: oc.label; color: oc.selected || oc.mark !== "" ? oc.edge : Theme.c.text; font.pixelSize: oc.big ? 20 : 17; font.weight: Font.DemiBold; font.family: Theme.type.family }
            }
        }
        MouseArea { id: ocArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: oc.tapped() }
    }
    function markOf(i) {
        if (!answered || !verdict) return ""
        if (options[i] === ch.answer) return "right"
        return i === picked ? "wrong" : ""
    }

    // a word of the bank
    component Chip: Item {
        id: cp
        property string label: ""
        property bool ghost: false
        signal tapped()
        readonly property int depth: ll.playful ? 3 : 0
        width: cpText.implicitWidth + 30; height: 44 + depth
        Rectangle { anchors.fill: parent; radius: ll.rad - 4; color: cp.ghost ? Theme.c.fill : Theme.c.border; visible: cp.ghost || cp.depth > 0 }
        Rectangle {
            visible: !cp.ghost
            width: parent.width; height: parent.height - cp.depth
            y: cpArea.pressed ? cp.depth : 0
            radius: ll.rad - 4
            color: cpArea.containsMouse ? Theme.c.cardHover : Theme.c.card
            border.width: 2; border.color: Theme.c.border
            Text { id: cpText; anchors.centerIn: parent; text: cp.label; color: Theme.c.text; font.pixelSize: 17; font.family: Theme.type.family }
        }
        MouseArea { id: cpArea; anchors.fill: parent; enabled: !cp.ghost; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: cp.tapped() }
    }

    // the sentence of a challenge, each word with its meaning on hover (dotted line under it)
    component Sentence: Flow {
        id: sn
        property var hints: []
        property string plain: ""
        property bool joined: false
        spacing: joined ? 0 : 6
        Repeater {
            model: sn.hints.length > 0 ? sn.hints.length : 0
            delegate: Item {
                required property int index
                readonly property var h: sn.hints[index] || ({})
                width: hw.implicitWidth; height: hw.implicitHeight + 4
                Text { id: hw; text: parent.h.t || ""; color: Theme.c.text; font.pixelSize: 22; font.family: Theme.type.family }
                Row {
                    visible: (parent.h.hint || "") !== ""
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                    spacing: 3
                    Repeater { model: Math.max(1, Math.floor(hw.implicitWidth / 6)); delegate: Rectangle { width: 3; height: 2; color: Theme.c.textDim } }
                }
                MouseArea { id: hArea; anchors.fill: parent; hoverEnabled: true; enabled: (parent.h.hint || "") !== "" }
                ToolTip.visible: hArea.containsMouse; ToolTip.text: h.hint || ""
            }
        }
        Text { visible: sn.hints.length === 0; width: sn.width; wrapMode: Text.Wrap; text: sn.plain; color: Theme.c.text; font.pixelSize: 22; font.family: Theme.type.family }
    }

    // the speaker buttons
    component Speaker: Row {
        property bool large: false
        spacing: 12
        LearnButton {
            implicitWidth: parent.large ? 110 : 54
            implicitHeight: (parent.large ? 90 : 48) + (ll.playful ? 4 : 0)
            tone: Theme.c.accent; fg: Theme.c.accentFg
            glyph: "volume"
            onClicked: ll.play(false)
        }
        LearnButton {
            visible: parent.large
            implicitWidth: 70
            implicitHeight: 64 + (ll.playful ? 4 : 0)
            anchors.bottom: parent.bottom
            tone: Theme.c.accent; fg: Theme.c.accentFg
            glyph: "turtle"
            onClicked: ll.play(true)
        }
    }

    // a speech bubble around a sentence
    component Bubble: Rectangle {
        default property alias inside: bcol.data
        width: parent ? parent.width : 400
        height: bcol.implicitHeight + 28
        radius: ll.rad
        color: "transparent"
        border.width: 2; border.color: Theme.c.border
        Column { id: bcol; x: 16; y: 14; width: parent.width - 32; spacing: 8 }
    }

    Component {
        id: selectView
        Column {
            spacing: 20
            // a word to learn: shown big with its reading
            Column {
                visible: ll.ch && ll.ch.prompt_lang !== ll.ch.answer_lang && ll.ch.answer_lang !== ll.ln.course
                width: parent.width
                spacing: 2
                Text { visible: (ll.ch ? ll.ch.reading : "") !== ""; width: parent.width; horizontalAlignment: Text.AlignHCenter; text: ll.ch ? ll.ch.reading : ""; color: Theme.c.textDim; font.pixelSize: 18; font.family: Theme.type.family }
                Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: ll.ch ? ll.ch.prompt : ""; color: Theme.c.text; font.pixelSize: 44; font.weight: Font.Bold; font.family: Theme.type.family }
                // hear the word again
                Speaker { visible: (ll.ch ? ll.ch.audio : "") !== ""; anchors.horizontalCenter: parent.horizontalCenter; topPadding: 8 }
            }
            Grid {
                id: selGrid
                width: parent.width
                columns: width >= 460 ? 2 : 1
                spacing: 12
                Repeater {
                    model: ll.options.length
                    delegate: OptionCard {
                        required property int index
                        width: (selGrid.width - (selGrid.columns - 1) * 12) / selGrid.columns
                        num: String(index + 1); big: true
                        label: ll.options[index]; reading: ll.readings[index] || ""
                        selected: ll.picked === index && !ll.answered
                        mark: ll.markOf(index)
                        onTapped: if (!ll.answered) ll.picked = index
                    }
                }
            }
        }
    }

    Component {
        id: assistView
        Column {
            spacing: 20
            Bubble {
                Row {
                    width: parent.width
                    spacing: 12
                    Speaker { visible: (ll.ch ? ll.ch.audio : "") !== "" }
                    Sentence { width: parent.width - 80; hints: ll.ch ? ll.ch.hints : []; plain: ll.ch ? ll.ch.prompt : ""; joined: ll.ch && ll.ch.prompt_lang === "jpn"; anchors.verticalCenter: parent.verticalCenter }
                }
                Text { visible: (ll.ch ? ll.ch.reading : "") !== ""; width: parent.width; wrapMode: Text.Wrap; text: ll.ch ? ll.ch.reading : ""; color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family }
            }
            Repeater {
                model: ll.options.length
                delegate: OptionCard {
                    required property int index
                    width: parent.width
                    num: String(index + 1)
                    label: ll.options[index]
                    selected: ll.picked === index && !ll.answered
                    mark: ll.markOf(index)
                    onTapped: if (!ll.answered) ll.picked = index
                }
            }
        }
    }

    Component {
        id: fillView
        Column {
            spacing: 18
            Text { width: parent.width; wrapMode: Text.Wrap; text: ll.ch ? ll.ch.prompt.replace("____", ll.picked >= 0 ? "［" + ll.options[ll.picked] + "］" : "＿＿＿") : ""; color: Theme.c.text; font.pixelSize: 26; font.family: Theme.type.family }
            Text { visible: text !== ""; width: parent.width; wrapMode: Text.Wrap; text: ll.ch ? ll.ch.help : ""; color: Theme.c.textDim; font.pixelSize: 16; font.italic: true; font.family: Theme.type.family }
            Grid {
                id: fillGrid
                width: parent.width
                columns: width >= 460 ? 2 : 1
                spacing: 12
                Repeater {
                    model: ll.options.length
                    delegate: OptionCard {
                        required property int index
                        width: (fillGrid.width - (fillGrid.columns - 1) * 12) / fillGrid.columns
                        num: String(index + 1)
                        label: ll.options[index]; reading: ll.readings[index] || ""
                        selected: ll.picked === index && !ll.answered
                        mark: ll.markOf(index)
                        onTapped: if (!ll.answered) ll.picked = index
                    }
                }
            }
        }
    }

    // the answer line of a word bank and the bank under it
    Component {
        id: bankParts
        Column {
            spacing: 24
            Item {
                width: parent.width
                height: Math.max(110, answerFlow.implicitHeight + 16)
                // the lines the words sit on
                Repeater {
                    model: Math.max(2, Math.ceil((answerFlow.implicitHeight + 8) / 56))
                    delegate: Rectangle { required property int index; y: 52 + index * 56; width: parent.width; height: 2; color: Theme.c.divider }
                }
                Flow {
                    id: answerFlow
                    width: parent.width
                    spacing: 8
                    Repeater {
                        model: ll.chosen.length
                        delegate: Chip {
                            required property int index
                            label: ll.bank[ll.chosen[index]] || ""
                            onTapped: ll.untap(index)
                        }
                    }
                }
            }
            Flow {
                width: parent.width
                spacing: 8
                Repeater {
                    model: ll.bank.length
                    delegate: Chip {
                        required property int index
                        label: ll.bank[index]
                        ghost: ll.chosen.indexOf(index) >= 0
                        onTapped: ll.tapBank(index)
                    }
                }
            }
        }
    }

    Component {
        id: arrangeView
        Column {
            spacing: 20
            Bubble {
                Row {
                    width: parent.width
                    spacing: 12
                    Speaker { visible: (ll.ch ? ll.ch.audio : "") !== "" }
                    Sentence { width: parent.width - ((ll.ch ? ll.ch.audio : "") !== "" ? 80 : 0); hints: ll.ch ? ll.ch.hints : []; plain: ll.ch ? ll.ch.prompt : ""; joined: ll.ch && ll.ch.prompt_lang === "jpn"; anchors.verticalCenter: parent.verticalCenter }
                }
                Text { visible: (ll.ch ? ll.ch.reading : "") !== ""; width: parent.width; wrapMode: Text.Wrap; text: ll.ch ? ll.ch.reading : ""; color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family }
            }
            Loader { width: parent.width; sourceComponent: bankParts }
        }
    }

    Component {
        id: listenView
        Column {
            spacing: 22
            Speaker { large: true; anchors.horizontalCenter: parent.horizontalCenter }
            Loader { width: parent.width; active: ll.useBank; visible: active; sourceComponent: bankParts }
            Rectangle {
                visible: !ll.useBank
                width: parent.width; height: 130
                radius: ll.rad
                color: Theme.c.card
                border.width: 2; border.color: typeArea.activeFocus ? Theme.c.accent : Theme.c.border
                ScrollView {
                    anchors.fill: parent; anchors.margins: 6
                    TextArea {
                        id: typeArea
                        placeholderText: ll.tr("Type in") + " " + (ll.ln.course_name || "")
                        wrapMode: TextArea.Wrap
                        readOnly: ll.answered
                        font.pixelSize: 18; font.family: Theme.type.family
                        color: Theme.c.text
                        background: null
                        onTextChanged: ll.typed = text
                        // a new challenge starts empty
                        Connections { target: ll; function onChKeyChanged() { typeArea.clear() } }
                        // Enter checks (Shift+Enter is a new line)
                        Keys.onReturnPressed: function(e) { if (e.modifiers & Qt.ShiftModifier) e.accepted = false; else { if (ll.answered) ll.next(); else ll.check(); e.accepted = true } }
                        Keys.onEscapePressed: quitPop.open()
                        Component.onCompleted: if (!ll.useBank) forceActiveFocus()
                    }
                }
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: ll.useBank ? ll.tr("Use the keyboard") : ll.tr("Use the word bank")
                color: Theme.c.accentText; font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; enabled: !ll.answered; onClicked: { ll.useBank = !ll.useBank; ll.chosen = []; ll.typed = ""; if (ll.useBank) keys.forceActiveFocus() } }
            }
        }
    }

    // the characters: a big character (or its romaji, or its sound) and four choices
    Component {
        id: kanaView
        Column {
            spacing: 22
            Item {
                width: parent.width
                height: ll.kind === "kana_listen" ? 110 : 150
                Text {
                    visible: ll.kind !== "kana_listen"
                    anchors.centerIn: parent
                    text: ll.ch ? ll.ch.prompt : ""
                    color: Theme.c.text
                    font.pixelSize: ll.kind === "kana_romaji" ? 110 : 64
                    font.weight: ll.kind === "kana_romaji" ? Font.Normal : Font.Bold
                    font.family: Theme.type.family
                }
                Speaker {
                    visible: (ll.ch ? ll.ch.audio : "") !== ""
                    large: ll.kind === "kana_listen"
                    anchors { right: ll.kind === "kana_listen" ? undefined : parent.right; horizontalCenter: ll.kind === "kana_listen" ? parent.horizontalCenter : undefined; verticalCenter: parent.verticalCenter }
                }
            }
            Grid {
                id: kGrid
                width: parent.width
                columns: width >= 460 ? 2 : 1
                spacing: 12
                Repeater {
                    model: ll.options.length
                    delegate: OptionCard {
                        required property int index
                        width: (kGrid.width - (kGrid.columns - 1) * 12) / kGrid.columns
                        num: String(index + 1); big: true
                        label: ll.options[index]
                        selected: ll.picked === index && !ll.answered
                        mark: ll.markOf(index)
                        onTapped: if (!ll.answered) ll.picked = index
                    }
                }
            }
        }
    }

    // tracing: draw over the faint character
    Component {
        id: traceView
        Column {
            spacing: 14
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 12
                Text { text: ll.ch ? ll.ch.reading : ""; color: Theme.c.textDim; font.pixelSize: 22; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                Speaker { visible: (ll.ch ? ll.ch.audio : "") !== ""; anchors.verticalCenter: parent.verticalCenter }
            }
            KanaTrace {
                id: tracer
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(300, parent.width); height: width
                kana: ll.ch ? ll.ch.prompt : ""
                ink: ll.answered ? (ll.verdict && ll.verdict.correct ? ll.cGood : ll.cBad) : Theme.c.accent
                locked: ll.answered
                onDrawnChanged: ll.traced = drawn
                Component.onCompleted: ll.tracer = tracer
                Component.onDestruction: if (ll.tracer === tracer) ll.tracer = null
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: ll.tr("Clear")
                visible: !ll.answered && ll.traced
                color: Theme.c.accentText; font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: tracer.clear() }
            }
        }
    }

    Component {
        id: matchView
        Row {
            spacing: 16
            Column {
                width: (parent.width - 16) / 2
                spacing: 12
                Repeater {
                    model: ll.pairs.length
                    delegate: OptionCard {
                        required property int index
                        width: parent.width
                        num: String(index + 1)
                        label: ll.pairs[index][0]; reading: ll.readings[index] || ""
                        selected: ll.leftSel === index
                        mark: ll.flashL === index ? "wrong" : (ll.matchedL.indexOf(index) >= 0 ? "right" : "")
                        faded: ll.matchedL.indexOf(index) >= 0 && !ll.answered
                        onTapped: ll.tapLeft(index)
                    }
                }
            }
            Column {
                width: (parent.width - 16) / 2
                spacing: 12
                Repeater {
                    model: ll.options.length
                    delegate: OptionCard {
                        required property int index
                        width: parent.width
                        num: String((index + 6) % 10)
                        label: ll.options[index]
                        selected: ll.rightSel === index
                        mark: ll.flashR === index ? "wrong" : (ll.matchedR.indexOf(index) >= 0 ? "right" : "")
                        faded: ll.matchedR.indexOf(index) >= 0 && !ll.answered
                        onTapped: ll.tapRight(index)
                    }
                }
            }
        }
    }

    // ---------- bottom: Skip / Check, then the feedback sheet ----------
    Rectangle {
        id: foot
        visible: ll.status !== "done"
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: Math.max(104, fbCol.implicitHeight + 40)
        color: !ll.answered ? "transparent" : Qt.tint(Theme.c.window, Qt.rgba(ll.verdict && ll.verdict.correct ? ll.cGood.r : ll.cBad.r, ll.verdict && ll.verdict.correct ? ll.cGood.g : ll.cBad.g, ll.verdict && ll.verdict.correct ? ll.cGood.b : ll.cBad.b, 0.16))
        Rectangle { visible: !ll.answered; width: parent.width; height: 2; color: Theme.c.divider }
        Item {
            x: (parent.width - Math.min(900, parent.width - 32)) / 2
            width: Math.min(900, parent.width - 32); height: parent.height
            // before checking
            LearnButton {
                visible: !ll.answered && ll.kind !== "match"
                anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                outline: true
                text: ll.kind === "listen" || ll.kind === "kana_listen" ? ll.tr("Can't listen now") : ll.tr("Skip")
                onClicked: ll.skip()
            }
            LearnButton {
                visible: !ll.answered
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                implicitWidth: 160
                tone: ll.cGood; fg: Theme.c.greenFg
                text: ll.tr("Check")
                enabled: ll.canCheck
                onClicked: ll.check()
            }
            // after checking
            Row {
                visible: ll.answered
                anchors { left: parent.left; right: contBtn.left; rightMargin: 16; verticalCenter: parent.verticalCenter }
                spacing: 14
                // the macaw is happy with a right answer and sad with a wrong one
                Mascot {
                    id: fbMascot
                    visible: ll.width >= 560
                    mood: ll.verdict && ll.verdict.correct ? "happy" : "sad"
                    size: 76
                    anchors.verticalCenter: parent.verticalCenter
                }
                Rectangle {
                    visible: !fbMascot.visible
                    width: 56; height: 56; radius: 28
                    color: Theme.c.card
                    anchors.verticalCenter: parent.verticalCenter
                    LearnGlyph { anchors.centerIn: parent; name: ll.verdict && ll.verdict.correct ? "check" : "x"; size: 30; stroke: 3; color: ll.verdict && ll.verdict.correct ? ll.cGood : ll.cBad }
                }
                Column {
                    id: fbCol
                    width: parent.width - (fbMascot.visible ? fbMascot.width : 56) - 14
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    readonly property color tone: ll.verdict && ll.verdict.correct ? ll.cGood : ll.cBad
                    Text { width: parent.width; wrapMode: Text.Wrap; text: ll.feedbackTitle(); color: fbCol.tone; font.pixelSize: 22; font.weight: Font.Bold; font.family: Theme.type.family }
                    Text {
                        visible: text !== ""
                        width: parent.width; wrapMode: Text.Wrap
                        text: ll.kind === "kana_trace" ? (ll.verdict && !ll.verdict.correct ? ll.tr("Follow the shape of the grey character.") : "") : (ll.verdict && !ll.verdict.correct && ll.kind !== "match" ? ll.verdict.shown : (ll.verdict && ll.verdict.note !== "" ? ll.verdict.shown : ""))
                        color: fbCol.tone; font.pixelSize: 17; font.family: Theme.type.family
                    }
                }
            }
            LearnButton {
                id: contBtn
                visible: ll.answered
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                implicitWidth: 160
                tone: ll.verdict && ll.verdict.correct ? ll.cGood : ll.cBad
                fg: ll.verdict && ll.verdict.correct ? Theme.c.greenFg : Theme.c.errorFg
                text: ll.tr("Continue")
                onClicked: ll.next()
            }
        }
    }
    function feedbackTitle() {
        if (!verdict) return ""
        if (!verdict.correct) return kind === "match" ? "" : (kind === "kana_trace" ? tr("Not quite.") : tr("Correct answer:"))
        if (verdict.note === "typo") return tr("You have a typo:")
        if (verdict.note === "accents") return tr("Pay attention to the accents:")
        if (s && s.combo >= 5 && s.combo % 5 === 0) return tr("%1 in a row!").arg(s.combo)
        const praise = [tr("Nicely done!"), tr("Correct!"), tr("Great job!"), tr("Excellent!"), tr("Amazing!")]
        return praise[(s ? s.solved : 0) % praise.length]
    }

    // ---------- out of hearts ----------
    Rectangle {
        anchors.fill: parent
        visible: ll.status === "no_hearts"
        color: Theme.c.scrim
        MouseArea { anchors.fill: parent }
        Rectangle {
            anchors.centerIn: parent
            width: Math.min(440, parent.width - 40)
            height: nhCol.implicitHeight + 48
            radius: ll.rad + 4
            color: Theme.c.window
            border.width: 1; border.color: Theme.c.border
            Column {
                id: nhCol
                x: 24; y: 24; width: parent.width - 48
                spacing: 14
                Mascot { mood: "tired"; size: 120; anchors.horizontalCenter: parent.horizontalCenter }
                Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: ll.tr("You ran out of hearts!"); bold: true; size: 22 }
                Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; dim: true; text: ll.tr("Refill them with gems, or practise what you learned to earn hearts back.") }
                LearnButton { width: parent.width; tone: Theme.c.accent; glyph: "gem"; text: ll.tr("Refill") + " · " + (ll.ln.price_refill || 60); enabled: (ll.ln.gems || 0) >= (ll.ln.price_refill || 60); onClicked: ll.command({cmd: "learn_buy", item: "refill"}) }
                LearnButton { width: parent.width; outline: true; glyph: "dumbbell"; text: ll.tr("Practise to earn hearts"); onClicked: ll.command({cmd: "learn_practice", mode: "weak"}) }
                LearnButton { width: parent.width; outline: true; text: ll.tr("End the lesson"); onClicked: ll.command({cmd: "learn_quit"}) }
            }
        }
    }

    // ---------- finished ----------
    Item {
        anchors.fill: parent
        visible: ll.status === "done"
        Column {
            anchors.centerIn: parent
            width: Math.min(560, parent.width - 40)
            spacing: 20
            // the macaw cheers (and says so)
            Mascot {
                anchors.horizontalCenter: parent.horizontalCenter
                mood: "cheer"
                size: Math.min(150, Math.max(100, ll.height * 0.17))
                say: ll.finishSay()
                bubbleSide: "right"
            }
            Lbl {
                width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; bold: true; size: 28
                text: ll.finishTitle()
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 12
                Repeater {
                    model: 3
                    delegate: Rectangle {
                        required property int index
                        readonly property var r: ll.s ? ll.s.result : ({})
                        readonly property color tone: ["#ffc800", ll.cGood, "#1cb0f6"][index]
                        width: Math.min(150, (ll.width - 64) / 3); height: 92
                        radius: ll.rad
                        color: tone
                        Text { x: 0; y: 6; width: parent.width; horizontalAlignment: Text.AlignHCenter; text: [ll.tr("Total XP"), ll.tr("Accuracy"), ll.tr("Time")][index]; color: "#ffffff"; font.pixelSize: 12; font.weight: Font.Black; font.family: Theme.type.family; font.capitalization: Font.AllUppercase }
                        Rectangle {
                            x: 3; y: 26; width: parent.width - 6; height: parent.height - 29; radius: ll.rad - 3
                            color: Theme.c.window
                            Row {
                                anchors.centerIn: parent
                                spacing: 6
                                LearnGlyph { name: ["zap", "target", "clock"][index]; size: 20; color: tone; anchors.verticalCenter: parent.verticalCenter }
                                Text {
                                    text: index === 0 ? String(r.xp || 0) : (index === 1 ? (r.accuracy || 0) + "%" : Math.floor((r.secs || 0) / 60) + ":" + ("0" + Math.floor((r.secs || 0) % 60)).slice(-2))
                                    color: tone; font.pixelSize: 22; font.weight: Font.Bold; font.family: Theme.type.family
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                            }
                        }
                    }
                }
            }
            // what changed: streak, goal, gems, achievements
            Column {
                width: parent.width
                spacing: 8
                Repeater {
                    model: ll.finishLines().length
                    delegate: Row {
                        required property int index
                        readonly property var m: ll.finishLines()[index]
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: 8
                        LearnGlyph { name: parent.m.glyph; size: 20; color: parent.m.tone; filled: true; fillColor: Qt.rgba(parent.m.tone.r, parent.m.tone.g, parent.m.tone.b, 0.3); anchors.verticalCenter: parent.verticalCenter }
                        Lbl { text: parent.m.text; anchors.verticalCenter: parent.verticalCenter }
                    }
                }
            }
            LearnButton {
                anchors.horizontalCenter: parent.horizontalCenter
                implicitWidth: 220
                tone: ll.cGood; fg: Theme.c.greenFg
                text: ll.tr("Continue")
                onClicked: ll.next()
            }
        }
    }
    function finishTitle() {
        if (!s) return ""
        if (s.kind === "legendary") return tr("Legendary!")
        if (s.kind === "kana") return s.lesson !== "" ? tr("New characters learned!") : tr("Practice complete!")
        if (s.kind !== "lesson") return tr("Practice complete!")
        return s.mistakes === 0 ? tr("Perfect lesson!") : tr("Lesson complete!")
    }
    function finishSay() {
        if (!s || !s.result) return ""
        const msgs = s.result.messages || []
        for (let i = 0; i < msgs.length; ++i) if (msgs[i].indexOf("milestone:") === 0) return tr("%1 days in a row! I'm so proud!").arg(msgs[i].substring(10))
        if (msgs.indexOf("goal") >= 0) return tr("Daily goal done! Open your chest.")
        if (finishQuip !== "") return finishQuip
        if (s.mistakes === 0) return tr("Not a single mistake!")
        return [tr("You did it!"), tr("Great work today!"), tr("Keep it up!")][(s.solved || 0) % 3]
    }
    function finishLines() {
        if (!s || !s.result) return []
        const out = []
        const r = s.result
        if ((r.gems || 0) > 0) out.push({glyph: "gem", tone: Qt.color("#1cb0f6"), text: "+" + r.gems + " " + tr("gems")})
        const msgs = r.messages || []
        for (let i = 0; i < msgs.length; ++i) {
            const m = msgs[i]
            if (m.indexOf("streak:") === 0) out.push({glyph: "flame", tone: Qt.color("#ff9600"), text: tr("Streak: %1 days").arg(m.substring(7))})
            else if (m.indexOf("milestone:") === 0) out.push({glyph: "flame", tone: Qt.color("#ff4b4b"), text: tr("Streak milestone: %1 days!").arg(m.substring(10))})
            else if (m === "goal") out.push({glyph: "target", tone: cGood, text: tr("Daily goal reached!")})
            else if (m === "chest") out.push({glyph: "gift", tone: Qt.color("#a560f0"), text: tr("Your daily chest is ready")})
            else if (m === "boost") out.push({glyph: "rocket", tone: Qt.color("#a560f0"), text: tr("XP boost: double XP")})
            else if (m.indexOf("achievement:") === 0) {
                const id = m.substring(12)
                const list = ln.achievements || []
                let name = id
                for (let k = 0; k < list.length; ++k) if (list[k].id === id) name = tr(list[k].name)
                out.push({glyph: "medal", tone: Qt.color("#ffc800"), text: tr("Achievement unlocked:") + " " + name})
            }
        }
        return out
    }

    // ---------- leaving in the middle ----------
    Popup {
        id: quitPop
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(420, (parent ? parent.width : 500) - 40)
        padding: 24
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        Overlay.modal: Rectangle { color: Theme.c.scrim }
        background: Rectangle { radius: ll.rad + 4; color: Theme.c.window; border.width: 1; border.color: Theme.c.border }
        onClosed: keys.forceActiveFocus()
        contentItem: Column {
            spacing: 14
            Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; bold: true; size: 20; text: ll.tr("Wait, don't go!") }
            Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; dim: true; text: ll.tr("You will lose the progress of this lesson if you leave now.") }
            LearnButton { width: parent.width; tone: Theme.c.accent; text: ll.tr("Keep learning"); onClicked: quitPop.close() }
            LearnButton { width: parent.width; outline: true; text: ll.tr("End the lesson"); onClicked: { quitPop.close(); ll.command({cmd: "learn_quit"}) } }
        }
    }
}
