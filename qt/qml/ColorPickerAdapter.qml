import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "ColorMath.js" as CM

// The colour chooser of the app. It takes the look of the active theme: Libadwaita (gnome), Fluent (windows),
// Material 3 (android) or the round Apple wheel (apple). One colour state (hex, RGB, HSV, alpha) is shared by all of
// them. Used like QtQuick.Dialogs.ColorDialog: set `selectedColor`, call open(), read `selectedColor` in onAccepted.
Popup {
    id: cp
    property string currentTheme: Theme.look
    property string title: ""
    property bool withAlpha: false
    property color selectedColor: "#3584e4"
    signal accepted()

    // ---- the colour being chosen ----
    property real r: 53
    property real g: 132
    property real b: 228
    property real h: 212
    property real s: 0.77
    property real v: 0.89
    property real alpha: 1
    readonly property string hex: CM.rgbToHex(r, g, b)
    readonly property color current: Qt.rgba(r / 255, g / 255, b / 255, alpha)

    function setRgb(nr, ng, nb) {
        r = CM.clamp(nr, 0, 255); g = CM.clamp(ng, 0, 255); b = CM.clamp(nb, 0, 255)
        var q = CM.rgbToHsv(r, g, b, h)
        h = q.h; s = q.s; v = q.v
    }
    function setHsv(nh, ns, nv) {
        h = nh; s = CM.clamp(ns, 0, 1); v = CM.clamp(nv, 0, 1)
        var c = CM.hsvToRgb(h, s, v)
        r = c.r; g = c.g; b = c.b
    }
    function setHsl(nh, ns, nl) {
        var c = CM.hslToRgb(nh, CM.clamp(ns, 0, 1), CM.clamp(nl, 0, 1))
        h = nh
        setRgb(c.r, c.g, c.b)
    }
    function setHex(text) {
        var c = CM.hexToRgb(text)
        if (!c) return false
        setRgb(c.r, c.g, c.b)
        return true
    }
    function load(col) { setRgb(col.r * 255, col.g * 255, col.b * 255); alpha = withAlpha ? col.a : 1 }
    function pick(col) { selectedColor = col; open() }

    parent: Overlay.overlay
    anchors.centerIn: parent
    readonly property string kind: ["gnome", "windows", "android", "apple"].indexOf(currentTheme) >= 0 ? currentTheme : "gnome"
    width: kind === "apple" ? 316 : (kind === "windows" ? 352 : 372)
    modal: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    Overlay.modal: Rectangle { color: Theme.c.scrim }
    onAboutToShow: load(selectedColor)

    readonly property real corner: kind === "windows" ? 8 : (kind === "android" ? 28 : (kind === "apple" ? 20 : 12))
    background: Item {
        // Windows: acrylic (translucent layer with a light edge); the others use the material of the theme.
        Glass { visible: cp.kind !== "windows"; anchors.fill: parent; radius: cp.corner; level: "thick"; solid: cp.kind === "android" ? Theme.c.surface : Theme.c.window; shadow: true }
        Rectangle {
            visible: cp.kind === "windows"
            anchors.fill: parent
            radius: cp.corner
            color: Qt.alpha(Theme.c.popover, 0.94)
            border.width: 1
            border.color: Theme.c.border
            Rectangle { anchors { top: parent.top; left: parent.left; right: parent.right; margins: 1 } height: 1; color: "#14ffffff" }
        }
    }

    readonly property var pickers: ({gnome: gnomeC, windows: windowsC, android: androidC, apple: appleC})
    Component { id: gnomeC; GnomeColorPicker { st: cp } }
    Component { id: windowsC; WindowsColorPicker { st: cp } }
    Component { id: androidC; AndroidColorPicker { st: cp } }
    Component { id: appleC; AppleColorPicker { st: cp } }

    contentItem: Column {
        spacing: 16
        topPadding: 20
        bottomPadding: 20
        leftPadding: 20
        rightPadding: 20
        readonly property real inner: cp.width - 40

        Row {
            width: parent.inner
            Text {
                width: parent.width - 64
                anchors.verticalCenter: parent.verticalCenter
                text: cp.title !== "" ? cp.title : Core.tr(Core.uiLang, "Choose a colour")
                color: Theme.c.text
                font.pixelSize: cp.kind === "android" ? 22 : (cp.kind === "windows" ? 18 : 17)
                font.weight: cp.kind === "windows" ? Font.DemiBold : Font.Bold
                font.family: Theme.type.family
                elide: Text.ElideRight
            }
            // The colour before (left half) and the new one (right half).
            Rectangle {
                id: chip
                readonly property real rad: cp.kind === "apple" || cp.kind === "android" ? 15 : (cp.kind === "windows" ? 4 : 8)
                width: 64; height: 30; radius: rad
                color: cp.selectedColor
                border.width: 1; border.color: Theme.c.border
                Rectangle { anchors.right: parent.right; width: 42; height: parent.height; radius: chip.rad; color: cp.current; border.width: 1; border.color: Theme.c.border }
            }
        }

        // The chooser scrolls when the window is too short for it; the colour box and the buttons stay in view.
        Flickable {
            width: parent.inner
            height: Math.min(body.height, Math.max(160, (cp.parent ? cp.parent.height : 700) - 250))
            contentWidth: width
            contentHeight: body.height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            Loader {
                id: body
                width: parent.width
                sourceComponent: cp.pickers[cp.kind]
            }
        }

        Rectangle {
            width: parent.inner
            height: 40
            radius: cp.kind === "windows" ? 4 : (cp.kind === "android" ? 12 : (cp.kind === "apple" ? 10 : 8))
            color: cp.kind === "android" ? Theme.c.surface2 : Theme.c.fill
            border.width: hexBox.activeFocus ? 2 : 1
            border.color: hexBox.activeFocus ? Theme.c.accent : Theme.c.border
            Text { x: 12; anchors.verticalCenter: parent.verticalCenter; text: "#"; color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
            TextInput {
                id: hexBox
                anchors { left: parent.left; leftMargin: 28; right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                text: cp.hex.substring(1).toUpperCase()
                color: Theme.c.text; selectionColor: Theme.c.accent; font.pixelSize: 15; font.family: Theme.type.family
                maximumLength: 6
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhUppercaseOnly
                validator: RegularExpressionValidator { regularExpression: /[0-9a-fA-F]{0,6}/ }
                selectByMouse: true
                // Typing replaces the binding of `text`; it is put back so picking a swatch or a slider shows here again.
                function rebind() { text = Qt.binding(function() { return cp.hex.substring(1).toUpperCase() }) }
                onTextEdited: if (text.length === 6 && cp.setHex(text)) rebind()
                onEditingFinished: { cp.setHex(text); rebind() }
            }
        }

        // Cancel and Select in the order of the platform (DialogButtons.qml).
        DialogButtons {
            width: parent.inner
            bleed: 20
            okText: Core.tr(Core.uiLang, "Select")
            onCancelled: cp.close()
            onAccepted: { cp.selectedColor = cp.current; cp.close(); cp.accepted() }
        }
    }
}
