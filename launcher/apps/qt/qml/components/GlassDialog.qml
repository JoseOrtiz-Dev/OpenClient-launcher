import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

Dialog {
    id: dialog
    property string glyph: "layers"
    padding: 20
    modal: true
    Overlay.modal: Rectangle {
        color: Theme.scrim
        Behavior on opacity { MotionNumber { role: "base" } }
    }
    enter: Transition {
        ParallelAnimation {
            MotionNumber { role: "enter"; property: "opacity"; from: 0; to: 1 }
            MotionNumber { role: "enter"; property: "scale"; from: 0.96; to: 1.0 }
        }
    }
    exit: Transition {
        ParallelAnimation {
            MotionNumber { role: "exit"; property: "opacity"; from: 1; to: 0 }
            MotionNumber { role: "exit"; property: "scale"; from: 1.0; to: 0.98 }
        }
    }
    background: Item {
        // Cristal: base opaca para que el texto no se mezcle con la página, y el material de GlassCard (fuerte) encima.
        Loader {
            anchors.fill: parent
            active: Theme.crystal
            sourceComponent: Item {
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radiusXl
                    color: Theme.base
                }
                GlassCard {
                    anchors.fill: parent
                    strong: true
                    hoverable: false
                    radius: Theme.radiusXl
                }
            }
        }
        Rectangle {
            anchors.fill: parent
            visible: !Theme.crystal
            radius: Theme.radiusXl
            color: Theme.glassElevated
            border.color: Theme.rim
            border.width: 1
            Rectangle {
                anchors.top: parent.top
                anchors.topMargin: 1
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.max(0, parent.width - parent.radius * 2)
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
    header: RowLayout {
        spacing: 12
        Rectangle {
            Layout.leftMargin: 20
            Layout.topMargin: 18
            Layout.preferredWidth: 36; Layout.preferredHeight: 36
            radius: Theme.radiusSm
            color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
            border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
            Glyph { anchors.centerIn: parent; name: dialog.glyph; color: Theme.highlight }
        }
        Label {
            Layout.fillWidth: true
            Layout.topMargin: 18
            Layout.rightMargin: 20
            text: dialog.title
            color: Theme.ink
            font.pixelSize: 18
            font.weight: Font.DemiBold
        }
    }
}
