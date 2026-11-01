import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// libadwaita-like popover menu. It is drawn inside the window, so it has proper rounded corners.
Menu {
    id: m
    padding: 6
    background: Glass { implicitWidth: 200; radius: Theme.shape.popover !== undefined ? Theme.shape.popover : 12; level: "regular"; shadow: true }
    delegate: AppMenuItem {
        id: mi
        implicitHeight: 36
        implicitWidth: 200
        contentItem: Text {
            text: mi.text
            color: mi.enabled ? Theme.c.text : Theme.c.textDim
            font.pixelSize: 15
            font.family: Theme.type.family
            verticalAlignment: Text.AlignVCenter
            leftPadding: 6
        }
        indicator: Item {}
        background: Rectangle {
            radius: 8
            color: mi.highlighted ? Theme.c.hover : "transparent"
        }
    }
}
