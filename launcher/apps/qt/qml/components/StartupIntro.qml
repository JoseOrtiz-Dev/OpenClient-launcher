import QtQuick
import OpenClient

// Arranque: cielo de Minecraft con estrellas cuadradas, el cubo cae como un bloque colocado y el telón
// se rompe bloque a bloque desde el centro para descubrir el launcher. Un clic o Esc lo adelantan.
FocusScope {
    id: intro
    property bool playing: false
    // true mientras se rompe el telón: el launcher ya puede empezar a animarse debajo.
    property bool revealing: false
    property real progress: 0
    signal finished()
    visible: playing

    readonly property real blockSize: Math.max(width, height) / 14
    readonly property int cols: Math.max(1, Math.ceil(width / blockSize))
    readonly property int rows: Math.max(1, Math.ceil(height / blockSize))
    readonly property int wave: 480       // ms entre el bloque del centro y el más lejano
    readonly property int blockTime: 320  // ms que tarda cada bloque en romperse

    function finish() {
        if (!playing) return
        playing = false
        revealing = false
        finished()
    }
    function reveal() {
        if (!playing || revealing) return
        revealing = true
        breakAnim.start()
    }
    function skip() {
        stage.stop()
        reveal()
    }

    onPlayingChanged: {
        if (!playing) return
        // Sin animaciones (modo patata o partículas apagadas) se entra directo.
        if (!Theme.motion) { Qt.callLater(finish); return }
        progress = 0
        sky.opacity = 1
        mark.opacity = 1
        cube.opacity = 0
        drop.y = -44
        wordmark.opacity = 0
        rise.y = 6
        forceActiveFocus()
        stage.restart()
    }
    Keys.onEscapePressed: skip()
    MouseArea { anchors.fill: parent; enabled: !intro.revealing; onClicked: intro.skip() }

    // Telón: bloques del color de fondo que se encogen desde el centro hacia los bordes.
    Repeater {
        model: intro.playing ? intro.cols * intro.rows : 0
        Rectangle {
            readonly property int col: index % intro.cols
            readonly property int row: Math.floor(index / intro.cols)
            readonly property real cx: (intro.cols - 1) / 2
            readonly property real cy: (intro.rows - 1) / 2
            readonly property real distance: Math.hypot(col - cx, row - cy) / Math.max(1, Math.hypot(cx, cy))
            readonly property real t: Math.max(0, Math.min(1,
                (intro.progress * (intro.wave + intro.blockTime) - distance * intro.wave) / intro.blockTime))
            readonly property real eased: 1 - Math.pow(1 - t, 5)
            x: col * intro.blockSize
            y: row * intro.blockSize
            width: Math.ceil(intro.blockSize) + 1
            height: width
            color: Theme.base
            scale: 1 - eased
            opacity: 1 - eased
        }
    }

    Item {
        id: sky
        anchors.fill: parent

        // Estrellas fijas: cuadros duros que parpadean a saltos, como el cielo del juego.
        Repeater {
            model: intro.playing ? 40 : 0
            Rectangle {
                readonly property real peak: 0.4 + Math.random() * 0.5
                x: Math.random() * intro.width
                y: Math.random() * intro.height
                width: 3
                height: 3
                color: Theme.ink
                opacity: 0.15
                SequentialAnimation on opacity {
                    running: intro.playing
                    loops: Animation.Infinite
                    PauseAnimation { duration: 300 + Math.random() * 900 }
                    PropertyAction { value: peak }
                    PauseAnimation { duration: 400 + Math.random() * 900 }
                    PropertyAction { value: 0.15 }
                }
            }
        }

        // Estrellas fugaces: un cuadro con una cola de cuadros en escalera.
        Repeater {
            model: intro.playing ? 9 : 0
            Item {
                id: shot
                readonly property real startX: intro.width * (0.25 + Math.random() * 0.8)
                readonly property real startY: intro.height * (-0.08 + Math.random() * 0.5)
                readonly property real travel: Math.max(intro.width, intro.height) * (0.55 + Math.random() * 0.25)
                readonly property int life: 900 + Math.random() * 450
                x: startX
                y: startY
                width: 6
                height: 6
                opacity: 0
                Rectangle { anchors.fill: parent; color: "white" }
                Repeater {
                    model: 5
                    Rectangle {
                        x: (index + 1) * 8
                        y: -(index + 1) * 8
                        width: 6
                        height: 6
                        color: index < 2 ? Qt.alpha("white", 0.55 - index * 0.25)
                                         : Qt.alpha(Theme.accent, 0.7 - (index - 2) * 0.27)
                    }
                }
                SequentialAnimation {
                    running: intro.playing
                    PauseAnimation { duration: index * 110 + Math.random() * 90 }
                    ParallelAnimation {
                        NumberAnimation { target: shot; property: "x"; from: shot.startX; to: shot.startX - shot.travel; duration: shot.life }
                        NumberAnimation { target: shot; property: "y"; from: shot.startY; to: shot.startY + shot.travel; duration: shot.life }
                        SequentialAnimation {
                            NumberAnimation { target: shot; property: "opacity"; to: 1; duration: shot.life * 0.1 }
                            PauseAnimation { duration: shot.life * 0.75 }
                            NumberAnimation { target: shot; property: "opacity"; to: 0; duration: shot.life * 0.15 }
                        }
                    }
                }
            }
        }
    }

    Column {
        id: mark
        anchors.centerIn: parent
        spacing: 14
        Image {
            id: cube
            anchors.horizontalCenter: parent.horizontalCenter
            source: "qrc:/assets/cube.svg"
            width: 92
            height: 92
            sourceSize: Qt.size(184, 184)
            opacity: 0
            transform: Translate { id: drop; y: -44 }
        }
        Text {
            id: wordmark
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("OpenClient")
            color: Theme.ink
            font.family: Theme.fontFamily
            font.pixelSize: 30
            font.weight: Font.DemiBold
            opacity: 0
            transform: Translate { id: rise; y: 6 }
        }
    }

    // El cubo cae, rebota un poco y se asienta; después aparece el nombre.
    SequentialAnimation {
        id: stage
        PauseAnimation { duration: 100 }
        ParallelAnimation {
            NumberAnimation { target: cube; property: "opacity"; from: 0; to: 1; duration: 90 }
            SequentialAnimation {
                NumberAnimation { target: drop; property: "y"; from: -44; to: 0; duration: 380; easing.type: Easing.InQuad }
                NumberAnimation { target: drop; property: "y"; to: -6; duration: 110; easing.type: Easing.OutCubic }
                NumberAnimation { target: drop; property: "y"; to: 0; duration: 130; easing.type: Easing.InQuad }
            }
            SequentialAnimation {
                PauseAnimation { duration: 450 }
                ParallelAnimation {
                    NumberAnimation { target: wordmark; property: "opacity"; to: 1; duration: 260; easing.type: Easing.OutQuint }
                    NumberAnimation { target: rise; property: "y"; to: 0; duration: 260; easing.type: Easing.OutQuint }
                }
            }
        }
        PauseAnimation { duration: 380 }
        ScriptAction { script: intro.reveal() }
    }

    ParallelAnimation {
        id: breakAnim
        NumberAnimation { target: sky; property: "opacity"; to: 0; duration: 220; easing.type: Easing.OutCubic }
        NumberAnimation { target: mark; property: "opacity"; to: 0; duration: 220; easing.type: Easing.OutCubic }
        NumberAnimation { target: intro; property: "progress"; from: 0; to: 1; duration: intro.wave + intro.blockTime }
        onFinished: intro.finish()
    }
}
