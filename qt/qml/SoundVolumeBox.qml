import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The volume of the interface sounds and the background music (general.sound_volume, 0-100), a small group in the
// fixed sidebar (SidebarNav) drawn like its other groups and the theme: Apple a white knob, Windows (Fluent) an accent
// knob in a ring, Android (Material 3) a thick track with a bar knob, GNOME a light knob. The controller reaches it
// like any control: up / down to it, left / right change it (PadNavigator), ✕ on the speaker mutes.
Rectangle {
    id: vb
    readonly property var g: Core.prefs.general || ({})
    readonly property string look: Theme.look === "apple" || Theme.look === "windows" || Theme.look === "android" ? Theme.look : "gnome"
    readonly property int saved: g.sound_volume !== undefined ? g.sound_volume : 60
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function save(v) { pending = Math.round(v); saveTimer.restart() }
    property int pending: -1
    // a drag or a held direction sends many values: the last one is saved
    Timer { id: saveTimer; interval: 150; onTriggered: if (vb.pending >= 0) { Core.cmd({cmd: "set_setting", path: "general.sound_volume", value: vb.pending}); settleTimer.restart() } }
    // the setting comes back from the core a moment later: the slider follows it again then
    Timer { id: settleTimer; interval: 700; onTriggered: vb.pending = -1 }

    implicitHeight: 40
    radius: look === "windows" ? (Theme.shape.control || 4) : (Theme.shape.card !== undefined ? Theme.shape.card : 12)
    color: look === "android" ? Theme.c.fill3 : (Theme.c.card !== undefined ? Theme.c.card : Theme.c.fill3)
    border.width: slider.activeFocus || muteBtn.activeFocus ? 2 : (look === "android" ? 0 : 1)
    border.color: slider.activeFocus || muteBtn.activeFocus ? Theme.c.accent : Theme.c.border

    FlatButton {
        id: muteBtn
        kind: "flat"; glyph: slider.value === 0 ? "mute" : "volume"
        anchors { left: parent.left; leftMargin: 4; verticalCenter: parent.verticalCenter }
        implicitWidth: 32; implicitHeight: 32
        ToolTip.visible: hovered; ToolTip.text: vb.tr("Volume of the theme sounds and music")
        property int before: 60
        onClicked: {
            if (slider.value > 0) { before = slider.value; slider.value = 0 }
            else slider.value = before > 0 ? before : 60
            vb.save(slider.value)
        }
    }
    Slider {
        id: slider
        anchors { left: muteBtn.right; leftMargin: 2; right: pct.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
        from: 0; to: 100; stepSize: 5
        // follows the setting except while it is being changed
        Binding on value { value: vb.saved; when: !slider.pressed && vb.pending < 0; restoreMode: Binding.RestoreNone }
        onMoved: vb.save(value)
        Accessible.name: vb.tr("Theme sound volume")
        background: Rectangle {
            x: slider.leftPadding; y: slider.topPadding + slider.availableHeight / 2 - height / 2
            width: slider.availableWidth
            height: vb.look === "android" ? 10 : 4
            radius: height / 2
            color: vb.look === "android" ? Theme.c.border : Theme.c.fill3
            Rectangle { width: slider.visualPosition * parent.width; height: parent.height; radius: parent.radius; color: Theme.c.accent }
        }
        handle: Rectangle {
            readonly property real d: vb.look === "windows" ? 18 : 16
            x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            width: vb.look === "android" ? 4 : d
            height: vb.look === "android" ? 22 : d
            radius: width / 2
            // Fluent: an accent dot in a light ring; Apple and GNOME: a light knob with a thin edge; Material: an
            // accent bar
            color: vb.look === "windows" || vb.look === "android" ? Theme.c.accent : "#ffffff"
            border.width: vb.look === "windows" ? 4 : (vb.look === "android" ? 0 : 1)
            border.color: vb.look === "windows" ? (Theme.c.card !== undefined ? Theme.c.card : Theme.c.window) : "#33000000"
            scale: slider.pressed ? 1.12 : 1
            Behavior on scale { NumberAnimation { duration: 120 } }
        }
    }
    Text {
        id: pct
        anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
        width: 30
        horizontalAlignment: Text.AlignRight
        text: Math.round(slider.value) + "%"
        color: Theme.c.textDim
        font.pixelSize: 12
        font.family: Theme.type.family
    }
}
