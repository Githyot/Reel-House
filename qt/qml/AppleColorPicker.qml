import QtQuick
import Reelhouse 1.0

// Apple (macOS / iOS): a compact round chromatic wheel (hue around, saturation outwards), a brightness slider, an
// opacity slider and the system colours.
Column {
    id: ap
    property var st
    spacing: 14
    readonly property var system: ["#ff3b30", "#ff9500", "#ffcc00", "#34c759", "#00c7be", "#30b0c7", "#32ade6", "#007aff", "#5856d6", "#af52de", "#ff2d55", "#a2845e", "#8e8e93", "#ffffff", "#000000"]

    Item {
        id: holder
        width: parent.width
        height: 226
        Item {
            id: wheel
            readonly property real d: 220
            width: d; height: d
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.verticalCenter: parent.verticalCenter
            Canvas {
                id: cv
                anchors.fill: parent
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset()
                    var r = width / 2
                    var g = ctx.createConicalGradient(r, r, 0)
                    for (var i = 0; i <= 12; i++) {
                        var c = Qt.hsva((i % 12) / 12, 1, 1, 1)
                        g.addColorStop(i / 12, "rgb(" + Math.round(c.r * 255) + "," + Math.round(c.g * 255) + "," + Math.round(c.b * 255) + ")")
                    }
                    ctx.fillStyle = g
                    ctx.beginPath(); ctx.arc(r, r, r - 1, 0, Math.PI * 2); ctx.fill()
                    var rg = ctx.createRadialGradient(r, r, 0, r, r, r)
                    rg.addColorStop(0, "rgba(255,255,255,1)")
                    rg.addColorStop(1, "rgba(255,255,255,0)")
                    ctx.fillStyle = rg
                    ctx.beginPath(); ctx.arc(r, r, r - 1, 0, Math.PI * 2); ctx.fill()
                }
                Component.onCompleted: requestPaint()
            }
            // Brightness darkens the wheel.
            Rectangle { anchors.fill: parent; anchors.margins: 1; radius: width / 2; color: "#000000"; opacity: 1 - ap.st.v }
            Rectangle { anchors.fill: parent; radius: width / 2; color: "transparent"; border.width: 1; border.color: "#33000000" }
            Rectangle {
                readonly property real a: ap.st.h * Math.PI / 180
                x: wheel.d / 2 + ap.st.s * (wheel.d / 2 - 1) * Math.cos(a) - width / 2
                y: wheel.d / 2 - ap.st.s * (wheel.d / 2 - 1) * Math.sin(a) - height / 2
                width: 26; height: 26; radius: 13
                color: ap.st.hex
                border.width: 3; border.color: "#ffffff"
                Rectangle { anchors.fill: parent; anchors.margins: -1; radius: width / 2; color: "transparent"; border.width: 1; border.color: "#59000000"; z: -1 }
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                function put(m) {
                    var r = width / 2, dx = m.x - r, dy = m.y - r
                    var s = Math.min(1, Math.sqrt(dx * dx + dy * dy) / r)
                    var h = Math.atan2(-dy, dx) * 180 / Math.PI
                    if (h < 0) h += 360
                    ap.st.setHsv(h, s, ap.st.v)
                }
                onPressed: function(m) { put(m) }
                onPositionChanged: function(m) { if (pressed) put(m) }
            }
        }
    }

    ChannelRow { width: parent.width; label: "☀"; kind: "apple"; from: 0; to: 100; value: ap.st.v * 100; suffix: "%"; c0: "#000000"; c1: Qt.hsva(ap.st.h / 360, ap.st.s, 1, 1); onMoved: function(v) { ap.st.setHsv(ap.st.h, ap.st.s, v / 100) } }
    ChannelRow { visible: ap.st.withAlpha; width: parent.width; label: "◐"; kind: "apple"; checker: true; from: 0; to: 100; value: ap.st.alpha * 100; suffix: "%"; c0: Qt.rgba(ap.st.r / 255, ap.st.g / 255, ap.st.b / 255, 0); c1: Qt.rgba(ap.st.r / 255, ap.st.g / 255, ap.st.b / 255, 1); onMoved: function(v) { ap.st.alpha = v / 100 } }
    SwatchGrid { width: parent.width; cell: 28; corner: 99; spacing: Math.max(4, (width - 8 * 28) / 7); colors: ap.system; selected: ap.st.hex; onPicked: function(h) { ap.st.setHex(h) } }
}
