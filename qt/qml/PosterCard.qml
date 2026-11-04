import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

Item {
    id: card
    property var item: ({})
    property int pw: Theme.spacing.posterW
    property int ph: Theme.spacing.posterH
    signal clicked()
    // controller / keyboard: the cover itself takes the focus (ring and lift), ✕/A or Enter opens it
    activeFocusOnTab: true
    Keys.onReturnPressed: card.clicked()
    Keys.onEnterPressed: card.clicked()
    Keys.onSpacePressed: card.clicked()
    width: pw
    height: ph + 40
    // the focused card is drawn above its neighbours, so its ring is not covered
    z: activeFocus ? 1 : 0

    RoundedPicture {
        id: pic
        width: card.pw
        height: card.ph
        url: card.item && card.item.poster ? card.item.poster : ""
        title: card.item && card.item.name ? card.item.name : ""
        zoom: area.containsMouse ? 1.12 : (area.pressed ? 1.06 : 1)
        // Apple: the card lifts a little under the pointer instead of getting an accent ring
        scale: card.activeFocus ? 1.06 : (Theme.layout.posterStyle === "soft" && area.containsMouse ? 1.035 : 1)
        Behavior on scale { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
    }
    // controller / keyboard focus: a soft glow around the cover
    Rectangle {
        visible: card.activeFocus
        x: -6; y: -6
        width: card.pw + 12
        height: card.ph + 12
        scale: pic.scale
        radius: Theme.shape.poster + 6
        color: "transparent"
        border.width: 6
        border.color: Qt.alpha(Theme.c.accent, 0.3)
    }
    // No picture: the name on the cover (GTK cover_titled).
    Text {
        visible: !(card.item && card.item.poster)
        x: 10
        width: card.pw - 20
        height: card.ph
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: card.item && card.item.name ? card.item.name : ""
        color: Theme.c.textDim
        font.pixelSize: 15
        font.weight: Font.Bold
        font.family: Theme.type.family
    }
    NewBadge { visible: !!(card.item && card.item.isNew === true); x: 6; y: card.ph - height - 6 }
    Rectangle {
        width: card.pw
        height: card.ph
        radius: Theme.shape.poster
        color: "transparent"
        scale: pic.scale
        border.width: card.activeFocus ? 4 : (area.containsMouse && Theme.layout.posterStyle !== "soft" ? 3 : 0)
        border.color: Theme.c.accent
    }
    Rectangle {
        visible: !!(card.item && card.item.watched === true)
        anchors { left: pic.left; top: pic.top; margins: 8 }
        radius: 6
        color: Theme.c.rating
        width: 58
        height: 20
        Text { anchors.centerIn: parent; text: Core.tr(Core.uiLang, "WATCHED"); color: "#1a1a1a"; font.pixelSize: 10; font.weight: Font.ExtraBold; font.family: Theme.type.family }
    }
    Text {
        anchors { left: parent.left; right: parent.right; top: pic.bottom; topMargin: 10 }
        text: card.item && card.item.name ? card.item.name : ""
        color: Theme.c.text
        font.pixelSize: 15
        font.weight: Font.Bold
        font.family: Theme.type.family
        elide: Text.ElideRight
    }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: !Theme.touch; cursorShape: Qt.PointingHandCursor; onClicked: card.clicked(); onPressAndHold: if (Theme.touch && card.item && card.item.name) card.ToolTip.show(card.item.name, 2000) }
}
