import QtQuick
import Reelhouse 1.0

// One stream of a title (GTK detail::refresh_streams row): an icon, the headline, a few lines of detail and the addon,
// with big badges at the right (quality, seeders coloured by count, size). A copy saved on disk is framed in green.
Rectangle {
    id: sr
    property var stream: ({})
    property string icon: stream.local ? "download" : "play"
    signal clicked()
    // the stream is for one episode: a torrent far bigger than an episode is a season pack (the addon gives the size
    // of the whole pack, but only the episode's file is downloaded)
    property bool episode: false
    // the size is always the episode file's own (the core reads the torrent's file list); a pack's size is never shown
    // in its place (HP, 2026-11-01): while it is read the row says so, and when it cannot be read it shows none
    readonly property string sizeShown: stream.sizeText ? stream.sizeText : (stream.sizeState === "looking" ? Core.tr(Core.uiLang, "size…") : "")
    readonly property bool saved: stream.local === true
    readonly property bool light: Theme.scheme === "light"
    readonly property var lines: {
        var l = (stream.lines || []).slice(0, saved ? 4 : 3)
        l.push(saved ? Core.tr(Core.uiLang, "Saved on disk") : (stream.addon || "") + (stream.isTorrent ? " · " + Core.tr(Core.uiLang, "torrent") : ""))
        return l
    }
    width: parent ? parent.width : 300
    height: Math.max(col.implicitHeight, badges.implicitHeight) + 22
    radius: Theme.shape.card
    color: saved ? (light ? "#3d2ec27e" : "#2e2ec27e") : (area.containsMouse ? Theme.c.cardHover : Theme.c.card)
    border.width: saved ? 2 : 1
    border.color: saved ? (light ? "#1a8a4f" : "#d92ec27e") : Theme.c.border

    Icon { id: ic; x: 12; anchors.verticalCenter: parent.verticalCenter; name: sr.icon; size: 18; color: Theme.c.textDim }
    Column {
        id: col
        anchors { left: ic.right; leftMargin: 12; right: badges.left; rightMargin: 10; verticalCenter: parent.verticalCenter }
        spacing: 2
        // the headline, then a flag for each audio language its name says it carries (HP, 2026-11-02)
        Item {
            width: parent.width
            height: head.implicitHeight
            Text {
                id: head
                width: Math.min(implicitWidth, parent.width - (flags.visible ? flags.width + 8 : 0))
                text: sr.stream.headline || sr.stream.name || ""
                color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family; elide: Text.ElideRight
            }
            Row {
                id: flags
                readonly property var audio: sr.stream.audio || []
                visible: audio.length > 0 || !!sr.stream.multiAudio
                anchors { left: head.right; leftMargin: 8; verticalCenter: head.verticalCenter }
                spacing: 4
                Repeater {
                    model: flags.audio.slice(0, 4)
                    delegate: Flag { required property var modelData; code: modelData.flag; h: 13; anchors.verticalCenter: parent.verticalCenter }
                }
                // "+2" when more languages, "DUAL"/"MULTI" when the name says so without naming them all
                Rectangle {
                    readonly property string label: flags.audio.length > 4 ? "+" + (flags.audio.length - 4)
                        : (sr.stream.multiAudio && flags.audio.length < 2 ? "MULTI" : "")
                    visible: label !== ""
                    anchors.verticalCenter: parent.verticalCenter
                    radius: 3
                    color: "transparent"
                    border.width: 1
                    border.color: Theme.c.textDim
                    implicitWidth: ml.implicitWidth + 8
                    implicitHeight: 15
                    Text { id: ml; anchors.centerIn: parent; text: parent.label; color: Theme.c.textDim; font.pixelSize: 10; font.weight: Font.Bold; font.family: Theme.type.family }
                }
            }
        }
        Repeater {
            model: sr.lines
            delegate: Text { required property var modelData; width: col.width; text: modelData; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family; elide: Text.ElideRight }
        }
    }
    Column {
        id: badges
        anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
        spacing: 4
        Rectangle {
            visible: sr.saved
            anchors.right: parent.right
            radius: height / 2
            color: Theme.c.greenBtn
            implicitWidth: st.implicitWidth + 20
            implicitHeight: 22
            Text { id: st; anchors.centerIn: parent; text: Core.tr(Core.uiLang, "Saved on disk"); color: Theme.c.greenFg; font.pixelSize: 12; font.weight: Font.ExtraBold; font.family: Theme.type.family }
        }
        Rectangle {
            visible: !!sr.stream.quality
            anchors.right: parent.right
            radius: 6
            color: Theme.c.accent
            implicitWidth: qt.implicitWidth + 18
            implicitHeight: 24
            Text { id: qt; anchors.centerIn: parent; text: sr.stream.quality || ""; color: Theme.c.accentFg; font.pixelSize: 15; font.weight: Font.ExtraBold; font.family: Theme.type.family }
        }
        Rectangle {
            readonly property int seeds: sr.stream.seeders || 0
            readonly property int level: seeds >= 20 ? 2 : (seeds >= 5 ? 1 : 0)
            visible: seeds > 0
            anchors.right: parent.right
            radius: 6
            color: sr.light ? ["#33e01b24", "#57f5c211", "#422ec27e"][level] : ["#4de01b24", "#47e5a50a", "#4d26a269"][level]
            border.width: 1
            border.color: sr.light ? ["#b21e2b", "#9a6a00", "#14733c"][level] : ["#99ff9c8a", "#99f9e27a", "#998ff0b4"][level]
            implicitWidth: sd.implicitWidth + 14
            implicitHeight: 24
            Text { id: sd; anchors.centerIn: parent; text: "▲ " + parent.seeds + " seeders"; color: sr.light ? ["#8f1420", "#6e4a00", "#0b5a2c"][parent.level] : ["#ff9c8a", "#f9e27a", "#8ff0b4"][parent.level]; font.pixelSize: 15; font.weight: Font.ExtraBold; font.family: Theme.type.family }
        }
        Text {
            visible: sr.sizeShown !== ""
            anchors.right: parent.right
            text: sr.sizeShown
            color: sr.stream.sizeText ? Theme.c.text : Theme.c.textDim
            font.pixelSize: 14
            font.weight: sr.stream.sizeText ? Font.Bold : Font.Normal
            font.family: Theme.type.family
        }
        Text {
            // a season torrent: only this episode's file is downloaded
            visible: sr.episode && !!sr.stream.packText
            anchors.right: parent.right
            text: Core.tr(Core.uiLang, "only this episode is downloaded")
            color: Theme.c.textDim
            font.pixelSize: 12
            font.family: Theme.type.family
        }
    }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sr.clicked() }
}
