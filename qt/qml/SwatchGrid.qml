import QtQuick
import Reelhouse 1.0

// A grid of colours to click.
Flow {
    id: sg
    property var colors: []
    property real cell: 28
    property real corner: 6
    property string selected: ""
    signal picked(string hex)
    spacing: 6
    Repeater {
        model: sg.colors
        delegate: Rectangle {
            required property string modelData
            width: sg.cell; height: sg.cell
            radius: sg.corner >= 99 ? width / 2 : sg.corner
            color: modelData
            border.width: modelData.toLowerCase() === sg.selected.toLowerCase() ? 3 : 1
            border.color: modelData.toLowerCase() === sg.selected.toLowerCase() ? Theme.c.accent : Theme.c.border
            Rectangle { visible: parent.border.width === 3; anchors.fill: parent; anchors.margins: 3; radius: parent.radius > 3 ? parent.radius - 3 : 0; color: "transparent"; border.width: 2; border.color: Theme.c.window }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: sg.picked(modelData) }
        }
    }
}
