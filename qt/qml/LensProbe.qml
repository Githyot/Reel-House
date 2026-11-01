import QtQuick
import Reelhouse 1.0

// Google Lens without a window: X-Ray hands it a picture (a frame of the video, a face cut out of it) and gets back
// what Lens says about it, as text, to match characters and actors. A hidden page (WebPane, kept rendering) uploads
// the picture to Lens like the Lens button of the quick search, waits for the results to settle (the AI overview
// streams in), reads the visible text of the results with a script and calls back.
//
//   LensProbe { id: lens }
//   lens.describe("/tmp/face.jpg", function(r) {
//       // r = {text, titles: [...], ai, url, error}
//       //   text:   the visible text of the results (the AI overview first when there is one), at most 12000 chars
//       //   titles: the result titles (h3 / headings / labelled links), at most 40, without repeats
//       //   ai:     the AI overview alone ("" when Google showed none)
//       //   url:    the results address
//       //   error:  "" when fine; "unavailable" (no Chromium), "lens" (Lens refused the picture), "timeout"
//   })
//
// Requests wait in a queue (one picture at a time). Nothing goes to the browser history. `busy` is true while one runs.
Item {
    id: lp
    // the size the hidden page is laid out at (a desktop window: Lens shows its full results there)
    width: 1280
    height: 900
    visible: false
    property bool busy: false
    property int timeoutMs: 45000
    // how long the results get to settle before they are read, and how many reads at most while the text still grows
    property int settleMs: 2500
    property int maxReads: 6
    property var queue: []
    property var job: null
    property int evalId: -1
    property int reads: 0
    property int lastLength: -1
    property var last: null

    function describe(imagePath, callback) {
        queue = queue.concat([{path: localPath(imagePath), cb: callback}])
        if (!busy) next()
    }

    // a file:// address or a path -> a path ("file:///C:/x.jpg" -> "C:/x.jpg", "file:///tmp/x.jpg" -> "/tmp/x.jpg")
    function localPath(p) {
        var s = String(p)
        if (!/^file:/.test(s)) return s
        s = decodeURIComponent(s.replace(/^file:\/\//, ""))
        return /^\/[A-Za-z]:/.test(s) ? s.slice(1) : s
    }

    // the profile and language of the app's browser (Core is absent in standalone tests)
    function setting(name, fallback) {
        try {
            if (name === "profile") return Core.state("browser").profileDir || fallback
            if (name === "lang") return Core.uiLang || fallback
        } catch (e) {}
        return fallback
    }

    function next() {
        if (queue.length === 0) { busy = false; return }
        busy = true
        job = queue[0]
        queue = queue.slice(1)
        reads = 0
        lastLength = -1
        last = null
        evalId = -1
        if (!pane.available) { finish({error: "unavailable"}); return }
        watchdog.restart()
        pane.lens(job.path, true)
    }

    function finish(r) {
        watchdog.stop()
        settle.stop()
        const done = job
        job = null
        const out = {text: (r && r.text) || "", titles: (r && r.titles) || [], ai: (r && r.ai) || "", url: (r && r.url) || pane.url, error: (r && r.error) || ""}
        if (done && done.cb) {
            try { done.cb(out) } catch (e) { console.warn("LensProbe callback: " + e) }
        }
        Qt.callLater(next)
    }

    // A results page: not the Lens start page, not the upload address, not Google's consent page.
    function isResults(url) {
        return /^https:\/\/[^\/]*google\.[^\/]+\/(search|webhp)\b/.test(url) || (/lens\.google\.com\//.test(url) && !/\/v3\/upload/.test(url) && /\/(search|results)/.test(url))
    }

    readonly property string extractScript: "
        const clean = s => (s || '').replace(/[ \\t]+/g, ' ').replace(/\\s*\\n\\s*/g, '\\n').trim();
        const vis = el => { const r = el.getBoundingClientRect(); const cs = getComputedStyle(el); return r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none'; };
        const main = document.querySelector('#rso') || document.querySelector('[role=\"main\"]') || document.querySelector('#search') || document.querySelector('main') || document.body;
        const aiHead = /^(AI Overview|AI Mode|Visão geral criada por IA|Visão geral de IA|Resumen creado con IA|Vista creada con IA|Visión general creada por IA)$/im;
        const stops = /\\n(Visual matches|Correspondências visuais|Coincidencias visuales|Show all|Mostrar tudo|Mostrar todo|Exact matches|Correspondências exatas|Coincidencias exactas|Products|Produtos|Productos)\\n/;
        const body = clean(main.innerText || '');
        // the AI overview: from its heading up to the next part of the results (it may sit outside the main column)
        const all = body.search(aiHead) >= 0 ? body : clean(document.body.innerText || '');
        let ai = '';
        const at = all.search(aiHead);
        if (at >= 0) {
            const rest = all.slice(at);
            const end = rest.slice(24).search(stops);
            ai = (end >= 0 ? rest.slice(0, end + 24) : rest.slice(0, 3000)).trim();
        }
        const titles = [];
        for (const el of main.querySelectorAll('h3, [role=\"heading\"][aria-level=\"3\"], a[aria-label]')) {
            if (!vis(el)) continue;
            const t = clean(el.tagName === 'A' ? el.getAttribute('aria-label') : el.innerText);
            if (t && t.length > 2 && t.length < 200 && !titles.includes(t)) titles.push(t);
            if (titles.length >= 40) break;
        }
        const text = ((ai && !body.includes(ai.slice(0, 80)) ? ai + '\\n\\n' : '') + body).slice(0, 12000);
        return { text: text, titles: titles, ai: ai.slice(0, 4000), url: location.href };"

    WebPane {
        id: pane
        anchors.fill: parent
        // drawn (slowly, WebPane lowers the rate while hidden) only while a picture is being looked up: idle, the page
        // sleeps instead of drawing at full speed behind the player
        keepRendering: lp.busy
        singleTab: true
        profile: lp.setting("profile", "")
        language: lp.setting("lang", "en")
        onLensFailed: if (lp.job) lp.finish({error: "lens"})
        onVisited: function(url, title) {
            if (!lp.job) return
            if (lp.isResults(url)) settle.restart()
        }
        onEvaluated: function(id, r) {
            if (id !== lp.evalId || !lp.job) return
            lp.evalId = -1
            const len = r && r.text ? r.text.length : 0
            lp.last = r
            lp.reads++
            // the AI overview and the results come in pieces: read again while the text keeps growing
            if (lp.reads < lp.maxReads && (len < 200 || len > lp.lastLength + 40)) {
                lp.lastLength = len
                settle.interval = 1500
                settle.restart()
                return
            }
            lp.finish(r)
        }
    }
    Timer {
        id: settle
        interval: lp.settleMs
        onTriggered: {
            if (!lp.job) return
            lp.evalId = pane.evaluate(lp.extractScript)
            interval = lp.settleMs
        }
    }
    Timer {
        id: watchdog
        interval: lp.timeoutMs
        // too slow: what was read so far, else an error
        onTriggered: if (lp.job) lp.finish(lp.last ? lp.last : {error: "timeout"})
    }
}
