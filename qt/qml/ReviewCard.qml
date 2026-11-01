import QtQuick
import QtQuick.Effects
import Reelhouse 1.0

// One review (GTK malinfo::review_card): avatar, name and date, the tag and score, the text (spoilers blurred until shown).
SectionCard {
    id: rv
    property var review: ({})
    property bool open: false
    property bool spoilerShown: false
    readonly property color tagColor: {
        var t = (review.tag || "").toLowerCase()
        if (t.indexOf("not") === 0) return Qt.alpha(Theme.c.error, 0.35)
        if (t.indexOf("mixed") === 0) return Qt.alpha(Theme.c.warning, 0.35)
        return Qt.alpha(Theme.c.success, 0.35)
    }
    Item {
        width: parent.width
        height: 42
        RoundedPicture { id: av; width: 42; height: 42; radius: 21; url: rv.review.avatar ? rv.review.avatar : "" }
        Column {
            anchors { left: av.right; leftMargin: 10; verticalCenter: parent.verticalCenter }
            Text { text: rv.review.user ? rv.review.user : ""; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
            Text { text: rv.review.date ? rv.review.date : ""; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
        }
        Row {
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: 8
            Rectangle {
                visible: !!rv.review.tag
                radius: 6
                color: rv.tagColor
                implicitWidth: tagT.implicitWidth + 16
                implicitHeight: 22
                anchors.verticalCenter: parent.verticalCenter
                Text { id: tagT; anchors.centerIn: parent; text: rv.review.tag ? rv.review.tag : ""; color: Theme.c.text; font.pixelSize: 12; font.weight: Font.Bold; font.family: Theme.type.family }
            }
            Text { visible: (rv.review.score || 0) > 0; text: "★ " + rv.review.score; color: Theme.c.rating; font.pixelSize: 15; font.weight: Font.ExtraBold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
        }
    }
    // The text may hold pictures (imgur and the like): they show in their place once the review is opened (GTK richtext).
    readonly property var pieces: review.pieces !== undefined && review.pieces.length > 0 ? review.pieces : [{text: review.text || ""}]
    readonly property int firstText: { for (var i = 0; i < pieces.length; ++i) if (pieces[i].text !== undefined) return i; return -1 }
    readonly property bool isLong: (firstText >= 0 && pieces[firstText].text.length > 420) || pieces.length > 1
    Column {
        id: body
        width: parent.width
        spacing: 8
        layer.enabled: rv.review.spoiler === true && !rv.spoilerShown
        layer.effect: MultiEffect { blurEnabled: true; blur: 0.6; blurMax: 24 }
        Repeater {
            model: rv.pieces
            delegate: Item {
                required property var modelData
                required property int index
                readonly property bool shown: index === rv.firstText || rv.open
                width: body.width
                height: shown ? (modelData.text !== undefined ? t.implicitHeight : 260) : 0
                visible: shown
                Text {
                    id: t
                    visible: modelData.text !== undefined
                    width: parent.width
                    text: modelData.text !== undefined ? modelData.text : ""
                    color: Theme.c.text
                    font.pixelSize: 14
                    font.family: Theme.type.family
                    wrapMode: Text.Wrap
                    maximumLineCount: rv.open ? 100000 : 6
                    elide: rv.open ? Text.ElideNone : Text.ElideRight
                }
                PosterImage {
                    visible: modelData.image !== undefined && parent.shown
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectFit
                    url: modelData.image !== undefined && parent.shown ? modelData.image : ""
                }
            }
        }
    }
    Item {
        width: parent.width
        height: 32
        Row {
            spacing: 8
            FlatButton { visible: rv.review.spoiler === true && !rv.spoilerShown; kind: "flat"; text: Core.tr(Core.uiLang, "Show spoilers"); implicitHeight: 32; onClicked: rv.spoilerShown = true }
            FlatButton { visible: rv.isLong; kind: "flat"; text: rv.open ? Core.tr(Core.uiLang, "Show less") : Core.tr(Core.uiLang, "Read more"); implicitHeight: 32; onClicked: rv.open = !rv.open }
        }
        Text {
            visible: (rv.review.reactions || 0) > 0
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            text: rv.review.reactions + " found this helpful"
            color: Theme.c.textDim
            font.pixelSize: 12
            font.family: Theme.type.family
        }
    }
}
