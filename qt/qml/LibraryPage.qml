import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The library (GTK browse::rebuild_library): your titles as category cards. Covers, synopses and scores that a saved title
// lacks are looked up in the background when the page is shown.
Item {
    id: page
    readonly property var cards: Core.library.cards !== undefined ? Core.library.cards : []
    readonly property int perLine: Theme.mobile ? 1 : Math.max(1, Math.min(3, Math.floor((width - 36) / 420)))
    onVisibleChanged: if (visible) Core.cmd({cmd: "fill_library"})

    Flickable {
        id: scroll
        visible: page.cards.length > 0
        anchors.fill: parent
        contentWidth: width
        contentHeight: grid.implicitHeight + 40
        clip: true
        boundsBehavior: Theme.mobile ? Flickable.DragOverBounds : Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }
        Grid {
            id: grid
            x: Theme.mobile ? 16 : 18
            y: 14
            width: scroll.width - (Theme.mobile ? 32 : 36)
            columns: page.perLine
            rowSpacing: 14
            columnSpacing: 14
            // the number is the model, so the cards stay (and keep their covers) while synopses and scores
            // arrive one by one in the background; a new array would rebuild every card each time
            Repeater {
                model: page.cards.length
                delegate: RichCard {
                    required property int index
                    readonly property var modelData: page.cards[index] || ({})
                    width: (grid.width - (page.perLine - 1) * 14) / page.perLine
                    card: modelData
                    onClicked: Core.cmd({cmd: "open_detail", item: modelData.item})
                }
            }
        }
    }
    // a phone: the list pulled down past its top looks the library up again
    PullToRefresh { target: scroll; onRefresh: Core.cmd({cmd: "fill_library"}) }
    Column {
        anchors.centerIn: parent
        visible: page.cards.length === 0
        spacing: 8
        // the macaw keeps the empty shelf company
        Mascot { anchors.horizontalCenter: parent.horizontalCenter; size: 120; mood: "think"; animate: parent.visible }
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: Core.tr(Core.uiLang, "Your library is empty"); color: Theme.c.text; font.pixelSize: 22; font.weight: Font.ExtraBold; font.family: Theme.type.family }
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: Core.tr(Core.uiLang, "Open a title and choose “Add to library” to keep it here."); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
    }
}
