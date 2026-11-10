import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
// Sidebar .nav row: icon, label and count; the active row has the orange edge glow.
Button {
    id: nav
    property string iconName: ""
    property string count: ""
    property bool active: false
    property bool compact: false
    property string mono: ""
    property color monoTint: Theme.secondary
    hoverEnabled: true
    implicitHeight: compact ? 44 : 40
    implicitWidth: compact ? 48 : 200
    leftPadding: compact ? 0 : 12
    rightPadding: compact ? 0 : 12
    font.family: Theme.family
    font.pixelSize: 14
    font.weight: Font.Medium
    Accessible.name: text
    ToolTip.visible: compact && hovered
    ToolTip.text: text
    contentItem: Item {
        Row {
            anchors.verticalCenter: parent.verticalCenter
            x: nav.compact ? (parent.width - width) / 2 : 0
            spacing: 12
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                visible: nav.mono.length === 0
                name: nav.iconName
                size: 20
                color: nav.active ? Theme.accentLink : (nav.hovered ? Theme.ink : Theme.secondary)
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                visible: nav.mono.length > 0
                width: 22; height: 22; radius: 6
                color: Theme.fill6
                border.color: Theme.fill6
                Text {
                    anchors.centerIn: parent
                    text: nav.mono
                    font.family: Theme.mono
                    font.pixelSize: 9
                    font.weight: Font.DemiBold
                    color: nav.monoTint
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: !nav.compact
                textFormat: Text.PlainText
                text: nav.text
                font: nav.font
                color: nav.active || nav.hovered ? Theme.ink : Theme.secondary
            }
        }
        Text {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: !nav.compact && nav.count.length > 0
            textFormat: Text.PlainText
            text: nav.count
            font.family: Theme.family
            font.pixelSize: 12
            font.features: { "tnum": 1 }
            color: Theme.muted
        }
    }
    background: Item {
        Rectangle {
            anchors.fill: parent
            radius: Theme.radiusMd
            visible: nav.active
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: "#2eff681f" }
                GradientStop { position: 0.8; color: "#08ff681f" }
            }
            border.color: "#1fff8c3c"
        }
        Rectangle {
            anchors.fill: parent
            radius: Theme.radiusMd
            visible: !nav.active && nav.hovered
            color: "#0bffffff"
        }
        MultiEffect {
            source: edge
            anchors.fill: edge
            visible: nav.active && !nav.compact
            shadowEnabled: true
            shadowColor: Theme.accent
            shadowBlur: 0.6
            shadowOpacity: 0.65
            blurMax: 16
            autoPaddingEnabled: true
        }
        Rectangle {
            id: edge
            visible: nav.active && !nav.compact
            x: -12; y: 9
            width: 3; height: parent.height - 18
            radius: 2
            color: Theme.accent
        }
        Rectangle {
            anchors.fill: parent; anchors.margins: -3; radius: Theme.radiusMd + 3
            color: "transparent"; border.width: 2; border.color: Theme.ember
            visible: nav.activeFocus && nav.focusReason !== Qt.MouseFocusReason
        }
    }
}
