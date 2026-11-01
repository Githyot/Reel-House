import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A small dialog that asks for one line of text (the title of a new book or chapter).
Popup {
    id: tp
    property string heading: ""
    property string actionText: ""
    signal accepted(string title)
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    padding: 24
    width: Math.min(420, (Overlay.overlay ? Overlay.overlay.width : 480) - 48)
    background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.popover; shadow: true }
    Overlay.modal: Rectangle { color: Theme.c.scrim }
    onOpened: field.forceActiveFocus()
    function submit() {
        if (field.text.trim() === "") return
        tp.accepted(field.text.trim())
        field.text = ""
        tp.close()
    }

    Column {
        width: parent.width
        spacing: 16
        DialogTitle { text: tp.heading }
        TextField {
            id: field
            width: parent.width
            height: 36
            color: Theme.c.text
            font.pixelSize: 15
            font.family: Theme.type.family
            selectByMouse: true
            background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: field.activeFocus ? 2 : 1; border.color: field.activeFocus ? Theme.c.accent : Theme.c.divider }
            onAccepted: tp.submit()
        }
        DialogButtons {
            bleed: 24
            okText: tp.actionText
            okEnabled: field.text.trim() !== ""
            onCancelled: tp.close()
            onAccepted: tp.submit()
        }
    }
}
