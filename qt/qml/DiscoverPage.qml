import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Discover (GTK browse::rebuild_home): the featured banner, Continue watching, Favorites, "people who liked it" rows and the
// catalogs of the addons. Catalog > Platforms keeps only the rows (and banner titles) of one platform.
Item {
    id: page
    property string platform: ""
    readonly property var home: Core.home
    readonly property var allRows: home.rows !== undefined ? home.rows : []
    readonly property var rows: platform === "" ? allRows : allRows.filter(function(r) { return r.platform === page.platform })
    readonly property var hero: (home.hero !== undefined ? home.hero : []).filter(function(h) { return page.platform === "" || h.platform === page.platform }).slice(0, 6)
    readonly property var errors: home.errors !== undefined ? home.errors : []

    Flickable {
        id: scroll
        visible: page.rows.length > 0
        anchors.fill: parent
        contentWidth: width
        contentHeight: col.implicitHeight + 40
        clip: true
        boundsBehavior: Theme.mobile ? Flickable.DragOverBounds : Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { }

        Column {
            id: col
            x: Theme.mobile ? 16 : 18
            y: 14
            width: scroll.width - (Theme.mobile ? 32 : 36)
            spacing: Theme.mobile ? 24 : 30

            HeroBanner { visible: items.length > 0; width: parent.width; items: page.hero }
            ContinueRow { visible: items.length > 0; width: parent.width; items: page.home.continue !== undefined ? page.home.continue : [] }
            PosterRow { visible: items.length > 0; width: parent.width; title: Core.tr(Core.uiLang, "Favorites"); items: page.home.favorites !== undefined ? page.home.favorites : [] }
            Repeater {
                // the number is the model, so the rows stay (and keep their scroll) when only their data changes
                model: (page.home.taste || []).length
                delegate: PosterRow { required property int index; readonly property var modelData: (page.home.taste || [])[index] || ({}); width: col.width; title: modelData.title; addon: Core.tr(Core.uiLang, "people who liked it also liked these"); items: modelData.items }
            }
            Repeater {
                model: page.rows.length
                delegate: PosterRow { required property int index; readonly property var modelData: page.rows[index] || ({}); width: col.width; title: modelData.title || ""; addon: modelData.addon || ""; items: modelData.items || [] }
            }
            // Addons that did not answer (GTK: "Some addons did not answer").
            PrefGroup {
                visible: page.errors.length > 0
                width: parent.width
                title: Core.tr(Core.uiLang, "Some addons did not answer")
                Repeater {
                    model: page.errors.slice(0, 6)
                    delegate: PrefRow { required property var modelData; title: modelData }
                }
            }
            Item {
                width: parent.width
                height: 60
                Text { anchors.centerIn: parent; visible: page.home.loading === true; text: Core.tr(Core.uiLang, "Loading the catalogs…"); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
            }
        }
    }
    // a phone: the page pulled down past its top asks the addons for the catalogs again
    PullToRefresh { target: scroll; onRefresh: Core.cmd({cmd: "load_home"}) }

    // Nothing yet: loading, or the status page of GTK.
    Column {
        anchors.centerIn: parent
        visible: page.rows.length === 0
        width: Math.min(parent.width - 60, 640)
        spacing: 12
        BusyIndicator { anchors.horizontalCenter: parent.horizontalCenter; visible: page.home.loading === true; running: visible }
        Text {
            visible: page.home.loading === true
            anchors.horizontalCenter: parent.horizontalCenter
            text: Core.tr(Core.uiLang, "Loading catalogs from your addons")
            color: Theme.c.textDim
            font.pixelSize: 15
            font.family: Theme.type.family
        }
        Mascot { visible: page.home.loading !== true; animate: visible; anchors.horizontalCenter: parent.horizontalCenter; size: 120; mood: page.errors.length > 0 ? "sad" : "sleep" }
        Text { visible: page.home.loading !== true; anchors.horizontalCenter: parent.horizontalCenter; text: Core.tr(Core.uiLang, "Nothing to show yet"); color: Theme.c.text; font.pixelSize: 24; font.weight: Font.ExtraBold; font.family: Theme.type.family }
        Text { visible: page.home.loading !== true; width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: Core.tr(Core.uiLang, "Check your internet connection, or add addons in Preferences."); color: Theme.c.textDim; font.pixelSize: 15; font.family: Theme.type.family }
        PrefGroup {
            visible: page.errors.length > 0 && page.home.loading !== true
            width: parent.width
            title: Core.tr(Core.uiLang, "Some addons did not answer")
            Repeater {
                model: page.errors.slice(0, 6)
                delegate: PrefRow { required property var modelData; title: modelData }
            }
        }
    }
}
