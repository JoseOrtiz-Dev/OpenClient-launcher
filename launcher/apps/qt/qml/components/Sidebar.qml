import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtCore
import OpenClient

Item {
    id: rail
    objectName: "navigationSidebar"
    // Barra estrecha (solo iconos): no se despliega al pasar el ratón, el nombre sale en una etiqueta flotante.
    // «Fijar» (Ctrl+B) la deja ancha de forma explícita y empuja el contenido.
    property bool pinned: railSettings.pinned
    readonly property bool expanded: pinned
    // Ancho que reserva Main para la barra (incluye el margen de 10 px a cada lado).
    property real railWidth: (pinned ? Theme.railExpanded : Theme.railCollapsed) + 20
    Behavior on railWidth { MotionNumber { role: "slow" } }
    function togglePin() { railSettings.pinned = !railSettings.pinned }
    Settings { id: railSettings; category: "sidebar"; property bool pinned: false }
    Shortcut { sequences: ["Ctrl+B"]; onActivated: rail.togglePin() }

    // Etiqueta flotante compartida: sigue al botón con el cursor y no parpadea al pasar de uno a otro.
    property Item tipTarget: null
    property string tipText: ""
    property real tipY: 0
    function showTip(item, label) {
        tipHide.stop()
        tipText = label
        tipTarget = item
        tipY = item.mapToItem(panel, 0, item.height / 2).y
    }
    function hideTip(item) { if (tipTarget === item) tipHide.restart() }
    function syncTip(item, active, label) { if (active) showTip(item, label); else hideTip(item) }
    Timer { id: tipHide; interval: 70; onTriggered: rail.tipTarget = null }
    ToolTip {
        id: railTip
        parent: panel
        visible: rail.tipTarget !== null && !rail.expanded
        delay: 260
        timeout: 0
        x: panel.width + 10
        y: rail.tipY - height / 2
        text: rail.tipText
        leftPadding: 12; rightPadding: 12; topPadding: 7; bottomPadding: 7
        contentItem: Text {
            text: railTip.text
            textFormat: Text.PlainText
            color: Theme.ink
            font.family: Theme.fontFamily
            font.pixelSize: 13
            font.weight: Font.Medium
        }
        background: Rectangle {
            radius: Theme.radiusSm
            color: Theme.card
            border.width: 1
            border.color: Theme.edge
        }
        enter: Transition {
            ParallelAnimation {
                MotionNumber { role: "enter"; property: "opacity"; from: 0; to: 1 }
                MotionNumber { role: "enter"; property: "x"; from: panel.width + 4; to: panel.width + 10 }
            }
        }
        exit: Transition {
            MotionNumber { role: "exit"; property: "opacity"; from: 1; to: 0 }
        }
    }
    property string currentPage: "menu"
    property bool settingsOpen: false
    property var destinations: []
    readonly property bool compact: height < 740
    readonly property var descriptions: ({
        menu: qsTr("Inicio rápido"),
        instances: qsTr("Biblioteca local"),
        downloads: qsTr("Catálogo Modrinth"),
        wardrobe: qsTr("Visor 3D y colección"),
        settings: qsTr("Java, memoria y tema")
    })
    // Main mantiene `hold` en true mientras se reproduce la intro; la entrada escalonada espera a que termine.
    property bool hold: false
    property bool revealed: false
    signal navigate(string page)
    signal createRequested()
    signal accountRequested()

    Timer {
        interval: 60
        running: !rail.hold && !rail.revealed
        onTriggered: rail.revealed = true
    }

    Rectangle {
        id: panel
        anchors.left: parent.left; anchors.top: parent.top; anchors.bottom: parent.bottom
        anchors.margins: 10
        width: rail.expanded ? Theme.railExpanded : Theme.railCollapsed
        Behavior on width { MotionNumber { role: "slow" } }
        HoverHandler { id: railHover }
        radius: Theme.radiusXl
        color: Theme.glassElevated
        border.color: Theme.rim
        border.width: 1

        FrostedBackdrop {
            anchors.fill: parent
            anchors.margins: 1
            source: Theme.backdrop
            live: launcher.particlesEnabled
            radius: Math.max(0, panel.radius - 1)
            strength: 0.8
        }

        Rectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: Math.max(0, panel.radius - 1)
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.tint(Theme.sidebar, Qt.alpha(Theme.accent, Theme.light ? 0.04 : 0.07)) }
                GradientStop { position: 0.42; color: Theme.glassElevated }
                GradientStop { position: 1; color: Theme.sidebar }
            }
        }

        // Grano fino del cristal opaco; encogido para no asomar por las esquinas redondeadas.
        Image {
            anchors.fill: parent
            anchors.margins: Math.ceil(panel.radius * 0.293 + 1)
            source: "qrc:/assets/glass/noise.png"
            fillMode: Image.Tile
            opacity: 0.045
            smooth: false
            asynchronous: true
        }

        // Destello superior: más tenue hacia las esquinas.
        Rectangle {
            anchors.top: parent.top
            anchors.topMargin: 1
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.max(0, parent.width - panel.radius * 2)
            height: 1
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Qt.alpha(Theme.glassHighlight, 0) }
                GradientStop { position: 0.2; color: Theme.glassHighlight }
                GradientStop { position: 0.8; color: Theme.glassHighlight }
                GradientStop { position: 1; color: Qt.alpha(Theme.glassHighlight, 0) }
            }
        }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 10
            spacing: 0
            Button {
                id: brand
                Layout.fillWidth: true
                Layout.preferredHeight: rail.compact ? 56 : 74
                hoverEnabled: true
                Accessible.name: "OpenClient, ir al menú"
                onHoveredChanged: rail.syncTip(brand, hovered || visualFocus, qsTr("Inicio"))
                onVisualFocusChanged: rail.syncTip(brand, hovered || visualFocus, qsTr("Inicio"))
                leftPadding: rail.expanded ? 12 : Math.max(0, (Theme.railCollapsed - 20 - (rail.compact ? 36 : 42)) / 2)
                rightPadding: leftPadding
                onClicked: rail.navigate("menu")
                background: Rectangle {
                    radius: Theme.radiusMd
                    color: brand.hovered ? Qt.alpha(Theme.ink, Theme.light ? 0.05 : 0.06) : "transparent"
                    Behavior on color { MotionColor { role: "fast" } }
                    border.color: brand.activeFocus ? Theme.highlight : "transparent"
                }
                contentItem: RowLayout {
                    spacing: 10
                    Rectangle {
                        id: brandTile
                        readonly property color gold: Theme.light ? "#b88723" : "#f1c65c"
                        Layout.preferredWidth: rail.compact ? 36 : 42
                        Layout.preferredHeight: width
                        radius: Theme.radiusMd
                        gradient: Gradient {
                            GradientStop { position: 0; color: Qt.alpha(brandTile.gold, brand.hovered ? 0.26 : 0.20) }
                            GradientStop { position: 1; color: Qt.alpha(Theme.light ? brandTile.gold : Theme.base, Theme.light ? 0.08 : 0.4) }
                        }
                        border.color: Qt.alpha(brandTile.gold, brand.hovered ? 0.45 : 0.30)
                        Behavior on border.color { MotionColor { role: "base" } }
                        // Halo dorado: dos capas con alfa en lugar de un sombreado.
                        Rectangle {
                            z: -1; anchors.fill: parent; anchors.margins: -3; radius: parent.radius + 3
                            color: Qt.alpha(brandTile.gold, brand.hovered ? 0.16 : 0.10)
                            Behavior on color { MotionColor { role: "base" } }
                        }
                        Rectangle {
                            z: -2; anchors.fill: parent; anchors.margins: -7; radius: parent.radius + 7
                            color: Qt.alpha(brandTile.gold, brand.hovered ? 0.08 : 0.05)
                            Behavior on color { MotionColor { role: "base" } }
                        }
                        Rectangle {
                            y: 1; x: parent.radius; height: 1
                            width: Math.max(0, parent.width - parent.radius * 2)
                            color: Theme.glassHighlight
                        }
                        Image {
                            anchors.fill: parent
                            anchors.margins: 5
                            source: "qrc:/assets/cube.svg"
                            sourceSize: Qt.size(84, 84)
                            rotation: brand.hovered && Theme.motion ? -9 : 0
                            scale: brand.down ? 0.92 : brand.hovered && Theme.motion ? 1.06 : 1
                            Behavior on rotation { NumberAnimation { duration: Theme.durationSlow; easing.type: Easing.OutBack; easing.overshoot: 2.4 } }
                            Behavior on scale { MotionNumber { role: "fast" } }
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        opacity: rail.expanded ? 1 : 0
                        visible: opacity > 0.01
                        Behavior on opacity { MotionNumber { role: "base" } }
                        Row {
                            Text { text: qsTr("Open"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: rail.width < 250 ? 17 : 19; font.weight: Font.Bold }
                            Text { text: qsTr("Client"); color: Theme.light ? "#895810" : "#f1c65c"; font.family: Theme.fontFamily; font.pixelSize: rail.width < 250 ? 17 : 19; font.weight: Font.Bold }
                        }
                        Text { text: qsTr("Minecraft Java"); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 11 }
                    }
                }
            }
            Rectangle { Layout.fillWidth: true; Layout.topMargin: 4; Layout.bottomMargin: rail.compact ? 8 : 12; implicitHeight: 1; color: Theme.border }
            Item {
                id: navArea
                Layout.fillWidth: true
                implicitHeight: navColumn.implicitHeight
                property Item activeButton: null
                // Última posición conocida: la pastilla se queda ahí mientras se desvanece (Créditos).
                property Item anchorButton: null
                property bool placed: false
                onActiveButtonChanged: if (activeButton) anchorButton = activeButton
                onAnchorButtonChanged: if (!placed && anchorButton) Qt.callLater(() => navArea.placed = true)

                Rectangle {
                    id: pill
                    objectName: "sidebarIndicator"
                    width: navArea.width
                    y: navArea.anchorButton ? navArea.anchorButton.y : 0
                    height: navArea.anchorButton ? navArea.anchorButton.height : 0
                    opacity: navArea.activeButton && rail.revealed ? 1 : 0
                    radius: Theme.radiusMd
                    // Degradado de acento .28 -> .12 con borde suave y resplandor (capas con alfa, sin shader).
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: Qt.alpha(Theme.accent, 0.28) }
                        GradientStop { position: 1; color: Qt.alpha(Theme.accent, 0.12) }
                    }
                    border.color: Qt.alpha(Theme.accentSoft, 0.35)
                    Behavior on y {
                        enabled: navArea.placed
                        MotionNumber { role: "slow" }
                    }
                    Behavior on height {
                        enabled: navArea.placed
                        MotionNumber { role: "slow" }
                    }
                    Behavior on opacity { MotionNumber { role: "base" } }

                    Rectangle {
                        z: -1; anchors.fill: parent; anchors.margins: -3; radius: parent.radius + 3
                        color: Qt.alpha(Theme.accent, 0.12)
                    }
                    Rectangle {
                        z: -2; anchors.fill: parent; anchors.margins: -7; radius: parent.radius + 7
                        color: Qt.alpha(Theme.accent, 0.05)
                    }
                    Rectangle {
                        anchors.top: parent.top
                        anchors.topMargin: 1
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - Theme.radiusMd * 2
                        height: 1
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: Qt.alpha(Theme.glassHighlight, 0) }
                            GradientStop { position: 0.5; color: Theme.glassHighlight }
                            GradientStop { position: 1; color: Qt.alpha(Theme.glassHighlight, 0) }
                        }
                    }
                    // Barra de acento pegada al borde de la barra lateral, con brillo detras.
                    Rectangle {
                        id: accentBar
                        width: 4
                        height: 22
                        radius: 2
                        x: -9
                        anchors.verticalCenter: parent.verticalCenter
                        color: Theme.accentSoft
                        Rectangle {
                            z: -1; anchors.fill: parent; anchors.margins: -3; radius: 5
                            color: Qt.alpha(Theme.accent, 0.40)
                        }
                        Rectangle {
                            z: -2; anchors.fill: parent; anchors.margins: -6; radius: 8
                            color: Qt.alpha(Theme.accent, 0.16)
                        }
                    }
                    // Al llegar, la barra de acento se estira y vuelve: confirma el destino sin parpadeos.
                    SequentialAnimation {
                        id: settle
                        MotionNumber { role: "fast"; target: accentBar; property: "scale"; to: 1.3 }
                        NumberAnimation { target: accentBar; property: "scale"; to: 1; duration: Theme.durationSlow; easing.type: Easing.OutBack; easing.overshoot: 1.8 }
                    }
                    Connections {
                        target: navArea
                        function onAnchorButtonChanged() { if (navArea.placed && Theme.motion) settle.restart() }
                    }
                }

                ColumnLayout {
                    id: navColumn
                    width: parent.width
                    spacing: rail.compact ? 4 : 5
                    Repeater {
                        model: rail.destinations
                        SidebarButton {
                            id: navButton
                            required property var modelData
                            required property int index
                            objectName: "sidebar-" + modelData.id
                            Layout.fillWidth: true
                            compact: rail.compact
                            expanded: rail.expanded
                            sharedIndicator: true
                            revealed: rail.revealed
                            revealIndex: index
                            text: modelData.label
                            glyph: modelData.glyph
                            subtitle: modelData.id === "official" ? (launcher.officialPack.name || qsTr("Instancia recomendada"))
                                                                  : (rail.descriptions[modelData.id] || "")
                            count: modelData.id === "instances" ? launcher.instances.length : -1
                            selected: rail.settingsOpen ? modelData.id === "settings" : rail.currentPage === modelData.id
                            onClicked: rail.navigate(modelData.id)
                            onWantsTipChanged: rail.syncTip(navButton, wantsTip, text)
                            onSelectedChanged: {
                                if (selected) navArea.activeButton = navButton
                                else if (navArea.activeButton === navButton) navArea.activeButton = null
                            }
                            Component.onCompleted: if (selected) navArea.activeButton = navButton
                        }
                    }
                }
            }
            Button {
                id: create
                Layout.fillWidth: true
                Layout.preferredHeight: rail.compact ? 34 : 38
                Layout.topMargin: rail.compact ? 10 : 14
                enabled: !launcher.gameBusy && !launcher.catalogBusy
                hoverEnabled: true
                Accessible.name: "Nueva instancia"
                onHoveredChanged: rail.syncTip(create, hovered || visualFocus, qsTr("Nueva instancia"))
                onVisualFocusChanged: rail.syncTip(create, hovered || visualFocus, qsTr("Nueva instancia"))
                opacity: rail.revealed ? 1 : 0
                Behavior on opacity {
                    SequentialAnimation {
                        PauseAnimation { duration: Theme.motion ? rail.destinations.length * 45 : 0 }
                        MotionNumber { role: "slow" }
                    }
                }
                onClicked: rail.createRequested()
                background: Rectangle {
                    radius: Theme.radiusSm
                    color: create.down ? Theme.edge : create.hovered ? Theme.soft : Qt.alpha(Theme.surface, 0.72)
                    Behavior on color { MotionColor { role: "fast" } }
                    border.color: create.activeFocus ? Theme.highlight : Theme.border
                    opacity: create.enabled ? 1 : 0.5
                }
                contentItem: RowLayout {
                    spacing: 8
                    Item { Layout.fillWidth: true }
                    Glyph {
                        Layout.preferredWidth: 15; Layout.preferredHeight: 15; name: "plus"; color: Theme.highlight
                        rotation: create.hovered && Theme.motion ? 90 : 0 // motion-ok: valor de destino, no duración
                        Behavior on rotation { MotionNumber { role: "fast" } }
                    }
                    Text {
                        text: qsTr("Nueva instancia"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 12; font.weight: Font.Medium
                        opacity: rail.expanded ? (create.enabled ? 1 : 0.5) : 0
                        visible: opacity > 0.01
                        Behavior on opacity { MotionNumber { role: "base" } }
                    }
                    Item { Layout.fillWidth: true }
                }
            }
            Item { Layout.fillHeight: true; Layout.minimumHeight: rail.compact ? 8 : 14 }
            SidebarButton {
                id: creditsButton
                objectName: "sidebar-credits"
                Layout.fillWidth: true
                compact: true
                expanded: rail.expanded
                implicitHeight: rail.compact ? 40 : 44
                text: qsTr("Créditos")
                glyph: "code"
                revealed: rail.revealed
                revealIndex: rail.destinations.length + 1
                selected: !rail.settingsOpen && rail.currentPage === "credits"
                onClicked: rail.navigate("credits")
                onWantsTipChanged: rail.syncTip(creditsButton, wantsTip, text)
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border; Layout.topMargin: 7; Layout.bottomMargin: 9 }
            Button {
                id: account
                objectName: "sidebar-account"
                Layout.fillWidth: true
                Layout.preferredHeight: rail.compact ? 52 : 62
                hoverEnabled: true
                Accessible.name: launcher.accountName ? qsTr("Cuenta de %1").arg(launcher.accountName) : qsTr("Iniciar sesión")
                leftPadding: rail.expanded ? 12 : Math.max(0, (Theme.railCollapsed - 20 - (rail.compact ? 32 : 36)) / 2)
                rightPadding: leftPadding
                onHoveredChanged: rail.syncTip(account, hovered || visualFocus, launcher.accountName || qsTr("Iniciar sesión"))
                onVisualFocusChanged: rail.syncTip(account, hovered || visualFocus, launcher.accountName || qsTr("Iniciar sesión"))
                onClicked: rail.accountRequested()
                opacity: rail.revealed ? 1 : 0
                Behavior on opacity {
                    SequentialAnimation {
                        PauseAnimation { duration: Theme.motion ? (rail.destinations.length + 2) * 45 : 0 }
                        MotionNumber { role: "slow" }
                    }
                }
                background: Rectangle {
                    radius: Theme.radiusMd
                    color: account.down ? Theme.edge : account.hovered ? Theme.soft : Theme.surface
                    Behavior on color { MotionColor { role: "fast" } }
                    border.color: account.activeFocus ? Theme.highlight : account.hovered ? Theme.edge : Theme.border
                }
                contentItem: RowLayout {
                    spacing: 9
                    scale: account.down ? 0.98 : 1
                    Behavior on scale { MotionNumber { role: "press" } }
                    Rectangle {
                        id: avatarTile
                        Layout.preferredWidth: rail.compact ? 32 : 36
                        Layout.preferredHeight: width
                        radius: Theme.radiusSm
                        color: Theme.soft
                        border.width: 1.5
                        // El borde indica el estado de la sesión: verde lista, ámbar requiere entrar de nuevo.
                        border.color: launcher.usingMicrosoft && launcher.microsoftNeedsLogin ? Theme.warning
                                    : launcher.accountName ? Qt.alpha(Theme.success, 0.75) : Theme.edge
                        Behavior on border.color { MotionColor { role: "base" } }
                        clip: true
                        readonly property bool hasFace: launcher.accountName.length > 0 && wardrobe.accountFace.length > 0
                        Image {
                            anchors.fill: parent
                            anchors.margins: 3
                            visible: parent.hasFace
                            source: parent.hasFace ? wardrobe.accountFace : ""
                            smooth: false
                            fillMode: Image.PreserveAspectFit
                        }
                        Glyph { anchors.centerIn: parent; visible: !parent.hasFace; name: "user"; color: Theme.highlight; width: 17; height: 17 }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        opacity: rail.expanded ? 1 : 0
                        visible: opacity > 0.01
                        Behavior on opacity { MotionNumber { role: "base" } }
                        Text {
                            Layout.fillWidth: true
                            text: launcher.accountName || qsTr("Tu cuenta")
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            color: Theme.ink
                            font.family: Theme.fontFamily
                            font.pixelSize: 13
                            font.weight: Font.DemiBold
                        }
                        RowLayout {
                            spacing: 5
                            Rectangle {
                                width: 5; height: 5; radius: 3
                                color: launcher.usingMicrosoft && launcher.microsoftNeedsLogin ? Theme.warning
                                     : launcher.accountName ? Theme.success : Theme.dim
                            }
                            Text {
                                text: launcher.usingMicrosoft ? (launcher.microsoftNeedsLogin ? qsTr("Vuelve a entrar") : qsTr("Microsoft · Java"))
                                    : launcher.accountName ? qsTr("Offline") : qsTr("Iniciar sesión")
                                color: Theme.dim
                                font.family: Theme.fontFamily
                                font.pixelSize: 11
                            }
                        }
                    }
                    Glyph {
                        name: "chevron"; rotation: -90; color: account.hovered ? Theme.highlight : Theme.dim
                        visible: rail.expanded
                        Layout.preferredWidth: 12; Layout.preferredHeight: 12
                        transform: Translate {
                            x: account.hovered && Theme.motion ? 3 : 0 // motion-ok: valor de destino, no duración
                            Behavior on x { MotionNumber { role: "fast" } }
                        }
                    }
                }
            }
            Button {
                id: pin
                objectName: "sidebar-pin"
                Layout.fillWidth: true
                Layout.preferredHeight: 32
                Layout.topMargin: 8
                hoverEnabled: true
                Accessible.name: rail.pinned ? qsTr("Soltar la barra lateral") : qsTr("Fijar la barra lateral")
                onHoveredChanged: rail.syncTip(pin, hovered || visualFocus, qsTr("Fijar la barra (Ctrl+B)"))
                onVisualFocusChanged: rail.syncTip(pin, hovered || visualFocus, qsTr("Fijar la barra (Ctrl+B)"))
                onClicked: rail.togglePin()
                background: Rectangle {
                    radius: Theme.radiusSm
                    color: pin.down ? Theme.edge : pin.hovered ? Theme.soft : "transparent"
                    Behavior on color { MotionColor { role: "fast" } }
                    border.color: pin.visualFocus ? Theme.highlight : "transparent"
                    border.width: 2
                }
                contentItem: RowLayout {
                    spacing: 8
                    Item { Layout.fillWidth: true }
                    Glyph {
                        Layout.preferredWidth: 16; Layout.preferredHeight: 16; name: "sidebar"
                        color: rail.pinned ? Theme.highlight : Theme.dim
                    }
                    Text {
                        text: rail.pinned ? qsTr("Soltar barra") : qsTr("Fijar barra  ·  Ctrl+B")
                        color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 11
                        opacity: rail.expanded ? 1 : 0
                        visible: opacity > 0.01
                        Behavior on opacity { MotionNumber { role: "base" } }
                    }
                    Item { Layout.fillWidth: true }
                }
            }
        }
    }
}
