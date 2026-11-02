import QtQuick
import QtQuick.Controls
import Reelhouse 1.0
import "Osd.js" as Osd

// A menu of the player (quality, audio, Anime4K, video scale), opened above its button, in the look of the theme
// (Osd.js): an Adwaita popover, a Fluent flyout, an M3 menu or an Apple glass panel.
// entries: [{text, checked, key, chevron}] plus {header: "Title"}, {sep: true} and {stepper: true, value, key, coarse} (− value +).
// The first header is the title of the menu.
Popup {
    id: pm
    property var entries: []
    property int menuWidth: 260
    signal picked(var entry)
    signal stepped(var entry, int dir)
    readonly property var o: Osd.t(Theme.layout.player)
    parent: Overlay.overlay
    padding: o.pad
    topPadding: o.pad + 2 + (sheet && Theme.mob !== undefined && Theme.mob.grabber === true ? 14 : 0)
    bottomPadding: o.pad + 2 + (sheet ? safeBottom + 6 : 0)
    modal: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    // Opens above `btn`, kept inside the window (the position follows the size, which is known only once filled).
    property real anchorX: 0
    property real anchorY: 0
    // a phone: a bottom sheet (full width, clear of the home indicator), its rows centred
    readonly property bool sheet: Theme.mobile === true
    readonly property real ovW: Overlay.overlay ? Overlay.overlay.width : 1000
    readonly property real ovH: Overlay.overlay ? Overlay.overlay.height : 800
    readonly property real safeBottom: Theme.safe !== undefined && Theme.safe.bottom !== undefined ? Theme.safe.bottom : 0
    width: sheet ? ovW : implicitWidth
    leftPadding: sheet ? Math.max(o.pad, (width - menuWidth) / 2) : o.pad
    rightPadding: sheet ? Math.max(o.pad, (width - menuWidth) / 2) : o.pad
    x: sheet ? 0 : Math.max(8, Math.min(anchorX, ovW - width - 8))
    y: sheet ? ovH - height : Math.max(8, anchorY - height - 10)
    function openAbove(btn) {
        const pt = btn.mapToItem(Overlay.overlay, 0, 0)
        anchorX = pt.x
        anchorY = Osd.above(btn, pt.y)
        open()
    }

    // A short rise and fade, with the theme's timing.
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.motion.fast || 120 }
        NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: Theme.motion.normal || 200; easing.type: Easing.OutCubic }
    }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Theme.motion.fast || 120 } }

    background: OsdPanel { bottomSheet: pm.sheet }
    contentItem: Flickable {
        implicitWidth: pm.menuWidth
        implicitHeight: Math.min(col.implicitHeight, pm.sheet ? pm.ovH * 0.6 : 520)
        contentHeight: col.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: col.implicitHeight > 520 ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
        Column {
            id: col
            width: pm.menuWidth
            spacing: pm.o.mark === "fill" ? 2 : 1
            Repeater {
                model: pm.entries
                delegate: Item {
                    id: row
                    required property var modelData
                    required property int index
                    readonly property bool isHeader: modelData.header !== undefined
                    readonly property bool isSep: modelData.sep === true
                    readonly property bool isStepper: modelData.stepper === true
                    width: col.width
                    height: isSep ? 9 : (isHeader ? head.implicitHeight : (isStepper ? 44 : opt.height))
                    Rectangle { visible: row.isSep; anchors.centerIn: parent; width: parent.width - pm.o.rowPad * 2; height: 1; color: pm.o.sep }
                    OsdHeader {
                        id: head
                        visible: row.isHeader
                        width: parent.width
                        title: row.index === 0
                        text: row.isHeader ? row.modelData.header : ""
                    }
                    OsdRow {
                        id: opt
                        visible: !row.isHeader && !row.isSep && !row.isStepper
                        width: parent.width
                        text: row.modelData.text !== undefined ? row.modelData.text : ""
                        checked: row.modelData.checked === true
                        chevron: row.modelData.chevron === true || row.modelData.key === "picker" || row.modelData.key === "upPrefs"
                        onActivated: { pm.picked(row.modelData); pm.close() }
                    }
                    Row {
                        visible: row.isStepper
                        anchors { left: parent.left; leftMargin: pm.o.rowPad - 4; verticalCenter: parent.verticalCenter }
                        spacing: 8
                        // {coarse: true}: also ± 1 s (dir ±10), for a dub seconds away
                        OsdButton { visible: row.modelData.coarse === true; kind: "tonal"; text: "−1 s"; implicitHeight: 32; anchors.verticalCenter: parent.verticalCenter; onClicked: pm.stepped(row.modelData, -10) }
                        OsdButton { kind: "round"; glyph: "minus"; anchors.verticalCenter: parent.verticalCenter; onClicked: pm.stepped(row.modelData, -1) }
                        Text {
                            width: row.modelData.coarse === true ? 64 : 80
                            horizontalAlignment: Text.AlignHCenter
                            anchors.verticalCenter: parent.verticalCenter
                            text: row.modelData.value !== undefined ? row.modelData.value : ""
                            color: pm.o.fg
                            font.pixelSize: 15
                            font.weight: 600
                            font.family: Theme.type.family
                        }
                        OsdButton { kind: "round"; glyph: "plus"; anchors.verticalCenter: parent.verticalCenter; onClicked: pm.stepped(row.modelData, 1) }
                        OsdButton { visible: row.modelData.coarse === true; kind: "tonal"; text: "+1 s"; implicitHeight: 32; anchors.verticalCenter: parent.verticalCenter; onClicked: pm.stepped(row.modelData, 10) }
                        OsdButton { kind: "tonal"; text: Core.tr(Core.uiLang, "Reset"); implicitHeight: 32; anchors.verticalCenter: parent.verticalCenter; onClicked: pm.stepped(row.modelData, 0) }
                    }
                }
            }
        }
    }
}
