import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import Reelhouse 1.0

// The Pomodoro timer: a ring that empties as the phase runs, the time left, Start / Pause, Reset and Skip, the focus
// sessions of the cycle as dots, and its settings (lengths, rounds, ticking, chime) behind the gear.
// The core keeps the timer (it runs whatever page is open, rings and shows the macaw when a phase ends); this only
// reads Core.state("pomodoro") and counts down from `endsAt` by itself between updates.
Item {
    id: pw
    // the popover of the header: a smaller ring
    property bool compact: false
    // the left sidebar of the Study mode: the ring fits the sidebar, icon buttons, the settings open in a popover
    property bool side: false
    property bool settingsOpen: false
    property var p: ({})
    property real nowMs: Date.now()
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function tz() { return -new Date().getTimezoneOffset() }
    function refresh() { p = Core.state("pomodoro"); nowMs = Date.now() }
    function op(o) { Core.cmd({cmd: "pomo", op: o, tz: tz()}); refresh() }
    function setPref(k, v) { Core.cmd({cmd: "set_setting", path: "general." + k, value: v}); refresh() }
    readonly property bool running: p.running === true
    readonly property string phase: p.phase || "focus"
    readonly property real lengthMs: Math.max(1, p.length || 1)
    readonly property real leftMs: running ? Math.max(0, (p.endsAt || 0) - nowMs) : (p.left !== undefined ? p.left : lengthMs)
    readonly property int rounds: p.rounds || 4
    readonly property int done: p.done || 0
    // focus is the tomato, the short break green, the long break the accent
    readonly property color tone: phase === "focus" ? Theme.c.error : (phase === "short" ? Theme.c.success : Theme.c.accent)
    function clock(ms) { const s = Math.ceil(ms / 1000); return Math.floor(s / 60) + ":" + ("0" + (s % 60)).slice(-2) }
    function phaseName(ph) { return ph === "short" ? tr("Short break") : (ph === "long" ? tr("Long break") : tr("Focus")) }
    implicitWidth: side ? 208 : (compact ? 300 : 340)
    implicitHeight: col.implicitHeight
    Component.onCompleted: refresh()
    onVisibleChanged: if (visible) refresh()
    Connections { target: Core; function onStudyChanged() { if (pw.visible) pw.refresh() } }
    Timer {
        interval: 250; repeat: true
        running: pw.visible && pw.running
        // the core moves to the next phase on its own; ask again once the time is up
        onTriggered: { pw.nowMs = Date.now(); if (pw.leftMs <= 0) pw.refresh() }
    }

    Column {
        id: col
        width: parent.width
        spacing: pw.side ? 8 : (pw.compact ? 10 : 14)

        // the phase, and the gear of the settings
        Item {
            width: parent.width; height: pw.side ? 30 : 36
            Rectangle { id: phaseDot; width: 10; height: 10; radius: 5; color: pw.tone; anchors.verticalCenter: parent.verticalCenter }
            Lbl {
                anchors { left: phaseDot.right; leftMargin: 8; right: gear.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
                elide: Text.ElideRight
                text: pw.side ? pw.tr("Pomodoro") + " · " + pw.phaseName(pw.phase) : (pw.settingsOpen ? pw.tr("Pomodoro settings") : pw.phaseName(pw.phase))
                bold: true; size: pw.side ? 14 : 16
            }
            FlatButton {
                id: gear
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                kind: "flat"; glyph: pw.settingsOpen ? "check" : "settings"; implicitWidth: pw.side ? 30 : 36; implicitHeight: implicitWidth; iconSize: pw.side ? 16 : 18
                ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: pw.settingsOpen ? pw.tr("Done") : pw.tr("Pomodoro settings")
                onClicked: if (pw.side) sidePrefs.openRight(gear); else pw.settingsOpen = !pw.settingsOpen
            }
            // in the sidebar the settings would not fit: they open beside it, in a full timer
            Popover {
                id: sidePrefs
                padding: 14
                contentItem: Loader {
                    active: sidePrefs.visible
                    width: 320
                    source: "PomodoroWidget.qml"
                    onLoaded: { item.width = 320; item.compact = true; item.settingsOpen = true }
                }
            }
        }

        // ---------- the timer ----------
        Item {
            id: ring
            visible: !pw.settingsOpen
            readonly property real d: pw.side ? Math.min(pw.width, 132) : (pw.compact ? 168 : 208)
            width: parent.width; height: d
            readonly property real frac: Math.max(0, Math.min(1, pw.leftMs / pw.lengthMs))
            Shape {
                id: rs
                width: ring.d; height: ring.d
                anchors.horizontalCenter: parent.horizontalCenter
                preferredRendererType: Shape.CurveRenderer
                readonly property real r: ring.d / 2 - 8
                ShapePath {
                    strokeColor: Theme.c.fill3; strokeWidth: 10; fillColor: "transparent"
                    PathAngleArc { centerX: ring.d / 2; centerY: ring.d / 2; radiusX: rs.r; radiusY: rs.r; startAngle: 0; sweepAngle: 360 }
                }
                ShapePath {
                    strokeColor: ring.frac > 0 ? pw.tone : "transparent"; strokeWidth: 10; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                    PathAngleArc { centerX: ring.d / 2; centerY: ring.d / 2; radiusX: rs.r; radiusY: rs.r; startAngle: -90; sweepAngle: 360 * ring.frac }
                }
            }
            Column {
                anchors.centerIn: parent
                spacing: 2
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: pw.clock(pw.leftMs)
                    color: Theme.c.text
                    font.pixelSize: pw.side ? 30 : (pw.compact ? 38 : 46)
                    font.weight: Font.DemiBold
                    font.family: Theme.type.display || Theme.type.family
                    // digits of one width, so the clock does not wobble
                    font.features: { "tnum": 1 }
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: pw.running ? pw.phaseName(pw.phase) : (pw.p.fresh === true ? pw.tr("Ready") : pw.tr("Paused"))
                    color: Theme.c.textDim
                    font.pixelSize: pw.side ? 11 : 13; font.family: Theme.type.family
                    width: Math.min(implicitWidth, ring.d - 36); elide: Text.ElideRight; horizontalAlignment: Text.AlignHCenter
                }
            }
        }

        // the focus sessions of this cycle
        Row {
            visible: !pw.settingsOpen
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 6
            Repeater {
                model: pw.rounds
                delegate: Rectangle {
                    required property int index
                    width: 10; height: 10; radius: 5
                    color: index < pw.done ? Theme.c.error : "transparent"
                    border.width: index < pw.done ? 0 : 1.5
                    border.color: index === pw.done && pw.phase === "focus" ? Theme.c.error : Theme.c.textDim
                }
            }
        }

        Row {
            visible: !pw.settingsOpen
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 10
            FlatButton {
                anchors.verticalCenter: parent.verticalCenter
                kind: "circular"; glyph: "refresh"; iconSize: 16
                ToolTip.visible: hovered; ToolTip.delay: 400; ToolTip.text: pw.p.fresh === true && !pw.running ? pw.tr("Start the cycle over") : pw.tr("Reset")
                onClicked: pw.op("reset")
            }
            FlatButton {
                anchors.verticalCenter: parent.verticalCenter
                kind: "suggested"; pill: true
                glyph: pw.running ? "pause" : "play"
                readonly property string label: pw.running ? pw.tr("Pause") : (pw.p.fresh === true ? pw.tr("Begin") : pw.tr("Resume"))
                // the sidebar: a round button, the word in its tooltip
                text: pw.side ? "" : label
                implicitWidth: pw.side ? 44 : Math.max(130, contentItem.implicitWidth + 40)
                implicitHeight: pw.side ? 44 : 40
                ToolTip.visible: pw.side && hovered; ToolTip.delay: 400; ToolTip.text: label
                onClicked: pw.op("toggle")
            }
            FlatButton {
                anchors.verticalCenter: parent.verticalCenter
                kind: "circular"; glyph: "next"; iconSize: 16
                ToolTip.visible: hovered; ToolTip.delay: 400; ToolTip.text: pw.phase === "focus" ? pw.tr("Skip to the break") : pw.tr("Skip the break")
                onClicked: pw.op("skip")
            }
        }

        Text {
            visible: !pw.settingsOpen && !pw.side
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: pw.tr("Focus sessions today") + ": " + (pw.p.todayPomos || 0) + "  ·  " + pw.tr("Round") + " " + Math.min(pw.rounds, pw.done + (pw.phase === "focus" ? 1 : 0)) + "/" + pw.rounds
            color: Theme.c.textDim
            font.pixelSize: 12; font.family: Theme.type.family
        }

        // ---------- the settings ----------
        Column {
            visible: pw.settingsOpen
            width: parent.width
            PrefSpin { first: true; title: pw.tr("Focus (minutes)"); from: 1; to: 180; value: pw.p.focus || 25; onMoved: function(v) { pw.setPref("pomo_focus", Math.round(v)) } }
            PrefSpin { first: false; title: pw.tr("Short break (minutes)"); from: 1; to: 60; value: pw.p.short || 5; onMoved: function(v) { pw.setPref("pomo_short", Math.round(v)) } }
            PrefSpin { first: false; title: pw.tr("Long break (minutes)"); from: 1; to: 120; value: pw.p.long || 15; onMoved: function(v) { pw.setPref("pomo_long", Math.round(v)) } }
            PrefSpin { first: false; title: pw.tr("Focus sessions before a long break"); from: 1; to: 12; value: pw.p.rounds || 4; onMoved: function(v) { pw.setPref("pomo_rounds", Math.round(v)) } }
            PrefSwitch { first: false; title: pw.tr("Start the next phase by itself"); checked: pw.p.auto === true; onToggled: function(on) { pw.setPref("pomo_auto", on) } }
            PrefSwitch { first: false; title: pw.tr("Ticking during focus"); checked: pw.p.tick === true; onToggled: function(on) { pw.setPref("pomo_tick", on) } }
            PrefSwitch { first: false; title: pw.tr("Chime when a phase ends"); checked: pw.p.sound !== false; onToggled: function(on) { pw.setPref("pomo_sound", on) } }
            Item { width: 1; height: 6 }
            FlatButton {
                anchors.horizontalCenter: parent.horizontalCenter
                kind: "flat"; glyph: "volume"; text: pw.tr("Test the chime")
                onClicked: Core.cmd({cmd: "play_sound", name: "pomo_focus_end"})
            }
        }
    }
}
