import QtQuick
import QtQuick.Dialogs
import QtCore
// "Browse…" beside a path field: opens the platform file or folder dialog (the
// KDE dialog on Plasma, where Ctrl+H shows hidden folders such as ~/.local) and
// writes the chosen absolute path into `target`. The dialog is created only
// while it is open, and the field stays editable for paths a dialog cannot express.
ActionButton {
    id: browse
    property Item target: null
    property string label: ""
    property bool folder: false
    property bool exactBytes: false
    property var nameFilters: []
    signal chosen(string path)
    small: true
    iconName: "folder"
    text: qsTr("Browse…")
    Accessible.name: qsTr("Browse for %1").arg(label)
    Accessible.description: folder ? qsTr("Choose a folder") : qsTr("Choose a file")
    onClicked: dialogLoader.active = true

    function localPath(url) {
        const value = url.toString();
        if (value.indexOf("file://") !== 0) return "";
        try { return decodeURIComponent(value.slice(7)); } catch (error) { return ""; }
    }
    function startFolder() {
        const current = target ? target.text : "";
        if (!exactBytes && current.indexOf("/") === 0) {
            const parent = folder ? current : current.slice(0, Math.max(1, current.lastIndexOf("/")));
            // Encode each segment so '#', '?' and '%' in folder names stay part of the path.
            return "file://" + parent.split("/").map(encodeURIComponent).join("/");
        }
        return StandardPaths.writableLocation(StandardPaths.HomeLocation);
    }
    function accept(url) {
        dialogLoader.active = false;
        const path = localPath(url);
        if (!path.length) return;
        // Exact byte mode reads "\\" as one backslash; nothing else needs escaping.
        const text = exactBytes ? path.replace(/\\/g, "\\\\") : path;
        if (target) { target.text = text; target.forceActiveFocus(); }
        chosen(text);
    }

    Loader {
        id: dialogLoader
        active: false
        sourceComponent: browse.folder ? folderDialog : fileDialog
        onLoaded: item.open()
    }
    Component {
        id: fileDialog
        FileDialog {
            title: qsTr("Choose %1").arg(browse.label)
            fileMode: FileDialog.OpenFile
            currentFolder: browse.startFolder()
            nameFilters: browse.nameFilters.length ? browse.nameFilters.concat([qsTr("All files (*)")]) : []
            onAccepted: browse.accept(selectedFile)
            onRejected: dialogLoader.active = false
        }
    }
    Component {
        id: folderDialog
        FolderDialog {
            title: qsTr("Choose %1").arg(browse.label)
            currentFolder: browse.startFolder()
            onAccepted: browse.accept(selectedFolder)
            onRejected: dialogLoader.active = false
        }
    }
}
