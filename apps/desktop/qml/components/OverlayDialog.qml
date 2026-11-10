import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
// L3 overlay from the mockup: rgba(30,30,38,.95), white 10% border, inset highlight,
// deep shadow and a 20 px radius. The heading shows the title once.
Dialog {
    id: dialog
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: 24
    header: Item {}
    // Qt's popup restores focus to the control that opened it when it closes.
    Overlay.modal: Rectangle { color: "#a6050508" }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.fast } NumberAnimation { property: "scale"; from: 0.98; to: 1; duration: Theme.fast; easing.type: Easing.OutCubic } }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Theme.instant } }
    background: Item {
        MultiEffect {
            source: face
            anchors.fill: face
            shadowEnabled: true
            shadowColor: "black"
            shadowBlur: 1
            shadowVerticalOffset: 24
            shadowOpacity: 0.85
            blurMax: 48
            autoPaddingEnabled: true
        }
        Rectangle {
            id: face
            anchors.fill: parent
            radius: Theme.radius2xl
            color: Theme.overlay
            border.color: Theme.line10
            Rectangle {
                anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: parent.radius; rightMargin: parent.radius; topMargin: 1 }
                height: 1
                color: "#12ffffff"
            }
        }
    }
}
