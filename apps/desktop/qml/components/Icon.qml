import QtQuick
import QtQuick.Shapes
import "Icons.js" as Icons
// Outline icon on the mockup's 24 px grid, drawn at `size` pixels.
Item {
    id: icon
    property string name: ""
    property real size: 20
    property color color: Theme.secondary
    property real strokeWidth: 1.75
    property bool filled: false
    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true
    Shape {
        anchors.centerIn: parent
        width: 24; height: 24
        scale: icon.size / 24
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: icon.filled ? "transparent" : icon.color
            strokeWidth: icon.filled ? 0 : icon.strokeWidth
            fillColor: icon.filled ? icon.color : "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: Icons.paths[icon.name] || "" }
        }
    }
}
