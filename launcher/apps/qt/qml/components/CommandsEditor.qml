import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

ColumnLayout {
    id: root
    property string pre: ""
    property string post: ""
    property string wrapper: ""
    property bool allowSave: true
    property string feedback: ""
    property bool feedbackError: false
    signal accepted(string pre, string post, string wrapper)
    spacing: 10

    readonly property string draftPre: preField.text.trim()
    readonly property string draftPost: postField.text.trim()
    readonly property string draftWrapper: wrapperField.text.trim()
    readonly property bool dirty: draftPre !== pre.trim() || draftPost !== post.trim() || draftWrapper !== wrapper.trim()
    onPreChanged: preField.text = pre
    onPostChanged: postField.text = post
    onWrapperChanged: wrapperField.text = wrapper

    Label {
        text: qsTr("Se ejecutan sin shell: el texto se divide en programa y argumentos, y las comillas dobles agrupan palabras. Reciben OPENCLIENT_INSTANCE_ID, OPENCLIENT_MC_VERSION, OPENCLIENT_LOADER y OPENCLIENT_GAME_DIR.")
        color: Theme.dim
        font.pixelSize: 11
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
    }
    Label { text: qsTr("Comando previo (se espera a que termine; si falla, no se abre Minecraft)"); color: Theme.ink; font.pixelSize: 13 }
    GlassField {
        id: preField
        Layout.fillWidth: true
        text: root.pre
        maximumLength: 512
        placeholderText: qsTr("Ej. %1 --rapido").arg("\"C:\\Scripts\\copia.exe\"")
        Accessible.name: "Comando previo"
    }
    Label { text: qsTr("Comando posterior (se ejecuta al cerrar Minecraft; recibe OPENCLIENT_EXIT_CODE)"); color: Theme.ink; font.pixelSize: 13 }
    GlassField {
        id: postField
        Layout.fillWidth: true
        text: root.post
        maximumLength: 512
        Accessible.name: "Comando posterior"
    }
    Label { text: qsTr("Comando envoltorio (se antepone al ejecutable de Java)"); color: Theme.ink; font.pixelSize: 13 }
    GlassField {
        id: wrapperField
        Layout.fillWidth: true
        text: root.wrapper
        maximumLength: 512
        Accessible.name: "Comando envoltorio"
    }
    RowLayout {
        Layout.fillWidth: true
        ActionButton {
            text: qsTr("Guardar comandos"); primary: true
            enabled: root.allowSave && root.dirty
            onClicked: {
                if (root.draftPre.length + root.draftPost.length + root.draftWrapper.length === 0)
                    root.accepted("", "", "")
                else
                    confirm.open()
            }
        }
        ActionButton {
            text: qsTr("Descartar cambios")
            enabled: root.dirty
            onClicked: {
                preField.text = root.pre
                postField.text = root.post
                wrapperField.text = root.wrapper
            }
        }
    }
    Label {
        visible: root.feedback.length > 0
        text: root.feedback
        textFormat: Text.PlainText
        color: root.feedbackError ? Theme.danger : Theme.success
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
    }

    GlassDialog {
        id: confirm
        objectName: "commandsConfirm"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(560, parent ? parent.width - 48 : 560)
        glyph: "code"
        title: qsTr("¿Permitir estos comandos?")
        contentItem: ColumnLayout {
            spacing: 12
            Label {
                text: qsTr("Se ejecutarán con tus permisos de usuario cada vez que juegues. Guarda solo comandos que conozcas.")
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
            Repeater {
                model: [
                    { label: qsTr("Previo"), value: root.draftPre },
                    { label: qsTr("Posterior"), value: root.draftPost },
                    { label: qsTr("Envoltorio"), value: root.draftWrapper }
                ]
                ColumnLayout {
                    required property var modelData
                    visible: modelData.value.length > 0
                    Layout.fillWidth: true
                    spacing: 4
                    Label { text: modelData.label; color: Theme.dim; font.pixelSize: 11 }
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: commandText.implicitHeight + 16
                        radius: Theme.radiusSm
                        color: Theme.crystal ? Theme.fieldFill : Theme.base
                        border.color: Theme.crystal ? Theme.fieldBorder : Theme.border
                        Label {
                            id: commandText
                            anchors.fill: parent
                            anchors.margins: 8
                            text: modelData.value
                            textFormat: Text.PlainText
                            wrapMode: Text.WrapAnywhere
                            font.family: "Consolas"
                            color: Theme.ink
                        }
                    }
                }
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                ActionButton { text: qsTr("Cancelar"); onClicked: confirm.close() }
                ActionButton {
                    text: qsTr("Guardar y permitir"); primary: true
                    onClicked: {
                        confirm.close()
                        root.accepted(root.draftPre, root.draftPost, root.draftWrapper)
                    }
                }
            }
        }
    }
}
