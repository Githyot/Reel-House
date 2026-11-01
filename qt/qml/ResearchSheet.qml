import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// The side sheet of Research mode: downloads, history, extensions or the browser's settings. It slides over the right
// edge of the page (pages are drawn in the scene by Chromium off screen, so QML can sit on top of them).
Rectangle {
    id: sh
    property string kind: ""          // downloads | history | extensions | settings
    property var page: null           // ResearchPage
    property var pane: null           // its WebPane
    property var hist: ({})           // Core.state("browser:history")
    // what the lists read (safe before `page` and `pane` are set)
    readonly property var dls: page ? page.dls : []
    readonly property var bsx: page ? page.bs : ({})
    readonly property var exts: pane ? pane.extensions : []
    readonly property bool extOk: pane ? pane.extensionsSupported : true
    readonly property bool avail: pane ? pane.available : false
    signal closed()
    radius: Theme.shape.card
    color: Theme.c.view
    border.width: 1
    border.color: Theme.c.border
    clip: true

    function tr(t) { return Core.tr(Core.uiLang, t) }
    function refreshHistory() { hist = Core.state("browser:history") }
    function fmtBytes(b) {
        if (!b || b <= 0) return "0 B"
        const u = ["B", "KB", "MB", "GB", "TB"]
        var i = 0
        while (b >= 1024 && i < u.length - 1) { b /= 1024; ++i }
        return (i === 0 ? b : b.toFixed(b < 10 ? 1 : 0)) + " " + u[i]
    }
    function hostOf(u) {
        const m = /^[a-z]+:\/\/([^\/?#]+)/i.exec(u || "")
        return m ? m[1].replace(/^www\./, "") : (u || "")
    }
    function dayOf(secs) {
        const d = new Date(secs * 1000), now = new Date()
        const start = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() / 1000
        if (secs >= start) return tr("Today")
        if (secs >= start - 86400) return tr("Yesterday")
        return d.toLocaleDateString(Qt.locale(Core.uiLang), Locale.LongFormat)
    }
    function stateText(st) {
        return ({progress: tr("Downloading"), paused: tr("Paused"), done: tr("Done"), failed: tr("Failed"), cancelled: tr("Cancelled"), interrupted: tr("Interrupted")})[st] || st
    }
    onKindChanged: {
        if (kind === "history") { histSearch.text = ""; Core.cmd({cmd: "browser_history_query", q: ""}); refreshHistory() }
        if (kind === "extensions" && pane) pane.refreshExtensions()
    }
    onVisibleChanged: if (visible && kind === "history") refreshHistory()

    // ---- header ----
    Item {
        id: head
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: 52
        Lbl {
            anchors { left: parent.left; leftMargin: 16; verticalCenter: parent.verticalCenter }
            bold: true
            size: 17
            text: ({downloads: sh.tr("Downloads"), history: sh.tr("History"), extensions: sh.tr("Extensions"), settings: sh.tr("Browser settings")})[sh.kind] || ""
        }
        FlatButton { anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter } kind: "flat"; glyph: "close"; implicitWidth: 36; implicitHeight: 34; onClicked: sh.closed() }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Theme.c.divider }
    }

    // ---- downloads ----
    Item {
        visible: sh.kind === "downloads"
        anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom }
        Lbl {
            visible: sh.dls.length === 0
            anchors.centerIn: parent
            width: parent.width - 40
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            dim: true
            text: sh.tr("Files you download in Research mode show here.")
        }
        ListView {
            id: dlList
            anchors { fill: parent; margins: 8; bottomMargin: 52 }
            clip: true
            spacing: 4
            model: sh.dls.length
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            delegate: Rectangle {
                id: dlRow
                required property int index
                readonly property var d: sh.dls[index] || ({})
                readonly property bool running: d.state === "progress" || d.state === "paused"
                width: dlList.width
                height: dlCol.implicitHeight + 16
                radius: Theme.shape.medium
                color: dlArea.containsMouse ? Theme.c.hover : "transparent"
                MouseArea {
                    id: dlArea
                    anchors.fill: parent
                    hoverEnabled: true
                    onDoubleClicked: if (dlRow.d.state === "done") sh.pane.openFile(dlRow.d.path)
                }
                Column {
                    id: dlCol
                    anchors { left: parent.left; right: dlBtns.left; top: parent.top; margins: 8; rightMargin: 4 }
                    spacing: 4
                    Lbl { width: parent.width; elide: Text.ElideMiddle; text: dlRow.d.name || ""; size: 14; bold: true }
                    Rectangle {
                        visible: dlRow.running
                        width: parent.width; height: 4; radius: 2
                        color: Theme.c.fill3
                        Rectangle {
                            height: parent.height; radius: 2
                            color: dlRow.d.state === "paused" ? Theme.c.textDim : Theme.c.accent
                            width: dlRow.d.total > 0 ? parent.width * Math.min(1, dlRow.d.received / dlRow.d.total) : parent.width * 0.15
                        }
                    }
                    Lbl {
                        width: parent.width
                        elide: Text.ElideRight
                        dim: true
                        size: 12
                        text: sh.stateText(dlRow.d.state) + " · " + (dlRow.running && dlRow.d.total > 0 ? sh.fmtBytes(dlRow.d.received) + " / " + sh.fmtBytes(dlRow.d.total) : sh.fmtBytes(Math.max(dlRow.d.received || 0, dlRow.d.total || 0))) + " · " + sh.hostOf(dlRow.d.url)
                    }
                }
                Row {
                    id: dlBtns
                    anchors { right: parent.right; rightMargin: 4; verticalCenter: parent.verticalCenter }
                    spacing: 0
                    FlatButton { visible: dlRow.running; kind: "flat"; glyph: dlRow.d.state === "paused" ? "play" : "pause"; implicitWidth: 32; implicitHeight: 32; ToolTip.visible: hovered; ToolTip.text: dlRow.d.state === "paused" ? sh.tr("Resume") : sh.tr("Pause"); onClicked: dlRow.d.state === "paused" ? sh.pane.resumeDownload(dlRow.d.id) : sh.pane.pauseDownload(dlRow.d.id) }
                    FlatButton { visible: dlRow.running; kind: "flat"; glyph: "close"; implicitWidth: 32; implicitHeight: 32; ToolTip.visible: hovered; ToolTip.text: sh.tr("Cancel"); onClicked: sh.pane.cancelDownload(dlRow.d.id) }
                    FlatButton { visible: dlRow.d.state === "done"; kind: "flat"; glyph: "folder"; implicitWidth: 32; implicitHeight: 32; ToolTip.visible: hovered; ToolTip.text: sh.tr("Show in folder"); onClicked: sh.pane.showInFolder(dlRow.d.path) }
                    FlatButton { visible: !dlRow.running; kind: "flat"; glyph: "trash"; implicitWidth: 32; implicitHeight: 32; ToolTip.visible: hovered; ToolTip.text: sh.tr("Remove from the list"); onClicked: { Core.cmd({cmd: "browser_dl_remove", id: dlRow.d.id}); sh.page.refreshDownloads() } }
                }
            }
        }
        FlatButton {
            visible: sh.dls.length > 0
            anchors { left: parent.left; bottom: parent.bottom; margins: 10 }
            kind: "plain"
            implicitHeight: 34
            text: sh.tr("Clear finished")
            onClicked: { Core.cmd({cmd: "browser_dl_clear"}); sh.page.refreshDownloads() }
        }
    }

    // ---- history ----
    Item {
        visible: sh.kind === "history"
        anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom }
        SearchEntry {
            id: histSearch
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 10 }
            placeholderText: sh.tr("Search the history")
            onTextChanged: histTimer.restart()
            Timer { id: histTimer; interval: 200; onTriggered: { Core.cmd({cmd: "browser_history_query", q: histSearch.text}); sh.refreshHistory() } }
        }
        Lbl {
            visible: (sh.hist.items || []).length === 0
            anchors.centerIn: parent
            width: parent.width - 40
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            dim: true
            text: histSearch.text !== "" ? sh.tr("Nothing found") : sh.tr("The pages you visit, here and in the player's quick search, show here.")
        }
        ListView {
            id: histList
            anchors { left: parent.left; right: parent.right; top: histSearch.bottom; bottom: histFoot.top; margins: 8 }
            clip: true
            model: (sh.hist.items || []).length
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            delegate: Column {
                id: hRow
                required property int index
                readonly property var v: (sh.hist.items || [])[index] || ({})
                readonly property string day: sh.dayOf(v.time || 0)
                readonly property bool firstOfDay: index === 0 || sh.dayOf(((sh.hist.items || [])[index - 1] || {}).time || 0) !== day
                width: histList.width
                Lbl { visible: hRow.firstOfDay; leftPadding: 8; topPadding: hRow.index === 0 ? 2 : 12; bottomPadding: 4; text: hRow.day; dim: true; bold: true; size: 12 }
                Rectangle {
                    width: parent.width
                    height: 48
                    radius: Theme.shape.medium
                    color: hArea.containsMouse ? Theme.c.hover : "transparent"
                    Icon { id: hIc; x: 8; anchors.verticalCenter: parent.verticalCenter; size: 16; color: Theme.c.textDim; name: hRow.v.kind === "lens" ? "lens" : (hRow.v.kind === "quick" ? "play" : "globe") }
                    Column {
                        anchors { left: hIc.right; leftMargin: 10; right: hDel.left; rightMargin: 4; verticalCenter: parent.verticalCenter }
                        Lbl { width: parent.width; elide: Text.ElideRight; text: hRow.v.title || hRow.v.url || ""; size: 14 }
                        Lbl { width: parent.width; elide: Text.ElideRight; dim: true; size: 12; text: Qt.formatTime(new Date((hRow.v.time || 0) * 1000), "hh:mm") + " · " + sh.hostOf(hRow.v.url) }
                    }
                    MouseArea {
                        id: hArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        ToolTip.visible: containsMouse
                        ToolTip.delay: 700
                        ToolTip.text: hRow.v.url || ""
                        onClicked: sh.page.newTab(hRow.v.url)
                    }
                    FlatButton {
                        id: hDel
                        visible: hArea.containsMouse || hovered
                        anchors { right: parent.right; rightMargin: 4; verticalCenter: parent.verticalCenter }
                        kind: "flat"; glyph: "close"; implicitWidth: 30; implicitHeight: 30
                        ToolTip.visible: hovered
                        ToolTip.text: sh.tr("Remove from the history")
                        onClicked: { Core.cmd({cmd: "browser_history_remove", time: hRow.v.time, url: hRow.v.url}); sh.refreshHistory() }
                    }
                }
            }
        }
        Item {
            id: histFoot
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 52
            Lbl { anchors { left: parent.left; leftMargin: 14; verticalCenter: parent.verticalCenter } dim: true; size: 12; text: (sh.hist.count || 0) + " " + sh.tr("pages") }
            FlatButton {
                id: clearBtn
                anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                kind: "plain"
                implicitHeight: 34
                text: sh.tr("Clear…")
                onClicked: clearMenu.openBelow(clearBtn, true)
            }
        }
        PopMenu {
            id: clearMenu
            items: [{label: sh.tr("The last hour")}, {label: sh.tr("Today")}, {label: sh.tr("Everything"), danger: true}]
            onPicked: function(i) {
                const now = Math.floor(Date.now() / 1000)
                const d = new Date()
                const since = i === 0 ? now - 3600 : (i === 1 ? Math.floor(new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() / 1000) : 0)
                Core.cmd({cmd: "browser_history_clear", since: since})
                sh.refreshHistory()
            }
        }
    }

    // ---- extensions ----
    Item {
        visible: sh.kind === "extensions"
        anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom }
        Column {
            id: extTop
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 14 }
            spacing: 10
            Lbl {
                width: parent.width
                wrapMode: Text.WordWrap
                dim: true
                size: 13
                text: sh.extOk
                      ? sh.tr("Add an unpacked extension: a folder with its manifest.json. An extension from the Chrome Web Store has to be downloaded and unpacked first.")
                      : sh.tr("Browser extensions are not supported by the built-in Chromium engine (CEF): its pages are drawn inside the app, a mode in which Chromium does not run Chrome extensions. Ad blocking and other extensions need your system browser for now.")
            }
            FlatButton {
                visible: sh.extOk
                enabled: sh.avail && sh.extOk
                kind: "suggested"
                glyph: "plus"
                text: sh.tr("Add an extension…")
                onClicked: extDialog.open()
            }
        }
        ListView {
            id: extList
            anchors { left: parent.left; right: parent.right; top: extTop.bottom; bottom: parent.bottom; margins: 8; topMargin: 12 }
            clip: true
            spacing: 2
            model: sh.exts.length
            delegate: Rectangle {
                id: xRow
                required property int index
                readonly property var ext: sh.exts[index] || ({})
                width: extList.width
                height: 52
                radius: Theme.shape.medium
                color: xArea.containsMouse ? Theme.c.hover : "transparent"
                MouseArea { id: xArea; anchors.fill: parent; hoverEnabled: true }
                Icon { id: xIc; x: 8; anchors.verticalCenter: parent.verticalCenter; name: "puzzle"; size: 18; color: xRow.ext.enabled ? Theme.c.accent : Theme.c.textDim }
                Column {
                    anchors { left: xIc.right; leftMargin: 10; right: xBtns.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
                    Lbl { width: parent.width; elide: Text.ElideRight; text: xRow.ext.name || ""; size: 14; bold: true }
                    Lbl { width: parent.width; elide: Text.ElideRight; dim: true; size: 11; text: xRow.ext.id || "" }
                }
                Row {
                    id: xBtns
                    anchors { right: parent.right; rightMargin: 4; verticalCenter: parent.verticalCenter }
                    FlatButton { kind: "flat"; glyph: xRow.ext.enabled ? "eye" : "eyeoff"; implicitWidth: 32; implicitHeight: 32; ToolTip.visible: hovered; ToolTip.text: xRow.ext.enabled ? sh.tr("Turn off") : sh.tr("Turn on"); onClicked: sh.pane.enableExtension(xRow.ext.id, !xRow.ext.enabled) }
                    FlatButton { kind: "flat"; glyph: "trash"; implicitWidth: 32; implicitHeight: 32; ToolTip.visible: hovered; ToolTip.text: sh.tr("Remove"); onClicked: sh.pane.removeExtension(xRow.ext.id) }
                }
            }
        }
        FolderDialog {
            id: extDialog
            title: sh.tr("Folder of an unpacked extension")
            onAccepted: sh.pane.addExtension(decodeURIComponent(selectedFolder.toString().replace(/^file:\/{2,3}/, Qt.platform.os === "windows" ? "" : "/")))
        }
    }

    // ---- settings ----
    Flickable {
        visible: sh.kind === "settings"
        anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom }
        contentHeight: setCol.implicitHeight + 24
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        Column {
            id: setCol
            x: 14
            y: 12
            width: parent.width - 28
            spacing: 16
            Column {
                width: parent.width
                spacing: 8
                Lbl { text: sh.tr("Search engine"); bold: true; size: 14 }
                Segmented {
                    model: (sh.bsx.engines || []).map(function(e) { return {id: e.id, label: e.label} })
                    current: sh.bsx.engine || "google"
                    onPicked: function(id) { Core.cmd({cmd: "browser_set", key: "engine", value: id}); sh.page.refresh() }
                }
            }
            PrefGroup {
                width: parent.width
                PrefSwitch { title: sh.tr("Open the tabs of last time"); checked: sh.bsx.restoreTabs !== false; onToggled: function(on) { Core.cmd({cmd: "browser_set", key: "restore_tabs", value: on}); sh.page.refresh() } }
                PrefSwitch { title: sh.tr("Pause the video during the quick search"); subtitle: sh.tr("The globe button of the player"); checked: sh.bsx.pauseQuick !== false; onToggled: function(on) { Core.cmd({cmd: "browser_set", key: "pause_quick", value: on}); sh.page.refresh() } }
            }
            // ---- the ad blocker (uBlock's filter lists, see src/adblock.rs) ----
            PrefGroup {
                id: adGroup
                width: parent.width
                readonly property string site: sh.pane ? sh.hostOf(sh.pane.url || "") : ""
                readonly property bool siteAllowed: site !== "" && (sh.bsx.adblockAllow || []).indexOf(site.replace(/^www\./, "")) >= 0
                PrefSwitch {
                    title: sh.tr("Ad blocker (uBlock)")
                    subtitle: (sh.bsx.adblockBusy ? sh.tr("Downloading the filter lists…")
                              : (sh.bsx.adblockRules || 0) > 0 ? sh.tr("%1 filters: uBlock filters, EasyList, EasyPrivacy").arg(sh.bsx.adblockRules)
                              : sh.tr("The filter lists are downloaded the first time"))
                              + (sh.bsx.adblockProblem ? "\n" + sh.bsx.adblockProblem : "")
                    checked: sh.bsx.adblock !== false
                    onToggled: function(on) { Core.cmd({cmd: "browser_set", key: "adblock", value: on}); sh.page.refresh() }
                }
                PrefSwitch {
                    visible: adGroup.site !== "" && sh.bsx.adblock !== false
                    title: sh.tr("Block ads on %1").arg(adGroup.site)
                    subtitle: sh.tr("Turn off for a site that does not work with the blocker")
                    checked: !adGroup.siteAllowed
                    onToggled: function(on) { Core.cmd({cmd: "browser_adblock_site", url: sh.pane.url, on: on}); sh.page.refresh() }
                }
                PrefButton {
                    visible: sh.bsx.adblock !== false
                    title: sh.tr("Filter lists")
                    subtitle: sh.tr("They are updated by themselves once a week")
                    label: sh.tr("Update now")
                    onActivated: { Core.cmd({cmd: "browser_adblock_update"}); sh.page.refresh() }
                }
            }
            Column {
                width: parent.width
                spacing: 4
                Lbl { text: sh.tr("Page engine"); bold: true; size: 14 }
                Lbl {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    dim: true
                    size: 13
                    text: !sh.pane ? "" : (sh.avail ? (sh.pane.runtimeVersion !== "" ? sh.pane.runtimeVersion : "Chromium (CEF)") + "\n" + sh.tr("Open source (BSD licence), built into Reelhouse.") : sh.tr(sh.pane.problem))
                }
            }
        }
    }
}
