import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// The Study mode (flashcards, in the way of Anki): the list of decks with what is due, and the review of a deck.
// Decks come from Anki packages (.apkg), from AnkiWeb, or are made here. Each card is answered Again / Hard / Good / Easy and comes back later.
// Pages: decks (home), add, browse, stats; the review (StudyReview) sits on top of them while a session is open.
Item {
    id: pg
    readonly property var st: Core.study
    readonly property var session: st.session !== undefined ? st.session : null
    readonly property bool inSession: session !== null && session !== undefined
    readonly property var decks: st.decks !== undefined ? st.decks : []
    readonly property var aw: st.ankiweb !== undefined ? st.ankiweb : ({})
    // Anki's colours of the three queues: new blue, learning red, to review green
    readonly property color cNew: Theme.scheme === "light" ? "#1c71d8" : "#78aeed"
    readonly property color cLearn: Theme.c.error
    readonly property color cDue: Theme.c.success
    function tr(s) { return Core.tr(Core.uiLang, s) }
    // one of the macaw's funny lines (src/mascot_lines.rs), random and never the last one
    function quip(m) { const r = Core.state("mascot:" + Core.uiLang + ":" + m); return r && r.text ? r.text : "" }
    // one centred column for every page of the mode, so they line up
    readonly property real cw: Math.min(1120, width - Theme.spacing.page * 2)
    readonly property real x0: (width - cw) / 2
    readonly property bool narrow: cw < 600
    function openMore() { if (inSession) review.openMore() }
    // the pages of the mode: decks (with the review on top of them), add, browse, stats
    property string view: "decks"
    // Conversation (KaiwaPage, local AI) is desktop only: not on Android or iOS (a narrow desktop window keeps it)
    readonly property bool talkOk: Qt.platform.os !== "android" && Qt.platform.os !== "ios"
    property real viewDeck: 0
    function go(v, did) {
        const d = did !== undefined ? did : 0
        if (v === "talk" && !talkOk) v = "decks"
        if (view === "browse" && v !== "browse") Core.cmd({cmd: "study_browse_close"})
        if (view === "stats" && v !== "stats") Core.cmd({cmd: "study_stats_close"})
        viewDeck = d
        view = v
        if (v === "browse") Core.cmd({cmd: "study_browse", did: d, q: ""})
        if (v === "stats") Core.cmd({cmd: "study_stats", did: d})
    }
    onVisibleChanged: {
        if (!visible && view !== "decks" && view !== "learn" && view !== "talk") go("decks")
        if (visible) openedAt = Date.now()
    }
    // how long opening the mode took, up to the first frame drawn after it ("[study] opened in N ms" in the debug
    // report); the frame comes from the render thread, the handler runs here when it is delivered
    property double openedAt: 0
    Connections {
        target: pg.openedAt > 0 ? pg.Window.window : null
        ignoreUnknownSignals: true
        function onFrameSwapped() {
            const ms = Date.now() - pg.openedAt
            pg.openedAt = 0
            Core.cmd({cmd: "perf_log", what: "study", msg: "opened in " + ms + " ms (" + pg.decks.length + " decks)"})
        }
    }
    // a Learn lesson covers the page (Main leaves Escape to it)
    readonly property bool learnLesson: visible && view === "learn" && learnPage.item !== null && learnPage.item.inLesson === true
    function sync() { if (pg.aw.logged === true) Core.cmd({cmd: "ankiweb_sync"}); else ankiLogin.open() }

    FileDialog {
        id: apkgDlg
        title: pg.tr("Choose an Anki package")
        nameFilters: ["Anki (*.apkg)", Core.tr(Core.uiLang, "All files") + " (*)"]
        onAccepted: Core.cmd({cmd: "study_import", path: Core.localPath(selectedFile.toString())})
    }
    Popup {
        id: ankiLogin
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(560, (parent ? parent.width : 600) - 60)
        padding: 22
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        Overlay.modal: Rectangle { color: Theme.c.scrim }
        background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
        contentItem: AnkiWebLogin { width: ankiLogin.availableWidth }
        Connections { target: Core; function onStudyChanged() { if (ankiLogin.opened && Core.study.ankiweb && Core.study.ankiweb.logged === true) ankiLogin.close() } }
    }

    TitlePrompt {
        id: newDeck
        heading: pg.tr("New deck")
        actionText: pg.tr("Create")
        onAccepted: function(t) { Core.cmd({cmd: "study_create_deck", name: t}) }
    }

    // a number of a queue as a small coloured chip (dim when nothing is waiting)
    component CountChip: Rectangle {
        id: chip
        property int value: 0
        property color tone: "gray"
        width: 44; height: 26
        radius: 13
        color: value > 0 ? Qt.rgba(tone.r, tone.g, tone.b, 0.16) : "transparent"
        Text {
            anchors.centerIn: parent
            text: chip.value
            color: chip.value > 0 ? chip.tone : Theme.c.textDim
            opacity: chip.value > 0 ? 1 : 0.55
            font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family
        }
    }

    // a phone with Material: "+ Card" is the FAB at the bottom right of the decks page
    Fab {
        visible: wanted && !pg.inSession && pg.view === "decks" && pg.decks.length > 0
        anchors { right: parent.right; bottom: parent.bottom; margins: 16 }
        glyph: "plus"
        tip: pg.tr("Add cards")
        onClicked: pg.go("add")
    }
    // ================= the decks =================
    Flickable {
        id: fl
        anchors.fill: parent
        visible: !pg.inSession && pg.view === "decks"
        contentWidth: width
        contentHeight: home.height + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        Column {
            id: home
            x: pg.x0
            y: 4
            width: pg.cw
            spacing: 12

            // Decks / Learn / Time are in the app's left sidebar (with the Pomodoro and the study time under them),
            // so the page starts with its own header. "Study" is already written above the page: the large title,
            // or the header bar in GNOME.
            StudyHeader {
                backButton: false
                title: ""
                subtitle: pg.decks.length === 0 ? "" : ((pg.aw.due || 0) > 0 ? (pg.aw.due || 0) + " " + pg.tr("cards to study today") : pg.tr("All done for today"))
                FlatButton {
                    anchors.verticalCenter: parent.verticalCenter
                    // the label carries its own "+": the icon only when the label is hidden (narrow)
                    kind: "plain"; glyph: pg.narrow ? "plus" : ""; text: pg.narrow ? "" : pg.tr("+ Card")
                    enabled: pg.decks.length > 0
                    ToolTip.visible: hovered && pg.narrow; ToolTip.text: pg.tr("Add cards")
                    onClicked: pg.go("add")
                }
                FlatButton {
                    anchors.verticalCenter: parent.verticalCenter
                    kind: "plain"; glyph: "download"; text: pg.narrow ? "" : pg.tr("Implement")
                    ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: pg.tr("Find shared decks on AnkiWeb, or import a file")
                    onClicked: sharedBrowser.open()
                }
                // AnkiWeb's shared decks (with "Import offline" for .apkg / .colpkg files)
                AnkiSharedBrowser { id: sharedBrowser }
                FlatButton {
                    anchors.verticalCenter: parent.verticalCenter
                    kind: "plain"; glyph: "refresh"; text: pg.narrow ? "" : pg.tr("Sync")
                    enabled: pg.aw.busy !== true
                    ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: pg.aw.logged === true ? pg.tr("Pull from AnkiWeb") : pg.tr("Log in to AnkiWeb")
                    onClicked: pg.sync()
                }
                // the trash (deleted cards and decks): written out, so it shows even where icons do not draw
                FlatButton {
                    anchors.verticalCenter: parent.verticalCenter
                    kind: "plain"; glyph: pg.narrow ? "trash" : ""
                    text: pg.narrow ? "" : pg.tr("Trash") + (pg.trashCount > 0 ? " (" + pg.trashCount + ")" : "")
                    ToolTip.visible: hovered && pg.narrow; ToolTip.text: pg.tr("Trash")
                    onClicked: trashPop.open()
                }
                FlatButton {
                    id: homeMore
                    anchors.verticalCenter: parent.verticalCenter
                    // "•••" is text: the Shape icon of "more" does not draw on every machine (HP's Fedora)
                    kind: "flat"; text: "•••"; implicitWidth: 40
                    ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: pg.tr("More")
                    onClicked: homeMenu.openBelow(homeMore, true)
                }
            }

            // today over all decks, and AnkiWeb: one slim line
            // the macaw with a line about today, the review goal, today's XP and the streak
            StudyBuddy {
                visible: pg.decks.length > 0
                width: parent.width
            }

            AnkiPanel {
                visible: pg.decks.length > 0 || pg.aw.logged === true
                width: parent.width
                onLoginRequested: ankiLogin.open()
            }

            // ---- no decks: what to do ----
            StudySurface {
                visible: pg.decks.length === 0
                width: parent.width
                height: empty.implicitHeight + 64
                Column {
                    id: empty
                    anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: 28 }
                    spacing: 14
                    // the macaw waves: nothing to study yet
                    Mascot {
                        // a new funny line each time the empty page shows
                        property string hello: ""
                        onVisibleChanged: if (visible) hello = pg.quip("study_hello")
                        Component.onCompleted: hello = pg.quip("study_hello")
                        anchors.horizontalCenter: parent.horizontalCenter
                        mood: "wave"
                        size: 120
                        say: hello !== "" ? hello : pg.tr("Let's study together!")
                        bubbleSide: "right"
                    }
                    Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: pg.tr("No decks yet"); bold: true; size: 22 }
                    Lbl {
                        width: Math.min(parent.width, 480); anchors.horizontalCenter: parent.horizontalCenter
                        horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; dim: true; size: 14
                        text: pg.tr("Import a deck exported from Anki (.apkg), pull your decks from AnkiWeb, or make a new one. In Anki, export with \"Support older Anki versions\" ticked.")
                    }
                    Flow {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Math.min(parent.width, b1.implicitWidth + b2.implicitWidth + b3.implicitWidth + 16)
                        spacing: 8
                        FlatButton { id: b1; kind: "suggested"; pill: true; glyph: "download"; text: pg.tr("Import a deck (.apkg)"); enabled: pg.st.importing !== true; onClicked: apkgDlg.open() }
                        FlatButton { id: b2; kind: "plain"; glyph: "refresh"; text: pg.aw.logged === true ? pg.tr("Pull from AnkiWeb") : pg.tr("Log in to AnkiWeb"); onClicked: pg.sync() }
                        FlatButton { id: b3; kind: "plain"; glyph: "plus"; text: pg.tr("New deck"); onClicked: newDeck.open() }
                    }
                }
            }

            // ---- the list of decks ----
            StudySurface {
                id: listBox
                visible: pg.decks.length > 0
                width: parent.width
                height: listHead.height + list.implicitHeight
                Item {
                    id: listHead
                    width: parent.width; height: 40
                    Text { x: 18; anchors.verticalCenter: parent.verticalCenter; text: pg.tr("Decks"); color: Theme.c.textDim; font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family }
                    // legend of the chips, over their columns
                    Row {
                        x: parent.width - width - (pg.narrow ? 100 : 162)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4
                        Repeater {
                            model: 3
                            delegate: Text {
                                required property int index
                                // over a 44 px chip: "Aprender" is wider than that, so the word shrinks to fit
                                width: 44; horizontalAlignment: Text.AlignHCenter
                                fontSizeMode: Text.HorizontalFit; minimumPixelSize: 9
                                text: [pg.tr("New"), pg.tr("Learn"), pg.tr("Due")][index]
                                color: [pg.cNew, pg.cLearn, pg.cDue][index]
                                font.pixelSize: 12; font.weight: Font.DemiBold; font.family: Theme.type.family
                            }
                        }
                    }
                    Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Theme.c.divider }
                }
                Column {
                    id: list
                    y: listHead.height
                    width: parent.width
                    Repeater {
                        model: pg.decks.length
                        delegate: Item {
                            id: row
                            required property int index
                            readonly property var d: pg.decks[index] || ({})
                            readonly property int depth: d.depth || 0
                            readonly property int toDo: (d.new || 0) + (d.learn || 0) + (d.review || 0)
                            width: list.width
                            height: depth > 0 ? 58 : 66
                            Rectangle { visible: row.index > 0; x: 16 + row.depth * 24; width: parent.width - x; height: 1; color: Theme.c.divider; opacity: 0.7 }
                            Rectangle {
                                anchors { fill: parent; margins: 4 }
                                radius: Theme.shape.control
                                color: ra.pressed ? Theme.c.selected : (ra.containsMouse ? Theme.c.hover : "transparent")
                            }
                            MouseArea { id: ra; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Core.cmd({cmd: "study_open", id: row.d.id}) }

                            // the deck: a tinted tile for top decks, a small branch for the ones under them
                            Rectangle {
                                id: tile
                                x: 16 + row.depth * 24
                                anchors.verticalCenter: parent.verticalCenter
                                width: row.depth > 0 ? 28 : 38; height: width
                                radius: Theme.layout.buttons === "capsule" ? width / 2 : Theme.shape.medium
                                color: row.depth > 0 ? "transparent" : Qt.rgba(Theme.c.accent.r, Theme.c.accent.g, Theme.c.accent.b, 0.15)
                                Icon { anchors.centerIn: parent; name: row.depth > 0 ? "right" : "study"; size: row.depth > 0 ? 16 : 20; color: row.depth > 0 ? Theme.c.textDim : Theme.c.accentText }
                            }
                            Column {
                                anchors { left: tile.right; leftMargin: 12; right: side.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
                                spacing: 2
                                Text {
                                    width: parent.width; elide: Text.ElideRight
                                    text: row.d.short || ""
                                    color: Theme.c.text
                                    font.pixelSize: row.depth > 0 ? 15 : 16
                                    font.weight: row.depth > 0 ? Font.Medium : Font.DemiBold
                                    font.family: Theme.type.family
                                }
                                Text {
                                    width: parent.width; elide: Text.ElideRight
                                    text: (row.d.total || 0) + " " + pg.tr("cards") + (row.toDo === 0 && (row.d.total || 0) > 0 ? "  ·  " + pg.tr("Done for today") : "")
                                    color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
                                }
                            }
                            Row {
                                id: side
                                anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                                spacing: 4
                                CountChip { anchors.verticalCenter: parent.verticalCenter; value: row.d.new || 0; tone: pg.cNew }
                                CountChip { anchors.verticalCenter: parent.verticalCenter; value: row.d.learn || 0; tone: pg.cLearn }
                                CountChip { anchors.verticalCenter: parent.verticalCenter; value: row.d.review || 0; tone: pg.cDue }
                                Item { width: pg.narrow ? 4 : 10; height: 1 }
                                FlatButton {
                                    anchors.verticalCenter: parent.verticalCenter
                                    kind: row.toDo > 0 ? "suggested" : "plain"
                                    pill: true
                                    glyph: pg.narrow ? "play" : ""
                                    text: pg.narrow ? "" : pg.tr("Study now")
                                    implicitHeight: 34
                                    implicitWidth: pg.narrow ? 40 : Math.max(96, contentItem.implicitWidth + 32)
                                    faded: row.toDo === 0
                                    ToolTip.visible: hovered && pg.narrow; ToolTip.text: pg.tr("Study now")
                                    onClicked: Core.cmd({cmd: "study_open", id: row.d.id})
                                }
                                FlatButton {
                                    id: rowMore
                                    anchors.verticalCenter: parent.verticalCenter
                                    kind: "flatround"; text: "•••"; implicitWidth: 38
                                    ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: pg.tr("More")
                                    onClicked: { deckMenu.did = row.d.id; deckMenu.name = row.d.name; deckMenu.openBelow(rowMore, true) }
                                }
                            }
                        }
                    }
                }
            }

            Text {
                visible: pg.decks.length > 0
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: pg.tr("Click a deck to study it. Keys in the review: Space shows the answer, 1–4 answer it.")
                color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
            }
        }
    }

    // the menu of the page (top right)
    PopMenu {
        id: homeMenu
        items: [
            {key: "new", label: pg.tr("New deck")},
            {key: "browse", label: pg.tr("Browse all cards")},
            {key: "stats", label: pg.tr("Statistics")},
            {key: "trash", label: pg.tr("Trash") + (pg.trashCount > 0 ? " (" + pg.trashCount + ")" : "")},
            {key: "account", label: pg.tr("AnkiWeb account")}
        ]
        onPicked: function(i) {
            const k = items[i].key
            if (k === "new") newDeck.open()
            else if (k === "browse") { if (pg.decks.length > 0) pg.go("browse") }
            else if (k === "stats") { if (pg.decks.length > 0) pg.go("stats") }
            else if (k === "trash") trashPop.open()
            else ankiLogin.open()
        }
    }
    // the menu of a deck
    PopMenu {
        id: deckMenu
        property real did: 0
        property string name: ""
        items: [
            {key: "study", label: pg.tr("Study now")},
            {key: "add", label: pg.tr("Add cards")},
            {key: "browse", label: pg.tr("Browse cards")},
            {key: "stats", label: pg.tr("Statistics")},
            {key: "jlab", label: pg.tr("Jlab mode") + ((pg.st.jlab && (pg.st.jlab.decks || []).indexOf(did) >= 0) ? " ✓" : "")},
            {key: "rename", label: pg.tr("Rename…")},
            {key: "delete", label: pg.tr("Delete…"), danger: true}
        ]
        onPicked: function(i) {
            const k = items[i].key
            if (k === "study") Core.cmd({cmd: "study_open", id: did})
            else if (k === "add") pg.go("add", did)
            else if (k === "browse") pg.go("browse", did)
            else if (k === "stats") pg.go("stats", did)
            else if (k === "jlab") jlabSet.openFor(did, name)
            else if (k === "rename") { renDeck.did = did; renField.text = name; renDeck.open() }
            else { delDeck.did = did; delDeck.name = name; delDeck.open() }
        }
    }

    Popup {
        id: renDeck
        property real did: 0
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        focus: true
        padding: 20
        width: Math.min(440, (Overlay.overlay ? Overlay.overlay.width : 480) - 48)
        Overlay.modal: Rectangle { color: Theme.c.scrim }
        background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
        onOpened: { renField.forceActiveFocus(); renField.selectAll() }
        function apply() { if (renField.text.trim() !== "") { Core.cmd({cmd: "study_rename_deck", id: renDeck.did, name: renField.text.trim()}); renDeck.close() } }
        Column {
            width: parent.width
            spacing: 14
            Lbl { width: parent.width; wrapMode: Text.Wrap; bold: true; size: 18; text: pg.tr("Rename deck") }
            Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; size: 13; text: pg.tr("Use :: to put a deck under another, like Languages::Japanese.") }
            TextField {
                id: renField
                width: parent.width
                height: 38
                color: Theme.c.text
                font.pixelSize: 15
                font.family: Theme.type.family
                selectByMouse: true
                background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: renField.activeFocus ? 2 : 1; border.color: renField.activeFocus ? Theme.c.accent : Theme.c.border }
                onAccepted: renDeck.apply()
            }
            Row {
                anchors.right: parent.right
                spacing: 8
                FlatButton { kind: "plain"; text: pg.tr("Cancel"); onClicked: renDeck.close() }
                FlatButton { kind: "suggested"; text: pg.tr("Rename"); onClicked: renDeck.apply() }
            }
        }
    }

    Popup {
        id: delDeck
        property real did: 0
        property string name: ""
        parent: Overlay.overlay
        anchors.centerIn: parent
        modal: true
        padding: 20
        width: Math.min(420, (Overlay.overlay ? Overlay.overlay.width : 480) - 48)
        Overlay.modal: Rectangle { color: Theme.c.scrim }
        background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
        Column {
            width: parent.width
            spacing: 14
            Lbl { width: parent.width; wrapMode: Text.Wrap; bold: true; size: 18; text: pg.tr("Delete this deck?") }
            Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; text: delDeck.name + " — " + pg.tr("it goes to the trash with its cards; you can restore it for 30 days.") }
            Row {
                anchors.right: parent.right
                spacing: 8
                FlatButton { kind: "plain"; text: pg.tr("Cancel"); onClicked: delDeck.close() }
                FlatButton { kind: "destructive"; text: pg.tr("Delete"); onClicked: { Core.cmd({cmd: "study_delete_deck", id: delDeck.did}); delDeck.close() } }
            }
        }
    }

    // what was deleted lately (cards and decks), to bring back
    readonly property int trashCount: st.trash !== undefined && st.trash !== null ? st.trash.length : 0
    function openTrash() { trashPop.open() }
    StudyTrash { id: trashPop }

    // The other pages of the mode are built the first time they are opened (Learn, Conversation and Time are
    // large) and then kept, so what the user was doing there is still there when they come back.
    component LazyView: Loader {
        property bool shown: false
        property bool opened: false
        anchors.fill: parent
        visible: shown
        active: shown || opened
        // built over a few frames: opening Learn or Time (large pages) no longer holds the window still
        asynchronous: true
        onLoaded: opened = true
    }
    LazyView { id: learnPage; shown: !pg.inSession && pg.view === "learn"; sourceComponent: LearnPage { onSwitchTo: function(v) { pg.go(v) } } }
    LazyView { shown: !pg.inSession && pg.view === "add"; sourceComponent: StudyAdd { decks: pg.decks; models: pg.st.models !== undefined ? pg.st.models : []; startDeck: pg.viewDeck; onBack: pg.go("decks") } }
    LazyView { shown: !pg.inSession && pg.view === "browse"; sourceComponent: StudyBrowse { browse: pg.st.browse !== undefined ? pg.st.browse : null; models: pg.st.models !== undefined ? pg.st.models : []; decks: pg.decks; onBack: pg.go("decks") } }
    // time spent studying, with the Pomodoro (StudyTime.qml)
    LazyView { shown: !pg.inSession && pg.view === "time"; sourceComponent: StudyTime { onSwitchTo: function(v) { pg.go(v) } } }
    // loaded by URL: KaiwaPage.qml is not packed into the Android and iOS builds
    LazyView { shown: pg.talkOk && !pg.inSession && pg.view === "talk"; source: pg.talkOk ? "KaiwaPage.qml" : "" }
    LazyView { shown: !pg.inSession && pg.view === "stats"; sourceComponent: StudyStats { stats: pg.st.stats !== undefined ? pg.st.stats : null; decks: pg.decks; onBack: pg.go("decks") } }

    // ================= the review =================
    StudyReview {
        id: review
        anchors.fill: parent
        visible: pg.inSession
        pageVisible: pg.visible
        session: pg.inSession ? pg.session : null
        cNew: pg.cNew
        cLearn: pg.cLearn
        cDue: pg.cDue
        trashCount: pg.trashCount
        onOpenTrash: trashPop.open()
    }
    // settings of Jlab mode, from the menu of a deck
    JlabSettings { id: jlabSet }
    // tells the study-time counter the user is here (window focus, pointer moving); takes no clicks
    StudyTimeBeacon { anchors.fill: parent; z: 1000 }
}
