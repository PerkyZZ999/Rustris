import QtQuick
import QtQuick.Controls.Basic
// Display text in Inter Tight (or Inter with tighter tracking): `level` 1 page
// title 34/800, 2 section 20/700, 3 card or panel 18/700, 0 hero 76/800.
Label {
    id: heading
    property int level: 2
    property int size: level === 0 ? 76 : level === 1 ? 34 : level === 2 ? 20 : 18
    textFormat: Text.PlainText
    color: Theme.ink
    font.family: Theme.display
    font.pixelSize: size
    font.weight: level <= 1 ? Font.ExtraBold : Font.Bold
    // Inter stands in for Inter Tight when it is missing; its wider spacing needs less tightening.
    font.letterSpacing: size * (level === 0 ? -0.04 : level === 1 ? -0.03 : -0.01) * (Theme.display === "Inter Tight" ? 1 : 0.35)
    wrapMode: Text.Wrap
}
