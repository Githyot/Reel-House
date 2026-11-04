import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Preferences > Controller: the controller plugged in (model, battery), the light bar of PlayStation controllers
// (the theme's accent colour unless one is picked), the touchpad as a mouse, the stick and repeat, vibration, the
// orbs, the sounds and music (only files the person picks) and what each button does.
Column {
    id: pc
    property var dlg
    readonly property var g: dlg ? dlg.g : ({})
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function set(path, v) { dlg.set(path, v) }
    spacing: 26
    readonly property var win: Window.window
    readonly property var pads: win && win.pads ? win.pads : []
    readonly property var pad: pads.length > 0 ? pads[pads.length - 1] : null
    readonly property bool ps: pad !== null && (pad.kind === "ps4" || pad.kind === "ps5")
    readonly property var hid: Core.padHid || ({})

    // ---- the controller plugged in ----
    PrefGroup {
        title: pc.tr("Controller")
        description: pc.pad === null ? pc.tr("No controller connected. Plug one in (USB or Bluetooth) and controller mode starts by itself.") : ""
        PrefRow {
            visible: pc.pad !== null
            title: pc.pad ? pc.pad.name : ""
            subtitle: {
                if (!pc.pad) return ""
                const kind = pc.pad.kind === "xbox" ? "Xbox" : (pc.pad.kind === "ps5" ? "PlayStation (DualSense)" : (pc.pad.kind === "ps4" ? "PlayStation (DualShock 4)" : pc.tr("Other controller")))
                var s = kind
                if (pc.hid.connected === true) {
                    s += " · " + (pc.hid.bluetooth ? "Bluetooth" : "USB")
                    if (pc.hid.battery >= 0) s += " · " + pc.tr("Battery") + " " + pc.hid.battery + "%" + (pc.hid.charging ? " (" + pc.tr("charging") + ")" : "")
                }
                return s
            }
            Icon { name: "gamepad"; size: 22; anchors.verticalCenter: parent ? parent.verticalCenter : undefined }
        }
    }

    // ---- light bar (DualShock 4 / DualSense) ----
    PrefGroup {
        title: pc.tr("Light bar")
        description: pc.tr("PlayStation controllers (DualShock 4 and DualSense) light up in the theme's accent colour, or in a colour of your own")
        PrefSwitch {
            title: pc.tr("Use the theme's accent colour")
            checked: pc.g.pad_light_accent !== false
            onToggled: function(on) { pc.set("general.pad_light_accent", on) }
        }
        PrefRow {
            visible: pc.g.pad_light_accent === false
            title: pc.tr("Colour of the light bar")
            subtitle: (pc.g.pad_light_color || "") !== "" ? pc.g.pad_light_color : pc.tr("Not chosen: the accent colour is used")
            Rectangle {
                width: 28; height: 28; radius: 14
                color: (pc.g.pad_light_color || "") !== "" ? pc.g.pad_light_color : Theme.c.accent
                border.width: 1; border.color: Theme.c.border
            }
            FlatButton { text: pc.tr("Choose…"); onClicked: lightDlg.open() }
            ColorPickerAdapter {
                id: lightDlg
                title: pc.tr("Colour of the light bar")
                selectedColor: (pc.g.pad_light_color || "") !== "" ? pc.g.pad_light_color : Theme.c.accent
                onAccepted: {
                    function p(v) { var s = Math.round(v * 255).toString(16); return s.length < 2 ? "0" + s : s }
                    pc.set("general.pad_light_color", "#" + p(selectedColor.r) + p(selectedColor.g) + p(selectedColor.b))
                }
            }
        }
        PrefSlider {
            title: pc.tr("Brightness of the light bar")
            from: 0; to: 100; step: 5
            property real dragged: -1
            value: pc.g.pad_light_brightness !== undefined ? pc.g.pad_light_brightness : 100
            onMoved: function(v) { dragged = v }
            onReleased: { if (dragged >= 0) pc.set("general.pad_light_brightness", Math.round(dragged)); dragged = -1 }
        }
    }

    // ---- touchpad, stick, vibration ----
    PrefGroup {
        title: pc.tr("Controller mode")
        PrefSwitch { title: pc.tr("Focus sound"); subtitle: pc.tr("A soft blip when the focus moves with a controller"); checked: pc.g.pad_sound !== false; onToggled: function(on) { pc.set("general.pad_sound", on) } }
        PrefSwitch { title: pc.tr("Orbs of time"); subtitle: pc.tr("Glowing orbs circle behind the pages while a controller is connected, like the PS2 loading screen"); checked: pc.g.pad_orbs !== false; onToggled: function(on) { pc.set("general.pad_orbs", on) } }
    }

    // ---- the interface sounds and the background music: only files the person picks (none ship with the app) ----
    PrefGroup {
        title: pc.tr("Sounds and music")
        description: (pc.g.sound_nav || pc.g.sound_select || pc.g.sound_music)
            ? pc.tr("Your files are copied into the data folder. The music plays on the menus, with or without a controller")
            : pc.tr("No sounds or music come with the app. Pick your own files below (mp3, ogg, wav, flac, m4a), or import a pack (a zip or folder with nav, select and music files): the music then plays on the menus, with or without a controller")
        PrefRow {
            title: pc.tr("Import sound pack…")
            subtitle: pc.tr("A zip or a folder with nav, select and music files (for example nav.mp3, select.mp3, music.mp3)")
            FlatButton { text: pc.tr("Zip…"); onClicked: pc.dlg.importSoundPack(false) }
            FlatButton { kind: "flat"; text: pc.tr("Folder…"); onClicked: pc.dlg.importSoundPack(true) }
        }
        PrefSwitch { first: false; title: pc.tr("Background music"); subtitle: pc.tr("Loops on the menus; stops in the player, in Study and when the window is not in front"); checked: pc.g.pad_music !== false; onToggled: function(on) { pc.set("general.pad_music", on) } }
        PrefSlider {
            id: volRow
            title: pc.tr("Sound volume")
            from: 0; to: 100; step: 5
            property real dragged: -1
            value: pc.g.sound_volume !== undefined ? pc.g.sound_volume : 60
            // a drag (or the controller's left / right, which sends no release) is saved once it rests
            onMoved: function(v) { dragged = v; volSave.restart() }
            onReleased: volSave.restart()
            Timer { id: volSave; interval: 250; onTriggered: { if (volRow.dragged >= 0) pc.set("general.sound_volume", Math.round(volRow.dragged)); volRow.dragged = -1 } }
        }
        Repeater {
            model: [{slot: "nav", title: "Navigation sound", none: "Built-in sound"},
                    {slot: "select", title: "Select sound", none: "Built-in sound"},
                    {slot: "music", title: "Music file", none: "No music: pick a file"}]
            delegate: PrefRow {
                id: soundRow
                required property var modelData
                readonly property string file: pc.g["sound_" + modelData.slot] || ""
                first: false
                title: pc.tr(modelData.title)
                subtitle: file !== "" ? file.split("/").pop() : pc.tr(modelData.none)
                FlatButton { text: pc.tr("Choose…"); onClicked: pc.dlg.pickSound(soundRow.modelData.slot) }
                FlatButton { visible: soundRow.file !== ""; kind: "flat"; text: pc.tr("Clear"); onClicked: Core.cmd({cmd: "set_ui_sound", slot: soundRow.modelData.slot, path: ""}) }
            }
        }
    }

    // ---- what the buttons do ----
    PrefsAdvanced { dlg: pc.dlg }
    Column {
        visible: pc.dlg ? pc.dlg.adv : false
        width: parent.width
        spacing: 26
        PrefGroup {
            title: pc.tr("Touchpad and sticks")
            PrefSwitch {
                title: pc.tr("Touchpad as a mouse")
                subtitle: pc.tr("PlayStation controllers: one finger moves the pointer, pressing the pad clicks, two fingers scroll")
                checked: pc.g.pad_touch_mouse !== false
                onToggled: function(on) { pc.set("general.pad_touch_mouse", on) }
            }
            PrefSlider {
                title: pc.tr("Touchpad speed")
                from: 20; to: 300; step: 10
                property real dragged: -1
                value: pc.g.pad_touch_speed || 100
                onMoved: function(v) { dragged = v }
                onReleased: { if (dragged >= 0) pc.set("general.pad_touch_speed", Math.round(dragged)); dragged = -1 }
            }
            PrefSlider {
                title: pc.tr("Stick deadzone")
                from: 5; to: 60; step: 5
                property real dragged: -1
                value: pc.g.pad_deadzone || 30
                onMoved: function(v) { dragged = v }
                onReleased: { if (dragged >= 0) pc.set("general.pad_deadzone", Math.round(dragged)); dragged = -1 }
            }
            PrefSwitch {
                title: pc.tr("Zoom with the right stick")
                subtitle: pc.tr("Off: L2 / R2 zoom the image reader and size the text reader, and the right stick only scrolls")
                checked: pc.g.pad_zoom === "stick"
                onToggled: function(on) { pc.set("general.pad_zoom", on ? "stick" : "triggers") }
            }
            PrefSlider {
                title: pc.tr("Repeat speed of a held direction")
                unit: " ms"
                from: 40; to: 300; step: 10
                property real dragged: -1
                value: pc.g.pad_repeat || 90
                onMoved: function(v) { dragged = v }
                onReleased: { if (dragged >= 0) pc.set("general.pad_repeat", Math.round(dragged)); dragged = -1 }
            }
            PrefSwitch {
                title: pc.tr("Vibrate when the focus moves")
                subtitle: pc.tr("A short buzz of PlayStation controllers")
                checked: pc.g.pad_rumble === true
                onToggled: function(on) { pc.set("general.pad_rumble", on); if (on) Core.padRumble(110, 0, 45) }
            }
        }

        // ---- look and focus sound ----
        PrefGroup {
            title: pc.tr("Buttons")
            description: pc.tr("The bar of hints on each screen shows the buttons that work there; it fades after a few seconds and comes back with any press.")
            Repeater {
                model: [
                    {title: "Everywhere", hints: [{button: "LS", text: "Move (menus, buttons, modes)"}, {button: "Pad", text: "Scroll the page"}, {button: "A", text: "Select (on a text box: keyboard)"}, {button: "B", text: "Back / close"}, {button: "RB", text: "Next tab or section (L1: previous)"}, {button: "RT", text: "Page down (L2: up)"}, {button: "Start", text: "Settings"}]},
                    {title: "Readers", hints: [{button: "Pad", text: "Turn the page / scroll"}, {button: "LS", text: "Tools"}, {button: "RT", text: "Zoom in / bigger text (or next page)"}, {button: "RB", text: "Next chapter"}, {button: "X", text: "Editor"}, {button: "Y", text: "Themes & Settings"}, {button: "R3", text: "Translation"}, {button: "Select", text: "Study card"}, {button: "L3", text: "Upscaling"}]},
                    {title: "Study review", hints: [{button: "A", text: "Show the answer, then Good"}, {button: "X", text: "Replay, then Again"}, {button: "Y", text: "Hard"}, {button: "B", text: "Easy"}]},
                    {title: "Player", hints: [{button: "A", text: "Play / pause"}, {button: "Pad", text: "Seek and volume"}, {button: "RB", text: "Previous / next episode"}, {button: "RT", text: "One minute"}, {button: "Y", text: "Subtitles and audio"}, {button: "X", text: "X-Ray"}]}
                ]
                delegate: PrefRow {
                    id: mapRow
                    required property var modelData
                    first: false
                    title: pc.tr(modelData.title)
                    PadHints {
                        vertical: true
                        kind: pc.pad ? pc.pad.kind : "ps4"
                        hints: mapRow.modelData.hints.map(function(h) { return {button: h.button, text: pc.tr(h.text)} })
                    }
                }
            }
        }
    }

}
