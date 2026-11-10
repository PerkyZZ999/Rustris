import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import "ScrollFocus.js" as ScrollFocus
import Rustris
// Settings: display preferences and the way to the prepared runner.
FocusScope {
    id: page
    required property Library backend
    required property var sessions
    signal done()
    signal runnerSettings()
    property alias firstControl: opaqueCheck
    function focusFirst() { (opaqueCheck.enabled ? opaqueCheck : doneButton).forceActiveFocus(); }
    Accessible.role: Accessible.Dialog
    Accessible.name: qsTr("Display settings")
    // Keep keyboard focus visible: scroll whichever control inside this page gains it.
    Connections {
        target: page.Window.window
        function onActiveFocusItemChanged() {
            const item = page.Window.activeFocusItem;
            if (!item || !page.visible) return;
            for (let p = item.parent; p; p = p.parent)
                if (p === settingsScroller) { ScrollFocus.reveal(settingsScroller, item); return; }
        }
    }
    ScrollView {
        id: settingsScroller
        anchors.fill: parent
        clip: true
        contentWidth: availableWidth
        ColumnLayout {
            width: parent.width
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 32; Layout.rightMargin: 32; Layout.topMargin: 28
                ColumnLayout {
                    spacing: 6
                    Heading { level: 1; text: qsTr("Settings") }
                    Label { textFormat: Text.PlainText; font.family: Theme.family; font.pixelSize: 14; color: Theme.secondary; text: qsTr("Preferences are saved on this device.") }
                }
                Item { Layout.fillWidth: true }
                ActionButton { id: doneButton; Layout.alignment: Qt.AlignBottom; text: qsTr("Done"); onClicked: page.done() }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 32; Layout.rightMargin: 32; Layout.topMargin: 24; Layout.bottomMargin: 40
                spacing: 24
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 760
                    Layout.alignment: Qt.AlignTop
                    spacing: 20
                    Panel {
                        Layout.fillWidth: true
                        opaque: page.backend.opaque
                        implicitHeight: display.implicitHeight + 40
                        ColumnLayout {
                            id: display
                            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 24; rightMargin: 24; topMargin: 18 }
                            spacing: 0
                            SectionHeader { iconName: "monitor"; title: qsTr("Display settings") }
                            Divider {}
                            Check {
                                id: opaqueCheck
                                Layout.fillWidth: true
                                Layout.topMargin: 12; Layout.bottomMargin: 12
                                toggle: true
                                text: qsTr("Opaque panels")
                                description: qsTr("Solid surfaces instead of frosted glass. Easier to read over bright artwork.")
                                checked: page.backend.opaque
                                enabled: !page.backend.busy
                                nextCheckState: function() { page.backend.preferences(!page.backend.opaque, page.backend.reduced_motion); return checkState; }
                            }
                            Divider {}
                            Check {
                                Layout.fillWidth: true
                                Layout.topMargin: 12; Layout.bottomMargin: 12
                                toggle: true
                                text: qsTr("Reduce motion")
                                description: qsTr("Stops card lift, artwork drift and sweeping indicators. Progress still updates.")
                                checked: page.backend.reduced_motion
                                enabled: !page.backend.busy
                                nextCheckState: function() { page.backend.preferences(page.backend.opaque, !page.backend.reduced_motion); return checkState; }
                            }
                            Divider {}
                            Caption { Layout.fillWidth: true; Layout.topMargin: 12; text: qsTr("Keyboard focus stays visible in either mode.") }
                        }
                    }
                    Panel {
                        Layout.fillWidth: true
                        opaque: page.backend.opaque
                        implicitHeight: artworkSettings.implicitHeight + 40
                        ColumnLayout {
                            id: artworkSettings
                            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 24; rightMargin: 24; topMargin: 18 }
                            spacing: 0
                            SectionHeader { iconName: "image"; title: qsTr("Artwork") }
                            Divider {}
                            Check {
                                Layout.fillWidth: true
                                Layout.topMargin: 12; Layout.bottomMargin: 12
                                toggle: true
                                text: qsTr("Download artwork automatically")
                                description: qsTr("Finds covers and banners for new games. Game names are sent to SteamGridDB (with your key) and the Steam Store. Your own artwork is never replaced.")
                                checked: page.backend.artwork_enabled
                                enabled: !page.backend.busy
                                nextCheckState: function() { page.backend.artwork_settings(!page.backend.artwork_enabled, "", false); return checkState; }
                            }
                            Divider {}
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 12; Layout.bottomMargin: 12
                                spacing: 6
                                Label { textFormat: Text.PlainText; text: qsTr("SteamGridDB API key"); font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink }
                                Caption {
                                    Layout.fillWidth: true
                                    visible: page.backend.error.length > 0 && keyField.text.length > 0
                                    color: Theme.danger
                                    text: page.backend.error
                                    Accessible.role: Accessible.AlertMessage
                                }
                                Caption {
                                    Layout.fillWidth: true
                                    text: page.backend.artwork_has_key
                                          ? qsTr("A key is saved. Enter a new one to replace it. Without a key, only the Steam Store is used.")
                                          : qsTr("Optional. Gives better covers for non-Steam games. Create one free at steamgriddb.com under Preferences → API.")
                                }
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Field {
                                        id: keyField
                                        property bool clearWhenSaved: false
                                        Connections {
                                            target: page.backend
                                            function onBusyChanged() {
                                                if (page.backend.busy || !keyField.clearWhenSaved) return;
                                                keyField.clearWhenSaved = false;
                                                if (page.backend.error.length === 0) keyField.text = "";
                                            }
                                        }
                                        Layout.fillWidth: true
                                        monospace: true
                                        // Not a password field: Qt's accessibility bridge drops its name. The key
                                        // is cleared once saved and never shown back.
                                        inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                                        placeholderText: page.backend.artwork_has_key ? qsTr("Saved — enter a new key to replace") : qsTr("Paste your API key")
                                        Accessible.name: qsTr("SteamGridDB API key")
                                    }
                                    ActionButton {
                                        small: true
                                        text: qsTr("Save key")
                                        enabled: !page.backend.busy && keyField.text.trim().length > 0
                                        // The key is checked by the worker; clear it only once it is saved.
                                        onClicked: { keyField.clearWhenSaved = true; page.backend.artwork_settings(page.backend.artwork_enabled, keyField.text, false); }
                                    }
                                    ActionButton {
                                        small: true; ghost: true
                                        visible: page.backend.artwork_has_key
                                        text: qsTr("Remove key")
                                        enabled: !page.backend.busy
                                        onClicked: page.backend.artwork_settings(page.backend.artwork_enabled, "", true)
                                    }
                                }
                            }
                            Divider {}
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 14
                                spacing: 16
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Label { textFormat: Text.PlainText; text: qsTr("Games without a cover"); font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink }
                                    Caption { Layout.fillWidth: true; text: page.backend.artwork_status.length ? page.backend.artwork_status : qsTr("Download artwork for every game that has neither your own image nor a downloaded one.") }
                                }
                                ActionButton {
                                    text: qsTr("Download missing artwork")
                                    iconName: "download"
                                    enabled: page.backend.artwork_enabled && !page.backend.artwork_fetching && !page.backend.busy
                                    onClicked: page.backend.fetch_missing_artwork()
                                }
                            }
                        }
                    }
                    Panel {
                        Layout.fillWidth: true
                        opaque: page.backend.opaque
                        implicitHeight: runners.implicitHeight + 40
                        ColumnLayout {
                            id: runners
                            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 24; rightMargin: 24; topMargin: 18 }
                            spacing: 0
                            SectionHeader { iconName: "wine"; title: qsTr("Runners") }
                            Divider {}
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 14
                                spacing: 16
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Label { textFormat: Text.PlainText; text: qsTr("Prepared UMU runner"); font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink }
                                    Caption { Layout.fillWidth: true; text: qsTr("UMU, Proton and runtime paths used by Windows games.") }
                                }
                                ActionButton { text: qsTr("Runner settings"); enabled: !page.sessions.action_busy; onClicked: page.runnerSettings() }
                            }
                            Caption { Layout.fillWidth: true; Layout.topMargin: 10; visible: page.sessions.error.length > 0; color: Theme.danger; text: page.sessions.error }
                        }
                    }
                }
                Panel {
                    Layout.preferredWidth: 300
                    Layout.alignment: Qt.AlignTop
                    opaque: page.backend.opaque
                    implicitHeight: about.implicitHeight + 40
                    ColumnLayout {
                        id: about
                        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
                        spacing: 10
                        SectionLabel { text: qsTr("Local library") }
                        Caption { Layout.fillWidth: true; text: qsTr("Rustris keeps its library, profiles and session logs in your own data folder. No account is required. Only automatic artwork, when you turn it on, contacts SteamGridDB and the Steam Store with game names.") }
                    }
                }
            }
        }
    }
    component SectionHeader: RowLayout {
        property string iconName
        property string title
        Layout.fillWidth: true
        Layout.bottomMargin: 12
        spacing: 12
        Rectangle {
            implicitWidth: 34; implicitHeight: 34; radius: 10
            color: "#1aff681f"
            Icon { anchors.centerIn: parent; name: parent.parent.iconName; size: 18; color: Theme.accentLink }
        }
        Heading { level: 3; text: parent.title }
    }
    component Divider: Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.hairline5 }
}
