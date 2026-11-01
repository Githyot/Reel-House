import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The Catalog menu (GTK browse.rs): two big entries, Platforms and Categories, each opening its own list (with a way back).
Popover {
    id: cp
    property string page: "menu"
    property string platform: ""          // the chosen platform, "" = everything
    property var platforms: []            // names found in the rows of the start page
    property int kind: 0                  // 0 anime, 1 movies, 2 series
    property var cats: ({})
    signal platformChosen(string name)
    signal categoryChosen(string name, string kind)
    onAboutToShow: { page = "menu"; if (cats.anime === undefined) cats = Core.state("categories") }
    padding: 8
    contentItem: Item {
        implicitWidth: Math.min(cp.page === "categories" ? 440 : 316, cp.ovW - 16)
        implicitHeight: cp.page === "menu" ? menuCol.implicitHeight : (cp.page === "platforms" ? platCol.implicitHeight : catCol.implicitHeight)

        Column {
            id: menuCol
            visible: cp.page === "menu"
            width: parent.width
            spacing: 4
            Repeater {
                model: [{key: "platforms", title: Core.tr(Core.uiLang, "Platforms"), sub: Core.tr(Core.uiLang, "Netflix, Disney+, Crunchyroll and more"), icon: "quality"},
                        {key: "categories", title: Core.tr(Core.uiLang, "Categories"), sub: Core.tr(Core.uiLang, "Anime, movies and series by genre"), icon: "list"}]
                delegate: Rectangle {
                    required property var modelData
                    width: menuCol.width
                    height: 58
                    radius: 10
                    color: mArea.containsMouse ? Theme.c.hover : "transparent"
                    Icon { id: mi; x: 12; anchors.verticalCenter: parent.verticalCenter; name: modelData.icon; size: 24 }
                    Column {
                        anchors { left: mi.right; leftMargin: 14; right: arrow.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                        spacing: 2
                        Text { text: modelData.title; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
                        Text { width: parent.width; elide: Text.ElideRight; text: modelData.sub; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                    }
                    Icon { id: arrow; anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } name: "right"; size: 16 }
                    MouseArea { id: mArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: cp.page = modelData.key }
                }
            }
        }

        // the way back to the menu
        component BackHead: Rectangle {
            property string title: ""
            width: parent.width
            height: 38
            radius: 8
            color: bArea.containsMouse ? Theme.c.hover : "transparent"
            Row {
                x: 6
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Icon { name: "left"; size: 16; anchors.verticalCenter: parent.verticalCenter }
                Text { text: parent.parent.title; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
            }
            MouseArea { id: bArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: cp.page = "menu" }
        }

        Column {
            id: platCol
            visible: cp.page === "platforms"
            width: parent.width
            spacing: 4
            BackHead { title: Core.tr(Core.uiLang, "Platforms") }
            Rectangle { width: parent.width; height: 1; color: Theme.c.divider }
            Flickable {
                width: parent.width
                height: Math.min(420, platList.implicitHeight)
                contentHeight: platList.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: platList
                    width: parent.width
                    Repeater {
                        model: [""].concat(cp.platforms)
                        delegate: Rectangle {
                            required property var modelData
                            width: platList.width
                            height: 40
                            radius: 8
                            color: pArea.containsMouse ? Theme.c.hover : "transparent"
                            Text { x: 12; anchors.verticalCenter: parent.verticalCenter; text: modelData === "" ? Core.tr(Core.uiLang, "Everything") : modelData; color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family }
                            Icon { visible: modelData === cp.platform; anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter } name: "check"; size: 16 }
                            MouseArea { id: pArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { cp.close(); cp.platformChosen(modelData) } }
                        }
                    }
                }
            }
        }

        Column {
            id: catCol
            visible: cp.page === "categories"
            width: parent.width
            spacing: 8
            BackHead { title: Core.tr(Core.uiLang, "Categories") }
            Rectangle { width: parent.width; height: 1; color: Theme.c.divider }
            // Anime / Movies / Series (a linked group of toggle buttons)
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                topPadding: 2
                Repeater {
                    model: ["Anime", "Movies", "Series"]
                    delegate: Rectangle {
                        required property var modelData
                        required property int index
                        width: 96
                        height: 34
                        color: cp.kind === index ? Theme.c.accent20 : (kArea.containsMouse ? Theme.c.selected : Theme.c.hover)
                        topLeftRadius: index === 0 ? 8 : 0
                        bottomLeftRadius: index === 0 ? 8 : 0
                        topRightRadius: index === 2 ? 8 : 0
                        bottomRightRadius: index === 2 ? 8 : 0
                        Text { anchors.centerIn: parent; text: Core.tr(Core.uiLang, modelData); color: Theme.c.text; font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family }
                        MouseArea { id: kArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: cp.kind = index }
                    }
                }
            }
            Flickable {
                width: parent.width
                height: Math.min(360, chips.implicitHeight + 8)
                contentHeight: chips.implicitHeight + 8
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Flow {
                    id: chips
                    y: 4
                    width: parent.width
                    spacing: 6
                    Repeater {
                        model: cp.kind === 0 ? (cp.cats.anime || []) : (cp.kind === 1 ? (cp.cats.movie || []) : (cp.cats.series || []))
                        delegate: GenreChip {
                            required property var modelData
                            text: modelData
                            onClicked: { cp.close(); cp.categoryChosen(modelData, cp.kind === 0 ? "anime" : (cp.kind === 1 ? "movie" : "series")) }
                        }
                    }
                }
            }
        }
    }
}
