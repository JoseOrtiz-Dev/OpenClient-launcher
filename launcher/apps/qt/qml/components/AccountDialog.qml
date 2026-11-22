import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenClient

// Cuentas: identidad del perfil activo a la izquierda y, a la derecha, dos pestañas (Premium / Microsoft y
// No premium / Perfil local) con su alta y la lista de cuentas guardadas de cada tipo.
GlassDialog {
    id: account
    title: qsTr("Tu cuenta")
    glyph: "user"
    padding: 1
    property string offlineFeedback: ""
    property string forgetId: ""
    property string forgetName: ""
    // 0 = Premium / Microsoft, 1 = No premium / Perfil local.
    property int tab: 0
    readonly property int corner: Theme.radiusXl + 6
    readonly property bool wide: width >= 760
    readonly property bool validOfflineName: /^[A-Za-z0-9_]{3,16}$/.test(offlineInput.text.trim())
    readonly property bool canChange: !launcher.gameBusy && !launcher.microsoftBusy && !launcher.accountDiagnosing
    readonly property bool needsLogin: launcher.usingMicrosoft && launcher.microsoftNeedsLogin
    readonly property color statusTone: needsLogin ? Theme.warning : launcher.accountName ? Theme.success : Theme.dim
    readonly property string statusText: needsLogin ? qsTr("Vuelve a iniciar sesión")
        : launcher.usingMicrosoft ? qsTr("Sesión Microsoft activa")
        : launcher.accountName ? qsTr("Perfil offline") : qsTr("Sin perfil")
    readonly property bool flowVisible: launcher.microsoftBusy || launcher.microsoftMessage.length > 0
    readonly property string footNote: launcher.gameBusy ? qsTr("Puedes cambiar de perfil cuando termine la partida.")
        : launcher.microsoftBusy ? qsTr("Puedes cerrar este panel: el acceso sigue esperando.") : qsTr("El perfil activo se usa al pulsar Jugar.")
    // Con Cristal la cabecera y la columna lateral son transparentes: se ve el material del diálogo.
    readonly property color headerTone: Theme.crystal ? "transparent" : Qt.tint(Theme.surface, Qt.alpha(Theme.accent, Theme.light ? 0.07 : 0.11))
    readonly property color sideTone: Theme.crystal ? "transparent" : Qt.tint(Theme.surface, Qt.alpha(Theme.accent, Theme.light ? 0.04 : 0.05))

    // Todas las cuentas en una lista: las de Microsoft (con su estado) y los perfiles offline.
    readonly property var entries: {
        const list = []
        for (const item of launcher.microsoftAccounts) {
            list.push({
                kind: "microsoft", key: item.id, name: item.username || qsTr("Cuenta dañada"), status: item.status,
                active: launcher.usingMicrosoft && item.id === launcher.microsoftAccountId
            })
        }
        for (const name of launcher.offlineProfiles) {
            list.push({ kind: "offline", key: name, name: name, status: "ok", active: !launcher.usingMicrosoft && name === launcher.offlineName })
        }
        return list
    }

    onOpened: {
        offlineInput.text = ""
        offlineFeedback = ""
        tab =launcher.accountName.length > 0 && !launcher.usingMicrosoft ? 1 : 0
        scroll.contentItem.contentY = 0
    }
    onFlowVisibleChanged: if (flowVisible) setTab(0)
    function setTab(index) {
        if (tab === index) return
        tab = index
        scroll.contentItem.contentY = 0
    }
    function saveOffline() {
        if (canChange && validOfflineName && launcher.setOfflineAccount(offlineInput.text.trim())) {
            offlineFeedback = qsTr("Perfil guardado. Ya puedes usarlo para jugar.")
            offlineInput.text = ""
        }
    }
    function askForget(id, name) {
        forgetId = id
        forgetName = name
        disconnectDialog.open()
    }

    // Línea de progreso indeterminada: un segmento que recorre la pista solo con transform.
    component WaitLine: Item {
        id: waitLine
        implicitHeight: 3
        clip: true
        Rectangle { anchors.fill: parent; radius: 2; color: Theme.rim }
        Rectangle {
            id: runner
            width: waitLine.width * 0.32
            height: parent.height
            radius: 2
            color: Theme.accent
            x: Theme.motion ? 0 : waitLine.width * 0.34
            NumberAnimation on x {
                running: Theme.motion && waitLine.visible
                from: -runner.width; to: waitLine.width
                duration: 1300; loops: Animation.Infinite; easing.type: Easing.InOutCubic // motion-ok: bucle infinito gateado
            }
        }
    }

    // Indicador de espera discreto: un punto orbita sobre un aro fijo (solo rotación).
    component Spinner: Item {
        id: spinner
        implicitWidth: 24; implicitHeight: 24
        Accessible.ignored: true
        Rectangle { anchors.fill: parent; radius: width / 2; color: "transparent"; border.width: 2; border.color: Theme.rim }
        Item {
            anchors.fill: parent
            rotation: 0
            RotationAnimation on rotation {
                running: Theme.motion && spinner.visible
                from: 0; to: 360; duration: 1400; loops: Animation.Infinite // motion-ok: bucle infinito gateado
            }
            Rectangle { x: parent.width / 2 - 2; y: 0; width: 4; height: 4; radius: 2; color: Theme.accent }
        }
    }

    // Botón secundario de la cuenta activa: icono en cuadro tintado; `danger` lo pinta en rojo.
    component AccountAction: Button {
        id: act
        property string glyphName: ""
        property bool danger: false
        readonly property bool hot: enabled && (hovered || down)
        readonly property color tone: danger ? Theme.danger : Theme.accent
        implicitHeight: 40
        implicitWidth: row.implicitWidth + 24
        hoverEnabled: true
        leftPadding: 10; rightPadding: 14
        scale: down && enabled ? 0.985 : 1
        Behavior on scale { MotionNumber { role: "press" } }
        Accessible.name: text
        background: Item {
            Rectangle {
                anchors.fill: parent; anchors.margins: -3
                radius: Theme.radiusMd + 3
                color: "transparent"
                border.width: 2
                border.color: Theme.highlight
                visible: act.visualFocus
            }
            Rectangle {
                anchors.fill: parent
                radius: Theme.radiusMd
                color: act.danger ? (act.down ? Qt.darker(Theme.danger, 1.15) : act.hot ? Theme.danger : Qt.alpha(Theme.danger, 0.08))
                    : act.down ? Theme.edge : act.hovered ? (Theme.crystal ? Theme.rowFillHover : Qt.alpha(Theme.ink, Theme.light ? 0.06 : 0.08))
                    : (Theme.crystal ? Theme.rowFill : Theme.surface)
                border.width: 1
                border.color: act.danger ? Theme.danger : act.hovered || act.visualFocus ? (Theme.crystal ? Theme.glassBorderHover : Theme.edge)
                    : (Theme.crystal ? Theme.glassBorder : Theme.border)
                opacity: act.enabled ? 1 : 0.45
                Behavior on color { MotionColor { role: "fast" } }
            }
        }
        contentItem: RowLayout {
            id: row
            spacing: 10
            opacity: act.enabled ? 1 : 0.5
            Rectangle {
                Layout.preferredWidth: 26; Layout.preferredHeight: 26
                radius: Theme.radiusSm
                color: act.danger && act.hot ? Qt.alpha("white", 0.2) : Qt.alpha(act.tone, act.hot ? 0.22 : 0.14)
                Behavior on color { MotionColor { role: "fast" } }
                Glyph {
                    anchors.centerIn: parent; width: 16; height: 16
                    name: act.glyphName
                    color: act.danger ? (act.hot ? "white" : Theme.danger) : Theme.highlight
                }
            }
            Label {
                text: act.text; textFormat: Text.PlainText
                color: act.danger ? (act.hot ? "white" : Theme.danger) : Theme.ink
                font.pixelSize: 13; font.weight: Font.DemiBold
            }
        }
    }

    component Heading: Label {
        color: Theme.ink
        font.pixelSize: 15
        font.weight: Font.DemiBold
    }

    // Etiqueta pequeña en mayúsculas espaciadas.
    component Overline: Label {
        color: Theme.dim
        font.pixelSize: 11
        font.weight: Font.DemiBold
        font.letterSpacing: 1.6
        font.capitalization: Font.AllUppercase
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    // Logotipo de Microsoft: cuatro cuadrados con los colores de la marca.
    component MicrosoftMark: Grid {
        columns: 2
        spacing: 2
        Rectangle { width: 8; height: 8; color: "#f25022" }
        Rectangle { width: 8; height: 8; color: "#7fba00" }
        Rectangle { width: 8; height: 8; color: "#00a4ef" }
        Rectangle { width: 8; height: 8; color: "#ffb900" }
    }

    // Icono en cuadro redondeado, a la cabecera de cada pestaña.
    component IconTile: Rectangle {
        property string glyphName: "user"
        implicitWidth: 60; implicitHeight: 60
        radius: Theme.radiusLg
        color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
        border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.35) : Theme.edge
        Glyph { anchors.centerIn: parent; width: 28; height: 28; name: parent.glyphName; color: Theme.highlight }
    }

    // Selector segmentado: la pestaña activa lleva fondo y borde de acento.
    component PaneTab: Button {
        id: tabButton
        property string glyphName: "user"
        property string sub: ""
        property bool current: false
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: 1
        hoverEnabled: true
        Accessible.name: current ? qsTr("%1, %2, seleccionada").arg(text).arg(sub) : qsTr("%1, %2").arg(text).arg(sub)
        background: Item {
            Rectangle {
                anchors.fill: parent
                radius: Theme.radiusMd
                color: tabButton.down ? Theme.edge : Theme.crystal ? Theme.rowFillHover : Qt.alpha(Theme.ink, Theme.light ? 0.05 : 0.06)
                opacity: !tabButton.current && (tabButton.hovered || tabButton.down) ? 1 : 0
                Behavior on opacity { MotionNumber { role: "fast" } }
            }
            Rectangle {
                anchors.fill: parent
                radius: Theme.radiusMd
                color: Theme.crystal ? Qt.alpha(Theme.accent, 0.16) : Theme.soft
                border.color: Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.5) : Qt.alpha(Theme.accent, 0.6)
                opacity: tabButton.current ? 1 : 0
                Behavior on opacity { MotionNumber { role: "base" } }
            }
            Rectangle {
                anchors.fill: parent; anchors.margins: -3
                radius: Theme.radiusMd + 3
                color: "transparent"
                border.width: 2
                border.color: Theme.highlight
                visible: tabButton.visualFocus
            }
        }
        contentItem: Item {
            RowLayout {
                anchors.centerIn: parent
                width: Math.min(parent.width - 16, implicitWidth)
                spacing: 12
                Glyph {
                    Layout.preferredWidth: 20; Layout.preferredHeight: 20
                    name: tabButton.glyphName
                    color: tabButton.current ? Theme.highlight : Theme.dim
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 1
                    Label {
                        Layout.fillWidth: true; elide: Text.ElideRight; text: tabButton.text; textFormat: Text.PlainText
                        color: tabButton.current ? Theme.highlight : Theme.ink
                        font.pixelSize: 14; font.weight: Font.DemiBold
                    }
                    Label {
                        Layout.fillWidth: true; elide: Text.ElideRight; text: tabButton.sub; textFormat: Text.PlainText
                        color: Theme.dim; font.pixelSize: 11
                    }
                }
            }
        }
    }

    // Botón principal grande de cada pestaña: icono a la izquierda, texto centrado y flecha a la derecha.
    component BigButton: Button {
        id: big
        property string glyphName: ""
        property bool microsoft: false
        property string trailing: "arrowright"
        Layout.fillWidth: true
        implicitHeight: 54
        hoverEnabled: true
        scale: big.down && big.enabled ? 0.99 : 1
        Behavior on scale { MotionNumber { role: "press" } }
        Accessible.name: text
        background: Item {
            Rectangle {
                anchors.fill: parent; anchors.margins: -3
                radius: Theme.radiusMd + 3
                color: "transparent"
                border.width: 2
                border.color: Theme.highlight
                visible: big.visualFocus
            }
            Rectangle {
                anchors.fill: parent
                radius: Theme.radiusMd
                color: big.down ? Theme.accentPressed : big.hovered ? Qt.lighter(Theme.accent, 1.07) : Theme.accent
                border.color: Theme.crystal ? Theme.glassHighlight : Qt.alpha("white", Theme.light ? 0.22 : 0.14)
                opacity: big.enabled ? 1 : 0.45
                Behavior on color { MotionColor { role: "fast" } }
            }
        }
        contentItem: RowLayout {
            spacing: 12
            opacity: big.enabled ? 1 : 0.5
            Item {
                Layout.leftMargin: 8
                Layout.preferredWidth: 20; Layout.preferredHeight: 20
                MicrosoftMark { anchors.centerIn: parent; visible: big.microsoft }
                Glyph { anchors.centerIn: parent; width: 20; height: 20; visible: !big.microsoft; name: big.glyphName; color: Theme.accentText }
            }
            Label {
                Layout.fillWidth: true
                text: big.text; textFormat: Text.PlainText; elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
                color: Theme.accentText; font.pixelSize: 14; font.weight: Font.DemiBold
            }
            Glyph {
                Layout.rightMargin: 8
                Layout.preferredWidth: 18; Layout.preferredHeight: 18
                name: big.trailing; color: Theme.accentText
            }
        }
    }

    // Cuentas guardadas de un tipo: pulsar una la activa; las de Microsoft con problemas abren su comprobación.
    component AccountList: ColumnLayout {
        id: list
        property var dlg: null
        property string kind: "microsoft"
        property string heading: ""
        property string emptyText: ""
        readonly property var items: dlg ? dlg.entries.filter(e => e.kind === list.kind) : []
        spacing: 8

        Heading { text: list.heading; font.pixelSize: 13 }
        Label {
            visible: list.items.length === 0
            Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim; font.pixelSize: 12
            text: list.emptyText
        }
        Repeater {
            model: list.items
            Button {
                id: card
                objectName: "accountCard"
                required property var modelData
                readonly property bool active: modelData.active
                readonly property bool healthy: modelData.status === "ok"
                readonly property color tone: !healthy ? Theme.warning : active ? Theme.accent : Theme.dim
                Layout.fillWidth: true
                implicitHeight: 62
                hoverEnabled: true
                enabled: list.dlg.canChange || active
                leftPadding: 12; rightPadding: 10
                Accessible.name: active ? qsTr("%1, %2, activa").arg(modelData.name).arg(modelData.kind === "microsoft" ? qsTr("Microsoft") : qsTr("offline")) : qsTr("%1, %2").arg(modelData.name).arg(modelData.kind === "microsoft" ? qsTr("Microsoft") : qsTr("offline"))
                function activate() {
                    if (active) return
                    if (modelData.kind === "offline") launcher.selectOfflineProfile(modelData.key)
                    else if (healthy) launcher.selectMicrosoftAccount(modelData.key)
                    else launcher.diagnoseMicrosoftAccount(modelData.key)
                }
                onClicked: activate()
                background: Rectangle {
                    radius: Theme.radiusMd
                    color: card.active ? (Theme.crystal ? Qt.alpha(Theme.accent, 0.14) : Theme.soft)
                        : card.down ? Theme.edge
                        : card.hovered ? (Theme.crystal ? Theme.rowFillHover : Qt.alpha(Theme.ink, Theme.light ? 0.04 : 0.05))
                        : (Theme.crystal ? Theme.rowFill : "transparent")
                    border.width: card.visualFocus ? 2 : 1
                    border.color: card.visualFocus ? Theme.highlight
                        : card.active ? (Theme.crystal ? Qt.alpha(Theme.accentSoft, 0.4) : Theme.edge)
                        : (Theme.crystal ? Theme.glassBorder : Theme.border)
                    Behavior on color { MotionColor { role: "fast" } }
                }
                contentItem: RowLayout {
                    spacing: 10
                    Rectangle {
                        Layout.preferredWidth: 38; Layout.preferredHeight: 38
                        radius: Theme.radiusSm
                        color: Theme.soft
                        border.width: 1.5
                        border.color: Qt.alpha(card.tone, card.active || !card.healthy ? 0.8 : 0.3)
                        clip: true
                        readonly property bool hasFace: card.active && wardrobe.accountFace.length > 0
                        Image {
                            anchors.fill: parent; anchors.margins: 3
                            visible: parent.hasFace
                            source: parent.hasFace ? wardrobe.accountFace : ""
                            smooth: false; fillMode: Image.PreserveAspectFit
                        }
                        Label {
                            anchors.centerIn: parent
                            visible: !parent.hasFace
                            text: card.modelData.name.slice(0, 2).toUpperCase()
                            color: Theme.highlight; font.pixelSize: 13; font.weight: Font.Bold
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 1
                        Label {
                            Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                            text: card.modelData.name
                            color: Theme.ink; font.weight: Font.DemiBold
                        }
                        Label {
                            Layout.fillWidth: true; elide: Text.ElideRight
                            font.pixelSize: 11
                            color: card.healthy ? Theme.dim : Theme.warning
                            text: card.modelData.kind === "offline" ? qsTr("Offline")
                                : card.healthy ? qsTr("Microsoft · sesión lista")
                                : card.modelData.status === "reauth" ? qsTr("Microsoft · hay que volver a entrar") : qsTr("Microsoft · sesión dañada")
                        }
                    }
                    // Insignia de estado.
                    Rectangle {
                        readonly property color badgeTone: card.active ? Theme.accent : !card.healthy ? Theme.warning : card.modelData.kind === "microsoft" ? Theme.success : Theme.dim
                        implicitWidth: badgeLabel.implicitWidth + 14; implicitHeight: 20; radius: 10
                        color: Qt.alpha(badgeTone, card.active ? 0.16 : 0.12)
                        border.color: Qt.alpha(badgeTone, card.active ? 0.4 : 0.35)
                        Label {
                            id: badgeLabel; anchors.centerIn: parent
                            text: card.active ? qsTr("Activa")
                                : !card.healthy ? (card.modelData.status === "reauth" ? qsTr("Reconectar") : qsTr("Dañada"))
                                : card.modelData.kind === "microsoft" ? qsTr("Lista") : qsTr("Local")
                            color: card.active ? Theme.highlight : Theme.ink; font.pixelSize: 10; font.weight: Font.DemiBold
                        }
                    }
                    ActionButton {
                        visible: !card.active
                        compact: true
                        text: card.healthy ? qsTr("Activar") : qsTr("Comprobar")
                        enabled: list.dlg.canChange
                        onClicked: card.activate()
                    }
                    ToolButton {
                        id: removeButton
                        objectName: "accountRemove"
                        enabled: list.dlg.canChange
                        Layout.preferredWidth: 30; Layout.preferredHeight: 30
                        hoverEnabled: true
                        Accessible.name: qsTr("Quitar %1").arg(card.modelData.name)
                        ToolTip.visible: hovered
                        ToolTip.delay: 500
                        ToolTip.text: qsTr("Quitar esta cuenta")
                        onClicked: {
                            if (card.modelData.kind === "offline") launcher.removeOfflineProfile(card.modelData.key)
                            else list.dlg.askForget(card.modelData.key, card.modelData.name)
                        }
                        background: Rectangle {
                            radius: Theme.radiusSm
                            color: removeButton.down ? Theme.edge : removeButton.hovered ? Theme.soft : "transparent"
                            border.width: removeButton.visualFocus ? 2 : 0
                            border.color: Theme.highlight
                            Behavior on color { MotionColor { role: "fast" } }
                        }
                        contentItem: Item { Glyph { anchors.centerIn: parent; width: 14; height: 14; name: "trash"; color: removeButton.hovered ? Theme.danger : Theme.dim } }
                    }
                }
            }
        }
    }

    header: Item { implicitHeight: 0; visible: false }

    background: Item {
        Rectangle {
            anchors.fill: parent
            radius: account.corner
            color: Theme.base
            border.color: Theme.crystal ? "transparent" : Theme.edge
            border.width: 1
        }
        // Cristal: el mismo material fuerte que `GlassDialog`, con las esquinas de este diálogo.
        Loader {
            anchors.fill: parent
            active: Theme.crystal
            sourceComponent: GlassCard {
                strong: true
                hoverable: false
                radius: account.corner
            }
        }
    }

    contentItem: Item {
        id: shell

        // Cabecera: sobre-título, título y cierre.
        Rectangle {
            id: top
            anchors.top: parent.top; anchors.left: parent.left; anchors.right: parent.right
            height: 84
            radius: account.corner - 1
            color: account.headerTone
            Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: account.corner; color: parent.color }
            ColumnLayout {
                anchors.left: parent.left; anchors.right: closeButton.left
                anchors.leftMargin: account.wide ? 32 : 24; anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                Overline { Layout.fillWidth: true; text: qsTr("Tu identidad en OpenClient") }
                Label {
                    Layout.fillWidth: true
                    text: qsTr("Tu próxima aventura empieza contigo.")
                    textFormat: Text.PlainText; elide: Text.ElideRight
                    color: Theme.ink; font.pixelSize: 24; font.weight: Font.DemiBold
                }
            }
            Button {
                id: closeButton
                objectName: "accountClose"
                anchors.right: parent.right; anchors.rightMargin: 24
                anchors.verticalCenter: parent.verticalCenter
                width: 40; height: 40
                hoverEnabled: true
                Accessible.name: qsTr("Cerrar")
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: qsTr("Cerrar")
                onClicked: account.close()
                background: Item {
                    Rectangle {
                        anchors.fill: parent
                        radius: width / 2
                        color: closeButton.down ? Theme.edge : closeButton.hovered ? (Theme.crystal ? Theme.rowFillHover : Theme.soft) : (Theme.crystal ? Theme.rowFill : Theme.surface)
                        border.color: closeButton.hovered ? (Theme.crystal ? Theme.glassBorderHover : Theme.edge) : (Theme.crystal ? Theme.glassBorder : Theme.border)
                        Behavior on color { MotionColor { role: "fast" } }
                    }
                    Rectangle {
                        anchors.fill: parent; anchors.margins: -3
                        radius: width / 2
                        color: "transparent"
                        border.width: 2
                        border.color: Theme.highlight
                        visible: closeButton.visualFocus
                    }
                }
                contentItem: Item { Glyph { anchors.centerIn: parent; width: 16; height: 16; name: "close"; color: closeButton.hovered ? Theme.ink : Theme.dim } }
            }
        }
        Rectangle { id: topLine; anchors.top: top.bottom; anchors.left: parent.left; anchors.right: parent.right; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }

        Item {
            id: body
            anchors.top: topLine.bottom; anchors.bottom: parent.bottom
            anchors.left: parent.left; anchors.right: parent.right

            // Columna izquierda: identidad del perfil activo.
            Rectangle {
                id: side
                visible: account.wide
                anchors.top: parent.top; anchors.bottom: parent.bottom; anchors.left: parent.left
                width: 290
                radius: account.corner - 1
                color: account.sideTone
                // Solo la esquina inferior izquierda queda redondeada.
                Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: account.corner; color: parent.color }
                Rectangle { anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom; width: account.corner; color: parent.color }
                Rectangle {
                    anchors.fill: parent
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.alpha(Theme.accent, Theme.light ? 0.07 : 0.10) }
                        GradientStop { position: 0.7; color: "transparent" }
                    }
                }
                Rectangle { anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 1; color: Theme.crystal ? Theme.divider : Theme.border }

                ColumnLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 28; anchors.rightMargin: 29; anchors.topMargin: 24; anchors.bottomMargin: 24
                    spacing: 0

                    // Avatar con marco ligeramente girado.
                    Item {
                        id: avatar
                        Layout.preferredWidth: 116; Layout.preferredHeight: 116
                        readonly property bool hasFace: launcher.accountName.length > 0 && wardrobe.accountFace.length > 0
                        Rectangle {
                            anchors.centerIn: parent
                            width: 104; height: 104
                            radius: 24
                            rotation: -5
                            color: Theme.soft
                            border.width: 2
                            border.color: Qt.alpha(account.statusTone, launcher.accountName ? 0.8 : 0.35)
                            Behavior on border.color { MotionColor { role: "slow" } }
                        }
                        Rectangle {
                            anchors.centerIn: parent
                            width: 80; height: 80
                            radius: 16
                            color: Theme.crystal ? Theme.fieldFill : Theme.surface
                            clip: true
                            Image {
                                anchors.fill: parent; anchors.margins: 4
                                visible: avatar.hasFace
                                source: avatar.hasFace ? wardrobe.accountFace : ""
                                smooth: false
                                fillMode: Image.PreserveAspectFit
                            }
                            Glyph { anchors.centerIn: parent; width: 30; height: 30; visible: !launcher.accountName; name: "user"; color: Theme.highlight }
                            Label {
                                anchors.centerIn: parent
                                visible: launcher.accountName.length > 0 && !avatar.hasFace
                                text: launcher.accountName.slice(0, 2).toUpperCase()
                                color: Theme.highlight; font.pixelSize: 24; font.weight: Font.Bold
                            }
                        }
                        // Cambio de perfil: un pulso corto confirma que el avatar es otro.
                        SequentialAnimation {
                            id: avatarPop
                            MotionNumber { role: "press"; target: avatar; property: "scale"; to: 0.92 }
                            NumberAnimation { target: avatar; property: "scale"; to: 1; duration: Theme.durationBase; easing.type: Easing.OutBack; easing.overshoot: 2.2 }
                        }
                        Connections {
                            target: launcher
                            function onAccountChanged() { if (account.opened && Theme.motion) avatarPop.restart() }
                        }
                    }

                    Overline { Layout.topMargin: 22; Layout.fillWidth: true; text: qsTr("Perfil actual") }
                    Label {
                        Layout.topMargin: 6; Layout.fillWidth: true
                        text: launcher.accountName || qsTr("Elige un perfil")
                        textFormat: Text.PlainText; elide: Text.ElideRight
                        color: Theme.ink; font.pixelSize: 26; font.weight: Font.DemiBold
                    }
                    RowLayout {
                        Layout.topMargin: 10; Layout.fillWidth: true
                        spacing: 8
                        Rectangle {
                            Layout.preferredWidth: 7; Layout.preferredHeight: 7; radius: 4
                            color: account.statusTone
                            Behavior on color { MotionColor { role: "base" } }
                        }
                        Label { Layout.fillWidth: true; text: account.statusText; elide: Text.ElideRight; color: Theme.dim; font.pixelSize: 13 }
                    }
                    Rectangle { Layout.topMargin: 22; Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                    RowLayout {
                        visible: launcher.usingMicrosoft
                        Layout.topMargin: 20; Layout.fillWidth: true
                        spacing: 12
                        Glyph { Layout.alignment: Qt.AlignTop; Layout.preferredWidth: 20; Layout.preferredHeight: 20; name: "shield"; color: Theme.highlight }
                        Label {
                            Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim; font.pixelSize: 13
                            text: qsTr("Skin y capas oficiales vinculadas a tu perfil de Minecraft Java.")
                        }
                    }
                    Label {
                        visible: !launcher.accountName
                        Layout.topMargin: 20; Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim; font.pixelSize: 13
                        text: qsTr("Añade una cuenta de Microsoft para jugar en servidores, o crea un perfil offline para un solo jugador.")
                    }
                    Item { Layout.fillHeight: true; Layout.minimumHeight: 12 }
                    Overline { Layout.fillWidth: true; text: qsTr("OpenClient"); color: Theme.accent }
                    Label { Layout.topMargin: 6; Layout.fillWidth: true; text: qsTr("Un perfil. Muchos mundos."); color: Theme.dim; font.pixelSize: 12; elide: Text.ElideRight }
                }
            }

            // Columna derecha: pestañas y su contenido.
            Item {
                id: main
                anchors.top: parent.top; anchors.bottom: parent.bottom; anchors.right: parent.right
                anchors.left: account.wide ? side.right : parent.left
                readonly property int gutter: account.wide ? 32 : 20

                Rectangle {
                    id: tabs
                    anchors.top: parent.top; anchors.topMargin: 24
                    anchors.left: parent.left; anchors.leftMargin: main.gutter
                    anchors.right: parent.right; anchors.rightMargin: main.gutter
                    height: 68
                    radius: Theme.radiusLg
                    color: Theme.crystal ? Theme.fieldFill : Theme.surface
                    border.color: Theme.crystal ? Theme.glassBorder : Theme.border
                    RowLayout {
                        anchors.fill: parent; anchors.margins: 4
                        spacing: 4
                        PaneTab {
                            objectName: "accountTabMicrosoft"
                            text: qsTr("Premium"); sub: qsTr("Microsoft"); glyphName: "shield"
                            current: account.tab === 0
                            onClicked: account.setTab(0)
                            Keys.onRightPressed: { account.setTab(1); nextItemInFocusChain(true).forceActiveFocus() }
                        }
                        PaneTab {
                            objectName: "accountTabOffline"
                            text: qsTr("No premium"); sub: qsTr("Perfil local"); glyphName: "gamepad"
                            current: account.tab === 1
                            onClicked: account.setTab(1)
                            Keys.onLeftPressed: { account.setTab(0); nextItemInFocusChain(false).forceActiveFocus() }
                        }
                    }
                }

                ScrollView {
                    id: scroll
                    anchors.top: tabs.bottom; anchors.topMargin: 8
                    anchors.bottom: parent.bottom
                    anchors.left: parent.left; anchors.right: parent.right
                    contentWidth: availableWidth
                    contentHeight: stack.height
                    clip: true
                    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

                    Item {
                        id: stack
                        width: scroll.availableWidth
                        height: Math.max(0, (account.tab === 0 ? premiumPane.implicitHeight : offlinePane.implicitHeight)) + 40

                        // Pestaña Premium / Microsoft.
                        ColumnLayout {
                            id: premiumPane
                            readonly property bool active: account.tab === 0
                            x: main.gutter; y: 16
                            width: stack.width - main.gutter * 2
                            spacing: 14
                            enabled: active
                            opacity: active ? 1 : 0
                            Behavior on opacity { MotionNumber { role: "base" } }
                            transform: Translate {
                                y: premiumPane.active ? 0 : 10
                                Behavior on y { MotionNumber { role: "slow" } }
                            }

                            IconTile { glyphName: "shield" }
                            Label {
                                Layout.fillWidth: true; Layout.topMargin: 4
                                text: qsTr("Conecta tu cuenta Microsoft"); textFormat: Text.PlainText; wrapMode: Text.WordWrap
                                color: Theme.ink; font.pixelSize: 24; font.weight: Font.DemiBold
                            }
                            Label {
                                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim; font.pixelSize: 14
                                text: qsTr("Accede a tu perfil de Minecraft: Java Edition, tu skin y tus capas oficiales.")
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 10
                                Repeater {
                                    model: [qsTr("Acceso a servidores con autenticación oficial"), qsTr("Renovación automática cuando sea posible"), qsTr("Sesión guardada con protección de Windows")]
                                    RowLayout {
                                        required property string modelData
                                        Layout.fillWidth: true
                                        spacing: 12
                                        Glyph { Layout.preferredWidth: 16; Layout.preferredHeight: 16; name: "check"; color: Theme.accent }
                                        Label { Layout.fillWidth: true; text: parent.modelData; textFormat: Text.PlainText; wrapMode: Text.WordWrap; color: Theme.ink; font.pixelSize: 13 }
                                    }
                                }
                            }

                            BigButton {
                                objectName: "addMicrosoft"
                                Layout.topMargin: 6
                                microsoft: true
                                text: launcher.microsoftAccounts.length > 0 ? qsTr("Usar otra cuenta Microsoft") : qsTr("Añadir cuenta Microsoft")
                                visible: !launcher.microsoftBusy
                                enabled: account.canChange
                                onClicked: launcher.startMicrosoftLogin(true)
                            }

                            // Inicio de sesión de Microsoft: espera mientras el navegador completa el acceso.
                            GlassPanel {
                                objectName: "microsoftWait"
                                Layout.fillWidth: true
                                visible: account.flowVisible
                                implicitHeight: flowColumn.implicitHeight + 28
                                frosted: false
                                padding: 14
                                ColumnLayout {
                                    id: flowColumn
                                    anchors.fill: parent
                                    spacing: 10
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 12
                                        Spinner { visible: launcher.microsoftBusy }
                                        Heading {
                                            Layout.fillWidth: true; elide: Text.ElideRight
                                            text: launcher.microsoftBusy ? qsTr("Esperando a Microsoft…") : qsTr("Microsoft")
                                        }
                                    }
                                    Label {
                                        visible: launcher.microsoftMessage.length > 0
                                        Layout.fillWidth: true
                                        text: launcher.microsoftMessage
                                        textFormat: Text.PlainText; wrapMode: Text.WordWrap
                                        color: launcher.microsoftBusy ? Theme.dim : Theme.highlight
                                    }
                                    WaitLine { visible: launcher.microsoftBusy; Layout.fillWidth: true }
                                    Flow {
                                        Layout.fillWidth: true
                                        spacing: 8
                                        visible: launcher.microsoftBusy
                                        ActionButton {
                                            text: qsTr("Abrir el navegador de nuevo"); glyph: "arrow"; compact: true
                                            onClicked: launcher.openMicrosoftVerification()
                                        }
                                        ActionButton {
                                            text: qsTr("Cancelar"); compact: true
                                            onClicked: launcher.cancelMicrosoftLogin()
                                        }
                                    }
                                }
                            }

                            // Comprobación de una cuenta: qué paso falla y qué hacer.
                            ColumnLayout {
                                objectName: "accountDiagnosis"
                                Layout.fillWidth: true
                                visible: launcher.accountDiagnosing || !!launcher.accountDiagnosis.code
                                spacing: 8
                                Heading { text: qsTr("Comprobación de la cuenta") }
                                RowLayout {
                                    visible: launcher.accountDiagnosing
                                    spacing: 10
                                    BusyIndicator { running: parent.visible && Theme.motion; Layout.preferredWidth: 20; Layout.preferredHeight: 20 }
                                    Label { text: qsTr("Comprobando con Microsoft, Xbox y Minecraft…"); color: Theme.dim }
                                }
                                Repeater {
                                    model: launcher.accountDiagnosing ? [] : (launcher.accountDiagnosis.steps || [])
                                    RowLayout {
                                        required property var modelData
                                        Layout.fillWidth: true
                                        spacing: 8
                                        Rectangle {
                                            Layout.preferredWidth: 9; Layout.preferredHeight: 9; radius: 5
                                            color: modelData.state === "ok" ? Theme.success
                                                : modelData.state === "fail" ? Theme.danger : "transparent"
                                            border.color: modelData.state === "skip" ? Theme.dim : "transparent"
                                        }
                                        Label {
                                            Layout.fillWidth: true; textFormat: Text.PlainText
                                            text: modelData.state === "skip" ? qsTr("%1 (sin probar)").arg(modelData.name) : modelData.name
                                            color: modelData.state === "fail" ? Theme.ink : Theme.dim
                                            font.weight: modelData.state === "fail" ? Font.DemiBold : Font.Normal
                                        }
                                    }
                                }
                                Label {
                                    visible: !launcher.accountDiagnosing && !!launcher.accountDiagnosis.detail
                                    Layout.fillWidth: true; wrapMode: Text.WordWrap; textFormat: Text.PlainText
                                    color: launcher.accountDiagnosis.code === "ok" ? Theme.success : Theme.ink
                                    text: launcher.accountDiagnosis.detail || ""
                                }
                                Flow {
                                    visible: !launcher.accountDiagnosing && !!launcher.accountDiagnosis.code
                                    Layout.fillWidth: true
                                    spacing: 8
                                    ActionButton {
                                        visible: ["reconnect", "retry", "open_xbox", "open_minecraft"].indexOf(launcher.accountDiagnosis.action) >= 0
                                        primary: true; compact: true; enabled: account.canChange
                                        text: ({ reconnect: qsTr("Volver a conectar"), retry: qsTr("Reintentar"),
                                                 open_xbox: qsTr("Abrir la cuenta de Xbox"), open_minecraft: qsTr("Ver Minecraft: Java Edition") })[launcher.accountDiagnosis.action] || ""
                                        onClicked: launcher.runDiagnosisAction(launcher.accountDiagnosis.action)
                                    }
                                    ActionButton { text: qsTr("Cerrar"); compact: true; onClicked: launcher.clearDiagnosis() }
                                }
                            }

                            // Acciones de la cuenta Microsoft activa.
                            Flow {
                                Layout.fillWidth: true
                                spacing: 10
                                visible: !launcher.microsoftBusy && launcher.usingMicrosoft
                                ActionButton {
                                    visible: account.needsLogin
                                    text: qsTr("Volver a conectar"); glyph: "arrow"; primary: true; compact: true; enabled: account.canChange
                                    onClicked: launcher.startMicrosoftLogin(true)
                                }
                                AccountAction {
                                    visible: !account.needsLogin && launcher.microsoftName.length > 0
                                    text: qsTr("Sincronizar skin"); glyphName: "shirt"; enabled: account.canChange
                                    onClicked: launcher.refreshMicrosoftProfile()
                                }
                                AccountAction {
                                    objectName: "accountCheck"
                                    visible: launcher.microsoftAccountId.length > 0
                                    text: qsTr("Comprobar sesión"); glyphName: "refresh"; enabled: account.canChange
                                    onClicked: launcher.diagnoseMicrosoftAccount(launcher.microsoftAccountId)
                                }
                                AccountAction {
                                    danger: true
                                    visible: launcher.microsoftAccountId.length > 0
                                    text: qsTr("Cerrar sesión en OpenClient"); glyphName: "logout"; enabled: account.canChange
                                    onClicked: account.askForget(launcher.microsoftAccountId, launcher.microsoftName)
                                }
                            }
                            AccountList {
                                Layout.fillWidth: true; Layout.topMargin: 6
                                dlg: account; kind: "microsoft"
                                heading: qsTr("Cuentas Microsoft guardadas")
                                emptyText: qsTr("Todavía no hay cuentas Microsoft guardadas.")
                            }

                            Rectangle { Layout.fillWidth: true; Layout.topMargin: 4; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                            Label {
                                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim; font.pixelSize: 11
                                text: qsTr("Necesitas Minecraft Java asociado a tu cuenta. Microsoft puede solicitar acceso de nuevo si revocas la sesión o cambia su seguridad.")
                            }
                            Label { Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim; font.pixelSize: 11; text: account.footNote }
                        }

                        // Pestaña No premium / Perfil local.
                        ColumnLayout {
                            id: offlinePane
                            readonly property bool active: account.tab === 1
                            x: main.gutter; y: 16
                            width: stack.width - main.gutter * 2
                            spacing: 14
                            enabled: active
                            opacity: active ? 1 : 0
                            Behavior on opacity { MotionNumber { role: "base" } }
                            transform: Translate {
                                y: offlinePane.active ? 0 : 10
                                Behavior on y { MotionNumber { role: "slow" } }
                            }

                            IconTile { glyphName: "user" }
                            Label {
                                Layout.fillWidth: true; Layout.topMargin: 4
                                text: qsTr("Elige tu nombre de jugador"); textFormat: Text.PlainText; wrapMode: Text.WordWrap
                                color: Theme.ink; font.pixelSize: 24; font.weight: Font.DemiBold
                            }
                            Label {
                                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim; font.pixelSize: 14
                                text: qsTr("Crea un perfil local para un jugador y servidores que admitan acceso sin autenticación Microsoft.")
                            }

                            ColumnLayout {
                                Layout.fillWidth: true; Layout.topMargin: 6
                                spacing: 8
                                Label { text: qsTr("Nombre de usuario"); color: Theme.ink; font.pixelSize: 13; font.weight: Font.DemiBold }
                                GlassField {
                                    id: offlineInput
                                    objectName: "offlineProfileName"
                                    Layout.fillWidth: true
                                    implicitHeight: 50
                                    radius: Theme.radiusMd
                                    legacyFill: Theme.surface
                                    leadingGlyph: "user"
                                    placeholderText: qsTr("Tu nombre en Minecraft")
                                    maximumLength: 16
                                    leftPadding: 44; rightPadding: 56
                                    Accessible.name: "Nombre del perfil offline"
                                    readonly property bool invalid: text.trim().length > 0 && !account.validOfflineName
                                    font.pixelSize: 14
                                    trailing: Label {
                                        text: qsTr("%1/16").arg(offlineInput.text.length)
                                        color: offlineInput.invalid ? Theme.danger : Theme.dim
                                        font.pixelSize: 11; font.features: Theme.tabular
                                    }
                                    // Estado no válido: borde de peligro encima del borde del campo.
                                    Rectangle {
                                        anchors.fill: parent
                                        radius: offlineInput.radius
                                        color: "transparent"
                                        border.width: !Theme.crystal && offlineInput.activeFocus ? 2 : 1
                                        border.color: Theme.danger
                                        opacity: offlineInput.invalid ? 1 : 0
                                        Behavior on opacity { MotionNumber { role: "fast" } }
                                    }
                                    onAccepted: account.saveOffline()
                                }
                                Label {
                                    Layout.fillWidth: true
                                    text: qsTr("De 3 a 16 letras, números o guion bajo (_).")
                                    color: offlineInput.invalid ? Theme.danger : Theme.dim
                                    font.pixelSize: 11
                                }
                                Label {
                                    Layout.fillWidth: true
                                    visible: launcher.accountError.length > 0
                                    text: launcher.accountError
                                    textFormat: Text.PlainText; wrapMode: Text.WordWrap; color: Theme.danger; font.pixelSize: 12
                                }
                            }

                            BigButton {
                                objectName: "addOffline"
                                glyphName: "gamepad"; trailing: "chevronright"
                                text: qsTr("Usar perfil local")
                                enabled: account.canChange && account.validOfflineName
                                onClicked: account.saveOffline()
                            }
                            Label {
                                visible: account.offlineFeedback.length > 0
                                Layout.fillWidth: true
                                text: account.offlineFeedback
                                textFormat: Text.PlainText; wrapMode: Text.WordWrap; color: Theme.success; font.pixelSize: 12
                            }

                            AccountList {
                                Layout.fillWidth: true; Layout.topMargin: 6
                                dlg: account; kind: "offline"
                                heading: qsTr("Perfiles locales guardados")
                                emptyText: qsTr("Todavía no hay perfiles locales guardados.")
                            }

                            Rectangle { Layout.fillWidth: true; Layout.topMargin: 4; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                            Label {
                                Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim; font.pixelSize: 11
                                text: qsTr("Este perfil no verifica una licencia ni permite entrar a servidores que exigen una cuenta premium. No necesita renovar sesión.")
                            }
                            Label { Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.dim; font.pixelSize: 11; text: account.footNote }
                        }
                    }
                }
            }
        }
    }

    GlassDialog {
        id: disconnectDialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: 415
        title: account.forgetName.length ? qsTr("Quitar %1").arg(account.forgetName) : qsTr("Desconectar Microsoft")
        glyph: "user"
        contentItem: ColumnLayout {
            spacing: 12
            Label { Layout.fillWidth: true; wrapMode: Text.WordWrap; color: Theme.ink; text: qsTr("Se borra la sesión guardada de esta cuenta. Las demás cuentas y tus instancias no se tocan. Para volver a usarla tendrás que iniciar sesión otra vez.") }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                ActionButton { text: qsTr("Cancelar"); onClicked: disconnectDialog.close() }
                ActionButton {
                    text: qsTr("Desconectar"); primary: true; enabled: account.canChange
                    onClicked: {
                        launcher.forgetMicrosoftAccount(account.forgetId)
                        disconnectDialog.close()
                    }
                }
            }
        }
    }
}
