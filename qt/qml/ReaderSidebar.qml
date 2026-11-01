import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The part of the left sidebar that belongs to the Text reader: the way back to the book, the book and the chapter in
// small type, and the reader's own buttons (edit, translate, Aa, previous / next chapter), which NovelReaderPage puts
// into `slot`. The page itself then has no bar on top: all of its height is for the text.
Column {
    id: rs
    readonly property alias slot: slotBox
    readonly property bool narrow: width < 150
    readonly property bool images: (Core.manga.mode || "video") === "images"
    readonly property var r: (images ? Core.manga.reader : (Core.manga.novels || ({})).reader) || ({})
    function tr(s) { return Core.tr(Core.uiLang, s) }
    readonly property string chapterLine: images ? (r.chapter ? (r.chapter.number ? tr("Chapter") + " " + r.chapter.number : "") + (r.chapter.title ? (r.chapter.number ? " · " : "") + r.chapter.title : "") : "")
        : (r.chapter ? tr("Chapter") + " " + r.chapter : "") + (r.chapterTitle ? (r.chapter ? " · " : "") + r.chapterTitle : "")
    spacing: 6

    Rectangle { visible: !rs.narrow; width: parent.width; height: 1; color: Theme.c.divider; opacity: 0.8 }
    FlatButton {
        width: parent.width
        kind: "flat"; glyph: "left"; iconSize: 18
        alignStart: !rs.narrow
        text: rs.narrow ? "" : rs.tr(rs.images ? "Back to the series" : "Back to the book")
        implicitHeight: 36
        ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: rs.tr("Back (Esc)")
        onClicked: Core.cmd({cmd: rs.images ? "manga_close_reader" : "novel_close_reader"})
    }
    Column {
        visible: !rs.narrow
        x: 12
        width: parent.width - 24
        spacing: 2
        Text {
            width: parent.width
            text: rs.r.title || ""
            color: Theme.c.text
            wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
            font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family
        }
        Text {
            visible: text !== ""
            width: parent.width
            text: rs.chapterLine
            color: Theme.c.textDim
            wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
            font.pixelSize: 12; font.family: Theme.type.family
        }
    }
    // ---- the reader's buttons, one per line with its name (icons only in a narrow rail) ----
    // Drawn here, not moved in from the reader: the reader that is open sets the window's activeReader and the
    // buttons call it (side* functions), so they are always under the chapter's name while a chapter is open.
    readonly property var rd: { const w = rs.Window.window; return w && w.activeReader ? w.activeReader : null }
    readonly property bool manga: rd !== null && rd.readerKind === "manga"
    // short names (HP: Upscaling, Editor, Translation, Study card); what each does is in the tooltip
    component SideBtn: FlatButton {
        property string label: ""
        property string tip: ""
        width: rs.width
        implicitHeight: 36
        // the arrow keys must keep turning the pages; with a controller the tools take the focus (↑/↓ move between
        // them, ←/→ still turn the pages, Main.qml) so every one of them can be reached
        readonly property bool padSideTool: true
        readonly property bool padOn: { const w = rs.Window.window; return !!(w && w.padMode) }
        focusPolicy: padOn ? Qt.StrongFocus : Qt.NoFocus
        activeFocusOnTab: padOn
        alignStart: !rs.narrow
        iconSize: 18
        text: rs.narrow ? "" : label
        ToolTip.visible: hovered && (rs.narrow || tip !== "")
        ToolTip.delay: rs.narrow ? 400 : 700
        ToolTip.text: tip !== "" ? tip : label
    }
    Column {
        id: btnBox
        visible: rs.rd !== null
        width: parent.width
        spacing: 2
        SideBtn {
            id: upB
            visible: rs.manga
            kind: rs.manga && rs.rd.upOn ? "suggested" : "flat"
            glyph: "upscale"
            label: "Upscaling"
            tip: rs.tr("Upscale the pages with AI")
            onClicked: rs.rd.sideUpscale(upB)
        }
        SideBtn {
            id: editB
            kind: rs.rd && (rs.manga ? rs.rd.editMode !== "" && rs.rd.editMode !== "card" : rs.rd.editing) ? "suggested" : "flat"
            glyph: "edit"
            label: rs.tr("Editor")
            tip: rs.manga ? rs.tr("Edit the text boxes and the pictures") : rs.tr("Edit the text of this chapter")
            onClicked: rs.rd.sideEdit(editB)
        }
        SideBtn {
            id: trB
            kind: rs.rd && (rs.manga ? (rs.rd.trOn || rs.rd.ocrOn) : rs.rd.trTarget !== "") ? "suggested" : "flat"
            glyph: "translate"
            label: rs.tr("Translation")
            tip: rs.manga ? rs.tr("Text of the pages and translation") : rs.tr("Translate the chapter")
            onClicked: rs.rd.sideTranslate(trB)
        }
        SideBtn {
            id: lookB
            kind: "flat"
            label: rs.tr("Themes & Settings")
            // "Aa" in place of an icon
            contentItem: Item {
                implicitWidth: lookRow.implicitWidth
                implicitHeight: 36
                Row {
                    id: lookRow
                    spacing: 10
                    x: rs.narrow ? (parent.width - width) / 2 : 12
                    anchors.verticalCenter: parent.verticalCenter
                    Text { text: "Aa"; color: Theme.c.text; font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                    Text {
                        visible: !rs.narrow
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, lookB.width - 56); elide: Text.ElideRight
                        text: lookB.label
                        color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family
                    }
                }
            }
            onClicked: rs.rd.sideLook(lookB)
        }
        SideBtn {
            visible: rs.manga
            kind: rs.manga && rs.rd.bubblesLocked ? "suggested" : "flat"
            glyph: "lock"
            label: rs.tr("Lock text boxes")
            tip: rs.manga && rs.rd.bubblesLocked ? rs.tr("Text boxes locked: a tap goes to the page") : rs.tr("Lock the text boxes (a tap goes to the page)")
            onClicked: rs.rd.sideBubbleLock()
        }
        SideBtn {
            visible: rs.manga
            kind: rs.manga && rs.rd.editMode === "card" ? "suggested" : "flat"
            glyph: "study"
            label: rs.tr("Study card")
            tip: rs.tr("Study card from a part of the page")
            onClicked: rs.rd.sideCard()
        }
        // previous / next chapter side by side in one pill, in the look of the theme (like Layout in Aa)
        SheetSegments {
            visible: !rs.manga
            width: parent.width
            model: rs.narrow
                ? [{id: "prev", label: "", glyph: "prev", enabled: !!rs.r.hasPrev, tip: rs.tr("Previous chapter (Left)")},
                   {id: "next", label: "", glyph: "next", enabled: !!rs.r.hasNext, tip: rs.tr("Next chapter (Right)")}]
                : [{id: "prev", label: rs.tr("Previous"), glyph: "prev", enabled: !!rs.r.hasPrev, tip: rs.tr("Previous chapter (Left)")},
                   {id: "next", label: rs.tr("Next"), glyph: "next", enabled: !!rs.r.hasNext, tip: rs.tr("Next chapter (Right)")}]
            onPicked: function(id) { Core.cmd({cmd: "novel_next", delta: id === "prev" ? -1 : 1}) }
        }
    }
    // kept for older callers (nothing is put here any more)
    Column {
        id: slotBox
        width: parent.width
        spacing: 2
    }
}
