.pragma library

// The themes of the readers, after Apple Books ("Themes & Settings"). Each has a day and a night look; the night one is
// used while the app is dark (unless the person turned that off). `family` empty means the app's own font.

// Text reader: background, text, font, heavier letters, paper grain.
var text = {
    "original": { name: "Original", bg: "#ffffff", fg: "#000000", nbg: "#121214", nfg: "#e9e9eb", family: "", heavy: false, grain: false },
    "quiet":    { name: "Quiet",    bg: "#4a4a4d", fg: "#c9c9cb", nbg: "#232325", nfg: "#9d9da1", family: "", heavy: false, grain: false },
    "paper":    { name: "Paper",    bg: "#f1eee8", fg: "#4a463f", nbg: "#2a2824", nfg: "#d2ccbf", family: "", heavy: false, grain: true },
    "bold":     { name: "Bold",     bg: "#ffffff", fg: "#000000", nbg: "#000000", nfg: "#ffffff", family: "", heavy: true,  grain: false },
    "calm":     { name: "Calm",     bg: "#f6ead2", fg: "#4f3c2a", nbg: "#3a3126", nfg: "#e8d8bb", family: "Lora", heavy: false, grain: false },
    "focus":    { name: "Focus",    bg: "#fffbe6", fg: "#2b2a22", nbg: "#1f1d15", nfg: "#f0e8c8", family: "PT Serif", heavy: false, grain: false }
}
var textOrder = ["original", "quiet", "paper", "bold", "calm", "focus"]

// Images reader: the colour around the pages. "theme" follows the app.
var pages = {
    "theme": { name: "Original", bg: "", fg: "", grain: false },
    "white": { name: "White",    bg: "#ffffff", fg: "#222222", grain: false },
    "paper": { name: "Paper",    bg: "#f1eee8", fg: "#4a463f", grain: true },
    "sepia": { name: "Sepia",    bg: "#f6ead2", fg: "#4f3c2a", grain: false },
    "dark":  { name: "Dark",     bg: "#1b1b1e", fg: "#cccccc", grain: false },
    "black": { name: "Black",    bg: "#000000", fg: "#bbbbbb", grain: false }
}
var pageOrder = ["theme", "white", "paper", "sepia", "dark", "black"]

// The fonts offered first (the app ships them); "" is the theme's own.
var fonts = ["", "Literata", "Lora", "PT Serif", "Inter", "Cantarell", "Roboto", "JetBrains Mono"]

// Speech bubbles of the Images reader: the fonts the app ships, and comic lettering fonts offered only when this
// computer has them installed (they are not ours to ship).
var bubbleFonts = ["Inter", "Cantarell", "Roboto", "Literata", "Lora", "PT Serif", "JetBrains Mono"]
var comicFonts = ["Anime Ace 2.0 BB", "Anime Ace BB", "Anime Ace", "CC Wild Words", "Wild Words", "Comic Neue", "Bangers", "Comic Sans MS"]

// The colours of a text theme for day or night: {bg, fg}
function textLook(id, night) {
    var t = text[id]
    if (!t) return null
    return { bg: night ? t.nbg : t.bg, fg: night ? t.nfg : t.fg, family: t.family, heavy: t.heavy, grain: t.grain }
}

// True when a colour is light (dark text goes on it).
function isLight(c) {
    var s = String(c)
    if (s.length < 7) return false
    var h = s.length === 9 ? s.substring(3) : s.substring(1)
    var r = parseInt(h.substring(0, 2), 16), g = parseInt(h.substring(2, 4), 16), b = parseInt(h.substring(4, 6), 16)
    return 0.299 * r + 0.587 * g + 0.114 * b > 150
}
