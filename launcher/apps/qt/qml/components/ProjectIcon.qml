import QtQuick
import OpenClient

// Icono de proyecto: esqueleto con pulso mientras carga, imagen con fundido al estar lista
// y monograma (iniciales + tono estable por nombre) si no hay imagen o falla.
// Solo anima opacity; sin movimiento cuando Theme.motion es falso.
Item {
    id: root
    property string source: ""
    property string name: ""
    // "mod" | "modpack" | "resourcepack" | "shader" | ""
    property string kind: ""
    property int size: 48
    property bool badge: false
    property real radius: size >= 56 ? Theme.radiusMd : Theme.radiusSm

    readonly property bool hasSource: source.length > 0
    readonly property bool isHttp: /^https?:\/\//i.test(source)
    readonly property bool ready: img.status === Image.Ready
    readonly property bool fallback: !hasSource || (img.status === Image.Error && !retrying)
    readonly property bool loading: hasSource && !ready && !fallback
    readonly property int fadeMs: Theme.durationBase

    // Reintento único ante error de red.
    property bool retried: false
    property bool retrying: false
    property bool reloading: false

    // Iniciales: primera letra de las dos primeras palabras, o las 2 primeras de la única palabra.
    readonly property var words: name.split(/[^A-Za-z0-9À-ɏ]+/).filter(w => w.length > 0)
    readonly property string initials: words.length === 0 ? ""
        : (words.length === 1 ? words[0].substring(0, 2) : words[0][0] + words[1][0]).toUpperCase()

    // Seis tonos de tokens existentes; el índice sale de la suma de códigos del nombre.
    readonly property var tones: [
        Theme.accent, Theme.success, Theme.warning, Theme.danger, Theme.highlight,
        Qt.tint(Theme.success, Qt.alpha(Theme.accent, 0.5))
    ]
    readonly property int toneIndex: {
        let sum = 0
        for (let i = 0; i < name.length; ++i) sum += name.charCodeAt(i)
        return sum % tones.length
    }
    readonly property color tone: tones[toneIndex]
    readonly property color toneText: Theme.light ? Qt.darker(tone, 1.25) : tone
    readonly property string kindGlyph: kind === "mod" ? "puzzle"
        : kind === "resourcepack" ? "image"
        : kind === "shader" ? "sun"
        : "layers"

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    onSourceChanged: {
        retried = false
        retrying = false
        reloading = false
        retryTimer.stop()
    }

    Timer {
        id: retryTimer
        interval: 1500
        onTriggered: {
            root.reloading = true
            Qt.callLater(() => { root.reloading = false; root.retrying = false })
        }
    }

    Rectangle {
        id: frame
        anchors.fill: parent
        radius: root.radius
        color: root.fallback ? Qt.alpha(root.tone, 0.14) : root.ready ? Theme.soft : "transparent"
        border.width: 1
        border.color: root.fallback ? Qt.alpha(root.tone, 0.34) : Theme.edge

        Rectangle {
            id: skeleton
            anchors.fill: parent
            radius: frame.radius
            color: Theme.soft
            visible: root.loading
            opacity: 1
            SequentialAnimation on opacity {
                running: root.loading && Theme.motion
                loops: Animation.Infinite
                onRunningChanged: if (!running) skeleton.opacity = 1
                NumberAnimation { to: 0.45; duration: 700; easing.type: Easing.InOutQuad } // motion-ok: esqueleto infinito gateado
                NumberAnimation { to: 1; duration: 700; easing.type: Easing.InOutQuad } // motion-ok: esqueleto infinito gateado
            }
        }

        Image {
            id: img
            anchors.fill: parent
            anchors.margins: Math.max(2, Math.round(root.size * 0.06))
            source: root.reloading ? "" : root.source
            sourceSize: Qt.size(root.size * 2, root.size * 2)
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            cache: true
            mipmap: true
            opacity: root.ready ? 1 : 0
            Behavior on opacity { MotionNumber { role: "base" } }
            onStatusChanged: {
                if (status === Image.Error && root.isHttp && !root.retried) {
                    root.retried = true
                    root.retrying = true
                    retryTimer.restart()
                }
            }
        }

        Text {
            anchors.centerIn: parent
            visible: root.fallback && root.initials.length > 0
            text: root.initials
            textFormat: Text.PlainText
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(root.size * 0.36)
            font.weight: Font.DemiBold
            color: root.toneText
        }

        Glyph {
            anchors.centerIn: parent
            width: Math.round(root.size * 0.5); height: width
            visible: root.fallback && root.initials.length === 0
            name: root.kindGlyph
            color: Theme.highlight
        }
    }

    Rectangle {
        visible: root.badge && root.size >= 40 && root.kind.length > 0
        width: 16; height: 16; radius: 8
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: -4
        anchors.bottomMargin: -4
        color: Theme.surface
        border.width: 1
        border.color: Theme.edge
        Glyph {
            anchors.centerIn: parent
            width: 10; height: 10
            name: root.kindGlyph
            color: Theme.highlight
        }
    }
}
