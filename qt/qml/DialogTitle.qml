import QtQuick
import Reelhouse 1.0

// The heading of a small dialog, as each platform writes it: GNOME and Apple centred and bold, Windows on the left
// in semibold, Android (Material 3 "headline small") on the left, larger and regular.
Text {
    readonly property string os: Theme.look
    width: parent ? parent.width : 300
    wrapMode: Text.Wrap
    horizontalAlignment: os === "gnome" || os === "apple" ? Text.AlignHCenter : Text.AlignLeft
    color: Theme.c.text
    font.family: Theme.type.display || Theme.type.family
    font.pixelSize: os === "android" ? 24 : (os === "apple" ? 17 : 20)
    font.weight: os === "android" ? Font.Normal : (os === "windows" ? Font.DemiBold : Font.Bold)
}
