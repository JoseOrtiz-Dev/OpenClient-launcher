import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

ColumnLayout {
    id: root
    spacing: 20
    Label { text: qsTr("Créditos"); color: Theme.ink; font.pixelSize: 24; font.weight: Font.DemiBold }
    Label {
        Layout.fillWidth: true; wrapMode: Text.WordWrap
        text: qsTr("Desarrollo y diseño de OpenClient.")
        color: Theme.dim; font.pixelSize: 13
    }
    GlassPanel {
        Layout.fillWidth: true
        implicitHeight: developer.implicitHeight + 52
        accented: true
        padding: 26
        RowLayout {
            id: developer
            anchors.fill: parent
            spacing: 24
            Rectangle {
                Layout.preferredWidth: root.width < 760 ? 100 : 136
                Layout.preferredHeight: width
                radius: Theme.radiusXl
                color: Theme.crystal ? Theme.rowFill : Theme.soft
                border.color: Theme.crystal ? Theme.glassBorder : Theme.edge
                AnimatedImage {
                    anchors.fill: parent
                    anchors.margins: 12
                    source: "qrc:/assets/patito.gif"
                    fillMode: Image.PreserveAspectFit
                    playing: root.visible && Qt.application.state === Qt.ApplicationActive
                    Accessible.name: "Patito animado de Jose Ortiz"
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 8
                Label { text: qsTr("Autor principal"); color: Theme.highlight; font.pixelSize: 13 }
                Label { Layout.fillWidth: true; wrapMode: Text.WordWrap; text: qsTr("Jose Ortiz"); color: Theme.ink; font.pixelSize: root.width < 760 ? 26 : 32; font.weight: Font.DemiBold }
                Label { text: qsTr("Arquitectura nativa C++/Qt Quick y servicios en Rust"); color: Theme.dim; font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            }
            Glyph {
                visible: root.width >= 860
                Layout.preferredWidth: 44; Layout.preferredHeight: 44
                name: "code"; color: Theme.edge
            }
        }
    }
    RowLayout {
        Layout.fillWidth: true; spacing: 20
        Image {
            source: "qrc:/assets/openclient-logo.svg"; fillMode: Image.PreserveAspectFit
            Layout.preferredWidth: root.width < 760 ? 210 : 280
            Layout.preferredHeight: 84
            Accessible.name: "OpenClient"
        }
        Item { Layout.fillWidth: true }
        ColumnLayout {
            spacing: 5
            Label { text: qsTr("Versión %1").arg(Qt.application.version); color: Theme.ink; font.pixelSize: 13; font.features: Theme.tabular }
            Label { text: qsTr("Interfaz · C++20 y Qt Quick"); color: Theme.dim; font.pixelSize: 12 }
            Label { text: qsTr("Motor · Rust (ABI C v3)"); color: Theme.dim; font.pixelSize: 12 }
        }
    }
    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
    RowLayout {
        Layout.fillWidth: true; spacing: 18
        ColumnLayout {
            Layout.fillWidth: true; spacing: 5
            Label { text: qsTr("Comunidad en Discord"); color: Theme.ink; font.pixelSize: 17; font.weight: Font.DemiBold }
            Label {
                Layout.fillWidth: true; wrapMode: Text.WordWrap
                text: qsTr("Reporta fallos, sugiere mejoras o consulta novedades del desarrollo.")
                color: Theme.dim; font.pixelSize: 13
            }
        }
        ActionButton {
            text: qsTr("Discord oficial"); glyph: "discord"; primary: true
            enabled: launcher.linkAvailable(1)
            onClicked: launcher.openLink(1)
        }
    }
}
