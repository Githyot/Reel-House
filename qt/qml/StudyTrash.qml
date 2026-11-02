import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The trash of Study: the cards (notes) and decks deleted in the last 30 days, newest first. Each one can come back
// ("Restore") or go for good; "Empty trash" forgets them all. Text buttons only (no Shape icons, which do not draw
// on every machine), so it works the same with a mouse and on a touch screen.
Popup {
    id: tp
    function tr(s) { return Core.tr(Core.uiLang, s) }
    readonly property var items: Core.study.trash !== undefined && Core.study.trash !== null ? Core.study.trash : []
    function when(at) {
        const d = new Date(at * 1000)
        const mins = Math.max(0, Math.round((Date.now() - d.getTime()) / 60000))
        if (mins < 1) return tr("just now")
        if (mins < 60) return mins + " min"
        if (mins < 24 * 60) return Math.round(mins / 60) + " h"
        return d.toLocaleDateString(Qt.locale(), Locale.ShortFormat)
    }
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: 20
    width: Math.min(560, (Overlay.overlay ? Overlay.overlay.width : 600) - 32)
    height: Math.min(620, (Overlay.overlay ? Overlay.overlay.height : 700) - 48)
    Overlay.modal: Rectangle { color: Theme.c.scrim }
    background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
    Item {
        anchors.fill: parent
        Column {
            id: head
            width: parent.width
            spacing: 6
            Lbl { width: parent.width; wrapMode: Text.Wrap; bold: true; size: 18; text: tp.tr("Trash") }
            Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; size: 13; text: tp.tr("Deleted cards and decks stay here for 30 days. Restore brings them back with what you learned about them.") }
        }
        ListView {
            id: list
            anchors { left: parent.left; right: parent.right; top: head.bottom; topMargin: 14; bottom: foot.top; bottomMargin: 12 }
            clip: true
            spacing: 6
            model: tp.items.length
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: Rectangle {
                id: row
                required property int index
                readonly property var e: tp.items[index]
                width: list.width
                height: Math.max(56, txt.implicitHeight + 20)
                radius: Theme.shape.control
                color: Theme.c.card !== undefined ? Theme.c.card : Theme.c.fill3
                border.width: 1
                border.color: Theme.c.border
                Column {
                    id: txt
                    anchors { left: parent.left; leftMargin: 12; right: btns.left; rightMargin: 10; verticalCenter: parent.verticalCenter }
                    spacing: 2
                    Text {
                        width: parent.width; elide: Text.ElideRight
                        text: (row.e.kind === "deck" ? tp.tr("Deck") + ": " : "") + (row.e.label !== "" ? row.e.label : tp.tr("(empty card)"))
                        color: Theme.c.text; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: Theme.type.family
                    }
                    Text {
                        width: parent.width; elide: Text.ElideRight
                        text: (row.e.kind === "deck" ? row.e.cards + " " + tp.tr("cards") : row.e.deck) + "  ·  " + tp.when(row.e.at)
                        color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
                    }
                }
                Row {
                    id: btns
                    anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                    spacing: 6
                    FlatButton { kind: "suggested"; pill: true; text: tp.tr("Restore"); implicitHeight: 34; onClicked: Core.cmd({cmd: "study_trash_restore", i: row.index}) }
                    FlatButton { kind: "dangertext"; text: tp.tr("Delete for good"); implicitHeight: 34; onClicked: Core.cmd({cmd: "study_trash_forget", i: row.index}) }
                }
            }
        }
        Lbl {
            visible: tp.items.length === 0
            anchors.centerIn: list
            dim: true
            text: tp.tr("The trash is empty.")
        }
        Row {
            id: foot
            anchors { right: parent.right; bottom: parent.bottom }
            spacing: 8
            FlatButton { kind: "destructive"; visible: tp.items.length > 0; text: tp.tr("Empty trash"); onClicked: Core.cmd({cmd: "study_trash_empty"}) }
            FlatButton { kind: "plain"; text: tp.tr("Close"); onClicked: tp.close() }
        }
    }
}
