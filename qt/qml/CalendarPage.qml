import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The calendar (GTK browse::rebuild_calendar): a month grid with the days new episodes of your library series come out,
// and "Coming up" below.
Item {
    id: page
    readonly property var cal: Core.calendar
    readonly property var entries: cal.entries !== undefined ? cal.entries : []
    property int year: new Date().getFullYear()
    property int month: new Date().getMonth() + 1          // 1..12
    readonly property var monthNames: ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]
    function pad(v) { return v < 10 ? "0" + v : "" + v }
    function iso(y, m, d) { return y + "-" + pad(m) + "-" + pad(d) }
    readonly property string todayIso: { var t = new Date(); return iso(t.getFullYear(), t.getMonth() + 1, t.getDate()) }
    // 0 = Monday
    readonly property int firstWeekday: (new Date(year, month - 1, 1).getDay() + 6) % 7
    readonly property int days: new Date(year, month, 0).getDate()
    function step(d) {
        var idx = year * 12 + (month - 1) + d
        year = Math.floor(idx / 12)
        month = idx - year * 12 + 1
    }
    function onDay(date) { return page.entries.filter(function(e) { return e.date === date }) }
    readonly property var upcoming: entries.filter(function(e) { return e.date >= page.todayIso }).slice(0, 60)
    function openShow(id) {
        var lib = Core.library.items !== undefined ? Core.library.items : []
        for (var i = 0; i < lib.length; ++i) if (lib[i].id === id) { Core.cmd({cmd: "open_detail", item: lib[i]}); return }
    }

    // A click on a day: who comes out that day (a popover under the day).
    property var dayEps: []
    property string dayDate: ""
    function showDay(date, eps, anchor) {
        dayDate = date
        dayEps = eps
        dayPop.openBelow(anchor, false)
    }
    function dayTitle() {
        const p = dayDate.split("-")
        if (p.length < 3) return dayDate
        return new Date(+p[0], +p[1] - 1, +p[2]).toLocaleDateString(Qt.locale(), "dddd, d MMMM yyyy")
    }
    Popover {
        id: dayPop
        contentItem: Item {
            implicitWidth: 340
            implicitHeight: inner.implicitHeight
            Column {
                id: inner
            width: 340
            spacing: 4
            Text { x: 6; topPadding: 4; width: parent.width - 12; text: page.dayTitle(); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family; elide: Text.ElideRight }
            Text { x: 6; visible: page.dayEps.length === 0; bottomPadding: 6; text: Core.tr(Core.uiLang, "No episodes of your series come out this day."); color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family; wrapMode: Text.Wrap; width: parent.width - 12 }
            Flickable {
                visible: page.dayEps.length > 0
                width: parent.width
                height: Math.min(dayCol.implicitHeight, 360)
                contentHeight: dayCol.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: dayCol
                    width: parent.width
                    Repeater {
                        model: page.dayEps
                        delegate: Rectangle {
                            activeFocusOnTab: true
                            Keys.onReturnPressed: dayRow.clicked(null)
                            Keys.onSpacePressed: dayRow.clicked(null)
                            border.width: activeFocus ? 3 : 0
                            border.color: Theme.c.accent
                            required property var modelData
                            width: dayCol.width
                            height: 64
                            radius: Theme.shape.small
                            color: dayRow.containsMouse ? Theme.c.hover : "transparent"
                            RoundedPicture { id: dpic; x: 6; anchors.verticalCenter: parent.verticalCenter; width: 36; height: 52; radius: 5; url: modelData.poster ? modelData.poster : "" }
                            Column {
                                anchors { left: dpic.right; leftMargin: 10; right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                                Text { width: parent.width; text: modelData.show; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family; elide: Text.ElideRight }
                                Text { width: parent.width; text: modelData.label; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family; elide: Text.ElideRight }
                            }
                            MouseArea { id: dayRow; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { dayPop.close(); page.openShow(modelData.showId) } }
                        }
                    }
                }
            }
        }
        }
    }

    // Asked again when it is shown and has not been loaded for a while (as the GTK page did).
    property double loadedAt: 0
    onVisibleChanged: if (visible && (Date.now() - loadedAt > 1800 * 1000 || entries.length === 0)) { loadedAt = Date.now(); Core.cmd({cmd: "load_calendar"}) }
    // built when first shown (a Loader in Main): visible from the start, so the first load is asked for here
    Component.onCompleted: if (visible) { loadedAt = Date.now(); Core.cmd({cmd: "load_calendar"}) }

    Column {
        anchors.centerIn: parent
        visible: page.cal.hasSeries === false
        spacing: 8
        Icon { anchors.horizontalCenter: parent.horizontalCenter; name: "calendar"; size: 64; color: Theme.c.textDim }
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: Core.tr(Core.uiLang, "No series in your library"); color: Theme.c.text; font.pixelSize: 22; font.weight: Font.ExtraBold; font.family: Theme.type.family }
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: Core.tr(Core.uiLang, "Add series to your library to see when new episodes come out."); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
    }

    Flickable {
        id: scroll
        visible: page.cal.hasSeries !== false
        anchors.fill: parent
        contentWidth: width
        contentHeight: col.implicitHeight + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }

        Column {
            id: col
            x: Math.max(Theme.mobile ? 16 : Theme.spacing.page, (scroll.width - 1100) / 2)
            y: 14
            width: Math.min(1100, scroll.width - 2 * (Theme.mobile ? 16 : Theme.spacing.page))
            spacing: 10

            Item {
                width: parent.width
                height: 40
                FlatButton { id: prevBtn; anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; kind: "circular"; glyph: "left"; implicitWidth: 36; implicitHeight: 36; background: Rectangle { radius: 18; color: prevBtn.hovered ? Theme.c.hover : "transparent" } onClicked: page.step(-1) }
                Row {
                    anchors.centerIn: parent
                    spacing: 10
                    Text { text: Core.tr(Core.uiLang, page.monthNames[page.month - 1]) + " " + page.year; color: Theme.c.text; font.pixelSize: Theme.type.rowTitle.size; font.weight: Theme.type.rowTitle.weight || Font.ExtraBold; font.letterSpacing: Theme.type.rowTitle.letterSpacing || 0; font.family: Theme.type.display || Theme.type.family }
                    BusyIndicator { running: page.cal.loading === true; visible: running; implicitWidth: 24; implicitHeight: 24; anchors.verticalCenter: parent.verticalCenter }
                }
                FlatButton { id: nextBtn; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; kind: "circular"; glyph: "right"; implicitWidth: 36; implicitHeight: 36; background: Rectangle { radius: 18; color: nextBtn.hovered ? Theme.c.hover : "transparent" } onClicked: page.step(1) }
            }
            Text { visible: !!page.cal.info; text: page.cal.info ? page.cal.info : ""; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }

            // the month
            Grid {
                id: grid
                columns: 7
                rowSpacing: 6
                columnSpacing: 6
                readonly property real cellW: (col.width - 6 * 6) / 7
                Repeater {
                    model: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map(function(d) { return Core.tr(Core.uiLang, d) })
                    delegate: Text { required property var modelData; width: grid.cellW; horizontalAlignment: Text.AlignHCenter; text: modelData; color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family }
                }
                Repeater {
                    model: page.firstWeekday + page.days
                    delegate: Item {
                        id: cell
                        required property int index
                        readonly property int day: index - page.firstWeekday + 1
                        readonly property string date: page.iso(page.year, page.month, day)
                        readonly property var eps: day >= 1 ? page.onDay(date) : []
                        width: grid.cellW
                        height: 92
                        Rectangle {
                            visible: cell.day >= 1
                            anchors.fill: parent
                            radius: Theme.shape.small
                            color: dayArea.containsMouse ? Theme.c.selected : Theme.c.fill3
                            border.width: cell.date === page.todayIso ? 2 : 0
                            border.color: Theme.c.accent
                            MouseArea {
                                id: dayArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: page.showDay(cell.date, cell.eps, dayArea)
                            }
                            Column {
                                x: 6
                                y: 6
                                width: parent.width - 12
                                spacing: 3
                                Text { text: "" + cell.day; color: Theme.c.text; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family }
                                Row {
                                    spacing: 3
                                    Repeater {
                                        model: cell.eps.slice(0, 3)
                                        delegate: RoundedPicture {
                                            required property var modelData
                                            width: 30
                                            height: 44
                                            radius: 4
                                            url: modelData.poster ? modelData.poster : ""
                                            ToolTip.visible: tipArea.containsMouse
                                            ToolTip.text: modelData.show + " " + modelData.label
                                            MouseArea { id: tipArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: page.openShow(modelData.showId) }
                                        }
                                    }
                                }
                                Text { visible: cell.eps.length > 3; text: "+" + (cell.eps.length - 3); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                            }
                        }
                    }
                }
            }

            Text { topPadding: 10; text: Core.tr(Core.uiLang, "Coming up"); color: Theme.c.text; font.pixelSize: Theme.type.rowTitle.size; font.weight: Theme.type.rowTitle.weight || Font.ExtraBold; font.letterSpacing: Theme.type.rowTitle.letterSpacing || 0; font.family: Theme.type.display || Theme.type.family }
            Text {
                visible: page.upcoming.length === 0
                text: page.cal.loading === true ? Core.tr(Core.uiLang, "Looking up release dates…") : Core.tr(Core.uiLang, "No upcoming episodes were announced for your series.")
                color: Theme.c.textDim
                font.pixelSize: 15
                font.family: Theme.type.family
            }
            Rectangle {
                visible: page.upcoming.length > 0
                width: parent.width
                height: upCol.implicitHeight
                radius: Theme.shape.card
                color: Theme.c.card
                border.width: 1
                border.color: Theme.c.divider
                clip: true
                Column {
                    id: upCol
                    width: parent.width
                    Repeater {
                        model: page.upcoming
                        delegate: Rectangle {
                            activeFocusOnTab: true
                            Keys.onReturnPressed: upArea.clicked(null)
                            Keys.onSpacePressed: upArea.clicked(null)
                            border.width: activeFocus ? 3 : 0
                            border.color: Theme.c.accent
                            required property var modelData
                            required property int index
                            width: upCol.width
                            height: 66
                            color: upArea.containsMouse ? Theme.c.cardHover : "transparent"
                            Rectangle { visible: index > 0; width: parent.width; height: 1; color: Theme.c.divider }
                            RoundedPicture { id: upPic; x: 12; anchors.verticalCenter: parent.verticalCenter; width: 34; height: 50; radius: 5; url: modelData.poster ? modelData.poster : "" }
                            Column {
                                anchors { left: upPic.right; leftMargin: 12; right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                                spacing: 2
                                Text { width: parent.width; text: modelData.show; color: Theme.c.text; font.pixelSize: 15; font.weight: Font.DemiBold; font.family: Theme.type.family; elide: Text.ElideRight }
                                Text { width: parent.width; text: modelData.date + "   " + (modelData.title ? modelData.label + " · " + modelData.title : modelData.label); color: Theme.c.textDim; font.pixelSize: 13; font.family: Theme.type.family; elide: Text.ElideRight }
                            }
                            MouseArea { id: upArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: page.openShow(modelData.showId) }
                        }
                    }
                }
            }
        }
    }
}
