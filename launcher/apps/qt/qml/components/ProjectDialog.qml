import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

GlassDialog {
    id: dialog
    objectName: "projectDialog"
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(920, parent.width - 48)
    height: Math.min(700, parent.height - 48)
    title: qsTr("Proyecto de Modrinth")
    glyph: "layers"
    property string projectId: ""
    property string projectKind: "mod"
    property string targetId: ""
    property string targetName: ""
    property string initialTitle: ""
    property int section: 0
    property bool submitted: false
    property string selectedVersionId: ""
    readonly property var info: launcher.projectInfo.id === projectId ? launcher.projectInfo : ({})
    readonly property var versions: info.versions || []
    readonly property var gallery: info.gallery || []
    readonly property var chosenVersion: versions.find(item => item.id === selectedVersionId) ||
        versions.find(item => info.recommended && item.id === info.recommended.id) || info.recommended || ({})
    readonly property var dependencies: chosenVersion.dependencies || []
    readonly property var conflictList: targetInstance && projectKind !== "modpack" ? (chosenVersion.conflicts || []) : []
    readonly property var incompatibleList: chosenVersion.incompatible || []
    readonly property bool hasConflict: dependencies.some(item => dependencyConflict(item)) || conflictList.length > 0
    // Complementos opcionales de la versión elegida (nunca marcados de entrada).
    property var optionalPicked: ({})
    readonly property string chosenId: chosenVersion.id || ""
    readonly property var optionalList: projectKind === "modpack" || !targetInstance ? [] : (chosenVersion.optional || []).filter(item => !!item.project)
    readonly property var pickedOptional: optionalList.filter(item => optionalPicked[item.project] === true && !optionalPresent(item)).map(item => item.project)
    onChosenIdChanged: optionalPicked = ({})
    readonly property var targetInstance: launcher.instances.find(item => item.id === targetId) || null
    readonly property bool added: targetInstance !== null && (targetInstance.installedContent || []).some(item => item.id === projectId)
    readonly property bool ready: !launcher.projectBusy && !launcher.projectError && info.id === projectId

    function showProject(project, kind, instance) {
        if (!launcher.loadProject(instance ? instance.id : "", project.id, kind)) return
        projectId = project.id
        projectKind = kind
        targetId = instance ? instance.id : ""
        targetName = instance ? qsTr("%1 · Minecraft %2").arg(instance.name).arg(instance.mcVersion) : qsTr("Instancia nueva · Fabric")
        initialTitle = project.title || qsTr("Proyecto")
        section = 0
        bodyScroll.contentItem.contentY = 0
        submitted = false
        selectedVersionId = ""
        optionalPicked = ({})
        open()
    }

    function chooseVersion(id) {
        if (!versions.some(item => item.id === id) || launcher.catalogBusy) return
        selectedVersionId = id
        submitted = false
        section = 3
        bodyScroll.contentItem.contentY = 0
    }

    function dependencyMatch(dependency) {
        if (!targetInstance) return null
        return (targetInstance.installedContent || []).find(item =>
            dependency.version_id ? item.versionId === dependency.version_id :
            dependency.project_id && item.id === dependency.project_id) || null
    }

    function dependencyConflict(dependency) {
        const match = dependencyMatch(dependency)
        return dependency.dependency_type === "incompatible" && match !== null && match.enabled === true
    }

    function dependencyName(dependency) {
        const resolvedVersion = (info.dependencyVersions || {})[dependency.version_id] || ({})
        return (info.dependencyNames || {})[dependency.project_id || resolvedVersion.project] || dependency.file_name ||
            (dependency.project_id ? qsTr("Proyecto %1").arg(dependency.project_id) : qsTr("Versión %1").arg(dependency.version_id || qsTr("sin identificar")))
    }

    function dependencyVersion(dependency) {
        const version = (info.dependencyVersions || {})[dependency.version_id] || ({})
        return version.number ? qsTr("Versión: %1").arg(version.number) : qsTr("El autor requiere una versión específica.")
    }

    function dependencyState(dependency) {
        const labels = { required: qsTr("Obligatoria"), optional: qsTr("Opcional"), embedded: qsTr("Incluida"), incompatible: qsTr("Incompatible") }
        const label = labels[dependency.dependency_type] || qsTr("Sin clasificar")
        if (dependencyConflict(dependency)) return qsTr("%1 · Conflicto con contenido activo").arg(label)
        const match = dependencyMatch(dependency)
        if (match) return match.enabled ? qsTr("%1 · Ya añadida").arg(label) : qsTr("%1 · Añadida, desactivada").arg(label)
        return label
    }

    function optionalPresent(item) {
        return targetInstance !== null && (targetInstance.installedContent || []).some(entry => entry.id === item.project)
    }

    function setOptional(project, on) {
        const next = Object.assign({}, optionalPicked)
        next[project] = on
        optionalPicked = next
    }

    function conflictText(item) {
        const name = item.title || qsTr("Proyecto %1").arg(item.project)
        return item.reason === "reverse"
            ? qsTr("«%1», que ya tienes, declara que no funciona con este proyecto.").arg(name)
            : qsTr("«%1» ya está activo en tu instancia y esta versión declara que no funciona con él.").arg(name)
    }

    function chosenSummary() {
        const base = projectKind === "modpack"
            ? qsTr("Versión: %1 · Instancia nueva con Fabric.").arg(chosenVersion.number)
            : qsTr("Versión: %1 · Se sincronizará al jugar.").arg(chosenVersion.number)
        const count = pickedOptional.length
        const extra = count > 0
            ? (count === 1 ? qsTr("+%1 complemento opcional").arg(count) : qsTr("+%1 complementos opcionales").arg(count))
            : optionalList.length > 0 ? qsTr("Tiene complementos opcionales (en Dependencias).") : ""
        return extra.length > 0 ? base + " · " + extra : base
    }

    function installSelected() {
        if (!installButton.enabled) return
        submitted = true
        launcher.installProjectVersion(targetId, projectId, projectKind, chosenVersion.id, pickedOptional)
    }

    onSectionChanged: bodyScroll.contentItem.contentY = 0

    contentItem: ColumnLayout {
        spacing: 16
        RowLayout {
            Layout.fillWidth: true
            spacing: 14
            ProjectIcon {
                source: dialog.info.icon || ""
                name: dialog.info.title || dialog.initialTitle
                kind: dialog.projectKind
                size: 64
            }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 5
                Label {
                    Layout.fillWidth: true; text: dialog.info.title || dialog.initialTitle
                    textFormat: Text.PlainText; color: Theme.ink
                    font.pixelSize: 24; font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                Label {
                    Layout.fillWidth: true; text: dialog.targetName
                    textFormat: Text.PlainText; color: Theme.highlight; elide: Text.ElideRight
                }
            }
            ActionButton { text: qsTr("Cerrar"); compact: true; onClicked: dialog.close() }
        }
        Label {
            Layout.fillWidth: true; visible: text.length > 0
            text: dialog.info.description || ""; textFormat: Text.PlainText
            color: Theme.dim; wrapMode: Text.WordWrap; maximumLineCount: 3; elide: Text.ElideRight
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 6
            Repeater {
                model: ["Descripción", qsTr("Galería"), qsTr("Versiones"), qsTr("Dependencias")]
                ActionButton {
                    required property string modelData
                    required property int index
                    text: modelData; compact: true; primary: dialog.section === index
                    enabled: dialog.ready
                    onClicked: dialog.section = index
                }
            }
            Item { Layout.fillWidth: true }
            Label {
                visible: dialog.ready && dialog.width > 800
                text: qsTr("Licencia · %1").arg(dialog.info.license || qsTr("Sin especificar"))
                textFormat: Text.PlainText; color: Theme.dim; font.pixelSize: 11
                Layout.maximumWidth: 180; elide: Text.ElideRight
            }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.crystal ? Theme.divider : Theme.edge }
        ScrollView {
            id: bodyScroll
            Layout.fillWidth: true; Layout.fillHeight: true
            clip: true
            contentWidth: availableWidth
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                width: bodyScroll.availableWidth
                spacing: 12
                Label {
                    Layout.fillWidth: true
                    visible: launcher.projectBusy || launcher.projectError.length > 0
                    text: launcher.projectBusy ? qsTr("Consultando el proyecto y sus versiones…") : launcher.projectError
                    textFormat: Text.PlainText; color: launcher.projectError.length > 0 ? Theme.danger : Theme.dim; wrapMode: Text.WordWrap
                    Layout.topMargin: 18
                }
                ActionButton {
                    visible: !launcher.projectBusy && launcher.projectError.length > 0
                    text: qsTr("Reintentar"); compact: true
                    onClicked: launcher.loadProject(dialog.targetId, dialog.projectId, dialog.projectKind)
                }
                Label {
                    Layout.fillWidth: true
                    visible: dialog.ready && dialog.section === 0
                    text: dialog.info.body || qsTr("El autor no ha añadido una descripción ampliada.")
                    textFormat: Text.PlainText; color: Theme.ink; wrapMode: Text.Wrap
                    lineHeight: 1.35; font.pixelSize: 14
                    Layout.rightMargin: 16
                }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 16
                    visible: dialog.ready && dialog.section === 1
                    Label {
                        visible: dialog.gallery.length === 0
                        text: qsTr("Este proyecto todavía no tiene imágenes en su galería.")
                        Layout.fillWidth: true; color: Theme.dim; wrapMode: Text.WordWrap
                    }
                    Repeater {
                        model: dialog.section === 1 ? dialog.gallery : []
                        ColumnLayout {
                            required property var modelData
                            Layout.fillWidth: true; Layout.rightMargin: 16; spacing: 6
                            Rectangle {
                                Layout.fillWidth: true; implicitHeight: Math.min(330, width * 0.56)
                                radius: 14; color: Theme.crystal ? Theme.fieldFill : Theme.soft; border.color: Theme.crystal ? Theme.glassBorder : Theme.edge
                                Image {
                                    id: screenshot
                                    anchors.fill: parent; anchors.margins: 7
                                    source: modelData.url
                                    sourceSize: Qt.size(1280, 720)
                                    asynchronous: true; fillMode: Image.PreserveAspectFit
                                    opacity: status === Image.Ready ? 1 : 0
                                    Behavior on opacity { MotionNumber { role: "base" } }
                                }
                                Label {
                                    anchors.centerIn: parent
                                    visible: screenshot.status !== Image.Ready
                                    text: screenshot.status === Image.Error ? qsTr("Imagen no disponible") : qsTr("Cargando imagen…")
                                    color: Theme.dim
                                }
                            }
                            Label {
                                Layout.fillWidth: true; visible: text.length > 0
                                text: modelData.title || ""; textFormat: Text.PlainText
                                color: Theme.dim; wrapMode: Text.WordWrap
                            }
                        }
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 8
                    visible: dialog.ready && dialog.section === 2
                    Label {
                        Layout.fillWidth: true; color: Theme.dim; wrapMode: Text.WordWrap
                        text: dialog.versions.length === 0 ? qsTr("No hay versiones compatibles para este destino.") :
                            qsTr("Versiones compatibles · %1\nElige una versión para revisar sus dependencias antes de añadirla.").arg(dialog.info.versionCount)
                    }
                    Repeater {
                        model: dialog.section === 2 ? dialog.versions : []
                        Rectangle {
                            required property var modelData
                            readonly property bool recommended: dialog.info.recommended && modelData.id === dialog.info.recommended.id
                            Layout.fillWidth: true; Layout.rightMargin: 16
                            implicitHeight: versionLayout.implicitHeight + 24
                            radius: 12
                            color: recommended ? (Theme.crystal ? Qt.alpha(Theme.accent, 0.12) : Theme.soft) : (Theme.crystal ? Theme.rowFill : Theme.card)
                            border.color: Theme.crystal ? (recommended ? Qt.alpha(Theme.accentSoft, 0.4) : Theme.glassBorder) : Theme.edge
                            ColumnLayout {
                                id: versionLayout
                                anchors.fill: parent; anchors.margins: 12; spacing: 5
                                Label {
                                    Layout.fillWidth: true
                                    text: recommended ? qsTr("%1 · Recomendada").arg(modelData.name || modelData.number || qsTr("Versión")) : (modelData.name || modelData.number || qsTr("Versión"))
                                    textFormat: Text.PlainText; color: recommended ? Theme.highlight : Theme.ink
                                    font.weight: Font.DemiBold; wrapMode: Text.Wrap
                                }
                                Label {
                                    Layout.fillWidth: true
                                    text: (modelData.channel === "release" ? qsTr("Estable") : modelData.channel === "beta" ? qsTr("Beta") : qsTr("Alfa")) +
                                        " · " + (modelData.loaders || []).join(", ") + " · " + (modelData.date || "").slice(0, 10)
                                    textFormat: Text.PlainText; color: Theme.dim; font.pixelSize: 12; wrapMode: Text.WordWrap
                                }
                                Label {
                                    Layout.fillWidth: true; text: qsTr("Minecraft %1").arg((modelData.games || []).join(", "))
                                    textFormat: Text.PlainText; color: Theme.dim; font.pixelSize: 12; wrapMode: Text.Wrap
                                }
                                ActionButton {
                                    compact: true
                                    text: dialog.chosenVersion.id === modelData.id ? qsTr("Seleccionada · Revisar") : qsTr("Elegir y revisar")
                                    selected: dialog.chosenVersion.id === modelData.id
                                    enabled: !launcher.catalogBusy
                                    onClicked: dialog.chooseVersion(modelData.id)
                                }
                            }
                        }
                    }
                    Label {
                        visible: dialog.info.versionCount > dialog.versions.length
                        text: qsTr("Se muestran las primeras 100 versiones compatibles."); color: Theme.dim
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 12
                    visible: dialog.ready && dialog.section === 3
                    Label {
                        Layout.fillWidth: true; color: Theme.ink; font.weight: Font.DemiBold
                        text: dialog.chosenVersion.name || dialog.chosenVersion.number || qsTr("Sin versión compatible")
                        textFormat: Text.PlainText; wrapMode: Text.Wrap
                    }
                    Label {
                        Layout.fillWidth: true; color: Theme.dim; wrapMode: Text.WordWrap
                        text: qsTr("Requisitos declarados por esta versión. Esta lista no es un análisis completo de compatibilidad entre todos tus mods.")
                    }
                    Label {
                        Layout.fillWidth: true; color: Theme.dim; wrapMode: Text.WordWrap
                        text: dialog.projectKind === "mod" ? qsTr("Las dependencias obligatorias se resuelven al sincronizar. Las opcionales no se añaden automáticamente.") :
                            dialog.projectKind === "modpack" ? qsTr("Los archivos del modpack se comprueban durante la instalación. Esta lista solo muestra sus declaraciones en Modrinth.") :
                            qsTr("Revisa también las instrucciones del autor: los requisitos de shaders y texturas pueden necesitar instalación manual.")
                    }
                    ColumnLayout {
                        objectName: "optionalComplements"
                        visible: dialog.optionalList.length > 0
                        Layout.fillWidth: true; Layout.rightMargin: 16; spacing: 4
                        Label {
                            Layout.fillWidth: true; color: Theme.ink; font.weight: Font.DemiBold
                            text: qsTr("Complementos opcionales")
                        }
                        Label {
                            Layout.fillWidth: true; color: Theme.dim; wrapMode: Text.WordWrap; font.pixelSize: 12
                            text: qsTr("El autor los recomienda, pero el mod funciona sin ellos. Marca los que quieras añadir junto con esta versión.")
                        }
                        Repeater {
                            model: dialog.optionalList
                            CheckBox {
                                id: optionalBox
                                required property var modelData
                                readonly property bool present: dialog.optionalPresent(modelData)
                                Layout.fillWidth: true
                                enabled: !present && !launcher.catalogBusy
                                checked: present || dialog.optionalPicked[modelData.project] === true
                                text: present ? qsTr("%1 · Ya añadido").arg(modelData.title || qsTr("Proyecto %1").arg(modelData.project))
                                    : (modelData.title || qsTr("Proyecto %1").arg(modelData.project))
                                Accessible.name: qsTr("Añadir también el complemento opcional %1").arg(modelData.title || modelData.project)
                                onToggled: dialog.setOptional(modelData.project, checked)
                                contentItem: Label {
                                    leftPadding: optionalBox.indicator.width + optionalBox.spacing
                                    text: optionalBox.text; textFormat: Text.PlainText
                                    color: Theme.ink; wrapMode: Text.WordWrap
                                    verticalAlignment: Text.AlignVCenter
                                    opacity: optionalBox.enabled ? 1 : 0.6
                                }
                            }
                        }
                    }
                    ColumnLayout {
                        objectName: "incompatibleNotice"
                        visible: dialog.incompatibleList.length > 0
                        Layout.fillWidth: true; Layout.rightMargin: 16; spacing: 4
                        Label {
                            Layout.fillWidth: true; color: Theme.warning; font.weight: Font.DemiBold
                            text: qsTr("Incompatible con otros mods")
                        }
                        Label {
                            Layout.fillWidth: true; color: Theme.dim; wrapMode: Text.WordWrap; font.pixelSize: 12
                            textFormat: Text.PlainText
                            text: qsTr("El autor declara que esta versión no funciona junto a: %1. Evita añadirlos después.")
                                .arg(dialog.incompatibleList.map(item => item.title || qsTr("Proyecto %1").arg(item.project)).join(", "))
                        }
                    }
                    Label {
                        visible: dialog.dependencies.length === 0
                        Layout.fillWidth: true; color: Theme.dim; wrapMode: Text.WordWrap
                        text: qsTr("No se han declarado dependencias para esta versión.")
                    }
                    Repeater {
                        model: dialog.section === 3 ? dialog.dependencies : []
                        ColumnLayout {
                            required property var modelData
                            Layout.fillWidth: true; Layout.rightMargin: 16; spacing: 5
                            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.crystal ? Theme.divider : Theme.edge }
                            Label {
                                Layout.fillWidth: true; Layout.topMargin: 7
                                text: dialog.dependencyName(modelData); textFormat: Text.PlainText
                                color: Theme.ink; font.weight: Font.DemiBold; wrapMode: Text.Wrap
                            }
                            Label {
                                Layout.fillWidth: true
                                text: dialog.dependencyState(modelData); textFormat: Text.PlainText
                                color: dialog.dependencyConflict(modelData) ? Theme.danger : Theme.highlight; wrapMode: Text.WordWrap
                            }
                            Label {
                                visible: !!modelData.version_id; Layout.fillWidth: true
                                text: dialog.dependencyVersion(modelData)
                                textFormat: Text.PlainText; color: Theme.dim; font.pixelSize: 12; wrapMode: Text.Wrap
                            }
                        }
                    }
                }
            }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.crystal ? Theme.divider : Theme.edge }
        Rectangle {
            objectName: "conflictBanner"
            visible: dialog.ready && dialog.conflictList.length > 0
            Layout.fillWidth: true
            implicitHeight: conflictBody.implicitHeight + 24
            radius: Theme.radiusMd
            color: Qt.alpha(Theme.danger, Theme.light ? 0.10 : 0.14)
            border.color: Qt.alpha(Theme.danger, 0.55)
            Accessible.role: Accessible.AlertMessage
            Accessible.name: "Conflicto: no se puede añadir esta versión a la instancia"
            ColumnLayout {
                id: conflictBody
                anchors.fill: parent; anchors.margins: 12; spacing: 4
                Label {
                    Layout.fillWidth: true; color: Theme.danger; font.weight: Font.DemiBold; wrapMode: Text.WordWrap
                    text: dialog.conflictList.length === 1 ? qsTr("Esta versión choca con un mod que ya tienes")
                        : qsTr("Esta versión choca con %1 mods que ya tienes").arg(dialog.conflictList.length)
                }
                Repeater {
                    model: dialog.conflictList.slice(0, 4)
                    Label {
                        required property var modelData
                        Layout.fillWidth: true; color: Theme.ink; wrapMode: Text.WordWrap; font.pixelSize: 12
                        textFormat: Text.PlainText
                        text: "• " + dialog.conflictText(modelData)
                    }
                }
                Label {
                    visible: dialog.conflictList.length > 4
                    Layout.fillWidth: true; color: Theme.dim; font.pixelSize: 12
                    text: qsTr("y %1 más.").arg(dialog.conflictList.length - 4)
                }
                Label {
                    Layout.fillWidth: true; color: Theme.dim; wrapMode: Text.WordWrap; font.pixelSize: 12
                    text: qsTr("Para añadirla, quita o desactiva ese contenido, o elige otra versión.")
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 14
            Label {
                Layout.fillWidth: true; color: dialog.hasConflict ? Theme.danger : Theme.dim; wrapMode: Text.WordWrap
                textFormat: Text.PlainText; font.pixelSize: 12
                text: dialog.submitted ? (launcher.catalogBusy ? qsTr("Procesando…") : launcher.catalogMessage) :
                    dialog.conflictList.length > 0 ? qsTr("No se puede añadir: choca con contenido de tu instancia.") :
                    dialog.hasConflict ? qsTr("Hay una incompatibilidad declarada con contenido activo. Revísala en Dependencias.") :
                    dialog.ready && dialog.versions.length === 0 ? qsTr("No hay versiones compatibles para este destino.") :
                    dialog.added ? qsTr("Este proyecto ya está en tu instancia.") :
                    dialog.chosenVersion.id ? dialog.chosenSummary() : qsTr("Cargando la selección…")
            }
            ActionButton {
                id: installButton
                text: dialog.projectKind === "modpack" ? qsTr("Instalar versión") : dialog.added ? qsTr("Añadido") : qsTr("Añadir versión")
                primary: !dialog.added
                enabled: dialog.ready && !!dialog.chosenVersion.id && !dialog.added && !dialog.hasConflict &&
                    (dialog.projectKind === "modpack" || dialog.targetInstance !== null) && !launcher.catalogBusy && !launcher.gameBusy
                onClicked: dialog.installSelected()
            }
        }
    }
}
