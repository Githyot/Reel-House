import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// Study: the time spent studying. Totals of today / this week / month / year and the streak, a report of one
// period (Day: minutes per hour, Week and Month: per day, Year: per month) with the comparison against the previous
// period up to the same point, the split by activity, and the Pomodoro timer beside it.
// What counts (the core decides, see controller_time.rs): the Study mode in use, Learn lessons, flashcard reviews,
// the quiz cards of the player, and Pomodoro focus sessions in any mode.
Item {
    id: tp
    signal switchTo(string view)
    property var st: ({})
    property string period: "week"
    property int offset: 0
    readonly property var v: st.view ? st.view : ({})
    readonly property var bars: v.bars ? v.bars : []
    readonly property real cw: Math.min(1000, width - Theme.spacing.page * 2)
    readonly property real x0: (width - cw) / 2
    readonly property bool wide: cw >= 820
    readonly property var loc: Qt.locale(String(Core.uiLang).replace("-", "_"))
    readonly property color cBlue: Theme.scheme === "light" ? "#1c71d8" : "#78aeed"
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function tz() { return -new Date().getTimezoneOffset() }
    function refresh() {
        Core.cmd({cmd: "studytime_view", period: tp.period, offset: tp.offset, tz: tp.tz()})
        tp.st = Core.state("studytime")
    }
    function show(p, o) { tp.period = p; tp.offset = Math.min(0, o); refresh() }
    onVisibleChanged: if (visible) refresh()
    // the numbers of today keep growing while the page is open
    Timer { interval: 30000; repeat: true; running: tp.visible; onTriggered: tp.refresh() }

    // ---------- formatting ----------
    function dur(secs) {
        const m = Math.round((secs || 0) / 60)
        if (m < 60) return m + " min"
        return Math.floor(m / 60) + " h" + (m % 60 > 0 ? " " + (m % 60) + " min" : "")
    }
    function axisDur(mins) { return mins >= 60 ? (Math.round(mins / 6) / 10) + " h" : Math.round(mins) + " min" }
    function dateOf(key) { const a = String(key).split("-"); return new Date(Number(a[0]), Number(a[1]) - 1, Number(a[2])) }
    function dayName(key) { return tp.loc.dayName(dateOf(key).getDay(), Locale.ShortFormat) }
    function longDate(key) { return dateOf(key).toLocaleDateString(tp.loc, "ddd d MMM") }
    function hourName(h) { return ("0" + h).slice(-2) + ":00" }
    function periodTitle() {
        if (!tp.v.start) return ""
        const names = {day: ["Today", "Yesterday"], week: ["This week", "Last week"], month: ["This month", "Last month"], year: ["This year", "Last year"]}
        if (tp.offset === 0 || tp.offset === -1) return tr(names[tp.period][-tp.offset])
        const a = dateOf(tp.v.start), b = dateOf(tp.v.end)
        if (tp.period === "day") return a.toLocaleDateString(tp.loc, "dddd d MMMM yyyy")
        if (tp.period === "week") return a.toLocaleDateString(tp.loc, "d MMM") + " – " + b.toLocaleDateString(tp.loc, "d MMM yyyy")
        if (tp.period === "month") return a.toLocaleDateString(tp.loc, "MMMM yyyy")
        return "" + a.getFullYear()
    }
    function compareText() {
        const now = tp.offset === 0
        if (tp.period === "day") return now ? tr("vs. yesterday at this time") : tr("vs. the day before")
        if (tp.period === "week") return now ? tr("vs. last week so far") : tr("vs. the week before")
        if (tp.period === "month") return now ? tr("vs. last month so far") : tr("vs. the month before")
        return now ? tr("vs. last year so far") : tr("vs. the year before")
    }
    function barName(i) {
        const b = tp.bars[i]
        if (!b) return ""
        if (tp.period === "day") return hourName(b.hour)
        if (tp.period === "year") return tp.loc.monthName((b.month || 1) - 1, Locale.LongFormat)
        return longDate(b.date)
    }
    // the label under a bar, or "" when the bars are too thin for every one to have a label
    function barLabel(i, slot) {
        const b = tp.bars[i]
        if (!b) return ""
        if (tp.period === "day") return i % (slot >= 22 ? 3 : 6) === 0 ? "" + b.hour : ""
        if (tp.period === "week") return dayName(b.date)
        if (tp.period === "year") { const n = tp.loc.monthName((b.month || 1) - 1, Locale.ShortFormat); return slot >= 34 ? n : n.charAt(0) }
        const d = dateOf(b.date).getDate()
        return slot >= 22 || d === 1 || d % 5 === 0 ? "" + d : ""
    }
    // a round top for the chart, in minutes (1, 2, 2.5, 5 × 10^k)
    function niceTop(mins) {
        const x = Math.max(5, mins)
        const p = Math.pow(10, Math.floor(Math.log(x) / Math.LN10))
        const steps = [1, 2, 2.5, 5, 10]
        for (let i = 0; i < steps.length; ++i) if (steps[i] * p >= x) return steps[i] * p
        return 10 * p
    }
    readonly property var acts: [
        {id: "review", t: tr("Card reviews"), c: Theme.c.success},
        {id: "lesson", t: tr("Lessons"), c: tp.cBlue},
        {id: "quiz", t: tr("Quizzes"), c: Theme.c.warning},
        {id: "pomodoro", t: tr("Pomodoro focus"), c: Theme.c.error},
        {id: "study", t: tr("Other study"), c: Theme.c.textDim}
    ]
    function splitOf(id) { const s = tp.v.split || []; for (let i = 0; i < s.length; ++i) if (s[i].id === id) return s[i].secs || 0; return 0 }

    FileDialog {
        id: csvDlg
        title: tp.tr("Export study time")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "csv"
        nameFilters: ["CSV (*.csv)"]
        onAccepted: Core.cmd({cmd: "studytime_export", path: Core.localPath(selectedFile.toString())})
    }

    // ---------- the bar on top: the export (Decks / Learn / Time are in the left sidebar) ----------
    Item {
        id: topBar
        x: tp.x0; y: 4
        width: tp.cw; height: 40
        FlatButton {
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            kind: "plain"; glyph: "download"; text: tp.cw < 520 ? "" : tp.tr("Export")
            ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: tp.tr("Export study time (CSV)")
            onClicked: csvDlg.open()
        }
    }

    Flickable {
        anchors { left: parent.left; right: parent.right; top: topBar.bottom; bottom: parent.bottom; topMargin: 8 }
        contentWidth: width
        contentHeight: col.height + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        Column {
            id: col
            x: tp.x0
            width: tp.cw
            spacing: 16

            // ---------- the totals ----------
            Grid {
                id: tiles
                width: parent.width
                columns: tp.cw < 560 ? 2 : 5
                spacing: 12
                Repeater {
                    model: [
                        {v: tp.dur(tp.st.today), t: tp.tr("Today"), p: "day"},
                        {v: tp.dur(tp.st.week), t: tp.tr("This week"), p: "week"},
                        {v: tp.dur(tp.st.month), t: tp.tr("This month"), p: "month"},
                        {v: tp.dur(tp.st.year), t: tp.tr("This year"), p: "year"},
                        {v: "" + (tp.st.streak || 0), t: (tp.st.streak || 0) === 1 ? tp.tr("day in a row") : tp.tr("days in a row"), p: ""}
                    ]
                    delegate: StudySurface {
                        id: tile
                        required property var modelData
                        required property int index
                        width: (tiles.width - tiles.spacing * (tiles.columns - 1)) / tiles.columns
                        height: 84
                        hovered: tileArea.containsMouse && modelData.p !== ""
                        Column {
                            anchors.centerIn: parent
                            spacing: 4
                            Text {
                                anchors.horizontalCenter: parent.horizontalCenter
                                text: tile.modelData.v
                                color: tile.index === 4 ? Theme.c.warningText : (tile.modelData.p === tp.period && tp.offset === 0 ? Theme.c.accentText : Theme.c.text)
                                font.pixelSize: 24; font.weight: Font.Bold; font.family: Theme.type.family
                            }
                            Text { anchors.horizontalCenter: parent.horizontalCenter; text: tile.modelData.t; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                        }
                        MouseArea {
                            id: tileArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: tile.modelData.p !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: if (tile.modelData.p !== "") tp.show(tile.modelData.p, 0)
                        }
                        ToolTip.visible: tile.index === 4 && tileArea.containsMouse
                        ToolTip.text: tp.tr("Longest streak") + ": " + (tp.st.longest || 0)
                    }
                }
            }

            Grid {
                id: twin
                width: parent.width
                columns: tp.wide ? 2 : 1
                spacing: 16
                readonly property real sideW: tp.wide ? 340 : width

                // ---------- the report ----------
                StudySurface {
                    id: report
                    width: tp.wide ? twin.width - twin.sideW - twin.spacing : twin.width
                    height: rcol.height + 36
                    Column {
                        id: rcol
                        x: 18; y: 18
                        width: parent.width - 36
                        spacing: 14

                        Flow {
                            width: parent.width
                            spacing: 10
                            Segmented {
                                model: [{id: "day", label: tp.tr("Day")}, {id: "week", label: tp.tr("Week")}, {id: "month", label: tp.tr("Month")}, {id: "year", label: tp.tr("Year")}]
                                current: tp.period
                                fitWidth: parent.width
                                onPicked: function(id) { tp.show(id, 0) }
                            }
                            Row {
                                spacing: 2
                                FlatButton {
                                    kind: "flat"; glyph: "left"; implicitWidth: 36; implicitHeight: 34
                                    ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: tp.tr("Earlier")
                                    onClicked: tp.show(tp.period, tp.offset - 1)
                                }
                                FlatButton {
                                    kind: "flat"; glyph: "right"; implicitWidth: 36; implicitHeight: 34
                                    enabled: tp.offset < 0
                                    ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: tp.tr("Later")
                                    onClicked: tp.show(tp.period, tp.offset + 1)
                                }
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 2
                            Lbl { width: parent.width; text: tp.periodTitle(); bold: true; size: 20; elide: Text.ElideRight }
                            Text {
                                width: parent.width
                                wrapMode: Text.Wrap
                                text: tp.dur(tp.v.total) + "  ·  " + (tp.v.change === null || tp.v.change === undefined ? tp.tr("nothing to compare with") : ((tp.v.change > 0 ? "+" : "") + tp.v.change + "% " + tp.compareText()))
                                color: tp.v.change > 0 ? Theme.c.successText : (tp.v.change < 0 ? Theme.c.errorText : Theme.c.textDim)
                                font.pixelSize: 14; font.family: Theme.type.family
                            }
                        }

                        // the bars, with two guide lines
                        Item {
                            id: chart
                            width: parent.width
                            height: 200
                            readonly property real labelW: 48
                            readonly property real plotH: height - 22
                            readonly property real peak: { let m = 0; for (let i = 0; i < tp.bars.length; ++i) m = Math.max(m, tp.bars[i].secs || 0); return m }
                            readonly property real topMin: tp.niceTop(peak / 60)
                            readonly property real slot: tp.bars.length > 0 ? (width - labelW) / tp.bars.length : 1
                            Repeater {
                                model: 3
                                delegate: Item {
                                    required property int index
                                    y: chart.plotH * index / 2
                                    width: chart.width; height: 1
                                    Rectangle { x: chart.labelW; width: parent.width - chart.labelW; height: 1; color: Theme.c.divider; opacity: index === 2 ? 1 : 0.6 }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: chart.labelW - 8
                                        horizontalAlignment: Text.AlignRight
                                        text: index === 2 ? "0" : tp.axisDur(chart.topMin * (2 - index) / 2)
                                        color: Theme.c.textDim
                                        font.pixelSize: 11; font.family: Theme.type.family
                                    }
                                }
                            }
                            Repeater {
                                model: tp.bars.length
                                delegate: Item {
                                    id: bar
                                    required property int index
                                    readonly property var b: tp.bars[index] || ({})
                                    x: chart.labelW + index * chart.slot
                                    width: chart.slot
                                    height: chart.height
                                    HoverHandler { id: barHover }
                                    Rectangle {
                                        readonly property real gap: Math.max(1, Math.min(8, chart.slot * 0.25))
                                        x: gap / 2
                                        width: Math.max(1, chart.slot - gap)
                                        height: (bar.b.secs || 0) > 0 ? Math.max(2, chart.plotH * Math.min(1, (bar.b.secs || 0) / 60 / chart.topMin)) : 0
                                        y: chart.plotH - height
                                        radius: Math.min(4, width / 2)
                                        color: Theme.c.accent
                                        opacity: bar.b.current === true || barHover.hovered ? 1 : 0.55
                                        Behavior on height { NumberAnimation { duration: Theme.motion.fast } }
                                    }
                                    Text {
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        y: chart.plotH + 5
                                        text: tp.barLabel(bar.index, chart.slot)
                                        color: bar.b.current === true ? Theme.c.accentText : Theme.c.textDim
                                        font.pixelSize: 11; font.weight: bar.b.current === true ? Font.Bold : Font.Normal; font.family: Theme.type.family
                                    }
                                    ToolTip.visible: barHover.hovered
                                    ToolTip.text: tp.barName(bar.index) + ": " + tp.dur(bar.b.secs)
                                }
                            }
                        }

                        // the numbers of the period
                        Flow {
                            id: nums
                            width: parent.width
                            spacing: 22
                            Repeater {
                                model: tp.period === "day" ? [
                                    {v: tp.dur(tp.v.total), t: tp.tr("Total")},
                                    {v: tp.v.best ? tp.hourName(tp.v.best.hour) : "–", t: tp.tr("Best hour")},
                                    {v: "" + (tp.v.pomos || 0), t: tp.tr("Pomodoros")}
                                ] : [
                                    {v: tp.dur(tp.v.total), t: tp.tr("Total")},
                                    {v: tp.dur(tp.v.avg), t: tp.tr("Average per day")},
                                    {v: tp.v.best ? tp.longDate(tp.v.best.date) : "–", t: tp.v.best ? tp.tr("Best day") + " · " + tp.dur(tp.v.best.secs) : tp.tr("Best day")},
                                    {v: (tp.v.studied || 0) + "/" + (tp.v.elapsed || 0), t: tp.tr("Days studied")},
                                    {v: "" + (tp.v.pomos || 0), t: tp.tr("Pomodoros")}
                                ]
                                delegate: Column {
                                    required property var modelData
                                    spacing: 2
                                    Text { text: modelData.v; color: Theme.c.text; font.pixelSize: 18; font.weight: Font.Bold; font.family: Theme.type.family }
                                    Text { text: modelData.t; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                                }
                            }
                        }

                        Rectangle { width: parent.width; height: 1; color: Theme.c.divider }

                        // ---------- by activity ----------
                        Lbl { text: tp.tr("By activity"); bold: true; size: 15 }
                        Rectangle {
                            id: track
                            width: parent.width; height: 12
                            radius: 6
                            color: Theme.c.fill3
                            clip: true
                            readonly property real total: Math.max(1, tp.v.total || 0)
                            Row {
                                anchors.fill: parent
                                Repeater {
                                    model: tp.acts.length
                                    delegate: Rectangle { required property int index; width: track.width * tp.splitOf(tp.acts[index].id) / track.total; height: track.height; color: tp.acts[index].c }
                                }
                            }
                        }
                        Grid {
                            id: legend
                            width: parent.width
                            columns: width < 420 ? 1 : 2
                            columnSpacing: 24
                            rowSpacing: 8
                            Repeater {
                                model: tp.acts.length
                                delegate: Item {
                                    id: lg
                                    required property int index
                                    readonly property real secs: tp.splitOf(tp.acts[index].id)
                                    width: (legend.width - legend.columnSpacing * (legend.columns - 1)) / legend.columns
                                    height: 22
                                    Rectangle { width: 10; height: 10; radius: 5; color: tp.acts[lg.index].c; anchors.verticalCenter: parent.verticalCenter }
                                    Text {
                                        x: 18; anchors.verticalCenter: parent.verticalCenter
                                        width: parent.width - 18 - val.width - 8
                                        elide: Text.ElideRight
                                        text: tp.acts[lg.index].t
                                        color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family
                                    }
                                    Text {
                                        id: val
                                        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                                        text: tp.dur(lg.secs) + ((tp.v.total || 0) > 0 ? "  " + Math.round(lg.secs * 100 / tp.v.total) + "%" : "")
                                        color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family
                                    }
                                }
                            }
                        }
                    }
                }

                // ---------- the Pomodoro ----------
                StudySurface {
                    id: pomoCard
                    width: twin.sideW
                    height: pomo.implicitHeight + 36
                    PomodoroWidget { id: pomo; x: 18; y: 18; width: parent.width - 36 }
                }
            }

            Text {
                width: parent.width
                wrapMode: Text.Wrap
                text: tp.tr("Counted: the Study mode while you use it, Learn lessons, flashcard reviews, the quiz cards of the player and Pomodoro focus sessions. Two minutes without activity pause the count.")
                color: Theme.c.textDim
                font.pixelSize: 12; font.family: Theme.type.family
            }
        }
    }
}
