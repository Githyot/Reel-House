import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Linked toggle buttons (GTK .linked): exactly one is on.
// Each part is as wide as its label plus 15 px on both sides. When `fitWidth` is given (the room the row may use)
// and the labels do not fit in it, the parts share that room evenly: parts that have an `icon` show only the icon
// (the label becomes a tooltip), the others elide their label. Portuguese labels are longer than the English ones,
// so a fixed width is never assumed.
Row {
    id: sg
    property var model: []              // [{id, label, icon?}]
    property string current: ""
    property real fitWidth: 0           // 0 = as wide as the labels want
    signal picked(string id)
    spacing: 0
    readonly property int pad: 15
    // the natural width of all the parts together
    readonly property real naturalWidth: {
        let w = 0
        for (let i = 0; i < model.length; ++i) w += Math.ceil(fm.advanceWidth(model[i].label || "")) + pad * 2
        return w
    }
    readonly property bool squeezed: fitWidth > 0 && model.length > 0 && naturalWidth > fitWidth
    readonly property real shareW: squeezed ? Math.floor(fitWidth / model.length) : 0
    FontMetrics { id: fm; font.pixelSize: Theme.type.body ? Theme.type.body.size : 14;font.weight: Font.Bold; font.family: Theme.type.family }
    Repeater {
        model: sg.model
        delegate: Rectangle {
            id: seg
            required property var modelData
            required property int index
            readonly property bool on: sg.current === modelData.id
            readonly property bool iconOnly: sg.squeezed && (modelData.icon || "") !== "" && lb.implicitWidth + sg.pad * 2 > sg.shareW
            width: sg.squeezed ? sg.shareW : lb.implicitWidth + sg.pad * 2
            height: 34
            radius: Theme.shape.control
            color: on ? Theme.c.accent : (ar.containsMouse ? Theme.c.buttonHover : Theme.c.button)
            // the ends are round, the inner corners square (per-corner radii: patches over a translucent colour showed as a darker strip)
            topLeftRadius: index === 0 ? radius : 0
            bottomLeftRadius: index === 0 ? radius : 0
            topRightRadius: index === sg.model.length - 1 ? radius : 0
            bottomRightRadius: index === sg.model.length - 1 ? radius : 0
            // a thin line between two parts that are both off (beside the accent part it only looked like a stray mark)
            Rectangle {
                visible: index > 0 && !seg.on && sg.current !== (sg.model[index - 1] || {}).id
                anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                width: 1; height: parent.height - 14
                color: Theme.c.border
            }
            Lbl {
                id: lb
                visible: !seg.iconOnly
                anchors.centerIn: parent
                width: Math.min(implicitWidth, seg.width - 16)
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
                text: seg.modelData.label
                bold: true
                color: seg.on ? Theme.c.accentFg : Theme.c.text
            }
            Icon { visible: seg.iconOnly; anchors.centerIn: parent; name: seg.modelData.icon || ""; size: 16; color: seg.on ? Theme.c.accentFg : Theme.c.text }
            MouseArea { id: ar; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sg.picked(seg.modelData.id) }
            ToolTip.visible: ar.containsMouse && (seg.iconOnly || lb.truncated)
            ToolTip.text: seg.modelData.label
        }
    }
}
