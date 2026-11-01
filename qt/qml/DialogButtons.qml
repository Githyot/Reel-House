import QtQuick
import Reelhouse 1.0

// The Cancel / main button pair at the bottom of a dialog, in the order and look of each platform:
// GNOME (AdwAlertDialog): both buttons share the width, Cancel left, the main one right;
// Windows (ContentDialog): on a footer band, the accent button left of Cancel, at the right;
// Apple: at the right, Cancel then the main button (the main one is always rightmost);
// Android (Material 3): text buttons at the right, Cancel then the main one.
Item {
    id: db
    property string cancelText: Core.tr(Core.uiLang, "Cancel")
    property string okText: Core.tr(Core.uiLang, "OK")
    property string okKind: "suggested"     // suggested | destructive
    property bool okEnabled: true
    property bool showCancel: true
    // Windows draws its buttons on a band of the dialog's own width: the dialog's side padding, to reach its edges.
    property real bleed: 0
    signal cancelled()
    signal accepted()

    readonly property string os: Theme.look
    readonly property bool stretch: os === "gnome"
    width: parent ? parent.width : 300
    // Windows: the band is 80 px high with the dialog's bottom padding counted in
    implicitHeight: os === "windows" ? Math.max(40, 80 - bleed) : (os === "android" ? 44 : 40)

    // Windows: the footer band (a step darker than the dialog) under the buttons.
    Rectangle {
        visible: db.os === "windows"
        x: -db.bleed; y: 0
        width: db.width + 2 * db.bleed; height: db.height + db.bleed
        // rounded like the dialog at the bottom, square at the top
        radius: Theme.shape.sheet
        color: Theme.c.layer
        Rectangle { width: parent.width; height: parent.radius; color: parent.color }
        Rectangle { width: parent.width; height: 1; color: Theme.c.divider }
    }

    Row {
        id: row
        spacing: db.os === "android" ? 8 : (db.os === "windows" ? 8 : 10)
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: db.os === "windows" ? db.bleed / 2 : 0
        anchors.right: db.stretch ? undefined : parent.right
        anchors.horizontalCenter: db.stretch ? parent.horizontalCenter : undefined
        // Windows puts the main button first; the others put Cancel first.
        layoutDirection: db.os === "windows" ? Qt.RightToLeft : Qt.LeftToRight
        readonly property real share: db.showCancel ? (db.width - spacing) / 2 : db.width
        FlatButton {
            visible: db.showCancel
            text: db.cancelText
            kind: db.os === "android" ? "text" : "plain"
            pill: db.os === "gnome"
            implicitHeight: db.os === "android" ? 40 : 36
            width: db.stretch ? row.share : Math.max(implicitWidth, db.os === "windows" ? 120 : 84)
            onClicked: db.cancelled()
        }
        FlatButton {
            text: db.okText
            kind: db.os === "android" ? (db.okKind === "destructive" ? "dangertext" : "text") : db.okKind
            enabled: db.okEnabled
            pill: db.os === "gnome"
            implicitHeight: db.os === "android" ? 40 : 36
            width: db.stretch ? row.share : Math.max(implicitWidth, db.os === "windows" ? 120 : 84)
            onClicked: db.accepted()
        }
    }
}
