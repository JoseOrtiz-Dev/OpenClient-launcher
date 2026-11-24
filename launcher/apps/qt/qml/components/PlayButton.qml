import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import OpenClient

// Botón principal del inicio. Cinco estados: sin instancia, lista, preparando, jugando y error de sesión.
// El color es el acento del tema; el texto usa accentText (blanco u oscuro según contraste).
Item {
    id: root
    objectName: "playButton"
    property bool hasInstance: false
    property string instanceId: ""
    property string instanceLabel: ""            // «Prueba visual · Fabric 1.21.4»
    property string emptyLabel: qsTr("Crear instancia") // texto sin instancia («Instalar DiosesMC»)
    property string emptyGlyph: "plus"
    property bool busy: false                    // otra operación en curso: desactiva INICIAR
    // Solo para pruebas visuales: fuerzan un estado y un porcentaje.
    property string forceMode: ""
    property int forcePercent: -1
    signal play()
    signal cancel()
    signal reconnect()
    signal empty()

    readonly property string mode: forceMode.length ? forceMode
        : launcher.runningInstanceIds.indexOf(instanceId) !== -1 ? "running"
        : instanceId.length && launcher.preparingInstanceId === instanceId ? "preparing"
        : !hasInstance ? "none"
        : !launcher.gameRunning && !launcher.instanceAccounts[instanceId] && launcher.usingMicrosoft && launcher.microsoftNeedsLogin ? "session"
        : "ready"
    readonly property int percent: forcePercent >= 0 ? forcePercent : Math.max(0, Math.min(100, launcher.gamePercent))
    readonly property bool clickable: mode === "ready" || mode === "none" || mode === "session"
    readonly property string caption: mode === "none" ? qsTr("Aún no tienes instancias")
        : mode === "ready" ? instanceLabel
        : mode === "preparing" ? (launcher.gameMessage || qsTr("Preparando el juego…"))
        : mode === "running" ? qsTr("Minecraft está abierto")
        : (launcher.accountError || qsTr("Tu sesión de Microsoft caducó"))
    readonly property string label: mode === "none" ? emptyLabel
        : mode === "ready" ? (launcher.gameRunning ? qsTr("Abrir con cuenta…") : qsTr("INICIAR"))
        : mode === "preparing" ? qsTr("Preparando %1 %").arg(percent)
        : mode === "running" ? qsTr("En juego")
        : qsTr("Volver a entrar")
    readonly property string glyph: mode === "none" ? emptyGlyph
        : mode === "ready" ? "play"
        : mode === "session" ? "alert" : ""

    // «Listo» y «sin instancia» usan el relleno de acento con degradado, brillo y destello; el resto, el plano de siempre.
    readonly property bool accentFill: mode === "ready" || mode === "none"

    // En tema claro el ámbar del aviso no llega a AA con texto blanco ni oscuro: se oscurece el fondo.
    readonly property color sessionFill: Theme.light ? Qt.darker(Theme.warning, 1.15) : Theme.warning
    readonly property color sessionTone: Theme.light ? "#ffffff" : Theme.warningText

    implicitWidth: 300
    implicitHeight: Theme.playHeight

    // Etiqueta con glifo opcional; se dibuja dos veces en «preparando» (sobre la pista y sobre el relleno).
    component Face: Row {
        property color tone: Theme.ink
        property bool dot: false
        anchors.verticalCenter: parent.verticalCenter
        spacing: 9
        Rectangle {
            visible: parent.dot
            width: 8; height: 8; radius: 4
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.success
        }
        Glyph {
            visible: root.glyph.length > 0
            width: 18; height: 18
            anchors.verticalCenter: parent.verticalCenter
            name: root.glyph; color: parent.tone
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.label
            textFormat: Text.PlainText
            color: parent.tone
            font.family: Theme.fontFamily
            font.pixelSize: root.mode === "ready" ? 17 : 15
            font.weight: Font.DemiBold
            font.letterSpacing: root.mode === "ready" ? 1.4 : 0
            font.features: Theme.tabular
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 8

        Button {
            id: main
            Layout.fillWidth: true
            Layout.fillHeight: true
            hoverEnabled: root.clickable
            Accessible.name: root.label
            Accessible.description: root.caption
            enabled: root.clickable ? !root.busy || root.mode === "session" : true
            scale: down && root.clickable && Theme.motion ? 0.99 : 1
            Behavior on scale { MotionNumber { role: "press" } }
            // Elevación: -1 px al pasar el ratón, +1 px al pulsar.
            transform: Translate {
                y: root.clickable && main.enabled ? (main.down ? 1 : main.hovered ? -1 : 0) : 0
                Behavior on y { MotionNumber { role: main.down ? "press" : "fast" } }
            }
            onHoveredChanged: if (hovered && root.accentFill && Theme.motion && enabled) sweep.restart()
            onClicked: {
                if (root.mode === "ready") root.play()
                else if (root.mode === "none") root.empty()
                else if (root.mode === "session") root.reconnect()
            }

            background: Item {
                // Capa de acento (INICIAR / Crear instancia): resplandor, degradado diagonal, luz superior y destello.
                Item {
                    id: accentLayer
                    anchors.fill: parent
                    visible: root.accentFill
                    opacity: main.enabled ? 1 : 0.55
                    readonly property int cut: Math.ceil(Theme.radiusMd * 0.293 + 1)   // el destello no llega a las esquinas

                    // Resplandor btnShadow: tres aros con alfa, desplazados hacia abajo (sin sombras con shader).
                    Repeater {
                        model: [ { grow: 9, alpha: 0.10 }, { grow: 5, alpha: 0.14 }, { grow: 2, alpha: 0.22 } ]
                        Rectangle {
                            required property var modelData
                            x: -modelData.grow; y: -modelData.grow + 6
                            width: parent.width + modelData.grow * 2
                            height: parent.height + modelData.grow * 2
                            radius: Theme.radiusMd + modelData.grow
                            color: Theme.btnShadow
                            opacity: modelData.alpha * (main.hovered && main.enabled ? 1.5 : 1)
                            Behavior on opacity { MotionNumber { role: "base" } }
                        }
                    }

                    Shape {
                        id: fill
                        anchors.fill: parent
                        preferredRendererType: Shape.CurveRenderer
                        ShapePath {
                            strokeWidth: -1
                            fillGradient: LinearGradient {      // 135°, de esquina a esquina
                                x1: 0; y1: 0; x2: fill.width; y2: fill.height
                                GradientStop { position: 0; color: Theme.btnFrom }
                                GradientStop { position: 1; color: Theme.btnTo }
                            }
                            PathRectangle { width: fill.width; height: fill.height; radius: Theme.radiusMd }
                        }
                    }
                    // Hover aclara (brightness 1.06); pulsado oscurece.
                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radiusMd
                        color: main.down ? Qt.rgba(0, 0, 0, 0.14) : main.hovered ? Qt.rgba(1, 1, 1, 0.07) : "transparent"
                        Behavior on color { MotionColor { role: "fast" } }
                    }
                    // Luz interior arriba y sombra fina abajo.
                    Rectangle {
                        x: Theme.radiusMd; y: 1; height: 1
                        width: Math.max(0, parent.width - Theme.radiusMd * 2)
                        color: Qt.rgba(1, 1, 1, main.hovered ? 0.45 : 0.35)
                        Behavior on color { MotionColor { role: "fast" } }
                    }
                    Rectangle {
                        x: Theme.radiusMd; y: parent.height - 3; height: 2
                        width: Math.max(0, parent.width - Theme.radiusMd * 2)
                        color: Qt.rgba(0, 0, 0, 0.18)
                    }
                    // Destello: banda blanca inclinada -20° que cruza de izquierda a derecha al pasar el ratón.
                    Item {
                        anchors.fill: parent
                        anchors.margins: accentLayer.cut
                        clip: true
                        Rectangle {
                            id: band
                            visible: sweep.running
                            width: accentLayer.width * 0.65
                            height: accentLayer.height
                            y: -accentLayer.cut
                            x: -accentLayer.cut
                            gradient: Gradient {
                                orientation: Gradient.Horizontal
                                GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0) }
                                GradientStop { position: 0.5; color: Qt.rgba(1, 1, 1, 0.35) }
                                GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0) }
                            }
                            transform: [
                                Matrix4x4 {   // skewX(-20°) alrededor del centro vertical
                                    matrix: Qt.matrix4x4(1, -Math.tan(20 * Math.PI / 180), 0, Math.tan(20 * Math.PI / 180) * band.height / 2,
                                                         0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1)
                                },
                                Translate { id: sweepShift; x: -1.2 * accentLayer.width }
                            ]
                        }
                    }
                    SequentialAnimation {
                        id: sweep
                        PropertyAction { target: sweepShift; property: "x"; value: -1.2 * accentLayer.width }
                        MotionNumber {
                            target: sweepShift; property: "x"
                            to: 1.55 * accentLayer.width
                            role: "slow"
                            duration: Theme.durationSlow * 2.2     // ~650 ms con movimiento
                        }
                    }
                }
                Rectangle {
                    id: face
                    anchors.fill: parent
                    visible: !root.accentFill
                    radius: Theme.radiusMd
                    color: root.mode === "preparing" || root.mode === "running" ? Theme.soft
                        : root.mode === "session" ? (main.down ? Qt.darker(root.sessionFill, 1.12) : main.hovered ? Qt.lighter(root.sessionFill, 1.07) : root.sessionFill)
                        : main.down ? Theme.accentPressed : main.hovered ? Qt.lighter(Theme.accent, 1.07) : Theme.accent
                    border.width: root.mode === "preparing" || root.mode === "running" ? 1 : 0
                    border.color: Theme.edge
                    opacity: main.enabled ? 1 : 0.55
                    Behavior on color { MotionColor { role: "fast" } }
                }
                // Texto sobre la pista (debajo del relleno); el contentItem no lo dibuja en «preparando».
                Face { visible: root.mode === "preparing"; anchors.centerIn: parent; tone: Theme.ink }
                // Relleno de progreso: el mismo texto en accentText queda recortado dentro, así siempre contrasta.
                Item {
                    visible: root.mode === "preparing"
                    width: parent.width * root.percent / 100
                    height: parent.height
                    clip: true
                    Behavior on width { MotionNumber { role: "base" } }
                    Rectangle { width: face.width; height: face.height; radius: Theme.radiusMd; color: Theme.accent }
                    Item {
                        width: face.width; height: face.height
                        Face { anchors.centerIn: parent; tone: Theme.accentText }
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: -3
                    radius: Theme.radiusMd + 3
                    color: "transparent"
                    border.width: 2
                    border.color: Theme.highlight
                    visible: main.visualFocus
                }
            }
            contentItem: Item {
                Face {
                    anchors.centerIn: parent
                    visible: root.mode !== "preparing"
                    dot: root.mode === "running"
                    tone: root.mode === "ready" || root.mode === "none" ? Theme.accentText
                        : root.mode === "session" ? root.sessionTone : Theme.ink
                }
            }
        }

        Button {
            id: stopButton
            objectName: "playCancel"
            visible: root.mode === "preparing"
            Layout.preferredWidth: Theme.playHeight
            Layout.fillHeight: true
            hoverEnabled: true
            Accessible.name: "Cancelar"
            ToolTip.visible: hovered || visualFocus
            ToolTip.delay: 400
            ToolTip.text: qsTr("Cancelar")
            onClicked: root.cancel()
            background: Rectangle {
                radius: Theme.radiusMd
                color: stopButton.down ? Theme.edge : stopButton.hovered ? Theme.soft : Qt.alpha(Theme.surface, 0.72)
                border.width: stopButton.visualFocus ? 2 : 1
                border.color: stopButton.visualFocus ? Theme.highlight : Theme.border
                Behavior on color { MotionColor { role: "fast" } }
            }
            contentItem: Item {
                Glyph { anchors.centerIn: parent; width: 18; height: 18; name: "stop"; color: Theme.ink }
            }
        }
    }
}
