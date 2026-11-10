import QtQuick
import QtQuick.Controls.Basic
// Label token: Inter 11 / 600 / +10% caps, muted.
Label {
    textFormat: Text.PlainText
    font.family: Theme.family
    font.pixelSize: 11
    font.weight: Font.DemiBold
    font.letterSpacing: 1.1
    font.capitalization: Font.AllUppercase
    color: Theme.muted
}
