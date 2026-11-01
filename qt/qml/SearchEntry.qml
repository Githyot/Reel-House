import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A search field in the style of libadwaita (magnifier inside, rounded, focus ring).
TextField {
    id: f
    implicitHeight: 36
    leftPadding: 36
    rightPadding: 12
    color: Theme.c.text
    placeholderTextColor: Theme.c.textDim
    font.pixelSize: 15
    font.family: Theme.type.family
    selectByMouse: true
    // Android: no word suggestions in search fields. The keyboard's "composing" word lost its place in the field
    // (typing "pokemon" gave "okemonp"); with suggestions off each letter goes straight in.
    inputMethodHints: Qt.platform.os === "android" ? Qt.ImhNoPredictiveText : Qt.ImhNone
    verticalAlignment: TextInput.AlignVCenter
    background: Rectangle {
        radius: Theme.shape.medium
        color: Theme.c.chip
        border.width: f.activeFocus ? 2 : 0
        border.color: Theme.c.accent
    }
    Icon { name: "search"; size: 16; color: Theme.c.textDim; anchors { left: parent.left; leftMargin: 11; verticalCenter: parent.verticalCenter } }
}
