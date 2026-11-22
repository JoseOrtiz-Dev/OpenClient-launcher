import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

// Botón píldora: relleno "tinta" con icono en el acento, fantasma con borde fino o cristal translúcido.
// Al pasar el ratón sube 1 px y el icono gira un poco; todo con transform, sin tocar el tamaño.
Button {
    id: root
    property string glyph: ""
    // "ink": relleno oscuro (claro en tema blanco) con icono de acento. "ghost": solo borde.
    // "glass": relleno blanco .055 y borde blanco .11 (hover .11 / .22); en tema claro se tinta con negro.
    // "link": enlace a una web: tinte del acento, halo al pasar el ratón, icono que gira y flecha ↗ que aparece.
    property string tone: "ink"
    property string trailingGlyph: ""   // icono pequeño a la derecha, solo visible al pasar el ratón
    property bool iconSpin: false       // el icono da una vuelta al pasar el ratón
    readonly property bool link: tone === "link"
    readonly property bool ink: tone === "ink"
    readonly property bool glass: tone === "glass"
    readonly property color glassTint: Theme.light ? "#000000" : "#ffffff"

    implicitHeight: 40
    implicitWidth: Math.max(96, row.implicitWidth + leftPadding + rightPadding)
    leftPadding: 18
    rightPadding: 18
    hoverEnabled: true
    Accessible.name: text

    // Ligero levantamiento al pasar el ratón y vuelta al pulsar.
    transform: Translate {
        y: root.hovered && !root.down && root.enabled ? -1 : 0
        Behavior on y { MotionNumber { role: "fast" } }
    }

    background: Item {
        // Halo que se enciende al pasar el ratón (solo "link").
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: height / 2
            color: "transparent"
            border.width: 3
            border.color: Qt.alpha(Theme.accent, 0.20)
            opacity: root.link && root.hovered && root.enabled ? 1 : 0
            Behavior on opacity { MotionNumber { role: "base" } }
        }
        Rectangle {
            id: fill
            anchors.fill: parent
            radius: height / 2
            color: root.glass
                ? Qt.alpha(root.glassTint, root.down ? 0.15 : root.hovered ? 0.11 : 0.055)
                : root.link
                ? Qt.tint(Qt.alpha(Theme.base, 0.72), Qt.alpha(Theme.accent, root.down ? 0.34 : root.hovered ? 0.24 : 0.10))
                : root.ink
                ? (root.down ? Qt.darker(root.inkFill, 1.1) : root.hovered ? Qt.lighter(root.inkFill, 1.28) : root.inkFill)
                : (root.down ? Qt.alpha(Theme.ink, 0.12) : root.hovered ? Qt.alpha(Theme.ink, 0.08) : "transparent")
            border.width: 1
            border.color: root.glass ? Qt.alpha(root.glassTint, root.hovered ? 0.22 : 0.11)
                : root.link ? Qt.alpha(Theme.accentSoft, root.hovered ? 0.85 : 0.5)
                : root.ink ? Qt.alpha(Theme.edge, root.hovered ? 1 : 0.7) : (root.hovered ? Theme.edge : Qt.alpha(Theme.ink, 0.24))
            opacity: root.enabled ? 1 : 0.5
            Behavior on color { MotionColor { role: "fast" } }
            Behavior on border.color { MotionColor { role: "fast" } }
        }
        // Foco de teclado: anillo separado del botón.
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            radius: height / 2
            color: "transparent"
            border.width: 2
            border.color: Theme.highlight
            visible: root.visualFocus
        }
    }

    readonly property color inkFill: Theme.light ? Theme.ink : Qt.darker(Theme.base, 1.35)
    readonly property color labelColor: root.ink ? (Theme.light ? Theme.base : Theme.ink) : Theme.ink

    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight
        RowLayout {
            id: row
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(parent.width, implicitWidth)
            x: (parent.width - width) / 2
            spacing: 9
            Item {
                visible: root.glyph.length > 0
                Layout.preferredWidth: 17
                Layout.preferredHeight: 17
                opacity: root.enabled ? 1 : 0.5
                rotation: root.hovered && root.enabled ? (root.iconSpin ? 360 : -8) : 0
                scale: root.hovered && root.enabled ? 1.08 : 1
                Behavior on rotation { MotionNumber { role: root.iconSpin ? "slow" : "fast" } }
                Behavior on scale { MotionNumber { role: "fast" } }
                Glyph {
                    anchors.centerIn: parent
                    width: 17; height: 17
                    name: root.glyph
                    color: root.link ? Theme.accentSoft : root.ink ? Theme.highlight : root.labelColor
                }
            }
            Text {
                visible: root.text.length > 0
                text: root.text
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.family: Theme.fontFamily
                font.pixelSize: 13
                font.weight: Font.Medium
                color: root.labelColor
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                opacity: root.enabled ? 1 : 0.5
            }
        }
    }

    // Flecha de «abre una web»: aparece desde la izquierda al pasar el ratón, dentro del relleno lateral.
    Glyph {
        visible: root.trailingGlyph.length > 0
        anchors.verticalCenter: parent.verticalCenter
        anchors.right: parent.right
        anchors.rightMargin: 6
        width: 11; height: 11
        name: root.trailingGlyph
        color: Theme.accentSoft
        opacity: root.hovered && root.enabled ? 1 : 0
        transform: Translate {
            x: root.hovered && root.enabled ? 0 : -4
            Behavior on x { MotionNumber { role: "fast" } }
        }
        Behavior on opacity { MotionNumber { role: "fast" } }
    }
}
