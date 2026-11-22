pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenClient

// Lista desplegable compartida (no editable). Campo como `GlassField`, chevron que gira al abrirse y popup de
// cristal con filas de 36 px. Respeta model, textRole, valueRole, currentIndex, displayText y onActivated.
// Uso: GlassComboBox { model: root.orders; textRole: "label"; onActivated: index => root.pick(index) }
ComboBox {
    id: control

    // Esquinas del campo.
    property real radius: Theme.crystal ? Theme.radiusMd : Theme.radiusSm
    // Relleno con los temas antiguos (`Theme.base` por defecto).
    property color legacyFill: Theme.base
    // Alto máximo de la lista antes de mostrar la barra de desplazamiento.
    property real popupMaxHeight: 320

    readonly property color accentTone: Theme.crystal ? Theme.accentSoft : Theme.highlight

    implicitHeight: 40
    leftPadding: 12
    rightPadding: 36
    hoverEnabled: true
    font.family: Theme.fontFamily
    font.pixelSize: 13
    opacity: enabled ? 1 : 0.55
    Behavior on opacity { MotionNumber { role: "base" } }

    contentItem: Text {
        text: control.displayText
        textFormat: Text.PlainText
        font: control.font
        color: Theme.ink
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }

    indicator: Glyph {
        name: "chevron"
        width: 16; height: 16
        x: control.width - width - 12
        y: (control.height - height) / 2
        color: control.activeFocus || control.popup.visible ? control.accentTone : Theme.dim
        rotation: control.popup.visible ? 180 : 0
        Behavior on rotation { MotionNumber { role: "base" } }
        Behavior on color { MotionColor { role: "fast" } }
    }

    background: Rectangle {
        radius: control.radius
        color: Theme.crystal ? Theme.fieldFill
            : control.down && !control.popup.visible ? Theme.soft : control.legacyFill
        border.width: !Theme.crystal && (control.visualFocus || control.popup.visible) ? 2 : 1
        border.color: control.visualFocus || control.popup.visible || control.activeFocus
            ? (Theme.crystal ? Theme.fieldBorderFocus : Theme.highlight)
            : control.hovered ? (Theme.crystal ? Theme.glassBorderHover : Theme.edge)
            : (Theme.crystal ? Theme.fieldBorder : Theme.border)
        Behavior on border.color { MotionColor { role: "fast" } }

        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            radius: parent.radius + 2
            color: "transparent"
            border.width: 2
            border.color: Qt.alpha(Theme.accent, 0.35)
            opacity: Theme.crystal && (control.visualFocus || control.popup.visible) ? 1 : 0
            Behavior on opacity { MotionNumber { role: "fast" } }
        }
    }

    delegate: ItemDelegate {
        id: option
        required property int index
        readonly property bool picked: control.currentIndex === index
        readonly property string label: {
            control.model; control.textRole
            return control.textAt(index)
        }
        width: ListView.view ? ListView.view.width : control.width
        height: 36
        padding: 0
        leftPadding: 12
        rightPadding: 12
        highlighted: control.highlightedIndex === index
        hoverEnabled: control.hoverEnabled
        Accessible.name: label
        contentItem: Text {
            text: option.label
            textFormat: Text.PlainText
            font.family: Theme.fontFamily
            font.pixelSize: 13
            font.weight: option.picked ? Font.DemiBold : Font.Normal
            color: option.picked ? control.accentTone : Theme.ink
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: Theme.radiusSm - 2
            color: option.highlighted || option.hovered
                ? (Theme.crystal ? Theme.rowFillHover : Theme.soft)
                : option.picked && !Theme.crystal ? Theme.soft : "transparent"
            Behavior on color { MotionColor { role: "fast" } }
        }
    }

    popup: Popup {
        y: control.height + 5
        width: control.width
        padding: 4
        implicitHeight: Math.min(contentItem.implicitHeight + topPadding + bottomPadding, control.popupMaxHeight)
        transformOrigin: Popup.Top

        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
            highlightMoveDuration: 0
            ScrollBar.vertical: ScrollBar {
                id: bar
                implicitWidth: 6
                background: Item {}
                contentItem: Rectangle {
                    implicitWidth: 4
                    radius: 2
                    color: Theme.dim
                    opacity: bar.active ? 0.55 : 0
                    Behavior on opacity { MotionNumber { role: "base" } }
                }
            }
        }

        background: Rectangle {
            radius: Theme.radiusMd
            antialiasing: true
            color: Theme.crystal ? Theme.glassStrong : Theme.glassElevated
            border.width: 1
            border.color: Theme.crystal ? Theme.glassBorder : Theme.rim
            Rectangle {
                visible: Theme.crystal
                anchors.top: parent.top
                anchors.topMargin: 1
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.max(0, parent.width - Theme.radiusMd * 2)
                height: 1
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: "transparent" }
                    GradientStop { position: 0.5; color: Theme.specular }
                    GradientStop { position: 1; color: "transparent" }
                }
            }
        }

        enter: Transition {
            MotionNumber { role: "enter"; property: "opacity"; from: 0; to: 1 }
            MotionNumber { role: "enter"; property: "scale"; from: 0.98; to: 1 }
        }
        exit: Transition {
            MotionNumber { role: "exit"; property: "opacity"; from: 1; to: 0 }
            MotionNumber { role: "exit"; property: "scale"; from: 1; to: 0.98 }
        }
    }
}
