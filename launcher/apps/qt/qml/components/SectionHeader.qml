import QtQuick
import QtQuick.Layouts
import OpenClient

GlassPanel {
    id: root
    property string eyebrow: ""
    property string title: ""
    property string description: ""
    property string glyph: "grid"
    property string detail: ""
    implicitHeight: 112
    padding: 20
    accented: true
    clip: true

    CrystalArt { anchors.fill: parent; anchors.margins: -root.padding + 1; banner: true }

    RowLayout {
        anchors.fill: parent
        spacing: 18
        Rectangle {
            width: 48; height: 48
            radius: Theme.radiusMd
            color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
            border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
            Glyph { anchors.centerIn: parent; width: 22; height: 22; name: root.glyph; color: Theme.highlight }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 4
            Text {
                text: root.title
                color: Theme.ink
                font.family: Theme.fontFamily
                font.pixelSize: 22
                font.weight: Font.DemiBold
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            Text {
                visible: root.description.length > 0
                text: root.description
                color: Theme.dim
                font.family: Theme.fontFamily
                font.pixelSize: 12
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
        }
        Rectangle {
            visible: root.detail.length > 0
            Layout.preferredWidth: detailText.implicitWidth + 22
            Layout.preferredHeight: 30
            radius: Theme.radiusSm
            color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
            border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
            Text {
                id: detailText
                anchors.centerIn: parent
                text: root.detail
                textFormat: Text.PlainText
                color: Theme.highlight
                font.family: Theme.fontFamily
                font.pixelSize: 11
                font.weight: Font.Medium
                font.features: Theme.tabular
            }
        }
    }
}
