import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// AnkiWeb's shared decks (ankiweb.net/shared/decks) inside the Study mode, drawn the way of the app: the site's
// categories on the left, a search on top, the results as cards, and the page of a deck with its description,
// sample cards and reviews. "Add to my decks" downloads the deck and imports it; "Import offline" takes a file.
// The data comes from the core (state "shared", commands shared_*); see src/ankishared.rs.
Popup {
    id: sb
    property var st: ({})
    readonly property var cats: st.categories !== undefined ? st.categories : []
    readonly property var results: st.results !== undefined ? st.results : []
    readonly property var deck: st.info !== undefined && st.info !== null ? st.info : ({})
    readonly property bool deckOpen: (deck.id || 0) > 0
    readonly property var dl: st.dl !== undefined ? st.dl : null
    readonly property bool busy: dl !== null && dl !== undefined
    readonly property bool wide: width >= 860
    readonly property string sortKey: st.sort || "rating"
    readonly property var sortNames: ({rating: "Rating", modified: "Recently updated", notes: "Most notes", audio: "Most audio", images: "Most images", title: "Title"})
    readonly property var sortKeys: ["rating", "modified", "notes", "audio", "images", "title"]
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function refresh() { st = Core.state("shared") }
    function search(page, sort, cat) {
        Core.cmd({cmd: "shared_search", q: q.text, cat: cat !== undefined ? cat : (st.cat || ""), page: page || 0, sort: sort || sortKey})
        refresh()
    }
    function openDeck(id) { Core.cmd({cmd: "shared_info", id: id}); refresh(); detailFl.contentY = 0 }
    function closeDeck() { Core.cmd({cmd: "shared_info", id: 0}); refresh() }
    function add(id) { Core.cmd({cmd: "shared_download", id: id}); refresh() }
    function back() { if (deckOpen) closeDeck(); else close() }
    // 1234 -> "1.2k"
    function shortNum(n) { n = n || 0; return n >= 10000 ? Math.round(n / 1000) + "k" : n >= 1000 ? (n / 1000).toFixed(1) + "k" : "" + n }
    function day(secs) { return secs > 0 ? new Date(secs * 1000).toLocaleDateString(Qt.locale(Core.uiLang.replace("-", "_")), Locale.ShortFormat) : "" }
    function dlText() {
        if (!busy) return ""
        if (dl.stage === "import") return tr("Adding to your decks…")
        return dl.pct >= 0 ? tr("Downloading…") + " " + dl.pct + "%" : tr("Downloading…")
    }
    readonly property bool pill: Theme.look !== "windows"

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(1180, (parent ? parent.width : 1000) - 40)
    height: (parent ? parent.height : 760) - 48
    modal: true
    focus: true
    padding: 0
    // Escape steps back from a deck's page first (the Shortcut below)
    closePolicy: Popup.CloseOnPressOutside
    Overlay.modal: Rectangle { color: Theme.c.scrim }
    background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
    onOpened: { Core.cmd({cmd: "shared_open"}); refresh(); q.text = st.query || ""; q.forceActiveFocus() }
    onClosed: Core.cmd({cmd: "shared_close"})

    Shortcut { sequence: "Esc"; enabled: sb.opened; onActivated: sb.back() }
    Connections { target: Core; function onStudyChanged() { if (sb.opened) sb.refresh() } }
    // the download progress is not pushed as events: look while something is running
    Timer { interval: 300; repeat: true; running: sb.opened && (sb.busy || sb.st.loading === true || sb.deck.loading === true); onTriggered: sb.refresh() }
    // search a moment after typing stops
    Timer { id: typing; interval: 600; onTriggered: sb.search(0) }

    FileDialog {
        id: offline
        title: sb.tr("Choose an Anki package")
        nameFilters: ["Anki (*.apkg *.colpkg)", sb.tr("All files") + " (*)"]
        onAccepted: Core.cmd({cmd: "study_import", path: Core.localPath(selectedFile.toString())})
    }
    PopMenu {
        id: sortMenu
        menuWidth: 220
        items: sb.sortKeys.map(function(k) { return {label: sb.tr(sb.sortNames[k]), checked: k === sb.sortKey} })
        onPicked: function(i) { sb.search(0, sb.sortKeys[i]) }
    }

    // a category of the site as a chip (selected: the accent)
    component CatChip: Rectangle {
        id: cc
        property string label: ""
        property bool on: false
        signal picked()
        implicitWidth: ct.implicitWidth + 26
        implicitHeight: 32
        radius: sb.pill ? height / 2 : Theme.shape.control
        color: on ? Theme.c.accent : (cm.containsMouse ? Theme.c.hover : Theme.c.fill3)
        border.width: !on && Theme.look === "windows" ? 1 : 0
        border.color: Theme.c.border
        Text {
            id: ct
            anchors.centerIn: parent
            text: cc.label
            color: cc.on ? Theme.c.accentFg : Theme.c.text
            font.pixelSize: 14; font.weight: cc.on ? Font.DemiBold : Font.Normal; font.family: Theme.type.family
        }
        MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: cc.picked() }
    }
    // thumbs up against thumbs down, as the site's like bar
    component LikeBar: Row {
        id: lb
        property int up: 0
        property int down: 0
        spacing: 6
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 44; height: 6; radius: 3
            color: lb.up + lb.down > 0 ? Theme.c.error : Theme.c.fill3
            Rectangle { width: lb.up + lb.down > 0 ? parent.width * lb.up / (lb.up + lb.down) : 0; height: parent.height; radius: 3; color: Theme.c.success }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: lb.up + lb.down > 0 ? Math.round(lb.up * 100 / (lb.up + lb.down)) + "% · " + sb.shortNum(lb.up + lb.down) : sb.tr("No ratings yet")
            color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
        }
    }
    // a small count with an icon (notes, audio, images)
    component Badge: Row {
        id: bd
        property string glyph: ""
        property string label: ""
        spacing: 4
        visible: label !== ""
        Icon { anchors.verticalCenter: parent.verticalCenter; name: bd.glyph; size: 14; color: Theme.c.textDim }
        Text { anchors.verticalCenter: parent.verticalCenter; text: bd.label; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
    }
    // the bar of an add in progress
    component DlBar: Column {
        id: db
        property bool shown: false
        visible: shown
        spacing: 4
        Rectangle {
            width: db.width; height: 6; radius: 3
            color: Theme.c.fill3
            Rectangle {
                height: parent.height; radius: 3
                color: Theme.c.accent
                width: sb.busy && sb.dl.stage === "download" && sb.dl.pct >= 0 ? parent.width * sb.dl.pct / 100 : parent.width
                opacity: sb.busy && (sb.dl.stage === "import" || sb.dl.pct < 0) ? 0.45 + 0.35 * pulse.v : 1
            }
        }
        Text { width: db.width; elide: Text.ElideRight; text: sb.dlText(); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
    }
    // the bar breathes while the size is unknown and during the import
    Item {
        id: pulse
        visible: false
        property real v: 0
        SequentialAnimation on v {
            running: sb.opened && sb.busy
            loops: Animation.Infinite
            NumberAnimation { from: 0; to: 1; duration: 700 }
            NumberAnimation { from: 1; to: 0; duration: 700 }
        }
    }

    contentItem: Item {
        // ---------- header ----------
        Item {
            id: head
            x: 20; width: parent.width - 40
            y: 14; height: 48
            FlatButton {
                id: backBtn
                anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                kind: Theme.look === "android" || Theme.look === "apple" ? "flatround" : "flat"
                glyph: sb.deckOpen ? "left" : "close"; iconSize: 20
                implicitWidth: 40; implicitHeight: 40
                ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: (sb.deckOpen ? sb.tr("Back") : sb.tr("Close")) + " (Esc)"
                onClicked: sb.back()
            }
            Column {
                anchors { left: backBtn.right; leftMargin: 10; right: headActs.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
                spacing: 1
                Text {
                    width: parent.width; elide: Text.ElideRight
                    text: sb.tr("Shared decks")
                    color: Theme.c.text; font.pixelSize: 20; font.weight: Font.Bold; font.family: Theme.type.display || Theme.type.family
                }
                Text {
                    width: parent.width; elide: Text.ElideRight
                    text: sb.tr("From the Anki community on AnkiWeb")
                    color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family
                }
            }
            Row {
                id: headActs
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                spacing: 6
                FlatButton {
                    anchors.verticalCenter: parent.verticalCenter
                    kind: "plain"; glyph: "download"; text: sb.wide ? sb.tr("Import offline") : ""
                    enabled: sb.st.importing !== true
                    ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: sb.tr("Add a deck file you downloaded (.apkg, .colpkg)")
                    onClicked: offline.open()
                }
            }
        }

        // ---------- the categories ----------
        Flickable {
            id: side
            x: 20
            y: head.y + head.height + 10
            width: sb.wide ? 260 : parent.width - 40
            height: sb.wide ? parent.height - y - 16 : Math.min(catCol.implicitHeight, 92)
            contentHeight: catCol.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Column {
                id: catCol
                width: side.width - (sb.wide ? 12 : 0)
                spacing: sb.wide ? 16 : 8
                Repeater {
                    model: sb.cats.length
                    delegate: Column {
                        id: grp
                        required property int index
                        readonly property var g: sb.cats[index] || ({})
                        width: catCol.width
                        spacing: 8
                        Row {
                            visible: sb.wide
                            spacing: 8
                            Icon { anchors.verticalCenter: parent.verticalCenter; name: grp.g.icon || "study"; size: 16; color: Theme.c.accentText }
                            Text { anchors.verticalCenter: parent.verticalCenter; text: sb.tr(grp.g.label || ""); color: Theme.c.textDim; font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family }
                        }
                        Flow {
                            width: parent.width
                            spacing: 6
                            Repeater {
                                model: (grp.g.items || []).length
                                delegate: CatChip {
                                    required property int index
                                    readonly property var it: grp.g.items[index] || ({})
                                    label: sb.tr(it.label || "")
                                    on: sb.st.cat === it.key
                                    onPicked: { sb.closeDeck(); sb.search(0, sb.sortKey, on ? "" : it.key) }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ---------- search, results and the page of a deck ----------
        Item {
            id: main
            x: sb.wide ? side.x + side.width + 16 : 20
            y: sb.wide ? side.y : side.y + side.height + 12
            width: parent.width - x - 20
            height: parent.height - y - 16

            Item {
                id: tools
                visible: !sb.deckOpen
                width: parent.width; height: 40
                TextField {
                    id: q
                    anchors { left: parent.left; right: sortBtn.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                    height: 38
                    placeholderText: sb.tr("Search shared decks")
                    placeholderTextColor: Theme.c.textDim
                    color: Theme.c.text
                    font.pixelSize: 15; font.family: Theme.type.family
                    selectByMouse: true
                    leftPadding: 38
                    background: Rectangle {
                        radius: Theme.layout.buttons === "capsule" ? height / 2 : Theme.shape.control
                        color: Theme.c.fill3
                        border.width: q.activeFocus ? 2 : (Theme.layout.buttons === "fluent" ? 1 : 0)
                        border.color: q.activeFocus ? Theme.c.accent : Theme.c.border
                    }
                    Icon { name: "search"; size: 16; color: Theme.c.textDim; anchors { left: parent.left; leftMargin: 13; verticalCenter: parent.verticalCenter } }
                    onTextEdited: typing.restart()
                    onAccepted: { typing.stop(); sb.search(0) }
                }
                FlatButton {
                    id: sortBtn
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    kind: "plain"; glyph: "list"
                    text: sb.width >= 640 ? sb.tr(sb.sortNames[sb.sortKey] || "Rating") : ""
                    ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: sb.tr("Order")
                    onClicked: sortMenu.openBelow(sortBtn, true)
                }
            }
            // what the list shows
            Row {
                id: statusRow
                visible: !sb.deckOpen
                anchors { top: tools.bottom; topMargin: 8 }
                width: parent.width
                height: 26
                spacing: 8
                BusyIndicator { visible: sb.st.loading === true; running: visible; width: 22; height: 22; anchors.verticalCenter: parent.verticalCenter }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 40
                    elide: Text.ElideRight
                    color: sb.st.error ? Theme.c.error : Theme.c.textDim
                    font.pixelSize: 13; font.family: Theme.type.family
                    text: sb.st.error ? sb.tr(sb.st.error)
                        : sb.st.loading === true ? sb.tr("Searching AnkiWeb…")
                        : sb.st.searched === true ? ((sb.st.catLabel ? sb.tr(sb.st.catLabel) + "  ·  " : "") + sb.shortNum(sb.st.total) + " " + sb.tr("decks")
                                                     + (sb.st.tooMany === true ? "  ·  " + sb.tr("Too many matches found. Please refine your search.") : ""))
                        : ""
                }
            }

            // ---- the front page: no search yet ----
            StudySurface {
                visible: !sb.deckOpen && sb.st.searched !== true && sb.st.loading !== true
                anchors { top: statusRow.bottom; topMargin: 6 }
                width: parent.width
                height: intro.implicitHeight + 56
                Column {
                    id: intro
                    anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: 28 }
                    spacing: 12
                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 64; height: 64; radius: 32
                        color: Qt.rgba(Theme.c.accent.r, Theme.c.accent.g, Theme.c.accent.b, 0.15)
                        Icon { anchors.centerIn: parent; name: "study"; size: 32; color: Theme.c.accentText }
                    }
                    Lbl { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; bold: true; size: 20; text: sb.tr("Find a deck made by the Anki community") }
                    Lbl {
                        width: Math.min(parent.width, 520); anchors.horizontalCenter: parent.horizontalCenter
                        horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; dim: true; size: 14
                        text: sb.tr("Pick a category or search by name. Open a deck to see its cards and reviews, then add it to your decks in one click.")
                    }
                    Lbl {
                        width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; dim: true; size: 12
                        text: sb.tr("Can't find what you're looking for? Try a search!")
                    }
                }
            }

            // ---- the results ----
            // A ListView, so only the cards on screen exist (each one is a StudySurface: glass and blur in the Apple
            // theme) and the ones that scroll out are reused: a page of a hundred decks stays smooth.
            ListView {
                id: resFl
                visible: !sb.deckOpen && sb.results.length > 0
                anchors { top: statusRow.bottom; topMargin: 6; bottom: parent.bottom }
                width: parent.width
                clip: true
                spacing: 8
                reuseItems: true
                cacheBuffer: 400
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                model: sb.results.length
                delegate: StudySurface {
                    id: card
                    required property int index
                    readonly property var r: sb.results[index] || ({})
                    readonly property bool adding: sb.busy && sb.dl.id === r.id
                    width: resFl.width - 12
                    height: Math.max(76, cardCol.implicitHeight + 24)
                    hovered: cma.containsMouse
                    MouseArea { id: cma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sb.openDeck(card.r.id) }
                    Column {
                        id: cardCol
                        anchors { left: parent.left; leftMargin: 16; right: addBtn.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
                        spacing: 6
                        Text {
                            width: parent.width
                            wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
                            text: card.r.title || ""
                            color: Theme.c.text; font.pixelSize: 15; font.weight: Font.DemiBold; font.family: Theme.type.family
                        }
                        Flow {
                            width: parent.width
                            spacing: 14
                            LikeBar { up: card.r.up || 0; down: card.r.down || 0 }
                            Badge { glyph: "list"; label: sb.shortNum(card.r.notes) + " " + sb.tr("notes") }
                            Badge { glyph: "audio"; label: (card.r.audio || 0) > 0 ? sb.shortNum(card.r.audio) : "" }
                            Badge { glyph: "image"; label: (card.r.images || 0) > 0 ? sb.shortNum(card.r.images) : "" }
                            Badge { glyph: "calendar"; label: sb.day(card.r.updated) }
                        }
                        DlBar { width: parent.width; shown: card.adding }
                    }
                    FlatButton {
                        id: addBtn
                        anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                        kind: "flatround"; glyph: "plus"; iconSize: 20
                        implicitWidth: 40; implicitHeight: 40
                        enabled: !sb.busy && sb.st.importing !== true
                        ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: sb.tr("Add to my decks")
                        onClicked: sb.add(card.r.id)
                    }
                }
                // pages
                footer: Item {
                    width: resFl.width - 12
                    height: pageRow.visible ? pageRow.implicitHeight + 16 : 12
                    Row {
                        id: pageRow
                        y: 8
                        visible: (sb.st.pages || 1) > 1
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: 10
                        FlatButton { kind: "plain"; glyph: "left"; enabled: (sb.st.page || 0) > 0; onClicked: { sb.search((sb.st.page || 0) - 1); resFl.contentY = 0 } }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: sb.tr("Page") + " " + ((sb.st.page || 0) + 1) + " / " + (sb.st.pages || 1)
                            color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family
                        }
                        FlatButton { kind: "plain"; glyph: "right"; enabled: (sb.st.page || 0) + 1 < (sb.st.pages || 1); onClicked: { sb.search((sb.st.page || 0) + 1); resFl.contentY = 0 } }
                    }
                }
            }
            Text {
                visible: !sb.deckOpen && sb.st.searched === true && sb.results.length === 0 && sb.st.loading !== true && !sb.st.error
                anchors { top: statusRow.bottom; topMargin: 30 }
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: sb.tr("No matches were found.")
                color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family
            }

            // ---- the page of a deck ----
            Flickable {
                id: detailFl
                visible: sb.deckOpen
                anchors.fill: parent
                contentHeight: det.implicitHeight + 24
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                Column {
                    id: det
                    width: detailFl.width - 12
                    spacing: 14
                    readonly property bool adding: sb.busy && sb.dl.id === sb.deck.id

                    Row {
                        visible: sb.deck.loading === true
                        spacing: 10
                        BusyIndicator { running: parent.visible; width: 24; height: 24 }
                        Text { anchors.verticalCenter: parent.verticalCenter; text: sb.tr("Opening the deck…"); color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family }
                    }
                    Text {
                        visible: !!sb.deck.error
                        width: parent.width; wrapMode: Text.Wrap
                        text: sb.tr(sb.deck.error || "")
                        color: Theme.c.error; font.pixelSize: 14; font.family: Theme.type.family
                    }

                    // title, numbers and the big button
                    StudySurface {
                        visible: !!sb.deck.title
                        raised: true
                        width: parent.width
                        height: topCol.implicitHeight + 40
                        Column {
                            id: topCol
                            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
                            spacing: 12
                            Text {
                                width: parent.width; wrapMode: Text.Wrap
                                text: sb.deck.title || ""
                                color: Theme.c.text; font.pixelSize: 22; font.weight: Font.Bold; font.family: Theme.type.display || Theme.type.family
                            }
                            Flow {
                                width: parent.width
                                spacing: 16
                                LikeBar { up: sb.deck.up || 0; down: sb.deck.down || 0 }
                                Badge { glyph: "list"; label: sb.shortNum(sb.deck.notes) + " " + sb.tr("notes") }
                                Badge { glyph: "audio"; label: (sb.deck.audio || 0) > 0 ? sb.shortNum(sb.deck.audio) + " " + sb.tr("sounds") : "" }
                                Badge { glyph: "image"; label: (sb.deck.images || 0) > 0 ? sb.shortNum(sb.deck.images) + " " + sb.tr("images") : "" }
                                Badge { glyph: "download"; label: sb.deck.size || "" }
                                Badge { glyph: "calendar"; label: sb.deck.updated ? sb.tr("Updated") + " " + sb.day(sb.deck.updated) : "" }
                            }
                            Flow {
                                width: parent.width
                                spacing: 8
                                FlatButton {
                                    kind: "suggested"; pill: true; glyph: "plus"
                                    implicitHeight: 44
                                    text: det.adding ? sb.dlText() : sb.tr("Add to my decks")
                                    enabled: !sb.busy && sb.st.importing !== true
                                    onClicked: sb.add(sb.deck.id)
                                }
                                FlatButton {
                                    kind: "plain"; glyph: "forward"
                                    implicitHeight: 44
                                    text: sb.tr("Open on AnkiWeb")
                                    onClicked: Qt.openUrlExternally(sb.deck.url)
                                }
                            }
                            DlBar { width: Math.min(parent.width, 420); shown: det.adding }
                            Flow {
                                visible: (sb.deck.tags || []).length > 0
                                width: parent.width
                                spacing: 6
                                Repeater {
                                    model: (sb.deck.tags || []).length
                                    delegate: Chip {
                                        required property int index
                                        text: sb.deck.tags[index]
                                        implicitHeight: 26
                                        onClicked: { q.text = sb.deck.tags[index]; sb.closeDeck(); sb.search(0, sb.sortKey, "") }
                                    }
                                }
                            }
                        }
                    }

                    // the description (the site's HTML, cleaned by the core)
                    StudySurface {
                        visible: !!sb.deck.html
                        width: parent.width
                        height: descText.implicitHeight + 64
                        Text {
                            x: 20; y: 16
                            text: sb.tr("Description")
                            color: Theme.c.textDim; font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family
                        }
                        Text {
                            id: descText
                            x: 20; y: 42
                            width: parent.width - 40
                            wrapMode: Text.Wrap
                            textFormat: Text.RichText
                            text: sb.deck.html || ""
                            color: Theme.c.text
                            linkColor: Theme.c.accentText
                            font.pixelSize: 14; font.family: Theme.type.family
                            onLinkActivated: function(link) { Qt.openUrlExternally(link) }
                            HoverHandler { cursorShape: descText.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
                        }
                    }

                    // sample cards
                    Text {
                        visible: (sb.deck.samples || []).length > 0
                        text: sb.tr("Sample cards")
                        color: Theme.c.text; font.pixelSize: 16; font.weight: Font.DemiBold; font.family: Theme.type.family
                    }
                    Flow {
                        visible: (sb.deck.samples || []).length > 0
                        width: parent.width
                        spacing: 10
                        Repeater {
                            model: (sb.deck.samples || []).length
                            delegate: StudySurface {
                                id: smp
                                required property int index
                                readonly property var s: sb.deck.samples[index] || ({})
                                width: sb.wide ? (det.width - 20) / 3 : det.width
                                height: smpCol.implicitHeight + 28
                                Column {
                                    id: smpCol
                                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 14 }
                                    spacing: 8
                                    Repeater {
                                        model: (smp.s.images || []).length
                                        delegate: Image {
                                            required property int index
                                            width: smpCol.width
                                            height: status === Image.Ready ? Math.min(160, implicitHeight * width / Math.max(1, implicitWidth)) : (status === Image.Error ? 0 : 100)
                                            visible: status !== Image.Error
                                            // through the app's picture cache (image://web), like the posters
                                            source: "image://web/" + encodeURIComponent(smp.s.images[index])
                                            asynchronous: true
                                            fillMode: Image.PreserveAspectFit
                                        }
                                    }
                                    Repeater {
                                        model: (smp.s.fields || []).length
                                        delegate: Column {
                                            required property int index
                                            readonly property var f: smp.s.fields[index]
                                            width: smpCol.width
                                            spacing: 1
                                            Text { width: parent.width; elide: Text.ElideRight; text: parent.f.name; color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family }
                                            Text { width: parent.width; wrapMode: Text.Wrap; maximumLineCount: 4; elide: Text.ElideRight; text: parent.f.text; color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family }
                                        }
                                    }
                                    Row {
                                        spacing: 4
                                        Repeater {
                                            model: (smp.s.sounds || []).length
                                            delegate: FlatButton {
                                                required property int index
                                                kind: "flatround"; glyph: "volume"; iconSize: 16
                                                implicitWidth: 34; implicitHeight: 34
                                                ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: sb.tr("Play the sound")
                                                onClicked: Core.cmd({cmd: "shared_play", url: smp.s.sounds[index]})
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // reviews
                    Text {
                        visible: (sb.deck.reviews || []).length > 0
                        text: sb.tr("Reviews")
                        color: Theme.c.text; font.pixelSize: 16; font.weight: Font.DemiBold; font.family: Theme.type.family
                    }
                    StudySurface {
                        visible: (sb.deck.reviews || []).length > 0
                        width: parent.width
                        height: revCol.implicitHeight + 16
                        Column {
                            id: revCol
                            y: 8
                            width: parent.width
                            Repeater {
                                model: (sb.deck.reviews || []).length
                                delegate: Item {
                                    id: rv
                                    required property int index
                                    readonly property var r: sb.deck.reviews[index]
                                    width: revCol.width
                                    height: rvCol.implicitHeight + 20
                                    Rectangle { visible: rv.index > 0; x: 16; width: parent.width - 32; height: 1; color: Theme.c.divider }
                                    Rectangle {
                                        id: thumb
                                        x: 16; y: 12
                                        width: 28; height: 28; radius: 14
                                        color: rv.r.up ? Qt.rgba(Theme.c.success.r, Theme.c.success.g, Theme.c.success.b, 0.16) : Qt.rgba(Theme.c.error.r, Theme.c.error.g, Theme.c.error.b, 0.16)
                                        Icon { anchors.centerIn: parent; name: rv.r.up ? "check" : "close"; size: 16; color: rv.r.up ? Theme.c.success : Theme.c.error }
                                    }
                                    Column {
                                        id: rvCol
                                        anchors { left: thumb.right; leftMargin: 12; right: parent.right; rightMargin: 16 }
                                        y: 12
                                        spacing: 4
                                        Text { text: rv.r.date || ""; color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family }
                                        Text { width: parent.width; wrapMode: Text.Wrap; text: rv.r.body; color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family }
                                        Text {
                                            visible: !!rv.r.reply
                                            width: parent.width; wrapMode: Text.Wrap
                                            text: "↳ " + (rv.r.reply || "")
                                            color: Theme.c.textDim; font.pixelSize: 13; font.italic: true; font.family: Theme.type.family
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
