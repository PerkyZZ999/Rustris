import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import "States.js" as States
// Sessions and captured output, styled as the mockup's Logs tab.
Item {
    id: panel
    required property var backend
    property bool active: false
    property bool showDone: false
    signal runnerSettings()
    signal done()
    property var snapshot: backend.snapshot_json.length ? JSON.parse(backend.snapshot_json) : ({})
    property var detail: snapshot.selected || null
    property string confirmationSession: ""
    property int confirmationGeneration: 0
    property string confirmationAction: ""
    property alias picker: sessionPicker
    readonly property bool overlayOpen: evidenceDialog.visible || confirmDialog.visible
    property alias logArea: logText
    function count(value) { return value.toLocaleString(Qt.locale(), "f", 0); }
    function confirm(action) {
        confirmationSession = detail.id;
        confirmationGeneration = detail.generation;
        confirmationAction = action;
        confirmDialog.open();
    }
    implicitHeight: column.implicitHeight
    ColumnLayout {
        id: column
        anchors.fill: parent
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            ComboBox {
                id: sessionPicker
                Layout.fillWidth: true
                Layout.maximumWidth: 640
                implicitHeight: 40
                model: panel.snapshot.sessions || []
                textRole: "name"
                valueRole: "id"
                displayText: panel.detail ? panel.detail.name + " · " + States.label(panel.detail.state) : qsTr("No session selected")
                Accessible.name: qsTr("Choose session")
                onActivated: panel.backend.choose(currentValue)
                // Long game names elide in the middle so the session state stays visible.
                contentItem: Text {
                    textFormat: Text.PlainText
                    leftPadding: sessionPicker.mirrored ? 0 : 12
                    rightPadding: sessionPicker.mirrored ? 12 : 0
                    text: sessionPicker.displayText
                    font.family: Theme.family
                    font.pixelSize: 13
                    color: sessionPicker.enabled ? Theme.ink : Theme.muted
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideMiddle
                    Accessible.ignored: true
                }
                indicator: Icon { x: sessionPicker.width - width - 12; y: (sessionPicker.height - height) / 2; name: "chevronDown"; size: 14; strokeWidth: 2; color: Theme.secondary }
                background: Rectangle {
                    radius: Theme.radiusMd
                    color: Theme.fill4
                    border.color: sessionPicker.activeFocus ? "#99ff8c3c" : Theme.control
                }
                popup: Popup {
                    y: sessionPicker.height + 6
                    width: sessionPicker.width
                    padding: 6
                    implicitHeight: Math.min(contentItem.implicitHeight + 12, 360)
                    contentItem: ListView { clip: true; implicitHeight: contentHeight; model: sessionPicker.popup.visible ? sessionPicker.delegateModel : null; currentIndex: sessionPicker.highlightedIndex }
                    background: Rectangle { radius: Theme.radiusLg; color: Theme.overlay; border.color: Theme.line10 }
                }
                delegate: ItemDelegate {
                    id: option
                    required property var modelData
                    required property int index
                    width: sessionPicker.width - 12
                    height: 38
                    highlighted: sessionPicker.highlightedIndex === index
                    contentItem: Label { textFormat: Text.PlainText; text: option.modelData.name + " · " + States.label(option.modelData.state) + " · " + new Date(option.modelData.created_ms).toLocaleString(Qt.locale()); color: option.highlighted ? Theme.ink : Theme.soft; font.family: Theme.family; font.pixelSize: 13; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter }
                    background: Rectangle { radius: Theme.radiusSm; color: option.highlighted || option.hovered ? Theme.fill7 : "transparent" }
                }
            }
            Item { Layout.fillWidth: true }
            ActionButton { small: true; iconName: "refresh"; text: qsTr("Refresh"); enabled: !panel.backend.closing; onClicked: panel.backend.refresh() }
            ActionButton { small: true; iconName: "shield"; text: qsTr("Preflight evidence"); enabled: panel.detail && panel.detail.preflight; onClicked: evidenceDialog.open() }
            ActionButton { small: true; iconName: "gear"; text: qsTr("Runner settings"); enabled: !panel.backend.action_busy; onClicked: panel.runnerSettings() }
            ActionButton { small: true; visible: panel.showDone; text: qsTr("Done"); onClicked: panel.done() }
        }
        Caption { Layout.fillWidth: true; text: qsTr("Process observation does not establish game health or playability. External launchers are outside this scope.") }
        Caption { Layout.fillWidth: true; visible: panel.backend.error.length > 0; color: Theme.danger; text: panel.backend.error; Accessible.role: Accessible.AlertMessage }
        Caption { Layout.fillWidth: true; visible: panel.detail && panel.detail.error !== null; color: Theme.danger; text: panel.detail && panel.detail.error ? panel.detail.error : "" }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            ActionButton { small: true; text: qsTr("Cancel preparation"); visible: panel.backend.launching; onClicked: panel.backend.cancel_preparation() }
            ActionButton { small: true; iconName: "pause"; text: qsTr("Stop"); enabled: !panel.backend.action_busy && !panel.backend.launching && panel.detail && panel.detail.can_stop; onClicked: panel.backend.control(panel.detail.id, "stop", panel.detail.generation) }
            ActionButton { small: true; danger: true; text: qsTr("Force stop…"); enabled: !panel.backend.action_busy && !panel.backend.launching && panel.detail && panel.detail.can_force; onClicked: panel.confirm("force") }
            ActionButton { small: true; iconName: "plug"; text: qsTr("Reconnect"); enabled: !panel.backend.action_busy && !panel.backend.launching && panel.detail && panel.detail.reserved; onClicked: panel.backend.control(panel.detail.id, "reconnect", panel.detail.generation) }
            ActionButton { small: true; danger: true; text: qsTr("Release unknown…"); visible: panel.detail && panel.detail.can_release; enabled: !panel.backend.action_busy && !panel.backend.launching; onClicked: panel.confirm("release") }
            Item { Layout.fillWidth: true }
        }
        Caption { Layout.fillWidth: true; visible: panel.detail && panel.detail.log_coverage === "runner_streams_application_unverified"; text: qsTr("Captured UMU/runner streams. Windows application console output is unverified; missing output does not establish game health.") }
        Panel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 260
            color: "#e6060609"
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: 12
                    Layout.leftMargin: 16
                    spacing: 12
                    Label {
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        elide: Text.ElideMiddle
                        font.family: Theme.mono; font.pixelSize: 12; color: Theme.secondary
                        text: panel.detail ? qsTr("session %1").arg(panel.detail.id) : qsTr("Play a profile to create a session.")
                    }
                    Label {
                        textFormat: Text.PlainText
                        font.family: Theme.family; font.pixelSize: 11; color: Theme.muted
                        text: panel.detail ? qsTr("%1 · owned children: %2 · root exit: %3 · root signal: %4").arg(States.label(panel.detail.state)).arg(panel.detail.owned_children === null ? qsTr("unobserved") : panel.detail.owned_children).arg(panel.detail.root_exit_code === null ? qsTr("unobserved") : panel.detail.root_exit_code).arg(panel.detail.root_signal === null ? qsTr("none observed") : panel.detail.root_signal) : ""
                    }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.hairline }
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    TextArea {
                        id: logText
                        objectName: "sessionLog"
                        textFormat: TextEdit.PlainText
                        text: panel.detail ? panel.detail.log_text : ""
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: Theme.soft
                        selectionColor: Theme.accent
                        selectedTextColor: Theme.accentText
                        font.family: Theme.mono
                        font.pixelSize: 12
                        leftPadding: 16; rightPadding: 16; topPadding: 10; bottomPadding: 10
                        Accessible.name: qsTr("Captured session output")
                        background: Rectangle { color: "transparent"; border.width: logText.activeFocus ? 2 : 0; border.color: Theme.ember; radius: 6 }
                    }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.hairline }
                Label {
                    Layout.fillWidth: true
                    Layout.margins: 12
                    Layout.leftMargin: 16
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    font.family: Theme.mono; font.pixelSize: 11; color: Theme.muted
                    text: panel.detail ? (panel.detail.capture_current ? "" : qsTr("Capture currently unavailable; counters are last observed.\n")) + qsTr("stdout %1 bytes · stderr %2 bytes · retained on disk %3 bytes\nCapture loss — queue: %4 · disk limit: %5 · disk error: %6 · memory tail: %7 · evicted logs: %8").arg(panel.count(panel.detail.logs.stdout_read)).arg(panel.count(panel.detail.logs.stderr_read)).arg(panel.count(panel.detail.logs.disk_bytes)).arg(panel.count(panel.detail.logs.disk_queue_dropped)).arg(panel.count(panel.detail.logs.disk_limit_dropped)).arg(panel.count(panel.detail.logs.disk_error_dropped)).arg(panel.count(panel.detail.logs.memory_dropped)).arg(panel.count(panel.detail.logs.evicted_logs)) : qsTr("UTF-8 display; control characters escaped. Stream labels and timestamps are capture observations.")
                }
            }
        }
        Caption { Layout.fillWidth: true; visible: panel.detail && (panel.detail.presentation_clipped || panel.detail.log_gap || panel.detail.logs.capture_unavailable || panel.detail.logs.disk_size_unknown || panel.detail.logs.errors.length > 0); color: Theme.danger; text: panel.detail ? (panel.detail.presentation_clipped ? qsTr("The display tail was clipped to 128 KiB. ") : "") + (panel.detail.log_gap ? qsTr("Earlier records are unavailable in this tail. ") : "") + (panel.detail.logs.capture_unavailable ? qsTr("Output capture is incomplete. ") : "") + (panel.detail.logs.disk_size_unknown ? qsTr("Partial disk size is unknown. ") : "") + panel.detail.logs.errors.join("\n") : "" }
        Caption { Layout.fillWidth: true; visible: panel.detail !== null; font.pixelSize: 11; color: Theme.muted; text: qsTr("UTF-8 display; control characters escaped. Stream labels and timestamps are capture observations.") }
    }
    OverlayDialog {
        id: evidenceDialog
        parent: Overlay.overlay
        width: Math.min(720, parent ? parent.width - 48 : 720)
        height: Math.min(520, parent ? parent.height - 48 : 520)
        title: qsTr("Preflight evidence")
        onOpened: evidenceBack.forceActiveFocus()
        contentItem: ColumnLayout {
            spacing: 14
            Heading { level: 2; text: evidenceDialog.title }
            Label { Layout.fillWidth: true; textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere; font.family: Theme.mono; font.pixelSize: 12; color: Theme.secondary; text: panel.detail ? qsTr("Saved launch revision %1 · %2\nExecutable: %3\nWorking directory: %4").arg(panel.detail.revision).arg(panel.detail.environment_policy).arg(panel.detail.executable).arg(panel.detail.cwd) : "" }
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentWidth: availableWidth
                ColumnLayout {
                    width: parent.width
                    spacing: 8
                    Repeater {
                        model: panel.detail && panel.detail.preflight ? panel.detail.preflight.observations : []
                        Caption { required property var modelData; Layout.fillWidth: true; text: qsTr("%1: %2 · %3 · %4\nEvidence: %5").arg(modelData.resource).arg(modelData.outcome).arg(modelData.source).arg(new Date(modelData.observed_ms).toLocaleString(Qt.locale())).arg(modelData.id) }
                    }
                    Repeater {
                        model: panel.detail && panel.detail.preflight ? panel.detail.preflight.findings : []
                        Caption { required property var modelData; Layout.fillWidth: true; color: Theme.danger; text: modelData.message + " [" + modelData.rule + "; " + modelData.evidence + "]" }
                    }
                }
            }
            ActionButton { id: evidenceBack; text: qsTr("Back to session"); Layout.alignment: Qt.AlignRight; onClicked: evidenceDialog.close() }
        }
    }
    OverlayDialog {
        id: confirmDialog
        parent: Overlay.overlay
        width: Math.min(480, parent ? parent.width - 48 : 480)
        title: panel.confirmationAction === "force" ? qsTr("Force stop this session?") : qsTr("Release an unknown reservation?")
        onOpened: confirmCancel.forceActiveFocus()
        contentItem: ColumnLayout {
            spacing: 18
            Heading { Layout.fillWidth: true; level: 2; text: confirmDialog.title }
            Caption { Layout.fillWidth: true; text: panel.confirmationAction === "force" ? qsTr("Force stop can lose unsaved game progress. Only this helper’s currently verified process scope will be signaled.") : qsTr("The game may still be running. Releasing permits another launch and conflicting writes. It does not stop any process or establish that it exited.") }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 10
                ActionButton { id: confirmCancel; text: qsTr("Cancel"); onClicked: confirmDialog.close() }
                ActionButton { danger: true; text: panel.confirmationAction === "force" ? qsTr("Force stop") : qsTr("Release reservation"); enabled: !panel.backend.action_busy && !panel.backend.launching; onClicked: { panel.backend.control(panel.confirmationSession, panel.confirmationAction, panel.confirmationGeneration); confirmDialog.close(); } }
            }
        }
    }
}
