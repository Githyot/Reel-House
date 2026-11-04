import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import Reelhouse 1.0

// The side panel of the Learn area: practice, the daily goal, the streak calendar, the quests of the day, the XP of
// the week compared with the user's own past weeks (no invented players), the shop and the achievements.
Column {
    id: sd
    property var ln: ({})
    signal command(var o)
    function tr(s) { return Core.tr(Core.uiLang, s) }
    spacing: 14
    readonly property bool playful: Theme.look === "android" || Theme.look === "apple"
    readonly property real rad: playful ? 18 : Math.max(6, Theme.shape.card)
    readonly property color cFlame: "#ff9600"
    readonly property color cGem: Theme.scheme === "dark" ? "#49c0f8" : "#1899d6"
    readonly property color cHeart: "#ff4b4b"
    readonly property color cIce: Theme.scheme === "dark" ? "#5fb8e0" : "#84d8ff"
    readonly property color cGold: "#ffc800"
    readonly property color cLegend: "#a560f0"
    readonly property var gemRules: ln.gem_rules !== undefined ? ln.gem_rules : ({})
    readonly property var quests: ln.quests !== undefined ? ln.quests : []
    readonly property var achievements: ln.achievements !== undefined ? ln.achievements : []
    readonly property var calendar: ln.calendar !== undefined ? ln.calendar : []
    readonly property var weekDays: ln.week_days !== undefined ? ln.week_days : []
    readonly property var weeks: ln.weeks !== undefined ? ln.weeks : []
    function dayLetter(i) { return Qt.locale(Core.uiLang).dayName((i + 1) % 7, Locale.NarrowFormat) }

    component Card: Rectangle {
        default property alias body: inner.data
        property alias spacing: inner.spacing
        width: sd.width
        height: inner.implicitHeight + 32
        radius: sd.rad
        color: Theme.c.card
        border.width: sd.playful ? 2 : 1
        border.color: Theme.c.border
        Column { id: inner; x: 16; y: 16; width: parent.width - 32; spacing: 10 }
    }
    component Bar: Rectangle {
        property real value: 0
        property color tone: Theme.c.accent
        height: 10; radius: 5
        color: Theme.c.fill
        Rectangle { width: parent.width * Math.max(0, Math.min(1, parent.value)); height: parent.height; radius: parent.radius; color: parent.tone; Behavior on width { NumberAnimation { duration: Theme.motion.normal } } }
    }

    // ---------- practice ----------
    Row {
        width: sd.width
        spacing: 10
        LearnButton {
            width: (parent.width - 10) / 2
            outline: true; glyph: "dumbbell"
            text: sd.tr("Practice")
            enabled: sd.ln.can_practice === true
            onClicked: sd.command({cmd: "learn_practice", mode: "weak"})
        }
        LearnButton {
            width: (parent.width - 10) / 2
            outline: true; glyph: "rotate"
            text: sd.tr("Mistakes") + ((sd.ln.mistakes || 0) > 0 ? " (" + sd.ln.mistakes + ")" : "")
            enabled: (sd.ln.mistakes || 0) > 0
            onClicked: sd.command({cmd: "learn_practice", mode: "mistakes"})
        }
    }

    // ---------- daily goal ----------
    Card {
        Row {
            spacing: 16
            // the ring of the day's XP
            Item {
                id: ring
                width: 76; height: 76
                readonly property real frac: Math.min(1, (sd.ln.today_xp || 0) / Math.max(1, sd.ln.goal || 20))
                Shape {
                    anchors.fill: parent
                    preferredRendererType: Shape.CurveRenderer
                    ShapePath {
                        strokeColor: Theme.c.fill; strokeWidth: 9; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                        PathAngleArc { centerX: 38; centerY: 38; radiusX: 32; radiusY: 32; startAngle: -90; sweepAngle: 360 }
                    }
                    ShapePath {
                        strokeColor: ring.frac >= 1 ? Theme.c.success : sd.cFlame; strokeWidth: 9; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                        PathAngleArc { centerX: 38; centerY: 38; radiusX: 32; radiusY: 32; startAngle: -90; sweepAngle: 360 * Math.max(0.001, ring.frac) }
                    }
                }
                LearnGlyph { anchors.centerIn: parent; name: ring.frac >= 1 ? "check" : "zap"; size: 26; color: ring.frac >= 1 ? Theme.c.successText : sd.cFlame; filled: ring.frac < 1; fillColor: Qt.rgba(1, 0.59, 0, 0.3) }
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                Lbl { text: sd.tr("Daily goal"); bold: true; size: 17 }
                Lbl { text: (sd.ln.today_xp || 0) + " / " + (sd.ln.goal || 20) + " XP"; dim: true }
                Lbl { text: (sd.ln.xp_total || 0) + " XP " + sd.tr("in total"); dim: true; size: 12 }
            }
        }
        Segmented {
            model: (sd.ln.goals || [10, 20, 30, 50]).map(function(g) { return {id: String(g), label: g + " XP"} })
            current: String(sd.ln.goal || 20)
            onPicked: function(id) { sd.command({cmd: "learn_set_goal", xp: parseInt(id)}) }
        }
        // the daily chest: it opens once the goal is reached
        Rectangle {
            width: parent.width
            height: chestRow.implicitHeight + 20
            radius: sd.rad - 4
            color: sd.ln.chest_ready === true ? Qt.rgba(sd.cLegend.r, sd.cLegend.g, sd.cLegend.b, 0.14) : Theme.c.fill
            Row {
                id: chestRow
                x: 12; y: 10
                width: parent.width - 24
                spacing: 12
                LearnGlyph {
                    id: chestGlyph
                    name: "gift"; size: 34
                    color: sd.ln.chest_ready === true ? sd.cLegend : Theme.c.textDim
                    filled: sd.ln.chest_ready === true; fillColor: Qt.rgba(sd.cLegend.r, sd.cLegend.g, sd.cLegend.b, 0.3)
                    anchors.verticalCenter: parent.verticalCenter
                    // a ready chest wiggles
                    SequentialAnimation on rotation {
                        running: sd.ln.chest_ready === true && sd.visible
                        loops: Animation.Infinite
                        NumberAnimation { to: -10; duration: 120 }
                        NumberAnimation { to: 10; duration: 160 }
                        NumberAnimation { to: 0; duration: 120 }
                        PauseAnimation { duration: 1400 }
                    }
                }
                Column {
                    width: parent.width - chestGlyph.width - (openBtn.visible ? openBtn.width + 12 : 0) - 12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Lbl { width: parent.width; text: sd.tr("Daily chest"); bold: true }
                    Lbl {
                        width: parent.width; wrapMode: Text.Wrap; dim: true; size: 12
                        text: sd.ln.chest_opened === true ? sd.tr("Opened. A new chest comes with tomorrow's goal.")
                            : (sd.ln.chest_ready === true ? sd.tr("Your goal is done: open it!") : sd.tr("%1–%2 gems when you reach your daily goal").arg(sd.gemRules.chest_min || 5).arg(sd.gemRules.chest_max || 20))
                    }
                }
                LearnButton {
                    id: openBtn
                    visible: sd.ln.chest_ready === true
                    implicitWidth: 92
                    tone: sd.cLegend; fg: "#ffffff"
                    text: sd.tr("Open")
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: sd.command({cmd: "learn_chest"})
                }
            }
        }
    }

    // ---------- streak ----------
    Card {
        Row {
            spacing: 12
            LearnGlyph { name: "flame"; size: 40; color: sd.ln.streak_today === true ? sd.cFlame : Theme.c.textDim; filled: true; fillColor: sd.ln.streak_today === true ? Qt.rgba(1, 0.59, 0, 0.35) : Theme.c.fill }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Lbl { text: (sd.ln.streak || 0) + " " + ((sd.ln.streak || 0) === 1 ? sd.tr("day streak") : sd.tr("days streak")); bold: true; size: 17 }
                Lbl { text: sd.ln.streak_today === true ? sd.tr("You practised today.") : sd.tr("Do a lesson today to keep it going."); dim: true; size: 13 }
            }
        }
        // five weeks, Monday first: days with XP in the flame colour, days saved by a freeze in ice
        Grid {
            id: cal
            columns: 7
            spacing: 4
            readonly property real cell: Math.floor((parent.width - 6 * 4) / 7)
            Repeater {
                model: 7
                delegate: Text {
                    required property int index
                    width: cal.cell; horizontalAlignment: Text.AlignHCenter
                    text: sd.dayLetter(index); color: Theme.c.textDim; font.pixelSize: 11; font.weight: Font.Bold; font.family: Theme.type.family
                }
            }
            Repeater {
                model: sd.calendar.length
                delegate: Rectangle {
                    required property int index
                    readonly property var d: sd.calendar[index] || ({})
                    width: cal.cell; height: Math.min(cal.cell, 30); radius: height / 2
                    color: (d.xp || 0) > 0 ? sd.cFlame : (d.frozen === true ? sd.cIce : "transparent")
                    border.width: d.today === true ? 2 : 0
                    border.color: Theme.c.accent
                    opacity: d.future === true ? 0.35 : 1
                    Text { anchors.centerIn: parent; text: parent.d.day || ""; color: (parent.d.xp || 0) > 0 || parent.d.frozen === true ? "#ffffff" : Theme.c.text; font.pixelSize: 12; font.weight: parent.d.today === true ? Font.Bold : Font.Normal; font.family: Theme.type.family }
                }
            }
        }
        Row {
            spacing: 6
            LearnGlyph { name: "snowflake"; size: 16; color: sd.cIce; anchors.verticalCenter: parent.verticalCenter }
            Lbl { text: sd.tr("Streak freezes") + ": " + (sd.ln.freezes || 0) + " / " + (sd.ln.max_freezes || 2) + "   ·   " + sd.tr("Longest") + ": " + (sd.ln.longest || 0); dim: true; size: 13; anchors.verticalCenter: parent.verticalCenter }
        }
    }

    // ---------- daily quests ----------
    Card {
        Lbl { text: sd.tr("Daily quests"); bold: true; size: 17 }
        Repeater {
            model: sd.quests.length
            delegate: Item {
                id: qrow
                required property int index
                readonly property var q: sd.quests[index] || ({})
                readonly property var glyphs: ({xp: "zap", lessons: "book", perfect: "target", listen: "volume", combo: "sparkles", goal: "flag", practice: "dumbbell"})
                width: parent.width; height: 52
                LearnGlyph { id: qg; name: qrow.glyphs[qrow.q.kind] || "star"; size: 26; color: qrow.q.done === true ? Theme.c.successText : sd.cFlame; anchors.verticalCenter: parent.verticalCenter }
                Column {
                    anchors { left: qg.right; leftMargin: 12; right: rew.left; rightMargin: 10; verticalCenter: parent.verticalCenter }
                    spacing: 6
                    Lbl { width: parent.width; elide: Text.ElideRight; text: sd.questTitle(qrow.q) }
                    Bar { width: parent.width; value: (qrow.q.progress || 0) / Math.max(1, qrow.q.target || 1); tone: qrow.q.done === true ? Theme.c.success : sd.cFlame }
                }
                Row {
                    id: rew
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    spacing: 4
                    LearnGlyph { name: qrow.q.done === true ? "check" : "gem"; size: 18; color: qrow.q.done === true ? Theme.c.successText : sd.cGem; filled: qrow.q.done !== true; fillColor: Qt.rgba(sd.cGem.r, sd.cGem.g, sd.cGem.b, 0.3) }
                    Text { visible: qrow.q.done !== true; text: qrow.q.reward || 0; color: sd.cGem; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family }
                }
            }
        }
    }
    function questTitle(q) {
        const t = q.target || 1
        switch (q.kind) {
        case "xp": return tr("Earn %1 XP").arg(t)
        case "lessons": return tr("Complete %1 lessons").arg(t)
        case "perfect": return t === 1 ? tr("Finish a lesson with no mistakes") : tr("Finish %1 lessons with no mistakes").arg(t)
        case "listen": return tr("Complete %1 listening challenges").arg(t)
        case "combo": return tr("Get %1 right in a row").arg(t)
        case "goal": return tr("Reach your daily goal")
        case "practice": return tr("Complete a practice")
        }
        return q.kind || ""
    }

    // ---------- this week, and the user's own league of weeks ----------
    Card {
        Lbl { text: sd.tr("This week"); bold: true; size: 17 }
        Row {
            id: wk
            spacing: 6
            readonly property real barW: Math.floor((parent.width - 6 * 6) / 7)
            readonly property real peak: Math.max(10, Math.max.apply(null, sd.weekDays.length > 0 ? sd.weekDays : [0]))
            Repeater {
                model: 7
                delegate: Column {
                    id: dayCol
                    required property int index
                    spacing: 4
                    readonly property real v: sd.weekDays[index] || 0
                    Item {
                        width: wk.barW; height: 64
                        Rectangle {
                            anchors.bottom: parent.bottom
                            width: parent.width; radius: 4
                            height: Math.max(3, parent.height * dayCol.v / wk.peak)
                            color: dayCol.v > 0 ? sd.cFlame : Theme.c.fill
                        }
                    }
                    Text { width: wk.barW; horizontalAlignment: Text.AlignHCenter; text: sd.dayLetter(index); color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family }
                }
            }
        }
        // the last eight weeks, ranked: a league against yourself
        Lbl {
            width: parent.width; wrapMode: Text.Wrap; dim: true; size: 13
            text: sd.weeks.length === 0 ? "" : sd.tr("Your league: your own last 8 weeks. This week is number %1.").arg(sd.ln.week_rank || 1)
        }
        Repeater {
            model: sd.weeks.length
            delegate: Item {
                id: wrow
                required property int index
                readonly property int back: sd.weeks.length - 1 - index
                readonly property real v: sd.weeks[index] || 0
                readonly property real peak: Math.max(10, Math.max.apply(null, sd.weeks))
                width: parent.width; height: 20
                Text { id: wl; width: 92; anchors.verticalCenter: parent.verticalCenter; text: wrow.back === 0 ? sd.tr("This week") : (wrow.back === 1 ? sd.tr("Last week") : sd.tr("%1 weeks ago").arg(wrow.back)); color: wrow.back === 0 ? Theme.c.text : Theme.c.textDim; font.pixelSize: 12; font.weight: wrow.back === 0 ? Font.Bold : Font.Normal; font.family: Theme.type.family; elide: Text.ElideRight }
                Rectangle { anchors { left: wl.right; leftMargin: 6; verticalCenter: parent.verticalCenter } height: 10; radius: 5; width: Math.max(4, (parent.width - wl.width - 60) * wrow.v / wrow.peak); color: wrow.back === 0 ? sd.cFlame : Theme.c.fill }
                Text { anchors { right: parent.right; verticalCenter: parent.verticalCenter } text: wrow.v + " XP"; color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
            }
        }
    }

    // ---------- the shop ----------
    Card {
        Item {
            width: parent.width; height: 30
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Lbl { text: sd.tr("Shop"); bold: true; size: 17; anchors.verticalCenter: parent.verticalCenter }
                LearnGlyph { name: "gem"; size: 18; color: sd.cGem; filled: true; fillColor: Qt.rgba(sd.cGem.r, sd.cGem.g, sd.cGem.b, 0.3); anchors.verticalCenter: parent.verticalCenter }
                Text { text: sd.ln.gems || 0; color: sd.cGem; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family; anchors.verticalCenter: parent.verticalCenter }
            }
            // how to get gems, in full
            Rectangle {
                id: infoBtn
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                width: 30; height: 30; radius: 15
                color: infoArea.containsMouse ? Theme.c.hover : "transparent"
                LearnGlyph { anchors.centerIn: parent; name: "info"; size: 18; color: Theme.c.textDim }
                MouseArea { id: infoArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: gemsPop.openBelow(infoBtn, true) }
                ToolTip.visible: infoArea.containsMouse; ToolTip.delay: 400; ToolTip.text: sd.tr("How to get gems")
            }
        }
        Lbl {
            width: parent.width; wrapMode: Text.Wrap; dim: true; size: 12
            text: sd.tr("Earn gems by finishing lessons (+%1), daily quests, streaks and the daily chest.").arg(sd.gemRules.lesson || 5)
        }
        ShopItem {
            glyph: "heart"; tone: sd.cHeart
            title: sd.tr("Refill hearts")
            subtitle: sd.ln.unlimited === true ? sd.tr("Unlimited hearts are on") : ((sd.ln.hearts || 0) >= (sd.ln.max_hearts || 5) ? sd.tr("Your hearts are full") : (sd.ln.hearts || 0) + " / " + (sd.ln.max_hearts || 5) + " · " + sd.tr("one comes back every 5 hours"))
            price: sd.ln.price_refill || 60
            can: sd.ln.unlimited !== true && (sd.ln.hearts || 0) < (sd.ln.max_hearts || 5)
            onBuy: sd.command({cmd: "learn_buy", item: "refill"})
        }
        ShopItem {
            glyph: "rocket"; tone: sd.cLegend
            title: sd.tr("XP boost")
            subtitle: (sd.ln.boost_left || 0) > 0 ? sd.tr("On: %1 min left (buying adds 15)").arg(Math.ceil((sd.ln.boost_left || 0) / 60)) : sd.tr("Double XP for %1 minutes").arg(sd.gemRules.boost_minutes || 15)
            price: sd.ln.price_boost || 80
            can: true
            onBuy: sd.command({cmd: "learn_buy", item: "boost"})
        }
        ShopItem {
            glyph: "snowflake"; tone: sd.cIce
            title: sd.tr("Streak freeze")
            subtitle: sd.tr("Keeps your streak for a day you miss") + " · " + (sd.ln.freezes || 0) + " / " + (sd.ln.max_freezes || 2)
            price: sd.ln.price_freeze || 50
            can: (sd.ln.freezes || 0) < (sd.ln.max_freezes || 2)
            onBuy: sd.command({cmd: "learn_buy", item: "freeze"})
        }
        PrefSwitch {
            width: parent.width
            title: sd.tr("Unlimited hearts")
            subtitle: sd.tr("Mistakes never stop a lesson")
            checked: sd.ln.unlimited === true
            onToggled: function(on) { sd.command({cmd: "set_setting", path: "general.learn_unlimited_hearts", value: on}) }
        }
    }
    component ShopItem: Item {
        id: si
        property string glyph: ""
        property color tone: Theme.c.accent
        property string title: ""
        property string subtitle: ""
        property int price: 0
        property bool can: true
        signal buy()
        width: parent.width; height: Math.max(56, siCol.implicitHeight + 12)
        LearnGlyph { id: sig; name: si.glyph; size: 30; color: si.tone; filled: true; fillColor: Qt.rgba(si.tone.r, si.tone.g, si.tone.b, 0.3); anchors.verticalCenter: parent.verticalCenter }
        Column {
            id: siCol
            anchors { left: sig.right; leftMargin: 12; right: siBtn.left; rightMargin: 10; verticalCenter: parent.verticalCenter }
            spacing: 2
            Lbl { width: parent.width; text: si.title; bold: true }
            Lbl { width: parent.width; text: si.subtitle; dim: true; size: 12; wrapMode: Text.Wrap }
        }
        LearnButton {
            id: siBtn
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            implicitWidth: 86
            outline: true; glyph: "gem"
            text: String(si.price)
            enabled: si.can && (sd.ln.gems || 0) >= si.price
            onClicked: si.buy()
        }
    }

    // how gems are earned, with the numbers the core uses
    Popover {
        id: gemsPop
        contentItem: Column {
            width: 300
            spacing: 8
            Row {
                spacing: 8
                LearnGlyph { name: "gem"; size: 20; color: sd.cGem; filled: true; fillColor: Qt.rgba(sd.cGem.r, sd.cGem.g, sd.cGem.b, 0.3); anchors.verticalCenter: parent.verticalCenter }
                Lbl { text: sd.tr("How to get gems"); bold: true; size: 16; anchors.verticalCenter: parent.verticalCenter }
            }
            Repeater {
                model: sd.gemLines()
                delegate: Item {
                    required property var modelData
                    width: 300; height: Math.max(26, glText.implicitHeight + 6)
                    LearnGlyph { id: glG; name: modelData.glyph; size: 18; color: modelData.tone; anchors.verticalCenter: parent.verticalCenter }
                    Lbl { id: glText; anchors { left: glG.right; leftMargin: 10; right: glAmt.left; rightMargin: 8; verticalCenter: parent.verticalCenter } wrapMode: Text.Wrap; size: 13; text: modelData.text }
                    Text { id: glAmt; anchors { right: parent.right; verticalCenter: parent.verticalCenter } text: modelData.gems; color: sd.cGem; font.pixelSize: 13; font.weight: Font.Bold; font.family: Theme.type.family }
                }
            }
            Lbl { width: 300; wrapMode: Text.Wrap; dim: true; size: 12; text: sd.tr("Spend them on heart refills, streak freezes and XP boosts. Everything stays on this computer.") }
        }
    }
    function gemLines() {
        const g = gemRules
        const ms = (g.milestones || []).map(function(m) { return m.days }).join(" / ")
        const mg = (g.milestones || []).map(function(m) { return "+" + m.gems }).join(" / ")
        return [
            {glyph: "book", tone: Theme.c.accent, text: tr("Finish a lesson or a practice"), gems: "+" + (g.lesson || 5)},
            {glyph: "target", tone: Theme.c.success, text: tr("No mistakes in it"), gems: "+" + (g.perfect || 3)},
            {glyph: "flag", tone: cFlame, text: tr("Each daily quest"), gems: "+" + (g.quest_min || 10) + "–" + (g.quest_max || 20)},
            {glyph: "medal", tone: cGold, text: tr("Each achievement"), gems: "+" + (g.achievement || 20)},
            {glyph: "flame", tone: cFlame, text: tr("Streak milestones (%1 days)").arg(ms), gems: mg},
            {glyph: "gift", tone: cLegend, text: tr("The daily chest, after your daily goal (sometimes with an XP boost)"), gems: "+" + (g.chest_min || 5) + "–" + (g.chest_max || 20)}
        ]
    }

    // ---------- achievements ----------
    Card {
        Lbl { text: sd.tr("Achievements"); bold: true; size: 17 }
        Grid {
            id: ach
            columns: 3
            spacing: 10
            readonly property real cell: Math.floor((parent.width - 20) / 3)
            Repeater {
                model: sd.achievements.length
                delegate: Column {
                    id: achCell
                    required property int index
                    readonly property var a: sd.achievements[index] || ({})
                    width: ach.cell
                    spacing: 4
                    Rectangle {
                        width: 52; height: 52; radius: 26
                        anchors.horizontalCenter: parent.horizontalCenter
                        color: parent.a.earned === true ? sd.cGold : Theme.c.fill
                        LearnGlyph { anchors.centerIn: parent; name: "medal"; size: 28; color: achCell.a.earned === true ? "#ffffff" : Theme.c.textDim }
                    }
                    Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: sd.tr(parent.a.name || ""); color: Theme.c.text; font.pixelSize: 12; font.weight: Font.Bold; font.family: Theme.type.family }
                    Bar { width: parent.width; height: 6; value: (parent.a.have || 0) / Math.max(1, parent.a.target || 1); tone: sd.cGold; visible: parent.a.earned !== true }
                    Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: (parent.a.have || 0) + " / " + (parent.a.target || 0); color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family }
                }
            }
        }
    }
    Lbl { width: sd.width; wrapMode: Text.Wrap; dim: true; size: 12; text: (sd.ln.words || 0) + " " + sd.tr("words learned") + " · " + (sd.ln.lessons_done || 0) + " / " + (sd.ln.lessons_total || 0) + " " + sd.tr("lessons") }
}
