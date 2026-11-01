import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "ReaderThemes.js" as RT

// "Themes & Settings" of the Text reader (Apple Books): text size, page turn, scrolling or pages, six themes, and a
// second page "Customize" with the font, spacing, margins, justification, bold text and the colours chosen by hand.
ReaderSheet {
    id: ls
    property var rd                         // the reader (NovelReaderPage)
    property bool customizing: false
    readonly property var g: Core.prefs.general || ({})
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function setting(k, v) { Core.cmd({cmd: "set_setting", path: "general." + k, value: v}) }
    title: customizing ? tr("Customize") : tr("Themes & Settings")
    canBack: customizing
    onBack: customizing = false
    onClosed: customizing = false
    sheetWidth: 400

    // ================= first page =================
    // ---- text size: a smaller A and a bigger A ----
    Item {
        visible: !ls.customizing
        width: parent.width
        height: 44
        Rectangle {
            id: sizeBox
            width: parent.width
            height: 44
            radius: Theme.look === "apple" ? 12 : (Theme.look === "android" ? 22 : (Theme.look === "windows" ? 4 : 10))
            color: Theme.look === "android" ? Theme.c.accent20 : Theme.c.fill
            border.width: Theme.look === "windows" ? 1 : 0
            border.color: Theme.c.controlStroke
            Row {
                anchors.fill: parent
                Repeater {
                    model: [{d: -1, px: 15}, {d: 1, px: 24}]
                    delegate: Item {
                        required property var modelData
                        required property int index
                        width: sizeBox.width / 2
                        height: sizeBox.height
                        Rectangle { anchors.fill: parent; anchors.margins: 3; radius: sizeBox.radius - 2; color: Theme.c.hover; visible: aArea.containsMouse }
                        Text {
                            anchors.centerIn: parent
                            text: "A"
                            color: Theme.c.text
                            font.family: Theme.type.family
                            font.pixelSize: modelData.px
                            font.weight: Font.DemiBold
                        }
                        MouseArea {
                            id: aArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: ls.setting("novel_size", Math.max(12, Math.min(40, (ls.g.novel_size || 19) + modelData.d)))
                        }
                    }
                }
            }
            Rectangle { anchors.centerIn: parent; width: 1; height: parent.height - 16; color: Theme.c.divider }
        }
        Text {
            anchors { right: sizeBox.right; rightMargin: 10; bottom: sizeBox.bottom; bottomMargin: 4 }
            text: (ls.g.novel_size || 19) + " px"
            color: Theme.c.textDim
            font.family: Theme.type.family
            font.pixelSize: 10
        }
    }

    // ---- scrolling or pages, and how a page turns ----
    Column {
        visible: !ls.customizing
        width: parent.width
        spacing: 6
        Lbl { text: ls.tr("Layout"); dim: true; size: 12 }
        SheetSegments {
            model: [{id: "scroll", label: ls.tr("Scrolling"), glyph: "down"}, {id: "pages", label: ls.tr("Pages"), glyph: "episodes"}]
            current: ls.g.text_pages ? "pages" : "scroll"
            onPicked: function(id) { ls.setting("text_pages", id === "pages") }
        }
    }
    Column {
        visible: !ls.customizing
        width: parent.width
        spacing: 6
        opacity: ls.g.text_pages ? 1 : 0.45
        enabled: !!ls.g.text_pages
        Lbl { text: ls.tr("Page turn"); dim: true; size: 12 }
        SheetSegments {
            model: [{id: "slide", label: ls.tr("Slide")}, {id: "curl", label: ls.tr("Curl")}, {id: "none", label: ls.tr("None")}]
            current: ls.g.text_turn || "slide"
            onPicked: function(id) { ls.setting("text_turn", id) }
        }
    }

    // ---- the six themes ----
    Grid {
        visible: !ls.customizing
        width: parent.width
        columns: 3
        spacing: 10
        Repeater {
            model: RT.textOrder
            delegate: ThemeCard {
                required property var modelData
                readonly property var look: RT.textLook(modelData, ls.rd ? ls.rd.night : false)
                width: (parent.width - 20) / 3
                name: ls.tr(RT.text[modelData].name)
                bg: look.bg
                fg: look.fg
                family: look.family
                heavy: look.heavy
                grain: look.grain
                selected: (ls.g.text_theme || "") === modelData
                onClicked: ls.setting("text_theme", modelData)
            }
        }
    }

    // ---- to the second page ----
    Rectangle {
        visible: !ls.customizing
        width: parent.width
        height: 48
        radius: Theme.look === "apple" ? 12 : (Theme.look === "android" ? 24 : Theme.shape.control)
        color: custArea.containsMouse ? Theme.c.cardHover : Theme.c.card
        border.width: Theme.look === "windows" || Theme.look === "gnome" ? 1 : 0
        border.color: Theme.c.border
        Row {
            anchors { left: parent.left; leftMargin: 14; verticalCenter: parent.verticalCenter }
            spacing: 10
            Icon { name: "settings"; size: 18; anchors.verticalCenter: parent.verticalCenter }
            Lbl { text: ls.tr("Customize"); size: 15; anchors.verticalCenter: parent.verticalCenter }
        }
        Lbl {
            anchors { right: chev.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
            text: (ls.g.text_theme || "") === "" ? ls.tr("Custom colours") : ls.tr(RT.text[ls.g.text_theme] ? RT.text[ls.g.text_theme].name : "")
            dim: true; size: 13
        }
        Icon { id: chev; anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter } name: "right"; size: 18; color: Theme.c.textDim }
        MouseArea { id: custArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: ls.customizing = true }
    }

    // ================= second page: Customize =================
    // ---- the font: each name written in its own font ----
    Column {
        visible: ls.customizing
        width: parent.width
        spacing: 8
        Lbl { text: ls.tr("Font"); dim: true; size: 12 }
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: RT.fonts
                delegate: Rectangle {
                    required property var modelData
                    readonly property bool on: (ls.g.text_font || "") === modelData
                    width: fl.implicitWidth + 28
                    height: 36
                    radius: Theme.look === "windows" ? 4 : (Theme.look === "android" ? 8 : 18)
                    color: on ? (Theme.look === "android" ? Theme.c.accent20 : Theme.c.accent) : (fArea.containsMouse ? Theme.c.buttonHover : Theme.c.button)
                    border.width: Theme.look === "android" && !on ? 1 : 0
                    border.color: Theme.c.textDim
                    Text {
                        id: fl
                        anchors.centerIn: parent
                        text: modelData === "" ? ls.tr("Original") : modelData
                        color: parent.on && Theme.look !== "android" ? Theme.c.accentFg : Theme.c.text
                        font.family: modelData === "" ? Theme.type.family : modelData
                        font.pixelSize: 15
                    }
                    MouseArea { id: fArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: ls.setting("text_font", modelData) }
                }
            }
        }
        PrefCombo {
            title: ls.tr("Other fonts"); first: true
            property var fams: [""].concat(Qt.fontFamilies())
            options: [ls.tr("Like the theme")].concat(Qt.fontFamilies())
            current: Math.max(0, fams.indexOf(ls.g.text_font || ""))
            onPicked: function(i) { ls.setting("text_font", fams[i]) }
        }
    }
    PrefGroup {
        visible: ls.customizing
        title: ls.tr("Text")
        PrefSwitch { title: ls.tr("Bold text"); checked: !!ls.g.text_bold; onToggled: function(on) { ls.setting("text_bold", on) } }
        PrefSwitch { title: ls.tr("Justify"); checked: (ls.g.text_align || "start") === "justify"; onToggled: function(on) { ls.setting("text_align", on ? "justify" : "start") } }
        PrefSlider { title: ls.tr("Line spacing"); first: false; from: 100; to: 300; step: 5; value: ls.g.text_line || 170; onMoved: function(v) { ls.setting("text_line", Math.round(v)) } }
        PrefSlider { title: ls.tr("Character spacing"); first: false; from: 0; to: 50; step: 1; unit: ""; value: ls.g.text_letter || 0; onMoved: function(v) { ls.setting("text_letter", Math.round(v)) } }
        PrefSlider { title: ls.tr("Word spacing"); first: false; from: 0; to: 30; step: 1; unit: " px"; value: ls.g.text_word || 0; onMoved: function(v) { ls.setting("text_word", Math.round(v)) } }
        PrefSlider { title: ls.tr("Margins"); subtitle: ls.tr("Width of the column of text"); first: false; from: 360; to: 1600; step: 20; unit: " px"; value: ls.g.text_width || 760; onMoved: function(v) { ls.setting("text_width", Math.round(v)) } }
        PrefSlider { title: ls.tr("Gap between paragraphs"); first: false; from: 0; to: 60; step: 1; unit: " px"; value: ls.g.text_para_gap !== undefined ? ls.g.text_para_gap : 14; onMoved: function(v) { ls.setting("text_para_gap", Math.round(v)) } }
        PrefSlider { title: ls.tr("Indent"); first: false; from: 0; to: 120; step: 2; unit: " px"; value: ls.g.text_indent || 0; onMoved: function(v) { ls.setting("text_indent", Math.round(v)) } }
    }
    PrefGroup {
        visible: ls.customizing
        title: ls.tr("Headings")
        PrefSlider { title: ls.tr("Size of the headings"); from: 80; to: 300; step: 5; value: ls.g.text_head_size || 135; onMoved: function(v) { ls.setting("text_head_size", Math.round(v)) } }
        PrefSwitch { title: ls.tr("Centred headings"); first: false; checked: !!ls.g.text_head_center; onToggled: function(on) { ls.setting("text_head_center", on) } }
    }
    PrefGroup {
        visible: ls.customizing
        title: ls.tr("Colours")
        description: ls.tr("Choosing a colour here leaves the themes for colours of your own.")
        PrefRow {
            title: ls.tr("Background")
            Row {
                spacing: 6
                Repeater {
                    model: [{k: "app", c: Theme.c.window}, {k: "white", c: "#ffffff"}, {k: "sepia", c: "#fbf0d9"}, {k: "gray", c: "#e4e4e4"}, {k: "dark", c: "#1f1f23"}, {k: "black", c: "#000000"}]
                    delegate: Rectangle {
                        required property var modelData
                        readonly property bool on: (ls.g.text_theme || "") === "" && (ls.g.text_bg || "app") === modelData.k
                        width: 26; height: 26; radius: 13
                        color: modelData.c
                        border.width: on ? 3 : 1
                        border.color: on ? Theme.c.accent : Theme.c.border
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { ls.setting("text_bg", modelData.k); ls.setting("text_theme", "") } }
                    }
                }
            }
        }
        PrefRow {
            title: ls.tr("Text colour"); first: false
            Row {
                spacing: 6
                Repeater {
                    model: [{k: "", c: "transparent"}, {k: "#1c1c1c", c: "#1c1c1c"}, {k: "#3b2f1e", c: "#3b2f1e"}, {k: "#8a8a8a", c: "#8a8a8a"}, {k: "#d8d8dc", c: "#d8d8dc"}, {k: "#ffffff", c: "#ffffff"}]
                    delegate: Rectangle {
                        required property var modelData
                        readonly property bool on: (ls.g.text_theme || "") === "" && (ls.g.text_fg || "") === modelData.k
                        width: 26; height: 26; radius: 13
                        color: modelData.c
                        border.width: on ? 3 : 1
                        border.color: on ? Theme.c.accent : Theme.c.border
                        Lbl { visible: modelData.k === ""; anchors.centerIn: parent; text: "A"; size: 12; bold: true }
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { ls.setting("text_fg", modelData.k); ls.setting("text_theme", "") } }
                    }
                }
            }
        }
    }
    PrefGroup {
        visible: ls.customizing
        title: ls.tr("Comfort")
        PrefSwitch { title: ls.tr("Night themes when the app is dark"); checked: ls.g.text_theme_night !== false; onToggled: function(on) { ls.setting("text_theme_night", on) } }
        PrefSlider { title: ls.tr("Night light"); first: false; from: 0; to: 100; step: 1; value: ls.g.red_light || 0; onMoved: function(v) { ls.setting("red_light", Math.round(v)) } }
    }
    FlatButton {
        visible: ls.customizing
        kind: "flat"
        glyph: "undo"
        text: ls.tr("Reset the text settings")
        onClicked: {
            const d = {text_font: "", text_bold: false, text_align: "start", text_line: 170, text_letter: 0, text_word: 0, text_width: 760, text_para_gap: 14, text_indent: 0, text_head_size: 135, text_head_center: false}
            for (const k in d) ls.setting(k, d[k])
        }
    }
}
