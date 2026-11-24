import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

// Se abre solo mientras se descarga o instala una actualización, o si falló. No se cierra con Esc ni haciendo
// clic fuera: durante la descarga se puede cancelar y, si falla, cerrar.
GlassDialog {
    id: dialog
    objectName: "updateDialog"
    readonly property string stage: maintenance.updateStage
    readonly property int percent: maintenance.updatePercent
    glyph: stage === "failed" ? "alert" : "download"
    title: stage === "failed" ? qsTr("No se pudo actualizar")
         : stage === "installing" ? qsTr("Instalando actualización")
         : qsTr("Descargando actualización")
    closePolicy: Popup.NoAutoClose
    visible: stage.length > 0

    contentItem: ColumnLayout {
        spacing: 14
        Label {
            Layout.fillWidth: true
            visible: dialog.stage !== "failed"
            text: dialog.stage === "installing"
                ? qsTr("OpenClient se cerrará y volverá a abrirse solo con la versión %1.").arg(maintenance.updateVersion)
                : qsTr("Versión %1 · %2 MB").arg(maintenance.updateVersion).arg(maintenance.updateSizeMb.toFixed(1))
            color: Theme.dim
            wrapMode: Text.WordWrap
            font.features: Theme.tabular
        }
        ColumnLayout {
            Layout.fillWidth: true
            visible: dialog.stage !== "failed"
            spacing: 6
            RowLayout {
                Layout.fillWidth: true
                Label {
                    Layout.fillWidth: true
                    text: dialog.stage === "installing" ? qsTr("Instalando…") : qsTr("Descargando…")
                    color: Theme.ink
                    font.pixelSize: 12
                }
                Label {
                    visible: dialog.stage === "downloading" && dialog.percent >= 0
                    text: qsTr("%1 %").arg(dialog.percent)
                    color: Theme.dim
                    font.pixelSize: 12
                    font.features: Theme.tabular
                }
            }
            ProgressBar {
                Layout.fillWidth: true
                from: 0; to: 100
                value: Math.max(0, dialog.percent)
                indeterminate: dialog.stage === "installing" || dialog.percent < 0
                Accessible.name: qsTr("Progreso de la actualización")
                Accessible.description: dialog.percent >= 0 ? qsTr("%1 por ciento").arg(dialog.percent) : qsTr("En curso")
            }
        }
        Label {
            Layout.fillWidth: true
            visible: dialog.stage === "failed"
            text: maintenance.updateMessage
            color: Theme.ink
            wrapMode: Text.WordWrap
        }
        Label {
            Layout.fillWidth: true
            visible: dialog.stage === "downloading"
            text: qsTr("Se comprueba la huella SHA-256 antes de instalar. Puedes desactivar esto en Ajustes › Actualizaciones.")
            color: Theme.dim
            font.pixelSize: 11
            wrapMode: Text.WordWrap
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: 8
            ActionButton {
                visible: dialog.stage === "downloading"
                text: qsTr("Cancelar")
                onClicked: maintenance.cancelUpdateDownload()
            }
            ActionButton {
                visible: dialog.stage === "failed"
                text: qsTr("Cerrar")
                primary: true
                onClicked: maintenance.dismissUpdate()
            }
        }
    }
}
