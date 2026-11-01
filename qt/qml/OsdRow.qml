import QtQuick
import Reelhouse 1.0
import "Osd.js" as Osd

// A row of a player menu. `checked` is the option in use; `selected` is the row being looked at (a language whose
// variants show in the next column); `chevron` says the row opens more. How "checked" shows follows the theme:
//   GNOME: an accent check on the right   Windows: the accent pill on the left and a light fill
//   Android: the secondary-container fill  Apple: a white check on the right
Rectangle {
    id: row
    property string text: ""
    property string sub: ""
    property string glyph: ""
    property bool checked: false
    property bool selected: false
    property bool chevron: false
    signal activated()
    readonly property var o: Osd.t(Theme.layout.player)
    readonly property bool filled: checked && o.mark === "fill"
    readonly property color textColor: filled ? o.selFg : o.fg

    width: parent ? parent.width : 220
    height: sub !== "" ? o.rowH + 14 : o.rowH
    radius: Osd.r(o.rowR, height)
    color: area.pressed ? o.press
         : filled ? (area.containsMouse ? o.selHover : o.sel)
         : (area.containsMouse ? o.hover : ((selected || (checked && o.mark === "bar")) ? o.sel : "transparent"))
    Behavior on color { ColorAnimation { duration: Theme.motion.fast || 120 } }

    // Windows: the selection pill
    Rectangle {
        visible: row.o.mark === "bar" && (row.checked || row.selected)
        width: 3
        height: row.checked ? 16 : 10
        radius: 1.5
        color: row.checked ? Theme.c.accent : "#80ffffff"
        anchors { left: parent.left; leftMargin: 1; verticalCenter: parent.verticalCenter }
    }
    Icon {
        id: lead
        visible: row.glyph !== ""
        name: row.glyph
        size: 18
        color: row.textColor
        anchors { left: parent.left; leftMargin: row.o.rowPad; verticalCenter: parent.verticalCenter }
    }
    Column {
        anchors { left: lead.visible ? lead.right : parent.left; leftMargin: lead.visible ? 10 : row.o.rowPad; right: trail.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
        spacing: 1
        Text {
            width: parent.width
            text: row.text
            color: row.textColor
            elide: Text.ElideRight
            font.pixelSize: row.o.rowSize
            font.weight: row.checked && (row.o.mark === "bar" || row.o.mark === "fill") ? 600 : row.o.rowWeight
            font.family: Theme.type.family
        }
        Text {
            visible: row.sub !== ""
            width: parent.width
            text: row.sub
            color: row.filled ? row.o.selFg : row.o.muted
            opacity: row.filled ? 0.8 : 1
            elide: Text.ElideRight
            font.pixelSize: 12
            font.family: Theme.type.family
        }
    }
    Row {
        id: trail
        anchors { right: parent.right; rightMargin: row.o.rowPad - 2; verticalCenter: parent.verticalCenter }
        spacing: 2
        Icon {
            visible: row.checked && row.o.mark !== "bar"
            name: "check"
            size: 18
            color: row.filled ? row.o.selFg : (row.o.markAccent ? Osd.readable(Theme.c.accent) : row.o.fg)
            anchors.verticalCenter: parent.verticalCenter
        }
        Icon { visible: row.chevron; name: "right"; size: 16; color: row.o.muted; anchors.verticalCenter: parent.verticalCenter }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: row.activated()
    }
}
