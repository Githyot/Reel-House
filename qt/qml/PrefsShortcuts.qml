import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "Accel.js" as Accel

// Preferences > Shortcuts (GTK prefs::shortcuts_page): choose an action, then press the new key. Esc cancels.
Column {
    id: sc
    property var dlg
    readonly property var actions: dlg && dlg.ex.shortcuts !== undefined ? dlg.ex.shortcuts : []
    property string capturing: ""          // the action waiting for its key
    property string message: ""
    spacing: 26
    onVisibleChanged: if (!visible) capturing = ""

    // Catches the next key while an action waits for one.
    Item {
        id: catcher
        width: 1
        height: 0
        focus: sc.capturing !== ""
        Keys.onPressed: function(e) {
            if (sc.capturing === "") return
            e.accepted = true
            if (Accel.isModifier(e.key)) return          // only modifiers so far: keep waiting
            var id = sc.capturing
            sc.capturing = ""
            if (e.key === Qt.Key_Escape) return
            var name = Accel.fromEvent(e)
            if (name === "") return
            var r = Core.cmd({cmd: "set_shortcut", id: id, key: name})
            if (r.ok === false) Core.toast(r.error)
        }
        Keys.onShortcutOverride: function(e) { e.accepted = sc.capturing !== "" }
    }

    PrefGroup {
        title: Core.tr(Core.uiLang, "Player")
        description: Core.tr(Core.uiLang, "Choose an action, then press the new key. Esc cancels.")
        Repeater {
            model: sc.actions
            delegate: PrefRow {
                required property var modelData
                title: modelData.label
                FlatButton {
                    anchors.verticalCenter: parent.verticalCenter
                    pill: true
                    implicitWidth: 150
                    implicitHeight: 34
                    kind: sc.capturing === modelData.id ? "suggested" : "plain"
                    text: sc.capturing === modelData.id ? Core.tr(Core.uiLang, "Press a key…") : modelData.keyLabel
                    onClicked: { sc.capturing = modelData.id; catcher.forceActiveFocus() }
                }
                FlatButton {
                    anchors.verticalCenter: parent.verticalCenter
                    kind: "flatround"
                    glyph: "undo"
                    ToolTip.visible: hovered
                    ToolTip.text: Core.tr(Core.uiLang, "Back to the default key")
                    onClicked: Core.cmd({cmd: "set_shortcut", id: modelData.id, key: modelData.default})
                }
            }
        }
    }
    PrefGroup {
        PrefButton { title: Core.tr(Core.uiLang, "Restore all keys"); subtitle: Core.tr(Core.uiLang, "Go back to the default shortcuts"); label: Core.tr(Core.uiLang, "Restore"); onActivated: Core.cmd({cmd: "reset_shortcuts"}) }
    }
    PrefGroup {
        title: Core.tr(Core.uiLang, "Always available")
        Repeater {
            model: [["Ctrl + 0 to 6", "Anime4K off / modes 1 to 6"], ["Ctrl + F", "Search"], ["Ctrl + ,", "Preferences"], ["Ctrl + Q", "Quit"], ["Mouse back / forward", "Go back, or reopen the page you left"]]
            delegate: PrefRow {
                required property var modelData
                title: Core.tr(Core.uiLang, modelData[1])
                Text { anchors.verticalCenter: parent.verticalCenter; text: modelData[0] === "Mouse back / forward" ? Core.tr(Core.uiLang, modelData[0]) : modelData[0]; color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
            }
        }
    }
}
