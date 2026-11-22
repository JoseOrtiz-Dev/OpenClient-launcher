pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenClient

// Campo de texto compartido. Con un tema Cristal: relleno negro translúcido, borde fino que pasa a `accentSoft`
// con el foco y un anillo suave del acento. Con los temas antiguos conserva el aspecto de siempre: relleno
// `base`/`surface`, borde `border` y borde `highlight` de 2 px con el foco.
// Uso: GlassField { placeholderText: qsTr("Buscar…"); leadingGlyph: "search" }
TextField {
    id: control

    // Nombre de un `Glyph` que se dibuja pequeño a la izquierda (vacío: sin icono).
    property string leadingGlyph: ""
    // Elemento opcional a la derecha (p. ej. un botón de borrar). Se coloca centrado en vertical, a 6 px del borde.
    property Item trailing: null
    // Esquinas del campo.
    property real radius: Theme.crystal ? Theme.radiusMd : Theme.radiusSm
    // Relleno con los temas antiguos (`Theme.base` por defecto; `Theme.surface` para campos sobre una tarjeta).
    property color legacyFill: Theme.base

    function attachTrailing() {
        if (!trailing)
            return
        trailing.parent = trailingSlot
        trailing.x = 0
        trailing.y = 0
    }
    onTrailingChanged: attachTrailing()
    Component.onCompleted: attachTrailing()

    implicitHeight: 40
    leftPadding: leadingGlyph.length > 0 ? 38 : 12
    rightPadding: trailing && trailing.visible ? trailing.width + 14 : 12
    hoverEnabled: true
    selectByMouse: true
    verticalAlignment: TextInput.AlignVCenter
    color: Theme.ink
    placeholderTextColor: Theme.dim
    selectionColor: Theme.accent
    selectedTextColor: Theme.accentText
    font.family: Theme.fontFamily
    font.pixelSize: 13
    opacity: enabled ? 1 : 0.55
    Behavior on opacity { MotionNumber { role: "base" } }

    background: Rectangle {
        radius: control.radius
        color: Theme.crystal ? Theme.fieldFill : control.legacyFill
        border.width: !Theme.crystal && control.activeFocus ? 2 : 1
        border.color: control.activeFocus ? (Theme.crystal ? Theme.fieldBorderFocus : Theme.highlight)
            : control.hovered ? (Theme.crystal ? Theme.glassBorderHover : Theme.edge)
            : (Theme.crystal ? Theme.fieldBorder : Theme.border)
        Behavior on border.color { MotionColor { role: "fast" } }

        // Anillo suave de foco (solo Cristal): aparece y se va con la opacidad.
        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            radius: parent.radius + 2
            color: "transparent"
            border.width: 2
            border.color: Qt.alpha(Theme.accent, 0.35)
            opacity: Theme.crystal && control.activeFocus ? 1 : 0
            Behavior on opacity { MotionNumber { role: "fast" } }
        }
    }

    Glyph {
        visible: control.leadingGlyph.length > 0
        x: 12
        anchors.verticalCenter: parent.verticalCenter
        width: 16; height: 16
        name: control.leadingGlyph
        color: control.activeFocus ? (Theme.crystal ? Theme.accentSoft : Theme.highlight) : Theme.dim
        Behavior on color { MotionColor { role: "fast" } }
        Accessible.ignored: true
    }

    Item {
        id: trailingSlot
        anchors.right: parent.right
        anchors.rightMargin: 6
        anchors.verticalCenter: parent.verticalCenter
        width: control.trailing ? control.trailing.width : 0
        height: control.trailing ? control.trailing.height : 0
    }
}
