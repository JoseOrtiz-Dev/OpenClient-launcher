import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

// Copias de seguridad de los mundos de una instancia: copiar, restaurar y borrar.
// Restaurar guarda antes una copia automática de lo que reemplaza, así que se puede deshacer.
ColumnLayout {
    id: panel
    objectName: "worldBackups"
    property var instance: null
    property var pending: null   // { backup, action: "restore" | "delete" }

    readonly property bool ready: !!instance && launcher.worldsInstance === instance.id
    readonly property var worlds: ready ? launcher.worlds : []
    readonly property var backups: ready ? launcher.worldBackups : []
    readonly property bool busy: launcher.catalogBusy || launcher.operationActive
    readonly property bool locked: busy || launcher.gameBusy

    spacing: 10

    function reload() {
        pending = null
        if (instance) launcher.loadWorlds(instance.id)
    }
    onInstanceChanged: reload()
    Component.onCompleted: reload()

    function bytesLabel(bytes) {
        const value = Number(bytes) || 0
        if (value >= 1073741824) return qsTr("%1 GB").arg((value / 1073741824).toFixed(1))
        if (value >= 1048576) return qsTr("%1 MB").arg(Math.round(value / 1048576))
        if (value >= 1024) return qsTr("%1 KB").arg(Math.round(value / 1024))
        return qsTr("%1 B").arg(value)
    }
    function dateLabel(ms) {
        const date = new Date(Number(ms))
        return isNaN(date.getTime()) ? "" : date.toLocaleString(Qt.locale(), Locale.ShortFormat)
    }
    function worldsLabel(list) {
        const names = list || []
        if (names.length === 0) return qsTr("sin índice")
        return names.length <= 3 ? names.join(", ") : qsTr("%1 y %2 más").arg(names.slice(0, 3).join(", ")).arg(names.length - 3)
    }

    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
    RowLayout {
        Layout.fillWidth: true
        Label {
            text: qsTr("Copias de seguridad")
            color: Theme.ink; font.pixelSize: 18; font.weight: Font.DemiBold
            Layout.fillWidth: true
        }
        ActionButton {
            objectName: "worldBackupAll"
            text: qsTr("Copiar todos los mundos"); glyph: "plus"; compact: true; primary: true
            enabled: !panel.locked && panel.worlds.length > 0
            onClicked: launcher.backupWorlds(panel.instance.id, [])
        }
    }
    Label {
        Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim
        text: qsTr("OpenClient también guarda una copia automática de tus mundos antes de actualizar un modpack (conserva las últimas 5) y antes de restaurar una.")
    }

    Flow {
        Layout.fillWidth: true
        visible: panel.worlds.length > 1
        spacing: 8
        Repeater {
            model: panel.worlds
            ActionButton {
                required property var modelData
                text: qsTr("Copiar «%1» · %2").arg(modelData.name).arg(panel.bytesLabel(modelData.bytes))
                compact: true
                enabled: !panel.locked
                onClicked: launcher.backupWorlds(panel.instance.id, [modelData.name])
            }
        }
    }

    Label {
        Layout.fillWidth: true
        visible: launcher.gameBusy
        text: qsTr("Cierra Minecraft para copiar o restaurar mundos.")
        color: Theme.warning; font.weight: Font.DemiBold
    }
    Label {
        Layout.fillWidth: true
        visible: text.length > 0 && panel.ready
        text: launcher.worldMessage
        textFormat: Text.PlainText; wrapMode: Text.WordWrap
        color: launcher.worldFailed ? Theme.danger : Theme.success
        Accessible.role: Accessible.StaticText
    }
    RowLayout {
        Layout.fillWidth: true
        visible: launcher.worldsLoading && panel.backups.length === 0
        spacing: 10
        BusyIndicator { running: parent.visible && Theme.motion; Layout.preferredWidth: 20; Layout.preferredHeight: 20 }
        Label { text: qsTr("Buscando copias…"); color: Theme.dim }
    }
    Label {
        visible: !launcher.worldsLoading && panel.ready && panel.backups.length === 0
        Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim
        text: panel.worlds.length === 0
            ? qsTr("Esta instancia todavía no tiene mundos que copiar.")
            : qsTr("Aún no hay copias. Pulsa «Copiar todos los mundos» antes de probar algo arriesgado.")
    }

    Repeater {
        model: panel.backups
        Rectangle {
            id: row
            required property var modelData
            readonly property bool asking: !!panel.pending && panel.pending.backup === modelData.id
            Layout.fillWidth: true
            implicitHeight: rowColumn.implicitHeight + 20
            radius: Theme.radiusMd; color: Theme.crystal ? Theme.rowFill : Theme.base
            border.color: asking ? (Theme.crystal ? Theme.accentSoft : Theme.highlight) : (Theme.crystal ? Theme.glassBorder : Theme.border)
            ColumnLayout {
                id: rowColumn
                anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
                anchors.margins: 10
                spacing: 8
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label {
                            Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                            color: Theme.ink; font.weight: Font.DemiBold
                            text: row.modelData.auto ? qsTr("Automática · %1").arg(panel.dateLabel(row.modelData.created)) : qsTr("Manual · %1").arg(panel.dateLabel(row.modelData.created))
                        }
                        Label {
                            Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                            color: Theme.dim; font.pixelSize: 11; font.features: Theme.tabular
                            text: panel.worldsLabel(row.modelData.worlds) + "  ·  " + panel.bytesLabel(row.modelData.bytes)
                        }
                    }
                    ActionButton {
                        objectName: "worldRestore"
                        text: qsTr("Restaurar"); glyph: "refresh"; compact: true
                        visible: !row.asking
                        enabled: !panel.locked && row.modelData.worlds.length > 0
                        onClicked: panel.pending = { backup: row.modelData.id, action: "restore" }
                    }
                    ActionButton {
                        text: qsTr("Eliminar"); compact: true
                        visible: !row.asking
                        enabled: !panel.locked
                        onClicked: panel.pending = { backup: row.modelData.id, action: "delete" }
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    visible: row.asking
                    spacing: 8
                    Label {
                        Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.ink; textFormat: Text.PlainText
                        text: panel.pending && panel.pending.action === "delete"
                            ? qsTr("¿Eliminar esta copia? No se puede deshacer.")
                            : qsTr("Los mundos «%1» volverán a como estaban el %2. Antes se guarda una copia de cómo están ahora.")
                                .arg(panel.worldsLabel(row.modelData.worlds)).arg(panel.dateLabel(row.modelData.created))
                    }
                    RowLayout {
                        spacing: 8
                        ActionButton { text: qsTr("Cancelar"); compact: true; onClicked: panel.pending = null }
                        ActionButton {
                            objectName: "worldConfirm"
                            primary: true; compact: true
                            text: panel.pending && panel.pending.action === "delete" ? qsTr("Eliminar copia") : qsTr("Restaurar")
                            enabled: !panel.locked
                            onClicked: {
                                const action = panel.pending.action
                                const backup = panel.pending.backup
                                panel.pending = null
                                if (action === "delete") launcher.deleteWorldBackup(panel.instance.id, backup)
                                else launcher.restoreWorldBackup(panel.instance.id, backup)
                            }
                        }
                    }
                }
            }
        }
    }
}
