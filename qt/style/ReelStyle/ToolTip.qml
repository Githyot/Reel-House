import QtQuick
import QtQuick.Templates as T
import Reelhouse 1.0

// The hover tips of the whole app, drawn per theme. The Basic style's own tip is a white box with a black edge that
// ignores the theme (and the window palette, because the attached tip is one shared popup).
T.ToolTip {
    id: control
    readonly property bool dark: Theme.scheme !== "light"
    // GNOME: libadwaita's dark translucent tip; Material: the inverse surface; Windows and Apple: the popover colour
    // with a hairline edge.
    readonly property color fill: Theme.id === "gnome" ? "#e6202020"
        : Theme.id === "android" ? (dark ? "#e6e0e9" : "#322f35")
        : (Theme.c.popover || "#2c2c2e")
    readonly property color ink: Theme.id === "gnome" ? "#ffffff"
        : Theme.id === "android" ? (dark ? "#322f35" : "#f5eff7")
        : (Theme.c.text || "#ffffff")
    x: parent ? (parent.width - implicitWidth) / 2 : 0
    y: -implicitHeight - 6
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset, implicitContentHeight + topPadding + bottomPadding)
    margins: 6
    topPadding: 6
    bottomPadding: 6
    leftPadding: 10
    rightPadding: 10
    delay: 450
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutsideParent | T.Popup.CloseOnReleaseOutsideParent
    contentItem: Text {
        text: control.text
        font.family: Theme.type.family
        font.pixelSize: 13
        color: control.ink
        wrapMode: Text.Wrap
        width: Math.min(implicitWidth, 360)
    }
    background: Rectangle {
        radius: Theme.id === "windows" || Theme.id === "android" ? 4 : 6
        color: control.fill
        border.width: Theme.id === "windows" || Theme.id === "apple" ? 1 : 0
        border.color: control.dark ? "#1fffffff" : "#1f000000"
    }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 120 } }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 90 } }
}
