import QtQuick
import OpenClient

// Inicio: el contenido principal a todo el ancho y, en la esquina superior derecha, solo la cabeza de la cuenta;
// se despliega al acercarse o pulsar (las novedades siguen en el botón de la barra superior).
Item {
    id: page
    property alias selectedId: content.selectedId
    signal instanceSelected(string id)
    signal navigate(string destination)
    signal createRequested()
    signal accountRequested()
    implicitHeight: content.implicitHeight

    HomeContent {
        id: content
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        onInstanceSelected: id => page.instanceSelected(id)
        onNavigate: destination => page.navigate(destination)
        onCreateRequested: page.createRequested()
        onAccountRequested: page.accountRequested()
    }
    AccountChip {
        id: chip
        anchors.top: parent.top
        anchors.right: parent.right
        z: 10
        onAccountRequested: page.accountRequested()
    }
    // Anillo que late sobre el punto verde «en línea» de la cabeza (11 px, a -3 px de la esquina inferior derecha).
    // Solo con sesión válida; el bucle corre únicamente con movimiento activado y la aplicación en primer plano.
    Rectangle {
        id: onlinePing
        readonly property bool live: launcher.accountName.length > 0 && !chip.needsLogin
        readonly property bool running: live && Theme.motion && page.visible && Qt.application.state === Qt.ApplicationActive
        anchors.right: chip.right; anchors.rightMargin: -3
        anchors.bottom: chip.bottom; anchors.bottomMargin: -3
        width: 11; height: 11; radius: 6
        color: chip.tone
        opacity: 0
        z: 11
        onRunningChanged: if (!running) { opacity = 0; scale = 1 }
        ParallelAnimation {
            running: onlinePing.running
            loops: Animation.Infinite
            NumberAnimation { target: onlinePing; property: "scale"; from: 1; to: 2.4; duration: Theme.durationSlow * 5; easing.type: Easing.OutCubic } // motion-ok: bucle del punto, gateado por Theme.motion y visible
            NumberAnimation { target: onlinePing; property: "opacity"; from: 0.7; to: 0; duration: Theme.durationSlow * 5; easing.type: Easing.OutCubic } // motion-ok: bucle del punto, gateado por Theme.motion y visible
        }
    }
}
