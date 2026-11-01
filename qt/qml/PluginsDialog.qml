import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Text mode: the plugins (sites of novels) offered by the repository, to install or remove.
Popup {
    id: pd
    property bool images: false             // images: the extensions of the Suwayomi server; text: the novel plugins
    readonly property var nv: Core.manga.novels || ({})
    readonly property var su: (Core.prefs.extra && Core.prefs.extra.suwayomi) ? Core.prefs.extra.suwayomi : ({})
    readonly property var items: images ? (su.exts || []) : ((nv.catalog || {}).items || [])
    // the language filter, the same one as Preferences > Images / Text (HP: "search in language specific")
    readonly property var g: Core.prefs.general || ({})
    readonly property var langsOn: images ? (g.manga_langs || []) : (g.novel_langs || [])
    function codeOf(p) { return images ? (p.code || "") : (p.lang || "").toLowerCase().replace("_", "-") }
    readonly property var itemLangs: {
        const out = [], seen = {}
        items.forEach(function(p) {
            const c = codeOf(p)
            if (c && !seen[c]) { seen[c] = true; out.push({code: c, name: images ? (p.langName || p.lang) : p.lang}) }
        })
        out.sort(function(a, b) { return a.name < b.name ? -1 : 1 })
        return out
    }
    readonly property string status: images ? (su.status || "") : ((nv.catalog || {}).status || "")
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(640, (Overlay.overlay ? Overlay.overlay.width : 700) - 48)
    height: Math.min(600, (Overlay.overlay ? Overlay.overlay.height : 700) - 48)
    modal: true
    padding: 16
    topPadding: 0
    background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.popover; shadow: true }
    onOpened: { Core.cmd({cmd: images ? "suwayomi_list_exts" : "novel_load_catalog"}); q.text = "" }
    Column {
        anchors.fill: parent
        spacing: 10
        // the dialog's padding is taken back so the title bar spans the whole window, like the others
        DialogHeader { x: -16; width: parent.width + 32; title: pd.images ? Core.tr(Core.uiLang, "Image sources") : Core.tr(Core.uiLang, "Text sources"); onClose: pd.close() }
        Lbl { width: parent.width; text: pd.status; visible: text !== ""; dim: true; wrapMode: Text.Wrap }
        Row {
            width: parent.width
            spacing: 8
            SearchEntry { id: q; width: parent.width - langBtn.width - 8; placeholderText: Core.tr(Core.uiLang, "Search sources") }
            LangButton {
                id: langBtn
                anchors.verticalCenter: parent.verticalCenter
                options: pd.itemLangs
                selected: pd.langsOn
                onChanged: function(list) { Core.cmd({cmd: "set_setting", path: pd.images ? "general.manga_langs" : "general.novel_langs", value: list}) }
            }
        }
        ListView {
            id: list
            width: parent.width
            height: parent.height - 132
            clip: true
            spacing: 6
            // the number is the model: installing a plugin changes its data, not the count, so the scroll stays where it is
            property var shown: pd.items.filter(function(p) { const c = pd.codeOf(p); if (pd.langsOn.length > 0 && c !== "all" && c !== "multi" && pd.langsOn.indexOf(c) < 0) return false; return q.text === "" || (p.name + " " + p.lang + " " + (p.langName || "") + " " + (p.site || "")).toLowerCase().indexOf(q.text.toLowerCase()) >= 0 })
            model: shown.length
            ScrollBar.vertical: ScrollBar {}
            delegate: Rectangle {
                required property int index
                readonly property var modelData: list.shown[index] || ({})
                width: ListView.view.width
                height: 62
                radius: Theme.shape.card
                color: Theme.c.card
                border.width: 1
                border.color: Theme.c.border
                SourceIcon { id: ic; x: 10; anchors.verticalCenter: parent.verticalCenter; size: 40; url: modelData.icon || ""; name: modelData.name }
                Column {
                    anchors { left: ic.right; leftMargin: 12; right: act.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                    Lbl { width: parent.width; elide: Text.ElideRight; text: modelData.name; bold: true; size: 15 }
                    Lbl { width: parent.width; elide: Text.ElideRight; text: (pd.images ? (modelData.langName || modelData.lang || "") + (modelData.nsfw ? "  ·  18+" : "") + (modelData.update ? "  ·  " + Core.tr(Core.uiLang, "Update available") : "") : (modelData.lang || "") + "  ·  v" + (modelData.version || "")); dim: true; size: 12 }
                }
                FlatButton {
                    id: act
                    anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                    implicitHeight: 34
                    kind: modelData.installed && !modelData.update ? "plain" : "suggested"
                    glyph: modelData.installed && !modelData.update ? "trash" : "download"
                    text: modelData.installed && !modelData.update ? Core.tr(Core.uiLang, "Remove") : (modelData.update ? Core.tr(Core.uiLang, "Update") : Core.tr(Core.uiLang, "Install"))
                    onClicked: {
                        const rm = modelData.installed && !modelData.update
                        if (pd.images) Core.cmd(rm ? {cmd: "suwayomi_uninstall", pkg: modelData.pkg} : {cmd: "suwayomi_install", pkg: modelData.pkg})
                        else Core.cmd(rm ? {cmd: "novel_remove_plugin", id: modelData.id} : {cmd: "novel_install", id: modelData.id})
                    }
                }
            }
        }
    }
}
