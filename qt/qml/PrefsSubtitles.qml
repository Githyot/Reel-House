import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// Preferences > Subtitles & karaoke (GTK prefs::subtitles_page + prefs3::karaoke_page): automatic subtitles, the
// translator, the look of the subtitles, then the karaoke of anime openings and endings.
Column {
    id: sp
    property var dlg
    readonly property var sb: dlg ? dlg.sec("subtitles") : ({})
    readonly property var st: sb.style !== undefined ? sb.style : ({})
    readonly property var g: dlg ? dlg.g : ({})
    // The provider shown, so its group appears at once.
    property string provider: sb.provider || "libretranslate"
    onSbChanged: provider = sb.provider || "libretranslate"
    spacing: 26
    // the local AI (Ollama) is desktop only: Android and iOS do not offer it (a narrow desktop window keeps it)
    readonly property bool localAi: Qt.platform.os !== "android" && Qt.platform.os !== "ios"

    readonly property var langIds: ["", "en", "pt", "es", "fr", "de", "it", "nl", "sv", "pl", "ru", "tr", "ar", "hi", "id", "vi", "th", "ja", "ko", "zh"]
    readonly property var langNames: [Core.tr(Core.uiLang, "System language"), "English", "Português", "Español", "Français", "Deutsch", "Italiano", "Nederlands", "Svenska", "Polski", "Русский", "Türkçe", "العربية", "हिन्दी", "Bahasa Indonesia", "Tiếng Việt", "ไทย", "日本語", "한국어", "中文"]
    function style(k, v) { dlg.set("subtitles.style." + k, v) }
    function hexOf(c) {
        function h(x) { var s = Math.round(x * 255).toString(16); return s.length < 2 ? "0" + s : s }
        return "#" + h(c.r) + h(c.g) + h(c.b)
    }

    // A row with a colour button (gtk::ColorDialogButton).
    component ColorRow: PrefRow {
        id: cr
        property string value: "#ffffff"
        signal chosen(string hex)
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 46
            height: 32
            radius: Theme.shape.control
            color: crArea.containsMouse ? Theme.c.buttonHover : Theme.c.button
            Rectangle { anchors.centerIn: parent; width: 26; height: 18; radius: Theme.shape.small / 2; color: cr.value; border.width: 1; border.color: "#59000000" }
            MouseArea { id: crArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { colorDlg.target = cr; colorDlg.selectedColor = cr.value; colorDlg.open() } }
        }
    }

    // ---------- Automatic subtitles ----------
    PrefGroup {
        title: Core.tr(Core.uiLang, "Automatic subtitles")
        description: Core.tr(Core.uiLang, "Subtitles are found and translated while the video buffers, so playback starts with them working.")
        PrefSwitch { title: Core.tr(Core.uiLang, "Find and translate subtitles"); checked: sp.sb.enabled === true; onToggled: function(on) { sp.dlg.set("subtitles.enabled", on) } }
        PrefCombo {
            title: Core.tr(Core.uiLang, "Subtitle language"); subtitle: Core.tr(Core.uiLang, "The language your subtitles are translated into")
            options: sp.langNames
            current: sp.dlg.idx(sp.langIds, sp.sb.target_lang || "")
            onPicked: function(i) { sp.dlg.set("subtitles.target_lang", sp.langIds[i]) }
        }
        PrefSpin { title: Core.tr(Core.uiLang, "Longest wait after buffering"); subtitle: Core.tr(Core.uiLang, "Seconds. Playback starts without subtitles after this."); from: 0; to: 300; step: 5; value: sp.sb.max_wait_secs || 0; onMoved: function(v) { sp.dlg.set("subtitles.max_wait_secs", v) } }
        PrefSwitch { title: Core.tr(Core.uiLang, "Remove notes for the hearing impaired"); subtitle: Core.tr(Core.uiLang, "Drops [music], (laughs) and NAME: labels from downloaded subtitles, so study cards stay clean"); checked: sp.sb.strip_sdh === true; onToggled: function(on) { sp.dlg.set("subtitles.strip_sdh", on) } }
        PrefSwitch { title: Core.tr(Core.uiLang, "Sync downloaded subtitles with the speech"); subtitle: Core.tr(Core.uiLang, "About 10 s into each video the app listens to the voices and fixes the subtitle delay. Also in the player\u2019s subtitle menu."); checked: sp.sb.auto_sync === true; onToggled: function(on) { sp.dlg.set("subtitles.auto_sync", on) } }
    }

    // ---------- Translator ----------
    PrefGroup {
        title: Core.tr(Core.uiLang, "Translator")
        PrefCombo {
            readonly property var ids: ["libretranslate", "claude", "gemini", "openai"].concat(sp.localAi ? ["ollama"] : [])
            title: Core.tr(Core.uiLang, "Translation service"); subtitle: Core.tr(Core.uiLang, "Subtitles already in your language are used as they are")
            options: [Core.tr(Core.uiLang, "LibreTranslate (free, automatic)"), "Claude (Anthropic)", "Gemini (Google)", "GPT (OpenAI)"].concat(sp.localAi ? [Core.tr(Core.uiLang, "Local AI (Ollama · qwen2.5:3b)")] : [])
            current: sp.dlg.idx(ids, sp.provider)
            onPicked: function(i) { sp.provider = ids[i]; sp.dlg.set("subtitles.provider", ids[i]) }
        }
        PrefButton { title: Core.tr(Core.uiLang, "Test the translator"); subtitle: sp.dlg.info("translator"); label: Core.tr(Core.uiLang, "Test"); kind: "suggested"; onActivated: Core.cmd({cmd: "test_translator"}) }
    }
    // the local AI: its state and, the first time, its download (the same one as Conversation's)
    Lbl {
        visible: sp.localAi && sp.provider === "ollama"
        width: parent.width
        wrapMode: Text.Wrap
        dim: true
        text: Core.tr(Core.uiLang, "The local AI translates on this computer, free and offline. Without a good graphics card it is slow: a chapter or a whole subtitle can take minutes. It is downloaded the first time it translates, or with the button below.")
    }
    // loaded by URL: OllamaPanel.qml is not packed into the Android and iOS builds
    Loader { active: sp.localAi && sp.provider === "ollama"; visible: active; width: parent.width; source: active ? "OllamaPanel.qml" : "" }
    PrefButton {
        visible: !sp.localAi || sp.provider !== "ollama"
        title: Core.tr(Core.uiLang, "Server and key of the translator")
        subtitle: sp.provider === "libretranslate" ? Core.tr(Core.uiLang, "Optional for LibreTranslate") : Core.tr(Core.uiLang, "Needed for this service")
        label: Core.tr(Core.uiLang, "Open")
        onActivated: sp.dlg.openAccount(sp.provider)
    }

    // ---------- Appearance ----------
    PrefGroup {
        title: Core.tr(Core.uiLang, "Appearance")
        description: Core.tr(Core.uiLang, "Changes apply to the video that is playing now.")
        // The preview (GTK .preview-box): the text as it will look over a dark picture.
        Item {
            width: parent.width
            height: 110
            Rectangle {
                anchors.fill: parent
                anchors.margins: 1
                radius: 11
                gradient: Gradient {
                    GradientStop { position: 0; color: "#3a4a5c" }
                    GradientStop { position: 1; color: "#141a22" }
                }
            }
            Rectangle {
                visible: sp.st.bg_enabled === true
                anchors.centerIn: prev
                width: prev.contentWidth + 16
                height: prev.contentHeight + 6
                color: sp.st.bg_color || "#000000"
                opacity: (sp.st.bg_opacity !== undefined ? sp.st.bg_opacity : 60) / 100
            }
            Text {
                id: shadow
                visible: (sp.st.shadow_offset || 0) > 0
                x: prev.x + (sp.st.shadow_offset || 0) * 1.5
                y: prev.y + (sp.st.shadow_offset || 0) * 1.5
                width: prev.width
                horizontalAlignment: Text.AlignHCenter
                text: prev.text
                font: prev.font
                color: sp.st.shadow_color || "#000000"
                wrapMode: Text.Wrap
            }
            Text {
                id: prev
                anchors { left: parent.left; right: parent.right; margins: 16 }
                y: Math.max(4, (parent.height - implicitHeight) * Math.min(1, (sp.st.position !== undefined ? sp.st.position : 90) / 100))
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: Core.tr(Core.uiLang, "The quick brown fox jumps over the lazy dog")
                font.family: sp.st.font || "Sans"
                font.pixelSize: Math.max(10, (sp.st.size || 55) * 0.45)
                font.bold: sp.st.bold === true
                font.italic: sp.st.italic === true
                font.underline: sp.st.underline === true
                color: Qt.alpha(sp.st.text_color || "#ffffff", (sp.st.text_opacity !== undefined ? sp.st.text_opacity : 100) / 100)
                style: (sp.st.border_size || 0) > 0 ? Text.Outline : Text.Normal
                styleColor: sp.st.border_color || "#000000"
            }
        }
        PrefRow {
            title: Core.tr(Core.uiLang, "Font")
            FlatButton { anchors.verticalCenter: parent.verticalCenter; text: sp.st.font || "Sans"; implicitHeight: 34; onClicked: { fontDlg.selectedFont = Qt.font({family: sp.st.font || "Sans"}); fontDlg.open() } }
        }
        PrefSpin { title: Core.tr(Core.uiLang, "Size"); from: 20; to: 120; value: sp.st.size || 55; onMoved: function(v) { sp.style("size", v) } }
        PrefSwitch { title: Core.tr(Core.uiLang, "Bold"); checked: sp.st.bold === true; onToggled: function(on) { sp.style("bold", on) } }
        PrefSwitch { title: Core.tr(Core.uiLang, "Italic"); checked: sp.st.italic === true; onToggled: function(on) { sp.style("italic", on) } }
        PrefSwitch { title: Core.tr(Core.uiLang, "Underline"); checked: sp.st.underline === true; onToggled: function(on) { sp.style("underline", on) } }
        ColorRow { title: Core.tr(Core.uiLang, "Text color"); value: sp.st.text_color || "#ffffff"; onChosen: function(h) { sp.style("text_color", h) } }
        PrefSpin { title: Core.tr(Core.uiLang, "Text opacity"); subtitle: Core.tr(Core.uiLang, "Percent"); from: 0; to: 100; step: 5; value: sp.st.text_opacity !== undefined ? sp.st.text_opacity : 100; onMoved: function(v) { sp.style("text_opacity", v) } }
    }
    PrefsAdvanced { dlg: sp.dlg }
    Column {
        visible: sp.dlg ? sp.dlg.adv : false
        width: parent.width
        spacing: 26
        PrefGroup {
            title: Core.tr(Core.uiLang, "Outline, shadow and background")
            collapsible: true
            ColorRow { title: Core.tr(Core.uiLang, "Outline color"); value: sp.st.border_color || "#000000"; onChosen: function(h) { sp.style("border_color", h) } }
            PrefSpin { title: Core.tr(Core.uiLang, "Outline width"); from: 0; to: 8; step: 0.5; value: sp.st.border_size || 0; onMoved: function(v) { sp.style("border_size", v) } }
            ColorRow { title: Core.tr(Core.uiLang, "Shadow color"); value: sp.st.shadow_color || "#000000"; onChosen: function(h) { sp.style("shadow_color", h) } }
            PrefSpin { title: Core.tr(Core.uiLang, "Shadow distance"); from: 0; to: 6; step: 0.5; value: sp.st.shadow_offset || 0; onMoved: function(v) { sp.style("shadow_offset", v) } }
            PrefSwitch { title: Core.tr(Core.uiLang, "Background box"); subtitle: Core.tr(Core.uiLang, "A solid box behind the text, like VLC"); checked: sp.st.bg_enabled === true; onToggled: function(on) { sp.style("bg_enabled", on) } }
            ColorRow { title: Core.tr(Core.uiLang, "Background color"); value: sp.st.bg_color || "#000000"; onChosen: function(h) { sp.style("bg_color", h) } }
            PrefSpin { title: Core.tr(Core.uiLang, "Background opacity"); subtitle: Core.tr(Core.uiLang, "Percent"); from: 0; to: 100; step: 5; value: sp.st.bg_opacity || 0; onMoved: function(v) { sp.style("bg_opacity", v) } }
            PrefSpin { title: Core.tr(Core.uiLang, "Vertical position"); subtitle: Core.tr(Core.uiLang, "0 is the top, 100 is the bottom"); from: 0; to: 100; value: sp.st.position !== undefined ? sp.st.position : 90; onMoved: function(v) { sp.style("position", v) } }
            PrefSwitch { title: Core.tr(Core.uiLang, "Override the subtitle’s own style"); subtitle: Core.tr(Core.uiLang, "Also restyle subtitles that bring their own fonts and colors"); checked: sp.st.force_style === true; onToggled: function(on) { sp.style("force_style", on) } }
        }

        // ---------- Karaoke ----------
        SectionHead { title: Core.tr(Core.uiLang, "Karaoke"); icon: "audio" }
        PrefGroup {
            title: Core.tr(Core.uiLang, "Karaoke")
            description: Core.tr(Core.uiLang, "Anime only: while an opening or ending plays, the lyrics of the song (Karaoke Mugen) appear as a second subtitle. Your own subtitle is not touched, and the subtitle style does not change the karaoke. Needs the opening/ending times (AniSkip or the chapters of the file).")
            PrefButton { first: true; title: Core.tr(Core.uiLang, "Karaoke studio"); subtitle: Core.tr(Core.uiLang, "Every setting of the lyrics in one window, with a live preview: font, colours, border, place, margins and timing"); label: Core.tr(Core.uiLang, "Open…"); kind: "suggested"; onActivated: studio.open() }
            PrefSwitch { title: Core.tr(Core.uiLang, "Karaoke on openings and endings"); subtitle: Core.tr(Core.uiLang, "The player's Subtitles menu says what was found for each episode"); checked: sp.g.karaoke === true; onToggled: function(on) { sp.dlg.set("general.karaoke", on) } }
        }
        PrefGroup {
            id: kLook
            readonly property string col: sp.g.karaoke_color || ""
            title: Core.tr(Core.uiLang, "Timing and look")
            collapsible: true
            description: Core.tr(Core.uiLang, "Changes show at once while the lyrics are on screen.")
            PrefSpin { title: Core.tr(Core.uiLang, "Delay"); subtitle: Core.tr(Core.uiLang, "Seconds added to the start of the lyrics. Negative shows them earlier, positive later. Only use it for a karaoke that is out of step"); from: -15; to: 15; step: 0.1; value: sp.g.karaoke_delay || 0; onMoved: function(v) { sp.dlg.set("general.karaoke_delay", Math.round(v * 10) / 10) } }
            PrefSwitch { title: Core.tr(Core.uiLang, "At the top of the screen"); subtitle: Core.tr(Core.uiLang, "Away from your subtitle, which stays at the bottom"); checked: sp.g.karaoke_top === true; onToggled: function(on) { sp.dlg.set("general.karaoke_top", on) } }
            PrefSpin { title: Core.tr(Core.uiLang, "Size"); subtitle: Core.tr(Core.uiLang, "Percentage of the size the file asks for"); from: 40; to: 250; step: 5; value: sp.g.karaoke_size || 100; onMoved: function(v) { sp.dlg.set("general.karaoke_size", v) } }
            PrefButton {
                title: Core.tr(Core.uiLang, "Colours, font, outline and margins")
                subtitle: Core.tr(Core.uiLang, "In the Karaoke studio, with a live preview")
                label: Core.tr(Core.uiLang, "Open")
                onActivated: studio.open()
            }
        }

    }

    KaraokeStudio { id: studio }

    FontDialog {
        id: karaFontDlg
        title: Core.tr(Core.uiLang, "Font of the lyrics")
        onAccepted: sp.dlg.set("general.karaoke_font", selectedFont.family)
    }
    FontDialog {
        id: fontDlg
        title: Core.tr(Core.uiLang, "Subtitle font")
        onAccepted: sp.style("font", selectedFont.family)
    }
    ColorPickerAdapter {
        id: colorDlg
        property var target
        onAccepted: if (target) { var h = sp.hexOf(selectedColor); target.chosen(h) }
    }
}
