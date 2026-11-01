import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A page with the titles of one category (GTK genre.rs): its name, cards with cover and details, "Load more".
Item {
    id: page
    readonly property var g: Core.genre
    readonly property var items: g.items !== undefined ? g.items : []
    readonly property int perLine: Math.max(1, Math.min(3, Math.floor((width - 36) / 420)))

    Flickable {
        id: scroll
        anchors.fill: parent
        contentWidth: width
        contentHeight: col.implicitHeight + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }
        Column {
            id: col
            x: 18
            y: 8
            width: scroll.width - 36
            spacing: 10
            Text { text: page.g.name ? page.g.name : ""; color: Theme.c.text; font.pixelSize: 34; font.weight: Font.ExtraBold; font.family: Theme.type.family }
            Text {
                visible: !!page.g.error || (page.items.length === 0 && page.g.loading !== true)
                width: parent.width
                wrapMode: Text.Wrap
                text: page.g.error ? page.g.error : Core.tr(Core.uiLang, "Nothing found.")
                color: Theme.c.textDim
                font.pixelSize: 15
                font.family: Theme.type.family
            }
            Grid {
                id: grid
                width: parent.width
                columns: page.perLine
                rowSpacing: 14
                columnSpacing: 14
                Repeater {
                    model: page.items
                    delegate: RichCard {
                        required property var modelData
                        width: (grid.width - (page.perLine - 1) * 14) / page.perLine
                        card: modelData
                        onClicked: Core.cmd({cmd: "open_card", card: modelData})
                    }
                }
            }
            BusyIndicator { anchors.horizontalCenter: parent.horizontalCenter; running: page.g.loading === true; visible: running }
            FlatButton {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.g.more === true && page.g.loading !== true
                text: Core.tr(Core.uiLang, "Load more")
                pill: true
                onClicked: Core.cmd({cmd: "genre_more"})
            }
            Item { width: 1; height: 14 }
        }
    }
}
