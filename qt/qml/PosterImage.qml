import QtQuick
import Reelhouse 1.0

// A picture from the web through the core-aware provider (WebP included), with a soft fade-in.
Image {
    id: im
    property string url: ""
    property string title: ""       // only used to find another cover by name when the address fails
    property string fallback: ""    // shown when the address fails (an episode without its own picture)
    property bool failed: false
    onUrlChanged: failed = false
    onStatusChanged: if (status === Image.Error && fallback !== "") failed = true
    readonly property string shown: failed && fallback !== "" ? fallback : url
    source: shown !== "" && shown !== "none" ? "image://web/" + encodeURIComponent(shown) + (title !== "" ? "@@" + encodeURIComponent(title) : "") + (Core.picGen > 0 ? "~~" + Core.picGen : "") : ""
    asynchronous: true
    cache: true
    fillMode: Image.PreserveAspectCrop
    opacity: status === Image.Ready ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Theme.motion.normal } }
}
