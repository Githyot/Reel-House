.pragma library
// The hiragana / katakana reader of the study cards: every Japanese word of a card becomes a quiet link ("jp:<word>"),
// and hovering it shows the word in hiragana, in katakana and in romaji, with its meaning when the card has one.
// The readings come from the card itself (the clip cards of the player's Editor write "word 【reading】" lines and
// "<b>word</b>: meaning" lines); a kanji with no word reading shows its on / kun readings from the card's kanji lines.

const ROMA = {
    "あ":"a","い":"i","う":"u","え":"e","お":"o","か":"ka","き":"ki","く":"ku","け":"ke","こ":"ko",
    "さ":"sa","し":"shi","す":"su","せ":"se","そ":"so","た":"ta","ち":"chi","つ":"tsu","て":"te","と":"to",
    "な":"na","に":"ni","ぬ":"nu","ね":"ne","の":"no","は":"ha","ひ":"hi","ふ":"fu","へ":"he","ほ":"ho",
    "ま":"ma","み":"mi","む":"mu","め":"me","も":"mo","や":"ya","ゆ":"yu","よ":"yo",
    "ら":"ra","り":"ri","る":"ru","れ":"re","ろ":"ro","わ":"wa","ゐ":"wi","ゑ":"we","を":"wo","ん":"n",
    "が":"ga","ぎ":"gi","ぐ":"gu","げ":"ge","ご":"go","ざ":"za","じ":"ji","ず":"zu","ぜ":"ze","ぞ":"zo",
    "だ":"da","ぢ":"ji","づ":"zu","で":"de","ど":"do","ば":"ba","び":"bi","ぶ":"bu","べ":"be","ぼ":"bo",
    "ぱ":"pa","ぴ":"pi","ぷ":"pu","ぺ":"pe","ぽ":"po","ゔ":"vu",
    "ぁ":"a","ぃ":"i","ぅ":"u","ぇ":"e","ぉ":"o","ゃ":"ya","ゅ":"yu","ょ":"yo","ゎ":"wa"
}
const SMALL_Y = { "ゃ": "a", "ゅ": "u", "ょ": "o" }

function isKanji(c) { return (c >= "一" && c <= "鿿") || (c >= "㐀" && c <= "䶿") || c === "々" }
function isHira(c) { return c >= "ぁ" && c <= "ゖ" }
function isKata(c) { return (c >= "ァ" && c <= "ヺ") || c === "ー" }
function hasJapanese(s) { return /[ぁ-ゖァ-ヺ一-鿿]/.test(s || "") }

function toHira(s) {
    let o = ""
    for (const c of s) o += (c >= "ァ" && c <= "ヶ") ? String.fromCharCode(c.charCodeAt(0) - 0x60) : c
    return o
}
function toKata(s) {
    let o = ""
    for (const c of s) o += (c >= "ぁ" && c <= "ゖ") ? String.fromCharCode(c.charCodeAt(0) + 0x60) : c
    return o
}
// Hepburn: きゃ = kya, しゃ = sha, っ doubles the next consonant, ー lengthens the vowel before it
function romaji(s) {
    const h = Array.from(toHira(s))
    let o = ""
    for (let i = 0; i < h.length; ++i) {
        const c = h[i], n = h[i + 1]
        if (c === "っ") { const nx = n ? (ROMA[n] || "") : ""; o += nx.startsWith("ch") ? "t" : (nx[0] || ""); continue }
        if (c === "ー") { o += o.length ? o[o.length - 1] : ""; continue }
        let r = ROMA[c]
        if (r === undefined) { o += c; continue }
        if (n && SMALL_Y[n] && r.endsWith("i") && r.length > 1) {
            const base = r.slice(0, -1)
            r = (base === "sh" || base === "ch" || base === "j") ? base + SMALL_Y[n] : base + "y" + SMALL_Y[n]
            ++i
        }
        o += r
    }
    return o
}

// what the card knows: "word 【reading】", "<b>word</b>: meaning", "<b>字</b> — meaning; 音 オン; 訓 くん"
// `extra` is the card's own list from the core ({words: [[word, reading]], kanji: [[字, "meaning; 音 …; 訓 …"]]}):
// the furigana of its fields (日本[にほん], <ruby>), a Word / Word Reading pair, and the kanji table of the app
function dict(html, extra) {
    const d = { reading: {}, meaning: {}, kanji: {} }
    const s = html || ""
    let m
    const rr = /([^\s<>【】]+)\s*【([^】]+)】/g
    while ((m = rr.exec(s)) !== null) if (!d.reading[m[1]]) d.reading[m[1]] = toHira(m[2])
    const mr = /<b>([^<]+)<\/b>:\s*([^<]+)/g
    while ((m = mr.exec(s)) !== null) if (!d.meaning[m[1]]) d.meaning[m[1]] = m[2].trim()
    const kr = /<b>(.)<\/b>\s*—\s*([^<]*)/g
    while ((m = kr.exec(s)) !== null) d.kanji[m[1]] = m[2].trim()
    if (extra) {
        for (const p of (extra.words || [])) if (p.length > 1 && !d.reading[p[0]]) d.reading[p[0]] = toHira(p[1])
        for (const p of (extra.kanji || [])) if (p.length > 1 && !d.kanji[p[0]]) d.kanji[p[0]] = p[1]
    }
    return d
}

function esc(s) { return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }

const PARTICLES = "はがをにでとのもへやか"
// the Japanese words of a plain text: a word the card knows (the longest), else kanji with the hiragana that follow
// (up to three, stopping at a particle), a katakana run, a hiragana run
function linkText(t, color, d) {
    const cs = Array.from(t)
    let o = "", i = 0
    while (i < cs.length) {
        const c = cs[i]
        let j = i
        let known = ""
        if (d) for (const w in d.reading) if (w.length > known.length && t.startsWith(w, cs.slice(0, i).join("").length)) known = w
        if (known && !isHira(c)) {
            j = i + Array.from(known).length
        } else if (isKanji(c)) {
            while (j < cs.length && isKanji(cs[j])) ++j
            let k = j
            while (k < cs.length && k - j < 3 && isHira(cs[k]) && !(k === j && PARTICLES.indexOf(cs[k]) >= 0 && k + 1 < cs.length && !isHira(cs[k + 1]))) ++k
            j = k
        } else if (isKata(c)) {
            while (j < cs.length && isKata(cs[j])) ++j
        } else if (isHira(c)) {
            while (j < cs.length && isHira(cs[j])) ++j
        }
        if (j > i) {
            const w = cs.slice(i, j).join("")
            o += '<a href="jp:' + encodeURIComponent(w) + '" style="text-decoration:none"><span style="color:' + color + '; text-decoration:none">' + w + '</span></a>'
            i = j
        } else {
            o += c
            ++i
        }
    }
    return o
}

// the html with its Japanese words made hoverable; tags, entities and existing links stay as they are
function linkify(html, color, d) {
    if (!hasJapanese(html)) return html
    let o = "", inA = 0
    const parts = html.split(/(<[^>]*>)/)
    for (const p of parts) {
        if (p.startsWith("<")) {
            if (/^<a[\s>]/i.test(p)) ++inA
            else if (/^<\/a>/i.test(p)) inA = Math.max(0, inA - 1)
            o += p
        } else {
            o += inA > 0 ? p : p.replace(/[^&]+|&[^;\s]*;?/g, function(x) { return x[0] === "&" ? x : linkText(x, color, d) })
        }
    }
    return o
}

// the lines of the hover box for a word ("jp:<word>")
function tip(link, d) {
    const w = decodeURIComponent(link.substring(3))
    let base = w, reading = d.reading[w] || ""
    // 食べる: the reading of 食べ, or of the kanji run alone with the okurigana added back
    if (!reading) {
        const run = Array.from(w).filter(isKanji).join("")
        const tail = Array.from(w).slice(run.length).join("")
        for (const k in d.reading) {
            const kRun = Array.from(k).filter(isKanji).join("")
            if (run && kRun === run && k.startsWith(run)) {
                const r = d.reading[k]
                const kTail = Array.from(k).slice(Array.from(run).length).join("")
                reading = (kTail && r.endsWith(kTail) ? r.slice(0, r.length - kTail.length) : r) + tail
                base = k
                break
            }
        }
    }
    const kana = !Array.from(w).some(isKanji)
    if (kana) reading = toHira(w)
    const lines = []
    lines.push("<big><b>" + esc(w) + "</b></big>")
    if (reading) {
        lines.push("ひらがな  " + esc(toHira(reading)))
        lines.push("カタカナ  " + esc(toKata(reading)))
        lines.push("<i>" + esc(romaji(reading)) + "</i>")
    } else {
        for (const c of Array.from(w)) if (d.kanji[c]) lines.push("<b>" + c + "</b> " + esc(d.kanji[c]))
    }
    const mean = d.meaning[w] || d.meaning[base]
    if (mean) lines.push(esc(mean))
    return lines.join("<br>")
}
