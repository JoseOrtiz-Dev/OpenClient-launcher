import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

GlassDialog {
    id: notes
    title: qsTr("Novedades de OpenClient")
    glyph: "news"
    signal discoverRequested()

    contentItem: ColumnLayout {
        spacing: 12
        ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                width: scroll.availableWidth
                spacing: 12
                Label {
                    text: qsTr("Versión %1").arg(Qt.application.version)
                    color: Theme.ink
                    font.pixelSize: 22
                    font.weight: Font.DemiBold
                    font.features: Theme.tabular
                }
                Label {
                    Layout.fillWidth: true
                    text: qsTr("Cambios principales incluidos en esta compilación.")
                    color: Theme.dim; wrapMode: Text.WordWrap
                }
                Repeater {
                    model: [
                        { glyph: "puzzle", title: qsTr("Catálogo de Modrinth integrado"),
                          detail: qsTr("Busca e instala Modpacks, Mods, Texturas y Shaders seleccionando la versión compatible y revisando dependencias.") },
                        { glyph: "layers", title: qsTr("Superficies en cristal opaco y temas"),
                          detail: qsTr("Cinco paletas revisadas con mejor contraste de lectura, estados de error diferenciados y transiciones suaves.") },
                        { glyph: "code", title: qsTr("Créditos y comunidad"),
                          detail: qsTr("Acceso directo al servidor de Discord del proyecto y ficha de desarrollo de Jose Ortiz.") }
                    ]
                    GlassPanel {
                        required property var modelData
                        Layout.fillWidth: true
                        implicitHeight: feature.implicitHeight + 28
                        padding: 14
                        frosted: false
                        RowLayout {
                            id: feature
                            anchors.fill: parent
                            spacing: 14
                            Rectangle {
                                width: 40; height: 40
                                radius: Theme.radiusMd
                                color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
                                border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
                                Glyph { anchors.centerIn: parent; name: modelData.glyph; color: Theme.highlight }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 4
                                Label { text: modelData.title; color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
                                Label { Layout.fillWidth: true; text: modelData.detail; color: Theme.dim; wrapMode: Text.WordWrap }
                            }
                        }
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            ActionButton { text: qsTr("Cerrar"); onClicked: notes.close() }
            ActionButton { text: qsTr("Explorar catálogo"); glyph: "compass"; primary: true; onClicked: notes.discoverRequested() }
        }
    }
}
