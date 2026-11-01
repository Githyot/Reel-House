import QtQuick
import Reelhouse 1.0

// The text of a speech bubble as the reader draws it: rich text in the bubble's own font, size and colour, with its
// highlights drawn behind the words as round "pills" (or plain boxes). Qt cannot round the background of a span, so
// Core.richPills turns the highlights into one rectangle per line and they are drawn here, under the text.
// With `source` set to another text edit (the bubble editor), only the highlights of that edit are drawn: put this
// item at the edit's own position and size (its background).
Item {
    id: bt
    property string text: ""
    property bool rich: true
    property string family: Theme.type.family
    property real pixelSize: 16
    property color color: "#111111"
    property int weight: Font.DemiBold
    property bool pill: true                       // round ends; false draws plain boxes
    property real highlightOpacity: 1
    property int horizontalAlignment: TextEdit.AlignHCenter
    property int verticalAlignment: TextEdit.AlignVCenter
    property var source: te          // a TextEdit or TextArea
    readonly property real contentHeight: te.contentHeight
    readonly property real contentWidth: te.contentWidth

    // the rectangles, in the coordinates of `source`
    property var pills: []
    function refresh() {
        const s = bt.source
        if (!s || !s.textDocument || (s === te && !bt.rich)) { if (pills.length > 0) pills = []; return }
        const r = Core.richPills(s.textDocument)
        const list = r.pills || []
        if (list.length === 0) { if (pills.length > 0) pills = []; return }
        // document coordinates -> the edit: where position 0 is in both
        const o = s.positionToRectangle(0)
        const dx = o.x - (r.ox || 0), dy = o.y - (r.oy || 0)
        pills = list.map(function(p) { return {x: p.x + dx, y: p.y + dy, w: p.w, h: p.h, c: p.c} })
    }
    function later() { Qt.callLater(bt.refresh) }
    Connections {
        target: bt.source
        ignoreUnknownSignals: true
        function onTextChanged() { bt.later() }
        function onWidthChanged() { bt.later() }
        function onHeightChanged() { bt.later() }
        function onContentSizeChanged() { bt.later() }        // TextEdit's notifier for contentWidth/Height
    }
    onSourceChanged: later()
    onPixelSizeChanged: later()
    onFamilyChanged: later()
    onWeightChanged: later()
    onRichChanged: later()
    Component.onCompleted: later()

    Repeater {
        model: bt.pills.length
        delegate: Rectangle {
            required property int index
            readonly property var p: bt.pills[index] || ({x: 0, y: 0, w: 0, h: 0, c: "transparent"})
            // a pill reaches a little past the first and last letter and is a touch shorter than the line
            readonly property real pad: bt.pill ? p.h * 0.28 : 1
            readonly property real inset: bt.pill ? p.h * 0.05 : 0
            x: p.x - pad
            y: p.y + inset
            width: p.w + 2 * pad
            height: Math.max(0, p.h - 2 * inset)
            radius: bt.pill ? height / 2 : 2
            color: p.c
            opacity: bt.highlightOpacity
        }
    }

    TextEdit {
        id: te
        anchors.fill: parent
        visible: bt.source === te
        enabled: false               // drawn only: the bubble's own mouse area takes the clicks
        readOnly: true
        selectByMouse: false
        activeFocusOnPress: false
        text: bt.source === te ? bt.text : ""
        textFormat: bt.rich ? TextEdit.RichText : TextEdit.PlainText
        wrapMode: TextEdit.Wrap
        color: bt.color
        font.family: bt.family
        font.pixelSize: Math.max(1, bt.pixelSize)
        font.weight: bt.weight
        horizontalAlignment: bt.horizontalAlignment
        verticalAlignment: bt.verticalAlignment
    }
}
