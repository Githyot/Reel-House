import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// One tab of Research mode's vertical tab list, in the sidebar: the site's icon (a spinner while it loads), the title,
// and a close button. Drawn like the tabs of each theme's own browser: Safari's sidebar rows (Apple), Epiphany's
// libadwaita rows (GNOME), Edge's vertical tabs (Windows: the current one raised on a card), Chrome's Material 3 rows
// (Android: pill shaped). `compact` (a narrow rail) shows the icon only. Right click opens its menu, a middle click
// closes it; dragging it up or down moves it in the list.
Rectangle {
    id: row
    property var tab: ({})            // {id, url, title, icon, loading}
    property bool current: false
    property bool compact: false
    readonly property string look: Theme.look
    signal activated()
    signal closeRequested()
    signal menuRequested(Item item, real x, real y)
    // the tab `id` was dragged by this many rows (the list moves it; the row may show another tab meanwhile)
    signal moveBy(int id, int rows)
    height: look === "android" ? 44 : (look === "windows" ? 36 : 34)
    radius: look === "android" ? height / 2 : (look === "windows" ? 4 : (look === "apple" ? 8 : 6))
    color: current ? (look === "windows" ? Theme.c.card : Theme.c.sidebarSelected) : (area.containsMouse ? Theme.c.hover : "transparent")
    border.width: current && look === "windows" ? 1 : 0
    border.color: Theme.c.border
    Behavior on color { ColorAnimation { duration: Theme.motion.fast } }

    // Edge: the accent mark at the left of the current tab
    Rectangle {
        visible: row.look === "windows" && row.current && !row.compact
        x: 1
        anchors.verticalCenter: parent.verticalCenter
        width: 3; height: 16; radius: 1.5
        color: Theme.c.accent
    }
    // the site's icon; the globe while there is none
    Item {
        id: iconBox
        x: row.compact ? (row.width - width) / 2 : (row.look === "android" ? 14 : 10)
        width: 16; height: 16
        anchors.verticalCenter: parent.verticalCenter
        Image {
            id: fav
            anchors.fill: parent
            visible: !row.tab.loading && status === Image.Ready
            source: row.tab.icon || ""
            sourceSize: Qt.size(32, 32)
            asynchronous: true
            fillMode: Image.PreserveAspectFit
        }
        Icon { anchors.centerIn: parent; visible: !row.tab.loading && fav.status !== Image.Ready; name: "globe"; size: 15; color: Theme.c.textDim }
        // loading: a small turning arc
        Rectangle {
            visible: !!row.tab.loading
            anchors.fill: parent
            radius: width / 2
            color: "transparent"
            border.width: 2
            border.color: Theme.c.fill3
            Rectangle { width: 6; height: 6; radius: 3; color: Theme.c.accent; x: parent.width / 2 - 3; y: -1 }
            RotationAnimation on rotation { running: !!row.tab.loading && row.visible; from: 0; to: 360; duration: 900; loops: Animation.Infinite }
        }
    }
    Text {
        visible: !row.compact
        anchors { left: iconBox.right; leftMargin: 10; right: closeBtn.visible ? closeBtn.left : parent.right; rightMargin: 6; verticalCenter: parent.verticalCenter }
        elide: Text.ElideRight
        text: row.tab.title || row.tab.url || Core.tr(Core.uiLang, "New tab")
        color: Theme.c.text
        font.pixelSize: row.look === "apple" ? 13 : 14
        font.weight: row.current ? (row.look === "android" ? Font.DemiBold : Font.Medium) : Font.Normal
        font.family: Theme.type.family
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
        cursorShape: dragging ? Qt.ClosedHandCursor : Qt.PointingHandCursor
        property real startY: 0
        property int pressedId: 0
        property bool dragging: false
        ToolTip.visible: containsMouse && !dragging && (row.compact || (row.tab.title || "").length > 26)
        ToolTip.text: row.tab.title || row.tab.url || ""
        ToolTip.delay: 600
        onPressed: function(m) { startY = m.y; dragging = false; pressedId = row.tab.id || 0 }
        onPositionChanged: function(m) {
            if (!pressed || m.buttons !== Qt.LeftButton) return
            const step = row.height + 2
            const d = m.y - startY
            if (Math.abs(d) > step * 0.6) {
                dragging = true
                const n = d > 0 ? Math.floor(d / step + 0.4) : Math.ceil(d / step - 0.4)
                if (n !== 0) { row.moveBy(pressedId, n); startY += n * step }
            }
        }
        onReleased: dragging = false
        onClicked: function(m) {
            if (dragging) return
            if (m.button === Qt.MiddleButton) row.closeRequested()
            else if (m.button === Qt.RightButton) row.menuRequested(row, m.x, m.y)
            else row.activated()
        }
    }
    Item {
        id: closeBtn
        // Chrome (Android) always shows it; the others on hover and on the current tab
        visible: !row.compact && (row.look === "android" || area.containsMouse || closeArea.containsMouse || row.current)
        anchors { right: parent.right; rightMargin: row.look === "android" ? 8 : 4; verticalCenter: parent.verticalCenter }
        width: 26; height: 26
        Rectangle { anchors.fill: parent; radius: row.look === "windows" ? 4 : width / 2; color: closeArea.containsMouse ? Theme.c.hover : "transparent" }
        Icon { anchors.centerIn: parent; name: "close"; size: 14; color: Theme.c.textDim }
        MouseArea {
            id: closeArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            ToolTip.visible: containsMouse
            ToolTip.text: Core.tr(Core.uiLang, "Close tab") + " (Ctrl+W)"
            onClicked: row.closeRequested()
        }
    }
}
