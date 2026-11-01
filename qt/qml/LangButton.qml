import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The button that chooses which languages the lists of sources show. Nothing ticked means every language.
Item {
    id: lb
    property var options: []            // [{code, name}]
    property var selected: []           // codes
    signal changed(var list)
    implicitWidth: row.implicitWidth + 28
    implicitHeight: 34
    readonly property string caption: {
        if (selected.length === 0) return Core.tr(Core.uiLang, "All languages")
        if (selected.length === 1) {
            for (let i = 0; i < options.length; ++i) if (options[i].code === selected[0]) return options[i].name
            return selected[0]
        }
        return selected.length + " " + Core.tr(Core.uiLang, "languages")
    }
    Rectangle {
        anchors.fill: parent
        radius: Theme.shape.medium
        color: ma.containsMouse || pop.visible ? Theme.c.buttonHover : Theme.c.button
    }
    Row {
        id: row
        anchors.centerIn: parent
        spacing: 8
        Icon { name: "translate"; size: 16; anchors.verticalCenter: parent.verticalCenter }
        Lbl { text: lb.caption; bold: true; anchors.verticalCenter: parent.verticalCenter; elide: Text.ElideRight; width: Math.min(implicitWidth, 170) }
        Icon { name: "down"; size: 16; anchors.verticalCenter: parent.verticalCenter }
    }
    MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: pop.openBelow(lb) }
    Popover {
        id: pop
        onOpened: q.text = ""
        contentItem: Item {
            implicitWidth: 300
            implicitHeight: inner.implicitHeight
            Column {
                id: inner
            width: 300
            spacing: 8
            Item {
                width: parent.width
                height: 32
                Lbl { x: 4; anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Languages"); bold: true; size: 16 }
                FlatButton { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; kind: "flat"; implicitHeight: 30; text: Core.tr(Core.uiLang, "All"); onClicked: lb.changed([]) }
            }
            SearchEntry { id: q; width: parent.width; placeholderText: Core.tr(Core.uiLang, "Search languages") }
            ListView {
                width: parent.width
                height: Math.min(340, contentHeight)
                clip: true
                model: lb.options.filter(function(o) { return q.text === "" || (o.name + " " + o.code).toLowerCase().indexOf(q.text.toLowerCase()) >= 0 })
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    required property var modelData
                    readonly property bool on: lb.selected.indexOf(modelData.code) >= 0
                    width: ListView.view.width
                    height: 36
                    radius: Theme.shape.small
                    color: ra.containsMouse ? Theme.c.hover : "transparent"
                    Rectangle {
                        id: box
                        x: 8; anchors.verticalCenter: parent.verticalCenter
                        width: 20; height: 20; radius: Math.min(5, Theme.shape.small)
                        color: on ? Theme.c.accent : "transparent"
                        border.width: on ? 0 : 2
                        border.color: Theme.c.textDim
                        Icon { visible: on; anchors.centerIn: parent; name: "check"; size: 14; color: Theme.c.accentFg }
                    }
                    Lbl { anchors.left: box.right; anchors.leftMargin: 10; anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; elide: Text.ElideRight; size: 15 }
                    MouseArea {
                        id: ra
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            const cur = lb.selected.filter(function(c) { return c !== modelData.code })
                            if (!on) cur.push(modelData.code)
                            lb.changed(cur)
                        }
                    }
                }
            }
        }
        }
    }
}
