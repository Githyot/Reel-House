import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The on-screen keyboard of controller mode: ✕/A on a text box opens it (Main.qml). QWERTY with a shift, a page of
// accented letters (Portuguese, Spanish, French…) and one of numbers and symbols, space, backspace and enter. The
// D-pad moves between the keys (PadNavigator, kept inside this popup), ✕/A types, □/X erases, △/Y is a space,
// L1 shift, R1 the next page of keys, OPTIONS/Menu enter, ○/B closes. Qt Virtual Keyboard was not used: it takes
// over every text box (mouse and keyboard too) once loaded, while this one only opens for the controller.
Popup {
    id: kb
    property Item target: null
    property bool shift: false
    property int layer: 0           // 0 letters, 1 accents, 2 numbers and symbols
    readonly property var layers: [
        ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm,.-"],
        ["áàâãäåæ", "éèêëíìîï", "óòôõöúùûü", "çñßœ¿¡ªº"],
        ["1234567890", "!@#$%&*()_", "+=/\\?:;\"'", "<>[]{}~^`|"]
    ]
    parent: Overlay.overlay
    modal: false
    focus: true
    closePolicy: Popup.CloseOnEscape
    x: parent ? (parent.width - width) / 2 : 0
    y: parent ? parent.height - height - 24 : 0
    width: Math.min(760, (parent ? parent.width : 800) - 32)
    padding: 14
    background: Rectangle { radius: Theme.shape.sheet !== undefined ? Theme.shape.sheet : 14; color: Theme.c.popover; border.width: 1; border.color: Theme.c.border }
    // the hints of the controller while it is open (Main.qml reads the nearest `controllerHints`)
    readonly property var controllerHints: {
        const t = function(x) { return Core.tr(Core.uiLang, x) }
        return [{button: "A", text: t("Type a letter")}, {button: "X", text: t("Erase")}, {button: "Y", text: t("Space")},
                {button: "LB", text: "Shift"}, {button: "RB", text: t("Keys")}, {button: "Start", text: t("Enter")},
                {button: "B", text: t("Close")}]
    }

    function openFor(item) {
        target = item
        shift = false
        layer = 0
        open()
    }
    onOpened: Qt.callLater(function() { const k = keys.itemAt(1); if (k) k.focusFirst() })
    onClosed: if (target) target.forceActiveFocus()

    function type(ch) {
        if (!target) return
        const c = shift ? ch.toUpperCase() : ch
        if (target.selectedText !== undefined && target.selectedText !== "") target.remove(target.selectionStart, target.selectionEnd)
        target.insert(target.cursorPosition, c)
        if (shift) shift = false
    }
    function erase() {
        if (!target) return
        if (target.selectedText !== undefined && target.selectedText !== "") { target.remove(target.selectionStart, target.selectionEnd); return }
        const p = target.cursorPosition
        if (p > 0) target.remove(p - 1, p)
    }
    function submit() {
        const t = target
        close()
        if (t) { t.forceActiveFocus(); if (t.accepted) t.accepted(); else Core.sendKey(t.Window.window, Qt.Key_Return) }
    }
    // the buttons of the controller while the keyboard is open (Main.qml hands them here first); true when used
    function padButton(b) {
        switch (b) {
        case "X": erase(); return true
        case "Y": type(" "); return true
        case "LB": shift = !shift; return true
        case "RB": layer = (layer + 1) % layers.length; return true
        case "Start": submit(); return true
        case "B": close(); return true
        }
        return false
    }

    component Key: Button {
        id: key
        property string label: ""
        property real units: 1
        signal typed()
        focusPolicy: Qt.StrongFocus
        activeFocusOnTab: true
        implicitHeight: 44
        implicitWidth: 44 * units
        onClicked: typed()
        contentItem: Text {
            text: key.label
            color: key.activeFocus ? Theme.c.accentFg : Theme.c.text
            font.pixelSize: 17; font.weight: Font.DemiBold; font.family: Theme.type.family
            horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: 8
            color: key.activeFocus ? Theme.c.accent : (key.down ? Theme.c.fill : Theme.c.fill3)
            border.width: key.activeFocus ? 2 : 0
            border.color: Theme.c.accent
        }
    }

    contentItem: Column {
        spacing: 8
        // what is in the box now
        Rectangle {
            width: parent.width
            height: 40
            radius: 8
            color: Theme.c.fill3
            Text {
                anchors { fill: parent; leftMargin: 12; rightMargin: 12 }
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideLeft
                text: kb.target && kb.target.text !== undefined ? (kb.target.echoMode === TextInput.Password ? "•".repeat(kb.target.text.length) : kb.target.text) : ""
                color: Theme.c.text; font.pixelSize: 16; font.family: Theme.type.family
            }
        }
        Repeater {
            id: keys
            model: kb.layers[kb.layer]
            delegate: Row {
                id: row
                required property string modelData
                required property int index
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 6
                function focusFirst() { const k = rowKeys.itemAt(0); if (k) k.forceActiveFocus(Qt.TabFocusReason) }
                Repeater {
                    id: rowKeys
                    model: row.modelData.length
                    delegate: Key {
                        required property int index
                        label: kb.shift ? row.modelData.charAt(index).toUpperCase() : row.modelData.charAt(index)
                        onTyped: kb.type(row.modelData.charAt(index))
                    }
                }
            }
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 6
            Key { label: kb.shift ? "⇧ ON" : "⇧"; units: 1.6; onTyped: kb.shift = !kb.shift }
            Key { label: ["àé", "#+=", "abc"][kb.layer]; units: 1.6; onTyped: kb.layer = (kb.layer + 1) % kb.layers.length }
            Key { label: Core.tr(Core.uiLang, "Space"); units: 5; onTyped: kb.type(" ") }
            Key { label: "⌫"; units: 1.6; onTyped: kb.erase() }
            Key { label: "⏎"; units: 1.6; onTyped: kb.submit() }
        }
    }
}
