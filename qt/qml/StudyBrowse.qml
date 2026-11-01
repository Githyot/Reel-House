import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Study: the card browser. Search the cards (of one deck or all), click one to edit its note or delete it.
// Wide: the list on the left and the editor on the right; narrow: the list, then the editor on its own.
Item {
    id: br
    property var browse: null
    property var models: []
    property var decks: []
    readonly property real cw: Math.min(1100, width - Theme.spacing.page * 2)
    readonly property real x0: (width - cw) / 2
    readonly property bool wide: cw > 820
    signal back()
    readonly property var rows: browse && browse.rows ? browse.rows : []
    readonly property real did: browse ? (browse.did || 0) : 0
    property int sel: -1
    readonly property var cur: sel >= 0 && sel < rows.length ? rows[sel] : null
    readonly property real curCid: cur ? cur.cid : 0
    readonly property var curModel: { if (!cur) return null; for (var i = 0; i < models.length; ++i) if (models[i].id === cur.mid) return models[i]; return null }
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function deckName(id) { for (var i = 0; i < decks.length; ++i) if (decks[i].id === id) return decks[i].name; return "" }
    function search() { if (!br.visible) return; br.sel = -1; Core.cmd({cmd: "study_browse", did: br.did, q: q.text}) }
    // the fields are filled when another card is picked, not on every refresh (what is typed stays)
    onCurCidChanged: { if (cur) { for (var i = 0; i < ed.count; ++i) { var it = ed.itemAt(i); if (it) it.value = cur.fields[i] !== undefined ? cur.fields[i] : "" } } }
    onVisibleChanged: if (!visible) { sel = -1; q.text = "" }

    StudyHeader {
        id: hdr
        x: br.x0; width: br.cw
        y: Theme.layout.largeTitles === true ? 0 : 12
        title: br.tr("Browse")
        subtitle: (br.did !== 0 ? br.deckName(br.did).split("::").join(" › ") + "  ·  " : "") + br.rows.length + (br.rows.length >= 300 ? "+" : "") + " " + br.tr("cards")
        onBack: br.back()
    }
    // search and deck filter
    Item {
        id: tools
        x: br.x0; width: br.cw; height: 40
        anchors { top: hdr.bottom; topMargin: 8 }
        TextField {
            id: q
            anchors { left: parent.left; right: deckBtn.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
            height: 38
            placeholderText: br.tr("Search the cards")
            placeholderTextColor: Theme.c.textDim
            color: Theme.c.text
            font.pixelSize: 15
            font.family: Theme.type.family
            selectByMouse: true
            leftPadding: 38
            background: Rectangle {
                radius: Theme.layout.buttons === "capsule" ? height / 2 : Theme.shape.control
                color: Theme.c.fill3
                border.width: q.activeFocus ? 2 : (Theme.layout.buttons === "fluent" ? 1 : 0)
                border.color: q.activeFocus ? Theme.c.accent : Theme.c.border
            }
            Icon { name: "search"; size: 16; color: Theme.c.textDim; anchors { left: parent.left; leftMargin: 13; verticalCenter: parent.verticalCenter } }
            onTextChanged: br.search()
        }
        FlatButton {
            id: deckBtn
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            kind: "plain"; glyph: "list"
            text: br.wide ? (br.did === 0 ? br.tr("All decks") : br.deckName(br.did).split("::").pop()) : ""
            implicitWidth: br.wide ? Math.min(240, contentItem.implicitWidth + 36) : 40
            ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: br.tr("Deck")
            onClicked: deckMenu.openBelow(deckBtn, true)
        }
    }
    // the macaw is always on the Study pages: with its magnifying glass, looking through the cards
    Mascot {
        id: buddy
        z: 3
        // beside the column when there is room, small in the corner otherwise
        size: br.x0 >= 120 ? 96 : 52
        x: br.width - size - 14
        y: br.height - size - 14
        mood: "peek"
        animate: br.visible
    }

    PopMenu {
        id: deckMenu
        menuWidth: 280
        items: [{label: br.tr("All decks"), checked: br.did === 0}].concat(br.decks.map(function(d) { return {label: d.name.split("::").join(" › "), checked: br.did === d.id} }))
        onPicked: function(i) { br.sel = -1; Core.cmd({cmd: "study_browse", did: i === 0 ? 0 : br.decks[i - 1].id, q: q.text}) }
    }

    StudySurface {
        id: listBox
        x: br.x0
        anchors { top: tools.bottom; topMargin: 12; bottom: parent.bottom; bottomMargin: 16 }
        width: br.wide ? 380 : br.cw
        visible: br.wide || br.cur === null
        ListView {
            id: lv
            anchors.fill: parent
            anchors.margins: 4
            model: br.rows.length
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            delegate: Item {
                id: rowItem
                required property int index
                readonly property var r: br.rows[index] || ({})
                width: ListView.view.width; height: 58
                Rectangle {
                    anchors { fill: parent; margins: 2 }
                    radius: Theme.shape.control
                    color: br.sel === rowItem.index ? Theme.c.selected : (ma.containsMouse ? Theme.c.hover : "transparent")
                }
                Rectangle {
                    x: 10; anchors.verticalCenter: parent.verticalCenter
                    width: 6; height: 6; radius: 3
                    color: rowItem.r.kind === "new" ? (Theme.scheme === "light" ? "#1c71d8" : "#78aeed") : (rowItem.r.kind === "learn" ? Theme.c.error : Theme.c.success)
                }
                Column {
                    anchors { left: parent.left; leftMargin: 24; right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                    spacing: 2
                    Text { width: parent.width; elide: Text.ElideRight; text: rowItem.r.front || ""; color: Theme.c.text; font.pixelSize: 14; font.weight: Font.Medium; font.family: Theme.type.family }
                    Text {
                        width: parent.width; elide: Text.ElideRight
                        text: (rowItem.r.back ? rowItem.r.back + "  ·  " : "") + (rowItem.r.deck || "").split("::").pop() + (rowItem.r.due ? "  ·  " + rowItem.r.due : "") + (rowItem.r.kind === "new" ? "  ·  " + br.tr("New") : "")
                        color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
                    }
                }
                MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: br.sel = rowItem.index }
            }
        }
        Column {
            visible: br.rows.length === 0
            anchors.centerIn: parent
            spacing: 8
            Icon { anchors.horizontalCenter: parent.horizontalCenter; name: "search"; size: 32; color: Theme.c.textDim; opacity: 0.6 }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: br.tr("Nothing found."); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
        }
    }

    // nothing chosen yet: say what to do
    Column {
        visible: br.cur === null && br.wide
        x: listBox.x + listBox.width + 16
        width: br.x0 + br.cw - x
        anchors.verticalCenter: listBox.verticalCenter
        spacing: 10
        Icon { anchors.horizontalCenter: parent.horizontalCenter; name: "edit"; size: 34; color: Theme.c.textDim; opacity: 0.6 }
        Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: br.tr("Pick a card to edit it"); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
    }

    // the editor of the chosen note
    StudySurface {
        id: edBox
        visible: br.cur !== null
        x: br.wide ? listBox.x + listBox.width + 16 : br.x0
        width: br.x0 + br.cw - x
        anchors { top: tools.bottom; topMargin: 12; bottom: parent.bottom; bottomMargin: 16 }
        Flickable {
            anchors.fill: parent
            anchors.margins: 20
            contentWidth: width
            contentHeight: ecol.height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Column {
                id: ecol
                width: parent.width
                spacing: 14
                Item {
                    width: parent.width; height: 36
                    FlatButton { id: toList; visible: !br.wide; anchors.verticalCenter: parent.verticalCenter; kind: "flat"; glyph: "left"; text: br.tr("Cards"); implicitHeight: 34; onClicked: br.sel = -1 }
                    Text {
                        anchors { left: toList.visible ? toList.right : parent.left; leftMargin: toList.visible ? 8 : 0; right: parent.right; verticalCenter: parent.verticalCenter }
                        elide: Text.ElideRight
                        text: (br.curModel ? br.curModel.name : "") + (br.cur ? "  ·  " + br.cur.deck.split("::").join(" › ") : "")
                        color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family
                    }
                }
                Repeater {
                    id: ed
                    model: br.curModel ? br.curModel.fields : []
                    delegate: Column {
                        required property var modelData
                        required property int index
                        property alias value: ta.text
                        width: ecol.width
                        spacing: 6
                        Text { text: modelData; color: Theme.c.textDim; font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family }
                        TextArea {
                            id: ta
                            width: parent.width
                            height: Math.max(index === 0 ? 96 : 72, Math.min(220, implicitHeight))
                            text: br.cur && br.cur.fields[index] !== undefined ? br.cur.fields[index] : ""
                            wrapMode: TextArea.Wrap
                            selectByMouse: true
                            color: Theme.c.text
                            font.pixelSize: 15
                            font.family: Theme.type.family
                            leftPadding: 12; rightPadding: 12; topPadding: 10; bottomPadding: 10
                            background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: ta.activeFocus ? 2 : 1; border.color: ta.activeFocus ? Theme.c.accent : Theme.c.border }
                        }
                    }
                }
                Item {
                    width: parent.width; height: 44
                    FlatButton {
                        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                        kind: "destructive"; glyph: "trash"; text: br.tr("Delete")
                        onClicked: { var n = br.cur.nid; br.sel = -1; Core.cmd({cmd: "study_delete_note", nid: n}) }
                    }
                    FlatButton {
                        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                        kind: "suggested"; pill: true; glyph: "check"; text: br.tr("Save")
                        implicitWidth: 130
                        onClicked: {
                            var vals = []
                            for (var i = 0; i < ed.count; ++i) vals.push(ed.itemAt(i).value)
                            Core.cmd({cmd: "study_edit_note", nid: br.cur.nid, fields: vals})
                        }
                    }
                }
            }
        }
    }
}
