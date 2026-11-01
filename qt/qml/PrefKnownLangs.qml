import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Preferences > General > "Languages I know": an ordered list (the first is the main language), plus the language
// being learned. The player uses the list to choose subtitles (src/controller_sub.rs): one in a known language is
// shown as it is, otherwise the best one is translated into the main language.
PrefGroup {
    id: kl
    property var known: []            // general.known_langs as saved (may be empty)
    property string fallback: "en"     // what an empty list means: the subtitle target / interface language
    property string studyLang: ""      // general.study_lang
    signal changedList(var list)
    signal changedStudy(string code)

    // Common languages by their own names, so a speaker finds theirs whatever the interface language is.
    readonly property var langs: [
        ["en", "English"], ["pt", "Português"], ["es", "Español"], ["fr", "Français"], ["de", "Deutsch"],
        ["it", "Italiano"], ["ja", "日本語"], ["ko", "한국어"], ["zh", "中文"], ["ru", "Русский"],
        ["ar", "العربية"], ["hi", "हिन्दी"], ["tr", "Türkçe"], ["pl", "Polski"], ["nl", "Nederlands"],
        ["sv", "Svenska"], ["no", "Norsk"], ["da", "Dansk"], ["fi", "Suomi"], ["cs", "Čeština"],
        ["hu", "Magyar"], ["ro", "Română"], ["el", "Ελληνικά"], ["he", "עברית"], ["th", "ไทย"],
        ["vi", "Tiếng Việt"], ["id", "Bahasa Indonesia"], ["ms", "Bahasa Melayu"], ["tl", "Filipino"], ["uk", "Українська"],
        ["bg", "Български"], ["hr", "Hrvatski"], ["sr", "Српски"], ["ca", "Català"], ["fa", "فارسی"]
    ]
    function base(code) { return ("" + (code || "")).toLowerCase().replace("_", "-").split("-")[0] }
    function nameOf(code) {
        for (var i = 0; i < langs.length; ++i) if (langs[i][0] === code) return langs[i][1]
        return code.toUpperCase()
    }
    // The list as the player uses it: an empty setting stands for the one language subtitles were made for.
    readonly property var shown: {
        const out = []
        for (var i = 0; i < known.length; ++i) { const b = base(known[i]); if (b !== "" && out.indexOf(b) < 0) out.push(b) }
        if (out.length === 0) out.push(base(fallback) || "en")
        return out
    }
    function move(i, d) {
        const l = shown.slice(), j = i + d
        if (j < 0 || j >= l.length) return
        const t = l[i]; l[i] = l[j]; l[j] = t
        changedList(l)
    }
    function remove(i) { const l = shown.slice(); l.splice(i, 1); changedList(l) }
    function add(code) { const l = shown.slice(); if (l.indexOf(code) < 0) l.push(code); changedList(l) }

    title: Core.tr(Core.uiLang, "Languages I know")
    description: Core.tr(Core.uiLang, "Subtitles in these languages are shown as they are, the first one you have wins. When a video has none, the best one is translated into your main language.")

    Repeater {
        model: kl.shown.length
        delegate: PrefRow {
            id: row
            required property int index
            readonly property string code: kl.shown[index] || ""
            title: kl.nameOf(code)
            subtitle: index === 0 ? Core.tr(Core.uiLang, "Main language") : ""
            FlatButton { kind: "flatround"; glyph: "up"; iconSize: 16; enabled: row.index > 0; anchors.verticalCenter: parent.verticalCenter; onClicked: kl.move(row.index, -1); ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Move up") }
            FlatButton { kind: "flatround"; glyph: "down"; iconSize: 16; enabled: row.index < kl.shown.length - 1; anchors.verticalCenter: parent.verticalCenter; onClicked: kl.move(row.index, 1); ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Move down") }
            FlatButton { kind: "flatround"; glyph: "close"; iconSize: 16; enabled: kl.shown.length > 1; anchors.verticalCenter: parent.verticalCenter; onClicked: kl.remove(row.index); ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Remove") }
        }
    }
    PrefRow {
        title: Core.tr(Core.uiLang, "Add a language")
        FlatButton { id: addBtn; glyph: "plus"; text: Core.tr(Core.uiLang, "Add"); implicitHeight: 34; anchors.verticalCenter: parent.verticalCenter; onClicked: addPop.openBelow(addBtn, true) }
    }
    PrefRow {
        id: studyRow
        title: Core.tr(Core.uiLang, "Language I'm learning")
        subtitle: Core.tr(Core.uiLang, "Used by Study and by the questions in the player")
        Text { text: kl.studyLang === "" ? Core.tr(Core.uiLang, "None") : kl.nameOf(kl.studyLang); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
        Icon { id: studyArrow; name: "down"; size: 16; color: Theme.c.textDim; anchors.verticalCenter: parent.verticalCenter }
        MouseArea { parent: studyRow; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: studyPop.openBelow(studyArrow, true) }
    }

    // The language lists: long, so they scroll inside the popover.
    component LangList: ListView {
        id: lv
        property var codes: []
        property string current: ""
        // an inline component cannot see the ids of this file: the names come in as a function
        property var namer: function(c) { return c }
        signal chosen(string code)
        implicitWidth: 260
        implicitHeight: Math.min(contentHeight, 340)
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }
        model: codes.length
        delegate: Rectangle {
            required property int index
            readonly property string code: lv.codes[index]
            width: lv.width
            height: 34
            radius: Theme.shape.small
            color: area.containsMouse ? Theme.c.hover : "transparent"
            Text { x: 10; width: parent.width - 44; elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter; text: parent.code === "" ? Core.tr(Core.uiLang, "None") : lv.namer(parent.code); color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
            Icon { visible: parent.code === lv.current; name: "check"; size: 16; color: Theme.c.text; anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } }
            MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: lv.chosen(parent.code) }
        }
    }
    Popover {
        id: addPop
        contentItem: LangList {
            namer: kl.nameOf
            codes: kl.langs.map(function(l) { return l[0] }).filter(function(c) { return kl.shown.indexOf(c) < 0 })
            onChosen: function(code) { addPop.close(); kl.add(code) }
        }
    }
    Popover {
        id: studyPop
        contentItem: LangList {
            namer: kl.nameOf
            codes: [""].concat(kl.langs.map(function(l) { return l[0] }))
            current: kl.studyLang
            onChosen: function(code) { studyPop.close(); kl.changedStudy(code) }
        }
    }
}
