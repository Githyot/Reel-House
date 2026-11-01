import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The browser in the app's sidebar (Research mode): loaded by SidebarNav / RailNav under their fixed entries, so the
// sidebar of every mode stays and the web page fills the rest of the window (ResearchPage). From the top: back,
// forward and reload; the address and search field; the pinned Essentials; "New tab" and the vertical tabs;
// downloads, history, extensions and the browser settings (they open as sheets beside the page).
// Each theme draws it like its own browser: Safari's sidebar (Apple), Epiphany with libadwaita rows (GNOME), Edge's
// vertical tabs (Windows), Chrome's Material 3 rows (Android). Narrower than 150 px (a rail) it shows icons only.
// Size: the width it is given; the height is implicitHeight (it grows with the tabs; the sidebar scrolls).
// The page is found by itself (Window.window.researchView, set in Main.qml); a Loader may also set `page`.
Item {
    id: rs
    property var page: Window.window && Window.window.researchView ? Window.window.researchView : null
    readonly property var pane: page ? page.pane : null
    readonly property bool compact: width < 150
    readonly property string look: Theme.look
    readonly property bool apple: look === "apple"
    readonly property bool windows: look === "windows"
    readonly property bool android: look === "android"
    readonly property real fieldH: android ? 44 : (apple ? 32 : 34)
    readonly property var tabs: pane ? pane.tabs : []
    readonly property var pins: page ? (page.bs.pins || []) : []
    implicitHeight: (compact ? compactCol.implicitHeight : fullCol.implicitHeight) + 4

    function tr(t) { return Core.tr(Core.uiLang, t) }
    // the page keeps a list of the sidebars showing its controls (SidebarNav and RailNav both load one)
    function register(on) {
        if (!page) return
        const list = (page.sidebars || []).filter(function(s) { return s !== rs })
        if (on) list.push(rs)
        page.sidebars = list
    }
    onPageChanged: register(true)
    Component.onCompleted: register(true)
    Component.onDestruction: register(false)
    Connections {
        target: rs.page
        ignoreUnknownSignals: true
        function onAddressFocusRequested() {
            if (!rs.visible) return
            if (rs.compact) addressPop.openRight(searchBtn)
            else { address.forceActiveFocus(); address.selectAll() }
        }
    }
    // the tab with this id moved by `rows` places (drag in the list)
    function moveTab(id, rows) {
        for (var i = 0; i < tabs.length; ++i) if (tabs[i].id === id) { pane.moveTab(id, i + rows); return }
    }
    // the icon of an open tab of that site, for an Essential (no extra requests)
    function iconFor(url) {
        const h = page ? page.hostOf(url) : ""
        for (var i = 0; i < tabs.length; ++i) if (tabs[i].icon && page.hostOf(tabs[i].url) === h) return tabs[i].icon
        return ""
    }

    // ================= full width =================
    Column {
        id: fullCol
        visible: !rs.compact
        width: parent.width
        spacing: rs.android ? 10 : 8

        // back, forward, reload / stop
        Row {
            spacing: rs.android ? 4 : 2
            FlatButton { kind: "flat"; glyph: "left"; implicitWidth: rs.android ? 40 : 34; implicitHeight: rs.android ? 40 : 30; enabled: !!rs.pane && rs.pane.canGoBack; ToolTip.visible: hovered; ToolTip.text: rs.tr("Back") + " (Alt+←)"; onClicked: rs.pane.back() }
            FlatButton { kind: "flat"; glyph: "right"; implicitWidth: rs.android ? 40 : 34; implicitHeight: rs.android ? 40 : 30; enabled: !!rs.pane && rs.pane.canGoForward; ToolTip.visible: hovered; ToolTip.text: rs.tr("Forward") + " (Alt+→)"; onClicked: rs.pane.forward() }
            FlatButton { kind: "flat"; glyph: rs.pane && rs.pane.loading ? "close" : "refresh"; implicitWidth: rs.android ? 40 : 34; implicitHeight: rs.android ? 40 : 30; ToolTip.visible: hovered; ToolTip.text: rs.pane && rs.pane.loading ? rs.tr("Stop") : rs.tr("Reload") + " (F5)"; onClicked: rs.pane.loading ? rs.pane.stop() : rs.pane.reload() }
        }

        // downloads, history, extensions, settings: sheets beside the page; fixed right under back / forward / reload
        Row {
            id: tools
            spacing: 2
            readonly property real btnW: Math.min(44, (fullCol.width - 3 * spacing) / 4)
            Repeater {
                model: [
                    {id: "downloads", icon: "download", tip: "Downloads (Ctrl+J)"},
                    {id: "history", icon: "history", tip: "History (Ctrl+H)"},
                    {id: "extensions", icon: "puzzle", tip: "Extensions"},
                    {id: "settings", icon: "settings", tip: "Browser settings"}
                ]
                delegate: FlatButton {
                    required property var modelData
                    kind: "flat"
                    glyph: modelData.icon
                    implicitWidth: tools.btnW
                    implicitHeight: rs.android ? 40 : 32
                    active: !!rs.page && rs.page.sheet === modelData.id
                    ToolTip.visible: hovered
                    ToolTip.text: rs.tr(modelData.tip)
                    onClicked: rs.page.toggleSheet(modelData.id)
                    // running downloads
                    Rectangle {
                        visible: modelData.id === "downloads" && !!rs.page && rs.page.activeDownloads > 0
                        anchors { right: parent.right; top: parent.top; margins: 2 }
                        width: 16; height: 16; radius: 8
                        color: Theme.c.accent
                        Text { anchors.centerIn: parent; text: rs.page ? rs.page.activeDownloads : ""; color: Theme.c.accentFg; font.pixelSize: 10; font.weight: Font.Bold; font.family: Theme.type.family }
                    }
                }
            }
        }
        // the address / search field: the site while it is not being edited, the whole address once focused
        TextField {
            id: address
            inputMethodHints: Qt.platform.os === "android" ? Qt.ImhNoPredictiveText | Qt.ImhUrlCharactersOnly : Qt.ImhNone   // see SearchEntry
            width: parent.width
            height: rs.fieldH
            leftPadding: 32
            rightPadding: 30
            color: Theme.c.text
            placeholderTextColor: Theme.c.textDim
            placeholderText: rs.tr("Search or type an address")
            font.pixelSize: rs.apple ? 13 : 14
            font.family: Theme.type.family
            selectByMouse: true
            verticalAlignment: TextInput.AlignVCenter
            // Safari centres the site in its field until it is edited
            horizontalAlignment: rs.apple && !activeFocus ? TextInput.AlignHCenter : TextInput.AlignLeft
            // set by hand, not bound: a binding would be broken by the first key typed
            function showSite() { if (!activeFocus) { text = rs.page && rs.pane ? rs.page.siteOf(rs.pane.url) : ""; cursorPosition = 0 } }
            onActiveFocusChanged: { if (activeFocus) { text = rs.pane ? rs.pane.url : ""; selectAll() } else showSite() }
            Connections { target: rs.pane; ignoreUnknownSignals: true; function onCurrentChanged() { address.showSite() } }
            Component.onCompleted: showSite()
            onAccepted: if (rs.page) rs.page.go(text)
            Keys.onEscapePressed: { text = rs.pane ? rs.pane.url : ""; if (rs.pane) rs.pane.focusPage() }
            background: Rectangle {
                radius: rs.android ? height / 2 : (rs.windows ? 4 : (rs.apple ? 8 : 6))
                color: rs.android ? Theme.c.surface2 || Theme.c.fill3 : Theme.c.fill3
                border.width: rs.windows ? 1 : (address.activeFocus ? 2 : 0)
                border.color: rs.windows ? Theme.c.controlStroke || Theme.c.border : Theme.c.accent
                // WinUI: the accent line under a focused field
                Rectangle { visible: rs.windows && address.activeFocus; anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 1 } height: 2; color: Theme.c.accent }
            }
            Icon {
                anchors { left: parent.left; leftMargin: 10; verticalCenter: parent.verticalCenter }
                size: 15
                color: Theme.c.textDim
                name: address.activeFocus || !rs.pane || rs.pane.url === "" ? "search" : "globe"
            }
            // pin the page to the Essentials
            Item {
                visible: !!rs.pane && rs.pane.url !== "" && !address.activeFocus
                anchors { right: parent.right; rightMargin: 3; verticalCenter: parent.verticalCenter }
                width: 26; height: 26
                Icon { anchors.centerIn: parent; name: "pin"; size: 14; color: rs.page && rs.page.isPinned(rs.pane.url) ? Theme.c.accent : Theme.c.textDim }
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    ToolTip.visible: containsMouse
                    ToolTip.text: rs.page && rs.page.isPinned(rs.pane.url) ? rs.tr("Remove from Essentials") : rs.tr("Add to Essentials") + " (Ctrl+D)"
                    onClicked: rs.page.pin(rs.pane.url, rs.pane.title, !rs.page.isPinned(rs.pane.url))
                }
            }
            // page loading: a thin bar on the lower edge
            Rectangle {
                visible: !!rs.pane && rs.pane.loading
                anchors { left: parent.left; bottom: parent.bottom; leftMargin: 6; bottomMargin: 1 }
                height: 2
                radius: 1
                color: Theme.c.accent
                width: (parent.width - 12) * 0.3
                SequentialAnimation on x {
                    running: !!rs.pane && rs.pane.loading && rs.visible
                    loops: Animation.Infinite
                    NumberAnimation { from: 6; to: address.width * 0.7 - 6; duration: 900; easing.type: Easing.InOutQuad }
                    NumberAnimation { from: address.width * 0.7 - 6; to: 6; duration: 900; easing.type: Easing.InOutQuad }
                }
            }
        }

        // Essentials: the pinned pages as tiles (Safari's favourites, Zen's essentials)
        Flow {
            id: pinFlow
            width: parent.width
            spacing: 6
            visible: rs.pins.length > 0
            readonly property int perRow: rs.android ? 5 : 4
            Repeater {
                model: rs.pins.length
                delegate: Rectangle {
                    id: tile
                    required property int index
                    readonly property var pin: rs.pins[index] || ({})
                    readonly property bool here: !!rs.pane && rs.page.hostOf(rs.pane.url) === rs.page.hostOf(pin.url)
                    readonly property string icon: rs.iconFor(pin.url)
                    width: (pinFlow.width - pinFlow.spacing * (pinFlow.perRow - 1)) / pinFlow.perRow
                    height: rs.android ? width : 40
                    radius: rs.android ? width / 2 : (rs.windows ? 4 : Theme.shape.medium)
                    color: here ? Theme.c.sidebarSelected : (tileArea.containsMouse ? Theme.c.hover : Theme.c.fill3)
                    border.width: here && rs.windows ? 1 : 0
                    border.color: Theme.c.accent
                    Image {
                        id: tileIcon
                        anchors.centerIn: parent
                        width: 18; height: 18
                        visible: status === Image.Ready
                        source: tile.icon
                        sourceSize: Qt.size(36, 36)
                        asynchronous: true
                    }
                    Text {
                        anchors.centerIn: parent
                        visible: !tileIcon.visible
                        text: rs.page ? rs.page.hostOf(tile.pin.url).charAt(0).toUpperCase() : ""
                        color: tile.here ? Theme.c.accentText || Theme.c.accent : Theme.c.text
                        font.pixelSize: 16
                        font.weight: Font.Bold
                        font.family: Theme.type.family
                    }
                    MouseArea {
                        id: tileArea
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        cursorShape: Qt.PointingHandCursor
                        ToolTip.visible: containsMouse
                        ToolTip.text: tile.pin.title || tile.pin.url || ""
                        onClicked: function(m) {
                            if (m.button === Qt.RightButton) rs.page.openPinMenu(tile.pin, tile, m.x, m.y)
                            else rs.page.openPin(tile.pin)
                        }
                    }
                }
            }
        }

        // new tab
        Rectangle {
            width: parent.width
            height: rs.android ? 44 : 34
            radius: rs.android ? height / 2 : (rs.windows ? 4 : (rs.apple ? 8 : 6))
            color: newArea.containsMouse ? Theme.c.hover : "transparent"
            Icon { id: plusIc; x: rs.android ? 14 : 10; anchors.verticalCenter: parent.verticalCenter; name: "plus"; size: 16; color: rs.android ? Theme.c.accent : Theme.c.textDim }
            Text { anchors { left: plusIc.right; leftMargin: 10; verticalCenter: parent.verticalCenter } text: rs.tr("New tab"); color: rs.android ? Theme.c.accentText || Theme.c.text : Theme.c.textDim; font.pixelSize: rs.apple ? 13 : 14; font.weight: rs.android ? Font.Medium : Font.Normal; font.family: Theme.type.family }
            Text { visible: !rs.android; anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } text: "Ctrl+T"; color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family }
            MouseArea { id: newArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: if (rs.page) rs.page.newTab("") }
        }
        // the vertical tabs
        Column {
            width: parent.width
            spacing: 2
            Repeater {
                model: rs.tabs.length
                delegate: ResearchTabRow {
                    required property int index
                    width: parent.width
                    tab: rs.tabs[index] || ({})
                    current: (rs.tabs[index] || {}).id === rs.pane.currentTab
                    onActivated: rs.pane.currentTab = tab.id
                    onCloseRequested: rs.pane.closeTab(tab.id)
                    onMenuRequested: function(item, x, y) { rs.page.openTabMenu(tab, item, x, y) }
                    onMoveBy: function(id, rows) { rs.moveTab(id, rows) }
                }
            }
        }
    }

    // ================= a narrow rail: icons only =================
    Column {
        id: compactCol
        visible: rs.compact
        width: parent.width
        spacing: 4
        readonly property real btn: Math.min(40, width)
        FlatButton { anchors.horizontalCenter: parent.horizontalCenter; kind: "flat"; glyph: "left"; implicitWidth: compactCol.btn; implicitHeight: 32; enabled: !!rs.pane && rs.pane.canGoBack; ToolTip.visible: hovered; ToolTip.text: rs.tr("Back"); onClicked: rs.pane.back() }
        FlatButton { anchors.horizontalCenter: parent.horizontalCenter; kind: "flat"; glyph: rs.pane && rs.pane.loading ? "close" : "refresh"; implicitWidth: compactCol.btn; implicitHeight: 32; ToolTip.visible: hovered; ToolTip.text: rs.tr("Reload"); onClicked: rs.pane.loading ? rs.pane.stop() : rs.pane.reload() }
        FlatButton {
            id: searchBtn
            anchors.horizontalCenter: parent.horizontalCenter
            kind: "flat"; glyph: "search"; implicitWidth: compactCol.btn; implicitHeight: 32
            ToolTip.visible: hovered; ToolTip.text: rs.tr("Search or type an address")
            onClicked: addressPop.openRight(searchBtn)
            Popover {
                id: addressPop
                onOpened: { popField.text = rs.pane ? rs.pane.url : ""; popField.forceActiveFocus(); popField.selectAll() }
                contentItem: TextField {
                    id: popField
                    width: 320
                    height: 36
                    color: Theme.c.text
                    placeholderText: rs.tr("Search or type an address")
                    placeholderTextColor: Theme.c.textDim
                    font.family: Theme.type.family
                    selectByMouse: true
                    onAccepted: { addressPop.close(); if (rs.page) rs.page.go(text) }
                    background: Rectangle { radius: rs.android ? height / 2 : Theme.shape.control; color: Theme.c.fill3; border.width: popField.activeFocus ? 2 : 0; border.color: Theme.c.accent }
                }
            }
        }
        FlatButton { anchors.horizontalCenter: parent.horizontalCenter; kind: "flat"; glyph: "plus"; implicitWidth: compactCol.btn; implicitHeight: 32; ToolTip.visible: hovered; ToolTip.text: rs.tr("New tab") + " (Ctrl+T)"; onClicked: rs.page.newTab("") }
        Rectangle { anchors.horizontalCenter: parent.horizontalCenter; width: parent.width - 16; height: 1; color: Theme.c.divider }
        Repeater {
            model: rs.tabs.length
            delegate: ResearchTabRow {
                required property int index
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(compactCol.width, 48)
                compact: true
                tab: rs.tabs[index] || ({})
                current: (rs.tabs[index] || {}).id === rs.pane.currentTab
                onActivated: rs.pane.currentTab = tab.id
                onCloseRequested: rs.pane.closeTab(tab.id)
                onMenuRequested: function(item, x, y) { rs.page.openTabMenu(tab, item, x, y) }
                onMoveBy: function(id, rows) { rs.moveTab(id, rows) }
            }
        }
        Rectangle { anchors.horizontalCenter: parent.horizontalCenter; width: parent.width - 16; height: 1; color: Theme.c.divider }
        FlatButton { anchors.horizontalCenter: parent.horizontalCenter; kind: "flat"; glyph: "download"; implicitWidth: compactCol.btn; implicitHeight: 32; active: !!rs.page && rs.page.sheet === "downloads"; ToolTip.visible: hovered; ToolTip.text: rs.tr("Downloads (Ctrl+J)"); onClicked: rs.page.toggleSheet("downloads") }
        FlatButton { anchors.horizontalCenter: parent.horizontalCenter; kind: "flat"; glyph: "history"; implicitWidth: compactCol.btn; implicitHeight: 32; active: !!rs.page && rs.page.sheet === "history"; ToolTip.visible: hovered; ToolTip.text: rs.tr("History (Ctrl+H)"); onClicked: rs.page.toggleSheet("history") }
        FlatButton { anchors.horizontalCenter: parent.horizontalCenter; kind: "flat"; glyph: "settings"; implicitWidth: compactCol.btn; implicitHeight: 32; active: !!rs.page && rs.page.sheet === "settings"; ToolTip.visible: hovered; ToolTip.text: rs.tr("Browser settings"); onClicked: rs.page.toggleSheet("settings") }
    }
}
