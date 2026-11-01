import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Preferences > Torrents (GTK prefs2::torrents_page): the disk space taken, and what is stored, grouped by title with its
// cover. Two views: what you chose to keep on disk, and what is only the streaming cache.
Column {
    id: tp
    readonly property var t: Core.torrents
    readonly property var groups: t.groups !== undefined ? t.groups : []
    property bool savedView: true
    property var openGroups: []
    readonly property var shown: groups.map(function(g) {
        var rows = (g.rows || []).filter(function(r) { return r.saved === tp.savedView })
        var bytes = rows.reduce(function(a, r) { return a + r.bytes }, 0)
        return {title: g.title, poster: g.poster, rows: rows, bytes: bytes}
    }).filter(function(g) { return g.rows.length > 0 })
    spacing: 26
    onVisibleChanged: if (visible) Core.refreshTorrents()

    function human(b) {
        var u = ["B", "KB", "MB", "GB", "TB"], i = 0
        while (b >= 1024 && i < u.length - 1) { b /= 1024; ++i }
        return (i === 0 ? b : b.toFixed(1)) + " " + u[i]
    }

    PrefGroup {
        title: Core.tr(Core.uiLang, "Disk space")
        PrefRow {
            title: tp.t.total || "0 B"
            subtitle: Core.tr(Core.uiLang, "Kept on purpose") + ": " + (tp.t.saved || "0 B") + " · " + Core.tr(Core.uiLang, "Streaming cache") + ": " + (tp.t.cache || "0 B")
            FlatButton {
                anchors.verticalCenter: parent.verticalCenter
                kind: "destructive"
                text: Core.tr(Core.uiLang, "Clear")
                implicitHeight: 34
                ToolTip.visible: hovered
                ToolTip.text: Core.tr(Core.uiLang, "Deletes the streaming cache. What you chose to keep stays.")
                onClicked: Core.cmd({cmd: "clear_torrent_cache"})
            }
        }
    }

    PrefGroup {
        id: content
        title: Core.tr(Core.uiLang, "Content")
        description: Core.tr(Core.uiLang, "Torrents of the same title are together, with its cover.")
        // The two views, as a linked pair of toggle buttons.
        suffix: Row {
            anchors.right: parent.right
            spacing: 0
            Repeater {
                model: [{label: Core.tr(Core.uiLang, "Saved on disk"), saved: true}, {label: Core.tr(Core.uiLang, "Streaming cache"), saved: false}]
                delegate: Item {
                    required property var modelData
                    required property int index
                    width: vLbl.implicitWidth + 26
                    height: 32
                    clip: true
                    // Linked: only the outer corners are round (a wider rounded box, cut by the item).
                    Rectangle {
                        x: index === 0 ? 0 : -10
                        width: parent.width + 10
                        height: parent.height
                        radius: Theme.shape.control
                        color: tp.savedView === modelData.saved ? Theme.c.selected : (vArea.containsMouse ? Theme.c.buttonHover : Theme.c.button)
                    }
                    Text { id: vLbl; anchors.centerIn: parent; text: modelData.label; color: Theme.c.text; font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family }
                    MouseArea { id: vArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: tp.savedView = modelData.saved }
                }
            }
        }
        PrefRow {
            visible: tp.shown.length === 0
            title: Core.tr(Core.uiLang, "Nothing stored")
            subtitle: tp.savedView ? Core.tr(Core.uiLang, "Torrents you keep with “Keep downloaded” appear here.") : Core.tr(Core.uiLang, "Torrents you stream appear here.")
        }
        Repeater {
            model: tp.shown
            // An expander row (adw::ExpanderRow): cover, title, count, size and "Remove all"; it opens to one row per torrent.
            delegate: Column {
                id: grp
                required property var modelData
                required property int index
                readonly property bool open: tp.openGroups.indexOf(modelData.title) >= 0
                property bool first: true
                width: parent ? parent.width : 400
                Rectangle { visible: !grp.first; width: parent.width; height: 1; color: Theme.c.divider }
                Item {
                    width: parent.width
                    height: 102
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            var o = tp.openGroups.filter(function(x) { return x !== grp.modelData.title })
                            if (!grp.open) o.push(grp.modelData.title)
                            tp.openGroups = o
                        }
                    }
                    RoundedPicture {
                        id: cov
                        x: 14
                        anchors.verticalCenter: parent.verticalCenter
                        width: 60
                        height: 90
                        radius: 6
                        url: grp.modelData.poster || ""
                    }
                    Column {
                        anchors { left: cov.right; leftMargin: 12; right: gSuffix.left; rightMargin: 10; verticalCenter: parent.verticalCenter }
                        spacing: 2
                        Text { width: parent.width; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight; text: grp.modelData.title; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
                        Text {
                            width: parent.width
                            text: (grp.modelData.rows.length === 1 ? Core.tr(Core.uiLang, "1 torrent") : grp.modelData.rows.length + " " + Core.tr(Core.uiLang, "torrents")) + " · " + (tp.savedView ? Core.tr(Core.uiLang, "kept on purpose") : Core.tr(Core.uiLang, "streaming cache"))
                            color: Theme.c.textDim
                            font.pixelSize: 13
                            font.family: Theme.type.family
                        }
                    }
                    Row {
                        id: gSuffix
                        anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                        spacing: 10
                        Text { anchors.verticalCenter: parent.verticalCenter; text: tp.human(grp.modelData.bytes); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
                        FlatButton {
                            anchors.verticalCenter: parent.verticalCenter
                            kind: "destructive"
                            text: Core.tr(Core.uiLang, "Remove all")
                            implicitHeight: 34
                            onClicked: grp.modelData.rows.forEach(function(r) { Core.cmd({cmd: "remove_torrent", key: r.key}) })
                        }
                        Icon { anchors.verticalCenter: parent.verticalCenter; name: "down"; size: 16; color: Theme.c.textDim; rotation: grp.open ? 180 : 0; Behavior on rotation { NumberAnimation { duration: Theme.motion.fast } } }
                    }
                }
                Rectangle {
                    visible: grp.open
                    width: parent.width
                    height: inner.implicitHeight
                    color: Theme.c.fill3
                    Column {
                        id: inner
                        width: parent.width
                        Repeater {
                            model: grp.modelData.rows
                            delegate: PrefRow {
                                required property var modelData
                                required property int index
                                first: false
                                title: modelData.title
                                subtitle: modelData.detail
                                highlight: modelData.saved
                                Text { anchors.verticalCenter: parent.verticalCenter; text: modelData.size; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "destructive"; text: Core.tr(Core.uiLang, "Remove"); implicitHeight: 34; onClicked: Core.cmd({cmd: "remove_torrent", key: modelData.key}) }
                            }
                        }
                    }
                }
            }
        }
    }
}
