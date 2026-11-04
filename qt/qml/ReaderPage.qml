import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// One page of the chapter: the picture and, over the speech bubbles, the text that was read (or its translation).
// Hand corrections come from the chapter's translation file (.md): bubbles with formatted text, moved or resized,
// added or removed, and the edits of the picture (brush, eraser, bucket, crop), drawn by image://edited/.
// Edit modes of the reader: "text" (choose, move and resize bubbles, draw new ones) and "paint" (the picture tools).
Item {
    id: pg
    property var rd                       // the reader
    property string url: ""
    property int pageNo: 0                // 1 based, as in the translation file
    readonly property real natW: img.implicitWidth
    readonly property bool ready: img.status === Image.Ready
    readonly property var textOf: rd && rd.pt.text && rd.pt.text[url] ? rd.pt.text[url] : null
    readonly property var blocks: textOf && textOf.blocks ? textOf.blocks : []
    readonly property var trs: rd && rd.trIds[url] !== undefined && rd.pt.translations && rd.pt.translations[String(rd.trIds[url])] ? (rd.pt.translations[String(rd.trIds[url])].texts || null) : null
    // the sharpened copy is drawn over the original and never decides the size: the page keeps its proportion
    readonly property string src: rd && rd.upOn && rd.pt.upscaled && rd.pt.upscaled[url] && rd.pt.upscaled[url].path ? rd.pt.upscaled[url].path : url
    onBlocksChanged: if (rd) rd.maybeTranslate(url)

    // ---- hand corrections of this page ----
    readonly property var db: rd ? rd.pageDb(pageNo) : null
    readonly property string ops: db && db.ops ? db.ops : ""
    readonly property bool edited: ops !== "" && ops !== "[]"
    property string shownOps: ""
    function syncOps() { if (edited) Core.setEditOps(url, ops); shownOps = edited ? ops : "" }
    onOpsChanged: syncOps()
    onUrlChanged: syncOps()
    Component.onCompleted: syncOps()
    readonly property rect crop: edited ? Core.cropOf(ops) : Qt.rect(0, 0, 1, 1)
    readonly property real ratio: img.status === Image.Ready && img.implicitWidth > 0 ? (img.implicitHeight * crop.height) / (img.implicitWidth * crop.width) : 1.45
    // page (0..1 of the original picture) <-> this item
    function px(nx) { return (nx - crop.x) / crop.width * width }
    function py(ny) { return (ny - crop.y) / crop.height * height }
    function nx(x) { return crop.x + x / Math.max(1, width) * crop.width }
    function ny(y) { return crop.y + y / Math.max(1, height) * crop.height }
    function plain(html) { return (html || "").replace(/<br\s*\/?>/gi, "\n").replace(/<[^>]*>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&") }

    // OCR blocks with their corrections, then the boxes drawn by hand; removed ones are left out
    readonly property var items: {
        const out = []
        const dbb = db && db.bubbles ? db.bubbles : []
        const showOcr = rd && rd.ocrOn
        for (let i = 0; showOcr && i < blocks.length; ++i) {
            let k = -1
            for (let j = 0; j < dbb.length; ++j) if (dbb[j].ocr === i) { k = j; break }
            const mt = trs && trs[i] !== undefined ? trs[i] : ""
            if (k >= 0) {
                if (dbb[k].deleted) continue
                out.push(Object.assign({dbIndex: k, mt: mt}, dbb[k]))
            } else {
                const b = blocks[i]
                out.push({dbIndex: -1, ocr: i, x: b.x, y: b.y, w: b.w, h: b.h, original: b.text, html: "", reviewed: false, mt: mt})
            }
        }
        for (let k = 0; k < dbb.length; ++k)
            // boxes drawn by hand, and corrected OCR boxes when the text of the pages is not being read now
            if ((dbb[k].ocr < 0 || !showOcr || dbb[k].ocr >= blocks.length) && !dbb[k].deleted) out.push(Object.assign({dbIndex: k, mt: ""}, dbb[k]))
        return out
    }

    PosterImage {
        id: img
        anchors.fill: parent
        url: pg.url
        fillMode: Image.PreserveAspectFit
        opacity: pg.edited ? 0 : (status === Image.Ready ? 1 : 0)
    }
    PosterImage {
        id: sharp
        anchors.fill: parent
        visible: !pg.edited
        url: pg.src !== pg.url ? pg.src : ""
        fillMode: Image.PreserveAspectFit
        sourceSize.width: img.implicitWidth > 0 ? img.implicitWidth * 2 : 0
    }
    // the page with its picture edits
    Image {
        id: editedImg
        anchors.fill: parent
        visible: pg.shownOps !== ""
        asynchronous: true
        cache: false
        fillMode: Image.Stretch
        source: pg.shownOps !== "" ? "image://edited/" + encodeURIComponent(pg.url) + "@" + Qt.md5(pg.shownOps) : ""
        sourceSize.width: img.implicitWidth > 0 ? img.implicitWidth * pg.crop.width : 0
    }
    BusyIndicator { visible: img.status === Image.Loading || editedImg.status === Image.Loading; running: visible; anchors.centerIn: parent; width: 36; height: 36 }

    // ---- text mode: a press on the page (outside the bubbles) draws a new box ----
    MouseArea {
        id: newBox
        anchors.fill: parent
        z: 1
        enabled: pg.rd && pg.rd.editMode === "text"
        visible: enabled
        cursorShape: Qt.CrossCursor
        property point a
        property bool drawing: false
        onPressed: function(m) { a = Qt.point(m.x, m.y); drawing = true; rubber.x = m.x; rubber.y = m.y; rubber.width = 0; rubber.height = 0 }
        onPositionChanged: function(m) {
            if (!drawing) return
            rubber.x = Math.min(a.x, m.x); rubber.y = Math.min(a.y, m.y)
            rubber.width = Math.abs(m.x - a.x); rubber.height = Math.abs(m.y - a.y)
        }
        onReleased: {
            drawing = false
            if (rubber.width > 12 && rubber.height > 12)
                pg.rd.addBubble(pg.pageNo, pg.url, pg.nx(rubber.x), pg.ny(rubber.y), rubber.width / pg.width * pg.crop.width, rubber.height / pg.height * pg.crop.height)
            else pg.rd.activate(null)
            rubber.width = 0
        }
    }
    Rectangle { id: rubber; z: 6; visible: newBox.drawing && width > 0; color: "#33" + Theme.c.accent.toString().substring(1); border.width: 2; border.color: Theme.c.accent; radius: 6 }

    // The model is a count: the core's state is rebuilt often, and a list of new objects each time would destroy and
    // make again every bubble of every page (the interface froze while translations came in).
    Repeater {
        model: pg.items.length
        delegate: Rectangle {
            id: bub
            required property int index
            readonly property var it: pg.items[index] || ({})
            property bool flip: false
            readonly property bool hand: (it.html || "") !== ""
            readonly property bool translated: hand || (pg.rd.trOn && !pg.rd.showOriginal && it.mt !== "")
            readonly property bool showsTr: translated !== flip
            readonly property string shown: showsTr ? (hand ? it.html : it.mt) : it.original
            readonly property bool rich: showsTr && hand
            readonly property bool active: !!pg.rd.active && pg.rd.active.page === pg.pageNo && (it.dbIndex >= 0 ? pg.rd.active.dbIndex === it.dbIndex : pg.rd.active.ocr === it.ocr && pg.rd.active.dbIndex < 0)
            readonly property bool textMode: pg.rd.editMode === "text"
            // while a handle is dragged the box follows it; saved when the handle is let go
            property real dx: 0
            property real dy: 0
            property real dw: 0
            property real dh: 0
            readonly property real bw: Math.max(8, (it.w || 0) / pg.crop.width * pg.width + dw)
            readonly property real bh: Math.max(8, (it.h || 0) / pg.crop.height * pg.height + dh)
            // Japanese is often written in a tall narrow column; the translation is horizontal, so the cover widens (Koharu: typesetting)
            readonly property bool tall: showsTr && !hand && bh > bw * 1.8
            readonly property real ew: tall ? Math.min(Math.max(bw, bh * 0.75), pg.width * 0.3) : bw
            // the look saved with the bubble (font, size, text colour, highlight style; see pagedb.rs)
            readonly property string fam: (it.font || "") !== "" ? it.font : Theme.type.family
            readonly property real ownF: Math.max(0.3, Math.min(4, (it.size || 100) / 100))
            // Text size of the reader (Aa) and of the bubble: the text is first fitted to its bubble, then scaled; a bigger
            // text widens the cover
            readonly property real fontF: Math.max(0.4, Math.min(3, ((pg.rd && pg.rd.g.reader_font) || 100) / 100)) * ownF
            readonly property real fitPx: Math.max(6, probe.fontInfo.pixelSize)
            readonly property real needH: showsTr && fontF > 1.001 ? shownText.contentHeight + 8 : 0
            readonly property real extra: textMode ? 0 : Math.max(0, needH - bh)
            x: Math.max(0, Math.min(pg.width - ew, pg.px(it.x || 0) + dx - (ew - bw) / 2))
            y: Math.max(0, Math.min(pg.height - height, pg.py(it.y || 0) + dy - extra / 2))
            z: active ? 4 : 2
            width: ew
            height: bh + extra
            // a soft oval like a bubble that covers the old letters (the text is "erased" under it)
            radius: showsTr ? Math.min(width, height) * 0.32 : 4
            color: showsTr ? "#fcffffff" : "#33ffeb3b"
            border.width: showsTr ? 0 : 1
            border.color: "#99ffeb3b"
            Rectangle { visible: bub.showsTr; anchors.fill: parent; anchors.margins: -2; radius: parent.radius + 2; color: "transparent"; border.width: 2; border.color: "#fcffffff"; z: -1 }
            // measures the size the text has when it is fitted to the bubble (not drawn)
            Text {
                id: probe
                opacity: 0
                width: bub.ew - 2 * Math.max(3, bub.ew * 0.08)
                height: bub.bh - 4
                x: Math.max(3, bub.ew * 0.08)
                text: pg.plain(bub.shown)
                wrapMode: Text.Wrap
                fontSizeMode: Text.Fit
                minimumPixelSize: 6
                font.pixelSize: 26
                font.family: bub.fam
                font.weight: Font.DemiBold
            }
            // the text, with its highlights drawn as pills behind the words (BubbleText)
            BubbleText {
                id: shownText
                anchors.fill: parent
                anchors.leftMargin: bub.showsTr ? Math.max(3, bub.width * 0.08) : 2
                anchors.rightMargin: anchors.leftMargin
                anchors.topMargin: 2
                anchors.bottomMargin: 2
                visible: bub.showsTr
                text: bub.showsTr ? bub.shown : ""
                rich: bub.rich
                color: (bub.it.color || "") !== "" ? bub.it.color : "#111111"
                pixelSize: Math.max(6, Math.round(bub.fitPx * bub.fontF))
                family: bub.fam
                pill: (bub.it.highlight || "") !== "plain"
                clip: bub.fontF <= 1.001
            }
            // reviewed by a person: a small check in the corner
            Rectangle {
                visible: bub.it.reviewed === true && (bub.textMode || bub.active)
                anchors { right: parent.right; top: parent.top; margins: -6 }
                width: 16; height: 16; radius: 8; color: Theme.c.success; z: 5
                Icon { anchors.centerIn: parent; name: "check"; size: 12; color: Theme.c.greenFg }
            }
            // a bubble in the study language whose words are all known: a small "quiz me" button in its corner
            // (docs/study-in-readers.md; the core marks the blocks of the page as "known")
            readonly property bool quizzable: pg.rd.quizOn === true && pg.rd.editMode === "" && typeof it.ocr === "number" && it.ocr >= 0 && !!pg.blocks[it.ocr] && pg.blocks[it.ocr].known === true
            Rectangle {
                visible: bub.quizzable
                anchors { left: parent.left; top: parent.top; margins: -9 }
                width: 22; height: 22; radius: 11; z: 5
                color: Theme.c.accent
                border.width: 2; border.color: "#ffffff"
                Icon { anchors.centerIn: parent; name: "study"; size: 13; color: Theme.c.accentFg }
                MouseArea {
                    id: quizArea
                    anchors.fill: parent
                    anchors.margins: -4
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    ToolTip.visible: containsMouse
                    ToolTip.text: pg.rd.tr("Quiz me")
                    onClicked: pg.rd.openQuiz(bub.it.original || "", (bub.it.html || "") !== "" ? pg.plain(bub.it.html) : (bub.it.mt || ""))
                }
            }
            // the pointer lights a block up; a click selects it, a right click opens the menu (GTK: pagetools)
            readonly property string key: pg.url + "|" + (it.dbIndex >= 0 ? "d" + it.dbIndex : "o" + it.ocr)
            readonly property bool selected: !!pg.rd.sel[key]
            property bool hl: false
            Rectangle {
                anchors.fill: parent; anchors.margins: -2
                radius: parent.radius + 2
                color: bub.selected ? "#40" + Theme.c.accent.toString().substring(1) : "transparent"
                border.width: bub.active ? 3 : (bub.selected ? 3 : (bubArea.containsMouse || bub.textMode ? 2 : 0))
                border.color: bub.active || bub.selected ? Theme.c.accent : (bub.textMode ? "#99ffffff" : "#b3ffffff")
                z: 2
            }
            Rectangle { visible: bub.hl; anchors.fill: parent; radius: parent.radius; color: "#80ffeb3b"; z: 1 }
            MouseArea {
                id: bubArea
                anchors.fill: parent
                hoverEnabled: true
                // locked while reading (MangaReaderPage.bubblesLocked): the box is only drawn, the touch reaches the page
                enabled: pg.rd.editMode !== "paint" && pg.rd.editMode !== "card" && !(pg.rd.bubblesLocked && pg.rd.editMode === "")
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                cursorShape: bub.textMode && bub.active ? Qt.SizeAllCursor : Qt.PointingHandCursor
                z: 3
                // text mode: the chosen box moves with the pointer
                property point grab
                property bool moved: false
                onPressed: function(m) { grab = mapToItem(pg, m.x, m.y); moved = false }
                onPositionChanged: function(m) {
                    if (!pressed || !bub.textMode || !bub.active) return
                    const p = mapToItem(pg, m.x, m.y)
                    bub.dx = p.x - grab.x; bub.dy = p.y - grab.y
                    moved = moved || Math.abs(bub.dx) + Math.abs(bub.dy) > 3
                }
                onReleased: if (moved) bub.commitGeometry()
                onClicked: function(m) {
                    if (moved) return
                    if (m.button === Qt.RightButton) {
                        if (!bub.selected) pg.rd.toggleSel(bub.key, bub.it.original, true)
                        bubMenu.popup()
                        return
                    }
                    pg.rd.toggleSel(bub.key, bub.it.original, true)
                    pg.rd.activate(Object.assign({page: pg.pageNo, url: pg.url}, bub.it))
                }
                onPressAndHold: function(m) { if (!bub.selected) pg.rd.toggleSel(bub.key, bub.it.original, true); bubMenu.popup() }
                onDoubleClicked: { pg.rd.activate(Object.assign({page: pg.pageNo, url: pg.url}, bub.it)); pg.rd.focusEditor() }
            }
            function commitGeometry() {
                const nx0 = pg.nx(Math.max(0, pg.px(it.x) + dx)), ny0 = pg.ny(Math.max(0, pg.py(it.y) + dy))
                const nw = bw / pg.width * pg.crop.width, nh = bh / pg.height * pg.crop.height
                pg.rd.saveBubble(pg.pageNo, it, {x: nx0, y: ny0, w: nw, h: nh})
                dx = 0; dy = 0; dw = 0; dh = 0
            }
            // eight handles to resize the chosen box (text mode)
            Repeater {
                model: bub.textMode && bub.active ? [[0, 0], [0.5, 0], [1, 0], [1, 0.5], [1, 1], [0.5, 1], [0, 1], [0, 0.5]] : []
                delegate: Rectangle {
                    required property var modelData
                    readonly property real fx: modelData[0]
                    readonly property real fy: modelData[1]
                    z: 6
                    width: 14; height: 14; radius: 7
                    color: "#ffffff"; border.width: 2; border.color: Theme.c.accent
                    x: fx * bub.width - 7; y: fy * bub.height - 7
                    MouseArea {
                        anchors.fill: parent
                        anchors.margins: -6
                        cursorShape: (fx === 0.5) ? Qt.SizeVerCursor : (fy === 0.5 ? Qt.SizeHorCursor : ((fx === fy) ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor))
                        property point a
                        onPressed: function(m) { a = mapToItem(pg, m.x, m.y) }
                        onPositionChanged: function(m) {
                            if (!pressed) return
                            const p = mapToItem(pg, m.x, m.y), ddx = p.x - a.x, ddy = p.y - a.y
                            if (fx === 0) { bub.dx = ddx; bub.dw = -ddx } else if (fx === 1) bub.dw = ddx
                            if (fy === 0) { bub.dy = ddy; bub.dh = -ddy } else if (fy === 1) bub.dh = ddy
                        }
                        onReleased: bub.commitGeometry()
                    }
                }
            }
            AppMenu {
                id: bubMenu
                AppMenuItem { text: pg.rd.tr("Translate"); onTriggered: pg.rd.openSheet() }
                AppMenuItem { text: pg.rd.tr("Copy"); onTriggered: pg.rd.copySelection() }
                AppMenuItem { text: bub.hl ? pg.rd.tr("Remove the highlight") : pg.rd.tr("Highlight"); onTriggered: bub.hl = !bub.hl }
                AppMenuItem { visible: bub.translated; height: visible ? implicitHeight : 0; text: bub.showsTr ? pg.rd.tr("Show the original") : pg.rd.tr("Show the translation"); onTriggered: bub.flip = !bub.flip }
                AppMenuItem { text: pg.rd.tr("Edit the translation"); onTriggered: { bub.flip = false; pg.rd.activate(Object.assign({page: pg.pageNo, url: pg.url}, bub.it)); pg.rd.focusEditor() } }
                AppMenuItem { text: pg.rd.tr("Remove this box"); onTriggered: pg.rd.removeBubble(pg.pageNo, bub.it) }
                AppMenuItem { text: pg.rd.tr("Clear the selection"); onTriggered: pg.rd.clearSel() }
            }
        }
    }

    // ---- paint mode: the picture tools (see MangaReaderPage: paintTool, paintColor, paintSize, paintAlpha, fillTol) ----
    Canvas {
        id: live
        anchors.fill: parent
        z: 7
        visible: pg.rd && pg.rd.editMode === "paint"
        property var pts: []
        onPaint: {
            const c = getContext("2d")
            c.reset()
            if (pts.length < 2) return
            const t = pg.rd.paintTool
            if (t === "crop") {
                c.fillStyle = "rgba(0,0,0,0.45)"
                c.fillRect(0, 0, width, height)
                const x = Math.min(pts[0], pts[2]), y = Math.min(pts[1], pts[3]), w = Math.abs(pts[2] - pts[0]), h = Math.abs(pts[3] - pts[1])
                c.clearRect(x, y, w, h)
                c.strokeStyle = Theme.c.accent; c.lineWidth = 2; c.strokeRect(x, y, w, h)
                return
            }
            c.lineCap = "round"; c.lineJoin = "round"
            c.lineWidth = pg.rd.paintSize
            c.globalAlpha = t === "eraser" ? 0.5 : pg.rd.paintAlpha
            c.strokeStyle = t === "eraser" ? "#ff4d4d" : pg.rd.paintColor
            c.beginPath()
            c.moveTo(pts[0], pts[1])
            for (let i = 2; i + 1 < pts.length; i += 2) c.lineTo(pts[i], pts[i + 1])
            if (pts.length === 2) c.lineTo(pts[0] + 0.1, pts[1])
            c.stroke()
        }
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: pg.rd && (pg.rd.paintTool === "brush" || pg.rd.paintTool === "eraser") ? Qt.BlankCursor : Qt.CrossCursor
            onPressed: function(m) {
                const t = pg.rd.paintTool
                if (t === "fill") {
                    pg.rd.addOp(pg.pageNo, pg.url, pg.ops, {t: "fill", x: pg.nx(m.x), y: pg.ny(m.y), c: pg.rd.paintColor, a: pg.rd.paintAlpha, tol: pg.rd.fillTol})
                    return
                }
                if (t === "picker") { pg.rd.paintColor = Core.pickColor(pg.url, pg.nx(m.x), pg.ny(m.y)); return }
                live.pts = [m.x, m.y, m.x, m.y]
                if (t !== "crop") live.pts = [m.x, m.y]
                live.requestPaint()
            }
            onPositionChanged: function(m) {
                cursorDot.x = m.x - cursorDot.width / 2; cursorDot.y = m.y - cursorDot.height / 2
                if (!pressed || live.pts.length === 0) return
                if (pg.rd.paintTool === "crop") live.pts = [live.pts[0], live.pts[1], m.x, m.y]
                else { const p = live.pts.slice(); p.push(m.x, m.y); live.pts = p }
                live.requestPaint()
            }
            onReleased: {
                const p = live.pts, t = pg.rd.paintTool
                live.pts = []
                live.requestPaint()
                if (p.length < 2) return
                if (t === "crop") {
                    const x = Math.min(p[0], p[2]), y = Math.min(p[1], p[3]), w = Math.abs(p[2] - p[0]), h = Math.abs(p[3] - p[1])
                    if (w > 20 && h > 20) pg.rd.addOp(pg.pageNo, pg.url, pg.ops, {t: "crop", x: pg.nx(x), y: pg.ny(y), w: w / pg.width * pg.crop.width, h: h / pg.height * pg.crop.height})
                    return
                }
                const n = []
                for (let i = 0; i + 1 < p.length; i += 2) n.push(Math.round(pg.nx(p[i]) * 10000) / 10000, Math.round(pg.ny(p[i + 1]) * 10000) / 10000)
                pg.rd.addOp(pg.pageNo, pg.url, pg.ops, {t: "stroke", tool: t, c: pg.rd.paintColor, a: pg.rd.paintAlpha, s: Math.round(pg.rd.paintSize / pg.width * pg.crop.width * 100000) / 100000, p: n})
            }
        }
        // the size of the brush under the pointer
        Rectangle {
            id: cursorDot
            visible: pg.rd && (pg.rd.paintTool === "brush" || pg.rd.paintTool === "eraser")
            width: pg.rd ? pg.rd.paintSize : 10; height: width; radius: width / 2
            color: "transparent"; border.width: 1; border.color: pg.rd && pg.rd.paintTool === "eraser" ? "#ff4d4d" : "#ffffff"
            Rectangle { anchors.fill: parent; anchors.margins: 1; radius: width / 2; color: "transparent"; border.width: 1; border.color: "#80000000" }
        }
    }

    // ---- card mode: a rectangle dragged on the page becomes a study card (the picture inside it and its text) ----
    readonly property bool cardMode: !!rd && rd.editMode === "card"
    readonly property var cardSel: cardMode && rd.cardSel && rd.cardSel.url === url ? rd.cardSel : null
    // The text inside the rectangle: every OCR block (with its hand correction) and every box drawn by hand that lies
    // mostly inside it, in the order of the page; with the translations that are known (by hand first, then machine).
    function cardTexts(sel) {
        const dbb = db && db.bubbles ? db.bubbles : []
        const list = []
        for (let i = 0; i < blocks.length; ++i) {
            let k = -1
            for (let j = 0; j < dbb.length; ++j) if (dbb[j].ocr === i) { k = j; break }
            const mt = trs && trs[i] !== undefined ? trs[i] : ""
            if (k >= 0) { if (!dbb[k].deleted) list.push({b: dbb[k], mt: mt}) }
            else list.push({b: {x: blocks[i].x, y: blocks[i].y, w: blocks[i].w, h: blocks[i].h, original: blocks[i].text, html: ""}, mt: mt})
        }
        for (let k = 0; k < dbb.length; ++k)
            if ((dbb[k].ocr < 0 || dbb[k].ocr >= blocks.length) && !dbb[k].deleted) list.push({b: dbb[k], mt: ""})
        const texts = [], trans = []
        for (let i = 0; i < list.length; ++i) {
            const b = list[i].b
            const iw = Math.min(b.x + b.w, sel.x + sel.w) - Math.max(b.x, sel.x)
            const ih = Math.min(b.y + b.h, sel.y + sel.h) - Math.max(b.y, sel.y)
            // at least 40 % of the block inside the rectangle
            if (iw <= 0 || ih <= 0 || iw * ih < 0.4 * Math.max(1e-6, b.w * b.h)) continue
            if ((b.original || "").trim() !== "") texts.push(b.original.trim())
            const t = (b.html || "") !== "" ? plain(b.html) : list[i].mt
            if ((t || "").trim() !== "") trans.push(t.trim())
        }
        return {text: texts.join("\n"), translation: trans.join("\n")}
    }
    readonly property var cardAuto: cardSel ? cardTexts(cardSel) : null
    onCardAutoChanged: if (cardAuto && rd) rd.cardAuto = cardAuto
    // the rectangle on screen, from the chosen one (kept in 0..1 of the original page)
    function syncCardBox() {
        if (!cardSel) return
        cardBox.x = px(cardSel.x); cardBox.y = py(cardSel.y)
        cardBox.width = cardSel.w / crop.width * width; cardBox.height = cardSel.h / crop.height * height
    }
    onCardSelChanged: syncCardBox()
    onWidthChanged: syncCardBox()
    onHeightChanged: syncCardBox()
    function commitCardBox() {
        const x0 = Math.max(0, cardBox.x), y0 = Math.max(0, cardBox.y)
        const x1 = Math.min(width, cardBox.x + cardBox.width), y1 = Math.min(height, cardBox.y + cardBox.height)
        if (x1 - x0 < 12 || y1 - y0 < 12) { syncCardBox(); return }
        rd.setCardSel({page: pageNo, url: url, x: nx(x0), y: ny(y0), w: (x1 - x0) / width * crop.width, h: (y1 - y0) / height * crop.height})
    }
    MouseArea {
        id: cardDraw
        anchors.fill: parent
        z: 8
        enabled: pg.cardMode
        visible: enabled
        preventStealing: true      // the strip (a list) would take the drag for scrolling
        cursorShape: Qt.CrossCursor
        property point a
        property bool drawing: false
        onPressed: function(m) { a = Qt.point(m.x, m.y); drawing = true; cardBox.x = m.x; cardBox.y = m.y; cardBox.width = 0; cardBox.height = 0 }
        onPositionChanged: function(m) {
            if (!drawing) return
            const bx = Math.max(0, Math.min(pg.width, m.x)), by = Math.max(0, Math.min(pg.height, m.y))
            cardBox.x = Math.min(a.x, bx); cardBox.y = Math.min(a.y, by)
            cardBox.width = Math.abs(bx - a.x); cardBox.height = Math.abs(by - a.y)
        }
        onReleased: {
            drawing = false
            if (cardBox.width > 12 && cardBox.height > 12) pg.commitCardBox()
            else pg.syncCardBox()
        }
    }
    // the darkened page around the rectangle
    Item {
        anchors.fill: parent
        z: 8.5
        visible: pg.cardMode && (pg.cardSel !== null || cardDraw.drawing)
        Rectangle { x: 0; y: 0; width: parent.width; height: Math.max(0, cardBox.y); color: "#73000000" }
        Rectangle { x: 0; y: cardBox.y + cardBox.height; width: parent.width; height: Math.max(0, parent.height - cardBox.y - cardBox.height); color: "#73000000" }
        Rectangle { x: 0; y: cardBox.y; width: Math.max(0, cardBox.x); height: cardBox.height; color: "#73000000" }
        Rectangle { x: cardBox.x + cardBox.width; y: cardBox.y; width: Math.max(0, parent.width - cardBox.x - cardBox.width); height: cardBox.height; color: "#73000000" }
    }
    Rectangle {
        id: cardBox
        z: 9
        visible: pg.cardMode && (pg.cardSel !== null || cardDraw.drawing) && width > 0
        color: "transparent"
        border.width: 2
        border.color: Theme.c.accent
        // drag inside to move it
        MouseArea {
            anchors.fill: parent
            anchors.margins: 8
            enabled: pg.cardSel !== null && !cardDraw.drawing
            preventStealing: true
            cursorShape: Qt.SizeAllCursor
            property point a
            property point start
            onPressed: function(m) { a = mapToItem(pg, m.x, m.y); start = Qt.point(cardBox.x, cardBox.y) }
            onPositionChanged: function(m) {
                if (!pressed) return
                const p = mapToItem(pg, m.x, m.y)
                cardBox.x = Math.max(0, Math.min(pg.width - cardBox.width, start.x + p.x - a.x))
                cardBox.y = Math.max(0, Math.min(pg.height - cardBox.height, start.y + p.y - a.y))
            }
            onReleased: pg.commitCardBox()
        }
        // eight handles to adjust it
        Repeater {
            model: pg.cardSel !== null && !cardDraw.drawing ? [[0, 0], [0.5, 0], [1, 0], [1, 0.5], [1, 1], [0.5, 1], [0, 1], [0, 0.5]] : []
            delegate: Rectangle {
                required property var modelData
                readonly property real fx: modelData[0]
                readonly property real fy: modelData[1]
                width: 14; height: 14; radius: 7
                color: "#ffffff"; border.width: 2; border.color: Theme.c.accent
                x: fx * cardBox.width - 7; y: fy * cardBox.height - 7
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -6
                    preventStealing: true
                    cursorShape: (fx === 0.5) ? Qt.SizeVerCursor : (fy === 0.5 ? Qt.SizeHorCursor : ((fx === fy) ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor))
                    property point a
                    property rect start
                    onPressed: function(m) { a = mapToItem(pg, m.x, m.y); start = Qt.rect(cardBox.x, cardBox.y, cardBox.width, cardBox.height) }
                    onPositionChanged: function(m) {
                        if (!pressed) return
                        const p = mapToItem(pg, m.x, m.y), ddx = p.x - a.x, ddy = p.y - a.y
                        if (fx === 0) { const nx0 = Math.max(0, Math.min(start.x + start.width - 16, start.x + ddx)); cardBox.x = nx0; cardBox.width = start.x + start.width - nx0 }
                        else if (fx === 1) cardBox.width = Math.max(16, Math.min(pg.width - start.x, start.width + ddx))
                        if (fy === 0) { const ny0 = Math.max(0, Math.min(start.y + start.height - 16, start.y + ddy)); cardBox.y = ny0; cardBox.height = start.y + start.height - ny0 }
                        else if (fy === 1) cardBox.height = Math.max(16, Math.min(pg.height - start.y, start.height + ddy))
                    }
                    onReleased: pg.commitCardBox()
                }
            }
        }
    }

    Text {
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 8 }
        visible: pg.rd && pg.rd.ocrOn && !!(pg.textOf && pg.textOf.error)
        text: pg.textOf && pg.textOf.error ? pg.textOf.error : ""
        color: "#ff7b72"
        wrapMode: Text.Wrap
        font.pixelSize: 13
        font.family: Theme.type.family
        style: Text.Outline
        styleColor: "#000000"
    }
}
