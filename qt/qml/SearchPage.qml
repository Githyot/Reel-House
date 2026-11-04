import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

Item {
    id: page
    readonly property real gutter: Theme.mobile ? 16 : Theme.spacing.page
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
            x: page.gutter
            y: Theme.mobile ? 8 : 14
            width: scroll.width - 2 * page.gutter
            spacing: Theme.mobile ? 24 : 30
            // the heading, and while late addons still answer a small spinner beside it
            Row {
                visible: !!Core.search.query && page.rows.length > 0
                width: parent.width
                spacing: 12
                Text { id: resultsFor; width: Math.min(implicitWidth, parent.width - (moreBusy.visible ? 40 : 0)); elide: Text.ElideRight; text: Core.tr(Core.uiLang, "Results for") + " “" + (Core.search.query || "") + "”"; color: Theme.c.text; font.pixelSize: Theme.mobile ? 22 : Theme.type.title.size + 6; font.weight: Font.ExtraBold; font.family: Theme.type.display || Theme.type.family }
                BusyIndicator { id: moreBusy; visible: Core.search.loading === true; running: visible; width: 28; height: 28; anchors.verticalCenter: resultsFor.verticalCenter }
            }
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
    // the Arara: thinking while it searches, a little sad with no results, waiting for a word otherwise
    StatusPage {
        anchors.centerIn: parent
        visible: page.rows.length === 0
        mood: Core.search.loading === true ? "think" : (Core.search.query ? "sad" : "idle")
        title: Core.search.query && Core.search.loading !== true ? Core.tr(Core.uiLang, "Results for") + " “" + Core.search.query + "”" : ""
        description: Core.search.loading === true ? Core.tr(Core.uiLang, "Searching…") : (Core.search.query ? Core.tr(Core.uiLang, "No results. Try another spelling or fewer words.") : Core.tr(Core.uiLang, "Type in the search box and press Enter."))
    }
}
