import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Research mode, the built-in browser. The browser's controls (address and search field, back / forward / reload,
// the pinned Essentials, the vertical tabs, downloads / history / extensions) live in the app's own sidebar
// (ResearchSidebar.qml, loaded there in this mode); this page is the web page itself, filling the content area, with
// the side sheets (ResearchSheet) sliding over its right edge, the page's menu and its alert / confirm / prompt.
// Pages are drawn by Chromium (CEF) off screen into a WebPane, so all of this is ordinary QML over the page. The core
// keeps the history, the downloads and the settings (src/browser.rs), shared with the player's quick search.
// Without a sidebar (Android's bottom bar on narrow windows) a slim bar on top of the page holds the controls.
Item {
    id: rp
    property var bs: ({})          // Core.state("browser"): engine, pins, saved tabs, settings
    property var dls: []           // Core.state("browser:downloads").items, newest first
    property string sheet: ""      // "" | downloads | history | extensions | settings
    property string lastSheet: ""  // what the sheet shows while it slides away
    onSheetChanged: if (sheet !== "") lastSheet = sheet
    property bool started: false
    // the ResearchSidebars showing this page's controls (they register themselves); without a visible one the slim
    // bar shows
    property var sidebars: []
    readonly property bool hasSidebar: sidebars.some(function(s) { return s.visible })
    readonly property alias pane: pane
    readonly property int activeDownloads: dls.filter(function(d) { return d.state === "progress" || d.state === "paused" }).length
    // the address field (in the sidebar, or the slim bar) is asked to take the keyboard
    signal addressFocusRequested()
    // the look of each theme: Safari's page on a card, Edge's rounded corner by the pane, flush in GNOME and Android
    readonly property bool apple: Theme.look === "apple"
    readonly property bool windows: Theme.look === "windows"
    readonly property bool android: Theme.look === "android"
    readonly property real inset: apple ? 8 : 0
    readonly property real pageRadius: apple ? 12 : (windows ? Theme.shape.large : 0)

    function tr(t) { return Core.tr(Core.uiLang, t) }
    function refresh() { bs = Core.state("browser") }
    function refreshDownloads() { dls = Core.state("browser:downloads").items || [] }
    // What the address field means (an address or a search with the chosen engine), worked out by the core.
    function addressFor(text) {
        Core.cmd({cmd: "browser_address", text: text})
        return Core.state("browser:address").address || ""
    }
    function go(text) {
        const u = addressFor(text)
        if (u === "") return
        start()
        pane.navigate(u)
        pane.focusPage()
    }
    function newTab(url) {
        start()
        pane.openTab(url ? url : (bs.home || "https://www.google.com/"), true)
        if (!url) focusAddress()
    }
    // From the quick search over the video ("Continue in Research mode").
    function openUrl(url) { start(); pane.openTab(url, true) }
    function focusAddress() { if (hasSidebar) addressFocusRequested(); else slimField.forceActiveFocus() }
    function closeCurrent() { if (pane.currentTab !== 0) pane.closeTab(pane.currentTab) }
    function cycle(step) {
        const t = pane.tabs
        if (t.length < 2) return
        var at = 0
        for (var i = 0; i < t.length; ++i) if (t[i].id === pane.currentTab) at = i
        pane.currentTab = t[(at + step + t.length) % t.length].id
    }
    function isPinned(url) { return (bs.pins || []).some(function(p) { return p.url === url }) }
    function pin(url, title, on) { Core.cmd({cmd: "browser_pin", url: url, title: title, on: on}); refresh() }
    // An essential: the tab already showing that site, else a new tab.
    function openPin(p) {
        const host = hostOf(p.url)
        const t = pane.tabs
        for (var i = 0; i < t.length; ++i) if (hostOf(t[i].url) === host) { pane.currentTab = t[i].id; return }
        newTab(p.url)
    }
    function hostOf(u) {
        const m = /^[a-z]+:\/\/([^\/?#]+)/i.exec(u || "")
        return m ? m[1].replace(/^www\./, "") : (u || "")
    }
    // What the address field shows while it is not edited: the site of a web page, nothing for inner pages
    // (data:, about:blank), the address itself otherwise (file:…).
    function siteOf(u) {
        if (!u || /^(data|about|blob|javascript):/i.test(u)) return ""
        return /^https?:/i.test(u) ? hostOf(u) : u
    }
    function toggleSheet(name) { sheet = sheet === name ? "" : name }
    // The menu of a tab (the sidebar's right click): opened here, where the actions are.
    function openTabMenu(tab, item, x, y) { tabMenu.target = tab; tabMenu.openAt(item, x, y) }
    function openPinMenu(p, item, x, y) { pinMenu.target = p; pinMenu.openAt(item, x, y) }
    // The first time the mode is shown: the tabs of last time (or the start page). Chromium starts only then.
    function start() {
        if (started) return
        started = true
        refresh()
        refreshDownloads()
        // without a page view, nothing opens by itself in the system browser: only what the user asks for
        if (!pane.available) return
        const saved = bs.restoreTabs !== false ? (bs.tabs || []) : []
        if (saved.length === 0) { pane.openTab(bs.home || "https://www.google.com/", true); return }
        for (var i = 0; i < saved.length; ++i) pane.openTab(saved[i].url, i === 0)
    }
    onVisibleChanged: if (visible) { start(); refresh() }
    // the app started in Research mode
    Component.onCompleted: if (visible) Qt.callLater(start)

    // The open tabs are written down (a moment after they change) to open them again next time.
    Timer {
        id: saveTabs
        interval: 1500
        onTriggered: Core.cmd({cmd: "browser_tabs", tabs: pane.tabs.filter(function(t) { return t.url !== "" && !/^data:/.test(t.url) }).map(function(t) { return {url: t.url, title: t.title} })})
    }

    // ---- the slim bar: the controls when there is no sidebar ----
    Rectangle {
        id: slim
        visible: !rp.hasSidebar
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: visible ? 48 : 0
        color: Theme.c.headerbar
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Theme.c.divider }
        Row {
            id: slimNav
            anchors { left: parent.left; leftMargin: 6; verticalCenter: parent.verticalCenter }
            FlatButton { kind: "flat"; glyph: "left"; implicitWidth: 36; implicitHeight: 34; enabled: pane.canGoBack; onClicked: pane.back() }
            FlatButton { kind: "flat"; glyph: "right"; implicitWidth: 36; implicitHeight: 34; enabled: pane.canGoForward; onClicked: pane.forward() }
            FlatButton { kind: "flat"; glyph: pane.loading ? "close" : "refresh"; implicitWidth: 36; implicitHeight: 34; onClicked: pane.loading ? pane.stop() : pane.reload() }
        }
        TextField {
            id: slimField
            inputMethodHints: Qt.platform.os === "android" ? Qt.ImhNoPredictiveText | Qt.ImhUrlCharactersOnly : Qt.ImhNone   // see SearchEntry
            anchors { left: slimNav.right; leftMargin: 6; right: slimTabs.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
            height: 36
            color: Theme.c.text
            placeholderText: rp.tr("Search or type an address")
            placeholderTextColor: Theme.c.textDim
            font.pixelSize: 14
            font.family: Theme.type.family
            selectByMouse: true
            leftPadding: 14
            function showSite() { if (!activeFocus) { text = rp.siteOf(pane.url); cursorPosition = 0 } }
            onActiveFocusChanged: { if (activeFocus) { text = pane.url; selectAll() } else showSite() }
            Connections { target: pane; function onCurrentChanged() { slimField.showSite() } }
            onAccepted: rp.go(text)
            background: Rectangle { radius: height / 2; color: Theme.c.fill3; border.width: slimField.activeFocus ? 2 : 0; border.color: Theme.c.accent }
        }
        Row {
            id: slimTabs
            anchors { right: parent.right; rightMargin: 6; verticalCenter: parent.verticalCenter }
            // the number of tabs (Chrome's square): opens the list
            FlatButton {
                id: tabsBtn
                kind: "flat"; implicitWidth: 40; implicitHeight: 34
                Rectangle { anchors.centerIn: parent; width: 22; height: 22; radius: 5; color: "transparent"; border.width: 2; border.color: Theme.c.text
                    Text { anchors.centerIn: parent; text: pane.tabs.length; color: Theme.c.text; font.pixelSize: 11; font.weight: Font.Bold; font.family: Theme.type.family } }
                onClicked: slimTabMenu.openBelow(tabsBtn, true)
            }
            FlatButton { kind: "flat"; glyph: "plus"; implicitWidth: 36; implicitHeight: 34; onClicked: rp.newTab("") }
            FlatButton { id: slimMore; kind: "flat"; glyph: "more"; implicitWidth: 36; implicitHeight: 34; onClicked: slimMoreMenu.openBelow(slimMore, true) }
        }
        PopMenu {
            id: slimTabMenu
            menuWidth: 280
            items: pane.tabs.map(function(t) { return {label: t.title || t.url || rp.tr("New tab"), checked: t.id === pane.currentTab} })
            onPicked: function(i) { pane.currentTab = pane.tabs[i].id }
        }
        PopMenu {
            id: slimMoreMenu
            items: [{label: rp.tr("Downloads")}, {label: rp.tr("History")}, {label: rp.tr("Browser settings")}, {label: rp.tr("Close tab"), danger: true}]
            onPicked: function(i) { if (i === 0) rp.toggleSheet("downloads"); else if (i === 1) rp.toggleSheet("history"); else if (i === 2) rp.toggleSheet("settings"); else rp.closeCurrent() }
        }
    }

    // ---- the page ----
    Item {
        id: pageArea
        anchors { left: parent.left; right: parent.right; top: slim.bottom; bottom: parent.bottom; margins: rp.inset; topMargin: rp.hasSidebar ? 0 : rp.inset }
        // the frame of the page (seen while a page loads, around its rounded corners and without an engine)
        Rectangle {
            anchors.fill: parent
            radius: rp.pageRadius
            color: Theme.c.view
            border.width: rp.apple || rp.windows ? 1 : 0
            border.color: Theme.c.border
        }
        WebPane {
            id: pane
            anchors.fill: parent
            anchors.margins: rp.apple || rp.windows ? 1 : 0
            radius: Math.max(0, rp.pageRadius - 1)
            profile: rp.bs.profileDir || ""
            language: Core.uiLang || "en"
            // on Android the page is a native view over the app: hidden while a side sheet is open over it (and, in WebPane,
            // while any popup such as the mode menu overlaps it)
            covered: rp.sheet !== ""
            onTabsChanged: if (rp.started) saveTabs.restart()
            onVisited: function(url, title) { Core.cmd({cmd: "browser_visit", url: url, title: title, kind: "page"}) }
            onTitleSeen: function(url, title) { Core.cmd({cmd: "browser_title", url: url, title: title}) }
            onDownload: function(d) {
                Core.cmd({cmd: "browser_dl", id: d.id, url: d.url, path: d.path, received: d.received, total: d.total, state: d.state})
                rp.refreshDownloads()
                if (d.state === "progress" && d.received === 0) Core.toast(rp.tr("Downloading") + " " + d.path.replace(/^.*[\\\/]/, ""))
                if (d.state === "done") Core.toast(rp.tr("Download finished") + ": " + d.path.replace(/^.*[\\\/]/, ""))
            }
            onMessage: function(text) { Core.toast(rp.tr(text)) }
            onKeyPressed: function(name) { rp.key(name) }
            onContextMenu: function(info) { pageMenu.show(info) }
            onJsDialog: function(id, info) { jsDlg.ask(id, info) }
            onJsDialogClosed: function(id) { if (jsDlg.dialogId === id) jsDlg.close() }
        }
        // page loading: a thin bar along the top edge
        Rectangle {
            visible: pane.loading
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: rp.pageRadius; rightMargin: rp.pageRadius }
            height: 2
            color: "transparent"
            clip: true
            Rectangle {
                height: 2; radius: 1
                color: Theme.c.accent
                width: parent.width * 0.3
                SequentialAnimation on x {
                    running: pane.loading && pageArea.visible
                    loops: Animation.Infinite
                    NumberAnimation { from: -pageArea.width * 0.3; to: pageArea.width; duration: 1100; easing.type: Easing.InOutQuad }
                }
            }
        }
        // the address of the link under the pointer, at the bottom left (as browsers do)
        Rectangle {
            visible: pane.statusText !== ""
            anchors { left: parent.left; bottom: parent.bottom; leftMargin: Math.max(6, rp.pageRadius / 2); bottomMargin: Math.max(6, rp.pageRadius / 2) }
            width: Math.min(statusLbl.implicitWidth + 16, parent.width * 0.6)
            height: 24
            radius: rp.android ? 12 : Theme.shape.small
            color: Theme.c.popover
            border.width: 1
            border.color: Theme.c.border
            Text { id: statusLbl; anchors { fill: parent; leftMargin: 8; rightMargin: 8 } verticalAlignment: Text.AlignVCenter; elide: Text.ElideMiddle; text: pane.statusText; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
        }
        // the page could not be opened: why, and a way to try again
        Rectangle {
            id: failBox
            readonly property var f: pane.failure
            visible: f.url !== undefined && pane.available
            anchors.centerIn: parent
            width: Math.min(440, parent.width - 40)
            height: failCol.implicitHeight + 40
            radius: Theme.shape.card
            color: Theme.c.card
            border.width: 1
            border.color: Theme.c.border
            Column {
                id: failCol
                anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
                spacing: 10
                Icon { anchors.horizontalCenter: parent.horizontalCenter; name: "globe"; size: 40; color: Theme.c.textDim }
                Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; bold: true; size: 18; text: rp.tr("This page could not be opened") }
                Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WrapAnywhere; dim: true; size: 13; text: rp.hostOf(failBox.f.url || "") + (failBox.f.error ? " · " + rp.tr(failBox.f.error) : "") }
                Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: 8
                    FlatButton { kind: "suggested"; text: rp.tr("Try again"); onClicked: pane.reload() }
                    FlatButton { kind: "plain"; text: rp.tr("Open in the system browser"); onClicked: Qt.openUrlExternally(pane.failure.url || pane.url) }
                }
            }
        }
        // no Chromium next to the program: what to do instead
        Column {
            visible: !pane.available
            anchors.centerIn: parent
            width: Math.min(460, parent.width - 40)
            spacing: 12
            Icon { anchors.horizontalCenter: parent.horizontalCenter; name: "globe"; size: 48; color: Theme.c.textDim }
            Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; bold: true; size: 20; text: rp.tr("The built-in browser is not available") }
            Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; dim: true; text: rp.tr(pane.problem) }
            Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; dim: true; text: rp.tr("Pages open in your system browser, and the history is still kept here.") }
        }
    }

    // ---- downloads, history, extensions and settings: a sheet over the right edge of the page ----
    ResearchSheet {
        id: sheetBox
        readonly property bool open: rp.sheet !== ""
        visible: x < rp.width - 1
        anchors { top: pageArea.top; bottom: pageArea.bottom; topMargin: 8; bottomMargin: 8 }
        width: Math.min(380, Math.max(300, rp.width * 0.32))
        x: open ? rp.width - width - 8 - rp.inset : rp.width + 12
        Behavior on x { NumberAnimation { duration: Theme.motion.normal; easing.type: Easing.OutCubic } }
        kind: rp.lastSheet
        page: rp
        pane: pane
        onClosed: rp.sheet = ""
        // lifted off the page
        Rectangle { anchors.fill: parent; anchors.margins: -1; z: -1; radius: parent.radius + 1; color: "#26000000"; anchors.topMargin: 2; anchors.bottomMargin: -3 }
    }

    // ---- the page's menu (right click): Chromium's own is replaced by this one ----
    PopMenu {
        id: pageMenu
        property var info: ({})
        property var acts: []
        menuWidth: 260
        function show(i) {
            info = i
            const a = [], labels = []
            function add(label, act, danger) { a.push(act); labels.push({label: label, danger: !!danger}) }
            if (i.link) {
                add(rp.tr("Open link in a new tab"), "openLink")
                add(rp.tr("Copy link address"), "copyLink")
                add(rp.tr("Save link"), "saveLink")
            }
            if (i.image) {
                add(rp.tr("Open image in a new tab"), "openImage")
                add(rp.tr("Copy image address"), "copyImage")
                add(rp.tr("Save image"), "saveImage")
            }
            if (i.selection) {
                add(rp.tr("Copy"), "copy")
                add(rp.tr("Search for “%1”").replace("%1", i.selection.length > 24 ? i.selection.slice(0, 24) + "…" : i.selection), "search")
            }
            if (i.editable) {
                if (i.canUndo) add(rp.tr("Undo"), "undo")
                if (i.canCut) add(rp.tr("Cut"), "cut")
                if (i.canCopy && !i.selection) add(rp.tr("Copy"), "copy")
                if (i.canPaste) add(rp.tr("Paste"), "paste")
                add(rp.tr("Select all"), "selectAll")
            }
            if (!i.link && !i.image && !i.selection && !i.editable) {
                if (pane.canGoBack) add(rp.tr("Back"), "back")
                if (pane.canGoForward) add(rp.tr("Forward"), "forward")
                add(rp.tr("Reload"), "reload")
                add(rp.isPinned(pane.url) ? rp.tr("Remove from Essentials") : rp.tr("Add to Essentials"), "pin")
                add(rp.tr("Open in the system browser"), "external")
            }
            acts = a
            items = labels
            openAt(pane, i.x, i.y)
        }
        onPicked: function(k) {
            const i = info
            switch (acts[k]) {
            case "openLink": pane.openTab(i.link, false); break
            case "copyLink": pane.copyText(i.link); break
            case "saveLink": pane.downloadUrl(i.link); break
            case "openImage": pane.openTab(i.image, false); break
            case "copyImage": pane.copyText(i.image); break
            case "saveImage": pane.downloadUrl(i.image); break
            case "search": pane.openTab(rp.addressFor(i.selection), true); break
            case "back": pane.back(); break
            case "forward": pane.forward(); break
            case "reload": pane.reload(); break
            case "pin": rp.pin(pane.url, pane.title, !rp.isPinned(pane.url)); break
            case "external": Qt.openUrlExternally(pane.url); break
            default: pane.editCommand(acts[k])   // copy, cut, paste, undo, selectAll
            }
        }
    }

    // ---- a page's alert / confirm / prompt ----
    Popup {
        id: jsDlg
        property int dialogId: 0
        property var info: ({})
        function ask(id, i) { dialogId = id; info = i; jsInput.text = i.value || ""; open(); if (i.kind === "prompt") jsInput.forceActiveFocus() }
        function answer(ok) { const id = dialogId; dialogId = 0; close(); pane.answerDialog(id, ok, jsInput.text) }
        parent: pageArea
        anchors.centerIn: parent
        width: Math.min(420, pageArea.width - 40)
        modal: true
        dim: true
        closePolicy: Popup.CloseOnEscape
        padding: 20
        onClosed: if (dialogId !== 0) { const id = dialogId; dialogId = 0; pane.answerDialog(id, false, "") }
        background: Rectangle { radius: Theme.shape.sheet !== undefined ? Math.min(Theme.shape.sheet, 16) : 12; color: Theme.c.popover; border.width: 1; border.color: Theme.c.border }
        contentItem: Column {
            spacing: 12
            Lbl { width: parent.width; elide: Text.ElideMiddle; dim: true; size: 12; text: rp.tr("%1 says").replace("%1", rp.hostOf(jsDlg.info.origin || "")) }
            Lbl { width: parent.width; wrapMode: Text.Wrap; text: jsDlg.info.message || ""; size: 15 }
            TextField {
                id: jsInput
                visible: jsDlg.info.kind === "prompt"
                width: parent.width
                color: Theme.c.text
                font.family: Theme.type.family
                selectByMouse: true
                onAccepted: jsDlg.answer(true)
                background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: jsInput.activeFocus ? 2 : 0; border.color: Theme.c.accent }
            }
            Row {
                anchors.right: parent.right
                spacing: 8
                FlatButton { visible: jsDlg.info.kind !== "alert"; kind: "plain"; text: rp.tr("Cancel"); onClicked: jsDlg.answer(false) }
                FlatButton { kind: "suggested"; text: rp.tr("OK"); onClicked: jsDlg.answer(true) }
            }
        }
    }

    // ---- menus of the sidebar's tabs and Essentials ----
    PopMenu {
        id: tabMenu
        property var target: ({})
        items: [
            {label: rp.isPinned(target.url) ? rp.tr("Remove from Essentials") : rp.tr("Add to Essentials")},
            {label: rp.tr("Duplicate tab")},
            {label: rp.tr("Open in the system browser")},
            {label: rp.tr("Close other tabs")},
            {label: rp.tr("Close tab"), danger: true}
        ]
        onPicked: function(i) {
            const t = target
            if (i === 0) rp.pin(t.url, t.title, !rp.isPinned(t.url))
            else if (i === 1) pane.openTab(t.url, true)
            else if (i === 2) Qt.openUrlExternally(t.url)
            else if (i === 3) { const ids = pane.tabs.map(function(x) { return x.id }); for (var k = 0; k < ids.length; ++k) if (ids[k] !== t.id) pane.closeTab(ids[k]) }
            else if (i === 4) pane.closeTab(t.id)
        }
    }
    PopMenu {
        id: pinMenu
        property var target: ({})
        items: [{label: rp.tr("Open in a new tab")}, {label: rp.tr("Remove from Essentials"), danger: true}]
        onPicked: function(i) { if (i === 0) rp.newTab(target.url); else rp.pin(target.url, target.title, false) }
    }

    // ---- keys (the page gives typing and editing keys to itself, the rest reach these Shortcuts) ----
    function key(name) {
        switch (name) {
        case "Ctrl+T": rp.newTab(""); break
        case "Ctrl+W": rp.closeCurrent(); break
        case "Ctrl+L": rp.focusAddress(); break
        case "Ctrl+Tab": rp.cycle(1); break
        case "Ctrl+Shift+Tab": rp.cycle(-1); break
        case "Escape": if (rp.sheet !== "") rp.sheet = ""; break
        }
    }
    Shortcut { enabled: rp.visible; sequence: "Ctrl+T"; onActivated: rp.key("Ctrl+T") }
    Shortcut { enabled: rp.visible; sequence: "Ctrl+W"; onActivated: rp.key("Ctrl+W") }
    // (Ctrl+F comes from Main.qml, which sends it here in this mode)
    Shortcut { enabled: rp.visible; sequences: ["Ctrl+L", "F6", "Alt+D"]; onActivated: rp.focusAddress() }
    Shortcut { enabled: rp.visible; sequence: "Ctrl+Tab"; onActivated: rp.cycle(1) }
    Shortcut { enabled: rp.visible; sequence: "Ctrl+Shift+Tab"; onActivated: rp.cycle(-1) }
    Shortcut { enabled: rp.visible; sequences: ["F5", "Ctrl+R"]; onActivated: pane.reload() }
    Shortcut { enabled: rp.visible; sequences: ["Alt+Left", "Back"]; onActivated: pane.back() }
    Shortcut { enabled: rp.visible; sequences: ["Alt+Right", "Forward"]; onActivated: pane.forward() }
    Shortcut { enabled: rp.visible; sequence: "Ctrl+H"; onActivated: rp.toggleSheet("history") }
    Shortcut { enabled: rp.visible; sequence: "Ctrl+J"; onActivated: rp.toggleSheet("downloads") }
    Shortcut { enabled: rp.visible; sequence: "Ctrl+D"; onActivated: if (pane.url !== "") rp.pin(pane.url, pane.title, !rp.isPinned(pane.url)) }
}
