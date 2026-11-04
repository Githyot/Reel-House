import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Study > Conversation, Start tab: Conversation by voice with Gemini Live (src/live.rs, src/kaiwa_live.rs).
// Without a Gemini key: what it is, the free key of Google AI Studio and a field to paste it (the same key as
// Preferences > Addons & accounts > Gemini). With one: whether scenes, lessons and free conversation start spoken (the
// voice call always is), push-to-talk or an open microphone, and under "Options" the headphones, the pause that ends a
// turn, the voice and the Live model. Desktop only (packed in "desktop_ai" with KaiwaPage).
Rectangle {
    id: vc
    // kaiwa_json().live
    property var info: ({})
    property bool more: false
    function tr(t) { return Core.tr(Core.uiLang, t) }
    function opts(o) { Core.cmd(Object.assign({cmd: "kaiwa_live_opts"}, o)) }
    readonly property bool hasKey: vc.info.hasKey === true
    readonly property bool usable: vc.info.available === true
    readonly property bool ptt: vc.info.mic === "ptt"
    readonly property var pauses: [800, 1200, 1800, 2500]
    readonly property var voices: vc.info.voices || []
    readonly property int pauseIdx: {
        const p = vc.info.pause || 1200
        let best = 0
        for (let i = 1; i < vc.pauses.length; ++i) if (Math.abs(vc.pauses[i] - p) < Math.abs(vc.pauses[best] - p)) best = i
        return best
    }
    radius: Theme.shape.card
    color: Theme.c.card
    border.width: Theme.id === "windows" || Theme.id === "gnome" ? 1 : 0
    border.color: Theme.id === "windows" ? Theme.c.border : Theme.c.divider
    implicitHeight: col.implicitHeight + 28

    Column {
        id: col
        x: 16; y: 14; width: parent.width - 32
        spacing: 8
        Item {
            width: parent.width; height: Math.max(headCol.implicitHeight, 34)
            Row {
                spacing: 10
                anchors.verticalCenter: parent.verticalCenter
                Lbl { text: "🎙️"; size: 22; anchors.verticalCenter: parent.verticalCenter }
                Column {
                    id: headCol
                    anchors.verticalCenter: parent.verticalCenter
                    Lbl { text: vc.tr("Conversation by voice"); bold: true; size: 16 }
                    Lbl { text: "Gemini Live" + (vc.usable ? " · " + (vc.info.model || "") : ""); dim: true; size: 12 }
                }
            }
            FlatButton {
                visible: vc.hasKey
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                kind: vc.more ? "suggested" : "flat"; glyph: "settings"; text: vc.tr("Options"); implicitHeight: 32
                onClicked: vc.more = !vc.more
            }
        }
        Lbl {
            width: parent.width; wrapMode: Text.Wrap; size: 13
            color: vc.hasKey && !vc.usable ? Theme.c.warningText : Theme.c.textDim
            text: !vc.hasKey ? vc.tr("Talking out loud with the tutor uses Gemini Live, free with a Google AI Studio key (the local AI and Claude cannot hear audio).")
                             : (vc.usable ? vc.tr("Talk out loud in real time: the tutor hears you, answers with its voice, notes your mistakes and keeps the scene going.")
                                          : vc.tr(vc.info.missing || ""))
        }
        // no key yet: make one (free) and paste it here
        Row {
            visible: !vc.hasKey
            width: parent.width
            spacing: 8
            TextField {
                id: keyField
                width: parent.width - saveKey.width - getKey.width - 16
                height: 34
                echoMode: TextInput.Password
                placeholderText: vc.tr("Paste your Gemini key")
                color: Theme.c.text; placeholderTextColor: Theme.c.textDim; font.family: Theme.type.family; font.pixelSize: 13
                background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: keyField.activeFocus ? 2 : 0; border.color: Theme.c.accent }
                onAccepted: saveKey.clicked()
            }
            FlatButton {
                id: saveKey
                kind: "suggested"; text: vc.tr("Save"); implicitHeight: 34
                enabled: keyField.text.trim() !== ""
                onClicked: {
                    if (!enabled) return
                    Core.cmd({cmd: "set_setting", path: "subtitles.gemini_key", value: keyField.text.trim()})
                    keyField.text = ""
                }
            }
            FlatButton {
                id: getKey
                kind: "flat"; glyph: "external"; text: vc.tr("Get a free key"); implicitHeight: 34
                ToolTip.visible: hovered; ToolTip.text: vc.info.keyUrl || "https://aistudio.google.com/apikey"
                onClicked: Qt.openUrlExternally(vc.info.keyUrl || "https://aistudio.google.com/apikey")
            }
        }
        Column {
            visible: vc.hasKey
            width: parent.width
            PrefSwitch {
                first: true
                title: vc.tr("Speak in scenes, lessons and free conversation")
                subtitle: vc.tr("The voice call is always spoken. Story time stays written.")
                checked: vc.info.speak === true
                onToggled: function(on) { vc.opts({speak: on}) }
            }
            PrefCombo {
                first: false
                title: vc.tr("Microphone")
                subtitle: vc.ptt ? vc.tr("Hold the button (or the space bar) while you speak: the most reliable in noisy rooms.") : vc.tr("Just talk: a pause ends your turn.")
                options: [vc.tr("Open microphone"), vc.tr("Push to talk")]
                current: vc.ptt ? 1 : 0
                onPicked: function(i) { vc.opts({mic: i === 1 ? "ptt" : "open"}) }
            }
            PrefSwitch {
                visible: vc.more
                first: false
                title: vc.tr("I use headphones")
                subtitle: vc.tr("The microphone stays on while the tutor speaks, so you can interrupt it. Without headphones it is muted then, so the tutor does not hear itself.")
                checked: vc.info.headset === true
                onToggled: function(on) { vc.opts({headset: on}) }
            }
            PrefCombo {
                visible: vc.more && !vc.ptt
                first: false
                title: vc.tr("Pause that ends your turn")
                subtitle: vc.tr("Longer gives you time to think in the middle of a sentence.")
                options: [vc.tr("Short (0.8 s)"), vc.tr("Normal (1.2 s)"), vc.tr("Long (1.8 s)"), vc.tr("Very long (2.5 s)")]
                current: vc.pauseIdx
                onPicked: function(i) { vc.opts({pause: vc.pauses[i]}) }
            }
            PrefCombo {
                visible: vc.more
                first: false
                title: vc.tr("Voice of the tutor")
                options: [vc.tr("Automatic") + " (" + (vc.info.voice || "") + ")"].concat(vc.voices)
                current: (vc.info.voiceSet || "") === "" ? 0 : Math.max(0, vc.voices.indexOf(vc.info.voiceSet) + 1)
                onPicked: function(i) { vc.opts({voice: i === 0 ? "" : vc.voices[i - 1]}) }
            }
            PrefEntry {
                visible: vc.more
                first: false
                title: vc.tr("Live model (empty: the default)")
                text: vc.info.modelSet || ""
                onEdited: function(t) { vc.opts({model: t.trim()}) }
            }
        }
    }
}
