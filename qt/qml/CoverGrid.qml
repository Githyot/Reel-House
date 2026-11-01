import QtQuick
import Reelhouse 1.0

// A wrapped, centred grid of covers (GTK FlowBox of cards).
Item {
    id: g
    property var model: []
    property bool showUnread: false
    property var libraryIds: ({})          // id -> true, for the "in library" tick
    signal opened(var item)
    signal menu(var item, var card, real x, real y)
    readonly property int cols: Math.max(1, Math.floor((width + 16) / 166))
    height: flow.height
    Flow {
        id: flow
        spacing: 16
        width: g.cols * 166 - 16
        x: Math.max(0, (g.width - width) / 2)
        // the number is the model, so the cards stay when only their data changes
        Repeater {
            model: g.model.length
            delegate: ShelfCard {
                id: cardItem
                required property int index
                readonly property var item: g.model[index] || ({})
                title: item.title || ""
                cover: item.cover || ""
                unread: g.showUnread ? (item.unread || 0) : -1
                inLibrary: !!item.inLibrary || !!g.libraryIds[item.id]
                onClicked: g.opened(item)
                onMenuRequested: function(x, y) { g.menu(item, cardItem, x, y) }
            }
        }
    }
}
