import QtQuick
import Reelhouse 1.0

// Tracing a character with the mouse: a faint character in a square, the user draws over it. The check is loose:
// most of the character must be covered by the strokes (coverage) and most of the strokes must lie on it
// (precision), compared on a coarse grid — the order and direction of the strokes are not checked.
Item {
    id: kt
    property string kana: ""
    property color ink: Theme.c.accent
    property bool locked: false
    // strokes drawn so far: arrays of {x, y}
    property var strokes: []
    readonly property bool drawn: strokes.length > 0
    readonly property real pen: Math.max(10, width * 0.055)
    implicitWidth: 280
    implicitHeight: 280
    onKanaChanged: clear()

    function clear() {
        strokes = []
        inkCanvas.requestPaint()
        guide.requestPaint()
    }

    // "ok" when the drawing follows the character closely enough, else "no"
    function verdict() {
        const N = 40
        const w = guide.width, h = guide.height
        if (w <= 0 || h <= 0 || strokes.length === 0) return "no"
        const ctx = guide.getContext("2d")
        let img = null
        try { img = ctx.getImageData(0, 0, w, h) } catch (e) { img = null }
        // without the pixels of the guide, a drawing over enough of the square passes
        const cw = w / N, ch = h / N
        const user = new Array(N * N).fill(false)
        const r = 2
        function mark(px, py) {
            const cx = Math.floor(px / cw), cy = Math.floor(py / ch)
            for (let dy = -r; dy <= r; ++dy) for (let dx = -r; dx <= r; ++dx) {
                const x = cx + dx, y = cy + dy
                if (x >= 0 && y >= 0 && x < N && y < N && dx * dx + dy * dy <= r * r) user[y * N + x] = true
            }
        }
        const raw = new Array(N * N).fill(false)
        for (let s = 0; s < strokes.length; ++s) {
            const pts = strokes[s]
            for (let i = 0; i < pts.length; ++i) {
                const a = pts[i], b = i + 1 < pts.length ? pts[i + 1] : pts[i]
                const steps = Math.max(1, Math.ceil(Math.hypot(b.x - a.x, b.y - a.y) / (cw / 2)))
                for (let k = 0; k <= steps; ++k) {
                    const px = a.x + (b.x - a.x) * k / steps, py = a.y + (b.y - a.y) * k / steps
                    mark(px, py)
                    const cx = Math.floor(px / cw), cy = Math.floor(py / ch)
                    if (cx >= 0 && cy >= 0 && cx < N && cy < N) raw[cy * N + cx] = true
                }
            }
        }
        if (!img) {
            const used = raw.filter(function(v) { return v }).length
            return used >= N * N * 0.04 ? "ok" : "no"
        }
        // the cells the character covers, and the same grown by two cells (for the precision)
        const glyph = new Array(N * N).fill(false)
        for (let y = 0; y < N; ++y) for (let x = 0; x < N; ++x) {
            const px = Math.min(w - 1, Math.floor((x + 0.5) * cw)), py = Math.min(h - 1, Math.floor((y + 0.5) * ch))
            if (img.data[(py * w + px) * 4 + 3] > 8) glyph[y * N + x] = true
        }
        let gl = 0, covered = 0, used = 0, onIt = 0
        for (let i = 0; i < N * N; ++i) {
            if (glyph[i]) { gl++; if (user[i]) covered++ }
        }
        for (let y = 0; y < N; ++y) for (let x = 0; x < N; ++x) {
            if (!raw[y * N + x]) continue
            used++
            let near = false
            for (let dy = -2; dy <= 2 && !near; ++dy) for (let dx = -2; dx <= 2 && !near; ++dx) {
                const xx = x + dx, yy = y + dy
                if (xx >= 0 && yy >= 0 && xx < N && yy < N && glyph[yy * N + xx]) near = true
            }
            if (near) onIt++
        }
        if (gl === 0) return used > 0 ? "ok" : "no"
        const coverage = covered / gl, precision = used > 0 ? onIt / used : 0
        return coverage >= 0.55 && precision >= 0.6 ? "ok" : "no"
    }

    Rectangle {
        anchors.fill: parent
        radius: Theme.shape.card
        color: Theme.c.card
        border.width: 2
        border.color: Theme.c.border
        // the guide lines of a writing square
        Repeater {
            model: 2
            delegate: Rectangle {
                required property int index
                x: index === 0 ? parent.width / 2 : 12
                y: index === 0 ? 12 : parent.height / 2
                width: index === 0 ? 1 : parent.width - 24
                height: index === 0 ? parent.height - 24 : 1
                color: Theme.c.divider
            }
        }
    }
    // the faint character (drawn in the canvas so its pixels can be compared with the strokes)
    Canvas {
        id: guide
        anchors.fill: parent
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            ctx.clearRect(0, 0, width, height)
            ctx.fillStyle = Qt.alpha(Theme.c.text, 0.16)
            const px = Math.round(Math.min(width, height) * (kt.kana.length > 1 ? 0.48 : 0.74))
            ctx.font = px + "px \"" + (Theme.type.family || "sans-serif") + "\""
            ctx.textAlign = "center"
            ctx.textBaseline = "middle"
            ctx.fillText(kt.kana, width / 2, height / 2 + px * 0.04)
        }
    }
    Canvas {
        id: inkCanvas
        anchors.fill: parent
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            ctx.clearRect(0, 0, width, height)
            ctx.strokeStyle = kt.ink
            ctx.lineWidth = kt.pen
            ctx.lineCap = "round"
            ctx.lineJoin = "round"
            for (let s = 0; s < kt.strokes.length; ++s) {
                const pts = kt.strokes[s]
                if (pts.length === 0) continue
                ctx.beginPath()
                ctx.moveTo(pts[0].x, pts[0].y)
                for (let i = 1; i < pts.length; ++i) ctx.lineTo(pts[i].x, pts[i].y)
                if (pts.length === 1) ctx.lineTo(pts[0].x + 0.1, pts[0].y)
                ctx.stroke()
            }
        }
    }
    MouseArea {
        anchors.fill: parent
        enabled: !kt.locked
        cursorShape: Qt.CrossCursor
        // the points of the stroke being drawn (copied into `strokes` as it grows, so the canvas repaints)
        property var cur: []
        onPressed: function(m) { cur = [{x: m.x, y: m.y}]; kt.strokes = kt.strokes.concat([cur]); inkCanvas.requestPaint() }
        onPositionChanged: function(m) {
            if (!pressed) return
            cur.push({x: m.x, y: m.y})
            const s = kt.strokes.slice(); s[s.length - 1] = cur; kt.strokes = s
            inkCanvas.requestPaint()
        }
    }
}
