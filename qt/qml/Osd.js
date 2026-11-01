.pragma library

// Look of everything drawn over the video (menus, popovers, cards, the control bar), one table per player style
// (Theme.layout.player). The player is always dark, whatever the app theme, so these are fixed dark tones; the accent
// still comes from the theme (Theme.c.accent / accentFg), see readable() for accent-coloured text.
//   adw    GNOME (libadwaita OSD and popovers)
//   fluent Windows 11 (Media Player: acrylic flyouts, 4 px controls, 8 px overlays, accent pill on the selected row)
//   m3     Android (Material 3 dark surfaces, 28 px menus with 16 px items, secondary-container selection)
//   glass  Apple (visionOS / macOS media controls: translucent glass, light hairline, capsules, check on the right)
var T = {
    adw: {
        panel: "#f2303030", stroke: "#1fffffff", strokeW: 1, radius: 12, sheetR: 12, cardR: 12, pad: 6, shadow: true, glossy: false,
        bar: "#e6303030", barR: 12,
        fg: "#ffffff", dim: "#b3ffffff", muted: "#80ffffff",
        titleSize: 15, titleWeight: 700, headSize: 13, headWeight: 700, headColor: "#a6ffffff",
        rowH: 36, rowR: 6, rowPad: 10, rowWeight: 400, rowSize: 14,
        hover: "#14ffffff", press: "#24ffffff", sel: "#1affffff", selHover: "#29ffffff", selFg: "#ffffff",
        mark: "check",          // check | bar | fill
        markAccent: true,
        sep: "#1affffff",
        ctlR: 6, btnH: 34, btnPadX: 14, btn: "#1fffffff", btnHover: "#2effffff", btnPress: "#40ffffff", btnFg: "#ffffff", btnStroke: "#00000000", btnWeight: 700,
        active: "#33ffffff", activeFg: "#ffffff", ghostHover: "#26ffffff", ghostIdle: "#14ffffff",
        field: "#1affffff", fieldStroke: "#00000000",
        sw: "adw", slider: "adw", seg: "adw"
    },
    fluent: {
        panel: "#f02c2c2c", stroke: "#17ffffff", strokeW: 1, radius: 8, sheetR: 8, cardR: 8, pad: 4, shadow: true, glossy: false,
        bar: "#f0202020", barR: 0,
        fg: "#ffffff", dim: "#c5ffffff", muted: "#87ffffff",
        titleSize: 14, titleWeight: 600, headSize: 14, headWeight: 600, headColor: "#ffffff",
        rowH: 36, rowR: 4, rowPad: 12, rowWeight: 400, rowSize: 14,
        hover: "#0fffffff", press: "#0affffff", sel: "#0fffffff", selHover: "#15ffffff", selFg: "#ffffff",
        mark: "bar",
        markAccent: true,
        sep: "#17ffffff",
        ctlR: 4, btnH: 32, btnPadX: 12, btn: "#0fffffff", btnHover: "#15ffffff", btnPress: "#08ffffff", btnFg: "#ffffff", btnStroke: "#12ffffff", btnWeight: 400,
        active: "#15ffffff", activeFg: "#ffffff", ghostHover: "#15ffffff", ghostIdle: "#0fffffff",
        field: "#0bffffff", fieldStroke: "#14ffffff",
        sw: "fluent", slider: "fluent", seg: "fluent"
    },
    m3: {
        panel: "#f72b2930", stroke: "#00000000", strokeW: 0, radius: 28, sheetR: 28, cardR: 16, pad: 8, shadow: true, glossy: false,
        bar: "#f2211f26", barR: 28,
        fg: "#e6e0e9", dim: "#cac4d0", muted: "#938f99",
        titleSize: 16, titleWeight: 500, headSize: 14, headWeight: 500, headColor: "#cac4d0",
        rowH: 40, rowR: 16, rowPad: 14, rowWeight: 400, rowSize: 14,
        hover: "#14e6e0e9", press: "#1fe6e0e9", sel: "#4a4458", selHover: "#564f66", selFg: "#e8def8",
        mark: "fill",
        markAccent: false,
        sep: "#49454f",
        ctlR: 999, btnH: 40, btnPadX: 18, btn: "#4a4458", btnHover: "#564f66", btnPress: "#5f5770", btnFg: "#e8def8", btnStroke: "#00000000", btnWeight: 500,
        active: "#4a4458", activeFg: "#e8def8", ghostHover: "#1fe6e0e9", ghostIdle: "#14e6e0e9",
        field: "#36343b", fieldStroke: "#938f99",
        sw: "m3", slider: "m3", seg: "m3"
    },
    glass: {
        panel: "#e81e1e22", stroke: "#26ffffff", strokeW: 1, radius: 20, sheetR: 22, cardR: 20, pad: 6, shadow: false, glossy: true,
        bar: "#b31c1c22", barR: 26,
        fg: "#ffffff", dim: "#b3ffffff", muted: "#8cffffff",
        titleSize: 15, titleWeight: 600, headSize: 13, headWeight: 600, headColor: "#99ffffff",
        rowH: 36, rowR: 10, rowPad: 12, rowWeight: 400, rowSize: 14,
        hover: "#1affffff", press: "#29ffffff", sel: "#1fffffff", selHover: "#2effffff", selFg: "#ffffff",
        mark: "check",
        markAccent: false,
        sep: "#1fffffff",
        ctlR: 999, btnH: 34, btnPadX: 16, btn: "#26ffffff", btnHover: "#38ffffff", btnPress: "#4dffffff", btnFg: "#ffffff", btnStroke: "#14ffffff", btnWeight: 600,
        active: "#40ffffff", activeFg: "#ffffff", ghostHover: "#2effffff", ghostIdle: "#1affffff",
        field: "#1fffffff", fieldStroke: "#1affffff",
        sw: "ios", slider: "glass", seg: "glass"
    }
}

// The table of a player style (unknown styles look like GNOME).
function t(ps) { return T[ps] || T.adw }

// A radius that is never more than half the height (999 means "capsule").
function r(radius, h) { return Math.min(radius, h / 2) }

// The accent as text / an icon on the dark panels: a dark accent (light app themes) is mixed with white until it reads.
function readable(c) {
    if (!c) return "#ffffff"
    var lum = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
    if (lum >= 0.42) return c
    var k = Math.min(0.6, (0.42 - lum) * 1.6 + 0.2)
    return Qt.rgba(c.r + (1 - c.r) * k, c.g + (1 - c.g) * k, c.b + (1 - c.b) * k, 1)
}

// Where a menu opened from a button of the control bar should end: above the whole bar (seek line included), so it never
// covers the seek line. `y` is the button's top in window coordinates.
function above(btn, y) {
    for (var p = btn; p; p = p.parent)
        if (p.objectName === "osdBar") return Math.min(y, p.mapToItem(null, 0, p.panelTop).y)
    return y
}
