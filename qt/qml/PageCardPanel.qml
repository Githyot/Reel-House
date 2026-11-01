import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The study card made from a part of a page (Images reader, card mode), like the player's clip editor: the picture
// cut from the page, the text read inside it (editable), its translation (with a button to translate it now), the deck
// and "Save to studies". The picture is cut from the real page by the C++ side (Core.cutPage), then the core copies it
// into the Study media folder and makes the card (study_add_picture).
Rectangle {
    id: cp
    property var rd
    readonly property var sel: rd ? rd.cardSel : null
    readonly property bool on: !!sel && !!rd && rd.editMode === "card"
    function tr(s) { return Core.tr(Core.uiLang, s) }
    visible: on
    radius: Theme.shape.card
    color: Theme.c.popover
    border.width: 1
    border.color: Theme.c.border
    implicitHeight: col.implicitHeight + 28

    // ---- the text and translation follow the page until the person types in them ----
    property bool textEdited: false
    property bool trEdited: false
    property string pageKey: ""
    onSelChanged: {
        if (!sel) return
        if (sel.url !== pageKey) { pageKey = sel.url; textEdited = false; trEdited = false }
        fillFromPage()
    }
    function fillFromPage() {
        const a = rd ? rd.cardAuto : null
        if (!a) return
        if (!textEdited) textArea.text = a.text || ""
        if (!trEdited) trArea.text = a.translation || ""
    }
    Connections { target: cp.rd; function onCardAutoChanged() { cp.fillFromPage() } }

    // ---- translate now (the same translator as the pages) ----
    property int trId: 0
    readonly property var trRes: trId > 0 && rd && rd.pt.translations ? rd.pt.translations[String(trId)] : null
    readonly property bool trBusy: trId > 0 && !trRes
    function translateNow() {
        const t = textArea.text.trim()
        if (t === "" || trBusy) return
        trId = rd.nextTrId++
        trTimeout.restart()
        Core.cmd({cmd: "translate_texts", kind: "manga", use: "study_cards", id: trId, texts: [t], target: rd.targetLang() || Core.uiLang})
    }
    onTrResChanged: {
        if (!trRes) return
        if (trRes.error) Core.toast(trRes.error)
        else { trArea.text = (trRes.texts || []).join("\n"); trEdited = true }
        trId = 0
    }
    // a chapter change drops the answer: the spinner does not stay forever
    Timer { id: trTimeout; interval: 90000; onTriggered: if (cp.trBusy) { cp.trId = 0; Core.toast(cp.tr("The translation did not answer.")) } }

    // ---- decks: the one used last is kept while the app runs ----
    readonly property var decks: {
        const list = (Core.study.decks || []).map(function(d) { return {id: d.id, name: d.name} })
        if (!list.some(function(d) { return d.name === "Seu Reels" })) list.unshift({id: 0, name: "Seu Reels"})
        return list
    }
    readonly property int deckIndex: {
        for (let i = 0; i < decks.length; ++i) if (decks[i].id === (rd ? rd.lastDeck : 0)) return i
        for (let i = 0; i < decks.length; ++i) if (decks[i].name === "Seu Reels") return i
        return 0
    }

    // ---- save: cut the picture, then make the card ----
    property string cutTag: ""
    readonly property bool saving: cutTag !== ""
    function save() {
        if (!sel || saving) return
        cutTag = "card" + Date.now()
        Core.cutPage(sel.url, Qt.rect(sel.x, sel.y, sel.w, sel.h), cutTag)
    }
    Connections {
        target: Core
        function onPageCut(tag, path, error) {
            if (tag !== cp.cutTag) return
            cp.cutTag = ""
            if (path === "") { Core.toast(cp.tr(error)); return }
            const ch = cp.rd.r.chapter || ({})
            const source = [cp.rd.r.title || "", ch.label || "", cp.tr("Page") + " " + (cp.sel ? cp.sel.page : "")].filter(function(x) { return x !== "" }).join(" · ")
            const d = cp.decks[cp.deckIndex] || {id: 0}
            Core.cmd({cmd: "study_add_picture", path: path, text: textArea.text.trim(), translation: trArea.text.trim(), deck: d.id, source: source})
            // ready for the next card
            cp.rd.cardSel = null
            cp.textEdited = false
            cp.trEdited = false
        }
    }

    component Field: Rectangle {
        property alias area: fieldArea
        width: parent.width
        height: Math.min(130, Math.max(52, fieldArea.implicitHeight + 14))
        radius: Theme.shape.control || 8
        color: Theme.c.view
        border.width: fieldArea.activeFocus ? 2 : 1
        border.color: fieldArea.activeFocus ? Theme.c.accent : Theme.c.border
        ScrollView {
            anchors.fill: parent; anchors.margins: 5
            TextArea {
                id: fieldArea
                wrapMode: TextArea.Wrap
                selectByMouse: true
                color: Theme.c.text
                selectionColor: Theme.c.accent
                selectedTextColor: Theme.c.accentFg
                placeholderTextColor: Theme.c.textDim
                font.pixelSize: 15
                font.family: Theme.type.family
                background: null
            }
        }
    }

    Flickable {
        id: scroll
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: col.implicitHeight + 28
        interactive: contentHeight > height + 1
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: scroll.interactive ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
        Column {
            id: col
            x: 14; y: 14
            width: cp.width - 28
            spacing: 10
            Item {
                width: parent.width; height: 30
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    Icon { name: "study"; size: 18; anchors.verticalCenter: parent.verticalCenter }
                    Lbl { text: cp.tr("New study card") + (cp.on ? "  ·  " + cp.tr("Page") + " " + cp.sel.page : ""); bold: true; size: 15; anchors.verticalCenter: parent.verticalCenter }
                }
                FlatButton { anchors { right: parent.right; verticalCenter: parent.verticalCenter } kind: "flatround"; glyph: "close"; onClicked: cp.rd.cardSel = null }
            }
            // the part of the page, shown from the page itself (the card gets the full-size cut)
            Item {
                id: prev
                width: parent.width
                readonly property real natW: full.implicitWidth
                readonly property real natH: full.implicitHeight
                readonly property real cutW: cp.on ? cp.sel.w * natW : 0
                readonly property real cutH: cp.on ? cp.sel.h * natH : 0
                readonly property real scale: cutW > 0 && cutH > 0 ? Math.min(width / cutW, 180 / cutH, 2) : 0
                height: scale > 0 ? cutH * scale : 60
                Rectangle { anchors.fill: parent; radius: 6; color: Theme.c.card }
                Item {
                    x: (prev.width - width) / 2
                    width: prev.cutW * prev.scale
                    height: prev.cutH * prev.scale
                    clip: true
                    PosterImage {
                        id: full
                        url: cp.on ? cp.sel.url : ""
                        width: prev.natW * prev.scale
                        height: prev.natH * prev.scale
                        x: cp.on ? -cp.sel.x * width : 0
                        y: cp.on ? -cp.sel.y * height : 0
                        fillMode: Image.Stretch
                    }
                }
                BusyIndicator { anchors.centerIn: parent; running: prev.scale <= 0 && cp.on; visible: running; width: 28; height: 28 }
            }
            Lbl { text: cp.tr("Text"); dim: true; size: 12 }
            Field { id: textField }
            Connections { target: textField.area; function onTextChanged() { if (textField.area.activeFocus) cp.textEdited = true } }
            Item {
                width: parent.width; height: 32
                Lbl { anchors.verticalCenter: parent.verticalCenter; text: cp.tr("Translation"); dim: true; size: 12 }
                Row {
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    spacing: 6
                    BusyIndicator { visible: cp.trBusy; running: visible; width: 24; height: 24; anchors.verticalCenter: parent.verticalCenter }
                    FlatButton {
                        kind: "flat"; glyph: "translate"; implicitHeight: 30
                        text: cp.trBusy ? cp.tr("Translating…") : cp.tr("Translate")
                        enabled: !cp.trBusy && textField.area.text.trim() !== ""
                        onClicked: cp.translateNow()
                    }
                }
            }
            Field { id: trField }
            Connections { target: trField.area; function onTextChanged() { if (trField.area.activeFocus) cp.trEdited = true } }
            PrefCombo {
                title: cp.tr("Deck"); first: true
                options: cp.decks.map(function(d) { return d.name })
                current: cp.deckIndex
                onPicked: function(i) { cp.rd.lastDeck = cp.decks[i].id }
            }
            Row {
                spacing: 8
                FlatButton {
                    kind: "suggested"
                    glyph: "study"
                    text: cp.saving ? cp.tr("Saving…") : cp.tr("Save to studies")
                    enabled: cp.on && !cp.saving
                    onClicked: cp.save()
                }
                BusyIndicator { visible: cp.saving; running: visible; width: 28; height: 28; anchors.verticalCenter: parent.verticalCenter }
            }
            Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; size: 11; text: cp.tr("Drag on the page to choose another part; drag the box or its handles to adjust it.") }
        }
    }
    // the text areas of the two fields (their ids live inside the component)
    property alias textArea: textField.area
    property alias trArea: trField.area
}
