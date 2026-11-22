import QtQuick
import QtQuick.Shapes
import OpenClient

// Fondo ambiental: cuatro resplandores suaves que derivan despacio (referencia «Cristal opaco»).
// Sin desenfoque: el degradado radial ya es suave. Solo se anima Translate y scale.
Item {
    id: root
    anchors.fill: parent
    clip: true
    // Main lo pone a false con la ventana minimizada o con diálogos que tapan el fondo.
    property bool active: true
    readonly property bool running: Theme.motion && visible && active
    // Los tamaños de la referencia son para ~1280×800; se escalan con la ventana.
    readonly property real u: Math.max(0.7, Math.min(1.4, Math.min(width / 1280, height / 800)))

    // Color del banner de la instancia (Inicio): si no es transparente, los dos orbes grandes lo toman. Cambia con fundido.
    property color tint: "transparent"
    readonly property bool tinted: tint.a > 0
    property color primaryTone: tinted ? Qt.alpha(tint, 0.34) : Theme.orbPrimary
    property color secondaryTone: tinted ? Qt.alpha(Qt.tint(tint, Qt.alpha("white", 0.22)), 0.2) : Theme.orbSecondary
    Behavior on primaryTone { MotionColor { role: "slow" } }
    Behavior on secondaryTone { MotionColor { role: "slow" } }

    component Orb: Item {
        id: orb
        required property color tint
        required property real diameter       // px a 1280×800
        property real dx: 0                   // deriva máxima a 1280×800
        property real dy: 0
        property real grow: 0.08              // escala final = 1 + grow
        property int legMs: 18000
        required property real unit           // factor de escala por tamaño de ventana
        required property bool drifting
        property bool fromEnd: false          // alternate-reverse: empieza en el extremo
        property real t: fromEnd ? 1 : 0      // 0..1, solo lo mueve la animación

        width: diameter * unit
        height: width
        transform: Translate { x: orb.dx * orb.unit * orb.t; y: orb.dy * orb.unit * orb.t }
        scale: 1 + orb.grow * orb.t

        Shape {
            anchors.fill: parent
            ShapePath {
                strokeWidth: -1
                fillColor: "transparent"
                fillGradient: RadialGradient {
                    centerX: orb.width / 2; centerY: orb.height / 2
                    focalX: centerX; focalY: centerY
                    centerRadius: orb.width * Math.SQRT1_2      // circle farthest-corner
                    GradientStop { position: 0; color: orb.tint }
                    GradientStop { position: 0.7; color: Qt.alpha(orb.tint, 0) }
                }
                PathRectangle { width: orb.width; height: orb.height }
            }
        }

        SequentialAnimation on t {
            running: orb.drifting
            loops: Animation.Infinite
            NumberAnimation { to: orb.fromEnd ? 0 : 1; duration: orb.legMs; easing.type: Easing.InOutSine }
            NumberAnimation { to: orb.fromEnd ? 1 : 0; duration: orb.legMs; easing.type: Easing.InOutSine }
        }
    }

    Orb {
        unit: root.u; drifting: root.running
        tint: root.primaryTone; diameter: 560
        x: root.width * 0.14; y: root.height * -0.12
        dx: 55; dy: 35; grow: 0.08; legMs: 18000
    }
    Orb {
        unit: root.u; drifting: root.running
        tint: root.secondaryTone; diameter: 480
        x: root.width * 0.96 - width; y: root.height * 0.22
        dx: -45; dy: 40; grow: 0.10; legMs: 22000
    }
    Orb {
        unit: root.u; drifting: root.running
        tint: Theme.orbTertiary; diameter: 520
        x: root.width * 0.20; y: root.height * 1.14 - height
        dx: 40; dy: -45; grow: 0.06; legMs: 20000
    }
    Orb {
        unit: root.u; drifting: root.running
        tint: Theme.orbWarm; diameter: 300
        x: root.width * -0.04; y: root.height * 0.04
        dx: -45; dy: 40; grow: 0.10; legMs: 16000; fromEnd: true
    }
}
