import QtQuick
import Reelhouse 1.0

// Reviews from MyAnimeList (or TMDB): counts by tag, up to eight cards and a link to all of them.
Column {
    id: rs
    property var reviews: []
    property string url: ""
    property bool tmdb: false
    visible: reviews.length > 0
    spacing: 10
    function count(prefix) {
        return reviews.filter(function(r) {
            var t = (r.tag || "").toLowerCase()
            if (prefix === "not") return t.indexOf("not") === 0
            if (prefix === "mixed") return t.indexOf("mixed") === 0
            return t.indexOf("not") !== 0 && t.indexOf("mixed") !== 0
        }).length
    }
    Text { text: Core.tr(Core.uiLang, "Reviews"); color: Theme.c.text; font.pixelSize: Theme.type.rowTitle.size; font.weight: Theme.type.rowTitle.weight || Font.ExtraBold; font.letterSpacing: Theme.type.rowTitle.letterSpacing || 0; font.family: Theme.type.display || Theme.type.family }
    Row {
        spacing: 6
        Repeater {
            model: [["rec", Core.tr(Core.uiLang, "Recommended"), Qt.alpha(Theme.c.success, 0.35)], ["mixed", Core.tr(Core.uiLang, "Mixed feelings"), Qt.alpha(Theme.c.warning, 0.35)], ["not", Core.tr(Core.uiLang, "Not recommended"), Qt.alpha(Theme.c.error, 0.35)]]
            delegate: Rectangle {
                required property var modelData
                readonly property int n: rs.count(modelData[0])
                visible: n > 0
                radius: 6
                color: modelData[2]
                implicitWidth: ct.implicitWidth + 16
                implicitHeight: 22
                Text { id: ct; anchors.centerIn: parent; text: modelData[1] + " " + parent.n; color: Theme.c.text; font.pixelSize: 12; font.weight: Font.Bold; font.family: Theme.type.family }
            }
        }
    }
    Repeater {
        model: rs.reviews
        delegate: ReviewCard { required property var modelData; width: rs.width; review: modelData }
    }
    FlatButton {
        visible: rs.url !== ""
        pill: true
        text: rs.tmdb ? Core.tr(Core.uiLang, "All reviews on TMDB") : Core.tr(Core.uiLang, "All reviews on MyAnimeList")
        onClicked: Qt.openUrlExternally(rs.url.replace(/\/+$/, "") + "/reviews")
    }
}
