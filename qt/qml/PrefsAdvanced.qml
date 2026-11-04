import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The "Advanced" switch at the foot of the general options of a Preferences tab. One switch for every tab
// (general.prefs_advanced): open, the finer settings of each tab show below it; closed, each tab shows only what
// most people change. Put the advanced groups in a `Column { visible: dlg.adv }` after it.
Item {
    id: pa
    property var dlg
    readonly property bool on: dlg ? dlg.adv : false
    width: parent ? parent.width : 600
    implicitHeight: 64
    // a controller (A = Space) and the keyboard reach it like a button
    activeFocusOnTab: true
    Keys.onSpacePressed: if (dlg) dlg.toggleAdv()
    Keys.onReturnPressed: if (dlg) dlg.toggleAdv()
    Rectangle { anchors.top: parent.top; width: parent.width; height: 1; color: Theme.c.divider }
    Rectangle {
        anchors { fill: parent; topMargin: 10 }
        radius: Theme.shape.control
        color: area.containsMouse || pa.activeFocus ? Theme.c.hover : "transparent"
        border.width: pa.activeFocus ? 2 : 0
        border.color: Theme.c.accent
        Row {
            x: 12
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 24
            spacing: 10
            Icon { name: "wrench"; size: 18; color: Theme.c.textDim; anchors.verticalCenter: parent.verticalCenter }
            Column {
                width: parent.width - 18 - 10 - 18 - 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text { text: Core.tr(Core.uiLang, "Advanced"); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
                Text {
                    width: parent.width
                    elide: Text.ElideRight
                    text: pa.on ? Core.tr(Core.uiLang, "Finer settings of every tab are shown. Click to hide them.") : Core.tr(Core.uiLang, "Finer settings, for those who want to go deeper.")
                    color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family
                }
            }
            Icon { name: "right"; size: 18; color: Theme.c.textDim; rotation: pa.on ? 90 : 0; anchors.verticalCenter: parent.verticalCenter; Behavior on rotation { NumberAnimation { duration: 120 } } }
        }
        MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: if (pa.dlg) pa.dlg.toggleAdv() }
    }
    Accessible.role: Accessible.Button
    Accessible.name: Core.tr(Core.uiLang, "Advanced")
}
