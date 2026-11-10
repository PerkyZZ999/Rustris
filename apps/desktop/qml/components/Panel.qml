import QtQuick
// L1 pane (rgba(18,18,24,.72), white 6% border, 16 px radius); `glass` gives the
// L2 frosted gradient with its inset highlight. Opaque mode uses the solid pane.
Rectangle {
    id: panel
    property bool glass: false
    property bool opaque: false
    radius: Theme.radiusXl
    color: opaque ? Theme.surface : Theme.pane
    border.color: glass ? Theme.line7 : Theme.hairline
    gradient: glass && !opaque ? glassGradient : null
    Gradient {
        id: glassGradient
        GradientStop { position: 0; color: Theme.glassTop }
        GradientStop { position: 1; color: Theme.glassBottom }
    }
    Rectangle {
        visible: panel.glass
        anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: panel.radius; rightMargin: panel.radius; topMargin: 1 }
        height: 1
        color: "#0fffffff"
    }
}
