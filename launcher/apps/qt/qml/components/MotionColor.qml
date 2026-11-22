import QtQuick
import OpenClient

// ColorAnimation con la duración y la curva del sistema de movimiento.
// role: press | fast | base | slow | enter | exit. Por defecto fast: es el cambio de color de un hover.
ColorAnimation {
    property string role: "fast"
    duration: role === "press" ? Theme.durationPress
            : role === "base" ? Theme.durationBase
            : role === "slow" || role === "enter" ? Theme.durationSlow
            : role === "exit" ? Theme.durationExit
            : Theme.durationFast
    easing.type: Easing.BezierSpline
    easing.bezierCurve: role === "enter" ? Theme.curveEnter : role === "exit" ? Theme.curveExit : Theme.curveStandard
}
