import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

// Progreso real de lo que está haciendo el motor, con Cancelar cuando se puede.
// `game: false` -> operación de contenido (descargas, importaciones, modpacks).
// `game: true`  -> preparación del lanzamiento de Minecraft (solo cancelable mientras prepara).
ColumnLayout {
    id: bar
    objectName: "operationBar"
    property bool game: false
    property string instanceId: ""
    property string heading: ""

    readonly property bool active: game ? launcher.preparingInstanceId.length > 0
        && (!instanceId.length || launcher.preparingInstanceId === instanceId) : launcher.operationActive
    readonly property string message: game ? launcher.gameMessage : (launcher.operationMessage || heading)
    readonly property int percent: game ? (launcher.gamePercent > 0 ? launcher.gamePercent : -1) : launcher.operationPercent
    readonly property bool cancellable: game ? active && launcher.gamePercent < 100 : launcher.operationCancellable

    visible: active
    spacing: 6

    RowLayout {
        Layout.fillWidth: true
        spacing: 10
        Label {
            Layout.fillWidth: true
            text: bar.message
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Theme.ink
            font.pixelSize: 12
        }
        Label {
            visible: bar.percent >= 0
            text: qsTr("%1 %").arg(bar.percent)
            color: Theme.dim
            font.pixelSize: 12
            font.features: Theme.tabular
        }
        ActionButton {
            visible: bar.cancellable
            text: bar.game ? qsTr("Cancelar lanzamiento") : qsTr("Cancelar")
            compact: true
            Accessible.name: bar.game ? qsTr("Cancelar el lanzamiento de Minecraft") : qsTr("Cancelar la operación en curso")
            onClicked: bar.game ? launcher.cancelLaunch() : launcher.cancelOperation()
        }
    }
    ProgressBar {
        Layout.fillWidth: true
        from: 0; to: 100
        value: Math.max(0, bar.percent)
        indeterminate: bar.percent < 0
        Accessible.name: bar.game ? qsTr("Progreso del lanzamiento") : qsTr("Progreso de la operación")
        Accessible.description: bar.percent >= 0 ? qsTr("%1 por ciento").arg(bar.percent) : qsTr("En curso")
    }
}
