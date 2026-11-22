import QtQuick
import QtQuick.Controls
import OpenClient

Menu {
    id: root
    width: 228
    padding: 6
    margins: 8
    font.family: Theme.fontFamily

    background: Item {
        implicitWidth: root.width
        // Cristal: base opaca + material de GlassCard (fuerte); sin desenfoque.
        Loader {
            anchors.fill: parent
            active: Theme.crystal
            sourceComponent: Item {
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radiusMd
                    color: Theme.base
                }
                GlassCard {
                    anchors.fill: parent
                    strong: true
                    hoverable: false
                    radius: Theme.radiusMd
                }
            }
        }
        Rectangle {
            anchors.fill: parent
            visible: !Theme.crystal
            radius: Theme.radiusMd
            antialiasing: true
            color: Theme.glassElevated
            border.color: Qt.alpha(Theme.ink, Theme.light ? 0.14 : 0.09)

            Rectangle {
                anchors.top: parent.top
                anchors.topMargin: 1
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.max(0, parent.width - Theme.radiusMd * 2)
                height: 1
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: "transparent" }
                    GradientStop { position: 0.5; color: Theme.specular }
                    GradientStop { position: 1; color: "transparent" }
                }
            }
        }
    }

    enter: Transition {
        MotionNumber { role: "enter"; property: "opacity"; from: 0; to: 1 }
        MotionNumber { role: "enter"; property: "scale"; from: 0.97; to: 1 }
    }
    exit: Transition {
        MotionNumber { role: "exit"; property: "opacity"; to: 0 }
    }
}
