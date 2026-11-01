import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// "Keep downloaded" (GTK dlpick.rs): choose the torrent to save on this computer for a season (or several).
Popup {
    id: dlg
    readonly property var d: Core.dlpick
    readonly property var seasons: d.seasons !== undefined ? d.seasons : []
    readonly property var list: d.list !== undefined ? d.list : []
    // Seasons ticked under "More seasons".
    property var extra: []
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(600, (parent ? parent.width : 700) - 40)
    height: Math.min(700, (parent ? parent.height : 760) - 40)
    modal: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    Overlay.modal: Rectangle { color: Theme.c.scrim }
    background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
    onAboutToShow: extra = []
    onClosed: if (Core.dlpick.open === true) Core.cmd({cmd: "dl_close"})
    // The core closes the picker when a download starts.
    Connections { target: Core; function onDlpickChanged() { if (Core.dlpick.open !== true && dlg.visible) dlg.close() } }

    function start(index) {
        var primary = dlg.d.season
        var ss = [primary]
        for (var i = 0; i < dlg.extra.length; ++i) if (dlg.extra[i] !== primary) ss.push(dlg.extra[i])
        Core.cmd({cmd: "dl_start", index: index, seasons: ss})
        dlg.close()
    }

    contentItem: Item {
        DialogHeader { id: head; title: Core.tr(Core.uiLang, "Keep downloaded"); onClose: dlg.close() }
        Flickable {
            id: scroll
            anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom }
            contentWidth: width
            contentHeight: col.implicitHeight + 36
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            Column {
                id: col
                x: 18
                y: 12
                width: Math.min(640, scroll.width - 36)
                spacing: 12
                PrefGroup {
                    PrefCombo {
                        title: Core.tr(Core.uiLang, "Season")
                        subtitle: Core.tr(Core.uiLang, "The torrents below are found for its first episode")
                        options: dlg.seasons.map(function(x) { return Core.tr(Core.uiLang, "Season") + " " + x })
                        current: Math.max(0, dlg.seasons.indexOf(dlg.d.season))
                        onPicked: function(i) { Core.cmd({cmd: "dl_load_streams", season: dlg.seasons[i]}) }
                    }
                }
                PrefGroup {
                    visible: dlg.seasons.length > 1
                    title: Core.tr(Core.uiLang, "More seasons")
                    description: Core.tr(Core.uiLang, "A torrent that holds several seasons can save them all. Tick the ones you also want.")
                    Repeater {
                        model: dlg.seasons
                        delegate: PrefSwitch {
                            required property var modelData
                            visible: modelData !== dlg.d.season
                            title: Core.tr(Core.uiLang, "Also season") + " " + modelData
                            checked: dlg.extra.indexOf(modelData) >= 0
                            onToggled: function(on) {
                                var e = dlg.extra.filter(function(x) { return x !== modelData })
                                if (on) e.push(modelData)
                                dlg.extra = e
                            }
                        }
                    }
                }
                Text { text: Core.tr(Core.uiLang, "Choose the torrent with the files you want"); color: Theme.c.text; font.pixelSize: 20; font.weight: Font.Bold; font.family: Theme.type.family }
                Text {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: dlg.d.loading === true ? Core.tr(Core.uiLang, "Asking your addons…") : (dlg.list.length === 0 ? Core.tr(Core.uiLang, "No torrents found for this season.") : Core.tr(Core.uiLang, "Every episode of the ticked seasons that the torrent holds is downloaded to this computer."))
                    color: Theme.c.textDim
                    font.pixelSize: 15
                    font.family: Theme.type.family
                }
                BusyIndicator { visible: dlg.d.loading === true; running: visible; implicitWidth: 32; implicitHeight: 32 }
                Column {
                    width: parent.width
                    spacing: 8
                    Repeater {
                        model: dlg.list
                        delegate: StreamRow {
                            required property var modelData
                            required property int index
                            width: col.width
                            stream: modelData
                            icon: "download"
                            onClicked: dlg.start(index)
                        }
                    }
                }
                Repeater {
                    model: (dlg.d.errors !== undefined ? dlg.d.errors : []).slice(0, 4)
                    delegate: Text { required property var modelData; width: col.width; wrapMode: Text.Wrap; text: modelData; color: Theme.c.error; font.pixelSize: 13; font.family: Theme.type.family }
                }
            }
        }
    }
}
