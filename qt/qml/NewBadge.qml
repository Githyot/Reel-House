import QtQuick
import Reelhouse 1.0

// The yellow NEW pill of a title with an episode not seen yet (GTK .new-badge).
Rectangle {
    radius: height / 2
    color: Theme.c.rating
    implicitWidth: nt.implicitWidth + 18
    implicitHeight: 20
    Text { id: nt; anchors.centerIn: parent; text: Core.tr(Core.uiLang, "NEW"); color: Theme.scheme === "light" ? "#ffffff" : "#1a1a1a"; font.pixelSize: 11; font.weight: Font.ExtraBold; font.family: Theme.type.family }
}
