import QtQuick
import QtQuick.Shapes
import Reelhouse 1.0
import "MaterialIcons.js" as Material
import "LucideIcons.js" as Lucide

// A symbolic icon drawn from an SVG path (24x24 grid), tinted with `color`.
Item {
    id: ic
    property string name: ""
    property color color: Theme.c.text
    property int size: 20
    // Solid shape: outline icons (Lucide) are filled too, with `fillColor` (the favourite star when it is on).
    property bool filled: false
    property color fillColor: color
    // Always the solid symbol, in every theme (the profile pictures: a white symbol on a colour).
    property bool solid: false
    // "staroutline" is the hollow star of the filled set; the outline set draws its own star for it.
    readonly property string key: name === "staroutline" && Theme.icons === "lucide" ? "star" : name
    readonly property bool stroked: !solid && Theme.icons === "lucide" && Lucide.paths[key] !== undefined
    implicitWidth: size
    implicitHeight: size
    Shape {
        width: 24
        height: 24
        scale: ic.size / 24
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer
        // Lucide (Apple theme): outlines with round ends; GNOME: the filled symbols above
        ShapePath {
            fillColor: ic.stroked ? (ic.filled ? ic.fillColor : "transparent") : (ic.filled ? ic.fillColor : ic.color)
            strokeColor: ic.stroked ? ic.color : "transparent"
            strokeWidth: ic.stroked ? Theme.iconStroke : 0
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: ic.stroked ? Lucide.paths[ic.key] : (Material.paths[ic.key] || "") }
        }
    }
}
