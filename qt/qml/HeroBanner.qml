import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Featured titles with their wallpapers, like the Stremio board.
Column {
    id: hero
    property var items: []
    property int current: 0
    spacing: 10
    onItemsChanged: if (current >= items.length) current = 0

    Item {
        width: hero.width
        height: 280

        RoundedPicture {
            anchors.fill: parent
            radius: Theme.shape.large
            url: hero.items.length > hero.current && hero.items[hero.current].background ? hero.items[hero.current].background : ""
        }
        // dark on the left (for the text) and at the bottom
        Rectangle {
            anchors.fill: parent
            radius: Theme.shape.large
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: "#d1000000" }
                GradientStop { position: 0.55; color: "#66000000" }
                GradientStop { position: 1.0; color: "#26000000" }
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: Theme.shape.large
            gradient: Gradient {
                GradientStop { position: 0.5; color: "#00000000" }
                GradientStop { position: 1.0; color: "#8c000000" }
            }
        }
        Column {
            id: txt
            property var m: hero.items.length > hero.current ? hero.items[hero.current] : ({})
            anchors { left: parent.left; bottom: parent.bottom; leftMargin: hero.items.length > 1 ? 66 : 28; bottomMargin: 26 }
            width: Math.min(parent.width - 140, 760)
            spacing: 8
            Text { width: parent.width; text: txt.m.name || ""; color: "#ffffff"; font.pixelSize: 40; font.weight: Font.ExtraBold; font.family: Theme.type.family; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight }
            Text {
                visible: text !== ""
                text: [txt.m.releaseInfo || "", (txt.m.genres || []).slice(0, 3).join(" · ")].filter(function(x) { return x !== "" }).join("   ")
                color: "#e0ffffff"; font.pixelSize: 15; font.family: Theme.type.family
            }
            Text { visible: !!txt.m.description; width: parent.width; text: txt.m.description || ""; color: "#e0ffffff"; font.pixelSize: 15; font.family: Theme.type.family; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight }
            Item { width: 1; height: 2 }
            FlatButton {
                kind: "suggested"
                text: Core.tr(Core.uiLang, "View")
                implicitWidth: 88
                implicitHeight: 42
                onClicked: Core.cmd({cmd: "open_detail", item: txt.m})
            }
        }
        Repeater {
            model: hero.items.length > 1 ? [ {icon: "left", step: -1}, {icon: "right", step: 1} ] : []
            delegate: Rectangle {
                width: 38
                height: 38
                radius: 19
                color: arrowArea.containsMouse ? "#cc000000" : "#99000000"
                anchors.verticalCenter: parent.verticalCenter
                x: modelData.step < 0 ? 12 : parent.width - width - 12
                Icon { anchors.centerIn: parent; name: modelData.icon; size: 22; color: "#ffffff" }
                MouseArea {
                    id: arrowArea
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: { hero.current = (hero.current + modelData.step + hero.items.length) % hero.items.length; auto.restart() }
                }
            }
        }
        Timer { id: auto; interval: 8000; running: hero.items.length > 1 && hero.visible; repeat: true; onTriggered: hero.current = (hero.current + 1) % hero.items.length }
    }
    Row {
        visible: hero.items.length > 1
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 8
        Repeater {
            model: hero.items.length
            delegate: Rectangle {
                width: 7
                height: 7
                radius: 3.5
                color: index === hero.current ? Theme.c.text : Theme.c.fill
                MouseArea { anchors.fill: parent; anchors.margins: -4; onClicked: hero.current = index }
            }
        }
    }
}
