import QtQuick
import Reelhouse 1.0

// A cover with its title under it: rounded cover, a green count in the corner, the title in one line.
Item {
    id: card
    property string title: ""
    property string cover: ""
    property int unread: -1             // number of chapters not read; -1 = no badge
    property bool inLibrary: false
    property int coverW: 150
    property int coverH: 215
    signal clicked()
    signal menuRequested(real x, real y)
    width: coverW
    height: coverH + 34

    Rectangle {
        anchors { fill: cov; margins: -3 }
        radius: Theme.shape.poster + 3
        color: "transparent"
        border.width: ma.containsMouse ? 3 : 0
        border.color: Theme.c.accent
        opacity: 0.55
    }
    RoundedPicture {
        id: cov
        width: card.coverW
        height: card.coverH
        url: card.cover
        title: card.title
        zoom: ma.containsMouse ? 1.12 : 1
    }
    // the title on the cover when there is no picture
    Lbl { visible: card.cover === ""; anchors.fill: cov; anchors.margins: 10; text: card.title; dim: true; wrapMode: Text.Wrap; elide: Text.ElideRight; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
    Rectangle {
        visible: card.unread > 0
        anchors { right: cov.right; top: cov.top; margins: 6 }
        height: 20
        width: Math.max(20, ub.implicitWidth + 12)
        radius: 10
        color: Theme.c.success
        Lbl { id: ub; anchors.centerIn: parent; text: card.unread; color: "#ffffff"; bold: true; size: 12 }
    }
    Rectangle {
        visible: card.inLibrary && card.unread === 0
        anchors { right: cov.right; top: cov.top; margins: 6 }
        width: 20; height: 20; radius: 10
        color: Theme.c.success
        Icon { anchors.centerIn: parent; name: "check"; size: 13; color: "#ffffff" }
    }
    Lbl {
        anchors { left: parent.left; right: parent.right; top: cov.bottom; topMargin: 8 }
        text: card.title
        bold: true
        size: 13
        elide: Text.ElideRight
    }
    MouseArea {
        id: ma
        anchors.fill: cov
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function(m) { if (m.button === Qt.RightButton) card.menuRequested(m.x, m.y); else card.clicked() }
        onPressAndHold: card.menuRequested(mouseX, mouseY)
    }
}
