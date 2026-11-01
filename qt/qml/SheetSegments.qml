import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A group of choices where exactly one is on, in the look of each theme:
//   apple   - the segmented control: a grey track and a raised white thumb;
//   gnome   - the libadwaita toggle group: a soft track, the active toggle raised;
//   windows - Fluent toggle buttons: square-ish, the active one filled with the accent;
//   android - Material 3 segmented buttons: outlined, round ends, a check mark on the chosen one.
Item {
    id: sg
    property var model: []              // [{id, label, glyph, enabled, tip}]; with no `current` it is a row of actions
    property string current: ""
    signal picked(string id)
    readonly property string tid: Theme.look
    readonly property bool track: tid === "apple" || tid === "gnome"
    width: parent ? parent.width : 300
    implicitHeight: tid === "android" ? 40 : 34
    height: implicitHeight

    // the track of the Apple and GNOME controls
    Rectangle {
        visible: sg.track
        anchors.fill: parent
        radius: sg.tid === "apple" ? 9 : 10
        color: Theme.c.fill
    }
    Row {
        id: row
        anchors.fill: parent
        anchors.margins: sg.track ? 2 : 0
        spacing: sg.tid === "windows" ? 4 : (sg.tid === "android" ? -1 : 2)
        Repeater {
            model: sg.model
            delegate: Rectangle {
                id: seg
                required property var modelData
                required property int index
                readonly property bool on: sg.current === modelData.id
                readonly property bool firstOne: index === 0
                readonly property bool lastOne: index === sg.model.length - 1
                readonly property bool usable: modelData.enabled !== false
                opacity: usable ? 1 : 0.4
                width: (row.width - row.spacing * (sg.model.length - 1)) / Math.max(1, sg.model.length)
                height: row.height
                radius: sg.tid === "apple" ? 7 : (sg.tid === "gnome" ? 8 : (sg.tid === "android" ? height / 2 : 4))
                color: {
                    if (sg.track) return on ? (Theme.scheme === "dark" ? Theme.c.surface2 : "#ffffff") : (area.containsMouse ? Theme.c.hover : "transparent")
                    if (sg.tid === "windows") return on ? Theme.c.accent : (area.containsMouse ? Theme.c.buttonHover : Theme.c.button)
                    return on ? Theme.c.accent20 : (area.containsMouse ? Theme.c.hover : "transparent")
                }
                // the raised thumb of the Apple and GNOME controls has a hair line; Fluent and Material are outlined
                border.width: sg.tid === "android" || (sg.tid === "windows" && !on) || (sg.track && on) ? 1 : 0
                border.color: sg.tid === "android" ? Theme.c.textDim : (sg.track ? "#14000000" : Theme.c.controlStroke)
                // Material: only the outer ends are round
                Rectangle {
                    visible: sg.tid === "android" && !seg.firstOne
                    anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                    width: parent.radius; color: parent.color
                    border.width: 0
                    Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Theme.c.textDim }
                    Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Theme.c.textDim }
                    Rectangle { anchors { left: parent.left; top: parent.top; bottom: parent.bottom } width: 1; color: Theme.c.textDim }
                }
                Rectangle {
                    visible: sg.tid === "android" && !seg.lastOne
                    anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
                    width: parent.radius; color: parent.color
                    Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Theme.c.textDim }
                    Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Theme.c.textDim }
                }
                Row {
                    anchors.centerIn: parent
                    spacing: 6
                    Icon {
                        visible: sg.tid === "android" && seg.on
                        name: "check"; size: 16
                        color: Theme.c.text
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Icon {
                        visible: (seg.modelData.glyph || "") !== "" && !(sg.tid === "android" && seg.on)
                        name: seg.modelData.glyph || ""
                        size: 16
                        color: sg.tid === "windows" && seg.on ? Theme.c.accentFg : Theme.c.text
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Text {
                        visible: text !== ""
                        text: seg.modelData.label
                        color: sg.tid === "windows" && seg.on ? Theme.c.accentFg : Theme.c.text
                        font.pixelSize: 13
                        font.weight: seg.on || sg.tid === "android" ? Font.DemiBold : Font.Normal
                        font.family: Theme.type.family
                        elide: Text.ElideRight
                        width: Math.min(implicitWidth, seg.width - 16)
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
                MouseArea {
                    id: area; anchors.fill: parent
                    enabled: seg.usable; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                    onClicked: sg.picked(seg.modelData.id)
                }
                ToolTip.visible: area.containsMouse && (modelData.tip || "") !== ""
                ToolTip.delay: 500
                ToolTip.text: modelData.tip || ""
            }
        }
    }
}
