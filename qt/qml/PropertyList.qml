import QtQuick
import Reelhouse 1.0

// A boxed list of "title / value" rows (GTK boxed-list with .property rows): the title small and dim, the value below.
Rectangle {
    id: pl
    property var rows: []        // [[title, value], ...]; empty values are left out
    readonly property var shown: rows.filter(function(r) { return r[1] !== undefined && r[1] !== null && ("" + r[1]) !== "" })
    radius: Theme.shape.group
    color: Theme.c.card
    border.width: 1
    border.color: Theme.c.divider
    implicitHeight: col.implicitHeight
    visible: shown.length > 0
    clip: true
    Column {
        id: col
        width: parent.width
        Repeater {
            model: pl.shown
            delegate: Item {
                required property var modelData
                required property int index
                width: col.width
                height: vt.implicitHeight + kt.implicitHeight + 22
                Rectangle { visible: index > 0; width: parent.width; height: 1; color: Theme.c.divider }
                Text { id: kt; x: 14; y: 9; text: Core.tr(Core.uiLang, "" + modelData[0]); color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
                Text { id: vt; x: 14; anchors.top: kt.bottom; anchors.topMargin: 2; width: parent.width - 28; text: "" + modelData[1]; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family; wrapMode: Text.Wrap }
            }
        }
    }
}
