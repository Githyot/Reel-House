import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A card of a deck in Jlab mode (core: jlab.rs, controller_jlab.rs): the sentence as word tiles at the chosen
// level of reading help, furigana above the words whose kanji the user does not know yet. A tap on a word looks
// it up; its meaning goes to the back and it becomes the gap of the reading card (the hint is the meaning).
//
//     JlabCard { width: parent.width; jlab: card.jlab; shown: rv.shown }
Column {
    id: jc
    property var jlab: ({})
    property bool shown: false
    property bool narrow: false
    readonly property var toks: jlab && jlab.tokens ? jlab.tokens : []
    readonly property bool reading: (jlab.phase || 0) >= 1
    function tr(s) { return Core.tr(Core.uiLang, s) }
    signal settings()
    function look(w) { if (w !== "") Core.cmd({cmd: "jlab_lookup", nid: jc.jlab.nid, word: w}) }
    spacing: 14

    // the phase and the quick levels of reading help (set for this kind of card)
    Item {
        width: parent.width
        height: 34
        Rectangle {
            id: phaseChip
            anchors.verticalCenter: parent.verticalCenter
            height: 26
            width: phaseLbl.implicitWidth + 22
            radius: 13
            color: jc.reading ? Theme.c.accent : Theme.c.fill
            Text {
                id: phaseLbl
                anchors.centerIn: parent
                text: (jc.reading ? "📖 " + jc.tr("Reading") : "🎧 " + jc.tr("Listening"))
                color: jc.reading ? Theme.c.accentText : Theme.c.text
                font.pixelSize: 13
                font.weight: Font.DemiBold
                font.family: Theme.type.family
            }
        }
        FlatButton {
            id: gear
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            kind: "plain"; glyph: "settings"; iconSize: 16; implicitHeight: 32
            ToolTip.visible: hovered; ToolTip.delay: 400; ToolTip.text: jc.tr("Jlab settings…")
            onClicked: jc.settings()
        }
        SheetSegments {
            anchors { right: gear.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
            width: Math.min(parent.width - phaseChip.width - gear.width - 18, jc.narrow ? 200 : 320)
            model: [
                {id: "0", label: jc.narrow ? "ro" : "romaji", tip: jc.tr("Romaji")},
                {id: "1", label: "かな", tip: jc.tr("Kana")},
                {id: "2", label: "漢+か", tip: jc.tr("Kana + known kanji")},
                {id: "3", label: "ふり", tip: jc.tr("Kanji + furigana")}
            ]
            current: "" + (jc.jlab.level || 0)
            onPicked: function(id) { Core.cmd({cmd: "jlab_set", key: jc.reading ? "read" : "listen", value: parseInt(id)}) }
        }
    }

    // the sentence: one tile per word
    Flow {
        visible: jc.jlab.showText !== false
        width: parent.width
        spacing: 4
        layoutDirection: Qt.LeftToRight
        Repeater {
            model: jc.toks
            delegate: Item {
                id: tile
                required property var modelData
                readonly property bool gap: modelData.gap === true && !jc.shown
                readonly property bool hot: modelData.gap === true && jc.shown
                readonly property bool busy: (jc.jlab.busy || "") !== "" && jc.jlab.busy === modelData.w
                width: Math.max(word.implicitWidth, ruby.implicitWidth) + (modelData.punct ? 0 : 8)
                height: ruby.height + word.height + 6
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.shape.small
                    visible: !tile.modelData.punct
                    color: tile.hot ? Theme.c.selected : (ta.containsMouse ? Theme.c.hover : "transparent")
                    border.width: tile.gap || tile.busy ? 2 : 0
                    border.color: Theme.c.accent
                }
                Text {
                    id: ruby
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 2
                    height: 16
                    text: tile.gap ? "" : (tile.modelData.r || "")
                    color: Theme.c.textDim
                    font.pixelSize: 12
                    font.family: Theme.type.family
                }
                Text {
                    id: word
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: ruby.height + 2
                    text: tile.gap ? "＿＿" : tile.modelData.t
                    color: tile.gap ? Theme.c.accent : (tile.modelData.tip !== "" ? Theme.c.accentText : Theme.c.text)
                    font.pixelSize: jc.narrow ? 24 : 28
                    font.family: Theme.type.family
                    font.underline: tile.modelData.tip !== "" && !tile.gap
                }
                MouseArea {
                    id: ta
                    anchors.fill: parent
                    enabled: !tile.modelData.punct
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: jc.look(tile.modelData.w)
                }
                ToolTip.visible: ta.containsMouse && (tile.modelData.tip || "") !== "" && !tile.gap
                ToolTip.delay: 300
                ToolTip.text: tile.modelData.tip || ""
            }
        }
    }
    Text {
        visible: jc.jlab.showText === false
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: "🎧"
        font.pixelSize: 44
    }
    // the gap's hint: the meaning looked up
    Text {
        visible: jc.reading && !jc.shown && jc.jlab.gapOn === true && (jc.jlab.hint || "") !== ""
        width: parent.width
        wrapMode: Text.Wrap
        text: jc.tr("Hint") + ": " + (jc.jlab.hint || "")
        color: Theme.c.textDim
        font.pixelSize: 16
        font.italic: true
        font.family: Theme.type.family
    }
    Text {
        visible: (jc.jlab.busy || "") !== ""
        text: jc.tr("Looking up…") + " " + (jc.jlab.busy || "")
        color: Theme.c.textDim
        font.pixelSize: 13
        font.family: Theme.type.family
    }
    Text {
        visible: jc.jlab.readOk === false && jc.jlab.level !== 3 && jc.jlab.showText !== false
        width: parent.width
        wrapMode: Text.Wrap
        text: jc.tr("No reading for this sentence: look a word up to get one.")
        color: Theme.c.warningText
        font.pixelSize: 13
        font.family: Theme.type.family
    }

    // ---------- the back ----------
    Rectangle { visible: jc.shown; width: parent.width; height: 1; color: Theme.c.divider }
    Text {
        visible: jc.shown && (jc.jlab.translation || "") !== ""
        width: parent.width
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignHCenter
        text: jc.jlab.translation || ""
        color: Theme.c.text
        font.pixelSize: jc.narrow ? 17 : 19
        font.family: Theme.type.family
    }
    // the words looked up: the dictionary
    Column {
        visible: jc.shown && jc.jlab.looks !== undefined && jc.jlab.looks.length > 0
        width: parent.width
        spacing: 6
        Text { text: jc.tr("Dictionary"); color: Theme.c.textDim; font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family }
        Repeater {
            model: jc.jlab.looks || []
            delegate: Rectangle {
                required property var modelData
                width: parent.width
                height: lk.implicitHeight + 14
                radius: Theme.shape.small
                color: modelData.gap ? Theme.c.selected : Theme.c.fill
                Row {
                    id: lk
                    x: 10
                    y: 7
                    width: parent.width - 54
                    spacing: 10
                    Column {
                        Text { text: modelData.ruby || ""; visible: text !== ""; color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family }
                        Text { text: modelData.shown || modelData.word; color: Theme.c.text; font.pixelSize: 18; font.weight: Font.DemiBold; font.family: Theme.type.family }
                    }
                    Text {
                        width: lk.width - x
                        anchors.verticalCenter: parent.verticalCenter
                        wrapMode: Text.Wrap
                        text: modelData.meaning
                        color: Theme.c.text
                        font.pixelSize: 15
                        font.family: Theme.type.family
                    }
                }
                FlatButton {
                    anchors { right: parent.right; rightMargin: 4; verticalCenter: parent.verticalCenter }
                    kind: "plain"; glyph: "close"; iconSize: 14; implicitHeight: 30
                    ToolTip.visible: hovered; ToolTip.delay: 400; ToolTip.text: jc.tr("Remove")
                    onClicked: Core.cmd({cmd: "jlab_unlook", nid: jc.jlab.nid, word: modelData.word})
                }
            }
        }
    }
    Text {
        visible: jc.shown && (!jc.jlab.looks || jc.jlab.looks.length === 0)
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: jc.tr("Tap a word to look it up")
        color: Theme.c.textDim
        font.pixelSize: 13
        font.family: Theme.type.family
    }
}
