import QtQuick
import Reelhouse 1.0

// One row of a boxed list (adw::ActionRow): title and subtitle on the left, the controls on the right.
Item {
    id: r
    property string title: ""
    property string subtitle: ""
    property bool first: true
    property bool selectable: false
    property bool highlight: false        // GTK .account-on
    default property alias suffix: suffixRow.data
    width: parent ? parent.width : 400
    // a phone: Material 56 dp (one line) / 72 dp (two lines) rows, iOS 44 / 60 pt
    readonly property int rowMin: Theme.mobile ? (Theme.mob !== undefined && Theme.mob[subtitle !== "" ? "row2H" : "rowH"] !== undefined ? Theme.mob[subtitle !== "" ? "row2H" : "rowH"] : 56) : 54
    implicitHeight: Math.max(rowMin, texts.implicitHeight + 20, suffixRow.implicitHeight + 16)
    onVisibleChanged: if (parent && parent.parent && parent.parent.parent && parent.parent.parent.relink) parent.parent.parent.relink()
    Rectangle { visible: !r.first; width: parent.width; height: 1; color: Theme.c.divider }
    Rectangle { anchors.fill: parent; anchors.margins: 1; visible: r.highlight; radius: Theme.shape.control; color: Theme.c.accent20 }
    Column {
        id: texts
        anchors { left: parent.left; leftMargin: 14; right: suffixRow.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
        spacing: 2
        Text { width: parent.width; text: r.title; color: Theme.c.text; font.pixelSize: Theme.mobile ? 16 : 15; font.family: Theme.type.family; wrapMode: Text.Wrap }
        TextEdit {
            visible: r.subtitle !== ""
            width: parent.width
            text: r.subtitle
            readOnly: true
            selectByMouse: r.selectable
            color: Theme.c.textDim
            font.pixelSize: Theme.mobile ? 14 : 13
            font.family: Theme.type.family
            wrapMode: Text.Wrap
        }
    }
    Row {
        id: suffixRow
        anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
        spacing: 8
    }
}
