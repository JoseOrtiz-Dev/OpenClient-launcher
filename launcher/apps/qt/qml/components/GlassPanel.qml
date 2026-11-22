import QtQuick
import OpenClient

Rectangle {
    id: root
    default property alias content: interior.data
    property int padding: 20
    property bool accented: false
    property bool frosted: accented
    // Solo con el estilo Cristal: brillo de cursor y borde de hover. Apagado por defecto para no crear spotlights en listas.
    property bool hoverable: false
    radius: Theme.crystal || accented ? Theme.radiusLg : Theme.radiusMd
    antialiasing: true
    color: Theme.crystal ? "transparent" : root.accented ? Theme.glassElevated : Theme.glass
    border.color: Theme.crystal ? "transparent" : root.accented ? Theme.edge : Theme.rim
    border.width: 1

    // Cristal: el material de GlassCard (relleno, grano, destello, borde) como fondo.
    Loader {
        z: -1
        anchors.fill: parent
        active: Theme.crystal
        sourceComponent: GlassCard {
            hoverable: root.hoverable
            accented: root.accented
            radius: root.radius
        }
    }

    FrostedBackdrop {
        anchors.fill: parent
        anchors.margins: 1
        visible: !Theme.crystal
        source: root.frosted && !Theme.crystal ? Theme.backdrop : null
        live: launcher.particlesEnabled
        radius: Math.max(0, root.radius - 1)
        strength: 0.78
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: 1
        visible: !Theme.crystal
        radius: Math.max(0, root.radius - 1)
        gradient: Gradient {
            GradientStop {
                position: 0
                color: root.accented
                    ? Qt.tint(Theme.surface, Qt.alpha(Theme.accent, Theme.light ? 0.05 : 0.085))
                    : Qt.tint(root.color, Qt.alpha(Theme.ink, Theme.light ? 0.015 : 0.035))
            }
            GradientStop {
                position: 1
                color: root.accented ? Qt.alpha(Theme.base, Theme.light ? 0.92 : 0.94) : root.color
            }
        }
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: 1
        visible: !Theme.crystal
        radius: Math.max(0, root.radius - 1)
        color: "transparent"
        border.color: Theme.rim
        opacity: 0.5
    }

    Rectangle {
        visible: !Theme.crystal
        anchors.top: parent.top
        anchors.topMargin: 1
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.max(0, parent.width - root.radius * 2)
        height: 1
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 0.5; color: Theme.specular }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    Item { id: interior; anchors.fill: parent; anchors.margins: root.padding }
}
