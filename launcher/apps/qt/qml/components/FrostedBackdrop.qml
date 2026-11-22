import QtQuick
import QtQuick.Effects
import OpenClient

// Blurs the region of `source` behind this item. The software renderer used by
// --smoke-test has no shader support, so the blur is skipped there.
Item {
    id: root
    property Item source: null
    property bool live: true
    property real radius: 0
    property real strength: 0.8
    readonly property bool available: source !== null && !launcher.potatoMode
        && GraphicsInfo.api !== GraphicsInfo.Software && GraphicsInfo.api !== GraphicsInfo.Unknown

    function refresh() {
        if (loader.item) loader.item.capture.scheduleUpdate()
    }

    Loader {
        id: loader
        anchors.fill: parent
        active: root.available && root.visible && root.width > 0 && root.height > 0
        sourceComponent: Item {
            property alias capture: capture
            ShaderEffectSource {
                id: capture
                anchors.fill: parent
                sourceItem: root.source
                live: root.live
                sourceRect: {
                    const origin = root.mapToItem(root.source, 0, 0)
                    return Qt.rect(origin.x + 0 * (root.x + root.y), origin.y, root.width, root.height)
                }
            }
            Rectangle {
                id: shape
                anchors.fill: parent
                radius: root.radius
                visible: false
                layer.enabled: true
            }
            MultiEffect {
                anchors.fill: parent
                source: capture
                autoPaddingEnabled: false
                blurEnabled: true
                blur: root.strength
                blurMax: 40
                saturation: 0.12
                maskEnabled: root.radius > 0
                maskSource: shape
            }
        }
    }
}
