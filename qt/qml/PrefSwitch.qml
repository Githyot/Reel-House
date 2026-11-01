import QtQuick
import Reelhouse 1.0

// A row with an on/off switch (adw::SwitchRow). The whole row toggles.
// `checked` is bound by the caller to the setting; a click only emits toggled() (assigning `checked` would break that
// binding). `isOn` shows the new state at once and follows `checked` again whenever the setting changes.
PrefRow {
    id: s
    property bool checked: false
    property bool isOn: checked
    onCheckedChanged: isOn = checked
    signal toggled(bool on)
    MouseArea { id: rowArea; parent: s; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { s.isOn = !s.isOn; s.toggled(s.isOn) } }
    // GNOME: 46x26 accent switch. Apple: the iOS switch, 51x31, green when on, with a bigger white knob and a soft shadow.
    // Material 3: 52x32 with an outline and a small knob when off. Windows 11: the 40x20 Fluent toggle.
    // HyperOS: the iOS shape, but the accent (HyperOS blue) when on and a light grey track when off.
    readonly property bool hyper: Theme.layout.toggle === "hyper"
    readonly property bool ios: Theme.layout.toggle === "ios" || hyper
    readonly property bool m3: Theme.layout.toggle === "m3"
    readonly property bool fluent: Theme.layout.toggle === "fluent"
    Rectangle {
        visible: !s.fluent
        width: s.ios ? 51 : (s.m3 ? 52 : 46)
        height: s.ios ? 31 : (s.m3 ? 32 : 26)
        border.width: s.m3 && !s.isOn ? 2 : 0
        border.color: Theme.c.textDim
        radius: height / 2
        color: s.isOn ? (s.ios && !s.hyper ? Theme.c.success : Theme.c.accent) : Theme.c.fill
        Behavior on color { ColorAnimation { duration: Theme.motion.fast } }
        Rectangle {
            width: s.ios ? 27 : (s.m3 ? (s.isOn ? 24 : 16) : 20)
            height: width
            Behavior on width { enabled: s.m3; NumberAnimation { duration: Theme.motion.fast } }
            radius: width / 2
            y: (parent.height - height) / 2
            x: s.isOn ? parent.width - width - (s.ios ? 2 : (s.m3 ? 4 : 3)) : (s.ios ? 2 : (s.m3 ? 6 : 3))
            color: s.m3 ? (s.isOn ? Theme.c.accentFg : Theme.c.textDim) : "#ffffff"
            border.width: s.ios && !s.hyper ? 1 : 0
            border.color: "#1a000000"
            Behavior on x { NumberAnimation { duration: Theme.motion.fast; easing.type: Easing.OutQuint } }
        }
    }
    // Fluent ToggleSwitch: off = outlined pill with a small dark knob; on = filled accent with a light knob.
    Rectangle {
        visible: s.fluent
        width: 40
        height: 20
        radius: 10
        anchors.verticalCenter: parent.verticalCenter
        color: s.isOn ? Theme.c.accent : (rowArea.containsMouse ? Theme.c.hover : "transparent")
        border.width: s.isOn ? 0 : 1
        border.color: Theme.c.textDim
        Behavior on color { ColorAnimation { duration: Theme.motion.fast } }
        Rectangle {
            width: rowArea.pressed ? 17 : (rowArea.containsMouse ? 14 : 12)
            height: rowArea.pressed ? 14 : width
            radius: height / 2
            y: (parent.height - height) / 2
            x: s.isOn ? parent.width - width - 4 : 4
            color: s.isOn ? Theme.c.accentFg : Theme.c.textDim
            Behavior on x { NumberAnimation { duration: Theme.motion.fast; easing.type: Easing.OutQuint } }
            Behavior on width { NumberAnimation { duration: Theme.motion.fast } }
        }
    }
}
