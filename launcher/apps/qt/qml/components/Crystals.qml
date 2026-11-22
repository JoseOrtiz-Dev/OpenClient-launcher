import QtQuick
import QtQuick.Shapes
import OpenClient

// Decoración de la tarjeta hero: tres cristales facetados que flotan y dos anillos orbitales muy tenues.
// Llena a su padre (la tarjeta) y recorta lo que sobresale. Solo anima Translate y rotation.
Item {
    id: root
    anchors.fill: parent
    clip: true
    property bool active: true            // false: cristales quietos (ventana minimizada, diálogos)
    property real unit: 1                 // factor de tamaño de los cristales (1 = tamaño de la referencia)
    readonly property bool running: Theme.motion && visible && active

    // Anillo: círculo (cx, cy, r) del viewBox 1100×260 de la referencia, estirado al tamaño del item.
    // Es un aro de 1 px relleno con degradado diagonal blanco .28 → 0 (Shape no admite trazo con degradado).
    component Ring: Shape {
        id: ring
        required property real cx
        required property real cy
        required property real r
        readonly property real rx: r * ring.width / 1100
        readonly property real ry: r * ring.height / 260
        readonly property real px: cx * ring.width / 1100
        readonly property real py: cy * ring.height / 260
        anchors.fill: parent
        opacity: 0.35
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeWidth: -1
            fillRule: ShapePath.OddEvenFill
            fillColor: "transparent"
            fillGradient: LinearGradient {
                x1: ring.px - ring.rx; y1: ring.py - ring.ry
                x2: ring.px + ring.rx; y2: ring.py + ring.ry
                GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0.28) }
                GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0) }
            }
            PathAngleArc { centerX: ring.px; centerY: ring.py; radiusX: ring.rx + 0.5; radiusY: ring.ry + 0.5; startAngle: 0; sweepAngle: 360 }
            PathAngleArc { centerX: ring.px; centerY: ring.py; radiusX: ring.rx - 0.5; radiusY: ring.ry - 0.5; startAngle: 0; sweepAngle: 360 }
        }
    }

    // Cristal flotante: t va de 0 a 1 y vuelta (InOutSine) y mueve Translate.y y rotation.
    component Crystal: Image {
        id: c
        required property real size           // px de la referencia
        required property real unit
        required property bool drifting
        property real dy: 0                   // desplazamiento vertical máximo
        property real rot0: 0                 // rotación inicial (grados)
        property real rot1: 0                 // rotación final
        property int legMs: 4500              // mitad del ciclo de la referencia
        property real t: 0

        width: size * unit
        height: width
        sourceSize: Qt.size(width * 2, height * 2)
        smooth: true
        mipmap: true
        asynchronous: true
        rotation: rot0 + (rot1 - rot0) * t
        transform: Translate { y: c.dy * c.unit * c.t }

        SequentialAnimation on t {
            running: c.drifting
            loops: Animation.Infinite
            NumberAnimation { to: 1; duration: c.legMs; easing.type: Easing.InOutSine }
            NumberAnimation { to: 0; duration: c.legMs; easing.type: Easing.InOutSine }
        }
    }

    Ring { cx: 480; cy: 40; r: 240 }
    Ring { cx: 760; cy: 240; r: 210 }

    Crystal {
        source: "qrc:/assets/glass/crystal-a.svg"; size: 112; opacity: 0.55
        unit: root.unit; drifting: root.running
        x: root.width * 0.48; y: -24 * root.unit
        dy: -12; rot0: 14; rot1: 20; legMs: 4500          // 9 s de ciclo
    }
    Crystal {
        source: "qrc:/assets/glass/crystal-b.svg"; size: 96; opacity: 0.45
        unit: root.unit; drifting: root.running
        x: root.width * 0.40; y: root.height + 32 * root.unit - height
        dy: 10; rot0: -18; rot1: -10; legMs: 5500         // 11 s
    }
    Crystal {
        source: "qrc:/assets/glass/crystal-c.svg"; size: 48; opacity: 0.35
        unit: root.unit; drifting: root.running
        x: root.width * 0.73 - width; y: 32 * root.unit
        dy: -8; rot0: 8; rot1: 2; legMs: 3750             // 7.5 s
    }
}
