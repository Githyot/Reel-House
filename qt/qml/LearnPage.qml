import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The Learn area of the Study mode (in the way of Duolingo): the course as a winding path of lessons grouped in
// units, the day's numbers on top (streak, gems, hearts), and the side panel (daily goal, quests, shop…). For
// Japanese a second view, Characters, holds the hiragana and katakana charts (KanaChart).
// A lesson in progress (LearnLesson) covers the page.
// Layout: a fixed bar on top, then the columns below it, each scrolling on its own; nothing goes under the bar.
// Narrow windows (under 880 px) show one column at a time with tabs.
Item {
    id: lp
    property var ln: ({})
    signal switchTo(string view)
    function tr(s) { return Core.tr(Core.uiLang, s) }
    // one of the macaw's funny lines (src/mascot_lines.rs), random and never the last one
    function quip(m) { const r = Core.state("mascot:" + Core.uiLang + ":" + m); return r && r.text ? r.text : "" }
    // the macaw's line beside the current lesson: a new one each time the page shows or a lesson ends
    property string cheerQuip: ""
    // every command carries the time zone, so a day starts at the user's midnight
    function cmd(o) { o.tz = -(new Date().getTimezoneOffset()); Core.cmd(o); refresh() }
    function refresh() {
        ln = Core.state("learn")
        // the study language changed elsewhere (Preferences): read its course
        if (visible && (ln.course || "") !== "" && ln.ready !== true && ln.loading !== true && (ln.error || "") === "") Qt.callLater(lp.cmd, {cmd: "learn_open"})
    }
    onVisibleChanged: if (visible) { cmd({cmd: "learn_open"}); cheerQuip = quip("learn_cheer") }
    Component.onCompleted: { if (visible) cmd({cmd: "learn_open"}); cheerQuip = quip("learn_cheer") }
    onInLessonChanged: if (!inLesson) cheerQuip = quip("learn_cheer")
    Connections { target: Core; function onStudyChanged() { if (lp.visible) lp.refresh() } }
    // the heart clock, the boost clock and the day change while the page is open
    Timer { interval: 30000; running: lp.visible; repeat: true; onTriggered: lp.refresh() }

    readonly property var session: ln.session !== undefined && ln.session !== null ? ln.session : null
    readonly property bool inLesson: session !== null
    readonly property var units: ln.units !== undefined ? ln.units : []
    readonly property bool hasKana: ln.kana !== undefined && ln.kana !== null
    readonly property bool playful: Theme.look === "android" || Theme.look === "apple"
    readonly property real rad: playful ? 18 : Math.max(6, Theme.shape.card)
    readonly property bool wide: width >= 880
    // path | chars (the Characters view, Japanese only)
    property string view: "path"
    readonly property string shownView: view === "chars" && hasKana ? "chars" : "path"
    // narrow windows: which column is shown (path | chars | progress)
    property string narrowTab: "path"
    readonly property real cw: Math.min(wide ? (width >= 1700 ? 1320 : 1120) : 680, width - Theme.spacing.page * 2)
    readonly property real x0: (width - cw) / 2
    // the colours of the game, the same in every theme so they mean the same thing
    readonly property color cFlame: "#ff9600"
    readonly property color cGem: Theme.scheme === "dark" ? "#49c0f8" : "#1899d6"
    readonly property color cHeart: "#ff4b4b"
    readonly property color cIce: "#84d8ff"
    readonly property color cGold: "#ffc800"
    readonly property color cLegend: "#a560f0"
    // one colour per unit, from a fixed set of hues; GNOME and Windows get them a little calmer
    function unitColor(i) {
        const hues = [0.33, 0.76, 0.56, 0.07, 0.9, 0.47, 0.12, 0.62]
        return Qt.hsla(hues[i % hues.length], playful ? 0.62 : 0.48, Theme.scheme === "dark" ? 0.42 : 0.45, 1)
    }
    // what the macaw says beside the current lesson (a different line each day)
    function cheerLine() {
        if (ln.chest_ready === true) return tr("Daily goal done! Open your chest.")
        if ((ln.today_xp || 0) === 0 && (ln.streak || 0) > 0 && ln.streak_today !== true) return tr("Keep your streak alive!")
        const lines = [tr("Let's learn something new today!"), tr("You can do it!"), tr("A few minutes a day makes a big difference."), tr("Ready for your next lesson?"), tr("Practice makes perfect!"), tr("I believe in you!")]
        if (cheerQuip !== "") return cheerQuip
        const day = Math.floor(Date.now() / 86400000)
        return lines[day % lines.length]
    }

    // ---------- the bar on top: Decks | Learn, Path | Characters, and the numbers of the day ----------
    Item {
        id: topBar
        visible: !lp.inLesson
        x: lp.x0; y: 12
        width: lp.cw; height: 52
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12
            // (Decks / Learn / Time are in the app's left sidebar)
            Segmented {
                visible: lp.wide && lp.hasKana
                model: [{id: "path", label: lp.tr("Path")}, {id: "chars", label: lp.tr("Characters")}]
                current: lp.shownView
                onPicked: function(id) { lp.view = id }
            }
        }
        Row {
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: lp.width < 520 ? 2 : 8
            StatChip {
                id: courseChip
                visible: lp.ln.course !== undefined && lp.ln.course !== ""
                badge: (lp.ln.course2 || "").toUpperCase()
                label: lp.width < 620 ? "" : (lp.ln.course_name || "")
                tip: lp.tr("Change the course")
                onActivated: coursePop.openBelow(courseChip, true)
            }
            StatChip {
                visible: (lp.ln.boost_left || 0) > 0
                glyph: "rocket"; tone: lp.cLegend; filled: true
                label: Math.ceil((lp.ln.boost_left || 0) / 60) + " min"
                tip: lp.tr("XP boost: double XP")
            }
            StatChip {
                glyph: "flame"; tone: lp.ln.streak_today === true ? lp.cFlame : Theme.c.textDim; filled: lp.ln.streak_today === true
                label: String(lp.ln.streak || 0)
                tip: lp.ln.streak_today === true ? lp.tr("Streak extended today") : lp.tr("Do a lesson today to keep your streak")
                onActivated: if (!lp.wide) lp.narrowTab = "progress"
            }
            StatChip {
                glyph: "gem"; tone: lp.cGem; filled: true
                label: String(lp.ln.gems || 0)
                tip: lp.tr("Gems: spend them in the shop")
                onActivated: if (!lp.wide) lp.narrowTab = "progress"
            }
            StatChip {
                glyph: "heart"; tone: lp.cHeart; filled: true
                label: lp.ln.unlimited === true ? "∞" : String(lp.ln.hearts !== undefined ? lp.ln.hearts : 5)
                tip: lp.ln.unlimited === true ? lp.tr("Unlimited hearts") : ((lp.ln.next_heart || 0) > 0 ? lp.tr("Next heart in") + " " + lp.clock(lp.ln.next_heart) : lp.tr("Your hearts are full"))
                onActivated: if (!lp.wide) lp.narrowTab = "progress"
            }
        }
    }
    function clock(secs) {
        const m = Math.ceil(secs / 60)
        return m >= 60 ? Math.floor(m / 60) + " h " + (m % 60) + " min" : m + " min"
    }

    // a number of the day with its picture (streak, gems, hearts) or the course badge
    component StatChip: Rectangle {
        id: sc
        property string glyph: ""
        property string badge: ""
        property string label: ""
        property string tip: ""
        property color tone: Theme.c.text
        property bool filled: false
        signal activated()
        width: scRow.implicitWidth + 18; height: 36
        radius: lp.playful ? 18 : Theme.shape.control
        color: scArea.containsMouse ? Theme.c.hover : "transparent"
        Row {
            id: scRow
            anchors.centerIn: parent
            spacing: 6
            Rectangle {
                visible: sc.badge !== ""
                width: 30; height: 22; radius: 6
                color: Theme.c.accent
                anchors.verticalCenter: parent.verticalCenter
                Text { anchors.centerIn: parent; text: sc.badge; color: Theme.c.accentFg; font.pixelSize: 12; font.weight: Font.Black; font.family: Theme.type.family }
            }
            LearnGlyph { visible: sc.glyph !== ""; name: sc.glyph; size: 20; color: sc.tone; filled: sc.filled; fillColor: Qt.rgba(sc.tone.r, sc.tone.g, sc.tone.b, 0.35); anchors.verticalCenter: parent.verticalCenter }
            Text { visible: sc.label !== ""; text: sc.label; color: sc.glyph !== "" ? sc.tone : Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
        }
        MouseArea { id: scArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: sc.activated() }
        ToolTip.visible: scArea.containsMouse && sc.tip !== ""; ToolTip.delay: 400; ToolTip.text: sc.tip
    }

    // ---------- choosing the course ----------
    Popover {
        id: coursePop
        contentItem: Column {
            width: 280
            spacing: 4
            Lbl { width: parent.width; text: lp.tr("I want to learn"); bold: true; leftPadding: 8; topPadding: 4; bottomPadding: 4 }
            Repeater {
                model: lp.ln.courses !== undefined ? lp.ln.courses.length : 0
                delegate: Rectangle {
                    required property int index
                    readonly property var c: lp.ln.courses[index]
                    width: parent.width; height: 40; radius: Theme.shape.control
                    color: cArea.containsMouse ? Theme.c.hover : (c.lang === lp.ln.course ? Theme.c.selected : "transparent")
                    Lbl { anchors { left: parent.left; leftMargin: 10; verticalCenter: parent.verticalCenter } text: c.name }
                    LearnGlyph { visible: c.lang === lp.ln.course; name: "check"; size: 16; color: Theme.c.accent; anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter } }
                    MouseArea { id: cArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { coursePop.close(); lp.cmd({cmd: "learn_set_course", lang: c.lang}) } }
                }
            }
            Lbl { width: parent.width; wrapMode: Text.Wrap; dim: true; size: 12; leftPadding: 8; rightPadding: 8; topPadding: 4; text: lp.tr("Lessons are translated into the first language you know (Preferences → languages).") }
        }
    }

    // ---------- narrow windows: the path, the characters and the side panel are tabs ----------
    Segmented {
        id: narrowTabs
        visible: !lp.wide && !lp.inLesson
        anchors { horizontalCenter: parent.horizontalCenter; top: topBar.bottom; topMargin: 4 }
        model: lp.hasKana ? [{id: "path", label: lp.tr("Path")}, {id: "chars", label: lp.tr("Characters")}, {id: "progress", label: lp.tr("Progress")}]
                          : [{id: "path", label: lp.tr("Path")}, {id: "progress", label: lp.tr("Progress")}]
        current: lp.narrowTab === "chars" && !lp.hasKana ? "path" : lp.narrowTab
        onPicked: function(id) { lp.narrowTab = id; if (id !== "progress") lp.view = id }
    }

    // where the columns start: under the bar (and the tabs), never under them
    readonly property real contentTop: topBar.y + topBar.height + (lp.wide ? 8 : narrowTabs.height + 14)
    readonly property real sideW: lp.wide ? (lp.cw >= 1200 ? 380 : 340) : lp.cw
    readonly property real pathW: lp.wide ? lp.cw - sideW - 32 : lp.cw
    readonly property bool showPath: !lp.inLesson && lp.shownView === "path" && (lp.wide || lp.narrowTab !== "progress")
    readonly property bool showChars: !lp.inLesson && lp.shownView === "chars" && (lp.wide || lp.narrowTab !== "progress")
    // the room between two lessons of the path follows the height of the window (as on Duolingo, generous)
    readonly property real step: Math.round(Math.max(108, Math.min(150, (lp.height - lp.contentTop) / 6)))

    // ---------- the path ----------
    Flickable {
        id: pathFl
        visible: lp.showPath
        x: lp.x0; y: lp.contentTop
        width: lp.pathW; height: lp.height - y
        contentWidth: width
        // room above the first unit and below the last (a spacer, so it shows from the start)
        contentHeight: pathCol.y + pathCol.height + 40
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        // the current lesson comes into view when the course is opened
        property bool scrolled: false
        function toCurrent() {
            for (let u = 0; u < unitRep.count; ++u) {
                const it = unitRep.itemAt(u)
                if (it && it.currentY >= 0) {
                    contentY = Math.max(0, Math.min(pathCol.y + it.y + it.currentY - height / 2 + 60, contentHeight - height))
                    scrolled = true
                    return
                }
            }
        }

        Column {
            id: pathCol
            y: 10
            width: parent.width
            spacing: 28

            // nothing to learn yet: pick a course
            Rectangle {
                visible: lp.ln.ready !== true
                width: parent.width
                height: emptyCol.implicitHeight + 48
                radius: lp.rad
                color: Theme.c.card
                Column {
                    id: emptyCol
                    x: 24; y: 24
                    width: parent.width - 48
                    spacing: 14
                    Mascot { mood: lp.ln.loading === true ? "think" : "wave"; size: 110; anchors.horizontalCenter: parent.horizontalCenter }
                    Lbl {
                        width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; bold: true; size: 20
                        text: lp.ln.loading === true ? lp.tr("Loading the course…") : (lp.ln.course === "" || lp.ln.course === undefined ? lp.tr("What do you want to learn?") : lp.tr("This course could not be opened"))
                    }
                    Lbl {
                        visible: text !== ""
                        width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; dim: true
                        text: lp.ln.loading === true ? "" : ((lp.ln.error || "") !== "" ? lp.ln.error : ((lp.ln.courses || []).length === 0 ? lp.tr("No course files were found. They come with the app in the learn folder (learn/*.json).") : lp.tr("Short daily lessons with sentences of real people (Tatoeba), words and listening.")))
                    }
                    BusyIndicator { visible: lp.ln.loading === true; running: visible; anchors.horizontalCenter: parent.horizontalCenter }
                    Flow {
                        visible: lp.ln.loading !== true
                        width: parent.width
                        spacing: 10
                        Repeater {
                            model: lp.ln.courses !== undefined ? lp.ln.courses.length : 0
                            delegate: LearnButton {
                                required property int index
                                outline: true
                                text: lp.ln.courses[index].name
                                onClicked: lp.cmd({cmd: "learn_set_course", lang: lp.ln.courses[index].lang})
                            }
                        }
                    }
                }
            }

            Repeater {
                id: unitRep
                model: lp.units.length
                delegate: Item {
                    id: unitItem
                    required property int index
                    readonly property var u: lp.units[index] || ({})
                    readonly property var lessons: u.lessons || []
                    readonly property color tone: lp.unitColor(index)
                    readonly property int level: u.level || 0
                    readonly property real step: lp.step
                    readonly property real pathTop: head.height + 56
                    // y of the current lesson inside this unit (-1 when it is not here)
                    readonly property int currentIndex: {
                        for (let i = 0; i < lessons.length; ++i) if (lessons[i].state === "current") return i
                        return -1
                    }
                    readonly property real currentY: currentIndex >= 0 ? pathTop + currentIndex * step : -1
                    readonly property var wave: [0, 0.55, 1, 0.55, 0, -0.55, -1, -0.55]
                    readonly property real amp: Math.min(110, pathCol.width * 0.2)
                    function nodeX(i, d) { return (pathCol.width - d) / 2 + wave[i % wave.length] * amp * (index % 2 === 0 ? 1 : -1) }
                    width: pathCol.width
                    height: pathTop + Math.max(0, lessons.length - 1) * step + 96

                    // the coloured header of the unit, with its level badge and the legendary round
                    Rectangle {
                        id: head
                        width: parent.width
                        height: Math.max(116, Math.max(headCol.implicitHeight, badge.height) + 40)
                        radius: lp.rad
                        color: unitItem.tone
                        Rectangle { visible: lp.playful; anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 4; radius: lp.rad; color: Qt.darker(unitItem.tone, 1.3); z: -1; anchors.bottomMargin: -4 }
                        Column {
                            id: headCol
                            anchors { left: parent.left; leftMargin: 22; right: badge.left; rightMargin: 14; verticalCenter: parent.verticalCenter }
                            spacing: 4
                            Text { text: lp.tr("Unit") + " " + (unitItem.index + 1); color: "#ddffffff"; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family; font.capitalization: lp.playful ? Font.AllUppercase : Font.MixedCase }
                            Text { width: parent.width; wrapMode: Text.Wrap; text: unitItem.u.title || ""; color: "#ffffff"; font.pixelSize: 22; font.weight: Font.Bold; font.family: Theme.type.display || Theme.type.family }
                            Text { visible: text !== "" && text !== unitItem.u.title; width: parent.width; elide: Text.ElideRight; text: unitItem.u.subtitle || ""; color: "#ccffffff"; font.pixelSize: 14; font.family: Theme.type.family }
                            // a finished unit can be played again as a legendary round
                            Rectangle {
                                visible: unitItem.level >= 1
                                width: legRow.implicitWidth + 22; height: 32
                                radius: lp.playful ? 16 : Theme.shape.control
                                color: legArea.containsMouse ? "#ffffff" : "#e6ffffff"
                                Row {
                                    id: legRow
                                    anchors.centerIn: parent
                                    spacing: 6
                                    LearnGlyph { name: "crown"; size: 16; color: lp.cLegend; filled: true; fillColor: Qt.rgba(0.65, 0.38, 0.94, 0.3); anchors.verticalCenter: parent.verticalCenter }
                                    Text { text: (unitItem.level >= 2 ? lp.tr("Replay legendary") : lp.tr("Go legendary")) + "  +" + ((lp.ln.gem_rules || {}).xp_legendary || 40) + " XP"; color: lp.cLegend; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
                                }
                                MouseArea { id: legArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: lp.cmd({cmd: "learn_start", lesson: "legendary-" + unitItem.u.id}) }
                                ToolTip.visible: legArea.containsMouse; ToolTip.delay: 500; ToolTip.text: lp.tr("Every word and sentence of the unit in one harder round")
                            }
                        }
                        // the level badge: progress, a crown when finished, a purple crown when legendary
                        Column {
                            id: badge
                            anchors { right: parent.right; rightMargin: 20; verticalCenter: parent.verticalCenter }
                            spacing: 4
                            Rectangle {
                                width: 52; height: 52; radius: 26
                                anchors.horizontalCenter: parent.horizontalCenter
                                color: unitItem.level === 2 ? lp.cLegend : (unitItem.level === 1 ? lp.cGold : "#33ffffff")
                                border.width: 3; border.color: "#ffffff"
                                LearnGlyph { anchors.centerIn: parent; name: unitItem.level >= 1 ? "crown" : "book"; size: 26; color: "#ffffff"; filled: unitItem.level >= 1; fillColor: "#55ffffff" }
                            }
                            Text {
                                anchors.horizontalCenter: parent.horizontalCenter
                                text: unitItem.level === 2 ? lp.tr("Legendary") : (unitItem.level === 1 ? lp.tr("Complete") : (unitItem.u.done || 0) + "/" + (unitItem.u.total || 0))
                                color: "#ffffff"; font.pixelSize: 12; font.weight: Font.Bold; font.family: Theme.type.family
                            }
                        }
                    }

                    // the macaw beside the current lesson, on the other side of the path, cheering
                    Mascot {
                        visible: unitItem.currentIndex >= 0 && lp.visible
                        readonly property real nx: unitItem.currentIndex >= 0 ? unitItem.nodeX(unitItem.currentIndex, 80) : 0
                        readonly property bool onRight: nx < (pathCol.width - 80) / 2 + 1
                        mood: lp.ln.chest_ready === true ? "cheer" : ((lp.ln.hearts || 0) === 0 && lp.ln.unlimited !== true ? "tired" : "wave")
                        size: Math.min(110, Math.max(80, pathCol.width * 0.17))
                        say: pathCol.width >= 420 ? lp.cheerLine() : ""
                        bubbleSide: "top"
                        x: onRight ? Math.min(pathCol.width - width - 4, pathCol.width / 2 + unitItem.amp * 0.6 + 30) : Math.max(4, pathCol.width / 2 - unitItem.amp * 0.6 - 30 - width)
                        y: unitItem.currentY - 8
                        z: 2
                    }

                    // the lessons, zig-zagging down (each unit starts to the other side)
                    Repeater {
                        model: unitItem.lessons.length
                        delegate: Item {
                            id: node
                            required property int index
                            readonly property var les: unitItem.lessons[index] || ({})
                            readonly property string st: les.state || "locked"
                            readonly property bool last: index === unitItem.lessons.length - 1
                            readonly property real d: 80
                            readonly property int depth: lp.playful ? 8 : 0
                            width: d; height: d + depth
                            x: unitItem.nodeX(index, d)
                            y: unitItem.pathTop + index * unitItem.step
                            readonly property color face: st === "locked" ? Theme.c.fill : (st === "done" && last ? (unitItem.level === 2 ? lp.cLegend : lp.cGold) : unitItem.tone)

                            // the ring that breathes around the current lesson
                            Rectangle {
                                visible: node.st === "current"
                                anchors.centerIn: parent
                                width: node.d + 22; height: width; radius: width / 2
                                color: "transparent"
                                border.width: 6
                                border.color: Qt.rgba(unitItem.tone.r, unitItem.tone.g, unitItem.tone.b, 0.35)
                                SequentialAnimation on scale {
                                    running: node.st === "current" && lp.visible
                                    loops: Animation.Infinite
                                    NumberAnimation { from: 1; to: 1.08; duration: 900; easing.type: Easing.InOutSine }
                                    NumberAnimation { from: 1.08; to: 1; duration: 900; easing.type: Easing.InOutSine }
                                }
                            }
                            Rectangle { visible: node.depth > 0; y: node.depth; width: node.d; height: node.d; radius: node.d / 2; color: node.st === "locked" ? Theme.c.border : Qt.darker(node.face, 1.3) }
                            Rectangle {
                                id: disc
                                width: node.d; height: node.d; radius: node.d / 2
                                y: nodeArea.pressed && node.st !== "locked" ? node.depth : 0
                                color: nodeArea.containsMouse && node.st !== "locked" ? Qt.lighter(node.face, 1.08) : node.face
                                border.width: lp.playful ? 0 : 1
                                border.color: Qt.rgba(0, 0, 0, 0.12)
                                LearnGlyph {
                                    anchors.centerIn: parent
                                    size: 32
                                    stroke: 2.4
                                    name: node.st === "locked" ? "lock" : (node.last ? "trophy" : (node.st === "done" ? "check" : "star"))
                                    color: node.st === "locked" ? Theme.c.textDim : "#ffffff"
                                    filled: node.st === "current" && !node.last
                                    fillColor: "#ffffff"
                                }
                            }
                            // "Begin" above the current lesson
                            Rectangle {
                                visible: node.st === "current"
                                anchors.horizontalCenter: parent.horizontalCenter
                                y: -42
                                width: startTxt.implicitWidth + 24; height: 30; radius: lp.playful ? 10 : Theme.shape.control
                                color: Theme.c.card
                                border.width: 2; border.color: Theme.c.border
                                Text { id: startTxt; anchors.centerIn: parent; text: lp.tr("Begin"); color: unitItem.tone; font.pixelSize: 14; font.weight: Font.Black; font.family: Theme.type.family; font.capitalization: lp.playful ? Font.AllUppercase : Font.MixedCase }
                                SequentialAnimation on y {
                                    running: node.st === "current" && lp.visible
                                    loops: Animation.Infinite
                                    NumberAnimation { from: -42; to: -48; duration: 700; easing.type: Easing.InOutSine }
                                    NumberAnimation { from: -48; to: -42; duration: 700; easing.type: Easing.InOutSine }
                                }
                            }
                            MouseArea {
                                id: nodeArea
                                width: node.d; height: node.d
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: lessonPop.show(node, unitItem.u, node.les, unitItem.tone, node.index, unitItem.lessons.length)
                            }
                        }
                    }
                }
            }

            Lbl {
                visible: (lp.ln.credits || "") !== ""
                width: parent.width
                wrapMode: Text.Wrap
                dim: true; size: 11
                horizontalAlignment: Text.AlignHCenter
                text: lp.ln.credits || ""
            }
        }
    }
    // scroll to the current lesson once the course is there
    Timer { interval: 150; running: lp.visible && lp.showPath && !pathFl.scrolled && lp.units.length > 0; onTriggered: pathFl.toCurrent() }

    // ---------- the characters (Japanese) ----------
    KanaChart {
        visible: lp.showChars
        x: lp.x0; y: lp.contentTop
        width: lp.pathW; height: lp.height - y
        kana: lp.hasKana ? lp.ln.kana : null
        rad: lp.rad
        onCommand: function(o) { lp.cmd(o) }
    }

    // ---------- the popup of a lesson ----------
    Popover {
        id: lessonPop
        property var unit: ({})
        property var les: ({})
        property color tone: Theme.c.accent
        property int n: 0
        property int total: 0
        function show(item, unit, les, tone, i, total) {
            lessonPop.unit = unit; lessonPop.les = les; lessonPop.tone = tone; lessonPop.n = i + 1; lessonPop.total = total
            openAt(item, item.width / 2 - 150, item.height + 8)
        }
        background: Rectangle { radius: lp.rad; color: lessonPop.les.state === "locked" ? Theme.c.popover : lessonPop.tone; border.width: lessonPop.les.state === "locked" ? 1 : 0; border.color: Theme.c.border }
        padding: 18
        contentItem: Column {
            width: 264
            spacing: 10
            readonly property color fg: lessonPop.les.state === "locked" ? Theme.c.text : "#ffffff"
            Text { width: parent.width; wrapMode: Text.Wrap; text: lessonPop.unit.title || ""; color: parent.fg; font.pixelSize: 17; font.weight: Font.Bold; font.family: Theme.type.family }
            Text { width: parent.width; text: lp.tr("Lesson %1 of %2").arg(lessonPop.n).arg(lessonPop.total); color: parent.fg; opacity: 0.85; font.pixelSize: 14; font.family: Theme.type.family }
            Text {
                visible: (lessonPop.les.words || []).length > 0 && lessonPop.les.state !== "locked"
                width: parent.width; wrapMode: Text.Wrap
                text: (lessonPop.les.words || []).join("・")
                color: parent.fg; font.pixelSize: 16; font.family: Theme.type.family
            }
            Text {
                visible: lessonPop.les.state === "locked"
                width: parent.width; wrapMode: Text.Wrap
                text: lp.tr("Complete all the lessons above to unlock this one.")
                color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family
            }
            LearnButton {
                visible: lessonPop.les.state !== "locked"
                width: parent.width
                tone: "#ffffff"; fg: lessonPop.tone
                text: lessonPop.les.state === "done" ? lp.tr("Practise again") + "  +10 XP" : lp.tr("Start lesson") + "  +10 XP"
                onClicked: { lessonPop.close(); lp.cmd({cmd: "learn_start", lesson: lessonPop.les.id}) }
            }
        }
    }

    // ---------- the side panel (scrolls on its own when taller than the window) ----------
    Flickable {
        visible: !lp.inLesson && (lp.wide || lp.narrowTab === "progress")
        x: lp.wide ? lp.x0 + lp.cw - lp.sideW : lp.x0
        y: lp.contentTop
        width: lp.sideW; height: lp.height - y
        contentWidth: width
        // room above the first card (its border and shadow) and below the last
        contentHeight: side.y + side.implicitHeight + 32
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        LearnSide {
            id: side
            y: 10
            width: parent.width
            ln: lp.ln
            onCommand: function(o) { lp.cmd(o) }
        }
    }

    // ---------- the lesson ----------
    LearnLesson {
        anchors.fill: parent
        visible: lp.inLesson
        s: lp.session
        ln: lp.ln
        pageVisible: lp.visible
        onCommand: function(o) { lp.cmd(o) }
    }
}
