import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The Android navigation (Material 3 "Navigation rail"): a narrow rail at the left, every place an icon with its label under it;
// the selected one has a pill behind the icon. At the top: the mode (Video, Images, Text) and search; at the bottom the small actions.
Item {
    id: rl
    property string tab: "discover"
    property string mtab: "library"
    property string platform: ""
    property var platforms: []
    property var window
    readonly property string mode: Core.manga.mode || "video"
    readonly property bool study: mode === "study"
    readonly property bool research: mode === "research"   // the browser has its own controls
    readonly property bool modal: mode === "images" || mode === "text" || mode === "study"
    readonly property int bellCount: Core.notices.items !== undefined ? Core.notices.items.length : 0
    // Study mode: the page shown (decks, learn, time), set by Main
    property string studyView: "decks"
    // Study > Conversation (KaiwaPage, local AI) is desktop only: not on Android or iOS (a narrow desktop window keeps it)
    readonly property bool talkTab: Qt.platform.os !== "android" && Qt.platform.os !== "ios"
    // Text mode: a chapter is open; the reader's buttons come into this rail as icons (ReaderSidebar)
    readonly property bool textReader: (mode === "text" && !!((Core.manga.novels || ({})).reader)) || (mode === "images" && !!Core.manga.reader)
    readonly property Item readerSlot: textReader && visible ? readerSide.slot : null
    // Windows: the compact pane (48 px): icons only, the selected one with the accent pill at the left
    readonly property bool flat: Theme.layout.navIndicator === "pill-left"
    signal tabSelected(string id)
    signal modeTabSelected(string id)
    signal searchToggled()
    signal platformChosen(string name)
    signal categoryChosen(string name, string kind)
    signal prefsRequested()
    signal aboutRequested()
    // Another profile is in use: back to the start page of the mode (its lists now are the other person's).
    function afterSwitch() {
        if (rl.study) rl.modeTabSelected("decks")
        else if (!rl.modal && !rl.research) rl.tabSelected(rl.tab)
    }

    Rectangle { anchors.fill: parent; color: Theme.c.pane }
    MouseArea {
        visible: !rl.flat
        x: 0; y: 0; width: parent.width; height: 64
        onDoubleClicked: rl.window.visibility === Window.Maximized ? rl.window.showNormal() : rl.window.showMaximized()
        DragHandler { target: null; onActiveChanged: if (active) rl.window.startSystemMove() }
    }

    component Dest: Item {
        id: d
        property string icon: ""
        property string label: ""
        property bool sel: false
        property bool badge: false
        signal activated()
        width: rl.width; height: rl.flat ? Theme.spacing.navRowH : Theme.spacing.navRowH + 8
        Rectangle {
            id: pill
            width: rl.flat ? d.width - 8 : (d.sel ? 56 : 40); height: rl.flat ? d.height - 4 : 32; radius: rl.flat ? Theme.shape.control : 16
            x: (parent.width - width) / 2; y: rl.flat ? 2 : 4
            color: d.sel ? Theme.c.sidebarSelected : (da.containsMouse ? Theme.c.hover : "transparent")
            Behavior on width { NumberAnimation { duration: Theme.motion.fast; easing.type: Easing.OutCubic } }
            Behavior on color { ColorAnimation { duration: Theme.motion.fast } }
            Icon { anchors.centerIn: parent; name: d.icon; size: rl.flat ? 16 : 24; color: Theme.c.text }
            Rectangle { visible: rl.flat && d.sel; x: 0; anchors.verticalCenter: parent.verticalCenter; width: 3; height: 16; radius: 1.5; color: Theme.c.accent }
            Rectangle { visible: d.badge; x: parent.width / 2 + 4; y: 4; width: 8; height: 8; radius: 4; color: Theme.c.error }
        }
        ToolTip.visible: rl.flat && da.containsMouse; ToolTip.text: d.label
        Text {
            visible: !rl.flat
            anchors { horizontalCenter: parent.horizontalCenter; top: pill.bottom; topMargin: 4 }
            text: d.label; color: Theme.c.text; font.pixelSize: 12; font.weight: d.sel ? Font.Bold : Font.Medium; font.family: Theme.type.family
            elide: Text.ElideRight; width: parent.width - 8; horizontalAlignment: Text.AlignHCenter
        }
        MouseArea { id: da; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: d.activated() }
    }

    Column {
        id: col
        anchors { left: parent.left; right: parent.right; top: parent.top; topMargin: rl.flat ? 8 : 12 }
        spacing: rl.flat ? 2 : 4
        // the mode: one button that opens the three
        Dest {
            id: modeDest
            icon: rl.mode === "images" ? "image" : (rl.mode === "text" ? "list" : (rl.mode === "study" ? "study" : "play"))
            label: rl.mode === "images" ? Core.tr(Core.uiLang, "Images") : (rl.mode === "text" ? Core.tr(Core.uiLang, "Text") : (rl.mode === "study" ? Core.tr(Core.uiLang, "Study") : Core.tr(Core.uiLang, "Video")))
            onActivated: modePop.openRight(modeDest)
            Popover {
                id: modePop
                contentItem: Column {
                    width: 200
                    Repeater {
                        model: [{key: "video", label: Core.tr(Core.uiLang, "Video"), icon: "play"}, {key: "images", label: Core.tr(Core.uiLang, "Images"), icon: "image"}, {key: "text", label: Core.tr(Core.uiLang, "Text"), icon: "list"}, {key: "study", label: Core.tr(Core.uiLang, "Study"), icon: "study"}, {key: "research", label: Core.tr(Core.uiLang, "Research"), icon: "globe"}]
                        delegate: Rectangle {
                            required property var modelData
                            width: 200; height: 48; radius: 24
                            color: rl.mode === modelData.key ? Theme.c.sidebarSelected : (mArea.containsMouse ? Theme.c.hover : "transparent")
                            Icon { id: mi; x: 16; anchors.verticalCenter: parent.verticalCenter; name: modelData.icon; size: 20; color: Theme.c.text }
                            Text { anchors { left: mi.right; leftMargin: 12; verticalCenter: parent.verticalCenter } text: modelData.label; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Medium; font.family: Theme.type.family }
                            MouseArea { id: mArea; anchors.fill: parent; hoverEnabled: true; onClicked: { modePop.close(); Core.cmd({cmd: "set_mode", mode: modelData.key}) } }
                        }
                    }
                }
            }
        }
        Dest { visible: !rl.modal && !rl.research; icon: "search"; label: Core.tr(Core.uiLang, "Search"); onActivated: rl.searchToggled() }
        Item { width: 1; height: 8 }
        Repeater {
            model: rl.research ? [] : rl.study ? [{id: "decks", label: Core.tr(Core.uiLang, "Decks"), icon: "study"}, {id: "learn", label: Core.tr(Core.uiLang, "Learn"), icon: "puzzle"}].concat(rl.talkTab ? [{id: "talk", label: Core.tr(Core.uiLang, "Conversation"), icon: "message"}] : []).concat([{id: "time", label: Core.tr(Core.uiLang, "Time"), icon: "stats"}]) : rl.modal
                ? [{id: "library", label: Core.tr(Core.uiLang, "Library"), icon: "bookmark"}, {id: "explore", label: Core.tr(Core.uiLang, "Explore"), icon: "compass"}, {id: "history", label: Core.tr(Core.uiLang, "History"), icon: "history"}]
                : [{id: "discover", label: Core.tr(Core.uiLang, "Discover"), icon: "compass"}, {id: "library", label: Core.tr(Core.uiLang, "Library"), icon: "star"}, {id: "calendar", label: Core.tr(Core.uiLang, "Calendar"), icon: "calendar"}]
            delegate: Dest {
                required property var modelData
                icon: modelData.icon; label: modelData.label
                sel: rl.study ? (rl.studyView === "learn" || rl.studyView === "talk" || rl.studyView === "time" ? rl.studyView : "decks") === modelData.id : (rl.modal ? rl.mtab : rl.tab) === modelData.id
                onActivated: rl.modal ? rl.modeTabSelected(modelData.id) : rl.tabSelected(modelData.id)
            }
        }
        Dest {
            id: catDest
            visible: !rl.modal && !rl.research
            icon: "list"; label: rl.platform === "" ? Core.tr(Core.uiLang, "Catalog") : rl.platform
            onActivated: catPop.openRight(catDest)
            CatalogPopover {
                id: catPop
                platform: rl.platform
                platforms: rl.platforms
                onPlatformChosen: function(name) { rl.platformChosen(name) }
                onCategoryChosen: function(name, kind) { rl.categoryChosen(name, kind) }
            }
        }
    }

    // what belongs to the mode, under the places: the reader's buttons (Text), the Pomodoro and today's time (Study),
    // the research tools (Research); it scrolls when the window is short
    Flickable {
        id: sectionArea
        anchors { left: parent.left; right: parent.right; top: col.bottom; topMargin: 8; bottom: foot.top; bottomMargin: 8; leftMargin: 4; rightMargin: 4 }
        contentWidth: width
        contentHeight: sectionCol.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height
        Column {
            id: sectionCol
            width: sectionArea.width
            spacing: 6
            ReaderSidebar { id: readerSide; width: parent.width; visible: rl.textReader }
            Loader {
                id: modeSection
                width: parent.width
                active: source !== ""
                // the Study part (Pomodoro, study time) is built over a few frames, not in the click that opens the mode
                asynchronous: true
                source: rl.study ? "StudySidebar.qml" : (rl.mode === "research" ? "ResearchSidebar.qml" : "")
            }
            Connections { target: modeSection.item; ignoreUnknownSignals: true; function onOpenTime() { rl.modeTabSelected("time") } }
        }
    }

    Column {
        id: foot
        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 12 }
        spacing: 4
        // the profile in use: a click opens the profiles beside the rail
        ProfileButton { anchors.horizontalCenter: parent.horizontalCenter; avatarSize: rl.flat ? 28 : 34; onSwitched: rl.afterSwitch() }
        FlatButton {
            id: bellBtn
            kind: "flat"; glyph: "bell"; implicitWidth: rl.flat ? 40 : 48; implicitHeight: rl.flat ? 40 : 48
            ToolTip.visible: hovered && !bellPop.visible; ToolTip.text: Core.tr(Core.uiLang, "Notifications")
            onClicked: bellPop.openRight(bellBtn)
            BellPopover { id: bellPop }
            Rectangle {
                visible: rl.bellCount > 0
                anchors { right: parent.right; rightMargin: 4; top: parent.top; topMargin: 4 }
                radius: 8; color: Theme.c.errorBg
                implicitWidth: Math.max(16, bc.implicitWidth + 8); implicitHeight: 16
                width: implicitWidth; height: implicitHeight
                Text { id: bc; anchors.centerIn: parent; text: "" + rl.bellCount; color: Theme.c.errorFg; font.pixelSize: 10; font.weight: Font.Bold; font.family: Theme.type.family }
            }
        }
        FlatButton { kind: "flat"; glyph: "refresh"; implicitWidth: rl.flat ? 40 : 48; implicitHeight: rl.flat ? 40 : 48; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Reload the covers and pictures"); onClicked: Core.reloadPictures() }
        // the feedback note (FeedbackBox in Main.qml)
        FlatButton {
            id: feedbackBtn
            visible: !Core.prefs.general || Core.prefs.general.feedback_button !== false
            kind: "flat"; glyph: "message"; implicitWidth: rl.flat ? 40 : 48; implicitHeight: rl.flat ? 40 : 48
            ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Send feedback")
            onClicked: if (rl.window && rl.window.openFeedback) rl.window.openFeedback(feedbackBtn, "")
        }
        FlatButton { kind: "flat"; glyph: "settings"; implicitWidth: rl.flat ? 40 : 48; implicitHeight: rl.flat ? 40 : 48; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Preferences"); onClicked: rl.prefsRequested() }
        FlatButton {
            id: moreBtn
            kind: "flat"; glyph: "more"; implicitWidth: rl.flat ? 40 : 48; implicitHeight: rl.flat ? 40 : 48
            ToolTip.visible: hovered && !morePop.visible; ToolTip.text: Core.tr(Core.uiLang, "Main menu")
            onClicked: morePop.openRight(moreBtn)
            Popover {
                id: morePop
                contentItem: Column {
                    width: 200
                    Repeater {
                        model: [{label: Core.tr(Core.uiLang, "About Reelhouse"), key: "about"}, {label: Core.tr(Core.uiLang, "Quit"), key: "quit"}]
                        delegate: Rectangle {
                            required property var modelData
                            width: 200; height: 48
                            color: moreArea.containsMouse ? Theme.c.hover : "transparent"
                            Text { x: 16; anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
                            MouseArea { id: moreArea; anchors.fill: parent; hoverEnabled: true; onClicked: { morePop.close(); if (modelData.key === "about") rl.aboutRequested(); else Qt.quit() } }
                        }
                    }
                }
            }
        }
    }
}
