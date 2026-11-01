import QtQuick
import QtQuick.Effects
import Reelhouse 1.0

// Saturation (across) and brightness (up) of one hue, as in every colour chooser.
Item {
    id: pl
    property real hue: 0
    property real sat: 1
    property real val: 1
    property real radius: 8
    property real knob: 20
    signal moved(real s, real v)

    Item {
        id: art
        anchors.fill: parent
        layer.enabled: true
        layer.effect: MultiEffect { maskEnabled: true; maskSource: mask; maskThresholdMin: 0.5; maskSpreadAtMin: 1.0 }
        Rectangle { anchors.fill: parent; color: Qt.hsva(pl.hue / 360, 1, 1, 1) }
        Rectangle { anchors.fill: parent; gradient: Gradient { orientation: Gradient.Horizontal; GradientStop { position: 0; color: "#ffffffff" } GradientStop { position: 1; color: "#00ffffff" } } }
        Rectangle { anchors.fill: parent; gradient: Gradient { GradientStop { position: 0; color: "#00000000" } GradientStop { position: 1; color: "#ff000000" } } }
    }
    Item {
        id: mask
        visible: false
        layer.enabled: true
        width: pl.width; height: pl.height
        Rectangle { anchors.fill: parent; radius: pl.radius; color: "#ffffff" }
    }
    Rectangle { anchors.fill: parent; radius: pl.radius; color: "transparent"; border.width: 1; border.color: Theme.c.border }
    Rectangle {
        x: pl.sat * pl.width - width / 2
        y: (1 - pl.val) * pl.height - height / 2
        width: pl.knob; height: pl.knob; radius: width / 2
        color: Qt.hsva(pl.hue / 360, pl.sat, pl.val, 1)
        border.width: 3; border.color: "#ffffff"
        Rectangle { anchors.fill: parent; anchors.margins: -1; radius: width / 2; color: "transparent"; border.width: 1; border.color: "#59000000"; z: -1 }
    }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.CrossCursor
        function put(m) { pl.moved(Math.max(0, Math.min(1, m.x / width)), Math.max(0, Math.min(1, 1 - m.y / height))) }
        onPressed: function(m) { put(m) }
        onPositionChanged: function(m) { if (pressed) put(m) }
    }
}
