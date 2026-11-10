import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Rustris
import "ScrollFocus.js" as ScrollFocus
// The mockup's Configuration page, for adding a game or editing its launch profile.
FocusScope {
    id: editor
    required property Library backend
    property var form: null
    property bool dirty: false
    property bool initializing: true
    property string gameName: ""
    property string gameId: ""
    property bool opened: false
    property int revision: 0
    signal finished()
    signal runnerSettings()
    readonly property bool creating: !(form && form.id)
    readonly property bool umu: umuChoice.checked
    readonly property string title: creating ? qsTr("Add a game") : qsTr("Configuration")
    component ScrolledField: Field {
        id: control
        onActiveFocusChanged: if (activeFocus) ScrollFocus.reveal(scroller, control)
    }
    component ScrolledAction: ActionButton {
        id: control
        onActiveFocusChanged: if (activeFocus) ScrollFocus.reveal(scroller, control)
    }
    component ScrolledBrowse: BrowseButton {
        id: control
        exactBytes: exactMode.checked
        onActiveFocusChanged: if (activeFocus) ScrollFocus.reveal(scroller, control)
        onChosen: editor.changed()
    }
    component ScrolledCheck: Check {
        id: control
        onActiveFocusChanged: if (activeFocus) ScrollFocus.reveal(scroller, control)
    }
    function open() {
        setForm();
        dirty = false;
        opened = true;
        Qt.callLater(function() { scroller.contentItem.contentY = 0; nameField.forceActiveFocus(); });
    }
    function close() {
        if (!opened) return;
        opened = false;
        finished();
    }
    function setForm() {
        initializing = true;
        form = JSON.parse(backend.draft_json);
        argumentModel.clear();
        for (let value of form.arguments) argumentModel.append({ value: value });
        environmentModel.clear();
        for (let entry of form.environment) environmentModel.append({ key: entry.key, value: entry.value === null ? "" : entry.value, unset: entry.value === null });
        nameField.text = form.name;
        executableField.text = form.executable;
        cwdField.text = form.cwd;
        prefixField.text = form.prefix;
        if (form.runner === "umu") umuChoice.checked = true; else nativeChoice.checked = true;
        Qt.callLater(function() { editor.initializing = false; });
    }
    function changed() { revision++; if (opened && !initializing) dirty = true; }
    function collect() {
        let value = Object.assign({}, form);
        value.name = nameField.text;
        value.executable = executableField.text;
        value.cwd = cwdField.text;
        value.prefix = prefixField.text;
        value.runner = umuChoice.checked ? "umu" : "native";
        value.arguments = [];
        value.environment = [];
        for (let i = 0; i < argumentModel.count; ++i) value.arguments.push(argumentModel.get(i).value);
        for (let j = 0; j < environmentModel.count; ++j) {
            let entry = environmentModel.get(j);
            value.environment.push({ key: entry.key, value: entry.unset ? null : entry.value });
        }
        return JSON.stringify(value);
    }
    function quoted(value) { return /^[A-Za-z0-9_@%+=:,.\/-]+$/.test(value) ? value : "'" + value.replace(/'/g, "'\\''") + "'"; }
    function preview() {
        let parts = [];
        for (let j = 0; j < environmentModel.count; ++j) {
            const entry = environmentModel.get(j);
            if (entry.key.length) parts.push(entry.unset ? "-u " + entry.key : entry.key + "=" + quoted(entry.value));
        }
        if (umuChoice.checked) parts.push("umu-run");
        parts.push(quoted(executableField.text.length ? executableField.text : "<executable>"));
        for (let i = 0; i < argumentModel.count; ++i) parts.push(quoted(argumentModel.get(i).value));
        return parts.join(" ");
    }
    function save() { if (backend.ready && !backend.busy) backend.save(collect()); }
    function scrollTo(section) { scroller.contentItem.contentY = Math.max(0, Math.min(section.y, scroller.contentItem.contentHeight - scroller.height)); }
    Shortcut { sequences: [StandardKey.Save]; enabled: editor.opened && editor.visible; onActivated: editor.save() }
    Connections {
        target: editor.backend
        function onDraft_jsonChanged() { if (editor.opened && editor.form) editor.setForm(); }
    }
    ListModel { id: argumentModel }
    ListModel { id: environmentModel }
    Accessible.role: Accessible.Dialog
    Accessible.name: title

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        anchors.topMargin: 24
        spacing: 0
        ActionButton {
            small: true; ghost: true
            leftPadding: 4
            iconName: "chevronLeft"
            text: editor.creating ? qsTr("Library") : editor.gameName
            Accessible.name: editor.creating ? qsTr("Back to library") : qsTr("Back to %1").arg(editor.gameName)
            enabled: !editor.backend.busy
            onClicked: editor.close()
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 8
            spacing: 16
            Rectangle {
                implicitWidth: 48; implicitHeight: 64; radius: 8
                color: Theme.raisedBase
                clip: true
                CoverArt { anchors.fill: parent; key: editor.gameId.length ? editor.gameId : editor.gameName }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                Heading { level: 1; size: 30; text: editor.title }
                Label {
                    textFormat: Text.PlainText
                    font.family: Theme.family; font.pixelSize: 13; color: Theme.secondary
                    text: editor.creating ? qsTr("A game you already have installed · saved locally")
                                          : qsTr("%1 · Local · changes apply the next time you press Play").arg(editor.gameName)
                }
            }
        }
        Panel {
            Layout.fillWidth: true
            Layout.topMargin: 16
            glass: true
            radius: 14
            implicitHeight: statusRow.implicitHeight + 20
            border.color: editor.dirty ? "#59ff7832" : Theme.line7
            RowLayout {
                id: statusRow
                anchors.fill: parent
                anchors.leftMargin: 16; anchors.rightMargin: 10
                spacing: 12
                Rectangle { width: 8; height: 8; radius: 4; color: editor.dirty ? Theme.accent : Theme.success }
                Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    font.family: Theme.family; font.pixelSize: 14
                    color: Theme.ink
                    text: editor.backend.busy ? qsTr("Saving…")
                          : editor.dirty ? qsTr("Unsaved changes") : (editor.creating ? qsTr("New launch profile") : qsTr("No unsaved changes"))
                }
                ActionButton { small: true; iconName: "undo"; text: qsTr("Cancel"); enabled: !editor.backend.busy; onClicked: editor.close() }
                ActionButton { small: true; primary: true; text: editor.backend.busy ? qsTr("Saving…") : qsTr("Save profile"); enabled: editor.backend.ready && !editor.backend.busy; onClicked: editor.save() }
            }
        }
        Label {
            Layout.fillWidth: true
            Layout.topMargin: 10
            visible: editor.backend.form_error.length > 0
            textFormat: Text.PlainText
            text: editor.backend.form_error
            color: Theme.danger
            wrapMode: Text.Wrap
            font.family: Theme.family; font.pixelSize: 14
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.topMargin: 22
            spacing: 28
            ColumnLayout {
                Layout.preferredWidth: 220
                Layout.maximumWidth: 220
                Layout.alignment: Qt.AlignTop
                spacing: 2
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Configuration sections")
                Repeater {
                    model: [{ icon: "info", label: qsTr("General") }, { icon: "wine", label: qsTr("Runner") }, { icon: "play", label: qsTr("Launch") }, { icon: "terminal", label: qsTr("Environment") }]
                    delegate: Button {
                        id: sectionLink
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: 36
                        hoverEnabled: true
                        focusPolicy: Qt.TabFocus
                        Accessible.name: modelData.label
                        onClicked: editor.scrollTo([generalSection, runnerSection, launchSection, environmentSection][index])
                        contentItem: Row {
                            spacing: 10
                            leftPadding: 12
                            Icon { anchors.verticalCenter: parent.verticalCenter; name: sectionLink.modelData.icon; size: 16; filled: sectionLink.modelData.icon === "play"; color: sectionLink.hovered ? Theme.ink : Theme.secondary }
                            Text { anchors.verticalCenter: parent.verticalCenter; text: sectionLink.modelData.label; font.family: Theme.family; font.pixelSize: 13; font.weight: Font.Medium; color: sectionLink.hovered ? Theme.ink : Theme.secondary }
                        }
                        background: Rectangle {
                            radius: 9; color: sectionLink.hovered ? "#0bffffff" : "transparent"
                            border.width: sectionLink.activeFocus && sectionLink.focusReason !== Qt.MouseFocusReason ? 2 : 0; border.color: Theme.ember
                        }
                    }
                }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.topMargin: 16
                    implicitHeight: note.implicitHeight + 28
                    radius: Theme.radiusLg
                    color: "transparent"
                    border.color: Theme.hairline
                    Column {
                        id: note
                        x: 14; y: 14
                        width: parent.width - 28
                        spacing: 4
                        Text { text: qsTr("Exact values"); font.family: Theme.family; font.pixelSize: 12; font.weight: Font.DemiBold; color: Theme.ink }
                        Text { width: parent.width; wrapMode: Text.Wrap; text: qsTr("Paths, arguments and variables are saved byte for byte. Nothing passes through a shell."); font.family: Theme.family; font.pixelSize: 12; lineHeight: 1.15; color: Theme.secondary }
                    }
                }
            }
            ScrollView {
                id: scroller
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentWidth: availableWidth
                enabled: editor.backend.ready && !editor.backend.busy
                ColumnLayout {
                    width: scroller.availableWidth - 8
                    spacing: 20
                    Section {
                        id: generalSection
                        iconName: "info"; title: qsTr("General")
                        SettingRow {
                            label: qsTr("Display name"); hint: qsTr("How this game appears in your library and search.")
                            ScrolledField { id: nameField; Layout.fillWidth: true; Accessible.name: qsTr("Game name"); placeholderText: qsTr("Name in your library"); onTextChanged: editor.changed() }
                        }
                    }
                    Section {
                        id: runnerSection
                        iconName: "wine"; title: qsTr("Runner")
                        linkText: qsTr("Runner settings")
                        onLinkActivated: editor.runnerSettings()
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.topMargin: 6
                            Layout.bottomMargin: 16
                            spacing: 10
                            Label { textFormat: Text.PlainText; text: qsTr("Profile type"); font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink }
                            Caption { Layout.fillWidth: true; text: qsTr("Linux games start directly. Windows games run through your prepared UMU and Proton runner.") }
                            ButtonGroup { id: runnerGroup; onClicked: editor.dirty = true }
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 4
                                spacing: 12
                                Accessible.role: Accessible.Grouping
                                Accessible.name: qsTr("Profile type")
                                RunnerChoice { id: nativeChoice; ButtonGroup.group: runnerGroup; text: qsTr("Linux executable"); detail: qsTr("A native Linux build, started with its saved arguments and environment."); footnote: qsTr("No compatibility layer") }
                                RunnerChoice { id: umuChoice; ButtonGroup.group: runnerGroup; text: qsTr("UMU / Proton"); detail: qsTr("A Windows build, run with the prepared UMU, Proton and runtime from Runners."); footnote: qsTr("Reviewed on first launch") }
                            }
                        }
                        SettingRow {
                            visible: editor.umu
                            label: qsTr("Wine prefix"); hint: qsTr("A private Windows folder for this game. Its saves and settings live here.")
                            ScrolledField { id: prefixField; Layout.fillWidth: true; monospace: true; Accessible.name: qsTr("Wine prefix"); placeholderText: qsTr("/absolute/path/to/prefix"); onTextChanged: editor.changed() }
                            ScrolledBrowse { target: prefixField; label: qsTr("Wine prefix"); folder: true }
                        }
                    }
                    Section {
                        id: launchSection
                        iconName: "play"; title: qsTr("Launch")
                        SettingRow {
                            label: qsTr("Executable"); hint: qsTr("The program Rustris starts.")
                            ScrolledField { id: executableField; Layout.fillWidth: true; monospace: true; Accessible.name: qsTr("Executable path"); placeholderText: qsTr("/absolute/path/to/game"); onTextChanged: editor.changed() }
                            ScrolledBrowse { target: executableField; label: qsTr("Executable path"); nameFilters: editor.umu ? [qsTr("Windows programs (*.exe *.EXE *.bat *.msi)")] : [] }
                        }
                        SettingRow {
                            label: qsTr("Working directory"); hint: qsTr("Leave empty to start in the executable’s folder.")
                            ScrolledField { id: cwdField; Layout.fillWidth: true; monospace: true; Accessible.name: qsTr("Working directory"); placeholderText: qsTr("Defaults to the executable’s folder"); onTextChanged: editor.changed() }
                            ScrolledBrowse { target: cwdField; label: qsTr("Working directory"); folder: true }
                        }
                        SettingRow {
                            label: qsTr("Exact Unix bytes"); hint: qsTr("Edit paths and values as escaped bytes. Use \\xHH for a byte and \\\\ for a literal backslash.")
                            Item { Layout.fillWidth: true }
                            ScrolledCheck {
                                id: exactMode
                                toggle: true
                                text: qsTr("Exact Unix bytes")
                                contentItem: Item {}
                                checked: editor.form ? editor.form.exact : false
                                nextCheckState: function() {
                                    editor.backend.convert(editor.collect(), !checked);
                                    editor.dirty = true;
                                    return checkState;
                                }
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 10
                            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.hairline5 }
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 6
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Label { textFormat: Text.PlainText; text: qsTr("Arguments"); font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink }
                                    Caption { Layout.fillWidth: true; text: qsTr("One token per row. Spaces stay inside a token; an empty row is an empty argument. No shell expansion.") }
                                }
                                ScrolledAction { small: true; iconName: "plus"; text: qsTr("Add argument"); onClicked: { argumentModel.append({ value: "" }); editor.dirty = true; } }
                            }
                            Repeater {
                                model: argumentModel
                                delegate: RowLayout {
                                    id: argumentRow
                                    required property int index
                                    required property string value
                                    Layout.fillWidth: true
                                    spacing: 8
                                    Label { Layout.preferredWidth: 28; textFormat: Text.PlainText; text: String(argumentRow.index + 1); horizontalAlignment: Text.AlignRight; font.family: Theme.mono; font.pixelSize: 12; color: Theme.muted }
                                    ScrolledField { Layout.fillWidth: true; monospace: true; text: argumentRow.value; Accessible.name: qsTr("Argument %1").arg(argumentRow.index + 1); onTextChanged: { if (argumentRow.value !== text) argumentModel.setProperty(argumentRow.index, "value", text); editor.changed(); } }
                                    ScrolledAction { small: true; ghost: true; iconName: "trash"; text: ""; implicitWidth: 32; Accessible.name: qsTr("Remove argument %1").arg(argumentRow.index + 1); ToolTip.visible: hovered; ToolTip.text: qsTr("Remove"); onClicked: { argumentModel.remove(argumentRow.index); editor.dirty = true; } }
                                }
                            }
                            SectionLabel { Layout.topMargin: 10; text: qsTr("Command preview") }
                            Rectangle {
                                Layout.fillWidth: true
                                Layout.bottomMargin: 16
                                implicitHeight: previewText.implicitHeight + 24
                                radius: Theme.radiusMd
                                color: "#e6060609"
                                border.color: Theme.line8
                                Label {
                                    id: previewText
                                    x: 14; y: 12
                                    width: parent.width - 28
                                    textFormat: Text.PlainText
                                    wrapMode: Text.WrapAnywhere
                                    font.family: Theme.mono; font.pixelSize: 12
                                    color: Theme.ember
                                    // Re-evaluated whenever the inputs it reads change.
                                    text: { editor.revision; editor.opened; umuChoice.checked; argumentModel.count; environmentModel.count; return editor.preview(); }
                                    Accessible.name: qsTr("Command preview: %1").arg(text)
                                }
                            }
                        }
                    }
                    Section {
                        id: environmentSection
                        iconName: "terminal"; title: qsTr("Environment")
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.bottomMargin: 16
                            spacing: 10
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 6
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Label { textFormat: Text.PlainText; text: qsTr("Environment overrides"); font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink }
                                    Caption { Layout.fillWidth: true; text: qsTr("Set or unset variables for this game only. Secret-looking names are left out of saved launch snapshots.") }
                                }
                                ScrolledAction { small: true; iconName: "plus"; text: qsTr("Add variable"); onClicked: { environmentModel.append({ key: "", value: "", unset: false }); editor.dirty = true; } }
                            }
                            Repeater {
                                model: environmentModel
                                delegate: RowLayout {
                                    id: envRow
                                    required property int index
                                    required property string key
                                    required property string value
                                    required property bool unset
                                    Layout.fillWidth: true
                                    spacing: 8
                                    ScrolledField { Layout.preferredWidth: 220; monospace: true; text: envRow.key; placeholderText: qsTr("KEY"); Accessible.name: qsTr("Environment key %1").arg(envRow.index + 1); onTextChanged: { if (envRow.key !== text) environmentModel.setProperty(envRow.index, "key", text); editor.changed(); } }
                                    ScrolledField { Layout.fillWidth: true; monospace: true; text: envRow.value; enabled: !envRow.unset; placeholderText: qsTr("Value"); Accessible.name: qsTr("Environment value %1").arg(envRow.index + 1); onTextChanged: { if (envRow.value !== text) environmentModel.setProperty(envRow.index, "value", text); editor.changed(); } }
                                    ScrolledCheck { text: qsTr("Unset"); checked: envRow.unset; Accessible.name: qsTr("Unset variable %1").arg(envRow.index + 1); onClicked: { environmentModel.setProperty(envRow.index, "unset", checked); editor.changed(); } }
                                    ScrolledAction { small: true; ghost: true; iconName: "trash"; text: ""; implicitWidth: 32; Accessible.name: qsTr("Remove variable %1").arg(envRow.index + 1); ToolTip.visible: hovered; ToolTip.text: qsTr("Remove"); onClicked: { environmentModel.remove(envRow.index); editor.dirty = true; } }
                                }
                            }
                            Caption { visible: environmentModel.count === 0; text: qsTr("No overrides. The game inherits Rustris’s allowlisted environment.") }
                        }
                    }
                    Caption { Layout.fillWidth: true; Layout.bottomMargin: 40; color: Theme.muted; text: qsTr("Profiles are saved locally. UMU profiles require prepared runner settings and a launch review.") }
                }
            }
        }
    }
    component Section: Panel {
        id: section
        property string iconName
        property string title
        property string linkText: ""
        signal linkActivated()
        default property alias rows: body.data
        Layout.fillWidth: true
        implicitHeight: sectionColumn.implicitHeight + 24
        opaque: editor.backend.opaque
        ColumnLayout {
            id: sectionColumn
            anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 24; rightMargin: 24; topMargin: 18 }
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                Layout.bottomMargin: 6
                spacing: 12
                Rectangle {
                    implicitWidth: 34; implicitHeight: 34; radius: 10
                    color: "#1aff681f"
                    Icon { anchors.centerIn: parent; name: section.iconName; size: 18; filled: section.iconName === "play"; color: Theme.accentLink }
                }
                Heading { Layout.fillWidth: true; level: 3; text: section.title }
                Button {
                    id: link
                    visible: section.linkText.length > 0
                    text: section.linkText
                    hoverEnabled: true
                    onClicked: section.linkActivated()
                    contentItem: Text { text: link.text; font.family: Theme.family; font.pixelSize: 13; font.weight: Font.DemiBold; color: link.hovered ? Theme.ember : Theme.accentLink }
                    background: Rectangle { color: "transparent"; radius: 6; border.width: link.activeFocus && link.focusReason !== Qt.MouseFocusReason ? 2 : 0; border.color: Theme.ember }
                }
            }
            ColumnLayout { id: body; Layout.fillWidth: true; spacing: 0 }
        }
    }
    component SettingRow: Item {
        id: row
        property string label
        property string hint
        default property alias control: holder.data
        Layout.fillWidth: true
        implicitHeight: Math.max(texts.implicitHeight, holder.implicitHeight) + 32
        Rectangle { width: parent.width; height: 1; color: Theme.hairline5 }
        ColumnLayout {
            id: texts
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width * 0.42
            spacing: 4
            Label { Layout.fillWidth: true; textFormat: Text.PlainText; text: row.label; wrapMode: Text.Wrap; font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink }
            Caption { Layout.fillWidth: true; text: row.hint }
        }
        RowLayout {
            id: holder
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width * 0.54
        }
    }
    component RunnerChoice: RadioButton {
        id: choice
        property string detail
        property string footnote
        Layout.fillWidth: true
        Layout.preferredWidth: 200
        hoverEnabled: true
        padding: 16
        onActiveFocusChanged: if (activeFocus) ScrollFocus.reveal(scroller, choice)
        indicator: Item {}
        contentItem: Column {
            spacing: 6
            Text { text: choice.text; font.family: Theme.family; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.ink }
            Text { width: parent.width; text: choice.detail; wrapMode: Text.Wrap; font.family: Theme.family; font.pixelSize: 12; lineHeight: 1.15; color: Theme.secondary }
            Text { width: parent.width; wrapMode: Text.Wrap; text: choice.footnote; font.family: Theme.mono; font.pixelSize: 11; color: choice.checked ? Theme.ember : Theme.muted }
        }
        background: Rectangle {
            radius: 14
            color: choice.checked ? "#14ff681f" : "#08ffffff"
            border.width: 1
            border.color: choice.checked ? "#b3ff7832" : (choice.hovered ? Theme.line16 : Theme.line8)
            Rectangle {
                anchors.fill: parent; anchors.margins: -4; radius: 18
                color: "transparent"; border.width: 2; border.color: Theme.ember
                visible: choice.activeFocus && choice.focusReason !== Qt.MouseFocusReason
            }
        }
    }
}
