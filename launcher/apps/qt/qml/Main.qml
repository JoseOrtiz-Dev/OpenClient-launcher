import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Layouts
import OpenClient

ApplicationWindow {
    id: window
    width: 1260; height: 800
    minimumWidth: 940; minimumHeight: 620
    visible: true
    title: qsTr("OpenClient")
    color: Theme.base
    font.family: Theme.fontFamily
    // Paleta de los controles Basic sin estilo propio. Con Cristal usa los tokens de campo/fila; con los temas antiguos
    // los roles nuevos toman el valor por defecto del sistema (`defaults`, un Control fuera de la ventana).
    readonly property Control defaults: Control {}
    palette.window: Theme.base
    palette.windowText: Theme.ink
    palette.base: Theme.crystal ? Theme.fieldFill : Theme.card
    palette.text: Theme.ink
    palette.button: Theme.crystal ? Theme.rowFillHover : Theme.card
    palette.buttonText: Theme.ink
    palette.highlight: Theme.accent
    palette.highlightedText: Theme.accentText
    palette.mid: Theme.crystal ? Theme.glassBorderHover : defaults.palette.mid
    palette.midlight: Theme.crystal ? Theme.rowFillHover : defaults.palette.midlight
    palette.dark: Theme.crystal ? Theme.accent : defaults.palette.dark
    palette.placeholderText: Theme.crystal ? Theme.dim : defaults.palette.placeholderText
    palette.toolTipBase: Theme.crystal ? Theme.glassStrong : defaults.palette.toolTipBase
    palette.toolTipText: Theme.crystal ? Theme.ink : defaults.palette.toolTipText
    property string activePage: "menu"
    property string quickInstanceId: ""
    // Instancia que muestra el Inicio (misma regla que HomeContent: elegida, predeterminada o la primera).
    readonly property var homeInstance: launcher.instances.find(item => item.id === quickInstanceId)
                                        || launcher.instances.find(item => item.id === launcher.defaultInstanceId)
                                        || launcher.instances[0] || null
    readonly property string homeInstanceId: homeInstance ? homeInstance.id : ""
    readonly property bool backdropActive: window.active && window.visibility !== Window.Minimized && !launcher.gameBusy
                                           && !window.settingsShown && !accountDialog.opened
    property string catalogTargetId: ""
    property string catalogKind: "modpack"
    property bool interactiveStartup: true
    property bool wasMaximized: false
    property bool settingsShown: false
    readonly property string presencePage: settingsShown ? "settings" : activePage
    onPresencePageChanged: discord.setPage(presencePage)
    function openDiscovery(kind, instanceId) {
        catalogKind = kind
        catalogTargetId = instanceId
        if (activePage === "downloads" && pageLoader.item)
            pageLoader.item.configure(kind, instanceId)
        else activePage = "downloads"
    }

    function showIntroduction() {
        if (launcher.setupPending && !launcher.accountName.length && !launcher.instances.length) {
            welcomeDialog.open()
        } else {
            if (launcher.setupPending) launcher.completeSetup()
            if (launcher.releasePending) releaseNotes.open()
        }
    }

    Component.onCompleted: {
        Theme.backdrop = backdropScene
        styleFrame()
        discord.setPage(presencePage)
        if (interactiveStartup) startupIntro.playing = true
    }
    // La barra de título nativa toma el color de la parte superior del fondo, así se funde con el launcher.
    function styleFrame() {
        launcher.styleWindowFrame(window,
            Qt.tint(Theme.base, Qt.alpha(Theme.accent, Theme.light ? 0.035 : 0.055)),
            Theme.ink,
            Theme.light ? Theme.border : Qt.tint(Theme.border, Qt.alpha(Theme.accent, 0.12)),
            !Theme.light)
    }
    Connections {
        target: Theme
        function onCurrentChanged() { window.styleFrame() }
    }
    function createInstance() {
        activePage = "instances"
        Qt.callLater(function() {
            if (window.activePage === "instances" && pageLoader.item)
                pageLoader.item.openEditor(null)
        })
    }
    Connections {
        target: launcher
        function onMinimizeRequested() {
            window.wasMaximized = window.visibility === Window.Maximized
            window.showMinimized()
        }
        function onGameCrashed(instanceId, exitCode) {
            crashNotice.show(instanceId, exitCode)
        }
        function onRefocusRequested() {
            if (window.visibility === Window.Minimized) {
                if (window.wasMaximized) window.showMaximized()
                else window.showNormal()
            }
            window.requestActivate()
        }
    }
    onClosing: close => {
        if (launcher.gameBusy || launcher.copyingInstance) {
            close.accepted = false
            window.activePage = "instances"
            if (launcher.closeWarning) closeNotice.open()
        }
    }
    // Solo QA: muestra los estados de PlayButton superpuestos (OPENCLIENT_QA_PLAY_GALLERY).
    property bool qaPlayGallery: false
    readonly property var destinations: [
        { id: "menu", label: qsTr("Inicio"), glyph: "home" },
        { id: "official", label: qsTr("Modpack oficial"), glyph: "star" },
        { id: "instances", label: qsTr("Instancias"), glyph: "layers" },
        { id: "downloads", label: qsTr("Explorar contenido"), glyph: "compass" },
        { id: "wardrobe", label: qsTr("Skins y Capas"), glyph: "shirt" },
        { id: "settings", label: qsTr("Ajustes"), glyph: "settings" }
    ]
    readonly property string pageTitle: activePage === "credits" ? qsTr("Créditos") : (destinations.find(v => v.id === activePage) || destinations[0]).label

    // Todo el contenido de la ventana: lo desenfoca el fondo de Ajustes. Los diálogos quedan fuera.
    Item {
        id: windowContent
        anchors.fill: parent

        Item {
            id: backdropScene
            anchors.fill: parent
            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.tint(Theme.base, Qt.alpha(Theme.accent, Theme.light ? 0.035 : 0.055)) }
                    GradientStop { position: 0.6; color: Theme.base }
                    GradientStop { position: 1; color: Theme.base }
                }
            }
            // Orbes de luz que derivan despacio (fondo «cristal»). Quietos con la ventana inactiva o con un diálogo encima:
            // así el desenfoque de la barra lateral y de Ajustes no recaptura el fondo a cada fotograma.
            // Banner de la instancia del Inicio: imagen suave que deriva muy despacio. Solo en el menú; fuera de él opacidad 0
            // (y sin cargar nada ni animar). Con la ventana inactiva o con un diálogo encima se queda quieto, como los orbes.
            BannerBackdrop {
                id: bannerBackdrop
                objectName: "bannerBackdrop"
                anchors.fill: parent
                readonly property bool onHome: window.activePage === "menu"
                instanceId: onHome ? window.homeInstanceId : ""
                active: window.backdropActive && onHome
                opacity: onHome ? 1 : 0
                visible: opacity > 0
                Behavior on opacity { MotionNumber { role: "base" } }
            }
            Orbs {
                objectName: "backdropOrbs"
                active: window.backdropActive
                tint: bannerBackdrop.onHome ? bannerBackdrop.accent : "transparent"
            }
            AmbientParticles {
                objectName: "workspaceParticles"
                anchors.fill: parent
                anchors.leftMargin: sidebar.railWidth
                anchors.topMargin: 68
                running: window.active && window.visibility !== Window.Minimized && !launcher.gameBusy && !window.settingsShown
            }
        }

        Loader {
            active: window.qaPlayGallery
            z: 60
            x: window.width - 370
            y: 70
            sourceComponent: PlayButtonGallery {}
        }

        RowLayout {
            anchors.fill: parent
            spacing: 0
            Sidebar {
                id: sidebar
                // Por encima del contenido: al desplegarse con el ratón se superpone sin empujarlo.
                z: 20
                Layout.preferredWidth: sidebar.railWidth
                Layout.fillHeight: true
                currentPage: window.activePage
                settingsOpen: window.settingsShown
                hold: startupIntro.playing && !startupIntro.revealing
                destinations: window.destinations
                onNavigate: page => {
                    if (page === "settings") window.settingsShown = true
                    else {
                        window.settingsShown = false
                        if (page === "downloads") window.openDiscovery("modpack", "")
                        else window.activePage = page
                    }
                }
                onCreateRequested: window.createInstance()
                onAccountRequested: accountDialog.open()
            }
            ColumnLayout {
                Layout.fillWidth: true; Layout.fillHeight: true; spacing: 0
                // Barra superior flotante: tarjeta de cristal redondeada con el mismo margen lateral que el contenido.
                Item {
                    Layout.fillWidth: true; implicitHeight: 68
                    GlassPanel {
                        id: topBar
                        objectName: "topBar"
                        anchors.fill: parent
                        anchors.leftMargin: window.width < 1090 ? 18 : 24
                        anchors.rightMargin: anchors.leftMargin
                        anchors.topMargin: 10; anchors.bottomMargin: 6
                        padding: 0
                        radius: Theme.radiusXl
                    RowLayout {
                        anchors.fill: parent; anchors.leftMargin: 22; anchors.rightMargin: 8; spacing: 8
                        Text {
                            text: window.pageTitle; color: Theme.ink
                            font.family: Theme.displayFamily; font.pixelSize: 15; font.weight: Font.Bold
                        }
                        Item { Layout.fillWidth: true }
                        GlassIconButton {
                            Layout.preferredWidth: 36; Layout.preferredHeight: 36
                            glyph: "news"
                            Accessible.name: qsTr("Novedades de OpenClient")
                            ToolTip.visible: hovered
                            ToolTip.delay: 450
                            ToolTip.text: qsTr("Novedades de OpenClient")
                            onClicked: releaseNotes.open()
                        }
                        Repeater {
                            model: [
                                { name: "Web oficial", glyph: "globe", link: 0 },
                                { name: "Discord", glyph: "discord", link: 1 },
                                { name: "Tienda oficial", glyph: "storefront", link: 2 },
                                { name: "GitHub", glyph: "github", link: 3 }
                            ]
                            Item {
                                id: communityLink
                                required property var modelData
                                readonly property bool available: launcher.linkAvailable(modelData.link)
                                Layout.preferredWidth: 36
                                Layout.preferredHeight: 36
                                HoverHandler { id: linkHover }
                                ToolTip.visible: linkHover.hovered
                                ToolTip.delay: 450
                                ToolTip.text: available ? modelData.name : qsTr("%1 · Enlace pendiente").arg(modelData.name)
                                GlassIconButton {
                                    anchors.fill: parent
                                    glyph: communityLink.modelData.glyph
                                    available: communityLink.available
                                    enabled: communityLink.available
                                    Accessible.name: communityLink.modelData.name
                                    Accessible.description: communityLink.available ? qsTr("Abrir en el navegador") : qsTr("Enlace pendiente")
                                    onClicked: launcher.openLink(communityLink.modelData.link)
                                }
                            }
                        }
                    }
                    }
                }
                Item {
                    Layout.fillWidth: true; Layout.fillHeight: true
                    ScrollView {
                        id: scroll
                        objectName: "pageScroll"
                        anchors.fill: parent
                        leftPadding: window.width < 1090 ? 18 : 24
                        rightPadding: leftPadding
                        topPadding: 20
                        bottomPadding: 24
                        clip: true
                        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                        ColumnLayout {
                            width: scroll.availableWidth
                            spacing: 16
                            Item {
                                id: pageContainer
                                Layout.fillWidth: true
                                Layout.preferredHeight: pageLoader.item ? pageLoader.item.implicitHeight : 0
                                Reveal { id: pageReveal; fadeTarget: pageContainer }
                                Loader {
                                    id: pageLoader
                                    width: parent.width
                                    height: item ? item.implicitHeight : 0
                                    sourceComponent: {
                                        switch (window.activePage) {
                                        case "official": return officialPage;
                                        case "instances": return instancesPage;
                                        case "downloads": return downloadsPage;
                                        case "wardrobe": return wardrobePage;
                                        case "credits": return creditsPage;
                                        default: return homePage;
                                        }
                                    }
                                    onLoaded: {
                                        scroll.contentItem.contentY = 0
                                        if (Theme.motion) {
                                            pageContainer.opacity = 0
                                            Qt.callLater(() => pageReveal.play(pageLoader.item))
                                        }
                                    }
                                }
                            }
                            Text {
                                visible: launcher.linkError.length > 0
                                text: launcher.linkError
                                color: Theme.danger
                                font.family: Theme.fontFamily
                                Layout.fillWidth: true
                                wrapMode: Text.WordWrap
                            }
                            Item { implicitHeight: 4 }
                        }
                    }
                }
            }
        }
    }
    // Botón cuadrado de cristal de la barra superior: relleno blanco .055, borde .11 y, al pasar, .11 / .22 con 1 px de elevación.
    // Sin desenfoque propio. El icono se atenúa si el enlace aún no existe.
    component GlassIconButton: Button {
        id: gib
        property string glyph: ""
        property bool available: true
        implicitWidth: 36
        implicitHeight: 36
        hoverEnabled: true
        transform: Translate {
            y: gib.hovered && !gib.down && gib.available && Theme.motion ? -1 : 0 // motion-ok: valor de destino, no duración
            Behavior on y { MotionNumber { role: "fast" } }
        }
        background: Rectangle {
            radius: Theme.radiusMd
            color: Qt.alpha(Theme.ink, !gib.available ? 0.03 : gib.down ? 0.15 : gib.hovered || gib.activeFocus ? 0.11 : 0.055)
            Behavior on color { MotionColor { role: "fast" } }
            border.width: gib.visualFocus ? 2 : 1
            border.color: gib.visualFocus ? Theme.highlight : Qt.alpha(Theme.ink, gib.available && gib.hovered ? 0.22 : 0.11)
            Behavior on border.color { MotionColor { role: "fast" } }
            Rectangle {
                y: 1; x: parent.radius; height: 1
                width: Math.max(0, parent.width - parent.radius * 2)
                color: Theme.glassHighlight
            }
        }
        contentItem: Item {
            Glyph {
                anchors.centerIn: parent
                width: 17; height: 17
                name: gib.glyph
                color: !gib.available ? Theme.dim : gib.hovered ? Theme.ink : Qt.alpha(Theme.ink, 0.75)
                opacity: gib.available ? 1 : 0.65
                Behavior on color { MotionColor { role: "fast" } }
            }
        }
    }
    Component {
        id: homePage
        HomePage {
            selectedId: window.quickInstanceId
            onInstanceSelected: id => window.quickInstanceId = id
            onNavigate: destination => window.activePage = destination
            onCreateRequested: window.createInstance()
            onAccountRequested: accountDialog.open()
        }
    }
    Component {
        id: instancesPage
        InstancesPage {
            onExplore: (instanceId, kind) => window.openDiscovery(kind, instanceId)
            onBrowseModpacks: window.openDiscovery("modpack", "")
        }
    }
    Component {
        id: downloadsPage
        DownloadsPage {
            preferredInstanceId: window.catalogTargetId
            preferredKind: window.catalogKind
            autoSearch: window.interactiveStartup
        }
    }
    Component { id: wardrobePage; WardrobePage {} }
    Component { id: officialPage; OfficialPage {} }
    Dialog {
        id: settingsDialog
        visible: window.settingsShown
        modal: true
        anchors.centerIn: parent
        title: qsTr("Ajustes")
        width: Math.min(1100, window.width - 64)
        height: Math.min(720, window.height - 60)
        padding: 0
        onClosed: window.settingsShown = false
        // Todo el fondo de la ventana se ve desenfocado; sin desenfoque (renderer por software, modo bajo consumo) queda solo el velo.
        Overlay.modal: Item {
            Behavior on opacity { MotionNumber { role: "base" } }
            FrostedBackdrop {
                id: settingsBlur
                anchors.fill: parent
                source: windowContent
                live: Theme.motion
                strength: 1.0
            }
            Rectangle {
                anchors.fill: parent
                color: Theme.scrim
                opacity: settingsBlur.available ? 0.6 : 1
            }
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
            // Cristal: base opaca + material de GlassCard (fuerte), como GlassDialog.
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
                color: Theme.glassElevated
                border.color: Theme.rim
                radius: Theme.radiusXl
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
            AmbientParticles {
                anchors.fill: parent
                anchors.margins: 10
                anchors.topMargin: 76
                anchors.leftMargin: 285
                density: 0.5
                opacity: 0.38
                running: settingsDialog.opened && window.active && window.visibility !== Window.Minimized && !launcher.gameBusy
            }
        }
        // El título y el botón de cerrar viven dentro de SettingsPage.
        header: Item { implicitHeight: 0 }
        contentItem: SettingsPage {
            onShowWelcome: {
                settingsDialog.close()
                welcomeDialog.open()
            }
            onAccountsRequested: {
                settingsDialog.close()
                accountDialog.open()
            }
            onCloseRequested: settingsDialog.close()
        }
    }
    GlassDialog {
        id: crashNotice
        objectName: "crashNotice"
        property string instanceId: ""
        property int exitCode: 0
        property string log: ""
        readonly property var instance: launcher.instances.find(item => item.id === instanceId) || null
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(720, window.width - 48)
        glyph: "news"
        title: qsTr("Minecraft se cerró con un error")
        function show(id, code) {
            instanceId = id
            exitCode = code
            const lines = launcher.instanceLog(id).split("\n")
            log = lines.slice(Math.max(0, lines.length - 80)).join("\n")
            open()
        }
        contentItem: ColumnLayout {
            spacing: 12
            Label {
                text: crashNotice.instance
                    ? qsTr("«%1» terminó con el código %2. Estas son las últimas líneas del registro; la causa suele aparecer al final.").arg(crashNotice.instance.name).arg(String(crashNotice.exitCode))
                    : qsTr("La instancia terminó con el código %1. Estas son las últimas líneas del registro; la causa suele aparecer al final.").arg(String(crashNotice.exitCode))
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
            Rectangle {
                id: diagnosis
                objectName: "crashDiagnosis"
                readonly property var findings: launcher.crashFindings
                visible: launcher.crashAnalyzing || findings.length > 0
                Layout.fillWidth: true
                implicitHeight: diagnosisBody.implicitHeight + 24
                radius: Theme.radiusMd
                color: Qt.alpha(Theme.accent, Theme.light ? 0.07 : 0.10)
                border.color: Theme.edge
                Accessible.role: Accessible.Grouping
                Accessible.name: launcher.crashAnalyzing ? qsTr("Causa probable: analizando el registro")
                    : findings.length === 1 ? qsTr("Causa probable: %1 hallazgo").arg(findings.length) : qsTr("Causa probable: %1 hallazgos").arg(findings.length)
                ColumnLayout {
                    id: diagnosisBody
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 8
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        Label { text: qsTr("Causa probable"); font.weight: Font.DemiBold; color: Theme.ink; Layout.fillWidth: true }
                        BusyIndicator { visible: launcher.crashAnalyzing; running: visible && Theme.motion; Layout.preferredWidth: 18; Layout.preferredHeight: 18 }
                        Label { visible: launcher.crashAnalyzing; text: qsTr("Analizando…"); color: Theme.dim; font.pixelSize: 12 }
                    }
                    ScrollView {
                        id: findingsScroll
                        visible: diagnosis.findings.length > 0
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.min(findingsColumn.implicitHeight, 190)
                        contentWidth: availableWidth
                        contentHeight: findingsColumn.implicitHeight
                        clip: true
                        ColumnLayout {
                            id: findingsColumn
                            width: findingsScroll.availableWidth
                            spacing: 10
                            Repeater {
                                model: diagnosis.findings
                                RowLayout {
                                    id: finding
                                    required property var modelData
                                    required property int index
                                    property bool shown: !Theme.motion
                                    Layout.fillWidth: true
                                    spacing: 10
                                    opacity: shown ? 1 : 0
                                    Behavior on opacity { MotionNumber { role: "base" } }
                                    Timer { running: Theme.motion; interval: finding.index * 45; onTriggered: finding.shown = true }
                                    Accessible.role: Accessible.ListItem
                                    Accessible.name: qsTr("%1. %2. Qué hacer: %3").arg(finding.modelData.title).arg(finding.modelData.detail).arg(finding.modelData.action)
                                    Rectangle {
                                        Layout.preferredWidth: 3
                                        Layout.fillHeight: true
                                        radius: 2
                                        color: finding.modelData.severity === "error" ? Theme.danger
                                            : finding.modelData.severity === "warning" ? Theme.warning : Theme.dim
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 2
                                        Label { Layout.fillWidth: true; text: finding.modelData.title; textFormat: Text.PlainText; wrapMode: Text.WordWrap; font.weight: Font.DemiBold; color: Theme.ink }
                                        Label { Layout.fillWidth: true; text: finding.modelData.detail; textFormat: Text.PlainText; wrapMode: Text.WordWrap; font.pixelSize: 12; color: Theme.dim }
                                        Label { Layout.fillWidth: true; text: qsTr("Qué hacer: %1").arg(finding.modelData.action); textFormat: Text.PlainText; wrapMode: Text.WordWrap; font.pixelSize: 12; color: Theme.ink }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Label {
                objectName: "crashNoDiagnosis"
                visible: !launcher.crashAnalyzing && launcher.crashFindings.length === 0
                text: qsTr("No se reconoció la causa: copia el registro y abre un issue.")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.pixelSize: 12
                color: Theme.dim
                Layout.fillWidth: true
            }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: launcher.crashFindings.length > 0 ? 170 : 300
                radius: Theme.radiusSm
                color: Theme.base
                border.color: Theme.border
                ScrollView {
                    id: crashScroll
                    anchors.fill: parent
                    anchors.margins: 1
                    clip: true
                    TextArea {
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        textFormat: TextEdit.PlainText
                        text: crashNotice.log.length ? crashNotice.log : qsTr("El registro está vacío.")
                        font.family: "Consolas"
                        font.pixelSize: 11
                        color: Theme.ink
                        background: null
                        onTextChanged: Qt.callLater(() => { crashScroll.ScrollBar.vertical.position = 1.0 - crashScroll.ScrollBar.vertical.size })
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Label { id: copiedLabel; visible: false; text: qsTr("Copiado."); color: Theme.success; Layout.fillWidth: true }
                Item { Layout.fillWidth: true; visible: !copiedLabel.visible }
                ActionButton { text: qsTr("Abrir carpeta de registros"); glyph: "folder"; onClicked: launcher.openInstanceFolder(crashNotice.instanceId, "logs") }
                ActionButton {
                    text: qsTr("Copiar registro")
                    onClicked: {
                        launcher.copyText(crashNotice.log)
                        copiedLabel.visible = true
                    }
                }
                ActionButton { text: qsTr("Cerrar"); primary: true; onClicked: crashNotice.close() }
            }
        }
        onClosed: copiedLabel.visible = false
    }
    GlassDialog {
        id: closeNotice
        objectName: "closeNotice"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(440, window.width - 48)
        glyph: "close"
        title: launcher.gameBusy ? qsTr("Minecraft sigue abierto") : qsTr("Hay una copia en curso")
        contentItem: ColumnLayout {
            spacing: 14
            Label {
                text: launcher.gameBusy
                    ? qsTr("OpenClient no se cierra mientras Minecraft está en marcha. Cierra el juego y vuelve a intentarlo.")
                    : qsTr("OpenClient no se cierra mientras copia una instancia. Espera a que termine.")
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
            ActionButton { text: qsTr("Entendido"); primary: true; Layout.alignment: Qt.AlignRight; onClicked: closeNotice.close() }
        }
    }
    Component { id: creditsPage; CreditsPage {} }
    StartupIntro {
        id: startupIntro
        z: 100
        anchors.fill: parent
        onFinished: {
            Qt.callLater(window.showIntroduction)
            maintenance.autoCheck()
        }
    }
    UpdateDialog {
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(440, window.width - 48)
    }
    WelcomeDialog {
        id: welcomeDialog
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(560, window.width - 48)
        onCompleted: {
            launcher.completeSetup()
            welcomeDialog.close()
            if (launcher.releasePending) Qt.callLater(() => releaseNotes.open())
        }
    }
    ReleaseNotes {
        id: releaseNotes
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(880, window.width - 64)
        height: Math.min(600, window.height - 64)
        onClosed: if (launcher.releasePending) launcher.acknowledgeRelease()
        onDiscoverRequested: {
            releaseNotes.close()
            window.openDiscovery("modpack", "")
        }
    }
    AccountDialog {
        id: accountDialog
        objectName: "mainAccountDialog"
        anchors.centerIn: parent
        width: Math.min(1000, window.width - 48)
        height: Math.min(700, window.height - 48)
        // Mismo fondo desenfocado que Ajustes; sin desenfoque (renderer por software, modo bajo consumo) queda solo el velo.
        Overlay.modal: Item {
            Behavior on opacity { MotionNumber { role: "base" } }
            FrostedBackdrop {
                id: accountBlur
                anchors.fill: parent
                source: windowContent
                live: Theme.motion
                strength: 1.0
            }
            Rectangle {
                anchors.fill: parent
                color: Theme.scrim
                opacity: accountBlur.available ? 0.6 : 1
            }
        }
    }
}
