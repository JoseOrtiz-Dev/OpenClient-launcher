import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs as NativeDialogs
import QtQuick.Layouts
import OpenClient

// Elegir la versión de un modpack vinculado. `info` es la respuesta de modpack_check
// (launcher.modpackInfo[id]); el dialogo aplica el cambio con launcher.updateModpack.
GlassDialog {
    id: dialog
    objectName: "modpackVersionDialog"
    property var instance: null
    property var info: ({})
    property string preselect: ""
    property string selectedId: ""
    signal started(string kind)

    readonly property var versions: info && info.versions ? info.versions : []
    readonly property string source: info && info.source ? info.source : ""
    readonly property string currentId: info && info.current ? (info.current.versionId || "") : ""
    readonly property string latestId: info && info.latest ? (info.latest.id || "") : ""
    readonly property int currentIndex: versions.findIndex(v => v.id === currentId)
    readonly property int selectedIndex: versions.findIndex(v => v.id === selectedId)
    readonly property var selected: selectedIndex >= 0 ? versions[selectedIndex] : null
    readonly property bool checking: !!(info && info.checking)
    readonly property bool busy: launcher.catalogBusy || launcher.operationActive
    readonly property bool canApply: !!selected && selected.id !== currentId && !busy && !launcher.gameBusy
        && !!instance && !!info && info.manifest !== false
    // El motor adapta la instancia al pack; avisamos antes si la versión elegida cambia el juego o el cargador.
    readonly property bool changesGame: !!selected && !!instance
        && ((selected.minecraft || []).length > 0 && !(selected.minecraft || []).includes(instance.mcVersion))
    readonly property bool changesLoader: !!selected && !!instance
        && ((selected.loaders || []).length > 0 && !(selected.loaders || []).includes(instance.category))
    readonly property bool movesBack: selectedIndex >= 0 && currentIndex >= 0 && selectedIndex > currentIndex

    title: qsTr("Cambiar versión del modpack")
    glyph: "layers"
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(640, (parent ? parent.width : 700) - 48)

    onAboutToShow: selectedId = preselect.length > 0 ? preselect : (info && info.updateAvailable ? latestId : "")

    function channelLabel(channel) {
        return channel === "release" ? qsTr("Estable") : channel === "beta" ? qsTr("Beta") : channel === "alpha" ? qsTr("Alfa") : (channel || "")
    }
    function dateLabel(value) {
        const date = new Date(value)
        return isNaN(date.getTime()) ? "" : date.toLocaleDateString(Qt.locale(), Locale.ShortFormat)
    }
    function listLabel(values) {
        return (values || []).slice(0, 3).join(", ")
    }

    component VersionRow: ItemDelegate {
        id: row
        required property var modelData
        required property int index
        readonly property bool chosen: modelData.id === dialog.selectedId
        readonly property bool installed: modelData.id === dialog.currentId
        width: ListView.view ? ListView.view.width : implicitWidth
        height: 58
        hoverEnabled: true
        padding: 0
        leftPadding: 12; rightPadding: 12
        Accessible.name: (installed && modelData.id === dialog.latestId ? qsTr("Versión %1, %2, instalada, la más reciente")
            : installed ? qsTr("Versión %1, %2, instalada")
            : modelData.id === dialog.latestId ? qsTr("Versión %1, %2, la más reciente")
            : qsTr("Versión %1, %2")).arg(modelData.number).arg(dialog.channelLabel(modelData.channel))
        Accessible.checkable: true
        Accessible.checked: chosen
        onClicked: dialog.selectedId = modelData.id
        background: Rectangle {
            radius: Theme.radiusSm
            color: row.chosen ? (Theme.crystal ? Qt.alpha(Theme.accent, 0.14) : Theme.soft)
                : row.hovered ? (Theme.crystal ? Theme.rowFillHover : Qt.alpha(Theme.ink, Theme.light ? 0.05 : 0.07)) : "transparent"
            border.width: row.activeFocus ? 2 : 1
            border.color: row.activeFocus ? Theme.highlight : row.chosen ? (Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.4) : Theme.edge) : "transparent"
            Behavior on color { MotionColor { role: "fast" } }
        }
        contentItem: RowLayout {
            spacing: 12
            Rectangle {
                Layout.preferredWidth: 18; Layout.preferredHeight: 18
                radius: 9
                color: "transparent"
                border.width: 2
                border.color: row.chosen ? Theme.accent : Theme.dim
                Rectangle {
                    anchors.centerIn: parent
                    width: 8; height: 8; radius: 4
                    color: Theme.accent
                    opacity: row.chosen ? 1 : 0
                    Behavior on opacity { MotionNumber { role: "fast" } }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Label {
                        text: row.modelData.number || row.modelData.id
                        textFormat: Text.PlainText
                        color: Theme.ink; font.weight: Font.DemiBold; font.features: Theme.tabular
                        elide: Text.ElideRight
                        Layout.maximumWidth: 220
                    }
                    Rectangle {
                        radius: 4; color: Theme.crystal ? Theme.rowFill : Theme.surface; border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                        implicitWidth: channelText.implicitWidth + 10; implicitHeight: 18
                        Label {
                            id: channelText; anchors.centerIn: parent
                            text: dialog.channelLabel(row.modelData.channel)
                            textFormat: Text.PlainText; color: Theme.dim; font.pixelSize: 10
                        }
                    }
                    Rectangle {
                        visible: row.installed
                        radius: 4; color: Qt.alpha(Theme.success, 0.14); border.color: Qt.alpha(Theme.success, 0.5)
                        implicitWidth: installedText.implicitWidth + 10; implicitHeight: 18
                        Label { id: installedText; anchors.centerIn: parent; text: qsTr("Instalada"); color: Theme.success; font.pixelSize: 10; font.weight: Font.DemiBold }
                    }
                    Rectangle {
                        visible: row.modelData.id === dialog.latestId && !row.installed
                        radius: 4; color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft; border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
                        implicitWidth: latestText.implicitWidth + 10; implicitHeight: 18
                        Label { id: latestText; anchors.centerIn: parent; text: qsTr("Más reciente"); color: Theme.highlight; font.pixelSize: 10; font.weight: Font.DemiBold }
                    }
                    Item { Layout.fillWidth: true }
                }
                Label {
                    Layout.fillWidth: true
                    text: dialog.dateLabel(row.modelData.date).length
                        ? qsTr("Minecraft %1  ·  %2  ·  %3").arg(dialog.listLabel(row.modelData.minecraft)).arg(dialog.listLabel(row.modelData.loaders)).arg(dialog.dateLabel(row.modelData.date))
                        : qsTr("Minecraft %1  ·  %2").arg(dialog.listLabel(row.modelData.minecraft)).arg(dialog.listLabel(row.modelData.loaders))
                    textFormat: Text.PlainText; color: Theme.dim; font.pixelSize: 11; font.features: Theme.tabular
                    elide: Text.ElideRight
                }
            }
        }
    }

    contentItem: ColumnLayout {
        spacing: 12

        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
            color: Theme.dim
            text: dialog.instance
                ? qsTr("Elige qué versión de «%1» quieres tener. Antes de cambiar nada se guarda una copia, así que podrás volver atrás.").arg(dialog.instance.name)
                : ""
        }

        // Cargando / error / vacío.
        RowLayout {
            Layout.fillWidth: true
            visible: dialog.checking && dialog.versions.length === 0
            spacing: 10
            BusyIndicator { running: parent.visible && Theme.motion; Layout.preferredWidth: 22; Layout.preferredHeight: 22 }
            Label { text: qsTr("Buscando versiones disponibles…"); color: Theme.dim; Layout.fillWidth: true }
        }
        Rectangle {
            Layout.fillWidth: true
            visible: !dialog.checking && dialog.versions.length === 0
            implicitHeight: emptyColumn.implicitHeight + 28
            radius: Theme.radiusMd; color: Theme.crystal ? Theme.fieldFill : Theme.base; border.color: Theme.crystal ? Theme.glassBorder : Theme.border
            ColumnLayout {
                id: emptyColumn
                anchors.fill: parent; anchors.margins: 14
                spacing: 10
                Label {
                    Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                    color: info && info.error ? Theme.danger : Theme.dim
                    text: info && info.error ? info.error
                        : dialog.source === "official" ? qsTr("El modpack oficial se actualiza desde la sección «Oficial».")
                        : dialog.source === "curseforge" ? qsTr("Los modpacks importados de CurseForge no se pueden actualizar desde aquí.")
                        : dialog.source === "file" ? qsTr("Este modpack se instaló desde un archivo. Elige un archivo .mrpack con la versión que quieres instalar.")
                        : qsTr("No hay otras versiones disponibles para esta instancia.")
                }
                RowLayout {
                    spacing: 8
                    ActionButton {
                        visible: dialog.source === "file"
                        text: qsTr("Elegir archivo .mrpack…"); glyph: "folder"; compact: true
                        enabled: !dialog.busy && !launcher.gameBusy
                        onClicked: updateFile.open()
                    }
                    ActionButton {
                        visible: dialog.source === "modrinth"
                        text: qsTr("Reintentar"); glyph: "refresh"; compact: true
                        enabled: !dialog.checking && !!dialog.instance
                        onClicked: launcher.checkModpack(dialog.instance.id, true)
                    }
                }
            }
        }

        // Lista de versiones.
        Rectangle {
            Layout.fillWidth: true
            visible: dialog.versions.length > 0
            Layout.preferredHeight: Math.min(versionList.contentHeight, 270) + 8
            radius: Theme.radiusMd; color: Theme.crystal ? Theme.fieldFill : Theme.base; border.color: Theme.crystal ? Theme.glassBorder : Theme.border
            ListView {
                id: versionList
                anchors.fill: parent; anchors.margins: 4
                clip: true
                spacing: 2
                model: dialog.versions
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: versionList.contentHeight > versionList.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff }
                Accessible.name: "Versiones disponibles del modpack"
                delegate: VersionRow {}
            }
        }

        // Falta el vínculo con la lista de archivos: no se puede actualizar sin repararlo.
        Label {
            Layout.fillWidth: true
            visible: !!info && info.manifest === false && dialog.versions.length > 0
            wrapMode: Text.WordWrap
            color: Theme.warning
            text: qsTr("Falta la lista de archivos de este modpack. Cierra esta ventana y usa «Reparar vínculo» antes de cambiar de versión.")
        }

        // Qué pasará al aplicar la versión elegida.
        Rectangle {
            Layout.fillWidth: true
            visible: !!dialog.selected
            implicitHeight: effectColumn.implicitHeight + 24
            radius: Theme.radiusMd; color: Theme.crystal ? Qt.alpha(Theme.accent, 0.10) : Theme.soft; border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.3) : Theme.edge
            ColumnLayout {
                id: effectColumn
                anchors.fill: parent; anchors.margins: 12
                spacing: 6
                Label {
                    Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                    color: Theme.ink; font.weight: Font.DemiBold
                    text: dialog.selected && dialog.selected.id === dialog.currentId
                        ? qsTr("Esta es la versión que ya tienes instalada.")
                        : qsTr("Se conservan tus ajustes modificados")
                }
                Label {
                    Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                    color: Theme.dim
                    visible: !!dialog.selected && dialog.selected.id !== dialog.currentId
                    text: info && info.modifiedCount > 0
                        ? qsTr("La configuración y los archivos que hayas cambiado no se sobrescriben (hemos detectado %1 modificados por ti), y tus mundos nunca se tocan. Los mods del modpack sí se sustituyen por los de la versión elegida. Al terminar verás la lista de lo que se conservó.").arg(info.modifiedCount)
                        : qsTr("La configuración y los archivos que hayas cambiado no se sobrescriben, y tus mundos nunca se tocan. Los mods del modpack sí se sustituyen por los de la versión elegida. Al terminar verás la lista de lo que se conservó.")
                }
                Label {
                    Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                    color: Theme.warning
                    visible: dialog.changesGame || dialog.changesLoader
                    text: dialog.changesGame && dialog.changesLoader
                        ? qsTr("Atención: esta versión usa otra versión de Minecraft y otro cargador de mods distinto al de tu instancia. La instancia se adaptará al modpack.")
                        : dialog.changesGame
                        ? qsTr("Atención: esta versión usa otra versión de Minecraft distinto al de tu instancia. La instancia se adaptará al modpack.")
                        : qsTr("Atención: esta versión usa otro cargador de mods distinto al de tu instancia. La instancia se adaptará al modpack.")
                }
                Label {
                    Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                    color: Theme.warning
                    visible: dialog.movesBack
                    text: qsTr("Es una versión anterior a la que tienes. Es posible que los mundos creados con la versión nueva no funcionen bien.")
                }
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
            ActionButton {
                visible: dialog.source === "file" && dialog.versions.length > 0
                text: qsTr("Usar un archivo…"); glyph: "folder"; compact: true
                enabled: !dialog.busy && !launcher.gameBusy
                onClicked: updateFile.open()
            }
            Item { Layout.fillWidth: true }
            ActionButton { text: qsTr("Cancelar"); onClicked: dialog.close() }
            ActionButton {
                objectName: "modpackVersionApply"
                primary: true
                text: dialog.selected ? (dialog.movesBack ? qsTr("Cambiar a %1") : qsTr("Actualizar a %1")).arg(dialog.selected.number || "") : qsTr("Elige una versión")
                enabled: dialog.canApply
                onClicked: {
                    launcher.updateModpack(dialog.instance.id, dialog.selected.id)
                    dialog.started("modpack_update")
                    dialog.close()
                }
            }
        }
    }

    NativeDialogs.FileDialog {
        id: updateFile
        title: qsTr("Elige el archivo del modpack")
        fileMode: NativeDialogs.FileDialog.OpenFile
        nameFilters: ["Modpack de Modrinth (*.mrpack)"]
        onAccepted: {
            if (!dialog.instance) return
            launcher.updateModpackFromFile(dialog.instance.id, selectedFile)
            dialog.started("modpack_update")
            dialog.close()
        }
    }
}
