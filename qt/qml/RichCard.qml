import QtQuick
import Reelhouse 1.0

// A title with its cover and the details next to it (GTK genre::rich_card): name, score, kind and year, categories, synopsis.
Item {
    id: rc
    property var card: ({})
    signal clicked()
    activeFocusOnTab: true
    Keys.onReturnPressed: rc.clicked()
    Keys.onEnterPressed: rc.clicked()
    Keys.onSpacePressed: rc.clicked()
    implicitHeight: Math.max(180, col.implicitHeight) + 16
    Rectangle { anchors.fill: parent; radius: Theme.shape.card || 12; color: area.containsMouse || rc.activeFocus ? Theme.c.cardHover : "transparent"; border.width: rc.activeFocus ? 3 : 0; border.color: Theme.c.accent }
    RoundedPicture {
        id: cover
        x: 8
        y: 8
        width: 120
        height: 180
        url: rc.card.image ? rc.card.image : ""
        Text {
            visible: !rc.card.image
            anchors.centerIn: parent
            width: parent.width - 16
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: rc.card.title ? rc.card.title : ""
            color: Theme.c.textDim
            font.pixelSize: 13
            font.family: Theme.type.family
        }
        NewBadge { visible: rc.card.isNew === true; anchors { left: parent.left; bottom: parent.bottom; margins: 6 } }
    }
    Column {
        id: col
        anchors { left: cover.right; leftMargin: 14; right: parent.right; rightMargin: 10; top: cover.top }
        spacing: 5
        Text { width: parent.width; text: rc.card.title ? rc.card.title : ""; color: Theme.c.text; font.pixelSize: 16; font.weight: Font.Bold; font.family: Theme.type.family; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight }
        Row {
            spacing: 8
            Text { visible: (rc.card.score || 0) > 0; text: "★ " + (rc.card.score || 0).toFixed(1); color: Theme.c.rating; font.pixelSize: 14; font.weight: Font.ExtraBold; font.family: Theme.type.family }
            Text {
                text: [rc.card.kind === "movie" ? Core.tr(Core.uiLang, "Movie") : (rc.card.kind === "series" ? Core.tr(Core.uiLang, "Series") : (rc.card.kind || "")), (rc.card.year || 0) > 0 ? "" + rc.card.year : ""].filter(function(x) { return x !== "" }).join(" · ")
                color: Theme.c.textDim
                font.pixelSize: 14
                font.family: Theme.type.family
            }
        }
        Row {
            spacing: 6
            visible: !!(rc.card.genres && rc.card.genres.length > 0)
            Repeater {
                model: rc.card.genres ? rc.card.genres.slice(0, 3) : []
                delegate: GenreChip { required property var modelData; text: modelData; clickable: false }
            }
        }
        Text { visible: text !== ""; width: parent.width; topPadding: 2; text: rc.card.synopsis ? rc.card.synopsis : ""; color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family; wrapMode: Text.Wrap; maximumLineCount: 6; elide: Text.ElideRight }
    }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: rc.clicked() }
}
