import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// The profiles of the people who share this computer (src/profiles.rs): each one has its own library, favourites,
// history, study and added books; the preferences, addons and keys are everyone's. A first page lists them (a click
// uses that profile), a second one edits a profile: its name, one of the twelve pictures of the app or a photo of
// the computer (its middle square, at 256 x 256). Drawn by Popover.qml: a popover beside the sidebar, a bottom sheet
// on a phone.
Popover {
    id: pp
    readonly property var st: Core.profiles || ({})
    readonly property var people: st.list || []
    readonly property int presetCount: st.presets || 12
    readonly property real w: Math.min(320, pp.ovW - 16)
    property string page: "list"        // list | edit
    property string editId: ""          // the profile being edited; "" = a new one
    property bool editMain: false
    property string editName: ""        // its name as it was ("" = never named)
    property int editPreset: 0
    property string editPhoto: ""       // its photo, or the one just prepared (photoNew)
    property bool photoNew: false
    property bool avatarTouched: false  // a picture was chosen in the editor
    property bool confirming: false     // "Delete profile" asks once more
    // a profile was switched to: the sidebar takes the window back to the start page
    signal switched()
    function tr(s) { return Core.tr(Core.uiLang, s) }
    function shownName(p) { return p.name !== undefined && p.name !== "" ? p.name : tr("Main profile") }
    function edit(p) {
        editId = p ? p.id : ""
        editMain = p ? p.main === true : false
        editName = p ? (p.name || "") : ""
        editPreset = p ? (p.preset || 0) : (people.length % presetCount)
        editPhoto = p ? (p.photo || "") : ""
        photoNew = false
        avatarTouched = false
        confirming = false
        nameField.text = p ? shownName(p) : ""
        page = "edit"
    }
    function choose(p) {
        if (p.active) { close(); return }
        const r = Core.cmd({cmd: "profile_switch", id: p.id})
        if (r.ok === false) { Core.toast(tr(r.error)); return }
        close()
        Core.toast(tr("Profile in use:") + " " + shownName(p))
        switched()
    }
    function save() {
        const typed = nameField.text.trim()
        let r = {ok: true}
        if (editId === "") {
            r = Core.cmd({cmd: "profile_add", name: typed !== "" ? typed : nameField.placeholderText, preset: editPreset, photo: photoNew ? editPhoto : ""})
        } else {
            // the main profile keeps no name until one is typed (it is shown translated: "Main profile")
            const unchanged = typed === "" || typed === editName || (editName === "" && typed === tr("Main profile"))
            if (!unchanged) r = Core.cmd({cmd: "profile_rename", id: editId, name: typed})
            if (r.ok !== false && avatarTouched)
                r = Core.cmd(photoNew ? {cmd: "profile_avatar", id: editId, photo: editPhoto} : {cmd: "profile_avatar", id: editId, preset: editPreset})
        }
        if (r.ok === false) Core.toast(tr(r.error))
        else page = "list"
    }
    function removeProfile() {
        const wasActive = editId === st.active
        const r = Core.cmd({cmd: "profile_delete", id: editId})
        if (r.ok === false) { Core.toast(tr(r.error)); return }
        page = "list"
        if (wasActive) switched()
    }
    onAboutToShow: { page = "list"; confirming = false }

    padding: 8
    contentWidth: pp.w

    FileDialog {
        id: photoDlg
        title: pp.tr("Choose a photo")
        nameFilters: [pp.tr("Images") + " (*.png *.jpg *.jpeg *.webp *.gif *.bmp)"]
        onAccepted: {
            // cut and sized by the window (QImage reads every format Qt knows, turned the way the camera held it)
            const r = Core.squareAvatar(selectedFile.toString())
            if (r.ok !== true) { Core.toast(pp.tr("Could not read that image.")); return }
            pp.editPhoto = r.path
            pp.photoNew = true
            pp.avatarTouched = true
        }
    }

    contentItem: Column {
        width: pp.w
        spacing: 6

        // ================= the profiles =================
        Item {
            visible: pp.page === "list"
            width: parent.width
            height: 34
            Text {
                x: 4
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 8
                elide: Text.ElideRight
                text: pp.tr("Profiles"); color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family
            }
        }
        Text {
            visible: pp.page === "list"
            x: 4
            width: parent.width - 8
            wrapMode: Text.Wrap
            text: pp.tr("Each profile has its own library, favorites, history and study.")
            color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
        }
        Repeater {
            model: pp.page === "list" ? pp.people.length : 0
            delegate: Rectangle {
                id: row
                required property int index
                readonly property var p: pp.people[index] || ({})
                width: pp.w
                height: Theme.mobile ? 60 : 52
                radius: Theme.shape.small
                color: row.p.active ? (Theme.c.sidebarSelected || Theme.c.selected) : (rowHover.hovered ? Theme.c.hover : "transparent")
                HoverHandler { id: rowHover }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: pp.choose(row.p)
                }
                ProfileAvatar {
                    id: rowPic
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    size: 36
                    preset: row.p.preset || 0
                    photo: row.p.photo || ""
                }
                Column {
                    anchors { left: rowPic.right; leftMargin: 12; right: editBtn.left; rightMargin: 6; verticalCenter: parent.verticalCenter }
                    spacing: 1
                    Text { width: parent.width; elide: Text.ElideRight; text: pp.shownName(row.p); color: Theme.c.text; font.pixelSize: 15; font.weight: row.p.active ? Font.DemiBold : Font.Normal; font.family: Theme.type.family }
                    Text { visible: row.p.active === true; width: parent.width; elide: Text.ElideRight; text: pp.tr("In use"); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family }
                }
                FlatButton {
                    id: editBtn
                    anchors { right: parent.right; rightMargin: 6; verticalCenter: parent.verticalCenter }
                    kind: "flatround"; glyph: "edit"; iconSize: 16
                    implicitWidth: Theme.mobile ? 44 : 32; implicitHeight: Theme.mobile ? 44 : 32
                    focusPolicy: Qt.NoFocus
                    ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: pp.tr("Edit profile")
                    onClicked: pp.edit(row.p)
                }
            }
        }
        // a new person: the editor opens empty
        Rectangle {
            visible: pp.page === "list" && pp.people.length < (pp.st.max || 12)
            width: pp.w
            height: Theme.mobile ? 56 : 48
            radius: Theme.shape.small
            color: addArea.containsMouse ? Theme.c.hover : "transparent"
            Rectangle {
                id: addRing
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 36; height: 36; radius: 18
                color: "transparent"
                border.width: 1.5
                border.color: Theme.c.textDim
                Icon { anchors.centerIn: parent; name: "plus"; size: 18; color: Theme.c.text }
            }
            Text {
                anchors { left: addRing.right; leftMargin: 12; right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
                elide: Text.ElideRight
                text: pp.tr("Add profile"); color: Theme.c.text; font.pixelSize: 15; font.family: Theme.type.family
            }
            MouseArea { id: addArea; anchors.fill: parent; hoverEnabled: !Theme.touch; cursorShape: Qt.PointingHandCursor; onClicked: pp.edit(null) }
        }

        // ================= one profile =================
        Item {
            visible: pp.page === "edit"
            width: parent.width
            height: 36
            FlatButton {
                id: backBtn
                anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                kind: "flatround"; glyph: "left"; iconSize: 16
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered; ToolTip.delay: 500; ToolTip.text: pp.tr("Back")
                onClicked: pp.page = "list"
            }
            Text {
                anchors { left: backBtn.right; leftMargin: 6; right: parent.right; rightMargin: 4; verticalCenter: parent.verticalCenter }
                elide: Text.ElideRight
                text: pp.editId === "" ? pp.tr("New profile") : pp.tr("Edit profile")
                color: Theme.c.text; font.pixelSize: 15; font.weight: Font.Bold; font.family: Theme.type.family
            }
        }
        Item {
            visible: pp.page === "edit"
            width: parent.width
            height: 88
            ProfileAvatar {
                anchors.centerIn: parent
                size: 76
                preset: pp.editPreset
                photo: pp.editPhoto
            }
        }
        // the name
        Rectangle {
            visible: pp.page === "edit"
            width: parent.width
            height: Theme.mobile ? 48 : 36
            radius: Theme.shape.control
            color: Theme.c.fill3
            border.width: nameField.activeFocus ? 2 : (Theme.layout.buttons === "fluent" ? 1 : 0)
            border.color: nameField.activeFocus ? Theme.c.accent : Theme.c.border
            TextField {
                id: nameField
                anchors { fill: parent; leftMargin: 10; rightMargin: 10 }
                background: null
                placeholderText: pp.editId === "" ? pp.tr("Profile") + " " + (pp.people.length + 1) : pp.tr("Name")
                placeholderTextColor: Theme.c.textDim
                color: Theme.c.text
                font.pixelSize: 15
                font.family: Theme.type.family
                selectByMouse: true
                maximumLength: 40
                verticalAlignment: TextInput.AlignVCenter
                onAccepted: pp.save()
            }
        }
        // the pictures of the app
        Text {
            visible: pp.page === "edit"
            x: 2
            topPadding: 4
            text: pp.tr("Pictures"); color: Theme.c.textDim; font.pixelSize: 12; font.family: Theme.type.family
        }
        Grid {
            id: presetGrid
            visible: pp.page === "edit"
            width: parent.width
            columns: 6
            spacing: 6
            readonly property real cell: (width - spacing * (columns - 1)) / columns
            Repeater {
                model: pp.page === "edit" ? pp.presetCount : 0
                delegate: Item {
                    id: cellItem
                    required property int index
                    readonly property bool chosen: pp.editPhoto === "" && pp.editPreset === index
                    width: presetGrid.cell
                    height: presetGrid.cell
                    ProfileAvatar {
                        anchors.centerIn: parent
                        size: Math.round(presetGrid.cell - 10)
                        preset: cellItem.index
                        ring: cellItem.chosen
                        opacity: cellArea.containsMouse || cellItem.chosen ? 1 : 0.88
                    }
                    MouseArea {
                        id: cellArea
                        anchors.fill: parent
                        hoverEnabled: !Theme.touch
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { pp.editPreset = cellItem.index; pp.editPhoto = ""; pp.photoNew = false; pp.avatarTouched = true }
                    }
                }
            }
        }
        // or a photo of the computer
        FlatButton {
            visible: pp.page === "edit"
            width: parent.width
            kind: "plain"; glyph: "camera"
            text: pp.tr("Choose a photo…")
            onClicked: photoDlg.open()
        }
        Text {
            visible: pp.page === "edit"
            x: 2
            width: parent.width - 4
            wrapMode: Text.Wrap
            text: pp.tr("Any picture: its middle square is used, at 256 × 256 pixels.")
            color: Theme.c.textDim; font.pixelSize: 11; font.family: Theme.type.family
        }
        // removing asks once more: everything of that person goes with it
        Rectangle {
            visible: pp.page === "edit" && pp.confirming
            width: parent.width
            height: confirmCol.implicitHeight + 20
            radius: Theme.shape.small
            color: Theme.c.errorBg !== undefined ? Qt.alpha(Theme.c.errorBg, 0.18) : Theme.c.fill3
            border.width: 1
            border.color: Theme.c.error
            Column {
                id: confirmCol
                x: 10; y: 10
                width: parent.width - 20
                spacing: 8
                Text {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: pp.tr("Delete this profile? Its library, favorites, history and study are deleted too.")
                    color: Theme.c.text; font.pixelSize: 13; font.family: Theme.type.family
                }
                Row {
                    anchors.right: parent.right
                    spacing: 8
                    FlatButton { kind: "plain"; text: pp.tr("Cancel"); onClicked: pp.confirming = false }
                    FlatButton { kind: "destructive"; text: pp.tr("Delete"); onClicked: pp.removeProfile() }
                }
            }
        }
        Item {
            visible: pp.page === "edit"
            width: parent.width
            height: saveBtn.implicitHeight + 4
            FlatButton {
                visible: pp.editId !== "" && !pp.editMain && !pp.confirming
                anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                kind: "dangertext"; glyph: "trash"; iconSize: 16
                text: pp.tr("Delete profile")
                onClicked: pp.confirming = true
            }
            FlatButton {
                id: saveBtn
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                kind: "suggested"
                text: pp.tr("Save")
                onClicked: pp.save()
            }
        }
    }
}
