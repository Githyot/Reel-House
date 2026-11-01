import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The Apple navigation (HIG "Sidebars"): a sidebar floating over the content in the glass layer. It holds the window buttons
// (traffic lights), the mode switch, search, the places of the app with accent coloured icons, and the small actions at the bottom.
// The header becomes a plain toolbar. GNOME has no sidebar: its header bar does all of this.
Item {
    id: sb
    property string tab: "discover"
    property string mtab: "library"
    property string platform: ""
    property var platforms: []
    property var window
    readonly property string mode: Core.manga.mode || "video"
    readonly property bool study: mode === "study"
    readonly property bool research: mode === "research"   // the browser: no Video places, its own controls below
    readonly property bool modal: mode === "images" || mode === "text" || mode === "study"
    readonly property int bellCount: Core.notices.items !== undefined ? Core.notices.items.length : 0
    // Study mode: the page shown (decks, learn, time; add / browse / stats count as decks), set by Main
    property string studyView: "decks"
    // Study > Conversation (KaiwaPage, local AI) is desktop only: not on Android or iOS (a narrow desktop window keeps it)
    readonly property bool talkTab: Qt.platform.os !== "android" && Qt.platform.os !== "ios"
    // Text mode: a chapter is open in the reader; its buttons then live in this sidebar (ReaderSidebar)
    readonly property bool textReader: (mode === "text" && !!((Core.manga.novels || ({})).reader)) || (mode === "images" && !!Core.manga.reader)
    readonly property Item readerSlot: textReader && visible ? readerSide.slot : null
    // the Library's categories (Images and Text) go under the places: ModeBrowser puts its list here
    readonly property Item librarySlot: modal && mtab === "library" && !textReader && visible ? libSlot : null
    signal tabSelected(string id)
    signal modeTabSelected(string id)
    signal searchRequested(string text)
    signal platformChosen(string name)
    signal categoryChosen(string name, string kind)
    signal prefsRequested()
    signal aboutRequested()
    readonly property bool docked: Theme.layout.paneStyle === "docked"
    // GNOME (Nautilus pattern): flush full-height sidebar, its own small header with the app name, no traffic lights.
    readonly property bool adw: Theme.layout.paneStyle === "adw"
    readonly property bool flat: Theme.layout.segmented === "underline"
    function focusSearch() { search.forceActiveFocus(); search.selectAll() }
    // Another profile is in use: back to the start page of the mode (its lists now are the other person's).
    function afterSwitch() {
        if (sb.study) sb.modeTabSelected("decks")
        else if (!sb.modal && !sb.research) sb.tabSelected(sb.tab)
    }

    Glass { visible: !sb.docked && !sb.adw; anchors.fill: parent; radius: Theme.shape.panel; level: "regular"; shadow: true; bordered: true }
    Rectangle {
        visible: sb.adw
        anchors.fill: parent
        color: Theme.c.sidebar || Theme.c.headerbar
        Rectangle { anchors { top: parent.top; bottom: parent.bottom; right: parent.right } width: 1; color: Theme.c.headerLine || Theme.c.divider }
    }
    Text {
        visible: sb.adw
        x: 18; y: 0; height: 46
        verticalAlignment: Text.AlignVCenter
        text: "Reelhouse"
        color: Theme.c.text
        font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family
    }

    // the top of the panel moves the window like a title bar
    MouseArea {
        visible: !sb.docked
        x: 0; y: 0; width: parent.width; height: 46
        onDoubleClicked: sb.window.visibility === Window.Maximized ? sb.window.showNormal() : sb.window.showMaximized()
        DragHandler { target: null; onActiveChanged: if (active) sb.window.startSystemMove() }
    }

    // traffic lights: close, minimise, zoom (the symbols show when the pointer is over them)
    Row {
        id: lights
        visible: !sb.docked && !sb.adw
        x: 18; y: 17
        spacing: 8
        HoverHandler { id: lightsHover }
        Repeater {
            model: [{c: "#ff5f57", g: "close", a: 0}, {c: "#febc2e", g: "minus", a: 1}, {c: "#28c840", g: "expand", a: 2}]
            delegate: Rectangle {
                required property var modelData
                width: 13; height: 13; radius: 6.5
                color: modelData.c
                border.width: 1
                border.color: "#26000000"
                Icon { visible: lightsHover.hovered; anchors.centerIn: parent; name: modelData.g; size: 9; color: "#99000000" }
                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        if (modelData.a === 0) Qt.quit()
                        else if (modelData.a === 1) sb.window.showMinimized()
                        else sb.window.visibility === Window.Maximized ? sb.window.showNormal() : sb.window.showMaximized()
                    }
                }
            }
        }
    }

    // the profile in use (ProfileButton): at the right end of the top of the panel, level with the traffic lights
    // (Apple) or the app name (GNOME); Windows has no such strip and keeps it at the start of the foot
    ProfileButton {
        visible: !sb.docked
        avatarSize: 26
        x: parent.width - width - 10
        y: (46 - height) / 2
        below: true
        onSwitched: sb.afterSwitch()
    }

    Column {
        id: col
        anchors { left: parent.left; right: parent.right; top: parent.top; topMargin: sb.docked ? 8 : 48; leftMargin: sb.docked ? Theme.spacing.navPad : 12; rightMargin: sb.docked ? Theme.spacing.navPad : 12 }
        spacing: sb.docked ? 4 : 10

        // Video | Images | Text | Study | Research, a capsule segmented control. Five names do not fit a sidebar
        // (least of all in Portuguese: "Vídeo Imagens Texto Estudo Pesquisa"), so each part shows its icon, with the
        // name as a tooltip; the names come back only when every one of them fits its part with room to spare.
        Rectangle {
            id: modeSwitch
            width: parent.width; height: 32; radius: sb.docked ? Theme.shape.control : 16
            color: Theme.c.fill3
            readonly property var modes: [{key: "video", label: Core.tr(Core.uiLang, "Video"), icon: "play"}, {key: "images", label: Core.tr(Core.uiLang, "Images"), icon: "image"}, {key: "text", label: Core.tr(Core.uiLang, "Text"), icon: "list"}, {key: "study", label: Core.tr(Core.uiLang, "Study"), icon: "study"}, {key: "research", label: Core.tr(Core.uiLang, "Research"), icon: "globe"}]
            readonly property int at: Math.max(0, ["video", "images", "text", "study", "research"].indexOf(sb.mode))
            readonly property real segW: (width - 4) / modes.length
            readonly property bool labels: {
                for (let i = 0; i < modes.length; ++i) if (modeFm.advanceWidth(modes[i].label) + 16 > segW) return false
                return true
            }
            FontMetrics { id: modeFm; font.pixelSize: 12; font.weight: Font.DemiBold; font.family: Theme.type.family }
            Rectangle {
                width: (parent.width - 4) / parent.modes.length; height: parent.height - 4; radius: sb.docked ? Theme.shape.control - 1 : height / 2; y: 2
                x: 2 + parent.at * width
                color: Theme.scheme === "light" ? Theme.c.card : Theme.c.buttonHover
                border.width: Theme.scheme === "light" ? 1 : 0
                border.color: Theme.c.border
                Behavior on x { NumberAnimation { duration: Theme.motion.normal; easing.type: Easing.OutQuint } }
            }
            Row {
                anchors.fill: parent; anchors.margins: 2
                Repeater {
                    model: modeSwitch.modes
                    delegate: Item {
                        id: modeSeg
                        required property var modelData
                        width: modeSwitch.segW; height: parent.height
                        // HP: the controller (and Tab) reach the modes too; ✕ / A or Space / Return picks one
                        activeFocusOnTab: true
                        function padActivate() { Core.cmd({cmd: "set_mode", mode: modelData.key}) }
                        Keys.onSpacePressed: padActivate()
                        Keys.onReturnPressed: padActivate()
                        Rectangle {
                            visible: modeSeg.activeFocus
                            anchors.fill: parent
                            radius: sb.docked ? Theme.shape.control - 1 : height / 2
                            color: "transparent"
                            border.width: Theme.shape.focusRing
                            border.color: Theme.c.accent50 !== undefined ? Theme.c.accent50 : Theme.c.accent
                        }
                        Icon { anchors.centerIn: parent; visible: !modeSwitch.labels; name: modelData.icon; size: 16; color: sb.mode === modelData.key ? Theme.c.accentText : Theme.c.text }
                        Text {
                            anchors.centerIn: parent; visible: modeSwitch.labels
                            width: Math.min(implicitWidth, parent.width - 8); elide: Text.ElideRight
                            text: modelData.label; color: Theme.c.text; font.pixelSize: 12; font.weight: sb.mode === modelData.key ? Font.DemiBold : Font.Medium; font.family: Theme.type.family
                        }
                        MouseArea { id: modeTip; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Core.cmd({cmd: "set_mode", mode: modelData.key}) }
                        ToolTip.visible: (modeTip.containsMouse || modeSeg.activeFocus) && !modeSwitch.labels
                        ToolTip.text: modelData.label
                    }
                }
            }
        }

        // search
        Rectangle {
            width: parent.width; height: sb.docked ? 32 : 34; radius: sb.docked ? Theme.shape.control : 17
            color: Theme.c.fill3
            border.width: sb.docked ? 1 : (search.activeFocus ? 2 : 0)
            border.color: sb.docked ? Theme.c.border : Theme.c.accent
            Rectangle { visible: sb.docked && search.activeFocus; anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 2; radius: 1; color: Theme.c.accent }
            visible: !sb.modal && !sb.research
            Icon { name: "search"; size: 15; color: Theme.c.textDim; anchors { left: parent.left; leftMargin: 11; verticalCenter: parent.verticalCenter } }
            TextField {
                id: search
                anchors { fill: parent; leftMargin: 32; rightMargin: 8 }
                background: null
                placeholderText: Core.tr(Core.uiLang, "Search")
                placeholderTextColor: Theme.c.textDim
                color: Theme.c.text
                font.pixelSize: 14
                font.family: Theme.type.family
                selectByMouse: true
                verticalAlignment: TextInput.AlignVCenter
                onAccepted: if (text.trim() !== "") { sb.searchRequested(text.trim()); text = ""; focus = false }
            }
        }

        // the places
        Repeater {
            model: sb.research ? [] : sb.study ? [{id: "decks", label: Core.tr(Core.uiLang, "Decks"), icon: "study"}, {id: "learn", label: Core.tr(Core.uiLang, "Learn"), icon: "puzzle"}].concat(sb.talkTab ? [{id: "talk", label: Core.tr(Core.uiLang, "Conversation"), icon: "message"}] : []).concat([{id: "time", label: Core.tr(Core.uiLang, "Time"), icon: "stats"}]) : sb.modal
                ? [{id: "library", label: Core.tr(Core.uiLang, "Library"), icon: "bookmark"}, {id: "explore", label: Core.tr(Core.uiLang, "Explore"), icon: "compass"}, {id: "history", label: Core.tr(Core.uiLang, "History"), icon: "history"}]
                : [{id: "discover", label: Core.tr(Core.uiLang, "Discover"), icon: "compass"}, {id: "library", label: Core.tr(Core.uiLang, "Library"), icon: "star"}, {id: "calendar", label: Core.tr(Core.uiLang, "Calendar"), icon: "calendar"}]
            delegate: Rectangle {
                id: row
                required property var modelData
                readonly property bool sel: sb.study ? (sb.studyView === "learn" || sb.studyView === "talk" || sb.studyView === "time" ? sb.studyView : "decks") === modelData.id : (sb.modal ? sb.mtab : sb.tab) === modelData.id
                width: col.width; height: Theme.spacing.navRowH; radius: Theme.shape.control
                color: sel ? Theme.c.sidebarSelected : (rowArea.containsMouse ? Theme.c.hover : "transparent")
                Behavior on color { ColorAnimation { duration: Theme.motion.fast } }
                Icon { id: rIc; x: 12; anchors.verticalCenter: parent.verticalCenter; name: modelData.icon; size: sb.docked ? 16 : 18; color: Theme.layout.navIconColor === "accent" ? Theme.c.accent : Theme.c.text }
                Rectangle { visible: row.sel && Theme.layout.navIndicator === "pill-left"; x: 0; anchors.verticalCenter: parent.verticalCenter; width: 3; height: 16; radius: 1.5; color: Theme.c.accent }
                Text { anchors { left: rIc.right; leftMargin: 12; right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter } elide: Text.ElideRight; text: modelData.label; color: Theme.c.text; font.pixelSize: sb.docked ? 14 : 15; font.weight: row.sel ? Font.DemiBold : Font.Normal; font.family: Theme.type.family }
                MouseArea { id: rowArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sb.modal ? sb.modeTabSelected(modelData.id) : sb.tabSelected(modelData.id) }
            }
        }

        // Catalog: platforms and categories (video)
        Rectangle {
            id: catRow
            visible: !sb.modal && !sb.research
            width: col.width; height: Theme.spacing.navRowH; radius: Theme.shape.control
            color: catArea.containsMouse || catPop.visible ? Theme.c.hover : "transparent"
            Icon { id: cIc; x: 12; anchors.verticalCenter: parent.verticalCenter; name: "list"; size: sb.docked ? 16 : 18; color: Theme.layout.navIconColor === "accent" ? Theme.c.accent : Theme.c.text }
            Text { anchors { left: cIc.right; leftMargin: 12; right: cArrow.left; rightMargin: 6; verticalCenter: parent.verticalCenter } elide: Text.ElideRight; text: sb.platform === "" ? Core.tr(Core.uiLang, "Catalog") : sb.platform; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
            Icon { id: cArrow; anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } name: "right"; size: 16; color: Theme.c.textDim }
            MouseArea { id: catArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: catPop.openRight(catRow) }
            CatalogPopover {
                id: catPop
                platform: sb.platform
                platforms: sb.platforms
                onPlatformChosen: function(name) { sb.platformChosen(name) }
                onCategoryChosen: function(name, kind) { sb.categoryChosen(name, kind) }
            }
        }
    }

    // ---- what belongs to the mode, under the places: the reader's buttons (Text), the Pomodoro and the study time
    // (Study), the research tools (Research). It scrolls when the window is short. ----
    Flickable {
        id: sectionArea
        anchors { left: parent.left; right: parent.right; top: col.bottom; topMargin: 10; bottom: sideBottom.implicitHeight > 0 ? sideBottom.top : (reloadBar.visible ? reloadBar.top : foot.top); bottomMargin: 8 }
        anchors.leftMargin: col.anchors.leftMargin
        anchors.rightMargin: col.anchors.rightMargin
        contentWidth: width
        contentHeight: sectionCol.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height
        ScrollBar.vertical: ScrollBar { policy: sectionArea.contentHeight > sectionArea.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
        Column {
            id: sectionCol
            width: sectionArea.width
            spacing: 8
            // kept alive (not in the Loader): the reader's buttons are put into its slot while a chapter is open
            ReaderSidebar { id: readerSide; width: parent.width; visible: sb.textReader }
            // the Library's categories (filled by ModeBrowser, see librarySlot)
            Column { id: libSlot; width: parent.width; spacing: 2; visible: sb.librarySlot !== null }
            // one component per mode; Research brings its own (ResearchSidebar.qml)
            Loader {
                id: modeSection
                width: parent.width
                active: source !== ""
                // the Study part (Pomodoro, study time) is built over a few frames, not in the click that opens the mode
                asynchronous: true
                source: sb.study ? "StudySidebar.qml" : (sb.mode === "research" ? "ResearchSidebar.qml" : "")
            }
            // the time boxes of the Study part open the Time page
            Connections { target: modeSection.item; ignoreUnknownSignals: true; function onOpenTime() { sb.modeTabSelected("time") } }
        }
    }

    // at the bottom of the fixed sidebar: the volume of the theme sounds and music (when a sound or music file is
    // set, or a controller is plugged in), then in controller mode what the buttons do (✕ ○ △ / A B Y, L1 R1...)
    readonly property var gen: Core.prefs.general || ({})
    Column {
        id: sideBottom
        anchors { left: parent.left; right: parent.right; bottom: reloadBar.visible ? reloadBar.top : foot.top; leftMargin: 12; rightMargin: 12; bottomMargin: 8 }
        spacing: 8
        SoundVolumeBox {
            id: soundVol
            width: parent.width
            visible: !!(sb.gen.sound_nav || sb.gen.sound_select || sb.gen.sound_music) || !!(sb.window && sb.window.padMode)
        }
        PadHints {
            id: padLegend
            width: parent.width
            visible: !!(sb.window && sb.window.padMode && sb.window.padLegendInSidebar)
            vertical: true
            kind: sb.window ? sb.window.padKind : "xbox"
            hints: sb.window ? sb.window.padHints : []
        }
    }

    // reload progress, then the small actions at the bottom
    Item {
        id: reloadBar
        visible: Core.picReload.active === true
        anchors { left: parent.left; right: parent.right; bottom: foot.top; leftMargin: 16; rightMargin: 16; bottomMargin: 6 }
        height: 14
        Rectangle { anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter } height: 3; radius: 1.5; color: Theme.c.fill3 }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            height: 3; radius: 1.5; color: Theme.c.accent
            width: (Core.picReload.asked || 0) > 0 ? parent.width * Math.min(1, (Core.picReload.done || 0) / Core.picReload.asked) : 0
            Behavior on width { NumberAnimation { duration: 180 } }
        }
    }
    // the footer buttons shrink a little so six fit the narrowest sidebar (232 px)
    readonly property int footBtn: Math.max(32, Math.min(40, Math.floor((width - 24) / 6)))
    Row {
        id: foot
        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: sb.docked ? 8 : 12 }
        spacing: 2
        // Windows (docked pane, 320 px): the profile in use first; the other themes show it at the top
        ProfileButton { visible: sb.docked; avatarSize: 28; anchors.verticalCenter: parent.verticalCenter; onSwitched: sb.afterSwitch() }
        // the Pomodoro timer (PomodoroButton.qml); 40 px each so five buttons fit the narrowest sidebar (232 px).
        // In the Study mode the timer is open above (StudySidebar), so the button is not repeated here.
        PomodoroButton { side: true; visible: !sb.study }
        FlatButton {
            id: bellBtn
            kind: "flat"; glyph: "bell"; implicitWidth: sb.footBtn
            ToolTip.visible: hovered && !bellPop.visible; ToolTip.text: Core.tr(Core.uiLang, "Notifications")
            onClicked: bellPop.openRight(bellBtn)
            BellPopover { id: bellPop }
            Rectangle {
                visible: sb.bellCount > 0
                anchors { right: parent.right; rightMargin: 2; top: parent.top; topMargin: 2 }
                radius: 8; color: Theme.c.errorBg
                implicitWidth: Math.max(16, bc.implicitWidth + 8); implicitHeight: 16
                width: implicitWidth; height: implicitHeight
                Text { id: bc; anchors.centerIn: parent; text: "" + sb.bellCount; color: Theme.c.errorFg; font.pixelSize: 10; font.weight: Font.Bold; font.family: Theme.type.family }
            }
        }
        FlatButton { kind: "flat"; glyph: "refresh"; implicitWidth: sb.footBtn; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Reload the covers and pictures"); onClicked: Core.reloadPictures() }
        // the feedback note (FeedbackBox in Main.qml; the player has the same button in its bar)
        FlatButton {
            id: feedbackBtn
            visible: !Core.prefs.general || Core.prefs.general.feedback_button !== false
            kind: "flat"; glyph: "message"; implicitWidth: sb.footBtn
            ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Send feedback")
            onClicked: if (sb.window && sb.window.openFeedback) sb.window.openFeedback(feedbackBtn, "")
        }
        FlatButton { kind: "flat"; glyph: "settings"; implicitWidth: sb.footBtn; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Preferences"); onClicked: sb.prefsRequested() }
        FlatButton {
            id: moreBtn
            kind: "flat"; glyph: "more"; implicitWidth: sb.footBtn
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
                            width: 200; height: 36; radius: Theme.shape.small
                            color: moreArea.containsMouse ? Theme.c.hover : "transparent"
                            Text { x: 12; anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
                            MouseArea { id: moreArea; anchors.fill: parent; hoverEnabled: true; onClicked: { morePop.close(); if (modelData.key === "about") sb.aboutRequested(); else Qt.quit() } }
                        }
                    }
                }
            }
        }
    }
}
