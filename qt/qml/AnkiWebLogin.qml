import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// AnkiWeb account of the Study mode: user name and password are sent once; AnkiWeb answers with a key that is kept.
// Used in Preferences (Accounts and keys) and in the Study panel.
Column {
    id: aw
    readonly property var a: Core.study.ankiweb !== undefined ? Core.study.ankiweb : ({})
    readonly property bool logged: a.logged === true
    function tr(s) { return Core.tr(Core.uiLang, s) }
    spacing: 18
    width: parent ? parent.width : 480

    PrefGroup {
        title: "AnkiWeb"
        description: aw.tr("Your Anki account. Pulling brings the decks, notes and cards of AnkiWeb here; cards already here take AnkiWeb's schedule. Reviews made here are not sent to AnkiWeb, and pictures and sounds of the cards come with .apkg packages only.")
        PrefEntry { id: user; visible: !aw.logged; title: aw.tr("Email"); text: aw.a.user || "" }
        PrefEntry { id: pass; visible: !aw.logged; first: false; title: aw.tr("Password"); password: true }
        PrefRow {
            first: aw.logged
            title: aw.logged ? aw.tr("Logged in as") + " " + (aw.a.user || "") : aw.tr("Account")
            subtitle: aw.a.status || (aw.logged ? "" : aw.tr("Not logged in"))
            FlatButton {
                visible: !aw.logged
                anchors.verticalCenter: parent.verticalCenter
                kind: "suggested"
                text: aw.a.busy === true ? aw.tr("Logging in…") : aw.tr("Log in")
                enabled: aw.a.busy !== true
                implicitHeight: 34
                onClicked: { Core.cmd({cmd: "ankiweb_login", user: user.value.trim(), pass: pass.value}); pass.clear() }
            }
            FlatButton {
                visible: aw.logged
                anchors.verticalCenter: parent.verticalCenter
                text: aw.tr("Log out")
                implicitHeight: 34
                onClicked: Core.cmd({cmd: "ankiweb_logout"})
            }
        }
        PrefButton {
            visible: aw.logged
            title: aw.tr("Pull from AnkiWeb")
            subtitle: aw.a.last > 0 ? aw.tr("Last pull") + ": " + new Date(aw.a.last * 1000).toLocaleString(Qt.locale(), "yyyy-MM-dd HH:mm") : aw.tr("Never")
            label: aw.a.busy === true ? aw.tr("Working…") : aw.tr("Pull")
            kind: "suggested"
            onActivated: if (aw.a.busy !== true) Core.cmd({cmd: "ankiweb_sync"})
        }
        PrefButton {
            title: aw.tr("No account yet?")
            subtitle: "ankiweb.net"
            label: aw.tr("Open")
            onActivated: Qt.openUrlExternally("https://ankiweb.net/account/signup")
        }
    }
}
