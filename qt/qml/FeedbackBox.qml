import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The feedback note: a small box to write to the author, which the core appends to Feedback.log in the data folder
// (with the time, the version and `place`, where the person was). It opens beside `near` (the floating button of the
// sidebar, the rail or the player bar through Main.openFeedback; the button in the X-Ray header), kept inside the window.
Popup {
    id: box
    property Item near: null
    property string place: ""
    parent: Overlay.overlay
    width: Math.min(380, (Overlay.overlay ? Overlay.overlay.width : 400) - 24)
    x: {
        if (!near || !Overlay.overlay) return 12
        const p = near.mapToItem(Overlay.overlay, 0, 0)
        return Math.max(12, Math.min(p.x + near.width - width, Overlay.overlay.width - width - 12))
    }
    y: {
        if (!near || !Overlay.overlay) return 12
        const p = near.mapToItem(Overlay.overlay, 0, 0)
        const above = p.y - height - 10
        return above > 12 ? above : Math.min(p.y + near.height + 10, Overlay.overlay.height - height - 12)
    }
    padding: 14
    modal: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: note.forceActiveFocus()
    background: Rectangle { radius: Theme.shape.sheet !== undefined ? Theme.shape.sheet : 14; color: Theme.c.popover; border.width: 1; border.color: Theme.c.border }
    contentItem: Column {
        spacing: 10
        Lbl { width: parent.width; text: Core.tr(Core.uiLang, "Feedback"); bold: true; size: 16 }
        Lbl { width: parent.width; text: Core.tr(Core.uiLang, "What did you like, what went wrong, what is missing? It is saved in Feedback.log for the author."); dim: true; size: 12; wrapMode: Text.Wrap }
        ScrollView {
            width: parent.width; height: 130
            TextArea {
                id: note
                wrapMode: TextEdit.Wrap
                color: Theme.c.text
                placeholderText: Core.tr(Core.uiLang, "Write here…")
                placeholderTextColor: Theme.c.textDim
                font.family: Theme.type.family
                font.pixelSize: 14
                selectByMouse: true
                background: Rectangle { radius: Theme.shape.control; color: Theme.c.fill3; border.width: note.activeFocus ? 2 : 0; border.color: Theme.c.accent }
                Keys.onPressed: function(e) { if ((e.key === Qt.Key_Return || e.key === Qt.Key_Enter) && (e.modifiers & Qt.ControlModifier)) { send.clicked(); e.accepted = true } }
            }
        }
        Row {
            spacing: 8
            layoutDirection: Qt.RightToLeft
            width: parent.width
            FlatButton {
                id: send
                kind: "suggested"; glyph: "check"; text: Core.tr(Core.uiLang, "Send")
                enabled: note.text.trim() !== ""
                ToolTip.visible: hovered; ToolTip.text: "Ctrl+Enter"
                onClicked: { Core.cmd({cmd: "feedback_add", text: note.text, place: box.place}); note.text = ""; box.close() }
            }
            FlatButton {
                kind: "flat"; glyph: "folder"; implicitWidth: 40
                ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Open Feedback.log")
                onClicked: Core.cmd({cmd: "feedback_open"})
            }
        }
    }
}
