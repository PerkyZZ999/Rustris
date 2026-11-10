import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Effects
import Rustris
import "ScrollFocus.js" as ScrollFocus
import "States.js" as States
// Game detail: hero, actions, facts strip and Overview / Configuration / Files / Logs.
FocusScope {
    id: page
    required property Library backend
    required property var sessions
    property var detail: null
    property var sessionSnapshot: ({})
    property bool profileReserved: false
    property int tab: 0
    property alias logs: logPanel
    signal back()
    signal play()
    signal configure()
    signal removeRequested()
    signal chooseArtwork()
    signal enterArtworkPath()
    signal runnerSettings()
    readonly property bool umu: detail !== null && detail.runner === "umu"
    readonly property bool reviewApproved: umu && detail.review_approved === true
    readonly property bool missing: detail !== null && (detail.availability.indexOf("missing") >= 0 || detail.availability.indexOf("Missing") >= 0)
    readonly property var gameSessions: (sessionSnapshot.sessions || []).filter(function(s) { return detail && s.game === detail.id; })
    readonly property var lastSession: gameSessions.length ? gameSessions[0] : null
    readonly property var activeSession: gameSessions.find(function(s) { return s.reserved; }) || null
    function when(ms) {
        const date = new Date(ms);
        const today = new Date();
        const yesterday = new Date(today.getTime() - 86400000);
        const time = date.toLocaleTimeString(Qt.locale(), Locale.ShortFormat);
        if (date.toDateString() === today.toDateString()) return qsTr("Today, %1").arg(time);
        if (date.toDateString() === yesterday.toDateString()) return qsTr("Yesterday, %1").arg(time);
        return date.toLocaleString(Qt.locale(), Locale.ShortFormat);
    }
    function showLogs(focusSession) { tab = 3; if (focusSession) Qt.callLater(function() { logPanel.picker.forceActiveFocus(); }); }
    // Focus the main action: Play when it is available, otherwise Configure.
    property bool focusPending: false
    property int focusReason: Qt.OtherFocusReason
    // A mouse-opened page focuses Play without the keyboard focus ring.
    function focusPrimary(reason) {
        if (reason !== undefined) focusReason = reason;
        if (backend.busy || sessions.busy) { focusPending = true; return; }
        focusPending = false;
        (playButton.enabled ? playButton : (configureButton.enabled ? configureButton : backButton)).forceActiveFocus(focusReason);
        focusReason = Qt.OtherFocusReason;
    }
    Connections {
        target: page.backend
        function onBusyChanged() { if (!page.backend.busy && page.focusPending && page.visible) Qt.callLater(page.focusPrimary); }
    }
    Connections {
        target: page.sessions
        function onBusyChanged() { if (!page.sessions.busy && page.focusPending && page.visible) Qt.callLater(page.focusPrimary); }
    }
    component DetailAction: ActionButton {
        id: action
        onActiveFocusChanged: if (activeFocus) ScrollFocus.reveal(scroller, action)
    }

    ScrollView {
        id: scroller
        anchors.fill: parent
        clip: true
        contentWidth: availableWidth
        ScrollBar.vertical.policy: ScrollBar.AsNeeded
        Column {
            width: scroller.availableWidth
            spacing: 0
            // Hero.
            Item {
                id: hero
                width: parent.width
                height: Math.max(460, heroContent.implicitHeight + 24 + 92)
                clip: true
                CoverArt {
                    id: heroArt
                    anchors.fill: parent
                    anchors.margins: -parent.width * 0.02
                    key: page.detail ? page.detail.id : ""
                    // D-011: a downloaded wide banner when there is one, otherwise the cover.
                    source: page.detail && page.detail.hero_url ? page.detail.hero_url : ""
                    sourceSize: Qt.size(256, 256)
                    layer.enabled: true
                    layer.effect: MultiEffect { blurEnabled: true; blur: heroArt.loaded ? 0.6 : 0.15; blurMax: 48 }
                }
                Rectangle {
                    anchors.fill: parent
                    gradient: Gradient {
                        GradientStop { position: 0; color: "#590b0b0f" }
                        GradientStop { position: 0.35; color: "#0d0b0b0f" }
                        GradientStop { position: 0.7; color: "#b30b0b0f" }
                        GradientStop { position: 1; color: Theme.background }
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: "#bf0b0b0f" }
                        GradientStop { position: 0.6; color: "transparent" }
                    }
                }
                ColumnLayout {
                    id: heroContent
                    anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 32; rightMargin: 32; topMargin: 24 }
                    height: hero.height - 24 - 92
                    spacing: 24
                    RowLayout {
                        spacing: 8
                        ActionButton {
                            id: backButton
                            small: true
                            text: qsTr("Library"); iconName: "chevronLeft"
                            Accessible.name: qsTr("Back to library")
                            onClicked: page.back()
                        }
                        Label { textFormat: Text.PlainText; text: "/ " + (page.detail ? page.detail.name : ""); elide: Text.ElideRight; Layout.maximumWidth: 480; font.family: Theme.family; font.pixelSize: 13; color: Theme.secondary }
                    }
                    Item { Layout.fillHeight: true }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 24
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.maximumWidth: 720
                            spacing: 14
                            Flow {
                                Layout.fillWidth: true
                                spacing: 8
                                Pill {
                                    text: page.detail ? page.detail.availability : ""
                                    tone: page.missing ? "warning" : "success"
                                }
                                Pill { text: page.umu ? qsTr("UMU / Proton") : qsTr("Linux executable") }
                                Pill { text: qsTr("Local library") }
                                Pill { visible: page.detail && page.detail.favorite; text: qsTr("Favorite") }
                            }
                            Heading {
                                Layout.fillWidth: true
                                level: 0
                                // 76 px at the full 720 px column; narrower windows scale the
                                // hero down so long words wrap whole instead of mid-word.
                                size: Math.round(Math.max(40, Math.min(76, width / 9.5)))
                                lineHeight: 0.92
                                maximumLineCount: 3
                                elide: Text.ElideRight
                                text: page.detail ? page.detail.name : qsTr("Select a game")
                            }
                            Label {
                                Layout.fillWidth: true
                                Layout.maximumWidth: 520
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                                font.family: Theme.family; font.pixelSize: 15; lineHeight: 1.2
                                color: Theme.soft
                                text: page.profileReserved ? qsTr("This profile has an active or unknown session. Reconcile it in Logs before changing it or launching again.")
                                      : (page.umu ? (page.reviewApproved ? qsTr("Launch details were reviewed. Play starts directly while the runner, prefix and launch details stay the same; any change shows the review again.")
                                                                         : qsTr("Review the prepared runner and launch details before starting. Rustris remembers your approval until they change."))
                                                  : qsTr("Play checks the saved executable and working directory, then starts the native profile. A process start does not establish playability."))
                            }
                        }
                        RowLayout {
                            Layout.alignment: Qt.AlignBottom
                            spacing: 10
                            DetailAction {
                                id: playButton
                                primary: true
                                implicitHeight: 56
                                radius: Theme.radiusLg
                                Layout.minimumWidth: 150
                                font.pixelSize: 16
                                iconName: page.umu && !page.reviewApproved ? "" : "play"
                                text: page.umu && !page.reviewApproved ? qsTr("Review and play") : qsTr("Play")
                                enabled: page.backend.ready && !page.backend.busy && !page.sessions.busy && !page.profileReserved && page.detail !== null
                                onClicked: page.play()
                            }
                            DetailAction {
                                id: configureButton
                                implicitHeight: 56
                                radius: Theme.radiusLg
                                iconName: "sliders"
                                text: qsTr("Configure")
                                enabled: page.backend.ready && !page.backend.busy && !page.profileReserved && !page.sessions.launching && !page.sessions.action_busy
                                onClicked: page.configure()
                            }
                            DetailAction {
                                implicitHeight: 56
                                implicitWidth: 56
                                radius: Theme.radiusLg
                                iconName: "heart"
                                text: ""
                                Accessible.name: page.detail && page.detail.favorite ? qsTr("Remove favorite") : qsTr("Add favorite")
                                ToolTip.visible: hovered
                                ToolTip.text: Accessible.name
                                enabled: page.backend.ready && !page.backend.busy
                                onClicked: page.backend.favorite(page.backend.selected_id, !page.detail.favorite)
                                contentItem: Item {
                                    Icon { anchors.centerIn: parent; name: "heart"; size: 20; filled: page.detail && page.detail.favorite; color: Theme.accentLink }
                                }
                            }
                        }
                    }
                }
            }
            // Facts strip overlapping the hero, then the tab bar.
            Item {
                width: parent.width
                height: facts.height - 64 + 22 + tabBar.height
                Panel {
                    id: facts
                    glass: true
                    opaque: page.backend.opaque
                    x: 32; y: -64
                    width: parent.width - 64
                    height: factsRow.implicitHeight
                    RowLayout {
                        id: factsRow
                        anchors.fill: parent
                        spacing: 0
                        Fact { label: qsTr("Last played"); value: page.lastSession ? page.when(page.lastSession.created_ms) : qsTr("Never"); sub: page.lastSession ? States.label(page.lastSession.state) : qsTr("No sessions yet") }
                        Fact { label: qsTr("Runner"); value: page.umu ? qsTr("UMU / Proton") : qsTr("Linux executable"); sub: page.umu ? qsTr("Prepared runner from Runners") : qsTr("Started directly") }
                        Fact { label: qsTr("Source"); value: qsTr("Local"); sub: qsTr("Added manually · stays on this device") }
                        Fact { label: qsTr("Status"); value: page.activeSession ? States.label(page.activeSession.state) : (page.missing ? qsTr("Needs attention") : qsTr("Ready to play")); dot: page.activeSession ? Theme.ember : (page.missing ? Theme.warning : Theme.success); sub: page.detail ? page.detail.availability : ""; last: true }
                    }
                }
                Item {
                    id: tabBar
                    x: 32; y: facts.height - 64 + 22
                    width: parent.width - 64
                    height: 48
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line7 }
                    Row {
                        id: tabRow
                        Accessible.role: Accessible.PageTabList
                        Accessible.name: qsTr("Game sections")
                        Repeater {
                            model: [{ icon: "info", label: qsTr("Overview") }, { icon: "sliders", label: qsTr("Configuration") }, { icon: "folder", label: qsTr("Files") }, { icon: "terminal", label: qsTr("Logs") }]
                            delegate: Button {
                                id: tabButton
                                required property int index
                                required property var modelData
                                readonly property bool on: page.tab === index
                                width: 132; height: 48
                                hoverEnabled: true
                                Accessible.role: Accessible.PageTab
                                Accessible.name: modelData.label
                                Accessible.selected: on
                                Accessible.onPressAction: tabButton.clicked()
                                onClicked: page.tab = index
                                onActiveFocusChanged: if (activeFocus) ScrollFocus.reveal(scroller, tabButton)
                                contentItem: Item {
                                    Row {
                                        anchors.centerIn: parent
                                        spacing: 8
                                        Icon { anchors.verticalCenter: parent.verticalCenter; name: tabButton.modelData.icon; size: 16; color: tabButton.on || tabButton.hovered ? Theme.ink : Theme.muted }
                                        Text { anchors.verticalCenter: parent.verticalCenter; text: tabButton.modelData.label; font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: tabButton.on || tabButton.hovered ? Theme.ink : Theme.muted }
                                        Rectangle {
                                            visible: tabButton.index === 3 && page.activeSession !== null
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: 18; height: 18; radius: 9
                                            color: "#29f5c451"; border.color: "#59f5c451"
                                            Text { anchors.centerIn: parent; text: "1"; font.family: Theme.family; font.pixelSize: 10; font.weight: Font.Bold; color: Theme.warning }
                                        }
                                    }
                                }
                                background: Rectangle {
                                    color: "transparent"
                                    Rectangle { anchors.fill: parent; anchors.margins: 4; radius: 8; color: "transparent"; border.width: 2; border.color: Theme.ember; visible: tabButton.activeFocus && tabButton.focusReason !== Qt.MouseFocusReason }
                                }
                            }
                        }
                    }
                    MultiEffect {
                        source: indicator
                        anchors.fill: indicator
                        shadowEnabled: true; shadowColor: Theme.accent; shadowBlur: 0.6; shadowOpacity: 0.8; blurMax: 16
                        autoPaddingEnabled: true
                    }
                    Rectangle {
                        id: indicator
                        y: parent.height - 1
                        width: 84; height: 2; radius: 1
                        x: page.tab * 132 + 24
                        color: Theme.accent
                        Behavior on x { NumberAnimation { duration: page.backend.reduced_motion ? 0 : Theme.slow; easing.type: Easing.OutCubic } }
                    }
                }
            }
            // Tab content.
            Item {
                width: parent.width
                height: content.implicitHeight + 24 + 48
                StackLayout {
                    id: content
                    x: 32; y: 24
                    width: parent.width - 64
                    currentIndex: page.tab
                    height: children[currentIndex] ? children[currentIndex].implicitHeight : 0
                    // Overview.
                    RowLayout {
                        spacing: 24
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignTop
                            spacing: 24
                            Panel {
                                Layout.fillWidth: true
                                implicitHeight: recent.implicitHeight + 44
                                opaque: page.backend.opaque
                                ColumnLayout {
                                    id: recent
                                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 22 }
                                    spacing: 0
                                    Heading { level: 3; text: qsTr("Recent sessions") }
                                    Item { implicitHeight: 12 }
                                    Repeater {
                                        model: page.gameSessions.slice(0, 6)
                                        delegate: Item {
                                            required property var modelData
                                            Layout.fillWidth: true
                                            implicitHeight: 40
                                            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hairline5 }
                                            RowLayout {
                                                anchors.fill: parent
                                                spacing: 16
                                                Label { Layout.preferredWidth: 160; textFormat: Text.PlainText; text: page.when(modelData.created_ms); elide: Text.ElideRight; font.family: Theme.family; font.pixelSize: 13; color: Theme.soft }
                                                Rectangle {
                                                    Layout.fillWidth: true; implicitHeight: 6; radius: 3; color: Theme.fill6
                                                    Rectangle {
                                                        height: parent.height; radius: 3
                                                        width: modelData.reserved ? parent.width * 0.7 : parent.width * 0.35
                                                        color: modelData.reserved ? Theme.accent : "#33ffffff"
                                                    }
                                                }
                                                Label { Layout.preferredWidth: 220; horizontalAlignment: Text.AlignRight; textFormat: Text.PlainText; text: States.label(modelData.state); elide: Text.ElideRight; font.family: Theme.mono; font.pixelSize: 12; color: modelData.reserved ? Theme.ink : Theme.muted }
                                            }
                                        }
                                    }
                                    Caption {
                                        Layout.fillWidth: true
                                        Layout.topMargin: 12
                                        font.pixelSize: 11
                                        color: Theme.muted
                                        text: page.gameSessions.length ? qsTr("Session states come from the helper’s process observation; they do not establish game health.")
                                                                       : qsTr("No sessions yet. Press Play to start one; its output appears in Logs.")
                                    }
                                }
                            }
                            Panel {
                                Layout.fillWidth: true
                                implicitHeight: about.implicitHeight + 44
                                opaque: page.backend.opaque
                                ColumnLayout {
                                    id: about
                                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 22 }
                                    spacing: 10
                                    Heading { level: 3; text: qsTr("Launch profile") }
                                    Caption {
                                        Layout.fillWidth: true
                                        font.pixelSize: 14
                                        text: page.detail ? (page.detail.argument_count === 1 ? qsTr("1 argument") : qsTr("%1 arguments").arg(page.detail.argument_count)) + " · " + (page.detail.environment_count === 1 ? qsTr("1 environment override") : qsTr("%1 environment overrides").arg(page.detail.environment_count)) : ""
                                    }
                                    Label { Layout.fillWidth: true; textFormat: Text.PlainText; text: page.detail ? page.detail.executable : ""; wrapMode: Text.WrapAnywhere; font.family: Theme.mono; font.pixelSize: 12; color: Theme.ink }
                                    Flow {
                                        Layout.fillWidth: true
                                        spacing: 8
                                        Pill { text: page.umu ? qsTr("Runner: UMU / Proton") : qsTr("Runner: native") }
                                        Pill { text: page.detail && page.detail.cwd ? qsTr("Working directory set") : qsTr("Working directory: executable’s folder") }
                                    }
                                }
                            }
                        }
                        ColumnLayout {
                            Layout.preferredWidth: 320
                            Layout.maximumWidth: 320
                            Layout.alignment: Qt.AlignTop
                            spacing: 24
                            Panel {
                                Layout.fillWidth: true
                                implicitHeight: art.implicitHeight + 40
                                opaque: page.backend.opaque
                                ColumnLayout {
                                    id: art
                                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
                                    spacing: 12
                                    SectionLabel { text: qsTr("Artwork") }
                                    RowLayout {
                                        spacing: 14
                                        Rectangle {
                                            implicitWidth: 72; implicitHeight: 108
                                            radius: 8
                                            color: Theme.raisedBase
                                            border.color: Theme.line7
                                            layer.enabled: true
                                            CoverArt {
                                                id: artwork
                                                anchors.fill: parent
                                                anchors.margins: 1
                                                imageName: "selectedArtwork"
                                                key: page.detail ? page.detail.id : ""
                                                source: page.detail && page.detail.artwork_url ? page.detail.artwork_url : ""
                                                sourceSize: Qt.size(144, 180)
                                            }
                                        }
                                        Caption {
                                            Layout.fillWidth: true
                                            text: page.detail && page.detail.artwork_label
                                                  ? (artwork.imageStatus === Image.Ready ? qsTr("Local artwork selected.") : qsTr("Artwork unavailable or unreadable. The original selection is preserved."))
                                                  : page.detail && page.detail.artwork_fetched ? qsTr("Downloaded artwork. Choose your own image to replace it.")
                                                  : qsTr("No local artwork selected.")
                                        }
                                    }
                                    DetailAction { Layout.fillWidth: true; small: true; iconName: "image"; text: qsTr("Choose artwork…"); enabled: page.backend.ready && !page.backend.busy; onClicked: page.chooseArtwork() }
                                    DetailAction { Layout.fillWidth: true; small: true; text: qsTr("Enter artwork path…"); enabled: page.backend.ready && !page.backend.busy; onClicked: page.enterArtworkPath() }
                                    DetailAction {
                                        Layout.fillWidth: true; small: true; iconName: "download"
                                        visible: page.backend.artwork_enabled
                                        text: page.detail && page.detail.artwork_fetched ? qsTr("Download again") : qsTr("Download artwork")
                                        enabled: page.backend.ready && !page.backend.artwork_fetching && page.detail !== null
                                        onClicked: page.backend.fetch_artwork(page.backend.selected_id)
                                    }
                                    Caption { Layout.fillWidth: true; visible: page.backend.artwork_enabled && page.backend.artwork_status.length > 0; text: page.backend.artwork_status; Accessible.role: Accessible.StaticText }
                                    DetailAction { Layout.fillWidth: true; small: true; ghost: true; text: qsTr("Clear artwork"); enabled: page.backend.ready && !page.backend.busy && page.detail && page.detail.artwork_label !== null; onClicked: page.backend.clear_artwork(page.backend.selected_id) }
                                }
                            }
                            Panel {
                                Layout.fillWidth: true
                                implicitHeight: manage.implicitHeight + 40
                                opaque: page.backend.opaque
                                ColumnLayout {
                                    id: manage
                                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
                                    spacing: 12
                                    SectionLabel { text: qsTr("Library entry") }
                                    Caption { Layout.fillWidth: true; text: qsTr("Removing keeps every game file on disk; only the saved profile and preferences go.") }
                                    DetailAction { Layout.fillWidth: true; small: true; danger: true; iconName: "trash"; text: qsTr("Remove from library"); enabled: page.backend.ready && !page.backend.busy && !page.profileReserved && !page.sessions.launching; onClicked: page.removeRequested() }
                                }
                            }
                        }
                    }
                    // Configuration summary.
                    RowLayout {
                        spacing: 24
                        Panel {
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignTop
                            implicitHeight: rows.implicitHeight + 8
                            opaque: page.backend.opaque
                            ColumnLayout {
                                id: rows
                                anchors { left: parent.left; right: parent.right; top: parent.top; topMargin: 4 }
                                spacing: 0
                                ConfigRow { label: qsTr("Runner"); value: page.umu ? qsTr("UMU / Proton") : qsTr("Linux executable"); first: true }
                                ConfigRow { label: qsTr("Executable"); value: page.detail ? page.detail.executable : "" }
                                ConfigRow { label: qsTr("Working directory"); value: page.detail && page.detail.cwd ? page.detail.cwd : qsTr("Executable’s folder") }
                                ConfigRow { visible: page.umu; label: qsTr("Wine prefix"); value: page.detail && page.detail.prefix ? page.detail.prefix : qsTr("Not set") }
                                ConfigRow { label: qsTr("Arguments"); value: page.detail ? String(page.detail.argument_count) : "" }
                                ConfigRow { label: qsTr("Environment"); value: page.detail ? (page.detail.environment_count === 1 ? qsTr("1 override") : qsTr("%1 overrides").arg(page.detail.environment_count)) : "" }
                            }
                        }
                        Panel {
                            Layout.preferredWidth: 320
                            Layout.alignment: Qt.AlignTop
                            glass: true
                            opaque: page.backend.opaque
                            implicitHeight: side.implicitHeight + 40
                            ColumnLayout {
                                id: side
                                anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
                                spacing: 10
                                Heading { level: 3; Layout.fillWidth: true; text: qsTr("Launch profile for this game") }
                                Caption { Layout.fillWidth: true; text: qsTr("Changes apply the next time you press Play. A running session keeps the snapshot it started with.") }
                                DetailAction { Layout.fillWidth: true; Layout.topMargin: 6; small: true; primary: true; text: qsTr("Open full configuration"); enabled: page.backend.ready && !page.backend.busy && !page.profileReserved && !page.sessions.launching; onClicked: page.configure() }
                                DetailAction { Layout.fillWidth: true; small: true; text: qsTr("Runner settings"); visible: page.umu; onClicked: page.runnerSettings() }
                            }
                        }
                    }
                    // Files.
                    Panel {
                        Layout.alignment: Qt.AlignTop
                        implicitHeight: files.implicitHeight + 8
                        opaque: page.backend.opaque
                        ColumnLayout {
                            id: files
                            anchors { left: parent.left; right: parent.right; top: parent.top; topMargin: 4 }
                            spacing: 0
                            ConfigRow { label: qsTr("Executable"); value: page.detail ? page.detail.executable : ""; first: true }
                            ConfigRow { label: qsTr("Working directory"); value: page.detail && page.detail.cwd ? page.detail.cwd : qsTr("Executable’s folder") }
                            ConfigRow { visible: page.umu; label: qsTr("Wine prefix"); value: page.detail && page.detail.prefix ? page.detail.prefix : qsTr("Not set") }
                            ConfigRow { label: qsTr("Artwork"); value: page.detail && page.detail.artwork_label ? page.detail.artwork_label : (page.detail && page.detail.artwork_fetched ? qsTr("Downloaded") : qsTr("Generated cover")) }
                            ConfigRow { label: qsTr("Availability"); value: page.detail ? page.detail.availability : ""; mono: false }
                        }
                    }
                    // Logs.
                    SessionPanel {
                        id: logPanel
                        backend: page.sessions
                        active: page.tab === 3 && page.visible
                        implicitHeight: 620
                        onRunnerSettings: page.runnerSettings()
                    }
                }
            }
        }
    }
    component Fact: Item {
        id: fact
        property string label
        property string value
        property string sub
        property color dot: "transparent"
        property bool last: false
        property bool monoValue: false
        Layout.fillWidth: true
        Layout.preferredWidth: 170
        implicitHeight: factColumn.implicitHeight + 32
        Rectangle { visible: !fact.last; anchors.right: parent.right; width: 1; height: parent.height; color: Theme.hairline }
        Column {
            id: factColumn
            x: 20; y: 16
            width: parent.width - 40
            spacing: 3
            SectionLabel { text: fact.label }
            Row {
                spacing: 8
                topPadding: 3
                Rectangle { visible: fact.dot !== Qt.color("transparent"); anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: fact.dot }
                Label { textFormat: Text.PlainText; text: fact.value; elide: Text.ElideRight; width: Math.min(implicitWidth, factColumn.width - 16); font.family: fact.monoValue ? Theme.mono : Theme.family; font.pixelSize: 15; font.weight: Font.DemiBold; color: Theme.ink }
            }
            Label { width: parent.width; textFormat: Text.PlainText; text: fact.sub; elide: Text.ElideRight; font.family: Theme.family; font.pixelSize: 12; color: Theme.muted }
        }
    }
    component ConfigRow: Item {
        id: row
        property string label
        property string value
        property bool first: false
        property bool mono: true
        Layout.fillWidth: true
        implicitHeight: Math.max(50, valueText.implicitHeight + 26)
        Rectangle { visible: !row.first; width: parent.width - 32; x: 16; height: 1; color: Theme.hairline5 }
        Label { x: 16; anchors.verticalCenter: parent.verticalCenter; textFormat: Text.PlainText; text: row.label; font.family: Theme.family; font.pixelSize: 14; color: Theme.soft }
        Label {
            id: valueText
            anchors.right: parent.right; anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, parent.width * 0.62)
            horizontalAlignment: Text.AlignRight
            textFormat: Text.PlainText
            text: row.value
            wrapMode: Text.WrapAnywhere
            font.family: row.mono ? Theme.mono : Theme.family
            font.pixelSize: row.mono ? 12 : 13
            color: Theme.ink
        }
    }
}
