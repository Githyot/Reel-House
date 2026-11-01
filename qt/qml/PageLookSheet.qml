import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "ReaderThemes.js" as RT

// "Themes & Settings" of the Images reader: the colour around the pages, the reading direction, how a page turns,
// how a page fits, two pages side by side and the space between the pages of the strip; a second page has the rest
// (zoom, size of the translated text, automatic scrolling, buttons and tap zones).
ReaderSheet {
    id: ls
    property var rd                         // the reader (MangaReaderPage)
    property bool more: false
    readonly property var g: Core.prefs.general || ({})
    readonly property string mode: g.manga_mode || "rtl"
    readonly property bool strip: mode === "webtoon"
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function setting(k, v) { Core.cmd({cmd: "set_setting", path: "general." + k, value: v}) }
    title: more ? tr("More options") : tr("Themes & Settings")
    canBack: more
    onBack: more = false
    onClosed: more = false
    sheetWidth: 400

    // ================= first page =================
    Grid {
        visible: !ls.more
        width: parent.width
        columns: 3
        spacing: 10
        Repeater {
            model: RT.pageOrder
            delegate: ThemeCard {
                required property var modelData
                readonly property var p: RT.pages[modelData]
                width: (parent.width - 20) / 3
                name: ls.tr(p.name)
                bg: p.bg !== "" ? p.bg : Theme.c.window
                fg: p.fg !== "" ? p.fg : Theme.c.text
                grain: p.grain
                pageSample: true
                selected: (ls.g.reader_bg || "dark") === modelData
                onClicked: ls.setting("reader_bg", modelData)
            }
        }
    }
    Column {
        visible: !ls.more
        width: parent.width
        spacing: 6
        Lbl { text: ls.tr("Reading mode"); dim: true; size: 12 }
        SheetSegments {
            model: [{id: "rtl", label: ls.tr("Right to left")}, {id: "ltr", label: ls.tr("Left to right")}, {id: "vertical", label: ls.tr("Vertical")}, {id: "webtoon", label: ls.tr("Webtoon")}]
            current: ls.mode
            onPicked: function(id) { ls.setting("manga_mode", id) }
        }
    }
    Column {
        visible: !ls.more
        width: parent.width
        spacing: 6
        opacity: ls.strip ? 0.45 : 1
        enabled: !ls.strip
        Lbl { text: ls.tr("Page turn"); dim: true; size: 12 }
        SheetSegments {
            model: [{id: "slide", label: ls.tr("Slide")}, {id: "curl", label: ls.tr("Curl")}, {id: "none", label: ls.tr("None")}]
            current: ls.g.reader_turn || "slide"
            onPicked: function(id) { ls.setting("reader_turn", id) }
        }
    }
    Column {
        visible: !ls.more
        width: parent.width
        spacing: 6
        Lbl { text: ls.tr("Fit the page"); dim: true; size: 12 }
        SheetSegments {
            model: [{id: "screen", label: ls.tr("Page")}, {id: "width", label: ls.tr("Width")}, {id: "height", label: ls.tr("Height")}, {id: "original", label: ls.tr("Original")}]
            current: ls.g.reader_fit || "screen"
            onPicked: function(id) { ls.setting("reader_fit", id) }
        }
    }
    PrefGroup {
        visible: !ls.more
        PrefSwitch {
            title: ls.tr("Two pages side by side")
            subtitle: ls.tr("Like an open book; the cover and wide pages stay alone")
            enabled: !ls.strip && ls.mode !== "vertical"
            opacity: enabled ? 1 : 0.5
            checked: !!ls.g.reader_spread
            onToggled: function(on) { ls.setting("reader_spread", on) }
        }
        PrefSlider {
            title: ls.tr("Space between pages"); subtitle: ls.tr("Webtoon strip")
            first: false
            enabled: ls.strip
            opacity: enabled ? 1 : 0.5
            from: 0; to: 120; step: 2; unit: " px"
            value: ls.g.reader_gap || 0
            onMoved: function(v) { ls.setting("reader_gap", Math.round(v)) }
        }
    }
    Rectangle {
        visible: !ls.more
        width: parent.width
        height: 48
        radius: Theme.look === "apple" ? 12 : (Theme.look === "android" ? 24 : Theme.shape.control)
        color: moreArea.containsMouse ? Theme.c.cardHover : Theme.c.card
        border.width: Theme.look === "windows" || Theme.look === "gnome" ? 1 : 0
        border.color: Theme.c.border
        Row {
            anchors { left: parent.left; leftMargin: 14; verticalCenter: parent.verticalCenter }
            spacing: 10
            Icon { name: "settings"; size: 18; anchors.verticalCenter: parent.verticalCenter }
            Lbl { text: ls.tr("More options"); size: 15; anchors.verticalCenter: parent.verticalCenter }
        }
        Icon { anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter } name: "right"; size: 18; color: Theme.c.textDim }
        MouseArea { id: moreArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: ls.more = true }
    }

    // ================= second page =================
    PrefGroup {
        visible: ls.more
        title: ls.tr("Zoom and text")
        PrefSlider { title: ls.tr("Zoom (%)"); from: 10; to: 400; step: 5; value: ls.g.reader_zoom || 100; onMoved: function(v) { ls.setting("reader_zoom", Math.round(v)) } }
        PrefSlider { title: ls.tr("Translated text size (%)"); first: false; from: 40; to: 250; step: 5; value: ls.g.reader_font || 100; onMoved: function(v) { ls.setting("reader_font", Math.round(v)) } }
        PrefSlider { title: ls.tr("Automatic scrolling (webtoon)"); first: false; from: 0; to: 100; step: 1; unit: ""; value: ls.g.reader_scroll || 0; onMoved: function(v) { ls.setting("reader_scroll", Math.round(v)) } }
    }
    PrefGroup {
        visible: ls.more
        title: ls.tr("Controls")
        PrefSwitch { title: ls.tr("Buttons on the screen"); checked: !!ls.g.reader_buttons; onToggled: function(on) { ls.setting("reader_buttons", on) } }
        PrefSwitch { title: ls.tr("Tap zones"); subtitle: ls.tr("Click the sides to turn the page"); first: false; checked: !!ls.g.reader_taps; onToggled: function(on) { ls.setting("reader_taps", on) } }
        PrefSwitch { title: ls.tr("Tap the text boxes"); subtitle: ls.tr("Off: the boxes over the speech bubbles are only drawn, a swipe or a tap goes to the page"); first: false; checked: ls.g.reader_bubble_taps !== false; onToggled: function(on) { ls.setting("reader_bubble_taps", on) } }
    }
}
