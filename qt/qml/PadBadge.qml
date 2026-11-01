import QtQuick
import Reelhouse 1.0

// A connected game controller: the pad drawing in the colour of its family and the model name.
Rectangle {
    id: pb
    property string kind: "pad"            // xbox, ps4, ps5, pad
    property string name: ""
    property bool compact: false           // header indicator: icon only
    readonly property var look: ({
        xbox: {c: "#107c10", fg: "#ffffff", label: "Xbox"},
        ps4:  {c: "#003791", fg: "#ffffff", label: "DualShock 4"},
        ps5:  {c: "#f2f2f2", fg: "#1f1f1f", label: "DualSense"},
        pad:  {c: Theme.c.accent, fg: Theme.c.accentFg || "#ffffff", label: Core.tr(Core.uiLang, "Controller")}
    })
    readonly property var lk: look[kind] || look.pad
    // the colour of the family, see-through: more in themes with glass panels, a little in the others
    function tint(c) { const q = Qt.color(c); return Qt.rgba(q.r, q.g, q.b, Theme.glass ? 0.38 : 0.78) }
    implicitWidth: compact ? 30 : row.implicitWidth + 16
    implicitHeight: compact ? 30 : 30
    radius: compact ? 15 : (Theme.shape.pill > 100 ? height / 2 : Theme.shape.control)
    color: compact ? "transparent" : tint(lk.c)
    Row {
        id: row
        anchors.centerIn: parent
        spacing: 6
        Rectangle {
            width: 22; height: 22; radius: 11
            color: pb.compact ? pb.tint(pb.lk.c) : "transparent"
            anchors.verticalCenter: parent.verticalCenter
            Icon { anchors.centerIn: parent; name: "ds4"; size: pb.compact ? 15 : 18; color: pb.lk.fg }
        }
        Text {
            visible: !pb.compact
            anchors.verticalCenter: parent.verticalCenter
            text: pb.lk.label
            color: pb.lk.fg
            font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family
        }
    }
}
