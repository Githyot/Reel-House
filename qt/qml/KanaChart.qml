import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The Characters view of Learn (Japanese): the hiragana and katakana charts — gojūon, dakuten / handakuten and
// yōon — each cell with its character, romaji and a bar of how well it is known (knowledge.md, "Characters"); a
// click plays its sound. "Learn the characters" starts the next lesson of the script; the lessons are listed too.
Item {
    id: kc
    property var kana: null
    property real rad: 12
    signal command(var o)
    function tr(s) { return Core.tr(Core.uiLang, s) }
    property string script: "h"
    readonly property var chart: kana ? (script === "h" ? kana.hira : kana.kata) || [] : []
    readonly property var lessons: kana ? (script === "h" ? kana.lessons_h : kana.lessons_k) || [] : []
    readonly property int learned: kana ? (script === "h" ? kana.learned_h : kana.learned_k) || 0 : 0
    readonly property var current: {
        for (let i = 0; i < lessons.length; ++i) if (lessons[i].state === "current") return lessons[i]
        return null
    }
    readonly property bool playful: Theme.look === "android" || Theme.look === "apple"
    readonly property color cGood: Theme.c.success
    // the character whose sound was played last (it lights up)
    property string lit: ""
    function play(cell) {
        lit = cell.c
        litTimer.restart()
        if ((cell.snd || "") !== "") command({cmd: "learn_play", url: cell.snd, slow: false})
    }
    Timer { id: litTimer; interval: 700; onTriggered: kc.lit = "" }
    function sectionTitle(id) {
        return ({gojuon: tr("Gojūon"), dakuten: tr("Dakuten and handakuten"), yoon: tr("Yōon (combined sounds)")})[id] || id
    }

    Flickable {
        id: fl
        anchors.fill: parent
        contentWidth: width
        contentHeight: col.y + col.height + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        Column {
            id: col
            y: 10
            width: fl.width
            spacing: 20

            // the head: the script, how many are learned, and the buttons
            Rectangle {
                width: parent.width
                height: headRow.implicitHeight + 36
                radius: kc.rad
                color: Theme.c.card
                border.width: kc.playful ? 2 : 1
                border.color: Theme.c.border
                Row {
                    id: headRow
                    x: 18; y: 18
                    width: parent.width - 36
                    spacing: 16
                    Mascot {
                        id: kMascot
                        visible: kc.width >= 520
                        mood: kc.learned === 0 ? "wave" : (kc.learned >= (kc.kana ? kc.kana.total : 104) ? "cheer" : "happy")
                        size: 96
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Column {
                        width: parent.width - (kMascot.visible ? kMascot.width + 16 : 0)
                        spacing: 10
                        Segmented {
                            model: [{id: "h", label: kc.tr("Hiragana")}, {id: "k", label: kc.tr("Katakana")}]
                            current: kc.script
                            onPicked: function(id) { kc.script = id }
                        }
                        Lbl {
                            width: parent.width; wrapMode: Text.Wrap; dim: true
                            text: kc.tr("Learn the sounds and shapes of the characters. Click a character to hear it.")
                        }
                        // how many of the script are known
                        Row {
                            spacing: 10
                            Rectangle {
                                width: Math.min(220, col.width * 0.35); height: 10; radius: 5
                                color: Theme.c.fill
                                anchors.verticalCenter: parent.verticalCenter
                                Rectangle { width: parent.width * Math.min(1, kc.learned / Math.max(1, kc.kana ? kc.kana.total : 104)); height: parent.height; radius: parent.radius; color: kc.cGood }
                            }
                            Lbl { text: kc.tr("%1 characters learned").arg(kc.learned); dim: true; size: 13; anchors.verticalCenter: parent.verticalCenter }
                        }
                        Flow {
                            width: parent.width
                            spacing: 10
                            LearnButton {
                                tone: kc.cGood; fg: Theme.c.greenFg
                                glyph: "star"
                                text: kc.current ? kc.tr("Learn the characters") : kc.tr("Review the characters")
                                onClicked: {
                                    if (kc.current) kc.command({cmd: "learn_start", lesson: kc.current.id})
                                    else kc.command({cmd: "learn_practice", mode: "kana-" + kc.script})
                                }
                            }
                            LearnButton {
                                outline: true; glyph: "dumbbell"
                                text: kc.tr("Practice")
                                enabled: kc.learned >= 4
                                onClicked: kc.command({cmd: "learn_practice", mode: "kana-" + kc.script})
                            }
                        }
                        Lbl {
                            visible: kc.kana !== null && (kc.kana.sounds || 0) === 0
                            width: parent.width; wrapMode: Text.Wrap; dim: true; size: 12
                            text: kc.tr("The character sounds are missing (the kana folder next to the program).")
                        }
                    }
                }
            }

            // the charts
            Repeater {
                model: kc.chart.length
                delegate: Column {
                    id: sec
                    required property int index
                    readonly property var s: kc.chart[index] || ({})
                    readonly property int cols: s.cols || 5
                    readonly property real gap: 8
                    readonly property real cell: Math.min(96, Math.floor((col.width - (cols - 1) * gap) / cols))
                    width: col.width
                    spacing: 10
                    Lbl { text: kc.sectionTitle(sec.s.id); bold: true; size: 17 }
                    Grid {
                        columns: sec.cols
                        spacing: sec.gap
                        anchors.horizontalCenter: parent.horizontalCenter
                        Repeater {
                            model: (sec.s.rows || []).length * sec.cols
                            delegate: Item {
                                id: cellItem
                                required property int index
                                readonly property var c: ((sec.s.rows || [])[Math.floor(index / sec.cols)] || [])[index % sec.cols] || null
                                width: sec.cell; height: Math.round(sec.cell * 0.92)
                                Rectangle {
                                    visible: cellItem.c !== null
                                    anchors.fill: parent
                                    radius: kc.playful ? 14 : Math.max(6, Theme.shape.control)
                                    color: kc.lit === (cellItem.c ? cellItem.c.c : "-") ? Qt.tint(Theme.c.card, Qt.rgba(kc.cGood.r, kc.cGood.g, kc.cGood.b, 0.25)) : (cArea.containsMouse ? Theme.c.cardHover : Theme.c.card)
                                    border.width: kc.playful ? 2 : 1
                                    border.color: cellItem.c && cellItem.c.s >= 0.8 ? kc.cGood : Theme.c.border
                                    Column {
                                        anchors.centerIn: parent
                                        anchors.verticalCenterOffset: -4
                                        spacing: 0
                                        Text {
                                            anchors.horizontalCenter: parent.horizontalCenter
                                            text: cellItem.c ? cellItem.c.c : ""
                                            color: cellItem.c && cellItem.c.seen ? Theme.c.text : Theme.c.textDim
                                            font.pixelSize: Math.round(sec.cell * (sec.cols === 3 ? 0.34 : 0.4)); font.family: Theme.type.family
                                        }
                                        Text {
                                            anchors.horizontalCenter: parent.horizontalCenter
                                            text: cellItem.c ? cellItem.c.r : ""
                                            color: Theme.c.textDim
                                            font.pixelSize: Math.max(11, Math.round(sec.cell * 0.15)); font.family: Theme.type.family
                                        }
                                    }
                                    // a small speaker on the characters that have a sound
                                    LearnGlyph {
                                        visible: cellItem.c !== null && (cellItem.c.snd || "") !== ""
                                        anchors { top: parent.top; right: parent.right; margins: 6 }
                                        name: "volume"; size: 12; color: Theme.c.textDim
                                    }
                                    // how well it is known
                                    Rectangle {
                                        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 8 }
                                        height: 5; radius: 3
                                        color: Theme.c.fill
                                        Rectangle {
                                            width: parent.width * Math.max(0, Math.min(1, cellItem.c ? cellItem.c.s : 0))
                                            height: parent.height; radius: parent.radius
                                            color: kc.cGood
                                        }
                                    }
                                    MouseArea { id: cArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: kc.play(cellItem.c) }
                                    ToolTip.visible: cArea.containsMouse; ToolTip.delay: 600
                                    ToolTip.text: cellItem.c ? (cellItem.c.seen ? kc.tr("Known: %1%").arg(Math.round(cellItem.c.s * 100)) : kc.tr("Not learned yet")) : ""
                                }
                            }
                        }
                    }
                }
            }

            // the lessons of the script, in order
            Column {
                width: parent.width
                spacing: 10
                Lbl { text: kc.tr("Lessons"); bold: true; size: 17 }
                Flow {
                    width: parent.width
                    spacing: 8
                    Repeater {
                        model: kc.lessons.length
                        delegate: Rectangle {
                            id: lesChip
                            required property int index
                            readonly property var l: kc.lessons[index] || ({})
                            width: lcRow.implicitWidth + 24; height: 40
                            radius: kc.playful ? 20 : Theme.shape.control
                            color: l.state === "current" ? kc.cGood : (lcArea.containsMouse ? Theme.c.cardHover : Theme.c.card)
                            border.width: l.state === "current" ? 0 : (kc.playful ? 2 : 1)
                            border.color: l.state === "done" ? kc.cGood : Theme.c.border
                            Row {
                                id: lcRow
                                anchors.centerIn: parent
                                spacing: 6
                                LearnGlyph { visible: lesChip.l.state === "done"; name: "check"; size: 16; color: kc.cGood; anchors.verticalCenter: parent.verticalCenter }
                                Text {
                                    text: (lesChip.l.chars || []).slice(0, 5).join(" ") + ((lesChip.l.chars || []).length > 5 ? " …" : "")
                                    color: lesChip.l.state === "current" ? Theme.c.greenFg : Theme.c.text
                                    font.pixelSize: 16; font.family: Theme.type.family
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                            }
                            MouseArea { id: lcArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: kc.command({cmd: "learn_start", lesson: lesChip.l.id}) }
                            ToolTip.visible: lcArea.containsMouse; ToolTip.delay: 500; ToolTip.text: kc.tr("Lesson %1 of %2").arg(lesChip.l.n || 0).arg(kc.lessons.length)
                        }
                    }
                }
            }
        }
    }
}
