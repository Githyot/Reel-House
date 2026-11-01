import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// Images and Text modes, laid out as Library (categories, unread counts), Explore (languages, sources, a source's
// titles) and History (by day, with a play button). The series page and the reader sit above it.
Item {
    id: root
    property string kind: "manga"                // manga (Images) | novel (Text)
    property string tab: "library"               // library | explore | history   (chosen in the header bar)
    property int searchTick: 0                   // the magnifier of the header bar
    readonly property bool isManga: kind === "manga"
    readonly property var st: isManga ? Core.manga : (Core.manga.novels || ({}))
    readonly property var lst: st.list || ({})
    readonly property bool inView: !!st.view
    readonly property bool inReader: !!st.reader
    readonly property var library: st.library || []
    readonly property var categories: st.categories || []
    readonly property var pins: isManga ? (st.pins || []) : []
    readonly property var langs: st.langs || []
    property bool browsing: false
    property string what: "popular"
    property string category: ""
    property bool catsOpen: false
    property bool searchOn: false
    property bool hisSearchOn: false
    property string autoRead: ""
    property var menuSeries: ({})
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function cmdName(n) { return kind + "_" + n }

    function openSeries(s) { Core.cmd({cmd: cmdName("open"), series: s}) }
    function browse(w, text) { what = w; Core.cmd({cmd: cmdName("browse"), what: w, text: text || ""}) }
    function openSource(src) {
        Core.cmd({cmd: cmdName("set_source"), id: src.id})
        sourceName = src.name
        sourceIcon = src.icon || ""
        sourceSite = src.site || ""
        browsing = true
        exEntry.text = ""
        what = "popular"          // choosing the source already reads its popular titles
    }
    property string sourceName: ""
    property string sourceIcon: ""
    property string sourceSite: ""
    // a text source that answers 403/503 (a bot check such as Cloudflare): on Android the site opens in the System
    // WebView, the person passes the check there and the source's requests carry its cookies (HP: AllNovel)
    readonly property bool blocked: !isManga && !lst.loading && /\b(403|503)\b|webview|cloudflare/i.test(lst.error || "")
    readonly property bool canCheck: Qt.platform.os === "android" && sourceSite !== ""

    // ---- sources, by language ----
    readonly property var allSources: (isManga ? [{id: "mangadex", name: "MangaDex", lang: "", code: "", langName: "", icon: ""}] : []).concat(st.sources || [])
    readonly property var langOptions: {
        const out = [], seen = {}
        const s = st.sources || []
        for (let i = 0; i < s.length; ++i) {
            if (s[i].code && !seen[s[i].code]) { seen[s[i].code] = true; out.push({code: s[i].code, name: s[i].langName || s[i].lang}) }
        }
        out.sort(function(a, b) { return a.name < b.name ? -1 : (a.name > b.name ? 1 : 0) })
        return out
    }
    readonly property var groups: {
        const keep = allSources.filter(function(x) {
            return x.lang === "" || langs.length === 0 || pins.indexOf(x.id) >= 0 || x.code === "all" || x.code === "multi" || langs.indexOf(x.code) >= 0
        })
        const out = []
        const pinned = pins.map(function(p) { return keep.find(function(x) { return x.id === p }) }).filter(function(x) { return !!x })
        if (pinned.length > 0) out.push({title: tr("Pinned"), items: pinned})
        const rest = keep.filter(function(x) { return pins.indexOf(x.id) < 0 })
        const builtin = rest.filter(function(x) { return x.lang === "" })
        if (builtin.length > 0) out.push({title: tr("Built in"), items: builtin})
        const names = [], by = {}
        rest.filter(function(x) { return x.lang !== "" }).forEach(function(x) {
            const k = x.langName || x.lang
            if (!by[k]) { by[k] = []; names.push(k) }
            by[k].push(x)
        })
        const mine = ((Core.prefs.general || {}).manga_lang || "en").toLowerCase()
        names.sort(function(a, b) {
            const am = by[a][0].code === mine ? 0 : 1, bm = by[b][0].code === mine ? 0 : 1
            return am !== bm ? am - bm : (a < b ? -1 : 1)
        })
        names.forEach(function(k) { out.push({title: k, items: by[k]}) })
        return out
    }
    function setPins(list) { Core.cmd({cmd: "set_setting", path: "general.manga_pins", value: list}) }
    function togglePin(id) {
        const cur = pins.slice()
        const at = cur.indexOf(id)
        if (at >= 0) cur.splice(at, 1); else cur.push(id)
        setPins(cur)
    }
    function setLangs(list) { Core.cmd({cmd: "set_setting", path: isManga ? "general.manga_langs" : "general.novel_langs", value: list}) }

    // ---- library ----
    readonly property var libraryIds: {
        const o = {}
        for (let i = 0; i < library.length; ++i) o[library[i].id] = true
        return o
    }
    readonly property var shownLibrary: {
        const q = libSearch.text.toLowerCase()
        return library.filter(function(s) {
            return (category === "" || (s.categories || []).indexOf(category) >= 0) && (q === "" || (s.title || "").toLowerCase().indexOf(q) >= 0)
        })
    }
    function countIn(cat) { return library.filter(function(s) { return (s.categories || []).indexOf(cat) >= 0 }).length }

    // ---- history ----
    function dayLabel(at) {
        const d = new Date(at * 1000), n = new Date()
        if (d.toDateString() === n.toDateString()) return tr("Today")
        if (d.toDateString() === new Date(n.getTime() - 86400000).toDateString()) return tr("Yesterday")
        return d.toLocaleDateString(Qt.locale(), "dddd, MMMM d")
    }
    readonly property var historyDays: {
        const out = []
        const q = hisSearch.text.toLowerCase()
        const h = (st.history || []).filter(function(x) { return q === "" || (x.title || "").toLowerCase().indexOf(q) >= 0 })
        for (let i = 0; i < h.length; ++i) {
            const day = dayLabel(h[i].at)
            if (out.length === 0 || out[out.length - 1].day !== day) out.push({day: day, items: []})
            out[out.length - 1].items.push(h[i])
        }
        return out
    }

    // ---- the chapter of the play button opens as soon as the list is here ----
    readonly property var view: st.view
    onViewChanged: {
        if (autoRead === "" || !view || view.loading) return
        const ch = (view.chapters || []).find(function(x) { return x.id === autoRead })
        autoRead = ""
        if (ch) Core.cmd({cmd: cmdName("read"), id: ch.id})
    }
    // a header tab closes the series page and the reader
    onTabChanged: {
        if (inReader) Core.cmd({cmd: cmdName("close_reader")})
        if (inView) Core.cmd({cmd: cmdName("close")})
    }
    onSearchTickChanged: doSearch()
    function doSearch() {
        if (tab === "library") { searchOn = !searchOn; if (searchOn) libSearch.forceActiveFocus(); else libSearch.text = "" }
        else if (tab === "explore" && browsing) exEntry.forceActiveFocus()
        else if (tab === "history") { hisSearchOn = !hisSearchOn; if (hisSearchOn) hisSearch.forceActiveFocus(); else hisSearch.text = "" }
    }

    // the mode was shown: the server of the images is started, the sources of the texts are read
    function entered() {
        if (isManga) Core.cmd({cmd: "suwayomi_enter"})
        else Core.cmd({cmd: "novel_load_catalog"})
    }
    onVisibleChanged: if (visible) entered()
    // the categories are there from the start, in Images and in Text
    Component.onCompleted: { if (categories.length > 0) catsOpen = true; if (visible) entered() }

    FileDialog {
        id: epubDlg
        title: root.tr("Choose a book")
        nameFilters: ["EPUB (*.epub)", root.tr("All files") + " (*)"]
        onAccepted: Core.cmd({cmd: "novel_import", path: Core.localPath(selectedFile.toString())})
    }
    TitlePrompt {
        id: newBookPrompt
        heading: root.tr("New book")
        actionText: root.tr("Create")
        onAccepted: function(t) { Core.cmd({cmd: "novel_new_book", title: t}) }
    }
    TitlePrompt {
        id: catPrompt
        heading: root.tr("New category")
        actionText: root.tr("Create")
        onAccepted: function(t) { Core.cmd({cmd: root.cmdName("add_category"), name: t}) }
    }
    PluginsDialog { id: plugins; images: root.isManga }
    // a cover chosen from a picture file (library menu or the series page)
    property string coverFor: ""
    function pickCover(id) { coverFor = id; coverDlg.open() }
    FileDialog {
        id: coverDlg
        title: root.tr("Choose a picture for the cover")
        nameFilters: [root.tr("Pictures") + " (*.jpg *.jpeg *.png *.webp *.bmp *.gif)", root.tr("All files") + " (*)"]
        onAccepted: Core.cmd({cmd: root.cmdName("set_cover"), id: root.coverFor, file: Core.keepAsWebp(Core.localPath(selectedFile.toString()))})
    }

    // the menu of a cover in the library (right click or long press)
    PopMenu {
        id: libMenu
        onPicked: function(i) {
            const it = items[i]
            if (it.key === "remove") Core.cmd({cmd: root.cmdName("toggle_library"), series: root.menuSeries})
            else if (it.key === "cover") root.pickCover(root.menuSeries.id)
            else if (it.key === "uncover") Core.cmd({cmd: root.cmdName("set_cover"), id: root.menuSeries.id, file: ""})
            else if (it.key === "cat") {
                let now = (root.menuSeries.categories || []).slice()
                const at = now.indexOf(it.cat)
                if (at >= 0) now.splice(at, 1); else now.push(it.cat)
                Core.cmd({cmd: root.cmdName("set_categories"), id: root.menuSeries.id, cats: now})
            }
        }
    }
    PopMenu {
        id: catMenu
        property string cat: ""
        items: [{label: root.tr("Remove category"), danger: true}]
        onPicked: { if (root.category === cat) root.category = ""; Core.cmd({cmd: root.cmdName("remove_category"), name: cat}) }
    }

    // ---- the categories in the sidebar: a small header with "+", then All and each category with its count ----
    Column {
        id: sideCats
        parent: root.catSlot ? root.catSlot : root
        visible: root.catsInSide && root.visible && root.tab === "library" && !root.inView && !root.inReader
        width: parent ? parent.width : 0
        spacing: 2
        Item {
            width: parent.width
            height: 34
            Text {
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                text: root.tr("Categories").toUpperCase()
                color: Theme.c.textDim
                font.pixelSize: 11
                font.weight: Font.DemiBold
                font.letterSpacing: 0.8
                font.family: Theme.type.family
            }
            FlatButton {
                id: sideAdd
                anchors { right: parent.right; rightMargin: 2; verticalCenter: parent.verticalCenter }
                kind: "flat"; glyph: "plus"; implicitWidth: 30; implicitHeight: 30
                ToolTip.visible: hovered
                ToolTip.text: root.tr("Add category")
                onClicked: catPrompt.open()
            }
        }
        Repeater {
            model: root.categories.length + 1
            delegate: Rectangle {
                id: sc
                required property int index
                readonly property string name: index === 0 ? "" : root.categories[index - 1]
                readonly property bool sel: root.category === name
                width: sideCats.width
                height: Math.max(36, (Theme.spacing.navRowH || 40) - 4)
                radius: Theme.shape.control
                color: sel ? Theme.c.sidebarSelected : (scArea.containsMouse ? Theme.c.hover : "transparent")
                Rectangle { visible: sc.sel && Theme.layout.navIndicator === "pill-left"; x: 0; anchors.verticalCenter: parent.verticalCenter; width: 3; height: 14; radius: 1.5; color: Theme.c.accent }
                Icon {
                    id: scIc
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    name: sc.index === 0 ? "bookmark" : "list"
                    size: 16
                    color: sc.sel ? Theme.c.accent : Theme.c.textDim
                }
                Text {
                    anchors { left: scIc.right; leftMargin: 10; right: scCount.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                    elide: Text.ElideRight
                    text: sc.index === 0 ? root.tr("All") : sc.name
                    color: Theme.c.text
                    font.pixelSize: 14
                    font.weight: sc.sel ? Font.DemiBold : Font.Normal
                    font.family: Theme.type.family
                }
                Rectangle {
                    id: scCount
                    anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                    width: Math.max(22, scNum.implicitWidth + 12)
                    height: 20
                    radius: 10
                    color: sc.sel ? Qt.rgba(Theme.c.accent.r, Theme.c.accent.g, Theme.c.accent.b, 0.18) : Theme.c.fill3
                    Text {
                        id: scNum
                        anchors.centerIn: parent
                        text: sc.index === 0 ? root.library.length : root.countIn(sc.name)
                        color: sc.sel ? Theme.c.accent : Theme.c.textDim
                        font.pixelSize: 11
                        font.weight: Font.DemiBold
                        font.family: Theme.type.family
                    }
                }
                MouseArea {
                    id: scArea
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    cursorShape: Qt.PointingHandCursor
                    onClicked: function(m) {
                        if (m.button === Qt.RightButton && sc.index > 0) { catMenu.cat = sc.name; catMenu.openAt(scArea, m.x, m.y) }
                        else root.category = sc.name
                    }
                    onPressAndHold: function(m) { if (sc.index > 0) { catMenu.cat = sc.name; catMenu.openAt(scArea, m.x, m.y) } }
                }
            }
        }
        // no category yet: a hint instead of an empty list
        Text {
            visible: root.categories.length === 0
            width: parent.width - 24
            x: 12
            topPadding: 2
            wrapMode: Text.WordWrap
            text: root.tr("Group your titles: tap + to create a category.")
            color: Theme.c.textDim
            font.pixelSize: 12
            font.family: Theme.type.family
        }
    }

    // ================= Library =================
    Item {
        id: libPage
        anchors.fill: parent
        visible: root.tab === "library"
        Rectangle {
            id: side
            visible: root.catsOpen && !root.catsInSide
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            width: visible ? 230 : 0
            color: Theme.c.headerbar
            Rectangle { anchors { right: parent.right; top: parent.top; bottom: parent.bottom } width: 1; color: Theme.c.headerLine }
            Lbl { id: sh; x: 16; y: 14; text: root.tr("Categories"); bold: true; size: 15 }
            ListView {
                anchors { left: parent.left; right: parent.right; top: sh.bottom; topMargin: 8; bottom: addCat.top; bottomMargin: 6 }
                clip: true
                model: root.categories.length + 1
                delegate: Rectangle {
                    required property int index
                    readonly property string name: index === 0 ? "" : root.categories[index - 1]
                    width: ListView.view.width - 16
                    x: 8
                    height: Theme.spacing.navRowH || 40
                    radius: Theme.layout.navIndicator === "pill-left" ? Theme.shape.control : (Theme.shape.small || 8)
                    color: root.category === name ? Theme.c.selected : (ca.containsMouse ? Theme.c.hover : "transparent")
                    Lbl { x: 12; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 60; elide: Text.ElideRight; text: index === 0 ? root.tr("All") : name; size: 15 }
                    Lbl { anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; text: index === 0 ? root.library.length : root.countIn(name); dim: true; size: 13 }
                    MouseArea {
                        id: ca
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        cursorShape: Qt.PointingHandCursor
                        onClicked: function(m) {
                            if (m.button === Qt.RightButton && index > 0) { catMenu.cat = name; catMenu.openAt(ca, m.x, m.y) }
                            else root.category = name
                        }
                    }
                }
            }
            FlatButton { id: addCat; anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: 10; kind: "plain"; glyph: "plus"; text: root.tr("Add category"); onClicked: catPrompt.open() }
        }
        Item {
            anchors { left: side.right; right: parent.right; top: parent.top; bottom: parent.bottom }
            Item { id: libBar; anchors { left: parent.left; right: parent.right; top: parent.top } height: 4 }
            SearchEntry {
                id: libSearch
                visible: root.searchOn
                anchors { top: libBar.bottom; horizontalCenter: parent.horizontalCenter }
                width: Math.min(520, parent.width - 40)
                placeholderText: root.tr("Search the library")
                Keys.onEscapePressed: { root.searchOn = false; text = "" }
            }
            Flickable {
                id: libFlick
                anchors { left: parent.left; right: parent.right; top: root.searchOn ? libSearch.bottom : libBar.bottom; topMargin: 8; bottom: parent.bottom }
                contentWidth: width
                contentHeight: libGrid.height + 40
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                CoverGrid {
                    id: libGrid
                    x: 24
                    y: 4
                    width: libFlick.width - 48
                    model: root.shownLibrary
                    showUnread: true
                    onOpened: function(item) { root.openSeries(item) }
                    onMenu: function(item, card, x, y) {
                        root.menuSeries = item
                        const it = [{key: "cover", label: root.tr("Change the cover…")}]
                        if ((item.cover || "").indexOf("/covers/") >= 0) it.push({key: "uncover", label: root.tr("Use the cover of the source")})
                        it.push({key: "remove", label: root.tr("Remove from library")})
                        const mine = item.categories || []
                        root.categories.forEach(function(c) {
                            const has = mine.indexOf(c) >= 0
                            it.push({key: "cat", cat: c, label: (has ? root.tr("Take out of") : root.tr("Put in")) + " " + c})
                        })
                        libMenu.items = it
                        libMenu.openAt(card, x, y)
                    }
                }
            }
            StatusPage {
                visible: root.shownLibrary.length === 0
                anchors.centerIn: parent
                icon: "bookmark"
                title: root.library.length === 0 ? root.tr("Your library is empty") : root.tr("Nothing here")
                description: root.library.length === 0 ? root.tr("Find a title in Explore and add it to the library.") : root.tr("No title matches this category or search.")
            }
        }
    }

    // ================= Explore =================
    Item {
        id: exPage
        anchors.fill: parent
        visible: root.tab === "explore"

        // --- the list of sources ---
        Flickable {
            id: srcFlick
            visible: !root.browsing
            anchors.fill: parent
            contentWidth: width
            contentHeight: srcCol.height + 48
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Column {
                id: srcCol
                x: Math.max(16, (srcFlick.width - 720) / 2)
                y: 16
                width: Math.min(720, srcFlick.width - 32)
                spacing: 18
                Row {
                    spacing: 8
                    LangButton {
                        options: root.langOptions
                        selected: root.langs
                        onChanged: function(list) { root.setLangs(list) }
                    }
                    FlatButton { kind: "plain"; implicitHeight: 34; glyph: "plus"; text: root.isManga ? root.tr("Image sources") : root.tr("Text sources"); onClicked: plugins.open() }
                }
                Repeater {
                    model: root.groups.length
                    delegate: Column {
                        required property int index
                        readonly property var grp: root.groups[index] || ({items: []})
                        width: srcCol.width
                        spacing: 8
                        Lbl { text: grp.title; bold: true; size: 15; leftPadding: 4 }
                        Rectangle {
                            width: parent.width
                            height: rows.height
                            radius: Theme.shape.card
                            color: Theme.c.card
                            border.width: 1
                            border.color: Theme.c.border
                            clip: true
                            Column {
                                id: rows
                                width: parent.width
                                Repeater {
                                    model: grp.items.length
                                    delegate: Item {
                                        required property int index
                                        readonly property var src: grp.items[index] || ({})
                                        width: rows.width
                                        height: 66
                                        Rectangle { anchors.fill: parent; color: ra.containsMouse ? Theme.c.hover : "transparent" }
                                        Rectangle { visible: index > 0; anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Theme.c.divider }
                                        SourceIcon { id: sic; x: 14; anchors.verticalCenter: parent.verticalCenter; size: 40; url: src.icon || ""; name: src.name || "" }
                                        Column {
                                            anchors { left: sic.right; leftMargin: 14; right: pinBtn.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                                            Lbl { width: parent.width; elide: Text.ElideRight; text: src.name || ""; bold: true; size: 15 }
                                            Lbl { width: parent.width; elide: Text.ElideRight; text: src.lang ? (src.langName || src.lang) : root.tr("Built in"); dim: true; size: 13 }
                                        }
                                        MouseArea { id: ra; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.openSource(src) }
                                        FlatButton {
                                            id: pinBtn
                                            visible: root.isManga
                                            anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                                            kind: "flatround"
                                            checkable: true
                                            checked: root.pins.indexOf(src.id) >= 0
                                            glyph: "pin"
                                            ToolTip.visible: hovered
                                            ToolTip.text: checked ? root.tr("Unpin") : root.tr("Pin to the top")
                                            onClicked: root.togglePin(src.id)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Lbl {
                    visible: root.groups.length === 0
                    width: parent.width
                    wrapMode: Text.Wrap
                    dim: true
                    text: root.isManga ? (root.st.status ? root.tr(root.st.status) : root.tr("The other servers appear when the Suwayomi server is ready.")) : root.tr("No text source installed yet. Use Text sources to install one.")
                }
            }
        }

        // --- the titles of one source ---
        Item {
            id: brPage
            visible: root.browsing
            anchors.fill: parent
            Item {
                id: brBar
                anchors { left: parent.left; right: parent.right; top: parent.top }
                height: 56
                FlatButton {
                    id: brBack
                    anchors { left: parent.left; leftMargin: 10; verticalCenter: parent.verticalCenter }
                    kind: "flat"; glyph: "left"; iconSize: 22; implicitWidth: 40
                    ToolTip.visible: hovered
                    ToolTip.text: root.tr("Back to the sources")
                    onClicked: root.browsing = false
                }
                SourceIcon { id: brIcon; anchors.left: brBack.right; anchors.leftMargin: 6; anchors.verticalCenter: parent.verticalCenter; size: 28; url: root.sourceIcon; name: root.sourceName }
                Lbl { anchors.left: brIcon.right; anchors.leftMargin: 10; anchors.verticalCenter: parent.verticalCenter; text: root.sourceName; bold: true; size: 18; width: Math.min(implicitWidth, brPage.width * 0.25); elide: Text.ElideRight }
                Row {
                    anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
                    spacing: 10
                    SearchEntry {
                        id: exEntry
                        width: Math.max(140, Math.min(300, brPage.width - 520))
                        placeholderText: root.isManga ? root.tr("Search this server") : root.tr("Search novels")
                        onAccepted: if (text.trim() !== "") root.browse("search", text.trim())
                    }
                    Segmented {
                        model: [{id: "popular", label: root.tr("Popular")}, {id: "latest", label: root.tr("Latest")}]
                        current: root.what
                        anchors.verticalCenter: parent.verticalCenter
                        onPicked: function(id) { exEntry.text = ""; root.browse(id, "") }
                    }
                }
            }
            Flickable {
                id: brFlick
                anchors { left: parent.left; right: parent.right; top: brBar.bottom; bottom: parent.bottom }
                contentWidth: width
                contentHeight: brCol.height + 40
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                Column {
                    id: brCol
                    x: 24
                    y: 4
                    width: brFlick.width - 48
                    spacing: 16
                    CoverGrid {
                        width: parent.width
                        model: root.lst.items || []
                        libraryIds: root.libraryIds
                        onOpened: function(item) { root.openSeries(item) }
                    }
                    Lbl {
                        visible: text !== ""
                        anchors.horizontalCenter: parent.horizontalCenter
                        // "The Suwayomi server is starting…" is a wait, not an error: the list comes by itself
                        readonly property bool waiting: (root.lst.error || "") === "The Suwayomi server is starting…"
                        text: root.lst.loading ? root.tr("Loading…") : (root.lst.error ? root.tr(root.lst.error) : ((root.lst.items || []).length === 0 ? root.tr("Nothing found.") : ""))
                        color: root.lst.error && !root.lst.loading && !waiting ? Theme.c.error : Theme.c.textDim
                        wrapMode: Text.Wrap
                        width: Math.min(implicitWidth, parent.width)
                        horizontalAlignment: Text.AlignHCenter
                    }
                    FlatButton {
                        visible: root.blocked && root.canCheck
                        anchors.horizontalCenter: parent.horizontalCenter
                        kind: "suggested"
                        glyph: "globe"
                        text: root.tr("Open the site to pass its check")
                        onClicked: siteCheck.active = true
                    }
                    Lbl {
                        visible: root.blocked && !root.canCheck
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Math.min(implicitWidth, parent.width)
                        wrapMode: Text.Wrap
                        dim: true
                        text: root.tr("The site blocks the app with a bot check. Pick another source of this title.")
                    }
                    FlatButton {
                        visible: !!root.lst.more && !root.lst.loading
                        anchors.horizontalCenter: parent.horizontalCenter
                        kind: "plain"
                        pill: true
                        text: root.tr("Load more")
                        onClicked: Core.cmd({cmd: root.cmdName("more")})
                    }
                }
            }
        }
    }

    // ================= History =================
    SearchEntry {
        id: hisSearch
        visible: root.tab === "history" && root.hisSearchOn
        anchors { top: parent.top; topMargin: 12; horizontalCenter: parent.horizontalCenter }
        width: Math.min(520, parent.width - 40)
        placeholderText: root.tr("Search the history")
        Keys.onEscapePressed: { root.hisSearchOn = false; text = "" }
    }
    Flickable {
        id: hisFlick
        visible: root.tab === "history"
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; top: root.hisSearchOn ? hisSearch.bottom : parent.top; topMargin: root.hisSearchOn ? 4 : 0 }
        contentWidth: width
        contentHeight: hisCol.height + 48
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        Column {
            id: hisCol
            x: Math.max(16, (hisFlick.width - 640) / 2)
            y: 16
            width: Math.min(640, hisFlick.width - 32)
            spacing: 8
            Repeater {
                model: root.historyDays.length
                delegate: Column {
                    required property int index
                    readonly property var dayData: root.historyDays[index] || ({day: "", items: []})
                    width: hisCol.width
                    spacing: 8
                    Lbl { text: dayData.day; bold: true; size: 15; topPadding: index > 0 ? 10 : 0; leftPadding: 4 }
                    Rectangle {
                        width: parent.width
                        height: hrows.height
                        radius: Theme.shape.card
                        color: Theme.c.card
                        border.width: 1
                        border.color: Theme.c.border
                        clip: true
                        Column {
                            id: hrows
                            width: parent.width
                            Repeater {
                                model: dayData.items.length
                                delegate: Item {
                                    required property int index
                                    readonly property var h: dayData.items[index] || ({})
                                    width: hrows.width
                                    height: 76
                                    Rectangle { anchors.fill: parent; color: hm.containsMouse ? Theme.c.hover : "transparent" }
                                    Rectangle { visible: index > 0; anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Theme.c.divider }
                                    MouseArea { id: hm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.openSeries({id: h.seriesId, title: h.title, cover: h.cover}) }
                                    RoundedPicture { id: hc; x: 14; anchors.verticalCenter: parent.verticalCenter; width: 40; height: 56; radius: 6; url: h.cover || "" }
                                    Column {
                                        anchors { left: hc.right; leftMargin: 14; right: ht.left; rightMargin: 10; verticalCenter: parent.verticalCenter }
                                        spacing: 2
                                        Lbl { width: parent.width; elide: Text.ElideRight; text: h.title || ""; bold: true; size: 15 }
                                        Lbl { width: parent.width; elide: Text.ElideRight; text: h.label || ""; dim: true; size: 13 }
                                    }
                                    Lbl { id: ht; anchors.right: hp.left; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter; text: Qt.formatTime(new Date((h.at || 0) * 1000), "HH:mm"); dim: true; size: 13 }
                                    FlatButton {
                                        id: hp
                                        anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                                        kind: "plain"
                                        implicitWidth: 36; implicitHeight: 36
                                        glyph: "play"
                                        ToolTip.visible: hovered
                                        ToolTip.text: root.isManga ? root.tr("Continue reading") : root.tr("Continue reading")
                                        onClicked: { root.autoRead = h.chapterId; root.openSeries({id: h.seriesId, title: h.title, cover: h.cover}) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    StatusPage {
        visible: root.tab === "history" && (root.st.history || []).length === 0
        anchors.centerIn: parent
        icon: "history"
        title: root.tr("Nothing read yet")
        description: root.tr("The chapters you open show up here.")
    }

    // ---- the buttons of each tab live in the header bar (slot of AppHeader), grouped on the right ----
    property Item slot: null
    // While a chapter is open the reader's buttons go into the left sidebar instead: Main hands that place down
    // (win.readerSideSlot, null when no sidebar or rail is shown) and the readers below put their buttons there.
    property Item sideSlot: null
    // the sidebar's place for the Library's categories (Main.librarySideSlot); without it the old side panel shows
    property Item catSlot: null
    readonly property bool catsInSide: catSlot !== null
    Row {
        id: libActs
        parent: root.slot ? root.slot : root
        visible: root.visible && !root.inView && !root.inReader
        spacing: 2
        FlatButton {
            visible: root.tab === "library" && !root.catsInSide
            kind: root.catsOpen ? "plain" : "flat"
            glyph: "bookmark"
            implicitWidth: 40
            ToolTip.visible: hovered
            ToolTip.text: root.tr("Categories")
            onClicked: root.catsOpen = !root.catsOpen
        }
        FlatButton {
            id: addBtn
            visible: root.tab === "library" && !root.isManga
            kind: "flat"; glyph: "plus"; implicitWidth: 40
            ToolTip.visible: hovered && !addMenu.visible
            ToolTip.text: root.tr("Add a book")
            onClicked: addMenu.openBelow(addBtn, true)
        }
        FlatButton { visible: root.tab === "library" && root.isManga; kind: "flat"; glyph: "refresh"; implicitWidth: 40; ToolTip.visible: hovered; ToolTip.text: root.tr("Look for new chapters"); onClicked: Core.cmd({cmd: "manga_refresh_library"}) }
        FlatButton {
            visible: root.tab !== "explore"
            kind: (root.tab === "library" ? root.searchOn : root.hisSearchOn) ? "plain" : "flat"
            glyph: "search"
            implicitWidth: 40
            ToolTip.visible: hovered
            ToolTip.text: root.tab === "library" ? root.tr("Search the library") : root.tr("Search the history")
            onClicked: root.doSearch()
        }
    }
    PopMenu {
        id: addMenu
        items: [{label: root.tr("Import EPUB")}, {label: root.tr("New book")}, {label: root.tr("Text sources")}]
        onPicked: function(i) { if (i === 0) epubDlg.open(); else if (i === 1) newBookPrompt.open(); else plugins.open() }
    }

    // ---- the series page and the reader sit above everything ----
    SeriesPage {
        onCoverRequested: function(id) { root.pickCover(id) }
        anchors.fill: parent
        kind: root.kind
        visible: root.inView && !root.inReader
        z: 2
    }
    MangaReaderPage {
        slot: root.slot
        sideSlot: root.isManga ? root.sideSlot : null
        anchors.fill: parent
        visible: root.isManga && root.inReader
        z: 3
    }
    NovelReaderPage {
        slot: root.slot
        sideSlot: root.isManga ? null : root.sideSlot
        anchors.fill: parent
        visible: !root.isManga && root.inReader
        z: 3
    }

    // the site's bot check, over the whole page (Android only: the file is built into the Android app)
    Loader {
        id: siteCheck
        active: false
        anchors.fill: parent
        z: 100
        source: active ? "qrc:/qml-android/SiteCheck.qml" : ""
        onLoaded: item.url = root.sourceSite
        Connections {
            target: siteCheck.item
            ignoreUnknownSignals: true
            function onFinished(ok) {
                siteCheck.active = false
                if (ok) root.browse(root.what, root.what === "search" ? exEntry.text : "")
            }
        }
    }
}
