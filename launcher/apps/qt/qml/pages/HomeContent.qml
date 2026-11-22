import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import OpenClient

ColumnLayout {
    id: root
    property string selectedId: ""
    readonly property var pack: launcher.officialPack
    readonly property var current: launcher.instances.find(instance => instance.id === selectedId)
                                   || launcher.instances.find(instance => instance.id === launcher.defaultInstanceId)
                                   || launcher.instances[0] || null
    readonly property string currentId: current ? current.id : ""
    Component.onCompleted: launcher.inspectOfficialPack()
    signal instanceSelected(string id)
    signal navigate(string destination)
    signal createRequested()
    signal accountRequested()
    spacing: 18

    // Punto de estado con un anillo que se expande y se desvanece («ping»). El bucle solo corre con movimiento
    // activado, el punto visible y la aplicación en primer plano.
    component PulseDot: Item {
        id: dot
        property color tone: Theme.accentSoft
        property real size: 8
        property bool pulsing: true
        readonly property bool running: pulsing && Theme.motion && dot.visible && Qt.application.state === Qt.ApplicationActive
        implicitWidth: size
        implicitHeight: size
        onRunningChanged: if (!running) { ping.opacity = 0; ping.scale = 1 }
        Rectangle {
            id: ping
            anchors.fill: parent
            radius: width / 2
            color: dot.tone
            opacity: 0
        }
        Rectangle { anchors.fill: parent; radius: width / 2; color: dot.tone }
        ParallelAnimation {
            running: dot.running
            loops: Animation.Infinite
            NumberAnimation { target: ping; property: "scale"; from: 1; to: 2.6; duration: Theme.durationSlow * 5; easing.type: Easing.OutCubic } // motion-ok: bucle del punto, gateado por Theme.motion y visible
            NumberAnimation { target: ping; property: "opacity"; from: 0.75; to: 0; duration: Theme.durationSlow * 5; easing.type: Easing.OutCubic } // motion-ok: bucle del punto, gateado por Theme.motion y visible
        }
    }

    // Tarjeta pulsable: Button con GlassCard de fondo (relleno, borde, grano y brillo de cursor) y el anillo de foco.
    component CardButton: Button {
        id: cb
        property bool selected: false
        property bool dashed: false
        hoverEnabled: true
        leftPadding: 16
        rightPadding: 16
        topPadding: 12
        bottomPadding: 12
        scale: down && enabled && Theme.motion ? 0.985 : 1
        Behavior on scale { MotionNumber { role: "press" } }
        Accessible.name: text
        background: GlassCard {
            id: glass
            padding: 0
            variant: cb.selected ? "active" : "default"
            hoverable: cb.enabled
            border.width: cb.dashed ? 0 : 1
            opacity: cb.enabled ? 1 : 0.5
            // «Crear instancia»: contorno discontinuo que se enciende con el acento al pasar el ratón.
            Rectangle {
                anchors.fill: parent
                radius: glass.radius - 1
                color: Qt.alpha(Theme.accent, 0.09)
                opacity: cb.dashed && cb.hovered ? 1 : 0
                Behavior on opacity { MotionNumber { role: "fast" } }
            }
            Shape {
                visible: cb.dashed
                anchors.fill: parent
                preferredRendererType: Shape.CurveRenderer
                ShapePath {
                    strokeWidth: 1
                    strokeColor: cb.hovered ? Qt.alpha(Theme.accentSoft, 0.5) : Qt.alpha(Theme.ink, 0.2)
                    Behavior on strokeColor { MotionColor { role: "fast" } }
                    fillColor: "transparent"
                    dashPattern: [4, 4]
                    PathRectangle { x: 0.5; y: 0.5; width: glass.width - 1; height: glass.height - 1; radius: glass.radius - 0.5 }
                }
            }
            Rectangle {
                anchors.fill: parent
                radius: glass.radius
                color: "transparent"
                border.width: 2
                border.color: Theme.highlight
                visible: cb.visualFocus
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 3
            RowLayout {
                Layout.fillWidth: true
                spacing: 1
                Text {
                    text: launcher.accountName ? qsTr("Hola, %1").arg(launcher.accountName) : qsTr("Bienvenido a OpenClient")
                    textFormat: Text.PlainText
                    color: Theme.ink
                    font.family: Theme.displayFamily
                    font.pixelSize: 28
                    font.weight: Font.ExtraBold
                    font.letterSpacing: -0.5
                }
                Text {
                    text: "."
                    color: Theme.accentSoft
                    font.family: Theme.displayFamily
                    font.pixelSize: 28
                    font.weight: Font.ExtraBold
                }
            }
        }
        // Hueco de la cabeza de la cuenta (esquina superior derecha de la página).
        Item { Layout.fillWidth: true }
        Item { Layout.preferredWidth: 52; Layout.preferredHeight: 44 }
    }

    // Héroe: la instancia a la izquierda y, a la derecha, el botón INICIAR con su selector.
    GlassCard {
        id: hero
        objectName: "homeHero"
        Layout.fillWidth: true
        implicitHeight: Theme.heroHeight(root.width)
        padding: 22
        variant: "hero"

        // Banner de la instancia: capa inferior, recortada al radio. Los velos van dentro para quedar recortados igual:
        // a la izquierda casi opaco (el texto mantiene contraste AA) y transparente hacia el 70 % del ancho, y uno inferior ligero.
        underlay: InstanceBanner {
            id: heroBanner
            anchors.fill: parent
            visible: root.current !== null
            instanceId: root.currentId
            radius: Math.max(0, hero.radius - 1)
            scrim: info.source === "generated" ? 0 : 0.3
            // El arte generado se ve más tenue que una imagen real.
            opacity: info.source === "generated" ? 0.5 : 1
            Behavior on opacity { MotionNumber { role: "base" } }
            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: Qt.alpha(Theme.heroC, 0.92) }
                    GradientStop { position: 0.4; color: Qt.alpha(Theme.heroC, 0.8) }
                    GradientStop { position: 0.72; color: Qt.alpha(Theme.heroC, 0) }
                    GradientStop { position: 1; color: Qt.alpha(Theme.heroC, 0) }
                }
            }
            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop { position: 0.55; color: Qt.alpha(Theme.heroC, 0) }
                    GradientStop { position: 1; color: Qt.alpha(Theme.heroC, 0.45) }
                }
            }
        }

        // Capa decorativa detrás del contenido; ocupa toda la tarjeta (el margen negativo cancela el relleno).
        Crystals {
            anchors.margins: -21
            // Con una imagen real los cristales bajan para no competir con ella.
            opacity: heroBanner.visible && heroBanner.info.source !== "generated" ? 0.4 : 1
            Behavior on opacity { MotionNumber { role: "base" } }
            unit: root.width >= 900 ? 1 : 0.85
            active: Qt.application.state === Qt.ApplicationActive && !launcher.gameBusy
        }
        RowLayout {
            anchors.fill: parent
            spacing: 24
            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                spacing: 12
                // Pastilla «Instancia seleccionada» con punto que late.
                Rectangle {
                    radius: height / 2
                    color: Qt.alpha(Theme.accent, 0.18)
                    border.color: Qt.alpha(Theme.accentSoft, 0.30)
                    implicitWidth: pillRow.implicitWidth + 24
                    implicitHeight: 26
                    Row {
                        id: pillRow
                        anchors.centerIn: parent
                        spacing: 8
                        PulseDot {
                            anchors.verticalCenter: parent.verticalCenter
                            size: 8
                            pulsing: root.current !== null
                        }
                        Text {
                            id: editionLabel
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.current ? qsTr("Instancia seleccionada") : qsTr("OpenClient · Java Edition")
                            color: Qt.tint(Theme.ink, Qt.alpha(Theme.accentSoft, 0.2))
                            font.family: Theme.fontFamily
                            font.pixelSize: 11
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.3
                        }
                    }
                }
                // Con instancia: su nombre y datos. Sin instancia: el titular de bienvenida.
                RowLayout {
                    visible: root.current !== null
                    Layout.fillWidth: true
                    spacing: 16
                    // Caja de cristal del bloque: sombra en capas, borde claro y destello superior.
                    Item {
                        id: iconBox
                        Layout.preferredWidth: 64; Layout.preferredHeight: 64
                        Connections {
                            target: root
                            function onCurrentIdChanged() { if (Theme.motion) iconPop.restart() }
                        }
                        SequentialAnimation {
                            id: iconPop
                            PropertyAction { target: iconBox; property: "scale"; value: 0.95 }
                            MotionNumber { target: iconBox; property: "scale"; to: 1; role: "base" }
                        }
                        Rectangle {
                            x: 3; y: 9; width: parent.width - 6; height: parent.height - 4
                            radius: Theme.radiusLg
                            color: Qt.alpha(Theme.base, 0.28)
                        }
                        Rectangle {
                            x: 6; y: 14; width: parent.width - 12; height: parent.height - 4
                            radius: Theme.radiusLg
                            color: Qt.alpha(Theme.base, 0.18)
                            z: -1
                        }
                        Rectangle {
                            anchors.fill: parent
                            radius: Theme.radiusLg
                            color: Qt.alpha(Theme.base, 0.36)
                            border.color: Qt.alpha(Theme.ink, 0.15)
                            Rectangle {
                                y: 1; x: parent.radius; height: 1
                                width: Math.max(0, parent.width - parent.radius * 2)
                                color: Theme.glassHighlight
                            }
                            InstanceBlock {
                                anchors.centerIn: parent
                                width: 44; height: 44
                                loader: root.current ? root.current.category : "vanilla"
                                fabric: root.current ? root.current.category === "fabric" : false
                            }
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 4
                        Text {
                            Layout.fillWidth: true
                            text: root.current ? root.current.name : ""
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            maximumLineCount: 1
                            color: Theme.ink
                            font.family: Theme.displayFamily
                            font.pixelSize: root.width >= 900 ? 34 : 28
                            font.weight: Font.ExtraBold
                            font.letterSpacing: -0.5
                        }
                        Text {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            color: Qt.alpha(Theme.ink, 0.65)
                            font.family: Theme.fontFamily
                            font.pixelSize: 13
                            font.weight: Font.Medium
                            font.features: Theme.tabular
                            text: root.current ? qsTr("%1 · %2 · %3 GB asignados").arg(root.current.loaderLabel).arg(root.current.mcVersion)
                                .arg(((root.current.ramMb || launcher.allocatedRamMb) / 1024).toFixed(1)) : ""
                        }
                    }
                }
                ColumnLayout {
                    visible: root.current === null
                    spacing: 0
                    Text {
                        text: qsTr("Tu próximo mundo")
                        color: Theme.ink
                        font.family: Theme.displayFamily
                        font.pixelSize: root.width >= 900 ? 38 : 30
                        font.weight: Font.ExtraBold
                        font.letterSpacing: -0.8
                    }
                    Text {
                        text: qsTr("empieza aquí.")
                        color: Theme.accentSoft
                        font.family: Theme.displayFamily
                        font.pixelSize: root.width >= 900 ? 38 : 30
                        font.weight: Font.ExtraBold
                        font.letterSpacing: -0.8
                    }
                }
                Text {
                    visible: root.current === null
                    Layout.fillWidth: true
                    text: qsTr("Prepara tus instancias de Vanilla y Fabric con sus propios mods, mundos y memoria.")
                    color: Theme.dim
                    font.family: Theme.fontFamily
                    font.pixelSize: 12
                    wrapMode: Text.WordWrap
                }
            }
            ColumnLayout {
                // Ancho fijo: el botón INICIAR no se estira en ventanas grandes.
                Layout.fillWidth: false
                Layout.preferredWidth: root.width >= 900 ? 320 : 270
                Layout.alignment: Qt.AlignVCenter
                spacing: 8
                PlayButton {
                    id: play
                    objectName: "homePlay"
                    Layout.fillWidth: true
                    readonly property bool offerPack: !root.current && (root.pack.state === "ready" || root.pack.state === "download")
                    hasInstance: root.current !== null
                    instanceLabel: root.current ? qsTr("Minecraft %1 · %2").arg(root.current.mcVersion).arg(root.current.loaderLabel) : ""
                    emptyLabel: offerPack ? qsTr("Instalar %1").arg(root.pack.name || qsTr("modpack oficial")) : qsTr("Crear primera instancia")
                    emptyGlyph: offerPack ? "download" : "plus"
                    busy: launcher.catalogBusy || launcher.copyingInstance
                    onPlay: if (root.current) launcher.playInstance(root.current.id)
                    onEmpty: if (offerPack) launcher.installOfficialPack(); else root.createRequested()
                    onCancel: launcher.cancelLaunch()
                    onReconnect: root.accountRequested()
                }
                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: play.caption
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: play.mode === "session" ? Theme.warning : Qt.alpha(Theme.ink, 0.55)
                    font.family: Theme.fontFamily
                    font.pixelSize: 11
                    font.weight: Font.Medium
                    font.features: Theme.tabular
                }
                // Selector oscuro de cristal.
                ComboBox {
                    id: instanceChooser
                    Layout.fillWidth: true
                    Layout.preferredHeight: 40
                    visible: root.current !== null
                    enabled: !launcher.gameBusy
                    hoverEnabled: true
                    model: launcher.instances
                    textRole: "name"
                    currentIndex: launcher.instances.findIndex(item => root.current && item.id === root.current.id)
                    Accessible.name: "Elegir instancia para jugar"
                    background: Rectangle {
                        radius: Theme.radiusMd
                        color: Theme.crystal ? Theme.fieldFill : Qt.alpha(Theme.base, instanceChooser.hovered || instanceChooser.popup.visible ? 0.6 : 0.45)
                        Behavior on color { MotionColor { role: "fast" } }
                        border.width: instanceChooser.visualFocus ? 2 : 1
                        border.color: instanceChooser.visualFocus ? Theme.highlight
                            : Theme.crystal ? (instanceChooser.hovered || instanceChooser.popup.visible ? Theme.glassBorderHover : Theme.fieldBorder)
                            : Qt.alpha(Theme.ink, instanceChooser.hovered || instanceChooser.popup.visible ? 0.25 : 0.12)
                        Behavior on border.color { MotionColor { role: "fast" } }
                        opacity: instanceChooser.enabled ? 1 : 0.5
                    }
                    contentItem: Text {
                        leftPadding: 14
                        rightPadding: 34
                        text: instanceChooser.displayText
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                        color: Qt.alpha(Theme.ink, instanceChooser.enabled ? 0.9 : 0.5)
                        font.family: Theme.fontFamily
                        font.pixelSize: 13
                        font.weight: Font.Medium
                    }
                    indicator: Glyph {
                        x: instanceChooser.width - width - 12
                        y: (instanceChooser.height - height) / 2
                        width: 14; height: 14
                        name: "chevron"
                        color: Qt.alpha(Theme.ink, 0.6)
                    }
                    onActivated: index => root.instanceSelected(launcher.instances[index].id)
                }
            }
        }
    }

    Label {
        Layout.fillWidth: true
        visible: launcher.gameMessage.length > 0 && !launcher.gameBusy
        text: launcher.gameMessage
        textFormat: Text.PlainText
        color: Theme.dim
        wrapMode: Text.WordWrap
    }
    OperationBar { Layout.fillWidth: true; heading: launcher.copyMessage }

    // Primera fila: Patrocinador oficial (siempre visible) y, si hay sitio, Modpack oficial al lado (3/5 y 2/5).
    GridLayout {
        Layout.fillWidth: true
        columns: root.width >= 1000 ? 2 : 1
        columnSpacing: 14
        rowSpacing: 14

        GlassCard {
            objectName: "sponsorCard"
            Layout.fillWidth: true
            Layout.preferredWidth: 3
            Layout.fillHeight: true
            implicitHeight: 150
            padding: 16
            variant: "sponsor"
            clip: true
            // Resplandor de acento abajo a la derecha (en la referencia, un círculo de 176 px con desenfoque).
            Shape {
                x: parent.width + 16 - 56 - 132; y: parent.height + 16 - 48 - 132
                width: 264; height: 264
                preferredRendererType: Shape.CurveRenderer
                ShapePath {
                    strokeWidth: -1
                    fillColor: "transparent"
                    fillGradient: RadialGradient {
                        centerX: 132; centerY: 132
                        focalX: centerX; focalY: centerY
                        centerRadius: 132
                        GradientStop { position: 0; color: Qt.alpha(Theme.accent, 0.15) }
                        GradientStop { position: 1; color: Qt.alpha(Theme.accent, 0) }
                    }
                    PathRectangle { width: 264; height: 264 }
                }
            }
            RowLayout {
                anchors.fill: parent
                spacing: 18
                Item {
                    Layout.preferredWidth: root.width > 770 ? 116 : 88
                    Layout.fillHeight: true
                    Image {
                        objectName: "enderLogo"
                        anchors.centerIn: parent
                        width: Math.min(parent.width, 108); height: width
                        source: "qrc:/assets/enderhosting.png"
                        sourceSize.width: 256; sourceSize.height: 256
                        fillMode: Image.PreserveAspectFit
                        smooth: true; mipmap: true
                        Accessible.name: "Logo de EnderHosting"
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 5
                    Rectangle {
                        radius: height / 2
                        color: Qt.alpha(Theme.ink, 0.08)
                        border.color: Qt.alpha(Theme.ink, 0.10)
                        implicitWidth: sponsorBadge.implicitWidth + 20
                        implicitHeight: 20
                        Text {
                            id: sponsorBadge
                            anchors.centerIn: parent
                            text: qsTr("Patrocinador oficial")
                            color: Qt.tint(Theme.ink, Qt.alpha(Theme.accentSoft, 0.45))
                            font.family: Theme.fontFamily
                            font.pixelSize: 10
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.3
                        }
                    }
                    RowLayout {
                        spacing: 0
                        Text { text: qsTr("Ender"); color: Theme.ink; font.family: Theme.displayFamily; font.pixelSize: 26; font.weight: Font.ExtraBold; font.letterSpacing: -0.5 }
                        Text { text: qsTr("Hosting"); color: Theme.accentSoft; font.family: Theme.displayFamily; font.pixelSize: 26; font.weight: Font.ExtraBold; font.letterSpacing: -0.5 }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: qsTr("Hosting de baja latencia para tu próximo servidor de Minecraft.")
                        wrapMode: Text.WordWrap
                        color: Qt.alpha(Theme.ink, 0.6)
                        font.family: Theme.fontFamily
                        font.pixelSize: 12
                    }
                }
                // 180 px: «Visitar hosting» con su icono en Inter no cabe en 154.
                ColumnLayout {
                    Layout.preferredWidth: 180
                    Layout.maximumWidth: 180
                    spacing: 8
                    PillButton {
                        Layout.fillWidth: true; implicitHeight: 36
                        text: qsTr("Visitar hosting"); glyph: "globe"
                        tone: "link"; iconSpin: true; trailingGlyph: "arrow"
                        onClicked: launcher.openLink(4)
                    }
                    PillButton { Layout.fillWidth: true; implicitHeight: 36; text: qsTr("Discord"); glyph: "discord"; tone: "glass"; onClicked: launcher.openLink(5) }
                }
            }
        }

        GlassCard {
            id: packCard
            objectName: "officialPackCard"
            Layout.fillWidth: true
            Layout.preferredWidth: 2
            Layout.fillHeight: true
            visible: root.pack.state !== "missing"
            // Estrecha (dos columnas): los botones pasan debajo del texto. Con Inter el texto y los dos botones
            // no caben en menos de ~600 px.
            readonly property bool stacked: width < 600
            implicitHeight: stacked ? 150 : 84
            padding: 16

            component PackButtons: RowLayout {
                spacing: 8
                ActionButton {
                    text: qsTr("Detalles")
                    onClicked: root.navigate("official")
                }
                ActionButton {
                    visible: root.pack.state === "ready" || root.pack.state === "update" || root.pack.state === "download"
                    text: root.pack.state === "update" ? qsTr("Actualizar a v%1").arg(root.pack.version)
                        : root.pack.state === "download" ? qsTr("Descargar e instalar")
                        : root.pack.version ? qsTr("Instalar v%1").arg(root.pack.version) : qsTr("Instalar")
                    glyph: root.pack.state === "update" ? "refresh" : "download"
                    primary: true
                    enabled: !launcher.gameBusy && !launcher.catalogBusy && !launcher.copyingInstance && !root.pack.loading
                    onClicked: launcher.installOfficialPack()
                }
            }

            ColumnLayout {
                anchors.fill: parent
                spacing: 10
                RowLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: 14
                    // Icono en cristal oscuro con tinte de acento.
                    Rectangle {
                        Layout.preferredWidth: 48; Layout.preferredHeight: 48
                        radius: Theme.radiusMd
                        gradient: Gradient {
                            GradientStop { position: 0; color: Qt.alpha(Theme.accent, 0.20) }
                            GradientStop { position: 1; color: Qt.alpha(Theme.base, 0.5) }
                        }
                        border.color: Qt.alpha(Theme.accentSoft, 0.25)
                        Rectangle {
                            y: 1; x: parent.radius; height: 1
                            width: Math.max(0, parent.width - parent.radius * 2)
                            color: Theme.glassHighlight
                        }
                        Glyph { anchors.centerIn: parent; width: 20; height: 20; name: "star"; color: Theme.accentSoft }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            text: qsTr("Modpack oficial")
                            color: Qt.alpha(Theme.ink, 0.5)
                            font.family: Theme.fontFamily
                            font.pixelSize: 11
                            font.weight: Font.Medium
                        }
                        Text {
                            Layout.fillWidth: true
                            text: root.pack.name || qsTr("Modpack oficial")
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            color: Theme.ink
                            font.family: Theme.displayFamily
                            font.pixelSize: 16
                            font.weight: Font.Bold
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            Rectangle {
                                visible: root.pack.state === "installed"
                                Layout.preferredWidth: 6; Layout.preferredHeight: 6
                                radius: 3
                                color: Theme.success
                            }
                            Text {
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                color: root.pack.state === "update" ? Theme.accentSoft : Qt.alpha(Theme.ink, 0.6)
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                                text: root.pack.state === "update" ? qsTr("Hay una versión nueva: v%1").arg(root.pack.version)
                                    : root.pack.state === "installed" ? (root.pack.installedVersion ? qsTr("Instalado · v%1").arg(root.pack.installedVersion) : qsTr("Instalado"))
                                    : root.pack.minecraft ? (root.pack.fabric ? qsTr("Minecraft %1 · Fabric").arg(root.pack.minecraft) : qsTr("Minecraft %1").arg(root.pack.minecraft))
                                    : root.pack.loading ? qsTr("Leyendo modpack…") : ""
                            }
                        }
                    }
                    PackButtons { visible: !packCard.stacked }
                }
                PackButtons { visible: packCard.stacked }
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Text { Layout.fillWidth: true; text: qsTr("Instancias recientes"); color: Theme.ink; font.family: Theme.displayFamily; font.pixelSize: 16; font.weight: Font.Bold }
        ActionButton { text: qsTr("Ver instancias"); glyph: "arrow"; compact: true; navigation: true; onClicked: root.navigate("instances") }
    }
    GridLayout {
        Layout.fillWidth: true
        columns: Math.min(launcher.instances.length + 1, root.width >= 1350 ? 4 : root.width >= 1000 ? 3 : 2)
        rowSpacing: 12
        columnSpacing: 14
        Repeater {
            model: launcher.instances.slice(0, 3)
            CardButton {
                id: recent
                required property var modelData
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                implicitHeight: 78
                text: modelData.name
                selected: root.current !== null && root.current.id === modelData.id
                onClicked: root.instanceSelected(modelData.id)
                contentItem: RowLayout {
                    spacing: 14
                    Rectangle {
                        Layout.preferredWidth: 44; Layout.preferredHeight: 44
                        radius: Theme.radiusMd
                        color: Theme.crystal ? Theme.fieldFill : Qt.alpha(Theme.base, 0.35)
                        border.color: Theme.glassBorder
                        scale: recent.hovered && Theme.motion ? 1.05 : 1
                        Behavior on scale { MotionNumber { role: "fast" } }
                        InstanceBlock { anchors.centerIn: parent; loader: recent.modelData.category || "vanilla"; fabric: recent.modelData.category === "fabric" }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text { Layout.fillWidth: true; text: recent.text; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 14; font.weight: Font.DemiBold }
                        Text { Layout.fillWidth: true; text: recent.modelData.loaderLabel + " · " + recent.modelData.mcVersion; elide: Text.ElideRight; color: Qt.alpha(Theme.ink, 0.55); font.family: Theme.fontFamily; font.pixelSize: 11; font.features: Theme.tabular }
                    }
                    Glyph {
                        name: "arrow"
                        color: recent.selected || recent.hovered ? Theme.accentSoft : Qt.alpha(Theme.ink, 0.35)
                        Behavior on color { MotionColor { role: "fast" } }
                        Layout.preferredWidth: 16; Layout.preferredHeight: 16
                        transform: Translate {
                            x: recent.hovered && Theme.motion ? 2 : 0 // motion-ok: valor de destino, no duración
                            y: recent.hovered && Theme.motion ? -2 : 0 // motion-ok: valor de destino, no duración
                            Behavior on x { MotionNumber { role: "fast" } }
                            Behavior on y { MotionNumber { role: "fast" } }
                        }
                    }
                }
            }
        }
        CardButton {
            id: create
            Layout.fillWidth: true; Layout.preferredWidth: 1
            Layout.maximumWidth: launcher.instances.length ? Number.POSITIVE_INFINITY : 250
            implicitHeight: 78
            dashed: true
            text: qsTr("Crear instancia")
            enabled: !launcher.gameBusy && !launcher.catalogBusy
            onClicked: root.createRequested()
            contentItem: Item {
                implicitWidth: createRow.implicitWidth
                Row {
                    id: createRow
                    anchors.centerIn: parent
                    spacing: 8
                    Glyph {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 16; height: 16
                        name: "plus"
                        color: create.hovered ? Theme.ink : Theme.accentSoft
                        Behavior on color { MotionColor { role: "fast" } }
                        rotation: create.hovered && Theme.motion ? 90 : 0 // motion-ok: valor de destino, no duración
                        Behavior on rotation { MotionNumber { role: "fast" } }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: create.text
                        textFormat: Text.PlainText
                        color: create.hovered ? Theme.ink : Theme.accentSoft
                        Behavior on color { MotionColor { role: "fast" } }
                        font.family: Theme.fontFamily
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                    }
                }
            }
        }
    }
}
