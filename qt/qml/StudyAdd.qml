import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Study: make new cards. Pick the deck and the note type, fill the fields, Add (Ctrl+Enter).
Item {
    id: ad
    property var decks: []
    property var models: []
    property real startDeck: 0
    property real deckId: 0
    property int modelIdx: 0
    readonly property var model: models.length > 0 ? models[Math.min(modelIdx, models.length - 1)] : null
    readonly property real cw: Math.min(760, width - Theme.spacing.page * 2)
    readonly property real x0: (width - cw) / 2
    signal back()
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function pickStart() { if (startDeck !== 0) deckId = startDeck; else if (deckId === 0 && decks.length > 0) deckId = decks[0].id }
    onDecksChanged: if (deckId === 0) pickStart()
    onStartDeckChanged: pickStart()
    onVisibleChanged: if (visible) { pickStart(); focusFirst.restart() }
    Component.onCompleted: pickStart()
    function deckIndex() { for (var i = 0; i < decks.length; ++i) if (decks[i].id === deckId) return i; return 0 }
    function add() {
        if (!ad.model || fields.count === 0) return
        var vals = []
        for (var i = 0; i < fields.count; ++i) vals.push(fields.itemAt(i).value)
        if (vals[0].trim() === "") return
        Core.cmd({cmd: "study_add_note", did: ad.deckId, mid: ad.model.id, fields: vals})
        happyTimer.restart()
        for (var k = 0; k < fields.count; ++k) fields.itemAt(k).value = ""
        fields.itemAt(0).takeFocus()
    }
    Timer { id: focusFirst; interval: 50; onTriggered: if (fields.count > 0 && fields.itemAt(0)) fields.itemAt(0).takeFocus() }

    StudyHeader {
        id: hdr
        x: ad.x0; width: ad.cw
        y: Theme.layout.largeTitles === true ? 0 : 12
        title: ad.tr("Add cards")
        subtitle: ad.decks.length > 0 ? ad.decks[ad.deckIndex()].name.split("::").join(" › ") : ""
        onBack: ad.back()
    }
    Flickable {
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; top: hdr.bottom; topMargin: 12 }
        contentWidth: width
        contentHeight: col.height + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        Column {
            id: col
            x: ad.x0
            width: ad.cw
            spacing: 20
            PrefGroup {
                PrefCombo {
                    title: ad.tr("Deck")
                    options: ad.decks.map(function(d) { return d.name })
                    current: ad.deckIndex()
                    onPicked: function(i) { ad.deckId = ad.decks[i].id }
                }
                PrefCombo {
                    title: ad.tr("Type")
                    options: ad.models.map(function(m) { return m.name })
                    current: ad.modelIdx
                    onPicked: function(i) { ad.modelIdx = i }
                }
            }
            StudySurface {
                width: parent.width
                height: fcol.implicitHeight + 40
                Column {
                    id: fcol
                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
                    spacing: 16
                    Text {
                        visible: ad.model !== null && ad.model.cloze === true
                        width: parent.width; wrapMode: Text.Wrap
                        color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family
                        text: ad.tr("Hide a word with {{c1::word}} or with a hint {{c1::word::hint}}. Each number makes one card.")
                    }
                    Repeater {
                        id: fields
                        model: ad.model ? ad.model.fields : []
                        delegate: Column {
                            required property var modelData
                            required property int index
                            property alias value: ta.text
                            function takeFocus() { ta.forceActiveFocus() }
                            width: fcol.width
                            spacing: 6
                            Text { text: modelData; color: Theme.c.textDim; font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family }
                            TextArea {
                                id: ta
                                width: parent.width
                                height: Math.max(index === 0 ? 96 : 72, Math.min(220, implicitHeight))
                                wrapMode: TextArea.Wrap
                                selectByMouse: true
                                color: Theme.c.text
                                font.pixelSize: 16
                                font.family: Theme.type.family
                                leftPadding: 12; rightPadding: 12; topPadding: 10; bottomPadding: 10
                                background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: ta.activeFocus ? 2 : 1; border.color: ta.activeFocus ? Theme.c.accent : Theme.c.border }
                            }
                        }
                    }
                }
            }
            Item {
                width: parent.width; height: 44
                Text {
                    anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                    text: ad.tr("Ctrl+Enter adds the card")
                    color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
                }
                FlatButton {
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    kind: "suggested"; pill: true; glyph: "plus"; text: ad.tr("Add")
                    implicitWidth: 140
                    enabled: ad.model !== null
                    onClicked: ad.add()
                }
            }
        }
    }
    // the macaw is always on the Study pages: happy for a moment after each card added
    Mascot {
        id: buddy
        z: 3
        // beside the column when there is room, small in the corner otherwise
        size: ad.x0 >= 120 ? 96 : 52
        x: ad.width - size - 14
        y: ad.height - size - 14
        mood: happyTimer.running ? "happy" : "idle"
        animate: ad.visible
        Timer { id: happyTimer; interval: 1400 }
    }

    Shortcut { sequences: ["Ctrl+Return", "Ctrl+Enter"]; enabled: ad.visible; onActivated: ad.add() }
}
