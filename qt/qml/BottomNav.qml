import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The navigation of a phone (and of a narrow Android window), along the bottom: three to five destinations, each an icon
// with its label, and "More" for everything the rail keeps (the mode, the catalog, notifications, reload, preferences).
// Two looks, chosen by the theme (Theme.mob.nav):
//   bar      Material 3 navigation bar: 80 dp, surface-container colour, the selected place in a 64x32 pill
//            (secondary container), 12 sp labels, 48 dp targets;
//   tab-bar  iOS tab bar: 49 pt, a hair line on top, 25 pt symbols with 10-11 pt labels, the selected one in the accent.
// Both stay clear of the home indicator (Theme.safe.bottom).
Rectangle {
    id: bn
    property string tab: "discover"
    property string mtab: "library"
    property string platform: ""
    property var platforms: []
    readonly property string mode: Core.manga.mode || "video"
    readonly property bool study: mode === "study"
    readonly property bool modal: mode === "images" || mode === "text" || mode === "study"
    // Study mode: the page shown (decks, learn, time), set by Main
    property string studyView: "decks"
    // Study > Conversation (KaiwaPage, local AI) is desktop only: not on Android or iOS (a narrow desktop window keeps it)
    readonly property bool talkTab: Qt.platform.os !== "android" && Qt.platform.os !== "ios"
    signal tabSelected(string id)
    signal modeTabSelected(string id)
    signal searchToggled()
    signal platformChosen(string name)
    signal categoryChosen(string name, string kind)
    signal prefsRequested()
    signal aboutRequested()
    // Another profile is in use: back to the start page of the mode (its lists now are the other person's).
    function afterSwitch() {
        if (bn.study) bn.modeTabSelected("decks")
        else if (!bn.modal) bn.tabSelected(bn.tab === "search" ? "discover" : bn.tab)
    }
    readonly property var mob: Theme.mob !== undefined ? Theme.mob : ({})
    readonly property bool ios: mob.nav === "tab-bar"
    readonly property real safeBottom: Theme.safe !== undefined && Theme.safe.bottom !== undefined ? Theme.safe.bottom : 0
    readonly property int barH: mob.barH !== undefined ? mob.barH : (ios ? 49 : 80)
    readonly property int labelSize: mob.navLabel !== undefined ? mob.navLabel : (ios ? 11 : 12)
    height: barH + safeBottom
    color: Theme.c.tabBar !== undefined ? Theme.c.tabBar : Theme.c.surface
    // iOS: the hair line along the top of the tab bar
    Rectangle { visible: bn.ios; anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Theme.c.separator !== undefined ? Theme.c.separator : Theme.c.divider }

    component Dest: Item {
        id: d
        property string icon: ""
        property string label: ""
        property bool sel: false
        property bool badge: false
        signal activated()
        height: bn.barH
        readonly property color ink: bn.ios ? (d.sel ? Theme.c.accent : Theme.c.textDim)
                                            : (d.sel ? (Theme.c.onSecondaryContainer !== undefined ? Theme.c.onSecondaryContainer : Theme.c.text) : Theme.c.textDim)
        // Material: the pill behind the icon (it widens when selected); iOS: nothing behind the symbol
        Rectangle {
            id: pill
            width: bn.ios ? 44 : (d.sel ? 64 : 40); height: bn.ios ? 28 : 32; radius: 16
            x: (parent.width - width) / 2; y: bn.ios ? 4 : 12
            color: bn.ios ? "transparent" : (d.sel ? (Theme.c.secondaryContainer !== undefined ? Theme.c.secondaryContainer : Theme.c.sidebarSelected) : (da.containsMouse ? Theme.c.hover : "transparent"))
            Behavior on width { NumberAnimation { duration: Theme.motion.fast; easing.type: Easing.OutCubic } }
            Icon { anchors.centerIn: parent; name: d.icon; size: bn.ios ? 25 : 24; color: d.ink }
            Rectangle { visible: d.badge; x: parent.width / 2 + 6; y: 2; width: 8; height: 8; radius: 4; color: Theme.c.error }
        }
        Text {
            anchors { horizontalCenter: parent.horizontalCenter; top: pill.bottom; topMargin: bn.ios ? 1 : 4 }
            text: d.label; color: d.ink; font.pixelSize: bn.labelSize; font.weight: d.sel ? Font.Bold : Font.Medium; font.family: Theme.type.family
            elide: Text.ElideRight; width: parent.width - 4; horizontalAlignment: Text.AlignHCenter
        }
        // the press ripple's stand-in: the state layer of the whole destination
        Rectangle { anchors.fill: parent; color: Theme.c.text; opacity: da.pressed ? 0.08 : 0 }
        MouseArea { id: da; anchors.fill: parent; hoverEnabled: !Theme.touch; cursorShape: Qt.PointingHandCursor; onClicked: d.activated() }
        Accessible.role: Accessible.Button
        Accessible.name: d.label
    }

    Row {
        id: row
        anchors { left: parent.left; right: parent.right; top: parent.top }
        readonly property int n: bn.study ? (bn.talkTab ? 5 : 4) : (bn.modal ? 4 : 5)
        Repeater {
            model: bn.study ? [{id: "decks", label: Core.tr(Core.uiLang, "Decks"), icon: "study"}, {id: "learn", label: Core.tr(Core.uiLang, "Learn"), icon: "puzzle"}].concat(bn.talkTab ? [{id: "talk", label: Core.tr(Core.uiLang, "Conversation"), icon: "message"}] : []).concat([{id: "time", label: Core.tr(Core.uiLang, "Time"), icon: "stats"}]) : bn.modal
                ? [{id: "library", label: Core.tr(Core.uiLang, "Library"), icon: "bookmark"}, {id: "explore", label: Core.tr(Core.uiLang, "Explore"), icon: "compass"}, {id: "history", label: Core.tr(Core.uiLang, "History"), icon: "history"}]
                : [{id: "discover", label: Core.tr(Core.uiLang, "Discover"), icon: "compass"}, {id: "library", label: Core.tr(Core.uiLang, "Library"), icon: "star"}, {id: "calendar", label: Core.tr(Core.uiLang, "Calendar"), icon: "calendar"}]
            delegate: Dest {
                required property var modelData
                width: bn.width / row.n
                icon: modelData.icon; label: modelData.label
                sel: bn.study ? (bn.studyView === "learn" || bn.studyView === "talk" || bn.studyView === "time" ? bn.studyView : "decks") === modelData.id : (bn.modal ? bn.mtab : bn.tab) === modelData.id
                onActivated: bn.modal ? bn.modeTabSelected(modelData.id) : bn.tabSelected(modelData.id)
            }
        }
        Dest { width: bn.width / row.n; visible: !bn.modal; icon: "search"; label: Core.tr(Core.uiLang, "Search"); sel: bn.tab === "search"; onActivated: bn.searchToggled() }
        Dest {
            id: moreDest
            width: bn.width / row.n
            icon: "more"; label: Core.tr(Core.uiLang, "More")
            badge: Core.notices.items !== undefined && Core.notices.items.length > 0
            onActivated: morePop.openBelow(moreDest, true)
            Popover {
                id: morePop
                contentItem: Column {
                    width: 280
                    Repeater {
                        model: {
                            var m = []
                            const lbl = {video: Core.tr(Core.uiLang, "Video"), images: Core.tr(Core.uiLang, "Images"), text: Core.tr(Core.uiLang, "Text")}
                            m.push({key: "mode:video", label: lbl.video, icon: "play", on: bn.mode === "video"})
                            m.push({key: "mode:images", label: lbl.images, icon: "image", on: bn.mode === "images"})
                            m.push({key: "mode:text", label: lbl.text, icon: "list", on: bn.mode === "text"})
                            m.push({key: "mode:study", label: Core.tr(Core.uiLang, "Study"), icon: "study", on: bn.mode === "study"})
                            m.push({key: "mode:research", label: Core.tr(Core.uiLang, "Research"), icon: "globe", on: bn.mode === "research"})
                            m.push({key: "sep", label: "", icon: "", on: false})
                            m.push({key: "profiles", label: Core.tr(Core.uiLang, "Profiles"), icon: "person", on: false})
                            if (!bn.modal) m.push({key: "catalog", label: bn.platform === "" ? Core.tr(Core.uiLang, "Catalog") : bn.platform, icon: "list", on: false})
                            m.push({key: "bell", label: Core.tr(Core.uiLang, "Notifications"), icon: "bell", on: false})
                            m.push({key: "reload", label: Core.tr(Core.uiLang, "Reload the covers and pictures"), icon: "refresh", on: false})
                            m.push({key: "prefs", label: Core.tr(Core.uiLang, "Preferences"), icon: "settings", on: false})
                            m.push({key: "about", label: Core.tr(Core.uiLang, "About Reelhouse"), icon: "info", on: false})
                            m.push({key: "quit", label: Core.tr(Core.uiLang, "Quit"), icon: "close", on: false})
                            return m
                        }
                        delegate: Rectangle {
                            required property var modelData
                            readonly property bool sep: modelData.key === "sep"
                            width: 280; height: sep ? 9 : (bn.ios ? 44 : 56); radius: bn.ios ? Theme.shape.control : 28
                            color: sep ? "transparent" : (modelData.on ? (Theme.c.secondaryContainer !== undefined ? Theme.c.secondaryContainer : Theme.c.sidebarSelected) : (mArea.containsMouse || mArea.pressed ? Theme.c.hover : "transparent"))
                            Rectangle { visible: sep; anchors.centerIn: parent; width: parent.width - 32; height: 1; color: Theme.c.divider }
                            Icon { id: mi; visible: !sep; x: 16; anchors.verticalCenter: parent.verticalCenter; name: modelData.icon; size: 22; color: bn.ios && !modelData.on ? Theme.c.accent : Theme.c.text }
                            Text { visible: !sep; anchors { left: mi.right; leftMargin: 14; right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter } elide: Text.ElideRight; text: modelData.label; color: Theme.c.text; font.pixelSize: 16; font.weight: modelData.on ? Font.Bold : Font.Medium; font.family: Theme.type.family }
                            Icon { visible: !sep && modelData.on; anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter } name: "check"; size: 18; color: Theme.c.text }
                            Rectangle {
                                visible: modelData.key === "bell" && Core.notices.items !== undefined && Core.notices.items.length > 0
                                anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
                                radius: 9; color: Theme.c.errorBg; width: Math.max(18, bc.implicitWidth + 10); height: 18
                                Text { id: bc; anchors.centerIn: parent; text: Core.notices.items !== undefined ? "" + Core.notices.items.length : ""; color: Theme.c.errorFg; font.pixelSize: 11; font.weight: Font.Bold; font.family: Theme.type.family }
                            }
                            MouseArea {
                                id: mArea; anchors.fill: parent; hoverEnabled: !Theme.touch; enabled: !parent.sep
                                onClicked: {
                                    morePop.close()
                                    const k = modelData.key
                                    if (k.indexOf("mode:") === 0) Core.cmd({cmd: "set_mode", mode: k.substring(5)})
                                    else if (k === "catalog") catPop.openBelow(moreDest, true)
                                    else if (k === "bell") bellPop.openBelow(moreDest, true)
                                    else if (k === "profiles") profilePop.openBelow(moreDest, true)
                                    else if (k === "reload") Core.reloadPictures()
                                    else if (k === "prefs") bn.prefsRequested()
                                    else if (k === "about") bn.aboutRequested()
                                    else Qt.quit()
                                }
                            }
                        }
                    }
                }
            }
            CatalogPopover {
                id: catPop
                platform: bn.platform
                platforms: bn.platforms
                onPlatformChosen: function(name) { bn.platformChosen(name) }
                onCategoryChosen: function(name, kind) { bn.categoryChosen(name, kind) }
            }
            BellPopover { id: bellPop }
            ProfilePopover { id: profilePop; onSwitched: bn.afterSwitch() }
        }
    }
}
