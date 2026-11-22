import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

Button {
    id: control
    property string glyph: "grid"
    property string subtitle: ""
    property bool selected: false
    property bool compact: false
    // Barra estrecha: solo el icono; la etiqueta la muestra la etiqueta flotante de Sidebar y el contador va en una insignia sobre el icono.
    property bool expanded: true
    readonly property int tileSize: compact ? 30 : 36
    property int count: -1
    // Con true, el fondo de selección lo dibuja la pastilla compartida de Sidebar.
    property bool sharedIndicator: false
    // Entrada escalonada: Sidebar pone `revealed` a true una vez y cada botón espera su turno.
    property bool revealed: true
    property int revealIndex: 0
    implicitHeight: compact ? 42 : 58
    hoverEnabled: true
    leftPadding: expanded ? 10 : Math.max(0, (Theme.railCollapsed - 20 - tileSize) / 2)
    rightPadding: leftPadding
    // Sidebar escucha esto para mostrar su etiqueta flotante.
    readonly property bool wantsTip: !expanded && (hovered || visualFocus)
    font.family: Theme.fontFamily
    Accessible.name: text
    Accessible.description: subtitle

    opacity: revealed ? 1 : 0
    transform: Translate {
        x: control.revealed ? 0 : -12
        Behavior on x {
            SequentialAnimation {
                PauseAnimation { duration: Theme.motion ? control.revealIndex * 45 : 0 }
                MotionNumber { role: "enter" }
            }
        }
    }
    Behavior on opacity {
        SequentialAnimation {
            PauseAnimation { duration: Theme.motion ? control.revealIndex * 45 : 0 }
            MotionNumber { role: "slow" }
        }
    }

    background: Rectangle {
        radius: Theme.radiusMd
        color: control.down ? Qt.alpha(Theme.ink, Theme.light ? 0.08 : 0.09)
             : control.hovered && !control.selected ? Qt.alpha(Theme.ink, Theme.light ? 0.04 : 0.05)
             : "transparent"
        Behavior on color { MotionColor { role: "fast" } }
        border.width: control.visualFocus ? 2 : 0
        border.color: Theme.highlight

        // Seleccion propia (solo si no hay pastilla compartida): mismo degradado de acento que la pastilla de Sidebar.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: !control.sharedIndicator && opacity > 0.01
            opacity: control.selected ? 1 : 0
            Behavior on opacity { MotionNumber { role: "base" } }
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Qt.alpha(Theme.accent, 0.28) }
                GradientStop { position: 1; color: Qt.alpha(Theme.accent, 0.12) }
            }
            border.width: 1
            border.color: Qt.alpha(Theme.accentSoft, 0.35)
        }
        Rectangle {
            visible: !control.sharedIndicator && opacity > 0.01
            width: 4
            height: 22
            radius: 2
            anchors.left: parent.left
            anchors.leftMargin: -1
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.accentSoft
            opacity: control.selected ? 1 : 0
            Behavior on opacity { MotionNumber { role: "base" } }
        }
    }

    contentItem: RowLayout {
        spacing: 10
        scale: control.down ? 0.975 : 1
        Behavior on scale { MotionNumber { role: "press" } }

        Rectangle {
            id: tile
            Layout.preferredWidth: control.tileSize
            Layout.preferredHeight: width
            radius: Theme.radiusSm
            color: control.hovered && !control.selected ? Theme.rim : "transparent"
            Behavior on color { MotionColor { role: "base" } }
            Rectangle {
                visible: !control.expanded && control.count > 0
                z: 2
                anchors.right: parent.right; anchors.top: parent.top
                anchors.rightMargin: -3; anchors.topMargin: -3
                width: Math.max(15, miniCount.implicitWidth + 8); height: 15; radius: 8
                color: Theme.accent
                Text {
                    id: miniCount
                    anchors.centerIn: parent
                    text: control.count > 9 ? "9+" : control.count
                    font.family: control.font.family; font.pixelSize: 9; font.weight: Font.DemiBold
                    color: Theme.accentText
                }
            }
            Glyph {
                anchors.centerIn: parent
                width: 18; height: 18
                name: control.glyph
                color: control.selected || control.hovered ? Theme.ink : Theme.dim
                Behavior on color { MotionColor { role: "base" } }
                transform: Translate {
                    x: control.hovered && !control.selected && Theme.motion ? 2 : 0 // motion-ok: valor de destino, no duración
                    Behavior on x { MotionNumber { role: "fast" } }
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            opacity: control.expanded ? 1 : 0
            visible: opacity > 0.01
            Behavior on opacity { MotionNumber { role: "base" } }
            Text {
                Layout.fillWidth: true
                text: control.text
                textFormat: Text.PlainText
                font.family: control.font.family
                font.pixelSize: 13
                font.weight: control.selected ? Font.DemiBold : Font.Medium
                color: control.selected ? Theme.ink : Qt.alpha(Theme.ink, 0.86)
                Behavior on color { MotionColor { role: "base" } }
                elide: Text.ElideRight
            }
            Text {
                Layout.fillWidth: true
                visible: !control.compact && control.subtitle.length > 0
                text: control.subtitle
                textFormat: Text.PlainText
                font.family: control.font.family
                font.pixelSize: 11
                color: Theme.dim
                elide: Text.ElideRight
            }
        }
        Rectangle {
            id: badge
            visible: control.expanded && control.count >= 0
            Layout.preferredWidth: Math.max(22, countText.implicitWidth + 12)
            Layout.preferredHeight: 22
            radius: Theme.radiusSm
            color: control.selected ? Theme.soft : Theme.rim
            Behavior on color { MotionColor { role: "base" } }
            Text {
                id: countText
                anchors.centerIn: parent
                text: control.count > 99 ? "99+" : control.count
                font.family: control.font.family
                font.pixelSize: 11
                font.features: Theme.tabular
                color: control.selected ? Theme.highlight : Theme.dim
            }
            SequentialAnimation {
                id: pop
                MotionNumber { role: "fast"; target: badge; property: "scale"; to: 1.22 }
                NumberAnimation { target: badge; property: "scale"; to: 1; duration: Theme.durationSlow; easing.type: Easing.OutBack; easing.overshoot: 2.2 }
            }
        }
    }
    onCountChanged: if (Theme.motion && badge.visible && revealed) pop.restart()
}
