import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// A page with the titles of one category (GTK genre.rs): its name, cards with cover and details, "Load more".
Item {
    id: page
    readonly property var g: Core.genre
    readonly property var items: g.items !== undefined ? g.items : []
    readonly property real gutter: Theme.mobile ? 16 : Theme.spacing.page
    readonly property int gap: Theme.spacing.rowGap || 14
    readonly property int perLine: Math.max(1, Math.min(5, Math.floor((width - 2 * gutter + gap) / (420 + gap))))

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
            x: page.gutter
            y: 8
            width: scroll.width - 2 * page.gutter
            spacing: 12
            Text { width: parent.width; elide: Text.ElideRight; text: page.g.name ? page.g.name : ""; color: Theme.c.text; font.pixelSize: Theme.mobile ? 28 : Theme.type.titleLarge.size; font.weight: Font.ExtraBold; font.family: Theme.type.display || Theme.type.family }
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
                rowSpacing: page.gap
                columnSpacing: page.gap
                Repeater {
                    model: page.items
                    delegate: RichCard {
                        required property var modelData
                        width: (grid.width - (page.perLine - 1) * page.gap) / page.perLine
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
