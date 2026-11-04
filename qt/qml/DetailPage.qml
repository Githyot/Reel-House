import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import Reelhouse 1.0

Item {
    id: page
    readonly property var d: Core.detail
    readonly property var meta: d.meta !== undefined ? d.meta : ({})
    readonly property bool isSeries: meta.kind === "series" || meta.kind === "anime" || (meta.videos !== undefined && meta.videos.length > 0)
    readonly property var streams: d.streams !== undefined ? d.streams : null
    readonly property var episodes: {
        var v = meta.videos !== undefined ? meta.videos : []
        var out = []
        for (var i = 0; i < v.length; ++i) if (d.season === undefined || v[i].season === d.season) out.push(v[i])
        return out
    }
    function pad2(v) { v = v || 0; return v < 10 ? "0" + v : "" + v }
    function nextEpisode() {
        var v = meta.videos !== undefined ? meta.videos : []
        for (var i = 0; i < v.length; ++i) if (!v[i].watched && v[i].season !== 0) return v[i]
        return v.length > 0 ? v[0] : null
    }

    // ---- wallpaper ----
    // The background art covers the whole page (cropped, never stretched, anchored to the top so faces and logos
    // stay in view), drifts up slowly while the information scrolls (parallax), and a scrim in the window colour
    // keeps the text readable: darker behind the information column, fading into the window toward the bottom.
    readonly property var gen: Core.prefs.general !== undefined ? Core.prefs.general : ({})
    readonly property bool lightScheme: Theme.scheme === "light"
    readonly property real parallaxMax: 140
    readonly property real scrolled: Math.max(0, infoScroll.contentY)
    Item {
        id: wall
        anchors.fill: parent
        clip: true

        // Moving background (GTK aurora.rs) with colours taken from the art: seen while the picture loads, or when
        // the title has none.
        Aurora {
            id: aurora
            x: 0
            y: 0
            width: wall.width
            height: Math.min(wall.height, 430)
            enabledLook: page.gen.animated_bg !== false
            motion: page.gen.aurora_motion === true
        }
        PosterImage {
            id: wallArt
            x: 0
            y: -Math.min(page.parallaxMax, page.scrolled * 0.3)
            width: wall.width
            height: wall.height + page.parallaxMax
            verticalAlignment: Image.AlignTop
            horizontalAlignment: Image.AlignHCenter
            url: page.meta.background ? page.meta.background : ""
            // decoded at the drawn size (x the screen's pixel ratio), not the file's (often 3840 px)
            sourceSize.width: width > 1 ? Math.ceil(width * Math.max(1, Screen.devicePixelRatio) / 64) * 64 : 0
            sourceSize.height: height > 1 ? Math.ceil(height * Math.max(1, Screen.devicePixelRatio) / 64) * 64 : 0
        }
        // behind the information column (left) the scrim is thicker than behind the streams
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: page.lightScheme ? Theme.c.window95 : Theme.c.window65 }
                GradientStop { position: 0.45; color: page.lightScheme ? Theme.c.window86 : Theme.c.window45 }
                GradientStop { position: 1.0; color: page.lightScheme ? Theme.c.window65 : Theme.c.window0 }
            }
        }
        // the art fades into the window toward the bottom of the page
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.0; color: Theme.c.window0 }
                GradientStop { position: 0.45; color: page.lightScheme ? Theme.c.window45 : Theme.c.window0 }
                GradientStop { position: 0.8; color: Theme.c.window86 }
                GradientStop { position: 1.0; color: Theme.c.window }
            }
        }
        // the further the information is scrolled, the quieter the art
        Rectangle {
            anchors.fill: parent
            color: Theme.c.window
            opacity: Math.min(0.55, page.scrolled / 900)
        }
    }

    // The picture is read once it is drawn (a short wait for its fade-in), at 40x60, and the core picks the colours.
    PosterImage {
        id: moodSrc
        x: 0
        y: 0
        width: 40
        height: 60
        sourceSize.width: 80
        sourceSize.height: 120
        z: -1
        url: page.meta.background ? page.meta.background : ""
        onStatusChanged: if (status === Image.Ready) moodTimer.restart()
    }
    Timer {
        id: moodTimer
        interval: Theme.motion.normal + 60
        onTriggered: Core.sampleMood(moodSrc, "aurora")
    }
    Connections {
        target: Core
        function onMoodReady(tag, colours) { if (tag === "aurora") aurora.colours = colours }
    }

    // ---- information column ----
    // There is no bar above this page (Main: chromeFloats): the back and cover buttons float at the top left, the
    // window buttons (GNOME, Windows, Android) at the top right. The panels leave room for them and use the rest of
    // the height.
    readonly property bool winButtons: Theme.layout.windowControls === "native-right"
    // a phone: the information above the streams, one column each, under the floating back button
    // stacked on a phone held upright; a tablet or a phone on its side (Waydroid's window) keeps the information
    // and the streams side by side, so the episode list is not pushed below the fold
    readonly property bool stacked: Theme.mobile && page.width < page.height * 1.2
    readonly property real safeTop: Theme.mobile && Theme.safe !== undefined && Theme.safe.top !== undefined ? Theme.safe.top : 0
    Rectangle {
        id: infoPanel
        x: Theme.mobile ? 12 : Math.max(64, page.width * 0.078)
        y: Theme.mobile ? 56 + page.safeTop : 14
        width: page.stacked ? page.width - 24 : Math.min(page.width * 0.5, page.width - streamsPanel.width - x - (Theme.mobile ? 24 : 32))
        height: page.stacked ? Math.round((page.height - y - 12) * 0.56) : page.height - y - (Theme.mobile ? 12 : 10)
        radius: Theme.shape.large
        color: Theme.c.panelInfo
        border.width: 1
        border.color: Theme.c.glassBorder
        clip: true

        Flickable {
            id: infoScroll
            anchors.fill: parent
            anchors.margins: 1
            contentWidth: width
            contentHeight: info.implicitHeight + 40
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }

            Column {
                id: info
                x: Theme.mobile ? 14 : 20
                y: 16
                width: infoScroll.width - (Theme.mobile ? 28 : 40)
                spacing: 14

                PosterImage {
                    visible: !!page.meta.logo
                    width: Math.min(320, parent.width)
                    height: 100
                    fillMode: Image.PreserveAspectFit
                    horizontalAlignment: Image.AlignLeft
                    url: page.meta.logo ? page.meta.logo : ""
                }
                Text {
                    visible: !page.meta.logo
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: page.meta.name ? page.meta.name : ""
                    color: Theme.c.text
                    font.pixelSize: Theme.mobile ? 28 : 40
                    font.weight: Font.ExtraBold
                    font.family: Theme.type.family
                }
                Row {
                    spacing: 10
                    Row {
                        visible: !!page.meta.rating
                        spacing: 4
                        anchors.verticalCenter: parent.verticalCenter
                        Icon { name: "star"; filled: true; size: 18; color: Theme.c.starEdge; fillColor: Theme.c.star; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: page.meta.rating ? page.meta.rating : ""; color: Theme.c.rating; font.pixelSize: 17; font.weight: Font.ExtraBold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                    }
                    Chip { visible: !!page.meta.releaseInfo; text: page.meta.releaseInfo ? page.meta.releaseInfo : "" }
                    Chip { visible: !!page.meta.runtime; text: page.meta.runtime ? page.meta.runtime : "" }
                }
                // Categories: a click opens the page with more titles of that category (GTK malinfo::genre_chips).
                Flow {
                    width: parent.width
                    spacing: 6
                    visible: page.d.genreChips !== undefined && page.d.genreChips.length > 0
                    Repeater {
                        model: page.d.genreChips !== undefined ? page.d.genreChips : []
                        delegate: GenreChip {
                            required property var modelData
                            text: modelData.label
                            ToolTip.visible: hov.hovered
                            ToolTip.text: (modelData.kind === "anime" ? Core.tr(Core.uiLang, "More anime in") : Core.tr(Core.uiLang, "More in")) + " " + modelData.name
                            HoverHandler { id: hov }
                            onClicked: win.openGenre(modelData.id, modelData.name, modelData.kind)
                        }
                    }
                }
                Flow {
                    width: parent.width
                    spacing: 10
                    FlatButton {
                        kind: "suggested"
                        glyph: "play"
                        // Started before (watchProgress > 0): "Continue" from where it stopped.
                        readonly property var prog: page.d.progress || null
                        readonly property bool resumes: !!prog && prog.position > 0
                        text: resumes ? Core.tr(Core.uiLang, "Continue") + (prog.label ? "  ·  " + prog.label : "") : Core.tr(Core.uiLang, "Play best match")
                        faded: resumes || page.isSeries ? false : !(page.streams && page.streams.list && page.streams.list.length > 0)
                        implicitHeight: 44
                        onClicked: {
                            if (resumes) {
                                Core.cmd({cmd: "resume", index: prog.index})
                            } else if (page.isSeries) {
                                var e = page.nextEpisode()
                                if (e) Core.cmd({cmd: "play_best", kind: page.meta.kind, id: page.meta.id, videoId: e.id, label: "S" + page.pad2(e.season) + "E" + page.pad2(e.episode)})
                            } else if (page.streams && page.streams.list && page.streams.list.length > 0) {
                                Core.cmd({cmd: "play", index: 0})
                            } else {
                                Core.cmd({cmd: "play_best", kind: page.meta.kind, id: page.meta.id, videoId: ""})
                            }
                        }
                    }
                    FlatButton { glyph: "info"; text: Core.tr(Core.uiLang, "Info"); implicitHeight: 44; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Everything about this anime, like MyAnimeList"); onClicked: win.openInfo() }
                    FlatButton {
                        glyph: page.meta.inLibrary ? "check" : "plus"
                        text: page.meta.inLibrary ? Core.tr(Core.uiLang, "In library") : Core.tr(Core.uiLang, "Add to library")
                        implicitHeight: 44
                        onClicked: Core.cmd({cmd: "toggle_library", item: page.meta})
                    }
                    // as tall as the buttons beside it, so the row reads as one group
                    Rectangle {
                        width: 44
                        height: width
                        radius: width / 2
                        color: favArea.containsMouse ? Theme.c.buttonHover : Theme.c.button
                        // On: a solid yellow star (easy to spot). Off: a hollow star in the text colour.
                        Icon { anchors.centerIn: parent; name: page.meta.favorite ? "star" : "staroutline"; filled: page.meta.favorite === true; size: 20; color: page.meta.favorite ? Theme.c.starEdge : Theme.c.text; fillColor: Theme.c.star }
                        ToolTip.visible: favArea.containsMouse
                        ToolTip.text: page.meta.favorite ? Core.tr(Core.uiLang, "Remove from favorites") : Core.tr(Core.uiLang, "Add to favorites")
                        MouseArea { id: favArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Core.cmd({cmd: "toggle_favorite", item: page.meta}); onPressAndHold: if (Theme.touch) parent.ToolTip.show(parent.ToolTip.text, 2500) }
                    }
                }
                Text {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: page.meta.description ? page.meta.description : ""
                    color: Theme.c.text
                    font.pixelSize: Theme.mobile ? 16 : 17
                    font.family: Theme.type.family
                    lineHeight: 1.2
                }
                Text {
                    visible: !!(page.meta.cast && page.meta.cast.length > 0)
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: Core.tr(Core.uiLang, "Cast") + ": " + (page.meta.cast ? page.meta.cast.slice(0, 8).join(", ") : "")
                    color: Theme.c.textDim
                    font.pixelSize: 16
                    font.family: Theme.type.family
                }

                // season picker
                Flow {
                    visible: page.isSeries && page.d.seasons !== undefined && page.d.seasons.length > 0
                    width: parent.width
                    spacing: 8
                    FlatButton {
                        kind: "plain"
                        implicitHeight: 38
                        // the control sizes its content item to the whole button, and a Row given a width lays its
                        // children from its left edge: wrapped in an Item so the label sits centred in the pill (HP)
                        contentItem: Item {
                            implicitWidth: seasonRow.implicitWidth
                            implicitHeight: seasonRow.implicitHeight
                            Row {
                                id: seasonRow
                                spacing: 10
                                anchors.centerIn: parent
                                Text { text: page.d.season === 0 ? Core.tr(Core.uiLang, "Specials") : Core.tr(Core.uiLang, "Season") + " " + page.d.season; color: Theme.c.text; font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                                Icon { name: "down"; size: 16; anchors.verticalCenter: parent.verticalCenter }
                            }
                        }
                        implicitWidth: 130
                        onClicked: seasonMenu.popup(0, height + 4)
                        AppMenu {
                            id: seasonMenu
                            Repeater {
                                model: page.d.seasons !== undefined ? page.d.seasons : []
                                delegate: AppMenuItem { text: modelData === 0 ? Core.tr(Core.uiLang, "Specials") : Core.tr(Core.uiLang, "Season") + " " + modelData; onTriggered: Core.cmd({cmd: "set_season", season: modelData}) }
                            }
                        }
                    }
                    Repeater {
                        model: page.d.times ? [[page.d.times.left, true, Core.tr(Core.uiLang, "Time to finish this season")], [page.d.times.season, false, Core.tr(Core.uiLang, "All episodes of this season")]].concat(page.d.times.series ? [[page.d.times.series, false, Core.tr(Core.uiLang, "All the content")]] : []) : []
                        delegate: Item {
                            required property var modelData
                            width: tc.width
                            height: 38
                            Rectangle {
                                id: tc
                                anchors.centerIn: parent
                                radius: height / 2
                                color: modelData[1] ? Theme.c.accent28 : Theme.c.chip
                                width: tcText.implicitWidth + 24
                                height: 28
                                Text { id: tcText; anchors.centerIn: parent; text: modelData[0]; color: Theme.c.text; font.pixelSize: 13; font.weight: modelData[1] ? Font.Bold : Font.Normal; font.family: Theme.type.family }
                            }
                            ToolTip.visible: tcArea.containsMouse
                            ToolTip.text: modelData[2]
                            MouseArea { id: tcArea; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.RightButton; onClicked: seasonMarks.popup() }
                        }
                    }
                    // Keep downloaded: download this season to your computer (GTK keep-button, green pill).
                    Item {
                        visible: page.d.season >= 1
                        width: keepBtn.width
                        height: 38
                        FlatButton {
                            id: keepBtn
                            anchors.verticalCenter: parent.verticalCenter
                            kind: "green"
                            pill: true
                            glyph: "download"
                            implicitWidth: 52
                            implicitHeight: 34
                            ToolTip.visible: hovered
                            ToolTip.text: Core.tr(Core.uiLang, "Keep downloaded: download this season to your computer")
                            onClicked: {
                                var r = Core.cmd({cmd: "dl_open"})
                                if (r.ok === false) Core.toast(r.error)
                                else dlPick.open()
                            }
                        }
                    }
                    AppMenu {
                        id: seasonMarks
                        width: 300
                        AppMenuItem { text: Core.tr(Core.uiLang, "Mark season as completed"); onTriggered: Core.cmd({cmd: "mark", scope: "season", watched: true}) }
                        AppMenuItem { text: Core.tr(Core.uiLang, "Mark all as completed"); onTriggered: Core.cmd({cmd: "mark", scope: "all", watched: true}) }
                        AppMenuItem { text: Core.tr(Core.uiLang, "Mark season as not completed"); onTriggered: Core.cmd({cmd: "mark", scope: "season", watched: false}) }
                        AppMenuItem { text: Core.tr(Core.uiLang, "Mark all as not completed"); onTriggered: Core.cmd({cmd: "mark", scope: "all", watched: false}) }
                    }
                }

                // episodes: a window of rows (ListView) that slides along inside its place in the column while the page
                // scrolls, so a season of hundreds of episodes builds only the rows on screen (each row has a picture with
                // an effect layer of its own) and reuses them; the number is the model, so rows stay when only data changes
                Rectangle {
                    id: epBox
                    visible: page.isSeries && page.episodes.length > 0
                    width: parent.width
                    radius: Theme.shape.card
                    color: Theme.c.rowA
                    clip: true
                    height: page.episodes.length * 115
                    ListView {
                        id: epList
                        width: parent.width
                        height: Math.min(epBox.height, infoScroll.height)
                        interactive: false
                        reuseItems: true
                        cacheBuffer: 230
                        model: page.episodes.length
                        // where the box is inside the scrolled content; the window follows the part of it on screen
                        readonly property real docY: info.y + epBox.y
                        function sync() {
                            const off = Math.max(0, Math.min(infoScroll.contentY - docY, epBox.height - height))
                            y = off
                            contentY = off
                        }
                        onDocYChanged: sync()
                        onHeightChanged: sync()
                        onCountChanged: sync()
                        Component.onCompleted: sync()
                        Connections { target: infoScroll; function onContentYChanged() { epList.sync() } }
                        delegate: Rectangle {
                            id: epRow
                            required property int index
                            readonly property var modelData: page.episodes[index] || ({})
                            readonly property bool watched: modelData.watched === true
                            readonly property bool hovered: epArea.containsMouse || markArea.containsMouse
                            width: epList.width
                            height: 115
                            // Episodes not watched yet are dimmed (GTK row.ep-unwatched), and light up under the mouse.
                            color: hovered ? Theme.c.cardHover : (watched ? (index % 2 === 0 ? Theme.c.rowA : Theme.c.rowB) : Theme.c.view)
                            Rectangle { visible: index > 0; width: parent.width; height: 1; color: Theme.c.divider }
                            Item {
                                anchors.fill: parent
                                opacity: epRow.watched || epRow.hovered ? 1 : 0.62
                                RoundedPicture {
                                    id: thumb
                                    x: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 175
                                    height: 98
                                    radius: 8
                                    url: epRow.modelData.thumbnail ? epRow.modelData.thumbnail : ""
                                    fallback: page.meta.background ? page.meta.background : ""
                                    blur: page.d.blurUnwatched === true && !epRow.watched && !win.inPlayer ? 1.0 : 0
                                    Rectangle {
                                        visible: epRow.watched
                                        x: 4
                                        y: parent.height - height - 4
                                        width: 84
                                        height: 22
                                        radius: 6
                                        color: Theme.c.rating
                                        Text { anchors.centerIn: parent; text: Core.tr(Core.uiLang, "WATCHED"); color: Theme.scheme === "light" ? "#ffffff" : "#1a1a1a"; font.pixelSize: 12; font.weight: Font.ExtraBold; font.family: Theme.type.family }
                                    }
                                }
                                Column {
                                    anchors { left: thumb.right; leftMargin: 14; right: suffix.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
                                    spacing: 6
                                    Text { width: parent.width; text: (epRow.modelData.episode !== null && epRow.modelData.episode !== undefined ? epRow.modelData.episode + ".  " : "") + (epRow.modelData.title ? epRow.modelData.title : Core.tr(Core.uiLang, "Episode") + " " + epRow.modelData.episode); color: Theme.c.text; font.pixelSize: 18; font.family: Theme.type.family; elide: Text.ElideRight }
                                    Text { text: epRow.modelData.released ? epRow.modelData.released.substring(0, 10) : ""; color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
                                }
                                Row {
                                    id: suffix
                                    anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                                    spacing: 10
                                    // On disk, downloading, or online (GTK .pill-saved / .pill-downloading / .pill-online).
                                    Rectangle {
                                        readonly property string st: epRow.modelData.state ? epRow.modelData.state : "online"
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: pillText.implicitWidth + 20
                                        height: 22
                                        radius: 11
                                        color: st === "saved" ? Theme.c.greenBtn : (st === "downloading" ? Theme.c.warning : Theme.c.selected)
                                        Text {
                                            id: pillText
                                            anchors.centerIn: parent
                                            text: parent.st === "saved" ? Core.tr(Core.uiLang, "Saved on disk") : (parent.st === "downloading" ? Core.tr(Core.uiLang, "Downloading") + " " + Math.round(epRow.modelData.percent || 0) + "%" : Core.tr(Core.uiLang, "Online file"))
                                            color: parent.st === "saved" ? Theme.c.greenFg : (parent.st === "downloading" ? (Theme.scheme === "light" ? "#ffffff" : "#261a00") : Theme.c.textDim)
                                            font.pixelSize: 12
                                            font.weight: parent.st === "online" ? Font.Normal : Font.ExtraBold
                                            font.family: Theme.type.family
                                        }
                                    }
                                    // Mark as watched / not watched.
                                    Rectangle {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 34
                                        height: 34
                                        radius: 17
                                        color: markArea.containsMouse ? Theme.c.hover : "transparent"
                                        Icon { visible: epRow.watched; anchors.centerIn: parent; name: "check"; size: 20; color: Theme.c.rating }
                                        Rectangle { visible: !epRow.watched; anchors.centerIn: parent; width: 16; height: 16; radius: 8; color: "transparent"; border.width: 2; border.color: Theme.c.textDim }
                                        ToolTip.visible: markArea.containsMouse
                                        ToolTip.text: epRow.watched ? Core.tr(Core.uiLang, "Mark as not watched") : Core.tr(Core.uiLang, "Mark as watched")
                                        MouseArea { id: markArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Core.cmd({cmd: "toggle_watched", id: epRow.modelData.id}) }
                                    }
                                    Icon { visible: page.d.selectedKey === epRow.modelData.id; anchors.verticalCenter: parent.verticalCenter; name: "play"; size: 18 }
                                }
                            }
                            MouseArea {
                                id: epArea
                                anchors { left: parent.left; top: parent.top; bottom: parent.bottom; right: parent.right; rightMargin: 60 }
                                hoverEnabled: true
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                cursorShape: Qt.PointingHandCursor
                                onClicked: function(m) {
                                    if (m.button === Qt.RightButton) { epMarks.popup(); return }
                                    Core.cmd({cmd: "choose_episode", id: epRow.modelData.id, label: "S" + page.pad2(epRow.modelData.season) + "E" + page.pad2(epRow.modelData.episode)})
                                }
                            }
                            // Right-click: everything before or after this one (GTK marks.rs).
                            AppMenu {
                                id: epMarks
                                width: 330
                                AppMenuItem { text: epRow.watched ? Core.tr(Core.uiLang, "Mark everything before as not watched") : Core.tr(Core.uiLang, "Mark everything before as watched"); onTriggered: Core.cmd({cmd: "mark", scope: "before", id: epRow.modelData.id, watched: !epRow.watched}) }
                                AppMenuItem { text: epRow.watched ? Core.tr(Core.uiLang, "Mark everything after as not watched") : Core.tr(Core.uiLang, "Mark everything after as watched"); onTriggered: Core.cmd({cmd: "mark", scope: "after", id: epRow.modelData.id, watched: !epRow.watched}) }
                            }
                        }
                    }
                }

                // The end of the page: similar titles, then what people wrote about it.
                SimilarRow { width: parent.width; recs: page.d.mal && page.d.mal.recs ? page.d.mal.recs : []; tmdb: !!(page.d.mal && page.d.mal.source === "tmdb") }
                ReviewsSection { width: parent.width; reviews: page.d.mal && page.d.mal.reviews ? page.d.mal.reviews : []; url: page.d.mal && page.d.mal.info && page.d.mal.info.url ? page.d.mal.info.url : ""; tmdb: !!(page.d.mal && page.d.mal.source === "tmdb") }
            }
        }
    }

    // ---- streams ----
    Rectangle {
        id: streamsPanel
        anchors { left: page.stacked ? parent.left : undefined; leftMargin: 12; right: parent.right; rightMargin: Theme.mobile ? 12 : 17; top: page.stacked ? infoPanel.bottom : parent.top; topMargin: page.stacked ? 10 : (Theme.mobile ? infoPanel.y : (page.winButtons ? 46 : 14)); bottom: parent.bottom; bottomMargin: Theme.mobile ? 12 : 18 }
        width: Math.max(360, page.width * 0.345)
        radius: Theme.shape.large
        color: Theme.c.panelDark
        border.width: 1
        border.color: Theme.c.border
        Column {
            id: sHead
            x: 14
            y: 14
            width: parent.width - 28
            spacing: 6
            Row {
                spacing: 10
                Text { text: Core.tr(Core.uiLang, "Streams") + (page.streams && page.streams.label ? " · " + page.streams.label : ""); color: Theme.c.text; font.pixelSize: 24; font.weight: Font.ExtraBold; font.family: Theme.type.family }
                BusyIndicator { visible: !!(page.streams && page.streams.loading); running: visible; implicitWidth: 26; implicitHeight: 26; anchors.verticalCenter: parent.verticalCenter }
            }
            // What to do next (GTK streams_hint).
            Text {
                width: parent.width
                wrapMode: Text.Wrap
                text: !page.streams ? (page.isSeries ? Core.tr(Core.uiLang, "Pick an episode to see its streams.") : Core.tr(Core.uiLang, "Loading…"))
                    : (page.streams.loading ? Core.tr(Core.uiLang, "Asking your addons…")
                    : ((page.streams.list || []).length === 0 ? Core.tr(Core.uiLang, "No streams found. Add more addons in Preferences, or turn on Real-Debrid.") : Core.tr(Core.uiLang, "Choose the source to play. You can change it from the player later.")))
                color: Theme.c.textDim
                font.pixelSize: 15
                font.family: Theme.type.family
            }
        }
        // The streams in sections by quality (4K, 1080p, 720p, lower), each showing only its 2 best until it is opened
        // (HP, 2026-11-02: "condensar os streams em qualidades ... somente deixe os 2 melhores aparentes"). Inside a
        // section the core's order (best first) is kept; a copy saved on disk is always shown.
        ListView {
            id: sList
            anchors { left: parent.left; right: parent.right; top: sHead.bottom; bottom: parent.bottom; margins: 14 }
            clip: true
            spacing: 8
            readonly property var rows: page.streams && page.streams.list ? page.streams.list : []
            readonly property int shownPerGroup: 2
            // the sections opened by the user; closed again when another episode's streams come in
            property var open: ({})
            property string openFor: ""
            readonly property string listKey: page.streams && page.streams.label !== undefined ? String(page.streams.label) : ""
            onListKeyChanged: if (listKey !== openFor) { openFor = listKey; open = ({}) }
            function groupOf(r) {
                var res = r.res || 0
                return res >= 2160 ? "4k" : (res >= 1080 ? "1080" : (res >= 720 ? "720" : "low"))
            }
            readonly property var groups: [
                { key: "4k", label: "4K" }, { key: "1080", label: "1080p" }, { key: "720", label: "720p" },
                { key: "low", label: Core.tr(Core.uiLang, "Below 720p") }]
            readonly property var entries: {
                var by = {}, out = [], g, i
                for (i = 0; i < rows.length; i++) {
                    g = groupOf(rows[i] || {})
                    if (!by[g]) by[g] = []
                    by[g].push(i)
                }
                for (var k = 0; k < groups.length; k++) {
                    var key = groups[k].key, idx = by[key] || []
                    if (idx.length === 0)
                        continue
                    var opened = !!open[key], extra = 0
                    for (i = shownPerGroup; i < idx.length; i++)
                        if (rows[idx[i]].local !== true)
                            extra++
                    out.push({ kind: "head", key: key, label: groups[k].label, count: idx.length, opened: opened, extra: extra })
                    for (i = 0; i < idx.length; i++)
                        if (opened || i < shownPerGroup || rows[idx[i]].local === true)
                            out.push({ kind: "row", index: idx[i] })
                    if (extra > 0)
                        out.push({ kind: "more", key: key, opened: opened, hidden: extra })
                }
                return out
            }
            function toggle(key) {
                var o = Object.assign({}, open)
                o[key] = !o[key]
                open = o
            }
            model: entries.length
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            delegate: Loader {
                id: ent
                required property int index
                readonly property var e: sList.entries[index] || ({})
                width: sList.width
                sourceComponent: e.kind === "head" ? headComp : (e.kind === "more" ? moreComp : rowComp)
                Component {
                    id: rowComp
                    StreamRow {
                        width: sList.width
                        stream: sList.rows[ent.e.index] || ({})
                        episode: page.isSeries
                        onClicked: Core.cmd({cmd: "play", index: ent.e.index})
                    }
                }
                // the section's title: the quality and how many streams it holds; a click opens or closes it
                Component {
                    id: headComp
                    Item {
                        width: sList.width
                        height: 34
                        Row {
                            anchors { left: parent.left; leftMargin: 4; verticalCenter: parent.verticalCenter }
                            spacing: 8
                            Icon { name: ent.e.extra > 0 ? (ent.e.opened ? "down" : "right") : "down"; size: 18; color: Theme.c.textDim; anchors.verticalCenter: parent.verticalCenter; opacity: ent.e.extra > 0 ? 1 : 0.4 }
                            Text { text: ent.e.label || ""; color: Theme.c.text; font.pixelSize: 18; font.weight: Font.ExtraBold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                            Text {
                                text: ent.e.count + " " + (ent.e.count === 1 ? Core.tr(Core.uiLang, "stream") : Core.tr(Core.uiLang, "streams"))
                                color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                        Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 1; color: Theme.c.border }
                        MouseArea { anchors.fill: parent; enabled: ent.e.extra > 0; cursorShape: Qt.PointingHandCursor; onClicked: sList.toggle(ent.e.key) }
                    }
                }
                // "Show 7 more" under the 2 best, or "Show less" when the section is open
                Component {
                    id: moreComp
                    Rectangle {
                        width: sList.width
                        height: 36
                        radius: Theme.shape.card
                        color: ma.containsMouse ? Theme.c.cardHover : "transparent"
                        border.width: 1
                        border.color: Theme.c.border
                        Row {
                            anchors.centerIn: parent
                            spacing: 6
                            Icon { name: ent.e.opened ? "up" : "down"; size: 16; color: Theme.c.accent; anchors.verticalCenter: parent.verticalCenter }
                            Text {
                                text: ent.e.opened ? Core.tr(Core.uiLang, "Show less") : Core.tr(Core.uiLang, "Show %1 more").arg(ent.e.hidden)
                                color: Theme.c.accent; font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sList.toggle(ent.e.key) }
                    }
                }
            }
        }
        // Addons that failed (GTK streams_errors: red captions).
        Column {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 14 }
            spacing: 4
            visible: !!(page.streams && page.streams.errors && page.streams.errors.length > 0 && (page.streams.list || []).length === 0)
            Repeater {
                model: page.streams && page.streams.errors ? page.streams.errors.slice(0, 5) : []
                delegate: Text { required property var modelData; width: parent.width; wrapMode: Text.Wrap; text: Core.tr(Core.uiLang, String(modelData)); color: Theme.c.errorText; font.pixelSize: 13; font.family: Theme.type.family }
            }
        }
    }
    DlPickDialog { id: dlPick }
}
