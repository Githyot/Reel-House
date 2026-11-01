import QtQuick
import Reelhouse 1.0

// A text with the look of the theme: `dim` for secondary text, `bold`, `size` in pixels.
Text {
    property bool dim: false
    property bool bold: false
    property int size: Theme.type.body ? Theme.type.body.size : 14
    color: dim ? Theme.c.textDim : Theme.c.text
    font.pixelSize: size
    font.weight: bold ? Font.Bold : Font.Normal
    // big bold labels are titles: the display face (SF Pro Display / Segoe UI Variable Display) and its tracking
    readonly property var typeRole: bold && size >= 20 ? Theme.type.title : Theme.type.body
    font.letterSpacing: typeRole && typeRole.letterSpacing !== undefined ? typeRole.letterSpacing : 0
    font.family: bold && size >= 20 ? (Theme.type.display || Theme.type.family) : Theme.type.family
}
