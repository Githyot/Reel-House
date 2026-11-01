import QtQuick
import Reelhouse 1.0

// The orbs of time (PS2 menu background): seven glowing orbs with long trails flying round a sphere, gathering into
// a ball, a string of pearls and clusters by the second of the clock. Drawn by the native OrbsItem (qt/src/orbsitem.cpp,
// positions from reelhouse::orbs), behind the pages in controller mode. On dark windows the background under them is
// darkened to near black, like the console; the pages keep their own cards, so their text stays readable. It stops
// completely while hidden, minimized or when the window is in the background.
Item {
    id: ob
    readonly property bool darkBg: Qt.color(Theme.c.window).hslLightness < 0.5
    readonly property bool running: visible && Qt.application.state === Qt.ApplicationActive
                                    && !!Window.window && Window.window.visibility !== Window.Minimized

    // the dim layer: near black on dark windows; light windows keep their colour (deep blue orbs instead)
    Rectangle {
        anchors.fill: parent
        visible: ob.darkBg
        color: "#04060c"
        opacity: 0.88
    }
    OrbsItem {
        anchors.fill: parent
        running: ob.running
        dark: ob.darkBg
        ring: 0.36
    }
}
