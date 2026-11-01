import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Reelhouse 1.0

// The GNOME header bar of Reelhouse (GTK browse.rs).
// Start page: search, Mode, Catalog and reload on the left, the view switcher in the middle, notifications and the main menu
// on the right. A title page keeps those (GTK place_chrome) and shows back, the search box with Library and Calendar.
// A category page shows back and the search box; the INFO page back and its title.
Rectangle {
    id: hb
    property string tab: "discover"
    property string mtab: "library"        // Images and Text modes: library | explore | history
    readonly property bool modal: Core.manga.mode === "images" || Core.manga.mode === "text"
    // Study and Research modes hide the video controls of the bar (search, catalog, reload, the view switcher)
    readonly property bool study: Core.manga.mode === "study" || Core.manga.mode === "research"
    signal modeTabSelected(string id)
    property string page: "home"           // home | detail | genre | info
    property string title: ""
    property bool searchOpen: false
    // Images and Text: 0 = lists, 1 = a series page, 2 = the reader. The title shows in the middle with a way back.
    property int depth: 0
    property string subTitle: ""
    signal modeBack()
    readonly property alias actionSlot: actionRow
    property string platform: ""
    property var platforms: []
    property var window
    readonly property bool inner: page !== "home"
    readonly property bool deep: modal && depth > 0
    readonly property bool chrome: page === "home" || page === "detail"
    signal tabSelected(string id)
    signal back()
    signal searchRequested(string text)
    signal toggleSearch()
    signal platformChosen(string name)
    signal categoryChosen(string name, string kind)
    signal prefsRequested()
    signal aboutRequested()
    signal artRequested()
    signal paneToggle()
    // Apple: the sidebar holds the places, search, the mode, the bell and the window buttons; this bar is only a toolbar
    readonly property bool side: Theme.sidebarNav && Theme.mobile !== true
    readonly property bool videoMode: (Core.manga.mode || "video") === "video"   // the cover / background button is for Video titles only
    readonly property bool research: (Core.manga.mode || "video") === "research"   // the browser has its own controls
    // A phone: the top app bar (Material 64 dp, iOS 44 pt) under the status bar (Theme.safe.top); the view switcher,
    // the catalog and the reload are in the bottom bar and its "More" sheet, so only search, the mode, the bell and the
    // menu stay here; no window buttons on a phone platform (a narrow desktop window keeps them).
    readonly property bool mobile: Theme.mobile === true
    readonly property bool phoneOs: Qt.platform.os === "android" || Qt.platform.os === "ios"
    readonly property real safeTop: mobile && Theme.safe !== undefined && Theme.safe.top !== undefined ? Theme.safe.top : 0
    height: (mobile && Theme.mob !== undefined && Theme.mob.appBarH !== undefined ? Theme.mob.appBarH : Theme.spacing.headerH) + safeTop
    // (a narrow desktop window keeps the window buttons on the start page only: a reader's bar has no room for them)
    readonly property bool captions: (!side || Theme.layout.windowControls === "native-right") && !phoneOs && !(mobile && (deep || inner))
    // A title page (Video), a series page (Images, Text) or the Text reader: no bar over the content. The page starts at
    // the top of the window and the way back (and the cover button of a title) float over it in the theme's round
    // buttons; only the window buttons stay at the top right. `floatBack` is false when the back button lives
    // elsewhere (the Text reader has it in the sidebar). `paneW`: the Windows pane under the left end of this bar.
    property bool floating: false
    property bool floatBack: true
    property real paneW: 0
    // the name of the page in the bar itself (GNOME's Study mode: no large titles there)
    property string barTitle: ""
    color: side || floating ? "transparent" : Theme.c.headerbar

    Rectangle { visible: !hb.side && !hb.floating; anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Theme.c.headerLine }

    // dragging and maximizing like a title bar (floating: only over the pane, the rest belongs to the page)
    MouseArea {
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
        width: hb.floating ? hb.paneW : parent.width
        onDoubleClicked: hb.window.visibility === Window.Maximized ? hb.window.showNormal() : hb.window.showMaximized()
        DragHandler { target: null; onActiveChanged: if (active) hb.window.startSystemMove() }
    }

    component MenuLabel: FlatButton {
        id: ml
        property string label: ""
        kind: "flat"
        implicitWidth: mlRow.implicitWidth + 20
        contentItem: Item {
            implicitWidth: mlRow.implicitWidth
            Row {
                id: mlRow
                spacing: 4
                anchors.centerIn: parent
                Text { text: ml.label; color: Theme.c.text; font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                Icon { name: "down"; size: 16; anchors.verticalCenter: parent.verticalCenter }
            }
        }
    }

    // ---- left ----
    Row {
        id: leftBox
        anchors { left: parent.left; leftMargin: 8; verticalCenter: parent.verticalCenter; verticalCenterOffset: hb.safeTop / 2 }
        spacing: 2
        FlatButton { visible: Theme.layout.paneStyle === "docked"; kind: "flat"; glyph: "menu"; iconSize: 18; implicitWidth: 40; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Navigation"); onClicked: hb.paneToggle() }
        Text { visible: Theme.layout.titleBarTitle === true; anchors.verticalCenter: parent.verticalCenter; leftPadding: 6; rightPadding: 8; text: "Reelhouse"; color: Theme.c.text; font.pixelSize: 12; font.family: Theme.type.family }
        FlatButton { visible: (hb.inner || hb.deep) && !hb.floating; kind: "flat"; glyph: "left"; iconSize: 22; implicitWidth: 40; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Back (Esc)"); onClicked: hb.deep ? hb.modeBack() : hb.back() }
        Text {
            visible: hb.barTitle !== "" && !hb.floating
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: Theme.spacing.page - 8
            text: hb.barTitle
            color: Theme.c.text
            font.pixelSize: hb.mobile ? 22 : 20; font.weight: Font.Bold; font.family: Theme.type.display || Theme.type.family
        }
        FlatButton { visible: !hb.inner && !hb.deep && !hb.side && !hb.study && !(hb.floating && hb.mobile); kind: "flat"; glyph: "search"; implicitWidth: 40; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Search (Ctrl+F)"); onClicked: hb.toggleSearch() }
        MenuLabel {
            id: modeBtn
            visible: hb.chrome && !hb.deep && !hb.side && !(hb.floating && hb.mobile)
            label: ({video: Core.tr(Core.uiLang, "Video"), images: Core.tr(Core.uiLang, "Images"), text: Core.tr(Core.uiLang, "Text"), study: Core.tr(Core.uiLang, "Study"), research: Core.tr(Core.uiLang, "Research")})[Core.manga.mode || "video"] || Core.tr(Core.uiLang, "Mode")
            ToolTip.visible: hovered && !modePop.visible
            ToolTip.text: Core.tr(Core.uiLang, "Video, Images (manga and comics) or Text (novels)")
            onClicked: modePop.openBelow(modeBtn)
            Popover {
                id: modePop
                contentItem: Column {
                    width: 220
                    Repeater {
                        model: [{key: "video", label: Core.tr(Core.uiLang, "Video"), icon: "play", on: true}, {key: "images", label: Core.tr(Core.uiLang, "Images"), icon: "image", on: true}, {key: "text", label: Core.tr(Core.uiLang, "Text"), icon: "list", on: true}, {key: "study", label: Core.tr(Core.uiLang, "Study"), icon: "study", on: true}, {key: "research", label: Core.tr(Core.uiLang, "Research"), icon: "globe", on: true}]
                        delegate: Rectangle {
                            required property var modelData
                            width: 220
                            height: 38
                            radius: Theme.shape.small
                            opacity: modelData.on ? 1 : 0.45
                            color: modeArea.containsMouse && modelData.on ? Theme.c.hover : "transparent"
                            Icon { id: mIc; x: 10; anchors.verticalCenter: parent.verticalCenter; name: modelData.icon; size: 16 }
                            Text { anchors { left: mIc.right; leftMargin: 10; verticalCenter: parent.verticalCenter } text: modelData.on ? modelData.label : modelData.label + " (coming soon)"; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
                            Icon { visible: modelData.key === (Core.manga.mode || "video"); anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } name: "check"; size: 16 }
                            MouseArea { id: modeArea; anchors.fill: parent; enabled: modelData.on; hoverEnabled: true; onClicked: { Core.cmd({cmd: "set_mode", mode: modelData.key}); modePop.close() } }
                        }
                    }
                }
            }
        }
        MenuLabel {
            id: catBtn
            visible: hb.chrome && !hb.modal && !hb.study && !hb.research && !hb.side && !hb.mobile
            label: hb.platform === "" ? Core.tr(Core.uiLang, "Catalog") : hb.platform
            ToolTip.visible: hovered && !catPop.visible
            ToolTip.text: Core.tr(Core.uiLang, "Choose a streaming platform for Discover, or browse anime, movies and series by category")
            onClicked: catPop.openBelow(catBtn)
            CatalogPopover {
                id: catPop
                platform: hb.platform
                platforms: hb.platforms
                onPlatformChosen: function(name) { hb.platformChosen(name) }
                onCategoryChosen: function(name, kind) { hb.categoryChosen(name, kind) }
            }
        }
        FlatButton { visible: hb.chrome && !hb.modal && !hb.study && !hb.research && !hb.side && !hb.mobile; kind: "flat"; glyph: "refresh"; implicitWidth: 40; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Reload the covers and pictures"); onClicked: Core.reloadPictures() }
        // how far the reload of the pictures is
        Text {
            visible: hb.chrome && !hb.modal && !hb.study && !hb.research && !hb.side && !hb.mobile && Core.picReload.active === true
            anchors.verticalCenter: parent.verticalCenter
            text: Core.tr(Core.uiLang, "Reloading the pictures") + "  " + (Core.picReload.done || 0) + " / " + (Core.picReload.asked || 0)
            color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family
        }
    }
    // slim bar on the lower edge of the header
    Item {
        visible: Core.picReload.active === true
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: 3
        Rectangle { anchors.fill: parent; color: Theme.c.fill3 }
        Rectangle {
            height: parent.height
            width: (Core.picReload.asked || 0) > 0 ? parent.width * Math.min(1, (Core.picReload.done || 0) / Core.picReload.asked) : 0
            color: Theme.c.accent
            Behavior on width { NumberAnimation { duration: 180 } }
        }
    }

    // ---- center ----
    Row {
        visible: !hb.inner && !hb.modal && !hb.study && !hb.research && !hb.side && !hb.mobile   // these tabs are for video (a phone has them in the bottom bar)
        anchors.centerIn: parent
        anchors.verticalCenterOffset: hb.safeTop / 2
        spacing: 12
        Repeater {
            model: [ {id: "discover", label: Core.tr(Core.uiLang, "Discover"), icon: "compass"}, {id: "library", label: Core.tr(Core.uiLang, "Library"), icon: "star"}, {id: "calendar", label: Core.tr(Core.uiLang, "Calendar"), icon: "calendar"} ]
            delegate: Rectangle {
                required property var modelData
                width: pillRow.implicitWidth + 36
                height: 34
                radius: Theme.shape.control
                color: hb.tab === modelData.id ? Theme.c.selected : (pillArea.containsMouse ? Theme.c.hover : "transparent")
                Row {
                    id: pillRow
                    anchors.centerIn: parent
                    spacing: 7
                    Icon { name: modelData.icon; size: 16; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: modelData.label; color: Theme.c.text; font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                }
                MouseArea { id: pillArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: hb.tabSelected(modelData.id) }
            }
        }
    }
    // Images and Text: Library, Explore and History (the view switcher)
    Row {
        visible: !hb.inner && hb.modal && !hb.deep && !hb.side && !hb.mobile
        anchors.centerIn: parent
        anchors.verticalCenterOffset: hb.safeTop / 2
        spacing: 12
        Repeater {
            model: [ {id: "library", label: Core.tr(Core.uiLang, "Library"), icon: "bookmark"}, {id: "explore", label: Core.tr(Core.uiLang, "Explore"), icon: "compass"}, {id: "history", label: Core.tr(Core.uiLang, "History"), icon: "history"} ]
            delegate: Rectangle {
                required property var modelData
                width: mpRow.implicitWidth + 36
                height: 34
                radius: Theme.shape.control
                color: hb.mtab === modelData.id ? Theme.c.selected : (mpArea.containsMouse ? Theme.c.hover : "transparent")
                Row {
                    id: mpRow
                    anchors.centerIn: parent
                    spacing: 7
                    Icon { name: modelData.icon; size: 16; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: modelData.label; color: Theme.c.text; font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                }
                MouseArea { id: mpArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: hb.modeTabSelected(modelData.id) }
            }
        }
    }
    Row {
        visible: (hb.page === "detail" || hb.page === "genre") && !hb.side && !hb.mobile
        anchors.centerIn: parent
        anchors.verticalCenterOffset: hb.safeTop / 2
        spacing: 8
        TextField {
            id: innerSearch
            // Android: no word suggestions in search fields. The keyboard's "composing" word lost its place in the field
            // (typing "pokemon" gave "okemonp"); with suggestions off each letter goes straight in.
            inputMethodHints: Qt.platform.os === "android" ? Qt.ImhNoPredictiveText : Qt.ImhNone
            width: Math.max(180, Math.min(420, hb.width - leftBox.width - rightBox.width - (hb.page === "detail" ? 330 : 60)))
            height: 34
            anchors.verticalCenter: parent.verticalCenter
            placeholderText: Core.tr(Core.uiLang, "Search movies, series and anime")
            color: Theme.c.text
            placeholderTextColor: Theme.c.textDim
            font.pixelSize: 15
            font.family: Theme.type.family
            leftPadding: 36
            selectByMouse: true
            background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: innerSearch.activeFocus ? 2 : 0; border.color: Theme.c.accent }
            Icon { name: "search"; size: 16; color: Theme.c.textDim; anchors { left: parent.left; leftMargin: 10; verticalCenter: parent.verticalCenter } }
            onAccepted: if (text.trim() !== "") { hb.searchRequested(text.trim()); text = "" }
        }
        Repeater {
            model: hb.page === "detail" ? [{id: "library", label: Core.tr(Core.uiLang, "Library"), icon: "star"}, {id: "calendar", label: Core.tr(Core.uiLang, "Calendar"), icon: "calendar"}] : []
            delegate: FlatButton {
                required property var modelData
                kind: "flat"
                glyph: modelData.icon
                text: modelData.label
                implicitHeight: 34
                anchors.verticalCenter: parent.verticalCenter
                onClicked: hb.tabSelected(modelData.id)
            }
        }
    }
    // a series or a chapter: its name, and under it the chapter
    Column {
        // (a phone: only when the page's buttons leave room for it)
        visible: hb.deep && !hb.inner && !hb.floating && (!hb.mobile || hb.width - leftBox.width - rightBox.width > 150)
        anchors.centerIn: parent
        anchors.verticalCenterOffset: hb.safeTop / 2
        width: Math.max(100, Math.min(implicitWidth, hb.width - leftBox.width - rightBox.width - 60))
        Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight; text: hb.title; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
        Text { visible: hb.subTitle !== ""; width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight; text: hb.subTitle; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
    }
    Text {
        visible: hb.page === "info" && !hb.floating
        anchors.centerIn: parent
        anchors.verticalCenterOffset: hb.safeTop / 2
        width: Math.min(implicitWidth, hb.width - (hb.mobile ? 180 : 400))
        elide: Text.ElideRight
        text: hb.title
        color: Theme.c.text
        font.pixelSize: 15
        font.weight: Font.Bold
        font.family: Theme.type.family
    }

    // ---- right ----
    Row {
        id: rightBox
        anchors { right: parent.right; rightMargin: hb.captions && hb.side ? 0 : 8; verticalCenter: parent.verticalCenter; verticalCenterOffset: hb.safeTop / 2 }
        spacing: 6
        // what the page offers (filled by the page): one group, then a thin line before the notifications and the menu
        Row { id: actionRow; spacing: 2; anchors.verticalCenter: parent.verticalCenter }
        // connected game controllers (icon in the colour of the model)
        Repeater {
            model: hb.window && hb.window.pads ? hb.window.pads : []
            delegate: PadBadge {
                required property var modelData
                anchors.verticalCenter: parent.verticalCenter
                kind: modelData.kind; name: modelData.name; compact: true
                HoverHandler { id: padHov }
                ToolTip.visible: padHov.hovered
                ToolTip.text: Core.tr(Core.uiLang, "Connected") + ": " + modelData.name
            }
        }
        Rectangle { visible: actionRow.visibleChildren.length > 1 && hb.chrome; width: 1; height: 20; anchors.verticalCenter: parent.verticalCenter; color: Theme.c.headerLine }
        // the Pomodoro timer (PomodoroButton.qml): the stopwatch, and the time left while it runs
        PomodoroButton { visible: hb.chrome && !hb.side && !hb.mobile; anchors.verticalCenter: parent.verticalCenter }
        FlatButton {
            id: bellBtn
            visible: hb.chrome && !hb.side && !(hb.mobile && (hb.deep || hb.inner))
            kind: "flat"
            readonly property int count: Core.notices.items !== undefined ? Core.notices.items.length : 0
            implicitWidth: bellRow.implicitWidth + 20
            ToolTip.visible: hovered && !bellPop.visible
            ToolTip.text: Core.tr(Core.uiLang, "Notifications")
            contentItem: Item {
                implicitWidth: bellRow.implicitWidth
                Row {
                    id: bellRow
                    anchors.centerIn: parent
                    spacing: 4
                    Icon { name: "bell"; size: 18; anchors.verticalCenter: parent.verticalCenter }
                    Rectangle {
                        visible: bellBtn.count > 0
                        anchors.verticalCenter: parent.verticalCenter
                        radius: height / 2
                        color: Theme.c.errorBg
                        implicitWidth: Math.max(18, cnt.implicitWidth + 12)
                        implicitHeight: 18
                        Text { id: cnt; anchors.centerIn: parent; text: "" + bellBtn.count; color: Theme.c.errorFg; font.pixelSize: 11; font.weight: Font.ExtraBold; font.family: Theme.type.family }
                    }
                }
            }
            onClicked: bellPop.openBelow(bellBtn, true)
            BellPopover { id: bellPop }
        }
        FlatButton {
            id: menuBtn
            visible: hb.chrome && !hb.side && !(hb.mobile && (hb.deep || hb.inner))
            kind: "flat"
            glyph: "menu"
            iconSize: 22
            implicitWidth: 40
            ToolTip.visible: hovered && !mainPop.visible
            ToolTip.text: Core.tr(Core.uiLang, "Main menu")
            onClicked: mainPop.openBelow(menuBtn, true)
            Popover {
                id: mainPop
                contentItem: Column {
                    width: 220
                    Repeater {
                        model: [{label: Core.tr(Core.uiLang, "Preferences"), key: "prefs", hint: "Ctrl+,"}, {label: Core.tr(Core.uiLang, "About Reelhouse"), key: "about", hint: ""}, {label: Core.tr(Core.uiLang, "Quit"), key: "quit", hint: "Ctrl+Q"}]
                        delegate: Rectangle {
                            required property var modelData
                            width: 220
                            height: 38
                            radius: Theme.shape.small
                            color: mmArea.containsMouse ? Theme.c.hover : "transparent"
                            Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
                            Text { anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } text: modelData.hint; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
                            MouseArea {
                                id: mmArea
                                anchors.fill: parent
                                hoverEnabled: true
                                onClicked: {
                                    mainPop.close()
                                    if (modelData.key === "prefs") hb.prefsRequested()
                                    else if (modelData.key === "about") hb.aboutRequested()
                                    else Qt.quit()
                                }
                            }
                        }
                    }
                }
            }
        }
        FlatButton { visible: hb.page === "detail" && hb.videoMode && !hb.floating; kind: "flat"; glyph: "image"; implicitWidth: 40; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Cover and background of this title"); onClicked: hb.artRequested() }
        Item { visible: hb.captions; width: 4; height: 1 }
        WinButton { visible: hb.captions; icon: "min"; anchors.verticalCenter: parent.verticalCenter; onClicked: hb.window.showMinimized() }
        WinButton { visible: hb.captions; icon: hb.window && hb.window.visibility === Window.Maximized ? "restore" : "max"; anchors.verticalCenter: parent.verticalCenter; onClicked: hb.window.visibility === Window.Maximized ? hb.window.showNormal() : hb.window.showMaximized() }
        WinButton { visible: hb.captions; icon: "close"; anchors.verticalCenter: parent.verticalCenter; onClicked: Qt.quit() }
    }

    // ---- floating: back, and the cover and background of a title, as round buttons over the page ----
    Flow {
        visible: hb.floating
        x: hb.paneW + 10
        y: 8 + hb.safeTop
        width: hb.mobile ? 100 : 40
        spacing: 8
        ModeButton {
            visible: hb.floatBack && (hb.inner || hb.deep)
            diameter: 40
            glyph: "left"
            tip: Core.tr(Core.uiLang, "Back (Esc)")
            onClicked: hb.deep ? hb.modeBack() : hb.back()
        }
        ModeButton {
            visible: hb.page === "detail" && hb.videoMode
            diameter: 40
            glyph: "image"
            tip: Core.tr(Core.uiLang, "Cover and background of this title")
            onClicked: hb.artRequested()
        }
    }
}
