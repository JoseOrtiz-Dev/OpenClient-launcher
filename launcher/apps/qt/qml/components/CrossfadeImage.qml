import QtQuick
import OpenClient

// Imagen con fundido cruzado al cambiar de `source`: la nueva se funde por encima y la anterior se retira cuando
// queda opaca (sin hueco intermedio). Solo anima opacity. Sin movimiento (Theme.motion falso) el cambio es inmediato.
Item {
    id: root
    property string source: ""
    // Rol de MotionNumber del fundido: base o slow.
    property string fadeRole: "base"
    property int fillMode: Image.PreserveAspectCrop
    // Ancho de decodificación: evita cargar más píxeles de los que se pintan.
    property int decodeWidth: 1280
    // Hay una imagen completamente visible.
    readonly property bool shown: a.opacity >= 1 || b.opacity >= 1

    property bool frontIsA: false
    property string applied: ""

    function update() {
        if (source === applied) return
        applied = source
        if (source.length === 0) {
            a.wanted = false
            b.wanted = false
            return
        }
        const next = frontIsA ? b : a
        const previous = frontIsA ? a : b
        next.source = source
        next.wanted = true
        next.z = 1
        previous.z = 0
        frontIsA = !frontIsA
    }
    onSourceChanged: update()
    Component.onCompleted: update()

    component Layer: Image {
        id: layer
        property bool wanted: false
        anchors.fill: parent
        fillMode: root.fillMode
        asynchronous: true
        cache: true
        smooth: true
        sourceSize.width: root.decodeWidth
        opacity: wanted && status === Image.Ready ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { MotionNumber { role: root.fadeRole } }
        onOpacityChanged: {
            const other = layer === a ? b : a
            if (opacity >= 1 && layer.wanted && (layer === a) === root.frontIsA) other.wanted = false
            else if (opacity <= 0 && !wanted && source.toString().length > 0) source = ""
        }
    }
    Layer { id: a }
    Layer { id: b }
}
