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
        width: row.width
        Text { id: rowTitle; width: Math.min(implicitWidth, row.width - (addonName.visible ? Math.min(addonName.implicitWidth, row.width * 0.4) + 12 : 0)); elide: Text.ElideRight; text: row.title; color: Theme.c.text; font.pixelSize: Theme.type.rowTitle.size; font.weight: Theme.type.rowTitle.weight || Font.ExtraBold; font.letterSpacing: Theme.type.rowTitle.letterSpacing || 0; font.family: Theme.type.display || Theme.type.family }
        // the addon's name sits on the baseline of the title, as a caption of it
        Text { id: addonName; visible: row.addon !== ""; anchors.baseline: rowTitle.baseline; width: Math.min(implicitWidth, row.width * 0.4); elide: Text.ElideRight; text: row.addon; color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family }
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
