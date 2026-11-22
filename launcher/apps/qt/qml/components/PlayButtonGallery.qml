import QtQuick
import QtQuick.Layouts
import OpenClient

// Solo para capturas de QA (OPENCLIENT_QA_PLAY_GALLERY): los cinco estados de PlayButton a la vez.
Rectangle {
    objectName: "playGallery"
    width: 350
    implicitHeight: column.implicitHeight + 28
    height: implicitHeight
    radius: Theme.radiusLg
    color: Theme.glassElevated
    border.color: Theme.rim
    ColumnLayout {
        id: column
        anchors.fill: parent
        anchors.margins: 14
        spacing: 10
        Repeater {
            model: [
                { mode: "ready", percent: -1 }, { mode: "preparing", percent: 42 }, { mode: "running", percent: -1 },
                { mode: "session", percent: -1 }, { mode: "none", percent: -1 }
            ]
            PlayButton {
                required property var modelData
                Layout.fillWidth: true
                forceMode: modelData.mode
                forcePercent: modelData.percent
                hasInstance: true
                instanceLabel: qsTr("Prueba visual · Fabric 1.21.4")
                emptyLabel: qsTr("Instalar DiosesMC")
                emptyGlyph: "download"
            }
        }
    }
}
