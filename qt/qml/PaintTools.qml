import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The picture tools of the page editor: brush, eraser, bucket, colour picker (eyedropper), crop; the colour wheel,
// size, opacity and the bucket's tolerance; undo and done. Every change is an operation kept in the chapter's .md.
Rectangle {
    id: pt
    property var rd
    function tr(s) { return Core.tr(Core.uiLang, s) }
    width: 64
    implicitHeight: col.implicitHeight + 20
    radius: Theme.shape.card
    color: Theme.c.popover
    border.width: 1
    border.color: Theme.c.border

    component Tool: FlatButton {
        property string tool: ""
        property string tip: ""
        kind: pt.rd.paintTool === tool ? "suggested" : "flat"
        implicitWidth: 44; implicitHeight: 40
        ToolTip.visible: hovered; ToolTip.text: tip
        onClicked: pt.rd.paintTool = tool
    }
    component Slide: Column {
        property string label: ""
        property alias from: sl.from
        property alias to: sl.to
        property alias value: sl.value
        property alias step: sl.stepSize
        signal moved(real v)
        width: parent.width; spacing: 0
        Lbl { anchors.horizontalCenter: parent.horizontalCenter; text: parent.label; size: 10; dim: true }
        Slider { id: sl; width: parent.width; orientation: Qt.Horizontal; onMoved: parent.moved(value) }
    }

    // On short windows (the reader limits the height) the tools scroll.
    Flickable {
        id: ptScroll
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: col.implicitHeight + 20
        interactive: contentHeight > height + 1
        boundsBehavior: Flickable.StopAtBounds
    Column {
        id: col
        x: 10; y: 10
        width: pt.width - 20
        spacing: 6
        Tool { tool: "brush"; glyph: "brush"; tip: pt.tr("Brush") + " (B)" }
        Tool { tool: "eraser"; glyph: "eraser"; tip: pt.tr("Eraser: takes your edits away") + " (E)" }
        Tool { tool: "fill"; glyph: "bucket"; tip: pt.tr("Bucket: fills a speech bubble or an area of the same colour") + " (G)" }
        Tool { tool: "picker"; glyph: "eyedropper"; tip: pt.tr("Colour picker: takes a colour from the page") + " (I)" }
        Tool { tool: "crop"; glyph: "crop"; tip: pt.tr("Crop") + " (C)" }
        // the colour: a click opens the colour wheel
        Rectangle {
            width: 44; height: 44; radius: 22
            color: pt.rd.paintColor
            border.width: 2; border.color: Theme.c.border
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { colorWheel.selectedColor = pt.rd.paintColor; colorWheel.open() } }
            ToolTip.visible: hov.hovered; ToolTip.text: pt.tr("Colour") + " " + pt.rd.paintColor
            HoverHandler { id: hov }
        }
        Row {
            spacing: 4
            Repeater {
                model: ["#ffffff", "#000000"]
                delegate: Rectangle { required property var modelData; width: 18; height: 18; radius: 9; color: modelData; border.width: 1; border.color: Theme.c.border; MouseArea { anchors.fill: parent; onClicked: pt.rd.paintColor = modelData } }
            }
        }
        Slide { label: pt.tr("Size"); from: 1; to: 120; value: pt.rd.paintSize; onMoved: function(v) { pt.rd.paintSize = Math.round(v) } }
        Slide { label: pt.tr("Opacity"); from: 0.05; to: 1; value: pt.rd.paintAlpha; onMoved: function(v) { pt.rd.paintAlpha = Math.round(v * 100) / 100 } }
        Slide { visible: pt.rd.paintTool === "fill"; label: pt.tr("Tolerance"); from: 0; to: 160; value: pt.rd.fillTol; onMoved: function(v) { pt.rd.fillTol = Math.round(v) } }
        FlatButton { kind: "flat"; glyph: "undo"; implicitWidth: 44; implicitHeight: 40; ToolTip.visible: hovered; ToolTip.text: pt.tr("Undo") + " (Ctrl+Z)"; onClicked: pt.rd.undoOp() }
        FlatButton { kind: "suggested"; glyph: "check"; implicitWidth: 44; implicitHeight: 40; ToolTip.visible: hovered; ToolTip.text: pt.tr("Done"); onClicked: pt.rd.editMode = "" }
    }
    }
    // the colour wheel (the round chooser of the app, whatever the theme)
    ColorPickerAdapter {
        id: colorWheel
        currentTheme: "apple"
        title: pt.tr("Colour")
        onAccepted: { const c = selectedColor; pt.rd.paintColor = "#" + [c.r, c.g, c.b].map(function(v) { const h = Math.round(v * 255).toString(16); return h.length < 2 ? "0" + h : h }).join("") }
    }
}
