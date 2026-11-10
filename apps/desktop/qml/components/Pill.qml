import QtQuick
import QtQuick.Controls.Basic
// Status pill (999 px radius): neutral over artwork, or success / warning with a dot.
Rectangle {
    id: pill
    property string text
    property string tone: "neutral"
    implicitHeight: 26
    implicitWidth: row.implicitWidth + 20
    radius: height / 2
    color: tone === "success" ? "#243dd68c" : tone === "warning" ? "#29f5c451" : "#800b0b0f"
    Accessible.role: Accessible.StaticText
    Accessible.name: text
    Row {
        id: row
        anchors.centerIn: parent
        spacing: 6
        Rectangle {
            visible: pill.tone !== "neutral"
            anchors.verticalCenter: parent.verticalCenter
            width: 6; height: 6; radius: 3
            color: pill.tone === "success" ? Theme.success : Theme.warning
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: pill.text
            font.family: Theme.family
            font.pixelSize: 12
            font.weight: pill.tone === "neutral" ? Font.Normal : Font.DemiBold
            color: pill.tone === "success" ? Theme.successText : pill.tone === "warning" ? Theme.warning : Theme.soft
            Accessible.ignored: true
        }
    }
}
