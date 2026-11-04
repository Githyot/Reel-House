import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// Preferences (GTK prefs.rs, an adw::PreferencesDialog). As in the GTK version: General holds Backup, Metadata and
// Upscaling as sections; Addons and Accounts share a tab.
Popup {
    id: dlg
    // One settings centre for every mode. The modes on top (General = the whole app, then Video, Images, Text,
    // Study, and Addons & accounts, which is shared: one box adds the addons of any mode) switch the pages shown
    // below them; every page has its general options and, under "Advanced", the finer ones (general.prefs_advanced).
    readonly property string appMode: Core.manga.mode || "video"
    readonly property bool desktopAi: Qt.platform.os !== "android" && Qt.platform.os !== "ios"
    function tt(s) { return Core.tr(Core.uiLang, s) }
    readonly property var modes: [
        {id: "app", label: tt("General"), icon: "settings", pages: [
            {id: "look", label: tt("Appearance")}, {id: "net", label: tt("Connection & privacy")}, {id: "backup", label: tt("Backup")},
            {id: "controller", label: tt("Controller")}, {id: "translators", label: tt("Translators")}, {id: "system", label: tt("System")}]},
        {id: "video", label: tt("Video"), icon: "play", pages: [
            {id: "playback", label: tt("Playback")}, {id: "torrents", label: tt("Torrents")}, {id: "subtitles", label: tt("Subtitles & karaoke")}, {id: "shortcuts", label: tt("Shortcuts")}]},
        {id: "images", label: tt("Images"), icon: "image", pages: [{id: "img_read", label: tt("Reading")}, {id: "img_src", label: tt("Sources")}]},
        {id: "text", label: tt("Text"), icon: "list", pages: [{id: "txt_read", label: tt("Reading")}, {id: "txt_src", label: tt("Sources")}]},
        {id: "study", label: tt("Study"), icon: "study", pages: [{id: "study", label: tt("General")}, {id: "watch", label: tt("While watching")}]
            .concat(desktopAi ? [{id: "voice", label: tt("Microphone")}] : [])},
        {id: "addons", label: tt("Addons & accounts"), icon: "puzzle", pages: [{id: "addons", label: tt("Addons")}, {id: "accounts", label: tt("Accounts and keys")}]}
    ]
    property string modeSel: "app"
    property string pageName: "look"
    // the page last open in each mode, so going back to a mode shows it again
    property var lastPage: ({})
    function pagesOf(m) { for (let i = 0; i < modes.length; ++i) if (modes[i].id === m) return modes[i].pages; return modes[0].pages }
    function modeOf(page) { for (let i = 0; i < modes.length; ++i) if (modes[i].pages.some(function(x) { return x.id === page })) return modes[i].id; return "" }
    readonly property string cur: pagesOf(modeSel).some(function(x) { return x.id === pageName }) ? pageName : pagesOf(modeSel)[0].id
    // go to a mode (and one of its pages; "" = the one last open there)
    function go(m, page) {
        const list = pagesOf(m)
        const want = page !== "" ? page : (lastPage[m] || "")
        modeSel = m
        pageName = list.some(function(x) { return x.id === want }) ? want : list[0].id
        const lp = Object.assign({}, lastPage); lp[m] = pageName; lastPage = lp
        scroll.contentY = 0
    }
    // the "Advanced" part of every page: open or closed for all of them, kept in the settings
    property bool adv: false
    function toggleAdv() { adv = !adv; set("general.prefs_advanced", adv) }

    readonly property var p: Core.prefs
    readonly property var g: p.general !== undefined ? p.general : ({})
    readonly property var ex: p.extra !== undefined ? p.extra : ({})
    readonly property var infos: Core.infos
    parent: Overlay.overlay
    anchors.centerIn: parent
    // a phone: the whole screen (Material full-screen dialog, iOS full sheet), under the status bar
    readonly property real safeTop: Theme.mobile && Theme.safe !== undefined && Theme.safe.top !== undefined ? Theme.safe.top : 0
    width: Theme.mobile ? (parent ? parent.width : 400) : Math.min(760, (parent ? parent.width : 900) - 40)
    height: Theme.mobile ? (parent ? parent.height : 700) : (parent ? parent.height : 700) - 60
    modal: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    Overlay.modal: Rectangle { color: Theme.c.scrim }
    onAboutToShow: Core.refreshPrefs()
    onClosed: Core.cmd({cmd: "save_settings"})

    // name: a page or an old tab name ("general" opens the mode the app is in)
    function show(name) {
        adv = g.prefs_advanced === true
        const old = {accounts: "accounts", metadata: "accounts", addons: "addons", backup: "backup", upscaling: "playback", images: "img_read", text: "txt_read"}
        const fromApp = {video: "video", images: "images", text: "text", study: "study"}
        const page = old[name] || name || "general"
        if (page === "general") go(fromApp[appMode] || "app", "")
        else if (modeOf(page) !== "") go(modeOf(page), page)
        else go("app", "")
        open()
    }
    function set(path, value) { Core.cmd({cmd: "set_setting", path: path, value: value}) }
    // L1 / R1 of a controller: the previous or next page (past the last page of a mode, the next mode)
    function padTab(step) {
        const all = []
        modes.forEach(function(m) { m.pages.forEach(function(x) { all.push({m: m.id, p: x.id}) }) })
        const i = Math.max(0, all.findIndex(function(x) { return x.p === cur }))
        const n = all[(i + step + all.length) % all.length]
        go(n.m, n.p)
    }
    // a sound of Preferences > Controller (PrefsController)
    function pickSound(slot) { soundDlg.slot = slot; soundDlg.open() }
    // a sound pack: a .zip, or a folder, with nav.*, select.* and music.* (src/uisounds.rs)
    function importSoundPack(folder) { if (folder) soundPackDirDlg.open(); else soundPackDlg.open() }
    function localPath(u) { return decodeURIComponent(u.toString().replace(/^file:\/{2,3}/, Qt.platform.os === "windows" ? "" : "/")) }
    function sec(name) { return p[name] !== undefined ? p[name] : ({}) }
    function idx(ids, v) { var i = ids.indexOf("" + v); return i < 0 ? 0 : i }
    function info(kind) { return infos[kind] !== undefined ? infos[kind] : "" }
    function keyed(v) { return (v || "") !== "" }
    function openAccount(id) { acctDlg.svc = id; acctDlg.open() }
    // Logins and API keys: one button per service here, the fields in a window of their own (acctDlg).
    readonly property var services: {
        const rd = sec("debrid"), as = sec("anime_skip"), mal = sec("mal"), md = sec("metadata"), sb = sec("subtitles"), st = sec("general")
        const T = function(x) { return Core.tr(Core.uiLang, x) }
        const yes = T("Connected"), no = T("Not set")
        return [
            {id: "mal", name: "MyAnimeList", ok: ex.malLogged === true, state: ex.malLogged === true ? yes + " · " + (mal.user || "") : no},
            {id: "ankiweb", name: "AnkiWeb", ok: keyed(st.ankiweb_user) && keyed(st.ankiweb_hkey), state: keyed(st.ankiweb_hkey) ? yes + " · " + st.ankiweb_user : no},
            {id: "debrid", name: "Real-Debrid", ok: keyed(rd.api_key), state: keyed(rd.api_key) ? (rd.enabled ? yes : T("Token saved, turned off")) : no},
            {id: "animeskip", name: "Anime Skip", ok: keyed(as.client_id), state: keyed(as.token) ? yes + " · " + (as.email || "") : (keyed(as.client_id) ? T("Client ID saved") : no)},
            {id: "metadata", name: "TMDB / OMDb", ok: keyed(md.tmdb_key) || keyed(md.omdb_key), state: (keyed(md.tmdb_key) ? "TMDB ✓ " : "") + (keyed(md.omdb_key) ? "OMDb ✓" : "") || no},
            {id: "libretranslate", name: "LibreTranslate", ok: true, state: keyed(sb.libre_url) ? sb.libre_url : T("Public servers")},
            {id: "claude", name: "Claude (Anthropic)", ok: keyed(sb.claude_key), state: keyed(sb.claude_key) ? T("Key saved") : no},
            {id: "gemini", name: "Gemini (Google)", ok: keyed(sb.gemini_key), state: keyed(sb.gemini_key) ? T("Key saved") : no},
            {id: "openai", name: "GPT (OpenAI)", ok: keyed(sb.openai_key), state: keyed(sb.openai_key) ? T("Key saved") : no}
        ]
    }

    background: Glass { radius: Theme.mobile ? 0 : Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: !Theme.mobile }

    contentItem: Item {
        // ---- header: title, the modes (General, Video, Images, Text, Study, Addons & accounts), the pages of the mode, close ----
        Item {
            id: head
            width: parent.width
            height: 128 + dlg.safeTop
            Text { anchors.horizontalCenter: parent.horizontalCenter; y: 14 + dlg.safeTop; text: Core.tr(Core.uiLang, "Preferences"); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
            WinButton { icon: "close"; anchors { right: parent.right; rightMargin: 12 } y: 10 + dlg.safeTop; onClicked: dlg.close() }
            // When every label does not fit (six modes, longer Portuguese words), only the chosen mode shows its label;
            // the others show their icon and say their name on hover.
            FontMetrics { id: tabFont; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
            readonly property bool compactTabs: {
                let w = 0
                for (let i = 0; i < dlg.modes.length; ++i) w += tabFont.advanceWidth(dlg.modes[i].label) + 16 + 7 + 30 + 6
                return w > head.width - 40
            }
            // the modes: they scroll sideways when they do not fit (a phone)
            Flickable {
                id: tabFlick
                x: 8; y: 46 + dlg.safeTop
                width: parent.width - 16; height: 38
                contentWidth: tabRowItem.implicitWidth; contentHeight: height
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                flickableDirection: Flickable.HorizontalFlick
                interactive: contentWidth > width
                Row {
                    id: tabRowItem
                    x: Math.max(0, (tabFlick.width - implicitWidth) / 2)
                    spacing: 6
                    Repeater {
                        model: dlg.modes
                        delegate: Rectangle {
                            required property var modelData
                            width: tabRow.implicitWidth + 30
                            height: 38
                            radius: Theme.shape.control
                            color: dlg.modeSel === modelData.id ? Theme.c.selected : (tabArea.containsMouse ? Theme.c.hover : "transparent")
                            Row {
                                id: tabRow
                                anchors.centerIn: parent
                                spacing: 7
                                Icon { name: modelData.icon; size: 16; anchors.verticalCenter: parent.verticalCenter }
                                Text { visible: !head.compactTabs || dlg.modeSel === modelData.id; text: modelData.label; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                            }
                            MouseArea { id: tabArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: dlg.go(modelData.id, "") }
                            ToolTip.visible: head.compactTabs && tabArea.containsMouse && dlg.modeSel !== modelData.id
                            ToolTip.text: modelData.label
                        }
                    }
                }
            }
            // the pages of the chosen mode, as small pills
            Flickable {
                id: subFlick
                x: 8; y: 90 + dlg.safeTop
                width: parent.width - 16; height: 30
                contentWidth: subRowItem.implicitWidth; contentHeight: height
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                flickableDirection: Flickable.HorizontalFlick
                interactive: contentWidth > width
                Row {
                    id: subRowItem
                    x: Math.max(0, (subFlick.width - implicitWidth) / 2)
                    spacing: 4
                    Repeater {
                        model: dlg.pagesOf(dlg.modeSel)
                        delegate: Rectangle {
                            required property var modelData
                            width: subText.implicitWidth + 24
                            height: 30
                            radius: 15
                            color: dlg.cur === modelData.id ? Theme.c.selected : (subArea.containsMouse ? Theme.c.hover : "transparent")
                            border.width: dlg.cur === modelData.id ? 0 : 1
                            border.color: Theme.c.divider
                            Text { id: subText; anchors.centerIn: parent; text: modelData.label; color: dlg.cur === modelData.id ? Theme.c.text : Theme.c.textDim; font.pixelSize: 13; font.weight: dlg.cur === modelData.id ? Font.Bold : Font.Normal; font.family: Theme.type.family }
                            MouseArea { id: subArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: dlg.go(dlg.modeSel, modelData.id) }
                        }
                    }
                }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.c.divider }
        }


        Flickable {
            id: scroll
            anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom }
            contentWidth: width
            contentHeight: pages.implicitHeight + 60
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }

            // Every page is a child of this column; only the chosen one is visible, so the column is as tall as it.
            Column {
                id: pages
                x: Math.max(18, (scroll.width - 640) / 2)
                y: 22
                width: Math.min(640, scroll.width - 36)
                spacing: 0

                // ================= General: the whole app =================
                Column {
                    visible: dlg.cur === "look"
                    width: parent.width
                    spacing: 26
                    PrefGroup {
                        title: Core.tr(Core.uiLang, "Appearance")
                        // The style is the whole look of the app: layout, materials, shapes and type (qt/themes/*.json).
                        PrefCombo {
                            // first "Automatic": the theme of this device's system (HyperOS on a Xiaomi phone, Android, iOS, GNOME)
                            readonly property var styles: [{id: "", name: Core.tr(Core.uiLang, "Automatic")}].concat(Theme.list)
                            title: Core.tr(Core.uiLang, "Style"); subtitle: Core.tr(Core.uiLang, "The look of the whole app: GNOME (header bar) or Apple (floating sidebar, glass, large titles)")
                            options: styles.map(function(t) { return t.name })
                            current: Math.max(0, styles.map(function(t) { return t.id }).indexOf(dlg.g.theme || ""))
                            onPicked: function(i) {
                                dlg.set("general.theme", styles[i].id)
                                Theme.load(styles[i].id || Theme.systemId)
                                Theme.apply(dlg.g.color_scheme || "system", dlg.g.accent || "")
                            }
                        }
                        PrefCombo {
                            readonly property var ids: ["system", "light", "dark"]
                            title: Core.tr(Core.uiLang, "Color mode"); subtitle: Core.tr(Core.uiLang, "Light, dark, or the same as your system")
                            options: [Core.tr(Core.uiLang, "System"), Core.tr(Core.uiLang, "Light"), Core.tr(Core.uiLang, "Dark")]
                            current: dlg.idx(ids, dlg.g.color_scheme || "system")
                            onPicked: function(i) { dlg.set("general.color_scheme", ids[i]); Theme.apply(ids[i], dlg.g.accent || "") }
                        }
                        PrefRow {
                            title: Core.tr(Core.uiLang, "Accent colour"); subtitle: Core.tr(Core.uiLang, "Buttons, selections and highlights")
                            FlatButton {
                                kind: "flat"; text: Core.tr(Core.uiLang, "Auto"); implicitHeight: 30
                                anchors.verticalCenter: parent.verticalCenter
                                ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "The accent colour of your system")
                                onClicked: { dlg.set("general.accent", ""); Theme.apply(dlg.g.color_scheme || "system", "") }
                            }
                            Repeater {
                                model: dlg.ex.accents !== undefined ? dlg.ex.accents : []
                                delegate: Rectangle {
                                    required property var modelData
                                    width: 26; height: 26; radius: 13
                                    anchors.verticalCenter: parent.verticalCenter
                                    color: modelData.hex
                                    border.width: (dlg.g.accent || "").toLowerCase() === modelData.hex.toLowerCase() ? 3 : 0
                                    border.color: Theme.scheme === "light" ? "#b3000000" : "#b3ffffff"
                                    ToolTip.visible: swArea.containsMouse; ToolTip.text: modelData.name
                                    MouseArea { id: swArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { dlg.set("general.accent", modelData.hex); Theme.apply(dlg.g.color_scheme || "system", modelData.hex) } }
                                }
                            }
                            // Any colour: a colour dialog (the swatch shows the colour in use).
                            Rectangle {
                                width: 26; height: 26; radius: 13
                                anchors.verticalCenter: parent.verticalCenter
                                color: (dlg.g.accent || "") !== "" ? dlg.g.accent : "transparent"
                                border.width: 2
                                border.color: Theme.c.border
                                Text { anchors.centerIn: parent; text: "+"; color: Theme.c.text; font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family }
                                ToolTip.visible: anyArea.containsMouse; ToolTip.text: Core.tr(Core.uiLang, "Any colour")
                                MouseArea { id: anyArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: anyDlg.open() }
                                ColorPickerAdapter {
                                    id: anyDlg
                                    title: Core.tr(Core.uiLang, "Accent colour")
                                    selectedColor: (dlg.g.accent || "") !== "" ? dlg.g.accent : "#3584e4"
                                    onAccepted: {
                                        function p(v) { var s = Math.round(v * 255).toString(16); return s.length < 2 ? "0" + s : s }
                                        var h = "#" + p(selectedColor.r) + p(selectedColor.g) + p(selectedColor.b)
                                        dlg.set("general.accent", h)
                                        Theme.apply(dlg.g.color_scheme || "system", h)
                                    }
                                }
                            }
                        }
                    }

                    PrefGroup {
                        title: Core.tr(Core.uiLang, "Interface")
                        PrefSlider {
                            title: Core.tr(Core.uiLang, "Night filter (less blue light)")
                            subtitle: Core.tr(Core.uiLang, "Warms the whole app. The text reader has its own, which works only while you read")
                            from: 0; to: 100; step: 5
                            value: dlg.g.night_filter || 0
                            onMoved: function(v) { win.nightFilter = v }
                            onReleased: dlg.set("general.night_filter", Math.round(win.nightFilter))
                        }
                        PrefCombo {
                            readonly property var ids: ["system", "en", "pt", "es"]
                            title: Core.tr(Core.uiLang, "UI language")
                            subtitle: Core.tr(Core.uiLang, "Language of menus and buttons. Automatic follows the main language of Languages I know, then the system")
                            options: [Core.tr(Core.uiLang, "Automatic (main language)"), "English", "Português (Brasil)", "Español"]
                            current: Math.max(0, ids.indexOf(dlg.g.ui_language || dlg.g.language || "system"))
                            // "ui_language" is the key the core reads back (an old "language" is only read when it is missing)
                            onPicked: function(i) { dlg.set("general.ui_language", ids[i]) }
                        }
                        PrefSlider {
                            title: Core.tr(Core.uiLang, "Size in general")
                            subtitle: Core.tr(Core.uiLang, "Text and posters, from 50% to 200%. Applies at once")
                            from: 50; to: 200; step: 5
                            // `value` stays bound to the saved setting while dragging, so the dragged value is kept apart
                            // and saved on release (saving `value` itself wrote the old size back: the row did nothing).
                            property real dragged: -1
                            value: dlg.g.ui_scale || 120
                            onMoved: function(v) { dragged = v }
                            onReleased: { if (dragged >= 0) dlg.set("general.ui_scale", Math.round(dragged)); dragged = -1 }
                        }
                    }

                    // Languages the user reads (subtitle choice and automatic translation) and the one being learned.
                    PrefKnownLangs {
                        known: dlg.g.known_langs || []
                        fallback: dlg.sec("subtitles").target_lang || Core.uiLang || "en"
                        studyLang: dlg.g.study_lang || ""
                        onChangedList: function(list) { dlg.set("general.known_langs", list) }
                        onChangedStudy: function(code) { dlg.set("general.study_lang", code) }
                    }

                    PrefsAdvanced { dlg: dlg }
                    Column {
                        visible: dlg.adv
                        width: parent.width
                        spacing: 26
                        PrefGroup {
                            title: Core.tr(Core.uiLang, "More of the interface")
                            PrefSwitch { title: Core.tr(Core.uiLang, "Feedback button"); subtitle: Core.tr(Core.uiLang, "A button in the sidebar and in the player to write notes to the author (saved in Feedback.log)"); checked: dlg.g.feedback_button !== false; onToggled: function(on) { dlg.set("general.feedback_button", on) } }
                            PrefSwitch { title: Core.tr(Core.uiLang, "Animated background"); subtitle: Core.tr(Core.uiLang, "Colours taken from the cover move slowly behind the title, like in music apps"); checked: dlg.g.animated_bg !== false; onToggled: function(on) { dlg.set("general.animated_bg", on) } }
                            PrefSwitch { title: Core.tr(Core.uiLang, "Moving background"); subtitle: Core.tr(Core.uiLang, "The colours drift all the time. Looks nice but uses processor; off, they only change with the cover"); checked: dlg.g.aurora_motion === true; onToggled: function(on) { dlg.set("general.aurora_motion", on) } }
                        }
                    }
                }

                Column {
                    visible: dlg.cur === "net"
                    width: parent.width
                    spacing: 26
                    PrefGroup {
                        title: Core.tr(Core.uiLang, "Connection")
                        description: !(Core.net.network !== false) ? Core.tr(Core.uiLang, "No internet connection right now: what you downloaded still opens") : Core.tr(Core.uiLang, "Without internet the app opens what you downloaded and the pages read before")
                        PrefSwitch {
                            first: true
                            title: Core.tr(Core.uiLang, "Offline mode")
                            subtitle: Core.tr(Core.uiLang, "Use only what is on this computer (downloaded chapters, novels and videos), even with a connection")
                            checked: Core.net.forced === true
                            onToggled: function(on) { Core.cmd({cmd: "offline_mode", on: on}) }
                        }
                    }

                    // the "VPN inside the app": Tor, WireGuard, a proxy or the system VPN (src/privacy.rs)
                    PrefsPrivacy {}

                }

                Column {
                    visible: dlg.cur === "backup"
                    width: parent.width
                    spacing: 26
                    PrefGroup {
                        id: oneFileGrp
                        readonly property var b: dlg.sec("backup")
                        title: Core.tr(Core.uiLang, "Backup and Sync")
                        description: Core.tr(Core.uiLang, "One .rhb file with everything Reelhouse keeps: settings and logins, library and progress, addons, the study collection, knowledge, Learn progress and study time, browser history, page translations, reader edits, books, extensions and the pictures in use. Caches, thumbnails and downloaded videos are left out. It holds your keys, so keep it private. Copy it to another computer and restore it there to sync.")
                        PrefButton {
                            title: Core.tr(Core.uiLang, "Back up now")
                            subtitle: { var t = oneFileGrp.b.last || 0; return Core.tr(Core.uiLang, "Last backup") + ": " + (t > 0 ? new Date(t * 1000).toLocaleString(Qt.locale(), "yyyy-MM-dd HH:mm") : Core.tr(Core.uiLang, "Never")) }
                            label: Core.tr(Core.uiLang, "Back up now"); kind: "suggested"
                            onActivated: Core.cmd({cmd: "backup_now"})
                        }
                        PrefButton { title: Core.tr(Core.uiLang, "Export a backup"); subtitle: Core.tr(Core.uiLang, "Choose where to save the file"); label: Core.tr(Core.uiLang, "Export"); onActivated: { saveDlg.selectedFile = Qt.resolvedUrl("file:///" + (dlg.ex.backupDir || "").replace(/\\/g, "/").replace(/^\/+/, "") + "/" + (dlg.ex.backupName || "reelhouse-backup.rhb")); saveDlg.open() } }
                        PrefButton { title: Core.tr(Core.uiLang, "Restore a backup"); subtitle: Core.tr(Core.uiLang, "Replaces your current data with the file's. A copy of what you have now is saved first."); label: Core.tr(Core.uiLang, "Restore…"); onActivated: openDlg.open() }
                        PrefSwitch { title: Core.tr(Core.uiLang, "Include study media"); subtitle: Core.tr(Core.uiLang, "Pictures, sounds and clips of the study cards. Turn off for a much smaller file."); checked: oneFileGrp.b.media !== false; onToggled: function(on) { dlg.set("backup.media", on) } }
                        PrefRow { title: Core.tr(Core.uiLang, "Status"); subtitle: dlg.info("backup") }
                    }
                    PrefGroup {
                        id: backupGrp
                        readonly property var b: dlg.sec("backup")
                        readonly property var everyIds: ["1", "3", "6", "12", "24", "72", "168", "custom"]
                        property bool custom: everyIds.indexOf("" + (b.every_hours || 24)) < 0
                        title: Core.tr(Core.uiLang, "Automatic backup")
                        collapsible: true
                        description: Core.tr(Core.uiLang, "A copy is saved by itself while Reelhouse is open. The oldest copies are deleted.")
                        PrefSwitch { title: Core.tr(Core.uiLang, "Back up automatically"); checked: backupGrp.b.auto === true; onToggled: function(on) { dlg.set("backup.auto", on) } }
                        PrefCombo {
                            title: Core.tr(Core.uiLang, "How often")
                            options: [Core.tr(Core.uiLang, "Every hour"), Core.tr(Core.uiLang, "Every 3 hours"), Core.tr(Core.uiLang, "Every 6 hours"), Core.tr(Core.uiLang, "Every 12 hours"), Core.tr(Core.uiLang, "Every day"), Core.tr(Core.uiLang, "Every 3 days"), Core.tr(Core.uiLang, "Every week"), Core.tr(Core.uiLang, "Other number of hours…")]
                            current: backupGrp.custom ? 7 : dlg.idx(backupGrp.everyIds, backupGrp.b.every_hours || 24)
                            onPicked: function(i) {
                                var grp = backupGrp
                                if (i === 7) grp.custom = true
                                else { grp.custom = false; dlg.set("backup.every_hours", parseInt(grp.everyIds[i])) }
                            }
                        }
                        PrefSpin { visible: backupGrp.custom; title: Core.tr(Core.uiLang, "Hours between backups"); from: 1; to: 720; value: backupGrp.b.every_hours || 24; onMoved: function(v) { dlg.set("backup.every_hours", v) } }
                        PrefSpin { title: Core.tr(Core.uiLang, "Copies to keep"); from: 1; to: 100; value: backupGrp.b.keep || 10; onMoved: function(v) { dlg.set("backup.keep", v) } }
                        PrefRow {
                            title: Core.tr(Core.uiLang, "Folder"); subtitle: dlg.ex.backupDir || ""; selectable: true
                            FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flat"; glyph: "undo"; implicitWidth: 34; implicitHeight: 34; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Use the default folder"); onClicked: dlg.set("backup.folder", "") }
                            FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Choose"); implicitHeight: 34; onClicked: folderDlg.open() }
                        }
                    }
                    PrefsAdvanced { dlg: dlg }
                    Column {
                        visible: dlg.adv
                        width: parent.width
                        spacing: 26
                        // The copy online: cloud → how it is reached (sync folder of the desktop app, or an account) → status.
                        PrefGroup {
                            id: cloudGrp
                            readonly property var c: dlg.sec("backup").cloud || ({})
                            readonly property var st: dlg.ex.cloud || ({})
                            readonly property var ids: ["gdrive", "icloud", "onedrive", "github"]
                            readonly property string prov: c.provider || "gdrive"
                            readonly property bool acct: c.method === "account" && prov !== "icloud"
                            readonly property var pend: st.pending || null
                            readonly property bool gh: prov === "github"
                            readonly property var ghs: st.github || ({})
                            readonly property var remote: st.remote || []
                            function human(b) {
                                var u = ["B", "KB", "MB", "GB", "TB"], i = 0
                                while (b >= 1024 && i < u.length - 1) { b /= 1024; ++i }
                                return (i === 0 ? b : b.toFixed(1)) + " " + u[i]
                            }
                            function when(t) { return t > 0 ? new Date(t * 1000).toLocaleString(Qt.locale(), "yyyy-MM-dd HH:mm") : Core.tr(Core.uiLang, "Never") }
                            title: Core.tr(Core.uiLang, "Online copy")
                            collapsible: true
                            expanded: c.enabled === true
                            description: Core.tr(Core.uiLang, "Keep a copy of every backup in your cloud: choose the cloud, then how Reelhouse reaches it.")
                            PrefSwitch { title: Core.tr(Core.uiLang, "Keep a copy online"); subtitle: Core.tr(Core.uiLang, "Every backup, automatic or by hand, is also sent to the cloud"); checked: cloudGrp.c.enabled === true; onToggled: function(on) { dlg.set("backup.cloud.enabled", on) } }
                            PrefCombo {
                                title: Core.tr(Core.uiLang, "Cloud")
                                options: ["Google Drive", "iCloud Drive", "OneDrive", "GitHub"]
                                current: Math.max(0, cloudGrp.ids.indexOf(cloudGrp.prov))
                                onPicked: function(i) { dlg.set("backup.cloud.provider", cloudGrp.ids[i]) }
                            }
                            PrefCombo {
                                visible: cloudGrp.prov !== "icloud" && !cloudGrp.gh
                                title: Core.tr(Core.uiLang, "Account access")
                                options: [Core.tr(Core.uiLang, "Sync folder of the desktop app"), Core.tr(Core.uiLang, "Sign in with a code")]
                                current: cloudGrp.acct ? 1 : 0
                                onPicked: function(i) { dlg.set("backup.cloud.method", i === 1 ? "account" : "folder") }
                            }
                            PrefRow {
                                visible: cloudGrp.prov === "icloud"
                                title: Core.tr(Core.uiLang, "Only through the sync folder")
                                subtitle: Core.tr(Core.uiLang, "Apple offers no iCloud Drive API for Windows programs. Install iCloud for Windows and turn on iCloud Drive: Reelhouse copies the backup into its folder and iCloud uploads it.")
                            }
                            // ---- sync folder ----
                            PrefRow {
                                visible: !cloudGrp.acct
                                title: Core.tr(Core.uiLang, "Sync folder")
                                subtitle: cloudGrp.st.folder ? cloudGrp.st.folder
                                          : Qt.platform.os === "linux" && cloudGrp.prov !== "icloud" ? Core.tr(Core.uiLang, "Not found. On Linux, mount the cloud with rclone and press Detect, or choose the folder.")
                                          : Core.tr(Core.uiLang, "Not found. Install the cloud's desktop app and sign in there, or choose the folder.")
                                selectable: true
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flat"; glyph: "undo"; implicitWidth: 34; implicitHeight: 34; visible: (cloudGrp.c.folder || "") !== ""; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Use the folder found"); onClicked: dlg.set("backup.cloud.folder", "") }
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Detect"); implicitHeight: 34; onClicked: Core.cmd({cmd: "cloud_detect"}) }
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Choose"); implicitHeight: 34; onClicked: cloudFolderDlg.open() }
                            }
                            // Linux: a plain folder is not synced by anything (no Google Drive / OneDrive app there)
                            PrefRow {
                                visible: !cloudGrp.acct && !cloudGrp.gh && cloudGrp.st.folderSynced === false
                                title: Core.tr(Core.uiLang, "This folder is not in the cloud")
                                subtitle: Core.tr(Core.uiLang, "Linux has no Google Drive or OneDrive app, so nothing sends this folder to the cloud: the copies stay on this computer. Mount the cloud with rclone (or GNOME Online Accounts) and choose a folder inside it, or use \"Sign in with a code\".")
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "How to"); implicitHeight: 34; onClicked: Qt.openUrlExternally(cloudGrp.prov === "onedrive" ? "https://rclone.org/onedrive/" : "https://rclone.org/drive/") }
                            }
                            // ---- GitHub: a personal access token and a private repository (cloud_github.rs) ----
                            PrefRow {
                                visible: cloudGrp.gh
                                title: Core.tr(Core.uiLang, "Personal access token (PAT)")
                                subtitle: Core.tr(Core.uiLang, "On GitHub make a fine-grained token with \"Contents: read and write\" on the backup repository (add \"Administration: read and write\" so Reelhouse can create it), or a classic token with the \"repo\" scope. The backups go to a private repository, one release per backup.")
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Open"); implicitHeight: 34; onClicked: Qt.openUrlExternally(cloudGrp.st.registerUrl || "") }
                            }
                            PrefEntry {
                                id: ghTok
                                visible: cloudGrp.gh
                                title: cloudGrp.ghs.connected === true ? Core.tr(Core.uiLang, "New token (empty keeps the one connected)") : Core.tr(Core.uiLang, "Token")
                                password: true
                            }
                            PrefEntry {
                                id: ghRepo
                                visible: cloudGrp.gh
                                first: false
                                title: Core.tr(Core.uiLang, "Repository (empty: %1)").arg(cloudGrp.ghs.defaultRepo || "reelhouse-backup")
                                text: cloudGrp.ghs.repo || ""
                            }
                            PrefButton {
                                readonly property bool same: ghTok.value.trim() === "" && ghRepo.value.trim() === (cloudGrp.ghs.repo || "")
                                readonly property bool linked: cloudGrp.ghs.connected === true
                                visible: cloudGrp.gh
                                highlight: linked
                                title: linked ? Core.tr(Core.uiLang, "Connected") : Core.tr(Core.uiLang, "GitHub account")
                                subtitle: linked ? (cloudGrp.ghs.account || "") + " · " + (cloudGrp.ghs.repo || "") : Core.tr(Core.uiLang, "Not connected")
                                label: cloudGrp.st.busy === "signin" ? Core.tr(Core.uiLang, "Checking…") : (linked && same ? Core.tr(Core.uiLang, "Disconnect") : Core.tr(Core.uiLang, "Connect"))
                                kind: linked && same ? "plain" : "suggested"
                                enabled: (cloudGrp.st.busy || "") === ""
                                onActivated: {
                                    if (linked && same) {
                                        Core.cmd({cmd: "cloud_sign_out"})
                                    } else {
                                        Core.cmd({cmd: "cloud_github_connect", token: ghTok.value, repo: ghRepo.value})
                                        ghTok.clear()
                                    }
                                }
                            }
                            // ---- account (device code sign-in) ----
                            PrefRow {
                                visible: cloudGrp.acct && !cloudGrp.gh
                                title: Core.tr(Core.uiLang, "Your app's client ID")
                                subtitle: cloudGrp.prov === "onedrive"
                                    ? Core.tr(Core.uiLang, "Register a free app at Microsoft Entra (App registrations): any name, account type \"personal Microsoft accounts\" (or any), then in Authentication turn on \"Allow public client flows\". Add the API permission Files.ReadWrite.AppFolder. Paste its Application (client) ID here.")
                                    : Core.tr(Core.uiLang, "In Google Cloud Console make a project, turn on the Google Drive API, set up the consent screen (add yourself as a test user), then create an OAuth client of type \"TVs and Limited Input devices\". Paste its client ID and secret here.")
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Open"); implicitHeight: 34; onClicked: Qt.openUrlExternally(cloudGrp.st.registerUrl || "") }
                            }
                            PrefEntry {
                                visible: cloudGrp.acct && !cloudGrp.gh
                                title: Core.tr(Core.uiLang, "Client ID")
                                text: cloudGrp.prov === "onedrive" ? (cloudGrp.c.ms_client_id || "") : (cloudGrp.c.google_client_id || "")
                                onEdited: function(t) { dlg.set(cloudGrp.prov === "onedrive" ? "backup.cloud.ms_client_id" : "backup.cloud.google_client_id", t.trim()) }
                            }
                            PrefEntry {
                                visible: cloudGrp.acct && cloudGrp.prov === "gdrive"
                                title: Core.tr(Core.uiLang, "Client secret")
                                password: true
                                text: cloudGrp.c.google_client_secret || ""
                                onEdited: function(t) { dlg.set("backup.cloud.google_client_secret", t.trim()) }
                            }
                            PrefButton {
                                visible: cloudGrp.acct && !cloudGrp.pend && !cloudGrp.gh
                                highlight: cloudGrp.st.signedIn === true
                                title: cloudGrp.st.signedIn === true ? Core.tr(Core.uiLang, "Signed in") : Core.tr(Core.uiLang, "Account")
                                subtitle: cloudGrp.st.signedIn === true ? (cloudGrp.st.account || cloudGrp.st.providerName || "") : Core.tr(Core.uiLang, "Not signed in")
                                label: cloudGrp.st.signedIn === true ? Core.tr(Core.uiLang, "Sign out") : Core.tr(Core.uiLang, "Sign in")
                                kind: cloudGrp.st.signedIn === true ? "plain" : "suggested"
                                onActivated: Core.cmd({cmd: cloudGrp.st.signedIn === true ? "cloud_sign_out" : "cloud_sign_in"})
                            }
                            PrefRow {
                                visible: cloudGrp.acct && cloudGrp.pend !== null
                                highlight: true
                                title: Core.tr(Core.uiLang, "Type this code on the sign-in page") + ": " + (cloudGrp.pend ? cloudGrp.pend.code : "")
                                subtitle: cloudGrp.pend ? cloudGrp.pend.url : ""
                                selectable: true
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Open page"); kind: "suggested"; implicitHeight: 34; onClicked: Qt.openUrlExternally(cloudGrp.pend ? cloudGrp.pend.url : "") }
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Cancel"); implicitHeight: 34; onClicked: Core.cmd({cmd: "cloud_cancel"}) }
                            }
                            // ---- status, upload and the backups online ----
                            PrefSpin { title: Core.tr(Core.uiLang, "Copies kept online"); from: 1; to: 50; value: cloudGrp.c.keep || 5; onMoved: function(v) { dlg.set("backup.cloud.keep", v) } }
                            PrefButton {
                                title: Core.tr(Core.uiLang, "Last upload")
                                subtitle: (cloudGrp.c.last_error || "") !== ""
                                    ? Core.tr(Core.uiLang, "Failed") + ": " + cloudGrp.c.last_error
                                    : cloudGrp.when(cloudGrp.c.last_upload || 0) + ((cloudGrp.c.last_upload || 0) > 0 ? " · " + (cloudGrp.c.last_name || "") + " · " + cloudGrp.human(cloudGrp.c.last_size || 0) : "")
                                label: cloudGrp.st.busy === "upload" ? Core.tr(Core.uiLang, "Uploading…") : Core.tr(Core.uiLang, "Upload now")
                                enabled: (cloudGrp.st.busy || "") === ""
                                onActivated: Core.cmd({cmd: "cloud_upload"})
                            }
                            PrefRow { visible: (cloudGrp.st.status || "") !== ""; title: Core.tr(Core.uiLang, "Status"); subtitle: Core.tr(Core.uiLang, cloudGrp.st.status || "") }
                            PrefButton {
                                title: Core.tr(Core.uiLang, "Backups online")
                                subtitle: cloudGrp.st.listed === true ? (cloudGrp.remote.length > 0 ? Core.tr(Core.uiLang, "Pick one to restore it") : Core.tr(Core.uiLang, "None yet")) : Core.tr(Core.uiLang, "Read the list to restore one of them")
                                label: cloudGrp.st.listed === true ? Core.tr(Core.uiLang, "Refresh") : Core.tr(Core.uiLang, "Show")
                                enabled: (cloudGrp.st.busy || "") === ""
                                onActivated: Core.cmd({cmd: "cloud_list"})
                            }
                            Repeater {
                                model: cloudGrp.remote.length
                                delegate: PrefButton {
                                    required property int index
                                    readonly property var entry: cloudGrp.remote[index] || ({})
                                    title: entry.name || ""
                                    subtitle: cloudGrp.when(entry.modified || 0) + " · " + cloudGrp.human(entry.size || 0)
                                    label: Core.tr(Core.uiLang, "Restore…")
                                    enabled: (cloudGrp.st.busy || "") === ""
                                    onActivated: { restoreAsk.remoteId = entry.id || ""; restoreAsk.remoteName = entry.name || ""; restoreAsk.open() }
                                }
                            }
                        }


                    }
                }

                PrefsController { id: controllerPage; visible: dlg.cur === "controller"; dlg: dlg; width: parent.width }

                PrefsTranslators { id: translatorsPage; visible: dlg.cur === "translators"; dlg: dlg; width: parent.width }

                Column {
                    visible: dlg.cur === "system"
                    width: parent.width
                    spacing: 26
                    // Updates of the app from the GitHub Releases (controller_update.rs, updater.rs).
                    PrefGroup {
                        id: updGrp
                        readonly property var u: dlg.ex.update || ({})
                        readonly property string st: u.state || ""
                        readonly property bool busy: st === "checking" || st === "downloading" || st === "installing"
                        readonly property bool has: (u.tag || "") !== ""
                        function mb(b) { return (b / 1048576).toFixed(b >= 104857600 ? 0 : 1) + " MB" }
                        title: Core.tr(Core.uiLang, "Updates")
                        description: Core.tr(Core.uiLang, "New versions come from the GitHub releases of Reelhouse. Your data (lists, progress, decks, settings) is kept in its own folder and is never replaced.")
                        PrefButton {
                            title: Core.tr(Core.uiLang, "Version %1").arg(updGrp.u.current || "")
                            subtitle: {
                                const T = function(x) { return Core.tr(Core.uiLang, x) }
                                if (updGrp.st === "checking") return T("Looking for updates…")
                                if (updGrp.st === "current") return T("You have the newest version.")
                                if (updGrp.st === "error") return T(updGrp.u.error || "")
                                if (updGrp.has) return T("Version %1 is available").arg(updGrp.u.tag)
                                return ""
                            }
                            label: updGrp.st === "checking" ? Core.tr(Core.uiLang, "Looking…") : Core.tr(Core.uiLang, "Check for updates")
                            enabled: !updGrp.busy
                            onActivated: Core.cmd({cmd: "update_check"})
                        }
                        PrefRow {
                            visible: updGrp.has
                            title: updGrp.u.name || updGrp.u.tag || ""
                            subtitle: {
                                const n = (updGrp.u.notes || "").trim()
                                return n === "" ? Core.tr(Core.uiLang, "No release notes.") : (n.length > 1500 ? n.substring(0, 1500) + "…" : n)
                            }
                            selectable: true
                            FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flat"; text: Core.tr(Core.uiLang, "Open"); implicitHeight: 34; onClicked: Qt.openUrlExternally(updGrp.u.page || "") }
                        }
                        PrefButton {
                            visible: updGrp.has
                            title: updGrp.u.canInstall === true ? Core.tr(Core.uiLang, "Download and install") : Core.tr(Core.uiLang, "Open the release page")
                            subtitle: {
                                const T = function(x) { return Core.tr(Core.uiLang, x) }
                                if (updGrp.st === "installing") return T("Reelhouse closes, the new version is copied and it opens again.")
                                if (updGrp.st === "opened") return T("The installer was opened. If nothing opened, install it by hand:") + "\n" + (updGrp.u.note || "")
                                if (updGrp.u.platform === "windows") return T("Reelhouse downloads the new version, closes, replaces its files (keeping a copy of the old ones) and opens again.")
                                if (updGrp.u.canInstall === true) return T("The package is saved in Downloads and opened with the system installer.")
                                return T("The release page opens: download and install the file of your device.")
                            }
                            label: updGrp.st === "downloading" ? Core.tr(Core.uiLang, "Cancel") : (updGrp.u.canInstall === true ? Core.tr(Core.uiLang, "Install") : Core.tr(Core.uiLang, "Open"))
                            kind: updGrp.st === "downloading" ? "plain" : "suggested"
                            enabled: updGrp.st !== "installing" && updGrp.st !== "checking"
                            onActivated: Core.cmd({cmd: updGrp.st === "downloading" ? "update_cancel" : "update_install"})
                        }
                        PrefRow {
                            visible: updGrp.st === "downloading"
                            title: Core.tr(Core.uiLang, "Downloading…")
                            subtitle: updGrp.mb(updGrp.u.done || 0) + " / " + updGrp.mb(updGrp.u.total || updGrp.u.size || 0)
                            ProgressBar {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 200
                                from: 0; to: Math.max(1, updGrp.u.total || updGrp.u.size || 1)
                                value: updGrp.u.done || 0
                            }
                        }
                        PrefSwitch {
                            title: Core.tr(Core.uiLang, "Check when the app opens")
                            subtitle: updGrp.u.tokenSet === true ? Core.tr(Core.uiLang, "Once, in the background; a notice shows when there is a new version") : Core.tr(Core.uiLang, "Once, in the background. While the repository is private, it needs the GitHub token of Backup > Online copy")
                            checked: dlg.g.update_check !== false
                            onToggled: function(on) { dlg.set("general.update_check", on) }
                        }
                    }

                    PrefsAdvanced { dlg: dlg }
                    Column {
                        visible: dlg.adv
                        width: parent.width
                        spacing: 26
                        PrefGroup {
                            title: Core.tr(Core.uiLang, "Diagnostics")
                            collapsible: true
                            description: Core.tr(Core.uiLang, "If something is slow or does not work, turn this on, repeat the problem for a minute, then send the report file.")
                            PrefSwitch { title: Core.tr(Core.uiLang, "Debug report"); subtitle: Core.tr(Core.uiLang, "Always on: writes what the app does and how long each part takes (processor use, network, player, Suwayomi, karaoke) to a text file you can send"); checked: true; enabled: false }
                            PrefRow {
                                title: Core.tr(Core.uiLang, "Report file"); subtitle: dlg.ex.debugFile || ""; selectable: true
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Open folder"); implicitHeight: 34; onClicked: Qt.openUrlExternally("file:///" + (dlg.ex.debugDir || "").replace(/\\/g, "/")) }
                            }
                        }

                        PrefGroup {
                            title: Core.tr(Core.uiLang, "This installation")
                            collapsible: true
                            PrefRow { title: Core.tr(Core.uiLang, "Data folder"); subtitle: dlg.ex.dataDir || ""; selectable: true }
                            PrefRow { title: Core.tr(Core.uiLang, "Player"); subtitle: dlg.ex.player || "" }
                            PrefRow { title: Core.tr(Core.uiLang, "Torrent engine"); subtitle: dlg.ex.engine || "" }
                        }

                    }
                }

                // ================= Video =================
                Column {
                    visible: dlg.cur === "playback"
                    width: parent.width
                    spacing: 26
                    PrefGroup {
                        title: Core.tr(Core.uiLang, "Playback")
                        PrefCombo {
                            readonly property var ids: ["ask", "best"]
                            title: Core.tr(Core.uiLang, "When I choose a title")
                            options: [Core.tr(Core.uiLang, "Let me choose the torrent"), Core.tr(Core.uiLang, "Start the best stream")]
                            current: dlg.idx(ids, dlg.g.pick_mode)
                            onPicked: function(i) { dlg.set("general.pick_mode", ids[i]) }
                        }
                        PrefCombo {
                            readonly property var ids: ["480", "720", "1080", "2160"]
                            title: Core.tr(Core.uiLang, "Preferred quality"); subtitle: Core.tr(Core.uiLang, "Used to rank the streams")
                            options: ["480p", "720p", "1080p", "4K"]
                            current: dlg.idx(ids, dlg.g.preferred_height)
                            onPicked: function(i) { dlg.set("general.preferred_height", parseInt(ids[i])) }
                        }
                        PrefSwitch { title: Core.tr(Core.uiLang, "Play the next episode by itself"); subtitle: Core.tr(Core.uiLang, "When an episode ends, the next one starts"); checked: dlg.g.autoplay_next === true; onToggled: function(on) { dlg.set("general.autoplay_next", on) } }
                        PrefSpin { title: Core.tr(Core.uiLang, "Next episode card"); subtitle: Core.tr(Core.uiLang, "Seconds before the end when the card appears"); from: 5; to: 120; step: 5; value: dlg.g.next_popup_secs || 30; onMoved: function(v) { dlg.set("general.next_popup_secs", v) } }
                        PrefSwitch { title: Core.tr(Core.uiLang, "Preview pictures"); subtitle: Core.tr(Core.uiLang, "Show a small picture above the seek bar while hovering it"); checked: dlg.g.thumbnails === true; onToggled: function(on) { dlg.set("general.thumbnails", on) } }
                        PrefSpin { title: Core.tr(Core.uiLang, "Skip button"); subtitle: Core.tr(Core.uiLang, "Seconds the Skip button in the player jumps forward"); from: 5; to: 300; step: 5; value: dlg.g.skip_seconds || 90; onMoved: function(v) { dlg.set("general.skip_seconds", v) } }
                        PrefSwitch { title: Core.tr(Core.uiLang, "Blur unwatched episodes"); subtitle: Core.tr(Core.uiLang, "Hides the pictures of episodes you have not watched, so they do not spoil the story"); checked: dlg.g.blur_unwatched === true; onToggled: function(on) { dlg.set("general.blur_unwatched", on) } }
                    }
                    PrefsAdvanced { dlg: dlg }
                    Column {
                        visible: dlg.adv
                        width: parent.width
                        spacing: 26
                        PrefGroup {
                            title: Core.tr(Core.uiLang, "Playback (advanced)")
                            PrefCombo {
                                readonly property var ids: ["auto-copy", "auto", "d3d11va", "vaapi", "no"]
                                title: Core.tr(Core.uiLang, "Hardware decoding"); subtitle: Core.tr(Core.uiLang, "“auto-copy” works on most computers.")
                                options: ["auto-copy", "auto", "d3d11va (Windows)", "vaapi (Linux)", Core.tr(Core.uiLang, "Off")]
                                current: dlg.idx(ids, dlg.g.hwdec)
                                onPicked: function(i) { dlg.set("general.hwdec", ids[i]) }
                            }
                            PrefSpin { title: Core.tr(Core.uiLang, "Buffer before playing"); subtitle: Core.tr(Core.uiLang, "Seconds of video loaded first, while subtitles are prepared"); from: 3; to: 60; value: dlg.g.buffer_seconds || 10; onMoved: function(v) { dlg.set("general.buffer_seconds", v) } }
                            PrefEntry { first: false; title: Core.tr(Core.uiLang, "Rule for the stream that plays by itself (optional)"); placeholder: Core.tr(Core.uiLang, "e.g. 1080p.*(web|bluray). Empty: the best ranked, keeping the same release"); text: dlg.g.autoplay_rule || ""; onEdited: function(t) { dlg.set("general.autoplay_rule", t.trim()) } }
                        }
                        PrefGroup {
                            id: shaderGrp
                            readonly property bool ok: (dlg.ex.shadersTotal || 0) > 0 && dlg.ex.shadersPresent === dlg.ex.shadersTotal
                            title: Core.tr(Core.uiLang, "Anime4K")
                            collapsible: true
                            description: Core.tr(Core.uiLang, "Real-time upscaling shaders for anime, by bloc97. They run on your graphics card while mpv plays.")
                            PrefButton {
                                title: Core.tr(Core.uiLang, "Shaders")
                                subtitle: dlg.info("shaders") !== "" ? dlg.info("shaders") : (dlg.ex.shadersPresent || 0) + " of " + (dlg.ex.shadersTotal || 0) + " files installed"
                                label: shaderGrp.ok ? Core.tr(Core.uiLang, "Reinstall") : Core.tr(Core.uiLang, "Download")
                                kind: shaderGrp.ok ? "plain" : "suggested"
                                onActivated: Core.cmd({cmd: "download_shaders"})
                            }
                        }
                        PrefGroup {
                            id: upGrp
                            readonly property var u: dlg.sec("upscaling")
                            title: Core.tr(Core.uiLang, "Upscaling")
                            collapsible: true
                            PrefSwitch { title: Core.tr(Core.uiLang, "Turn on Anime4K"); checked: upGrp.u.enabled === true; onToggled: function(on) { dlg.set("upscaling.enabled", on) } }
                            PrefCombo {
                                title: Core.tr(Core.uiLang, "Mode"); subtitle: dlg.ex.modeInfo || ""
                                options: (dlg.ex.modes || []).map(function(m) { return Core.tr(Core.uiLang, "Mode") + " " + m })
                                current: Math.max(0, (dlg.ex.modes || []).indexOf(upGrp.u.mode))
                                onPicked: function(i) { dlg.set("upscaling.mode", dlg.ex.modes[i]) }
                            }
                            PrefCombo {
                                readonly property var ids: ["HQ", "Fast"]
                                title: Core.tr(Core.uiLang, "Quality")
                                options: [Core.tr(Core.uiLang, "High quality (strong graphics card)"), Core.tr(Core.uiLang, "Fast (weaker graphics card)")]
                                current: dlg.idx(ids, upGrp.u.quality)
                                onPicked: function(i) { dlg.set("upscaling.quality", ids[i]) }
                            }
                            PrefSwitch { title: Core.tr(Core.uiLang, "Clamp highlights"); subtitle: Core.tr(Core.uiLang, "Reduces ringing around bright edges"); checked: upGrp.u.clamp_highlights === true; onToggled: function(on) { dlg.set("upscaling.clamp_highlights", on) } }
                            PrefSpin { title: Core.tr(Core.uiLang, "Skip videos taller than"); subtitle: Core.tr(Core.uiLang, "Pixels. Large videos gain little from upscaling."); from: 360; to: 2160; step: 120; value: upGrp.u.max_source_height || 1080; onMoved: function(v) { dlg.set("upscaling.max_source_height", v) } }
                            PrefSwitch { title: Core.tr(Core.uiLang, "Use the gpu-hq profile"); subtitle: Core.tr(Core.uiLang, "Better scaling quality, uses more of the graphics card"); checked: upGrp.u.gpu_hq === true; onToggled: function(on) { dlg.set("upscaling.gpu_hq", on) } }
                            PrefEntry { title: Core.tr(Core.uiLang, "Scaler"); text: upGrp.u.scaler || ""; onEdited: function(t) { dlg.set("upscaling.scaler", t.trim()) } }
                        }
                        PrefGroup { title: Core.tr(Core.uiLang, "Shortcuts in the player"); description: Core.tr(Core.uiLang, "Ctrl+1 to Ctrl+6 pick a mode. Ctrl+0 turns Anime4K off.") }
                    }
                }

                PrefsTorrents { id: torrentsPage; visible: dlg.cur === "torrents"; g: dlg.g; width: parent.width }

                PrefsSubtitles { id: subsPage; visible: dlg.cur === "subtitles"; dlg: dlg; width: parent.width }

                PrefsShortcuts { id: keysPage; visible: dlg.cur === "shortcuts"; dlg: dlg; width: parent.width }

                // ================= Images and Text: reading, then the sources =================
                PrefsMode { visible: dlg.cur === "img_read"; dlg: dlg; kind: "images"; part: "read"; width: parent.width }

                PrefsMode { visible: dlg.cur === "img_src"; dlg: dlg; kind: "images"; part: "sources"; width: parent.width }

                PrefsMode { visible: dlg.cur === "txt_read"; dlg: dlg; kind: "text"; part: "read"; width: parent.width }

                PrefsMode { visible: dlg.cur === "txt_src"; dlg: dlg; kind: "text"; part: "sources"; width: parent.width }

                // ================= Study =================
                Column {
                    visible: dlg.cur === "study"
                    width: parent.width
                    spacing: 26
                    PrefGroup {
                        title: Core.tr(Core.uiLang, "Study")
                        description: Core.tr(Core.uiLang, "Decks, AnkiWeb and the cards shown while you watch")
                        PrefSwitch { first: true; title: Core.tr(Core.uiLang, "Unlimited hearts"); subtitle: Core.tr(Core.uiLang, "Learn lessons: mistakes never take hearts or stop a lesson"); checked: dlg.g.learn_unlimited_hearts === true; onToggled: function(on) { dlg.set("general.learn_unlimited_hearts", on) } }
                        PrefButton { title: Core.tr(Core.uiLang, "Where to start in Learn"); subtitle: Core.tr(Core.uiLang, "Lost your progress? Pick the unit and lesson where you were and the path picks up there"); label: Core.tr(Core.uiLang, "Choose…"); onActivated: learnStart.open() }
                        PrefButton { title: "AnkiWeb"; subtitle: dlg.services[1].state; label: Core.tr(Core.uiLang, "Open"); onActivated: dlg.openAccount("ankiweb") }
                    }
                    // The macaw's pop-ups at the top of the window (notify_study.rs, BannerHost.qml)
                    PrefGroup {
                        title: Core.tr(Core.uiLang, "Reminders")
                        description: Core.tr(Core.uiLang, "Arara reminds you of your streak, daily goal, quests and cards due, at most a few times a day")
                        PrefSwitch { first: true; title: Core.tr(Core.uiLang, "In-app notifications"); subtitle: Core.tr(Core.uiLang, "Pop-ups with Arara at the top of the window"); checked: dlg.g.banners !== false; onToggled: function(on) { dlg.set("general.banners", on) } }
                        PrefSpin { visible: dlg.g.banners !== false; title: Core.tr(Core.uiLang, "Quiet from"); subtitle: Core.tr(Core.uiLang, "Hour when the reminders stop"); from: 0; to: 23; value: dlg.g.banner_quiet_from !== undefined ? dlg.g.banner_quiet_from : 22; onMoved: function(v) { dlg.set("general.banner_quiet_from", v) } }
                        PrefSpin { visible: dlg.g.banners !== false; title: Core.tr(Core.uiLang, "Quiet until"); subtitle: Core.tr(Core.uiLang, "Hour when they start again (the same hour on both: never quiet)"); from: 0; to: 23; value: dlg.g.banner_quiet_to !== undefined ? dlg.g.banner_quiet_to : 8; onMoved: function(v) { dlg.set("general.banner_quiet_to", v) } }
                        PrefButton { visible: dlg.g.banners !== false; title: Core.tr(Core.uiLang, "See an example"); label: Core.tr(Core.uiLang, "Show"); onActivated: Core.cmd({cmd: "banner_test", kind: ["streak", "goal", "review", "quest", "idle"][Math.floor(Math.random() * 5)]}) }
                    }

                    PrefsAdvanced { dlg: dlg }
                    Column {
                        visible: dlg.adv
                        width: parent.width
                        spacing: 26
                        PrefGroup {
                            id: acGrp
                            title: "Autocards"
                            collapsible: true
                            expanded: false
                            description: Core.tr(Core.uiLang, "Sentence mining into your Anki (learnjapanese.moe/autocards): make the word card with Yomitan, then press Ctrl+M in the player. The newest card of the mining deck gets the picture of the frame, the sentence audio and the lines around it. Needs Anki open with AnkiConnect (2055492159) and a Lapis-like note type.")
                            PrefEntry { title: Core.tr(Core.uiLang, "Mining deck"); text: dlg.g.autocards_deck || ""; onEdited: function(t) { dlg.set("general.autocards_deck", t.trim()) } }
                            PrefEntry { first: false; title: Core.tr(Core.uiLang, "Note type (for sentence cards)"); text: dlg.g.autocards_model || ""; onEdited: function(t) { dlg.set("general.autocards_model", t.trim()) } }
                            PrefEntry { first: false; title: Core.tr(Core.uiLang, "Sentence field"); text: dlg.g.autocards_f_sentence || ""; onEdited: function(t) { dlg.set("general.autocards_f_sentence", t.trim()) } }
                            PrefEntry { first: false; title: Core.tr(Core.uiLang, "Expression field"); text: dlg.g.autocards_f_expression || ""; onEdited: function(t) { dlg.set("general.autocards_f_expression", t.trim()) } }
                            PrefEntry { first: false; title: Core.tr(Core.uiLang, "Picture field"); text: dlg.g.autocards_f_picture || ""; onEdited: function(t) { dlg.set("general.autocards_f_picture", t.trim()) } }
                            PrefEntry { first: false; title: Core.tr(Core.uiLang, "Sentence audio field"); text: dlg.g.autocards_f_audio || ""; onEdited: function(t) { dlg.set("general.autocards_f_audio", t.trim()) } }
                            PrefSpin { title: Core.tr(Core.uiLang, "Previous lines"); subtitle: Core.tr(Core.uiLang, "Context before the sentence"); from: 0; to: 5; value: dlg.g.autocards_prev || 0; onMoved: function(v) { dlg.set("general.autocards_prev", v) } }
                            PrefSpin { title: Core.tr(Core.uiLang, "Next lines"); subtitle: Core.tr(Core.uiLang, "Context after the sentence"); from: 0; to: 5; value: dlg.g.autocards_next || 0; onMoved: function(v) { dlg.set("general.autocards_next", v) } }
                            PrefSpin { title: Core.tr(Core.uiLang, "Audio padding"); subtitle: Core.tr(Core.uiLang, "Milliseconds of sound kept before and after the line"); from: 0; to: 1500; step: 50; value: dlg.g.autocards_pad_ms !== undefined ? dlg.g.autocards_pad_ms : 250; onMoved: function(v) { dlg.set("general.autocards_pad_ms", v) } }
                            PrefEntry { first: false; title: "AnkiConnect"; text: dlg.g.autocards_url || ""; onEdited: function(t) { dlg.set("general.autocards_url", t.trim()) } }
                            PrefButton { title: Core.tr(Core.uiLang, "Test the connection"); subtitle: dlg.info("autocards"); label: Core.tr(Core.uiLang, "Test"); kind: "suggested"; onActivated: Core.cmd({cmd: "autocards_test"}) }
                        }
                    }
                }

                Column {
                    visible: dlg.cur === "watch"
                    width: parent.width
                    spacing: 26
                    // study while watching: the subtitle flash cards, questions, parts without subtitles, word mixing,
                    // mined cards and the macaw's cues, each a switch (the same card is in the Study mode)
                    StudySettings { }

                }

                Column {
                    visible: dlg.cur === "voice"
                    width: parent.width
                    spacing: 26
                    // the microphone (Conversation, voice tests): a test with fallback to another input (MicTest.qml)
                    // (desktop only: MicTest.qml is left out of the Android and iOS builds, so the group hides there)
                    PrefGroup {
                        visible: micTest.status === Loader.Ready
                        title: Core.tr(Core.uiLang, "Microphone")
                        description: Core.tr(Core.uiLang, "Check that the microphone works. When the chosen one sends only silence, the app tries the others by itself.")
                        Item {
                            width: parent.width; height: micTest.item ? micTest.item.implicitHeight + 24 : 0
                            Loader { id: micTest; source: "MicTest.qml"; x: 14; y: 12; width: parent.width - 28 }
                        }
                        // when a turn ends by itself (reelhouse::vad): how far over the room's noise the voice must be, and the
                        // pause that ends it (general.voice_margin, general.voice_pause; 0 = automatic, 1.2 s)
                        PrefSlider {
                            title: Core.tr(Core.uiLang, "Cut sensitivity")
                            subtitle: Core.tr(Core.uiLang, "How far over the room's noise your voice must be to count as speech. Lower hears a softer voice; higher ignores more noise (raise it if the microphone never stops by itself)")
                            from: 4; to: 24; step: 1; unit: " dB"
                            property real dragged: -1
                            value: dlg.g.voice_margin || 10
                            onMoved: function(v) { dragged = v }
                            onReleased: { if (dragged >= 0) dlg.set("general.voice_margin", Math.round(dragged)); dragged = -1 }
                        }
                        PrefSlider {
                            title: Core.tr(Core.uiLang, "Silence to end")
                            subtitle: Core.tr(Core.uiLang, "How long a pause ends your turn in Conversation")
                            from: 0.6; to: 3; step: 0.1; unit: " s"; decimals: 1
                            property real dragged: -1
                            value: (dlg.g.voice_pause || 1200) / 1000
                            onMoved: function(v) { dragged = v }
                            onReleased: { if (dragged >= 0) dlg.set("general.voice_pause", Math.round(dragged * 10) * 100); dragged = -1 }
                        }
                        PrefSwitch { title: Core.tr(Core.uiLang, "Send what I say at once"); subtitle: Core.tr(Core.uiLang, "In Conversation, what the microphone heard goes to the tutor without a click (off: it waits in the box)"); checked: dlg.g.kaiwa_autosend !== false; onToggled: function(on) { dlg.set("general.kaiwa_autosend", on) } }
                        PrefSwitch { title: Core.tr(Core.uiLang, "Hands-free conversation"); subtitle: Core.tr(Core.uiLang, "After each answer of the tutor (and its voice) the microphone opens again by itself"); checked: dlg.g.kaiwa_handsfree !== false; onToggled: function(on) { dlg.set("general.kaiwa_handsfree", on) } }
                    }
                }

                // ================= Addons & accounts: one place for the addons of every mode =================
                Column {
                    visible: dlg.cur === "addons"
                    width: parent.width
                    spacing: 26
                    PrefGroup {
                        title: dlg.tt("Add an addon")
                        description: dlg.tt("One box for every mode. Paste the link of a video addon (manifest.json or stremio://), of a repository of manga and comic extensions (index.min.json or its GitHub page) or of a list of novel plugins (.json). The app finds out which mode it is for.")
                        PrefCombo {
                            id: addKind
                            readonly property var ids: ["auto", "video", "images", "text"]
                            title: dlg.tt("Kind")
                            options: [dlg.tt("Find out by itself"), dlg.tt("Video (catalogs, streams, subtitles)"), dlg.tt("Images (manga and comics)"), dlg.tt("Text (light and web novels)")]
                            onPicked: function(i) { current = i }
                        }
                        PrefEntry {
                            id: addonEntry
                            first: false
                            title: dlg.tt("Addon link")
                            apply: true
                            onApplied: function(t) { Core.cmd({cmd: "add_any_addon", url: t.trim(), kind: addKind.ids[addKind.current]}); addonEntry.clear() }
                        }
                        PrefRow { title: dlg.tt("Status"); subtitle: dlg.tt(dlg.info("addon")) }
                    }


                    PrefGroup {
                        title: Core.tr(Core.uiLang, "Video: installed addons")
                        description: Core.tr(Core.uiLang, "Turn addons off or remove them. Catalogs, streams and subtitles come from these.")
                        Repeater {
                            model: dlg.ex.addons !== undefined ? dlg.ex.addons : []
                            delegate: PrefSwitch {
                                required property var modelData
                                title: modelData.name
                                subtitle: modelData.needsSetup === true ? Core.tr(Core.uiLang, "Not set up: it ignores every request. Remove it and add the link from its /configure page.") + "\n" + modelData.url : modelData.url
                                checked: modelData.enabled
                                onToggled: function(on) { Core.cmd({cmd: "toggle_addon", url: modelData.url, on: on}) }
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flat"; glyph: "trash"; implicitWidth: 34; implicitHeight: 34; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Remove"); onClicked: Core.cmd({cmd: "remove_addon", url: modelData.url}) }
                            }
                        }
                    }
                    PrefGroup {
                        readonly property var su: dlg.ex.suwayomi || ({})
                        id: repoGrp
                        title: dlg.tt("Images: repositories")
                        description: dlg.tt("Extensions in the Tachiyomi / Mihon format, run by the Suwayomi server. Choose the sources in Images > Sources.")
                        Repeater {
                            model: repoGrp.su.repos || []
                            delegate: PrefRow {
                                required property var modelData
                                title: modelData
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flat"; glyph: "trash"; implicitWidth: 34; implicitHeight: 34; ToolTip.visible: hovered; ToolTip.text: dlg.tt("Remove"); onClicked: Core.cmd({cmd: "suwayomi_remove_repo", url: modelData}) }
                            }
                        }
                        PrefButton {
                            title: (repoGrp.su.repos || []).length > 0 ? dlg.tt("Sources") : dlg.tt("No repository yet")
                            subtitle: repoGrp.su.status || ""
                            label: dlg.tt("Choose")
                            onActivated: dlg.go("images", "img_src")
                        }
                    }
                    PrefGroup {
                        id: novelGrp
                        readonly property var nv: Core.manga.novels || ({})
                        readonly property string repo: dlg.g.text_repo || ""
                        title: dlg.tt("Text: list of plugins")
                        description: dlg.tt("Light novels and web novels. One list at a time: adding another replaces it. Choose the sources in Text > Sources.")
                        PrefRow {
                            visible: novelGrp.repo !== ""
                            title: novelGrp.repo
                            subtitle: (novelGrp.nv.catalog || {}).status || ""
                            FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flatround"; glyph: "refresh"; ToolTip.visible: hovered; ToolTip.text: dlg.tt("Read the repository"); onClicked: Core.cmd({cmd: "novel_load_catalog"}) }
                            FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flat"; glyph: "trash"; implicitWidth: 34; implicitHeight: 34; ToolTip.visible: hovered; ToolTip.text: dlg.tt("Remove"); onClicked: { dlg.set("general.text_repo", ""); Core.cmd({cmd: "novel_load_catalog"}) } }
                        }
                        PrefButton {
                            title: novelGrp.repo !== "" ? dlg.tt("Sources") : dlg.tt("No repository yet")
                            label: dlg.tt("Choose")
                            onActivated: dlg.go("text", "txt_src")
                        }
                    }

                    PrefsAdvanced { dlg: dlg }
                    Column {
                        visible: dlg.adv
                        width: parent.width
                        spacing: 26
                        SectionHead { title: Core.tr(Core.uiLang, "Extensions"); icon: "puzzle" }
                        PrefGroup {
                            description: Core.tr(Core.uiLang, "Small JavaScript modules that react to what happens in the app (a title opened, playback, a clip saved, a card answered). Each one is a folder with manifest.json and main.js.")
                            Repeater {
                                model: dlg.ex.extensions !== undefined ? dlg.ex.extensions : []
                                delegate: PrefSwitch {
                                    required property var modelData
                                    title: modelData.name + (modelData.version ? "  " + modelData.version : "")
                                    subtitle: modelData.error !== "" ? modelData.error : (modelData.description || (modelData.hooks || []).join(", "))
                                    enabled: modelData.error === ""
                                    checked: modelData.enabled === true
                                    onToggled: function(on) { Core.cmd({cmd: "extension_toggle", id: modelData.id, on: on}) }
                                }
                            }
                            PrefRow {
                                title: Core.tr(Core.uiLang, "Extensions folder")
                                subtitle: dlg.ex.extensionsDir || ""
                                selectable: true
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Open"); implicitHeight: 34; onClicked: Qt.openUrlExternally(Core.fileUrl(dlg.ex.extensionsDir || "")) }
                                FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flatround"; glyph: "refresh"; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Load again"); onClicked: Core.cmd({cmd: "extensions_reload"}) }
                            }
                        }
                    }
                }

                Column {
                    visible: dlg.cur === "accounts"
                    width: parent.width
                    spacing: 26
                    SectionHead { title: Core.tr(Core.uiLang, "Accounts and keys"); icon: "star" }
                    PrefGroup {
                        description: Core.tr(Core.uiLang, "Each service opens its own window for the login or the key.")
                        Repeater {
                            model: dlg.services
                            delegate: PrefButton {
                                required property var modelData
                                title: modelData.name
                                subtitle: modelData.state
                                label: modelData.ok ? Core.tr(Core.uiLang, "Manage") : Core.tr(Core.uiLang, "Connect")
                                kind: modelData.ok ? "" : "suggested"
                                onActivated: dlg.openAccount(modelData.id)
                            }
                        }
                    }
                }

            }
        }
    }

    // a sound of controller mode (Preferences > General > Controller mode); the core copies it into the data folder
    FileDialog {
        id: soundDlg
        property string slot: "nav"
        title: Core.tr(Core.uiLang, "Choose…")
        fileMode: FileDialog.OpenFile
        // any format Qt Multimedia plays: picked sounds go through a MediaPlayer (SoundEffect read only WAV)
        nameFilters: [Core.tr(Core.uiLang, "Audio files") + " (*.mp3 *.ogg *.wav *.flac *.m4a *.opus *.aac)"]
        onAccepted: Core.cmd({cmd: "set_ui_sound", slot: slot, path: decodeURIComponent(selectedFile.toString().replace(/^file:\/{2,3}/, Qt.platform.os === "windows" ? "" : "/"))})
    }
    FileDialog {
        id: saveDlg
        title: Core.tr(Core.uiLang, "Save backup")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "rhb"
        nameFilters: [Core.tr(Core.uiLang, "Reelhouse backup") + " (*.rhb)"]
        onAccepted: Core.cmd({cmd: "backup_now", path: decodeURIComponent(selectedFile.toString().replace(/^file:\/{2,3}/, Qt.platform.os === "windows" ? "" : "/"))})
    }
    FileDialog {
        id: openDlg
        title: Core.tr(Core.uiLang, "Open backup")
        fileMode: FileDialog.OpenFile
        // .proto.gz: backups of older versions
        nameFilters: [Core.tr(Core.uiLang, "Reelhouse backup") + " (*.rhb *.proto.gz)", Core.tr(Core.uiLang, "All files") + " (*)"]
        onAccepted: { restoreAsk.remoteId = ""; restoreAsk.remoteName = ""; restoreAsk.open() }
    }
    FileDialog {
        id: soundPackDlg
        title: Core.tr(Core.uiLang, "Import sound pack…")
        fileMode: FileDialog.OpenFile
        nameFilters: [Core.tr(Core.uiLang, "Sound pack") + " (*.zip)", Core.tr(Core.uiLang, "All files") + " (*)"]
        onAccepted: Core.cmd({cmd: "import_sound_pack", path: dlg.localPath(selectedFile)})
    }
    FolderDialog {
        id: soundPackDirDlg
        title: Core.tr(Core.uiLang, "Import sound pack…")
        onAccepted: Core.cmd({cmd: "import_sound_pack", path: dlg.localPath(selectedFolder)})
    }
    FolderDialog {
        id: cloudFolderDlg
        title: Core.tr(Core.uiLang, "Sync folder of the cloud")
        onAccepted: dlg.set("backup.cloud.folder", decodeURIComponent(selectedFolder.toString().replace(/^file:\/{2,3}/, Qt.platform.os === "windows" ? "" : "/")))
    }
    FolderDialog {
        id: folderDlg
        title: Core.tr(Core.uiLang, "Folder for automatic backups")
        onAccepted: dlg.set("backup.folder", decodeURIComponent(selectedFolder.toString().replace(/^file:\/{2,3}/, Qt.platform.os === "windows" ? "" : "/")))
    }
    // Restoring replaces everything: ask first (GTK AlertDialog).
    Popup {
        id: restoreAsk
        // a backup kept online (its id and name in the list), or empty for the file picked in openDlg
        property string remoteId: ""
        property string remoteName: ""
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(440, (parent ? parent.width : 480) - 48)
        modal: true
        padding: 24
        Overlay.modal: Rectangle { color: Theme.c.scrim }
        background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.popover; shadow: true }
        contentItem: Column {
            spacing: 16
            DialogTitle { text: Core.tr(Core.uiLang, "Restore this backup?") }
            Text { visible: restoreAsk.remoteName !== ""; width: parent.width; horizontalAlignment: Theme.look === "gnome" || Theme.look === "apple" ? Text.AlignHCenter : Text.AlignLeft; wrapMode: Text.Wrap; text: restoreAsk.remoteName; color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.mono }
            Text { width: parent.width; horizontalAlignment: Theme.look === "gnome" || Theme.look === "apple" ? Text.AlignHCenter : Text.AlignLeft; wrapMode: Text.Wrap; text: Core.tr(Core.uiLang, "Your current settings, library, progress, study collection and the rest will be replaced. A copy of what you have now is saved first, in the backups folder."); color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family }
            DialogButtons {
                bleed: 24
                okText: Core.tr(Core.uiLang, "Restore")
                okKind: "destructive"
                onCancelled: restoreAsk.close()
                onAccepted: {
                    restoreAsk.close()
                    if (restoreAsk.remoteId !== "") Core.cmd({cmd: "cloud_restore", id: restoreAsk.remoteId, name: restoreAsk.remoteName})
                    else Core.cmd({cmd: "restore_backup", path: decodeURIComponent(openDlg.selectedFile.toString().replace(/^file:\/{2,3}/, Qt.platform.os === "windows" ? "" : "/"))})
                }
            }
        }
    }

    // ---- Study > "Where to start in Learn": the path of the course picks up at the lesson chosen (learn.rs start_at) ----
    Popup {
        id: learnStart
        property var ln: ({})
        property int unit: 0
        property int lesson: 1
        readonly property var units: ln.units || []
        readonly property var u: units[Math.min(unit, Math.max(0, units.length - 1))] || ({})
        readonly property var ls: u.lessons || []
        // where the path is now: the current lesson, else the last one
        function place() {
            for (var i = 0; i < units.length; ++i) {
                var l = units[i].lessons || []
                for (var k = 0; k < l.length; ++k) if (l[k].state === "current") { unit = i; lesson = k + 1; return }
            }
            unit = Math.max(0, units.length - 1); lesson = 1
        }
        function load() { ln = Core.state("learn"); if (unit === 0 && lesson === 1) place() }
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(480, (parent ? parent.width : 520) - 48)
        modal: true
        padding: 24
        Overlay.modal: Rectangle { color: Theme.c.scrim }
        background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.popover; shadow: true }
        onAboutToShow: { unit = 0; lesson = 1; Core.cmd({cmd: "learn_open", tz: -(new Date().getTimezoneOffset())}); load() }
        Connections { target: Core; enabled: learnStart.visible; function onStudyChanged() { learnStart.load() } }
        contentItem: Column {
            spacing: 16
            DialogTitle { text: Core.tr(Core.uiLang, "Where to start in Learn") }
            Text {
                width: parent.width
                horizontalAlignment: Theme.look === "gnome" || Theme.look === "apple" ? Text.AlignHCenter : Text.AlignLeft
                wrapMode: Text.Wrap
                color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family
                text: (learnStart.ln.course || "") === "" ? Core.tr(Core.uiLang, "Choose the language you study first (Learn, or Study language in the settings).")
                    : learnStart.units.length === 0 ? Core.tr(Core.uiLang, "Loading the course…")
                    : Core.tr(Core.uiLang, "Every lesson before the one you pick counts as finished, and the ones after it as not. Your XP, streak, gems and cards stay as they are.")
            }
            PrefGroup {
                visible: learnStart.units.length > 0
                width: parent.width
                PrefCombo {
                    title: Core.tr(Core.uiLang, "Unit")
                    options: learnStart.units.map(function(x, i) { return (i + 1) + ". " + (x.title || "") })
                    current: learnStart.unit
                    onPicked: function(i) { learnStart.unit = i; learnStart.lesson = 1 }
                }
                PrefSpin {
                    title: Core.tr(Core.uiLang, "Lesson")
                    from: 1; to: Math.max(1, learnStart.ls.length)
                    value: learnStart.lesson
                    onMoved: function(v) { learnStart.lesson = Math.round(v) }
                }
            }
            DialogButtons {
                bleed: 24
                okText: Core.tr(Core.uiLang, "Start here")
                okEnabled: learnStart.ls.length > 0
                onCancelled: learnStart.close()
                onAccepted: {
                    var l = learnStart.ls[Math.min(learnStart.lesson, learnStart.ls.length) - 1]
                    learnStart.close()
                    if (l) Core.cmd({cmd: "learn_start_from", lesson: l.id})
                }
            }
        }
    }

    // ---- one window per account or key (opened from Accounts and keys, or from the translator) ----
    Popup {
        id: acctDlg
        property string svc: ""
        readonly property var sb: dlg.sec("subtitles")
        readonly property var svcInfo: dlg.services.find(function(x) { return x.id === acctDlg.svc }) || ({name: ""})
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(600, (parent ? parent.width : 700) - 60)
        height: Math.min(acctCol.implicitHeight + 110, (parent ? parent.height : 600) - 80)
        modal: true
        focus: true
        padding: 0
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        Overlay.modal: Rectangle { color: Theme.c.scrim }
        background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
        contentItem: Item {
            DialogHeader { id: acctHead; title: acctDlg.svcInfo.name; onClose: acctDlg.close() }
            Flickable {
                anchors { left: parent.left; right: parent.right; top: acctHead.bottom; bottom: parent.bottom }
                contentHeight: acctCol.implicitHeight + 24
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                Column {
                    id: acctCol
                    x: 20; width: parent.width - 40
                    spacing: 18
                PrefGroup {
                    id: rdGrp
                    visible: acctDlg.svc === "debrid"
                    readonly property var rd: dlg.sec("debrid")
                    title: Core.tr(Core.uiLang, "Real-Debrid")
                    description: Core.tr(Core.uiLang, "With Real-Debrid, torrents are fetched by your account and play like normal files. Without it, Reelhouse downloads torrents directly from peers.")
                    PrefSwitch { title: Core.tr(Core.uiLang, "Use Real-Debrid for torrents"); checked: rdGrp.rd.enabled === true; onToggled: function(on) { dlg.set("debrid.enabled", on) } }
                    PrefEntry { title: Core.tr(Core.uiLang, "API token"); password: true; text: rdGrp.rd.api_key || ""; onEdited: function(t) { dlg.set("debrid.api_key", t.trim()) } }
                    PrefButton { title: Core.tr(Core.uiLang, "Test the token"); subtitle: dlg.info("realdebrid"); label: Core.tr(Core.uiLang, "Test"); kind: "suggested"; onActivated: Core.cmd({cmd: "test_real_debrid"}) }
                    PrefButton { title: Core.tr(Core.uiLang, "Get your API token"); subtitle: Core.tr(Core.uiLang, "real-debrid.com/apitoken"); label: Core.tr(Core.uiLang, "Open"); onActivated: Qt.openUrlExternally("https://real-debrid.com/apitoken") }
                }
                PrefGroup {
                    id: skipGroup
                    visible: acctDlg.svc === "animeskip"
                    readonly property var a: dlg.sec("anime_skip")
                    readonly property bool logged: (a.token || "") !== ""
                    title: Core.tr(Core.uiLang, "Anime Skip")
                    description: Core.tr(Core.uiLang, "Skip openings, endings and recaps in anime using community timestamps from anime-skip.com.")
                    PrefSwitch { title: Core.tr(Core.uiLang, "Show the skip button"); checked: skipGroup.a.enabled === true; onToggled: function(on) { dlg.set("anime_skip.enabled", on) } }
                    PrefSwitch { title: Core.tr(Core.uiLang, "Skip automatically"); subtitle: Core.tr(Core.uiLang, "Jump over openings and endings without asking"); checked: skipGroup.a.auto_skip === true; onToggled: function(on) { dlg.set("anime_skip.auto_skip", on) } }
                    PrefEntry { title: Core.tr(Core.uiLang, "Client ID"); text: skipGroup.a.client_id || ""; onEdited: function(t) { dlg.set("anime_skip.client_id", t.trim()) } }
                    PrefButton { title: Core.tr(Core.uiLang, "Get a client ID"); subtitle: Core.tr(Core.uiLang, "Create one in your Anime Skip account settings"); label: Core.tr(Core.uiLang, "Open"); onActivated: Qt.openUrlExternally("https://anime-skip.com/account") }
                    PrefEntry { id: skipEmail; visible: !skipGroup.logged; title: Core.tr(Core.uiLang, "Email or username"); text: skipGroup.a.email || ""; onEdited: function(t) { dlg.set("anime_skip.email", t.trim()) } }
                    PrefEntry { id: skipPass; visible: !skipGroup.logged; title: Core.tr(Core.uiLang, "Password"); password: true }
                    PrefRow {
                        title: Core.tr(Core.uiLang, "Account")
                        highlight: skipGroup.logged
                        subtitle: dlg.info("animeskip") !== "" ? dlg.info("animeskip") : (skipGroup.logged ? Core.tr(Core.uiLang, "Logged in as") + " " + skipGroup.a.email : Core.tr(Core.uiLang, "Not logged in"))
                        FlatButton { visible: !skipGroup.logged; anchors.verticalCenter: parent.verticalCenter; kind: "suggested"; text: Core.tr(Core.uiLang, "Log in"); implicitHeight: 34; onClicked: { Core.cmd({cmd: "anime_skip_login", email: skipGroup.a.email || "", password: skipPass.value}); skipPass.clear() } }
                        FlatButton { visible: skipGroup.logged; anchors.verticalCenter: parent.verticalCenter; kind: "destructive"; text: Core.tr(Core.uiLang, "Log out"); implicitHeight: 34; onClicked: Core.cmd({cmd: "anime_skip_logout"}) }
                    }
                }
                PrefGroup {
                    id: malGroup
                    visible: acctDlg.svc === "mal"
                    readonly property var m: dlg.sec("mal")
                    readonly property bool logged: dlg.ex.malLogged === true
                    readonly property string redirect: "http://localhost:51789/callback"
                    title: Core.tr(Core.uiLang, "MyAnimeList")
                    description: Core.tr(Core.uiLang, "Log in to see and change your status and score in the anime page, and to mark watched episodes from your list. AniSync (an addon) also marks episodes as watched.")
                    PrefButton { title: Core.tr(Core.uiLang, "1. Create an app"); subtitle: Core.tr(Core.uiLang, "On myanimelist.net/apiconfig choose Create ID. App Type: other (with web, also fill the Client Secret)."); label: Core.tr(Core.uiLang, "Open"); onActivated: Qt.openUrlExternally("https://myanimelist.net/apiconfig") }
                    PrefRow {
                        title: Core.tr(Core.uiLang, "2. Redirect URL")
                        subtitle: Core.tr(Core.uiLang, "Add this address, on its own line, to App Redirect URL on myanimelist.net/apiconfig and save. The field accepts several addresses; it must list this one exactly.")
                        TextEdit { anchors.verticalCenter: parent.verticalCenter; readOnly: true; selectByMouse: true; text: malGroup.redirect; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.mono }
                        FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flat"; glyph: "copy"; implicitWidth: 34; implicitHeight: 34; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Copy"); onClicked: { clipEdit.text = malGroup.redirect; clipEdit.selectAll(); clipEdit.copy(); win.showToast(Core.tr(Core.uiLang, "Copied")) } }
                        TextEdit { id: clipEdit; visible: false }
                    }
                    PrefEntry { visible: !malGroup.logged; title: Core.tr(Core.uiLang, "3. Client ID"); text: malGroup.m.client_id || ""; onEdited: function(t) { dlg.set("mal.client_id", t.trim()) } }
                    PrefEntry { visible: !malGroup.logged; title: Core.tr(Core.uiLang, "Keep another redirect URL? Type it here (otherwise leave empty)"); text: malGroup.m.redirect || ""; onEdited: function(t) { dlg.set("mal.redirect", t.trim()) } }
                    PrefEntry { visible: !malGroup.logged; title: Core.tr(Core.uiLang, "Client Secret (only if the app has one)"); password: true; text: malGroup.m.client_secret || ""; onEdited: function(t) { dlg.set("mal.client_secret", t.trim()) } }
                    PrefRow {
                        title: Core.tr(Core.uiLang, "4. Account")
                        highlight: malGroup.logged
                        subtitle: dlg.info("mal") !== "" ? dlg.info("mal") : (malGroup.logged ? Core.tr(Core.uiLang, "Logged in as") + " " + (malGroup.m.user || "") : Core.tr(Core.uiLang, "Not logged in"))
                        FlatButton { visible: !malGroup.logged; anchors.verticalCenter: parent.verticalCenter; kind: "suggested"; text: Core.tr(Core.uiLang, "Log in"); implicitHeight: 34; onClicked: Core.cmd({cmd: "mal_login"}) }
                        FlatButton { visible: malGroup.logged; anchors.verticalCenter: parent.verticalCenter; kind: "destructive"; text: Core.tr(Core.uiLang, "Log out"); implicitHeight: 34; onClicked: Core.cmd({cmd: "mal_logout"}) }
                    }
                    PrefEntry { id: pasteEntry; visible: !malGroup.logged; title: Core.tr(Core.uiLang, "Browser did not come back? Paste its address here"); apply: true; onApplied: function(t) { Core.cmd({cmd: "mal_finish_with", url: t}); pasteEntry.clear() } }
                    PrefButton { title: Core.tr(Core.uiLang, "Your list"); subtitle: dlg.ex.malListInfo || ""; label: Core.tr(Core.uiLang, "Refresh"); onActivated: Core.cmd({cmd: "refresh_mal_list"}) }
                    PrefRow { title: Core.tr(Core.uiLang, "AniSync"); subtitle: dlg.ex.anisync === true ? Core.tr(Core.uiLang, "Installed. Episodes it knows you watched show as WATCHED.") : Core.tr(Core.uiLang, "Not installed. Add your personal AniSync address in Addons.") }
                }
                PrefGroup {
                    id: metaGrp
                    visible: acctDlg.svc === "metadata"
                    readonly property var m: dlg.sec("metadata")
                    title: Core.tr(Core.uiLang, "Artwork and ratings")
                    description: Core.tr(Core.uiLang, "Cinemeta is always tried first. When it has no picture or title, TMDB and OMDb (with your keys) are used as backups. TMDB can also replace the artwork and rating.")
                    PrefSwitch { title: Core.tr(Core.uiLang, "Use TMDB for artwork and ratings"); checked: metaGrp.m.tmdb_art === true; onToggled: function(on) { dlg.set("metadata.tmdb_art", on) } }
                    PrefEntry { title: Core.tr(Core.uiLang, "TMDB API key or read access token"); password: true; text: metaGrp.m.tmdb_key || ""; onEdited: function(t) { dlg.set("metadata.tmdb_key", t.trim()) } }
                    PrefEntry { title: Core.tr(Core.uiLang, "OMDb API key (backup for posters and titles)"); password: true; text: metaGrp.m.omdb_key || ""; onEdited: function(t) { dlg.set("metadata.omdb_key", t.trim()) } }
                    PrefButton { title: Core.tr(Core.uiLang, "Get a free OMDb key"); subtitle: Core.tr(Core.uiLang, "omdbapi.com/apikey.aspx"); label: Core.tr(Core.uiLang, "Open"); onActivated: Qt.openUrlExternally("https://www.omdbapi.com/apikey.aspx") }
                    PrefButton { title: Core.tr(Core.uiLang, "Get a free TMDB key"); subtitle: Core.tr(Core.uiLang, "themoviedb.org/settings/api"); label: Core.tr(Core.uiLang, "Open"); onActivated: Qt.openUrlExternally("https://www.themoviedb.org/settings/api") }
                }
                PrefGroup {
                    visible: acctDlg.svc === "libretranslate"
                    title: Core.tr(Core.uiLang, "LibreTranslate")
                    description: Core.tr(Core.uiLang, "Works by itself: subtitles are translated inside Reelhouse through public LibreTranslate servers. Nothing to install. Fill in a server only if you run your own.")
                    PrefEntry { title: Core.tr(Core.uiLang, "Your own server (optional)"); text: acctDlg.sb.libre_url || ""; onEdited: function(t) { dlg.set("subtitles.libre_url", t.trim()) } }
                    PrefEntry { title: Core.tr(Core.uiLang, "API key (optional)"); password: true; text: acctDlg.sb.libre_key || ""; onEdited: function(t) { dlg.set("subtitles.libre_key", t.trim()) } }
                }
                PrefGroup {
                    visible: acctDlg.svc === "claude"
                    title: Core.tr(Core.uiLang, "Claude (Anthropic)")
                    PrefEntry { title: Core.tr(Core.uiLang, "API key"); password: true; text: acctDlg.sb.claude_key || ""; onEdited: function(t) { dlg.set("subtitles.claude_key", t.trim()) } }
                    PrefEntry { title: Core.tr(Core.uiLang, "Model"); text: acctDlg.sb.claude_model || ""; onEdited: function(t) { dlg.set("subtitles.claude_model", t.trim()) } }
                }
                PrefGroup {
                    visible: acctDlg.svc === "gemini"
                    title: Core.tr(Core.uiLang, "Gemini (Google)")
                    PrefEntry { title: Core.tr(Core.uiLang, "API key"); password: true; text: acctDlg.sb.gemini_key || ""; onEdited: function(t) { dlg.set("subtitles.gemini_key", t.trim()) } }
                    PrefEntry { title: Core.tr(Core.uiLang, "Model"); text: acctDlg.sb.gemini_model || ""; onEdited: function(t) { dlg.set("subtitles.gemini_model", t.trim()) } }
                    // the same free key serves the translators, the microphone and Conversation by voice (Gemini Live)
                    PrefButton { title: Core.tr(Core.uiLang, "Get a free Gemini key"); subtitle: Core.tr(Core.uiLang, "aistudio.google.com/apikey · also used by Conversation by voice"); label: Core.tr(Core.uiLang, "Open"); onActivated: Qt.openUrlExternally("https://aistudio.google.com/apikey") }
                }
                PrefGroup {
                    visible: acctDlg.svc === "openai"
                    title: Core.tr(Core.uiLang, "GPT (OpenAI)")
                    description: Core.tr(Core.uiLang, "Any server with an OpenAI-compatible API also works: change the base URL.")
                    PrefEntry { title: Core.tr(Core.uiLang, "API key"); password: true; text: acctDlg.sb.openai_key || ""; onEdited: function(t) { dlg.set("subtitles.openai_key", t.trim()) } }
                    PrefEntry { title: Core.tr(Core.uiLang, "Model"); text: acctDlg.sb.openai_model || ""; onEdited: function(t) { dlg.set("subtitles.openai_model", t.trim()) } }
                    PrefEntry { title: Core.tr(Core.uiLang, "Base URL"); text: acctDlg.sb.openai_base || ""; onEdited: function(t) { dlg.set("subtitles.openai_base", t.trim()) } }
                }


                    AnkiWebLogin { visible: acctDlg.svc === "ankiweb"; width: parent.width }
                }
            }
        }
    }

}
