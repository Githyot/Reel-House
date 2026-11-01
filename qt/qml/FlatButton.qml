import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// libadwaita-style buttons: flat (header), plain (grey), suggested (accent), pill.
Button {
    id: b
    property string glyph: ""
    property string kind: "plain"       // flat | plain | suggested | destructive | green | circular | text | dangertext (Material text buttons)
    property bool faded: false
    property bool active: false          // osd: its popover is open
    property bool pill: false            // fully round ends (GTK .pill)
    property int iconSize: 18
    // a row of a sidebar: icon and label at the left edge instead of centred
    property bool alignStart: false
    // On a phone every button is at least the platform's touch target (48 dp Material, 44 pt iOS)
    readonly property int target: Theme.mobile ? (Theme.mob !== undefined && Theme.mob.target !== undefined ? Theme.mob.target : 48) : 0
    implicitHeight: Math.max(target, kind === "osd" ? 40 : (kind === "circular" || kind === "flatround" ? 34 : 40))
    implicitWidth: Math.max(text === "" ? target : 0, kind === "osd" ? Math.max(40, contentItem.implicitWidth + 24) : (kind === "circular" || kind === "flatround" ? 34 : (contentItem.implicitWidth + 36)))
    // Touch: a long press shows what hovering would (the attached ToolTip of the button), and does not click.
    onPressAndHold: if (Theme.touch && b.ToolTip.text !== "") b.ToolTip.show(b.ToolTip.text, 2500)
    padding: 0
    hoverEnabled: true
    // Text and icon colour: on the accent, error and green fills the theme's own "on" colour (Material dark has dark text).
    readonly property color fg: kind === "osd" ? "#ffffff" : kind === "suggested" ? Theme.c.accentFg : kind === "destructive" ? Theme.c.errorFg : kind === "green" ? Theme.c.greenFg
                              : kind === "text" ? Theme.c.accent : kind === "dangertext" ? Theme.c.error : Theme.c.text
    readonly property bool filled: kind === "suggested" || kind === "destructive" || kind === "green"
    // State layer over a filled button (Material/Fluent: a few % of the text colour on hover, more when pressed).
    function stateLayer(base) { return b.down ? Qt.tint(base, Qt.rgba(b.fg.r, b.fg.g, b.fg.b, 0.16)) : (b.hovered ? Qt.tint(base, Qt.rgba(b.fg.r, b.fg.g, b.fg.b, 0.08)) : base) }
    contentItem: Item {
        // from the natural width of the label (not its shown width), so a button given less room elides its label
        // instead of running into its neighbours, without a binding loop
        implicitWidth: (b.glyph !== "" ? b.iconSize : 0) + (b.glyph !== "" && b.text !== "" ? 8 : 0) + (b.text !== "" ? btnLabel.implicitWidth : 0)
        implicitHeight: contentRow.implicitHeight
        Row {
            id: contentRow
            anchors.horizontalCenter: b.alignStart ? undefined : parent.horizontalCenter
            anchors.left: b.alignStart ? parent.left : undefined
            anchors.leftMargin: b.alignStart ? 12 : 0
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Icon { visible: b.glyph !== ""; name: b.glyph; size: b.iconSize; color: b.fg; anchors.verticalCenter: parent.verticalCenter }
            Text {
                id: btnLabel
                visible: b.text !== ""
                width: Math.max(0, Math.min(implicitWidth, b.width - (b.alignStart ? 24 : 16) - (b.glyph !== "" ? b.iconSize + 8 : 0)))
                elide: Text.ElideRight
                text: b.text
                color: b.fg
                opacity: b.faded ? 0.6 : 1
                font.pixelSize: 15
                font.weight: Theme.type.buttonWeight !== undefined ? Theme.type.buttonWeight : Font.Bold
                font.letterSpacing: Theme.type.buttonLetterSpacing || 0
                font.family: Theme.type.family
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
    background: Rectangle {
        // Apple and Material: every button is a capsule; GNOME and Windows: rounded rectangles, round only where it says so
        radius: (b.kind === "osd" && Theme.layout.player === "fluent") ? Theme.shape.small : b.kind === "circular" || b.kind === "flatround" || b.kind === "osd" || Theme.layout.buttons === "capsule" ? height / 2 : (b.pill ? height / 2 : Theme.shape.control)
        opacity: b.enabled ? 1 : 0.5
        // Fluent: a hairline control stroke around standard and accent buttons.
        border.width: Theme.layout.buttons === "fluent" && b.kind !== "flat" && b.kind !== "flatround" && b.kind !== "osd" && b.kind !== "text" && b.kind !== "dangertext" ? 1 : 0
        border.color: Theme.c.controlStroke !== undefined ? Theme.c.controlStroke : "transparent"
        color: {
            if (b.kind === "osd") return b.down ? "#40ffffff" : (b.active ? "#33ffffff" : (b.hovered ? "#26ffffff" : "transparent"))
            if (b.kind === "flatround") return b.down ? Theme.c.selected : (b.checked ? Theme.c.button : (b.hovered ? Theme.c.hover : "transparent"))
            if (b.kind === "flat") return b.down ? Theme.c.selected : (b.hovered ? Theme.c.hover : "transparent")
            if (b.kind === "text" || b.kind === "dangertext") return b.down ? Qt.rgba(b.fg.r, b.fg.g, b.fg.b, 0.16) : (b.hovered ? Qt.rgba(b.fg.r, b.fg.g, b.fg.b, 0.08) : "transparent")
            if (b.kind === "suggested") return b.faded ? Theme.c.accentFaded : b.stateLayer(Theme.c.accent)
            if (b.kind === "green") return b.stateLayer(Theme.c.greenBtn)
            if (b.kind === "destructive") return b.stateLayer(Theme.c.errorBg)
            return b.down ? Qt.tint(Theme.c.buttonHover, Theme.c.hover) : (b.hovered ? Theme.c.buttonHover : Theme.c.button)
        }
        Behavior on color { ColorAnimation { duration: Theme.motion.fast } }
        // Keyboard / game-controller focus ring.
        Rectangle {
            visible: b.visualFocus
            anchors.fill: parent
            anchors.margins: -Theme.shape.focusRing
            radius: parent.radius + Theme.shape.focusRing
            color: "transparent"
            border.width: Theme.shape.focusRing
            border.color: Theme.c.accent50 !== undefined ? Theme.c.accent50 : Theme.c.accent
        }
    }
}
