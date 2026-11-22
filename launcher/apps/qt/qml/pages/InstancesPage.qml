import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs as NativeDialogs
import QtQuick.Layouts
import OpenClient

ColumnLayout {
    id: root
    objectName: "instanceLibrary"
    spacing: 14
    signal explore(string instanceId, string kind)
    signal browseModpacks()
    property string detailsId: ""
    property int activeFilter: 0
    property bool compactView: launcher.compactLibrary
    // Cuadrícula: banner de 112 px (antes 56); la tarjeta crece lo mismo que baja su contenido (112 - 63 = 49 px).
    readonly property int bannerHeight: 112
    readonly property int cardHeight: 196 + (bannerHeight - 63)
    readonly property var filters: ["Todas", qsTr("Fabric"), qsTr("Forge"), qsTr("NeoForge"), qsTr("Quilt"), qsTr("Vanilla"), qsTr("Favoritas")]
    readonly property var current: launcher.instances.find(instance => instance.id === detailsId) || null
    readonly property var filtered: launcher.instances.filter(instance => {
        const matchesFilter = activeFilter === 0
            || (activeFilter === 6 ? !!instance.favorite : instance.category === filters[activeFilter].toLowerCase())
        const q = search.text.trim().toLowerCase()
        const matchesSearch = !q.length || (instance.name + " " + instance.mcVersion + " " + (instance.loaderLabel || "")).toLowerCase().includes(q)
        return matchesFilter && matchesSearch
    })
    property var selected: null
    property var menuInstance: null

    // Importaciones externas (CurseForge, MultiMC/Prism): archivo elegido y avisos del resultado.
    property url curseForgeFile
    property bool importPending: false
    property var importNotes: []
    property int importUnresolved: 0
    readonly property var loaderNames: ({ fabric: qsTr("Fabric"), forge: qsTr("Forge"), neoforge: qsTr("NeoForge"), quilt: qsTr("Quilt"), vanilla: qsTr("Vanilla") })

    Connections {
        target: launcher
        function onModpackResultChanged() {
            if (!root.importPending) return
            root.importPending = false
            if (launcher.modpackError.length > 0) return
            const result = launcher.modpackResult
            root.importNotes = (result.warnings || []).map(item => String(item))
            root.importUnresolved = Number(result.unresolved) || 0
        }
        function onCopyChanged() {
            if (!launcher.copyingInstance) root.importPending = false
        }
    }

    function startImport(kind, url) {
        importNotes = []
        importUnresolved = 0
        importPending = true
        if (kind === "curseforge") launcher.importCurseForge(url, true)
        else launcher.importMultiMC(url)
    }

    property string chosenLoader: "fabric"
    // Imagen de banner elegida para una instancia nueva (file:///...); se aplica al crearla.
    property string pendingBanner: ""
    property bool bannerRejected: false
    property int versionEra: 0
    property string versionFilterText: ""

    readonly property var loaderCatalog: [
        { id: "fabric", label: qsTr("Fabric"), note: qsTr("Ligero · 1.14+"), color: "#20a670", minMinor: 14 },
        { id: "forge", label: qsTr("Forge"), note: qsTr("Clásico · 1.7.10+"), color: "#d97724", minMinor: 7 },
        { id: "neoforge", label: qsTr("NeoForge"), note: qsTr("Moderno · 1.20.1+"), color: "#e05d34", minMinor: 20 },
        { id: "quilt", label: qsTr("Quilt"), note: qsTr("Modular · 1.14+"), color: "#8a5cf5", minMinor: 14 },
        { id: "vanilla", label: qsTr("Vanilla"), note: qsTr("Oficial · Todas"), color: "#7e8da7", minMinor: 7 }
    ]
    readonly property var activeLoaderMeta: loaderCatalog.find(item => item.id === chosenLoader) || loaderCatalog[0]

    readonly property var fallbackCatalog: [
        { id: "1.21.4", minor: 21, era: "1.21", featured: true, java: 21, tag: qsTr("Última estable"), forge: "54.1.0", neoforge: "21.4.84" },
        { id: "1.21.3", minor: 21, era: "1.21", featured: false, java: 21, tag: qsTr("Estable"), forge: "53.0.23", neoforge: "21.3.58" },
        { id: "1.21.1", minor: 21, era: "1.21", featured: true, java: 21, tag: qsTr("Recomendada mods"), forge: "52.0.40", neoforge: "21.1.122" },
        { id: "1.21", minor: 21, era: "1.21", featured: false, java: 21, tag: qsTr("Release"), forge: "51.0.33", neoforge: "21.0.167" },
        { id: "1.20.6", minor: 20, era: "1.20", featured: false, java: 21, tag: qsTr("Estable"), forge: "50.1.32", neoforge: "20.6.119" },
        { id: "1.20.4", minor: 20, era: "1.20", featured: true, java: 17, tag: qsTr("Muy popular"), forge: "49.1.13", neoforge: "20.4.237" },
        { id: "1.20.2", minor: 20, era: "1.20", featured: false, java: 17, tag: qsTr("Release"), forge: "48.1.0", neoforge: "20.2.88" },
        { id: "1.20.1", minor: 20, era: "1.20", featured: true, java: 17, tag: qsTr("LTS Modpacks"), forge: "47.3.0", neoforge: "47.1.106" },
        { id: "1.19.4", minor: 19, era: "1.19-1.17", featured: true, java: 17, tag: qsTr("Estable"), forge: "45.3.0", neoforge: "" },
        { id: "1.19.2", minor: 19, era: "1.19-1.17", featured: true, java: 17, tag: qsTr("LTS Mods"), forge: "43.4.0", neoforge: "" },
        { id: "1.18.2", minor: 18, era: "1.19-1.17", featured: true, java: 17, tag: qsTr("LTS Mods"), forge: "40.2.21", neoforge: "" },
        { id: "1.17.1", minor: 17, era: "1.19-1.17", featured: false, java: 16, tag: qsTr("Caves & Cliffs"), forge: "37.1.1", neoforge: "" },
        { id: "1.16.5", minor: 16, era: "1.16-1.14", featured: true, java: 8, tag: qsTr("Nether Update"), forge: "36.2.39", neoforge: "" },
        { id: "1.16.1", minor: 16, era: "1.16-1.14", featured: false, java: 8, tag: qsTr("Speedrun"), forge: "32.0.108", neoforge: "" },
        { id: "1.15.2", minor: 15, era: "1.16-1.14", featured: false, java: 8, tag: qsTr("Estable"), forge: "31.2.57", neoforge: "" },
        { id: "1.14.4", minor: 14, era: "1.16-1.14", featured: false, java: 8, tag: qsTr("Estable"), forge: "28.2.26", neoforge: "" },
        { id: "1.12.2", minor: 12, era: "classic", featured: true, java: 8, tag: qsTr("Clásico Forge"), forge: "14.23.5.2860", neoforge: "" },
        { id: "1.8.9", minor: 8, era: "classic", featured: true, java: 8, tag: qsTr("PvP Clásico"), forge: "11.15.1.2318", neoforge: "" },
        { id: "1.7.10", minor: 7, era: "classic", featured: true, java: 8, tag: qsTr("Clásico Técnico"), forge: "10.13.4.1614", neoforge: "" }
    ]

    // Etiquetas para versiones con comunidad de mods activa; el resto solo muestra su fecha.
    readonly property var featuredTags: ({
        "1.21.1": qsTr("Recomendada mods"), "1.20.4": qsTr("Muy popular"), "1.20.1": qsTr("LTS modpacks"), "1.19.2": qsTr("LTS mods"),
        "1.18.2": qsTr("LTS mods"), "1.16.5": qsTr("Nether Update"), "1.12.2": qsTr("Clásico Forge"), "1.8.9": qsTr("PvP clásico"), "1.7.10": qsTr("Clásico técnico")
    })
    readonly property var versionData: launcher.minecraftVersions
    readonly property bool liveCatalog: !!versionData.versions && versionData.versions.length > 0

    function versionEraOf(id, type) {
        if (type === "snapshot") return "snapshots"
        const parts = id.split(/[^0-9]+/).filter(part => part.length > 0).map(Number)
        if (parts[0] !== 1) return parts[0] + ".x"
        const minor = parts[1] || 0
        if (minor >= 20) return "1." + minor
        if (minor >= 17) return "1.19-1.17"
        if (minor >= 13) return "1.16-1.13"
        if (minor >= 7) return "1.12-1.7"
        return "old"
    }

    readonly property var catalogVersions: {
        if (!liveCatalog)
            return fallbackCatalog.map(v => ({ id: v.id, type: "release", released: "", java: v.java,
                era: versionEraOf(v.id, "release"), featured: v.featured, tag: v.tag }))
        const list = []
        let newest = true
        for (const v of versionData.versions) {
            if (v.type !== "release" && !(v.type === "snapshot" && launcher.showSnapshots)) continue
            const known = fallbackCatalog.find(f => f.id === v.id)
            const tag = v.type === "snapshot" ? qsTr("Snapshot") : newest ? qsTr("Última estable") : (featuredTags[v.id] || "")
            list.push({ id: v.id, type: v.type, released: v.released, java: known ? known.java : 0,
                era: versionEraOf(v.id, v.type), featured: v.type === "release" && tag.length > 0, tag: tag })
            if (v.type === "release") newest = false
        }
        return list
    }
    readonly property string latestRelease: {
        const release = catalogVersions.find(v => v.type === "release")
        return release ? release.id : "1.21.4"
    }

    readonly property var versionEras: {
        const ranges = { "1.19-1.17": "1.19 – 1.17", "1.16-1.13": "1.16 – 1.13", "1.12-1.7": "1.12 – 1.7", "old": qsTr("1.6 y anteriores") }
        const eras = [{ id: "featured", label: qsTr("Destacadas") }]
        const seen = {}
        for (const v of catalogVersions) {
            if (seen[v.era] || v.era === "snapshots") continue
            seen[v.era] = true
            eras.push({ id: v.era, label: ranges[v.era] || (v.era.startsWith("1.") ? v.era + ".x" : v.era) })
        }
        if (catalogVersions.some(v => v.era === "snapshots")) eras.push({ id: "snapshots", label: qsTr("Snapshots") })
        return eras
    }

    function loaderColorFor(cat) {
        if (cat === "fabric") return "#20a670"
        if (cat === "forge") return "#d97724"
        if (cat === "neoforge") return "#e05d34"
        if (cat === "quilt") return "#8a5cf5"
        return "#7e8da7"
    }

    function versionCompatibleWithLoader(verObj, loaderId) {
        if (loaderId === "vanilla") return true
        const list = liveCatalog ? versionData[loaderId] : null
        if (list && list.length !== undefined) return list.indexOf(verObj.id) >= 0
        const known = fallbackCatalog.find(f => f.id === verObj.id)
        if (loaderId === "neoforge") return !!known && known.neoforge.length > 0
        if (loaderId === "forge") return !known || known.forge.length > 0
        const parts = verObj.id.split(/[^0-9]+/).filter(part => part.length > 0).map(Number)
        return parts[0] !== 1 || (parts[1] || 0) >= 14
    }

    function versionSubtitle(verObj, loaderId) {
        const kind = loaderId === "vanilla" ? (verObj.type === "snapshot" ? qsTr("Snapshot") : qsTr("Release")) : activeLoaderMeta.label
        const when = verObj.released ? Qt.formatDate(new Date(verObj.released + "T12:00:00"), qsTr("MMM yyyy")) : ""
        return [kind, when || (verObj.java ? "Java " + verObj.java : "")].filter(part => part.length > 0).join(" · ")
    }

    readonly property var filteredVersions: catalogVersions.filter(v => {
        if (!versionCompatibleWithLoader(v, chosenLoader)) return false
        const q = versionFilterText.trim().toLowerCase()
        if (q.length > 0) return v.id.toLowerCase().includes(q) || v.tag.toLowerCase().includes(q)
        const eraId = versionEras[Math.min(versionEra, versionEras.length - 1)].id
        if (eraId === "featured") return v.featured
        return v.era === eraId
    })

    function selectLoader(loaderId) {
        chosenLoader = loaderId
        const typed = versionField.text.trim()
        const current = catalogVersions.find(v => v.id === typed)
        if (current && !versionCompatibleWithLoader(current, loaderId)) {
            versionEra = 0
            const fallbackVersion = catalogVersions.find(v => v.type === "release" && versionCompatibleWithLoader(v, loaderId))
            versionField.text = fallbackVersion ? fallbackVersion.id : root.latestRelease
        }
    }

    function openActions(instance, anchor, x, y) {
        menuInstance = instance
        instanceActions.popup(anchor, x, y)
    }

    function openEditor(instance) {
        selected = instance
        nameField.text = instance ? instance.name : ""
        launcher.loadMinecraftVersions(false)
        versionField.text = instance ? instance.mcVersion : root.latestRelease
        chosenLoader = instance && instance.category ? instance.category : "fabric"
        versionEra = 0
        versionFilterText = ""
        editorError.text = ""
        pendingBanner = ""
        editor.open()
    }

    ColumnLayout {
        id: library
        visible: root.current === null
        Layout.fillWidth: true
        spacing: 14
        SectionHeader {
            Layout.fillWidth: true
            title: qsTr("Biblioteca de instancias")
            description: qsTr("Perfiles independientes de Minecraft con Fabric, Forge, NeoForge, Quilt y Vanilla.")
            detail: launcher.instances.length === 1 ? qsTr("1 instancia") : qsTr("%1 instancias").arg(launcher.instances.length)
            glyph: "layers"
        }
        GlassPanel {
            Layout.fillWidth: true
            implicitHeight: 62
            padding: 11
            RowLayout {
                anchors.fill: parent
                spacing: 7
                GlassField {
                    id: search
                    placeholderText: qsTr("Buscar instancias por nombre, versión o cargador…")
                    Layout.fillWidth: true
                    Layout.minimumWidth: 110
                    Layout.preferredHeight: 38
                }
                GlassComboBox {
                    id: filterBox
                    visible: root.width < 1120
                    model: root.filters
                    currentIndex: root.activeFilter
                    onActivated: index => root.activeFilter = index
                    Layout.preferredWidth: 118
                    Layout.preferredHeight: 38
                    Accessible.name: "Filtrar instancias"
                }
                RowLayout {
                    visible: root.width >= 1120
                    spacing: 3
                    Repeater {
                        model: root.filters
                        ActionButton {
                            required property string modelData
                            required property int index
                            text: modelData
                            compact: true
                            navigation: true
                            selected: root.activeFilter === index
                            onClicked: root.activeFilter = index
                        }
                    }
                }
                ActionButton {
                    compact: true
                    glyph: root.compactView ? "grid" : "list"
                    ToolTip.text: root.compactView ? qsTr("Ver cuadrícula") : qsTr("Ver lista")
                    onClicked: launcher.compactLibrary = !root.compactView
                }
                ActionButton {
                    id: importButton
                    text: root.width >= 800 ? qsTr("Importar") : ""; glyph: "folder"; compact: true
                    ToolTip.text: qsTr("Importar instancia")
                    enabled: !launcher.gameBusy && !launcher.catalogBusy
                    onClicked: importMenu.popup(importButton, 0, importButton.height + 6)
                }
                ActionButton { text: qsTr("Modpacks"); glyph: "compass"; compact: true; onClicked: root.browseModpacks() }
                ActionButton { text: qsTr("Nueva instancia"); glyph: "plus"; compact: true; primary: true; enabled: !launcher.gameBusy && !launcher.catalogBusy; onClicked: root.openEditor(null) }
            }
        }
        GlassPanel {
            objectName: "importProgressCard"
            visible: launcher.operationActive
            Layout.fillWidth: true
            implicitHeight: copyBar.implicitHeight + 28
            padding: 14
            OperationBar { id: copyBar; anchors.fill: parent; heading: launcher.copyMessage }
        }
        Label {
            visible: launcher.copyMessage.length > 0 && !launcher.operationActive
            text: launcher.copyMessage
            textFormat: Text.PlainText
            color: Theme.dim
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }
        GlassPanel {
            id: importNotice
            objectName: "importNotice"
            visible: root.importNotes.length > 0 || root.importUnresolved > 0
            Layout.fillWidth: true
            implicitHeight: noticeBody.implicitHeight + 32
            padding: 16
            border.color: Qt.alpha(Theme.warning, 0.55)
            Accessible.role: Accessible.AlertMessage
            Accessible.name: "La importación terminó con avisos"
            ColumnLayout {
                id: noticeBody
                anchors.fill: parent
                spacing: 6
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    Label {
                        Layout.fillWidth: true
                        text: qsTr("La importación terminó con avisos")
                        color: Theme.warning; font.weight: Font.DemiBold; font.pixelSize: 14
                    }
                    ActionButton {
                        text: qsTr("Entendido"); compact: true
                        onClicked: { root.importNotes = []; root.importUnresolved = 0 }
                    }
                }
                Label {
                    visible: root.importUnresolved > 0
                    Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.ink; font.pixelSize: 12
                    text: root.importUnresolved === 1
                        ? qsTr("1 mod de CurseForge no se pudo descargar y no está en la instancia. Tendrás que añadirlo a mano.")
                        : qsTr("%1 mods de CurseForge no se pudieron descargar y no están en la instancia. Tendrás que añadirlos a mano.").arg(root.importUnresolved)
                }
                Repeater {
                    model: root.importNotes.slice(0, 6)
                    Label {
                        required property string modelData
                        Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.ink; font.pixelSize: 12
                        textFormat: Text.PlainText
                        text: "• " + modelData
                    }
                }
                Label {
                    visible: root.importNotes.length > 6
                    Layout.fillWidth: true; color: Theme.dim; font.pixelSize: 12
                    text: qsTr("y %1 avisos más.").arg(root.importNotes.length - 6)
                }
            }
        }
        Label {
            visible: launcher.gameMessage.length > 0 && !launcher.gameBusy
            text: launcher.gameMessage
            textFormat: Text.PlainText
            color: Theme.ink
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }
        OperationBar { Layout.fillWidth: true; game: true }
        Label {
            visible: launcher.libraryError.length > 0 && !editor.opened && !removeDialog.opened
            text: launcher.libraryError
            color: Theme.danger
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }
        GlassPanel {
            visible: root.filtered.length === 0
            Layout.fillWidth: true
            implicitHeight: 196
            ColumnLayout {
                anchors.centerIn: parent
                spacing: 8
                Rectangle {
                    Layout.alignment: Qt.AlignHCenter
                    width: 44; height: 44; radius: Theme.radiusMd
                    color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
                    border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
                    Glyph { anchors.centerIn: parent; name: "layers"; color: Theme.highlight }
                }
                Label {
                    text: launcher.libraryError.length ? qsTr("Biblioteca no disponible") :
                        launcher.instances.length ? qsTr("Sin coincidencias para este filtro") : qsTr("Todavía no hay instancias")
                    color: Theme.ink
                    font.pixelSize: 17
                    font.weight: Font.DemiBold
                    Layout.alignment: Qt.AlignHCenter
                }
                Label {
                    visible: !launcher.libraryError.length && !launcher.instances.length
                    text: qsTr("Crea una instancia con Fabric, Forge, NeoForge, Quilt o Vanilla, o importa un .mrpack.")
                    color: Theme.dim
                    font.pixelSize: 12
                    Layout.alignment: Qt.AlignHCenter
                }
            }
        }
        GridLayout {
            Layout.fillWidth: true
            columns: root.compactView ? 1 : root.width >= 1190 ? 4 : root.width >= 840 ? 3 : 2
            rowSpacing: 12
            columnSpacing: 12
            Repeater {
                model: root.filtered
                GlassPanel {
                    id: card
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    implicitHeight: root.compactView ? 70 : root.cardHeight
                    padding: 0
                    clip: true
                    readonly property string loaderId: modelData.category || "vanilla"
                    readonly property color loaderColor: root.loaderColorFor(loaderId)
                    readonly property int totalMods: (modelData.installedContent || []).length
                    readonly property int activeMods: (modelData.installedContent || []).filter(item => item.enabled !== false).length
                    readonly property string playtimeText: (modelData.playtimeHours || 0) > 0 ? qsTr("%1 h").arg(modelData.playtimeHours) : qsTr("0 h")
                    border.color: Theme.crystal ? (cardHover.containsMouse ? Theme.glassBorderHover : "transparent")
                        : cardHover.containsMouse ? Theme.highlight : Theme.rim
                    Behavior on border.color { MotionColor { role: "fast" } }
                    transform: Translate {
                        y: cardHover.containsMouse && Theme.motion && !root.compactView ? -2 : 0
                        Behavior on y { MotionNumber { role: "fast" } }
                    }
                    MouseArea {
                        id: cardHover
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: mouse => {
                            if (mouse.button === Qt.RightButton)
                                root.openActions(card.modelData, card, mouse.x, mouse.y)
                            else root.detailsId = card.modelData.id
                        }
                    }
                    // Banda de banner de la instancia. En la vista compacta no se crea ni pide nada.
                    InstanceBanner {
                        anchors.top: parent.top
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.margins: 1
                        height: root.compactView ? 0 : root.bannerHeight
                        radius: Theme.radiusMd - 1
                        visible: !root.compactView
                        instanceId: root.compactView ? "" : card.modelData.id
                        decodeWidth: 640
                        scrim: 0.38
                        Rectangle {
                            anchors.fill: parent
                            color: Qt.alpha(card.loaderColor, cardHover.containsMouse ? 0.16 : 0.08)
                            Behavior on color { MotionColor { role: "fast" } }
                        }
                    }
                    GridLayout {
                        anchors.fill: parent
                        anchors.margins: 13
                        // En la cuadrícula la primera fila (icono y etiquetas) se apoya en la parte baja del banner grande.
                        anchors.topMargin: root.compactView ? 13 : root.bannerHeight - 50
                        columns: root.compactView ? 3 : 1
                        rowSpacing: 8
                        columnSpacing: 12
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 40
                            spacing: 10
                            Rectangle {
                                width: 38; height: 38; radius: Theme.radiusSm
                                color: Theme.crystal ? Theme.rowFill : Theme.surface
                                border.color: Theme.crystal ? (cardHover.containsMouse ? Theme.glassBorderHover : Theme.glassBorder)
                                    : cardHover.containsMouse ? Theme.edge : Theme.rim
                                InstanceBlock { anchors.centerIn: parent; loader: card.loaderId; fabric: card.loaderId === "fabric" }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                RowLayout {
                                    spacing: 6
                                    Rectangle {
                                        radius: 4
                                        color: Qt.alpha(card.loaderColor, 0.18)
                                        border.color: Qt.alpha(card.loaderColor, 0.42)
                                        implicitWidth: loaderTag.implicitWidth + 10
                                        implicitHeight: 18
                                        Label {
                                            id: loaderTag
                                            anchors.centerIn: parent
                                            text: card.modelData.loaderLabel + " " + card.modelData.mcVersion
                                            textFormat: Text.PlainText
                                            color: Theme.ink; font.pixelSize: 10; font.weight: Font.Medium; font.features: Theme.tabular
                                        }
                                    }
                                    Rectangle {
                                        visible: !!card.modelData.modpackVersion
                                        radius: 4
                                        color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
                                        border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
                                        implicitWidth: packTag.implicitWidth + 8
                                        implicitHeight: 18
                                        Label {
                                            id: packTag
                                            anchors.centerIn: parent
                                            text: card.modelData.modpackVersion || ""
                                            textFormat: Text.PlainText
                                            color: Theme.highlight; font.pixelSize: 10; font.features: Theme.tabular
                                        }
                                    }
                                }
                                Label {
                                    visible: !root.compactView && !!(card.modelData.lastPlayed && card.modelData.lastPlayed.length > 0)
                                    text: card.modelData.lastPlayed || ""
                                    textFormat: Text.PlainText; elide: Text.ElideRight
                                    color: Qt.alpha(Theme.ink, 0.78); font.pixelSize: 10
                                }
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.topMargin: root.compactView ? 0 : 4
                            spacing: 4
                            Label {
                                Layout.fillWidth: true
                                text: (card.modelData.favorite ? "★  " : "") + card.modelData.name
                                textFormat: Text.PlainText; elide: Text.ElideRight
                                color: Theme.ink; font.pixelSize: 15; font.weight: Font.DemiBold
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 10
                                Label {
                                    text: card.totalMods > 0 ? qsTr("%1/%2 activos").arg(card.activeMods).arg(card.totalMods) : qsTr("Sin mods")
                                    textFormat: Text.PlainText
                                    color: Theme.dim; font.pixelSize: 11; font.features: Theme.tabular
                                }
                                Label { text: "·"; color: Theme.dim; font.pixelSize: 11 }
                                Label {
                                    Layout.fillWidth: true
                                    text: card.playtimeText + (card.modelData.lastPlayed ? "  ·  " + card.modelData.lastPlayed : "")
                                    textFormat: Text.PlainText; elide: Text.ElideRight
                                    color: Theme.dim; font.pixelSize: 11; font.features: Theme.tabular
                                }
                            }
                        }
                        Rectangle { visible: !root.compactView; Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.rim }
                        RowLayout {
                            Layout.fillWidth: !root.compactView
                            spacing: 6
                            ActionButton {
                                visible: !root.compactView
                                glyph: "folder"; compact: true
                                ToolTip.text: qsTr("Abrir carpeta de %1").arg(card.modelData.name)
                                onClicked: launcher.openInstanceFolder(card.modelData.id, "")
                            }
                            Item { Layout.fillWidth: true; visible: !root.compactView }
                            ActionButton {
                                id: optionsButton
                                glyph: "settings"; compact: true
                                ToolTip.text: qsTr("Opciones de %1").arg(card.modelData.name)
                                onClicked: root.openActions(card.modelData, optionsButton, 0, optionsButton.height + 6)
                            }
                            ActionButton {
                                text: qsTr("Jugar"); glyph: "play"; compact: true; primary: true
                                enabled: !launcher.gameBusy
                                onClicked: launcher.playInstance(card.modelData.id)
                            }
                        }
                    }
                }
            }
            ActionButton {
                visible: root.activeFilter === 0 && !search.text.trim().length && root.filtered.length > 0
                Layout.fillWidth: true; Layout.preferredWidth: 1
                implicitHeight: root.compactView ? 70 : root.cardHeight
                text: qsTr("Nueva instancia"); glyph: "plus"; brand: true
                enabled: !launcher.gameBusy && !launcher.catalogBusy
                onClicked: root.openEditor(null)
            }
        }
    }
    Loader {
        active: root.current !== null
        visible: active
        Layout.fillWidth: true
        Layout.preferredHeight: item ? item.implicitHeight : 0
        sourceComponent: InstanceDetails {
            instance: root.current
            onBack: root.detailsId = ""
            onEdit: root.openEditor(root.current)
            onRetire: {
                root.selected = root.current
                removeError.text = ""
                removeDialog.open()
            }
            onExplore: (instanceId, kind) => root.explore(instanceId, kind)
        }
    }
    Label {
        visible: root.current !== null && ((launcher.gameMessage.length > 0 && !launcher.gameBusy) || launcher.libraryError.length > 0)
        text: launcher.libraryError || launcher.gameMessage
        textFormat: Text.PlainText
        color: launcher.libraryError.length > 0 ? Theme.danger : Theme.ink
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
    }
    OperationBar { visible: root.current !== null && launcher.gameBusy; Layout.fillWidth: true; game: true }
    NativeDialogs.FolderDialog {
        id: importFolder
        title: qsTr("Selecciona la carpeta exportada de OpenClient")
        onAccepted: launcher.importInstance(selectedFolder)
    }
    NativeDialogs.FileDialog {
        id: modpackPicker
        title: qsTr("Elige el modpack que quieres instalar")
        nameFilters: ["Modpacks de Modrinth (*.mrpack)"]
        onAccepted: launcher.importModpack(selectedFile)
    }
    NativeDialogs.FileDialog {
        id: curseForgePicker
        title: qsTr("Elige el modpack de CurseForge")
        nameFilters: ["Modpacks de CurseForge (*.zip)"]
        onAccepted: {
            root.curseForgeFile = selectedFile
            launcher.importCurseForge(selectedFile, false)
            curseForgeDialog.open()
        }
    }
    NativeDialogs.FolderDialog {
        id: multiMcFolder
        title: qsTr("Selecciona la carpeta de la instancia de MultiMC o Prism")
        onAccepted: root.startImport("mmc", selectedFolder)
    }
    NativeDialogs.FileDialog {
        id: multiMcZip
        title: qsTr("Elige el .zip de la instancia de MultiMC o Prism")
        nameFilters: ["Instancias de MultiMC / Prism (*.zip)"]
        onAccepted: root.startImport("mmc", selectedFile)
    }
    GlassMenu {
        id: importMenu
        width: 292
        GlassMenuItem { glyph: "layers"; text: qsTr("Archivo de Modrinth (.mrpack)"); onTriggered: modpackPicker.open() }
        GlassMenuItem { glyph: "layers"; text: qsTr("CurseForge (.zip)…"); enabled: !launcher.copyingInstance; onTriggered: curseForgePicker.open() }
        GlassMenuItem { glyph: "folder"; text: qsTr("MultiMC / Prism (carpeta)…"); enabled: !launcher.copyingInstance; onTriggered: multiMcFolder.open() }
        GlassMenuItem { glyph: "folder"; text: qsTr("MultiMC / Prism (.zip)…"); enabled: !launcher.copyingInstance; onTriggered: multiMcZip.open() }
        GlassMenuItem { glyph: "folder"; text: qsTr("Carpeta de OpenClient"); onTriggered: importFolder.open() }
    }
    GlassMenu {
        id: instanceActions
        GlassMenuItem { glyph: "eye"; text: qsTr("Ver detalles"); onTriggered: root.detailsId = root.menuInstance.id }
        GlassMenuItem { glyph: "edit"; text: qsTr("Editar"); enabled: !launcher.gameBusy && !launcher.catalogBusy; onTriggered: root.openEditor(root.menuInstance) }
        GlassMenuItem {
            glyph: "star"
            text: root.menuInstance && root.menuInstance.favorite ? qsTr("Quitar de favoritas") : qsTr("Marcar como favorita")
            enabled: !launcher.gameBusy && !launcher.catalogBusy
            onTriggered: launcher.toggleFavorite(root.menuInstance.id)
        }
        GlassMenuItem { glyph: "compass"; text: qsTr("Explorar contenido"); onTriggered: root.explore(root.menuInstance.id, root.menuInstance.category === "vanilla" ? "resourcepack" : "mod") }
        GlassMenuItem { glyph: "folder"; text: qsTr("Abrir carpeta"); onTriggered: launcher.openInstanceFolder(root.menuInstance.id, "") }
        GlassMenuItem { glyph: "copy"; text: qsTr("Duplicar instancia"); enabled: !launcher.gameBusy && !launcher.catalogBusy; onTriggered: launcher.duplicateInstance(root.menuInstance.id) }
        MenuSeparator {
            topPadding: 5
            bottomPadding: 5
            leftPadding: 6
            rightPadding: 6
            contentItem: Rectangle { implicitHeight: 1; color: Theme.crystal ? Theme.divider : Theme.rim }
        }
        GlassMenuItem {
            glyph: "trash"
            danger: true
            text: qsTr("Retirar instancia")
            enabled: !launcher.gameBusy && !launcher.catalogBusy
            onTriggered: {
                root.selected = root.menuInstance
                removeError.text = ""
                removeDialog.open()
            }
        }
    }

    Connections {
        target: launcher
        function onBannerFailed(message) {
            root.bannerRejected = true
            if (editor.visible) editorError.text = message
        }
    }
    NativeDialogs.FileDialog {
        id: bannerPicker
        title: qsTr("Elige una imagen para el banner")
        nameFilters: [qsTr("Imágenes (*.png *.jpg *.jpeg *.webp)")]
        onAccepted: {
            editorError.text = ""
            if (root.selected) launcher.setBannerImage(root.selected.id, selectedFile)
            else root.pendingBanner = selectedFile.toString()
        }
    }

    GlassDialog {
        id: editor
        objectName: "instanceEditor"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(780, parent ? parent.width - 48 : 780)
        modal: true
        title: root.selected ? qsTr("Configurar versión e instancia") : qsTr("Nueva instancia de Minecraft")
        glyph: "layers"
        contentItem: ColumnLayout {
            spacing: 14

            // 0. Banner con vista previa en vivo: la imagen de la instancia o el arte generado según cargador, versión y nombre.
            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: 100
                InstanceBanner {
                    id: editorBanner
                    anchors.fill: parent
                    radius: Theme.radiusMd
                    instanceId: root.selected ? root.selected.id : ""
                    previewCategory: root.chosenLoader
                    previewVersion: versionField.text.trim() || root.latestRelease
                    previewName: nameField.text.trim() || (root.activeLoaderMeta.label + " " + (versionField.text.trim() || root.latestRelease))
                    previewImage: root.selected ? "" : root.pendingBanner
                    scrim: 0.12
                    Rectangle {
                        anchors.fill: parent
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: Qt.alpha(Theme.base, 0.82) }
                            GradientStop { position: 0.6; color: Qt.alpha(Theme.base, 0.45) }
                            GradientStop { position: 1; color: Qt.alpha(Theme.base, 0.1) }
                        }
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radiusMd
                    color: "transparent"
                    border.color: Theme.glassBorder
                }
                ColumnLayout {
                    anchors.left: parent.left
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: 16
                    anchors.bottomMargin: 12
                    width: Math.max(80, parent.width - bannerButtons.width - 48)
                    spacing: 2
                    Label {
                        Layout.fillWidth: true
                        text: editorBanner.displayName
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: Theme.ink
                        font.family: Theme.displayFamily
                        font.pixelSize: 20
                        font.weight: Font.ExtraBold
                        font.letterSpacing: -0.3
                    }
                    Label {
                        Layout.fillWidth: true
                        text: qsTr("Minecraft %1 · %2").arg(editorBanner.mcVersion).arg(root.activeLoaderMeta.label)
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: Qt.alpha(Theme.ink, 0.82)
                        font.pixelSize: 12
                        font.features: Theme.tabular
                    }
                }
                Rectangle {
                    id: bannerButtons
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: 10
                    width: bannerButtonRow.implicitWidth + 8
                    height: 44
                    radius: Theme.radiusSm + 2
                    color: Qt.alpha(Theme.base, 0.62)
                    border.color: Theme.glassBorder
                    Row {
                        id: bannerButtonRow
                        anchors.centerIn: parent
                        spacing: 4
                        ActionButton {
                            text: qsTr("Elegir imagen…")
                            glyph: "image"; compact: true
                            onClicked: bannerPicker.open()
                        }
                        ActionButton {
                            visible: root.selected ? editorBanner.info.source === "custom" : root.pendingBanner.length > 0
                            text: qsTr("Quitar imagen")
                            compact: true
                            onClicked: {
                                editorError.text = ""
                                if (root.selected) launcher.clearBanner(root.selected.id)
                                else root.pendingBanner = ""
                            }
                        }
                    }
                }
            }

            // 1. Identidad y vista previa del perfil
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: 68
                radius: Theme.radiusMd
                color: Theme.crystal ? Theme.rowFill : Theme.base
                border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 12
                    Rectangle {
                        width: 44; height: 44; radius: Theme.radiusSm
                        color: Qt.alpha(root.activeLoaderMeta.color, 0.14)
                        border.color: Qt.alpha(root.activeLoaderMeta.color, 0.45)
                        InstanceBlock { anchors.centerIn: parent; loader: root.chosenLoader; fabric: root.chosenLoader === "fabric" }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 3
                        Label {
                            text: qsTr("Nombre de la instancia")
                            color: Theme.dim; font.pixelSize: 11
                        }
                        GlassField {
                            id: nameField
                            Layout.fillWidth: true
                            Layout.preferredHeight: 30
                            implicitHeight: 30
                            radius: Theme.radiusSm - 2
                            legacyFill: Theme.surface
                            placeholderText: root.activeLoaderMeta.label + " " + (versionField.text.trim() || root.latestRelease)
                            maximumLength: 64
                            Accessible.name: "Nombre de la instancia"
                        }
                    }
                    ColumnLayout {
                        Layout.preferredWidth: 165
                        spacing: 3
                        Label {
                            text: qsTr("Versión seleccionada")
                            color: Theme.dim; font.pixelSize: 11
                        }
                        GlassField {
                            id: versionField
                            Layout.fillWidth: true
                            Layout.preferredHeight: 30
                            implicitHeight: 30
                            radius: Theme.radiusSm - 2
                            legacyFill: Theme.surface
                            placeholderText: qsTr("Ej. 1.21.4")
                            maximumLength: 40
                            color: Theme.highlight
                            font.features: Theme.tabular
                             font.weight: Font.DemiBold
                            Accessible.name: "Versión de Minecraft"
                        }
                    }
                }
            }

            // 2. Selector de Cargador (Modloader)
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6
                Label { text: qsTr("1. Elige el cargador (Modloader)"); color: Theme.ink; font.pixelSize: 13; font.weight: Font.DemiBold }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Repeater {
                        model: root.loaderCatalog
                        Rectangle {
                            id: loaderCard
                            required property var modelData
                            readonly property bool active: root.chosenLoader === modelData.id
                            Layout.fillWidth: true
                            Layout.preferredWidth: 1
                            implicitHeight: 58
                            radius: Theme.radiusSm
                            color: active ? Qt.alpha(modelData.color, Theme.light ? 0.14 : 0.18)
                                : Theme.crystal ? (loaderHover.hovered ? Theme.rowFillHover : Theme.rowFill)
                                : (loaderHover.hovered ? Theme.surface : Theme.base)
                            border.width: active ? 2 : 1
                            border.color: active ? modelData.color
                                : Theme.crystal ? (loaderHover.hovered ? Theme.glassBorderHover : Theme.glassBorder)
                                : (loaderHover.hovered ? Theme.edge : Theme.border)
                            Behavior on color { MotionColor { role: "fast" } }
                            HoverHandler { id: loaderHover }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.selectLoader(loaderCard.modelData.id)
                            }
                            ColumnLayout {
                                anchors.fill: parent
                                anchors.margins: 8
                                spacing: 2
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 6
                                    Rectangle {
                                        width: 8; height: 8; radius: 4
                                        color: loaderCard.modelData.color
                                    }
                                    Label {
                                        Layout.fillWidth: true
                                        text: loaderCard.modelData.label
                                        color: Theme.ink; font.pixelSize: 13; font.weight: Font.DemiBold
                                        elide: Text.ElideRight
                                    }
                                }
                                Label {
                                    Layout.fillWidth: true
                                    text: loaderCard.modelData.note
                                    color: Theme.dim; font.pixelSize: 10
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                }
            }

            // 3. Explorador de versiones de Minecraft
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 8
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    Label { text: qsTr("2. Elige la versión de Minecraft"); color: Theme.ink; font.pixelSize: 13; font.weight: Font.DemiBold }
                    Item { Layout.fillWidth: true }
                    GlassField {
                        id: versionSearch
                        Layout.preferredWidth: 185
                        Layout.preferredHeight: 30
                        implicitHeight: 30
                        radius: Theme.radiusSm - 2
                        placeholderText: qsTr("Filtrar versión (ej. 1.20.1)…")
                        font.pixelSize: 11
                        onTextChanged: root.versionFilterText = text
                    }
                }
                Flow {
                    Layout.fillWidth: true
                    spacing: 5
                    Repeater {
                        model: root.versionEras
                        ActionButton {
                            required property var modelData
                            required property int index
                            text: modelData.label
                            compact: true
                            selected: root.versionEra === index && !root.versionFilterText.trim().length
                            onClicked: {
                                versionSearch.text = ""
                                root.versionEra = index
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Label {
                        objectName: "versionCatalogStatus"
                        Layout.fillWidth: true
                        text: launcher.versionsLoading ? qsTr("Actualizando la lista de versiones…")
                            : launcher.versionsError.length > 0 ? qsTr("No se pudo consultar a Mojang. Se muestran las versiones incluidas en el launcher.")
                            : root.versionData.offline ? qsTr("Sin conexión: se muestra la última lista guardada.")
                            : root.liveCatalog ? (launcher.showSnapshots
                                ? qsTr("Lista oficial de Mojang · %1 versiones y snapshots").arg(root.catalogVersions.filter(v => v.type === "release").length)
                                : qsTr("Lista oficial de Mojang · %1 versiones").arg(root.catalogVersions.filter(v => v.type === "release").length))
                            : ""
                        color: launcher.versionsError.length > 0 || root.versionData.offline ? Theme.warning : Theme.dim
                        font.pixelSize: 10
                        font.features: Theme.tabular
                        elide: Text.ElideRight
                    }
                    ActionButton {
                        glyph: "refresh"; compact: true
                        enabled: !launcher.versionsLoading
                        ToolTip.text: qsTr("Actualizar la lista de versiones")
                        onClicked: launcher.loadMinecraftVersions(true)
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 214
                    radius: Theme.radiusMd
                    color: Theme.crystal ? Theme.fieldFill : Theme.base
                    border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                    clip: true
                    Label {
                        anchors.centerIn: parent
                        width: parent.width - 32
                        visible: root.filteredVersions.length === 0
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                        text: launcher.versionsLoading ? qsTr("Cargando versiones…")
                            : root.versionFilterText.trim().length > 0
                                ? qsTr("Ninguna versión coincide con «%1» para %2.").arg(root.versionFilterText.trim()).arg(root.activeLoaderMeta.label)
                                : qsTr("%1 no tiene versiones en este grupo. Prueba otro.").arg(root.activeLoaderMeta.label)
                        color: Theme.dim
                        font.pixelSize: 12
                    }
                    ScrollView {
                        id: versionScroll
                        anchors.fill: parent
                        anchors.margins: 8
                        clip: true
                        contentWidth: availableWidth
                        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                        GridLayout {
                            width: versionScroll.availableWidth
                            columns: 3
                            rowSpacing: 7
                            columnSpacing: 7
                            Repeater {
                                model: root.filteredVersions
                                Rectangle {
                                    id: verCard
                                    required property var modelData
                                    readonly property bool chosen: versionField.text.trim() === modelData.id
                                    Layout.fillWidth: true
                                    Layout.preferredWidth: 1
                                    implicitHeight: 54
                                    radius: Theme.radiusSm
                                    color: Theme.crystal ? (chosen ? Qt.alpha(Theme.accent, 0.16) : verHover.hovered ? Theme.rowFillHover : Theme.rowFill)
                                        : chosen ? Theme.soft : (verHover.hovered ? Theme.surface : Theme.card)
                                    border.width: chosen ? 2 : 1
                                    border.color: chosen ? root.activeLoaderMeta.color
                                        : Theme.crystal ? (verHover.hovered ? Theme.glassBorderHover : Theme.glassBorder)
                                        : (verHover.hovered ? Theme.edge : Theme.border)
                                    Behavior on border.color { MotionColor { role: "fast" } }
                                    HoverHandler { id: verHover }
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: versionField.text = verCard.modelData.id
                                    }
                                    RowLayout {
                                        anchors.fill: parent
                                        anchors.leftMargin: 10; anchors.rightMargin: 10
                                        spacing: 8
                                        ColumnLayout {
                                            Layout.fillWidth: true
                                            spacing: 2
                                            RowLayout {
                                                spacing: 6
                                                Label {
                                                    text: verCard.modelData.id
                                                    color: Theme.ink; font.pixelSize: 14; font.weight: Font.Bold; font.features: Theme.tabular
                                                }
                                                Rectangle {
                                                    visible: verCard.modelData.tag.length > 0
                                                    radius: 3
                                                    color: Qt.alpha(root.activeLoaderMeta.color, 0.16)
                                                    implicitWidth: featTag.implicitWidth + 8
                                                    implicitHeight: 15
                                                    Label {
                                                        id: featTag
                                                        anchors.centerIn: parent
                                                        text: verCard.modelData.tag
                                                        color: Theme.highlight; font.pixelSize: 9
                                                    }
                                                }
                                            }
                                            Label {
                                                Layout.fillWidth: true
                                                text: root.versionSubtitle(verCard.modelData, root.chosenLoader)
                                                color: Theme.dim; font.pixelSize: 10; font.features: Theme.tabular
                                                elide: Text.ElideRight
                                            }
                                        }
                                        Label {
                                            visible: verCard.chosen
                                            text: "✓"
                                            color: root.activeLoaderMeta.color
                                            font.pixelSize: 14; font.weight: Font.Bold
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Label { id: editorError; visible: text.length > 0; Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.danger }

            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                Label {
                    Layout.fillWidth: true
                    text: root.chosenLoader === "fabric" && launcher.autoSodium && !root.selected
                        ? qsTr("%1 · Minecraft %2 · Incluye Sodium").arg(root.activeLoaderMeta.label).arg(versionField.text.trim() || root.latestRelease)
                        : qsTr("%1 · Minecraft %2").arg(root.activeLoaderMeta.label).arg(versionField.text.trim() || root.latestRelease)
                    color: Theme.dim; font.pixelSize: 12; font.features: Theme.tabular
                    elide: Text.ElideRight
                }
                ActionButton { text: qsTr("Cancelar"); onClicked: editor.close() }
                ActionButton {
                    text: root.selected ? qsTr("Guardar cambios") : qsTr("Crear instancia")
                    glyph: root.selected ? "settings" : "plus"
                    primary: true
                    onClicked: {
                        const finalVersion = versionField.text.trim() || root.latestRelease
                        const finalName = nameField.text.trim() || (root.activeLoaderMeta.label + " " + finalVersion)
                        const before = launcher.instances.map(item => item.id)
                        if (!launcher.saveInstance(root.selected ? root.selected.id : "", finalName, finalVersion, root.chosenLoader)) {
                            editorError.text = launcher.libraryError
                            return
                        }
                        // Instancia nueva con imagen elegida: se aplica al id recién creado. Si se rechaza, el diálogo
                        // sigue abierto (ya en modo edición) con el aviso.
                        const created = root.selected ? null : launcher.instances.find(item => before.indexOf(item.id) < 0)
                        if (created && root.pendingBanner.length > 0) {
                            root.bannerRejected = false
                            launcher.setBannerImage(created.id, root.pendingBanner)
                            root.pendingBanner = ""
                            if (root.bannerRejected) {
                                root.selected = created
                                return
                            }
                        }
                        editor.close()
                    }
                }
            }
        }
    }
    GlassDialog {
        id: curseForgeDialog
        objectName: "curseForgeDialog"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(620, (parent ? parent.width : 680) - 48)
        modal: true
        title: qsTr("Importar modpack de CurseForge")
        glyph: "layers"
        readonly property var preview: launcher.curseForgePreview
        readonly property bool loading: Object.keys(preview).length === 0
        readonly property bool failed: !loading && !!preview.error
        readonly property int unresolved: Number(preview.unresolved) || 0
        readonly property int included: Number(preview.overrideFiles) || 0
        readonly property var notes: (preview.warnings || []).map(item => String(item))
        readonly property bool busy: launcher.catalogBusy || launcher.copyingInstance || launcher.gameBusy
        contentItem: ColumnLayout {
            spacing: 14

            RowLayout {
                Layout.fillWidth: true
                visible: curseForgeDialog.loading
                spacing: 10
                BusyIndicator { running: parent.visible && Theme.motion; Layout.preferredWidth: 22; Layout.preferredHeight: 22 }
                Label { Layout.fillWidth: true; text: qsTr("Leyendo el archivo…"); color: Theme.dim }
            }

            Rectangle {
                Layout.fillWidth: true
                visible: curseForgeDialog.failed
                implicitHeight: failBody.implicitHeight + 28
                radius: Theme.radiusMd
                color: Qt.alpha(Theme.danger, Theme.light ? 0.10 : 0.14)
                border.color: Qt.alpha(Theme.danger, 0.55)
                Accessible.role: Accessible.AlertMessage
                Accessible.name: "No se pudo leer el archivo de CurseForge"
                ColumnLayout {
                    id: failBody
                    anchors.fill: parent; anchors.margins: 14; spacing: 4
                    Label {
                        Layout.fillWidth: true; color: Theme.danger; font.weight: Font.DemiBold
                        text: qsTr("No se pudo leer este archivo")
                    }
                    Label {
                        Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.ink; font.pixelSize: 12
                        textFormat: Text.PlainText
                        text: curseForgeDialog.preview.error || ""
                    }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                visible: !curseForgeDialog.loading && !curseForgeDialog.failed
                spacing: 12

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 3
                    Label {
                        Layout.fillWidth: true
                        text: curseForgeDialog.preview.name || qsTr("Modpack sin nombre")
                        textFormat: Text.PlainText; elide: Text.ElideRight
                        color: Theme.ink; font.pixelSize: 18; font.weight: Font.DemiBold
                    }
                    Label {
                        Layout.fillWidth: true
                        textFormat: Text.PlainText; elide: Text.ElideRight
                        color: Theme.dim; font.pixelSize: 12; font.features: Theme.tabular
                        text: [curseForgeDialog.preview.version ? qsTr("v%1").arg(curseForgeDialog.preview.version) : "",
                               curseForgeDialog.preview.minecraft ? qsTr("Minecraft %1").arg(curseForgeDialog.preview.minecraft) : "",
                               (root.loaderNames[curseForgeDialog.preview.loader] || curseForgeDialog.preview.loader || "")
                                   + (curseForgeDialog.preview.loaderVersion ? " " + curseForgeDialog.preview.loaderVersion : "")]
                              .filter(part => part.length > 0).join(" · ")
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    Rectangle {
                        Layout.fillWidth: true; Layout.preferredWidth: 3
                        implicitHeight: 74
                        radius: Theme.radiusMd
                        color: curseForgeDialog.unresolved > 0 ? Qt.alpha(Theme.warning, Theme.light ? 0.10 : 0.14) : (Theme.crystal ? Theme.rowFill : Theme.base)
                        border.color: curseForgeDialog.unresolved > 0 ? Qt.alpha(Theme.warning, 0.55) : (Theme.crystal ? Theme.glassBorder : Theme.border)
                        Accessible.name: qsTr("%1 mods que OpenClient no puede descargar").arg(curseForgeDialog.unresolved)
                        RowLayout {
                            anchors.fill: parent; anchors.margins: 14; spacing: 12
                            Label {
                                objectName: "curseForgeUnresolved"
                                text: curseForgeDialog.unresolved
                                color: curseForgeDialog.unresolved > 0 ? Theme.warning : Theme.ink
                                font.pixelSize: 30; font.weight: Font.DemiBold; font.features: Theme.tabular
                            }
                            Label {
                                Layout.fillWidth: true; wrapMode: Text.WordWrap
                                color: Theme.ink; font.pixelSize: 12
                                text: curseForgeDialog.unresolved === 1 ? qsTr("mod que no se puede descargar sin una clave de CurseForge")
                                    : qsTr("mods que no se pueden descargar sin una clave de CurseForge")
                            }
                        }
                    }
                    Rectangle {
                        Layout.fillWidth: true; Layout.preferredWidth: 2
                        implicitHeight: 74
                        radius: Theme.radiusMd
                        color: Theme.crystal ? Theme.rowFill : Theme.base
                        border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                        Accessible.name: qsTr("%1 archivos incluidos en el paquete").arg(curseForgeDialog.included)
                        RowLayout {
                            anchors.fill: parent; anchors.margins: 14; spacing: 12
                            Label {
                                text: curseForgeDialog.included
                                color: Theme.highlight
                                font.pixelSize: 30; font.weight: Font.DemiBold; font.features: Theme.tabular
                            }
                            Label {
                                Layout.fillWidth: true; wrapMode: Text.WordWrap
                                color: Theme.dim; font.pixelSize: 12
                                text: curseForgeDialog.included === 1 ? qsTr("archivo incluido en el paquete") : qsTr("archivos incluidos en el paquete")
                            }
                        }
                    }
                }

                Label {
                    objectName: "curseForgeMessage"
                    Layout.fillWidth: true; wrapMode: Text.WordWrap
                    color: Theme.ink; lineHeight: 1.25
                    textFormat: Text.PlainText
                    text: curseForgeDialog.preview.message || ""
                }
                Repeater {
                    model: curseForgeDialog.notes.slice(0, 5)
                    Label {
                        required property string modelData
                        Layout.fillWidth: true; wrapMode: Text.WordWrap
                        color: Theme.warning; font.pixelSize: 12
                        textFormat: Text.PlainText
                        text: "• " + modelData
                    }
                }
                Label {
                    visible: curseForgeDialog.notes.length > 5
                    Layout.fillWidth: true; color: Theme.dim; font.pixelSize: 12
                    text: qsTr("y %1 avisos más.").arg(curseForgeDialog.notes.length - 5)
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                Item { Layout.fillWidth: true }
                ActionButton {
                    text: curseForgeDialog.failed ? qsTr("Cerrar") : qsTr("Cancelar")
                    onClicked: curseForgeDialog.close()
                }
                ActionButton {
                    objectName: "curseForgeConfirm"
                    visible: !curseForgeDialog.loading && !curseForgeDialog.failed && curseForgeDialog.preview.needsConfirmation === true
                    text: qsTr("Crear instancia solo con los archivos incluidos")
                    glyph: "plus"
                    primary: true
                    enabled: !curseForgeDialog.busy
                    onClicked: {
                        root.startImport("curseforge", root.curseForgeFile)
                        curseForgeDialog.close()
                    }
                }
            }
        }
    }
    GlassDialog {
        id: removeDialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: 420
        modal: true
        title: qsTr("Retirar instancia")
        contentItem: ColumnLayout {
            spacing: 12
            Label {
                text: qsTr("¿Retirar «%1»? Sus archivos pasarán a la carpeta de recuperación.").arg(root.selected ? root.selected.name : "")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
            Label { id: removeError; visible: text.length > 0; Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.danger }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                ActionButton { text: qsTr("Cancelar"); onClicked: removeDialog.close() }
                ActionButton {
                    text: qsTr("Retirar"); primary: true
                    onClicked: {
                        if (launcher.deleteInstance(root.selected.id))
                            removeDialog.close()
                        else
                            removeError.text = launcher.libraryError
                    }
                }
            }
        }
    }
}
