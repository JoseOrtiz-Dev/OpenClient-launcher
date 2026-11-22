import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs as NativeDialogs
import QtQuick.Layouts
import OpenClient

ColumnLayout {
    id: root
    spacing: 16
    readonly property bool sideBySide: width >= 780
    property var selectedForRemoval: null
    readonly property var savedSkin: wardrobe.collection.find(skin => skin.id === wardrobe.savedSkinId)

    GridLayout {
        Layout.fillWidth: true
        columns: root.sideBySide ? 2 : 1
        columnSpacing: 16
        rowSpacing: 16

        GlassPanel {
            id: previewPanel
            Layout.fillWidth: true
            Layout.preferredWidth: root.sideBySide ? root.width * 0.52 : root.width
            Layout.alignment: Qt.AlignTop
            implicitHeight: previewContent.implicitHeight + padding * 2
            padding: 16

            ColumnLayout {
                id: previewContent
                anchors.fill: parent
                spacing: 12

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label {
                            Layout.fillWidth: true
                            text: qsTr("Visor 3D")
                            color: Theme.ink
                            font.pixelSize: 19
                            font.weight: Font.DemiBold
                        }
                        Label {
                            Layout.fillWidth: true
                            text: wardrobe.hasSkin ? wardrobe.name : qsTr("Añade un PNG o busca un jugador")
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            color: Theme.dim
                            font.pixelSize: 12
                        }
                    }
                    GlassComboBox {
                        id: modelPicker
                        legacyFill: Theme.surface
                        Layout.preferredWidth: 134
                        Layout.preferredHeight: 36
                        model: ["Clásico · 4 px", qsTr("Slim · 3 px")]
                        currentIndex: wardrobe.slim ? 1 : 0
                        enabled: wardrobe.hasSkin && !wardrobe.legacy && !wardrobe.busy
                        Accessible.name: "Modelo de la skin"
                        onActivated: index => wardrobe.slim = index === 1
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: root.sideBySide ? 396 : 272
                    radius: Theme.radiusMd
                    color: Theme.fieldFill
                    gradient: Theme.crystal ? null : legacyPreviewFill
                    border.color: modelPreview.activeFocus ? (Theme.crystal ? Theme.fieldBorderFocus : Theme.highlight)
                        : (Theme.crystal ? Theme.glassBorder : Theme.border)
                    Gradient {
                        id: legacyPreviewFill
                        GradientStop { position: 0; color: Theme.card }
                        GradientStop { position: 1; color: Theme.base }
                    }
                    clip: true

                    SkinPreview {
                        id: modelPreview
                        anchors.fill: parent
                        anchors.margins: 12
                        activeFocusOnTab: true
                        skin: wardrobe.skin
                        cape: wardrobe.cape
                        slim: wardrobe.slim
                        capeVisible: capeSwitch.checked
                        Accessible.name: "Visor 3D"
                        Accessible.description: qsTr("Usa las flechas para girar.")
                        Keys.onLeftPressed: orbit(-18, 0)
                        Keys.onRightPressed: orbit(18, 0)
                        Keys.onUpPressed: orbit(0, -18)
                        Keys.onDownPressed: orbit(0, 18)
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                        preventStealing: true
                        property point previous
                        onPressed: mouse => {
                            previous = Qt.point(mouse.x, mouse.y)
                            modelPreview.forceActiveFocus()
                        }
                        onPositionChanged: mouse => {
                            if (!pressed) return
                            modelPreview.orbit(mouse.x - previous.x, mouse.y - previous.y)
                            previous = Qt.point(mouse.x, mouse.y)
                        }
                        onWheel: wheel => {
                            modelPreview.zoomBy(wheel.angleDelta.y / 120)
                            wheel.accepted = true
                        }
                    }
                }

                Flow {
                    Layout.fillWidth: true
                    spacing: 8
                    ActionButton {
                        text: qsTr("Frente")
                        compact: true
                        onClicked: modelPreview.setView(false)
                    }
                    ActionButton {
                        text: qsTr("Espalda")
                        compact: true
                        onClicked: modelPreview.setView(true)
                    }
                    ActionButton {
                        id: capeSwitch
                        visible: wardrobe.hasCape
                        text: qsTr("Mostrar capa")
                        compact: true
                        checkable: true
                        checked: true
                        selected: checked
                        glyph: checked ? "check" : "layers"
                    }
                }
                Label {
                    Layout.fillWidth: true
                    text: qsTr("Arrastra para girar · Usa la rueda para acercar")
                    color: Theme.dim
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                }
                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                Flow {
                    visible: wardrobe.hasSkin
                    Layout.fillWidth: true
                    spacing: 8
                    ActionButton {
                        text: root.savedSkin ? qsTr("Guardar cambios") : qsTr("Guardar skin")
                        glyph: "plus"
                        compact: true
                        enabled: !wardrobe.busy && (!root.savedSkin || root.savedSkin.slim !== wardrobe.slim)
                        onClicked: wardrobe.saveSkin()
                    }
                    ActionButton {
                        text: qsTr("Exportar PNG")
                        compact: true
                        enabled: !wardrobe.busy
                        onClicked: exportPicker.open()
                    }
                }
            }
        }

        GlassPanel {
            Layout.fillWidth: true
            Layout.preferredWidth: root.sideBySide ? root.width * 0.48 : root.width
            Layout.alignment: Qt.AlignTop
            implicitHeight: wardrobeContent.implicitHeight + padding * 2
            padding: 16

            ColumnLayout {
                id: wardrobeContent
                anchors.fill: parent
                spacing: 10
                TabBar {
                    id: wardrobeTabs
                    Layout.fillWidth: true
                    Layout.preferredHeight: 40
                    background: Rectangle {
                        radius: Theme.radiusSm
                        color: Theme.crystal ? Theme.fieldFill : Theme.base
                    }
                    onCurrentIndexChanged: {
                        if (modelPreview) modelPreview.setView(currentIndex === 1)
                    }
                    Repeater {
                        model: 2
                        delegate: TabButton {
                            id: tab
                            required property int index
                            implicitHeight: 40
                            text: index === 0 ? qsTr("Skins") : qsTr("Capas (%1)").arg(wardrobe.capes.length)
                            font.weight: checked ? Font.DemiBold : Font.Medium
                            contentItem: Label {
                                text: tab.text
                                color: tab.checked ? Theme.ink : Theme.dim
                                font: tab.font
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                            }
                            background: Rectangle {
                                radius: Theme.radiusSm
                                color: Theme.crystal ? (tab.checked ? Qt.alpha(Theme.accent, 0.16) : tab.hovered ? Theme.rowFillHover : "transparent")
                                    : tab.checked ? Theme.soft : tab.hovered ? Theme.surface : "transparent"
                                border.color: tab.activeFocus ? (Theme.crystal ? Theme.fieldBorderFocus : Theme.highlight)
                                    : tab.checked ? (Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge) : "transparent"
                            }
                        }
                    }
                }

                ColumnLayout {
                    visible: wardrobeTabs.currentIndex === 0
                    Layout.fillWidth: true
                    spacing: 12

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            Layout.fillWidth: true
                            text: qsTr("Mi colección (%1)").arg(wardrobe.collection.length)
                            color: Theme.ink
                            font.pixelSize: 16
                            font.weight: Font.DemiBold
                        }
                        ActionButton {
                            text: qsTr("Añadir PNG")
                            glyph: "plus"
                            compact: true
                            primary: true
                            enabled: !wardrobe.busy
                            onClicked: picker.open()
                        }
                    }
                    Label {
                        visible: wardrobe.collection.length === 0
                        Layout.fillWidth: true
                        text: qsTr("Añade un PNG para guardarlo en este equipo.")
                        wrapMode: Text.WordWrap
                        color: Theme.dim
                        font.pixelSize: 12
                    }
                    ListView {
                        id: saved
                        visible: wardrobe.collection.length > 0
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.min(226, wardrobe.collection.length * 58 - 6)
                        spacing: 6
                        clip: true
                        model: wardrobe.collection
                        ScrollBar.vertical: ScrollBar {}
                        delegate: Rectangle {
                            id: skinRow
                            required property var modelData
                            readonly property bool shown: modelData.id === wardrobe.savedSkinId
                            width: saved.width - 8
                            height: 52
                            radius: Theme.radiusSm
                            color: Theme.crystal ? (shown ? Qt.alpha(Theme.accent, 0.16) : Theme.rowFill)
                                : shown ? Theme.soft : Theme.surface
                            border.color: shown ? Theme.highlight : (Theme.crystal ? Theme.glassBorder : Theme.border)
                            RowLayout {
                                anchors.fill: parent
                                anchors.margins: 6
                                spacing: 7
                                Image {
                                    Layout.preferredWidth: 34
                                    Layout.preferredHeight: 34
                                    source: skinRow.modelData.icon
                                    smooth: false
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2
                                    Label {
                                        Layout.fillWidth: true
                                        text: skinRow.modelData.name || qsTr("Skin guardada")
                                        textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.ink
                                    }
                                    Label { text: skinRow.modelData.slim ? qsTr("Slim") : qsTr("Clásico"); color: Theme.dim; font.pixelSize: 10 }
                                }
                                Label {
                                    visible: skinRow.shown
                                    text: qsTr("En el visor")
                                    color: Theme.ink
                                    font.pixelSize: 11
                                }
                                ActionButton {
                                    visible: !skinRow.shown
                                    text: qsTr("Ver")
                                    compact: true
                                    enabled: !wardrobe.busy
                                    onClicked: wardrobe.openSkin(skinRow.modelData.id)
                                }
                                ActionButton {
                                    glyph: "close"; compact: true
                                    ToolTip.text: qsTr("Quitar skin guardada")
                                    enabled: !wardrobe.busy
                                    onClicked: {
                                        root.selectedForRemoval = skinRow.modelData
                                        removeDialog.open()
                                    }
                                }
                            }
                        }
                    }

                    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                    Label {
                        text: qsTr("Buscar jugador")
                        color: Theme.ink
                        font.pixelSize: 16
                        font.weight: Font.DemiBold
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        GlassField {
                            id: player
                            Layout.fillWidth: true
                            Layout.minimumWidth: 75
                            Layout.preferredHeight: 36
                            placeholderText: qsTr("Nombre de Minecraft")
                            maximumLength: 16
                            enabled: !wardrobe.busy
                            Accessible.name: "Nombre del jugador de Minecraft Java"
                            onAccepted: if (text.trim().length > 0) wardrobe.lookup(text)
                        }
                        ActionButton {
                            text: wardrobe.busy ? qsTr("Cargando…") : qsTr("Buscar")
                            glyph: "search"; compact: true; primary: true
                            enabled: !wardrobe.busy && player.text.trim().length > 0
                            onClicked: wardrobe.lookup(player.text)
                        }
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 7
                        ActionButton {
                            visible: launcher.accountName.length > 0
                            text: qsTr("Mi perfil")
                            ToolTip.text: launcher.accountName
                            glyph: "shirt"; compact: true
                            enabled: !wardrobe.busy
                            onClicked: {
                                player.text = launcher.accountName
                                wardrobe.lookup(launcher.accountName)
                            }
                        }
                    }
                }

                ColumnLayout {
                    visible: wardrobeTabs.currentIndex === 1
                    Layout.fillWidth: true
                    spacing: 12
                    Label {
                        Layout.fillWidth: true
                        text: wardrobe.capes.length > 0
                            ? qsTr("Selecciona una capa para verla en el modelo.")
                            : qsTr("Este perfil no tiene capas oficiales disponibles.")
                        wrapMode: Text.WordWrap
                        color: Theme.dim
                        font.pixelSize: 12
                    }
                    Flow {
                        id: capeGrid
                        visible: wardrobe.capes.length > 0
                        Layout.fillWidth: true
                        spacing: 8
                        readonly property int columns: Math.max(1, Math.min(4, Math.floor((width + spacing) / 132)))

                        Repeater {
                            model: wardrobe.capes
                            delegate: Button {
                                id: capeTile
                                required property var modelData
                                readonly property bool chosen: wardrobe.selectedCapeId === modelData.id
                                readonly property bool equipped: launcher.usingMicrosoft && modelData.official
                                    && launcher.activeCapeId === modelData.id
                                width: Math.floor((capeGrid.width - capeGrid.spacing * (capeGrid.columns - 1)) / capeGrid.columns)
                                height: 58
                                padding: 8
                                text: modelData.alias || qsTr("Capa")
                                hoverEnabled: true
                                enabled: !wardrobe.busy
                                Accessible.checkable: true
                                Accessible.checked: chosen
                                readonly property string capeStatus: equipped ? qsTr("Equipada") : modelData.source || ""
                                Accessible.description: capeStatus
                                ToolTip.visible: hovered && capeName.truncated
                                ToolTip.delay: 500
                                ToolTip.text: text
                                onClicked: {
                                    capeSwitch.checked = true
                                    wardrobe.selectCape(modelData.id)
                                }

                                background: Rectangle {
                                    radius: Theme.radiusSm
                                    color: Theme.crystal
                                        ? (capeTile.chosen || capeTile.down ? Qt.alpha(Theme.accent, 0.16) : capeTile.hovered ? Theme.rowFillHover : Theme.rowFill)
                                        : capeTile.chosen || capeTile.down ? Theme.soft
                                        : capeTile.hovered ? Theme.card : Theme.surface
                                    border.width: capeTile.activeFocus ? 2 : 1
                                    border.color: capeTile.activeFocus || capeTile.chosen ? Theme.highlight : (Theme.crystal ? Theme.glassBorder : Theme.border)
                                }

                                contentItem: RowLayout {
                                    spacing: 8
                                    Rectangle {
                                        Layout.preferredWidth: 28
                                        Layout.preferredHeight: 40
                                        radius: Theme.radiusSm
                                        color: Theme.crystal ? Theme.fieldFill : Theme.base
                                        Image {
                                            id: capeImage
                                            anchors.fill: parent
                                            anchors.margins: 3
                                            source: capeTile.modelData.icon || ""
                                            smooth: false
                                            fillMode: Image.PreserveAspectFit
                                        }
                                        Glyph {
                                            anchors.centerIn: parent
                                            width: 16
                                            height: 16
                                            visible: capeImage.status !== Image.Ready
                                            name: "layers"
                                            color: Theme.dim
                                        }
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 2
                                        Label {
                                            id: capeName
                                            Layout.fillWidth: true
                                            text: capeTile.text
                                            textFormat: Text.PlainText
                                            elide: Text.ElideRight
                                            color: Theme.ink
                                            font.pixelSize: 12
                                            font.weight: capeTile.chosen ? Font.DemiBold : Font.Normal
                                        }
                                        Label {
                                            Layout.fillWidth: true
                                            text: capeTile.capeStatus
                                            textFormat: Text.PlainText
                                            elide: Text.ElideRight
                                            color: capeTile.equipped ? Theme.success : Theme.dim
                                            font.pixelSize: 11
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Flow {
                        visible: wardrobe.capes.length > 0
                        Layout.fillWidth: true
                        spacing: 8
                        ActionButton {
                            text: qsTr("Sin capa")
                            compact: true
                            enabled: !wardrobe.busy
                            selected: wardrobe.selectedCapeId.length === 0
                            onClicked: wardrobe.selectCape("none")
                        }
                        ActionButton {
                            readonly property var chosenCape: wardrobe.capes.find(c => c.id === wardrobe.selectedCapeId)
                            visible: launcher.usingMicrosoft && !!chosenCape && chosenCape.official
                                && launcher.activeCapeId !== wardrobe.selectedCapeId
                            text: qsTr("Equipar en mi cuenta"); glyph: "shirt"; compact: true; primary: true
                            enabled: !launcher.microsoftBusy && !launcher.gameBusy
                            onClicked: launcher.equipMicrosoftCape(wardrobe.selectedCapeId)
                        }
                    }
                    Label {
                        visible: launcher.usingMicrosoft && launcher.microsoftMessage.length > 0 && wardrobe.capes.length > 0
                        Layout.fillWidth: true
                        text: launcher.microsoftMessage
                        textFormat: Text.PlainText; wrapMode: Text.WordWrap
                        color: Theme.dim; font.pixelSize: 11
                    }
                }

                Label {
                    Layout.fillWidth: true
                    visible: text.length > 0
                    text: wardrobe.message
                    textFormat: Text.PlainText; wrapMode: Text.WordWrap
                    color: Theme.dim; font.pixelSize: 11
                }
            }
        }
    }

    NativeDialogs.FileDialog {
        id: picker
        title: qsTr("Añadir una skin a tu colección")
        nameFilters: ["Imágenes PNG (*.png)"]
        onAccepted: wardrobe.importPng(selectedFile)
    }
    NativeDialogs.FileDialog {
        id: exportPicker
        title: qsTr("Exportar la skin en PNG")
        fileMode: NativeDialogs.FileDialog.SaveFile
        nameFilters: ["Imágenes PNG (*.png)"]
        defaultSuffix: "png"
        onAccepted: wardrobe.exportPng(selectedFile)
    }

    GlassDialog {
        id: removeDialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: 390
        title: qsTr("Quitar skin guardada")
        glyph: "shirt"
        contentItem: ColumnLayout {
            spacing: 13
            Label {
                Layout.fillWidth: true
                text: qsTr("¿Quitar «%1» de tu colección?").arg(root.selectedForRemoval?.name || qsTr("Skin guardada"))
                textFormat: Text.PlainText; wrapMode: Text.WordWrap; color: Theme.ink
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                ActionButton { text: qsTr("Cancelar"); onClicked: removeDialog.close() }
                ActionButton {
                    text: qsTr("Quitar"); primary: true
                    onClicked: {
                        wardrobe.removeSkin(root.selectedForRemoval.id)
                        removeDialog.close()
                    }
                }
            }
        }
    }
}
