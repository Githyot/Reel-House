import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "Osd.js" as Osd

// The Subtitles menu of the player, laid out like "Themes & Settings" of the readers: one column under a title bar
// (back on a page, the title, a close button) with the language (a row that opens the list of languages), the
// variants of the chosen language, the delay and the position, then rows that open a page of their own (Forced
// translation, Karaoke) or the preferences (Subtitle style), and Study while watching.
Popup {
    id: sm
    property var p: ({})
    property string page: "main"            // main | langs | translation | karaoke
    property string chosen: ""              // language key shown in Variants
    signal prefsRequested(string name)
    parent: Overlay.overlay
    padding: o.pad + 2
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    property real anchorX: 0
    property real anchorY: 0
    // a phone: a bottom sheet as wide as the screen; the three columns are swiped sideways inside it
    readonly property bool sheet: Theme.mobile === true
    readonly property real ovW: Overlay.overlay ? Overlay.overlay.width : 1000
    readonly property real ovH: Overlay.overlay ? Overlay.overlay.height : 800
    readonly property real safeBottom: Theme.safe !== undefined && Theme.safe.bottom !== undefined ? Theme.safe.bottom : 0
    width: sheet ? ovW : implicitWidth
    onPageChanged: flick.contentY = 0
    bottomPadding: padding + (sheet ? safeBottom + 6 : 0)
    topPadding: padding + (sheet && Theme.mob !== undefined && Theme.mob.grabber === true ? 14 : 0)
    x: sheet ? 0 : Math.max(8, Math.min(anchorX - width / 2, ovW - width - 8))
    y: sheet ? ovH - height : Math.max(8, anchorY - height - 10)
    function openAbove(btn) {
        const pt = btn.mapToItem(Overlay.overlay, btn.width / 2, 0)
        anchorX = pt.x
        anchorY = Osd.above(btn, pt.y)
        page = "main"
        chosen = activeLang !== "" ? activeLang : (langs.length > 0 ? langs[0].key : "")
        Core.refreshPrefs()   // the "Study while watching" switch reads the preferences
        open()
    }
    function arr(v) { return v ? v : [] }
    readonly property var g: Core.prefs && Core.prefs.general ? Core.prefs.general : ({})
    function setPref(path, value) { Core.cmd({cmd: "set_setting", path: path, value: value}); Core.refreshPrefs() }

    readonly property var tracks: arr(p.subs)
    readonly property var cands: arr(p.subCands)
    readonly property string activeLang: { var t = tracks.filter(function(x) { return x.selected }); return t.length > 0 ? t[0].langKey : "" }
    // Languages: what is already loaded, then what the addons offer.
    readonly property var langs: {
        var out = [], seen = {}
        tracks.concat(cands).forEach(function(x) { if (!seen[x.langKey]) { seen[x.langKey] = true; out.push({key: x.langKey, name: x.langName}) } })
        return out
    }
    // The variants of the chosen language, computed once per state change (the Repeaters below use count models,
    // so the 4×/s player refresh does not rebuild their rows).
    readonly property var chosenTracks: tracks.filter(function(t) { return t.langKey === sm.chosen })
    readonly property var chosenCands: cands.filter(function(c) { return c.langKey === sm.chosen })

    readonly property var o: Osd.t(Theme.layout.player)
    background: OsdPanel { bottomSheet: sm.sheet }
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.motion.fast || 120 }
        NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: Theme.motion.normal || 200; easing.type: Easing.OutCubic }
    }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Theme.motion.fast || 120 } }

    // ---- small pieces in the look of the theme's OSD popovers (Osd.js) ----
    // the heading of a column
    component Head: OsdHeader { title: true }
    // the heading of a group inside a column
    component Cap: OsdHeader { }
    // A group of rows on a card, like the boxed lists of "Themes & Settings".
    // (its own column is given to `children` by name: anything declared plainly goes to `rows`)
    component Group: Rectangle {
        id: gr
        default property alias rows: grCol.data
        width: parent ? parent.width : 300
        height: grCol.implicitHeight + 8
        radius: Theme.layout.player === "m3" ? 16 : (Theme.layout.player === "fluent" ? 6 : 12)
        color: sm.o.field
        border.width: sm.o.fieldStroke !== "#00000000" ? 1 : 0
        border.color: sm.o.fieldStroke
        children: [ Column { id: grCol; x: 4; y: 4; width: gr.width - 8; spacing: 1 } ]
    }
    // A row that opens more: an icon, its name, what is chosen now and an arrow ("Customize   Calm  >").
    component LinkRow: Rectangle {
        id: lr
        property string text: ""
        property string value: ""
        property string glyph: ""
        signal activated()
        width: parent ? parent.width : 300
        height: sm.o.rowH + 8
        radius: Osd.r(sm.o.rowR, height)
        color: lrA.pressed ? sm.o.press : (lrA.containsMouse ? sm.o.hover : "transparent")
        Row {
            anchors { left: parent.left; leftMargin: sm.o.rowPad; verticalCenter: parent.verticalCenter }
            spacing: 10
            Icon { visible: lr.glyph !== ""; name: lr.glyph; size: 18; color: sm.o.fg; anchors.verticalCenter: parent.verticalCenter }
            Text { width: Math.min(implicitWidth, lr.width * 0.55); elide: Text.ElideRight; text: lr.text; color: sm.o.fg; font.pixelSize: sm.o.rowSize + 1; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
        }
        Row {
            anchors { right: parent.right; rightMargin: sm.o.rowPad - 2; verticalCenter: parent.verticalCenter }
            spacing: 4
            Text { width: Math.min(implicitWidth, lr.width * 0.38); elide: Text.ElideRight; visible: lr.value !== ""; text: lr.value; color: sm.o.dim; font.pixelSize: sm.o.rowSize; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
            Icon { name: "right"; size: 18; color: sm.o.dim; anchors.verticalCenter: parent.verticalCenter }
        }
        MouseArea { id: lrA; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: lr.activated() }
    }
    // a caption over a control, like "Layout" / "Page turn" in "Themes & Settings"
    component Caption: Text {
        x: 4
        color: sm.o.headColor
        font.pixelSize: sm.o.headSize
        font.weight: sm.o.headWeight
        font.family: Theme.type.family
    }
    // a row with a label and a switch
    component SwitchRow: Item {
        id: swr
        property string text: ""
        property bool checked: false
        signal toggled(bool on)
        width: parent ? parent.width : 200
        height: sm.o.rowH + 4
        Text { x: sm.o.rowPad; width: parent.width - x - swc.width - sm.o.rowPad - 8; anchors.verticalCenter: parent.verticalCenter; text: swr.text; elide: Text.ElideRight; color: sm.o.fg; font.pixelSize: sm.o.rowSize; font.family: Theme.type.family }
        OsdSwitch { id: swc; anchors { right: parent.right; rightMargin: sm.o.rowPad; verticalCenter: parent.verticalCenter } checked: swr.checked; onToggled: function(on) { swr.toggled(on) } }
    }
    component HSep: Rectangle { width: parent ? parent.width : 100; height: 1; color: sm.o.sep }
    // "− value +  Reset".
    component Stepper: Row {
        id: stp
        property string value: ""
        property string tipMinus: ""
        property string tipPlus: ""
        signal step(int dir)
        leftPadding: sm.o.rowPad - 4
        spacing: 6
        height: 40
        OsdButton { kind: "round"; glyph: "minus"; anchors.verticalCenter: parent.verticalCenter; ToolTip.visible: hovered && stp.tipMinus !== ""; ToolTip.text: stp.tipMinus; onClicked: stp.step(-1) }
        Text { width: 76; horizontalAlignment: Text.AlignHCenter; anchors.verticalCenter: parent.verticalCenter; text: stp.value; color: sm.o.fg; font.pixelSize: 15; font.weight: 600; font.family: Theme.type.family }
        OsdButton { kind: "round"; glyph: "plus"; anchors.verticalCenter: parent.verticalCenter; ToolTip.visible: hovered && stp.tipPlus !== ""; ToolTip.text: stp.tipPlus; onClicked: stp.step(1) }
        OsdButton { kind: "tonal"; text: Core.tr(Core.uiLang, "Reset"); implicitHeight: 30; anchors.verticalCenter: parent.verticalCenter; onClicked: stp.step(0) }
    }
    component Slide: OsdSlider {
        width: parent ? parent.width - (sm.o.rowPad - 6) * 2 : 200
        x: sm.o.rowPad - 6
    }
    // Buttons side by side, one of them on.
    component Linked: OsdSegmented {
        x: sm.o.rowPad - 4
        width: parent ? parent.width - (sm.o.rowPad - 4) * 2 : 240
    }
    // dim help text
    component Note: Text {
        x: sm.o.rowPad
        width: parent ? parent.width - sm.o.rowPad * 2 : 200
        wrapMode: Text.Wrap
        color: sm.o.muted
        font.pixelSize: 13
        font.family: Theme.type.family
    }

    contentItem: Item {
        implicitWidth: sm.sheet ? sm.ovW - sm.leftPadding - sm.rightPadding : Math.min(400, sm.ovW - sm.leftPadding - sm.rightPadding - 16)
        implicitHeight: Math.min(bar.height + 8 + pages.implicitHeight, sm.ovH * 0.8 - sm.topPadding - sm.bottomPadding)
        clip: true

        // ---- the title bar, like "Themes & Settings": back (on a page), the title, close ----
        Item {
            id: bar
            width: parent.width
            height: 40
            OsdButton {
                id: backBtn
                visible: sm.page !== "main"
                anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                kind: "round"; glyph: "left"
                onClicked: sm.page = "main"
            }
            Text {
                // GNOME and Apple centre the title; Fluent and Material put it at the start
                readonly property bool centred: Theme.look === "gnome" || Theme.look === "apple"
                x: centred ? (parent.width - width) / 2 : (backBtn.visible ? backBtn.width + 10 : 6)
                width: Math.min(implicitWidth, parent.width - 2 * (closeBtn.width + 12))
                anchors.verticalCenter: parent.verticalCenter
                elide: Text.ElideRight
                text: Core.tr(Core.uiLang, sm.page === "langs" ? "Languages" : sm.page === "translation" ? "Forced translation" : sm.page === "karaoke" ? "Karaoke" : "Subtitles")
                color: sm.o.fg
                font.pixelSize: sm.o.titleSize + 2
                font.weight: sm.o.titleWeight
                font.family: Theme.type.family
            }
            OsdButton {
                id: closeBtn
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                kind: "round"; glyph: "close"
                onClicked: sm.close()
            }
        }

        Flickable {
            id: flick
            anchors { left: parent.left; right: parent.right; top: bar.bottom; topMargin: 8; bottom: parent.bottom }
            contentWidth: width
            contentHeight: pages.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            interactive: contentHeight > height + 1
            ScrollBar.vertical: ScrollBar { policy: flick.interactive ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
        Item {
            id: pages
            width: flick.width
            implicitHeight: sm.page === "langs" ? langPage.implicitHeight : sm.page === "translation" ? transPage.implicitHeight : sm.page === "karaoke" ? karaPage.implicitHeight : mainPage.implicitHeight
            height: implicitHeight

        // ================= the main page =================
        Column {
            id: mainPage
            visible: sm.page === "main"
            width: parent.width
            spacing: 14
            // the language (its list is a page of its own) and its variants
            Group {
                LinkRow {
                    glyph: "subtitles"
                    text: Core.tr(Core.uiLang, "Language")
                    value: { const l = sm.langs.filter(function(x) { return x.key === sm.chosen }); return sm.activeLang === "" && sm.chosen === "" ? Core.tr(Core.uiLang, "Off") : (l.length ? l[0].name : Core.tr(Core.uiLang, "Off")) }
                    onActivated: sm.page = "langs"
                }
            }
            Column {
                width: parent.width
                spacing: 6
                visible: sm.chosen !== ""
                Caption { text: Core.tr(Core.uiLang, "Variants") }
                Group {
                    Repeater {
                        model: sm.chosenTracks.length
                        delegate: OsdRow {
                            required property int index
                            readonly property var modelData: sm.chosenTracks[index] || ({})
                            text: modelData.title !== "" ? modelData.title : modelData.langName + " " + modelData.id
                            checked: modelData.selected
                            onActivated: { Core.cmd({cmd: "set_sub", id: modelData.id}); sm.close() }
                        }
                    }
                    Repeater {
                        model: sm.chosenCands.length
                        delegate: OsdRow {
                            required property int index
                            readonly property var modelData: sm.chosenCands[index] || ({})
                            text: modelData.addon + " #" + (index + 1)
                            onActivated: { Core.cmd({cmd: "load_sub_cand", index: modelData.index}); sm.close() }
                        }
                    }
                }
            }
            Column {
                width: parent.width
                spacing: 6
                Caption { text: Core.tr(Core.uiLang, "Delay") }
                Stepper {
                    tipMinus: Core.tr(Core.uiLang, "Subtitles earlier (0.1 s)")
                    tipPlus: Core.tr(Core.uiLang, "Subtitles later (0.1 s)")
                    value: ((sm.p.subDelay || 0) >= 0 ? "+" : "") + (sm.p.subDelay || 0).toFixed(1) + " s"
                    onStep: function(dir) { Core.cmd({cmd: "sub_delay", value: dir === 0 ? 0 : (sm.p.subDelay || 0) + dir * 0.1}) }
                }
                // finds the delay by itself from the speech of the video (crate::subsync)
                OsdButton {
                    kind: "tonal"
                    glyph: "refresh"
                    enabled: sm.p.subSyncBusy !== true
                    text: sm.p.subSyncBusy === true ? Core.tr(Core.uiLang, "Syncing…") : Core.tr(Core.uiLang, "Sync with the speech")
                    implicitHeight: 32
                    ToolTip.visible: hovered
                    ToolTip.text: Core.tr(Core.uiLang, "Listens to the next two minutes of the video and moves the subtitle to match the voices")
                    onClicked: Core.cmd({cmd: "sub_sync"})
                }
                Text {
                    visible: text !== ""
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: sm.p.subSyncNote ? Core.tr(Core.uiLang, sm.p.subSyncNote) : ""
                    color: "#b3ffffff"
                    font.pixelSize: 12
                    font.family: Theme.type.family
                }
            }
            Column {
                width: parent.width
                spacing: 6
                Caption { text: Core.tr(Core.uiLang, "Vertical position") }
                Slide {
                    id: vpos
                    from: 0; to: 100; stepSize: 1
                    // follows the player, except while dragged (the state arriving 4×/s would snap it back)
                    Binding on value { when: !vpos.pressed; restoreMode: Binding.RestoreNone; value: sm.p.subPos !== undefined ? sm.p.subPos : 100 }
                    onMoved: Core.cmd({cmd: "set_sub_layout", pos: Math.round(value), align: sm.p.subAlign || "center", margin: Math.round(hmargin.value)})
                }
            }
            Column {
                width: parent.width
                spacing: 6
                Caption { text: Core.tr(Core.uiLang, "Horizontal position") }
                Linked {
                    options: [["left", "Left"], ["center", "Center"], ["right", "Right"]]
                    current: sm.p.subAlign || "center"
                    onChosen: function(id) { Core.cmd({cmd: "set_sub_layout", pos: Math.round(vpos.value), align: id, margin: Math.round(hmargin.value)}) }
                }
            }
            Column {
                width: parent.width
                spacing: 6
                Caption { text: Core.tr(Core.uiLang, "Distance from the side") }
                Slide {
                    id: hmargin
                    from: 0; to: 400; stepSize: 5
                    Binding on value { when: !hmargin.pressed; restoreMode: Binding.RestoreNone; value: sm.p.subMargin || 0 }
                    onMoved: Core.cmd({cmd: "set_sub_layout", pos: Math.round(vpos.value), align: sm.p.subAlign || "center", margin: Math.round(value)})
                }
            }
            // what opens more: the two pages, and the style in Preferences
            Group {
                LinkRow { glyph: "translate"; text: Core.tr(Core.uiLang, "Forced translation"); value: sm.p.translating !== null && sm.p.translating !== undefined ? (sm.p.translating || 0) + "%" : ""; onActivated: sm.page = "translation" }
                LinkRow { glyph: "audio"; text: Core.tr(Core.uiLang, "Karaoke"); value: (sm.p.karaoke || ({})).on === true ? Core.tr(Core.uiLang, "On") : ""; onActivated: sm.page = "karaoke" }
                LinkRow { glyph: "settings"; text: Core.tr(Core.uiLang, "Subtitle style…"); onActivated: { sm.close(); sm.prefsRequested("subtitles") } }
            }
            // study while watching (studyplay.rs): questions about lines of the subtitle in the study language
            Group {
                SwitchRow {
                    text: Core.tr(Core.uiLang, "Study while watching")
                    checked: sm.g.study_in_player === true
                    onToggled: function(on) { sm.setPref("general.study_in_player", on) }
                }
                Text {
                    x: sm.o.rowPad; width: parent.width - sm.o.rowPad * 2
                    visible: sm.g.study_in_player === true && (sm.g.study_lang || "") === ""
                    wrapMode: Text.Wrap
                    bottomPadding: 6
                    text: Core.tr(Core.uiLang, "Choose the language you are learning in Preferences.")
                    color: sm.o.muted; font.pixelSize: 12; font.family: Theme.type.family
                }
                Cap { visible: sm.g.study_in_player === true; text: Core.tr(Core.uiLang, "How often") + " · " + Math.round(qrate.value) + "%" }
                Slide {
                    id: qrate
                    visible: sm.g.study_in_player === true
                    from: 1; to: 100; stepSize: 1
                    Binding on value { when: !qrate.pressed; restoreMode: Binding.RestoreNone; value: sm.g.study_quiz_rate !== undefined ? sm.g.study_quiz_rate : 50 }
                    onPressedChanged: if (!pressed) sm.setPref("general.study_quiz_rate", Math.round(value))
                    ToolTip.visible: hovered
                    ToolTip.text: Core.tr(Core.uiLang, "100%: a question about every 2 minutes")
                }
                // the kinds, each the user's choice (all of them, with their rates, in Preferences > Study)
                Repeater {
                    model: sm.g.study_in_player === true ? 5 : 0
                    delegate: SwitchRow {
                        required property int index
                        readonly property var row: [
                            ["study_quiz_cards", "Flash cards during videos"],
                            ["study_quiz_listen", "Parts without subtitles"],
                            ["study_wordmix", "Mix learned words into subtitles"],
                            ["study_mining", "Make cards from single words"],
                            ["study_mascot_cues", "The macaw helps while you watch"]
                        ][index]
                        text: Core.tr(Core.uiLang, row[1])
                        checked: sm.g[row[0]] !== false
                        onToggled: function(on) { sm.setPref("general." + row[0], on) }
                    }
                }
            }
        }

        // ================= Languages: what is loaded, then what the addons offer =================
        Column {
            id: langPage
            visible: sm.page === "langs"
            width: parent.width
            spacing: 6
            Group {
                OsdRow { text: Core.tr(Core.uiLang, "Off"); checked: sm.activeLang === ""; onActivated: { Core.cmd({cmd: "set_sub", id: 0}); sm.close() } }
                Repeater {
                    model: sm.langs.length
                    delegate: OsdRow {
                        required property int index
                        readonly property var modelData: sm.langs[index] || ({})
                        text: modelData.name
                        checked: sm.activeLang === modelData.key
                        selected: sm.chosen === modelData.key
                        chevron: true
                        // back to the main page, where the variants of this language are
                        onActivated: { sm.chosen = modelData.key; sm.page = "main" }
                    }
                }
            }
            Note { visible: sm.langs.length === 0; topPadding: 6; text: Core.tr(Core.uiLang, "None found yet") }
        }

        // ================= Forced translation =================
        Column {
            id: transPage
            visible: sm.page === "translation"
            width: parent.width
            spacing: 4
            property int src: 0
            property int lang: 0
            readonly property var langIds: ["en", "pt", "es", "fr", "de", "it", "nl", "sv", "pl", "ru", "tr", "ar", "hi", "id", "vi", "th", "ja", "ko", "zh"]
            readonly property var langNames: ["English", "Português", "Español", "Français", "Deutsch", "Italiano", "Nederlands", "Svenska", "Polski", "Русский", "Türkçe", "العربية", "हिन्दी", "Bahasa Indonesia", "Tiếng Việt", "ไทย", "日本語", "한국어", "中文"]
            // Every subtitle that can be translated: the tracks built into the video (the one on screen first), what
            // the addons offer, then files already loaded.
            readonly property var sources: {
                var out = [], counts = {}
                sm.tracks.filter(function(t) { return !t.external }).sort(function(a, b) { return (b.selected ? 1 : 0) - (a.selected ? 1 : 0) }).forEach(function(t) {
                    out.push({label: (t.title !== "" ? t.title : t.langName) + " · " + Core.tr(Core.uiLang, "built into the video"), index: 0, file: "", lang: t.lang, track: t.id})
                })
                sm.cands.forEach(function(c) {
                    counts[c.langKey] = (counts[c.langKey] || 0) + 1
                    out.push({label: c.langName + " · " + c.addon + " #" + counts[c.langKey], index: c.index, file: "", lang: "", track: 0})
                })
                sm.tracks.filter(function(t) { return t.external && t.file !== "" }).forEach(function(t) { out.push({label: t.title !== "" ? t.title : t.langName, index: 0, file: t.file, lang: t.lang, track: 0}) })
                return out
            }
            readonly property bool running: sm.p.translating !== null && sm.p.translating !== undefined
            onVisibleChanged: if (visible) { src = 0; var i = langIds.indexOf(sm.p.targetLang || "en"); lang = i < 0 ? 0 : i }
            Note { visible: transPage.sources.length === 0; text: Core.tr(Core.uiLang, "No subtitles to translate yet") }
            Column {
                visible: transPage.sources.length > 0
                width: parent.width
                spacing: 2
                enabled: !transPage.running
                opacity: enabled ? 1 : 0.5
                Cap { text: Core.tr(Core.uiLang, "Subtitle to translate"); topPadding: 0 }
                Flickable {
                    width: parent.width
                    height: Math.min(srcCol.implicitHeight, 200)
                    contentHeight: srcCol.implicitHeight
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    Column {
                        id: srcCol
                        width: parent.width
                        Repeater {
                            model: transPage.sources.length
                            delegate: OsdRow {
                                required property int index
                                readonly property var modelData: transPage.sources[index] || ({})
                                width: srcCol.width
                                text: modelData.label || ""
                                checked: transPage.src === index
                                onActivated: transPage.src = index
                            }
                        }
                    }
                }
                Cap { text: Core.tr(Core.uiLang, "Translate to") }
                // a drop-down field in the theme's shape
                Rectangle {
                    x: sm.o.rowPad - 4
                    width: parent.width - x * 2
                    height: Theme.layout.player === "m3" ? 44 : 36
                    radius: Theme.layout.player === "m3" ? 8 : Osd.r(sm.o.ctlR, height)
                    color: langA.containsMouse ? sm.o.btnHover : sm.o.field
                    border.width: sm.o.fieldStroke !== "#00000000" ? 1 : 0
                    border.color: sm.o.fieldStroke
                    Text { x: 12; anchors.verticalCenter: parent.verticalCenter; text: transPage.langNames[transPage.lang]; color: sm.o.fg; font.pixelSize: 14; font.family: Theme.type.family }
                    Icon { anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } name: "down"; size: 16; color: sm.o.dim }
                    MouseArea { id: langA; anchors.fill: parent; hoverEnabled: true; onClicked: langMenu.popup(0, parent.height) }
                    AppMenu {
                        id: langMenu
                        Repeater {
                            model: transPage.langNames
                            delegate: AppMenuItem { required property var modelData; required property int index; text: modelData; onTriggered: transPage.lang = index }
                        }
                    }
                }
                Item { width: 1; height: 4 }
                OsdButton {
                    x: sm.o.rowPad - 4
                    width: parent.width - x * 2
                    kind: "accent"
                    glyph: "translate"
                    text: Core.tr(Core.uiLang, "Translate")
                    onClicked: {
                        var s = transPage.sources[transPage.src]
                        if (s) Core.cmd({cmd: "translate_subtitle", index: s.index, file: s.file, lang: s.lang, track: s.track || 0, target: transPage.langIds[transPage.lang]})
                    }
                }
            }
            // Progress while translating, and how the last one ended.
            Column {
                visible: transPage.running
                x: 6
                width: parent.width - 12
                spacing: 8
                topPadding: 10
                Item {
                    width: parent.width
                    height: 22
                    Text { text: Core.tr(Core.uiLang, "Translating…"); color: sm.o.fg; font.pixelSize: 15; font.weight: sm.o.titleWeight; font.family: Theme.type.family }
                    Text { anchors.right: parent.right; text: (sm.p.translating || 0) + "%"; color: sm.o.muted; font.pixelSize: 14; font.family: Theme.type.family }
                }
                Rectangle {
                    width: parent.width; height: Theme.layout.player === "m3" ? 4 : 6; radius: height / 2; color: Theme.layout.player === "m3" ? "#4a4458" : "#33ffffff"
                    Rectangle { width: parent.width * (sm.p.translating || 0) / 100; height: parent.height; radius: height / 2; color: Theme.c.accent }
                }
                OsdButton { kind: "tonal"; text: Core.tr(Core.uiLang, "Cancel"); onClicked: Core.cmd({cmd: "cancel_translation"}) }
            }
            Text {
                visible: !transPage.running && !!sm.p.translateNote
                x: 8
                width: parent.width - 16
                topPadding: 8
                wrapMode: Text.Wrap
                text: sm.p.translateNote ? (sm.p.translateNote.ok ? "✓ " : "") + sm.p.translateNote.text : ""
                color: sm.p.translateNote && sm.p.translateNote.ok ? "#57e389" : "#ff7b63"
                font.pixelSize: 14
                font.family: Theme.type.family
            }
        }

        // ================= Karaoke =================
        Column {
            id: karaPage
            visible: sm.page === "karaoke"
            width: parent.width
            spacing: 4
            readonly property var k: sm.p.karaoke || ({})
            readonly property string colorMode: (k.color || "") === "" ? "file" : (k.color === "accent" ? "accent" : "own")
            Column {
                id: karaCols
                width: parent.width
                spacing: 10
                // Variants: the best match, or one of the songs found.
                Column {
                    width: parent.width
                    Head { text: Core.tr(Core.uiLang, "Variants") }
                    OsdRow { text: Core.tr(Core.uiLang, "Automatic (best match)"); checked: karaPage.k.forced === null || karaPage.k.forced === undefined; onActivated: Core.cmd({cmd: "karaoke_choose", index: -1}) }
                    Repeater {
                        model: (karaPage.k.variants || []).length
                        delegate: OsdRow { required property int index; readonly property var modelData: (karaPage.k.variants || [])[index] || ""; text: modelData; checked: karaPage.k.forced === index; onActivated: Core.cmd({cmd: "karaoke_choose", index: index}) }
                    }
                    Note { visible: (karaPage.k.variants || []).length === 0; topPadding: 6; text: Core.tr(Core.uiLang, "No songs found yet") }
                }
                HSep { }
                // Look.
                Column {
                    width: parent.width
                    Head { text: Core.tr(Core.uiLang, "Look") }
                    SwitchRow { text: Core.tr(Core.uiLang, "At the top of the screen"); checked: karaPage.k.top === true; onToggled: function(on) { Core.cmd({cmd: "set_setting", path: "general.karaoke_top", value: on}) } }
                    Cap { text: Core.tr(Core.uiLang, "Size (%)") + " · " + Math.round(kSize.value) }
                    Slide { id: kSize; from: 40; to: 250; stepSize: 5; Binding on value { when: !kSize.pressed; restoreMode: Binding.RestoreNone; value: karaPage.k.size || 100 } onPressedChanged: if (!pressed) Core.cmd({cmd: "set_setting", path: "general.karaoke_size", value: Math.round(value)}) }
                    Cap { text: Core.tr(Core.uiLang, "Distance from the edge") + " · " + Math.round(kMargin.value) }
                    Slide { id: kMargin; from: 0; to: 400; stepSize: 5; Binding on value { when: !kMargin.pressed; restoreMode: Binding.RestoreNone; value: karaPage.k.margin || 0 } onPressedChanged: if (!pressed) Core.cmd({cmd: "set_setting", path: "general.karaoke_margin", value: Math.round(value)}) }
                    Cap { text: Core.tr(Core.uiLang, "Letters") }
                    Linked {
                        options: [["", "As the file"], ["on", "Bold"], ["off", "Regular"]]
                        current: karaPage.k.bold || ""
                        onChosen: function(id) { Core.cmd({cmd: "set_setting", path: "general.karaoke_bold", value: id}) }
                    }
                    OsdRow { text: (karaPage.k.font ? karaPage.k.font : Core.tr(Core.uiLang, "Font: as the file has it")); onActivated: { sm.close(); sm.prefsRequested("subtitles") } }
                    Cap { text: Core.tr(Core.uiLang, "Outline") + " · " + (kOut.value < 0 ? Core.tr(Core.uiLang, "file") : Math.round(kOut.value)) }
                    Slide { id: kOut; from: -1; to: 12; stepSize: 1; Binding on value { when: !kOut.pressed; restoreMode: Binding.RestoreNone; value: karaPage.k.outline !== undefined ? karaPage.k.outline : -1 } onPressedChanged: if (!pressed) Core.cmd({cmd: "set_setting", path: "general.karaoke_outline", value: Math.round(value)}) }
                    Cap { text: Core.tr(Core.uiLang, "Shadow") + " · " + (kShadow.value < 0 ? Core.tr(Core.uiLang, "file") : Math.round(kShadow.value)) }
                    Slide { id: kShadow; from: -1; to: 12; stepSize: 1; Binding on value { when: !kShadow.pressed; restoreMode: Binding.RestoreNone; value: karaPage.k.shadow !== undefined ? karaPage.k.shadow : -1 } onPressedChanged: if (!pressed) Core.cmd({cmd: "set_setting", path: "general.karaoke_shadow", value: Math.round(value)}) }
                    Item {
                        width: parent.width
                        height: 40
                        Text { x: sm.o.rowPad; anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Before it is sung"); color: sm.o.fg; font.pixelSize: sm.o.rowSize; font.family: Theme.type.family }
                        Row {
                            anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                            spacing: 4
                            Repeater {
                                model: ["", "#ffffff", "#f6d32d", "#62a0ea", "#33d17a", "#c061cb"]
                                delegate: Rectangle {
                                    required property var modelData
                                    width: 22; height: 22; radius: 11
                                    color: modelData === "" ? "transparent" : modelData
                                    border.width: (karaPage.k.unfilled || "").toLowerCase() === modelData ? 2 : 1
                                    border.color: (karaPage.k.unfilled || "").toLowerCase() === modelData ? Osd.readable(Theme.c.accent) : "#66ffffff"
                                    Text { visible: modelData === ""; anchors.centerIn: parent; text: "A"; color: "#ffffff"; font.pixelSize: 11; font.family: Theme.type.family }
                                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: Core.cmd({cmd: "set_setting", path: "general.karaoke_unfilled", value: modelData}) }
                                }
                            }
                        }
                    }
                    OsdRow { text: Core.tr(Core.uiLang, "Back to the file's own look"); onActivated: { ["karaoke_font", "karaoke_bold", "karaoke_unfilled", "karaoke_color"].forEach(function(k) { Core.cmd({cmd: "set_setting", path: "general." + k, value: ""}) }); Core.cmd({cmd: "set_setting", path: "general.karaoke_outline", value: -1}); Core.cmd({cmd: "set_setting", path: "general.karaoke_shadow", value: -1}) } }
                    Cap { text: Core.tr(Core.uiLang, "Colour") }
                    Linked {
                        options: [["file", "Original"], ["accent", "App accent"], ["own", "Own"]]
                        current: karaPage.colorMode
                        onChosen: function(id) { Core.cmd({cmd: "set_setting", path: "general.karaoke_color", value: id === "file" ? "" : (id === "accent" ? "accent" : ownSwatch.hex)}) }
                    }
                    Item {
                        width: parent.width
                        height: 40
                        Text { x: sm.o.rowPad; anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Own colour"); color: sm.o.fg; font.pixelSize: sm.o.rowSize; font.family: Theme.type.family }
                        Row {
                            id: ownSwatch
                            readonly property string hex: (karaPage.k.color || "").charAt(0) === "#" ? karaPage.k.color : "#ff9f1c"
                            anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                            spacing: 4
                            // A few colours to pick from (the full colour dialog is in Preferences).
                            Repeater {
                                model: ["#ff9f1c", "#f6d32d", "#33d17a", "#62a0ea", "#c061cb", "#ffffff"]
                                delegate: Rectangle {
                                    required property var modelData
                                    width: 22; height: 22; radius: 11
                                    color: modelData
                                    border.width: ownSwatch.hex.toLowerCase() === modelData && karaPage.colorMode === "own" ? 2 : 0
                                    border.color: Osd.readable(Theme.c.accent)
                                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: Core.cmd({cmd: "set_setting", path: "general.karaoke_color", value: modelData}) }
                                }
                            }
                        }
                    }
                }
                HSep { }
                // Fine tuning.
                Column {
                    width: parent.width
                    Head { text: Core.tr(Core.uiLang, "Fine tuning") }
                    SwitchRow { text: Core.tr(Core.uiLang, "Karaoke"); checked: karaPage.k.on === true; onToggled: function(on) { Core.cmd({cmd: "karaoke_on", on: on}) } }
                    Cap { text: Core.tr(Core.uiLang, "Delay of this song") }
                    Stepper {
                        visible: (karaPage.k.songKey || "") !== ""
                        value: ((karaPage.k.songDelay || 0) >= 0 ? "+" : "") + (karaPage.k.songDelay || 0).toFixed(1) + " s"
                        onStep: function(dir) { Core.cmd({cmd: "karaoke_set_song_delay", key: karaPage.k.songKey, value: dir === 0 ? 0 : Math.round(((karaPage.k.songDelay || 0) + dir * 0.1) * 10) / 10}) }
                    }
                    Note { visible: (karaPage.k.songKey || "") === ""; font.pixelSize: 12; text: Core.tr(Core.uiLang, "Choose a variant, or wait for an opening or ending to start.") }
                    Cap { text: Core.tr(Core.uiLang, "Delay of all the karaokes") }
                    Stepper {
                        tipMinus: Core.tr(Core.uiLang, "Earlier (0.1 s)")
                        tipPlus: Core.tr(Core.uiLang, "Later (0.1 s)")
                        value: ((karaPage.k.delay || 0) >= 0 ? "+" : "") + (karaPage.k.delay || 0).toFixed(1) + " s"
                        onStep: function(dir) { Core.cmd({cmd: "set_setting", path: "general.karaoke_delay", value: dir === 0 ? 0 : Math.round(((karaPage.k.delay || 0) + dir * 0.1) * 10) / 10}) }
                    }
                    Cap { text: Core.tr(Core.uiLang, "Status") }
                    Note { text: karaPage.k.status || ""; color: sm.o.fg; font.pixelSize: 14 }
                    Note { visible: (karaPage.k.detail || "") !== ""; text: karaPage.k.detail || ""; font.pixelSize: 12 }
                    Item { width: 1; height: 4 }
                    OsdRow { text: Core.tr(Core.uiLang, "Search karaoke again"); onActivated: Core.cmd({cmd: "karaoke_retry"}) }
                    OsdRow { text: Core.tr(Core.uiLang, "Karaoke studio…"); onActivated: { sm.close(); studio.open() } }
                    OsdRow { text: Core.tr(Core.uiLang, "More in Preferences…"); onActivated: { sm.close(); sm.prefsRequested("subtitles") } }
                }
            }
        }
        }
        }
    }

    KaraokeStudio { id: studio }
}
