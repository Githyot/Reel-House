import QtQuick
import Reelhouse 1.0
import "Osd.js" as Osd

// Trim bar of the player's Editor: two round handles (start and end) slide on the bar, and the bar zooms in by itself
// as the chosen part gets shorter, so the handles can be placed to the millisecond.
//   length of the part   zoom level   time shown on the bar
//   10 s or more         0            the whole video
//   under 10 s           1            30 s
//   under 5 s            2            15 s
//   under 2 s            3            6 s
// While a handle is dragged the bar only zooms in (around the handle, which stays under the pointer); it zooms back out
// when the handle is let go. Near the edges of the bar the dragged handle pans the view.
// `startTime`, `endTime` and `position` come from the player; the new times go out in `startEdited` / `endEdited`.
// The playhead is always drawn and kept in view: when it leaves the shown window (playing on, a seek with the keys) the
// bar pans to it, unless a handle is held.
Item {
    id: tl
    property real duration: 1
    property real position: 0
    property real startTime: -1
    property real endTime: -1
    property bool playing: false        // the video is playing: the playhead glides between the state updates
    property color fg: "#ffffff"
    property color accent: Theme.c.accent
    readonly property var o: Osd.t(Theme.layout.player)
    readonly property color ink: Osd.readable(accent)
    signal startEdited(real t)
    signal endEdited(real t)
    signal seekRequested(real t)

    readonly property real minGap: 0.05
    readonly property real delta: startTime >= 0 && endTime > startTime ? endTime - startTime : -1
    readonly property int zoomLevel: delta < 0 ? 0 : (delta < 2 ? 3 : (delta < 5 ? 2 : (delta < 10 ? 1 : 0)))
    readonly property var windows: [30, 15, 6]
    readonly property real tw: Math.max(1, track.width)

    // What the bar shows now: where it starts and how many seconds it holds.
    property int shownLevel: 0
    property real viewStart: 0
    property real viewSpan: Math.max(1, duration)
    property bool animate: true
    property bool dragging: false
    property string dragWhich: ""
    property real dragX: 0

    implicitHeight: 118
    implicitWidth: 480
    activeFocusOnTab: true

    function xOf(t) { return (t - viewStart) / viewSpan * tw }
    function tOf(x) { return viewStart + x / tw * viewSpan }
    function snap(t) { var q = zoomLevel >= 2 ? 1000 : 100; return Math.round(t * q) / q }
    function clamp(v, a, b) { return Math.max(a, Math.min(b, v)) }
    function spanOf(level) { return Math.max(1, level === 0 ? duration : Math.min(duration, windows[level - 1])) }
    function fmtT(t, digits) {
        t = Math.max(0, t)
        var m = Math.floor(t / 60), s = t - m * 60
        var str = digits > 0 ? s.toFixed(digits) : Math.floor(s).toString()
        if (s < 10) str = "0" + str
        return m + ":" + str
    }

    // The view the bar settles in: the chosen part in the middle.
    function retarget() {
        var span = spanOf(shownLevel)
        var c = delta >= 0 ? (startTime + endTime) / 2 : position
        viewSpan = span
        viewStart = clamp(c - span / 2, 0, Math.max(0, duration - span))
    }
    onDurationChanged: if (!dragging) retarget()
    // Keep the playhead in the window: when it goes out, the window moves so the playhead sits a quarter in from the
    // side it came from (the part stays in view when it fits).
    onPositionChanged: {
        if (!visible || dragging || viewSpan >= duration) return
        const margin = viewSpan * 0.04
        if (position >= viewStart + margin && position <= viewStart + viewSpan - margin) return
        const ahead = position > viewStart + viewSpan - margin
        viewStart = clamp(ahead ? position - viewSpan * 0.25 : position - viewSpan * 0.75, 0, Math.max(0, duration - viewSpan))
    }
    onStartTimeChanged: if (!dragging) retarget()
    onEndTimeChanged: if (!dragging) retarget()
    onZoomLevelChanged: {
        if (!dragging) { shownLevel = zoomLevel; retarget(); return }
        if (zoomLevel > shownLevel) {
            // Zoom in around the handle that is being dragged: it stays under the pointer.
            var th = dragWhich === "start" ? startTime : endTime
            shownLevel = zoomLevel
            var span = spanOf(shownLevel)
            animate = false
            viewSpan = span
            viewStart = clamp(th - dragX / tw * span, 0, Math.max(0, duration - span))
        }
    }
    Component.onCompleted: recenter()
    // Straight to the part (or the playhead) with no animation: when the Editor opens.
    function recenter() { animate = false; shownLevel = zoomLevel; retarget(); animate = true }

    Behavior on viewStart { enabled: tl.animate; NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
    Behavior on viewSpan { enabled: tl.animate; NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }

    // ---- dragging (mouse down, move, up on a handle, as times) ----
    property real grab: 0
    function dragBegin(which, x) {
        dragging = true; dragWhich = which; dragX = x; animate = false
        // where on the grip the pointer took hold, so the handle does not jump
        grab = x - xOf(which === "start" ? startTime : endTime)
        forceActiveFocus()
    }
    function dragMove(x) {
        if (!dragging) return
        dragX = x
        var t = snap(clamp(tOf(x - grab), 0, duration))
        if (dragWhich === "start") startEdited(Math.min(t, (endTime >= 0 ? endTime : duration) - minGap))
        else endEdited(Math.max(t, startTime + minGap))
    }
    function dragEnd() {
        dragging = false
        dragWhich = ""
        animate = true
        shownLevel = zoomLevel
        retarget()
    }
    function nudge(dir, big) {
        var which = dragWhich !== "" ? dragWhich : lastWhich
        var step = Math.max(0.001, snap(viewSpan / tw)) * (big ? 10 : 1) * dir
        if (which === "start") startEdited(clamp(snap(startTime + step), 0, endTime - minGap))
        else endEdited(clamp(snap(endTime + step), startTime + minGap, duration))
    }
    property string lastWhich: "start"
    Keys.onLeftPressed: function(e) { nudge(-1, e.modifiers & Qt.ShiftModifier) }
    Keys.onRightPressed: function(e) { nudge(1, e.modifiers & Qt.ShiftModifier) }

    // The view moves when a handle is held near an edge of the bar.
    Timer {
        interval: 16
        repeat: true
        running: tl.dragging && (tl.dragX < 28 || tl.dragX > tl.tw - 28)
        onTriggered: {
            var edge = 28
            var k = tl.dragX < edge ? -(edge - tl.dragX) / edge : (tl.dragX - (tl.tw - edge)) / edge
            k = tl.clamp(k, -1.5, 1.5)
            var vs = tl.clamp(tl.viewStart + k * tl.viewSpan * 0.012, 0, Math.max(0, tl.duration - tl.viewSpan))
            if (vs === tl.viewStart) return
            tl.viewStart = vs
            tl.dragMove(tl.dragX)
        }
    }

    // ---- the zoom and the length of the part ----
    Row {
        id: info
        width: parent.width
        height: 20
        Text { width: parent.width / 3; text: tl.delta >= 0 ? tl.fmtT(tl.delta, 3) + " s" : "—"; color: tl.fg; font.pixelSize: 13; font.weight: 600; font.family: Theme.type.family }
        // where the playhead is now
        Row {
            width: parent.width / 3
            spacing: 4
            Rectangle { width: 8; height: 8; radius: 4; color: tl.fg; anchors.verticalCenter: parent.verticalCenter; opacity: tl.playing ? 1 : 0.6 }
            Text { text: tl.fmtT(tl.position, 2); color: tl.fg; font.pixelSize: 12; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
        }
        Text { width: parent.width / 3; horizontalAlignment: Text.AlignRight; text: (tl.zoomLevel > 0 ? Core.tr(Core.uiLang, "Zoom") + " " + tl.zoomLevel : Core.tr(Core.uiLang, "Whole video")) + "  ·  " + (tl.viewSpan >= 100 ? Math.round(tl.viewSpan) : tl.viewSpan.toFixed(1)) + " s"; color: "#b3ffffff"; font.pixelSize: 12; font.family: Theme.type.family }
    }

    // ---- the bar ----
    Rectangle {
        id: track
        x: 14
        y: 40
        width: parent.width - 28
        height: 44
        // the theme's control shape: 4 px Windows, 8 px GNOME, 12 px Android and Apple
        radius: Theme.layout.player === "fluent" ? 4 : (Theme.layout.player === "adw" ? 8 : 12)
        color: "#1fffffff"
        border.width: Theme.layout.player === "m3" ? 0 : 1
        border.color: "#26ffffff"
        clip: true

        // Time marks: the step grows and shrinks with the zoom so the labels never crowd.
        readonly property var steps: [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600]
        readonly property real major: { for (var i = 0; i < steps.length; i++) if (steps[i] / tl.viewSpan * tl.tw >= 64) return steps[i]; return 3600 }
        readonly property real minor: major / 5
        readonly property int first: Math.floor(tl.viewStart / minor)
        readonly property int count: Math.min(400, Math.ceil(tl.viewSpan / minor) + 2)
        readonly property int digits: major < 0.01 ? 3 : (major < 0.1 ? 2 : (major < 1 ? 1 : 0))
        Repeater {
            model: track.count
            delegate: Rectangle {
                required property int index
                readonly property int n: track.first + index
                readonly property bool big: n % 5 === 0
                x: tl.xOf(n * track.minor)
                y: parent.height - height - 1
                width: 1
                height: big ? 14 : 7
                color: "#59ffffff"
                visible: x >= 0 && x <= tl.tw && n >= 0
            }
        }
        // The part that is kept.
        Rectangle {
            visible: tl.delta >= 0
            x: Math.max(0, tl.xOf(tl.startTime))
            width: Math.max(0, Math.min(tl.tw, tl.xOf(tl.endTime)) - Math.max(0, tl.xOf(tl.startTime)))
            height: parent.height
            color: Qt.rgba(tl.accent.r, tl.accent.g, tl.accent.b, 0.30)
            border.width: 0
            Rectangle { width: parent.width; height: 2; color: tl.accent }
            Rectangle { y: parent.height - 2; width: parent.width; height: 2; color: tl.accent }
        }
        // The playhead: a line with a small cap, gliding while the video plays (the player state comes ~4×/s).
        Item {
            id: playhead
            visible: tl.position >= tl.viewStart - 0.5 && tl.position <= tl.viewStart + tl.viewSpan + 0.5
            x: tl.xOf(tl.position)
            width: 1
            height: parent.height
            Behavior on x { enabled: tl.playing && !tl.dragging; SmoothedAnimation { duration: 260; velocity: -1 } }
            Rectangle { x: -1; width: 2; height: parent.height; color: tl.fg; opacity: 0.95 }
            Rectangle { x: -5; y: -5; width: 10; height: 10; radius: 5; color: tl.fg }
        }
        // Click or drag on the bar: the video goes there.
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onPressed: function(m) { tl.forceActiveFocus(); tl.seekRequested(tl.clamp(tl.tOf(m.x), 0, tl.duration)) }
            onPositionChanged: function(m) { if (pressed) tl.seekRequested(tl.clamp(tl.tOf(m.x), 0, tl.duration)) }
        }
    }
    // Labels of the major marks, under the bar.
    Item {
        x: track.x; y: track.y + track.height + 3; width: track.width; height: 16; clip: true
        Repeater {
            model: track.count
            delegate: Text {
                required property int index
                readonly property int n: track.first + index
                visible: n % 5 === 0 && n >= 0 && tl.xOf(n * track.minor) > -20 && tl.xOf(n * track.minor) < tl.tw + 20
                x: Math.max(0, Math.min(tl.tw - width, tl.xOf(n * track.minor) - width / 2))
                text: tl.fmtT(n * track.minor, track.digits)
                color: tl.o.muted; font.pixelSize: 11; font.family: Theme.type.family
            }
        }
    }

    // ---- the two round handles ----
    component Handle: Item {
        id: h
        property string which: "start"
        property real t: 0
        readonly property bool isStart: which === "start"
        readonly property bool on: t >= 0 && tl.xOf(t) >= -2 && tl.xOf(t) <= tl.tw + 2
        readonly property real px: track.x + tl.xOf(t)
        visible: on
        z: tl.dragWhich === which ? 3 : 1
        x: px - (isStart ? 25 : 3)
        y: track.y - 8
        width: 28
        height: track.height + 16
        // the line on the cut
        Rectangle { x: isStart ? 24 : 2; width: 3; height: parent.height; radius: 1.5; color: tl.accent }
        // the round grip, on the side of the line that is outside the kept part
        Rectangle {
            id: grip
            x: isStart ? 1 : 6
            anchors.verticalCenter: parent.verticalCenter
            width: tl.dragWhich === h.which ? 24 : 22
            height: width
            radius: width / 2
            color: "#ffffff"
            border.width: 3
            border.color: tl.accent
            scale: hover.containsMouse && tl.dragWhich !== h.which ? 1.08 : 1
            Behavior on scale { NumberAnimation { duration: 90 } }
            Behavior on width { NumberAnimation { duration: 90 } }
        }
        // the time over the handle, down to the millisecond
        Rectangle {
            visible: tl.dragWhich === h.which || hover.containsMouse
            x: (isStart ? 25 : 3) - width / 2
            y: -26
            width: bubble.implicitWidth + 14
            height: 22
            radius: Theme.layout.player === "fluent" ? 4 : height / 2
            color: tl.accent
            Text { id: bubble; anchors.centerIn: parent; text: tl.fmtT(h.t, 3); color: Theme.c.accentFg; font.pixelSize: 12; font.weight: 600; font.family: Theme.type.family }
        }
        MouseArea {
            id: hover
            anchors.fill: parent
            hoverEnabled: true
            preventStealing: true
            cursorShape: Qt.SizeHorCursor
            // mouse down / move / up become times of the video through the bar
            onPressed: function(m) { tl.lastWhich = h.which; tl.dragBegin(h.which, mapToItem(track, m.x, 0).x) }
            onPositionChanged: function(m) { if (pressed) tl.dragMove(mapToItem(track, m.x, 0).x) }
            onReleased: tl.dragEnd()
            onCanceled: tl.dragEnd()
        }
    }
    Handle { which: "start"; t: tl.startTime }
    Handle { which: "end"; t: tl.endTime }
}
