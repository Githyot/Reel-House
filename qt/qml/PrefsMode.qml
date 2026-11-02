import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The tab of a mode in Preferences: Images (reading, the Suwayomi server, repositories, extensions, text recognition)
// or Text (reading and the plugins of the novel sources).
Column {
    id: pm
    property string kind: "images"          // images | text
    property var dlg
    readonly property var g: dlg ? dlg.g : ({})
    readonly property var ex: dlg ? dlg.ex : ({})
    readonly property var su: ex.suwayomi || ({})
    readonly property var nv: Core.manga.novels || ({})
    readonly property var pt: Core.manga.pagetools || ({})
    readonly property bool images: kind === "images"
    // a phone with the Suwayomi address still on this device: no server can start here (Java)
    // Android runs the extensions itself (docs/android-extensions.md); only a phone told to use a Suwayomi server at
    // its own address (which it cannot run) gets the explanation below
    readonly property bool ownHost: Qt.platform.os === "android" && g.suwayomi_managed !== false
    readonly property bool phoneLocal: (Qt.platform.os === "ios" || (Qt.platform.os === "android" && g.suwayomi_managed === false)) && /^https?:\/\/(127\.0\.0\.1|localhost)/.test(g.suwayomi_url || "http://127.0.0.1")
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function set(path, v) { Core.cmd({cmd: "set_setting", path: path, value: v}) }
    function idx(list, v) { const i = list.indexOf(v); return i < 0 ? 0 : i }
    spacing: 26

    // what comes from the servers arrives by itself; ask for the numbers again while this tab is open
    Timer { interval: 3000; repeat: true; running: pm.visible && pm.images; onTriggered: Core.refreshPrefs() }
    // The list of sources is long (hundreds): it is shown in pieces, a few more every moment, so the window never stalls.
    property int limit: 30
    onQueryChanged: limit = 30
    onLangsOnChanged: limit = 30
    Timer { interval: 140; repeat: true; running: pm.visible && pm.limit < pm.shown.length; onTriggered: pm.limit = pm.limit + 30 }
    readonly property var page: shown.slice(0, limit)

    // ---------- the lists of extensions / plugins, filtered ----------
    property string query: ""
    readonly property var allItems: images ? (su.exts || []) : ((nv.catalog || {}).items || [])
    readonly property var langsOn: images ? (g.manga_langs || []) : (g.novel_langs || [])
    readonly property var itemLangs: {
        const out = [], seen = {}
        allItems.forEach(function(e) {
            const code = images ? e.code : (e.lang || "").toLowerCase().replace("_", "-")
            if (code && !seen[code]) { seen[code] = true; out.push({code: code, name: images ? (e.langName || e.lang) : e.lang}) }
        })
        out.sort(function(a, b) { return a.name < b.name ? -1 : 1 })
        return out
    }
    readonly property var shown: allItems.filter(function(e) {
        const code = images ? e.code : (e.lang || "").toLowerCase().replace("_", "-")
        if (langsOn.length > 0 && code !== "all" && code !== "multi" && langsOn.indexOf(code) < 0) return false
        if (images && e.nsfw && !g.show_nsfw) return false
        const q = query.toLowerCase()
        return q === "" || (e.name + " " + (e.lang || "")).toLowerCase().indexOf(q) >= 0
    })
    readonly property int toInstall: shown.filter(function(e) { return !e.installed }).length

    // ================= Images: reading =================
    PrefGroup {
        visible: pm.images
        title: pm.tr("Reading")
        description: pm.tr("The language filters MangaDex and the sources of the Suwayomi server. More options are in the reader itself.")
        PrefCombo {
            title: pm.tr("Language of the chapters"); subtitle: pm.tr("Also decides which sources come first in Explore.")
            options: (pm.ex.mangaLangs || []).map(function(l) { return l.name })
            current: Math.max(0, (pm.ex.mangaLangs || []).map(function(l) { return l.code }).indexOf((pm.g.manga_lang || "en").toLowerCase()))
            onPicked: function(i) { pm.set("general.manga_lang", pm.ex.mangaLangs[i].code) }
        }
        PrefCombo {
            readonly property var ids: ["rtl", "ltr", "vertical", "webtoon"]
            title: pm.tr("Reading mode"); subtitle: pm.tr("Can also be changed inside the reader.")
            options: [pm.tr("Right to left"), pm.tr("Left to right"), pm.tr("Vertical (page by page)"), pm.tr("Webtoon (continuous strip)")]
            current: pm.idx(ids, pm.g.manga_mode)
            onPicked: function(i) { pm.set("general.manga_mode", ids[i]) }
        }
        PrefCombo {
            readonly property var ids: ["width", "screen", "height", "original"]
            title: pm.tr("Fit the picture"); subtitle: pm.tr("How a page is sized in the reader.")
            options: [pm.tr("To the width"), pm.tr("To the screen"), pm.tr("To the height"), pm.tr("Original size")]
            current: pm.idx(ids, pm.g.reader_fit)
            onPicked: function(i) { pm.set("general.reader_fit", ids[i]) }
        }
        PrefCombo {
            readonly property var ids: ["dark", "black", "white", "theme"]
            title: pm.tr("Background")
            options: [pm.tr("Dark"), pm.tr("Black"), pm.tr("White"), pm.tr("Like the app")]
            current: pm.idx(ids, pm.g.reader_bg)
            onPicked: function(i) { pm.set("general.reader_bg", ids[i]) }
        }
        PrefSwitch { title: pm.tr("Buttons on the screen"); subtitle: pm.tr("Arrows on the sides of the page that turn it with a click"); checked: !!pm.g.reader_buttons; onToggled: function(on) { pm.set("general.reader_buttons", on) } }
        PrefSwitch { title: pm.tr("Tap zones"); subtitle: pm.tr("Click the left or the right part of the page to turn it"); checked: !!pm.g.reader_taps; onToggled: function(on) { pm.set("general.reader_taps", on) } }
        PrefSwitch { title: pm.tr("Data saver"); subtitle: pm.tr("Smaller pictures, for slow connections"); checked: !!pm.g.manga_saver; onToggled: function(on) { pm.set("general.manga_saver", on) } }
    }

    // ================= Images: upscaling =================
    PrefGroup {
        visible: pm.images
        title: pm.tr("Upscaling (AI)")
        description: pm.tr("The upscale button of the reader makes the pages sharper with an AI model that runs on your graphics card (Vulkan: NVIDIA, AMD and Intel). Each page is done once and kept.")
        PrefRow {
            title: pm.tr("Engine")
            subtitle: pm.pt.engine ? pm.tr("Ready") : pm.tr("Not downloaded yet (about 40 MB). The button of the reader downloads it too.")
            FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: pm.pt.engine ? "plain" : "suggested"; implicitHeight: 34; glyph: "download"; text: pm.pt.engine ? pm.tr("Download again") : pm.tr("Download"); onClicked: Core.cmd({cmd: "upscale_install"}) }
        }
        PrefEntry { title: pm.tr("Another engine (optional)"); first: false; text: pm.g.up_engine || ""; onEdited: function(t) { pm.set("general.up_engine", t.trim()) } }
        PrefRow { title: pm.tr("Models"); first: false; subtitle: pm.tr("Any NCNN model works: put its .param and .bin files in the \"models\" folder next to the engine. The model is chosen in the options of the reader.") }
        PrefSpin { title: pm.tr("Tile size"); subtitle: pm.tr("0 is automatic. Lower it if the graphics card runs out of memory"); first: false; from: 0; to: 1024; step: 32; value: pm.g.up_tile || 0; onMoved: function(v) { pm.set("general.up_tile", v) } }
        PrefSpin { title: pm.tr("Space for upscaled pages (MB)"); subtitle: pm.tr("The oldest pages are deleted when it is full"); first: false; from: 100; to: 20000; step: 100; value: pm.g.up_cache_mb || 1024; onMoved: function(v) { pm.set("general.up_cache_mb", v) } }
    }

    // ================= Images: text of the pages =================
    PrefGroup {
        visible: pm.images
        title: pm.tr("Text of the pages")
        description: pm.tr("The reader reads the speech bubbles (OCR); then click a bubble to translate it. The page is sent to the reading service.")
        PrefCombo {
            readonly property var ids: ["ocrspace", "vision"]
            title: pm.tr("Reading service"); subtitle: pm.tr("Google Cloud Vision reads vertical Japanese text much better than the free service")
            options: ["OCR.space", "Google Cloud Vision"]
            current: pm.idx(ids, pm.g.ocr_provider)
            onPicked: function(i) { pm.set("general.ocr_provider", ids[i]) }
        }
        PrefEntry { title: pm.tr("API key (optional for OCR.space)"); first: false; password: true; text: pm.g.ocr_key || ""; onEdited: function(t) { pm.set("general.ocr_key", t.trim()) } }
        PrefCombo {
            readonly property var ids: ["jpn", "kor", "chs", "eng", "auto"]
            title: pm.tr("Language written on the pages"); first: false
            options: [pm.tr("Japanese"), pm.tr("Korean"), pm.tr("Chinese"), pm.tr("English"), pm.tr("Automatic")]
            current: pm.idx(ids, pm.g.ocr_lang)
            onPicked: function(i) { pm.set("general.ocr_lang", ids[i]) }
        }
        PrefCombo {
            readonly property var ids: ["en", "pt", "es", "fr", "de", "it", "nl", "sv", "pl", "ru", "tr", "ar", "hi", "id", "vi", "th", "ja", "ko", "zh"]
            title: pm.tr("Translate into"); subtitle: pm.tr("Uses the translator of the subtitles"); first: false
            options: ["English", "Português", "Español", "Français", "Deutsch", "Italiano", "Nederlands", "Svenska", "Polski", "Русский", "Türkçe", "العربية", "हिन्दी", "Bahasa Indonesia", "Tiếng Việt", "ไทย", "日本語", "한국어", "中文"]
            current: pm.idx(ids, ((pm.dlg && pm.dlg.sec) ? pm.dlg.sec("subtitles").target_lang : "") || "en")
            onPicked: function(i) { pm.set("subtitles.target_lang", ids[i]) }
        }
    }

    // ================= Text: reading =================
    PrefGroup {
        visible: !pm.images
        title: pm.tr("Reading")
        PrefSpin { title: pm.tr("Text size"); subtitle: pm.tr("Can also be changed inside the reader."); from: 10; to: 48; value: pm.g.novel_size || 19; onMoved: function(v) { pm.set("general.novel_size", v) } }
    }
    PrefGroup {
        visible: !pm.images
        title: pm.tr("Source repository")
        description: pm.tr("Light novels and web novels. Sources are JavaScript plugins in the format that manga and light novel readers use. The app comes with no source: paste the address of a plugin list you trust.")
        PrefEntry { title: pm.tr("Repository address"); text: pm.g.text_repo || ""; onEdited: function(t) { pm.set("general.text_repo", t.trim()) } }
        PrefRow {
            title: pm.tr("Status"); subtitle: (pm.nv.catalog || {}).status || pm.tr("Not read yet"); first: false
            FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "suggested"; implicitHeight: 34; text: pm.tr("Read the repository"); onClicked: Core.cmd({cmd: "novel_load_catalog"}) }
        }
    }

    // ================= Images: the Suwayomi server =================
    PrefGroup {
        visible: pm.images
        title: Qt.platform.os === "android" ? pm.tr("Sources (extensions)") : pm.tr("Sources (Suwayomi)")
        description: Qt.platform.os === "android" ? pm.tr("The app runs extensions in the Tachiyomi / Mihon format by itself. The app comes with no repository: add the ones you want below.") : pm.tr("Suwayomi runs extensions in the Tachiyomi / Mihon format. The app downloads it once and runs it by itself while the Images mode is open. The app comes with no repository: add the ones you want below.")
        PrefSwitch { title: Qt.platform.os === "android" ? pm.tr("Run them inside the app") : pm.tr("Run it inside the app"); subtitle: Qt.platform.os === "android" ? pm.tr("Turn off to use a Suwayomi server instead") : pm.tr("Turn off to use a Suwayomi server you started yourself"); checked: pm.g.suwayomi_managed !== false; onToggled: function(on) { pm.set("general.suwayomi_managed", on) } }
        PrefEntry { visible: pm.g.suwayomi_managed === false; first: false; title: pm.tr("Server address"); text: pm.g.suwayomi_url || ""; onEdited: function(t) { pm.set("general.suwayomi_url", t.trim()) } }
        PrefEntry { visible: pm.g.suwayomi_managed === false; first: false; title: pm.tr("User name (leave empty if there is none)"); text: pm.g.suwayomi_user || ""; onEdited: function(t) { pm.set("general.suwayomi_user", t.trim()) } }
        PrefEntry { visible: pm.g.suwayomi_managed === false; first: false; title: pm.tr("Password"); password: true; text: pm.g.suwayomi_pass || ""; onEdited: function(t) { pm.set("general.suwayomi_pass", t) } }
        PrefRow {
            title: pm.tr("Status"); subtitle: pm.su.status || ""; first: false
            FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "suggested"; implicitHeight: 34; text: pm.g.suwayomi_managed === false ? pm.tr("Connect") : pm.tr("Restart"); onClicked: Core.cmd({cmd: "suwayomi_restart"}) }
        }
    }

    // ================= Images: repositories =================
    PrefGroup {
        visible: pm.images
        title: pm.tr("Repositories")
        description: pm.tr("Where the extensions come from. Add the address of a repository (index.min.json, index.pb or its GitHub page).")
        PrefEntry { id: repoEntry; title: pm.tr("Add a repository"); apply: true; text: ""; onApplied: function(t) { Core.cmd({cmd: "suwayomi_add_repo", text: t.trim()}); repoEntry.clear() } }
        Repeater {
            model: pm.su.repos || []
            delegate: PrefRow {
                required property var modelData
                title: modelData
                first: false
                FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flat"; glyph: "trash"; implicitWidth: 34; implicitHeight: 34; ToolTip.visible: hovered; ToolTip.text: pm.tr("Remove"); onClicked: Core.cmd({cmd: "suwayomi_remove_repo", url: modelData}) }
            }
        }
    }

    // ================= the sources to install =================
    Column {
        width: parent.width
        spacing: 8
        Lbl { text: pm.images ? pm.tr("Manga and comic sources") : pm.tr("Novel sources"); bold: true; size: 15 }
        Lbl {
            width: parent.width
            wrapMode: Text.Wrap
            dim: true
            size: 13
            text: pm.images ? pm.tr("MangaDex is built in; the rest comes from extensions. Install the ones you want (all of a language at once, if you like); each one adds a source to Explore.") : pm.tr("Install a plugin, then choose it in Explore. Plugins that need a login or a captcha in a browser may not work.")
        }
        Flow {
            width: parent.width
            spacing: 8
            LangButton { options: pm.itemLangs; selected: pm.langsOn; onChanged: function(list) { pm.set(pm.images ? "general.manga_langs" : "general.novel_langs", list) } }
            FlatButton {
                visible: pm.images
                kind: pm.g.show_nsfw ? "suggested" : "plain"
                implicitHeight: 34
                text: "18+"
                ToolTip.visible: hovered
                ToolTip.text: pm.tr("Show the sources for adults")
                onClicked: pm.set("general.show_nsfw", !pm.g.show_nsfw)
            }
            SearchEntry { width: 220; height: 34; implicitHeight: 34; placeholderText: pm.tr("Search sources"); onTextChanged: pm.query = text }
            FlatButton {
                visible: pm.images && pm.toInstall > 0
                kind: "suggested"
                implicitHeight: 34
                glyph: "download"
                text: pm.tr("Install all") + " (" + pm.toInstall + ")"
                onClicked: Core.cmd({cmd: "suwayomi_install_many", pkgs: pm.shown.filter(function(e) { return !e.installed }).map(function(e) { return e.pkg })})
            }
        }
        Rectangle {
            width: parent.width
            height: 460
            radius: 12
            color: Theme.c.card
            border.width: 1
            border.color: Theme.c.divider
            clip: true
            ListView {
                id: extList
                anchors.fill: parent
                // the number is the model, so installing one keeps the scroll where it is
                model: pm.page.length
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                delegate: Item {
                    required property int index
                    readonly property var e: pm.page[index] || ({})
                    width: ListView.view.width
                    height: 62
                    Rectangle { visible: index > 0; anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: 1; color: Theme.c.divider }
                    SourceIcon { id: eic; x: 12; anchors.verticalCenter: parent.verticalCenter; size: 38; url: e.icon || ""; name: e.name || "" }
                    Column {
                        anchors.left: eic.right; anchors.leftMargin: 12; anchors.right: eAct.left; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter
                        Lbl { width: parent.width; elide: Text.ElideRight; text: e.name || ""; bold: true; size: 15 }
                        Lbl { width: parent.width; elide: Text.ElideRight; text: (pm.images ? (e.langName || e.lang || "") : (e.lang || "")) + (e.nsfw ? "  ·  18+" : "") + (e.update ? "  ·  " + pm.tr("Update available") : ""); dim: true; size: 12 }
                    }
                    Row {
                        id: eAct
                        anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter
                        spacing: 6
                        Icon { visible: !!e.installed && !e.update; anchors.verticalCenter: parent.verticalCenter; name: "check"; size: 18; color: Theme.c.success }
                        FlatButton {
                            visible: !e.installed || !!e.update
                            kind: "suggested"; implicitHeight: 32
                            glyph: "download"
                            text: e.update ? pm.tr("Update") : pm.tr("Install")
                            onClicked: pm.images ? Core.cmd({cmd: "suwayomi_install", pkg: e.pkg}) : Core.cmd({cmd: "novel_install", id: e.id})
                        }
                        FlatButton {
                            visible: !!e.installed
                            kind: "flat"; glyph: "trash"; implicitWidth: 34; implicitHeight: 34
                            ToolTip.visible: hovered
                            ToolTip.text: pm.tr("Remove")
                            onClicked: pm.images ? Core.cmd({cmd: "suwayomi_uninstall", pkg: e.pkg}) : Core.cmd({cmd: "novel_remove_plugin", id: e.id})
                        }
                    }
                }
            }
            Lbl { visible: pm.page.length === 0; anchors.centerIn: parent; dim: true; text: pm.allItems.length === 0 ? (pm.images && pm.phoneLocal ? pm.tr("The extensions come from a Suwayomi server, which needs Java and cannot run on a phone. Put the address of a Suwayomi server running on a computer under Suwayomi server (Preferences). MangaDex works without it.") : pm.tr("Nothing yet. The list appears when the server is ready.")) : pm.tr("No source matches.") }
        }
    }
}
