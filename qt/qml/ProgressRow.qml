import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A line of a popover that tells how far a long job is: what it does, "done of total", and a slim bar (libadwaita progress bar).
Item {
    id: pr
    property string label: ""
    property int done: 0
    property int total: 0
    property bool busy: false           // total not known yet: the bar moves back and forth
    property string error: ""
    property string detail: ""
    readonly property real fraction: total > 0 ? Math.min(1, done / total) : 0
    readonly property bool finished: total > 0 && done >= total
    implicitHeight: col.implicitHeight + 16
    implicitWidth: 340
    Column {
        id: col
        x: 14; y: 8
        width: parent.width - 28
        spacing: 6
        Item {
            width: parent.width
            height: head.implicitHeight
            Lbl { id: head; text: pr.label; bold: true; size: 13; width: parent.width - cnt.implicitWidth - 8; elide: Text.ElideRight }
            Lbl {
                id: cnt
                anchors.right: parent.right
                dim: true; size: 13
                text: pr.error !== "" ? "" : (pr.total > 0 ? (pr.finished ? "100%" : pr.done + " / " + pr.total + "  ·  " + Math.round(pr.fraction * 100) + "%") : "")
            }
        }
        Rectangle {
            width: parent.width
            height: 6
            radius: 3
            color: Theme.c.chip
            clip: true
            Rectangle {
                visible: pr.error === ""
                height: parent.height
                radius: 3
                color: pr.finished ? Theme.c.success : Theme.c.accent
                width: pr.total > 0 ? parent.width * pr.fraction : parent.width * 0.3
                x: 0
                Behavior on width { enabled: pr.total > 0; NumberAnimation { duration: 180 } }
                SequentialAnimation on x {
                    running: pr.visible && pr.total <= 0 && pr.busy && pr.error === ""
                    loops: Animation.Infinite
                    NumberAnimation { to: pr.width * 0.7; duration: 900; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 0; duration: 900; easing.type: Easing.InOutQuad }
                }
            }
            Rectangle { visible: pr.error !== ""; anchors.fill: parent; radius: 3; color: Theme.c.error }
        }
        Lbl {
            visible: text !== ""
            width: parent.width
            text: pr.error !== "" ? pr.error : pr.detail
            dim: pr.error === ""
            color: pr.error !== "" ? Theme.c.error : Theme.c.textDim
            size: 12
            wrapMode: Text.Wrap
        }
    }
}
