import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "Osd.js" as Osd

// A slider of the player menus in the theme's own shape:
//   GNOME: thin trough, white round knob       Windows: thin rail, dark thumb with an accent dot
//   Android: thick M3 track cut round a bar     Apple: white fill, white knob
// Callers keep the "Binding on value when !pressed" pattern: the player state arrives 4×/s.
Slider {
    id: sl
    readonly property string kind: Osd.t(Theme.layout.player).slider
    readonly property color fill: kind === "glass" ? "#ffffff" : Theme.c.accent
    // Above this value (e.g. 100 % of volume) the fill takes a warning colour and a tick marks the limit; -1 is off.
    property real boostFrom: -1
    readonly property bool boosting: boostFrom > from && boostFrom < to
    // the theme's own orange (GNOME, Windows, Apple) / M3's error tone on dark (Android)
    readonly property color boostColor: kind === "m3" ? "#ffb4ab" : (kind === "fluent" ? "#f7630c" : (kind === "glass" ? "#ff9f0a" : "#ff7800"))
    implicitHeight: kind === "m3" ? 36 : 28
    hoverEnabled: true

    background: Item {
        x: sl.leftPadding
        y: sl.topPadding + sl.availableHeight / 2 - height / 2
        width: sl.availableWidth
        height: sl.kind === "m3" ? 12 : (sl.kind === "glass" ? 6 : 4)
        readonly property real hx: sl.visualPosition * (sl.availableWidth - sl.handle.width)
        // Material 3: the active and inactive parts stop short of the handle bar
        readonly property real gap: sl.kind === "m3" ? 6 : 0
        Rectangle {
            anchors.fill: parent
            visible: sl.kind !== "m3"
            radius: height / 2
            color: sl.kind === "fluent" ? "#8bffffff" : (sl.kind === "glass" ? "#33ffffff" : "#33ffffff")
        }
        Rectangle {
            visible: sl.kind === "m3"
            x: parent.hx + sl.handle.width + parent.gap
            width: Math.max(0, parent.width - x)
            height: parent.height
            radius: height / 2
            color: "#4a4458"
        }
        Rectangle {
            width: sl.kind === "m3" ? Math.max(0, parent.hx - parent.gap) : parent.hx + sl.handle.width / 2
            height: parent.height
            radius: height / 2
            color: sl.fill
        }
        // where boostFrom sits on the track (the centre of the handle at that value)
        readonly property real bx: sl.boosting ? (sl.boostFrom - sl.from) / (sl.to - sl.from) * (sl.availableWidth - sl.handle.width) + sl.handle.width / 2 : 0
        // the part of the fill past the limit, in the warning colour
        Rectangle {
            visible: sl.boosting && sl.value > sl.boostFrom
            x: parent.bx
            width: Math.max(0, (sl.kind === "m3" ? parent.hx - parent.gap : parent.hx + sl.handle.width / 2) - x)
            height: parent.height
            radius: height / 2
            color: sl.boostColor
        }
        // the tick at the limit (100 %), a little taller than the track
        Rectangle {
            visible: sl.boosting
            x: parent.bx - width / 2
            anchors.verticalCenter: parent.verticalCenter
            width: 2
            height: parent.height + 8
            radius: 1
            color: sl.value > sl.boostFrom ? sl.boostColor : "#b3ffffff"
        }
    }
    handle: Rectangle {
        readonly property real d: sl.kind === "m3" ? 4 : (sl.kind === "fluent" ? 20 : 18)
        x: sl.leftPadding + sl.visualPosition * (sl.availableWidth - width)
        y: sl.topPadding + sl.availableHeight / 2 - height / 2
        width: d
        height: sl.kind === "m3" ? 32 : d
        radius: sl.kind === "m3" ? 2 : d / 2
        color: sl.kind === "m3" ? Theme.c.accent : (sl.kind === "fluent" ? "#454545" : "#ffffff")
        border.width: sl.kind === "fluent" ? 1 : (sl.kind === "adw" ? 1 : 0)
        border.color: sl.kind === "fluent" ? "#17ffffff" : "#33000000"
        scale: sl.pressed && sl.kind !== "fluent" ? 1.1 : 1
        Behavior on scale { NumberAnimation { duration: 90 } }
        // Windows: the accent dot inside the thumb, bigger on hover, smaller while pressed
        Rectangle {
            visible: sl.kind === "fluent"
            anchors.centerIn: parent
            width: sl.pressed ? 10 : (sl.hovered ? 14 : 12)
            height: width
            radius: width / 2
            color: Theme.c.accent
            Behavior on width { NumberAnimation { duration: 90 } }
        }
    }
}
