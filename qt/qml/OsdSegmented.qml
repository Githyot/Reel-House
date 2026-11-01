import QtQuick
import Reelhouse 1.0
import "Osd.js" as Osd

// Options side by side, one of them on (subtitle alignment, karaoke letters, speed presets…):
//   GNOME: an AdwToggleGroup (a trough with the chosen toggle raised)   Windows: a 4 px group with the accent pill under the chosen one
//   Android: M3 outlined segmented buttons (chosen = secondary container with a check)   Apple: a capsule with a sliding thumb
// options: [[id, label]]; labels are translated here.
Item {
    id: sg
    property var options: []
    property string current: ""
    property bool translate: true
    signal chosen(string id)
    readonly property var o: Osd.t(Theme.layout.player)
    readonly property string kind: o.seg
    readonly property int n: Math.max(1, options.length)
    readonly property int idx: { for (let i = 0; i < options.length; i++) if (String(options[i][0]) === current) return i; return -1 }
    readonly property real pad: kind === "m3" ? 0 : (kind === "fluent" ? 2 : 3)
    readonly property real cellW: (width - pad * 2) / n

    implicitWidth: 240
    implicitHeight: kind === "m3" ? 38 : (kind === "fluent" ? 32 : 32)

    // the trough / outline
    Rectangle {
        anchors.fill: parent
        radius: sg.kind === "fluent" ? 4 : (sg.kind === "adw" ? 9 : height / 2)
        color: sg.kind === "m3" ? "transparent" : (sg.kind === "fluent" ? "#0bffffff" : "#1fffffff")
        border.width: sg.kind === "m3" || sg.kind === "fluent" ? 1 : 0
        border.color: sg.kind === "m3" ? "#938f99" : "#14ffffff"
    }
    // the chosen one: a raised toggle (GNOME), a capsule thumb (Apple) or a light cell (Windows) that slides
    Rectangle {
        visible: sg.idx >= 0 && sg.kind !== "m3"
        x: sg.pad + Math.max(0, sg.idx) * sg.cellW
        y: sg.pad
        width: sg.cellW
        height: parent.height - sg.pad * 2
        radius: sg.kind === "fluent" ? 3 : (sg.kind === "adw" ? 6 : height / 2)
        color: sg.kind === "fluent" ? "#15ffffff" : (sg.kind === "glass" ? "#4dffffff" : "#3dffffff")
        Behavior on x { NumberAnimation { duration: Theme.motion.normal || 200; easing.type: Easing.OutCubic } }
        Rectangle {
            visible: sg.kind === "fluent"
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 2 }
            width: 16; height: 3; radius: 1.5
            color: Theme.c.accent
        }
    }
    Row {
        x: sg.pad
        y: sg.pad
        Repeater {
            model: sg.options.length
            delegate: Item {
                id: cell
                required property int index
                readonly property var opt: sg.options[cell.index] || ["", ""]
                readonly property bool on: cell.index === sg.idx
                width: sg.cellW
                height: sg.height - sg.pad * 2
                // Material: the chosen segment is filled; dividers between segments
                Rectangle {
                    visible: sg.kind === "m3" && cell.on
                    anchors.fill: parent
                    anchors.margins: 1
                    radius: cell.index === 0 || cell.index === sg.n - 1 ? height / 2 : 0
                    color: "#4a4458"
                    // square off the inner side of the end segments
                    Rectangle { visible: sg.n > 1 && (cell.index === 0 || cell.index === sg.n - 1); x: cell.index === 0 ? parent.width / 2 : 0; width: parent.width / 2; height: parent.height; color: parent.color }
                }
                Rectangle { visible: sg.kind === "m3" && cell.index > 0; width: 1; height: parent.height; color: "#938f99" }
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: sg.kind === "m3" ? 1 : 0
                    radius: sg.kind === "fluent" ? 3 : height / 2
                    color: !cell.on && cellA.containsMouse ? (sg.kind === "m3" ? "#14e6e0e9" : "#12ffffff") : "transparent"
                }
                Row {
                    anchors.centerIn: parent
                    spacing: 6
                    Icon { visible: sg.kind === "m3" && cell.on; name: "check"; size: 16; color: "#e8def8"; anchors.verticalCenter: parent.verticalCenter }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, cell.width - (sg.kind === "m3" && cell.on ? 30 : 8))
                        elide: Text.ElideRight
                        text: sg.translate ? Core.tr(Core.uiLang, String(cell.opt[1])) : String(cell.opt[1])
                        color: sg.kind === "m3" ? (cell.on ? "#e8def8" : "#e6e0e9") : (cell.on || sg.kind !== "fluent" ? "#ffffff" : "#c5ffffff")
                        font.pixelSize: 13
                        font.weight: cell.on ? (sg.kind === "fluent" ? 600 : (sg.kind === "m3" ? 500 : 600)) : (sg.kind === "m3" ? 500 : 400)
                        font.family: Theme.type.family
                    }
                }
                MouseArea { id: cellA; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sg.chosen(String(cell.opt[0])) }
            }
        }
    }
}
