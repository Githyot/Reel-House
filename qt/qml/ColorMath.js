.pragma library

function clamp(v, a, b) { return Math.max(a, Math.min(b, v)) }

function hexToRgb(hex) {
    var s = String(hex).trim().replace("#", "")
    if (s.length === 3) s = s[0] + s[0] + s[1] + s[1] + s[2] + s[2]
    if (s.length === 8) s = s.substring(2)
    if (!/^[0-9a-fA-F]{6}$/.test(s)) return null
    return { r: parseInt(s.substring(0, 2), 16), g: parseInt(s.substring(2, 4), 16), b: parseInt(s.substring(4, 6), 16) }
}

function hex2(n) { var s = Math.round(clamp(n, 0, 255)).toString(16); return s.length < 2 ? "0" + s : s }
function rgbToHex(r, g, b) { return "#" + hex2(r) + hex2(g) + hex2(b) }

function hueOf(r, g, b, mx, d, hint) {
    var h = hint || 0
    if (d > 0) {
        if (mx === r) h = 60 * (((g - b) / d) % 6)
        else if (mx === g) h = 60 * ((b - r) / d + 2)
        else h = 60 * ((r - g) / d + 4)
        if (h < 0) h += 360
    }
    return h
}

function rgbToHsv(r, g, b, hint) {
    r /= 255; g /= 255; b /= 255
    var mx = Math.max(r, g, b), mn = Math.min(r, g, b), d = mx - mn
    return { h: hueOf(r, g, b, mx, d, hint), s: mx === 0 ? 0 : d / mx, v: mx }
}

function hsvToRgb(h, s, v) {
    var c = v * s, hp = (((h % 360) + 360) % 360) / 60, x = c * (1 - Math.abs(hp % 2 - 1)), m = v - c
    var r = 0, g = 0, b = 0
    if (hp < 1) { r = c; g = x } else if (hp < 2) { r = x; g = c } else if (hp < 3) { g = c; b = x }
    else if (hp < 4) { g = x; b = c } else if (hp < 5) { r = x; b = c } else { r = c; b = x }
    return { r: (r + m) * 255, g: (g + m) * 255, b: (b + m) * 255 }
}

function rgbToHsl(r, g, b, hint) {
    r /= 255; g /= 255; b /= 255
    var mx = Math.max(r, g, b), mn = Math.min(r, g, b), d = mx - mn, l = (mx + mn) / 2
    var den = 1 - Math.abs(2 * l - 1)
    return { h: hueOf(r, g, b, mx, d, hint), s: den <= 0 || d === 0 ? 0 : d / den, l: l }
}

function hslToRgb(h, s, l) {
    var c = (1 - Math.abs(2 * l - 1)) * s, hp = (((h % 360) + 360) % 360) / 60, x = c * (1 - Math.abs(hp % 2 - 1)), m = l - c / 2
    var r = 0, g = 0, b = 0
    if (hp < 1) { r = c; g = x } else if (hp < 2) { r = x; g = c } else if (hp < 3) { g = c; b = x }
    else if (hp < 4) { g = x; b = c } else if (hp < 5) { r = x; b = c } else { r = c; b = x }
    return { r: (r + m) * 255, g: (g + m) * 255, b: (b + m) * 255 }
}

// A tone of a hue (Material tonal palette: 0 is black, 100 is white; colour fades near both ends).
var TONES = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99, 100]
function tone(h, s, t) {
    var l = t / 100, k = 1 - Math.pow(Math.abs(2 * l - 1), 3)
    var c = hslToRgb(h, s * k, l)
    return rgbToHex(c.r, c.g, c.b)
}
function tonalPalette(h, s) { return TONES.map(function(t) { return tone(h, s, t) }) }
