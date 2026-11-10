import QtQuick
import QtQuick.Shapes
import "Covers.js" as Covers
// Local artwork when it loads; otherwise the mockup's generated landscape,
// chosen from the game id so each game keeps its own placeholder.
Item {
    id: cover
    property string key: ""
    property url source: ""
    property bool dim: false
    property string imageName: ""
    property size sourceSize: Qt.size(288, 432)
    readonly property bool loaded: image.status === Image.Ready
    readonly property alias imageStatus: image.status
    readonly property var colors: Covers.pick(key)
    clip: true
    Accessible.ignored: true
    Item {
        anchors.fill: parent
        visible: !cover.loaded
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: cover.colors.top }
                GradientStop { position: 1; color: cover.colors.bottom }
            }
        }
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            asynchronous: true  // triangulate off the GUI thread
            ShapePath {
                strokeColor: "transparent"
                fillGradient: RadialGradient {
                    centerX: cover.width * cover.colors.gx; centerY: cover.height * cover.colors.gy
                    focalX: centerX; focalY: centerY
                    centerRadius: Math.max(cover.width, cover.height) * 0.55
                    GradientStop { position: 0; color: cover.colors.glow }
                    GradientStop { position: 0.7; color: "transparent" }
                }
                startX: 0; startY: 0
                PathLine { x: cover.width; y: 0 }
                PathLine { x: cover.width; y: cover.height }
                PathLine { x: 0; y: cover.height }
                PathLine { x: 0; y: 0 }
            }
        }
        Shape {
            width: 100; height: 100
            transform: Scale { xScale: cover.width / 100; yScale: cover.height / 100 }
            preferredRendererType: Shape.CurveRenderer
            asynchronous: true  // triangulate off the GUI thread
            ShapePath { strokeColor: "transparent"; fillColor: cover.colors.far; PathSvg { path: Covers.shapes[cover.colors.shape][0] } }
            ShapePath { strokeColor: "transparent"; fillColor: "#0dffffff"; PathSvg { path: Covers.mist } }
            ShapePath { strokeColor: "transparent"; fillColor: cover.colors.near; PathSvg { path: Covers.shapes[cover.colors.shape][1] } }
        }
    }
    Image {
        id: image
        objectName: cover.imageName
        anchors.fill: parent
        source: cover.source
        asynchronous: true
        cache: false
        sourceSize: cover.sourceSize
        fillMode: Image.PreserveAspectCrop
        visible: status === Image.Ready
    }
    Rectangle {
        anchors.fill: parent
        visible: cover.dim
        color: "#66000000"
    }
}
