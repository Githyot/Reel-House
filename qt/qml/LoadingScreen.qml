import QtQuick
import Reelhouse 1.0

// The loading screen of the app start: the macaw with its popcorn on the yellow disc (qt/icon/reelhouse-round.svg,
// tools/app_icon.py), bobbing over the dark splash colour, three dots hopping under it. On Android it takes over from
// the system splash and the Qt splash (qt/android/res/drawable/splash.xml), which show the same disc on the same
// colour, so the start reads as one screen. It fades out once the core has sent its first data (Core.home), after
// at least `minMs` (no flash) and at most `maxMs` (a slow network never holds the app back).
Item {
    id: root
    property int minMs: 700
    property int maxMs: 3000
    readonly property color backdrop: "#1b1530"   // = @color/splash_background on Android
    property bool minDone: false
    readonly property bool coreReady: Core.home !== undefined && Object.keys(Core.home).length > 0
    property bool done: false
    function finish() { if (!done) { done = true; fade.start() } }
    onCoreReadyChanged: if (coreReady && minDone) finish()

    Timer { interval: root.minMs; running: true; onTriggered: { root.minDone = true; if (root.coreReady) root.finish() } }
    Timer { interval: root.maxMs; running: true; onTriggered: root.finish() }

    visible: opacity > 0
    // swallow clicks and keys while it covers the app
    MouseArea { anchors.fill: parent; enabled: !root.done; hoverEnabled: true }

    Rectangle { anchors.fill: parent; color: root.backdrop }

    Column {
        id: col
        anchors.centerIn: parent
        anchors.verticalCenterOffset: -12
        spacing: 22

        Item {
            // 160 dp, the size of the Android splash disc
            width: 160; height: 160
            anchors.horizontalCenter: parent.horizontalCenter
            Image {
                id: logo
                width: parent.width; height: parent.height
                source: "qrc:/icon/reelhouse-round-512.png"
                sourceSize: Qt.size(512, 512)
                smooth: true
                mipmap: true
                transformOrigin: Item.Bottom
                // a little hop with a squash on landing, like a bird bouncing in its seat at the cinema
                SequentialAnimation on y {
                    loops: Animation.Infinite
                    running: root.visible
                    PauseAnimation { duration: 250 }
                    NumberAnimation { from: 0; to: -14; duration: 280; easing.type: Easing.OutQuad }
                    NumberAnimation { from: -14; to: 0; duration: 280; easing.type: Easing.InQuad }
                    PauseAnimation { duration: 340 }
                }
                SequentialAnimation on scale {
                    loops: Animation.Infinite
                    running: root.visible
                    NumberAnimation { from: 1.0; to: 0.96; duration: 120 }
                    NumberAnimation { from: 0.96; to: 1.0; duration: 130; easing.type: Easing.OutBack }
                    PauseAnimation { duration: 900 }
                }
            }
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 10
            Repeater {
                model: 3
                Rectangle {
                    required property int index
                    width: 10; height: 10; radius: 5
                    color: ["#ffd84d", "#ffb43a", "#ff9d2e"][index]
                    SequentialAnimation on opacity {
                        loops: Animation.Infinite
                        running: root.visible
                        PauseAnimation { duration: index * 160 }
                        NumberAnimation { from: 0.35; to: 1; duration: 260 }
                        NumberAnimation { from: 1; to: 0.35; duration: 260 }
                        PauseAnimation { duration: (2 - index) * 160 + 120 }
                    }
                }
            }
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Core.tr(Core.uiLang, "Loading…")
            color: "#cfc8e6"
            font.pixelSize: 15
            font.family: Theme.type.family
        }
    }

    ParallelAnimation {
        id: fade
        NumberAnimation { target: root; property: "opacity"; to: 0; duration: 320; easing.type: Easing.InQuad }
        NumberAnimation { target: col; property: "scale"; to: 1.08; duration: 320; easing.type: Easing.InQuad }
    }
}
