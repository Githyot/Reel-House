import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The INFO page (GTK malinfo.rs): a lean MyAnimeList (or TMDB for movies and series) inside the app.
Item {
    id: page
    readonly property var d: Core.detail
    readonly property var meta: d.meta !== undefined ? d.meta : ({})
    readonly property var mal: d.mal !== undefined ? d.mal : ({})
    readonly property var info: mal.info ? mal.info : null
    readonly property bool tmdb: !!info && info.provider === "tmdb"
    function fmtNum(n) { return Math.round(n || 0).toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",") }
    function names(v) { return (v || []).map(function(x) { return x.name }).join(", ") }
    function arr(v) { return v ? v : [] }

    Flickable {
        id: scroll
        anchors.fill: parent
        contentWidth: width
        contentHeight: col.implicitHeight + 50
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }
        Column {
            id: col
            x: Math.max(18, (scroll.width - 980) / 2)
            y: 8
            width: Math.min(980, scroll.width - 36)
            spacing: 14

            // ---- not (yet) on MyAnimeList: what the catalog knows ----
            Column {
                visible: !page.info
                width: parent.width
                spacing: 12
                Text { width: parent.width; wrapMode: Text.Wrap; text: page.meta.name ? page.meta.name : ""; color: Theme.c.text; font.pixelSize: 34; font.weight: Font.ExtraBold; font.family: Theme.type.family }
                BusyIndicator { visible: page.mal.loading === true; running: visible; implicitWidth: 32; implicitHeight: 32 }
                Text {
                    width: parent.width
                    wrapMode: Text.Wrap
                    visible: text !== ""
                    text: page.mal.loading === true ? Core.tr(Core.uiLang, "Looking this title up on MyAnimeList…")
                        : (page.mal.notFound === true ? Core.tr(Core.uiLang, "This title was not found on MyAnimeList. Add a TMDB key in Preferences (Metadata) to load movie and series details. Here is what the catalog knows.") : (page.mal.error ? page.mal.error : ""))
                    color: page.mal.error && page.mal.loading !== true && page.mal.notFound !== true ? Theme.c.error : Theme.c.textDim
                    font.pixelSize: 15
                    font.family: Theme.type.family
                }
                FlatButton { visible: (page.mal.loading === true && page.mal.slow === true) || (!!page.mal.error && page.mal.loading !== true); pill: true; text: Core.tr(Core.uiLang, "Try again"); onClicked: Core.cmd({cmd: "retry_mal"}) }
                SectionCard {
                    width: parent.width
                    title: Core.tr(Core.uiLang, "About")
                    Text { visible: !!page.meta.description; width: parent.width; wrapMode: Text.Wrap; text: page.meta.description ? page.meta.description : ""; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
                    PropertyList {
                        width: parent.width
                        rows: [["Year", page.meta.releaseInfo], ["Runtime", page.meta.runtime], ["Genres", page.arr(page.meta.genres).join(", ")], ["Cast", page.arr(page.meta.cast).slice(0, 10).join(", ")]]
                    }
                }
            }

            // ---- header ----
            Row {
                visible: !!page.info
                width: parent.width
                spacing: 18
                RoundedPicture { width: 170; height: 250; url: page.info && page.info.image ? page.info.image : (page.meta.poster ? page.meta.poster : "") }
                Column {
                    width: parent.width - 188
                    spacing: 8
                    Text { width: parent.width; wrapMode: Text.Wrap; text: page.info ? page.info.title : ""; color: Theme.c.text; font.pixelSize: 34; font.weight: Font.ExtraBold; font.family: Theme.type.family }
                    Repeater {
                        model: page.info ? [page.info.titleEn, page.info.titleJa].filter(function(t) { return t && t !== page.info.title }) : []
                        delegate: Text { required property var modelData; width: parent.width; wrapMode: Text.Wrap; text: modelData; color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
                    }
                    Text { visible: page.info && page.arr(page.info.synonyms).length > 0; width: parent.width; wrapMode: Text.Wrap; text: page.info ? page.arr(page.info.synonyms).join(" · ") : ""; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
                    Row {
                        topPadding: 6
                        spacing: 22
                        Column {
                            Text { text: page.info && page.info.score > 0 ? page.info.score.toFixed(2) : "N/A"; color: Theme.c.rating; font.pixelSize: 32; font.weight: Font.ExtraBold; font.family: Theme.type.family }
                            Text { visible: page.info && page.info.scoredBy > 0; text: page.info ? page.fmtNum(page.info.scoredBy) + " users" : ""; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                        }
                        Repeater {
                            model: page.info ? [["Ranked", page.info.rank > 0 ? "#" + page.fmtNum(page.info.rank) : "N/A"], ["Popularity", page.info.popularity > 0 ? "#" + page.fmtNum(page.info.popularity) : "N/A"], ["Members", page.fmtNum(page.info.members)]].filter(function(x) { return !(page.tmdb && (x[0] === "Ranked" || x[0] === "Members")) }) : []
                            delegate: Column {
                                required property var modelData
                                anchors.verticalCenter: parent.verticalCenter
                                Text { text: Core.tr(Core.uiLang, "" + modelData[0]); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                                Text { text: modelData[1]; color: Theme.c.text; font.pixelSize: 20; font.weight: Font.ExtraBold; font.family: Theme.type.family }
                            }
                        }
                    }
                    Flow {
                        width: parent.width
                        spacing: 6
                        Repeater {
                            model: page.info ? [page.info.kind, page.info.status, page.info.premiered, page.info.duration, page.info.episodes ? page.info.episodes + " episodes" : ""].filter(function(x) { return !!x }) : []
                            delegate: Chip { required property var modelData; text: modelData }
                        }
                    }
                    Flow {
                        width: parent.width
                        spacing: 6
                        Repeater {
                            model: page.info ? page.arr(page.info.genres).concat(page.arr(page.info.themes)).concat(page.tmdb ? [] : page.arr(page.info.demographics)) : []
                            delegate: GenreChip {
                                required property var modelData
                                text: modelData.name
                                clickable: !page.tmdb
                                onClicked: win.openGenre(modelData.id, modelData.name, "anime")
                            }
                        }
                    }
                    Row {
                        topPadding: 4
                        spacing: 8
                        FlatButton { visible: page.info && !!page.info.url; pill: true; text: page.tmdb ? Core.tr(Core.uiLang, "Open on TMDB") : Core.tr(Core.uiLang, "Open on MyAnimeList"); onClicked: Qt.openUrlExternally(page.info.url) }
                        FlatButton { visible: page.info && !!page.info.trailer; pill: true; text: Core.tr(Core.uiLang, "Trailer"); onClicked: Qt.openUrlExternally(page.info.trailer) }
                    }
                }
            }

            // ---- my list (MyAnimeList only) ----
            SectionCard {
                id: myList
                visible: !!page.info && !page.tmdb
                width: parent.width
                title: Core.tr(Core.uiLang, "My list")
                property var my: page.mal.my ? page.mal.my : null
                property int statusIdx: 0
                property int scoreIdx: 0
                property int watched: 0
                readonly property var scoreLabels: ["No score", "(10) Masterpiece", "(9) Great", "(8) Very good", "(7) Good", "(6) Fine", "(5) Average", "(4) Bad", "(3) Very bad", "(2) Horrible", "(1) Appalling"]
                readonly property var statusLabels: [Core.tr(Core.uiLang, "Not on my list")].concat(page.arr(page.mal.statuses).map(function(s) { return s.label }))
                function reset() {
                    var st = page.arr(page.mal.statuses)
                    statusIdx = 0
                    for (var i = 0; i < st.length; ++i) if (my && st[i].id === my.status) statusIdx = i + 1
                    scoreIdx = my && my.score > 0 ? 11 - Math.min(10, my.score) : 0
                    watched = my ? my.watched : 0
                }
                onMyChanged: reset()
                Component.onCompleted: reset()
                Text { visible: page.mal.logged !== true; width: parent.width; wrapMode: Text.Wrap; text: Core.tr(Core.uiLang, "Log in to MyAnimeList to set your status and score for this anime."); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
                FlatButton { visible: page.mal.logged !== true; kind: "suggested"; pill: true; text: Core.tr(Core.uiLang, "Log in"); onClicked: win.openPrefs("addons") }
                Rectangle {
                    visible: page.mal.logged === true
                    width: parent.width
                    height: 3 * 54
                    radius: 12
                    color: Theme.c.card
                    border.width: 1
                    border.color: Theme.c.divider
                    Column {
                        width: parent.width
                        PrefCombo { width: parent.width; title: Core.tr(Core.uiLang, "Status"); options: myList.statusLabels; current: myList.statusIdx
                            onPicked: function(i) {
                                myList.statusIdx = i
                                var st = page.arr(page.mal.statuses)
                                if (i > 0 && st[i - 1].id === "completed" && page.info && page.info.episodes > 0) myList.watched = page.info.episodes
                            }
                        }
                        PrefCombo { width: parent.width; first: false; title: Core.tr(Core.uiLang, "Score"); options: myList.scoreLabels.map(function(x) { return Core.tr(Core.uiLang, x) }); current: myList.scoreIdx; onPicked: function(i) { myList.scoreIdx = i } }
                        PrefSpin { width: parent.width; first: false; title: Core.tr(Core.uiLang, "Episodes watched"); subtitle: page.info && page.info.episodes ? "of " + page.info.episodes : ""; from: 0; to: page.info && page.info.episodes > 0 ? page.info.episodes : 10000; value: myList.watched; onMoved: function(v) { myList.watched = v } }
                    }
                }
                Row {
                    visible: page.mal.logged === true
                    spacing: 8
                    FlatButton {
                        kind: "suggested"
                        pill: true
                        text: Core.tr(Core.uiLang, "Save")
                        onClicked: {
                            if (myList.statusIdx === 0) { win.showToast("Choose a status first."); return }
                            var st = page.arr(page.mal.statuses)[myList.statusIdx - 1].id
                            Core.cmd({cmd: "set_my_status", status: st, score: myList.scoreIdx === 0 ? 0 : 11 - myList.scoreIdx, watched: myList.watched})
                        }
                    }
                    FlatButton { visible: !!myList.my; kind: "destructive"; pill: true; text: Core.tr(Core.uiLang, "Remove from my list"); onClicked: Core.cmd({cmd: "remove_my_status"}) }
                }
            }

            SectionCard { visible: page.info && !!page.info.synopsis; width: parent.width; title: Core.tr(Core.uiLang, "Synopsis")
                Text { width: parent.width; wrapMode: Text.Wrap; text: page.info ? page.info.synopsis : ""; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family } }
            SectionCard { visible: page.info && !!page.info.background; width: parent.width; title: Core.tr(Core.uiLang, "Background")
                Text { width: parent.width; wrapMode: Text.Wrap; text: page.info ? page.info.background : ""; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family } }
            SectionCard {
                visible: !!page.info
                width: parent.width
                title: Core.tr(Core.uiLang, "Information")
                PropertyList {
                    width: parent.width
                    rows: page.info ? [["Type", page.info.kind], ["Episodes", page.info.episodes ? "" + page.info.episodes : ""], ["Status", page.info.status], ["Aired", page.info.aired], ["Premiered", page.info.premiered], ["Broadcast", page.info.broadcast],
                        [page.tmdb ? "Production" : "Studios", page.names(page.info.studios)], ["Producers", page.names(page.info.producers)], [page.tmdb ? "Networks" : "Licensors", page.names(page.info.licensors)],
                        ["Source", page.info.source], ["Duration", page.info.duration], ["Rating", page.info.rating]].concat(page.arr(page.info.facts)) : []
                }
            }
            SectionCard {
                visible: !!page.info
                width: parent.width
                title: Core.tr(Core.uiLang, "Statistics")
                PropertyList {
                    width: parent.width
                    rows: page.info ? [["Score", page.info.score > 0 ? page.info.score.toFixed(2) + " (" + Core.tr(Core.uiLang, "scored by") + " " + page.fmtNum(page.info.scoredBy) + " " + Core.tr(Core.uiLang, "users") + ")" : "N/A"],
                        ["Ranked", page.tmdb ? "" : (page.info.rank > 0 ? "#" + page.fmtNum(page.info.rank) : "N/A")], ["Popularity", page.info.popularity > 0 ? "#" + page.fmtNum(page.info.popularity) : "N/A"],
                        ["Members", page.tmdb ? "" : page.fmtNum(page.info.members)], ["Favorites", page.tmdb ? "" : page.fmtNum(page.info.favorites)]] : []
                }
            }
            SectionCard {
                visible: page.info && page.arr(page.info.relations).length > 0
                width: parent.width
                title: Core.tr(Core.uiLang, "Related")
                Repeater {
                    model: page.info ? page.arr(page.info.relations) : []
                    delegate: Column {
                        required property var modelData
                        width: parent.width
                        spacing: 4
                        Text { text: modelData.kind; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                        Flow {
                            width: parent.width
                            spacing: 6
                            Repeater {
                                model: modelData.entries
                                delegate: GenreChip {
                                    required property var modelData
                                    readonly property bool anime: modelData.kind.toLowerCase() === "anime"
                                    text: anime ? modelData.name : modelData.name + " (" + modelData.kind + ")"
                                    onClicked: anime ? Core.cmd({cmd: "open_detail", item: {id: "mal:" + modelData.id, kind: "series", name: modelData.name, malId: modelData.id}})
                                                     : Qt.openUrlExternally("https://myanimelist.net/manga/" + modelData.id)
                                }
                            }
                        }
                    }
                }
            }
            Repeater {
                model: page.info ? [["Opening themes", page.arr(page.info.openings)], ["Ending themes", page.arr(page.info.endings)]].filter(function(x) { return x[1].length > 0 }) : []
                delegate: SectionCard {
                    required property var modelData
                    width: col.width
                    title: Core.tr(Core.uiLang, "" + modelData[0])
                    Repeater {
                        model: modelData[1].slice(0, 20)
                        delegate: Text { required property var modelData; width: parent.width; wrapMode: Text.Wrap; text: modelData; color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family }
                    }
                }
            }
            SectionCard {
                visible: page.info && (page.arr(page.info.streaming).length + page.arr(page.info.external).length) > 0
                width: parent.width
                title: Core.tr(Core.uiLang, "Links")
                Flow {
                    width: parent.width
                    spacing: 6
                    Repeater {
                        model: page.info ? page.arr(page.info.streaming).concat(page.arr(page.info.external)) : []
                        delegate: GenreChip { required property var modelData; text: modelData.name; onClicked: Qt.openUrlExternally(modelData.url) }
                    }
                }
            }
            Repeater {
                model: [[page.tmdb ? "Cast" : "Characters and voice actors", page.arr(page.mal.characters), true], ["Staff", page.arr(page.mal.staff), false]].filter(function(x) { return x[1].length > 0 })
                delegate: SectionCard {
                    required property var modelData
                    width: col.width
                    title: Core.tr(Core.uiLang, "" + modelData[0])
                    ListView {
                        width: parent.width
                        height: 140 + 64
                        orientation: ListView.Horizontal
                        spacing: 14
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        model: modelData[1]
                        ScrollBar.horizontal: ScrollBar { }
                        delegate: Column {
                            required property var modelData
                            width: 104
                            spacing: 3
                            RoundedPicture { width: 100; height: 140; radius: 8; url: modelData.image ? modelData.image : "" }
                            Text { width: 100; elide: Text.ElideRight; text: modelData.name; color: Theme.c.text; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family }
                            Text { width: 100; elide: Text.ElideRight; text: modelData.role !== undefined ? modelData.role : modelData.positions; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                            Text { visible: !!modelData.actor; width: 100; elide: Text.ElideRight; text: modelData.actor ? modelData.actor : ""; color: Theme.c.text; font.pixelSize: 12; font.family: Theme.type.family }
                        }
                    }
                }
            }
            SimilarRow { width: parent.width; recs: page.arr(page.mal.recs); tmdb: page.tmdb }
            ReviewsSection { width: parent.width; reviews: page.arr(page.mal.reviews); url: page.info && page.info.url ? page.info.url : ""; tmdb: page.tmdb }
        }
    }
}
