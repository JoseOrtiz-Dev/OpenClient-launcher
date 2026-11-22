import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

GlassDialog {
    id: welcome
    property int step: 0
    signal completed()
    title: step === 0 ? qsTr("Configuración inicial") : qsTr("Perfil local")
    glyph: step === 0 ? "settings" : "user"
    closePolicy: Popup.NoAutoClose

    contentItem: ColumnLayout {
        spacing: 16
        Label {
            Layout.fillWidth: true
            text: qsTr("Paso %1 de 2").arg(welcome.step + 1)
            color: Theme.highlight
            font.pixelSize: 12
            font.weight: Font.Medium
            font.features: Theme.tabular
        }
        StackLayout {
            Layout.fillWidth: true
            currentIndex: welcome.step
            ColumnLayout {
                spacing: 13
                Label {
                    Layout.fillWidth: true
                    text: qsTr("Elige un tema de color y si quieres mostrar efectos ambientales.")
                    color: Theme.ink; wrapMode: Text.WordWrap
                }
                GridLayout {
                    Layout.fillWidth: true
                    columns: 3
                    Repeater {
                        model: Theme.themes
                        ActionButton {
                            required property var modelData
                            Layout.fillWidth: true
                            Layout.preferredWidth: 1
                            compact: true
                            text: modelData.name
                            primary: launcher.themeId === modelData.id
                            onClicked: launcher.themeId = modelData.id
                        }
                    }
                }
                GlassSwitch {
                    text: qsTr("Partículas y animaciones")
                    checked: launcher.particlesEnabled
                    onToggled: launcher.particlesEnabled = checked
                }
            }
            ColumnLayout {
                spacing: 13
                Label {
                    Layout.fillWidth: true
                    text: qsTr("Define un nombre para jugar en modo offline. Puedes cambiarlo o conectar Microsoft después desde Cuenta.")
                    color: Theme.ink; wrapMode: Text.WordWrap
                }
                GlassField {
                    id: nameField
                    Layout.fillWidth: true
                    text: launcher.accountName
                    placeholderText: qsTr("Nombre de jugador")
                    maximumLength: 16
                    Accessible.name: "Nombre de jugador offline"
                    onAccepted: saveProfile.clicked()
                }
                Label {
                    Layout.fillWidth: true
                    visible: text.length > 0
                    text: launcher.accountError
                    textFormat: Text.PlainText
                    color: Theme.danger; wrapMode: Text.WordWrap
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            ActionButton {
                text: welcome.step === 0 ? qsTr("Saltar") : qsTr("Atrás")
                onClicked: {
                    if (welcome.step === 0) welcome.completed()
                    else welcome.step = 0
                }
            }
            Item { Layout.fillWidth: true }
            ActionButton {
                visible: welcome.step === 1
                text: qsTr("Más tarde")
                onClicked: welcome.completed()
            }
            ActionButton {
                id: saveProfile
                text: welcome.step === 0 ? qsTr("Continuar") : qsTr("Guardar perfil")
                primary: true
                enabled: welcome.step === 0 || nameField.text.trim().length > 0
                onClicked: {
                    if (welcome.step === 0) {
                        welcome.step = 1
                        nameField.forceActiveFocus()
                    } else if (launcher.setOfflineAccount(nameField.text.trim())) {
                        welcome.completed()
                    }
                }
            }
        }
    }
}
