import QtQuick
import Reelhouse 1.0

// The heading of a section merged into another tab (GTK prefs::page while merging): a line, an icon and a big title.
Column {
    property string title: ""
    property string icon: ""
    width: parent ? parent.width : 600
    spacing: 6
    topPadding: 8
    Rectangle { width: parent.width; height: 1; color: Theme.c.divider }
    Row {
        topPadding: 6
        spacing: 10
        Icon { name: parent.parent.icon; size: 22; anchors.verticalCenter: parent.verticalCenter }
        Text { text: parent.parent.title; color: Theme.c.text; font.pixelSize: 24; font.weight: Font.ExtraBold; font.family: Theme.type.family }
    }
}
