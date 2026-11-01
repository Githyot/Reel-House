import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A horizontal row of covers ("Similar anime", "Continue watching", the rows of Discover…) that a mouse can move:
// a visible scroll bar in the theme's colours under the covers, the vertical wheel scrolls it sideways, and the row
// can be dragged with the pointer. At either end the wheel goes on to the page, so the page still scrolls.
// A focused card grows (1.06) and gets a glow ring, so the row keeps `focusRoom` pixels free around the cards (left
// and right as list margins, above by moving the content down): the clip of the list no longer cuts the ring. The
// rows that use it sit `focusRoom` further left, are that much wider and taller, and close the gap above
// (PosterRow, ContinueRow, SimilarRow), so the covers stay where they were.
ListView {
    id: hl
    orientation: ListView.Horizontal
    spacing: 14
    clip: true
    property int focusRoom: 16
    leftMargin: focusRoom
    rightMargin: focusRoom
    // a horizontal list ignores topMargin and places its cards at y 0: the content is moved down instead
    Translate { id: lift; y: hl.focusRoom }
    Component.onCompleted: contentItem.transform = [lift]
    reuseItems: true
    boundsBehavior: Flickable.StopAtBounds
    flickableDirection: Flickable.HorizontalFlick
    // room under the captions of the covers for the bar
    readonly property int barRoom: canScroll ? 14 : 0
    readonly property bool canScroll: contentWidth + leftMargin + rightMargin > width + 1
    readonly property real minX: originX - leftMargin
    readonly property real maxX: Math.max(minX, originX + contentWidth + rightMargin - width)

    // the wheel: a notch moves the row by about one cover, smoothly; trackpads move it by their own pixels
    NumberAnimation { id: glide; target: hl; property: "contentX"; duration: 160; easing.type: Easing.OutCubic }
    MouseArea {
        parent: hl
        anchors.fill: parent
        z: 2
        acceptedButtons: Qt.NoButton
        onWheel: function(w) {
            const dx = w.pixelDelta.x !== 0 || w.pixelDelta.y !== 0 ? (Math.abs(w.pixelDelta.x) > Math.abs(w.pixelDelta.y) ? w.pixelDelta.x : w.pixelDelta.y)
                                                                    : (Math.abs(w.angleDelta.x) > Math.abs(w.angleDelta.y) ? w.angleDelta.x : w.angleDelta.y) * 1.6
            const from = glide.running ? glide.to : hl.contentX
            const to = Math.max(hl.minX, Math.min(hl.maxX, from - dx))
            // nothing left to scroll this way: the page gets the wheel
            if (!hl.canScroll || Math.abs(to - from) < 0.5) { w.accepted = false; return }
            glide.stop()
            if (w.pixelDelta.x !== 0 || w.pixelDelta.y !== 0) hl.contentX = to
            else { glide.from = hl.contentX; glide.to = to; glide.start() }
            w.accepted = true
        }
    }

    ScrollBar.horizontal: ScrollBar {
        id: bar
        policy: hl.canScroll ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
        height: hovered || pressed ? 10 : 7
        padding: 1
        Behavior on height { NumberAnimation { duration: Theme.motion.fast } }
        contentItem: Rectangle {
            implicitHeight: 5
            radius: height / 2
            color: bar.pressed ? Theme.c.accent : Theme.c.textDim
            opacity: bar.pressed ? 1 : (bar.hovered ? 0.8 : 0.5)
            Behavior on opacity { NumberAnimation { duration: Theme.motion.fast } }
        }
        background: Rectangle { radius: height / 2; color: Theme.c.fill3; opacity: 0.6 }
    }
}
