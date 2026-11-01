import QtQuick
import Reelhouse 1.0

// Lies over the Study page and tells the core, every 20 s, whether the window has the focus and whether the pointer
// moved since the last report: the study-time counter stops after two minutes without any activity (answers and
// other Study commands count by themselves). It takes no clicks and does not block the hover of what is below.
Item {
    id: bc
    property bool moved: false
    readonly property bool winActive: Window.active
    function ping() {
        Core.cmd({cmd: "time_ping", focused: bc.winActive, input: bc.moved, tz: -new Date().getTimezoneOffset()})
        bc.moved = false
    }
    HoverHandler {
        blocking: false
        onPointChanged: bc.moved = true
    }
    Timer { interval: 20000; repeat: true; running: bc.visible; onTriggered: bc.ping() }
    onWinActiveChanged: if (visible) ping()
    onVisibleChanged: if (visible) { moved = true; ping() }
}
