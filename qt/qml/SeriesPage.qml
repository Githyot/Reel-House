import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import Reelhouse 1.0

// The page of a series or a book: cover, title, Resume, star and menu, tags, description, facts and the chapters.
Rectangle {
    id: sp
    property string kind: "manga"
    readonly property bool isManga: kind === "manga"
    color: Theme.c.window
    readonly property var st: isManga ? Core.manga : (Core.manga.novels || ({}))
    readonly property var v: st.view || ({})
    readonly property var s: v.series || ({})
    readonly property var chapters: v.chapters || []
    readonly property var allCats: st.categories || []
    property bool descOpen: false
    property var menuChapter: ({})
    function tr(t) { return Core.tr(Core.uiLang, t) }
    function cmd(n) { return kind + "_" + n }
    readonly property var nextChapter: {
        for (let i = 0; i < chapters.length; ++i) if (chapters[i].id === v.next) return chapters[i]
        return null
    }
    // the first part of the title of a chapter: "Ch. 5 · Name"
    function chapterTitle(c) {
        if (c.label) return c.label
        return (c.number ? tr("Chapter") + " " + c.number : "") + (c.title && c.number ? " · " : "") + (c.title || "")
    }

    // The manga server may still be starting when the page opens: while it cannot be reached, ask again by itself
    // (every 3 s, up to 20 times) instead of leaving the error on the screen.
    property int retries: 0
    readonly property bool unreachable: isManga && !v.loading && !!v.error && String(v.error).indexOf("Could not reach") === 0
    onUnreachableChanged: if (!unreachable && !v.error) retries = 0
    readonly property string sid: s.id || ""
    onSidChanged: retries = 0
    Timer {
        interval: 3000
        running: sp.visible && sp.unreachable && sp.retries < 20
        repeat: false
        onTriggered: { sp.retries++; Core.cmd({cmd: "manga_open", series: sp.s}) }
    }

    TitlePrompt {
        id: addPrompt
        heading: sp.tr("New chapter")
        actionText: sp.tr("Add")
        onAccepted: function(t) { Core.cmd({cmd: "novel_add_chapter", title: t}) }
    }
    PopMenu {
        id: moreMenu
        items: {
            const it = [{key: "read", label: sp.tr("Mark all as read")}, {key: "unread", label: sp.tr("Mark all as not read")}, {key: "cover", label: sp.tr("Change the cover…")}]
            if ((sp.s.cover || "").indexOf("/covers/") >= 0) it.push({key: "uncover", label: sp.tr("Use the cover of the source")})
            if (sp.s.inLibrary) sp.allCats.forEach(function(c) { it.push({key: "cat", cat: c, checked: (sp.s.categories || []).indexOf(c) >= 0, label: c}) })
            return it
        }
        onPicked: function(i) {
            const it = items[i]
            if (it.key === "read") Core.cmd({cmd: sp.cmd("mark_all"), read: true})
            else if (it.key === "unread") Core.cmd({cmd: sp.cmd("mark_all"), read: false})
            else if (it.key === "cover") sp.coverRequested(sp.s.id)
            else if (it.key === "uncover") Core.cmd({cmd: sp.cmd("set_cover"), id: sp.s.id, file: ""})
            else {
                const now = (sp.s.categories || []).filter(function(c) { return c !== it.cat })
                if (!it.checked) now.push(it.cat)
                Core.cmd({cmd: sp.cmd("set_categories"), id: sp.s.id, cats: now})
            }
        }
    }
    signal coverRequested(string id)
    PopMenu {
        id: chMenu
        items: [{key: "mark", label: sp.menuChapter.read ? sp.tr("Mark as not read") : sp.tr("Mark as read")}, {key: "before", label: sp.tr("Mark everything before as read")}]
        onPicked: function(i) {
            if (items[i].key === "mark") Core.cmd({cmd: sp.cmd("mark"), series: sp.s.id, id: sp.menuChapter.id, read: !sp.menuChapter.read})
            else Core.cmd({cmd: sp.cmd("mark_before"), id: sp.menuChapter.id})
        }
    }

    // The cover, large and softened, behind the top of the page: it covers the full width (cropped, never
    // stretched), drifts up while the page scrolls and fades into the window colour.
    Item {
        id: coverWall
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: Math.min(parent.height, 520)
        clip: true
        visible: (sp.s.cover || "") !== ""
        PosterImage {
            // only the picture is faded: the fade to the window colour below must stay solid, or the band ends in a line
            opacity: Theme.scheme === "light" ? 0.45 : 0.6
            x: -48
            y: -48 - Math.min(90, Math.max(0, fl.contentY) * 0.3)
            width: coverWall.width + 96
            height: coverWall.height + 186
            verticalAlignment: Image.AlignVCenter
            url: sp.s.cover || ""
            layer.enabled: coverWall.visible
            layer.effect: MultiEffect { blurEnabled: true; blur: 1.0; blurMax: 64 }
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.0; color: Theme.c.window45 }
                GradientStop { position: 0.55; color: Theme.c.window65 }
                GradientStop { position: 0.85; color: Theme.c.window95 }
                GradientStop { position: 1.0; color: Theme.c.window }
            }
        }
    }

    // a phone with Material: the main action (Resume / Start reading) is the FAB at the bottom right
    Fab {
        id: readFab
        visible: wanted && !!sp.nextChapter
        anchors { right: parent.right; bottom: parent.bottom; margins: 16 }
        glyph: "play"
        label: v.started ? (sp.isManga ? sp.tr("Resume") : sp.tr("Continue")) : sp.tr("Start reading")
        onClicked: Core.cmd({cmd: sp.cmd("read"), id: sp.nextChapter.id})
    }
    Flickable {
        id: fl
        anchors.fill: parent
        contentWidth: width
        contentHeight: col.height + 48 + (readFab.visible ? readFab.height + 16 : 0)
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        Column {
            id: col
            x: Math.max(14, (fl.width - 780) / 2)
            // no bar above the page: the back button floats at the top left and the window buttons at the top right;
            // when the column reaches under them it starts below them
            y: x < 64 || fl.width - x - width < 130 ? 54 : 12
            width: Math.min(780, fl.width - 28)
            spacing: 16

            // cover and the main actions
            Row {
                width: parent.width
                spacing: 18
                RoundedPicture { id: cov; width: Theme.mobile ? 110 : 150; height: Theme.mobile ? 158 : 215; url: sp.s.cover || "" }
                Column {
                    width: parent.width - cov.width - 18
                    anchors.verticalCenter: cov.verticalCenter
                    spacing: 6
                    Lbl { width: parent.width; text: sp.s.title || ""; wrapMode: Text.Wrap; bold: true; size: 24 }
                    Lbl { width: parent.width; visible: (sp.s.authors || []).length > 0; text: (sp.s.authors || []).join(", "); dim: true; bold: true; size: 15; wrapMode: Text.Wrap }
                    // the status ("Ongoing") and, beside it, a button to choose another cover for this title
                    Row {
                        width: parent.width
                        spacing: 6
                        Lbl { id: statusLbl; anchors.verticalCenter: parent.verticalCenter; width: Math.min(implicitWidth, parent.width - 40); text: [sp.s.status || "", v.sourceName || ""].filter(function(x) { return x !== "" }).join(" · "); visible: text !== ""; dim: true; size: 14; elide: Text.ElideRight }
                        FlatButton {
                            anchors.verticalCenter: parent.verticalCenter
                            kind: "flat"; glyph: "image"; iconSize: 16; implicitWidth: 30; implicitHeight: 30
                            ToolTip.visible: hovered; ToolTip.text: sp.tr("Change the cover…")
                            onClicked: sp.coverRequested(sp.s.id)
                        }
                    }
                    Row {
                        topPadding: 10
                        spacing: 8
                        FlatButton {
                            visible: !!sp.nextChapter && !readFab.visible
                            kind: "suggested"
                            pill: true
                            glyph: "play"
                            text: v.started ? (sp.isManga ? sp.tr("Resume") : sp.tr("Continue")) : sp.tr("Start reading")
                            onClicked: Core.cmd({cmd: sp.cmd("read"), id: sp.nextChapter.id})
                        }
                        FlatButton {
                            kind: "flatround"
                            glyph: "star"
                            ToolTip.visible: hovered
                            ToolTip.text: sp.s.inLibrary ? sp.tr("Remove from the library") : sp.tr("Add to the library")
                            // On: a solid yellow star. Off: a hollow star in the text colour.
                            contentItem: Item { Icon { anchors.centerIn: parent; name: sp.s.inLibrary ? "star" : "staroutline"; filled: sp.s.inLibrary === true; size: 20; color: sp.s.inLibrary ? Theme.c.starEdge : Theme.c.text; fillColor: Theme.c.star } }
                            onClicked: Core.cmd({cmd: sp.cmd("toggle_library"), series: sp.s})
                        }
                        FlatButton {
                            id: moreBtn
                            kind: "flatround"
                            glyph: "more"
                            focusPolicy: Qt.NoFocus
                            ToolTip.visible: hovered && !moreMenu.visible
                            ToolTip.text: sp.tr("More")
                            onClicked: moreMenu.openBelow(moreBtn)
                        }
                    }
                }
            }

            // tags: the source first
            Flow {
                width: parent.width
                spacing: 6
                visible: (sp.s.tags || []).length > 0 || !!v.sourceName
                Rectangle {
                    visible: !!v.sourceName
                    height: 28; width: srcRow.implicitWidth + 18; radius: 14
                    color: Theme.c.chip
                    Row {
                        id: srcRow
                        anchors.centerIn: parent
                        spacing: 6
                        SourceIcon { size: 16; url: v.sourceIcon || ""; name: v.sourceName || ""; anchors.verticalCenter: parent.verticalCenter }
                        Lbl { text: v.sourceName || ""; size: 12; bold: true; anchors.verticalCenter: parent.verticalCenter }
                    }
                }
                Repeater {
                    model: (sp.s.tags || []).slice(0, 14)
                    delegate: Rectangle {
                        required property var modelData
                        height: 28; width: tg.implicitWidth + 20; radius: 14
                        color: Theme.c.chip
                        Lbl { id: tg; anchors.centerIn: parent; text: modelData; size: 12 }
                    }
                }
            }

            Lbl {
                id: desc
                width: parent.width
                visible: !!sp.s.description
                text: sp.s.description || ""
                wrapMode: Text.Wrap
                maximumLineCount: sp.descOpen ? 1000 : 5
                elide: Text.ElideRight
                size: 15
                lineHeight: 1.2
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: sp.descOpen = !sp.descOpen }
            }

            // facts
            Column {
                width: parent.width
                spacing: 10
                Row {
                    visible: !!sp.s.inLibrary && (sp.s.categories || []).length > 0
                    width: parent.width
                    spacing: 12
                    Icon { name: "bookmark"; size: 20; anchors.verticalCenter: parent.verticalCenter }
                    Lbl { text: sp.tr("Categories"); bold: true; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 32 - catRow.width; elide: Text.ElideRight }
                    Row {
                        id: catRow
                        spacing: 6
                        anchors.verticalCenter: parent.verticalCenter
                        Repeater { model: sp.s.categories || []; delegate: Rectangle { required property var modelData; height: 26; width: cl.implicitWidth + 18; radius: 13; color: Theme.c.chip; Lbl { id: cl; anchors.centerIn: parent; text: modelData; size: 12 } } }
                    }
                }
                Row {
                    visible: !v.loading && !!v.lastUpdate
                    width: parent.width
                    spacing: 12
                    Icon { name: "calendar"; size: 20; anchors.verticalCenter: parent.verticalCenter }
                    Lbl { text: sp.tr("Last update"); bold: true; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 32 - lu.implicitWidth; elide: Text.ElideRight }
                    Lbl { id: lu; text: v.lastUpdate || ""; dim: true; anchors.verticalCenter: parent.verticalCenter }
                }
            }

            // chapters
            Item {
                width: parent.width
                height: 34
                Lbl {
                    anchors.verticalCenter: parent.verticalCenter
                    text: v.loading ? sp.tr("Chapters") : (sp.chapters.length + " " + sp.tr("chapters") + " · " + (v.unread || 0) + " " + sp.tr("not read"))
                    bold: true
                    size: 15
                }
                FlatButton { visible: !sp.isManga && !!v.own; anchors.right: parent.right; kind: "plain"; implicitHeight: 32; glyph: "plus"; text: sp.tr("Add chapter"); onClicked: addPrompt.open() }
            }
            BusyIndicator { visible: !!v.loading; running: visible; anchors.horizontalCenter: parent.horizontalCenter; width: 36; height: 36 }
            Lbl { visible: !!v.error; width: parent.width; text: Core.tr(Core.uiLang, v.error || ""); color: Theme.c.error; wrapMode: Text.Wrap }
            Lbl { visible: sp.unreachable && sp.retries < 20; width: parent.width; dim: true; text: sp.tr("Trying again…") }
            StatusPage {
                visible: !v.loading && !v.error && sp.chapters.length === 0
                anchors.horizontalCenter: parent.horizontalCenter
                icon: "list"
                title: sp.tr("No chapters")
                description: sp.tr("There are none in this language. Change the language in Preferences.")
            }
            // MangaDex lists licensed English chapters only as links to another site, which cannot be read here: the
            // other languages often have them (HP: Slime with 0 chapters)
            FlatButton {
                visible: sp.isManga && !v.loading && !v.error && sp.chapters.length === 0 && ((Core.prefs.general || {}).manga_lang || "en") !== "all"
                anchors.horizontalCenter: parent.horizontalCenter
                kind: "suggested"
                glyph: "translate"
                text: sp.tr("Show chapters in every language")
                onClicked: { Core.cmd({cmd: "set_setting", path: "general.manga_lang", value: "all"}); Core.cmd({cmd: "manga_open", series: sp.s}) }
            }
            // chapters, newest first: a window of rows (ListView) that slides along inside its place in the
            // column while the page scrolls, so a series of a thousand chapters builds only the rows on screen and reuses
            // them; the number is the model, so the rows stay when only their data changes
            Rectangle {
                id: chBox
                visible: sp.chapters.length > 0
                width: parent.width
                radius: Theme.shape.card
                color: Theme.c.card
                border.width: 1
                border.color: Theme.c.border
                clip: true
                height: sp.chapters.length * 64
                ListView {
                    id: chList
                    width: parent.width
                    height: Math.min(chBox.height, fl.height)
                    interactive: false
                    reuseItems: true
                    cacheBuffer: 128
                    model: sp.chapters.length
                    // where the box is inside the scrolled content; the window follows the part of it on screen
                    readonly property real docY: col.y + chBox.y
                    function sync() {
                        const off = Math.max(0, Math.min(fl.contentY - docY, chBox.height - height))
                        y = off
                        contentY = off
                    }
                    onDocYChanged: sync()
                    onHeightChanged: sync()
                    onCountChanged: sync()
                    Component.onCompleted: sync()
                    Connections { target: fl; function onContentYChanged() { chList.sync() } }
                    delegate: Item {
                        required property int index
                        readonly property var ch: sp.chapters[sp.chapters.length - 1 - index] || ({})
                        width: chList.width
                        height: 64
                        Rectangle { anchors.fill: parent; color: cm.containsMouse ? Theme.c.hover : (ch.current ? Theme.c.selected : "transparent") }
                        Rectangle { visible: index > 0; anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Theme.c.divider }
                        MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Core.cmd({cmd: sp.cmd("read"), id: ch.id}) }
                        Column {
                            anchors { left: parent.left; leftMargin: 16; right: sfx.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                            opacity: ch.read ? 0.5 : 1
                            Lbl { width: parent.width; elide: Text.ElideRight; text: sp.chapterTitle(ch); bold: true; size: Theme.mobile ? 16 : 15 }
                            Lbl { width: parent.width; elide: Text.ElideRight; visible: text !== ""; text: [ch.group || "", (ch.published || "").substring(0, 10)].filter(function(x) { return x !== "" }).join(" · "); dim: true; size: 13 }
                        }
                        Row {
                            id: sfx
                            anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                            spacing: 8
                            Lbl { visible: !!ch.current && ch.pages > 0; anchors.verticalCenter: parent.verticalCenter; text: ch.page + "/" + ch.pages; dim: true; size: 13 }
                            Rectangle {
                                visible: !!ch.new
                                anchors.verticalCenter: parent.verticalCenter
                                height: 22; width: nl.implicitWidth + 16; radius: 11
                                color: Theme.c.errorBg
                                Lbl { id: nl; anchors.centerIn: parent; text: sp.tr("New"); color: Theme.c.errorFg; bold: true; size: 12 }
                            }
                            FlatButton {
                                id: rowMore
                                kind: "flatround"
                                glyph: "more"
                                focusPolicy: Qt.NoFocus
                                anchors.verticalCenter: parent.verticalCenter
                                onClicked: { sp.menuChapter = ch; chMenu.openBelow(rowMore, true) }
                            }
                        }
                    }
                }
            }
        }
    }
}
