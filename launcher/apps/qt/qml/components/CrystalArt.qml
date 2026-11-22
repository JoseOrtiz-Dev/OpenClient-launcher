import QtQuick
import OpenClient

Canvas {
    id: art
    property bool banner: false
    property color accent: Theme.accent
    property bool light: Theme.light
    onAccentChanged: requestPaint()
    onLightChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d")
        ctx.reset()
        if (width <= 0 || height <= 0)
            return

        const glow = ctx.createRadialGradient(width * 0.84, 0, 0, width * 0.84, 0, width * 0.76)
        glow.addColorStop(0, Qt.alpha(accent, banner ? (light ? 0.14 : 0.21) : (light ? 0.18 : 0.27)))
        glow.addColorStop(0.55, Qt.alpha(accent, banner ? 0.05 : 0.08))
        glow.addColorStop(1, Qt.alpha(accent, 0))
        ctx.fillStyle = glow
        ctx.fillRect(0, 0, width, height)

        if (!banner) {
            const underglow = ctx.createRadialGradient(width * 0.12, height, 0, width * 0.12, height, width * 0.56)
            underglow.addColorStop(0, Qt.alpha(accent, light ? 0.14 : 0.22))
            underglow.addColorStop(1, Qt.alpha(accent, 0))
            ctx.fillStyle = underglow
            ctx.fillRect(0, 0, width, height)
        }

        function orbit(x, y, radius, flatten, alpha) {
            ctx.save()
            ctx.translate(x, y)
            ctx.rotate(-0.40)
            ctx.scale(1, flatten)
            ctx.beginPath()
            ctx.arc(0, 0, radius, 0, Math.PI * 2)
            ctx.strokeStyle = Qt.alpha(accent, alpha)
            ctx.lineWidth = 1.15
            ctx.stroke()
            ctx.restore()
        }

        function crystal(x, y, size, rotation, opacity) {
            ctx.save()
            ctx.translate(x, y)
            ctx.rotate(rotation)
            ctx.scale(size, size)
            ctx.globalAlpha = opacity

            const fill = ctx.createLinearGradient(-0.5, -0.8, 0.6, 0.8)
            fill.addColorStop(0, Qt.alpha("white", light ? 0.65 : 0.52))
            fill.addColorStop(0.45, Qt.alpha(accent, 0.30))
            fill.addColorStop(1, accent)
            ctx.fillStyle = fill
            ctx.beginPath()
            ctx.moveTo(0, -0.82)
            ctx.lineTo(0.55, -0.36)
            ctx.lineTo(0.44, 0.48)
            ctx.lineTo(0, 0.80)
            ctx.lineTo(-0.48, 0.42)
            ctx.lineTo(-0.55, -0.37)
            ctx.closePath()
            ctx.fill()

            ctx.fillStyle = Qt.alpha(accent, 0.50)
            ctx.beginPath()
            ctx.moveTo(0, -0.82)
            ctx.lineTo(0.55, -0.36)
            ctx.lineTo(0.44, 0.48)
            ctx.lineTo(0, 0.80)
            ctx.lineTo(0.16, -0.20)
            ctx.closePath()
            ctx.fill()

            ctx.strokeStyle = Qt.alpha("white", light ? 0.38 : 0.24)
            ctx.lineWidth = 0.03
            ctx.beginPath()
            ctx.moveTo(0, -0.82)
            ctx.lineTo(0.16, -0.20)
            ctx.lineTo(0, 0.80)
            ctx.stroke()
            ctx.restore()
        }

        if (banner) {
            orbit(width - 134, height * 0.54, 156, 0.46, 0.22)
            orbit(width - 118, height * 0.58, 108, 0.50, 0.13)
            crystal(width - 102, height * 0.38, 50, 0.42, 0.30)
            crystal(width - 186, height * 0.16, 19, 0.48, 0.22)
            crystal(width - 36, height * 0.88, 25, 0.24, 0.24)
        } else {
            orbit(width * 0.22, height * 0.68, width * 0.33, 0.48, 0.22)
            orbit(width * 0.46, height * 0.40, width * 0.29, 0.53, 0.18)
            crystal(width * 0.52, height * 0.04, 98, 0.50, 0.33)
            crystal(width * 0.43, height * 0.90, 68, -0.42, 0.27)
            crystal(width * 0.96, height * 0.42, 56, 0.36, 0.29)
        }
    }
}
