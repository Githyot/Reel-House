import QtQuick
import Reelhouse 1.0
import "ColorMath.js" as CM

// GNOME (Libadwaita): the palette of the GNOME colours, then the custom editor with RGB or HSV sliders.
Column {
    id: gp
    property var st
    property bool hsv: false
    spacing: 14
    readonly property var swatches: ["#99c1f1", "#62a0ea", "#3584e4", "#1c71d8", "#1a5fb4", "#8ff0a4", "#57e389", "#33d17a", "#2ec27e", "#26a269",
        "#f9f06b", "#f8e45c", "#f6d32d", "#f5c211", "#e5a50a", "#ffbe6f", "#ffa348", "#ff7800", "#e66100", "#c64600",
        "#f66151", "#ed333b", "#e01b24", "#c01c28", "#a51d2d", "#dc8add", "#c061cb", "#9141ac", "#813d9c", "#613583",
        "#cdab8f", "#b5835a", "#986a44", "#865e3c", "#63452c", "#ffffff", "#deddda", "#9a9996", "#5e5c64", "#000000"]

    Text { text: Core.tr(Core.uiLang, "Palette"); color: Theme.c.text; font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family }
    SwatchGrid { width: parent.width; cell: 27; corner: 99; colors: gp.swatches; selected: gp.st.hex; onPicked: function(h) { gp.st.setHex(h) } }

    Row {
        width: parent.width
        spacing: 8
        Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - segs.width - 8; text: Core.tr(Core.uiLang, "Custom"); color: Theme.c.text; font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family }
        Row {
            id: segs
            spacing: 4
            FlatButton { text: "RGB"; kind: gp.hsv ? "plain" : "suggested"; implicitHeight: 30; onClicked: gp.hsv = false }
            FlatButton { text: "HSV"; kind: gp.hsv ? "suggested" : "plain"; implicitHeight: 30; onClicked: gp.hsv = true }
        }
    }

    ColorPlane { width: parent.width; height: 150; radius: 8; hue: gp.st.h; sat: gp.st.s; val: gp.st.v; onMoved: function(s, v) { gp.st.setHsv(gp.st.h, s, v) } }
    ChannelRow { width: parent.width; label: "H"; kind: "adw"; rainbow: true; from: 0; to: 360; value: gp.st.h; suffix: "°"; onMoved: function(v) { gp.st.setHsv(v, gp.st.s, gp.st.v) } }

    Column {
        width: parent.width
        spacing: 8
        visible: !gp.hsv
        ChannelRow { width: parent.width; label: "R"; kind: "adw"; value: gp.st.r; c0: Qt.rgba(0, gp.st.g / 255, gp.st.b / 255, 1); c1: Qt.rgba(1, gp.st.g / 255, gp.st.b / 255, 1); onMoved: function(v) { gp.st.setRgb(v, gp.st.g, gp.st.b) } }
        ChannelRow { width: parent.width; label: "G"; kind: "adw"; value: gp.st.g; c0: Qt.rgba(gp.st.r / 255, 0, gp.st.b / 255, 1); c1: Qt.rgba(gp.st.r / 255, 1, gp.st.b / 255, 1); onMoved: function(v) { gp.st.setRgb(gp.st.r, v, gp.st.b) } }
        ChannelRow { width: parent.width; label: "B"; kind: "adw"; value: gp.st.b; c0: Qt.rgba(gp.st.r / 255, gp.st.g / 255, 0, 1); c1: Qt.rgba(gp.st.r / 255, gp.st.g / 255, 1, 1); onMoved: function(v) { gp.st.setRgb(gp.st.r, gp.st.g, v) } }
    }
    Column {
        width: parent.width
        spacing: 8
        visible: gp.hsv
        ChannelRow { width: parent.width; label: "S"; kind: "adw"; from: 0; to: 100; value: gp.st.s * 100; suffix: "%"; c0: Qt.hsva(gp.st.h / 360, 0, gp.st.v, 1); c1: Qt.hsva(gp.st.h / 360, 1, gp.st.v, 1); onMoved: function(v) { gp.st.setHsv(gp.st.h, v / 100, gp.st.v) } }
        ChannelRow { width: parent.width; label: "V"; kind: "adw"; from: 0; to: 100; value: gp.st.v * 100; suffix: "%"; c0: "#000000"; c1: Qt.hsva(gp.st.h / 360, gp.st.s, 1, 1); onMoved: function(v) { gp.st.setHsv(gp.st.h, gp.st.s, v / 100) } }
    }
    ChannelRow { visible: gp.st.withAlpha; width: parent.width; label: "A"; kind: "adw"; checker: true; from: 0; to: 100; value: gp.st.alpha * 100; suffix: "%"; c0: Qt.rgba(gp.st.r / 255, gp.st.g / 255, gp.st.b / 255, 0); c1: Qt.rgba(gp.st.r / 255, gp.st.g / 255, gp.st.b / 255, 1); onMoved: function(v) { gp.st.alpha = v / 100 } }
}
