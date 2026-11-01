import QtQuick
import Reelhouse 1.0

// Text mode (novels and own books): Library, Explore and History, the tab comes from the header bar.
ModeBrowser {
    kind: "novel"
    property string mtab: "library"
    tab: mtab
}
