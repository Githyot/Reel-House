import QtQuick
import Reelhouse 1.0

// libadwaita StatusPage: a big dim icon, a title and a line under it, centred.
Column {
    id: sp
    property string icon: "list"
    property string title: ""
    property string description: ""
    spacing: 10
    Icon { anchors.horizontalCenter: parent.horizontalCenter; name: sp.icon; size: 96; color: Theme.c.textDim; opacity: 0.45 }
    Lbl { anchors.horizontalCenter: parent.horizontalCenter; text: sp.title; bold: true; size: 24; horizontalAlignment: Text.AlignHCenter; width: Math.min(implicitWidth, 520); wrapMode: Text.Wrap }
    Lbl { anchors.horizontalCenter: parent.horizontalCenter; visible: sp.description !== ""; text: sp.description; dim: true; size: 15; horizontalAlignment: Text.AlignHCenter; width: Math.min(460, sp.parent ? sp.parent.width - 40 : 460); wrapMode: Text.Wrap }
}
