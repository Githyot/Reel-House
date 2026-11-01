import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The "Themes & Settings" panel of the readers, docked on the LEFT edge of the reader, under the header: the sides
// are for tools and the middle is the page, so the panel never sits over the text or the pictures. When the window
// has room the reader moves its content right by `inset` (and the panel stays open while the look is tried out);
// on a narrow window it lies over the content and a click beside it closes it. Drawn the way each theme asks:
//   apple   - a floating glass panel with 20 px corners, a little away from the edges, a round close button;
//   gnome   - a libadwaita sidebar: attached to the edge, the sidebar colour, a hair line on its right, centred title;
//   android - a Material 3 standard side sheet: the surface colour, 16 px corners on its outer (right) side;
//   windows - a Fluent navigation pane: attached, the pane colour, a thin stroke on its right, title at the start.
// `canBack` shows a back arrow (the panel's second page, "Customize" / "More options").
Item {
    id: sh
    property string title: ""
    property bool canBack: false
    signal back()
    signal closed()
    default property alias content: body.data
    property real sheetWidth: 400
    property bool opened: false

    readonly property string tid: Theme.look
    readonly property bool floating: tid === "apple"
    readonly property real gap: floating ? 12 : 0
    readonly property real cornerR: tid === "apple" ? 20 : (tid === "android" ? 16 : 0)
    readonly property real panelW: Math.max(0, Math.min(sheetWidth, width - 24))
    // docked: there is room for the content beside the panel, which then moves over instead of being covered
    readonly property bool docked: width - panelW - gap >= 560
    // 0 closed .. 1 open; the panel slides in from the left edge
    property real openFrac: opened ? 1 : 0
    Behavior on openFrac { NumberAnimation { duration: Theme.motion.normal; easing.type: Easing.OutCubic } }
    // how far the reader's content moves right (follows the slide)
    readonly property real inset: docked ? (gap + panelW) * openFrac : 0

    anchors.fill: parent
    z: 30
    visible: openFrac > 0
    onOpenFracChanged: if (openFrac === 0 && !opened) closed()

    function open() { opened = true }
    function close() { opened = false }
    // the "Aa" button of the header opens it and closes it again
    function openFrom(btn) { opened = !opened }

    // The own pieces are given to `children` by name: anything declared plainly would go to the default property
    // (`content`, the column inside the panel).
    children: [
    // a narrow window: the panel lies over the content, a click beside it closes it
    MouseArea {
        anchors.fill: parent
        enabled: sh.opened && !sh.docked
        visible: enabled
        onClicked: sh.close()
        onWheel: function(w) { w.accepted = true }
    },

    Item {
        id: panel
        x: sh.gap - (sh.panelW + sh.gap) * (1 - sh.openFrac)
        y: sh.gap
        width: sh.panelW
        height: sh.height - 2 * sh.gap
        opacity: sh.floating ? sh.openFrac : 1

        // nothing under the panel gets the clicks or the wheel
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onWheel: function(w) { w.accepted = true } }

        // Apple: the glass material of the theme
        Glass {
            visible: sh.tid === "apple"
            anchors.fill: parent
            radius: sh.cornerR
            level: "thick"
            shadow: true
        }
        // the others: a solid surface attached to the left edge; Material rounds only its outer (right) corners
        Rectangle {
            visible: sh.tid !== "apple"
            anchors.fill: parent
            radius: sh.cornerR
            color: sh.tid === "android" ? Theme.c.surface : (sh.tid === "gnome" ? Theme.c.sidebar : (Theme.c.pane || Theme.c.popover))
            Rectangle {
                visible: sh.cornerR > 0
                anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                width: sh.cornerR
                color: parent.color
            }
        }
        // GNOME and Fluent: the line between the panel and the page
        Rectangle {
            visible: sh.tid === "gnome" || sh.tid === "windows"
            anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
            width: 1
            color: sh.tid === "windows" ? Theme.c.controlStroke : Theme.c.border
        }

        // ---- the title bar of the panel ----
        Item {
            id: titleBar
            width: parent.width
            height: sh.tid === "android" ? 60 : 52
            FlatButton {
                visible: sh.canBack
                anchors { left: parent.left; leftMargin: 10; verticalCenter: parent.verticalCenter }
                kind: "flatround"
                glyph: "left"
                focusPolicy: Qt.NoFocus
                onClicked: sh.back()
            }
            Text {
                // GNOME and Apple centre the title (header bar, sheet); Fluent and Material put it at the start
                readonly property bool centred: sh.tid === "gnome" || sh.tid === "apple"
                x: centred ? (parent.width - width) / 2 : (sh.canBack ? 54 : 20)
                width: Math.min(implicitWidth, parent.width - 120)
                anchors.verticalCenter: parent.verticalCenter
                elide: Text.ElideRight
                text: sh.title
                color: Theme.c.text
                font.family: Theme.type.family
                font.pixelSize: sh.tid === "android" ? 22 : (sh.tid === "windows" ? 20 : 17)
                font.weight: sh.tid === "android" ? Font.Normal : (sh.tid === "windows" ? Font.DemiBold : Font.Bold)
            }
            // Apple: a grey round close button; the others: a flat close button
            Rectangle {
                visible: sh.tid === "apple"
                anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
                width: 30; height: 30; radius: 15
                color: closeArea.containsMouse ? Theme.c.buttonHover : Theme.c.button
                Icon { anchors.centerIn: parent; name: "close"; size: 16; color: Theme.c.textDim }
                MouseArea { id: closeArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sh.close() }
            }
            FlatButton {
                visible: sh.tid !== "apple"
                anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                kind: "flatround"
                glyph: "close"
                focusPolicy: Qt.NoFocus
                onClicked: sh.close()
            }
        }
        // ---- what the reader puts inside; scrolls when it is taller than the reader ----
        Flickable {
            id: scroller
            anchors { left: parent.left; right: parent.right; top: titleBar.bottom; bottom: parent.bottom }
            contentWidth: width
            contentHeight: body.implicitHeight + 24
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            interactive: contentHeight > height + 1
            ScrollBar.vertical: ScrollBar { policy: scroller.interactive ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
            Column {
                id: body
                x: 16
                y: 4
                width: parent.width - 32
                spacing: 14
            }
        }
    }
    ]
}
