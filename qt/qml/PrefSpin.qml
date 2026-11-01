import QtQuick
import Reelhouse 1.0

// A number with − and + (adw::SpinRow).
PrefRow {
    id: sp
    property real value: 0
    property real from: 0
    property real to: 100
    property real step: 1
    readonly property int digits: step < 1 ? 1 : 0
    signal moved(real v)
    // `value` stays bound to the setting; `shown` is the number on screen, follows `value` and moves at once on a click.
    property real shown: value
    onValueChanged: shown = value
    function set(v) { v = Number(Math.max(sp.from, Math.min(sp.to, Math.round(v / sp.step) * sp.step)).toFixed(sp.digits)); if (v !== sp.shown) { sp.shown = v; sp.moved(v) } }
    Rectangle {
        width: 132
        height: 34
        radius: Theme.shape.control
        color: Theme.c.button
        anchors.verticalCenter: parent.verticalCenter
        Text { anchors.centerIn: parent; text: Number(sp.shown).toFixed(sp.digits); color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
        MouseArea { width: 34; height: parent.height; cursorShape: Qt.PointingHandCursor; onClicked: sp.set(sp.shown - sp.step); Icon { anchors.centerIn: parent; name: "minus"; size: 16 } }
        MouseArea { x: parent.width - 34; width: 34; height: parent.height; cursorShape: Qt.PointingHandCursor; onClicked: sp.set(sp.shown + sp.step); Icon { anchors.centerIn: parent; name: "plus"; size: 16 } }
        WheelHandler { onWheel: function(e) { sp.set(sp.shown + (e.angleDelta.y > 0 ? sp.step : -sp.step)) } }
    }
}
