import QtQuick
import Reelhouse 1.0
import "Osd.js" as Osd

// A heading in a player menu. `title`: the name of the popover (bigger, full white);
// otherwise a section heading (smaller and dimmer, like the libadwaita / Fluent / M3 / Apple list headers).
Text {
    property bool title: false
    readonly property var o: Osd.t(Theme.layout.player)
    leftPadding: o.rowPad
    rightPadding: o.rowPad
    topPadding: title ? 6 : 10
    bottomPadding: 6
    color: title ? o.fg : o.headColor
    font.pixelSize: title ? o.titleSize : o.headSize
    font.weight: title ? o.titleWeight : o.headWeight
    font.family: Theme.type.family
    elide: Text.ElideRight
}
