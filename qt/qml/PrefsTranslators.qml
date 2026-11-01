import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Preferences > Translators (controller_translators.rs, translate.rs): which translator serves each place of the app
// that translates, a side-by-side test of every translator (time and result) to help choose, and the list of
// translators with their keys. "Automatic" keeps the choice each place always had.
Column {
    id: tp
    property var dlg
    readonly property var tc: dlg ? dlg.sec("translate") : ({})
    readonly property var tinfo: dlg && dlg.ex.translators !== undefined ? dlg.ex.translators : ({})
    readonly property var tlist: tinfo.list !== undefined ? tinfo.list : []
    readonly property var places: tinfo.uses !== undefined ? tinfo.uses : []
    // the slowest answer of the last test, so the bars of the comparison share one scale
    readonly property real slowest: {
        let m = 1
        for (let i = 0; i < tlist.length; ++i) if (tlist[i].state === "ok" && tlist[i].ms > m) m = tlist[i].ms
        return m
    }
    // the quickest answer, highlighted
    readonly property real fastest: {
        let m = -1
        for (let i = 0; i < tlist.length; ++i) if (tlist[i].state === "ok" && (m < 0 || tlist[i].ms < m)) m = tlist[i].ms
        return m
    }
    property string sampleText: ""
    spacing: 26

    function tt(x) { return Core.tr(Core.uiLang, x) }
    // ids and labels of the combo of a place: Automatic first, then every translator (those without a key say so)
    function choiceIds() { return ["auto"].concat(tlist.map(function(t) { return t.id })) }
    function choiceLabels(p) {
        return [tt("Automatic") + " (" + p.auto + ")"].concat(tlist.map(function(t) { return t.ready ? tt(t.name) : tt(t.name) + " — " + tt("no key yet") }))
    }
    // where a translator is used now, for its row ("Images reader, Text reader")
    function usedIn(id) {
        const out = []
        for (let i = 0; i < places.length; ++i) if (places[i].choice === id) out.push(tt(places[i].name))
        return out.join(", ")
    }
    // services whose key lives in the Accounts window of the preferences
    function hasAccount(id) { return id === "libretranslate" || id === "claude" || id === "gemini" || id === "openai" }
    function resultLine(t) {
        if (t.state === "running") return tt("Testing…")
        if (t.state === "ok") return "“" + t.text + "”"
        if (t.state === "error") return t.text
        return ""
    }

    // ---------- What is free and what needs a key ----------
    PrefGroup {
        title: tp.tt("Translators")
        description: tp.tt("Free services need nothing, but they are shared by everyone: they can be slow, refuse long chapters for a while or change without notice. Services with your own key (Microsoft Translator, Claude, Gemini, GPT) are faster and steadier; Microsoft gives 2 million characters a month free, the language models charge per use but translate dialogue more naturally. Test them below with the same sentence and choose for each place.")
    }

    // ---------- Where × which translator ----------
    PrefGroup {
        title: tp.tt("Where each translator is used")
        description: tp.tt("“Automatic” keeps the usual choice: the service of Preferences > Subtitles, or the free services one after another.")
        Repeater {
            model: tp.places.length
            delegate: PrefCombo {
                required property int index
                readonly property var place: tp.places[index] || ({})
                title: tp.tt(place.name || "")
                subtitle: tp.tt(place.detail || "")
                options: tp.choiceLabels(place)
                current: Math.max(0, tp.choiceIds().indexOf(place.choice || "auto"))
                onPicked: function(i) { tp.dlg.set("translate.per_use." + place.id, tp.choiceIds()[i]) }
            }
        }
    }

    // the local AI, when a place uses it (its state and, the first time, its download)
    // loaded by URL and desktop only: OllamaPanel.qml is not packed into the Android and iOS builds
    Loader {
        active: Qt.platform.os !== "android" && Qt.platform.os !== "ios" && (tp.usedIn("ollama") !== "" || (tp.dlg && tp.dlg.sec("subtitles").provider === "ollama"))
        visible: active
        width: parent.width
        source: active ? "OllamaPanel.qml" : ""
    }

    // ---------- Compare: the same sentence with every translator ----------
    PrefGroup {
        title: tp.tt("Compare the translators")
        description: tp.tt("Translates a sentence with every translator that is ready, all at the same time, and shows how long each took. Language:") + " " + (tp.tinfo.target || "")
        PrefEntry {
            title: tp.tt("Sentence to translate (empty: the example)")
            text: tp.sampleText
            onEdited: function(t) { tp.sampleText = t }
        }
        PrefButton {
            title: tp.tt("Test all")
            subtitle: "“" + (tp.sampleText.trim() !== "" ? tp.sampleText.trim() : (tp.tinfo.sample || "")) + "”"
            label: tp.tt("Test all")
            kind: "suggested"
            onActivated: Core.cmd({cmd: "translator_test_all", text: tp.sampleText})
        }
        Repeater {
            model: tp.tlist.length
            delegate: PrefRow {
                required property int index
                readonly property var tk: tp.tlist[index] || ({})
                visible: (tk.state || "") !== ""
                title: tp.tt(tk.name) + (tk.via && tk.via !== tk.name ? " · " + tk.via : "")
                subtitle: tp.resultLine(tk)
                selectable: true
                highlight: tk.state === "ok" && tk.ms === tp.fastest
                // a bar as long as the time, on the scale of the slowest answer
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: tk.state === "ok"
                    width: 90
                    height: 6
                    radius: 3
                    color: Theme.c.divider
                    Rectangle { width: Math.max(4, parent.width * (tk.ms || 0) / tp.slowest); height: parent.height; radius: 3; color: Theme.c.accent }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 70
                    horizontalAlignment: Text.AlignRight
                    text: tk.state === "ok" ? tk.ms + " ms" : (tk.state === "error" ? tp.tt("Failed") : "…")
                    color: tk.state === "error" ? Theme.c.error : Theme.c.textDim
                    font.pixelSize: 13
                    font.family: Theme.type.family
                }
            }
        }
    }

    // ---------- Every translator: state, keys, a test of its own ----------
    PrefGroup {
        title: tp.tt("Available translators")
        Repeater {
            model: tp.tlist.length
            delegate: PrefRow {
                required property int index
                readonly property var tk: tp.tlist[index] || ({})
                readonly property string used: tp.usedIn(tk.id)
                title: tp.tt(tk.name) + " · " + (tk.free ? tp.tt("Free") : (tk.ready ? tp.tt("Key saved") : tp.tt("Needs a key")))
                subtitle: tp.tt(tk.note || "") + (used !== "" ? "\n" + tp.tt("Used for:") + " " + used : "")
                    + ((tk.state || "") !== "" ? "\n" + (tk.state === "ok" ? tk.ms + " ms: " : "") + tp.resultLine(tk) : "")
                highlight: used !== ""
                FlatButton {
                    visible: tp.hasAccount(tk.id) || tk.id === "azure"
                    anchors.verticalCenter: parent.verticalCenter
                    kind: "flat"; glyph: "settings"; implicitWidth: 34; implicitHeight: 34
                    ToolTip.visible: hovered; ToolTip.text: tp.tt("Server and key of the translator")
                    onClicked: {
                        if (tk.id === "azure") azureGroup.expanded = true
                        else tp.dlg.openAccount(tk.id)
                    }
                }
                FlatButton {
                    anchors.verticalCenter: parent.verticalCenter
                    kind: tk.ready ? "plain" : "flat"
                    enabled: tk.ready && tk.state !== "running"
                    text: tp.tt("Test")
                    implicitHeight: 34
                    onClicked: Core.cmd({cmd: "translator_test", id: tk.id, text: tp.sampleText})
                }
            }
        }
    }

    // ---------- Microsoft Translator (Azure) ----------
    PrefGroup {
        id: azureGroup
        title: "Microsoft Translator (Azure)"
        collapsible: true
        expanded: (tp.tc.azure_key || "") === ""
        description: tp.tt("Create a Translator resource in the Azure portal (the free F0 tier gives 2 million characters a month), then copy one of its keys and its region from “Keys and Endpoint”.")
        PrefEntry { title: tp.tt("Key"); password: true; text: tp.tc.azure_key || ""; onEdited: function(t) { tp.dlg.set("translate.azure_key", t.trim()) } }
        PrefEntry { first: false; title: tp.tt("Region (for example westeurope; empty for a global resource)"); text: tp.tc.azure_region || ""; onEdited: function(t) { tp.dlg.set("translate.azure_region", t.trim()) } }
        PrefEntry { first: false; title: tp.tt("Address (optional, only for a resource with its own address)"); text: tp.tc.azure_endpoint || ""; onEdited: function(t) { tp.dlg.set("translate.azure_endpoint", t.trim()) } }
        PrefButton { title: tp.tt("Get a key"); subtitle: "portal.azure.com"; label: tp.tt("Open"); onActivated: Qt.openUrlExternally("https://portal.azure.com/#create/Microsoft.CognitiveServicesTextTranslation") }
        PrefButton {
            title: tp.tt("Test the translator")
            subtitle: {
                const a = tp.tlist.find(function(t) { return t.id === "azure" })
                return a && (a.state || "") !== "" ? (a.state === "ok" ? a.ms + " ms: " : "") + tp.resultLine(a) : ""
            }
            label: tp.tt("Test")
            kind: "suggested"
            onActivated: Core.cmd({cmd: "translator_test", id: "azure", text: tp.sampleText})
        }
    }

}
