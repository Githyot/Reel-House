import QtQuick
import Reelhouse 1.0

// A card of the INFO page (GTK .mal-card): a heading and its content.
Rectangle {
    id: sc
    property string title: ""
    default property alias content: inner.data
    radius: 12
    color: Qt.rgba(0, 0, 0, 0)
    implicitHeight: col.implicitHeight + 24
    Rectangle { anchors.fill: parent; radius: 12; color: Theme.c.window; opacity: 0.6 }
    border.width: 1
    border.color: Theme.c.divider
    Column {
        id: col
        x: 12
        y: 12
        width: sc.width - 24
        spacing: 8
        Text { visible: sc.title !== ""; text: sc.title; color: Theme.c.text; font.pixelSize: 17; font.weight: Font.Bold; font.family: Theme.type.family }
        Column { id: inner; width: parent.width; spacing: 8 }
    }
}
