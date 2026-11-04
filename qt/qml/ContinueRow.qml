import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

Column {
    id: row
    property var items: []
    // 14 between the title and the covers; the list starts focusRoom (16) higher to leave room for the focus ring
    spacing: -2
    Text { text: Core.tr(Core.uiLang, "Continue watching"); color: Theme.c.text; font.pixelSize: Theme.type.rowTitle.size; font.weight: Theme.type.rowTitle.weight || Font.ExtraBold; font.letterSpacing: Theme.type.rowTitle.letterSpacing || 0; font.family: Theme.type.display || Theme.type.family }
    HScrollList {
        // room for the focus ring around the cards (HScrollList.focusRoom)
        x: -focusRoom
        width: row.width + 2 * focusRoom
        height: Theme.spacing.contH + 46 + barRoom + focusRoom
        model: row.items
        delegate: Item {
            id: cw
            // controller / keyboard: the card takes the focus (not its ⋮ button); ✕/A or Enter plays, △/Y opens the ⋮ menu
            activeFocusOnTab: true
            Keys.onReturnPressed: Core.cmd({cmd: "resume", index: index})
            Keys.onEnterPressed: Core.cmd({cmd: "resume", index: index})
            Keys.onSpacePressed: Core.cmd({cmd: "resume", index: index})
            function padMenu() { menu.popup(moreBtn, 0, moreBtn.height) }
            scale: activeFocus ? 1.06 : 1
            z: activeFocus ? 1 : 0
            Behavior on scale { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
            width: Theme.spacing.contW
            height: Theme.spacing.contH + 46
            RoundedPicture {
                id: pic
                width: cw.width
                height: Theme.spacing.contH
                url: modelData.poster ? modelData.poster : ""
                blur: area.containsMouse ? 0.3 : 0
                Behavior on blur { NumberAnimation { duration: Theme.motion.fast } }
            }
            // a soft light of the accent colour behind the play button, only while the mouse is over the card
            Item {
                id: glow
                anchors.centerIn: pic
                width: 58; height: 58
                opacity: area.containsMouse ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.motion.normal } }
                Repeater {
                    model: 6
                    Rectangle {
                        required property int index
                        anchors.centerIn: parent
                        width: 58 + (5 - index) * 22
                        height: width
                        radius: width / 2
                        color: Theme.c.accent
                        opacity: 0.07
                    }
                }
            }
            // dark round play mark and the progress bar, as in the GTK version
            Rectangle {
                anchors.centerIn: pic
                width: 58
                height: 58
                radius: 29
                color: area.containsMouse ? Qt.alpha(Theme.c.accent, 0.85) : "#8c000000"
                scale: area.containsMouse ? 1.18 : 1
                Behavior on scale { NumberAnimation { duration: Theme.motion.fast; easing.type: Easing.OutCubic } }
                Behavior on color { ColorAnimation { duration: Theme.motion.fast } }
                Icon { anchors.centerIn: parent; name: "play"; size: 32; color: area.containsMouse ? Theme.c.accentFg : "#ffffff" }
            }
            Rectangle {
                anchors { left: pic.left; right: pic.right; bottom: pic.bottom }
                height: 5
                color: "#4dffffff"
                Rectangle { width: parent.width * (modelData.fraction || 0); height: parent.height; color: Theme.c.accent }
            }
            Rectangle {
                width: cw.width
                height: Theme.spacing.contH
                radius: Theme.shape.poster
                color: "transparent"
                border.width: cw.activeFocus ? 4 : (area.containsMouse ? 3 : 0)
                border.color: Theme.c.accent
            }
            MouseArea {
                id: area
                width: cw.width
                height: Theme.spacing.contH
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: Core.cmd({cmd: "resume", index: index})
            }
            Text {
                anchors { left: parent.left; right: moreBtn.left; top: pic.bottom; topMargin: 10 }
                text: (modelData.label ? modelData.label + " · " : "") + modelData.name
                color: Theme.c.text
                font.pixelSize: 14
                font.family: Theme.type.family
                elide: Text.ElideRight
            }
            FlatButton {
                id: moreBtn
                kind: "flat"
                glyph: "more"
                focusPolicy: Qt.NoFocus
                implicitWidth: 28
                implicitHeight: 28
                iconSize: 18
                anchors { right: parent.right; top: pic.bottom; topMargin: 4 }
                onClicked: menu.popup(0, height)
                AppMenu {
                    id: menu
                    AppMenuItem { text: Core.tr(Core.uiLang, "Play"); onTriggered: Core.cmd({cmd: "resume", index: index}) }
                    AppMenuItem { text: Core.tr(Core.uiLang, "Details"); onTriggered: Core.cmd({cmd: "open_detail", item: modelData}) }
                    AppMenuItem { text: modelData.favorite ? Core.tr(Core.uiLang, "Remove from favorites") : Core.tr(Core.uiLang, "Add to favorites"); onTriggered: Core.cmd({cmd: "toggle_favorite", item: modelData}) }
                    AppMenuItem { text: Core.tr(Core.uiLang, "Dismiss"); onTriggered: Core.cmd({cmd: "remove_progress", id: modelData.id}) }
                }
            }
        }
    }
}
