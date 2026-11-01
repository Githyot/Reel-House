import QtQuick
import Reelhouse 1.0

// The controller hints in controller mode: what each button does on this screen, with glyphs of the controller
// plugged in (PlayStation or Xbox; anything else shows Xbox letters), drawn in the theme's own colours, corners and
// font. Two forms:
// - `vertical` (the legend at the bottom of the fixed sidebar): a compact group like the sidebar's other groups, the
//   pairs "glyph label" in two columns (one when the sidebar is too narrow);
// - a floating pill (Main.qml puts it where it covers no content: bottom centre on normal pages, the top right under
//   the header in readers and reviews), the pairs in one row, or in `columns` per row.
// `idle` (no button pressed for a few seconds) dims the pill; any press brings it back. It never disappears, so what
// the buttons do can always be read.
// `hints`: [{button, text}], button = "A", "B", "X", "Y", "LB", "RB", "LT", "RT", "L3", "R3", "RS", "Start", "Select",
// "Pad".
Rectangle {
    id: ph
    property string kind: "xbox"
    property var hints: []
    property bool vertical: false
    property bool idle: false
    // pairs per row of the pill; 0 = all in one row
    property int columns: 0
    readonly property bool ps: kind === "ps4" || kind === "ps5"
    readonly property string look: Theme.look === "apple" || Theme.look === "windows" || Theme.look === "android" ? Theme.look : "gnome"
    readonly property int glyph: vertical ? 16 : 18
    readonly property int pad: vertical ? 10 : 12
    // the sidebar legend: two columns when each pair fits half the width
    readonly property int sideCols: width - 2 * pad >= 2 * 92 + 10 ? 2 : 1
    implicitWidth: grid.implicitWidth + 2 * pad
    implicitHeight: grid.implicitHeight + (vertical ? 16 : 12)
    width: implicitWidth
    height: implicitHeight
    radius: vertical
        ? (look === "windows" ? (Theme.shape.control || 4) : (Theme.shape.card !== undefined ? Theme.shape.card : 12))
        : (look === "apple" ? height / 2 : look === "android" ? Math.min(height / 2, 16) : (Theme.shape.popover !== undefined ? Theme.shape.popover : 12))
    // the sidebar: the card colour of its other groups (Android: a tonal surface without a line); the pill: the
    // popover colour
    color: vertical ? (look === "android" ? Theme.c.fill3 : (Theme.c.card !== undefined ? Theme.c.card : Theme.c.fill3))
                    : Theme.c.popover
    border.width: look === "android" && vertical ? 0 : 1
    border.color: Theme.c.border
    opacity: idle && !vertical ? 0.35 : 1
    Behavior on opacity { NumberAnimation { duration: 400 } }
    Grid {
        id: grid
        x: ph.pad
        anchors.verticalCenter: parent.verticalCenter
        columns: ph.vertical ? ph.sideCols : (ph.columns > 0 ? ph.columns : Math.max(1, ph.hints.length))
        rowSpacing: ph.vertical ? 5 : 6
        columnSpacing: ph.vertical ? 10 : 14
        Repeater {
            model: ph.hints
            delegate: Row {
                id: hint
                required property var modelData
                spacing: 6
                // the sidebar: every cell the same width, the label cut when long
                readonly property real cellW: ph.vertical ? (ph.width - 2 * ph.pad - (ph.sideCols - 1) * grid.columnSpacing) / ph.sideCols : -1
                PadGlyph { id: gl; button: hint.modelData.button; ps: ph.ps; plain: true; size: ph.glyph; anchors.verticalCenter: parent.verticalCenter }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: hint.cellW > 0 ? Math.min(implicitWidth, hint.cellW - gl.width - hint.spacing) : implicitWidth
                    elide: Text.ElideRight
                    text: hint.modelData.text
                    color: ph.vertical ? Theme.c.textDim : Theme.c.text
                    font.pixelSize: 12
                    font.family: Theme.type.family
                }
            }
        }
    }
}
