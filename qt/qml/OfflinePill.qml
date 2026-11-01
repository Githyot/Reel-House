import QtQuick
import Reelhouse 1.0

// The offline state of the app (src/connectivity.rs): a small pill at the top while there is no internet or the
// user chose "Offline mode". Downloaded chapters, novels and videos still open; pages read before come from disk.
// Pressed: turns the chosen offline mode off, or asks for a new check of the connection.
Rectangle {
    id: pill
    readonly property bool forced: Core.net.forced === true
    readonly property bool shown: Core.net.online === false
    readonly property bool narrow: (parent ? parent.width : 800) < 560
    function tr(t) { return Core.tr(Core.uiLang, t) }

    visible: opacity > 0
    opacity: shown ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: 180 } }
    implicitWidth: row.implicitWidth + 28
    implicitHeight: Theme.mobile ? 36 : 32
    radius: Theme.shape.pill
    color: Theme.c.popover
    border.width: 1
    border.color: forced ? Theme.c.border : Theme.c.warning

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 8
        Icon { name: "wifioff"; size: 16; color: pill.forced ? Theme.c.textDim : Theme.c.warning; anchors.verticalCenter: parent.verticalCenter }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: pill.forced ? pill.tr("Offline mode") : pill.tr("No connection")
            color: Theme.c.text
            font.pixelSize: 13
            font.weight: Font.DemiBold
            font.family: Theme.type.family
        }
        Text {
            visible: !pill.narrow
            anchors.verticalCenter: parent.verticalCenter
            text: pill.tr("showing what is downloaded")
            color: Theme.c.textDim
            font.pixelSize: 13
            font.family: Theme.type.family
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: pill.forced ? pill.tr("Turn off") : ""
            visible: text !== ""
            color: Theme.c.accentText
            font.pixelSize: 13
            font.weight: Font.DemiBold
            font.family: Theme.type.family
        }
    }
    MouseArea {
        anchors.fill: parent
        enabled: pill.shown
        cursorShape: pill.forced ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (pill.forced) Core.cmd({cmd: "offline_mode", on: false })
    }
}
