import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
// .btn from the mockup: secondary by default; `primary`, `ghost`, `danger`, `small`.
Button {
    id: button
    property bool primary: false
    property bool ghost: false
    property bool danger: false
    property bool small: false
    property string iconName: ""
    property real iconSize: small ? 14 : 16
    property real radius: small ? Theme.radiusSm : Theme.radiusMd
    font.family: Theme.family
    font.pixelSize: small ? 13 : 14
    font.weight: Font.DemiBold
    implicitHeight: small ? 32 : 40
    leftPadding: text.length ? (small ? 12 : 16) : 0
    rightPadding: leftPadding
    topPadding: 0
    bottomPadding: 0
    implicitWidth: Math.max(implicitHeight, implicitContentWidth + leftPadding + rightPadding)
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    readonly property color labelColor: !enabled ? Theme.muted
        : primary ? Theme.accentText
        : danger ? Theme.danger
        : (ghost && !hovered) ? Theme.secondary : Theme.ink
    contentItem: Row {
        spacing: 8
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: button.iconName.length > 0
            name: button.iconName
            size: button.iconSize
            color: button.labelColor
            filled: button.iconName === "play"
            strokeWidth: button.iconName === "plus" ? 2.25 : 1.75
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: button.text.length > 0
            textFormat: Text.PlainText
            text: button.text
            font: button.font
            color: button.labelColor
            elide: Text.ElideRight
        }
    }
    background: Item {
        implicitHeight: button.implicitHeight
        opacity: button.enabled ? 1 : 0.45
        MultiEffect {
            source: face
            anchors.fill: face
            visible: button.primary && button.enabled
            shadowEnabled: true
            shadowColor: Theme.accent
            shadowBlur: 0.8
            shadowVerticalOffset: button.down ? 2 : 8
            shadowOpacity: button.down ? 0.45 : (button.hovered ? 0.95 : 0.75)
            blurMax: 32
            autoPaddingEnabled: true
        }
        Rectangle {
            id: face
            anchors.fill: parent
            radius: button.radius
            border.width: 1
            border.color: button.primary ? "#8cffbe78"
                : button.danger ? "#4de5484d"
                : button.ghost ? (button.hovered ? Theme.line16 : "transparent")
                : (button.hovered ? Theme.line16 : Theme.line9)
            gradient: Gradient {
                GradientStop { position: 0; color: button.primary ? (button.hovered ? Theme.accentHoverTop : Theme.accentTop)
                    : button.danger ? (button.hovered ? "#24e5484d" : "#0fe5484d")
                    : button.ghost ? (button.hovered ? Theme.fill9 : "transparent")
                    : (button.hovered ? Theme.fill9 : Theme.fill5) }
                GradientStop { position: 1; color: button.primary ? (button.hovered ? Theme.accentHoverBottom : Theme.accentStrong)
                    : button.danger ? (button.hovered ? "#24e5484d" : "#0fe5484d")
                    : button.ghost ? (button.hovered ? Theme.fill9 : "transparent")
                    : (button.hovered ? Theme.fill9 : Theme.fill5) }
            }
            // Inset top highlight on the primary face. It stays inside the rounded
            // corners and fades at its ends, like the mockup's inset box-shadow.
            Rectangle {
                visible: button.primary
                anchors { left: parent.left; right: parent.right; top: parent.top
                          leftMargin: button.radius; rightMargin: button.radius; topMargin: 1 }
                height: 1
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: "#00ffe1be" }
                    GradientStop { position: 0.15; color: "#59ffe1be" }
                    GradientStop { position: 0.85; color: "#59ffe1be" }
                    GradientStop { position: 1; color: "#00ffe1be" }
                }
            }
            transform: Translate { y: button.down ? 1 : 0 }
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            radius: button.radius + 4
            color: "transparent"
            border.width: 2
            border.color: Theme.ember
            // The Ember ring marks keyboard and programmatic focus, not a mouse click.
            visible: button.activeFocus && button.focusReason !== Qt.MouseFocusReason
        }
    }
}
