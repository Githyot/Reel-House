import QtQuick
import QtQuick.Controls
import QtWebView
import Reelhouse 1.0

// A site's bot check (Cloudflare "Just a moment…") in the Android System WebView: the person opens the site, passes the
// check, and "Done" hands its cookies and the browser's User-Agent to the core, which sends them with the text source's
// requests to that site (sitepass.rs). The bar sits above the page: the native page covers anything drawn over it.
Item {
    id: sc
    property string url: ""
    signal finished(bool ok)
    Rectangle { anchors.fill: parent; color: Theme.c.window }
    Rectangle {
        id: bar
        width: parent.width
        height: 56
        color: Theme.c.headerbar !== undefined ? Theme.c.headerbar : Theme.c.window
        Row {
            anchors.left: parent.left
            anchors.leftMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            FlatButton { kind: "plain"; text: Core.tr(Core.uiLang, "Cancel"); onClicked: sc.finished(false) }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: sc.width - 260
                elide: Text.ElideRight
                text: page.loading ? Core.tr(Core.uiLang, "Loading…") : Core.tr(Core.uiLang, "When the site opens normally, tap Done.")
                color: Theme.c.text
                font.pixelSize: 15
                font.family: Theme.type.family
            }
        }
        FlatButton {
            anchors.right: parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            kind: "suggested"
            text: Core.tr(Core.uiLang, "Done")
            onClicked: {
                const u = page.url.toString() || sc.url
                const cookie = Core.webCookies(u)
                Core.cmd({cmd: "set_site_pass", url: sc.url, cookie: cookie, ua: Core.webUserAgent()})
                sc.finished(cookie !== "")
            }
        }
    }
    WebView {
        id: page
        anchors.top: bar.bottom
        anchors.left: parent.left
        width: parent.width
        height: parent.height - bar.height
        url: sc.url
    }
}
