import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import Reelhouse 1.0

// The progress panel at the bottom left (GTK tasks.rs): work in the background (downloads, translations, searches), each
// with a Cancel button, like the operations list of GNOME Files. It can show one summary bar instead of one bar per task,
// and it can shrink to a round button with a progress ring.
Item {
    id: tp
    readonly property var items: Core.tasks.items !== undefined ? Core.tasks.items : []
    property bool compact: false
    property bool minimized: false
    readonly property var known: items.filter(function(t) { return t.fraction !== null && t.fraction !== undefined })
    readonly property real overall: known.length === 0 ? -1 : known.reduce(function(a, t) { return a + t.fraction }, 0) / known.length
    visible: items.length > 0
    width: minimized ? ring.width : card.width
    height: minimized ? ring.height : card.height

    // An Adwaita progress bar: 6 px trough, accent fill; without a fraction it pulses.
    component Bar: Rectangle {
        id: bar
        property real fraction: -1
        width: parent ? parent.width : 100
        height: 6
        radius: 3
        color: Theme.c.fill
        clip: true
        Rectangle {
            visible: bar.fraction >= 0
            height: parent.height
            radius: 3
            width: parent.width * Math.max(0, Math.min(1, bar.fraction))
            color: Theme.c.accent
            Behavior on width { NumberAnimation { duration: 200 } }
        }
        Rectangle {
            id: pulse
            visible: bar.fraction < 0
            height: parent.height
            radius: 3
            width: parent.width * 0.25
            color: Theme.c.accent
            SequentialAnimation on x {
                running: pulse.visible && tp.visible
                loops: Animation.Infinite
                NumberAnimation { from: 0; to: bar.width * 0.75; duration: 900; easing.type: Easing.InOutQuad }
                NumberAnimation { from: bar.width * 0.75; to: 0; duration: 900; easing.type: Easing.InOutQuad }
            }
        }
    }

    Rectangle {
        id: card
        visible: !tp.minimized
        width: 330
        height: cardCol.implicitHeight + 24
        radius: Theme.shape.popover
        color: Theme.c.popover
        border.width: 1
        border.color: Theme.c.border
        Column {
            id: cardCol
            x: 14
            y: 12
            width: parent.width - 26
            spacing: 10
            Item {
                width: parent.width
                height: 32
                Text { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Background tasks"); color: Theme.c.textDim; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family }
                Row {
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    spacing: 4
                    FlatButton { kind: "flatround"; glyph: "list"; implicitWidth: 32; implicitHeight: 32; checkable: true; checked: tp.compact; onClicked: tp.compact = !tp.compact; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Single progress bar") }
                    FlatButton { kind: "flatround"; glyph: "down"; implicitWidth: 32; implicitHeight: 32; onClicked: tp.minimized = true; ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Minimize") }
                }
            }
            // One row per task.
            Column {
                visible: !tp.compact
                width: parent.width
                spacing: 12
                Repeater {
                    model: tp.items
                    delegate: Item {
                        required property var modelData
                        width: parent.width
                        height: Math.max(rowCol.implicitHeight, 34)
                        Column {
                            id: rowCol
                            anchors { left: parent.left; right: stopBtn.left; rightMargin: 10; verticalCenter: parent.verticalCenter }
                            spacing: 3
                            Text { width: parent.width; elide: Text.ElideRight; text: modelData.title; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
                            Text { width: parent.width; elide: Text.ElideRight; text: modelData.what + (modelData.detail ? " · " + modelData.detail : ""); color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
                            Item { width: 1; height: 2 }
                            Bar { fraction: modelData.fraction === null || modelData.fraction === undefined ? -1 : modelData.fraction }
                        }
                        FlatButton {
                            id: stopBtn
                            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                            kind: "flatround"
                            glyph: "close"
                            implicitWidth: 34
                            implicitHeight: 34
                            ToolTip.visible: hovered
                            ToolTip.text: Core.tr(Core.uiLang, "Cancel")
                            onClicked: Core.cmd({cmd: "cancel_task", key: modelData.key})
                        }
                    }
                }
            }
            // The summary.
            Column {
                visible: tp.compact
                width: parent.width
                spacing: 3
                Text { width: parent.width; elide: Text.ElideRight; text: tp.items.length === 1 ? tp.items[0].title : tp.items.length + " tasks"; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
                Text {
                    width: parent.width
                    elide: Text.ElideRight
                    text: tp.overall >= 0 ? Math.round(tp.overall * 100) + "%" : (tp.items.length === 1 ? tp.items[0].what : Core.tr(Core.uiLang, "Working…"))
                    color: Theme.c.textDim
                    font.pixelSize: 13
                    font.family: Theme.type.family
                }
                Item { width: 1; height: 2 }
                Bar { fraction: tp.overall }
            }
        }
    }

    // The round button: a ring that fills as the work goes, with an arrow in the middle.
    Rectangle {
        id: ring
        visible: tp.minimized
        width: 52
        height: 52
        radius: 26
        color: ringArea.containsMouse ? Qt.lighter(Theme.c.popover, 1.15) : Theme.c.popover
        border.width: 1
        border.color: Theme.c.border
        Shape {
            anchors.centerIn: parent
            width: 44
            height: 44
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: "#59808080"
                strokeWidth: 3.5
                fillColor: "transparent"
                PathAngleArc { centerX: 22; centerY: 22; radiusX: 19; radiusY: 19; startAngle: 0; sweepAngle: 360 }
            }
            ShapePath {
                strokeColor: Theme.c.accent
                strokeWidth: 3.5
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathAngleArc { centerX: 22; centerY: 22; radiusX: 19; radiusY: 19; startAngle: -90; sweepAngle: 360 * Math.max(0, tp.overall) }
            }
        }
        Icon { anchors.centerIn: parent; name: "download"; size: 18 }
        MouseArea { id: ringArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: tp.minimized = false }
        ToolTip.visible: ringArea.containsMouse
        ToolTip.text: Core.tr(Core.uiLang, "Background tasks")
    }
}
