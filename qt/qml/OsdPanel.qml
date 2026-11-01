import QtQuick
import QtQuick.Effects
import Reelhouse 1.0
import "Osd.js" as Osd

// The panel behind a menu, popover or card of the player (see Osd.js for the look of each theme).
// sheet: true for the bigger panels (cards, the torrent list), which take the theme's sheet radius.
Item {
    id: pn
    property bool sheet: false
    // a phone's bottom sheet: round at the top only, flat against the bottom edge
    property bool bottomSheet: false
    property real radius: Math.min(sheet || bottomSheet ? o.sheetR : o.radius, height / 2)
    property color color: o.panel
    property color borderColor: o.stroke
    readonly property var o: Osd.t(Theme.layout.player)

    // A soft shadow under the panel (not for Apple's glass, where it would show through and muddy the tint).
    Rectangle {
        visible: pn.o.shadow && !Theme.mobile
        anchors.fill: parent
        radius: pn.radius
        color: "#000000"
        opacity: 0.55
        layer.enabled: visible
        layer.effect: MultiEffect { shadowEnabled: true; shadowColor: "#99000000"; shadowBlur: 0.9; shadowVerticalOffset: 6; autoPaddingEnabled: true }
    }
    Rectangle {
        anchors.fill: parent
        radius: pn.radius
        bottomLeftRadius: pn.bottomSheet ? 0 : pn.radius
        bottomRightRadius: pn.bottomSheet ? 0 : pn.radius
        color: pn.color
        border.width: pn.o.strokeW
        border.color: pn.borderColor
        Rectangle { visible: pn.bottomSheet && Theme.mob !== undefined && Theme.mob.grabber === true; anchors { top: parent.top; topMargin: 6; horizontalCenter: parent.horizontalCenter } width: 36; height: 5; radius: 2.5; color: "#66ffffff" }
        // Apple glass: a faint light from the top edge, like the visionOS materials.
        Rectangle {
            visible: pn.o.glossy
            anchors { fill: parent; margins: 1 }
            radius: Math.max(0, pn.radius - 1)
            gradient: Gradient {
                GradientStop { position: 0; color: "#17ffffff" }
                GradientStop { position: 0.45; color: "#00ffffff" }
            }
        }
    }
}
