import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

ColumnLayout {
    id: root
    objectName: "officialPage"
    spacing: 16
    readonly property var pack: launcher.officialPack
    readonly property bool working: launcher.copyingInstance || launcher.gameBusy || launcher.catalogBusy
    readonly property bool running: launcher.runningInstanceIds.indexOf(pack.installedId) !== -1
    readonly property bool preparing: !!pack.installedId && launcher.preparingInstanceId === pack.installedId
    readonly property int totalItems: (pack.mods || []).length + (pack.shaders || []).length + (pack.resourcepacks || []).length
    Component.onCompleted: launcher.inspectOfficialPack()

    component ContentList: GlassPanel {
        id: list
        property string title
        property string glyph
        property var items: []
        Layout.fillWidth: true
        visible: items.length > 0
        implicitHeight: listBody.implicitHeight + 36
        padding: 18
        ColumnLayout {
            id: listBody
            anchors.fill: parent
            spacing: 12
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Glyph { Layout.preferredWidth: 16; Layout.preferredHeight: 16; name: list.glyph; color: Theme.highlight }
                Text {
                    text: list.title
                    color: Theme.ink
                    font.family: Theme.fontFamily
                    font.pixelSize: 14
                    font.weight: Font.DemiBold
                }
                Item { Layout.fillWidth: true }
                Text {
                    text: list.items.length
                    color: Theme.dim
                    font.family: Theme.fontFamily
                    font.pixelSize: 12
                    font.features: Theme.tabular
                }
            }
            Flow {
                Layout.fillWidth: true
                spacing: 6
                Repeater {
                    model: list.items
                    Rectangle {
                        required property string modelData
                        implicitWidth: Math.min(chip.implicitWidth + 18, listBody.width)
                        implicitHeight: 26
                        radius: Theme.radiusSm
                        color: Theme.crystal ? Theme.rowFill : Theme.soft
                        border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                        Text {
                            id: chip
                            anchors.centerIn: parent
                            width: Math.min(implicitWidth, parent.width - 18)
                            text: modelData
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            color: Theme.ink
                            font.family: Theme.fontFamily
                            font.pixelSize: 12
                        }
                    }
                }
            }
        }
    }

    SectionHeader {
        Layout.fillWidth: true
        glyph: "star"
        title: root.pack.name || qsTr("Modpack oficial")
        description: root.pack.minecraft
            ? (root.pack.fabric ? qsTr("Minecraft %1 · Fabric %2").arg(root.pack.minecraft).arg(root.pack.fabric) : qsTr("Minecraft %1").arg(root.pack.minecraft))
            : qsTr("Instancia recomendada por OpenClient.")
        detail: root.pack.version ? qsTr("v%1").arg(root.pack.version) : ""
    }

    GlassPanel {
        Layout.fillWidth: true
        implicitHeight: statusBody.implicitHeight + 40
        padding: 20
        ColumnLayout {
            id: statusBody
            anchors.fill: parent
            spacing: 12
            Text {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                color: root.pack.error ? Theme.danger : Theme.ink
                font.family: Theme.fontFamily
                font.pixelSize: 13
                text: {
                    if (root.pack.error) return root.pack.error
                    switch (root.pack.state) {
                    case "missing": return qsTr("No se encontró el archivo del modpack. Copia el .mrpack como modpack/official.mrpack junto a openclient-qt.exe.")
                    case "download": return qsTr("El modpack oficial se descarga una sola vez (%1 MB) y se comprueba su integridad antes de instalarlo.").arg(root.pack.downloadSizeMb)
                    case "ready": return qsTr("Listo para instalar. OpenClient descarga los mods desde Modrinth y aplica las configuraciones incluidas.")
                    case "update": return qsTr("Tienes la v%1 y hay una v%2. Al actualizar, OpenClient guarda antes una copia de seguridad por si quieres volver atrás. Tus mundos no se sobrescriben.")
                        .arg(root.pack.installedVersion).arg(root.pack.version)
                    default: return root.pack.installedVersion ? qsTr("Instalado · v%1").arg(root.pack.installedVersion) : qsTr("Instalado.")
                    }
                }
            }
            Text {
                visible: launcher.copyMessage.length > 0 && !launcher.operationActive
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                textFormat: Text.PlainText
                text: launcher.copyMessage
                color: Theme.dim
                font.family: Theme.fontFamily
                font.pixelSize: 12
            }
            OperationBar {
                Layout.fillWidth: true
                heading: launcher.copyMessage
            }
            ProgressBar {
                visible: launcher.copyingInstance && !launcher.operationActive
                Layout.fillWidth: true
                indeterminate: true
                Accessible.name: "Preparando la operación"
            }
            RowLayout {
                spacing: 10
                ActionButton {
                    visible: root.pack.state === "installed" || root.pack.state === "update"
                    text: root.running ? qsTranslate("PlayButton", "En juego")
                        : root.preparing ? qsTranslate("PlayButton", "Preparando %1 %").arg(launcher.gamePercent)
                        : launcher.gameRunning ? qsTranslate("PlayButton", "Abrir con cuenta…")
                        : root.pack.state === "update" ? qsTr("Jugar v%1").arg(root.pack.installedVersion) : qsTr("Jugar")
                    glyph: "play"
                    primary: root.pack.state === "installed"
                    enabled: !root.running && !launcher.preparingInstanceId.length && !launcher.copyingInstance && !launcher.catalogBusy
                    onClicked: launcher.requestPlayInstance(root.pack.installedId)
                }
                ActionButton {
                    visible: root.pack.state === "ready" || root.pack.state === "update" || root.pack.state === "download"
                    text: root.pack.state === "update" ? qsTr("Actualizar a v%1").arg(root.pack.version)
                        : root.pack.state === "download" ? qsTr("Descargar e instalar")
                        : root.pack.version ? qsTr("Instalar v%1").arg(root.pack.version) : qsTr("Instalar modpack")
                    glyph: root.pack.state === "update" ? "refresh" : root.pack.state === "download" ? "download" : "plus"
                    primary: true
                    enabled: !root.working && !root.pack.loading
                    onClicked: launcher.installOfficialPack()
                }
            }
        }
    }

    Text {
        visible: root.pack.loading
        text: qsTr("Leyendo el contenido del modpack…")
        color: Theme.dim
        font.family: Theme.fontFamily
        font.pixelSize: 12
    }
    ContentList { title: qsTr("Mods"); glyph: "puzzle"; items: root.pack.mods || [] }
    ContentList { title: qsTr("Shaders"); glyph: "sun"; items: root.pack.shaders || [] }
    ContentList { title: qsTr("Paquetes de recursos"); glyph: "image"; items: root.pack.resourcepacks || [] }
}
