import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The slim line at the top of the decks: today's numbers over all decks on the left,
// the AnkiWeb account and the state of the last pull on the right (Log in when there is no account).
StudySurface {
    id: ap
    readonly property var a: Core.study.ankiweb !== undefined ? Core.study.ankiweb : ({})
    readonly property bool logged: a.logged === true
    readonly property bool compact: width < 560
    signal loginRequested()
    function tr(s) { return Core.tr(Core.uiLang, s) }
    implicitHeight: compact ? 92 : 56
    height: implicitHeight

    component Stat: Row {
        property string value: ""
        property string label: ""
        property color tint: Theme.c.text
        spacing: 6
        Text { anchors.baseline: lb.baseline; text: parent.value; color: parent.tint; font.pixelSize: 18; font.weight: Font.Bold; font.family: Theme.type.family }
        Text { id: lb; text: parent.label; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
    }

    Row {
        id: stats
        x: 18
        y: ap.compact ? 12 : (ap.height - height) / 2
        spacing: 20
        Stat { value: "" + (ap.a.due || 0); label: ap.tr("to study"); tint: Theme.c.accentText }
        Stat { value: "" + (ap.a.reviewedToday || 0); label: ap.tr("reviewed"); tint: Theme.c.success }
        Stat { visible: !ap.compact || (ap.a.againToday || 0) > 0; value: "" + (ap.a.againToday || 0); label: ap.tr("again"); tint: Theme.c.error }
    }

    Row {
        id: acct
        anchors { right: parent.right; rightMargin: 12 }
        y: ap.compact ? ap.height - height - 8 : (ap.height - height) / 2
        spacing: 8
        BusyIndicator { visible: ap.a.busy === true; running: visible; width: 22; height: 22; anchors.verticalCenter: parent.verticalCenter }
        Rectangle {
            visible: ap.a.busy !== true
            anchors.verticalCenter: parent.verticalCenter
            width: 8; height: 8; radius: 4
            color: ap.logged ? Theme.c.success : Theme.c.textDim
            opacity: ap.logged ? 1 : 0.6
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Math.max(80, ap.width - (ap.compact ? 0 : stats.width) - 140))
            elide: Text.ElideRight
            text: "AnkiWeb · " + (ap.a.status ? ap.a.status
                  : ap.logged ? (ap.a.last > 0 ? ap.tr("Last pull") + " " + new Date(ap.a.last * 1000).toLocaleString(Qt.locale(), "dd/MM HH:mm") : ap.tr("Not pulled yet"))
                  : ap.tr("Not logged in"))
            color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family
        }
        FlatButton {
            visible: !ap.logged
            anchors.verticalCenter: parent.verticalCenter
            kind: "flat"; text: ap.tr("Log in"); implicitHeight: 32
            onClicked: ap.loginRequested()
        }
    }
}
