import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

// Fila de la lista de contenido de una instancia: icono, título con tipo y autor, versión y archivo,
// interruptor y papelera. Los datos salen de los roles de `launcher.content`; la fila no conoce rutas ni
// acciones, solo emite toggleRequested() y removeRequested() para las filas gestionadas (no dependencias).
Item {
    id: row

    required property string title
    required property string author
    required property string version
    required property string fileName
    required property string kind
    required property string iconUrl
    required property string projectId
    required property string updateVersion
    required property bool fileEnabled
    required property bool present
    required property bool managed
    required property bool dependency
    required property var flags
    // Hay una operación de contenido o el juego en marcha: los controles esperan.
    property bool busy: false

    signal toggleRequested()
    signal removeRequested()

    readonly property bool controllable: managed && !dependency && projectId.length > 0
    readonly property bool loose: !managed
    readonly property bool dimmed: !fileEnabled
    readonly property bool hasVersionColumn: width >= 560
    readonly property string kindGlyph: kind === "mod" ? "puzzle" : kind === "resourcepack" ? "image" : "sun"
    readonly property string kindLabel: kind === "mod" ? qsTr("Mod") : kind === "resourcepack" ? qsTr("Textura") : qsTr("Shader")
    readonly property string shownTitle: title.length > 0 ? title : fileName
    // [{ text, tone }] con tone: "warn" | "ok" | "neutral"
    readonly property var badges: {
        const list = []
        if (dependency) list.push({ text: qsTr("Dependencia"), tone: "neutral" })
        if (updateVersion.length > 0) list.push({ text: qsTr("Actualización %1").arg(updateVersion), tone: "ok" })
        if (managed && !present) list.push({ text: qsTr("Sin descargar"), tone: "neutral" })
        if (flags.indexOf("duplicate") >= 0) list.push({ text: qsTr("Duplicado"), tone: "warn" })
        if (flags.indexOf("modifiedLocally") >= 0) list.push({ text: qsTr("Modificado"), tone: "warn" })
        if (flags.indexOf("nameClash") >= 0) list.push({ text: qsTr("Nombre en uso"), tone: "warn" })
        return list
    }

    implicitHeight: 64
    height: 64
    Accessible.role: Accessible.ListItem
    Accessible.name: author.length > 0 ? qsTr("%1, de %2").arg(shownTitle).arg(author) : shownTitle

    HoverHandler { id: hover }

    // Hover suave: solo opacidad.
    Rectangle {
        anchors.fill: parent
        color: Theme.crystal ? Theme.rowFillHover : Theme.ink
        opacity: hover.hovered ? (Theme.crystal ? 1 : Theme.light ? 0.045 : 0.055) : 0
        Behavior on opacity { MotionNumber { role: "fast" } }
    }
    Rectangle {
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        height: 1
        color: Theme.crystal ? Theme.divider : Theme.border
        opacity: Theme.crystal ? 1 : 0.7
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 14; anchors.rightMargin: 14
        spacing: 14

        ProjectIcon {
            Layout.alignment: Qt.AlignVCenter
            size: 48
            source: row.iconUrl
            name: row.shownTitle
            kind: row.kind
            opacity: row.dimmed ? 0.5 : 1
            Behavior on opacity { MotionNumber { role: "base" } }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 0
            Layout.minimumWidth: 90
            Layout.horizontalStretchFactor: 5
            Layout.alignment: Qt.AlignVCenter
            spacing: 3
            opacity: row.dimmed ? 0.55 : 1
            Behavior on opacity { MotionNumber { role: "base" } }

            RowLayout {
                id: titleLine
                Layout.fillWidth: true
                spacing: 7
                Label {
                    Layout.maximumWidth: Math.max(40, titleLine.width - kindMark.width - titleLine.spacing)
                    text: row.shownTitle
                    textFormat: Text.PlainText
                    color: Theme.ink
                    font.pixelSize: 14; font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                Glyph {
                    id: kindMark
                    Layout.preferredWidth: 14; Layout.preferredHeight: 14
                    name: row.kindGlyph
                    color: Theme.dim
                }
                Item { Layout.fillWidth: true }
            }
            RowLayout {
                id: metaLine
                Layout.fillWidth: true
                spacing: 7
                Label {
                    Layout.maximumWidth: Math.max(48, metaLine.width - badgeRow.implicitWidth - metaLine.spacing)
                    text: row.author.length > 0 ? qsTr("de %1").arg(row.author) : row.kindLabel
                    textFormat: Text.PlainText
                    color: Theme.dim
                    font.pixelSize: 12
                    elide: Text.ElideRight
                }
                RowLayout {
                    id: badgeRow
                    spacing: 5
                    Repeater {
                        model: row.badges
                        Rectangle {
                            id: pill
                            required property var modelData
                            readonly property color tone: modelData.tone === "warn" ? Theme.warning
                                : modelData.tone === "ok" ? Theme.success : Theme.dim
                            implicitWidth: pillText.implicitWidth + 12
                            implicitHeight: 17
                            radius: 4
                            color: modelData.tone === "neutral" ? (Theme.crystal ? Theme.rowFillHover : Theme.rim) : Qt.alpha(tone, 0.12)
                            border.width: 1
                            border.color: modelData.tone === "neutral" ? (Theme.crystal ? Theme.glassBorder : Theme.border) : Qt.alpha(tone, 0.5)
                            Label {
                                id: pillText
                                anchors.centerIn: parent
                                text: pill.modelData.text
                                textFormat: Text.PlainText
                                color: pill.tone
                                font.pixelSize: 10; font.weight: Font.DemiBold
                            }
                        }
                    }
                }
                Item { Layout.fillWidth: true }
            }
        }

        ColumnLayout {
            visible: row.hasVersionColumn
            Layout.fillWidth: true
            Layout.preferredWidth: 0
            Layout.minimumWidth: 100
            Layout.horizontalStretchFactor: 4
            Layout.alignment: Qt.AlignVCenter
            spacing: 3
            opacity: row.dimmed ? 0.55 : 1
            Behavior on opacity { MotionNumber { role: "base" } }
            Label {
                Layout.fillWidth: true
                text: row.version.length > 0 ? row.version : "—"
                textFormat: Text.PlainText
                color: row.version.length > 0 ? Theme.ink : Theme.dim
                font.pixelSize: 13; font.weight: Font.DemiBold; font.features: Theme.tabular
                elide: Text.ElideRight
            }
            Label {
                Layout.fillWidth: true
                text: row.fileName.length > 0 ? row.fileName : qsTr("Pendiente de sincronizar")
                textFormat: Text.PlainText
                color: Theme.dim
                font.pixelSize: 11
                elide: Text.ElideMiddle
            }
        }

        // Controles solo para lo que OpenClient gestiona; un archivo suelto aún no se puede cambiar desde aquí.
        Item {
            Layout.preferredWidth: 104
            Layout.preferredHeight: 36
            Layout.alignment: Qt.AlignVCenter

            RowLayout {
                visible: row.controllable
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4

                GlassSwitch {
                    id: toggle
                    implicitWidth: 44; implicitHeight: 34
                    enabled: !row.busy
                    checked: row.fileEnabled
                    Accessible.name: row.fileEnabled ? qsTr("Desactivar %1").arg(row.shownTitle) : qsTr("Activar %1").arg(row.shownTitle)
                    ToolTip.visible: hovered && enabled
                    ToolTip.delay: 600
                    ToolTip.text: row.fileEnabled ? qsTr("Desactivar") : qsTr("Activar")
                    onToggled: row.toggleRequested()
                    // Si la petición no prosperó, al recargar la lista el interruptor vuelve al estado real.
                    Connections {
                        target: launcher
                        function onContentChanged() { toggle.checked = Qt.binding(() => row.fileEnabled) }
                    }
                }

                Button {
                    id: trash
                    hoverEnabled: true
                    padding: 0
                    implicitWidth: 34; implicitHeight: 34
                    enabled: !row.busy
                    scale: down ? 0.94 : 1
                    Behavior on scale { MotionNumber { role: "press" } }
                    Accessible.name: qsTr("Quitar %1").arg(row.shownTitle)
                    ToolTip.visible: hovered && enabled
                    ToolTip.delay: 600
                    ToolTip.text: qsTr("Quitar de la instancia")
                    onClicked: row.removeRequested()
                    background: Rectangle {
                        radius: Theme.radiusSm
                        opacity: trash.enabled ? 1 : 0.5
                        color: trash.down ? Qt.alpha(Theme.danger, 0.24) : trash.hovered ? Qt.alpha(Theme.danger, 0.13) : "transparent"
                        border.width: trash.visualFocus ? 2 : 1
                        border.color: trash.visualFocus ? Theme.highlight : trash.hovered || trash.down ? Qt.alpha(Theme.danger, 0.5) : "transparent"
                        Behavior on color { MotionColor { role: "fast" } }
                    }
                    contentItem: Item {
                        Glyph {
                            anchors.centerIn: parent
                            width: 17; height: 17
                            name: "trash"
                            color: trash.hovered || trash.down ? Theme.danger : Theme.dim
                            opacity: trash.enabled ? 1 : 0.5
                        }
                    }
                }
            }

            Label {
                visible: row.loose
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                horizontalAlignment: Text.AlignRight
                text: qsTr("Archivo suelto")
                textFormat: Text.PlainText
                color: Theme.dim
                font.pixelSize: 11
                elide: Text.ElideLeft
            }
        }
    }
}
