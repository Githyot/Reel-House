import QtQuick
import Reelhouse 1.0

// A label, a colour slider and its number.
Row {
    id: cr
    property string label: ""
    property string kind: "adw"
    property real from: 0
    property real to: 255
    property real value: 0
    property bool rainbow: false
    property bool checker: false
    property color c0: "#000000"
    property color c1: "#ffffff"
    property int digits: 0
    property string suffix: ""
    signal moved(real v)
    spacing: 10
    height: Math.max(sld.implicitHeight, 26)
    Text { width: 20; anchors.verticalCenter: parent.verticalCenter; text: cr.label; color: Theme.c.textDim; font.pixelSize: 13; font.weight: Font.DemiBold; font.family: Theme.type.family }
    ColorSlider {
        id: sld
        width: cr.width - 20 - 46 - cr.spacing * 2
        anchors.verticalCenter: parent.verticalCenter
        kind: cr.kind; from: cr.from; to: cr.to; value: cr.value; rainbow: cr.rainbow; checker: cr.checker; c0: cr.c0; c1: cr.c1
        onMoved: function(v) { cr.moved(v) }
    }
    Text { width: 46; horizontalAlignment: Text.AlignRight; anchors.verticalCenter: parent.verticalCenter; text: Number(cr.value).toFixed(cr.digits) + cr.suffix; color: Theme.c.text; font.pixelSize: 13; font.family: Theme.type.family }
}
