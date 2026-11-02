import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A text field row (adw::EntryRow / PasswordEntryRow): the title small above the text. `apply` shows a ✓ button
// (changes are then sent only when it is pressed or Enter is hit); otherwise every change is sent.
Item {
    id: e
    property string title: ""
    property string text: ""
    property bool password: false
    property bool apply: false
    property bool first: true
    // Grey hint shown while the field is empty.
    property string placeholder: ""
    // What is in the field now (`text` is only the value given from outside; typing does not change it).
    readonly property alias value: field.text
    signal edited(string text)
    signal applied(string text)
    width: parent ? parent.width : 400
    implicitHeight: 60
    onTextChanged: if (field.text !== text) field.text = text
    Rectangle { visible: !e.first; width: parent.width; height: 1; color: Theme.c.divider }
    Text { x: 14; y: 8; text: e.title; color: field.activeFocus ? Theme.c.accentText : Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
    TextField {
        id: field
        x: 6
        y: 22
        width: parent.width - 12 - (applyBtn.visible ? 44 : 0) - (eye.visible ? 40 : 0)
        height: 34
        text: e.text
        placeholderText: e.placeholder
        placeholderTextColor: Theme.c.textDim
        echoMode: e.password && !eye.shown ? TextInput.Password : TextInput.Normal
        color: Theme.c.text
        font.pixelSize: 15
        font.family: Theme.type.family
        selectByMouse: true
        background: Item {}
        onTextEdited: if (!e.apply) e.edited(text)
        onAccepted: if (e.apply && text.trim() !== "") e.applied(text)
    }
    FlatButton {
        id: eye
        property bool shown: false
        visible: e.password
        anchors { right: applyBtn.visible ? applyBtn.left : parent.right; rightMargin: 6; verticalCenter: parent.verticalCenter }
        kind: "flat"
        glyph: shown ? "eyeoff" : "eye"
        implicitWidth: 34
        implicitHeight: 34
        ToolTip.visible: hovered
        ToolTip.text: shown ? Core.tr(Core.uiLang, "Hide") : Core.tr(Core.uiLang, "Show")
        onClicked: shown = !shown
    }
    FlatButton {
        id: applyBtn
        visible: e.apply && field.text.trim() !== ""
        anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
        kind: "suggested"
        glyph: "check"
        implicitWidth: 34
        implicitHeight: 34
        background: Rectangle { radius: 17; color: Theme.c.accent }
        onClicked: e.applied(field.text)
    }
    function clear() { field.text = "" }
}
