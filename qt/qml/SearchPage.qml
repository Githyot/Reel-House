import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

Item {
    id: page
    readonly property var rows: Core.search.rows !== undefined ? Core.search.rows : []
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
            y: 14
            width: scroll.width - 36
            spacing: 30
            Text { visible: !!Core.search.query; text: Core.tr(Core.uiLang, "Results for") + " “" + (Core.search.query || "") + "”"; color: Theme.c.text; font.pixelSize: 24; font.weight: Font.ExtraBold; font.family: Theme.type.family }
            // the number is the model: rows that are already there keep their covers when a late addon answers
            Repeater {
                model: page.rows.length
                delegate: PosterRow {
                    required property int index
                    readonly property var row: page.rows[index] || ({})
                    width: col.width; title: row.title || ""; addon: row.addon || ""; items: row.items || []
                }
            }
        }
    }
    // the macaw: thinking while it searches, a little sad with no results, waiting for a word otherwise
    Mascot {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: searchHint.top
        anchors.bottomMargin: 12
        visible: page.rows.length === 0
        animate: visible
        size: 110
        mood: Core.search.loading === true ? "think" : (Core.search.query ? "sad" : "idle")
    }
    Text {
        id: searchHint
        anchors.centerIn: parent
        visible: page.rows.length === 0
        text: Core.search.loading === true ? Core.tr(Core.uiLang, "Searching…") : (Core.search.query ? Core.tr(Core.uiLang, "No results. Try another spelling or fewer words.") : Core.tr(Core.uiLang, "Type in the search box and press Enter."))
        color: Theme.c.textDim
        font.pixelSize: 16
        font.family: Theme.type.family
    }
}
