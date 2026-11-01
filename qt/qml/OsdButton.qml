import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "Osd.js" as Osd

// A button drawn over the video, always light-on-dark (FlatButton follows the app's light/dark colours, which can put
// dark text on the dark player). Kinds:
//   osd     flat, for the control bar (`active` while its popover is open)
//   tonal   a filled neutral button (Android: the M3 filled tonal button)
//   accent  the theme accent ("suggested" works too)
//   round   a small round icon button (close, −, +); "circular" and "flatround" work too
Button {
    id: b
    property string glyph: ""
    property string kind: "osd"
    property bool active: false
    property int iconSize: 18
    property bool pill: false
    readonly property var o: Osd.t(Theme.layout.player)
    readonly property bool isAccent: kind === "accent" || kind === "suggested"
    readonly property bool isRound: kind === "round" || kind === "circular" || kind === "flatround"
    readonly property bool isTonal: kind === "tonal" || kind === "plain" || kind === "flat"
    readonly property color fg: isAccent ? Theme.c.accentFg : (active ? o.activeFg : (isTonal ? o.btnFg : o.fg))

    readonly property int target: Theme.mobile ? (Theme.mob !== undefined && Theme.mob.target !== undefined ? Theme.mob.target : 48) : 0
    implicitHeight: Math.max(isRound ? Math.min(target, 40) : target, isRound ? 32 : (kind === "osd" ? 40 : o.btnH))
    implicitWidth: isRound ? implicitHeight : Math.max(implicitHeight, contentItem.implicitWidth + (text !== "" ? o.btnPadX * 2 : 16))
    onPressAndHold: if (Theme.touch && b.ToolTip.text !== "") b.ToolTip.show(b.ToolTip.text, 2500)
    padding: 0
    hoverEnabled: true
    focusPolicy: Qt.TabFocus

    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight
        Row {
            id: row
            anchors.centerIn: parent
            spacing: 8
            Icon { visible: b.glyph !== ""; name: b.glyph; size: b.iconSize; color: b.fg; anchors.verticalCenter: parent.verticalCenter }
            Text {
                visible: b.text !== ""
                text: b.text
                color: b.fg
                font.pixelSize: 14
                font.weight: b.o.btnWeight
                font.family: Theme.type.family
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
    background: Rectangle {
        opacity: b.enabled ? 1 : 0.4
        radius: {
            if (b.isRound || b.pill) return height / 2
            if (b.kind === "osd") return b.o.ctlR >= 999 ? height / 2 : (Theme.layout.player === "fluent" ? 4 : height / 2)
            return Osd.r(b.o.ctlR, height)
        }
        border.width: (b.isTonal || b.isRound) && b.o.btnStroke !== "#00000000" ? 1 : 0
        border.color: b.o.btnStroke
        color: {
            if (b.isAccent) {
                // a state layer of the text colour over the accent (Material / Fluent hover and press)
                const k = b.down ? 0.16 : (b.hovered ? 0.08 : 0)
                return Qt.tint(Theme.c.accent, Qt.rgba(b.fg.r, b.fg.g, b.fg.b, k))
            }
            if (b.isTonal) return b.down ? b.o.btnPress : (b.hovered ? b.o.btnHover : b.o.btn)
            if (b.active) return b.down ? b.o.btnPress : b.o.active
            return b.down ? b.o.press : (b.hovered ? b.o.ghostHover : (b.isRound ? b.o.ghostIdle : "transparent"))
        }
        Behavior on color { ColorAnimation { duration: Theme.motion.fast || 120 } }
        // Windows: the accent pill under a bar button whose flyout is open
        Rectangle {
            visible: b.kind === "osd" && b.active && Theme.layout.player === "fluent"
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 2 }
            width: 16; height: 3; radius: 1.5
            color: Theme.c.accent
        }
        // keyboard / game-controller focus ring
        Rectangle {
            visible: b.visualFocus
            anchors { fill: parent; margins: -3 }
            radius: parent.radius + 3
            color: "transparent"
            border.width: 2
            border.color: Osd.readable(Theme.c.accent)
        }
    }
}
