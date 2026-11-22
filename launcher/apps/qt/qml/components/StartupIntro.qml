import QtQuick
import QtQuick.Layouts
import OpenClient

FocusScope {
    id: intro
    property bool playing: false
    signal finished()
    visible: playing
    opacity: 1

    function finish() {
        if (!playing) return
        playing = false
        finished()
    }

    onPlayingChanged: if (playing) forceActiveFocus()
    Keys.onEscapePressed: finish()
    MouseArea { anchors.fill: parent; onClicked: intro.finish() }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.tint(Theme.base, Theme.soft) }
            GradientStop { position: 1; color: Theme.base }
        }
        CrystalArt { anchors.fill: parent; opacity: 0.55 }
    }
    ColumnLayout {
        id: mark
        anchors.centerIn: parent
        spacing: 8
        opacity: 0
        scale: 0.92
        Image {
            source: "qrc:/assets/cube.svg"
            Layout.preferredWidth: 92
            Layout.preferredHeight: 92
            Layout.alignment: Qt.AlignHCenter
            sourceSize: Qt.size(184, 184)
        }
        Text {
            text: qsTr("OpenClient")
            color: Theme.ink
            font.family: Theme.fontFamily
            font.pixelSize: 34
            font.weight: Font.Bold
            Layout.alignment: Qt.AlignHCenter
        }
        Text {
            text: qsTr("Minecraft Java Edition")
            color: Theme.dim
            font.family: Theme.fontFamily
            font.pixelSize: 12
            Layout.alignment: Qt.AlignHCenter
        }
    }
    SequentialAnimation {
        running: intro.playing
        ParallelAnimation {
            NumberAnimation { target: mark; property: "opacity"; from: 0; to: 1; duration: 220; easing.type: Easing.OutCubic }
            NumberAnimation { target: mark; property: "scale"; from: 0.92; to: 1.0; duration: 360; easing.type: Easing.OutCubic }
        }
        PauseAnimation { duration: 240 }
        ParallelAnimation {
            NumberAnimation { target: intro; property: "opacity"; from: 1; to: 0; duration: 200; easing.type: Easing.InCubic }
            NumberAnimation { target: mark; property: "scale"; from: 1.0; to: 1.03; duration: 200; easing.type: Easing.InCubic }
        }
        ScriptAction { script: intro.finish() }
    }
}
