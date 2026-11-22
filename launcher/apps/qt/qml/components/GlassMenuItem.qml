import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

MenuItem {
    id: root
    property string glyph: ""
    property bool danger: false
    readonly property color tone: danger ? Theme.danger : Theme.ink

    implicitHeight: 36
    leftPadding: 10
    rightPadding: 10
    indicator: null
    arrow: null

    contentItem: RowLayout {
        spacing: 10
        opacity: root.enabled ? 1 : 0.4

        Glyph {
            visible: root.glyph !== ""
            name: root.glyph
            color: root.danger ? Theme.danger : root.highlighted ? Theme.highlight : Theme.dim
            Layout.preferredWidth: 16
            Layout.preferredHeight: 16
        }
        Text {
            text: root.text
            color: root.tone
            font.family: Theme.fontFamily
            font.pixelSize: 13
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            Layout.fillWidth: true
        }
    }

    background: Rectangle {
        radius: Theme.radiusSm
        readonly property bool on: root.highlighted && root.enabled
        color: !on ? "transparent"
            : root.danger ? Qt.alpha(Theme.danger, root.down ? 0.22 : 0.14)
            : Theme.crystal ? (root.down ? Qt.alpha(Theme.accent, 0.18) : Theme.rowFillHover)
            : root.down ? Qt.alpha(Theme.accent, 0.22) : Theme.soft
        border.width: Theme.crystal && on ? 1 : 0
        border.color: root.danger ? Qt.alpha(Theme.danger, 0.3) : Theme.glassBorder
        Behavior on color { MotionColor { role: "fast" } }
    }
}
