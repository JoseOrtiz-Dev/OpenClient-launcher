import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

ColumnLayout {
    id: root
    objectName: "catalogPage"
    property string preferredInstanceId: ""
    property string preferredKind: "modpack"
    property bool autoSearch: true
    property string chosenInstanceId: ""
    property string kind: "modpack"
    property string category: ""
    property string submittedQuery: ""
    property bool gridView: false
    property bool inventoryOpen: false
    property bool waitingToSearch: false
    spacing: 12

    ProjectDialog { id: projectDialog }

    function openProject(project) {
        projectDialog.showProject(project, kind, browsingPacks ? null : instance)
    }

    readonly property bool browsingPacks: kind === "modpack"
    readonly property var kinds: [
        { id: "modpack", label: qsTr("Modpacks"), glyph: "layers", description: qsTr("Paquetes completos listos para jugar") },
        { id: "mod", label: qsTr("Mods"), glyph: "puzzle", description: qsTr("Mods compatibles con tu instancia") },
        { id: "resourcepack", label: qsTr("Texturas"), glyph: "image", description: qsTr("Paquetes de recursos") },
        { id: "shader", label: qsTr("Shaders"), glyph: "sun", description: qsTr("Paquetes para Iris / Oculus") }
    ]
    readonly property var selectedKind: kinds.find(item => item.id === kind) || kinds[0]
    readonly property var filters: ({
        modpack: [["adventure", qsTr("Aventura")], ["lightweight", qsTr("Ligero")], ["optimization", qsTr("Optimización")], ["multiplayer", qsTr("Multijugador")], ["technology", qsTr("Tecnología")], ["magic", qsTr("Magia")]],
        mod: [["adventure", qsTr("Aventura")], ["optimization", qsTr("Optimización")], ["utility", qsTr("Utilidades")], ["technology", qsTr("Tecnología")], ["magic", qsTr("Magia")]],
        resourcepack: [["vanilla-like", qsTr("Estilo Vanilla")], ["realistic", qsTr("Realista")], ["simplistic", qsTr("Simple")], ["themed", qsTr("Temático")]],
        shader: [["fantasy", qsTr("Fantasía")], ["realistic", qsTr("Realista")], ["cartoon", qsTr("Caricatura")], ["potato", qsTr("Ligero")]]
    })
    readonly property var targets: launcher.instances.filter(item => ["fabric", "quilt", "forge", "neoforge", "vanilla"].includes(item.category))
    readonly property var instance: targets.find(item => item.id === chosenInstanceId) || targets[0] || null
    readonly property bool compatible: browsingPacks || (instance !== null && (kind === "resourcepack" || instance.category !== "vanilla"))
    readonly property var content: !browsingPacks && instance
                                   ? (instance.installedContent || []).filter(item => (item.type || "mod") === kind) : []
    readonly property bool matching: launcher.catalogKind === kind
                                     && (browsingPacks || (instance && launcher.catalogInstance === instance.id))
                                     && launcher.catalogFilters.category === category
                                     && launcher.catalogFilters.sort === sort.currentValue
                                     && launcher.catalogFilters.query === submittedQuery
    readonly property var results: matching ? launcher.catalogResults : []

    Component.onCompleted: configure(preferredKind, preferredInstanceId)
    Connections {
        target: launcher
        function onCatalogChanged() {
            if (!launcher.catalogBusy && root.waitingToSearch) {
                root.waitingToSearch = false
                Qt.callLater(root.search, 0)
            }
        }
    }

    function configure(initialKind, id) {
        chosenInstanceId = id
        query.clear()
        selectKind(initialKind === "mod" && instance && instance.category === "vanilla" ? "resourcepack" : initialKind)
    }

    function selectKind(value) {
        if (!kinds.some(item => item.id === value)) return
        kind = value
        category = ""
        if (autoSearch) {
            if (launcher.catalogBusy) waitingToSearch = true
            else search(0)
        }
    }

    function search(page) {
        if (!compatible || launcher.catalogBusy) return
        if (page === 0) submittedQuery = query.text.trim()
        launcher.searchCatalog(browsingPacks ? "" : instance.id, submittedQuery, kind, category, sort.currentValue, page)
    }

    function downloadCount(count) {
        if (count >= 1000000) return qsTr("%1 M").arg((count / 1000000).toFixed(1))
        if (count >= 1000) return qsTr("%1 mil").arg((count / 1000).toFixed(1))
        return String(count || 0)
    }

    function categoryName(tag) {
        const match = (filters[kind] || []).find(item => item[0] === tag)
        return match ? match[1] : tag
    }

    // Desplegable del catálogo: GlassComboBox con el alto de 38 px y la letra de 12 px de siempre.
    component CatalogCombo: GlassComboBox {
        implicitHeight: 38
        popupMaxHeight: 260
        font.pixelSize: 12
    }

    GlassPanel {
        Layout.fillWidth: true
        implicitHeight: root.width >= 860 ? 78 : 64
        padding: 6
        RowLayout {
            anchors.fill: parent
            spacing: 6
            Repeater {
                model: root.kinds
                Button {
                    id: kindButton
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                    hoverEnabled: true
                    checked: root.kind === modelData.id
                    Accessible.name: modelData.label
                    ToolTip.visible: hovered
                    ToolTip.delay: 500
                    ToolTip.text: modelData.description
                    onClicked: if (root.kind !== modelData.id) root.selectKind(modelData.id)
                    background: Rectangle {
                        radius: Theme.radiusMd
                        color: Theme.crystal ? (kindButton.checked ? Qt.alpha(Theme.accent, 0.16) : kindButton.hovered ? Theme.rowFillHover : "transparent")
                            : kindButton.checked ? Theme.soft : kindButton.hovered ? Theme.surface : "transparent"
                        border.width: kindButton.activeFocus ? 2 : 1
                        border.color: Theme.crystal
                            ? (kindButton.activeFocus ? Theme.fieldBorderFocus : kindButton.checked ? Qt.alpha(Theme.accentSoft, 0.35) : kindButton.hovered ? Theme.glassBorder : "transparent")
                            : kindButton.checked || kindButton.activeFocus ? Theme.highlight : kindButton.hovered ? Theme.border : "transparent"
                        Behavior on color { MotionColor { role: "fast" } }
                    }
                    contentItem: RowLayout {
                        spacing: 10
                        Glyph {
                            name: kindButton.modelData.glyph
                            Layout.preferredWidth: 22; Layout.preferredHeight: 22
                            color: kindButton.checked ? Theme.highlight : Theme.dim
                        }
                        ColumnLayout {
                            Layout.fillWidth: true; spacing: 3
                            Label {
                                Layout.fillWidth: true; elide: Text.ElideRight
                                text: kindButton.modelData.label
                                color: kindButton.checked ? Theme.highlight : Theme.ink
                                font.pixelSize: 14; font.weight: Font.DemiBold
                            }
                            Label {
                                visible: root.width >= 860
                                Layout.fillWidth: true; elide: Text.ElideRight
                                text: kindButton.modelData.description
                                color: Theme.dim; font.pixelSize: 11
                            }
                        }
                    }
                }
            }
        }
    }
    GlassPanel {
        Layout.fillWidth: true
        implicitHeight: 62
        padding: 10
        accented: true
        RowLayout {
            anchors.fill: parent
            spacing: 10
            Glyph { name: "search"; color: Theme.dim; Layout.leftMargin: 5 }
            GlassField {
                id: query
                Layout.fillWidth: true
                Layout.preferredHeight: 38
                implicitHeight: 38
                placeholderText: qsTr("Buscar %1 en Modrinth").arg(root.selectedKind.label.toLowerCase())
                maximumLength: 100
                enabled: !launcher.catalogBusy && root.compatible
                Accessible.name: placeholderText
                onAccepted: root.search(0)
            }
            ActionButton {
                text: qsTr("Buscar"); glyph: "search"; primary: true
                enabled: root.compatible && !launcher.catalogBusy
                onClicked: root.search(0)
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 14
        GlassPanel {
            Layout.preferredWidth: 190
            Layout.alignment: Qt.AlignTop
            implicitHeight: sidebar.implicitHeight + 28
            padding: 14
            ColumnLayout {
                id: sidebar
                anchors.fill: parent
                spacing: 7
                Label { text: qsTr("Filtros"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
                Label { text: qsTr("Proveedor"); color: Theme.dim; font.pixelSize: 12; Layout.topMargin: 3 }
                Label {
                    text: qsTr("Modrinth")
                    color: Theme.ink
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                }
                Label {
                    Layout.fillWidth: true
                    objectName: "catalogCurseforgeStatus"
                    text: qsTr("CurseForge · Próximamente")
                    color: Theme.dim
                    font.pixelSize: 12
                    wrapMode: Text.WordWrap
                }
                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border; Layout.topMargin: 4 }
                Label { text: qsTr("Categorías"); color: Theme.dim; font.pixelSize: 12; Layout.topMargin: 4 }
                Repeater {
                    model: [["", qsTr("Todas")]].concat(root.filters[root.kind] || [])
                    ActionButton {
                        required property var modelData
                        text: modelData[1]
                        Layout.fillWidth: true
                        compact: true
                        selected: root.category === modelData[0]
                        enabled: root.compatible && !launcher.catalogBusy
                        onClicked: {
                            if (root.category === modelData[0]) return
                            root.category = modelData[0]
                            root.search(0)
                        }
                    }
                }
                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                Label { text: qsTr("Ordenar por"); color: Theme.dim; font.pixelSize: 12; Layout.topMargin: 4 }
                CatalogCombo {
                    id: sort
                    objectName: "catalogSort"
                    Layout.fillWidth: true
                    model: [
                        { id: "", label: qsTr("Recomendados") },
                        { id: "downloads", label: qsTr("Más descargas") },
                        { id: "newest", label: qsTr("Más recientes") },
                        { id: "updated", label: qsTr("Actualizados") }
                    ]
                    textRole: "label"; valueRole: "id"
                    enabled: root.compatible && !launcher.catalogBusy
                    Accessible.name: "Ordenar proyectos"
                    onActivated: root.search(0)
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            spacing: 9
            RowLayout {
                Layout.fillWidth: true
                Label {
                    Layout.fillWidth: true
                    text: root.matching ? qsTr("%1 proyectos").arg(launcher.catalogTotal) : qsTr("Explorar %1").arg(root.kinds.find(item => item.id === root.kind).label.toLowerCase())
                    color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold; font.features: Theme.tabular
                }
                ActionButton {
                    compact: true; glyph: "list"; selected: !root.gridView || root.width < 880
                    ToolTip.text: qsTr("Vista de lista")
                    onClicked: root.gridView = false
                }
                ActionButton {
                    compact: true; glyph: "grid"; selected: root.gridView && root.width >= 880
                    enabled: root.width >= 880
                    ToolTip.text: qsTr("Vista de cuadrícula")
                    onClicked: root.gridView = true
                }
            }
            RowLayout {
                visible: !root.browsingPacks
                Layout.fillWidth: true
                Label { text: qsTr("Instancia de destino"); color: Theme.dim }
                CatalogCombo {
                    Layout.fillWidth: true
                    model: root.targets; textRole: "name"
                    currentIndex: root.targets.findIndex(item => root.instance && item.id === root.instance.id)
                    enabled: !launcher.catalogBusy
                    Accessible.name: "Instancia de destino"
                    onActivated: index => {
                        root.chosenInstanceId = root.targets[index].id
                        if (root.instance.category === "vanilla" && root.kind !== "resourcepack")
                            root.selectKind("resourcepack")
                        else root.search(0)
                    }
                }
                ActionButton {
                    compact: true; text: qsTr("Contenido instalado")
                    enabled: root.instance !== null
                    selected: root.inventoryOpen
                    onClicked: root.inventoryOpen = !root.inventoryOpen
                }
            }
            GlassPanel {
                visible: root.inventoryOpen && !root.browsingPacks && root.instance !== null
                Layout.fillWidth: true
                implicitHeight: inventory.implicitHeight + 26
                padding: 13
                ColumnLayout {
                    id: inventory
                    anchors.fill: parent
                    Label { text: root.content.length ? qsTr("Contenido añadido") : qsTr("Todavía no hay contenido de este tipo."); color: Theme.ink }
                    Repeater {
                        model: root.content
                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            Label { Layout.fillWidth: true; text: modelData.title || ""; textFormat: Text.PlainText; color: Theme.ink; elide: Text.ElideRight }
                            ActionButton {
                                compact: true; text: modelData.enabled ? qsTr("Desactivar") : qsTr("Activar")
                                enabled: !launcher.catalogBusy && !launcher.gameBusy
                                onClicked: launcher.changeContent(root.instance.id, modelData.id, "content_toggle", root.kind)
                            }
                            ActionButton {
                                compact: true; text: qsTr("Quitar")
                                enabled: !launcher.catalogBusy && !launcher.gameBusy
                                onClicked: launcher.changeContent(root.instance.id, modelData.id, "content_remove", root.kind)
                            }
                        }
                    }
                    ActionButton {
                        compact: true; text: qsTr("Sincronizar archivos")
                        enabled: root.instance !== null && !launcher.catalogBusy && !launcher.gameBusy
                        onClicked: launcher.syncContent(root.instance.id)
                    }
                }
            }
            OperationBar { Layout.fillWidth: true }
            Label {
                Layout.fillWidth: true
                visible: text.length > 0 && !launcher.operationActive
                text: launcher.catalogBusy ? qsTr("Cargando…") :
                      !root.compatible ? qsTr("Elige una instancia compatible con este tipo de contenido.") :
                      launcher.catalogMessage || (root.kind === "shader" ? qsTr("Instala Iris y selecciona el shader dentro de Minecraft.") : "")
                color: Theme.dim
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }

            GridLayout {
                Layout.fillWidth: true
                columns: root.gridView && root.width >= 880 ? 2 : 1
                columnSpacing: 9; rowSpacing: 9
                Repeater {
                    model: root.results
                    GlassPanel {
                        id: card
                        required property var modelData
                        readonly property bool added: root.content.some(item => item.id === modelData.id)
                        readonly property bool isCf: modelData.provider === "curseforge"
                        readonly property var tags: (modelData.categories || []).filter(tag => !["fabric", "forge", "quilt", "neoforge", "iris"].includes(tag)).slice(0, 3)
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        implicitHeight: columnsLayout.implicitHeight + 26
                        padding: 13
                        border.color: Theme.crystal ? (cardHover.hovered ? Theme.glassBorderHover : "transparent")
                            : cardHover.hovered ? Theme.highlight : Theme.rim
                        Behavior on border.color { MotionColor { role: "fast" } }
                        HoverHandler { id: cardHover }
                        ColumnLayout {
                            id: columnsLayout
                            anchors.fill: parent
                            spacing: 8
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 10
                                ProjectIcon {
                                    source: card.modelData.icon || ""
                                    name: card.modelData.title || ""
                                    kind: root.kind
                                    size: 48
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 3
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 8
                                        Label { Layout.fillWidth: true; text: card.modelData.title || ""; textFormat: Text.PlainText; color: Theme.ink; elide: Text.ElideRight; font.pixelSize: 15; font.weight: Font.DemiBold }
                                        Rectangle {
                                            radius: 4
                                            color: Qt.alpha(card.isCf ? Theme.warning : Theme.success, 0.14)
                                            border.color: Qt.alpha(card.isCf ? Theme.warning : Theme.success, 0.36)
                                            implicitWidth: providerTag.implicitWidth + 10
                                            implicitHeight: 18
                                            Label {
                                                id: providerTag
                                                anchors.centerIn: parent
                                                text: card.isCf ? qsTr("CurseForge") : qsTr("Modrinth")
                                                color: card.isCf ? Theme.warning : Theme.success
                                                font.pixelSize: 10; font.weight: Font.Medium
                                            }
                                        }
                                    }
                                    Label {
                                        Layout.fillWidth: true; text: card.modelData.description || ""; textFormat: Text.PlainText
                                        color: Theme.dim; elide: Text.ElideRight; wrapMode: Text.WordWrap; maximumLineCount: 2
                                    }
                                }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                Label {
                                    Layout.fillWidth: true
                                    text: qsTr("%1  ·  %2 descargas").arg(card.modelData.author || (card.isCf ? qsTr("CurseForge") : qsTr("Modrinth"))).arg(root.downloadCount(card.modelData.downloads))
                                    textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.dim; font.pixelSize: 11; font.features: Theme.tabular
                                }
                                ActionButton {
                                    text: qsTr("Ver proyecto"); compact: true
                                    enabled: !launcher.projectBusy
                                    onClicked: root.openProject(card.modelData)
                                }
                                ActionButton {
                                    text: root.browsingPacks ? qsTr("Instalar") : card.added ? qsTr("Añadido") : qsTr("Añadir")
                                    compact: true; primary: !card.added
                                    enabled: root.compatible && !card.added && !(root.browsingPacks && card.isCf) && !launcher.catalogBusy && !launcher.gameBusy
                                    onClicked: {
                                        if (root.browsingPacks) launcher.installModpack(card.modelData.id)
                                        else launcher.changeContent(root.instance.id, card.modelData.id, "content_add", root.kind)
                                    }
                                }
                            }
                            Flow {
                                visible: card.tags.length > 0
                                Layout.fillWidth: true
                                spacing: 5
                                Repeater {
                                    model: card.tags
                                    Rectangle {
                                        required property string modelData
                                        implicitWidth: tagText.implicitWidth + 14
                                        implicitHeight: tagText.implicitHeight + 6
                                        radius: Theme.radiusSm - 2; color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
                                        Label { id: tagText; anchors.centerIn: parent; text: root.categoryName(modelData); textFormat: Text.PlainText; color: Theme.highlight; font.pixelSize: 10 }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            RowLayout {
                visible: root.matching && launcher.catalogTotal > 24
                Layout.alignment: Qt.AlignHCenter
                spacing: 6
                readonly property int maxPage: Math.min(100, Math.ceil(launcher.catalogTotal / 24) - 1)
                readonly property int startPage: Math.max(0, Math.min(launcher.catalogPage - 2, maxPage - 4))
                ActionButton {
                    compact: true; text: qsTr("Anterior")
                    enabled: launcher.catalogPage > 0 && !launcher.catalogBusy
                    onClicked: root.search(launcher.catalogPage - 1)
                }
                Repeater {
                    model: Math.min(5, Math.max(1, parent.maxPage + 1))
                    ActionButton {
                        required property int index
                        readonly property int pageIndex: parent.startPage + index
                        compact: true
                        text: String(pageIndex + 1)
                        selected: launcher.catalogPage === pageIndex
                        enabled: !launcher.catalogBusy && pageIndex <= parent.maxPage
                        onClicked: if (launcher.catalogPage !== pageIndex) root.search(pageIndex)
                    }
                }
                ActionButton {
                    compact: true; text: qsTr("Siguiente")
                    enabled: !launcher.catalogBusy && launcher.catalogPage < 100 && (launcher.catalogPage + 1) * 24 < launcher.catalogTotal
                    onClicked: root.search(launcher.catalogPage + 1)
                }
            }
        }
    }
}
