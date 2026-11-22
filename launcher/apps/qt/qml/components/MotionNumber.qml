import QtQuick
import OpenClient

// NumberAnimation con la duración y la curva del sistema de movimiento.
// role: press | fast | base | slow | enter | exit. Sin movimiento (Theme.motion = false) dura 0.
NumberAnimation {
    property string role: "base"
    duration: role === "press" ? Theme.durationPress
            : role === "fast" ? Theme.durationFast
            : role === "slow" || role === "enter" ? Theme.durationSlow
            : role === "exit" ? Theme.durationExit
            : Theme.durationBase
    easing.type: Easing.BezierSpline
    easing.bezierCurve: role === "enter" ? Theme.curveEnter : role === "exit" ? Theme.curveExit : Theme.curveStandard
}
