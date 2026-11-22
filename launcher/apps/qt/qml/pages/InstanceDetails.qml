import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs as NativeDialogs
import QtQuick.Layouts
import QtCore
import OpenClient

ColumnLayout {
    id: root
    objectName: "instanceDetails"
    required property var instance
    property int section: 0
    property var files: []
    property string logText: ""
    property string contentKind: "mod"
    property string addonFilter: "all"
    readonly property var contentTypes: [
        { kind: "mod", label: qsTr("Mods"), folder: "mods" },
        { kind: "shader", label: qsTr("Shaders"), folder: "shaderpacks" },
        { kind: "resourcepack", label: qsTr("Texturas"), folder: "resourcepacks" }
    ]
    readonly property var contentType: contentTypes.find(item => item.kind === contentKind) || contentTypes[0]
    readonly property bool canBrowseContent: instance !== null && (contentKind === "resourcepack" || instance.category !== "vanilla")
    readonly property int activeModsCount: instance ? (instance.installedContent || []).filter(i => i.enabled !== false).length : 0
    // Estado del modpack vinculado (respuesta de modpack_check) y de la operación en curso.
    readonly property var packInfo: instance ? (launcher.modpackInfo[instance.id] || ({})) : ({})
    readonly property bool hasPack: !!(instance && (instance.modpackVersion || instance.modpack))
    readonly property bool packLinked: !!packInfo.linked
    readonly property bool packLegacy: !!packInfo.legacy && !packLinked
    readonly property bool packChecking: !!packInfo.checking
    readonly property bool updateAvailable: packLinked && !!packInfo.updateAvailable && !!packInfo.latest
    readonly property string latestLabel: packInfo.latest ? versionName(packInfo.latest.number) : ""
    readonly property bool operationRunning: launcher.operationActive || launcher.catalogBusy
    readonly property bool canChangePack: !!instance && !operationRunning && !launcher.gameBusy
    readonly property string exportName: instance ? String(instance.name).replace(/[\\/:*?"<>|]/g, "-") : "instancia"
    property string pendingOp: ""
    property var packResult: null
    signal back()
    signal edit()
    signal retire()
    signal explore(string instanceId, string kind)
    spacing: 14

    function openContent(kind) {
        if (instance) explore(instance.id, kind)
    }

    function openVersionDialog(preselect) {
        versionDialog.preselect = preselect || ""
        versionDialog.open()
    }

    function versionName(number) {
        const text = String(number || "")
        return /^[0-9]/.test(text) ? qsTr("v%1").arg(text) : text
    }

    // Una sola comprobación por instancia y sesión: el controlador ya limita las repetidas.
    function autoCheck() {
        if (instance && hasPack) launcher.checkModpack(instance.id)
    }
    onInstanceChanged: { packResult = null; autoCheck() }
    Component.onCompleted: autoCheck()

    // Se llama justo después de lanzar una operación desde los diálogos.
    function trackOperation(kind) {
        packResult = null
        pendingOp = launcher.catalogBusy ? kind : ""
    }

    function finishOperation() {
        const message = launcher.catalogMessage
        const error = launcher.modpackError
        const cancelled = message === "Operación cancelada."
        const failed = !cancelled && error.length > 0 && message === error
        const result = launcher.modpackResult || ({})
        let detail = ""
        if (!failed && !cancelled) {
            if (pendingOp === "modpack_export" && result.embeddedCount > 0) {
                const names = (result.embeddedJars || []).slice(0, 5).join(", ")
                detail = (names.length === 0 ? qsTr("%1 mod(s) no están en Modrinth y se copiaron dentro del archivo.").arg(result.embeddedCount)
                        : result.embeddedCount > 5 ? qsTr("%1 mod(s) no están en Modrinth y se copiaron dentro del archivo (%2 y otros).").arg(result.embeddedCount).arg(names)
                        : qsTr("%1 mod(s) no están en Modrinth y se copiaron dentro del archivo (%2).").arg(result.embeddedCount).arg(names))
                    + " " + qsTr("Antes de compartirlo, comprueba que sus licencias permiten redistribuirlos.")
            } else if (result.kept && result.kept.length > 0) {
                const shown = result.kept.slice(0, 6).join(", ")
                const more = Math.max(0, (result.keptCount || result.kept.length) - 6)
                detail = more > 0 ? qsTr("Se conservaron tus archivos modificados: %1 y %2 más.").arg(shown).arg(more) : qsTr("Se conservaron tus archivos modificados: %1.").arg(shown)
            }
        }
        packResult = { tone: failed ? "error" : cancelled ? "neutral" : "ok", text: message, detail: detail, warn: pendingOp === "modpack_export" }
        pendingOp = ""
    }

    Connections {
        target: launcher
        function onCatalogChanged() {
            if (root.pendingOp.length > 0 && !launcher.catalogBusy) root.finishOperation()
        }
    }

    function selectSection(index) {
        section = index
        if (!instance) return
        if (index === 2 || index === 3)
            files = launcher.instanceFiles(instance.id, index === 2 ? "worlds" : "screenshots")
        else if (index === 4)
            logText = launcher.instanceLog(instance.id)
    }

    ActionButton { text: qsTr("Volver a la biblioteca"); glyph: "layers"; compact: true; navigation: true; onClicked: root.back() }
    GlassPanel {
        Layout.fillWidth: true
        implicitHeight: 112
        padding: 18
        accented: true
        clip: true
        CrystalArt { anchors.fill: parent; anchors.margins: -17; banner: true }
        RowLayout {
            anchors.fill: parent
            spacing: 16
            Rectangle {
                width: 56; height: 56; radius: Theme.radiusMd
                color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
                border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
                InstanceBlock { anchors.centerIn: parent; width: 40; height: 40; loader: root.instance ? root.instance.category : "vanilla"; fabric: !!root.instance && root.instance.category === "fabric" }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 5
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Label {
                        text: root.instance ? root.instance.name : ""
                        color: Theme.ink; font.pixelSize: 21; font.weight: Font.DemiBold
                        textFormat: Text.PlainText; elide: Text.ElideRight
                        Layout.maximumWidth: 380
                    }
                    Rectangle {
                        visible: !!(root.instance && root.instance.modpackVersion)
                        radius: 4
                        color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
                        border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
                        implicitWidth: packBadge.implicitWidth + 10; implicitHeight: 20
                        Label {
                            id: packBadge; anchors.centerIn: parent
                            text: root.instance && root.instance.modpackVersion ? qsTr("Modpack %1").arg(root.instance.modpackVersion) : ""
                            textFormat: Text.PlainText; color: Theme.highlight; font.pixelSize: 10; font.features: Theme.tabular
                        }
                    }
                    Button {
                        id: updateChip
                        objectName: "modpackUpdateChip"
                        visible: root.updateAvailable
                        hoverEnabled: true
                        padding: 0; leftPadding: 8; rightPadding: 8
                        implicitHeight: 20
                        text: qsTr("Actualización disponible: %1").arg(root.latestLabel)
                        Accessible.name: qsTr("%1. Abre la elección de versión del modpack.").arg(text)
                        onClicked: root.openVersionDialog(root.packInfo.latest.id)
                        background: Rectangle {
                            radius: 4
                            color: updateChip.hovered ? Qt.alpha(Theme.success, 0.24) : Qt.alpha(Theme.success, 0.14)
                            border.width: updateChip.activeFocus ? 2 : 1
                            border.color: updateChip.activeFocus ? Theme.highlight : Qt.alpha(Theme.success, 0.55)
                            Behavior on color { MotionColor { role: "fast" } }
                        }
                        contentItem: Label {
                            text: updateChip.text
                            textFormat: Text.PlainText
                            color: Theme.success; font.pixelSize: 10; font.weight: Font.DemiBold; font.features: Theme.tabular
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                    Item { Layout.fillWidth: true }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    Label {
                        text: root.instance ? qsTr("%1 · Minecraft %2").arg(root.instance.loaderLabel).arg(root.instance.mcVersion) : ""
                        color: Theme.highlight; textFormat: Text.PlainText; font.pixelSize: 12; font.weight: Font.Medium; font.features: Theme.tabular
                    }
                    Label { text: "·"; color: Theme.dim }
                    Label {
                        text: qsTr("%1 proyectos activos").arg(root.activeModsCount)
                        color: Theme.dim; textFormat: Text.PlainText; font.pixelSize: 12; font.features: Theme.tabular
                    }
                    Label { text: "·"; color: Theme.dim }
                    Label {
                        text: root.instance && root.instance.lastPlayed
                              ? qsTr("%1 h jugadas · %2").arg(String((root.instance && root.instance.playtimeHours) || 0)).arg(root.instance.lastPlayed)
                              : qsTr("%1 h jugadas").arg(String((root.instance && root.instance.playtimeHours) || 0))
                        color: Theme.dim; textFormat: Text.PlainText; font.pixelSize: 12; font.features: Theme.tabular
                        Layout.fillWidth: true; elide: Text.ElideRight
                    }
                }
            }
            ActionButton {
                visible: !!root.instance && root.instance.category !== "vanilla" && root.width >= 980
                text: qsTr("Actualizar mods"); glyph: "compass"; compact: true
                enabled: root.instance !== null && !launcher.gameBusy && !launcher.catalogBusy
                onClicked: launcher.updateInstanceContent(root.instance.id)
            }
            ActionButton {
                glyph: "star"
                compact: true; selected: root.instance && !!root.instance.favorite
                ToolTip.text: qsTr("Cambiar favorito")
                Accessible.name: root.instance && root.instance.favorite ? qsTr("Quitar de favoritos") : qsTr("Añadir a favoritos")
                enabled: root.instance !== null && !launcher.gameBusy
                onClicked: launcher.toggleFavorite(root.instance.id)
            }
            ActionButton {
                text: qsTr("Jugar"); glyph: "play"; primary: true
                enabled: root.instance !== null && !launcher.gameBusy
                onClicked: launcher.playInstance(root.instance.id)
            }
        }
    }
    // Operación en curso: progreso real del motor, con Cancelar solo si se puede cancelar.
    Rectangle {
        id: operationBanner
        objectName: "operationBanner"
        Layout.fillWidth: true
        visible: root.operationRunning
        implicitHeight: operationColumn.implicitHeight + 28
        radius: Theme.radiusMd
        color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
        border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
        onVisibleChanged: if (visible && Theme.motion) bannerFade.restart()
        MotionNumber { id: bannerFade; role: "base"; target: operationBanner; property: "opacity"; from: 0; to: 1 }
        ColumnLayout {
            id: operationColumn
            anchors.fill: parent; anchors.margins: 14
            spacing: 10
            RowLayout {
                Layout.fillWidth: true
                spacing: 12
                Label {
                    Layout.fillWidth: true
                    textFormat: Text.PlainText; elide: Text.ElideRight
                    color: Theme.ink; font.weight: Font.DemiBold
                    text: launcher.operationMessage.length > 0 ? launcher.operationMessage
                        : launcher.operationKind === "modpack_update" ? qsTr("Actualizando el modpack…")
                        : launcher.operationKind === "modpack_restore" ? qsTr("Restaurando la copia…")
                        : launcher.operationKind === "modpack_link" ? qsTr("Vinculando el modpack…")
                        : launcher.operationKind === "modpack_export" ? qsTr("Exportando el modpack…")
                        : qsTr("Trabajando…")
                }
                Label {
                    visible: launcher.operationActive && launcher.operationPercent >= 0
                    text: qsTr("%1 %").arg(launcher.operationPercent)
                    color: Theme.highlight; font.features: Theme.tabular; font.weight: Font.DemiBold
                }
                ActionButton {
                    objectName: "operationCancel"
                    visible: launcher.operationActive && launcher.operationCancellable
                    text: qsTr("Cancelar"); compact: true
                    Accessible.name: "Cancelar la operación en curso"
                    onClicked: launcher.cancelOperation()
                }
            }
            ProgressBar {
                id: operationBar
                Layout.fillWidth: true
                from: 0; to: 100
                value: Math.max(0, launcher.operationPercent)
                indeterminate: !launcher.operationActive || launcher.operationPercent < 0
                Accessible.name: "Progreso de la operación"
                background: Rectangle { implicitHeight: 4; radius: 2; color: Theme.crystal ? Theme.rowFillHover : Theme.rim }
                contentItem: Item {
                    implicitHeight: 4
                    clip: true
                    Rectangle {
                        visible: !operationBar.indeterminate
                        height: parent.height; radius: 2; color: Theme.accent
                        width: parent.width * operationBar.visualPosition
                        Behavior on width { MotionNumber { role: "base" } }
                    }
                    Rectangle {
                        id: operationRunner
                        visible: operationBar.indeterminate
                        width: parent.width * 0.32; height: parent.height; radius: 2; color: Theme.accent
                        x: Theme.motion ? 0 : parent.width * 0.34
                        NumberAnimation on x {
                            running: Theme.motion && operationRunner.visible && operationBanner.visible
                            from: -operationRunner.width; to: operationBar.width
                            duration: 1300; loops: Animation.Infinite; easing.type: Easing.InOutCubic // motion-ok: bucle infinito gateado
                        }
                    }
                }
            }
        }
    }
    // Resultado de la última operación de modpack (actualizar, restaurar, vincular o exportar).
    Rectangle {
        id: resultBanner
        objectName: "operationResult"
        Layout.fillWidth: true
        visible: !root.operationRunning && !!root.packResult
        readonly property color tone: root.packResult ? (root.packResult.tone === "error" ? Theme.danger
            : root.packResult.tone === "ok" ? Theme.success : Theme.dim) : Theme.dim
        implicitHeight: resultColumn.implicitHeight + 24
        radius: Theme.radiusMd
        color: Qt.alpha(tone, 0.10)
        border.color: Qt.alpha(tone, 0.45)
        ColumnLayout {
            id: resultColumn
            anchors.fill: parent; anchors.margins: 12
            spacing: 6
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap; textFormat: Text.PlainText
                    color: Theme.ink
                    text: root.packResult ? root.packResult.text : ""
                }
                ActionButton {
                    glyph: "close"; compact: true
                    ToolTip.text: qsTr("Descartar este mensaje")
                    Accessible.name: "Descartar el resultado de la operación"
                    onClicked: root.packResult = null
                }
            }
            Label {
                Layout.fillWidth: true
                visible: !!root.packResult && root.packResult.detail.length > 0
                wrapMode: Text.WordWrap; textFormat: Text.PlainText
                color: root.packResult && root.packResult.warn ? Theme.warning : Theme.dim
                text: root.packResult ? root.packResult.detail : ""
            }
        }
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 6
        Repeater {
            model: ["General", qsTr("Contenido"), qsTr("Mundos"), qsTr("Capturas"), qsTr("Registro"), qsTr("Ajustes")]
            ActionButton {
                required property string modelData
                required property int index
                text: modelData; compact: true; selected: root.section === index
                Layout.fillWidth: true
                onClicked: root.selectSection(index)
            }
        }
    }
    GlassPanel {
        Layout.fillWidth: true
        implicitHeight: Math.max(150, (pane.item ? pane.item.implicitHeight : 0) + 36)
        padding: 18
        ParallelAnimation {
            id: tabTransition
            MotionNumber { role: "base"; target: pane; property: "opacity"; from: 0; to: 1 }
            MotionNumber { role: "base"; target: pane; property: "y"; from: 4; to: 0 }
        }
        Loader {
            id: pane
            width: parent.width
            height: item ? item.implicitHeight : 0
            sourceComponent: [overview, addons, filesPane, filesPane, logsPane, options][root.section]
            onLoaded: if (Theme.motion) tabTransition.restart()
        }
    }
    Component {
        id: overview
        ColumnLayout {
            spacing: 12
            Label { text: qsTr("Resumen y archivos de la instancia"); color: Theme.ink; font.pixelSize: 18; font.weight: Font.DemiBold }
            Label {
                visible: !!(root.instance && root.instance.modpackVersion)
                text: visible ? (root.instance.fabricVersion
                                 ? qsTr("Modpack %1 · Fabric Loader %2").arg(root.instance.modpackVersion).arg(root.instance.fabricVersion)
                                 : qsTr("Modpack %1").arg(root.instance.modpackVersion)) : ""
                textFormat: Text.PlainText
                color: Theme.highlight
                font.features: Theme.tabular
            }
            Label { text: qsTr("Abre la carpeta de esta instancia, crea una copia para pruebas o expórtala para compartirla."); color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            Flow {
                Layout.fillWidth: true
                spacing: 8
                ActionButton { text: qsTr("Abrir carpeta"); glyph: "folder"; compact: true; onClicked: launcher.openInstanceFolder(root.instance.id, "") }
                ActionButton {
                    text: launcher.copyingInstance ? qsTr("Copiando…") : qsTr("Duplicar instancia")
                    glyph: "layers"; compact: true
                    enabled: !!root.instance && !launcher.catalogBusy && !launcher.gameBusy
                    onClicked: launcher.duplicateInstance(root.instance.id)
                }
                ActionButton {
                    text: qsTr("Exportar instancia"); glyph: "arrow"; compact: true
                    enabled: !!root.instance && !launcher.catalogBusy && !launcher.gameBusy
                    onClicked: exportFolder.open()
                }
                ActionButton {
                    objectName: "exportMrpackButton"
                    text: qsTr("Exportar .mrpack…"); glyph: "arrow"; compact: true
                    enabled: root.canChangePack
                    onClicked: exportDialog.open()
                }
                ActionButton {
                    text: qsTr("Explorar catálogo"); glyph: "compass"; compact: true; primary: true
                    enabled: !!root.instance
                    onClicked: root.openContent(root.instance && root.instance.category === "vanilla" ? "resourcepack" : "mod")
                }
            }
            Label {
                visible: launcher.gameBusy
                text: qsTr("Cierra Minecraft para continuar")
                color: Theme.warning; font.weight: Font.DemiBold
            }
            // Versión del modpack: comprobar, cambiar, volver atrás o vincular una instancia antigua.
            Rectangle {
                id: packCard
                objectName: "modpackCard"
                Layout.fillWidth: true
                visible: root.hasPack
                implicitHeight: packColumn.implicitHeight + 28
                radius: Theme.radiusMd
                color: Theme.crystal ? Theme.rowFill : Theme.base
                border.color: root.updateAvailable ? Qt.alpha(Theme.success, 0.55) : (Theme.crystal ? Theme.glassBorder : Theme.border)
                ColumnLayout {
                    id: packColumn
                    anchors.fill: parent; anchors.margins: 14
                    spacing: 8
                    Label { text: qsTr("Versión del modpack"); color: Theme.ink; font.pixelSize: 15; font.weight: Font.DemiBold }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        BusyIndicator {
                            visible: root.packChecking
                            running: visible && Theme.motion
                            Layout.preferredWidth: 18; Layout.preferredHeight: 18
                        }
                        Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap; textFormat: Text.PlainText
                            color: root.packInfo.error && !root.packLinked ? Theme.warning : Theme.dim
                            text: {
                                const current = (root.packInfo.current && root.packInfo.current.versionNumber) || (root.instance ? root.instance.modpackVersion : "") || ""
                                if (root.packChecking) return qsTr("Buscando actualizaciones…")
                                if (root.packLegacy)
                                    return qsTr("Esta instancia viene de un modpack, pero OpenClient aún no sabe cuál es. Vincúlala para poder actualizarla.")
                                if (root.packLinked && root.packInfo.manifest === false)
                                    return qsTr("Falta la lista de archivos del modpack. Repara el vínculo para poder cambiar de versión.")
                                if (root.updateAvailable)
                                    return qsTr("Hay una versión nueva: %1. Tienes la %2.").arg(root.latestLabel).arg(root.versionName(current))
                                if (root.packInfo.error)
                                    return qsTr("No se pudo comprobar si hay versiones nuevas. %1").arg(root.packInfo.error)
                                if (root.packLinked && root.packInfo.source === "modrinth")
                                    return qsTr("Tienes la última versión (%1).").arg(root.versionName(current))
                                return qsTr("Versión instalada: %1.").arg(root.versionName(current))
                            }
                        }
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 8
                        ActionButton {
                            objectName: "modpackUpdateButton"
                            visible: root.updateAvailable && root.packInfo.manifest !== false
                            text: qsTr("Actualizar a %1").arg(root.latestLabel); glyph: "refresh"; compact: true; primary: true
                            enabled: root.canChangePack
                            onClicked: root.openVersionDialog(root.packInfo.latest.id)
                        }
                        ActionButton {
                            text: qsTr("Buscar actualización"); glyph: "search"; compact: true
                            visible: root.packLinked
                            enabled: !root.packChecking && !root.operationRunning
                            onClicked: launcher.checkModpack(root.instance.id, true)
                        }
                        ActionButton {
                            objectName: "modpackChangeButton"
                            text: qsTr("Cambiar versión…"); glyph: "layers"; compact: true
                            visible: root.packLinked && root.packInfo.manifest !== false
                            enabled: root.canChangePack
                            onClicked: root.openVersionDialog("")
                        }
                        ActionButton {
                            objectName: "modpackRestoreButton"
                            text: qsTr("Restaurar versión anterior…"); glyph: "refresh"; compact: true
                            visible: root.packLinked
                            enabled: root.canChangePack
                            onClicked: snapshotDialog.open()
                        }
                        ActionButton {
                            objectName: "modpackLinkButton"
                            text: root.packLegacy ? qsTr("Vincular a un modpack…") : qsTr("Reparar vínculo…")
                            glyph: "plus"; compact: true
                            visible: root.packLegacy || (root.packLinked && root.packInfo.manifest === false)
                            enabled: root.canChangePack
                            onClicked: linkDialog.open()
                        }
                    }
                }
            }
            Label {
                visible: text.length > 0
                text: launcher.copyMessage
                textFormat: Text.PlainText
                color: Theme.dim
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }
    NativeDialogs.FolderDialog {
        id: exportFolder
        title: qsTr("Elige dónde guardar la exportación")
        onAccepted: {
            if (root.instance) launcher.exportInstance(root.instance.id, selectedFolder)
        }
    }
    Component {
        id: addons
        ColumnLayout {
            id: addonList
            objectName: "contentView"
            spacing: 10
            readonly property var allContent: root.instance ? (root.instance.installedContent || []) : []
            readonly property var counts: launcher.content.counts
            readonly property int totalCount: counts.all || 0
            readonly property int visibleCount: launcher.content.visibleCount
            readonly property bool ready: !!root.instance && launcher.contentInstance === root.instance.id
            readonly property bool busy: launcher.catalogBusy || launcher.gameBusy
            readonly property bool filtering: launcher.content.query.trim().length > 0 || launcher.content.kind.length > 0
            property bool awaitingFeedback: false
            property string feedback: ""

            function changeProject(project, action, itemKind) {
                awaitingFeedback = true
                feedback = ""
                launcher.changeContent(root.instance.id, project, action, itemKind || root.contentKind)
            }

            function reload(force) {
                if (!root.instance) return
                launcher.loadContent(root.instance.id, force)
                refreshConflicts()
            }
            function refreshConflicts() {
                if (root.instance) launcher.contentConflicts(root.instance.id)
            }
            function titleOf(project) {
                const found = allContent.find(item => item.id === project)
                return found ? (found.title || found.id) : project
            }
            function chooseFilter(kind) {
                root.addonFilter = kind
                if (kind !== "all") root.contentKind = kind
                launcher.content.kind = kind === "all" ? "" : kind
            }
            readonly property bool hasConflicts: !!root.instance && launcher.conflictsInstance === root.instance.id && launcher.conflictCount > 0
            readonly property string statusText: {
                if (!ready || launcher.contentError.length > 0) return ""
                if (launcher.contentResolving)
                    return launcher.contentPending > 0 ? qsTr("Identificando archivos… quedan %1").arg(launcher.contentPending) : qsTr("Identificando archivos…")
                if (launcher.contentOffline) return qsTr("Sin conexión: se muestran los datos de los propios archivos.")
                if (launcher.contentTruncated) return qsTr("Hay demasiados archivos en una carpeta: solo se muestran los primeros 5 000.")
                if (totalCount === 0) return ""
                return filtering ? qsTr("%1 de %2 proyectos").arg(visibleCount).arg(totalCount) : qsTr("%1 proyectos").arg(totalCount)
            }
            Connections {
                target: root
                function onInstanceChanged() {
                    // La biblioteca se recarga tras cada cambio y vuelve a asignar la instancia: solo se carga si es otra.
                    if (root.instance && launcher.contentInstance !== root.instance.id) addonList.reload(false)
                    else addonList.refreshConflicts()
                }
            }
            Connections {
                target: launcher
                function onCatalogChanged() {
                    if (!launcher.catalogBusy) {
                        addonList.refreshConflicts()
                        if (addonList.awaitingFeedback) {
                            addonList.feedback = launcher.catalogMessage
                            addonList.awaitingFeedback = false
                        }
                    }
                }
            }
            ContentToolbar {
                id: toolbar
                Layout.fillWidth: true
                model: launcher.content
                filter: root.addonFilter === "all" ? "all" : root.contentKind
                canAdd: root.canBrowseContent
                busy: addonList.busy
                showUpdates: !!root.instance && root.instance.category !== "vanilla"
                canCheckUpdates: addonList.allContent.length > 0
                onFilterChosen: kind => addonList.chooseFilter(kind)
                onAddRequested: root.openContent(root.contentKind)
                onUpdatesRequested: {
                    addonList.awaitingFeedback = true
                    addonList.feedback = ""
                    launcher.updateInstanceContent(root.instance.id)
                }
                onRefreshRequested: addonList.reload(true)
                onFolderRequested: launcher.openInstanceFolder(root.instance.id, root.contentType.folder)
            }
            Label {
                visible: !root.canBrowseContent
                Layout.fillWidth: true; wrapMode: Text.WordWrap
                text: qsTr("El catálogo de mods y shaders requiere una instancia Fabric. Puedes añadir texturas a esta instancia Vanilla.")
                color: Theme.dim
            }
            Rectangle {
                objectName: "conflictBanner"
                Layout.fillWidth: true
                visible: addonList.hasConflicts
                implicitHeight: conflictColumn.implicitHeight + 24
                radius: Theme.radiusMd
                color: Qt.alpha(Theme.warning, 0.10)
                border.color: Qt.alpha(Theme.warning, 0.5)
                ColumnLayout {
                    id: conflictColumn
                    anchors.fill: parent; anchors.margins: 12
                    spacing: 5
                    Label {
                        Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                        color: Theme.ink; font.weight: Font.DemiBold
                        text: launcher.conflictCount === 1 ? qsTr("1 conflicto entre mods") : qsTr("%1 conflictos entre mods").arg(launcher.conflictCount)
                    }
                    Label {
                        Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                        color: Theme.dim
                        text: qsTr("Estos mods dicen no ser compatibles entre sí. Si los usas juntos, Minecraft puede fallar o cerrarse al abrirlo. Desactiva uno de cada pareja para evitarlo.")
                    }
                    Repeater {
                        model: launcher.conflicts.slice(0, 5)
                        Label {
                            required property var modelData
                            Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                            color: Theme.warning
                            text: qsTr("•  %1  y  %2").arg(modelData.aTitle || addonList.titleOf(modelData.a)).arg(modelData.bTitle || addonList.titleOf(modelData.b))
                        }
                    }
                    Label {
                        visible: launcher.conflictCount > 5
                        Layout.fillWidth: true; color: Theme.dim
                        text: qsTr("… y %1 más.").arg(launcher.conflictCount - 5)
                    }
                }
            }
            Label {
                visible: !!root.instance && launcher.conflictsInstance === root.instance.id && launcher.conflictsCacheMissing && addonList.allContent.length > 0
                Layout.fillWidth: true; wrapMode: Text.WordWrap
                color: Theme.dim; font.pixelSize: 11
                text: qsTr("Todavía no se puede comprobar si hay mods incompatibles entre sí. Se sabrá después de sincronizar los archivos o de jugar una vez.")
            }
            // Estado de la carga: la línea siempre ocupa su sitio para que la lista no salte.
            RowLayout {
                Layout.fillWidth: true
                Layout.preferredHeight: 16
                spacing: 8
                Rectangle {
                    id: statusDot
                    visible: launcher.contentResolving || launcher.contentOffline
                    Layout.preferredWidth: 6; Layout.preferredHeight: 6
                    radius: 3
                    color: launcher.contentResolving ? Theme.accent : Theme.warning
                    SequentialAnimation on opacity {
                        running: launcher.contentResolving && Theme.motion
                        loops: Animation.Infinite
                        onRunningChanged: if (!running) statusDot.opacity = 1
                        NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad } // motion-ok: pulso infinito gateado
                        NumberAnimation { to: 1; duration: 600; easing.type: Easing.InOutQuad } // motion-ok: pulso infinito gateado
                    }
                }
                Label {
                    objectName: "contentStatus"
                    Layout.fillWidth: true
                    text: addonList.statusText
                    textFormat: Text.PlainText
                    color: launcher.contentOffline && !launcher.contentResolving ? Theme.warning : Theme.dim
                    font.pixelSize: 11; font.features: Theme.tabular
                    elide: Text.ElideRight
                }
            }
            // La lista tiene altura propia (la página entera ya se desplaza) y solo crea las filas visibles.
            Rectangle {
                id: listFrame
                objectName: "contentListFrame"
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(maxHeight, Math.max(minHeight, addonList.totalCount * 64 + 2))
                readonly property int minHeight: 224
                readonly property int maxHeight: Math.max(320, Window.height - 300)
                readonly property bool loadingFirst: addonList.totalCount === 0 && (launcher.contentLoading || !addonList.ready)
                readonly property bool failed: !loadingFirst && addonList.totalCount === 0 && launcher.contentError.length > 0
                readonly property bool emptyAll: !loadingFirst && !failed && addonList.totalCount === 0
                readonly property bool emptyFilter: !loadingFirst && addonList.totalCount > 0 && addonList.visibleCount === 0
                radius: Theme.radiusMd
                color: Theme.crystal ? Theme.rowFill : Theme.base
                border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                clip: true

                ListView {
                    id: list
                    objectName: "contentList"
                    anchors.fill: parent
                    anchors.margins: 1
                    clip: true
                    model: addonList.ready ? launcher.content : null
                    reuseItems: true
                    cacheBuffer: 256
                    boundsBehavior: Flickable.StopAtBounds
                    interactive: contentHeight > height
                    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                    delegate: ContentRow {
                        width: ListView.view ? ListView.view.width : 0
                        busy: addonList.busy
                        onToggleRequested: addonList.changeProject(projectId, "content_toggle", kind)
                        onRemoveRequested: addonList.changeProject(projectId, "content_remove", kind)
                    }
                }

                // Esqueleto mientras llega la primera lista.
                Column {
                    id: skeleton
                    visible: listFrame.loadingFirst
                    anchors.left: parent.left; anchors.right: parent.right
                    anchors.top: parent.top; anchors.margins: 1
                    SequentialAnimation on opacity {
                        running: skeleton.visible && Theme.motion
                        loops: Animation.Infinite
                        onRunningChanged: if (!running) skeleton.opacity = 1
                        NumberAnimation { to: 0.5; duration: 700; easing.type: Easing.InOutQuad } // motion-ok: esqueleto infinito gateado
                        NumberAnimation { to: 1; duration: 700; easing.type: Easing.InOutQuad } // motion-ok: esqueleto infinito gateado
                    }
                    Repeater {
                        model: 3
                        Item {
                            width: skeleton.width; height: 64
                            Rectangle { anchors.left: parent.left; anchors.leftMargin: 14; anchors.verticalCenter: parent.verticalCenter; width: 48; height: 48; radius: Theme.radiusSm; color: Theme.crystal ? Theme.rowFillHover : Theme.soft; border.width: 1; border.color: Theme.crystal ? Theme.glassBorder : Theme.edge }
                            Rectangle { x: 76; y: 18; width: 120 + (index % 3) * 36; height: 12; radius: 4; color: Theme.crystal ? Theme.rowFillHover : Theme.soft }
                            Rectangle { x: 76; y: 38; width: 70; height: 10; radius: 4; color: Theme.crystal ? Theme.rowFill : Qt.alpha(Theme.ink, 0.06) }
                            Rectangle { visible: skeleton.width > 560; x: Math.round(skeleton.width * 0.52); y: 18; width: 90; height: 12; radius: 4; color: Theme.crystal ? Theme.rowFillHover : Theme.soft }
                            Rectangle { visible: skeleton.width > 560; x: Math.round(skeleton.width * 0.52); y: 38; width: 130; height: 10; radius: 4; color: Theme.crystal ? Theme.rowFill : Qt.alpha(Theme.ink, 0.06) }
                            Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 1; color: Theme.crystal ? Theme.divider : Theme.border; opacity: Theme.crystal ? 1 : 0.7 }
                        }
                    }
                }

                // Vacío, sin resultados o error.
                ColumnLayout {
                    visible: listFrame.emptyAll || listFrame.emptyFilter || listFrame.failed
                    anchors.centerIn: parent
                    width: Math.min(parent.width - 48, 380)
                    spacing: 8
                    Rectangle {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.preferredWidth: 48; Layout.preferredHeight: 48
                        radius: Theme.radiusMd
                        color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
                        border.width: 1
                        border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
                        Glyph {
                            anchors.centerIn: parent
                            width: 22; height: 22
                            name: listFrame.failed ? "alert" : listFrame.emptyFilter ? "search"
                                : root.addonFilter === "mod" ? "puzzle" : root.addonFilter === "resourcepack" ? "image"
                                : root.addonFilter === "shader" ? "sun" : "layers"
                            color: listFrame.failed ? Theme.warning : Theme.highlight
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap; textFormat: Text.PlainText
                        color: Theme.ink; font.pixelSize: 15; font.weight: Font.DemiBold
                        text: listFrame.failed ? qsTr("No se pudo leer el contenido")
                            : listFrame.emptyFilter ? qsTr("Ningún proyecto coincide")
                            : root.addonFilter === "mod" ? qsTr("Esta instancia no tiene mods")
                            : root.addonFilter === "resourcepack" ? qsTr("Esta instancia no tiene texturas")
                            : root.addonFilter === "shader" ? qsTr("Esta instancia no tiene shaders")
                            : qsTr("Esta instancia aún no tiene contenido")
                    }
                    Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap; textFormat: Text.PlainText
                        color: Theme.dim; font.pixelSize: 12
                        text: listFrame.failed ? launcher.contentError
                            : listFrame.emptyFilter ? qsTr("Prueba con otra búsqueda o quita los filtros para ver los %1 proyectos instalados.").arg(addonList.totalCount)
                            : qsTr("Usa «Añadir contenido» para explorar el catálogo, o copia archivos a la carpeta de la instancia y pulsa «Actualizar».")
                    }
                    ActionButton {
                        visible: listFrame.emptyFilter
                        Layout.alignment: Qt.AlignHCenter
                        Layout.topMargin: 4
                        text: qsTr("Quitar filtros"); glyph: "close"; compact: true
                        onClicked: toolbar.clearFilters()
                    }
                    ActionButton {
                        visible: listFrame.failed
                        Layout.alignment: Qt.AlignHCenter
                        Layout.topMargin: 4
                        text: qsTr("Reintentar"); glyph: "refresh"; compact: true
                        onClicked: addonList.reload(true)
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                ActionButton {
                    text: qsTr("Sincronizar archivos"); compact: true
                    enabled: !!root.instance && !launcher.catalogBusy && !launcher.gameBusy
                    onClicked: {
                        addonList.awaitingFeedback = true
                        addonList.feedback = ""
                        launcher.syncContent(root.instance.id)
                    }
                }
                Label {
                    Layout.fillWidth: true; wrapMode: Text.WordWrap
                    text: qsTr("Sincroniza para descargar o retirar los archivos en disco ahora, o inicia el juego para hacerlo automáticamente.")
                    color: Theme.dim; font.pixelSize: 11
                }
            }
            Label {
                visible: addonList.feedback.length > 0
                text: addonList.feedback; textFormat: Text.PlainText
                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.highlight
            }
            Component.onCompleted: {
                launcher.content.query = ""
                launcher.content.kind = root.addonFilter === "all" ? "" : root.contentKind
                reload(false)
            }
            Component.onDestruction: { if (launcher) launcher.releaseContent() }
        }
    }
    Component {
        id: filesPane
        ColumnLayout {
            spacing: 12
            RowLayout {
                Layout.fillWidth: true
                Label { text: root.section === 3 ? qsTr("Capturas locales") : qsTr("Mundos guardados"); color: Theme.ink; font.pixelSize: 18; font.weight: Font.DemiBold; Layout.fillWidth: true }
                ActionButton { text: qsTr("Actualizar"); compact: true; onClicked: root.selectSection(root.section) }
                ActionButton {
                    text: qsTr("Abrir carpeta"); glyph: "folder"; compact: true
                    onClicked: launcher.openInstanceFolder(root.instance.id, root.section === 3 ? "screenshots" : "worlds")
                }
            }
            Label { visible: root.files.length === 0; text: qsTr("Todavía no hay archivos en esta sección."); color: Theme.dim }
            GridLayout {
                Layout.fillWidth: true
                columns: root.section === 3 ? (root.width > 900 ? 4 : 3) : 1
                rowSpacing: 10
                columnSpacing: 10
                Repeater {
                    model: root.files
                    ColumnLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        Image {
                            visible: root.section === 3
                            Layout.fillWidth: true
                            Layout.preferredHeight: visible ? 105 : 0
                            source: visible ? (modelData.url || "") : ""
                            sourceSize.width: 340
                            sourceSize.height: 200
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                        }
                        Label { text: modelData.name; textFormat: Text.PlainText; color: Theme.ink; elide: Text.ElideRight; Layout.fillWidth: true }
                    }
                }
            }
            Loader {
                Layout.fillWidth: true
                active: root.section === 2 && !!root.instance
                visible: active
                sourceComponent: WorldBackups { instance: root.instance }
            }
        }
    }
    Component {
        id: logsPane
        ColumnLayout {
            spacing: 10
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Último registro"); color: Theme.ink; font.pixelSize: 18; font.weight: Font.DemiBold; Layout.fillWidth: true }
                ActionButton { text: qsTr("Actualizar"); compact: true; onClicked: root.selectSection(4) }
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: 255
                clip: true
                TextArea {
                    text: root.logText
                    readOnly: true
                    textFormat: TextEdit.PlainText
                    selectByMouse: true
                    wrapMode: TextEdit.WrapAnywhere
                    font.family: "Consolas"
                    color: Theme.ink
                }
            }
        }
    }
    Component {
        id: options
        ColumnLayout {
            id: settings
            readonly property int savedRam: root.instance ? root.instance.ramMb || 0 : 0
            readonly property int installedRam: launcher.hardware.installed_ram_mb || 0
            readonly property bool canSave: !!root.instance && !launcher.gameBusy && !launcher.catalogBusy
            spacing: 12
            Label { text: qsTr("Configuración de la instancia"); color: Theme.ink; font.pixelSize: 18; font.weight: Font.DemiBold }
            RowLayout {
                ActionButton { text: qsTr("Editar nombre o versión"); glyph: "settings"; enabled: !launcher.gameBusy; onClicked: root.edit() }
                ActionButton { text: qsTr("Abrir carpeta"); glyph: "folder"; onClicked: launcher.openInstanceFolder(root.instance.id, "") }
                ActionButton { text: qsTr("Retirar instancia"); enabled: !launcher.gameBusy; onClicked: root.retire() }
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
            Label { text: qsTr("Cuenta de esta instancia"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
            Label {
                text: qsTr("Elige con qué cuenta se abre solo esta instancia. Sin elegir, usa la cuenta activa del launcher.")
                color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true
            }
            GlassComboBox {
                id: accountChoice
                objectName: "instanceAccountChoice"
                readonly property var choices: {
                    const list = [{ label: qsTr("Cuenta activa del launcher"), kind: "", key: "" }]
                    for (const item of launcher.microsoftAccounts)
                        list.push({ label: item.status === "ok" ? qsTr("%1 · Microsoft").arg(item.username || qsTr("Cuenta dañada")) : qsTr("%1 · Microsoft (volver a conectar)").arg(item.username || qsTr("Cuenta dañada")), kind: "microsoft", key: item.id })
                    for (const name of launcher.offlineProfiles) list.push({ label: qsTr("%1 · Offline").arg(name), kind: "offline", key: name })
                    return list
                }
                readonly property var own: root.instance ? launcher.instanceAccounts[root.instance.id] : undefined
                Layout.fillWidth: true
                Layout.maximumWidth: 420
                Layout.preferredHeight: 34
                enabled: !!root.instance && !launcher.gameBusy
                model: choices
                textRole: "label"
                currentIndex: Math.max(0, choices.findIndex(item => own ? item.kind === own.kind && item.key === own.key : item.kind === ""))
                Accessible.name: "Cuenta de esta instancia"
                onActivated: index => launcher.setInstanceAccount(root.instance.id, choices[index].kind, choices[index].key)
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
            Label { text: qsTr("Java y memoria por instancia"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
            Label {
                text: qsTr("Memoria global configurada · %1 GB  ·  JVM: %2").arg((launcher.allocatedRamMb / 1024).toFixed(2)).arg(launcher.jvmFlagsPreview)
                color: Theme.dim
                font.features: Theme.tabular
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
            GlassSwitch {
                id: customMemory
                text: qsTr("Usar asignación de RAM propia en esta instancia")
                checked: settings.savedRam > 0
                enabled: settings.canSave && (settings.installedRam >= 1024 || settings.savedRam > 0)
            }
            RowLayout {
                Label { text: qsTr("Memoria en MB"); color: Theme.dim; visible: customMemory.checked }
                SpinBox {
                    id: instanceMemory
                    from: 1024
                    to: Math.max(1024, settings.installedRam)
                    value: settings.savedRam || launcher.allocatedRamMb
                    stepSize: 256
                    editable: true
                    visible: customMemory.checked
                    Accessible.name: "RAM de la instancia en megabytes"
                }
                ActionButton {
                    text: qsTr("Guardar memoria"); primary: true
                    enabled: settings.canSave && (!customMemory.checked || settings.installedRam >= 1024)
                    onClicked: {
                        if (launcher.setInstanceRam(root.instance.id, customMemory.checked ? instanceMemory.value : 0))
                            memoryFeedback.text = qsTr("Memoria guardada.")
                        else
                            memoryFeedback.text = launcher.libraryError
                    }
                }
            }
            Label {
                id: memoryFeedback
                visible: text.length > 0
                textFormat: Text.PlainText
                color: memoryFeedback.text === qsTr("Memoria guardada.") ? Theme.success : Theme.danger
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
            InstanceAdvanced { instance: root.instance; Layout.fillWidth: true }
        }
    }

    ModpackVersionDialog {
        id: versionDialog
        instance: root.instance
        info: root.packInfo
        onStarted: kind => root.trackOperation(kind)
    }
    SnapshotDialog {
        id: snapshotDialog
        instance: root.instance
        onStarted: kind => root.trackOperation(kind)
    }

    // Vincular una instancia antigua con el modpack del que salió (o con un archivo .mrpack).
    GlassDialog {
        id: linkDialog
        objectName: "modpackLinkDialog"
        property int pick: -1
        property bool loading: false
        property bool skipNext: false
        property string loadError: ""
        readonly property var candidates: launcher.linkCandidates
        readonly property var chosen: pick >= 0 && pick < candidates.length ? candidates[pick] : null
        title: root.packLegacy ? qsTr("Vincular a un modpack") : qsTr("Reparar vínculo del modpack")
        glyph: "layers"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(600, (parent ? parent.width : 660) - 48)
        onAboutToShow: {
            pick = -1
            loadError = ""
            if (!root.instance) return
            skipNext = true
            loading = true
            launcher.loadLinkCandidates(root.instance.id)
        }
        Connections {
            target: launcher
            function onLinkCandidatesChanged() {
                if (linkDialog.skipNext) { linkDialog.skipNext = false; return }
                linkDialog.loading = false
                linkDialog.loadError = ""
            }
            function onModpackResultChanged() {
                if (linkDialog.visible && !linkDialog.loading && linkDialog.candidates.length === 0)
                    linkDialog.loadError = launcher.modpackError
            }
        }
        contentItem: ColumnLayout {
            spacing: 12
            Label {
                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim
                text: qsTr("OpenClient compara los archivos de la instancia con los del modpack. Los que coinciden pasan a actualizarse con él; el resto se consideran tuyos y no se tocan.")
            }
            RowLayout {
                Layout.fillWidth: true
                visible: linkDialog.loading
                spacing: 10
                BusyIndicator { running: parent.visible && Theme.motion; Layout.preferredWidth: 22; Layout.preferredHeight: 22 }
                Label { text: qsTr("Buscando modpacks parecidos…"); color: Theme.dim; Layout.fillWidth: true }
            }
            Label {
                Layout.fillWidth: true
                visible: !linkDialog.loading && linkDialog.candidates.length === 0
                wrapMode: Text.WordWrap; textFormat: Text.PlainText
                color: linkDialog.loadError.length ? Theme.danger : Theme.dim
                text: linkDialog.loadError.length ? linkDialog.loadError
                    : qsTr("No encontramos ningún modpack parecido en Modrinth. Si tienes el archivo .mrpack original, puedes elegirlo.")
            }
            Rectangle {
                Layout.fillWidth: true
                visible: !linkDialog.loading && linkDialog.candidates.length > 0
                Layout.preferredHeight: Math.min(candidateList.contentHeight, 250) + 8
                radius: Theme.radiusMd
                color: Theme.crystal ? Theme.fieldFill : Theme.base
                border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                ListView {
                    id: candidateList
                    anchors.fill: parent; anchors.margins: 4
                    clip: true; spacing: 2
                    model: linkDialog.candidates
                    boundsBehavior: Flickable.StopAtBounds
                    Accessible.name: "Modpacks parecidos"
                    delegate: ItemDelegate {
                        id: candidate
                        required property var modelData
                        required property int index
                        readonly property bool usable: !!modelData.versionId
                        width: ListView.view.width
                        height: 60
                        padding: 0; leftPadding: 12; rightPadding: 12
                        hoverEnabled: true
                        enabled: usable
                        Accessible.name: usable ? (modelData.title || qsTr("Modpack")) : qsTr("%1, no se encontró tu versión").arg(modelData.title || qsTr("Modpack"))
                        Accessible.checkable: true
                        Accessible.checked: linkDialog.pick === index
                        onClicked: linkDialog.pick = index
                        background: Rectangle {
                            radius: Theme.radiusSm
                            color: Theme.crystal ? (linkDialog.pick === candidate.index ? Qt.alpha(Theme.accent, 0.16) : candidate.hovered ? Theme.rowFillHover : "transparent")
                                : linkDialog.pick === candidate.index ? Theme.soft : candidate.hovered ? Qt.alpha(Theme.ink, Theme.light ? 0.05 : 0.07) : "transparent"
                            border.width: candidate.activeFocus ? 2 : 1
                            border.color: candidate.activeFocus ? Theme.highlight
                                : linkDialog.pick === candidate.index ? (Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge) : "transparent"
                        }
                        contentItem: RowLayout {
                            spacing: 12
                            opacity: candidate.usable ? 1 : 0.55
                            ProjectIcon {
                                source: candidate.modelData.icon || ""
                                name: candidate.modelData.title || ""
                                kind: "modpack"
                                size: 38
                            }
                            ColumnLayout {
                                Layout.fillWidth: true; spacing: 2
                                Label {
                                    Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                                    text: candidate.modelData.title || qsTr("Modpack")
                                    color: Theme.ink; font.weight: Font.DemiBold
                                }
                                Label {
                                    Layout.fillWidth: true; elide: Text.ElideRight
                                    text: candidate.usable ? qsTr("Reconocimos la versión que tienes instalada") : qsTr("No se encontró tu versión exacta; no se puede vincular")
                                    color: Theme.dim; font.pixelSize: 11
                                }
                            }
                        }
                    }
                }
            }
            Label {
                Layout.fillWidth: true
                visible: launcher.gameBusy
                text: qsTr("Cierra Minecraft para continuar")
                color: Theme.warning; font.weight: Font.DemiBold
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                ActionButton {
                    text: qsTr("Elegir archivo .mrpack…"); glyph: "folder"; compact: true
                    enabled: root.canChangePack
                    onClicked: linkFile.open()
                }
                Item { Layout.fillWidth: true }
                ActionButton { text: qsTr("Cancelar"); onClicked: linkDialog.close() }
                ActionButton {
                    objectName: "modpackLinkApply"
                    text: qsTr("Vincular"); primary: true
                    enabled: !!linkDialog.chosen && !!linkDialog.chosen.versionId && root.canChangePack
                    onClicked: {
                        launcher.linkModpack(root.instance.id, linkDialog.chosen.project, linkDialog.chosen.versionId)
                        root.trackOperation("modpack_link")
                        linkDialog.close()
                    }
                }
            }
        }
    }
    NativeDialogs.FileDialog {
        id: linkFile
        title: qsTr("Elige el archivo del modpack")
        fileMode: NativeDialogs.FileDialog.OpenFile
        nameFilters: ["Modpack de Modrinth (*.mrpack)"]
        onAccepted: {
            if (!root.instance) return
            launcher.linkModpackFromFile(root.instance.id, selectedFile)
            root.trackOperation("modpack_link")
            linkDialog.close()
        }
    }

    // Exportar la instancia como .mrpack para compartirla o abrirla en otros launchers.
    GlassDialog {
        id: exportDialog
        objectName: "exportMrpackDialog"
        title: qsTr("Exportar como .mrpack")
        glyph: "arrow"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(560, (parent ? parent.width : 620) - 48)
        onAboutToShow: includeSaves.checked = false
        contentItem: ColumnLayout {
            spacing: 12
            Label {
                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim
                text: qsTr("Crea un archivo .mrpack con los mods, la configuración y los paquetes de recursos de la instancia. Los mods que están en Modrinth se enlazan en lugar de copiarse; los demás van dentro del archivo.")
            }
            Label {
                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim
                text: qsTr("Nunca se incluyen tus cuentas, registros ni capturas.")
            }
            CheckBox {
                id: includeSaves
                objectName: "exportIncludeSaves"
                text: qsTr("Incluir mundos (saves)")
                checked: false
                Accessible.name: "Incluir mundos (saves) en el archivo exportado"
            }
            Label {
                Layout.fillWidth: true; wrapMode: Text.WordWrap
                color: includeSaves.checked ? Theme.warning : Theme.dim
                font.pixelSize: 12
                text: includeSaves.checked
                    ? qsTr("Atención: los mundos pueden ocupar varios GB. El archivo será mucho más pesado y quien lo reciba tendrá tus partidas.")
                    : qsTr("Desactivado: el archivo será ligero y sin tus partidas. Actívalo solo si quieres compartir también tus mundos.")
            }
            Label {
                Layout.fillWidth: true
                visible: launcher.gameBusy
                text: qsTr("Cierra Minecraft para continuar")
                color: Theme.warning; font.weight: Font.DemiBold
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Item { Layout.fillWidth: true }
                ActionButton { text: qsTr("Cancelar"); onClicked: exportDialog.close() }
                ActionButton {
                    objectName: "exportMrpackChoose"
                    text: qsTr("Elegir destino…"); primary: true; glyph: "folder"
                    enabled: root.canChangePack
                    onClicked: exportFile.open()
                }
            }
        }
    }
    NativeDialogs.FileDialog {
        id: exportFile
        title: qsTr("Guardar el modpack exportado")
        fileMode: NativeDialogs.FileDialog.SaveFile
        defaultSuffix: "mrpack"
        nameFilters: ["Modpack de Modrinth (*.mrpack)"]
        currentFile: StandardPaths.writableLocation(StandardPaths.DocumentsLocation) + "/" + root.exportName + ".mrpack"
        onAccepted: {
            if (!root.instance) return
            launcher.exportMrpack(root.instance.id, selectedFile, includeSaves.checked)
            root.trackOperation("modpack_export")
            exportDialog.close()
        }
    }
}
