.pragma library
// Readable session states shared by the detail page and logs.
function label(value) {
    var labels = {
        preparing: qsTr("Preparing"), running: qsTr("Process scope observed"), stop_requested: qsTr("Stop requested"),
        force_requested: qsTr("Force requested"), finalizing: qsTr("Finishing output capture"), exited: qsTr("Exited"),
        stopped: qsTr("Stopped"), cancelled: qsTr("Cancelled before spawn"), launch_failed: qsTr("Launch failed"),
        unknown: qsTr("Tracking unknown — reserved"), released_unknown: qsTr("Unknown reservation released")
    };
    return labels[value] || value;
}
