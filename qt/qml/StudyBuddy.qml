import QtQuick
import QtQuick.Shapes
import Reelhouse 1.0

// The macaw on the list of decks: a line about today (cards due, the streak) and the day's chips: the review goal
// (today's cards, done / all), today's XP towards the daily goal (shared with Learn) and the streak.
StudySurface {
    id: sb
    readonly property var a: Core.study.ankiweb !== undefined ? Core.study.ankiweb : ({})
    readonly property var g: Core.study.game !== undefined && Core.study.game !== null ? Core.study.game : ({})
    readonly property int due: a.due || 0
    readonly property int reviewed: a.reviewedToday || 0
    readonly property int streak: g.streak || 0
    readonly property bool streakToday: g.streakToday === true
    readonly property bool wide: width >= 720
    readonly property color cFlame: "#ff9600"
    readonly property color cXp: "#f2b01e"
    function tr(s) { return Core.tr(Core.uiLang, s) }
    // one of the macaw's funny lines (src/mascot_lines.rs), random and never the last one
    function quip(m) { const r = Core.state("mascot:" + Core.uiLang + ":" + m); return r && r.text ? r.text : "" }
    // the funny lines for a quiet day, picked again each time the page shows (a click on the bird is handled by
    // Mascot itself: a hop and a "tap" line)
    property string doneQuip: ""
    property string emptyQuip: ""
    function freshQuips() { doneQuip = quip("buddy_done"); emptyQuip = quip("buddy_empty") }
    onVisibleChanged: if (visible) freshQuips()
    Component.onCompleted: freshQuips()

    // what the macaw says, and how it looks saying it
    readonly property var talk: {
        if (due > 0 && streak > 0 && !streakToday)
            return {mood: "surprised", text: tr("Review a few cards to keep your %1-day streak!").arg(streak)}
        if (due > 0 && reviewed === 0)
            return {mood: "wave", text: tr("%1 cards are waiting for you today.").arg(due)}
        if (due > 0)
            return {mood: "happy", text: tr("%1 cards to go. You're doing great!").arg(due)}
        if (reviewed > 0)
            return {mood: "cheer", text: doneQuip !== "" ? doneQuip : tr("All done for today! See you tomorrow.")}
        return {mood: "idle", text: emptyQuip !== "" ? emptyQuip : tr("Nothing due today. Add some new cards?")}
    }

    implicitHeight: wide ? 116 : 116 + chips.height + 12
    height: implicitHeight

    Mascot {
        id: bird
        x: 14
        y: 10
        size: 96
        mood: sb.talk.mood
        say: sb.talk.text
        bubbleSide: "right"
        animate: sb.visible
    }

    // a chip: a picture (ring or glyph), a number, a label
    component Chip: Rectangle {
        id: chip
        property string value: ""
        property string label: ""
        property color tone: Theme.c.accent
        default property alias icon: iconBox.data
        width: chipRow.implicitWidth + 22
        height: 44
        radius: Theme.layout.buttons === "capsule" ? height / 2 : Theme.shape.medium
        color: Qt.rgba(tone.r, tone.g, tone.b, 0.12)
        Row {
            id: chipRow
            anchors.centerIn: parent
            spacing: 8
            Item { id: iconBox; width: 26; height: 26; anchors.verticalCenter: parent.verticalCenter }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                Text { text: chip.value; color: chip.tone; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
                Text { text: chip.label; color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family }
            }
        }
    }

    Flow {
        id: chips
        spacing: 8
        layoutDirection: sb.wide ? Qt.RightToLeft : Qt.LeftToRight
        width: sb.wide ? sb.width - 380 : sb.width - 28
        x: sb.wide ? sb.width - width - 14 : 14
        y: sb.wide ? (sb.height - height) / 2 : bird.y + bird.height + 8
        // the daily review goal: the cards of today, done out of all of them
        Chip {
            id: goalChip
            readonly property int all: sb.reviewed + sb.due
            readonly property real frac: all > 0 ? sb.reviewed / all : 0
            tone: frac >= 1 && all > 0 ? Theme.c.success : Theme.c.accentText
            value: all > 0 ? sb.reviewed + "/" + all : "–"
            label: frac >= 1 && all > 0 ? sb.tr("Review goal done") : sb.tr("Review goal")
            Shape {
                anchors.fill: parent
                preferredRendererType: Shape.CurveRenderer
                ShapePath {
                    strokeColor: Theme.c.fill3; strokeWidth: 4; fillColor: "transparent"
                    PathAngleArc { centerX: 13; centerY: 13; radiusX: 11; radiusY: 11; startAngle: 0; sweepAngle: 360 }
                }
                ShapePath {
                    strokeColor: goalChip.tone; strokeWidth: 4; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                    PathAngleArc { centerX: 13; centerY: 13; radiusX: 11; radiusY: 11; startAngle: -90; sweepAngle: 360 * Math.max(0.001, goalChip.frac) }
                }
            }
        }
        // today's XP towards the daily goal (lessons and reviews together)
        Chip {
            tone: (sb.g.xpToday || 0) >= (sb.g.goal || 20) ? Theme.c.success : sb.cXp
            value: (sb.g.xpToday || 0) + "/" + (sb.g.goal || 20) + " XP"
            label: sb.tr("Daily goal")
            LearnGlyph { anchors.centerIn: parent; name: "zap"; size: 22; color: sb.cXp; filled: true; fillColor: Qt.rgba(0.95, 0.69, 0.12, 0.35) }
        }
        Chip {
            visible: sb.streak > 0
            tone: sb.streakToday ? sb.cFlame : Theme.c.textDim
            value: "" + sb.streak
            label: sb.tr("day streak")
            LearnGlyph { anchors.centerIn: parent; name: "flame"; size: 22; color: sb.streakToday ? sb.cFlame : Theme.c.textDim; filled: sb.streakToday; fillColor: Qt.rgba(1, 0.59, 0, 0.35) }
        }
    }
}
