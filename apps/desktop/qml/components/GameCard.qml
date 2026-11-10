import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
// One library entry: the mockup's 2:3 cover card (grid and artwork wall) or a list row.
ItemDelegate {
    id: card
    required property string game_id
    required property string title
    required property string availability
    required property string runner
    required property bool favorite
    required property string artwork_url
    property string mode: "grid"
    property bool selected: false
    property bool reducedMotion: false
    property bool favoriteEnabled: true
    // One rounded mask shared by every card (a hidden layered Rectangle of card size).
    property Item coverMask: null
    signal activated(bool keyboard)
    signal toggleFavorite()
    signal playRequested()
    readonly property bool listRow: mode === "list"
    readonly property bool lifted: !listRow && !reducedMotion && (hovered || visualFocus)
    readonly property string runnerLabel: runner === "umu" ? qsTr("UMU / Proton") : qsTr("Linux executable")
    readonly property bool missing: availability.indexOf("missing") >= 0 || availability.indexOf("Missing") >= 0
    padding: 0
    hoverEnabled: true
    highlighted: selected
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.ListItem
    Accessible.name: title
    Accessible.description: availability
    Accessible.selected: selected
    Accessible.onPressAction: card.clicked()
    onClicked: activated(false)
    Keys.onReturnPressed: activated(true)
    Keys.onEnterPressed: activated(true)
    Keys.onSpacePressed: activated(true)
    background: Item {}
    // Only the active view's content is built: a library of 1,000 cards creates and
    // destroys delegates on every filter change.
    contentItem: Item {
        Loader {
            width: parent.width
            height: parent.height
            active: !card.listRow
            sourceComponent: coverView
        }
        Loader {
            anchors.fill: parent
            active: card.listRow
            sourceComponent: rowView
        }
    }
    Component {
        id: coverView
        Item {
        // Grid / wall cover.
        Item {
            id: cover
            width: parent.width
            height: width * 1.5
            y: card.lifted ? -4 : 0
            Behavior on y { NumberAnimation { duration: Theme.base; easing.type: Easing.OutCubic } }
            // Analytic shadow: no offscreen layer per card.
            RectangularShadow {
                anchors.fill: frame
                visible: card.lifted || card.selected
                radius: Theme.radiusLg
                color: card.selected && !card.lifted ? "#8cff681f" : "#d9000000"
                blur: card.lifted ? 36 : 24
                offset.y: card.lifted ? 18 : 0
                spread: card.lifted ? -6 : 0
            }
            Rectangle {
                id: frame
                anchors.fill: parent
                radius: Theme.radiusLg
                color: Theme.raisedBase
                border.color: Theme.line7
                layer.enabled: card.coverMask !== null
                layer.effect: MultiEffect {
                    maskEnabled: true
                    maskSource: card.coverMask
                }
                CoverArt {
                    anchors.fill: parent
                    key: card.game_id
                    source: card.artwork_url
                    scale: card.lifted ? 1.06 : 1
                    Behavior on scale { NumberAnimation { duration: 600; easing.type: Easing.OutCubic } }
                }
                Rectangle {
                    anchors.fill: parent
                    gradient: Gradient {
                        GradientStop { position: 0.52; color: "transparent" }
                        GradientStop { position: 1; color: "#e008080b" }
                    }
                }
                Text {
                    visible: card.mode === "grid" || card.mode === "wall"
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 10 }
                    textFormat: Text.PlainText
                    text: card.title
                    font.family: Theme.display
                    font.pixelSize: card.mode === "wall" ? 15 : 13
                    font.weight: Font.ExtraBold
                    font.letterSpacing: 0.26
                    font.capitalization: Font.AllUppercase
                    lineHeight: 1.1
                    wrapMode: Text.Wrap
                    maximumLineCount: 3
                    elide: Text.ElideRight
                    color: Theme.ink
                    style: Text.Raised
                    styleColor: "#66000000"
                    Accessible.ignored: true
                }
                Rectangle {
                    visible: card.missing
                    x: 8; y: 8
                    height: 22; radius: 6
                    width: warnRow.implicitWidth + 14
                    color: "#d1281e08"
                    Row {
                        id: warnRow
                        anchors.centerIn: parent
                        spacing: 5
                        Icon { anchors.verticalCenter: parent.verticalCenter; name: "alert"; size: 11; strokeWidth: 2.25; color: Theme.warning }
                        Text { anchors.verticalCenter: parent.verticalCenter; text: qsTr("Files missing"); font.family: Theme.family; font.pixelSize: 11; font.weight: Font.DemiBold; color: Theme.warning }
                    }
                }
                // Pointer shortcut only, created while hovered; keyboard users open the
                // game page, whose Play button holds focus.
                Loader {
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 8 }
                    active: card.hovered
                    sourceComponent: ActionButton {
                        small: true
                        primary: true
                        text: card.runner === "umu" ? qsTr("Review and play") : qsTr("Play")
                        iconName: card.runner === "umu" ? "" : "play"
                        Accessible.name: (card.runner === "umu" ? qsTr("Review and play %1") : qsTr("Play %1")).arg(card.title)
                        enabled: !card.missing
                        NumberAnimation on opacity { from: 0; to: 1; duration: Theme.fast }
                        onClicked: card.playRequested()
                    }
                }
            }
            Rectangle {
                anchors.fill: frame
                radius: Theme.radiusLg
                color: "transparent"
                border.width: card.selected ? 2 : 1
                border.color: card.selected ? Theme.accent : (card.lifted ? "#66ffac52" : "transparent")
            }
            Rectangle {
                anchors.fill: frame; anchors.margins: -4; radius: Theme.radiusLg + 4
                color: "transparent"; border.width: 2; border.color: Theme.ember
                visible: card.activeFocus && card.focusReason !== Qt.MouseFocusReason
            }
            FavoriteBadge {
                x: parent.width - width - 8; y: 8
                visible: card.favorite || card.hovered || activeFocus
            }
        }
        Column {
            visible: card.mode === "grid"
            y: cover.height + 9
            width: parent.width
            spacing: 3
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: card.title
                elide: Text.ElideRight
                font.family: Theme.family; font.pixelSize: 13; font.weight: Font.DemiBold
                color: Theme.ink
                Accessible.ignored: true
            }
            Row {
                width: parent.width
                spacing: 6
                Text { anchors.verticalCenter: parent.verticalCenter; text: card.runner === "umu" ? "UMU" : "LX"; font.family: Theme.mono; font.pixelSize: 9; font.weight: Font.DemiBold; color: card.runner === "umu" ? Theme.info : Theme.success }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 30
                    textFormat: Text.PlainText
                    text: card.runnerLabel
                    elide: Text.ElideRight
                    font.family: Theme.family; font.pixelSize: 12
                    color: Theme.muted
                    Accessible.ignored: true
                }
            }
        }
        }
    }
    Component {
        id: rowView
        // List row.
        Rectangle {
            color: card.selected ? "#14ff681f" : (card.hovered ? "#08ffffff" : "transparent")
            Rectangle { visible: card.selected; width: 3; height: parent.height; color: Theme.accent }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hairline5 }
            Rectangle {
                anchors.fill: parent; anchors.margins: 2; radius: 6
                color: "transparent"; border.width: 2; border.color: Theme.ember
                visible: card.activeFocus && card.focusReason !== Qt.MouseFocusReason
            }
            Row {
                anchors.fill: parent
                anchors.leftMargin: 14; anchors.rightMargin: 14
                spacing: 16
                Item {
                    width: 44; height: parent.height
                    Rectangle {
                        anchors.centerIn: parent
                        width: 32; height: 48; radius: 6
                        clip: true
                        color: Theme.raisedBase
                        CoverArt { anchors.fill: parent; key: card.game_id; source: card.artwork_url; sourceSize: Qt.size(64, 96) }
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 44 - 180 - 220 - 32 - 64
                    textFormat: Text.PlainText; text: card.title; elide: Text.ElideRight
                    font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink
                    Accessible.ignored: true
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 180
                    textFormat: Text.PlainText; text: card.runnerLabel; elide: Text.ElideRight
                    font.family: Theme.family; font.pixelSize: 13; color: Theme.secondary
                    Accessible.ignored: true
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 220
                    textFormat: Text.PlainText; text: card.availability; elide: Text.ElideRight
                    font.family: Theme.family; font.pixelSize: 13; color: card.missing ? Theme.warning : Theme.muted
                    Accessible.ignored: true
                }
                Item {
                    width: 32; height: parent.height
                    FavoriteBadge { anchors.centerIn: parent }
                }
            }
        }
    }
    component FavoriteBadge: CheckBox {
        id: badge
        checked: card.favorite
        enabled: card.favoriteEnabled
        hoverEnabled: true
        padding: 0
        implicitWidth: 24; implicitHeight: 24
        Accessible.name: qsTr("Favorite %1").arg(card.title)
        nextCheckState: function() { card.toggleFavorite(); return checkState; }
        indicator: Rectangle {
            width: 24; height: 24; radius: 7
            color: "#990b0b0f"
            border.color: badge.activeFocus && badge.focusReason !== Qt.MouseFocusReason ? Theme.ember : "transparent"
            border.width: 2
            Icon {
                anchors.centerIn: parent
                name: "star"; size: 12
                filled: card.favorite
                strokeWidth: 2
                color: card.favorite ? Theme.ember : (badge.hovered ? Theme.ink : Theme.secondary)
            }
        }
        contentItem: Item {}
    }
}
