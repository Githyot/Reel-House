import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// Karaoke studio: one window with every setting of the karaoke lyrics (the style fields of the ASS file that Karaoke
// Mugen writes, plus the timing) and a live preview. Changes are saved at once and show in the player while an opening
// or ending is on screen. Opened from Preferences > Subtitles & karaoke and from the player's Karaoke menu.
Popup {
    id: ks
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(1040, (parent ? parent.width : 1000) - 40)
    height: Math.min(720, (parent ? parent.height : 700) - 40)
    modal: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    Overlay.modal: Rectangle { color: "#80000000" }
    onAboutToShow: Core.refreshPrefs()
    onClosed: Core.cmd({cmd: "save_settings"})
    background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }

    readonly property var p: Core.prefs
    readonly property var g: p.general !== undefined ? p.general : ({})
    readonly property var k: (Core.player && Core.player.karaoke) ? Core.player.karaoke : ({})
    readonly property bool wide: width > 760
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function set(key, v) { Core.cmd({cmd: "set_setting", path: "general." + key, value: v}) }
    function setMany(o) { for (var key in o) set(key, o[key]) }
    function val(key, d) { return ks.g[key] !== undefined && ks.g[key] !== null ? ks.g[key] : d }
    function hexOf(c) {
        function h(x) { var s = Math.round(x * 255).toString(16); return s.length < 2 ? "0" + s : s }
        return "#" + h(c.r) + h(c.g) + h(c.b)
    }

    // What the file keeps when nothing is chosen.
    readonly property var defaults: ({
        karaoke_top: true, karaoke_size: 100, karaoke_margin: 0, karaoke_color: "", karaoke_font: "", karaoke_bold: "",
        karaoke_outline: -1, karaoke_shadow: -1, karaoke_unfilled: "", karaoke_italic: "", karaoke_underline: "",
        karaoke_outline_color: "", karaoke_back_color: "", karaoke_scale_x: 0, karaoke_scale_y: 0, karaoke_spacing: -100,
        karaoke_align: 0, karaoke_margin_l: 0, karaoke_margin_r: 0, karaoke_border: ""
    })
    readonly property var presets: [
        {name: "As the file has it", v: ({})},
        {name: "Classic fansub", v: ({karaoke_color: "#ffd24a", karaoke_unfilled: "#ffffff", karaoke_outline_color: "#1a1a1a", karaoke_back_color: "#000000", karaoke_outline: 3, karaoke_shadow: 1, karaoke_bold: "on", karaoke_border: "outline"})},
        {name: "Neon", v: ({karaoke_color: "accent", karaoke_unfilled: "#bfe7ff", karaoke_outline_color: "#00131f", karaoke_back_color: "#00131f", karaoke_outline: 4, karaoke_shadow: 0, karaoke_bold: "on", karaoke_border: "outline"})},
        {name: "Readable box", v: ({karaoke_color: "#ffffff", karaoke_unfilled: "#9aa0a6", karaoke_back_color: "#000000", karaoke_outline: 0, karaoke_shadow: 0, karaoke_bold: "on", karaoke_border: "box"})},
        {name: "Big and centred", v: ({karaoke_size: 140, karaoke_align: 5, karaoke_bold: "on", karaoke_outline: 4, karaoke_shadow: 2})}
    ]
    function applyPreset(i) { var o = {}; for (var key in defaults) o[key] = defaults[key]; var v = presets[i].v; for (var q in v) o[q] = v[q]; setMany(o) }

    // ---- values the preview needs ----
    readonly property color mainColor: { var c = val("karaoke_color", ""); return c === "" ? "#ff9f1c" : (c === "accent" ? Theme.c.accent : c) }
    readonly property color restColor: { var c = val("karaoke_unfilled", ""); return c !== "" ? c : Qt.lighter(mainColor, 1.9) }
    readonly property color edgeColor: { var c = val("karaoke_outline_color", ""); return c !== "" ? c : "#101010" }
    readonly property color backColor: { var c = val("karaoke_back_color", ""); return c !== "" ? c : "#000000" }
    readonly property int align: val("karaoke_align", 0) > 0 ? val("karaoke_align", 0) : (val("karaoke_top", true) ? 8 : 2)

    // A small colour button that opens the colour dialog.
    component ColorRow: PrefRow {
        id: cr
        property string value: ""          // "" = the file's own
        property string fallback: "#ffffff"
        signal chosen(string hex)
        signal cleared()
        FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flatround"; glyph: "undo"; visible: cr.value !== ""; ToolTip.visible: hovered; ToolTip.text: ks.tr("Back to the file's own"); onClicked: cr.cleared() }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 46; height: 32; radius: 8
            color: crArea.containsMouse ? Theme.c.buttonHover : Theme.c.button
            Rectangle { anchors.centerIn: parent; width: 26; height: 18; radius: 4; color: cr.value !== "" ? cr.value : cr.fallback; opacity: cr.value !== "" ? 1 : 0.45; border.width: 1; border.color: "#59000000" }
            MouseArea { id: crArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { colorDlg.target = cr; colorDlg.selectedColor = cr.value !== "" ? cr.value : cr.fallback; colorDlg.open() } }
        }
    }

    // The 3×3 grid of places (numpad layout of ASS \an), used beside the preview and in the narrow layout.
    component AlignGrid: Grid {
        columns: 3
        spacing: 4
        Repeater {
            model: [7, 8, 9, 4, 5, 6, 1, 2, 3]
            delegate: Rectangle {
                required property int modelData
                width: 44; height: 34; radius: 7
                readonly property bool on: ks.val("karaoke_align", 0) === modelData
                color: on ? Theme.c.accent : (gArea.containsMouse ? Theme.c.buttonHover : Theme.c.button)
                Rectangle { anchors.centerIn: parent; width: 12; height: 12; radius: 6; color: parent.on ? Theme.c.accentFg : Theme.c.textDim }
                MouseArea { id: gArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: ks.set("karaoke_align", modelData) }
            }
        }
    }

    contentItem: Item {
        // ---- header ----
        DialogHeader {
            id: head
            title: ks.tr("Karaoke studio")
            divider: true
            onClose: ks.close()
            FlatButton { text: ks.tr("Reset everything"); glyph: "undo"; implicitHeight: 34; onClicked: ks.applyPreset(0) }
        }

        // ---- left: the preview, the place and the presets ----
        Column {
            id: left
            x: 24; y: head.height + 16
            width: ks.wide ? 380 : parent.width - 48
            spacing: 14
            visible: ks.wide

            Rectangle {
                id: screen
                width: parent.width
                height: Math.round(width * 9 / 16)
                radius: 10
                clip: true
                gradient: Gradient {
                    GradientStop { position: 0; color: "#16213e" }
                    GradientStop { position: 0.55; color: "#3a2a5c" }
                    GradientStop { position: 1; color: "#b4552d" }
                }
                readonly property real f: height / 600     // the preview is drawn as if the script were 600 tall, so the letters are readable
                readonly property real fs: 52 * (ks.val("karaoke_size", 100) / 100) * (ks.val("karaoke_scale_y", 0) > 0 ? ks.val("karaoke_scale_y", 0) / 100 : 1) * f * 1.0
                readonly property real edge: Math.max(0, ks.val("karaoke_outline", -1) < 0 ? 2 : ks.val("karaoke_outline", -1)) * f
                readonly property real drop: Math.max(0, ks.val("karaoke_shadow", -1) < 0 ? 1 : ks.val("karaoke_shadow", -1)) * f
                readonly property bool boxed: ks.val("karaoke_border", "") === "box"
                readonly property string fam: ks.val("karaoke_font", "") !== "" ? ks.val("karaoke_font", "") : Theme.type.family
                readonly property bool bold: ks.val("karaoke_bold", "") !== "off"
                readonly property bool ital: ks.val("karaoke_italic", "") === "on"
                readonly property bool under: ks.val("karaoke_underline", "") === "on"
                readonly property real sp: ks.val("karaoke_spacing", -100) > -100 ? ks.val("karaoke_spacing", -100) * f : 0

                // The frame of a video: a dim "scene".
                Text { anchors.centerIn: parent; text: "▶"; color: "#22ffffff"; font.pixelSize: parent.height * 0.5 }

                Item {
                    id: lyric
                    readonly property int col: (ks.align - 1) % 3          // 0 left, 1 centre, 2 right
                    readonly property int row: Math.floor((ks.align - 1) / 3)   // 0 bottom, 1 middle, 2 top
                    readonly property real mv: (ks.val("karaoke_margin", 0) > 0 ? ks.val("karaoke_margin", 0) : 24) * screen.f
                    readonly property real ml: (ks.val("karaoke_margin_l", 0) > 0 ? ks.val("karaoke_margin_l", 0) : 40) * screen.f
                    readonly property real mr: (ks.val("karaoke_margin_r", 0) > 0 ? ks.val("karaoke_margin_r", 0) : 40) * screen.f
                    width: Math.min(parent.width - ml - mr, box.width + 16)
                    height: box.height + 8
                    x: col === 0 ? ml : (col === 2 ? parent.width - mr - width : (parent.width - width) / 2)
                    y: row === 2 ? mv : (row === 0 ? parent.height - mv - height : (parent.height - height) / 2)
                    Rectangle { anchors.fill: parent; radius: 4; color: screen.boxed ? ks.backColor : "transparent" }
                    Item {
                        id: box
                        anchors.centerIn: parent
                        width: base.implicitWidth * (ks.val("karaoke_scale_x", 0) > 0 ? ks.val("karaoke_scale_x", 0) / 100 : 1)
                        height: base.implicitHeight
                        // shadow
                        Text { visible: !screen.boxed && screen.drop > 0; x: screen.drop; y: screen.drop; text: base.text; font: base.font; color: ks.backColor; opacity: 0.85 }
                        // the part still to be sung, with its border
                        Text { id: base; text: "さくら ひらひら Sakura"; font.family: screen.fam; font.pixelSize: Math.max(8, screen.fs); font.bold: screen.bold; font.italic: screen.ital; font.underline: screen.under; font.letterSpacing: screen.sp
                            color: ks.restColor; style: screen.boxed || screen.edge <= 0 ? Text.Normal : Text.Outline; styleColor: ks.edgeColor
                            transform: Scale { xScale: ks.val("karaoke_scale_x", 0) > 0 ? ks.val("karaoke_scale_x", 0) / 100 : 1 } }
                        // the part already sung (the first half)
                        Item {
                            width: parent.width * 0.5
                            height: parent.height
                            clip: true
                            Text { text: base.text; font: base.font; color: ks.mainColor; style: base.style; styleColor: ks.edgeColor
                                transform: Scale { xScale: ks.val("karaoke_scale_x", 0) > 0 ? ks.val("karaoke_scale_x", 0) / 100 : 1 } }
                        }
                    }
                }
            }

            // Where the lyrics sit.
            Text { text: ks.tr("Place on the screen"); color: Theme.c.text; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: Theme.type.family }
            Row {
                spacing: 14
                AlignGrid { }
                Column {
                    spacing: 8
                    anchors.verticalCenter: parent.verticalCenter
                    FlatButton { text: ks.tr("Automatic"); kind: ks.val("karaoke_align", 0) === 0 ? "suggested" : "plain"; implicitHeight: 32; onClicked: ks.set("karaoke_align", 0) }
                    Text { width: 190; wrapMode: Text.Wrap; text: ks.tr("Automatic follows the file (or the top of the screen, if you chose that)"); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                }
            }

            // Looks to start from.
            Text { text: ks.tr("Looks to start from"); color: Theme.c.text; font.pixelSize: 14; font.weight: Font.DemiBold; font.family: Theme.type.family }
            Flow {
                width: parent.width
                spacing: 6
                Repeater {
                    model: ks.presets
                    delegate: FlatButton { required property var modelData; required property int index; text: ks.tr(modelData.name); implicitHeight: 32; onClicked: ks.applyPreset(index) }
                }
            }
        }

        // ---- right: every setting ----
        Flickable {
            id: scroll
            x: ks.wide ? 24 + 380 + 20 : 0
            y: head.height
            width: parent.width - x - (ks.wide ? 24 : 0)
            height: parent.height - head.height
            contentHeight: settingsCol.implicitHeight + 40
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Column {
                // not `col`: the preview's `lyric` has a `col` property, and an id would win the lookup there
                id: settingsCol
                x: ks.wide ? 0 : 24
                y: 16
                width: parent.width - (ks.wide ? 0 : 48)
                spacing: 22

                PrefGroup {
                    title: ks.tr("Lyrics")
                    PrefSwitch { first: true; title: ks.tr("Karaoke on openings and endings"); subtitle: (ks.k.status || ""); checked: ks.val("karaoke", true) === true; onToggled: function(on) { ks.set("karaoke", on); Core.cmd({cmd: "karaoke_on", on: on}) } }
                    PrefButton { title: ks.tr("Search the karaoke again"); subtitle: ks.tr("While an opening or ending of an episode is open"); label: ks.tr("Search"); onActivated: Core.cmd({cmd: "karaoke_retry"}) }
                }

                PrefGroup {
                    title: ks.tr("Timing")
                    description: ks.tr("Lyrics usually come a little late. A negative number shows them earlier.")
                    PrefSpin { first: true; title: ks.tr("Delay of all the karaokes"); subtitle: ks.tr("Seconds"); from: -15; to: 15; step: 0.1; value: ks.val("karaoke_delay", 0); onMoved: function(v) { ks.set("karaoke_delay", Math.round(v * 10) / 10) } }
                    PrefSpin {
                        visible: (ks.k.songKey || "") !== ""
                        title: ks.tr("Delay of this song only"); subtitle: ks.tr("Seconds, on top of the one above"); from: -15; to: 15; step: 0.1; value: ks.k.songDelay || 0
                        onMoved: function(v) { Core.cmd({cmd: "karaoke_set_song_delay", key: ks.k.songKey, value: Math.round(v * 10) / 10}) }
                    }
                }

                PrefGroup {
                    title: ks.tr("Letters")
                    PrefRow {
                        first: true
                        title: ks.tr("Font")
                        subtitle: ks.val("karaoke_font", "") !== "" ? ks.val("karaoke_font", "") : ks.tr("As the file has it")
                        FlatButton { anchors.verticalCenter: parent.verticalCenter; text: ks.tr("Choose…"); implicitHeight: 34; onClicked: fontDlg.open() }
                        FlatButton { anchors.verticalCenter: parent.verticalCenter; kind: "flatround"; glyph: "undo"; ToolTip.visible: hovered; ToolTip.text: ks.tr("Back to the file's own"); onClicked: ks.set("karaoke_font", "") }
                    }
                    PrefSpin { title: ks.tr("Size"); subtitle: ks.tr("Percentage of the size the file asks for"); from: 40; to: 300; step: 5; value: ks.val("karaoke_size", 100); onMoved: function(v) { ks.set("karaoke_size", v) } }
                    PrefCombo {
                        readonly property var ids: ["", "on", "off"]
                        title: ks.tr("Bold")
                        options: [ks.tr("As the file has it"), ks.tr("Bold"), ks.tr("Regular")]
                        current: Math.max(0, ids.indexOf(ks.val("karaoke_bold", "")))
                        onPicked: function(i) { ks.set("karaoke_bold", ids[i]) }
                    }
                    PrefCombo {
                        readonly property var ids: ["", "on", "off"]
                        title: ks.tr("Italic")
                        options: [ks.tr("As the file has it"), ks.tr("Italic"), ks.tr("Upright")]
                        current: Math.max(0, ids.indexOf(ks.val("karaoke_italic", "")))
                        onPicked: function(i) { ks.set("karaoke_italic", ids[i]) }
                    }
                    PrefCombo {
                        readonly property var ids: ["", "on", "off"]
                        title: ks.tr("Underline")
                        options: [ks.tr("As the file has it"), ks.tr("Underlined"), ks.tr("No underline")]
                        current: Math.max(0, ids.indexOf(ks.val("karaoke_underline", "")))
                        onPicked: function(i) { ks.set("karaoke_underline", ids[i]) }
                    }
                    PrefSpin { title: ks.tr("Width of the letters"); subtitle: ks.tr("Percent. 0 keeps the file's own"); from: 0; to: 300; step: 5; value: ks.val("karaoke_scale_x", 0); onMoved: function(v) { ks.set("karaoke_scale_x", v) } }
                    PrefSpin { title: ks.tr("Height of the letters"); subtitle: ks.tr("Percent. 0 keeps the file's own"); from: 0; to: 300; step: 5; value: ks.val("karaoke_scale_y", 0); onMoved: function(v) { ks.set("karaoke_scale_y", v) } }
                    PrefSpin { title: ks.tr("Space between letters"); subtitle: ks.tr("Pixels of the script. -100 keeps the file's own"); from: -100; to: 40; step: 1; value: ks.val("karaoke_spacing", -100); onMoved: function(v) { ks.set("karaoke_spacing", v) } }
                }

                PrefGroup {
                    title: ks.tr("Colours")
                    PrefCombo {
                        first: true
                        title: ks.tr("Colour of the lyrics"); subtitle: ks.tr("The part already sung")
                        options: [ks.tr("As the file has it"), ks.tr("Accent colour of the app"), ks.tr("Own colour")]
                        current: ks.val("karaoke_color", "") === "" ? 0 : (ks.val("karaoke_color", "") === "accent" ? 1 : 2)
                        onPicked: function(i) { ks.set("karaoke_color", i === 0 ? "" : (i === 1 ? "accent" : (ownCol.value !== "" ? ownCol.value : "#ff9f1c"))) }
                    }
                    ColorRow {
                        id: ownCol
                        title: ks.tr("Own colour"); subtitle: ks.tr("Used when the choice above is \"Own colour\"")
                        value: ks.val("karaoke_color", "").charAt(0) === "#" ? ks.val("karaoke_color", "") : ""
                        fallback: "#ff9f1c"
                        onChosen: function(h) { ks.set("karaoke_color", h) }
                        onCleared: ks.set("karaoke_color", "")
                    }
                    ColorRow {
                        title: ks.tr("Colour before it is sung"); subtitle: ks.tr("The part still to be sung")
                        value: ks.val("karaoke_unfilled", "")
                        fallback: "#ffffff"
                        onChosen: function(h) { ks.set("karaoke_unfilled", h) }
                        onCleared: ks.set("karaoke_unfilled", "")
                    }
                    ColorRow {
                        title: ks.tr("Colour of the border"); subtitle: ks.tr("Around the letters")
                        value: ks.val("karaoke_outline_color", "")
                        fallback: "#101010"
                        onChosen: function(h) { ks.set("karaoke_outline_color", h) }
                        onCleared: ks.set("karaoke_outline_color", "")
                    }
                    ColorRow {
                        title: ks.tr("Colour of the shadow or box"); subtitle: ks.tr("Under the letters, or behind them in box mode")
                        value: ks.val("karaoke_back_color", "")
                        fallback: "#000000"
                        onChosen: function(h) { ks.set("karaoke_back_color", h) }
                        onCleared: ks.set("karaoke_back_color", "")
                    }
                }

                PrefGroup {
                    title: ks.tr("Border and shadow")
                    PrefCombo {
                        readonly property var ids: ["", "outline", "box"]
                        first: true
                        title: ks.tr("Style")
                        options: [ks.tr("As the file has it"), ks.tr("Border and shadow"), ks.tr("Box behind the letters")]
                        current: Math.max(0, ids.indexOf(ks.val("karaoke_border", "")))
                        onPicked: function(i) { ks.set("karaoke_border", ids[i]) }
                    }
                    PrefSpin { title: ks.tr("Border"); subtitle: ks.tr("Pixels of the script. -1 keeps the file's own"); from: -1; to: 12; step: 1; value: ks.val("karaoke_outline", -1); onMoved: function(v) { ks.set("karaoke_outline", v) } }
                    PrefSpin { title: ks.tr("Shadow"); subtitle: ks.tr("Pixels of the script. -1 keeps the file's own"); from: -1; to: 12; step: 1; value: ks.val("karaoke_shadow", -1); onMoved: function(v) { ks.set("karaoke_shadow", v) } }
                }

                PrefGroup {
                    title: ks.tr("Distance from the edges")
                    description: ks.tr("In pixels of the lyrics script (usually 1080 tall). 0 keeps the file's own.")
                    PrefSpin { first: true; title: ks.tr("From the top or bottom"); from: 0; to: 500; step: 5; value: ks.val("karaoke_margin", 0); onMoved: function(v) { ks.set("karaoke_margin", v) } }
                    PrefSpin { title: ks.tr("From the left"); from: 0; to: 800; step: 5; value: ks.val("karaoke_margin_l", 0); onMoved: function(v) { ks.set("karaoke_margin_l", v) } }
                    PrefSpin { title: ks.tr("From the right"); from: 0; to: 800; step: 5; value: ks.val("karaoke_margin_r", 0); onMoved: function(v) { ks.set("karaoke_margin_r", v) } }
                }

                // On a narrow window the places and looks sit here, under the settings.
                PrefGroup {
                    visible: !ks.wide
                    title: ks.tr("Place on the screen")
                    PrefRow {
                        first: true
                        title: ks.tr("Automatic")
                        subtitle: ks.tr("Automatic follows the file (or the top of the screen, if you chose that)")
                        FlatButton { anchors.verticalCenter: parent.verticalCenter; text: ks.tr("Automatic"); kind: ks.val("karaoke_align", 0) === 0 ? "suggested" : "plain"; implicitHeight: 32; onClicked: ks.set("karaoke_align", 0) }
                        AlignGrid { anchors.verticalCenter: parent.verticalCenter }
                    }
                }
                PrefGroup {
                    visible: !ks.wide
                    title: ks.tr("Looks to start from")
                    Repeater {
                        model: ks.presets
                        delegate: PrefButton { required property var modelData; required property int index; title: ks.tr(modelData.name); label: ks.tr("Use"); onActivated: ks.applyPreset(index) }
                    }
                }
            }
        }
    }

    FontDialog {
        id: fontDlg
        title: ks.tr("Font of the lyrics")
        onAccepted: ks.set("karaoke_font", selectedFont.family)
    }
    ColorPickerAdapter {
        id: colorDlg
        property var target
        // only emits: `value` stays bound to the setting, which the saved colour updates
        onAccepted: if (target) target.chosen(ks.hexOf(selectedColor))
    }
}
