import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import OpenClient

Button {
    id: root
    property string glyph: ""
    property bool primary: false
    property bool selected: false
    property bool compact: false
    property bool brand: false
    property bool navigation: false
    property bool iconTile: false
    readonly property color foreground: primary ? Theme.accentText : selected || brand ? Theme.highlight : Theme.ink
    implicitHeight: compact ? 36 : 42
    implicitWidth: Math.max(compact ? 36 : 90, (contentItem ? contentItem.implicitWidth : 0) + 24)
    hoverEnabled: true
    leftPadding: navigation ? 12 : 10
    rightPadding: 10
    scale: root.down && root.enabled ? 0.985 : 1.0
    Behavior on scale { MotionNumber { role: "press" } }
    Accessible.name: text.length ? text : ToolTip.text
    ToolTip.visible: hovered && ToolTip.text.length > 0
    ToolTip.delay: 500
    background: Rectangle {
        radius: root.navigation ? Theme.radiusMd : Theme.radiusSm
        antialiasing: true
        color: Theme.crystal ? (root.primary ? "transparent"
                                : root.selected || root.brand ? Qt.alpha(Theme.accent, root.down ? 0.24 : root.hovered ? 0.2 : 0.16)
                                : root.down ? Qt.tint(Theme.rowFillHover, Theme.rowFill)
                                : root.hovered ? Theme.rowFillHover
                                : root.navigation ? "transparent" : Theme.rowFill)
                            : root.primary ? (root.down ? Qt.darker(Theme.accent, 1.15) : root.hovered ? Qt.lighter(Theme.accent, 1.07) : Theme.accent)
                            : root.down ? Theme.edge
                            : root.selected || root.brand ? Theme.soft
                            : root.hovered ? Qt.alpha(Theme.ink, Theme.light ? 0.06 : 0.08)
                            : root.navigation ? "transparent" : Theme.surface
        Behavior on color { MotionColor { role: "fast" } }
        border.width: root.activeFocus ? 2 : 1
        border.color: root.activeFocus ? (Theme.crystal ? Theme.fieldBorderFocus : Theme.highlight)
                    : root.primary ? Qt.alpha("white", Theme.crystal ? 0.14 : Theme.light ? 0.22 : 0.14)
                    : root.selected || root.brand ? (Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge)
                    : Theme.crystal ? (root.hovered || root.down ? Theme.glassBorderHover : root.navigation ? "transparent" : Theme.glassBorder)
                    : root.navigation ? "transparent" : Theme.border
        Behavior on border.color { enabled: Theme.crystal; MotionColor { role: "fast" } }
        opacity: root.enabled ? 1 : 0.5

        // Cristal, primario: degradado diagonal (135 grados) btnFrom a btnTo bajo el borde.
        Loader {
            anchors.fill: parent
            anchors.margins: 1
            active: Theme.crystal && root.primary
            sourceComponent: Shape {
                id: fill
                preferredRendererType: Shape.CurveRenderer
                ShapePath {
                    strokeWidth: -1
                    fillGradient: LinearGradient {
                        x1: 0; y1: 0; x2: fill.width; y2: fill.height
                        GradientStop {
                            position: 0
                            color: root.down ? Qt.darker(Theme.btnFrom, 1.12) : root.hovered ? Qt.lighter(Theme.btnFrom, 1.08) : Theme.btnFrom
                            Behavior on color { MotionColor { role: "fast" } }
                        }
                        GradientStop {
                            position: 1
                            color: root.down ? Qt.darker(Theme.btnTo, 1.12) : root.hovered ? Qt.lighter(Theme.btnTo, 1.08) : Theme.btnTo
                            Behavior on color { MotionColor { role: "fast" } }
                        }
                    }
                    PathRectangle {
                        width: fill.width; height: fill.height
                        radius: Math.max(0, (root.navigation ? Theme.radiusMd : Theme.radiusSm) - 1)
                    }
                }
            }
        }
    }
    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight
        RowLayout {
            id: row
            anchors.verticalCenter: parent.verticalCenter
            width: root.navigation ? parent.width : Math.min(parent.width, implicitWidth)
            x: root.navigation ? 0 : (parent.width - width) / 2
            spacing: 9
            Rectangle {
                visible: root.glyph.length > 0
                Layout.preferredWidth: root.iconTile ? 34 : 18
                Layout.preferredHeight: root.iconTile ? 34 : 18
                Layout.alignment: Qt.AlignHCenter
                radius: Theme.radiusSm
                color: !root.iconTile ? "transparent"
                     : Theme.crystal ? (root.selected ? Qt.alpha(Theme.accent, 0.16) : Theme.rowFill)
                     : root.selected ? Theme.soft : Theme.rim
                border.color: !root.iconTile ? "transparent"
                            : Theme.crystal ? (root.selected ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.glassBorder)
                            : root.selected ? Theme.edge : Theme.rim
                opacity: root.enabled ? 1 : 0.5
                Glyph {
                    anchors.centerIn: parent
                    width: 17; height: 17
                    name: root.glyph
                    color: root.iconTile && !root.selected ? Theme.dim : root.foreground
                }
            }
            Text {
                visible: root.text.length > 0
                text: root.text
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.family: Theme.fontFamily
                font.pixelSize: 13
                font.weight: root.primary || root.selected ? Font.DemiBold : Font.Medium
                color: root.primary ? Theme.accentText : root.iconTile ? (root.selected ? Theme.ink : Theme.dim) : root.foreground
                Layout.fillWidth: true
                horizontalAlignment: root.navigation ? Text.AlignLeft : Text.AlignHCenter
                opacity: root.enabled ? 1 : 0.5
            }
        }
    }
}
