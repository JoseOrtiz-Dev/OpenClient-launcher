pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenClient

// Interruptor compartido: pista de 40×22 con perilla de 16 px. Con un tema Cristal la pista encendida es un
// degradado btnFrom→btnTo con un halo suave; con los antiguos, `accent` plano. La perilla se mueve con Translate.
// El texto (`text`) o un `contentItem` propio se dibujan a la derecha de la pista; sin texto la pista se centra.
// Uso: GlassSwitch { checked: launcher.fullscreen; onToggled: launcher.fullscreen = checked; Accessible.name: qsTr("Pantalla completa") }
Switch {
    id: control

    readonly property real trackWidth: 40
    readonly property real trackHeight: 22
    readonly property real knobSize: 16
    readonly property real knobInset: 3
    readonly property bool hasContent: implicitContentWidth > 0

    hoverEnabled: true
    spacing: 10
    padding: 0
    implicitWidth: Math.max(trackWidth, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(trackHeight, implicitContentHeight) + topPadding + bottomPadding
    leftPadding: mirrored ? 0 : trackWidth + (hasContent ? spacing : 0)
    rightPadding: mirrored ? trackWidth + (hasContent ? spacing : 0) : 0
    font.family: Theme.fontFamily
    font.pixelSize: 13

    contentItem: Text {
        text: control.text
        textFormat: Text.PlainText
        font: control.font
        color: Theme.ink
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
        opacity: control.enabled ? 1 : 0.55
        Behavior on opacity { MotionNumber { role: "base" } }
    }

    indicator: Item {
        id: track
        implicitWidth: control.trackWidth
        implicitHeight: control.trackHeight
        width: control.trackWidth
        height: control.trackHeight
        x: control.hasContent ? (control.mirrored ? control.width - width : 0) : (control.width - width) / 2
        y: (control.height - height) / 2
        opacity: control.enabled ? 1 : 0.5
        Behavior on opacity { MotionNumber { role: "base" } }

        // Halo bajo la pista encendida (solo Cristal): capas con alfa, sin sombras con shader.
        Repeater {
            model: 2
            Rectangle {
                required property int index
                anchors.fill: parent
                anchors.leftMargin: -(index + 1) * 2
                anchors.rightMargin: -(index + 1) * 2
                anchors.topMargin: -(index + 1) * 2 + 2
                anchors.bottomMargin: -(index + 1) * 2 - 2
                radius: height / 2
                color: Qt.alpha(Theme.btnShadow, index === 0 ? 0.30 : 0.14)
                opacity: Theme.crystal && control.checked ? 1 : 0
                Behavior on opacity { MotionNumber { role: "base" } }
            }
        }

        // Anillo de foco de teclado.
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            radius: height / 2
            color: "transparent"
            border.width: 2
            border.color: Theme.crystal ? Theme.accentSoft : Theme.highlight
            opacity: control.visualFocus ? 1 : 0
            Behavior on opacity { MotionNumber { role: "fast" } }
        }

        // Pista apagada.
        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: Theme.crystal ? Theme.rowFillHover : Theme.surface
            border.width: 1
            border.color: control.hovered ? (Theme.crystal ? Theme.glassBorderHover : Theme.edge)
                : (Theme.crystal ? Theme.fieldBorder : Theme.border)
            Behavior on border.color { MotionColor { role: "fast" } }
        }

        // Pista encendida: se funde por encima con la opacidad (degradado en Cristal, acento plano en el resto).
        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: Theme.crystal ? "transparent" : (control.down ? Theme.accentPressed : Theme.accent)
            gradient: Theme.crystal ? onGradient : null
            opacity: control.checked ? 1 : 0
            Behavior on opacity { MotionNumber { role: "fast" } }
            Behavior on color { MotionColor { role: "fast" } }
            Gradient {
                id: onGradient
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Theme.btnFrom }
                GradientStop { position: 1; color: Theme.btnTo }
            }
        }

        // Perilla: solo se mueve con un Translate (durante el arrastre sigue al dedo sin animar).
        Rectangle {
            x: control.knobInset
            y: (parent.height - height) / 2
            width: control.knobSize; height: control.knobSize
            radius: width / 2
            color: control.checked ? (Theme.crystal ? Theme.ink : Theme.accentText)
                : (Theme.crystal ? Qt.alpha(Theme.ink, 0.72) : Theme.dim)
            scale: control.down ? 1.12 : 1
            Behavior on scale { MotionNumber { role: "press" } }
            Behavior on color { MotionColor { role: "fast" } }
            transform: Translate {
                x: control.visualPosition * (control.trackWidth - control.knobSize - 2 * control.knobInset)
                Behavior on x { enabled: !control.down; MotionNumber { role: "fast" } }
            }
        }
    }
}
