import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The header shared by the pages of the Study mode: an optional back button, a title with a line under it,
// and the actions of the page on the right (children go there).
Item {
    id: sh
    property string title: ""
    property string subtitle: ""
    property bool backButton: true
    default property alias actions: act.data
    signal back()
    function tr(s) { return Core.tr(Core.uiLang, s) }
    width: parent ? parent.width : 600
    implicitHeight: Math.max(52, txt.implicitHeight + 12)

    FlatButton {
        id: bk
        visible: sh.backButton
        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
        kind: Theme.look === "android" || Theme.look === "apple" ? "flatround" : "flat"
        glyph: "left"; iconSize: 22
        implicitWidth: 40; implicitHeight: 40
        ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: sh.tr("Back") + " (Esc)"
        onClicked: sh.back()
    }
    Column {
        id: txt
        anchors { left: bk.visible ? bk.right : parent.left; leftMargin: bk.visible ? 8 : 0; right: act.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
        spacing: 1
        Text {
            visible: text !== ""
            width: parent.width
            elide: Text.ElideRight
            text: sh.title
            color: Theme.c.text
            font.pixelSize: 20
            font.weight: Font.Bold
            font.family: Theme.type.family
        }
        Text {
            visible: text !== ""
            width: parent.width
            elide: Text.ElideRight
            text: sh.subtitle
            color: Theme.c.textDim
            // a phone: the line wraps instead of being cut by the buttons
            wrapMode: Theme.mobile ? Text.Wrap : Text.NoWrap
            maximumLineCount: 2
            font.pixelSize: Theme.mobile ? 14 : 13
            font.family: Theme.type.family
        }
    }
    Row {
        id: act
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        spacing: 6
    }
}
