import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Study: statistics. What was done today, the last 30 days, how many cards of each kind, and what is due in the next two weeks.
Item {
    id: sx
    property var stats: null
    property var decks: []
    readonly property real cw: Math.min(860, width - Theme.spacing.page * 2)
    readonly property real x0: (width - cw) / 2
    signal back()
    function tr(s) { return Core.tr(Core.uiLang, s) }
    readonly property var s: stats ? stats : ({})
    readonly property var last30: s.last30 ? s.last30 : []
    readonly property var forecast: s.forecast ? s.forecast : []
    readonly property real did: stats ? (stats.did || 0) : 0
    readonly property color cNew: Theme.scheme === "light" ? "#1c71d8" : "#78aeed"
    function maxOf(a) { var m = 1; for (var i = 0; i < a.length; ++i) m = Math.max(m, a[i]); return m }
    function deckName(id) { for (var i = 0; i < decks.length; ++i) if (decks[i].id === id) return decks[i].name; return "" }
    function stateColor(k) {
        return k === "new" ? sx.cNew : (k === "learn" ? Theme.c.error : (k === "young" ? Theme.c.success : Qt.darker(Theme.c.success, 1.5)))
    }

    StudyHeader {
        id: hdr
        x: sx.x0; width: sx.cw
        y: Theme.layout.largeTitles === true ? 0 : 12
        title: sx.tr("Statistics")
        subtitle: sx.did === 0 ? sx.tr("All decks") : sx.deckName(sx.did).split("::").join(" › ")
        onBack: sx.back()
        FlatButton {
            id: deckBtn
            anchors.verticalCenter: parent.verticalCenter
            kind: "plain"; glyph: "list"
            text: sx.cw > 560 ? sx.tr("Deck") : ""
            onClicked: deckMenu.openBelow(deckBtn, true)
        }
    }
    // the macaw is always on the Study pages: pleased with a good retention, thinking otherwise
    Mascot {
        id: buddy
        z: 3
        // beside the column when there is room, small in the corner otherwise
        size: sx.x0 >= 120 ? 96 : 52
        x: sx.width - size - 14
        y: sx.height - size - 14
        mood: !sx.stats || (sx.s.retention || 0) === 0 ? "think" : ((sx.s.retention || 0) >= 0.85 ? "cheer" : ((sx.s.retention || 0) >= 0.7 ? "happy" : "think"))
        animate: sx.visible
    }

    PopMenu {
        id: deckMenu
        menuWidth: 280
        items: [{label: sx.tr("All decks"), checked: sx.did === 0}].concat(sx.decks.map(function(d) { return {label: d.name.split("::").join(" › "), checked: sx.did === d.id} }))
        onPicked: function(i) { Core.cmd({cmd: "study_stats", did: i === 0 ? 0 : sx.decks[i - 1].id}) }
    }

    Flickable {
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; top: hdr.bottom; topMargin: 12 }
        contentWidth: width
        contentHeight: col.height + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        Column {
            id: col
            x: sx.x0
            width: sx.cw
            spacing: 16

            // three numbers
            Grid {
                id: nums
                width: parent.width
                columns: sx.cw < 520 ? 1 : 3
                spacing: 12
                Repeater {
                    model: [
                        {v: "" + (sx.s.today || 0), t: sx.tr("Reviews today"), c: Theme.c.accentText},
                        {v: sx.last30.length > 0 && (sx.s.retention || 0) > 0 ? Math.round(sx.s.retention * 100) + "%" : "–", t: sx.tr("Answered right (30 days)"), c: Theme.c.success},
                        {v: "" + (sx.s.totalReviews || 0), t: sx.tr("Reviews in all"), c: Theme.c.text}
                    ]
                    delegate: StudySurface {
                        required property var modelData
                        width: (nums.width - nums.spacing * (nums.columns - 1)) / nums.columns
                        height: nums.columns === 1 ? 64 : 92
                        Column {
                            anchors.centerIn: parent
                            spacing: 4
                            Text { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.v; color: modelData.c; font.pixelSize: 28; font.weight: Font.Bold; font.family: Theme.type.family }
                            Text { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.t; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                        }
                    }
                }
            }

            // the cards by kind
            StudySurface {
                id: kinds
                width: col.width
                height: legend.y + legend.height + 16
                readonly property var parts: [
                    {n: sx.s.new || 0, c: sx.stateColor("new"), t: sx.tr("New")},
                    {n: sx.s.learning || 0, c: sx.stateColor("learn"), t: sx.tr("Learning")},
                    {n: sx.s.young || 0, c: sx.stateColor("young"), t: sx.tr("Young")},
                    {n: sx.s.mature || 0, c: sx.stateColor("mature"), t: sx.tr("Mature")},
                    {n: sx.s.suspended || 0, c: Theme.c.textDim, t: sx.tr("Suspended")}
                ]
                readonly property real total: Math.max(1, parts.reduce(function(a, p) { return a + p.n }, 0))
                Text { x: 18; y: 14; text: sx.tr("Cards"); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.DemiBold; font.family: Theme.type.family }
                Rectangle {
                    id: track
                    x: 18; y: 44; width: parent.width - 36; height: 12
                    radius: 6
                    color: Theme.c.fill3
                    clip: true
                    Row {
                        anchors.fill: parent
                        Repeater {
                            model: kinds.parts.length
                            delegate: Rectangle { required property int index; width: track.width * kinds.parts[index].n / kinds.total; height: track.height; color: kinds.parts[index].c }
                        }
                    }
                }
                Flow {
                    id: legend
                    x: 18; y: 70; width: parent.width - 36
                    spacing: 18
                    Repeater {
                        model: kinds.parts.length
                        delegate: Row {
                            required property int index
                            spacing: 6
                            Rectangle { width: 10; height: 10; radius: 5; color: kinds.parts[index].c; anchors.verticalCenter: parent.verticalCenter }
                            Text { text: kinds.parts[index].t + "  " + kinds.parts[index].n; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
                        }
                    }
                }
            }

            // bars: the last 30 days, then the next 14
            Repeater {
                model: 2
                delegate: StudySurface {
                    id: chart
                    required property int index
                    readonly property var vals: index === 0 ? sx.last30 : sx.forecast
                    readonly property color tone: index === 0 ? Theme.c.accent : Theme.c.success
                    readonly property real peak: sx.maxOf(vals)
                    width: col.width; height: 200
                    Text { x: 18; y: 14; text: chart.index === 0 ? sx.tr("Reviews, last 30 days") : sx.tr("Due, next 14 days"); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.DemiBold; font.family: Theme.type.family }
                    Text { anchors { right: parent.right; rightMargin: 18; top: parent.top; topMargin: 16 } text: sx.tr("Most in a day") + ": " + chart.peak; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                    Row {
                        id: bars
                        x: 18; y: 48; width: parent.width - 36; height: 134
                        spacing: 3
                        Repeater {
                            model: chart.vals.length
                            delegate: Item {
                                required property int index
                                readonly property real v: chart.vals[index]
                                width: (bars.width - bars.spacing * (chart.vals.length - 1)) / Math.max(1, chart.vals.length)
                                height: bars.height
                                Rectangle {
                                    anchors.bottom: parent.bottom
                                    width: parent.width
                                    height: Math.max(parent.v > 0 ? 3 : 1, parent.height * parent.v / chart.peak)
                                    radius: Math.min(3, width / 2)
                                    color: parent.v > 0 ? chart.tone : Theme.c.divider
                                }
                                ToolTip.visible: bh.hovered && v > 0; ToolTip.text: "" + v
                                HoverHandler { id: bh }
                            }
                        }
                    }
                }
            }
        }
    }
}
