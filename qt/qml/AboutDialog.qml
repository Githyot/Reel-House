import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// About Reelhouse (GTK adw::AboutDialog).
Popup {
    id: ab
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Theme.mobile ? Math.min(380, (parent ? parent.width : 400) - 32) : 380
    modal: true
    focus: true
    padding: 24
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    Overlay.modal: Rectangle { color: Theme.c.scrim }
    background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
    contentItem: Column {
        spacing: 12
        Item {
            width: parent.width
            height: 30
            WinButton { anchors.right: parent.right; icon: "close"; onClicked: ab.close() }
        }
        // the app icon in the look of the theme (qt/icon/themes), with Arara the macaw waving beside it
        Item {
            width: parent.width
            height: 112
            Image {
                id: appIcon
                anchors.horizontalCenter: parent.horizontalCenter
                width: 112; height: 112
                source: "qrc:/icon/themes/reelhouse-" + (Theme.look || "gnome") + "-256.png"
                sourceSize: Qt.size(224, 224)
                smooth: true
                mipmap: true
            }
            Mascot {
                x: appIcon.x + appIcon.width - 26
                anchors.bottom: parent.bottom
                anchors.bottomMargin: -6
                size: 78
                mood: "wave"
                animate: ab.visible
            }
        }
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: Core.tr(Core.uiLang, "Reelhouse"); color: Theme.c.text; font.pixelSize: 26; font.weight: Font.ExtraBold; font.family: Theme.type.family }
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: Core.tr(Core.uiLang, "Reelhouse"); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            radius: 13
            color: Theme.c.accent20
            implicitWidth: vt.implicitWidth + 24
            implicitHeight: 26
            Text { id: vt; anchors.centerIn: parent; text: Core.tr(Core.uiLang, "0.3.0 (Qt)"); color: Theme.c.accentText; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family }
        }
        Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: Core.tr(Core.uiLang, "A media center with mpv built in. Addons, Real-Debrid, Anime4K, translated subtitles and Anime Skip."); color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family }
        Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: Core.tr(Core.uiLang, "Arara, our hyacinth macaw, keeps you company (and runs on energy drinks)."); color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
        PrefGroup {
            width: parent.width
            title: Core.tr(Core.uiLang, "Built on")
            PrefRow { title: Core.tr(Core.uiLang, "mpv") }
            PrefRow { title: Core.tr(Core.uiLang, "Anime4K by bloc97") }
            PrefRow { title: Core.tr(Core.uiLang, "thumbfast by po5") }
            PrefRow { title: "Lucide icons (ISC)" }
            PrefRow { title: Core.tr(Core.uiLang, "Anime Skip") }
            PrefRow { title: Core.tr(Core.uiLang, "rqbit") }
            PrefRow { title: Core.tr(Core.uiLang, "Qt") }
        }
        Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: Core.tr(Core.uiLang, "GNU General Public License, version 3"); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
    }
}
