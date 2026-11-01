import QtQuick
import QtQuick.Shapes
import Reelhouse 1.0

// The stopwatch symbol of the Pomodoro button (the icon sets have none), drawn like the Lucide "timer" on a 24 px grid.
// With `progress` (0..1, what is left of the phase) the dial fills as a small pie in `tone`, so the button shows the
// running phase at a glance.
Item {
    id: g
    property int size: 18
    property color color: Theme.c.text
    property color tone: color
    property real progress: -1
    implicitWidth: size
    implicitHeight: size
    readonly property real k: size / 24

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        // what is left of the phase, as a pie inside the dial
        ShapePath {
            strokeColor: "transparent"
            fillColor: g.progress > 0 ? g.tone : "transparent"
            startX: 12 * g.k; startY: 14 * g.k
            PathAngleArc { centerX: 12 * g.k; centerY: 14 * g.k; radiusX: 5.4 * g.k; radiusY: 5.4 * g.k; startAngle: -90; sweepAngle: 360 * Math.max(0, Math.min(1, g.progress)); moveToStart: false }
            PathLine { x: 12 * g.k; y: 14 * g.k }
        }
        // the dial
        ShapePath {
            strokeColor: g.color; strokeWidth: 2 * g.k; fillColor: "transparent"; capStyle: ShapePath.RoundCap
            PathAngleArc { centerX: 12 * g.k; centerY: 14 * g.k; radiusX: 8 * g.k; radiusY: 8 * g.k; startAngle: 0; sweepAngle: 360 }
        }
        // the button on top and, when idle, the hand
        ShapePath {
            strokeColor: g.color; strokeWidth: 2 * g.k; fillColor: "transparent"; capStyle: ShapePath.RoundCap
            startX: 10 * g.k; startY: 2 * g.k
            PathLine { x: 14 * g.k; y: 2 * g.k }
            PathMove { x: 12 * g.k; y: 14 * g.k }
            PathLine { x: g.progress > 0 ? 12 * g.k : 15 * g.k; y: g.progress > 0 ? 14 * g.k : 11 * g.k }
        }
    }
}
