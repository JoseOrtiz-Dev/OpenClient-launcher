import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs as NativeDialogs
import QtQuick.Layouts
import OpenClient

ColumnLayout {
    id: root
    objectName: "instanceAdvanced"
    property var instance: null
    readonly property bool canSave: !!instance && !launcher.gameBusy && !launcher.catalogBusy
    readonly property string javaPath: instance ? instance.javaPath || "" : ""
    readonly property bool ownWindow: !!instance && (instance.windowWidth || 0) > 0
    spacing: 12

    function save(fields, message, target) {
        target.isError = !launcher.setInstanceSettings(instance.id, fields)
        target.text = target.isError ? launcher.libraryError : message
    }

    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
    Label { text: qsTr("Java de esta instancia"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
    RowLayout {
        Layout.fillWidth: true
        GlassField {
            Layout.fillWidth: true
            readOnly: true
            selectByMouse: true
            text: root.javaPath || qsTr("Automático (usa la configuración global)")
            Accessible.name: "Java de la instancia"
        }
        ActionButton { text: qsTr("Elegir Java"); enabled: root.canSave; onClicked: javaPicker.open() }
        ActionButton { text: qsTr("Automático"); enabled: root.canSave && root.javaPath.length > 0; onClicked: root.save({ javaPath: "" }, qsTr("Java automático."), javaFeedback) }
    }
    Label { text: qsTr("La versión debe coincidir con la que pide esta versión de Minecraft; si no, el juego no se inicia y lo avisa."); color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
    Label { id: javaFeedback; property bool isError: false; visible: text.length > 0; textFormat: Text.PlainText; color: isError ? Theme.danger : Theme.success; wrapMode: Text.WordWrap; Layout.fillWidth: true }
    NativeDialogs.FileDialog {
        id: javaPicker
        title: qsTr("Selecciona el ejecutable de Java de esta instancia")
        nameFilters: Qt.platform.os === "windows" ? ["Java (java.exe javaw.exe)"] : ["Java (java)"]
        onAccepted: root.save({ javaPath: selectedFile }, qsTr("Java guardado."), javaFeedback)
    }

    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
    Label { text: qsTr("Argumentos JVM de esta instancia"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
    GlassField {
        id: argsField
        Layout.fillWidth: true
        maximumLength: 512
        text: root.instance ? root.instance.jvmArgs || "" : ""
        placeholderText: qsTr("Ej. -Dfml.readTimeout=120")
        Accessible.name: "Argumentos JVM de la instancia"
    }
    GlassSwitch {
        id: globalArgs
        text: qsTr("Usar también los argumentos JVM globales (van primero)")
        checked: !root.instance || root.instance.useGlobalJvmArgs !== false
    }
    ActionButton {
        text: qsTr("Guardar argumentos"); primary: true; enabled: root.canSave
        onClicked: root.save({ jvmArgs: argsField.text, useGlobalJvmArgs: globalArgs.checked }, qsTr("Argumentos guardados."), argsFeedback)
    }
    Label { id: argsFeedback; property bool isError: false; visible: text.length > 0; textFormat: Text.PlainText; color: isError ? Theme.danger : Theme.success; wrapMode: Text.WordWrap; Layout.fillWidth: true }

    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
    Label { text: qsTr("Ventana de Minecraft"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
    GlassSwitch { id: ownWindowSwitch; text: qsTr("Usar un tamaño propio en esta instancia"); checked: root.ownWindow }
    RowLayout {
        visible: ownWindowSwitch.checked
        spacing: 10
        Label { text: qsTr("Ancho"); color: Theme.dim }
        SpinBox { id: widthBox; from: 320; to: 7680; stepSize: 16; editable: true; value: root.ownWindow ? root.instance.windowWidth : launcher.windowWidth; Accessible.name: "Ancho de la ventana de la instancia" }
        Label { text: qsTr("Alto"); color: Theme.dim }
        SpinBox { id: heightBox; from: 240; to: 4320; stepSize: 16; editable: true; value: root.ownWindow ? root.instance.windowHeight : launcher.windowHeight; Accessible.name: "Alto de la ventana de la instancia" }
    }
    ActionButton {
        text: qsTr("Guardar ventana"); primary: true; enabled: root.canSave
        onClicked: root.save(ownWindowSwitch.checked ? { windowWidth: widthBox.value, windowHeight: heightBox.value } : { windowWidth: 0, windowHeight: 0 }, qsTr("Ventana guardada."), windowFeedback)
    }
    Label { id: windowFeedback; property bool isError: false; visible: text.length > 0; textFormat: Text.PlainText; color: isError ? Theme.danger : Theme.success; wrapMode: Text.WordWrap; Layout.fillWidth: true }

    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
    Label { text: qsTr("Entrar a un servidor al jugar"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
    Label { text: qsTr("Minecraft se conecta directamente al abrirse. Déjalo vacío para llegar al menú principal."); color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
    RowLayout {
        Layout.fillWidth: true
        GlassField {
            id: serverField
            Layout.fillWidth: true
            maximumLength: 260
            text: root.instance ? root.instance.joinServer || "" : ""
            placeholderText: qsTr("play.ejemplo.net o 192.168.1.20:25565")
            inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhUrlCharactersOnly
            Accessible.name: "Servidor al que entrar al jugar"
            onAccepted: if (root.canSave) root.save({ joinServer: text }, text.trim().length ? qsTr("Servidor guardado.") : qsTr("Sin servidor."), serverFeedback)
        }
        ActionButton {
            text: qsTr("Guardar servidor"); primary: true; enabled: root.canSave
            onClicked: root.save({ joinServer: serverField.text }, serverField.text.trim().length ? qsTr("Servidor guardado.") : qsTr("Sin servidor."), serverFeedback)
        }
    }
    Label { id: serverFeedback; property bool isError: false; visible: text.length > 0; textFormat: Text.PlainText; color: isError ? Theme.danger : Theme.success; wrapMode: Text.WordWrap; Layout.fillWidth: true }

    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
    Label { text: qsTr("Notas"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
    ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: 110
        TextArea {
            id: notesField
            color: Theme.crystal ? Theme.ink : palette.text
            placeholderTextColor: Theme.crystal ? Theme.dim : palette.placeholderText
            selectionColor: Theme.crystal ? Theme.accent : palette.highlight
            selectedTextColor: Theme.crystal ? Theme.accentText : palette.highlightedText
            // Con los temas antiguos replica el fondo por defecto de Basic.
            background: Rectangle {
                radius: Theme.crystal ? Theme.radiusMd : 0
                color: Theme.crystal ? Theme.fieldFill : notesField.palette.base
                border.width: !Theme.crystal && notesField.activeFocus ? 2 : 1
                border.color: Theme.crystal ? (notesField.activeFocus ? Theme.fieldBorderFocus : Theme.fieldBorder)
                    : (notesField.activeFocus ? notesField.palette.highlight : notesField.palette.mid)
            }
            wrapMode: TextEdit.Wrap
            text: root.instance ? root.instance.notes || "" : ""
            placeholderText: qsTr("Apuntes sobre esta instancia (hasta 2000 caracteres)")
            onTextChanged: if (length > 2000) remove(2000, length)
            Accessible.name: "Notas de la instancia"
        }
    }
    ActionButton {
        text: qsTr("Guardar notas"); primary: true; enabled: root.canSave
        onClicked: root.save({ notes: notesField.text }, qsTr("Notas guardadas."), notesFeedback)
    }
    Label { id: notesFeedback; property bool isError: false; visible: text.length > 0; textFormat: Text.PlainText; color: isError ? Theme.danger : Theme.success; wrapMode: Text.WordWrap; Layout.fillWidth: true }

    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
    Label { text: qsTr("Comandos de esta instancia"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
    Label {
        text: qsTr("Si dejas un campo vacío se usa el comando global. Los comandos no viajan al exportar ni al importar una instancia.")
        color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true
    }
    CommandsEditor {
        id: commands
        Layout.fillWidth: true
        allowSave: root.canSave
        pre: root.instance ? root.instance.preLaunch || "" : ""
        post: root.instance ? root.instance.postExit || "" : ""
        wrapper: root.instance ? root.instance.wrapper || "" : ""
        onAccepted: (pre, post, wrapper) => {
            const ok = launcher.setInstanceSettings(root.instance.id, { preLaunch: pre, postExit: post, wrapper: wrapper })
            commands.feedbackError = !ok
            commands.feedback = ok ? qsTr("Comandos guardados.") : launcher.libraryError
        }
    }
}
