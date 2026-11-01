import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "Osd.js" as Osd

// The quick search of the player (its globe button): a search bar over the video, filled with the title or the
// subtitle line on screen, a Google Lens button that searches with the picture on screen, and the results in a small
// window over the video that can be moved, resized and closed. The page is drawn by Chromium off screen (WebPane),
// so it composites with the video and the player's controls like any other panel. Everything visited here goes to
// the history of Research mode (src/browser.rs); "Continue in Research mode" opens the page there.
Item {
    id: qs
    property bool opened: false
    property bool showResults: false
    property bool resumeAfter: false    // the video was playing when the search opened: it plays again on close
    property bool lensing: false        // the next page is the answer of Google Lens (history kind "lens")
    property bool grabbing: false       // mpv is taking the picture for Lens (on a worker thread of the core)
    property double grabAt: 0           // when the picture was asked (for the log)
    property double searchAt: 0         // when the last search or Lens started (for the log)
    property var bs: ({})               // Core.state("browser")
    readonly property var p: Core.player
    readonly property var o: Osd.t(Theme.layout.player || "adw")
    readonly property color accentInk: Osd.readable(Theme.c.accent)
    signal openInResearch(string url)
    visible: opened

    function tr(t) { return Core.tr(Core.uiLang, t) }
    function clean(t) { return (t || "").replace(/\s*\n\s*/g, " ").replace(/\{[^}]*\}/g, "").trim() }
    readonly property string titleText: clean(p.title)
    readonly property string lineText: clean(p.subText)

    function open() {
        bs = Core.state("browser")
        if (!opened) {
            field.text = titleText
            resumeAfter = bs.pauseQuick !== false && p.paused === false
            if (resumeAfter) Core.cmd({cmd: "set_pause", value: true})
        }
        opened = true
        field.forceActiveFocus()
        field.selectAll()
        // Chromium starts while the user types (after the bar is drawn), not when Search or Lens is pressed
        warm.restart()
    }
    function close(resume) {
        if (!opened) return
        opened = false
        showResults = false
        grabbing = false
        searchAt = 0
        grabWatch.stop()
        warm.stop()
        if (resume !== false && resumeAfter) Core.cmd({cmd: "set_pause", value: false})
        resumeAfter = false
    }
    function toggle() { opened ? close() : open() }
    function search() {
        const q = field.text.trim()
        if (q === "") return
        Core.cmd({cmd: "browser_address", text: q})
        const u = Core.state("browser:address").address || ""
        if (u === "") return
        lensing = false
        grabbing = false
        showResults = true
        searchAt = Date.now()
        qpane.navigate(u)
    }
    // Google Lens with the frame on screen: mpv saves it (without subtitles) on a worker thread of the core, the
    // answer comes as Core.lensFrame; then the page view uploads it. The window stays responsive meanwhile.
    function lens() {
        if (grabbing) return
        const r = Core.cmd({cmd: "browser_grab_frame_async"})
        if (r.ok === false) { Core.toast(tr(r.error || "The picture could not be taken.")); return }
        grabbing = true
        grabAt = Date.now()
        searchAt = grabAt
        lensing = true
        showResults = true
        grabWatch.restart()
    }
    Connections {
        target: Core
        ignoreUnknownSignals: true
        function onLensFrame(f) {
            if (!qs.grabbing) return
            qs.grabbing = false
            grabWatch.stop()
            console.log("[lens] screenshot " + Math.round(f.ms || 0) + " ms (core), " + (Date.now() - qs.grabAt) + " ms until the window got it")
            if (!f.frame) {
                qs.lensFailedEarly(f.error || "The picture could not be taken.")
                return
            }
            qpane.lens(f.frame)
        }
    }
    // mpv never answered: an error instead of waiting forever
    Timer {
        id: grabWatch
        interval: 10000
        onTriggered: {
            if (!qs.grabbing) return
            qs.grabbing = false
            console.log("[lens] screenshot: no answer from mpv in " + interval + " ms")
            qs.lensFailedEarly("The picture could not be taken.")
        }
    }
    // no picture for Lens: an error message, and the results window goes when it has nothing to show
    function lensFailedEarly(error) {
        lensing = false
        searchAt = 0
        pageWatch.stop()
        if (!qpane.hasPicture) showResults = false
        Core.toast(tr(error))
    }
    // the page of a search (or of Lens) arrived: the loading cover goes, and the log says how long it took
    function pageDone(what) {
        if (searchAt <= 0) return
        console.log("[quicksearch] " + what + ": page loaded in " + (Date.now() - searchAt) + " ms")
        searchAt = 0
        pageWatch.stop()
    }
    onSearchAtChanged: if (searchAt > 0) pageWatch.restart()
    // a page that never finishes: the cover goes after a while (the page shows what it has, the bar says Loading…)
    Timer {
        id: pageWatch
        interval: 30000
        onTriggered: {
            if (qs.searchAt <= 0) return
            // the picture and Lens have their own time limits (grabWatch, WebPane's 25 s): wait for them
            if (qs.grabbing || qpane.lensStage > 0) { restart(); return }
            console.log("[quicksearch] no page after " + (Date.now() - qs.searchAt) + " ms (still loading: " + qpane.loading + ")")
            qs.searchAt = 0
        }
    }
    Timer {
        id: warm
        interval: 300
        onTriggered: {
            if (!qs.opened || !qpane.available) return
            const t0 = Date.now()
            qpane.warmUp()
            console.log("[quicksearch] browser ready in " + (Date.now() - t0) + " ms (warm-up)")
        }
    }

    // leaving the player closes it (without starting the video again)
    Connections {
        target: qs.parent
        ignoreUnknownSignals: true
        function onVisibleChanged() { if (qs.parent && !qs.parent.visible) qs.close(false) }
    }

    // ---- the search bar ----
    OsdPanel {
        id: bar
        anchors.horizontalCenter: parent.horizontalCenter
        y: 64
        width: Math.min(720, parent.width - 32)
        height: 56
        // clicks on the panel stay here (a click on the video would pause or play it)
        MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.ArrowCursor }
        Row {
            id: barRow
            anchors { fill: parent; leftMargin: 14; rightMargin: 8 }
            spacing: 8
            Icon { anchors.verticalCenter: parent.verticalCenter; name: "globe"; size: 18; color: qs.o.fg }
            TextField {
                id: field
                anchors.verticalCenter: parent.verticalCenter
                width: barRow.width - 18 - searchBtn.width - lensBtn.width - closeBtn.width - barRow.spacing * 4
                height: 38
                color: qs.o.fg
                placeholderText: qs.tr("Search about this video")
                placeholderTextColor: qs.o.muted
                selectionColor: Theme.c.accent
                selectedTextColor: Theme.c.accentFg
                font.pixelSize: 15
                font.family: Theme.type.family
                selectByMouse: true
                leftPadding: 12
                rightPadding: 12
                background: Rectangle { radius: Osd.r(qs.o.ctlR, height); color: qs.o.field; border.width: field.activeFocus ? 2 : (qs.o.fieldStroke !== "#00000000" ? 1 : 0); border.color: field.activeFocus ? Theme.c.accent : qs.o.fieldStroke }
                onAccepted: qs.search()
                // Escape closes the search instead of leaving the player (its "back" key)
                Keys.onShortcutOverride: function(e) { if (e.key === Qt.Key_Escape) e.accepted = true }
                Keys.onEscapePressed: qs.close()
            }
            OsdButton { id: searchBtn; anchors.verticalCenter: parent.verticalCenter; kind: "accent"; pill: qs.o.ctlR >= 999; glyph: "search"; text: qs.tr("Search"); onClicked: qs.search() }
            OsdButton { id: lensBtn; anchors.verticalCenter: parent.verticalCenter; kind: "tonal"; pill: qs.o.ctlR >= 999; glyph: "lens"; text: "Google Lens"; ToolTip.visible: hovered; ToolTip.text: qs.tr("Search with the picture on screen"); onClicked: qs.lens() }
            OsdButton { id: closeBtn; anchors.verticalCenter: parent.verticalCenter; kind: "round"; glyph: "close"; iconSize: 16; ToolTip.visible: hovered; ToolTip.text: qs.tr("Close") + " (Esc)"; onClicked: qs.close() }
        }
    }
    // what to search: the title or the line on screen
    Row {
        id: chips
        anchors { top: bar.bottom; topMargin: 8; left: bar.left; leftMargin: 8 }
        spacing: 6
        Repeater {
            model: [{label: qs.tr("Title"), text: qs.titleText}, {label: qs.tr("Subtitle line"), text: qs.lineText}]
            delegate: Rectangle {
                required property var modelData
                visible: modelData.text !== ""
                height: 30
                width: chipText.implicitWidth + 24
                radius: 15
                color: chipArea.containsMouse ? qs.o.btnHover : qs.o.btn
                border.width: qs.o.btnStroke !== "#00000000" ? 1 : 0
                border.color: qs.o.btnStroke
                Text { id: chipText; anchors.centerIn: parent; text: modelData.label; color: qs.o.btnFg; font.pixelSize: 13; font.family: Theme.type.family }
                MouseArea {
                    id: chipArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    ToolTip.visible: containsMouse
                    ToolTip.text: modelData.text
                    onClicked: { field.text = modelData.text; field.forceActiveFocus() }
                }
            }
        }
    }

    // ---- the results: a small window over the video ----
    OsdPanel {
        id: resWin
        sheet: true
        visible: qs.showResults
        width: Math.max(300, Math.min(qs.bs.quickW || 520, qs.width - 24))
        height: Math.max(220, Math.min(qs.bs.quickH || 420, qs.height - y - 12))
        x: qs.width - width - 24
        y: chips.y + chips.height + 12
        HoverHandler { cursorShape: Qt.ArrowCursor }
        // the bar on top: drag it to move the window
        Item {
            id: resHead
            anchors { left: parent.left; right: parent.right; top: parent.top }
            height: 40
            DragHandler {
                target: resWin
                xAxis.minimum: 0; xAxis.maximum: qs.width - resWin.width
                yAxis.minimum: 0; yAxis.maximum: qs.height - resWin.height
            }
            Row {
                anchors { left: parent.left; leftMargin: 6; verticalCenter: parent.verticalCenter }
                spacing: 0
                OsdButton { kind: "osd"; glyph: "left"; implicitHeight: 32; implicitWidth: 32; iconSize: 16; enabled: qpane.canGoBack; onClicked: qpane.back() }
                OsdButton { kind: "osd"; glyph: "right"; implicitHeight: 32; implicitWidth: 32; iconSize: 16; enabled: qpane.canGoForward; onClicked: qpane.forward() }
            }
            Text {
                anchors { left: parent.left; leftMargin: 76; right: headBtns.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
                elide: Text.ElideRight
                text: qpane.loading ? qs.tr("Loading…") : (qpane.title || "")
                color: qs.o.dim
                font.pixelSize: 13
                font.family: Theme.type.family
            }
            Row {
                id: headBtns
                anchors { right: parent.right; rightMargin: 6; verticalCenter: parent.verticalCenter }
                OsdButton { kind: "osd"; glyph: "expand"; implicitHeight: 32; implicitWidth: 32; iconSize: 16; enabled: qpane.url !== "" && qpane.url !== "about:blank"; ToolTip.visible: hovered; ToolTip.text: qs.tr("Continue in Research mode"); onClicked: { qs.openInResearch(qpane.url); qs.showResults = false } }
                OsdButton { kind: "osd"; glyph: "external"; implicitHeight: 32; implicitWidth: 32; iconSize: 16; enabled: qpane.url !== "" && qpane.url !== "about:blank"; ToolTip.visible: hovered; ToolTip.text: qs.tr("Open in the system browser"); onClicked: Qt.openUrlExternally(qpane.url) }
                OsdButton { kind: "osd"; glyph: "close"; implicitHeight: 32; implicitWidth: 32; iconSize: 16; ToolTip.visible: hovered; ToolTip.text: qs.tr("Close the results"); onClicked: qs.showResults = false }
            }
        }
        WebPane {
            id: qpane
            anchors { left: parent.left; right: parent.right; top: resHead.bottom; bottom: parent.bottom; leftMargin: 6; rightMargin: 6; bottomMargin: 14 }
            radius: Math.max(4, resWin.radius - 6)
            singleTab: true
            profile: qs.bs.profileDir || ""
            language: Core.uiLang || "en"
            onVisited: function(url, title) {
                Core.cmd({cmd: "browser_visit", url: url, title: title, kind: qs.lensing ? "lens" : "quick"})
                qs.pageDone(qs.lensing ? "Google Lens" : "Search")
                qs.lensing = false
            }
            onTitleSeen: function(url, title) { Core.cmd({cmd: "browser_title", url: url, title: title}) }
            onHasPictureChanged: if (hasPicture && qs.searchAt > 0) console.log("[quicksearch] first picture " + (Date.now() - qs.searchAt) + " ms after the request")
            onNavFailed: function(url, status, http) { qs.pageDone("failed " + status) }
            onDownload: function(d) { Core.cmd({cmd: "browser_dl", id: d.id, url: d.url, path: d.path, received: d.received, total: d.total, state: d.state}) }
            onMessage: function(text) { Core.toast(qs.tr(text)) }
            onKeyPressed: function(name) { if (name === "Escape") qs.showResults = false }
            // the page's right click: the few things that make sense in this small window
            onContextMenu: function(info) {
                qsMenu.info = info
                qsMenu.items = info.link ? [{label: qs.tr("Open link in Research mode")}, {label: qs.tr("Copy link address")}]
                             : (info.selection ? [{label: qs.tr("Copy")}, {label: qs.tr("Search for “%1”").replace("%1", info.selection.slice(0, 24))}] : [{label: qs.tr("Back")}, {label: qs.tr("Reload")}])
                qsMenu.openAt(qpane, info.x, info.y)
            }
        }
        PopMenu {
            id: qsMenu
            property var info: ({})
            menuWidth: 240
            onPicked: function(i) {
                const m = info
                if (m.link) { if (i === 0) { qs.openInResearch(m.link); qs.showResults = false } else qpane.copyText(m.link) }
                else if (m.selection) { if (i === 0) qpane.editCommand("copy"); else { field.text = m.selection; qs.search() } }
                else if (i === 0) qpane.back()
                else qpane.reload()
            }
        }
        // what is going on while there is no page to show (instead of an empty dark area): the picture being taken,
        // sent to Lens, or the first picture of the page still on its way
        Rectangle {
            id: waitCover
            readonly property string what: qs.grabbing ? qs.tr("Taking the picture…")
                                           : (qpane.lensStage > 0 ? qs.tr("Sending the picture to Google Lens…")
                                           : ((!qpane.hasPicture || qs.searchAt > 0) ? (qpane.runtimeVersion === "" ? qs.tr("Starting the browser…") : qs.tr("Loading…")) : ""))
            visible: qpane.available && what !== ""
            anchors.fill: qpane
            radius: qpane.radius
            color: qs.o.panel
            Column {
                anchors.centerIn: parent
                width: parent.width - 32
                spacing: 12
                BusyIndicator {
                    anchors.horizontalCenter: parent.horizontalCenter
                    running: waitCover.visible
                    implicitWidth: 36
                    implicitHeight: 36
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    text: waitCover.what
                    color: qs.o.dim
                    font.pixelSize: 13
                    font.family: Theme.type.family
                }
            }
        }
        // no page view here: the address went to the system browser
        Text {
            visible: !qpane.available
            anchors.centerIn: qpane
            width: qpane.width - 32
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: qs.tr("The results opened in your system browser.") + "\n" + qs.tr(qpane.problem)
            color: qs.o.dim
            font.pixelSize: 13
            font.family: Theme.type.family
        }
        // the corner: drag it to resize; the size is kept for next time
        Item {
            anchors { right: parent.right; bottom: parent.bottom }
            width: 22; height: 22
            Canvas {
                anchors.fill: parent
                onPaint: {
                    const c = getContext("2d")
                    c.reset()
                    c.strokeStyle = qs.o.muted
                    c.lineWidth = 1.5
                    for (var i = 0; i < 3; ++i) { c.beginPath(); c.moveTo(width - 4 - i * 5, height - 3); c.lineTo(width - 3, height - 4 - i * 5); c.stroke() }
                }
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.SizeFDiagCursor
                property real px: 0
                property real py: 0
                property real w0: 0
                property real h0: 0
                onPressed: function(m) {
                    // the window stays where it is while it grows (its first place follows the right edge)
                    resWin.x = resWin.x
                    resWin.y = resWin.y
                    const g = mapToItem(qs, m.x, m.y); px = g.x; py = g.y; w0 = resWin.width; h0 = resWin.height
                }
                onPositionChanged: function(m) {
                    if (!pressed) return
                    const g = mapToItem(qs, m.x, m.y)
                    qs.bs = Object.assign({}, qs.bs, {quickW: Math.round(Math.max(300, w0 + g.x - px)), quickH: Math.round(Math.max(220, h0 + g.y - py))})
                }
                onReleased: {
                    Core.cmd({cmd: "browser_set", key: "quick_w", value: qs.bs.quickW})
                    Core.cmd({cmd: "browser_set", key: "quick_h", value: qs.bs.quickH})
                }
            }
        }
    }
}
