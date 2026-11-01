import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "ReaderThemes.js" as RT

// The chosen speech bubble, beside the page: the original text (for who can read it), the machine translation,
// and the translation to correct, with bold, italic, underline, strike and highlight; then the look of the whole
// bubble (font, size, text colour, highlight drawn as a pill or a plain box) with a preview of how the page shows it.
// "Confirm" keeps it as reviewed by a person in the chapter's translation file (.md), look included, so another
// computer draws the bubble the same.
Rectangle {
    id: ep
    property var rd
    readonly property var it: rd ? rd.active : null
    readonly property bool on: it !== null && it !== undefined
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function focusEditor() { ed.forceActiveFocus() }
    visible: on
    radius: Theme.shape.card
    color: Theme.c.popover
    border.width: 1
    border.color: Theme.c.border
    implicitHeight: col.implicitHeight + 28

    // ---- the look of the whole bubble (see pagedb.rs: font, size, color, highlight) ----
    property string lookFont: ""            // "" = the reader's own font
    property int lookSize: 100              // percent of the size fitted to the box
    property string lookColor: ""           // "" = the default dark ink
    property string lookHl: ""              // "pill" (also "") or "plain"
    readonly property string fontShown: lookFont !== "" ? lookFont : Theme.type.family
    function look() { return {font: lookFont, size: lookSize, color: lookColor, highlight: lookHl} }

    // the editor starts from the hand translation, else the machine one, else empty
    property string loadedKey: ""
    onItChanged: load()
    Component.onCompleted: load()
    function load() {
        if (!on) { loadedKey = ""; trId = 0; return }
        const k = it.page + "|" + it.dbIndex + "|" + it.ocr
        if (k === loadedKey) return
        loadedKey = k
        trId = 0
        lookFont = it.font || ""
        lookSize = it.size || 100
        lookColor = it.color || ""
        lookHl = it.highlight || ""
        ed.text = it.html && it.html !== "" ? it.html : (it.mt || "")
    }

    // ---- translate the original now (when the page was not translated): the same translator as the pages ----
    property int trId: 0
    readonly property var trRes: trId > 0 && rd && rd.pt.translations ? rd.pt.translations[String(trId)] : null
    readonly property bool trBusy: trId > 0 && !trRes
    function translateNow() {
        const t = on ? (it.original || "").trim() : ""
        if (t === "" || trBusy) return
        trId = rd.nextTrId++
        trTimeout.restart()
        Core.cmd({cmd: "translate_texts", kind: "manga", use: "bubbles", id: trId, texts: [t], target: rd.targetLang() || Core.uiLang})
    }
    // the answer goes into the editor as plain text (escaped: the editor reads rich text)
    function escaped(t) { return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\n/g, "<br>") }
    onTrResChanged: {
        if (!trRes) return
        if (trRes.error) Core.toast(trRes.error)
        else { ed.text = escaped((trRes.texts || []).join("\n")); ed.forceActiveFocus() }
        trId = 0
    }
    // a chapter change drops the answer: the spinner does not stay forever
    Timer { id: trTimeout; interval: 90000; onTriggered: if (ep.trBusy) { ep.trId = 0; Core.toast(ep.tr("The translation did not answer.")) } }
    function save(reviewed) {
        if (!on) return
        rd.saveBubble(it.page, it, Object.assign({html: Core.richSimple(ed.textDocument), reviewed: reviewed}, look()))
    }
    // A change of the look is kept at once (with the text as it is in the editor), so the page shows it while it is
    // being chosen; it does not change whether the bubble was reviewed.
    function setLook(key, value) {
        if (key === "font") lookFont = value
        else if (key === "size") lookSize = Math.max(50, Math.min(250, value))
        else if (key === "color") lookColor = value
        else if (key === "highlight") lookHl = value
        if (on) rd.saveBubble(it.page, it, Object.assign({html: Core.richSimple(ed.textDocument), reviewed: it.reviewed === true}, look()))
    }
    function format(kind, value) {
        Core.richFormat(ed.textDocument, ed.selectionStart, ed.selectionEnd, kind, value)
        refreshState()
        edPills.later()
        previewLater()
    }

    component Tool: FlatButton {
        property string what: ""
        property bool on: false
        kind: on ? "suggested" : "flat"
        implicitWidth: 34; implicitHeight: 32
        focusPolicy: Qt.NoFocus
        onClicked: ep.format(what, "")
    }
    // a round colour: `col` "" is "none" (highlight) or "automatic" (text colour)
    component Swatch: Rectangle {
        id: sw
        property string col: ""
        property bool chosen: false
        property string tip: ""
        signal picked()
        width: 24; height: 24; radius: 12
        color: col === "" ? "transparent" : col
        border.width: chosen ? 2 : 1
        border.color: chosen ? Theme.c.accent : Theme.c.border
        // the chosen one has a ring around it, apart from the colour itself
        Rectangle { visible: sw.chosen; anchors.fill: parent; anchors.margins: -4; radius: width / 2; color: "transparent"; border.width: 2; border.color: Theme.c.accent }
        Text { visible: sw.col === ""; anchors.centerIn: parent; text: "∅"; color: Theme.c.textDim; font.pixelSize: 12 }
        MouseArea { id: swArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sw.picked() }
        ToolTip.visible: sw.tip !== "" && swArea.containsMouse
        ToolTip.text: sw.tip
    }
    property var fmt: ({})
    function refreshState() { fmt = Core.richState(ed.textDocument, Math.max(0, ed.cursorPosition - 1)) }

    // the preview follows the editor (a little later, not at every key)
    property string previewHtml: ""
    function previewNow() { previewHtml = Core.richSimple(ed.textDocument) }
    function previewLater() { Qt.callLater(ep.previewNow) }

    // fonts: the reader's own ("Original"), comic lettering fonts when this computer has them, then the app's fonts
    readonly property var fontList: {
        const fams = Qt.fontFamilies()
        const comic = RT.comicFonts.filter(function(f) { return fams.indexOf(f) >= 0 })
        return [""].concat(comic).concat(RT.bubbleFonts)
    }
    function fontName(f) { return f === "" ? tr("Original") : f }

    // When the window is too short for the whole panel (its height is limited by the reader), the content scrolls.
    Flickable {
        id: epScroll
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: col.implicitHeight + 28
        interactive: contentHeight > height + 1
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: epScroll.interactive ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
    Column {
        id: col
        x: 14; y: 14
        width: ep.width - 28
        spacing: 10
        Item {
            width: parent.width; height: 30
            Lbl { anchors.verticalCenter: parent.verticalCenter; text: ep.on ? ep.tr("Page") + " " + ep.it.page + (ep.it.reviewed ? "  ·  ✓ " + ep.tr("Reviewed") : "") : ""; bold: true; size: 15 }
            FlatButton { anchors { right: parent.right; verticalCenter: parent.verticalCenter } kind: "flatround"; glyph: "close"; onClicked: ep.rd.activate(null) }
        }
        Lbl { text: ep.tr("Original"); dim: true; size: 12 }
        Rectangle {
            width: parent.width
            height: Math.min(150, Math.max(44, origText.implicitHeight + 16))
            radius: Theme.shape.control || 8
            color: Theme.c.view
            border.width: 1
            border.color: Theme.c.border
            ScrollView {
                anchors.fill: parent; anchors.margins: 6
                TextArea {
                    id: origText
                    readOnly: true; selectByMouse: true; wrapMode: TextArea.Wrap
                    text: ep.on ? (ep.it.original || "") : ""
                    color: Theme.c.text
                    selectionColor: Theme.c.accent
                    selectedTextColor: Theme.c.accentFg
                    font.pixelSize: 17; font.family: Theme.type.family
                    background: null
                }
            }
        }
        Column {
            width: parent.width; spacing: 4
            visible: ep.on && (ep.it.mt || "") !== "" && (ep.it.html || "") !== ""
            Lbl { text: ep.tr("Machine translation"); dim: true; size: 12 }
            Lbl { width: parent.width; text: ep.on ? (ep.it.mt || "") : ""; wrapMode: Text.Wrap; size: 13; dim: true }
        }
        Item {
            width: parent.width; height: 32
            Lbl { anchors.verticalCenter: parent.verticalCenter; text: ep.tr("Translation"); dim: true; size: 12 }
            Row {
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                spacing: 6
                BusyIndicator { visible: ep.trBusy; running: visible; width: 24; height: 24; anchors.verticalCenter: parent.verticalCenter }
                FlatButton {
                    kind: "flat"; glyph: "translate"; implicitHeight: 30
                    focusPolicy: Qt.NoFocus
                    text: ep.trBusy ? ep.tr("Translating…") : ep.tr("Translate")
                    enabled: ep.on && !ep.trBusy && (ep.it.original || "").trim() !== ""
                    ToolTip.visible: hovered
                    ToolTip.text: ep.tr("Translate the original text now")
                    onClicked: ep.translateNow()
                }
            }
        }
        // ---- formatting of the selected words ----
        Flow {
            width: parent.width; spacing: 4
            Tool { text: "B"; what: "bold"; on: ep.fmt.bold === true; font.bold: true }
            Tool { text: "I"; what: "italic"; on: ep.fmt.italic === true; font.italic: true }
            Tool { text: "U"; what: "underline"; on: ep.fmt.underline === true; font.underline: true }
            Tool { text: "S"; what: "strike"; on: ep.fmt.strike === true; font.strikeout: true }
            Tool { text: "⌫"; what: "clear"; ToolTip.visible: hovered; ToolTip.text: ep.tr("Remove the formatting") }
            Item { width: 8; height: 32 }
            Item {
                width: hlRow.implicitWidth; height: 32
                Row {
                    id: hlRow
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    Repeater {
                        model: ["#fff176", "#a5d6a7", "#f8bbd0", "#90caf9", "#ffcc80", ""]
                        delegate: Swatch {
                            required property var modelData
                            col: modelData
                            tip: col === "" ? ep.tr("Remove the highlight") : ep.tr("Highlight the selected words")
                            onPicked: ep.format("highlight", col)
                        }
                    }
                }
            }
        }
        // ---- the editor: the colours of the theme's text fields; highlights drawn behind as on the page ----
        Rectangle {
            width: parent.width
            height: Math.min(220, Math.max(90, ed.implicitHeight + 16))
            radius: Theme.shape.control || 8
            color: Theme.c.view
            border.width: ed.activeFocus ? 2 : 1
            border.color: ed.activeFocus ? Theme.c.accent : Theme.c.border
            ScrollView {
                anchors.fill: parent; anchors.margins: 6
                TextArea {
                    id: ed
                    textFormat: TextEdit.RichText
                    wrapMode: TextArea.Wrap
                    selectByMouse: true
                    persistentSelection: true       // the colour chooser takes the focus for a moment
                    color: Theme.c.text
                    selectionColor: Theme.c.accent
                    selectedTextColor: Theme.c.accentFg
                    placeholderTextColor: Theme.c.textDim
                    font.pixelSize: 16; font.family: ep.fontShown
                    placeholderText: ep.tr("Write the translation")
                    background: null
                    // The highlights of the editor, under its text (pale on a dark theme so the letters stay readable).
                    // A child with a negative z, not the background: inside a ScrollView the background does not scroll.
                    BubbleText {
                        id: edPills
                        anchors.fill: parent
                        z: -1
                        source: ed
                        pill: ep.lookHl !== "plain"
                        highlightOpacity: Theme.scheme === "dark" ? 0.45 : 0.9
                    }
                    onCursorPositionChanged: ep.refreshState()
                    onTextChanged: ep.previewLater()
                    Keys.onPressed: function(e) {
                        if (!(e.modifiers & Qt.ControlModifier)) return
                        const k = {[Qt.Key_B]: "bold", [Qt.Key_I]: "italic", [Qt.Key_U]: "underline"}[e.key]
                        if (k) { ep.format(k, ""); e.accepted = true }
                        else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { ep.save(true); e.accepted = true }
                    }
                }
            }
        }

        // ---- the look of the whole bubble ----
        Lbl { text: ep.tr("Look of the bubble"); dim: true; size: 12; topPadding: 4 }
        // font and size
        Row {
            width: parent.width
            spacing: 6
            FlatButton {
                id: fontBtn
                kind: "plain"
                implicitHeight: 34
                width: parent.width - sizeRow.width - parent.spacing
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered && !fontPop.visible
                ToolTip.text: ep.tr("Font of the bubble")
                contentItem: Item {
                    Text {
                        anchors { left: parent.left; leftMargin: 12; right: fontChev.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
                        text: ep.fontName(ep.lookFont)
                        elide: Text.ElideRight
                        color: Theme.c.text
                        font.family: ep.fontShown
                        font.pixelSize: 15
                    }
                    Icon { id: fontChev; anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } name: "down"; size: 16; color: Theme.c.textDim }
                }
                onClicked: fontPop.openBelow(fontBtn, false)
            }
            Row {
                id: sizeRow
                spacing: 2
                FlatButton {
                    kind: "flat"; implicitWidth: 36; implicitHeight: 34; focusPolicy: Qt.NoFocus
                    text: "A−"
                    enabled: ep.lookSize > 50
                    ToolTip.visible: hovered; ToolTip.text: ep.tr("Smaller text")
                    onClicked: ep.setLook("size", ep.lookSize - 10)
                }
                Lbl { width: 46; height: 34; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; text: ep.lookSize + "%"; size: 13 }
                FlatButton {
                    kind: "flat"; implicitWidth: 36; implicitHeight: 34; focusPolicy: Qt.NoFocus
                    text: "A+"
                    enabled: ep.lookSize < 250
                    ToolTip.visible: hovered; ToolTip.text: ep.tr("Bigger text")
                    onClicked: ep.setLook("size", ep.lookSize + 10)
                }
            }
        }
        // text colour
        // (the label on its own line: eight swatches do not leave room beside it in a narrow panel)
        Item {
            width: parent.width; height: 58
            Lbl { id: colLbl; anchors { left: parent.left; top: parent.top } text: ep.tr("Text colour"); size: 13 }
            Row {
                anchors { left: parent.left; bottom: parent.bottom }
                spacing: 8
                readonly property var cols: ["", "#111111", "#ffffff", "#d32f2f", "#1565c0", "#2e7d32", "#6a1b9a"]
                Repeater {
                    model: parent.cols
                    delegate: Swatch {
                        required property var modelData
                        col: modelData
                        chosen: ep.lookColor === modelData
                        tip: col === "" ? ep.tr("Automatic (dark ink)") : ""
                        onPicked: ep.setLook("color", col)
                    }
                }
                // a colour of one's own, with the app's colour chooser
                Swatch {
                    readonly property bool own: ep.lookColor !== "" && parent.cols.indexOf(ep.lookColor) < 0
                    col: own ? ep.lookColor : ""
                    chosen: own
                    tip: ep.tr("Another colour…")
                    Icon { visible: !parent.own; anchors.centerIn: parent; name: "plus"; size: 14; color: Theme.c.text }
                    onPicked: picker.pick(ep.lookColor !== "" ? ep.lookColor : "#111111")
                }
            }
        }
        // how the highlights are drawn
        Item {
            width: parent.width; height: 36
            Lbl { anchors.verticalCenter: parent.verticalCenter; text: ep.tr("Highlight"); size: 13 }
            SheetSegments {
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                width: Math.min(220, parent.width * 0.6)
                model: [{id: "pill", label: ep.tr("Pill")}, {id: "plain", label: ep.tr("Plain")}]
                current: ep.lookHl === "plain" ? "plain" : "pill"
                onPicked: function(id) { ep.setLook("highlight", id) }
            }
        }
        // how the page will show it: a white bubble with the font, size, colour and highlights chosen
        Lbl { text: ep.tr("Preview"); dim: true; size: 12 }
        Rectangle {
            width: parent.width
            height: Math.min(200, Math.max(56, preview.contentHeight + 28))
            radius: Theme.shape.control || 8
            color: Theme.scheme === "dark" ? "#2a2a2e" : "#d9d9de"
            Rectangle {
                anchors.fill: parent
                anchors.margins: 6
                radius: Math.min(width, height) * 0.32
                color: "#fcffffff"
                clip: true
                BubbleText {
                    id: preview
                    anchors.fill: parent
                    anchors.leftMargin: Math.max(6, parent.width * 0.06)
                    anchors.rightMargin: anchors.leftMargin
                    anchors.topMargin: 6
                    anchors.bottomMargin: 6
                    text: ep.previewHtml
                    rich: true
                    family: ep.fontShown
                    pixelSize: Math.round(16 * ep.lookSize / 100)
                    color: ep.lookColor !== "" ? ep.lookColor : "#111111"
                    pill: ep.lookHl !== "plain"
                }
            }
        }
        Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; size: 11; text: ep.tr("Ctrl+B, Ctrl+I, Ctrl+U format the selection. Ctrl+Enter confirms. In the text edit mode, drag the box to move it and its round handles to resize it.") }
        Row {
            spacing: 8
            FlatButton { kind: "suggested"; glyph: "check"; text: ep.tr("Confirm"); onClicked: ep.save(true) }
            // icons only, so the row never runs out of the panel (the names are tooltips)
            FlatButton { kind: "flat"; glyph: "download"; implicitWidth: 40; ToolTip.visible: hovered; ToolTip.text: ep.tr("Save"); onClicked: ep.save(false) }
            FlatButton { kind: "flat"; glyph: "trash"; implicitWidth: 40; ToolTip.visible: hovered; ToolTip.text: ep.tr("Remove"); onClicked: { ep.rd.removeBubble(ep.it.page, ep.it); ep.rd.activate(null) } }
        }
    }
    }

    // ---- the font list: each name written in its own font ----
    Popover {
        id: fontPop
        contentItem: Column {
            width: Math.max(220, fontBtn.width)
            Repeater {
                model: ep.fontList
                delegate: Rectangle {
                    required property var modelData
                    readonly property bool cur: ep.lookFont === modelData
                    width: parent.width
                    height: 36
                    radius: Theme.shape.small !== undefined ? Theme.shape.small : 6
                    color: fArea.containsMouse ? Theme.c.hover : "transparent"
                    Text {
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 44
                        elide: Text.ElideRight
                        text: ep.fontName(modelData)
                        color: Theme.c.text
                        font.family: modelData === "" ? Theme.type.family : modelData
                        font.pixelSize: 15
                    }
                    Icon { visible: parent.cur; anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter } name: "check"; size: 16 }
                    MouseArea { id: fArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { fontPop.close(); ep.setLook("font", modelData) } }
                }
            }
        }
    }
    ColorPickerAdapter {
        id: picker
        title: ep.tr("Text colour")
        onAccepted: ep.setLook("color", String(selectedColor))
    }
}
