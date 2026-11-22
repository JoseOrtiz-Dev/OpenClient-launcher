import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

// Copias de seguridad que el motor guarda antes de cada cambio de versión del modpack.
// Restaurar una copia deja la instancia como estaba en ese momento.
GlassDialog {
    id: dialog
    objectName: "snapshotDialog"
    property var instance: null
    property var chosen: null        // copia pendiente de confirmar
    property bool loading: false
    property bool skipNext: false    // listSnapshots vacía la lista de forma síncrona antes de pedirla
    property string loadError: ""
    signal started(string kind)

    readonly property var items: instance && launcher.snapshotsInstance === instance.id ? launcher.snapshots : []
    readonly property bool busy: launcher.catalogBusy || launcher.operationActive

    title: chosen ? qsTr("Confirmar restauración") : qsTr("Restaurar versión anterior")
    glyph: "refresh"
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(600, (parent ? parent.width : 660) - 48)

    function reload() {
        if (!instance) return
        loadError = ""
        skipNext = true
        loading = true
        launcher.listSnapshots(instance.id)
    }
    onAboutToShow: { chosen = null; reload() }

    Connections {
        target: launcher
        function onSnapshotsChanged() {
            if (dialog.skipNext) { dialog.skipNext = false; return }
            dialog.loading = false
        }
        function onModpackResultChanged() {
            if (dialog.visible && dialog.loading === false && dialog.items.length === 0 && !dialog.chosen)
                dialog.loadError = launcher.modpackError
        }
    }

    function bytesLabel(bytes) {
        const value = Number(bytes) || 0
        if (value >= 1073741824) return qsTr("%1 GB").arg((value / 1073741824).toFixed(1))
        if (value >= 1048576) return qsTr("%1 MB").arg(Math.round(value / 1048576))
        if (value >= 1024) return qsTr("%1 KB").arg(Math.round(value / 1024))
        return qsTr("%1 B").arg(value)
    }
    function dateLabel(seconds) {
        const date = new Date(Number(seconds) * 1000)
        return isNaN(date.getTime()) ? "" : date.toLocaleString(Qt.locale(), Locale.ShortFormat)
    }
    function versionLabel(value) {
        return value && String(value).length ? String(value) : "anterior"
    }

    component SnapshotRow: ItemDelegate {
        id: row
        required property var modelData
        required property int index
        width: ListView.view ? ListView.view.width : implicitWidth
        height: 62
        hoverEnabled: true
        padding: 0
        leftPadding: 14; rightPadding: 10
        enabled: !dialog.busy && !launcher.gameBusy
        Accessible.name: qsTr("Restaurar la versión %1, copia del %2").arg(dialog.versionLabel(modelData.from)).arg(dialog.dateLabel(modelData.created))
        onClicked: dialog.chosen = modelData
        background: Rectangle {
            radius: Theme.radiusSm
            color: row.hovered ? (Theme.crystal ? Theme.rowFillHover : Qt.alpha(Theme.ink, Theme.light ? 0.05 : 0.07)) : "transparent"
            border.width: row.activeFocus ? 2 : 1
            border.color: row.activeFocus ? Theme.highlight : "transparent"
            Behavior on color { MotionColor { role: "fast" } }
        }
        contentItem: RowLayout {
            spacing: 12
            opacity: row.enabled ? 1 : 0.5
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Label {
                    Layout.fillWidth: true
                    text: qsTr("Volver a la versión %1").arg(dialog.versionLabel(row.modelData.from))
                    textFormat: Text.PlainText; color: Theme.ink; font.weight: Font.DemiBold; elide: Text.ElideRight
                }
                Label {
                    Layout.fillWidth: true
                    text: qsTr("Copia del %1  ·  hecha antes de pasar a la %2  ·  %3")
                        .arg(dialog.dateLabel(row.modelData.created)).arg(dialog.versionLabel(row.modelData.to)).arg(dialog.bytesLabel(row.modelData.bytes))
                    textFormat: Text.PlainText; color: Theme.dim; font.pixelSize: 11; font.features: Theme.tabular; elide: Text.ElideRight
                }
            }
            Label { text: qsTr("Restaurar"); color: Theme.highlight; font.pixelSize: 12; font.weight: Font.DemiBold }
        }
    }

    contentItem: ColumnLayout {
        spacing: 12

        // Paso 1: lista de copias.
        ColumnLayout {
            Layout.fillWidth: true
            visible: !dialog.chosen
            spacing: 12
            Label {
                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim
                text: qsTr("OpenClient guarda una copia antes de cada cambio de versión y conserva las últimas tres. Elige a cuál quieres volver.")
            }
            RowLayout {
                Layout.fillWidth: true
                visible: dialog.loading
                spacing: 10
                BusyIndicator { running: parent.visible && Theme.motion; Layout.preferredWidth: 22; Layout.preferredHeight: 22 }
                Label { text: qsTr("Buscando copias…"); color: Theme.dim; Layout.fillWidth: true }
            }
            Rectangle {
                Layout.fillWidth: true
                visible: !dialog.loading && dialog.items.length === 0
                implicitHeight: emptyColumn.implicitHeight + 28
                radius: Theme.radiusMd; color: Theme.crystal ? Theme.fieldFill : Theme.base; border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                ColumnLayout {
                    id: emptyColumn
                    anchors.fill: parent; anchors.margins: 14
                    spacing: 6
                    Label {
                        Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                        font.weight: Font.DemiBold
                        color: dialog.loadError.length ? Theme.danger : Theme.ink
                        text: dialog.loadError.length ? qsTr("No se pudieron leer las copias.") : qsTr("Aún no hay copias")
                    }
                    Label {
                        Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText; color: Theme.dim
                        text: dialog.loadError.length ? dialog.loadError : qsTr("Se crea una automáticamente la primera vez que cambies o actualices la versión del modpack.")
                    }
                    ActionButton {
                        visible: dialog.loadError.length > 0
                        text: qsTr("Reintentar"); glyph: "refresh"; compact: true
                        onClicked: dialog.reload()
                    }
                }
            }
            Rectangle {
                Layout.fillWidth: true
                visible: !dialog.loading && dialog.items.length > 0
                Layout.preferredHeight: Math.min(snapshotList.contentHeight, 250) + 8
                radius: Theme.radiusMd; color: Theme.crystal ? Theme.fieldFill : Theme.base; border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                ListView {
                    id: snapshotList
                    anchors.fill: parent; anchors.margins: 4
                    clip: true
                    spacing: 2
                    model: dialog.items
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar { policy: snapshotList.contentHeight > snapshotList.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff }
                    Accessible.name: "Copias de seguridad del modpack"
                    delegate: SnapshotRow {}
                }
            }
        }

        // Paso 2: confirmar.
        ColumnLayout {
            Layout.fillWidth: true
            visible: !!dialog.chosen
            spacing: 12
            Label {
                Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                color: Theme.ink; font.weight: Font.DemiBold
                text: dialog.chosen ? qsTr("¿Volver a la versión %1?").arg(dialog.versionLabel(dialog.chosen.from)) : ""
            }
            Label {
                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim
                text: qsTr("Los archivos del modpack se sustituirán por los de la copia del %1. Antes de hacerlo se guarda una copia del estado actual, así que también podrás deshacer este cambio. Tus mundos no se tocan.")
                    .arg(dialog.chosen ? dialog.dateLabel(dialog.chosen.created) : "")
            }
        }

        Label {
            Layout.fillWidth: true
            visible: launcher.gameBusy
            text: qsTr("Cierra Minecraft para continuar")
            color: Theme.warning
            font.weight: Font.DemiBold
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Item { Layout.fillWidth: true }
            ActionButton {
                text: dialog.chosen ? qsTr("Volver a la lista") : qsTr("Cerrar")
                onClicked: dialog.chosen ? dialog.chosen = null : dialog.close()
            }
            ActionButton {
                objectName: "snapshotRestore"
                visible: !!dialog.chosen
                primary: true
                text: qsTr("Restaurar esta versión")
                enabled: !!dialog.chosen && !dialog.busy && !launcher.gameBusy
                onClicked: {
                    launcher.restoreSnapshot(dialog.instance.id, dialog.chosen.id)
                    dialog.started("modpack_restore")
                    dialog.close()
                }
            }
        }
    }
}
