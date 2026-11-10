import QtQuick
import QtQuick.Controls.Basic
// .chip filter toggle with an optional count and icon.
Button {
    id: chip
    property string count: ""
    property string iconName: ""
    property bool on: false
    checkable: false
    hoverEnabled: true
    font.family: Theme.family
    font.pixelSize: 13
    font.weight: Font.Medium
    implicitHeight: 32
    leftPadding: 12
    rightPadding: 12
    Accessible.checkable: true
    Accessible.checked: on
    contentItem: Row {
        spacing: 7
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: chip.iconName.length > 0
            name: chip.iconName
            size: 14
            filled: chip.iconName === "star"
            color: chip.on ? Theme.ember : Theme.secondary
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: chip.text
            font: chip.font
            color: chip.on ? Theme.accentChipText : (chip.hovered ? Theme.ink : Theme.secondary)
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: chip.count.length > 0
            textFormat: Text.PlainText
            text: chip.count
            font.family: Theme.family
            font.pixelSize: 11
            font.features: { "tnum": 1 }
            color: chip.on ? Theme.ember : Theme.muted
        }
    }
    background: Rectangle {
        radius: Theme.radiusSm
        color: chip.on ? Theme.accentSoft : Theme.fill3
        border.color: chip.on ? Theme.accentBorder : (chip.hovered ? Theme.line16 : Theme.line8)
        Rectangle {
            anchors.fill: parent; anchors.margins: -4; radius: parent.radius + 4
            color: "transparent"; border.width: 2; border.color: Theme.ember
            visible: chip.activeFocus && chip.focusReason !== Qt.MouseFocusReason
        }
    }
}
