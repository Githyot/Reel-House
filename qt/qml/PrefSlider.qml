import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A row with a slider and its value (the size and night filter rows).
PrefRow {
    id: ps
    property real value: 0
    property real from: 0
    property real to: 100
    property real step: 1
    property string unit: "%"
    // digits after the point in the value shown (the pause of the microphone: 1.2 s)
    property int decimals: 0
    signal moved(real v)
    signal released()
    Slider {
        id: sl
        width: Math.min(220, Math.max(110, ps.width * 0.36))
        from: ps.from
        to: ps.to
        stepSize: ps.step
        anchors.verticalCenter: parent.verticalCenter
        // Only reports the drag: `value` stays bound to the setting, and the slider follows it while not held.
        onMoved: ps.moved(value)
        onPressedChanged: if (!pressed) ps.released()
        background: Rectangle {
            x: sl.leftPadding
            y: sl.topPadding + sl.availableHeight / 2 - height / 2
            width: sl.availableWidth
            height: 4
            radius: 2
            color: Theme.c.fill
            Rectangle { width: sl.visualPosition * parent.width; height: parent.height; radius: 2; color: Theme.c.accent }
        }
        Binding on value { value: ps.value; when: !sl.pressed }
        handle: Rectangle { x: sl.leftPadding + sl.visualPosition * (sl.availableWidth - width); y: sl.topPadding + sl.availableHeight / 2 - height / 2; width: 18; height: 18; radius: 9; color: "#ffffff"; border.width: 1; border.color: "#33000000" }
    }
    Text { width: Math.max(46, implicitWidth); text: (ps.decimals > 0 ? Number(sl.value).toFixed(ps.decimals) : Math.round(sl.value)) + ps.unit; color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
}
