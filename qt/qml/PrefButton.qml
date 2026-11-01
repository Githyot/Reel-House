import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A row with a button at the right (action_button of the GTK preferences).
PrefRow {
    id: pb
    property string label: ""
    property string kind: "plain"
    signal activated()
    // a long label is elided at half the row (the title keeps its room) and shown whole in the tooltip
    FlatButton { anchors.verticalCenter: parent.verticalCenter; width: Math.min(implicitWidth, pb.width * 0.5); kind: pb.kind; text: pb.label; implicitHeight: 34; ToolTip.visible: hovered && width < implicitWidth; ToolTip.text: pb.label; onClicked: pb.activated() }
}
