import QtQuick
import Reelhouse 1.0

// Windows (Fluent): the spectrum with 8 px corners, hue and alpha sliders, the basic colours and R G B boxes.
Column {
    id: wp
    property var st
    spacing: 14
    readonly property var basic: ["#ff8c00", "#f7630c", "#ca5010", "#da3b01", "#ef6950", "#d13438", "#ff4343", "#e74856",
        "#e81123", "#ea005e", "#c30052", "#e3008c", "#bf0077", "#c239b3", "#9a0089", "#0078d4",
        "#0063b1", "#8e8cd8", "#6b69d6", "#8764b8", "#744da9", "#b146c2", "#881798", "#0099bc",
        "#2d7d9a", "#00b7c3", "#038387", "#00b294", "#018574", "#00cc6a", "#10893e", "#7a7574",
        "#5d5a58", "#68768a", "#515c6b", "#567c73", "#486860", "#498205", "#107c10", "#767676"]

    ColorPlane { width: parent.width; height: 170; radius: 8; knob: 22; hue: wp.st.h; sat: wp.st.s; val: wp.st.v; onMoved: function(s, v) { wp.st.setHsv(wp.st.h, s, v) } }
    ChannelRow { width: parent.width; label: ""; kind: "fluent"; rainbow: true; from: 0; to: 360; value: wp.st.h; suffix: "°"; onMoved: function(v) { wp.st.setHsv(v, wp.st.s, wp.st.v) } }
    ChannelRow { visible: wp.st.withAlpha; width: parent.width; label: ""; kind: "fluent"; checker: true; from: 0; to: 100; value: wp.st.alpha * 100; suffix: "%"; c0: Qt.rgba(wp.st.r / 255, wp.st.g / 255, wp.st.b / 255, 0); c1: Qt.rgba(wp.st.r / 255, wp.st.g / 255, wp.st.b / 255, 1); onMoved: function(v) { wp.st.alpha = v / 100 } }

    Text { text: Core.tr(Core.uiLang, "Basic colors"); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
    SwatchGrid { width: parent.width; cell: 32; corner: 4; spacing: 5; colors: wp.basic; selected: wp.st.hex; onPicked: function(h) { wp.st.setHex(h) } }

    Row {
        width: parent.width
        spacing: 8
        Repeater {
            model: [{l: "R", k: 0}, {l: "G", k: 1}, {l: "B", k: 2}]
            delegate: Rectangle {
                required property var modelData
                width: (wp.width - 16) / 3; height: 36; radius: 4
                color: Theme.c.surface
                border.width: 1; border.color: Theme.c.border
                Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: modelData.l; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
                TextInput {
                    id: box
                    anchors { left: parent.left; leftMargin: 28; right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                    text: Math.round(modelData.k === 0 ? wp.st.r : (modelData.k === 1 ? wp.st.g : wp.st.b))
                    color: Theme.c.text; selectionColor: Theme.c.accent; font.pixelSize: 14; font.family: Theme.type.family
                    validator: IntValidator { bottom: 0; top: 255 }
                    selectByMouse: true
                    onEditingFinished: { var n = parseInt(text); if (!isNaN(n)) wp.st.setRgb(modelData.k === 0 ? n : wp.st.r, modelData.k === 1 ? n : wp.st.g, modelData.k === 2 ? n : wp.st.b) }
                }
                // Fluent text box: the accent line shows under the box that has the cursor.
                Rectangle { visible: box.activeFocus; anchors.bottom: parent.bottom; width: parent.width; height: 2; radius: 1; color: Theme.c.accent }
            }
        }
    }
}
