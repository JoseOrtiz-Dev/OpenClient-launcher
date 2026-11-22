import QtQuick
import QtQuick.Effects
import OpenClient

// Banner de una instancia con esquinas redondeadas: su imagen (personalizada, del modpack o última captura) o, si no hay,
// el arte generado de BannerArt. Con `instanceId` vacío es una previsualización: usa los preview*.
// Los hijos declarados dentro se pintan encima del velo y quedan recortados a las esquinas.
// Con GPU el recorte usa MultiEffect + máscara (igual que FrostedBackdrop); con el renderizador por software, un recorte
// rectangular (no hay shaders).
Item {
    id: root
    property string instanceId: ""
    property real radius: Theme.radiusLg
    // Velo oscuro uniforme, 0..1.
    property real scrim: 0
    // Para previsualizar (instancia nueva o editándose): si no están vacíos mandan sobre los datos de la instancia.
    property string previewCategory: ""
    property string previewVersion: ""
    property string previewName: ""
    property color previewAccent: "transparent"
    property string previewImage: ""        // imagen elegida aún sin aplicar (file:///…)
    // Ancho de decodificación de la imagen: 640 basta para bandas pequeñas.
    property int decodeWidth: 1280
    default property alias overlay: overlayItem.data

    readonly property var instance: instanceId.length > 0 ? (launcher.instances.find(item => item.id === instanceId) || null) : null
    readonly property var info: { launcher.bannerRevision; return instanceId.length > 0 ? launcher.bannerInfo(instanceId) : emptyInfo }
    readonly property var emptyInfo: ({ source: "generated", sharp: "", soft: "", accent: "" })
    readonly property string category: previewCategory.length > 0 ? previewCategory : (instance && instance.category) || "vanilla"
    readonly property string mcVersion: previewVersion.length > 0 ? previewVersion : (instance && instance.mcVersion) || ""
    readonly property string displayName: previewName.length > 0 ? previewName : (instance && instance.name) || ""
    readonly property string imageSource: previewImage.length > 0 ? previewImage : (info.sharp || "")
    readonly property bool generated: imageSource.length === 0
    readonly property color accent: previewImage.length === 0 && info.accent && info.accent.length > 0 ? info.accent
        : previewAccent.a > 0 ? previewAccent : Theme.loaderTone(category)
    readonly property bool gpu: GraphicsInfo.api !== GraphicsInfo.Software && GraphicsInfo.api !== GraphicsInfo.Unknown

    implicitWidth: 320
    implicitHeight: 100
    clip: !gpu
    Accessible.ignored: true

    // Con GPU el recorte redondeado es el efecto de la propia capa del contenido, que sigue visible: una fuente
    // oculta (visible: false) no siempre se vuelve a pintar al cambiar su imagen o al recrear la página.
    Rectangle {
        id: mask
        anchors.fill: parent
        radius: root.radius
        antialiasing: true
        visible: false
        layer.enabled: root.gpu
    }
    Item {
        id: content
        anchors.fill: parent
        layer.enabled: root.gpu
        layer.smooth: true
        layer.effect: MultiEffect {
            autoPaddingEnabled: false
            maskEnabled: true
            maskSource: mask
        }

        BannerArt {
            anchors.fill: parent
            category: root.category
            mcVersion: root.mcVersion
            seed: root.instanceId.length > 0 ? root.instanceId : root.displayName
            accent: root.previewAccent.a > 0 ? root.previewAccent : "transparent"
            // La imagen se funde por encima; con ella totalmente visible el arte deja de pintarse.
            visible: !picture.shown
        }
        CrossfadeImage {
            id: picture
            anchors.fill: parent
            source: root.imageSource
            decodeWidth: root.decodeWidth
        }
        Rectangle {
            anchors.fill: parent
            visible: root.scrim > 0
            color: Qt.alpha(Theme.base, root.scrim)
        }
        Item { id: overlayItem; anchors.fill: parent }
    }

}
