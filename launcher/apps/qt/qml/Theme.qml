pragma Singleton
import QtQuick

QtObject {
    // Neutros con un leve matiz del acento y escalones de elevación cortos: el color vive en el acento,
    // no en las superficies. Cada acento conserva contraste AA con su texto (ver accentText).
    readonly property var themes: [
        // Todos los temas son Cristal: superficies opacas muy oscuras y orbes de color detrás (ver Orbs/GlassCard).
        // rgba() de la referencia visual -> rgba255(r, g, b, alfa).
        { id: "crystal-ruby", name: "Cristal Rubí", light: false, crystal: true, accent: "#e11d48",
          base: "#070509", sidebar: "#0c080f", surface: "#140d15", card: "#1a1119", border: "#2a2027", ink: "#f8fafc", dim: "#a8a0a8",
          accentSoft: "#fb7185", accentDeep: "#9f1239",
          orbPrimary: rgba255(190, 18, 60, 0.36), orbSecondary: rgba255(244, 63, 94, 0.24), orbTertiary: rgba255(124, 58, 237, 0.24), orbWarm: rgba255(245, 158, 11, 0.16),
          glassFill: rgba255(20, 13, 21, 0.62), glassStrong: rgba255(15, 10, 17, 0.78),
          heroA: rgba255(159, 18, 57, 0.42), heroB: rgba255(58, 16, 33, 0.62), heroC: rgba255(17, 11, 18, 0.80),
          btnFrom: "#ff4d73", btnTo: "#e11d48", btnShadow: rgba255(225, 29, 72, 0.45) },
        { id: "crystal-amethyst", name: "Cristal Amatista", light: false, crystal: true, accent: "#9333ea",
          base: "#06050b", sidebar: "#0b0912", surface: "#120e1c", card: "#171124", border: "#272036", ink: "#f8fafc", dim: "#a79fb8",
          accentSoft: "#c084fc", accentDeep: "#6b21a8",
          orbPrimary: rgba255(147, 51, 234, 0.38), orbSecondary: rgba255(217, 70, 239, 0.24), orbTertiary: rgba255(79, 70, 229, 0.28), orbWarm: rgba255(245, 158, 11, 0.14),
          glassFill: rgba255(18, 13, 27, 0.62), glassStrong: rgba255(13, 10, 22, 0.78),
          heroA: rgba255(126, 34, 206, 0.42), heroB: rgba255(46, 16, 75, 0.62), heroC: rgba255(14, 11, 22, 0.80),
          btnFrom: "#c084fc", btnTo: "#9333ea", btnShadow: rgba255(147, 51, 234, 0.48) },
        { id: "crystal-obsidian", name: "Cristal Obsidiana", light: false, crystal: true, accent: "#38bdf8",
          base: "#05070a", sidebar: "#090c10", surface: "#0e1218", card: "#131820", border: "#222a35", ink: "#f8fafc", dim: "#9aa6b5",
          accentSoft: "#7dd3fc", accentDeep: "#0369a1",
          orbPrimary: rgba255(14, 165, 233, 0.25), orbSecondary: rgba255(99, 102, 241, 0.22), orbTertiary: rgba255(139, 92, 246, 0.18), orbWarm: rgba255(245, 158, 11, 0.12),
          glassFill: rgba255(14, 18, 24, 0.64), glassStrong: rgba255(10, 13, 19, 0.80),
          heroA: rgba255(30, 58, 86, 0.46), heroB: rgba255(17, 24, 39, 0.68), heroC: rgba255(10, 12, 17, 0.84),
          btnFrom: "#38bdf8", btnTo: "#0284c7", btnShadow: rgba255(56, 189, 248, 0.42) },
        // Dark: el neutro del lanzador con el mismo material de cristal; orbes plateados muy contenidos.
        { id: "dark-obsidian", name: "Dark", light: false, crystal: true, accent: "#9bb3d9",
          base: "#060708", sidebar: "#0a0b0d", surface: "#0e1013", card: "#13151a", border: "#21252b", ink: "#f3f5f8", dim: "#98a1ad",
          accentSoft: "#c4d3ec", accentDeep: "#4a5f85",
          orbPrimary: rgba255(100, 120, 155, 0.24), orbSecondary: rgba255(170, 185, 215, 0.13), orbTertiary: rgba255(60, 75, 110, 0.22), orbWarm: rgba255(245, 158, 11, 0.05),
          glassFill: rgba255(15, 17, 21, 0.64), glassStrong: rgba255(11, 12, 15, 0.80),
          heroA: rgba255(58, 72, 102, 0.40), heroB: rgba255(21, 25, 33, 0.66), heroC: rgba255(9, 10, 12, 0.84),
          btnFrom: "#c4d3ec", btnTo: "#8aa3cc", btnShadow: rgba255(155, 179, 217, 0.30) }
    ]
    readonly property var current: themes.find(t => t.id === launcher.themeId) || themes[0]
    readonly property bool light: current.light
    property Item backdrop: null

    readonly property color accent: current.accent
    readonly property color base: current.base
    readonly property color sidebar: current.sidebar
    readonly property color surface: current.surface
    readonly property color card: current.card
    readonly property color border: current.border
    readonly property color ink: current.ink
    readonly property color dim: current.dim

    // Tema "Cristal": los cuatro temas lo usan. Las propiedades con respaldo (accentSoft, orbs…) se derivan del
    // acento si un tema no las define, así cualquier componente puede usarlas.
    readonly property bool crystal: current.crystal === true
    readonly property color accentSoft: current.accentSoft !== undefined ? current.accentSoft : highlight
    readonly property color accentDeep: current.accentDeep !== undefined ? current.accentDeep : Qt.darker(accent, light ? 1.25 : 1.6)
    readonly property color orbPrimary: current.orbPrimary !== undefined ? current.orbPrimary : Qt.alpha(accent, light ? 0.16 : 0.30)
    readonly property color orbSecondary: current.orbSecondary !== undefined ? current.orbSecondary : Qt.alpha(accentSoft, light ? 0.12 : 0.20)
    readonly property color orbTertiary: current.orbTertiary !== undefined ? current.orbTertiary : Qt.alpha(accentDeep, light ? 0.14 : 0.28)
    readonly property color orbWarm: current.orbWarm !== undefined ? current.orbWarm : rgba255(245, 158, 11, light ? 0.08 : 0.10)
    readonly property color glassFill: current.glassFill !== undefined ? current.glassFill : glass
    readonly property color glassStrong: current.glassStrong !== undefined ? current.glassStrong : glassElevated
    readonly property color glassBorder: light ? Qt.alpha("#4f5b6e", 0.18) : Qt.alpha("white", 0.09)
    readonly property color glassBorderHover: light ? Qt.alpha("#4f5b6e", 0.30) : Qt.alpha("white", 0.17)
    readonly property color glassHighlight: light ? Qt.alpha("white", 0.75) : Qt.alpha("white", 0.14)
    // Interior de las tarjetas con el estilo Cristal: campos, filas y separadores (los temas antiguos conservan sus colores).
    readonly property color fieldFill: Qt.rgba(0, 0, 0, 0.42)
    readonly property color fieldBorder: Qt.alpha("white", 0.12)
    readonly property color fieldBorderFocus: accentSoft
    readonly property color rowFill: Qt.alpha("white", 0.035)
    readonly property color rowFillHover: Qt.alpha("white", 0.07)
    readonly property color divider: Qt.alpha("white", 0.07)
    readonly property color heroA: current.heroA !== undefined ? current.heroA : Qt.alpha(accentDeep, light ? 0.20 : 0.42)
    readonly property color heroB: current.heroB !== undefined ? current.heroB : Qt.alpha(Qt.tint(base, Qt.alpha(accentDeep, 0.30)), light ? 0.40 : 0.62)
    readonly property color heroC: current.heroC !== undefined ? current.heroC : Qt.alpha(base, light ? 0.55 : 0.80)
    readonly property color btnFrom: current.btnFrom !== undefined ? current.btnFrom : highlight
    readonly property color btnTo: current.btnTo !== undefined ? current.btnTo : accent
    readonly property color btnShadow: current.btnShadow !== undefined ? current.btnShadow : Qt.alpha(accent, 0.45)

    readonly property color soft: Qt.alpha(accent, light ? 0.09 : 0.11)
    readonly property color edge: Qt.alpha(accent, light ? 0.30 : 0.26)
    readonly property color highlight: light ? accent : Qt.tint(accent, Qt.alpha("white", 0.28))
    // Pick the accent label colour by WCAG contrast instead of per-theme exceptions.
    readonly property color accentText: contrastWithWhite(accent) >= 4.5 ? "#ffffff" : "#0e1014"
    readonly property color accentPressed: Qt.darker(accent, 1.15)

    // Opaque glass: dense tint over a blurred backdrop, lit from the top edge.
    readonly property color glass: Qt.alpha(card, light ? 0.86 : 0.88)
    readonly property color glassElevated: Qt.alpha(surface, light ? 0.94 : 0.95)
    readonly property color specular: Qt.alpha("white", light ? 0.75 : 0.10)
    readonly property color rim: Qt.alpha(light ? "#4f5b6e" : "#ffffff", light ? 0.14 : 0.07)
    readonly property color scrim: Qt.alpha(base, light ? 0.58 : 0.72)

    readonly property color success: light ? "#1f8a5a" : "#3fb87f"
    readonly property color warning: light ? "#a86a0b" : "#e0a43a"
    readonly property color danger: light ? "#c2372f" : "#e5544b"
    // Texto sobre un relleno de aviso, elegido por contraste como accentText.
    readonly property color warningText: contrastWithWhite(warning) >= 4.5 ? "#ffffff" : "#0e1014"

    readonly property int radiusSm: 8
    readonly property int radiusMd: 12
    readonly property int radiusLg: 16
    readonly property int radiusXl: 20

    // Color propio de cada cargador (arte generado de banners, orbes del Inicio).
    function loaderTone(category) {
        return category === "fabric" ? "#20a670" : category === "forge" ? "#d97724" : category === "neoforge" ? "#e05d34"
            : category === "quilt" ? "#8a5cf5" : "#7e8da7"
    }

    // Menú principal: barra lateral estrecha, botón INICIAR y alto del héroe según el ancho.
    readonly property int railCollapsed: 64
    readonly property int railExpanded: 224
    readonly property int playHeight: 60
    function heroHeight(width) {
        return width >= 1400 ? 260 : width >= 1100 ? 240 : 200
    }

    // Fuentes incluidas en los recursos (resources/fonts, licencia OFL); main.cpp las registra al arrancar.
    readonly property string fontFamily: "Inter"
    readonly property string displayFamily: "Plus Jakarta Sans"
    readonly property var tabular: ({ "tnum": 1 })

    // The ambient-effects switch and Potato PC mode in Ajustes turn off interface motion.
    readonly property bool motion: launcher.particlesEnabled && !launcher.potatoMode
    // Brillo que sigue al cursor sobre las tarjetas: infinito y dependiente del puntero, así que exige movimiento.
    readonly property bool spotlight: launcher.spotlightEnabled && motion
    // Sistema de movimiento (tokens de Material 3). Sin movimiento todo vale 0.
    readonly property int durationPress: motion ? 80 : 0     // pulsar
    readonly property int durationFast: motion ? 120 : 0     // hover, color, feedback
    readonly property int durationBase: motion ? 200 : 0     // fundidos y cambios de estado
    readonly property int durationSlow: motion ? 300 : 0     // entrada de diálogos y páginas
    readonly property int durationExit: motion ? 150 : 0     // toda salida: más corta que su entrada
    readonly property var curveStandard: [0.2, 0, 0, 1, 1, 1]       // casi todo
    readonly property var curveEnter: [0.05, 0.7, 0.1, 1, 1, 1]     // lo que aparece (emphasized-decelerate)
    readonly property var curveExit: [0.3, 0, 0.8, 0.15, 1, 1]      // lo que se va (emphasized-accelerate)

    // Color a partir de los canales 0-255 de la referencia visual.
    function rgba255(r, g, b, a) {
        return Qt.rgba(r / 255, g / 255, b / 255, a)
    }
    function channel(value) {
        return value <= 0.03928 ? value / 12.92 : Math.pow((value + 0.055) / 1.055, 2.4)
    }
    function contrastWithWhite(colour) {
        const c = Qt.color(colour)
        const luminance = 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
        return 1.05 / (luminance + 0.05)
    }
}
