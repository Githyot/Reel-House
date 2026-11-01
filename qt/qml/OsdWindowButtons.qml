import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Reelhouse 1.0

// Window buttons drawn over the video (minimise, maximise / restore, close). The player is always dark, so the symbols
// stay light whatever the app theme. `traffic`: the Apple traffic lights (close, minimise, zoom, coloured, symbols on
// hover), as in the main window's sidebar; otherwise the theme's caption buttons (GNOME round, Windows 46x32, Android
// 40 px circles). `closesPlayer`: close (the red light on Apple) leaves the player instead of quitting the app.
Row {
    id: wbs
    property var window
    property bool traffic: false
    property bool closesPlayer: false
    spacing: traffic ? 8 : (fluent ? 0 : 6)
    readonly property bool fluent: Theme.layout.captionButtons === "fluent"
    readonly property bool m3: Theme.layout.captionButtons === "m3"
    readonly property bool maxed: !!wbs.window && wbs.window.visibility === Window.Maximized

    // The same calls as the window buttons of the app (AppHeader WinButton, SidebarNav), made once the click is fully
    // handled (Qt.callLater): the bar under these buttons has a DragHandler that moves the window, so the state change
    // does not run inside the press / release it is also watching.
    function act(a) {
        if (a === "close" && wbs.closesPlayer) {
            // back to the app's normal window, not a full-screen library
            if (wbs.window && wbs.window.visibility === Window.FullScreen) wbs.window.showNormal()
            Core.cmd({cmd: "close_player"})
            return
        }
        const w = wbs.window
        if (!w) { console.log("[player] window buttons: no window"); return }
        Qt.callLater(function() {
            if (a === "close") Qt.quit()
            else if (a === "min") w.showMinimized()
            else w.visibility === Window.Maximized ? w.showNormal() : w.showMaximized()
        })
    }

    HoverHandler { id: lightsHover; enabled: wbs.traffic }

    Repeater {
        model: wbs.traffic
            ? [{c: "#ff5f57", g: "close", a: "close"}, {c: "#febc2e", g: "minus", a: "min"}, {c: "#28c840", g: "expand", a: "max"}]
            : [{c: "", g: "min", a: "min"}, {c: "", g: wbs.maxed ? "restore" : "max", a: "max"}, {c: "", g: "close", a: "close"}]
        delegate: Rectangle {
            id: btn
            required property var modelData
            anchors.verticalCenter: parent ? parent.verticalCenter : undefined
            width: wbs.traffic ? 13 : (wbs.fluent ? 46 : (wbs.m3 ? 40 : 26))
            height: wbs.traffic ? 13 : (wbs.fluent ? 32 : (wbs.m3 ? 40 : 26))
            radius: wbs.traffic ? 6.5 : (wbs.fluent ? 0 : height / 2)
            color: wbs.traffic ? modelData.c
                : wbs.fluent ? (area.containsMouse ? (modelData.a === "close" ? "#c42b1c" : "#1fffffff") : "transparent")
                : wbs.m3 ? (area.containsMouse ? "#1fffffff" : "transparent")
                : (area.containsMouse ? "#33ffffff" : "#1fffffff")
            border.width: wbs.traffic ? 1 : 0
            border.color: "#26000000"
            Icon {
                anchors.centerIn: parent
                visible: !wbs.traffic || lightsHover.hovered
                name: btn.modelData.g
                size: wbs.traffic ? 9 : (wbs.fluent ? 12 : (wbs.m3 ? 18 : 14))
                color: wbs.traffic ? "#99000000" : "#ffffff"
            }
            MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; onClicked: wbs.act(btn.modelData.a) }
            ToolTip.visible: area.containsMouse && btn.modelData.a === "close" && wbs.closesPlayer
            ToolTip.delay: 700
            ToolTip.text: Core.tr(Core.uiLang, "Close the player")
        }
    }
}
