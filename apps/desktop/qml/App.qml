import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Shapes
import QtCore
import Qt.labs.folderlistmodel
import Rustris
import "components" as UI

ApplicationWindow {
    id: window
    width: 1440
    height: 900
    minimumWidth: 980
    minimumHeight: 660
    visible: true
    title: qsTr("Rustris — Library")
    // The mockup draws its own title bar and window controls.
    flags: Qt.Window | Qt.FramelessWindowHint
    color: UI.Theme.background
    font.family: UI.Theme.family
    font.pixelSize: 14
    palette.window: UI.Theme.graphite
    palette.windowText: UI.Theme.ink
    palette.text: UI.Theme.ink
    palette.base: UI.Theme.background
    palette.button: UI.Theme.elevated
    palette.buttonText: UI.Theme.ink
    palette.highlight: UI.Theme.accent
    palette.highlightedText: UI.Theme.accentText
    palette.toolTipBase: UI.Theme.overlay
    palette.toolTipText: UI.Theme.ink
    LayoutMirroring.enabled: Qt.application.layoutDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true
    // library | detail | editor | runners | settings | sessions
    property string page: "library"
    property string returnPage: "library"
    property bool sidebarCompact: false
    property var detail: backend.detail_json.length > 0 ? JSON.parse(backend.detail_json) : null
    property bool discardApproved: false
    property var sessionSnapshot: sessions.snapshot_json.length ? JSON.parse(sessions.snapshot_json) : ({})
    property bool profileReserved: sessionSnapshot.sessions ? sessionSnapshot.sessions.some(function(s) { return s.reserved && s.game === backend.selected_id; }) : false
    readonly property bool overlayOpen: removeDialog.visible || discardDialog.visible || closeSessionsDialog.visible || artworkPicker.visible || artworkPathDialog.visible || reviewDialog.visible || detailPage.logs.overlayOpen || sessionsPage.overlayOpen
    property string pendingPlay: ""
    // Returns false when an unsaved form keeps the user on the current page.
    function go(target, keepFocus) {
        if (page === target) return true;
        // Leaving an edited profile asks first; Cancel and Save close the form explicitly.
        if (page === "editor" && profileEditor.opened && profileEditor.dirty && target !== "runners") {
            discardDialog.target = target;
            discardDialog.open();
            return false;
        }
        if (target !== "editor" && target !== "runners" && target !== "settings" && target !== "sessions") returnPage = target;
        page = target;
        if (!keepFocus) Qt.callLater(focusPage);
        return true;
    }
    function back() { go(returnPage); }
    function focusPage() {
        if (page === "library") libraryPage.focusSelection();
        else if (page === "settings") settingsPage.focusFirst();
        else if (page === "sessions") sessionsPage.picker.forceActiveFocus();
        else if (page === "detail") detailPage.focusPrimary();
    }
    function focusSelection() { if (page === "library") libraryPage.focusSelection(); }
    function openLibrary(view) { if (go("library")) libraryPage.choose(view); }
    function openGame(id, keyboard) { detailPage.tab = 0; go("detail", true); Qt.callLater(function() { detailPage.focusPrimary(keyboard ? Qt.TabFocusReason : Qt.MouseFocusReason); }); }
    property bool reloadAfterReview: false
    function play() {
        if (!window.detail) return;
        detailPage.showLogs();
        go("detail");
        // Launch exactly the game whose saved details are shown.
        if (window.detail.runner === "umu") sessions.preview_umu(window.detail.id, window.detail.revision);
        else sessions.play(window.detail.id, window.detail.revision);
    }
    // A card's Play waits until that game's details have loaded, so it can never
    // launch the previously selected game.
    function playWhenLoaded(id) {
        pendingPlay = id;
        openGame(id, false);
        if (window.detail && window.detail.id === id && !backend.busy) { pendingPlay = ""; play(); }
    }
    onDetailChanged: if (pendingPlay.length && window.detail && window.detail.id === pendingPlay) { pendingPlay = ""; Qt.callLater(play); }
    function editSelected() {
        backend.edit(backend.selected_id);
    }
    function search(text) {
        if (globalSearch.text !== text) globalSearch.text = text;
        if (libraryPage.search.text !== text) libraryPage.search.text = text;
        backend.filter(text, libraryPage.view === "favorites");
    }
    Sessions { id: sessions; objectName: "sessionBackend" }
    Timer { interval: 200; repeat: true; running: backend.ready && sessions.active_count > 0 && !sessions.closing; onTriggered: sessions.poll() }
    Connections {
        target: sessions
        function onClose_readyChanged() { if (sessions.close_ready) window.close(); }
        // New runners clear remembered reviews (D-010); reload so pages say Review and play.
        function onRunner_saved() { backend.load(); }
        function onBusyChanged() { if (!sessions.busy && window.reloadAfterReview) { window.reloadAfterReview = false; backend.load(); } }
        function onRunner_loaded() { window.discardApproved = false; runnerPage.open(); window.go("runners"); }
        function onReview_ready() {
            const review = JSON.parse(sessions.snapshot_json).review;
            if (!review) return;
            // D-010: an unchanged, previously approved review launches directly, but
            // only while that game's page is still in front; if the user moved on
            // (for example into Configure) the prepared launch is dropped.
            if (review.ready && review.approved) {
                if (window.page === "detail" && !window.overlayOpen && window.detail && window.detail.id === review.game) {
                    sessions.confirm_umu(review.ticket);
                    detailPage.showLogs(); window.go("detail", true);
                } else {
                    sessions.discard_umu(review.ticket);
                }
                return;
            }
            reviewDialog.confirmed = false; reviewDialog.review = review; reviewDialog.open();
        }
    }
    Library { id: backend; objectName: "libraryBackend"; Component.onCompleted: load() }
    Shortcut { sequences: [StandardKey.Find, "Ctrl+K"]; enabled: !window.overlayOpen && window.page !== "editor" && window.page !== "runners"; onActivated: { window.go("library", true); globalSearch.forceActiveFocus(); globalSearch.selectAll(); } }
    Shortcut { sequences: [StandardKey.New]; enabled: backend.ready && !backend.busy && !window.overlayOpen && window.page !== "editor" && window.page !== "runners"; onActivated: backend.create() }
    Shortcut { sequences: ["Ctrl+B"]; enabled: !window.overlayOpen; onActivated: window.sidebarCompact = !window.sidebarCompact }
    // Escape leaves the current page wherever focus is on it (the sidebar included).
    Shortcut {
        sequences: [StandardKey.Cancel]
        enabled: !window.overlayOpen && window.page !== "library" && !detailPage.logs.picker.popup.visible && !sessionsPage.picker.popup.visible
        onActivated: {
            if (window.page === "editor") { if (!backend.busy) profileEditor.close(); }
            else if (window.page === "runners") { if (!sessions.action_busy) runnerPage.close(); }
            else if (window.page === "detail") window.openLibrary(libraryPage.view);
            else window.back();
        }
    }
    onClosing: function(event) {
        if (backend.busy) { event.accepted = false; backend.request_close(); }
        else if (((profileEditor.opened && profileEditor.dirty) || (runnerPage.opened && runnerPage.dirty) || (artworkPathDialog.visible && artworkPathDialog.dirty)) && !discardApproved) { event.accepted = false; discardDialog.open(); }
        else if (!sessions.close_ready && (sessions.busy || sessions.active_count > 0)) { event.accepted = false; closeSessionsDialog.open(); }
    }
    Connections {
        target: backend
        function onReadyChanged() { if (backend.ready) { sessions.watch(backend.selected_id); Qt.callLater(libraryPage.revealSelection); } }
        function onSelected_idChanged() { if (backend.ready) sessions.watch(backend.selected_id); }
        function onSelected_rowChanged() { Qt.callLater(libraryPage.revealSelection); }
        function onModelReset() { Qt.callLater(libraryPage.revealSelection); }
        function onOpen_editor() {
            window.discardApproved = false;
            profileEditor.gameName = window.detail ? window.detail.name : "";
            profileEditor.gameId = window.detail ? window.detail.id : "";
            profileEditor.open();
            window.go("editor");
        }
        function onSaved() { profileEditor.opened = false; detailPage.tab = 0; window.returnPage = "library"; window.go("detail"); }
        function onArtwork_saved() { artworkPathDialog.dirty = false; artworkPathDialog.close(); }
        function onClose_readyChanged() {
            if (backend.close_ready) {
                backend.cancel_close();
                window.close();
            }
        }
    }

    // Ambient ember glow at the top right of the window.
    Shape {
        anchors.fill: parent
        ShapePath {
            strokeColor: "transparent"
            fillGradient: RadialGradient {
                centerX: window.width * 0.72; centerY: -window.height * 0.08
                focalX: centerX; focalY: centerY
                centerRadius: 900
                GradientStop { position: 0; color: "#14ff681f" }
                GradientStop { position: 0.62; color: "transparent" }
            }
            startX: 0; startY: 0
            PathLine { x: window.width; y: 0 }
            PathLine { x: window.width; y: window.height }
            PathLine { x: 0; y: window.height }
            PathLine { x: 0; y: 0 }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        // Title bar.
        Rectangle {
            id: header
            Layout.fillWidth: true
            implicitHeight: 56
            color: UI.Theme.header
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: UI.Theme.hairline }
            // Drags that start on empty title-bar space move the window; controls
            // such as the search field keep their own presses and drags.
            DragHandler {
                target: null
                grabPermissions: PointerHandler.TakeOverForbidden
                onActiveChanged: if (active) window.startSystemMove()
            }
            TapHandler {
                acceptedButtons: Qt.LeftButton
                onDoubleTapped: window.visibility === Window.Maximized ? window.showNormal() : window.showMaximized()
            }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 20
                anchors.rightMargin: 12
                spacing: 14
                Row {
                    Layout.preferredWidth: 196
                    spacing: 10
                    Accessible.role: Accessible.StaticText
                    Accessible.name: qsTr("Rustris")
                    Shape {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 28; height: 28
                        preferredRendererType: Shape.CurveRenderer
                        ShapePath {
                            scale: Qt.size(28 / 32, 28 / 32)
                            fillColor: "transparent"
                            strokeWidth: 2.4
                            joinStyle: ShapePath.RoundJoin
                            strokeGradient: LinearGradient { x1: 0; y1: 0; x2: 32; y2: 32; GradientStop { position: 0; color: "#ffac52" } GradientStop { position: 1; color: "#ff5c12" } }
                            PathSvg { path: "M16 2.6 27.8 9.4v13.2L16 29.4 4.2 22.6V9.4z" }
                        }
                        ShapePath {
                            scale: Qt.size(28 / 32, 28 / 32)
                            strokeColor: "transparent"
                            fillGradient: LinearGradient { x1: 0; y1: 0; x2: 32; y2: 32; GradientStop { position: 0; color: "#ffac52" } GradientStop { position: 1; color: "#ff5c12" } }
                            PathSvg { path: "M13 11v10l8.2-5z" }
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Rustris")
                        font.family: UI.Theme.display
                        font.pixelSize: 18
                        font.weight: Font.Bold
                        font.letterSpacing: -0.18
                        color: UI.Theme.ink
                        Accessible.ignored: true
                    }
                }
                UI.Field {
                    id: globalSearch
                    // Shrinks before the window controls do; caps at the mockup's 480 px.
                    Layout.fillWidth: true
                    Layout.preferredWidth: 480
                    Layout.maximumWidth: 480
                    Layout.minimumWidth: 160
                    implicitHeight: 38
                    iconName: "search"
                    rightPadding: 64
                    placeholderText: backend.total === 1 ? qsTr("Search 1 game") : qsTr("Search %1 games").arg(backend.total.toLocaleString(Qt.locale(), "f", 0))
                    Accessible.name: qsTr("Search games")
                    onTextEdited: { window.go("library", true); window.search(text); }
                    Keys.onDownPressed: libraryPage.focusSelection()
                    Rectangle {
                        anchors.right: parent.right; anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        implicitWidth: kbd.implicitWidth + 12; implicitHeight: 20; radius: 5
                        color: UI.Theme.fill4; border.color: UI.Theme.line10
                        Text { id: kbd; anchors.centerIn: parent; text: "Ctrl K"; font.family: UI.Theme.mono; font.pixelSize: 11; color: UI.Theme.secondary }
                    }
                }
                Item { Layout.fillWidth: true }
                Button {
                    id: activity
                    readonly property bool busy: sessions.active_count > 0
                    hoverEnabled: true
                    implicitHeight: 34
                    leftPadding: 8; rightPadding: 12
                    Accessible.name: qsTr("Sessions and logs")
                    Accessible.description: busy ? qsTr("%1 active").arg(sessions.active_count) : qsTr("No active sessions")
                    onClicked: { sessions.refresh(); window.go("sessions"); }
                    contentItem: Row {
                        spacing: 8
                        Item {
                            width: 20; height: 20
                            anchors.verticalCenter: parent.verticalCenter
                            Shape {
                                anchors.fill: parent
                                preferredRendererType: Shape.CurveRenderer
                                ShapePath { strokeColor: "#1fffffff"; strokeWidth: 2.5; fillColor: "transparent"; PathAngleArc { centerX: 10; centerY: 10; radiusX: 8; radiusY: 8; startAngle: 0; sweepAngle: 360 } }
                                ShapePath {
                                    strokeColor: activity.busy ? UI.Theme.accent : UI.Theme.muted
                                    strokeWidth: 2.5; capStyle: ShapePath.RoundCap; fillColor: "transparent"
                                    PathAngleArc { centerX: 10; centerY: 10; radiusX: 8; radiusY: 8; startAngle: -90; sweepAngle: activity.busy ? 150 : 0 }
                                }
                                RotationAnimation on rotation { running: activity.busy && !backend.reduced_motion; from: 0; to: 360; duration: 1600; loops: Animation.Infinite }
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: activity.busy ? qsTr("%1 active").arg(sessions.active_count) : qsTr("Sessions")
                            font.family: UI.Theme.family; font.pixelSize: 13; font.weight: Font.DemiBold
                            color: UI.Theme.ink
                        }
                    }
                    background: Rectangle {
                        radius: height / 2
                        color: activity.busy ? "#14ff681f" : (activity.hovered ? UI.Theme.fill6 : "transparent")
                        border.color: activity.busy ? "#40ff7832" : (activity.hovered ? UI.Theme.line16 : UI.Theme.line8)
                        Rectangle { anchors.fill: parent; anchors.margins: -3; radius: height / 2; color: "transparent"; border.width: 2; border.color: UI.Theme.ember; visible: activity.activeFocus && activity.focusReason !== Qt.MouseFocusReason }
                    }
                }
                UI.IconButton {
                    iconName: "gear"
                    Accessible.name: qsTr("Settings")
                    ToolTip.visible: hovered; ToolTip.text: qsTr("Settings")
                    onClicked: window.go("settings")
                }
                Rectangle { implicitWidth: 1; implicitHeight: 20; color: UI.Theme.line10; Layout.leftMargin: 4; Layout.rightMargin: 4 }
                Row {
                    spacing: 6
                    UI.IconButton { round: true; iconName: "min"; iconSize: 14; Accessible.name: qsTr("Minimize"); onClicked: window.showMinimized() }
                    UI.IconButton { round: true; iconName: "max"; iconSize: 14; Accessible.name: window.visibility === Window.Maximized ? qsTr("Restore") : qsTr("Maximize"); onClicked: window.visibility === Window.Maximized ? window.showNormal() : window.showMaximized() }
                    UI.IconButton { round: true; closeStyle: true; iconName: "close"; iconSize: 14; Accessible.name: qsTr("Close"); onClicked: window.close() }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0
            // Sidebar.
            Rectangle {
                Layout.preferredWidth: window.sidebarCompact ? 72 : (window.width < 1200 ? 220 : 240)
                Layout.fillHeight: true
                color: backend.opaque ? UI.Theme.raisedBase : UI.Theme.sidebar
                Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: UI.Theme.hairline }
                ColumnLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12; anchors.rightMargin: 12; anchors.topMargin: 18; anchors.bottomMargin: 16
                    spacing: 2
                    Accessible.role: Accessible.Grouping
                    Accessible.name: qsTr("Primary")
                    UI.NavItem { Layout.fillWidth: true; compact: window.sidebarCompact; iconName: "library"; text: qsTr("Library"); count: backend.total.toLocaleString(Qt.locale(), "f", 0); active: (window.page === "library" || window.page === "detail") && libraryPage.view === "all"; onClicked: window.openLibrary("all") }
                    UI.NavItem { Layout.fillWidth: true; compact: window.sidebarCompact; iconName: "star"; text: qsTr("Favorites"); count: backend.favorite_total.toLocaleString(Qt.locale(), "f", 0); active: (window.page === "library" || window.page === "detail") && libraryPage.view === "favorites"; onClicked: window.openLibrary("favorites") }
                    UI.NavItem { Layout.fillWidth: true; compact: window.sidebarCompact; iconName: "terminal"; text: qsTr("Sessions"); count: sessions.active_count > 0 ? String(sessions.active_count) : ""; active: window.page === "sessions"; onClicked: { sessions.refresh(); window.go("sessions"); } }
                    UI.NavItem { Layout.fillWidth: true; compact: window.sidebarCompact; iconName: "wine"; text: qsTr("Runners"); active: window.page === "runners"; enabled: !sessions.action_busy; onClicked: sessions.load_runner_settings() }
                    UI.NavItem { Layout.fillWidth: true; compact: window.sidebarCompact; iconName: "gear"; text: qsTr("Settings"); active: window.page === "settings"; onClicked: window.go("settings") }
                    Rectangle { Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 12; Layout.leftMargin: 4; Layout.rightMargin: 4; implicitHeight: 1; color: UI.Theme.hairline }
                    UI.SectionLabel { visible: !window.sidebarCompact; leftPadding: 12; bottomPadding: 8; text: qsTr("Sources") }
                    UI.NavItem { Layout.fillWidth: true; compact: window.sidebarCompact; mono: "LO"; monoTint: UI.Theme.accentLink; text: qsTr("Local"); count: backend.total.toLocaleString(Qt.locale(), "f", 0); onClicked: window.openLibrary("all") }
                    Item { Layout.fillHeight: true; Layout.minimumHeight: 24 }
                    Rectangle {
                        visible: !window.sidebarCompact
                        Layout.fillWidth: true
                        Layout.leftMargin: 4; Layout.rightMargin: 4; Layout.bottomMargin: 10
                        implicitHeight: systemCard.implicitHeight + 24
                        radius: UI.Theme.radiusLg
                        color: "#06ffffff"
                        border.color: UI.Theme.hairline
                        Column {
                            id: systemCard
                            x: 12; y: 12
                            width: parent.width - 24
                            spacing: 6
                            Row {
                                spacing: 8
                                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 7; height: 7; radius: 4; color: backend.ready ? UI.Theme.success : UI.Theme.warning }
                                Text { text: backend.busy ? (backend.closing ? qsTr("Finishing changes…") : (backend.ready ? qsTr("Saving changes…") : qsTr("Opening library…"))) : qsTr("Local library"); font.family: UI.Theme.family; font.pixelSize: 12; font.weight: Font.DemiBold; color: UI.Theme.ink }
                            }
                            Text { width: parent.width; wrapMode: Text.Wrap; text: qsTr("No account required\nProfiles stay on this device"); font.family: UI.Theme.mono; font.pixelSize: 11; lineHeight: 1.2; color: UI.Theme.muted }
                        }
                    }
                    UI.NavItem {
                        Layout.fillWidth: true
                        compact: window.sidebarCompact
                        iconName: "sidebar"
                        text: window.sidebarCompact ? qsTr("Expand sidebar") : qsTr("Collapse")
                        onClicked: window.sidebarCompact = !window.sidebarCompact
                        Rectangle {
                            visible: !window.sidebarCompact
                            anchors.right: parent.right; anchors.rightMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            implicitWidth: bkbd.implicitWidth + 12; implicitHeight: 20; radius: 5
                            color: UI.Theme.fill4; border.color: UI.Theme.line10
                            Text { id: bkbd; anchors.centerIn: parent; text: "Ctrl B"; font.family: UI.Theme.mono; font.pixelSize: 11; color: UI.Theme.secondary }
                        }
                    }
                }
            }
            // Pages.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 0
                Rectangle {
                    Layout.fillWidth: true
                    Layout.leftMargin: 32; Layout.rightMargin: 32; Layout.topMargin: 16
                    visible: backend.error.length > 0
                    implicitHeight: errorLayout.implicitHeight + 24
                    radius: UI.Theme.radiusLg
                    color: "#1fe5484d"
                    border.color: "#59e5484d"
                    RowLayout {
                        id: errorLayout
                        anchors.fill: parent; anchors.margins: 12; anchors.leftMargin: 16
                        spacing: 12
                        UI.Icon { name: "alert"; size: 18; strokeWidth: 2; color: UI.Theme.danger }
                        Label { textFormat: Text.PlainText; Layout.fillWidth: true; text: backend.error; color: UI.Theme.danger; wrapMode: Text.Wrap; font.family: UI.Theme.family; font.pixelSize: 14; Accessible.name: text }
                        UI.ActionButton { small: true; text: qsTr("Reload library"); enabled: !backend.busy; onClicked: backend.load() }
                    }
                }
                StackLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    currentIndex: ["library", "detail", "editor", "runners", "settings", "sessions"].indexOf(window.page)
                    UI.LibraryPage {
                        id: libraryPage
                        backend: backend
                        onOpenGame: (id, keyboard) => window.openGame(id, keyboard)
                        onSearchEdited: text => window.search(text)
                        onPlayGame: id => window.playWhenLoaded(id)
                    }
                    UI.DetailPage {
                        id: detailPage
                        backend: backend
                        sessions: sessions
                        detail: window.detail
                        sessionSnapshot: window.sessionSnapshot
                        profileReserved: window.profileReserved
                        onBack: window.openLibrary(libraryPage.view)
                        onPlay: window.play()
                        onConfigure: window.editSelected()
                        onRemoveRequested: removeDialog.open()
                        onChooseArtwork: { artworkPicker.targetGame = backend.selected_id; artworkPicker.open(); }
                        onEnterArtworkPath: { window.discardApproved = false; backend.prepare_artwork_path(); artworkPathDialog.targetGame = backend.selected_id; artworkPathDialog.open(); }
                        onRunnerSettings: sessions.load_runner_settings()
                    }
                    UI.ProfileEditor {
                        id: profileEditor
                        backend: backend
                        onFinished: window.back()
                        onRunnerSettings: sessions.load_runner_settings()
                    }
                    UI.RunnerDialog {
                        id: runnerPage
                        backend: sessions
                        opaque: backend.opaque
                        onClosed: window.go(profileEditor.opened ? "editor" : window.returnPage)
                    }
                    UI.SettingsPage {
                        id: settingsPage
                        backend: backend
                        sessions: sessions
                        onDone: window.back()
                        onRunnerSettings: sessions.load_runner_settings()
                    }
                    FocusScope {
                        id: sessionsHost
                        property alias picker: sessionsPage.picker
                        ColumnLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 32; anchors.rightMargin: 32; anchors.topMargin: 28; anchors.bottomMargin: 32
                            spacing: 20
                            ColumnLayout {
                                spacing: 6
                                UI.Heading { level: 1; text: qsTr("Sessions and logs") }
                                Label { textFormat: Text.PlainText; font.family: UI.Theme.family; font.pixelSize: 14; color: UI.Theme.secondary; text: sessions.active_count === 1 ? qsTr("1 active session") : qsTr("%1 active sessions").arg(sessions.active_count) }
                            }
                            UI.SessionPanel {
                                id: sessionsPage
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                backend: sessions
                                active: window.page === "sessions"
                                showDone: true
                                onDone: window.back()
                                onRunnerSettings: sessions.load_runner_settings()
                            }
                        }
                    }
                }
            }
        }
    }
    // Frameless resize edges.
    Repeater {
        model: [
            { edges: Qt.LeftEdge, x: 0, y: 8, w: 6, h: -16, cursor: Qt.SizeHorCursor },
            { edges: Qt.RightEdge, x: -6, y: 8, w: 6, h: -16, cursor: Qt.SizeHorCursor },
            { edges: Qt.BottomEdge, x: 8, y: -6, w: -16, h: 6, cursor: Qt.SizeVerCursor },
            { edges: Qt.BottomEdge | Qt.RightEdge, x: -10, y: -10, w: 10, h: 10, cursor: Qt.SizeFDiagCursor },
            { edges: Qt.BottomEdge | Qt.LeftEdge, x: 0, y: -10, w: 10, h: 10, cursor: Qt.SizeBDiagCursor }
        ]
        delegate: MouseArea {
            required property var modelData
            x: modelData.x < 0 ? window.width + modelData.x : modelData.x
            y: modelData.y < 0 ? window.height + modelData.y : modelData.y
            width: modelData.w <= 0 ? window.width + modelData.w : modelData.w
            height: modelData.h <= 0 ? window.height + modelData.h : modelData.h
            visible: window.visibility !== Window.Maximized && window.visibility !== Window.FullScreen
            cursorShape: modelData.cursor
            acceptedButtons: Qt.LeftButton
            onPressed: window.startSystemResize(modelData.edges)
        }
    }
    UI.OverlayDialog {
        id: artworkPicker
        property string targetGame: ""
        property url folder
        title: qsTr("Choose local artwork")
        width: Math.min(560, parent.width - 48)
        height: Math.min(640, parent.height - 48)
        closePolicy: Popup.CloseOnEscape
        onAboutToShow: folder = StandardPaths.writableLocation(StandardPaths.HomeLocation)
        onOpened: { artworkFileList.currentIndex = -1; artworkFileList.positionViewAtBeginning(); parentFolder.forceActiveFocus(); }
        FolderListModel {
            id: artworkFolders
            folder: artworkPicker.folder
            showDirsFirst: true
            nameFilters: ["*.png", "*.jpg", "*.jpeg", "*.webp", "*.bmp"]
            showDotAndDotDot: false
            sortField: FolderListModel.Name
        }
        contentItem: ColumnLayout {
            Accessible.role: Accessible.Dialog
            Accessible.name: artworkPicker.title
            spacing: 12
            UI.Heading { level: 2; size: 24; text: artworkPicker.title }
            UI.Caption { Layout.fillWidth: true; text: qsTr("Choose an image in this folder, or open a folder. The file stays where it is.") }
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                UI.ActionButton {
                    id: parentFolder
                    small: true
                    iconName: "chevronLeft"
                    text: qsTr("Up")
                    Accessible.name: qsTr("Parent folder")
                    enabled: artworkFolders.parentFolder.toString().length > 0 && artworkFolders.parentFolder !== artworkPicker.folder
                    onClicked: artworkPicker.folder = artworkFolders.parentFolder
                }
                Label { Layout.fillWidth: true; textFormat: Text.PlainText; elide: Text.ElideMiddle; font.family: UI.Theme.mono; font.pixelSize: 12; color: UI.Theme.muted; text: decodeURIComponent(artworkPicker.folder.toString().replace("file://", "")) }
            }
            ListView {
                id: artworkFileList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: artworkFolders
                spacing: 4
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Artwork files")
                function focusBeforeFiles() {
                    if (parentFolder.enabled) parentFolder.forceActiveFocus()
                    else artworkPickerCancel.forceActiveFocus()
                }
                function focusFile(row) {
                    currentIndex = row
                    positionViewAtIndex(row, ListView.Contain)
                    forceLayout()
                    const item = itemAtIndex(row)
                    if (item) item.forceActiveFocus()
                    else focusBeforeFiles()
                }
                delegate: Button {
                    id: fileButton
                    required property int index
                    required property string fileName
                    required property url fileUrl
                    required property bool fileIsDir
                    width: ListView.view.width
                    implicitHeight: 38
                    hoverEnabled: true
                    activeFocusOnTab: true
                    text: fileIsDir ? fileName + "/" : fileName
                    Accessible.name: fileIsDir ? qsTr("Folder %1").arg(fileName) : fileName
                    contentItem: Row {
                        spacing: 10
                        leftPadding: 10
                        UI.Icon { anchors.verticalCenter: parent.verticalCenter; name: fileButton.fileIsDir ? "folder" : "image"; size: 15; color: fileButton.hovered ? UI.Theme.ink : UI.Theme.secondary }
                        Text { anchors.verticalCenter: parent.verticalCenter; width: fileButton.width - 50; elide: Text.ElideMiddle; text: fileButton.text; font.family: UI.Theme.family; font.pixelSize: 13; color: fileButton.hovered ? UI.Theme.ink : UI.Theme.soft }
                    }
                    background: Rectangle {
                        radius: UI.Theme.radiusSm
                        color: fileButton.hovered ? UI.Theme.fill7 : "transparent"
                        border.width: fileButton.activeFocus && fileButton.focusReason !== Qt.MouseFocusReason ? 2 : 0
                        border.color: UI.Theme.ember
                    }
                    Keys.onPressed: event => {
                        if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
                        if (event.key === Qt.Key_Tab && !(event.modifiers & Qt.ShiftModifier)) {
                            if (index + 1 < artworkFileList.count) artworkFileList.focusFile(index + 1)
                            else artworkPickerCancel.forceActiveFocus()
                            event.accepted = true
                        } else if (event.key === Qt.Key_Backtab || (event.key === Qt.Key_Tab && (event.modifiers & Qt.ShiftModifier))) {
                            if (index > 0) artworkFileList.focusFile(index - 1)
                            else artworkFileList.focusBeforeFiles()
                            event.accepted = true
                        }
                    }
                    onClicked: {
                        if (fileIsDir) artworkPicker.folder = fileUrl
                        else if (artworkPicker.targetGame === backend.selected_id) {
                            backend.set_artwork(artworkPicker.targetGame, fileUrl)
                            artworkPicker.close()
                        }
                    }
                }
            }
            UI.Caption { Layout.fillWidth: true; visible: artworkFolders.status === FolderListModel.Ready && artworkFolders.count === 0; text: qsTr("No images or folders here.") }
            UI.ActionButton { id: artworkPickerCancel; Layout.alignment: Qt.AlignRight; text: qsTr("Cancel"); onClicked: artworkPicker.close() }
        }
    }
    UI.OverlayDialog {
        id: artworkPathDialog
        property string targetGame: ""
        property bool dirty: false
        width: Math.min(640, parent.width - 48)
        title: qsTr("Enter artwork path")
        closePolicy: backend.busy ? Popup.NoAutoClose : Popup.CloseOnEscape
        // Reset before the dialog shows; the open animation would otherwise clear typed text.
        onAboutToShow: { artworkPath.text = ""; artworkExact.checked = false; dirty = false; }
        onOpened: artworkPath.forceActiveFocus()
        contentItem: ColumnLayout {
            Accessible.role: Accessible.Dialog; Accessible.name: artworkPathDialog.title
            spacing: 14
            UI.Heading { level: 2; size: 24; text: artworkPathDialog.title }
            UI.Caption { Layout.fillWidth: true; text: qsTr("Enter an absolute path to an existing image. The file remains in its current location.") }
            UI.Field { id: artworkPath; Layout.fillWidth: true; monospace: true; Accessible.name: qsTr("Artwork path"); placeholderText: qsTr("/absolute/path/to/image.png"); onTextEdited: artworkPathDialog.dirty = true }
            UI.Check { id: artworkExact; text: qsTr("Artwork path uses exact Unix bytes"); onToggled: if (artworkPathDialog.visible) artworkPathDialog.dirty = true }
            UI.Caption { Layout.fillWidth: true; visible: artworkExact.checked; text: qsTr("Use \\xNN for bytes and \\\\ for a literal backslash. Non-text paths are preserved but cannot be displayed by Qt.") }
            UI.Caption { Layout.fillWidth: true; visible: backend.form_error.length > 0; color: UI.Theme.danger; text: backend.form_error; Accessible.role: Accessible.AlertMessage }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 10
                UI.ActionButton { text: qsTr("Cancel"); enabled: !backend.busy; onClicked: artworkPathDialog.close() }
                UI.ActionButton { text: qsTr("Save artwork path"); primary: true; enabled: !backend.busy && artworkPathDialog.targetGame === backend.selected_id; onClicked: backend.set_artwork_path(artworkPathDialog.targetGame, artworkPath.text, artworkExact.checked) }
            }
        }
    }
    UI.OverlayDialog {
        id: removeDialog
        width: 480
        title: qsTr("Remove library entry?")
        onOpened: removeCancel.forceActiveFocus()
        contentItem: ColumnLayout {
            spacing: 18
            UI.Heading { Layout.fillWidth: true; level: 2; text: removeDialog.title }
            UI.Caption { Layout.fillWidth: true; text: qsTr("This removes the saved profile and library preferences for this entry. Game files stay on disk.") }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 10
                UI.ActionButton { id: removeCancel; text: qsTr("Cancel"); onClicked: removeDialog.close() }
                UI.ActionButton { danger: true; text: qsTr("Remove entry"); onClicked: { backend.remove(backend.selected_id); removeDialog.close(); window.openLibrary(libraryPage.view); } }
            }
        }
    }
    UI.OverlayDialog {
        id: discardDialog
        // Empty when closing the window; otherwise the page the user was going to.
        property string target: ""
        width: 480
        title: qsTr("Discard unsaved changes?")
        onClosed: target = ""
        onOpened: keepEditing.forceActiveFocus()
        contentItem: ColumnLayout {
            spacing: 18
            UI.Heading { Layout.fillWidth: true; level: 2; text: discardDialog.title }
            UI.Caption { Layout.fillWidth: true; text: qsTr("The changes in this form have not been saved.") }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 10
                UI.ActionButton { id: keepEditing; text: qsTr("Keep editing"); onClicked: discardDialog.close() }
                UI.ActionButton {
                    danger: true
                    text: discardDialog.target.length ? qsTr("Discard changes") : qsTr("Discard and close")
                    onClicked: {
                        const destination = discardDialog.target;
                        if (destination.length) {
                            profileEditor.opened = false;
                            profileEditor.dirty = false;
                            discardDialog.close();
                            window.go(destination);
                        } else {
                            window.discardApproved = true;
                            artworkPathDialog.close();
                            window.close();
                        }
                    }
                }
            }
        }
    }
    UI.UmuReviewDialog {
        id: reviewDialog
        backend: sessions
        // The sessions worker records the approval (D-010) while it launches; reload the
        // library once it is idle so the page offers Play instead of Review and play.
        onLaunchRequested: { window.reloadAfterReview = true; detailPage.showLogs(); window.go("detail", true); }
        // Once the modal has closed, focus the launched session in Logs.
        onClosed: if (confirmed) Qt.callLater(function() { detailPage.logs.picker.forceActiveFocus(); })
    }
    UI.OverlayDialog {
        id: closeSessionsDialog
        width: 520
        title: qsTr("Leave sessions running?")
        onOpened: keepOpen.forceActiveFocus()
        closePolicy: sessions.closing ? Popup.NoAutoClose : Popup.CloseOnEscape
        contentItem: ColumnLayout {
            spacing: 18
            UI.Heading { Layout.fillWidth: true; level: 2; text: closeSessionsDialog.title }
            UI.Caption { Layout.fillWidth: true; text: qsTr("Keep Rustris open to control sessions, or leave helpers observing and capturing output. Pending launch work finishes before closing. Unknown sessions keep their reservations; helper loss can affect game output.") }
            UI.Caption { Layout.fillWidth: true; visible: sessions.error.length > 0; text: sessions.error; color: UI.Theme.danger }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 10
                UI.ActionButton { id: keepOpen; text: qsTr("Keep open"); enabled: !sessions.closing; onClicked: closeSessionsDialog.close() }
                UI.ActionButton { text: qsTr("Leave running and close"); enabled: !sessions.closing; onClicked: sessions.leave_running() }
            }
        }
    }
}
