import QtQuick
import QtQuick.Controls.Basic
// Checkbox, or the mockup's .tg switch when `toggle` is set. Both stay check boxes
// for assistive technology.
CheckBox {
    id: control
    property bool toggle: false
    property string description: ""
    font.family: Theme.family
    font.pixelSize: 14
    font.weight: toggle ? Font.DemiBold : Font.Normal
    spacing: 12
    padding: 4
    hoverEnabled: true
    indicator: Item {
        implicitWidth: control.toggle ? 42 : 20
        implicitHeight: control.toggle ? 24 : 20
        x: control.toggle && control.text.length ? control.width - width - control.rightPadding : control.leftPadding
        y: (control.height - height) / 2
        opacity: control.enabled ? 1 : 0.45
        Rectangle {
            anchors.fill: parent
            visible: control.toggle
            radius: 12
            color: control.checked ? Theme.accent : "#1fffffff"
            border.color: control.checked ? "#80ffbe78" : Theme.line10
            Rectangle {
                width: 18; height: 18; radius: 9
                y: 2
                x: control.checked ? 21 : 2
                color: control.checked ? "#fff5ec" : Theme.soft
                Behavior on x { NumberAnimation { duration: Theme.base; easing.type: Easing.OutBack } }
            }
        }
        Rectangle {
            anchors.fill: parent
            visible: !control.toggle
            radius: 5
            color: control.checked ? Theme.accent : Theme.fill4
            border.color: control.checked ? Theme.accent : Theme.control
            Icon {
                anchors.centerIn: parent
                visible: control.checked
                name: "check"
                size: 14
                strokeWidth: 3
                color: Theme.accentText
            }
        }
    }
    contentItem: Item {
        readonly property real lead: control.toggle ? 0 : control.indicator.width + control.spacing
        readonly property real trail: control.toggle ? control.indicator.width + control.spacing : 0
        // Natural width from the label, so an unstretched check box keeps its text.
        implicitWidth: lead + trail + Math.max(labelText.implicitWidth, control.description.length ? Math.min(descriptionText.implicitWidth, 360) : 0)
        implicitHeight: labels.implicitHeight
        Column {
            id: labels
            x: parent.lead
            width: parent.width - parent.lead - parent.trail
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Text {
                id: labelText
                width: parent.width
                textFormat: Text.PlainText
                text: control.text
                font: control.font
                color: control.enabled ? Theme.ink : Theme.muted
                wrapMode: Text.Wrap
            }
            Text {
                id: descriptionText
                width: parent.width
                visible: control.description.length > 0
                textFormat: Text.PlainText
                text: control.description
                font.family: Theme.family
                font.pixelSize: 13
                color: Theme.secondary
                wrapMode: Text.Wrap
                lineHeight: 1.15
            }
        }
    }
    background: Rectangle {
        radius: Theme.radiusSm
        color: "transparent"
        border.width: 2
        border.color: Theme.ember
        visible: control.activeFocus && control.focusReason !== Qt.MouseFocusReason
    }
}
