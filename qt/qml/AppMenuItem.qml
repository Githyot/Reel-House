import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// An entry of an AppMenu, drawn like a libadwaita popover menu row.
MenuItem {
    id: mi
    implicitHeight: 36
    implicitWidth: Math.max(200, t.implicitWidth + 30)
    contentItem: Text {
        id: t
        text: mi.text
        color: mi.enabled ? Theme.c.text : Theme.c.textDim
        font.pixelSize: 15
        font.family: Theme.type.family
        verticalAlignment: Text.AlignVCenter
        leftPadding: 6
    }
    indicator: Item {}
    arrow: Item {}
    background: Rectangle {
        radius: 8
        color: mi.highlighted ? Theme.c.hover : "transparent"
    }
}
