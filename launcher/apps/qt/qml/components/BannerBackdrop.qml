import QtQuick
import OpenClient

// Fondo de ventana del Inicio: la versión suave (ya desenfocada) de la imagen de la instancia, con fundido cruzado al
// cambiar de instancia, deriva muy lenta (solo scale y Translate) y degradado hacia Theme.base abajo para fundirse con el fondo.
// Si la instancia no tiene imagen no pinta nada: los orbes toman el color (`accent`) y ya.
Item {
    id: root
    property string instanceId: ""
    // false con la ventana inactiva, minimizada o con un diálogo encima: la deriva se pausa.
    property bool active: true

    readonly property var instance: instanceId.length > 0 ? (launcher.instances.find(item => item.id === instanceId) || null) : null
    readonly property var info: { launcher.bannerRevision; return instanceId.length > 0 ? launcher.bannerInfo(instanceId) : emptyInfo }
    readonly property var emptyInfo: ({ source: "generated", sharp: "", soft: "", accent: "" })
    readonly property bool hasImage: !!info.soft && info.soft.length > 0 && info.source !== "generated"
    // Color dominante de la imagen o, sin imagen, el del cargador. Transparente si no hay instancia.
    readonly property color accent: instanceId.length === 0 ? "transparent"
        : info.accent && info.accent.length > 0 ? info.accent
        : Theme.loaderTone((instance && instance.category) || "vanilla")
    readonly property bool running: Theme.motion && active && visible && hasImage
    // 0..1 y vuelta. Sin movimiento la imagen queda quieta en el punto medio.
    property real drift: 0.5
    readonly property real phase: Theme.motion ? drift : 0.5

    clip: true
    Accessible.ignored: true

    CrossfadeImage {
        anchors.fill: parent
        source: root.hasImage ? root.info.soft : ""
        fadeRole: "slow"
        decodeWidth: 960
        opacity: 0.5
        scale: 1.06 + 0.06 * root.phase
        transform: Translate {
            x: (root.phase - 0.5) * 0.03 * root.width
            y: (0.5 - root.phase) * 0.02 * root.height
        }
    }

    // Se funde con el fondo hacia abajo.
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.alpha(Theme.base, 0) }
            GradientStop { position: 0.5; color: Qt.alpha(Theme.base, 0.3) }
            GradientStop { position: 1; color: Theme.base }
        }
    }

    SequentialAnimation {
        running: root.running
        loops: Animation.Infinite
        NumberAnimation { target: root; property: "drift"; to: 1; duration: 45000; easing.type: Easing.InOutSine } // motion-ok: deriva infinita, gateada por Theme.motion, active y visible
        NumberAnimation { target: root; property: "drift"; to: 0; duration: 45000; easing.type: Easing.InOutSine } // motion-ok: deriva infinita, gateada por Theme.motion, active y visible
    }
}
