import QtQuick
import Reelhouse 1.0

// The icon of a source (a site): the picture when there is one, else the first letter on a coloured square.
Item {
    id: si
    property string url: ""
    property string name: ""
    property int size: 40
    width: size
    height: size
    readonly property color tint: Qt.hsla(((name.length > 0 ? name.charCodeAt(0) : 0) * 37 % 360) / 360, 0.45, Theme.scheme === "light" ? 0.55 : 0.38, 1)
    Rectangle {
        anchors.fill: parent
        radius: si.size * 0.22
        color: si.tint
        Lbl { anchors.centerIn: parent; text: si.name.length > 0 ? si.name.charAt(0).toUpperCase() : "?"; color: "#ffffff"; bold: true; size: Math.round(si.size * 0.45) }
    }
    RoundedPicture { anchors.fill: parent; radius: si.size * 0.22; visible: si.url !== ""; url: si.url; placeholder: "transparent" }
}
