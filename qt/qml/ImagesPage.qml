import QtQuick
import Reelhouse 1.0

// Images mode (manga and comics): Library, Explore and History, the tab comes from the header bar.
ModeBrowser {
    kind: "manga"
    property string mtab: "library"
    tab: mtab
}
