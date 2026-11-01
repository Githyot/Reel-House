import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

Column {
    id: row
    property string title: ""
    property string addon: ""
    property var items: []
    // 14 between the title and the covers; the list starts focusRoom (16) higher to leave room for the focus ring
    spacing: -2
    Row {
        spacing: 12
        Text { text: row.title; color: Theme.c.text; font.pixelSize: Theme.type.rowTitle.size; font.weight: Theme.type.rowTitle.weight || Font.ExtraBold; font.letterSpacing: Theme.type.rowTitle.letterSpacing || 0; font.family: Theme.type.display || Theme.type.family }
        Text { visible: row.addon !== ""; text: row.addon; color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family; topPadding: 8 }
    }
    HScrollList {
        id: list
        // room for the focus ring around the cards (HScrollList.focusRoom)
        x: -focusRoom
        width: row.width + 2 * focusRoom
        height: Theme.spacing.posterH + 46 + barRoom + focusRoom
        model: row.items.length
        delegate: PosterCard {
            required property int index
            readonly property var modelData: row.items[index] || ({})
            item: modelData
            onClicked: Core.cmd({cmd: "open_detail", item: modelData})
        }
    }
}
