import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Core.tr(Core.uiLang, "Similar anime") (MyAnimeList) or Core.tr(Core.uiLang, "Recommended and similar") (TMDB): a row of covers; a click finds the title in the catalog.
Column {
    id: row
    property var recs: []
    property bool tmdb: false
    visible: recs.length > 0
    // 14 between the title and the covers; the list starts focusRoom (16) higher to leave room for the focus ring
    spacing: -2
    Row {
        spacing: 12
        Text { text: row.tmdb ? Core.tr(Core.uiLang, "Recommended and similar") : Core.tr(Core.uiLang, "Similar anime"); color: Theme.c.text; font.pixelSize: Theme.type.rowTitle.size; font.weight: Theme.type.rowTitle.weight || Font.ExtraBold; font.letterSpacing: Theme.type.rowTitle.letterSpacing || 0; font.family: Theme.type.display || Theme.type.family }
        Text { text: row.tmdb ? "from TMDB" : "from MyAnimeList"; color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family; topPadding: 8 }
    }
    HScrollList {
        id: list
        // room for the focus ring around the cards (HScrollList.focusRoom)
        x: -focusRoom
        width: row.width + 2 * focusRoom
        height: Theme.spacing.posterH * 0.75 + 46 + barRoom + focusRoom
        model: row.recs
        delegate: PosterCard {
            required property var modelData
            pw: Theme.spacing.posterW * 0.75
            ph: Theme.spacing.posterH * 0.75
            item: ({ name: modelData.title, poster: modelData.image, isNew: modelData.isNew })
            onClicked: Core.cmd({cmd: "open_card", card: modelData})
        }
    }
}
