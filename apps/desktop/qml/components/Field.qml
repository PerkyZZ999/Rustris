import QtQuick
import QtQuick.Controls.Basic
// .fld from the mockup: 40 px, radius 10, ember border and soft glow on focus.
TextField {
    id: field
    property string iconName: ""
    property bool monospace: false
    font.family: monospace ? Theme.mono : Theme.family
    font.pixelSize: monospace ? 13 : 14
    color: Theme.ink
    placeholderTextColor: Theme.muted
    selectionColor: Theme.accent
    selectedTextColor: Theme.accentText
    selectByMouse: true
    implicitHeight: 40
    leftPadding: iconName.length ? 38 : 12
    rightPadding: 12
    verticalAlignment: TextInput.AlignVCenter
    background: Rectangle {
        radius: Theme.radiusMd
        color: Theme.fill4
        border.width: 1
        // Input boundaries keep at least 3:1 against every panel (V-081).
        border.color: field.activeFocus ? "#99ff8c3c" : Theme.control
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: parent.radius + 3
            color: "transparent"
            border.width: 3
            border.color: "#26ff681f"
            visible: field.activeFocus
        }
        Icon {
            visible: field.iconName.length > 0
            name: field.iconName
            size: 16
            strokeWidth: 2
            color: Theme.muted
            anchors.left: parent.left
            anchors.leftMargin: 12
            anchors.verticalCenter: parent.verticalCenter
        }
    }
}
