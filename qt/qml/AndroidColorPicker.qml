import QtQuick
import Reelhouse 1.0
import "ColorMath.js" as CM

// Android (Material 3 / Material You): the dynamic colours from the colour of the system, the tonal palette of the
// colour being chosen and three M3 sliders: hue, colourfulness and tone.
Column {
    id: ap
    property var st
    spacing: 14
    readonly property var hsl: CM.rgbToHsl(st.r, st.g, st.b, st.h)
    readonly property var seed: CM.rgbToHsl(parseInt(Theme.c.accent.toString().substring(1, 3), 16), parseInt(Theme.c.accent.toString().substring(3, 5), 16), parseInt(Theme.c.accent.toString().substring(5, 7), 16), 210)
    // Primary, secondary and tertiary roles of the system colour, light and dark tones.
    readonly property var dynamic: [CM.tone(seed.h, Math.max(seed.s, 0.55), 40), CM.tone(seed.h, Math.max(seed.s, 0.55), 80), CM.tone(seed.h, Math.max(seed.s, 0.55), 90),
        CM.tone(seed.h, Math.max(seed.s, 0.55) * 0.3, 40), CM.tone(seed.h, Math.max(seed.s, 0.55) * 0.3, 80), CM.tone(seed.h, Math.max(seed.s, 0.55) * 0.3, 90),
        CM.tone(seed.h + 60, Math.max(seed.s, 0.55) * 0.6, 40), CM.tone(seed.h + 60, Math.max(seed.s, 0.55) * 0.6, 80), CM.tone(seed.h + 60, Math.max(seed.s, 0.55) * 0.6, 90)]
    readonly property var tones: CM.tonalPalette(st.h, Math.max(hsl.s, 0.35))

    Row {
        spacing: 12
        Rectangle { width: 56; height: 56; radius: 28; color: ap.st.hex; border.width: 1; border.color: Theme.c.border }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            Text { text: ap.st.hex.toUpperCase(); color: Theme.c.text; font.pixelSize: 20; font.weight: Font.Medium; font.family: Theme.type.family }
            Text { text: "H " + Math.round(ap.st.h) + "°   S " + Math.round(ap.hsl.s * 100) + "%   T " + Math.round(ap.hsl.l * 100); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
        }
    }

    Text { text: Core.tr(Core.uiLang, "Dynamic colours"); color: Theme.c.text; font.pixelSize: 14; font.weight: Font.Medium; font.family: Theme.type.family }
    SwatchGrid { width: parent.width; cell: 36; corner: 99; spacing: 8; colors: ap.dynamic; selected: ap.st.hex; onPicked: function(h) { ap.st.setHex(h) } }

    Text { text: Core.tr(Core.uiLang, "Tonal palette"); color: Theme.c.text; font.pixelSize: 14; font.weight: Font.Medium; font.family: Theme.type.family }
    Row {
        id: tonal
        width: parent.width
        spacing: 0
        Repeater {
            model: ap.tones
            delegate: Rectangle {
                required property string modelData
                required property int index
                width: tonal.width / ap.tones.length
                height: 40
                color: modelData
                radius: index === 0 ? 12 : (index === ap.tones.length - 1 ? 12 : 0)
                border.width: modelData.toLowerCase() === ap.st.hex.toLowerCase() ? 3 : 0
                border.color: Theme.c.accent
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: ap.st.setHex(modelData) }
            }
        }
    }

    Column {
        width: parent.width
        spacing: 4
        ChannelRow { width: parent.width; label: "H"; kind: "m3"; rainbow: true; from: 0; to: 360; value: ap.st.h; suffix: "°"; onMoved: function(v) { ap.st.setHsl(v, ap.hsl.s, ap.hsl.l) } }
        ChannelRow { width: parent.width; label: "S"; kind: "m3"; from: 0; to: 100; value: ap.hsl.s * 100; suffix: "%"
            c0: Qt.hsla(ap.st.h / 360, 0, ap.hsl.l, 1); c1: Qt.hsla(ap.st.h / 360, 1, ap.hsl.l, 1); onMoved: function(v) { ap.st.setHsl(ap.st.h, v / 100, ap.hsl.l) } }
        ChannelRow { width: parent.width; label: "T"; kind: "m3"; from: 0; to: 100; value: ap.hsl.l * 100
            c0: "#000000"; c1: "#ffffff"; onMoved: function(v) { ap.st.setHsl(ap.st.h, ap.hsl.s, v / 100) } }
        ChannelRow { visible: ap.st.withAlpha; width: parent.width; label: "A"; kind: "m3"; checker: true; from: 0; to: 100; value: ap.st.alpha * 100; suffix: "%"
            c0: Qt.rgba(ap.st.r / 255, ap.st.g / 255, ap.st.b / 255, 0); c1: Qt.rgba(ap.st.r / 255, ap.st.g / 255, ap.st.b / 255, 1); onMoved: function(v) { ap.st.alpha = v / 100 } }
    }
}
