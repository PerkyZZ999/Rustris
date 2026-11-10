import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Effects
import Rustris
// Library: title, filters, view switch and the cover grid / artwork wall / list.
Item {
    id: page
    required property Library backend
    property string view: "all"          // all | favorites
    property string layoutMode: "grid"   // grid | wall | list
    property alias search: filterField
    property alias gameList: grid
    signal openGame(string id, bool keyboard)
    signal playGame(string id)
    signal searchEdited(string text)
    readonly property bool listMode: layoutMode === "list"
    function count(value) { return value.toLocaleString(Qt.locale(), "f", 0); }
    function choose(value) { view = value; backend.filter(filterField.text, view === "favorites"); }
    function focusSelection() {
        if (grid.currentItem) grid.currentItem.forceActiveFocus();
        else filterField.forceActiveFocus();
    }
    function revealSelection() {
        if (backend.ready && backend.selected_row >= 0 && backend.selected_row < grid.count)
            grid.positionViewAtIndex(backend.selected_row, GridView.Contain);
    }
    function moveSelection(step) {
        backend.step(step);
        Qt.callLater(function() { page.revealSelection(); page.focusSelection(); });
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        anchors.topMargin: 28
        spacing: 0
        RowLayout {
            Layout.fillWidth: true
            spacing: 16
            ColumnLayout {
                spacing: 6
                Heading { level: 1; text: page.view === "favorites" ? qsTr("Favorites") : qsTr("Library") }
                Label {
                    textFormat: Text.PlainText
                    font.family: Theme.family; font.pixelSize: 14; color: Theme.secondary
                    text: (page.backend.total === 1 ? qsTr("1 game") : qsTr("%1 games").arg(page.count(page.backend.total)))
                          + " · " + (page.backend.favorite_total === 1 ? qsTr("1 favorite") : qsTr("%1 favorites").arg(page.count(page.backend.favorite_total)))
                          + " · " + qsTr("saved locally")
                }
            }
            Item { Layout.fillWidth: true }
            ActionButton {
                Layout.alignment: Qt.AlignBottom
                text: qsTr("Add game"); iconName: "plus"; primary: true
                enabled: page.backend.ready && !page.backend.busy && !page.backend.closing
                onClicked: page.backend.create()
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 22
            spacing: 10
            Field {
                id: filterField
                Layout.fillWidth: true
                Layout.maximumWidth: 380
                iconName: "search"
                placeholderText: qsTr("Filter by title or runner")
                Accessible.name: qsTr("Filter library")
                onTextEdited: page.searchEdited(text)
            }
            Item { Layout.fillWidth: true }
            Rectangle {
                implicitWidth: viewRow.implicitWidth + 6
                implicitHeight: 38
                radius: Theme.radiusMd
                color: Theme.fill4
                border.color: Theme.line7
                Accessible.role: Accessible.Grouping
                Accessible.name: qsTr("View")
                Row {
                    id: viewRow
                    anchors.centerIn: parent
                    Repeater {
                        model: [{ key: "grid", icon: "grid", label: qsTr("Grid") }, { key: "wall", icon: "wall", label: qsTr("Artwork wall") }, { key: "list", icon: "list", label: qsTr("List") }]
                        delegate: Button {
                            id: segment
                            required property var modelData
                            readonly property bool on: page.layoutMode === modelData.key
                            implicitWidth: 36; implicitHeight: 32
                            hoverEnabled: true
                            Accessible.name: qsTr("%1 view").arg(modelData.label)
                            Accessible.checkable: true
                            Accessible.checked: on
                            ToolTip.visible: hovered
                            ToolTip.text: modelData.label
                            onClicked: { page.layoutMode = modelData.key; Qt.callLater(page.revealSelection); }
                            contentItem: Item { Icon { anchors.centerIn: parent; name: segment.modelData.icon; size: 16; color: segment.on || segment.hovered ? Theme.ink : Theme.muted } }
                            background: Rectangle {
                                radius: 7
                                color: segment.on ? Theme.fill10 : "transparent"
                                Rectangle { anchors.fill: parent; anchors.margins: -2; radius: 9; color: "transparent"; border.width: 2; border.color: Theme.ember; visible: segment.activeFocus && segment.focusReason !== Qt.MouseFocusReason }
                            }
                        }
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 14
            spacing: 8
            Accessible.role: Accessible.ToolBar
            Accessible.name: qsTr("Filters")
            Chip { text: qsTr("All"); count: page.count(page.backend.total); on: page.view === "all"; onClicked: page.choose("all") }
            Chip { text: qsTr("Favorites"); iconName: "star"; count: page.count(page.backend.favorite_total); on: page.view === "favorites"; onClicked: page.choose("favorites") }
            Item { Layout.fillWidth: true }
            Label {
                textFormat: Text.PlainText
                font.family: Theme.family; font.pixelSize: 12; color: Theme.muted
                text: qsTr("Showing %1 of %2").arg(page.count(page.backend.shown)).arg(page.count(page.backend.total))
            }
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.topMargin: 22
            visible: page.listMode
            implicitHeight: 34
            color: "transparent"
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line7 }
            RowLayout {
                anchors.fill: parent; anchors.leftMargin: 14; anchors.rightMargin: 14
                spacing: 16
                SectionLabel { Layout.preferredWidth: 44; text: "" }
                SectionLabel { Layout.fillWidth: true; text: qsTr("Title") }
                SectionLabel { Layout.preferredWidth: 180; text: qsTr("Runner") }
                SectionLabel { Layout.preferredWidth: 220; text: qsTr("Status") }
                SectionLabel { Layout.preferredWidth: 32; text: "" }
            }
        }
        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: page.listMode ? 0 : 22
            clip: true
            model: page.backend
            reuseItems: false  // pooled delegates lost their list-item accessibility (V-082)
            currentIndex: page.backend.selected_row
            keyNavigationEnabled: false
            boundsBehavior: Flickable.StopAtBounds
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Game library")
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            readonly property real minCard: page.layoutMode === "wall" ? 196 : 136
            readonly property real gapX: page.layoutMode === "wall" ? 16 : 20
            // Columns × cell width must fit the view; each cell carries its trailing gap.
            readonly property real usable: width - 12
            readonly property int columns: page.listMode ? 1 : Math.max(1, Math.floor((usable + gapX) / (minCard + gapX)))
            readonly property real cardWidth: page.listMode ? usable : (usable + gapX) / columns - gapX
            cellWidth: page.listMode ? usable : Math.floor(usable / columns * 100) / 100
            cellHeight: page.listMode ? 66 : cardWidth * 1.5 + (page.layoutMode === "wall" ? 16 : 24 + 48)
            topMargin: 6
            bottomMargin: 40
            delegate: GameCard {
                id: card
                required property int index
                width: grid.cellWidth - (page.listMode ? 0 : grid.gapX)
                height: grid.cellHeight - (page.listMode ? 0 : (page.layoutMode === "wall" ? 16 : 24))
                mode: page.layoutMode
                selected: page.backend.selected_id === game_id
                reducedMotion: page.backend.reduced_motion
                favoriteEnabled: page.backend.ready && !page.backend.busy
                coverMask: sharedMask
                activeFocusOnTab: GridView.isCurrentItem || (page.backend.selected_row < 0 && index === 0)
                onActivated: keyboard => { page.backend.select(game_id); page.openGame(game_id, keyboard); }
                onToggleFavorite: page.backend.favorite(game_id, !favorite)
                onPlayRequested: { page.backend.select(game_id); page.playGame(game_id); }
                Keys.onRightPressed: page.moveSelection(1)
                Keys.onLeftPressed: page.moveSelection(-1)
                Keys.onDownPressed: page.moveSelection(grid.columns)
                Keys.onUpPressed: page.moveSelection(-grid.columns)
            }
            // Rounded-corner mask shared by every cover (all covers share one size).
            Rectangle {
                id: sharedMask
                width: Math.max(1, grid.cellWidth - grid.gapX)
                height: width * 1.5
                radius: Theme.radiusLg
                visible: false
                layer.enabled: true
            }
            Column {
                anchors.centerIn: parent
                width: Math.min(420, parent.width - 48)
                visible: page.backend.shown === 0
                spacing: 12
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 64; height: 64; radius: 16
                    color: Theme.fill5; border.color: Theme.line8
                    Icon { anchors.centerIn: parent; name: page.backend.total === 0 ? "library" : "filter"; size: 28; color: Theme.secondary }
                }
                Heading {
                    width: parent.width; level: 2; horizontalAlignment: Text.AlignHCenter
                    text: !page.backend.ready ? (page.backend.busy ? qsTr("Opening your library…") : qsTr("Library unavailable"))
                          : (page.backend.total === 0 ? qsTr("Make room for your games") : qsTr("No games match these filters"))
                }
                Caption {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter
                    text: !page.backend.ready ? (page.backend.busy ? qsTr("Reading your saved library…") : qsTr("Check the message above and reload after fixing the problem. Your existing database has been preserved."))
                          : (page.backend.total === 0 ? qsTr("Add a game you already have installed.\nGive it a name and save its launch profile.") : qsTr("Try another search or switch back to All."))
                }
                ActionButton {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: page.backend.ready && page.backend.total === 0
                    text: qsTr("Add your first game"); iconName: "plus"; primary: true
                    enabled: page.backend.ready && !page.backend.busy
                    onClicked: page.backend.create()
                }
                ActionButton {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: page.backend.ready && page.backend.total > 0
                    text: qsTr("Clear filters"); primary: true
                    onClicked: { filterField.text = ""; page.choose("all"); }
                }
            }
        }
    }
}
