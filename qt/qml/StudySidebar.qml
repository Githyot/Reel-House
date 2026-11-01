import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The part of the left sidebar that belongs to the Study mode (under Decks / Learn / Time): the Pomodoro, always open
// for a quick click, and right under it the time studied today and this week, side by side. A click on a box opens
// the Time page on that period. In a narrow rail only the Pomodoro button and the minutes of today fit.
Column {
    id: ss
    // the Time page is opened through the sidebar's own navigation
    signal openTime()
    readonly property bool narrow: width < 150
    property var t: ({})
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function refresh() { t = Core.state("studytime") }
    function dur(secs) {
        const m = Math.round((secs || 0) / 60)
        if (m < 60) return m + " min"
        return Math.floor(m / 60) + " h" + (m % 60 > 0 ? " " + (m % 60) : "")
    }
    spacing: 8
    Component.onCompleted: refresh()
    onVisibleChanged: if (visible) refresh()
    // the numbers grow while studying; the core counts, this only reads them now and then
    Timer { interval: 30000; repeat: true; running: ss.visible; onTriggered: ss.refresh() }
    Connections { target: Core; function onStudyChanged() { if (ss.visible) ss.refresh() } }

    // ---- the Pomodoro, open ----
    Rectangle {
        visible: !ss.narrow
        width: parent.width
        height: pomo.implicitHeight + 20
        radius: Theme.shape.card !== undefined ? Theme.shape.card : 12
        color: Theme.c.card !== undefined ? Theme.c.card : Theme.c.fill3
        border.width: 1
        border.color: Theme.c.border
        PomodoroWidget { id: pomo; side: true; x: 10; y: 10; width: parent.width - 20 }
    }
    PomodoroButton { visible: ss.narrow; side: true; anchors.horizontalCenter: parent.horizontalCenter }

    // ---- volume of the card sounds and clips (the Study players, not the video player) ----
    Rectangle {
        visible: !ss.narrow
        width: parent.width
        height: 44
        radius: Theme.shape.card !== undefined ? Theme.shape.card : 12
        color: Theme.c.card !== undefined ? Theme.c.card : Theme.c.fill3
        border.width: 1
        border.color: Theme.c.border
        property real vol: Core.prefs.general && Core.prefs.general.study_volume !== undefined ? Core.prefs.general.study_volume : 100
        FlatButton {
            id: muteBtn
            kind: "flat"; glyph: vslider.value === 0 ? "mute" : "volume"
            anchors { left: parent.left; leftMargin: 6; verticalCenter: parent.verticalCenter }
            implicitWidth: 32; implicitHeight: 32
            ToolTip.visible: hovered; ToolTip.text: ss.tr("Volume of the card sounds and clips")
            property real before: 100
            onClicked: {
                if (vslider.value > 0) { before = vslider.value; vslider.value = 0 }
                else vslider.value = before > 0 ? before : 100
                Core.cmd({cmd: "study_set_volume", volume: vslider.value})
            }
        }
        Slider {
            id: vslider
            anchors { left: muteBtn.right; leftMargin: 4; right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
            from: 0; to: 100; stepSize: 5
            value: parent.vol
            onMoved: Core.cmd({cmd: "study_set_volume", volume: value})
            background: Rectangle {
                x: vslider.leftPadding; y: vslider.topPadding + vslider.availableHeight / 2 - height / 2
                width: vslider.availableWidth; height: 4; radius: 2
                color: Theme.c.fill3
                Rectangle { width: vslider.visualPosition * parent.width; height: parent.height; radius: 2; color: Theme.c.accent }
            }
            handle: Rectangle {
                x: vslider.leftPadding + vslider.visualPosition * (vslider.availableWidth - width)
                y: vslider.topPadding + vslider.availableHeight / 2 - height / 2
                width: 16; height: 16; radius: 8
                color: Theme.c.accent
                border.width: 2; border.color: Theme.c.card !== undefined ? Theme.c.card : Theme.c.window
            }
        }
    }

    // ---- today and this week ----
    Row {
        width: parent.width
        spacing: 8
        Repeater {
            model: ss.narrow ? [{v: ss.t.today, l: ss.tr("Today")}] : [{v: ss.t.today, l: ss.tr("Today")}, {v: ss.t.week, l: ss.tr("This week")}]
            delegate: Rectangle {
                id: box
                required property var modelData
                width: ss.narrow ? ss.width : (ss.width - 8) / 2
                height: ss.narrow ? 44 : 58
                radius: Theme.shape.card !== undefined ? Theme.shape.card : 12
                color: boxArea.containsMouse ? Theme.c.hover : (Theme.c.card !== undefined ? Theme.c.card : Theme.c.fill3)
                border.width: 1
                border.color: Theme.c.border
                Column {
                    anchors.centerIn: parent
                    width: parent.width - 12
                    spacing: 1
                    Text {
                        width: parent.width; horizontalAlignment: Text.AlignHCenter
                        text: ss.dur(box.modelData.v)
                        color: Theme.c.text
                        font.pixelSize: ss.narrow ? 12 : 17; font.weight: Font.Bold; font.family: Theme.type.family
                        fontSizeMode: Text.HorizontalFit; minimumPixelSize: 9
                    }
                    Text {
                        width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight
                        text: box.modelData.l
                        color: Theme.c.textDim
                        font.pixelSize: ss.narrow ? 9 : 11; font.family: Theme.type.family
                    }
                }
                MouseArea { id: boxArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: ss.openTime() }
                ToolTip.visible: boxArea.containsMouse && ss.narrow
                ToolTip.text: box.modelData.l + ": " + ss.dur(box.modelData.v)
            }
        }
    }
}
