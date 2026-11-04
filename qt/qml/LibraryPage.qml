import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The library (GTK browse::rebuild_library): your titles as category cards. Covers, synopses and scores that a saved title
// lacks are looked up in the background when the page is shown.
Item {
    id: page
    readonly property var cards: Core.library.cards !== undefined ? Core.library.cards : []
    // the same gutter as the large title; on a wide screen more columns instead of very long synopsis lines
    readonly property real gutter: Theme.mobile ? 16 : Theme.spacing.page
    readonly property int gap: Theme.spacing.rowGap || 14
    readonly property int perLine: Theme.mobile ? 1 : Math.max(1, Math.min(5, Math.floor((width - 2 * gutter + gap) / (420 + gap))))
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
            x: page.gutter
            y: Theme.mobile ? 8 : 14
            width: scroll.width - 2 * page.gutter
            columns: page.perLine
            rowSpacing: page.gap
            columnSpacing: page.gap
            // the number is the model, so the cards stay (and keep their covers) while synopses and scores
            // arrive one by one in the background; a new array would rebuild every card each time
            Repeater {
                model: page.cards.length
                delegate: RichCard {
                    required property int index
                    readonly property var modelData: page.cards[index] || ({})
                    width: (grid.width - (page.perLine - 1) * page.gap) / page.perLine
                    card: modelData
                    onClicked: Core.cmd({cmd: "open_detail", item: modelData.item})
                }
            }
        }
    }
    // a phone: the list pulled down past its top looks the library up again
    PullToRefresh { target: scroll; onRefresh: Core.cmd({cmd: "fill_library"}) }
    // the Arara keeps the empty shelf company
    StatusPage {
        anchors.centerIn: parent
        visible: page.cards.length === 0
        mood: "think"
        title: Core.tr(Core.uiLang, "Your library is empty")
        description: Core.tr(Core.uiLang, "Open a title and choose “Add to library” to keep it here.")
    }
}
