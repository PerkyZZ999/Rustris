import QtQuick
import QtQuick.Controls.Basic
// .ib (36 px square) or .wc window control (28 px round) from the mockup header.
Button {
    id: control
    property string iconName: ""
    property real iconSize: 18
    property bool round: false
    property bool closeStyle: false
    property bool dot: false
    hoverEnabled: true
    implicitWidth: round ? 28 : 36
    implicitHeight: round ? 28 : 36
    padding: 0
    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            name: control.iconName
            size: control.iconSize
            strokeWidth: control.round ? 2 : 1.75
            color: control.closeStyle && control.hovered ? "white" : (control.hovered ? Theme.ink : Theme.secondary)
        }
        Rectangle {
            visible: control.dot
            x: parent.width - 15; y: 7
            width: 7; height: 7; radius: 4
            color: Theme.accent
            border.width: 2; border.color: Theme.background
        }
    }
    background: Rectangle {
        radius: control.round ? width / 2 : Theme.radiusMd
        color: control.hovered ? (control.closeStyle ? Theme.closeHover : (control.round ? Theme.fill10 : Theme.fill6)) : "transparent"
        Rectangle {
            anchors.fill: parent; anchors.margins: -3; radius: parent.radius + 3
            color: "transparent"; border.width: 2; border.color: Theme.ember
            visible: control.activeFocus && control.focusReason !== Qt.MouseFocusReason
        }
    }
}
