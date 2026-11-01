import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// "Cover and background" of a title (GTK artdialog.rs): the size and shape of the current pictures, a new image from
// the computer, fitted automatically or with a crop frame. Custom pictures are kept in the data folder.
Popup {
    id: dlg
    readonly property var art: Core.detail.art !== undefined ? Core.detail.art : ({})
    property string page: "main"          // main | choice | crop
    property string which: "poster"       // poster | background
    property string file: ""              // the chosen image (local path)
    property int iw: 0
    property int ih: 0
    readonly property int tw: which === "poster" ? 600 : 1920
    readonly property int th: which === "poster" ? 900 : 1080
    readonly property real ratio: tw / th
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(760, (parent ? parent.width : 800) - 40)
    height: Math.min(680, (parent ? parent.height : 700) - 40)
    modal: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    Overlay.modal: Rectangle { color: "#80000000" }
    background: Glass { radius: Theme.shape.sheet; level: "thick"; solid: Theme.c.window; shadow: true }
    onAboutToShow: page = "main"

    function shapeName(w, h) {
        var r = w / Math.max(1, h)
        var known = [["1:1", 1], ["2:3", 2 / 3], ["3:4", 0.75], ["4:5", 0.8], ["9:16", 9 / 16], ["16:9", 16 / 9], ["16:10", 1.6], ["4:3", 4 / 3], ["3:2", 1.5], ["21:9", 21 / 9]]
        for (var i = 0; i < known.length; ++i) if (Math.abs(r - known[i][1]) < 0.03) return known[i][0]
        return Core.tr(Core.uiLang, "custom")
    }
    function describe(w, h) { return w + " × " + h + " px · " + Core.tr(Core.uiLang, "ratio") + " " + (w / Math.max(1, h)).toFixed(2) + " (" + shapeName(w, h) + ")" }
    function needed(wh) { var w = wh === "poster" ? 600 : 1920, h = wh === "poster" ? 900 : 1080; return Core.tr(Core.uiLang, "Best shape") + ": " + shapeName(w, h) + " (" + (wh === "poster" ? Core.tr(Core.uiLang, "tall cover") : Core.tr(Core.uiLang, "wide wallpaper")) + "), " + Core.tr(Core.uiLang, "up to") + " " + w + " × " + h + " px" }
    function pathOf(u) { var s = u.toString(); return s.indexOf("file:") === 0 ? decodeURIComponent(s.replace(/^file:\/{2,3}/, Qt.platform.os === "windows" ? "" : "/")) : s }
    function finish(r) { if (r.ok) dlg.close(); else Core.toast(r.error) }

    FileDialog {
        id: pickDlg
        title: dlg.which === "poster" ? Core.tr(Core.uiLang, "Choose a cover image") : Core.tr(Core.uiLang, "Choose a background image")
        nameFilters: [Core.tr(Core.uiLang, "Images") + " (*.png *.jpg *.jpeg *.webp *.gif *.bmp)"]
        onAccepted: {
            var f = dlg.pathOf(selectedFile)
            var info = Core.imageInfo(f)
            if (info.w === undefined) { Core.toast(Core.tr(Core.uiLang, "Could not read that image.")); return }
            dlg.file = f
            dlg.iw = info.w
            dlg.ih = info.h
            dlg.page = "choice"
        }
    }

    contentItem: Item {
        DialogHeader {
            id: head
            title: Core.tr(Core.uiLang, "Cover and background")
            backVisible: dlg.page !== "main"
            onBack: dlg.page = dlg.page === "crop" ? "choice" : "main"
            onClose: dlg.close()
        }

        // ================= the two pictures =================
        Flickable {
            visible: dlg.page === "main"
            anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom }
            contentHeight: mainCol.implicitHeight + 32
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: mainCol
                x: 18
                y: 16
                width: parent.width - 36
                spacing: 18
                Repeater {
                    model: ["poster", "background"]
                    delegate: Row {
                        id: artRow
                        required property string modelData
                        readonly property string url: (modelData === "poster" ? dlg.art.poster : dlg.art.background) || ""
                        readonly property string orig: (modelData === "poster" ? dlg.art.origPoster : dlg.art.origBackground) || ""
                        readonly property bool hidden: url === "none"
                        spacing: 16
                        width: mainCol.width
                        Rectangle {
                            width: artRow.modelData === "poster" ? 110 : 240
                            height: artRow.modelData === "poster" ? 165 : 135
                            radius: Theme.shape.poster
                            color: Theme.c.placeholder
                            clip: true
                            // The real size is read from the picture itself (no sourceSize).
                            Image { id: nowPic; anchors.fill: parent; fillMode: Image.PreserveAspectCrop; asynchronous: true; source: artRow.url !== "" && !artRow.hidden ? "image://web/" + encodeURIComponent(artRow.url) : "" }
                        }
                        Image { id: origPic; visible: false; asynchronous: true; source: artRow.orig !== "" && artRow.orig !== artRow.url ? "image://web/" + encodeURIComponent(artRow.orig) : "" }
                        Column {
                            width: artRow.width - (artRow.modelData === "poster" ? 110 : 240) - 16
                            spacing: 4
                            Text { text: artRow.modelData === "poster" ? Core.tr(Core.uiLang, "Cover") : Core.tr(Core.uiLang, "Background"); color: Theme.c.text; font.pixelSize: 20; font.weight: Font.Bold; font.family: Theme.type.family }
                            Text {
                                width: parent.width; wrapMode: Text.Wrap
                                text: Core.tr(Core.uiLang, "Now") + ": " + (artRow.hidden ? Core.tr(Core.uiLang, "Hidden") : (artRow.url === "" ? Core.tr(Core.uiLang, "No picture") : (nowPic.status === Image.Ready ? dlg.describe(nowPic.implicitWidth, nowPic.implicitHeight) : Core.tr(Core.uiLang, "Size unknown until the picture has loaded"))))
                                color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family
                            }
                            Text {
                                visible: origPic.status === Image.Ready
                                width: parent.width; wrapMode: Text.Wrap
                                text: Core.tr(Core.uiLang, "Original from the addon") + ": " + dlg.describe(origPic.implicitWidth, origPic.implicitHeight)
                                color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family
                            }
                            Text { width: parent.width; wrapMode: Text.Wrap; text: dlg.needed(artRow.modelData); color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family }
                            Item { width: 1; height: 6 }
                            Flow {
                                width: parent.width
                                spacing: 8
                                FlatButton { kind: "suggested"; text: Core.tr(Core.uiLang, "Choose an image…"); implicitHeight: 36; onClicked: { dlg.which = artRow.modelData; pickDlg.open() } }
                                FlatButton {
                                    text: artRow.hidden ? Core.tr(Core.uiLang, "Show again") : Core.tr(Core.uiLang, "Hide"); implicitHeight: 36
                                    onClicked: { Core.cmd(artRow.hidden ? {cmd: "set_art", which: artRow.modelData, reset: true} : {cmd: "set_art", which: artRow.modelData, value: "none"}); dlg.close() }
                                }
                                FlatButton { visible: dlg.art.override === true; text: Core.tr(Core.uiLang, "Restore original"); implicitHeight: 36; onClicked: { Core.cmd({cmd: "set_art", which: artRow.modelData, reset: true}); dlg.close() } }
                            }
                        }
                    }
                }
            }
        }

        // ================= the chosen image: automatic or manual fit =================
        Column {
            visible: dlg.page === "choice"
            anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom; margins: 18 }
            spacing: 12
            readonly property bool shapeOk: Math.abs(dlg.iw / Math.max(1, dlg.ih) - dlg.ratio) < 0.02
            readonly property bool tooBig: dlg.iw > dlg.tw || dlg.ih > dlg.th
            Image {
                width: parent.width
                height: Math.max(120, parent.height - 220)
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                source: dlg.file !== "" && dlg.page !== "main" ? "file:///" + dlg.file.replace(/^\//, "") : ""
            }
            Text { text: Core.tr(Core.uiLang, "Your image") + ": " + dlg.describe(dlg.iw, dlg.ih); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family }
            Text { text: dlg.needed(dlg.which); color: Theme.c.textDim; font.pixelSize: 14; font.family: Theme.type.family }
            Text {
                width: parent.width; wrapMode: Text.Wrap
                color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family
                text: parent.shapeOk ? (parent.tooBig ? Core.tr(Core.uiLang, "The shape is right, but the image is larger than needed. Automatic mode shrinks it.") : Core.tr(Core.uiLang, "The shape and size are right for the app."))
                                     : (parent.tooBig ? Core.tr(Core.uiLang, "The image is larger than needed and has another shape. Choose how to fit it.") : Core.tr(Core.uiLang, "The shape does not match. Cropping is needed so it is not stretched."))
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 10
                FlatButton { pill: true; text: Core.tr(Core.uiLang, "Back"); onClicked: dlg.page = "main" }
                FlatButton { pill: true; text: Core.tr(Core.uiLang, "Manual: choose the crop"); onClicked: { crop.reset(); dlg.page = "crop" } }
                FlatButton {
                    pill: true; kind: "suggested"; text: Core.tr(Core.uiLang, "Automatic: shrink to fit")
                    ToolTip.visible: hovered; ToolTip.text: Core.tr(Core.uiLang, "Cuts the edges evenly to the right shape and lowers the resolution")
                    onClicked: dlg.finish(Core.saveArt(dlg.file, dlg.which, dlg.art.dir || "", Qt.rect(0, 0, 0, 0)))
                }
            }
        }

        // ================= crop frame =================
        Column {
            id: crop
            visible: dlg.page === "crop"
            anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom; margins: 18 }
            spacing: 10
            // The frame in image pixels.
            property real frac: 1
            property real cx: 0
            property real cy: 0
            function maxFrame() { return dlg.iw / dlg.ih > dlg.ratio ? [dlg.ih * dlg.ratio, dlg.ih] : [dlg.iw, dlg.iw / dlg.ratio] }
            readonly property real fw: maxFrame()[0] * frac
            readonly property real fh: maxFrame()[1] * frac
            function reset() { frac = 1; var m = maxFrame(); cx = (dlg.iw - m[0]) / 2; cy = (dlg.ih - m[1]) / 2; zoom.value = 100 }
            function setFrac(f) {
                var ox = cx + fw / 2, oy = cy + fh / 2
                frac = Math.max(0.1, Math.min(1, f))
                cx = Math.max(0, Math.min(dlg.iw - fw, ox - fw / 2))
                cy = Math.max(0, Math.min(dlg.ih - fh, oy - fh / 2))
            }
            Text { width: parent.width; wrapMode: Text.Wrap; text: Core.tr(Core.uiLang, "Drag the frame to choose what stays. The frame keeps the right shape") + " (" + dlg.shapeName(dlg.tw, dlg.th) + ")."; color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family }
            Item {
                id: stage
                width: parent.width
                height: Math.max(200, parent.height - 170)
                // How the image sits when shown whole.
                readonly property real sc: Math.min(width / Math.max(1, dlg.iw), height / Math.max(1, dlg.ih))
                readonly property real ox: (width - dlg.iw * sc) / 2
                readonly property real oy: (height - dlg.ih * sc) / 2
                Image { anchors.fill: parent; fillMode: Image.PreserveAspectFit; asynchronous: true; source: dlg.file !== "" && dlg.page === "crop" ? "file:///" + dlg.file.replace(/^\//, "") : "" }
                // Dark outside the frame.
                Rectangle { x: stage.ox; y: stage.oy; width: dlg.iw * stage.sc; height: frameBox.y - stage.oy; color: "#99000000" }
                Rectangle { x: stage.ox; y: frameBox.y + frameBox.height; width: dlg.iw * stage.sc; height: stage.oy + dlg.ih * stage.sc - y; color: "#99000000" }
                Rectangle { x: stage.ox; y: frameBox.y; width: frameBox.x - stage.ox; height: frameBox.height; color: "#99000000" }
                Rectangle { x: frameBox.x + frameBox.width; y: frameBox.y; width: stage.ox + dlg.iw * stage.sc - x; height: frameBox.height; color: "#99000000" }
                Rectangle {
                    id: frameBox
                    x: stage.ox + crop.cx * stage.sc
                    y: stage.oy + crop.cy * stage.sc
                    width: crop.fw * stage.sc
                    height: crop.fh * stage.sc
                    color: "transparent"
                    border.width: 2
                    border.color: "#f2ffffff"
                    // Thirds.
                    Repeater {
                        model: [1, 2]
                        delegate: Item {
                            required property int modelData
                            Rectangle { x: frameBox.width * modelData / 3; width: 1; height: frameBox.height; color: "#59ffffff" }
                            Rectangle { y: frameBox.height * modelData / 3; height: 1; width: frameBox.width; color: "#59ffffff" }
                        }
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.SizeAllCursor
                    property real px: 0
                    property real py: 0
                    property real sx: 0
                    property real sy: 0
                    onPressed: function(m) { px = m.x; py = m.y; sx = crop.cx; sy = crop.cy }
                    onPositionChanged: function(m) {
                        crop.cx = Math.max(0, Math.min(dlg.iw - crop.fw, sx + (m.x - px) / stage.sc))
                        crop.cy = Math.max(0, Math.min(dlg.ih - crop.fh, sy + (m.y - py) / stage.sc))
                    }
                    onWheel: function(w) { zoom.value = Math.max(10, Math.min(100, zoom.value + (w.angleDelta.y > 0 ? -4 : 4))) }
                }
            }
            Row {
                width: parent.width
                spacing: 10
                Text { anchors.verticalCenter: parent.verticalCenter; text: Core.tr(Core.uiLang, "Frame size"); color: Theme.c.text; font.pixelSize: 14; font.family: Theme.type.family }
                Slider {
                    id: zoom
                    width: parent.width - 100
                    from: 10; to: 100; value: 100
                    onValueChanged: crop.setFrac(value / 100)
                }
            }
            Text {
                readonly property bool big: crop.fw > dlg.tw
                text: Core.tr(Core.uiLang, "Crop") + ": " + Math.round(crop.fw) + " × " + Math.round(crop.fh) + " px " + Core.tr(Core.uiLang, "from the original") + " → " + Core.tr(Core.uiLang, "saved as") + " " + (big ? dlg.tw : Math.round(crop.fw)) + " × " + (big ? dlg.th : Math.round(crop.fh)) + " px"
                color: Theme.c.text; font.pixelSize: 14; font.weight: Font.Bold; font.family: Theme.type.family
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 10
                FlatButton { pill: true; text: Core.tr(Core.uiLang, "Back"); onClicked: dlg.page = "choice" }
                FlatButton { pill: true; kind: "suggested"; text: Core.tr(Core.uiLang, "Use this crop"); onClicked: dlg.finish(Core.saveArt(dlg.file, dlg.which, dlg.art.dir || "", Qt.rect(crop.cx, crop.cy, crop.fw, crop.fh))) }
            }
        }
    }
}
