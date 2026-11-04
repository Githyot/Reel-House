import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// libadwaita StatusPage: a big dim icon (or the Arara with a mood), a title and a line under it, centred.
// Every empty or loading page uses it, so they share one hierarchy: picture, title (Theme.type.title), description
// (body, dim), then optional actions (children go under the description, centred).
Column {
    id: sp
    property string icon: "list"
    property string title: ""
    property string description: ""
    // "" draws the icon; any Mascot mood ("idle", "think", "sad", "sleep"…) draws the Arara instead
    property string mood: ""
    // a spinner instead of the picture while something loads
    property bool busy: false
    default property alias actions: actionRow.data
    readonly property real textW: Math.min(460, sp.parent ? sp.parent.width - 40 : 460)
    spacing: 10
    BusyIndicator { anchors.horizontalCenter: parent.horizontalCenter; visible: sp.busy; running: visible }
    Mascot { anchors.horizontalCenter: parent.horizontalCenter; visible: !sp.busy && sp.mood !== ""; animate: visible && sp.visible; size: Theme.mobile ? 104 : 120; mood: sp.mood !== "" ? sp.mood : "idle" }
    Icon { anchors.horizontalCenter: parent.horizontalCenter; visible: !sp.busy && sp.mood === ""; name: sp.icon; size: Theme.mobile ? 72 : 96; color: Theme.c.textDim; opacity: 0.45 }
    Item { width: 1; height: 2 }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        visible: text !== ""
        width: Math.min(implicitWidth, 520, sp.textW + 60)
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: sp.title
        color: Theme.c.text
        font.pixelSize: Theme.type.title !== undefined ? Math.max(20, Theme.type.title.size + 2) : 22
        font.weight: Font.ExtraBold
        font.family: Theme.type.display || Theme.type.family
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        visible: text !== ""
        width: sp.textW
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: sp.description
        color: Theme.c.textDim
        font.pixelSize: 15
        lineHeight: 1.15
        font.family: Theme.type.family
    }
    Column {
        id: actionRow
        anchors.horizontalCenter: parent.horizontalCenter
        visible: children.length > 0
        topPadding: 6
        spacing: 10
    }
}
