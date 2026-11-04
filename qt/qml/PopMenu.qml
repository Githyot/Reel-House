import QtQuick
import Reelhouse 1.0

// A menu in a libadwaita popover. items: [{label, danger?, checked?}], `picked(index)` when one is chosen.
Popover {
    id: pm
    property var items: []
    property int menuWidth: 250
    signal picked(int index)
    contentItem: Column {
        width: pm.menuWidth
        Repeater {
            model: pm.items
            delegate: Rectangle {
                required property var modelData
                required property int index
                width: pm.menuWidth
                height: 36
                radius: Theme.shape.small
                color: ma.containsMouse ? Theme.c.hover : "transparent"
                Lbl { x: 12; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 44; elide: Text.ElideRight; text: modelData.label; color: modelData.danger ? Theme.c.errorText : Theme.c.text; size: 15 }
                Icon { visible: !!modelData.checked; anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; name: "check"; size: 16 }
                MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { pm.close(); pm.picked(index) } }
            }
        }
    }
}
