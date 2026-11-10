import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import "ScrollFocus.js" as ScrollFocus
// Runners page: the prepared UMU / Proton / runtime selection used by UMU profiles.
FocusScope {
    id: dialog
    required property var backend
    property bool dirty: false
    property bool initializing: false
    property bool exact: false
    property bool opened: false
    property bool opaque: false
    property bool advanced: false
    // D-012: the UMU launchers, Proton builds and runtimes found on this computer.
    readonly property var detected: {
        try { return JSON.parse(backend.detected_runners_json || "{}"); } catch (error) { return {}; }
    }
    readonly property var protons: detected.proton || []
    readonly property var runtimes: detected.runtimes || []
    readonly property var launchers: detected.umu || []
    signal closed()
    function runtimeFor(needs) {
        for (let i = 0; i < runtimes.length; ++i) if (runtimes[i].directory === needs) return runtimes[i];
        return null;
    }
    function choose(build) {
        const prepared = runtimeFor(build.needs);
        if (!prepared) return;
        if (launchers.length && (!umu.text.length || dialog.exact)) umu.text = launchers[0].path;
        proton.text = build.path;
        runtime.text = prepared.path;
    }
    // Show the manual fields when nothing was found or the saved choice is not in the list.
    function chooseView() {
        let listed = false;
        for (let i = 0; i < protons.length; ++i) if (protons[i].path === proton.text) listed = true;
        advanced = exact || protons.length === 0 || (proton.text.length > 0 && !listed);
    }
    readonly property string title: qsTr("Runners")
    function collect() { return JSON.stringify({umu: umu.text, proton: proton.text, runtime: runtime.text, exact: exact}); }
    function applyDraft() {
        initializing = true;
        const draft = JSON.parse(backend.runner_draft_json);
        umu.text = draft.umu; proton.text = draft.proton; runtime.text = draft.runtime; exact = draft.exact;
        initializing = false;
    }
    function changed() { if (!initializing && opened) dirty = true; }
    function open() {
        applyDraft(); chooseView(); dirty = false; opened = true;
        Qt.callLater(function() { if (advanced || !protonList.count) umu.focusInput(); else protonList.focusFirst(); });
    }
    function close() { if (!opened) return; opened = false; closed(); }
    Connections {
        target: dialog.backend
        function onRunner_draft_jsonChanged() { if (dialog.opened) dialog.applyDraft(); }
        function onRunner_saved() { dialog.dirty = false; dialog.close(); }
    }
    Accessible.role: Accessible.Dialog
    Accessible.name: qsTr("Runner settings")
    // Keep keyboard focus visible: scroll whichever control inside this page gains it.
    Connections {
        target: dialog.Window.window
        function onActiveFocusItemChanged() {
            const item = dialog.Window.activeFocusItem;
            if (!item || !dialog.visible) return;
            for (let p = item.parent; p; p = p.parent)
                if (p === runnerScroller) { ScrollFocus.reveal(runnerScroller, item); return; }
        }
    }
    ScrollView {
        id: runnerScroller
        anchors.fill: parent
        clip: true
        contentWidth: availableWidth
        ColumnLayout {
            width: parent.width
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 32; Layout.rightMargin: 32; Layout.topMargin: 28
                spacing: 16
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    Heading { level: 1; text: dialog.title }
                    Label {
                        Layout.fillWidth: true
                        Layout.maximumWidth: 640
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        font.family: Theme.family; font.pixelSize: 14; lineHeight: 1.15; color: Theme.secondary
                        text: qsTr("The compatibility layer UMU profiles use. Choose existing, prepared components; Rustris does not install or update them.")
                    }
                }
                ActionButton { Layout.alignment: Qt.AlignBottom; text: qsTr("Cancel"); enabled: !dialog.backend.action_busy; onClicked: dialog.close() }
                ActionButton { Layout.alignment: Qt.AlignBottom; text: qsTr("Save runner settings"); primary: true; enabled: !dialog.backend.action_busy; onClicked: dialog.backend.save_runner_settings(dialog.collect()) }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 32; Layout.rightMargin: 32; Layout.topMargin: 24; Layout.bottomMargin: 40
                spacing: 24
                Panel {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignTop
                    glass: true
                    opaque: dialog.opaque
                    implicitHeight: form.implicitHeight + 44
                    ColumnLayout {
                        id: form
                        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 22 }
                        spacing: 0
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            Rectangle {
                                implicitWidth: 34; implicitHeight: 34; radius: 10
                                color: "#1aff681f"
                                Text { anchors.centerIn: parent; text: "UM"; font.family: Theme.mono; font.pixelSize: 11; font.weight: Font.DemiBold; color: Theme.accentLink }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Heading { level: 3; text: qsTr("Prepared UMU runner") }
                                Caption { text: qsTr("Used by every UMU / Proton profile · checked again before each launch") }
                            }
                        }
                        SectionLabel { Layout.topMargin: 20; text: qsTr("Installed Proton builds") }
                        Caption {
                            Layout.fillWidth: true; Layout.topMargin: 6
                            text: dialog.launchers.length
                                  ? qsTr("Choose a build; Rustris fills in the Proton folder and the runtime it needs, and uses the UMU launcher at %1. Validated builds are the ones Rustris was tested with.").arg(umu.text.length ? umu.text : dialog.launchers[0].path)
                                  : qsTr("No umu-run launcher was found. Install umu-launcher, or enter its path under Advanced settings.")
                        }
                        Caption {
                            Layout.fillWidth: true; Layout.topMargin: 6
                            visible: dialog.protons.length === 0
                            text: qsTr("No Proton builds were found in Steam's compatibilitytools.d or steamapps folders. Enter the paths under Advanced settings.")
                        }
                        ColumnLayout {
                            id: protonList
                            readonly property int count: dialog.protons.length
                            // Rows whose runtime is missing are disabled; focus the first usable one.
                            function focusFirst() {
                                for (let i = 0; i < protonRows.count; ++i) {
                                    const row = protonRows.itemAt(i);
                                    if (row && row.enabled) { row.forceActiveFocus(); return; }
                                }
                                advancedToggle.forceActiveFocus();
                            }
                            Layout.fillWidth: true; Layout.topMargin: 12
                            spacing: 8
                            Accessible.role: Accessible.List
                            Accessible.name: qsTr("Installed Proton builds")
                            Repeater {
                                id: protonRows
                                model: dialog.protons
                                delegate: RadioButton {
                                    id: buildRow
                                    required property var modelData
                                    readonly property var prepared: dialog.runtimeFor(modelData.needs)
                                    Layout.fillWidth: true
                                    hoverEnabled: true
                                    padding: 14
                                    enabled: prepared !== null && !dialog.exact
                                    checked: proton.text === modelData.path
                                    text: modelData.label
                                    Accessible.name: modelData.label
                                    Accessible.description: (modelData.validated && prepared && prepared.validated ? qsTr("Validated") : qsTr("Not validated"))
                                                            + ". " + (prepared ? qsTr("Uses %1").arg(prepared.label) : qsTr("Needs %1, which is not prepared").arg(modelData.needs_label || qsTr("an unknown runtime")))
                                    onClicked: dialog.choose(modelData)
                                    indicator: Item {}
                                    contentItem: RowLayout {
                                        spacing: 12
                                        ColumnLayout {
                                            Layout.fillWidth: true
                                            spacing: 4
                                            Text { text: buildRow.modelData.label; font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: buildRow.enabled ? Theme.ink : Theme.muted }
                                            Text {
                                                Layout.fillWidth: true; elide: Text.ElideMiddle
                                                text: buildRow.prepared ? qsTr("Runtime: %1 %2").arg(buildRow.prepared.label).arg(buildRow.prepared.version)
                                                                        : qsTr("Needs %1, which is not prepared on this computer").arg(buildRow.modelData.needs_label || qsTr("an unknown runtime"))
                                                font.family: Theme.family; font.pixelSize: 12; color: Theme.secondary
                                            }
                                            Text { Layout.fillWidth: true; elide: Text.ElideMiddle; text: buildRow.modelData.path; font.family: Theme.mono; font.pixelSize: 11; color: Theme.muted }
                                        }
                                        Pill {
                                            tone: buildRow.modelData.validated && buildRow.prepared && buildRow.prepared.validated ? "success" : "warning"
                                            text: buildRow.modelData.validated && buildRow.prepared && buildRow.prepared.validated ? qsTr("Validated") : qsTr("Not validated")
                                        }
                                    }
                                    background: Rectangle {
                                        radius: 14
                                        color: buildRow.checked ? "#14ff681f" : "#08ffffff"
                                        border.width: 1
                                        border.color: buildRow.checked ? "#b3ff7832" : (buildRow.hovered ? Theme.line16 : Theme.line8)
                                        Rectangle {
                                            anchors.fill: parent; anchors.margins: -4; radius: 18
                                            color: "transparent"; border.width: 2; border.color: Theme.ember
                                            visible: buildRow.activeFocus && buildRow.focusReason !== Qt.MouseFocusReason
                                        }
                                    }
                                }
                            }
                        }
                        Check {
                            Layout.fillWidth: true
                            Layout.topMargin: 16
                            toggle: true
                            id: advancedToggle
                            text: qsTr("Advanced settings")
                            description: qsTr("Edit the UMU, Proton and runtime paths directly.")
                            checked: dialog.advanced
                            nextCheckState: function() { dialog.advanced = !dialog.advanced; return checkState; }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: dialog.advanced
                            spacing: 0
                        RunnerField { id: umu; folder: false; label: qsTr("UMU executable"); hint: qsTr("The umu-run launcher.") }
                        RunnerField { id: proton; label: qsTr("Proton directory"); hint: qsTr("A prepared Proton build, for example UMU-Proton or GE-Proton.") }
                        RunnerField { id: runtime; label: qsTr("Prepared runtime directory"); hint: qsTr("The Steam Linux Runtime the Proton build expects.") }
                        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.hairline5 }
                        Check {
                            Layout.fillWidth: true
                            Layout.topMargin: 12
                            toggle: true
                            text: qsTr("Edit exact Unix bytes")
                            description: qsTr("Exact byte mode uses \\xNN escapes for non-text bytes and \\\\ for a literal backslash.")
                            checked: dialog.exact
                            nextCheckState: function() { dialog.backend.convert_runner_settings(dialog.collect(), !dialog.exact); dialog.changed(); return checkState; }
                        }
                        }
                        Caption { Layout.fillWidth: true; Layout.topMargin: 10; visible: dialog.backend.error.length > 0; color: Theme.danger; text: dialog.backend.error; Accessible.role: Accessible.AlertMessage }
                    }
                }
                Panel {
                    Layout.preferredWidth: 300
                    Layout.alignment: Qt.AlignTop
                    opaque: dialog.opaque
                    implicitHeight: words.implicitHeight + 40
                    ColumnLayout {
                        id: words
                        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
                        spacing: 14
                        SectionLabel { text: qsTr("In plain words") }
                        Explainer { mono: "UM"; title: qsTr("UMU"); body: qsTr("Starts Windows games the way Steam would, inside a container, without Steam running.") }
                        Explainer { mono: "PR"; title: qsTr("Proton"); body: qsTr("Wine plus extras tuned for games. It translates Windows calls for Linux.") }
                        Explainer { mono: "RT"; title: qsTr("Runtime"); body: qsTr("A known set of libraries, so the game sees the same system on every computer.") }
                        Explainer { mono: "LX"; title: qsTr("Linux executable"); body: qsTr("Native games need none of this and start directly.") }
                    }
                }
            }
        }
    }
    component RunnerField: ColumnLayout {
        id: field
        property string label
        property string hint
        property bool folder: true
        property alias text: input.text
        function focusInput() { input.forceActiveFocus(); }
        Layout.fillWidth: true
        spacing: 6
        Rectangle { Layout.fillWidth: true; Layout.topMargin: 16; implicitHeight: 1; color: Theme.hairline5 }
        Label { Layout.topMargin: 12; textFormat: Text.PlainText; text: field.label; font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink }
        Caption { Layout.fillWidth: true; text: field.hint }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 4
            spacing: 8
            Field { id: input; Layout.fillWidth: true; monospace: true; Accessible.name: field.label; onTextChanged: dialog.changed() }
            BrowseButton { target: input; label: field.label; folder: field.folder; exactBytes: dialog.exact }
        }
    }
    component Explainer: RowLayout {
        id: item
        property string mono
        property string title
        property string body
        Layout.fillWidth: true
        spacing: 10
        Rectangle {
            Layout.alignment: Qt.AlignTop
            implicitWidth: 22; implicitHeight: 22; radius: 6
            color: Theme.fill6
            Text { anchors.centerIn: parent; text: item.mono; font.family: Theme.mono; font.pixelSize: 9; font.weight: Font.DemiBold; color: Theme.accentLink }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            Text { text: item.title; font.family: Theme.family; font.pixelSize: 13; font.weight: Font.DemiBold; color: Theme.ink }
            Text { Layout.fillWidth: true; text: item.body; wrapMode: Text.Wrap; font.family: Theme.family; font.pixelSize: 12; lineHeight: 1.15; color: Theme.secondary }
        }
    }
}
