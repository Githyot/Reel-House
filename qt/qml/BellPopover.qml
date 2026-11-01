import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The notification center (GTK browse.rs): new episodes of what is in the library and favorites, new chapters of the
// Images and Text series, and the pop-ups of the Study mode (reminders, goals, the Pomodoro…) with the macaw, so a
// banner that went by can still be read and clicked.
Popover {
    id: bp
    readonly property var items: Core.notices.items !== undefined ? Core.notices.items : []
    function tr(s) { return Core.tr(Core.uiLang, s) }
    // "now", "5 min", "3 h", "yesterday", or the date
    function ago(at) {
        if (!at) return ""
        const s = Math.max(0, Date.now() / 1000 - at)
        if (s < 60) return tr("now")
        if (s < 3600) return Math.floor(s / 60) + " min"
        if (s < 86400) return Math.floor(s / 3600) + " h"
        if (s < 2 * 86400) return tr("yesterday")
        return new Date(at * 1000).toLocaleDateString(Qt.locale(String(Core.uiLang).replace("-", "_")), Locale.ShortFormat)
    }
    padding: 8
    contentWidth: Math.min(372, bp.ovW - 16)
    contentItem: Column {
        width: Math.min(372, bp.ovW - 16)
        spacing: 6
        Item {
            width: parent.width
            height: 34
            Text {
                x: 4
                anchors { verticalCenter: parent.verticalCenter; right: tools.left; rightMargin: 8 }
                elide: Text.ElideRight
                text: bp.tr("Notifications"); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family
            }
            Row {
                id: tools
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                spacing: 2
                FlatButton { kind: "circular"; glyph: "refresh"; background: Rectangle { radius: 17; color: parent.down ? Theme.c.selected : (parent.hovered ? Theme.c.hover : "transparent") } ToolTip.visible: hovered; ToolTip.text: bp.tr("Check now"); onClicked: Core.cmd({cmd: "check_new_episodes"}) }
                FlatButton { kind: "circular"; glyph: "clearall"; enabled: bp.items.length > 0; background: Rectangle { radius: 17; color: parent.down ? Theme.c.selected : (parent.hovered ? Theme.c.hover : "transparent"); opacity: parent.enabled ? 1 : 0.4 } ToolTip.visible: hovered; ToolTip.text: bp.tr("Clear all"); onClicked: Core.cmd({cmd: "clear_all_notices"}) }
            }
        }
        Text { visible: bp.items.length === 0; width: parent.width; topPadding: 14; bottomPadding: 14; horizontalAlignment: Text.AlignHCenter; text: bp.tr("Nothing new."); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
        Flickable {
            visible: bp.items.length > 0
            width: parent.width
            height: Math.min(440, list.implicitHeight)
            contentHeight: list.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: list.implicitHeight > 440 ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
            Column {
                id: list
                width: parent.width
                spacing: 4
                Repeater {
                    model: bp.items.length
                    delegate: Rectangle {
                        id: row
                        required property int index
                        readonly property var n: bp.items[index] || ({})
                        readonly property bool banner: n.kind === "banner"
                        width: list.width
                        height: banner ? Math.max(62, bTxt.implicitHeight + 16) : 66
                        radius: Theme.shape.small
                        color: rowHover.hovered ? Theme.c.hover : "transparent"
                        // (a handler: it stays hovered over the × button too)
                        HoverHandler { id: rowHover }
                        MouseArea {
                            id: nArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                bp.close()
                                if (row.banner) {
                                    // the banner's own action (open the Study mode, a deck, the Pomodoro…), then it is read
                                    if (row.n.action) Core.cmd(row.n.action)
                                    Core.cmd({cmd: "dismiss_notice", id: row.n.id})
                                } else {
                                    Core.cmd({cmd: "open_notice", id: row.n.id, title: row.n.title, poster: row.n.poster ? row.n.poster : ""})
                                }
                            }
                        }

                        // ---- an episode or new chapters: the cover, the title, what came out ----
                        RoundedPicture { id: np; visible: !row.banner; x: 6; anchors.verticalCenter: parent.verticalCenter; width: 36; height: 54; radius: 5; url: !row.banner && row.n.poster ? row.n.poster : "" }
                        Column {
                            visible: !row.banner
                            anchors { left: np.right; leftMargin: 10; right: badge.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                            spacing: 1
                            Text { width: parent.width; elide: Text.ElideRight; text: row.n.title || ""; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
                            Text { width: parent.width; elide: Text.ElideRight; text: row.n.kind ? (row.n.label || "") + " " + bp.tr("new chapters") + " · " + bp.tr(row.n.kind === "novel" ? "Text" : "Images") : (row.n.episodeTitle ? (row.n.label || "") + " · " + row.n.episodeTitle : (row.n.label || "")); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                        }
                        NewBadge { id: badge; visible: !row.banner; anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter } }

                        // ---- a Study pop-up: the macaw in its mood, title and message, when ----
                        Rectangle {
                            id: bird
                            visible: row.banner
                            x: 6; anchors.verticalCenter: parent.verticalCenter
                            width: 44; height: 44; radius: 22
                            color: Qt.rgba(Theme.c.accent.r, Theme.c.accent.g, Theme.c.accent.b, 0.14)
                            Mascot { anchors.centerIn: parent; size: 38; mood: row.n.mood || "idle"; animate: false }
                        }
                        Column {
                            id: bTxt
                            visible: row.banner
                            anchors { left: bird.right; leftMargin: 10; right: when.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                            spacing: 1
                            Text { width: parent.width; elide: Text.ElideRight; text: row.n.title || ""; color: Theme.c.text; font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family }
                            Text { visible: text !== ""; width: parent.width; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight; text: row.n.label || ""; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                        }
                        Text {
                            id: when
                            visible: row.banner && !rowHover.hovered
                            anchors { right: parent.right; rightMargin: 10; top: parent.top; topMargin: 10 }
                            text: row.banner ? bp.ago(row.n.at) : ""
                            color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family
                        }
                        // remove it without following it
                        FlatButton {
                            visible: row.banner && rowHover.hovered
                            anchors { right: parent.right; rightMargin: 4; top: parent.top; topMargin: 4 }
                            kind: "flatround"; glyph: "close"; iconSize: 14; implicitWidth: 26; implicitHeight: 26
                            ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: bp.tr("Remove")
                            onClicked: Core.cmd({cmd: "dismiss_notice", id: row.n.id})
                        }
                    }
                }
            }
        }
    }
}
