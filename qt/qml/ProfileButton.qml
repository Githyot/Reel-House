import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The round picture of the profile in use, in the sidebar and the rail: a click opens the profiles (ProfilePopover),
// where another person picks theirs or a new one is made.
Item {
    id: pb
    property int avatarSize: 28
    // the popover opens under it (the top of the sidebar) instead of beside it (the bottom of the rail)
    property bool below: false
    signal switched()
    readonly property var people: Core.profiles.list || []
    readonly property var me: {
        for (let i = 0; i < people.length; ++i) if (people[i].active) return people[i]
        return people.length > 0 ? people[0] : ({})
    }
    readonly property string meName: me.name !== undefined && me.name !== "" ? me.name : Core.tr(Core.uiLang, "Main profile")
    implicitWidth: avatarSize + 8
    implicitHeight: avatarSize + 8
    Accessible.role: Accessible.Button
    Accessible.name: Core.tr(Core.uiLang, "Profiles") + ": " + meName
    activeFocusOnTab: true
    Keys.onSpacePressed: open()
    Keys.onReturnPressed: open()
    function open() { if (below) pop.openBelow(pb, false); else pop.openRight(pb) }

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: area.pressed ? Theme.c.selected : (area.containsMouse || pop.visible ? Theme.c.hover : "transparent")
        border.width: pb.activeFocus ? 2 : 0
        border.color: Theme.c.accent
    }
    ProfileAvatar {
        anchors.centerIn: parent
        size: pb.avatarSize
        preset: pb.me.preset || 0
        photo: pb.me.photo || ""
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: pb.open()
    }
    ToolTip.visible: area.containsMouse && !pop.visible
    ToolTip.delay: 400
    ToolTip.text: Core.tr(Core.uiLang, "Profile in use:") + " " + pb.meName
    ProfilePopover { id: pop; onSwitched: pb.switched() }
}
