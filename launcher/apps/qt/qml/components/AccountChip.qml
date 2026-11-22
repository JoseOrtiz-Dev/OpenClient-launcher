import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

// Esquina superior derecha del inicio: solo la cabeza de la cuenta. Al acercar el ratón, enfocar con el teclado
// o pulsar, se despliega hacia la izquierda con el nombre, el estado y «Cambiar». La pulsación la deja abierta
// hasta que el puntero se aleja un rato o se vuelve a pulsar. Solo se animan opacidad y transformaciones.
Item {
    id: chip
    objectName: "accountChip"
    property bool pinned: false
    property bool peek: false
    signal accountRequested()

    readonly property bool shown: pinned || peek || head.visualFocus
    readonly property bool needsLogin: launcher.usingMicrosoft && launcher.microsoftNeedsLogin
    readonly property color tone: needsLogin ? Theme.warning : launcher.accountName ? Theme.success : Theme.dim
    readonly property string statusText: launcher.usingMicrosoft ? (needsLogin ? qsTr("Vuelve a entrar") : qsTr("Microsoft · Java"))
        : launcher.accountName ? qsTr("Perfil offline") : qsTr("Elige un perfil")
    readonly property real panelWidth: 236

    width: 44
    height: 44
    implicitWidth: 44
    implicitHeight: 44

    // Zona sensible: la cabeza y, abierto, también el panel; así el panel no se cierra al cruzar el hueco entre ambos.
    Item {
        id: zone
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: chip.shown ? chip.panelWidth + 8 + 44 : 44
        height: 60
        HoverHandler { id: hover }
    }
    Timer { interval: 120; running: hover.hovered && !chip.shown; onTriggered: chip.peek = true }
    Timer { interval: 260; running: !hover.hovered && chip.peek; onTriggered: chip.peek = false }
    Timer { interval: 2500; running: chip.pinned && !hover.hovered && !head.activeFocus; onTriggered: chip.pinned = false }

    Rectangle {
        id: panel
        objectName: "accountChipPanel"
        width: chip.panelWidth
        height: 54
        radius: Theme.radiusMd
        color: Theme.crystal ? Qt.tint(Theme.base, Theme.glassStrong) : Theme.surface
        border.color: Theme.crystal ? Theme.glassBorder : Theme.border
        anchors.right: head.left
        anchors.rightMargin: 8
        anchors.verticalCenter: head.verticalCenter
        opacity: chip.shown ? 1 : 0
        visible: opacity > 0.01
        Behavior on opacity { MotionNumber { role: "base" } }
        transform: [
            Translate {
                x: chip.shown ? 0 : 14
                Behavior on x { MotionNumber { role: "slow" } }
            },
            Scale {
                origin.x: panel.width; origin.y: panel.height / 2
                xScale: chip.shown ? 1 : 0.96
                yScale: xScale
                Behavior on xScale { MotionNumber { role: "slow" } }
            }
        ]
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 14
            anchors.rightMargin: 10
            spacing: 8
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Text {
                    Layout.fillWidth: true
                    text: launcher.accountName || qsTr("Sin perfil")
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Theme.ink
                    font.family: Theme.fontFamily; font.pixelSize: 14; font.weight: Font.DemiBold
                }
                Text {
                    Layout.fillWidth: true
                    text: chip.statusText
                    elide: Text.ElideRight
                    color: chip.needsLogin ? Theme.warning : Theme.dim
                    font.family: Theme.fontFamily; font.pixelSize: 11
                }
            }
            ActionButton {
                text: qsTr("Cambiar"); compact: true
                onClicked: { chip.pinned = false; chip.peek = false; chip.accountRequested() }
            }
        }
    }

    Button {
        id: head
        objectName: "accountChipHead"
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: 44
        height: 44
        hoverEnabled: true
        Accessible.name: qsTr("%1, %2").arg(launcher.accountName ? qsTr("Cuenta de %1").arg(launcher.accountName) : qsTr("Elegir perfil")).arg(chip.statusText)
        Accessible.description: qsTr("Muestra los datos de la cuenta")
        scale: down && Theme.motion ? 0.96 : 1
        Behavior on scale { MotionNumber { role: "press" } }
        onClicked: chip.pinned = !chip.pinned
        Keys.onEscapePressed: { chip.pinned = false; chip.peek = false }
        background: Rectangle {
            radius: Theme.radiusMd
            color: chip.shown || head.hovered ? (Theme.crystal ? Qt.tint(Theme.base, Qt.alpha(Theme.accent, 0.18)) : Theme.soft)
                : (Theme.crystal ? Qt.tint(Theme.base, Theme.glassStrong) : Theme.surface)
            border.width: head.visualFocus ? 2 : 1.5
            border.color: head.visualFocus ? Theme.highlight : Qt.alpha(chip.tone, 0.75)
            Behavior on color { MotionColor { role: "fast" } }
        }
        contentItem: Item {
            clip: true
            readonly property bool hasFace: launcher.accountName.length > 0 && wardrobe.accountFace.length > 0
            Image {
                anchors.fill: parent
                anchors.margins: 5
                visible: parent.hasFace
                source: parent.hasFace ? wardrobe.accountFace : ""
                smooth: false
                fillMode: Image.PreserveAspectFit
            }
            Glyph { anchors.centerIn: parent; visible: !parent.hasFace; name: "user"; width: 20; height: 20; color: Theme.highlight }
        }
        // Punto de estado en la esquina: verde con sesión, ámbar si hay que volver a entrar.
        Rectangle {
            width: 11; height: 11; radius: 6
            anchors.right: parent.right; anchors.bottom: parent.bottom
            anchors.rightMargin: -3; anchors.bottomMargin: -3
            color: chip.tone
            border.width: 2
            border.color: Theme.base
        }
    }
}
