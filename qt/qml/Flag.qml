import QtQuick
import Reelhouse 1.0

// A small country flag drawn in code, for the audio languages of a stream. Windows has no flag emojis, a Qt build may
// lack the SVG plugin, and Shape items have failed to draw on HP's Fedora, so a flag is only plain Rectangles (and a
// ★ glyph): each layer is a box on a 30x20 grid {x, y, w, h, c} with optional round (circle), rot (degrees), sy (squash
// after turning), ch (keep only the top part, 0..1), b (ring colour, no fill) and t (a glyph in colour c).
// `code` is the country ("br", "us", "jp"…); a country without a drawing shows its code on a chip.
Item {
    id: fl
    property string code: ""
    property int h: 14
    implicitHeight: h
    implicitWidth: Math.round(h * 1.5)
    readonly property real k: width / 30

    function bg(c) { return { x: 0, y: 0, w: 30, h: 20, c: c } }
    function stripesH(cols) {
        var out = [], s = 20 / cols.length
        for (var i = 0; i < cols.length; i++)
            out.push({ x: 0, y: i * s, w: 30, h: s + 0.2, c: cols[i] })
        return out
    }
    function stripesV(cols) {
        var out = [], s = 30 / cols.length
        for (var i = 0; i < cols.length; i++)
            out.push({ x: i * s, y: 0, w: s + 0.2, h: 20, c: cols[i] })
        return out
    }
    function dot(cx, cy, r, c) { return { x: cx - r, y: cy - r, w: 2 * r, h: 2 * r, c: c, round: true } }
    function starAt(cx, cy, size, c) { return { x: cx - size / 2, y: cy - size / 2, w: size, h: size, c: c, t: "★" } }
    readonly property var layers: {
        var i, out
        switch (code) {
        case "br": return [bg("#009c3b"), { x: 6.16, y: 1.16, w: 17.68, h: 17.68, c: "#ffdf00", rot: 45, sy: 0.624 }, dot(15, 10, 4.6, "#002776")]
        case "pt": return [bg("#da291c"), { x: 0, y: 0, w: 12, h: 20, c: "#046a38" }, dot(12, 10, 3.6, "#ffe900")]
        case "us":
            out = [bg("#ffffff")]
            for (i = 0; i < 13; i += 2)
                out.push({ x: 0, y: i * 20 / 13, w: 30, h: 20 / 13, c: "#b22234" })
            out.push({ x: 0, y: 0, w: 13, h: 10.77, c: "#3c3b6e" })
            return out
        case "gb": return [bg("#012169"),
                           { x: -3, y: 8, w: 36, h: 4, c: "#ffffff", rot: 33.7 }, { x: -3, y: 8, w: 36, h: 4, c: "#ffffff", rot: -33.7 },
                           { x: -3, y: 9.3, w: 36, h: 1.4, c: "#c8102e", rot: 33.7 }, { x: -3, y: 9.3, w: 36, h: 1.4, c: "#c8102e", rot: -33.7 },
                           { x: 12, y: 0, w: 6, h: 20, c: "#ffffff" }, { x: 0, y: 7, w: 30, h: 6, c: "#ffffff" },
                           { x: 13.2, y: 0, w: 3.6, h: 20, c: "#c8102e" }, { x: 0, y: 8.2, w: 30, h: 3.6, c: "#c8102e" }]
        case "jp": return [bg("#ffffff"), dot(15, 10, 6, "#bc002d")]
        case "es": return [bg("#aa151b"), { x: 0, y: 5, w: 30, h: 10, c: "#f1bf00" }]
        case "mx": return stripesV(["#006847", "#ffffff", "#ce1126"]).concat([dot(15, 10, 2.6, "#8c5a2b")])
        case "fr": return stripesV(["#0055a4", "#ffffff", "#ef4135"])
        case "it": return stripesV(["#009246", "#ffffff", "#ce2b37"])
        case "de": return stripesH(["#000000", "#dd0000", "#ffce00"])
        case "ru": return stripesH(["#ffffff", "#0039a6", "#d52b1e"])
        case "nl": return stripesH(["#ae1c28", "#ffffff", "#21468b"])
        case "pl": return stripesH(["#ffffff", "#dc143c"])
        case "ua": return stripesH(["#0057b7", "#ffd700"])
        case "in": return stripesH(["#ff9933", "#ffffff", "#138808"]).concat([{ x: 12.4, y: 7.4, w: 5.2, h: 5.2, c: "transparent", round: true, b: "#000080" }])
        case "th": return [bg("#a51931"), { x: 0, y: 3.3, w: 30, h: 13.4, c: "#f4f5f8" }, { x: 0, y: 6.7, w: 30, h: 6.6, c: "#2d2a4a" }]
        case "kr": return [bg("#ffffff"), dot(15, 10, 5, "#0047a0"), { x: 10, y: 5, w: 10, h: 10, c: "#cd2e3a", round: true, ch: 0.5 },
                           { x: 3, y: 2.6, w: 5, h: 1.2, c: "#000000", rot: 33.7 }, { x: 22, y: 16.2, w: 5, h: 1.2, c: "#000000", rot: 33.7 },
                           { x: 22, y: 2.6, w: 5, h: 1.2, c: "#000000", rot: -33.7 }, { x: 3, y: 16.2, w: 5, h: 1.2, c: "#000000", rot: -33.7 }]
        case "cn": return [bg("#de2910"), starAt(5.5, 5.5, 7, "#ffde00"), starAt(11, 2.4, 2.6, "#ffde00"), starAt(12.8, 4.8, 2.6, "#ffde00"),
                           starAt(12.8, 7.8, 2.6, "#ffde00"), starAt(11, 10.2, 2.6, "#ffde00")]
        case "tr": return [bg("#e30a17"), dot(11.5, 10, 5, "#ffffff"), dot(12.8, 10, 4, "#e30a17"), starAt(19, 10, 5, "#ffffff")]
        case "sa": return [bg("#006c35"), { x: 8, y: 13, w: 14, h: 1.2, c: "#ffffff" }]
        default: return []
        }
    }

    Item {
        anchors.fill: parent
        clip: true
        visible: fl.layers.length > 0
        Repeater {
            model: fl.layers
            delegate: Item {
                id: lay
                required property var modelData
                readonly property real bw: modelData.w * fl.k
                readonly property real bh: modelData.h * fl.k
                x: modelData.x * fl.k
                y: modelData.y * fl.k
                width: bw
                height: modelData.ch ? bh * modelData.ch : bh
                clip: !!modelData.ch
                transform: [
                    Rotation { origin.x: lay.bw / 2; origin.y: lay.bh / 2; angle: lay.modelData.rot || 0 },
                    Scale { origin.x: lay.bw / 2; origin.y: lay.bh / 2; yScale: lay.modelData.sy || 1 }
                ]
                Rectangle {
                    width: lay.bw
                    height: lay.bh
                    antialiasing: true
                    radius: lay.modelData.round ? width / 2 : 0
                    color: lay.modelData.t || lay.modelData.b ? "transparent" : lay.modelData.c
                    border.width: lay.modelData.b ? Math.max(1, fl.k * 0.7) : 0
                    border.color: lay.modelData.b || "transparent"
                    Text {
                        visible: !!lay.modelData.t
                        anchors.centerIn: parent
                        text: lay.modelData.t || ""
                        color: lay.modelData.c
                        font.pixelSize: Math.max(4, Math.round(lay.bh * 1.15))
                    }
                }
            }
        }
    }
    // a country without a drawing: its code on a chip
    Rectangle {
        anchors.fill: parent
        visible: fl.layers.length === 0
        color: Theme.c.cardHover
        Text { anchors.centerIn: parent; text: fl.code.toUpperCase(); color: Theme.c.text; font.pixelSize: Math.round(fl.h * 0.62); font.weight: Font.Bold; font.family: Theme.type.family }
    }
    // a thin edge, so white flags stand apart from a light card
    Rectangle { anchors.fill: parent; color: "transparent"; border.width: 1; border.color: Theme.scheme === "light" ? "#33000000" : "#40ffffff" }
}
