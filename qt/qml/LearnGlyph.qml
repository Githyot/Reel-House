import QtQuick
import QtQuick.Shapes
import Reelhouse 1.0
import "LearnGlyphs.js" as Glyphs

// The pictures of the Learn area (streak flame, hearts, gems, lock, trophy…), drawn as outlines from Lucide paths
// (https://lucide.dev, ISC licence) so no picture files are needed. `filled` paints the inside too.
Item {
    id: g
    property string name: ""
    property color color: Theme.c.text
    property int size: 20
    property bool filled: false
    property color fillColor: color
    property real stroke: 2
    implicitWidth: size
    implicitHeight: size
    Shape {
        width: 24
        height: 24
        scale: g.size / 24
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: g.filled ? g.fillColor : "transparent"
            strokeColor: g.color
            strokeWidth: g.stroke
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: Glyphs.paths[g.name] || "" }
        }
    }
}
