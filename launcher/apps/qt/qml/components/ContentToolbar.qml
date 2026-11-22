import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

// Cabecera de la lista de contenido: búsqueda, orden, filtros por tipo con contador y acciones.
// Trabaja directamente sobre el modelo filtrado (`launcher.content`): query, kind y order.
ColumnLayout {
    id: bar
    objectName: "contentToolbar"
    // ContentFilterModel
    required property var model
    // "all" | "mod" | "resourcepack" | "shader"
    property string filter: "all"
    property bool canAdd: true
    property bool canCheckUpdates: true
    property bool showUpdates: true
    property bool busy: false
    readonly property var counts: model ? model.counts : ({})
    readonly property bool wide: width >= 1010
    readonly property var orders: [
        { id: "name", label: qsTr("Nombre (A-Z)") },
        { id: "nameDesc", label: qsTr("Nombre (Z-A)") },
        { id: "recent", label: qsTr("Más recientes") },
        { id: "enabled", label: qsTr("Activados primero") }
    ]
    readonly property var chips: [
        { id: "all", label: qsTr("Todos"), count: counts.all || 0 },
        { id: "mod", label: qsTr("Mods"), count: counts.mod || 0 },
        { id: "resourcepack", label: qsTr("Texturas"), count: counts.resourcepack || 0 },
        { id: "shader", label: qsTr("Shaders"), count: counts.shader || 0 }
    ]

    // «Quitar filtros» del estado sin resultados: vacía la búsqueda y vuelve a «Todos».
    function clearFilters() {
        search.text = ""
        filterChosen("all")
    }

    signal filterChosen(string kind)
    signal addRequested()
    signal updatesRequested()
    signal refreshRequested()
    signal folderRequested()

    spacing: 10

    component Chip: Button {
        id: chip
        property int count: 0
        property bool picked: false
        hoverEnabled: true
        padding: 0
        implicitHeight: 32
        implicitWidth: chipRow.implicitWidth + 24
        scale: down ? 0.97 : 1
        Behavior on scale { MotionNumber { role: "press" } }
        Accessible.name: qsTr("%1, %2 elementos").arg(text).arg(count)
        background: Rectangle {
            radius: Theme.radiusSm
            color: Theme.crystal
                ? (chip.down ? Qt.alpha(Theme.accent, 0.22) : chip.picked ? Qt.alpha(Theme.accent, 0.16)
                   : chip.hovered ? Theme.rowFillHover : Theme.rowFill)
                : chip.down ? Theme.edge : chip.picked ? Theme.soft
                 : chip.hovered ? Qt.alpha(Theme.ink, Theme.light ? 0.06 : 0.08) : Theme.surface
            border.width: chip.visualFocus ? 2 : 1
            border.color: chip.visualFocus ? (Theme.crystal ? Theme.fieldBorderFocus : Theme.highlight)
                : chip.picked ? (Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge)
                : (Theme.crystal ? Theme.glassBorder : Theme.border)
            Behavior on color { MotionColor { role: "fast" } }
        }
        contentItem: Item {
            implicitWidth: chipRow.implicitWidth
            implicitHeight: chipRow.implicitHeight
            Row {
                id: chipRow
                anchors.centerIn: parent
                spacing: 7
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: chip.text
                    textFormat: Text.PlainText
                    font.family: Theme.fontFamily; font.pixelSize: 13
                    font.weight: chip.picked ? Font.DemiBold : Font.Medium
                    color: chip.picked ? (Theme.crystal ? Theme.accentSoft : Theme.highlight) : Theme.ink
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: chip.count
                    font.family: Theme.fontFamily; font.pixelSize: 12; font.features: Theme.tabular
                    color: chip.picked ? (Theme.crystal ? Theme.accentSoft : Theme.highlight) : Theme.dim
                }
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 10

        GlassField {
            id: search
            objectName: "contentSearch"
            Layout.fillWidth: true
            Layout.preferredHeight: 36
            leadingGlyph: "search"
            placeholderText: qsTr("Buscar en %1 proyectos…").arg(bar.counts.all || 0)
            Accessible.name: qsTr("Buscar en el contenido instalado")
            onTextChanged: if (bar.model) bar.model.query = text
            Keys.onEscapePressed: text = ""
            trailing: Button {
                id: clearSearch
                visible: search.text.length > 0
                width: 24; height: 24
                padding: 0
                hoverEnabled: true
                focusPolicy: Qt.TabFocus
                Accessible.name: qsTr("Borrar la búsqueda")
                onClicked: { search.text = ""; search.forceActiveFocus() }
                background: Rectangle {
                    radius: Theme.radiusSm - 2
                    color: clearSearch.down ? (Theme.crystal ? Theme.rowFillHover : Theme.edge)
                        : clearSearch.hovered ? (Theme.crystal ? Theme.rowFillHover : Qt.alpha(Theme.ink, 0.1)) : "transparent"
                    border.width: clearSearch.visualFocus ? 2 : 0
                    border.color: Theme.highlight
                }
                contentItem: Item {
                    Glyph {
                        anchors.centerIn: parent
                        width: 12; height: 12
                        name: "close"
                        color: clearSearch.hovered ? Theme.ink : Theme.dim
                    }
                }
            }
        }

        GlassComboBox {
            id: sortBox
            objectName: "contentSort"
            Layout.preferredWidth: 210
            Layout.preferredHeight: 36
            model: bar.orders
            textRole: "label"
            currentIndex: Math.max(0, bar.orders.findIndex(item => bar.model && item.id === bar.model.order))
            rightPadding: 34
            Accessible.name: qsTr("Ordenar el contenido")
            onActivated: index => { if (bar.model) bar.model.order = bar.orders[index].id }
            contentItem: Row {
                spacing: 6
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Ordenar")
                    font.family: Theme.fontFamily; font.pixelSize: 12
                    color: Theme.dim
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: sortBox.availableWidth - x
                    text: sortBox.displayText
                    textFormat: Text.PlainText
                    font.family: Theme.fontFamily; font.pixelSize: 12; font.weight: Font.Medium
                    color: Theme.ink
                    elide: Text.ElideRight
                }
            }
        }
    }

    GridLayout {
        Layout.fillWidth: true
        columns: bar.wide ? 3 : 1
        rowSpacing: 10
        columnSpacing: 12

        RowLayout {
            spacing: 6
            Repeater {
                model: bar.chips
                Chip {
                    required property var modelData
                    text: modelData.label
                    count: modelData.count
                    picked: bar.filter === modelData.id
                    onClicked: bar.filterChosen(modelData.id)
                }
            }
        }
        Item { visible: bar.wide; Layout.fillWidth: true }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: 8
            ActionButton {
                objectName: "contentAdd"
                text: qsTr("Añadir contenido"); glyph: "plus"; compact: true; primary: true
                enabled: bar.canAdd
                onClicked: bar.addRequested()
            }
            ActionButton {
                objectName: "contentCheckUpdates"
                visible: bar.showUpdates
                text: qsTr("Buscar actualizaciones"); glyph: "download"; compact: true
                enabled: bar.canCheckUpdates && !bar.busy
                onClicked: bar.updatesRequested()
            }
            ActionButton {
                objectName: "contentRefresh"
                text: qsTr("Actualizar"); glyph: "refresh"; compact: true
                onClicked: bar.refreshRequested()
            }
            ActionButton {
                objectName: "contentOpenFolder"
                text: qsTr("Abrir carpeta"); glyph: "folder"; compact: true
                onClicked: bar.folderRequested()
            }
        }
    }
}
