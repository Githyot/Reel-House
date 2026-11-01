import QtQuick
import Reelhouse 1.0

// A row that picks one of several values (adw::ComboRow): the chosen value and an arrow; a click opens the list.
PrefRow {
    id: cb
    property var options: []        // labels
    property int current: 0
    signal picked(int index)
    MouseArea { parent: cb; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: pop.openBelow(arrow, true) }
    Text { text: cb.options[cb.current] !== undefined ? cb.options[cb.current] : ""; color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter; elide: Text.ElideRight; width: Math.min(implicitWidth, cb.width * 0.5) }
    Icon { id: arrow; name: "down"; size: 16; color: Theme.c.textDim; anchors.verticalCenter: parent.verticalCenter }
    Popover {
        id: pop
        contentItem: Column {
            width: Math.max(220, implicitWidth)
            Repeater {
                model: cb.options
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    width: Math.max(220, lbl.implicitWidth + 60)
                    height: 34
                    radius: Theme.shape.small
                    color: optArea.containsMouse ? Theme.c.hover : "transparent"
                    Text { id: lbl; x: 10; anchors.verticalCenter: parent.verticalCenter; text: modelData; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
                    Icon { visible: index === cb.current; name: "check"; size: 16; anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } }
                    MouseArea { id: optArea; anchors.fill: parent; hoverEnabled: true; onClicked: { pop.close(); cb.picked(index) } }
                }
            }
        }
    }
}
