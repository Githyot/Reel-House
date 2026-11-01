import QtQuick
import Reelhouse 1.0

// The title bar of a dialog window, the same in every dialog of a theme: GNOME and Apple centre a bold title,
// Windows and Android put it on the left (Fluent semibold, Material "title large"). An optional back arrow on the
// left, extra buttons (`default` children) before the close button on the right.
Item {
    id: dh
    property string title: ""
    property bool backVisible: false
    property bool divider: false
    default property alias extra: extraRow.data
    signal back()
    signal close()

    readonly property string os: Theme.look
    readonly property bool centred: os === "gnome" || os === "apple"
    width: parent ? parent.width : 400
    height: os === "android" ? 64 : (os === "apple" ? 52 : 48)

    FlatButton {
        id: backBtn
        visible: dh.backVisible
        anchors { left: parent.left; leftMargin: 10; verticalCenter: parent.verticalCenter }
        kind: "flatround"; glyph: "left"
        onClicked: dh.back()
    }
    Text {
        x: dh.centred ? (parent.width - width) / 2 : (dh.backVisible ? backBtn.x + backBtn.width + 8 : 22)
        width: Math.min(implicitWidth, parent.width - 2 * Math.max(extraRow.width + closeBtn.width + 28, dh.backVisible ? 56 : 22))
        anchors.verticalCenter: parent.verticalCenter
        text: dh.title
        elide: Text.ElideRight
        color: Theme.c.text
        font.family: Theme.type.family
        font.pixelSize: dh.os === "android" ? 22 : (dh.os === "windows" ? 18 : 15)
        font.weight: dh.os === "android" ? Font.Normal : (dh.os === "windows" ? Font.DemiBold : Font.Bold)
    }
    Row {
        id: extraRow
        anchors { right: closeBtn.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
        spacing: 8
    }
    WinButton {
        id: closeBtn
        icon: "close"
        anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
        onClicked: dh.close()
    }
    Rectangle { visible: dh.divider; anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.c.divider }
}
