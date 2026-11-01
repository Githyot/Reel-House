import QtQuick
import QtQuick.Effects
import Reelhouse 1.0

// The material of the theme. GNOME: a plain solid panel. Apple (HIG "Materials"): what is behind the panel is read from
// `Theme.backdrop`, blurred and tinted (ultraThin, thin, regular or thick: the thicker, the more opaque), with a hair line
// at the edge. Without a backdrop (or a theme without glass) it falls back to the solid colour.
Item {
    id: g
    property real radius: 12
    property string level: "regular"
    property color solid: Theme.c.popover
    property bool bordered: true
    property bool shadow: false
    // Not on a phone (Theme.mobile): a blurred copy of the backdrop for every panel is too much for a phone GPU, so the
    // panel is the material's tint over the solid colour instead (iOS draws its sheets that way too).
    readonly property bool on: Theme.glass && !Theme.mobile && Theme.backdrop !== null && width > 4 && height > 4
    readonly property var lv: (Theme.material.levels || ({}))[level] || ({})
    property rect area: Qt.rect(0, 0, 1, 1)

    // Where this panel is inside the backdrop (read again when it moves or opens).
    function remap() {
        if (!Theme.backdrop || width <= 0 || height <= 0) return
        const p = g.mapToItem(Theme.backdrop, 0, 0)
        area = Qt.rect(Math.max(0, p.x), Math.max(0, p.y), width, height)
    }
    onWidthChanged: remap()
    onHeightChanged: remap()
    onVisibleChanged: if (visible) remap()
    Component.onCompleted: remap()
    Connections { target: Theme; function onBackdropChanged() { g.remap() } }
    Timer { running: g.on && g.visible; interval: 400; repeat: true; onTriggered: g.remap() }

    // a soft shadow under floating panels
    Rectangle {
        visible: g.shadow && g.on
        anchors.fill: parent
        anchors.margins: -1
        radius: g.radius
        color: "transparent"
        layer.enabled: visible
        layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.elevation.shadowColor || "#59000000"; shadowBlur: 1.0; shadowVerticalOffset: Theme.elevation.shadowOffsetY !== undefined ? Theme.elevation.shadowOffsetY : 8; autoPaddingEnabled: true }
    }

    // solid fallback (on a phone the glass theme's tint lies over it, so the panel keeps its tone)
    Rectangle {
        visible: !g.on
        anchors.fill: parent
        radius: g.radius
        color: g.solid
        border.width: g.bordered ? 1 : 0
        border.color: Theme.c.border
        Rectangle { visible: Theme.glass && Theme.mobile; anchors.fill: parent; radius: g.radius; color: g.lv[Theme.scheme === "light" ? "light" : "dark"] || "transparent" }
    }

    // glass: the blurred backdrop, then the tint, then the edge
    Item {
        id: glassBox
        visible: g.on
        anchors.fill: parent
        ShaderEffectSource {
            id: snap
            anchors.fill: parent
            sourceItem: Theme.backdrop
            sourceRect: g.area
            live: g.on && g.visible
            hideSource: false
            visible: false
        }
        MultiEffect {
            anchors.fill: parent
            source: snap
            blurEnabled: true
            blur: g.lv.blur !== undefined ? g.lv.blur : 1.0
            blurMax: Theme.material.blurMax || 48
            saturation: Theme.material.saturation !== undefined ? Theme.material.saturation : 0.4
            autoPaddingEnabled: false
            maskEnabled: true
            maskSource: glassMask
            maskThresholdMin: 0.5
            maskSpreadAtMin: 1.0
        }
        Item {
            id: glassMask
            anchors.fill: parent
            visible: false
            layer.enabled: true
            Rectangle { anchors.fill: parent; radius: g.radius; color: "black" }
        }
        Rectangle { anchors.fill: parent; radius: g.radius; color: g.lv[Theme.scheme === "light" ? "light" : "dark"] || g.solid }
        // the light that catches the top edge, and the hair line all around
        Rectangle {
            anchors.fill: parent
            radius: g.radius
            color: "transparent"
            border.width: g.bordered ? 1 : 0
            border.color: Theme.c.glassBorder
        }
        Rectangle {
            visible: g.bordered
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: g.radius * 0.6; rightMargin: g.radius * 0.6; topMargin: 1 }
            height: 1
            color: Theme.c.glassHighlight
        }
    }
}
