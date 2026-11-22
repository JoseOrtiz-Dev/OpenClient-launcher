import QtQuick
import OpenClient

// Motas de luz con profundidad: las cercanas son grandes, difusas y rápidas; las lejanas, pequeñas y
// lentas. Cada mota nace en un punto, deriva con una curva suave y se desvanece; la siguiente vida
// empieza en otro sitio, así el fondo no parece una cinta que se repite.
Item {
    id: root
    property bool running: false
    property real density: 1
    property real motionSeconds: 0
    // Paralaje con el cursor, suavizado por fotograma (-0.5 … 0.5).
    property real pointerX: 0
    property real pointerY: 0

    // dir: deriva en px/s a profundidad 1 · sway: ondulación en px · life: vida en s · size: px · alpha: opacidad máxima
    readonly property var profiles: ({
        // Cristal: mismo carácter que su tema hermano, algo más suave para no competir con los orbes.
        "crystal-ruby":     { dirX:  0.10, dirY: -1.00, speed: 12, sway: 16, curl: 0.35, lifeMin:  9, lifeMax: 16, sizeMin: 5, sizeMax: 32, alpha: 0.58, twinkle: 0.30 },
        "crystal-amethyst": { dirX: -0.12, dirY: -0.60, speed:  8, sway: 18, curl: 0.30, lifeMin: 10, lifeMax: 18, sizeMin: 5, sizeMax: 30, alpha: 0.58, twinkle: 0.28 },
        "crystal-obsidian": { dirX:  0.65, dirY: -0.20, speed:  7, sway: 10, curl: 0.22, lifeMin: 11, lifeMax: 19, sizeMin: 4, sizeMax: 26, alpha: 0.48, twinkle: 0.12 },
        "dark-obsidian":    { dirX:  0.65, dirY: -0.20, speed:  7, sway: 10, curl: 0.22, lifeMin: 11, lifeMax: 19, sizeMin: 4, sizeMax: 26, alpha: 0.46, twinkle: 0.12 }
    })
    readonly property var profile: profiles[Theme.current.id] || profiles["crystal-ruby"]
    readonly property url spriteSource: "qrc:/assets/particles/" + Theme.current.id + ".svg"
    readonly property int particleCount: Math.min(42, Math.max(12, Math.round(width * height * density / 22000)))

    visible: launcher.particlesEnabled
    clip: true

    function seed(index: int, salt: int): real {
        return ((index * (157 + salt * 18) + salt * 127 + index * index * 43) % 997) / 997
    }
    // Ruido determinista por partícula y vida: misma entrada, misma posición, sin estado.
    function hash(index: int, cycle: int, salt: int): real {
        const v = Math.sin(index * 12.9898 + cycle * 78.233 + salt * 37.719) * 43758.5453
        return v - Math.floor(v)
    }
    function smooth(edge0: real, edge1: real, x: real): real {
        const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0)))
        return t * t * (3 - 2 * t)
    }

    HoverHandler { id: pointer }

    FrameAnimation {
        id: frames
        running: root.running && root.visible
        onTriggered: {
            const dt = Math.min(frames.frameTime, 0.05)
            root.motionSeconds += dt
            const tx = pointer.hovered && Theme.motion ? pointer.point.position.x / Math.max(1, root.width) - 0.5 : 0
            const ty = pointer.hovered && Theme.motion ? pointer.point.position.y / Math.max(1, root.height) - 0.5 : 0
            const follow = Math.min(1, dt * 2.4)
            root.pointerX += (tx - root.pointerX) * follow
            root.pointerY += (ty - root.pointerY) * follow
        }
    }

    Repeater {
        model: root.particleCount
        Image {
            id: mote
            required property int index
            readonly property var p: root.profile
            readonly property real depth: root.seed(index, 2)
            readonly property real phase: root.seed(index, 4) * 2 * Math.PI
            readonly property real life: p.lifeMin + (p.lifeMax - p.lifeMin) * root.seed(index, 3)
            readonly property real clock: root.motionSeconds + root.seed(index, 6) * life
            readonly property int cycle: Math.floor(clock / life)
            readonly property real age: clock - cycle * life
            readonly property real lifeProgress: age / life
            readonly property real pace: 0.35 + 0.95 * depth
            readonly property real envelope: root.smooth(0, 0.16, lifeProgress) * (1 - root.smooth(0.76, 1, lifeProgress))
            readonly property real twinkle: 1 - p.twinkle * (0.5 + 0.5 * Math.sin(root.motionSeconds * (0.7 + depth * 1.1) + phase))
            // Las del medio son las más visibles; las muy cercanas se vuelven bokeh tenue.
            readonly property real presence: 0.74 + 0.26 * Math.sin(Math.PI * Math.min(1, depth * 1.15))

            source: root.spriteSource
            sourceSize: Qt.size(48, 48)
            smooth: true
            width: p.sizeMin + (p.sizeMax - p.sizeMin) * Math.pow(depth, 1.7)
            height: width
            x: root.hash(index, cycle, 1) * (root.width + 80) - 40 - width / 2
                + p.dirX * p.speed * pace * age
                + Math.sin(age * p.curl + phase) * p.sway * pace
                - root.pointerX * 34 * depth
            y: root.hash(index, cycle, 2) * (root.height + 80) - 40 - height / 2
                + p.dirY * p.speed * pace * age
                + Math.cos(age * p.curl * 0.7 + phase) * p.sway * 0.45 * pace
                - root.pointerY * 22 * depth
            scale: 0.85 + 0.15 * envelope
            opacity: envelope * twinkle * presence * p.alpha
        }
    }
}
