import QtQuick
import Reelhouse 1.0

// A group of a preferences page (adw::PreferencesGroup): a heading, a description and a boxed list of rows.
Column {
    id: pg
    property string title: ""
    property string description: ""
    default property alias rows: list.data
    property alias suffix: head.data
    // Collapsible groups show only their heading until opened (keeps long pages short).
    property bool collapsible: false
    property bool expanded: !collapsible
    spacing: 8
    readonly property bool inset: Theme.layout.listRow === "inset-grouped"
    width: parent ? parent.width : 600
    function relink() {
        var first = true
        for (var i = 0; i < list.children.length; ++i) {
            var c = list.children[i]
            if (c.first !== undefined) { c.first = first; if (c.visible) first = false }
        }
    }
    Item {
        id: head
        width: parent.width
        height: visible ? Math.max(t.implicitHeight, head.children.length > 3 ? 34 : 0, 1) : 0
        visible: pg.title !== ""
        // Apple (inset grouped list): a small secondary heading above the group
        Icon {
            id: chev
            visible: pg.collapsible
            x: pg.inset ? 10 : 0
            anchors.verticalCenter: t.verticalCenter
            name: "right"; size: 16; color: Theme.c.textDim
            rotation: pg.expanded ? 90 : 0
            Behavior on rotation { NumberAnimation { duration: 120 } }
        }
        MouseArea {
            visible: pg.collapsible
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: pg.expanded = !pg.expanded
        }
        Text {
            id: t
            x: (pg.inset ? 14 : 0) + (pg.collapsible ? 22 : 0)
            text: pg.title
            color: pg.inset ? Theme.c.textDim : Theme.c.text
            font.pixelSize: pg.inset ? 13 : 15
            font.weight: pg.inset ? Font.DemiBold : Font.Bold
            font.family: Theme.type.family
        }
    }
    Text { visible: pg.expanded && pg.description !== ""; width: parent.width; wrapMode: Text.Wrap; text: pg.description; color: Theme.c.textDim; font.pixelSize: Theme.mobile ? 14 : 13; font.family: Theme.type.family }
    Rectangle {
        width: parent.width
        height: list.implicitHeight
        radius: Theme.shape.group
        color: Theme.c.card
        border.width: pg.inset ? 0 : 1
        border.color: Theme.c.divider
        visible: pg.expanded && list.implicitHeight > 0
        clip: true
        Column {
            id: list
            width: parent.width
            onChildrenChanged: pg.relink()
        }
    }
    Component.onCompleted: relink()
}
