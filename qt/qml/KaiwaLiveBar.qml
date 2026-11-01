import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The bottom of a spoken session of Conversation (Gemini Live, controller_kaiwa.rs): the goals of the scene and the
// score, one big microphone button (push-to-talk: hold it, or the space bar; open microphone: on / off) with the level
// of the voice round it, what the call is doing, a hint, hanging up / calling again, and a line to type a word that
// will not come out. `live` is the light state "kaiwa_live" that KaiwaPage reads several times a second during a call.
Item {
    id: lb
    property var live: ({})
    // kaiwa_json().live (the options) and the open session
    property var info: ({})
    property var sess: null
    function tr(t) { return Core.tr(Core.uiLang, t) }
    function talk(down) { Core.cmd({cmd: "kaiwa_live_talk", down: down}) }
    readonly property bool running: lb.live.running === true && lb.sess !== null && lb.live.sid === lb.sess.id
    readonly property bool ptt: (lb.running ? lb.live.mic : lb.info.mic) === "ptt"
    readonly property bool held: lb.running && lb.live.open === true
    readonly property bool connecting: lb.running && lb.live.phase !== "live"
    readonly property bool speakingNow: lb.running && lb.live.speaking === true
    readonly property var goals: lb.sess && lb.sess.goals ? lb.sess.goals : []
    property bool pressing: false
    implicitHeight: col.implicitHeight + 12

    function secs(n) {
        const m = Math.floor(n / 60), s = Math.floor(n % 60)
        return m + ":" + (s < 10 ? "0" : "") + s
    }
    readonly property string statusText: {
        if (!lb.running) return lb.tr("The call ended. Call again to go on, or End for the report.")
        if (lb.live.ending === true) return lb.tr("The scene is over: the tutor says goodbye…")
        if (lb.live.phase === "connecting") return lb.tr("Connecting to Gemini Live…")
        if (lb.live.phase === "reconnecting") return lb.tr("Reconnecting…")
        if (lb.speakingNow) return lb.live.muted === true ? lb.tr("The tutor is speaking (your microphone waits)") : (lb.ptt ? lb.tr("The tutor is speaking: hold the button to interrupt") : lb.tr("The tutor is speaking: talk to interrupt"))
        if (lb.ptt) return lb.held ? lb.tr("Speak now, release to send") : lb.tr("Hold the button (or the space bar) to speak")
        return lb.held ? lb.tr("Listening: just talk") : lb.tr("Microphone off: click to turn it on")
    }

    // push-to-talk with the space bar (while the bar has the focus: a click on the button gives it)
    focus: true
    Keys.onPressed: function(e) {
        if (e.key === Qt.Key_Space && lb.ptt && lb.running) {
            if (!e.isAutoRepeat && !lb.pressing) { lb.pressing = true; lb.talk(true) }
            e.accepted = true
        }
    }
    Keys.onReleased: function(e) {
        if (e.key === Qt.Key_Space && lb.ptt) {
            if (!e.isAutoRepeat && lb.pressing) { lb.pressing = false; lb.talk(false) }
            e.accepted = true
        }
    }

    Column {
        id: col
        width: parent.width
        spacing: 8
        topPadding: 6
        // the goals of the scene and the score
        Flow {
            visible: lb.goals.length > 0
            width: parent.width
            spacing: 6
            Repeater {
                model: lb.goals
                delegate: Rectangle {
                    id: goalChip
                    required property var modelData
                    required property int index
                    readonly property bool done: goalChip.modelData.done === true
                    height: 28; radius: 14
                    width: Math.min(lb.width, gl.implicitWidth + 24)
                    color: done ? Qt.rgba(Theme.c.success.r, Theme.c.success.g, Theme.c.success.b, 0.18) : Theme.c.fill
                    Lbl {
                        id: gl
                        x: 12; width: parent.width - 24; anchors.verticalCenter: parent.verticalCenter
                        elide: Text.ElideRight; size: 12
                        text: (goalChip.done ? "✓ " : (goalChip.index + 1) + ". ") + lb.tr(goalChip.modelData.text || "")
                        color: goalChip.done ? Theme.c.success : Theme.c.text
                    }
                }
            }
            Rectangle {
                height: 28; radius: 14
                width: sc.implicitWidth + 24
                color: Qt.rgba(Theme.c.accent.r, Theme.c.accent.g, Theme.c.accent.b, 0.16)
                Lbl { id: sc; anchors.centerIn: parent; size: 12; bold: true; color: Theme.c.accentText; text: "★ " + (lb.sess ? (lb.sess.score || 0) : 0) }
            }
        }
        // hint · the microphone · hang up / call again
        Item {
            width: parent.width; height: 80
            FlatButton {
                anchors { right: micArea.left; rightMargin: 24; verticalCenter: parent.verticalCenter }
                visible: lb.running
                kind: "flat"; glyph: "lightbulb"; implicitWidth: 44; implicitHeight: 44
                enabled: !lb.connecting
                ToolTip.visible: hovered; ToolTip.text: lb.tr("Hint: something you could say")
                onClicked: Core.cmd({cmd: "kaiwa_hint"})
            }
            Item {
                id: micArea
                width: 76; height: 76
                anchors.centerIn: parent
                // the level of the voice
                Rectangle {
                    anchors.centerIn: parent
                    readonly property real lvl: lb.held && lb.live.muted !== true ? Math.max(0, Math.min(1, lb.live.level || 0)) : 0
                    width: 76 + lvl * 26; height: width; radius: width / 2
                    visible: lvl > 0.05
                    color: Qt.rgba(Theme.c.accent.r, Theme.c.accent.g, Theme.c.accent.b, 0.25)
                }
                // the tutor speaking
                Rectangle {
                    anchors.centerIn: parent
                    width: 88; height: 88; radius: 44
                    visible: lb.speakingNow
                    color: "transparent"
                    border.width: 3; border.color: Theme.c.accentText
                    SequentialAnimation on opacity { running: lb.speakingNow; loops: Animation.Infinite; NumberAnimation { from: 0.25; to: 0.9; duration: 420 } NumberAnimation { from: 0.9; to: 0.25; duration: 420 } }
                }
                Rectangle {
                    anchors.fill: parent; radius: 38
                    color: !lb.running ? Theme.c.success : (lb.held ? (lb.ptt ? Theme.c.error : Theme.c.accent) : Theme.c.button)
                    opacity: lb.connecting ? 0.55 : 1
                    Icon {
                        anchors.centerIn: parent
                        name: !lb.running ? "play" : (lb.held ? "mic" : (lb.ptt ? "mic" : "mute"))
                        size: 30
                        color: !lb.running || lb.held ? Theme.c.accentFg : Theme.c.text
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        enabled: !lb.connecting
                        onPressed: {
                            lb.forceActiveFocus()
                            if (lb.running && lb.ptt) { lb.pressing = true; lb.talk(true) }
                        }
                        onReleased: if (lb.pressing) { lb.pressing = false; lb.talk(false) }
                        onCanceled: if (lb.pressing) { lb.pressing = false; lb.talk(false) }
                        onClicked: {
                            if (!lb.running) Core.cmd({cmd: "kaiwa_live_call"})
                            else if (!lb.ptt) lb.talk(!lb.held)
                        }
                    }
                }
            }
            FlatButton {
                anchors { left: micArea.right; leftMargin: 24; verticalCenter: parent.verticalCenter }
                visible: lb.running
                kind: "destructive"; glyph: "close"; text: lb.tr("Hang up"); implicitHeight: 40
                onClicked: Core.cmd({cmd: "kaiwa_live_stop"})
            }
        }
        Lbl {
            width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap
            dim: true; size: 13
            text: lb.statusText + (lb.running && lb.live.seconds !== undefined ? "  ·  " + lb.secs(lb.live.seconds) : "")
                  + (lb.running && typeof lb.live.latency === "number" ? "  ·  " + lb.tr("answer in %1 ms").replace("%1", Math.round(lb.live.latency)) : "")
        }
        Lbl {
            visible: lb.running && (lb.live.warn || "") !== ""
            width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap
            color: Theme.c.warning; size: 12
            text: lb.live.warn || ""
        }
        // a word that will not come out: typed, it goes as your turn
        Row {
            visible: lb.running
            width: parent.width
            spacing: 6
            TextField {
                id: liveInput
                width: parent.width - liveSend.width - 6
                height: 36
                placeholderText: lb.tr("Or type it here (Enter sends)")
                color: Theme.c.text; placeholderTextColor: Theme.c.textDim; font.family: Theme.type.family; font.pixelSize: 14
                background: Rectangle { radius: 18; color: Theme.c.fill3; border.width: liveInput.activeFocus ? 2 : 0; border.color: Theme.c.accent }
                leftPadding: 14; rightPadding: 14
                onAccepted: liveSend.clicked()
            }
            FlatButton {
                id: liveSend
                kind: "suggested"; glyph: "send"; implicitWidth: 44; implicitHeight: 36
                enabled: liveInput.text.trim() !== ""
                onClicked: {
                    if (!enabled) return
                    Core.cmd({cmd: "kaiwa_live_text", text: liveInput.text})
                    liveInput.text = ""
                    lb.forceActiveFocus()
                }
            }
        }
    }
}
