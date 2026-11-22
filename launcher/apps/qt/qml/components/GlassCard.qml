pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Shapes
import OpenClient

// Tarjeta de «cristal opaco»: relleno translúcido + grano fino + línea de destello + brillo que sigue al cursor.
// Sin desenfoque por tarjeta (el fondo ya viene suavizado por Orbs). API compatible con GlassPanel:
// los hijos se declaran dentro y quedan en un área con margen `padding`.
Rectangle {
    id: root
    default property alias content: interior.data
    property alias underlay: underlayItem.data
    property int padding: 20
    // "default" | "hero" | "sponsor" | "active"
    property string variant: "default"
    property bool hoverable: true
    // Relleno fuerte (glassStrong) para diálogos y menús.
    property bool strong: false
    // Compatibilidad con GlassPanel: `accented` equivale a la variante "active" si no se fija otra.
    property bool accented: false
    property bool frosted: false                 // sin efecto: el cristal ya no usa backdrop por tarjeta
    readonly property bool hovered: hover.hovered && hoverable
    readonly property string look: variant !== "default" ? variant : (accented ? "active" : "default")

    // El grano se encoge lo justo para no asomar por las esquinas redondeadas (r·(1-√½)+1 px).
    readonly property int grainInset: Math.ceil(radius * 0.293 + 1)

    radius: Theme.radiusLg
    antialiasing: true
    color: "transparent"
    border.width: 1
    border.color: look === "hero" ? Qt.alpha(Theme.accentSoft, hovered ? 0.38 : 0.22)
        : look === "active" ? Qt.alpha(Theme.accentSoft, hovered ? 0.6 : 0.42)
        : hovered ? Theme.glassBorderHover : Theme.glassBorder
    Behavior on border.color { MotionColor { role: "base" } }

    // Radio del círculo que llega a la esquina más lejana de (cx, cy), como `circle` en CSS.
    function farthest(cx, cy, w, h) {
        return Math.hypot(Math.max(cx, w - cx), Math.max(cy, h - cy))
    }

    HoverHandler { id: hover; enabled: root.hoverable }

    // Relleno según variante. Solo se instancia el de la variante activa.
    Loader {
        anchors.fill: parent
        anchors.margins: 1
        sourceComponent: root.look === "hero" ? heroFill
            : root.look === "sponsor" ? sponsorFill
            : root.look === "active" ? activeFill : plainFill
    }

    Component {
        id: plainFill
        Rectangle {
            radius: Math.max(0, root.radius - 1)
            antialiasing: true
            color: root.strong ? Theme.glassStrong : Theme.glassFill
            Behavior on color { MotionColor { role: "base" } }
        }
    }

    Component {
        id: heroFill
        Shape {
            id: s
            preferredRendererType: Shape.CurveRenderer
            readonly property real r: Math.max(0, root.radius - 1)
            readonly property real q: (s.width + s.height) / 4   // 135°: la línea del degradado cruza el centro

            // Base: degradado diagonal heroB → heroC
            ShapePath {
                strokeWidth: -1
                fillColor: "transparent"
                fillGradient: LinearGradient {
                    x1: s.width / 2 - s.q; y1: s.height / 2 - s.q
                    x2: s.width / 2 + s.q; y2: s.height / 2 + s.q
                    GradientStop { position: 0; color: Theme.heroB }
                    GradientStop { position: 1; color: Theme.heroC }
                }
                PathRectangle { width: s.width; height: s.height; radius: s.r }
            }
            // Resplandor de acento abajo a la derecha: elipse 70 % × 85 % en (88 %, 85 %)
            ShapePath {
                id: glowA
                readonly property real k: 0.85 * s.height / Math.max(1, 0.70 * s.width)
                strokeWidth: -1
                fillColor: "transparent"
                fillTransform: Qt.matrix4x4(1, 0, 0, 0, 0, k, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1)
                fillGradient: RadialGradient {
                    centerX: 0.88 * s.width; centerY: 0.85 * s.height / glowA.k
                    focalX: centerX; focalY: centerY
                    centerRadius: 0.70 * s.width
                    GradientStop { position: 0; color: Qt.alpha(Theme.accent, 0.22) }
                    GradientStop { position: 0.62; color: Qt.alpha(Theme.accent, 0) }
                }
                PathRectangle { width: s.width; height: s.height; radius: s.r }
            }
            // Luz heroA arriba a la izquierda: elipse 85 % × 95 % en (15 %, 10 %)
            ShapePath {
                id: glowB
                readonly property real k: 0.95 * s.height / Math.max(1, 0.85 * s.width)
                strokeWidth: -1
                fillColor: "transparent"
                fillTransform: Qt.matrix4x4(1, 0, 0, 0, 0, k, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1)
                fillGradient: RadialGradient {
                    centerX: 0.15 * s.width; centerY: 0.10 * s.height / glowB.k
                    focalX: centerX; focalY: centerY
                    centerRadius: 0.85 * s.width
                    GradientStop { position: 0; color: Theme.heroA }
                    GradientStop { position: 0.62; color: Qt.alpha(Theme.heroA, 0) }
                }
                PathRectangle { width: s.width; height: s.height; radius: s.r }
            }
        }
    }

    Component {
        id: sponsorFill
        Shape {
            id: s
            preferredRendererType: Shape.CurveRenderer
            readonly property real r: Math.max(0, root.radius - 1)

            ShapePath {
                strokeWidth: -1
                fillColor: Theme.glassFill
                PathRectangle { width: s.width; height: s.height; radius: s.r }
            }
            // Resplandor «violeta» (token orbSecondary) a la izquierda
            ShapePath {
                strokeWidth: -1
                fillColor: "transparent"
                fillGradient: RadialGradient {
                    centerX: 0.12 * s.width; centerY: 0.52 * s.height
                    focalX: centerX; focalY: centerY
                    centerRadius: root.farthest(centerX, centerY, s.width, s.height)
                    GradientStop { position: 0; color: Qt.alpha(Theme.orbSecondary, 0.30) }
                    GradientStop { position: 0.52; color: Qt.alpha(Theme.orbSecondary, 0) }
                }
                PathRectangle { width: s.width; height: s.height; radius: s.r }
            }
            // Resplandor de acento arriba a la derecha
            ShapePath {
                strokeWidth: -1
                fillColor: "transparent"
                fillGradient: RadialGradient {
                    centerX: 0.82 * s.width; centerY: 0.20 * s.height
                    focalX: centerX; focalY: centerY
                    centerRadius: root.farthest(centerX, centerY, s.width, s.height)
                    GradientStop { position: 0; color: Qt.alpha(Theme.accent, 0.24) }
                    GradientStop { position: 0.55; color: Qt.alpha(Theme.accent, 0) }
                }
                PathRectangle { width: s.width; height: s.height; radius: s.r }
            }
        }
    }

    Component {
        id: activeFill
        Shape {
            id: s
            preferredRendererType: Shape.CurveRenderer
            readonly property real r: Math.max(0, root.radius - 1)

            ShapePath {
                strokeWidth: -1
                fillColor: Theme.glassStrong
                PathRectangle { width: s.width; height: s.height; radius: s.r }
            }
            // Resplandor de acento a la izquierda
            ShapePath {
                strokeWidth: -1
                fillColor: "transparent"
                fillGradient: RadialGradient {
                    centerX: 0.15 * s.width; centerY: 0.5 * s.height
                    focalX: centerX; focalY: centerY
                    centerRadius: root.farthest(centerX, centerY, s.width, s.height)
                    GradientStop { position: 0; color: Qt.alpha(Theme.accent, 0.24) }
                    GradientStop { position: 0.65; color: Qt.alpha(Theme.accent, 0) }
                }
                PathRectangle { width: s.width; height: s.height; radius: s.r }
            }
        }
    }

    // Capa opcional entre el relleno y el grano (p. ej. el banner del héroe). Queda dentro del borde; recórtala a radius - 1.
    Item { id: underlayItem; anchors.fill: parent; anchors.margins: 1 }

    // Grano fino en mosaico, sin capas ni máscaras. Inset para no desbordar las esquinas.
    Image {
        anchors.fill: parent
        anchors.margins: root.grainInset
        source: "qrc:/assets/glass/noise.png"
        fillMode: Image.Tile
        opacity: 0.045
        smooth: false
        cache: true
        asynchronous: true
    }

    // Brillo de cursor: círculo de 210 px recortado a la forma redondeada. Solo cambia la opacidad.
    Loader {
        anchors.fill: parent
        anchors.margins: 1
        active: root.hoverable && Theme.spotlight
        sourceComponent: Shape {
            id: spot
            preferredRendererType: Shape.CurveRenderer
            opacity: root.hovered ? 1 : 0
            visible: opacity > 0
            Behavior on opacity { MotionNumber { role: "base" } }
            ShapePath {
                strokeWidth: -1
                fillColor: "transparent"
                fillGradient: RadialGradient {
                    centerX: hover.point.position.x - 1; centerY: hover.point.position.y - 1
                    focalX: centerX; focalY: centerY
                    centerRadius: 210
                    GradientStop { position: 0; color: Qt.alpha(Theme.accentSoft, 0.14) }
                    GradientStop { position: 0.4; color: Qt.rgba(1, 1, 1, 0.04) }
                    GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0) }
                }
                PathRectangle { width: spot.width; height: spot.height; radius: Math.max(0, root.radius - 1) }
            }
        }
    }

    // Línea de destello superior (inset 1 px), más tenue hacia las esquinas.
    Rectangle {
        y: 1
        x: root.radius
        width: Math.max(0, root.width - root.radius * 2)
        height: 1
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Qt.alpha(Theme.glassHighlight, 0) }
            GradientStop { position: 0.15; color: Theme.glassHighlight }
            GradientStop { position: 0.85; color: Theme.glassHighlight }
            GradientStop { position: 1; color: Qt.alpha(Theme.glassHighlight, 0) }
        }
    }

    Item { id: interior; anchors.fill: parent; anchors.margins: root.padding }
}
