import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The Pomodoro in the header bar (or at the bottom of the sidebar): a stopwatch that fills with the phase colour
// while the timer runs, with the time left beside it in the header; a click opens the timer in a popover.
FlatButton {
    id: pb
    // in the sidebar: no room for the time, and the popover opens to the right
    property bool side: false
    property var p: ({})
    property real nowMs: Date.now()
    function refresh() { p = Core.state("pomodoro"); nowMs = Date.now() }
    readonly property bool running: p.running === true
    // shown while a phase is under way (running or paused)
    readonly property bool busy: running || (p.fresh === false)
    readonly property real leftMs: running ? Math.max(0, (p.endsAt || 0) - nowMs) : (p.left || 0)
    readonly property color tone: p.phase === "short" ? Theme.c.success : (p.phase === "long" ? Theme.c.accent : Theme.c.error)
    function clock(ms) { const s = Math.ceil(ms / 1000); return Math.floor(s / 60) + ":" + ("0" + (s % 60)).slice(-2) }
    kind: "flat"
    implicitWidth: side ? 40 : (busy ? contentItem.implicitWidth + 20 : 40)
    ToolTip.visible: hovered && !pop.visible
    ToolTip.text: Core.tr(Core.uiLang, "Pomodoro timer")
    // the core's days follow the window's time zone (minutes east of UTC)
    Component.onCompleted: { Core.cmd({cmd: "pomo", op: "sync", tz: -new Date().getTimezoneOffset()}); refresh() }
    Connections { target: Core; function onStudyChanged() { pb.refresh() } }
    // once a second is enough for the button; the popover counts more finely
    Timer { interval: 1000; repeat: true; running: pb.visible && pb.running; onTriggered: { pb.nowMs = Date.now(); if (pb.leftMs <= 0) pb.refresh() } }
    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight
        Row {
            id: row
            anchors.centerIn: parent
            spacing: 6
            PomodoroGlyph {
                anchors.verticalCenter: parent.verticalCenter
                size: 18
                color: pb.busy ? pb.tone : Theme.c.text
                tone: pb.tone
                progress: pb.busy ? pb.leftMs / Math.max(1, pb.p.length || 1) : -1
            }
            Text {
                visible: pb.busy && !pb.side
                anchors.verticalCenter: parent.verticalCenter
                text: pb.clock(pb.leftMs)
                color: pb.running ? Theme.c.text : Theme.c.textDim
                font.pixelSize: 14; font.weight: Font.DemiBold; font.family: Theme.type.family
                font.features: { "tnum": 1 }
            }
        }
    }
    onClicked: { if (pb.side) pop.openRight(pb); else pop.openBelow(pb, true) }
    Popover {
        id: pop
        padding: 14
        contentItem: PomodoroWidget { compact: true; width: 300 }
    }
}
