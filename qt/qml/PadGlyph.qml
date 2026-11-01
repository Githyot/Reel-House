import QtQuick
import QtQuick.Shapes
import Reelhouse 1.0

// A button of a controller, drawn here (no logo files): PlayStation ✕ ○ □ △ or Xbox A B X Y in their colours.
// `button` is the position name the core uses: "A" (bottom), "B" (right), "X" (left), "Y" (top), or "LB", "RB",
// "Start", "Pad" (D-pad / stick).
Rectangle {
    id: g
    property string button: "A"
    property bool ps: false
    property real size: 22
    // `plain`: no key box (the themed legends of PadHints): face buttons are their coloured symbol alone, the other
    // buttons a small label on a light chip of the theme
    property bool plain: false
    readonly property bool face: ["A", "B", "X", "Y"].indexOf(button) >= 0
    readonly property bool light: plain && Theme.scheme === "light"
    readonly property var xboxCol: light ? ({A: "#2f8a2f", B: "#c23b27", X: "#2366b5", Y: "#a67c0c"}) : ({A: "#5cb85c", B: "#e0533d", X: "#3d8fe0", Y: "#e0b13d"})
    readonly property var psCol: light ? ({A: "#3563b8", B: "#c43d3d", X: "#a54ea1", Y: "#178a6b"}) : ({A: "#7fa7e8", B: "#e86b6b", X: "#d48fd0", Y: "#4fc3a1"})
    readonly property color mark: face ? (ps ? psCol[button] : xboxCol[button]) : (plain ? Theme.c.text : "#ffffff")
    readonly property real w: size * (plain ? 0.62 : 0.46)
    implicitWidth: face ? size : Math.max(plain ? size : size * 1.6, lbl.implicitWidth + (plain ? 8 : 10))
    implicitHeight: size
    radius: face ? size / 2 : (plain ? Math.min(size / 3, Theme.shape.small !== undefined ? Theme.shape.small : 6) : size / 3)
    color: plain ? (face ? "transparent" : Theme.c.fill3) : "#2a2d33"
    border.width: plain ? 0 : 1
    border.color: "#555a63"
    Text {
        id: lbl
        anchors.centerIn: parent
        visible: !g.ps || !g.face
        text: g.face ? g.button
            : g.button === "LB" ? (g.ps ? "L1" : "LB")
            : g.button === "RB" ? (g.ps ? "R1" : "RB")
            : g.button === "LT" ? (g.ps ? "L2" : "LT")
            : g.button === "RT" ? (g.ps ? "R2" : "RT")
            : g.button === "Start" ? (g.ps ? "OPT" : "☰")
            : g.button === "Select" ? (g.ps ? "SHARE" : "⧉")
            : g.button === "Touch" ? "TOUCH"
            : g.button === "Pad" ? "✥" : g.button
        color: g.mark
        font.pixelSize: g.face ? g.size * (g.plain ? 0.8 : 0.6) : (g.button === "Pad" && g.plain ? g.size * 0.8 : g.size * (g.plain ? 0.52 : 0.48))
        font.weight: Font.Bold
        font.family: Theme.type.family
    }
    // PlayStation symbols as lines: cross (bottom), circle (right), square (left), triangle (top)
    Shape {
        visible: g.ps && g.face
        anchors.centerIn: parent
        width: g.w
        height: g.w
        ShapePath {
            strokeColor: g.mark
            strokeWidth: Math.max(1.5, g.size * 0.09)
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg {
                path: g.button === "A" ? ("M0,0 L" + g.w + "," + g.w + " M" + g.w + ",0 L0," + g.w)
                    : g.button === "B" ? ("M" + g.w / 2 + ",0 A" + g.w / 2 + "," + g.w / 2 + " 0 1,1 " + (g.w / 2 - 0.01) + ",0 Z")
                    : g.button === "X" ? ("M0,0 L" + g.w + ",0 L" + g.w + "," + g.w + " L0," + g.w + " Z")
                    : ("M" + g.w / 2 + ",0 L" + g.w + "," + g.w * 0.87 + " L0," + g.w * 0.87 + " Z")
            }
        }
    }
}
