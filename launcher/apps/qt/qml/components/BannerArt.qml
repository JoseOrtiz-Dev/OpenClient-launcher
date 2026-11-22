import QtQuick
import OpenClient

// Arte generado para instancias sin imagen: paisaje de bloques en tres capas, cielo con la paleta del acento o del cargador,
// y unas pocas estrellas, nubes y un sol/luna cuadrados. Determinista por categoría + versión + semilla.
// Se dibuja una vez (y al cambiar las propiedades o el tamaño); no anima nada. Todo es dibujo propio, sin assets.
Item {
    id: root
    property string category: "vanilla"     // vanilla | fabric | quilt | forge | neoforge
    property string mcVersion: ""
    property string seed: ""                // id o nombre de la instancia
    property color accent: "transparent"    // transparente: se usa el color del cargador
    readonly property color tone: accent.a > 0 ? accent : Theme.loaderTone(category)
    readonly property string key: category + "|" + mcVersion + "|" + seed

    onKeyChanged: canvas.requestPaint()
    onToneChanged: canvas.requestPaint()

    Canvas {
        id: canvas
        anchors.fill: parent
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            const w = width, h = height
            ctx.clearRect(0, 0, w, h)
            if (w < 8 || h < 8) return

            // Generador pseudoaleatorio estable (FNV-1a + mulberry32).
            let hash = 2166136261
            for (let i = 0; i < root.key.length; ++i) hash = Math.imul(hash ^ root.key.charCodeAt(i), 16777619)
            let state = hash >>> 0
            const rnd = () => {
                state = (state + 0x6D2B79F5) >>> 0
                let t = state
                t = Math.imul(t ^ (t >>> 15), t | 1)
                t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
                return ((t ^ (t >>> 14)) >>> 0) / 4294967296
            }

            const cell = Math.max(3, Math.round(h / 22))
            const cols = Math.ceil(w / cell) + 1
            const rows = Math.ceil(h / cell)
            const tone = root.tone
            const hue = tone.hslHue >= 0 ? tone.hslHue : 0.6
            const sat = Math.min(0.7, Math.max(0.28, tone.hslSaturation))

            // Cielo: casi negro arriba, luz del acento en el horizonte.
            const sky = ctx.createLinearGradient(0, 0, 0, h)
            sky.addColorStop(0, Qt.hsla(hue, sat * 0.55, 0.055, 1))
            sky.addColorStop(0.55, Qt.hsla(hue, sat * 0.7, 0.15, 1))
            sky.addColorStop(1, Qt.hsla(hue, sat, 0.30, 1))
            ctx.fillStyle = sky
            ctx.fillRect(0, 0, w, h)

            // Estrellas: cuadrados pequeños en la mitad alta.
            const star = Math.max(1, Math.round(cell * 0.3))
            for (let i = 0; i < 16; ++i) {
                const sx = Math.floor(rnd() * cols) * cell
                const sy = Math.floor(rnd() * rows * 0.4) * cell
                ctx.fillStyle = Qt.rgba(1, 1, 1, 0.25 + rnd() * 0.4)
                ctx.fillRect(sx, sy, star, star)
            }

            // Sol o luna cuadrados con un halo en bloques.
            const sunCol = Math.floor((0.18 + rnd() * 0.64) * cols)
            const sunRow = Math.floor((0.12 + rnd() * 0.2) * rows)
            ctx.fillStyle = Qt.hsla(hue, sat * 0.8, 0.8, 0.10)
            ctx.fillRect((sunCol - 1) * cell, (sunRow - 1) * cell, cell * 5, cell * 5)
            ctx.fillStyle = Qt.hsla(hue, sat * 0.6, 0.86, 0.55)
            ctx.fillRect(sunCol * cell, sunRow * cell, cell * 3, cell * 3)

            // Nubes rectangulares muy tenues.
            for (let i = 0; i < 4; ++i) {
                const cx = Math.floor(rnd() * cols)
                const cy = 1 + Math.floor(rnd() * rows * 0.35)
                const cw = 4 + Math.floor(rnd() * 4)
                ctx.fillStyle = Qt.hsla(hue, sat * 0.5, 0.85, 0.07 + rnd() * 0.05)
                ctx.fillRect(cx * cell, cy * cell, cw * cell, cell)
                ctx.fillRect((cx + 1) * cell, (cy - 1) * cell, Math.max(2, cw - 3) * cell, cell)
            }

            // Capa de relieve: alturas en bloques con ruido de valor suavizado (escalones).
            const terrain = (base, amp, step, lightness, satMul, trees) => {
                const lattice = []
                for (let i = 0; i < Math.ceil(cols / step) + 2; ++i) lattice.push(rnd())
                const heights = []
                for (let c = 0; c < cols; ++c) {
                    const i = Math.floor(c / step)
                    const f = c / step - i
                    const s = f * f * (3 - 2 * f)
                    heights.push(Math.max(1, Math.round(base + ((lattice[i] * (1 - s) + lattice[i + 1] * s) - 0.5) * 2 * amp)))
                }
                const body = Qt.hsla(hue, sat * satMul, lightness, 1)
                const crest = Qt.hsla(hue, sat * satMul, lightness + 0.04, 1)
                for (let c = 0; c < cols; ++c) {
                    const top = h - heights[c] * cell
                    ctx.fillStyle = body
                    ctx.fillRect(c * cell, top, cell, h - top)
                    ctx.fillStyle = crest
                    ctx.fillRect(c * cell, top, cell, cell)
                }
                if (!trees) return
                ctx.fillStyle = Qt.hsla(hue, sat * satMul, lightness - 0.01, 1)
                for (let c = 2; c < cols - 2; c += 4 + Math.floor(rnd() * 7)) {
                    const top = h - heights[c] * cell
                    ctx.fillRect(c * cell, top - 2 * cell, cell, 2 * cell)
                    ctx.fillRect((c - 1) * cell, top - 4 * cell, cell * 3, 2 * cell)
                    ctx.fillRect(c * cell, top - 5 * cell, cell, cell)
                }
            }
            terrain(rows * 0.42, rows * 0.16, 7, 0.17, 0.55, false)
            terrain(rows * 0.28, rows * 0.12, 5, 0.115, 0.5, false)
            terrain(rows * 0.15, rows * 0.07, 4, 0.07, 0.45, true)
        }
    }
}
