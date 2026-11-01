import QtQuick

// The fine grain of paper (the "Paper" theme of the readers): small soft specks spread evenly. The same specks every
// time (a fixed pseudo-random sequence), painted again only when the size changes.
Canvas {
    id: grain
    property color speck: "#000000"
    property real density: 0.0016          // specks per square pixel
    enabled: false
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onSpeckChanged: requestPaint()
    onPaint: {
        const c = getContext("2d")
        c.reset()
        let seed = 7
        function rnd() { seed = (seed * 16807) % 2147483647; return seed / 2147483647 }
        const n = Math.min(12000, Math.round(width * height * density))
        c.fillStyle = speck
        for (let i = 0; i < n; ++i) {
            c.globalAlpha = 0.03 + rnd() * 0.06
            const s = 0.6 + rnd() * 1.4
            c.fillRect(rnd() * width, rnd() * height, s, s)
        }
    }
}
