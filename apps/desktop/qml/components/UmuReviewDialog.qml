import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
// First-launch review for a UMU profile, as an L3 overlay.
OverlayDialog {
    id: dialog
    required property var backend
    property var review: null
    property bool confirmed: false
    signal launchRequested()
    width: Math.min(880, parent.width - 48)
    height: Math.min(760, parent.height - 48)
    title: qsTr("Review UMU launch")
    onOpened: reviewCancel.forceActiveFocus()
    onClosed: { if (!confirmed && review) backend.discard_umu(review.ticket); review = null; }
    contentItem: ColumnLayout {
        Accessible.role: Accessible.Dialog
        Accessible.name: dialog.title
        spacing: 14
        Heading { level: 2; size: 24; text: dialog.title }
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: warning.implicitHeight + 24
            radius: Theme.radiusLg
            color: "#14f5c451"
            border.color: "#40f5c451"
            RowLayout {
                id: warning
                anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: 14; rightMargin: 14 }
                spacing: 10
                Icon { Layout.alignment: Qt.AlignTop; name: "alert"; size: 16; strokeWidth: 2; color: Theme.warning }
                Caption { Layout.fillWidth: true; color: Theme.soft; text: qsTr("Launching may change game files, prefix settings and saves. Runner updates and Proton fixes are disabled; upstream metadata may contact the network. Windows application console output is unverified.") }
            }
        }
        ScrollView {
            Layout.fillWidth: true; Layout.fillHeight: true; clip: true; contentWidth: availableWidth
            ColumnLayout {
                width: parent.width; spacing: 10
                SectionLabel { text: qsTr("Game") }
                Label { Layout.fillWidth: true; textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere; font.family: Theme.mono; font.pixelSize: 12; color: Theme.soft; text: dialog.review ? qsTr("Game: %1\nExecutable: %2\nWorking directory: %3\nSelected prefix: %4\nEffective prefix: %5").arg(dialog.review.name).arg(dialog.review.game_executable).arg(dialog.review.cwd).arg(dialog.review.prefix).arg(dialog.review.effective_prefix || qsTr("unavailable")) : "" }
                SectionLabel { Layout.topMargin: 6; text: qsTr("Runner") }
                // D-012: untested combinations are allowed but called out.
                Caption {
                    Layout.fillWidth: true
                    visible: dialog.review !== null && dialog.review.ready
                    color: dialog.review && dialog.review.validated ? Theme.success : Theme.warning
                    text: dialog.review && dialog.review.validated
                          ? qsTr("Validated: Rustris was tested with this UMU, Proton and runtime set.")
                          : qsTr("Not validated: Rustris has not been tested with this UMU, Proton and runtime set. It may still work; if the game misbehaves, try a validated set before changing the prefix.")
                }
                Label { Layout.fillWidth: true; textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere; font.family: Theme.mono; font.pixelSize: 12; color: Theme.soft; text: dialog.review ? qsTr("UMU: %1 (%2)\nProton: %3 (%4)\nRuntime: %5 (%6)").arg(dialog.review.tool).arg(dialog.review.umu_version || qsTr("unavailable")).arg(dialog.review.proton).arg(dialog.review.proton_version || qsTr("unavailable")).arg(dialog.review.runtime).arg(dialog.review.runtime_version || qsTr("unavailable")) : "" }
                Label { Layout.topMargin: 6; textFormat: Text.PlainText; text: qsTr("Arguments"); color: Theme.ink; font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold }
                Repeater { model: dialog.review ? dialog.review.arguments : []; Label { required property string modelData; Layout.fillWidth: true; textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere; font.family: Theme.mono; font.pixelSize: 12; color: Theme.soft; text: modelData } }
                Label { Layout.topMargin: 6; textFormat: Text.PlainText; text: qsTr("Effective environment"); color: Theme.ink; font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold }
                Repeater { model: dialog.review ? dialog.review.environment : []; Label { required property var modelData; Layout.fillWidth: true; textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere; font.family: Theme.mono; font.pixelSize: 12; color: Theme.soft; text: modelData[0] + "=" + modelData[1] } }
                Label { Layout.topMargin: 6; textFormat: Text.PlainText; text: qsTr("Preflight evidence"); color: Theme.ink; font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold }
                Repeater {
                    model: dialog.review ? dialog.review.preflight.observations : []
                    Caption { required property var modelData; Layout.fillWidth: true; text: qsTr("%1: %2 · %3 · %4\nEvidence: %5").arg(modelData.resource).arg(modelData.outcome).arg(modelData.source).arg(new Date(modelData.observed_ms).toLocaleString(Qt.locale())).arg(modelData.id) }
                }
                Repeater {
                    model: dialog.review ? dialog.review.preflight.findings : []
                    Caption { required property var modelData; Layout.fillWidth: true; color: Theme.danger; text: modelData.message + " [" + modelData.rule + "; " + modelData.evidence + "]" }
                }
            }
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: 10
            ActionButton { id: reviewCancel; text: qsTr("Cancel"); onClicked: dialog.close() }
            ActionButton { text: qsTr("Launch game"); iconName: "play"; primary: true; enabled: dialog.review && dialog.review.ready && !dialog.backend.action_busy; onClicked: { dialog.confirmed = true; dialog.backend.confirm_umu(dialog.review.ticket); dialog.close(); dialog.launchRequested(); } }
        }
    }
}
