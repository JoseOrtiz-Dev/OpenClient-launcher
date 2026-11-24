import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs as NativeDialogs
import QtQuick.Layouts
import OpenClient

ColumnLayout {
    id: root
    objectName: "settingsPage"
    // 0 Juego · 1 General · 2 Cuenta · 3 Almacenamiento · 4 Discord · 5 Privacidad
    property int section: 0
    signal showWelcome()
    signal accountsRequested()
    signal closeRequested()
    // Fuera del StackLayout: dentro contaría como una sección más.
    Reveal { id: sectionReveal; fadeTarget: sections; stagger: 40; duration: 480; distance: 16; maxSlots: 10 } // motion-ok: Reveal: se rehace en la fase 3
    onSectionChanged: {
        if (section === 3) launcher.refreshStorage()
        else if (section === 0 && javaMajor) launcher.checkJava(javaMajor.currentValue)
    }
    onVisibleChanged: if (!visible) searchField.text = ""
    function sizeText(bytes) {
        const value = bytes || 0
        if (value < 1048576) return qsTr("%1 KB").arg(Math.round(value / 1024))
        if (value < 1073741824) return qsTr("%1 MB").arg((value / 1048576).toFixed(1))
        return qsTr("%1 GB").arg((value / 1073741824).toFixed(2))
    }
    // Megabytes a gigabytes sin ceros sobrantes: 4096 → «4», 4352 → «4.25».
    function gbText(mb) { return String(Number(((mb || 0) / 1024).toFixed(2))) }
    // Minúsculas y sin tildes, para que «configuración» y «CONFIGURACION» coincidan.
    function fold(text) { return String(text).toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "") }
    function openResult(target) {
        searchField.text = ""
        root.section = target
    }
    readonly property var gcOptions: [
        { id: "automatic", label: qsTr("Automático (recomendado)") },
        { id: "g1gc", label: qsTr("G1GC equilibrado (baja pausa)") },
        { id: "generational_zgc", label: qsTr("ZGC generacional (Java 21+)") },
        { id: "low_memory", label: qsTr("Ahorro de memoria (G1GC + deduplicación)") }
    ]
    readonly property var categories: [
        { title: qsTr("Juego"), glyph: "play" },
        { title: qsTr("General"), glyph: "settings" },
        { title: qsTr("Cuenta"), glyph: "user" },
        { title: qsTr("Almacenamiento"), glyph: "folder" },
        { title: qsTr("Discord"), glyph: "discord" },
        { title: qsTr("Privacidad"), glyph: "eye" }
    ]
    // Un elemento por ajuste o bloque principal. `keywords` lleva sinónimos; la búsqueda ignora tildes y mayúsculas.
    readonly property var searchIndex: [
        { title: qsTr("Memoria asignada"), section: 0, keywords: qsTr("ram memoria gigas gb deslizador asignacion heap xmx rendimiento") },
        { title: qsTr("Avisar si la RAM asignada es muy alta"), section: 0, keywords: qsTr("ram memoria aviso advertencia alta 75") },
        { title: qsTr("Hardware del equipo"), section: 0, keywords: qsTr("hardware procesador cpu detectar ram instalada") },
        { title: qsTr("Optimización de JVM"), section: 0, keywords: qsTr("jvm java gc recolector basura g1gc zgc rendimiento optimizacion") },
        { title: qsTr("Pre-asignar bloques de RAM"), section: 0, keywords: qsTr("alwayspretouch pretouch memoria reservar tirones jvm") },
        { title: qsTr("Prioridad alta de proceso"), section: 0, keywords: qsTr("prioridad proceso cpu planificador windows rendimiento") },
        { title: qsTr("Argumentos JVM adicionales"), section: 0, keywords: qsTr("argumentos jvm flags parametros opciones java") },
        { title: qsTr("Runtimes de Java"), section: 0, keywords: qsTr("java runtime version adoptium instalar javaw ruta ejecutable") },
        { title: qsTr("Iniciar en pantalla completa"), section: 0, keywords: qsTr("pantalla completa fullscreen ventana juego") },
        { title: qsTr("Resolución de la ventana"), section: 0, keywords: qsTr("ventana resolucion ancho alto tamaño pantalla minecraft") },
        { title: qsTr("Comandos de lanzamiento"), section: 0, keywords: qsTr("comandos lanzamiento pre post cierre wrapper hook script") },
        { title: qsTr("Temas de color"), section: 1, keywords: qsTr("tema color paleta apariencia acento oscuro claro estilo") },
        { title: qsTr("Partículas y animaciones"), section: 1, keywords: qsTr("particulas animaciones efectos movimiento transiciones apariencia") },
        { title: qsTr("Brillo de cursor"), section: 1, keywords: qsTr("brillo cursor resplandor spotlight luz tarjetas raton puntero efecto") },
        { title: qsTr("Modo bajo consumo (Potato PC)"), section: 1, keywords: qsTr("potato bajo consumo rendimiento desenfoque blur ahorro equipo lento") },
        { title: qsTr("Pre-instalar Sodium en instancias Fabric"), section: 1, keywords: qsTr("sodium fabric mods optimizacion fps instancias") },
        { title: qsTr("Inicio y cierre"), section: 1, keywords: qsTr("inicio cierre arranque salir launcher") },
        { title: qsTr("Minimizar al jugar"), section: 1, keywords: qsTr("minimizar jugar ventana launcher inicio") },
        { title: qsTr("Volver al launcher"), section: 1, keywords: qsTr("volver launcher cerrar juego mostrar enfocar") },
        { title: qsTr("Avisar al cerrar con el juego abierto"), section: 1, keywords: qsTr("cerrar aviso juego abierto salir") },
        { title: qsTr("Mostrar el registro si Minecraft falla"), section: 1, keywords: qsTr("registro log crash fallo error cierre inesperado") },
        { title: qsTr("Mostrar snapshots al crear instancias"), section: 1, keywords: qsTr("snapshots versiones prueba mojang instancias") },
        { title: qsTr("Descargas simultáneas"), section: 1, keywords: qsTr("descargas simultaneas conexion velocidad paralelo red") },
        { title: qsTr("Ver la bienvenida otra vez"), section: 1, keywords: qsTr("bienvenida introduccion tutorial inicio") },
        { title: qsTr("Actualizaciones"), section: 1, keywords: qsTr("actualizaciones version canal estable beta buscar actualizar") },
        { title: qsTr("CurseForge"), section: 1, keywords: qsTr("curseforge catalogo contenido proximamente") },
        { title: qsTr("Perfil activo"), section: 2, keywords: qsTr("cuenta perfil microsoft offline usuario sesion skin") },
        { title: qsTr("Gestionar cuentas"), section: 2, keywords: qsTr("gestionar cuentas cambiar perfil iniciar sesion microsoft") },
        { title: qsTr("Carpeta de OpenClient"), section: 3, keywords: qsTr("carpeta ubicacion datos directorio mover ruta abrir") },
        { title: qsTr("Modo portable"), section: 3, keywords: qsTr("portable carpeta data usb llevar") },
        { title: qsTr("Espacio en disco"), section: 3, keywords: qsTr("espacio disco almacenamiento tamaño instancias ocupado") },
        { title: qsTr("Limpieza de archivos"), section: 3, keywords: qsTr("limpiar limpieza vaciar papelera instancias retiradas archivos del juego borrar") },
        { title: qsTr("Mostrar actividad en Discord"), section: 4, keywords: qsTr("discord presencia actividad estado perfil rich presence") },
        { title: qsTr("Mostrar el servidor"), section: 4, keywords: qsTr("discord servidor direccion ip lan privacidad") },
        { title: qsTr("Diagnóstico"), section: 5, keywords: qsTr("diagnostico informe errores reporte log privacidad datos") }
    ]
    readonly property string query: searchField.text.trim()
    readonly property bool searching: query.length > 0
    // Coincidencias cuyo título contiene todas las palabras primero; después las que solo coinciden por sinónimos.
    readonly property var searchResults: {
        if (query.length === 0) return []
        const terms = fold(query).split(/\s+/).filter(term => term.length > 0)
        const inTitle = []
        const inKeywords = []
        for (const entry of searchIndex) {
            const title = fold(entry.title)
            const haystack = title + " " + fold(entry.keywords)
            if (!terms.every(term => haystack.includes(term))) continue
            if (terms.every(term => title.includes(term))) inTitle.push(entry)
            else inKeywords.push(entry)
        }
        return inTitle.concat(inKeywords)
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 0
        Item {
            Layout.preferredWidth: root.width < 860 ? 240 : 280
            Layout.fillHeight: true
            Rectangle {
                anchors.fill: parent
                anchors.margins: 1
                color: Theme.crystal ? Theme.rowFill : Theme.sidebar
                radius: Theme.radiusXl - 1
                // Esquinas derechas rectas: solo la izquierda sigue la curva del diálogo.
                Rectangle { anchors.top: parent.top; anchors.bottom: parent.bottom; anchors.right: parent.right; width: parent.radius; color: parent.color }
                Rectangle { anchors.right: parent.right; height: parent.height; width: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 16
                    spacing: 6
                    GlassField {
                        id: searchField
                        objectName: "settingsSearch"
                        Layout.fillWidth: true
                        Layout.preferredHeight: 42
                        Layout.bottomMargin: 10
                        placeholderText: qsTr("Buscar ajustes…")
                        leadingGlyph: "search"
                        legacyFill: Theme.surface
                        maximumLength: 64
                        Accessible.name: qsTr("Buscar ajustes")
                        Keys.onEscapePressed: event => {
                            if (text.length > 0) {
                                text = ""
                                event.accepted = true
                            } else {
                                event.accepted = false
                            }
                        }
                        trailing: Button {
                            id: clearSearch
                            visible: searchField.text.length > 0
                            width: 28; height: 28
                            hoverEnabled: true
                            Accessible.name: qsTr("Borrar búsqueda")
                            onClicked: {
                                searchField.text = ""
                                searchField.forceActiveFocus()
                            }
                            background: Rectangle {
                                radius: Theme.radiusSm
                                color: clearSearch.down ? Theme.edge : clearSearch.hovered ? Qt.alpha(Theme.ink, Theme.light ? 0.06 : 0.08) : "transparent"
                                Behavior on color { MotionColor { role: "fast" } }
                            }
                            contentItem: Item {
                                Glyph { anchors.centerIn: parent; width: 13; height: 13; name: "close"; color: Theme.dim }
                            }
                        }
                    }
                    ColumnLayout {
                        objectName: "settingsCategories"
                        visible: !root.searching
                        Layout.fillWidth: true
                        spacing: 4
                        Repeater {
                            model: root.categories
                            ActionButton {
                                required property var modelData
                                required property int index
                                text: modelData.title
                                glyph: modelData.glyph
                                iconTile: true
                                navigation: true
                                selected: root.section === index
                                Layout.fillWidth: true
                                Layout.preferredHeight: 48
                                onClicked: root.section = index
                            }
                        }
                    }
                    ListView {
                        id: searchList
                        objectName: "settingsSearchResults"
                        visible: root.searching && root.searchResults.length > 0
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        spacing: 2
                        boundsBehavior: Flickable.StopAtBounds
                        model: root.searchResults
                        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                        delegate: Button {
                            id: hit
                            required property var modelData
                            width: ListView.view.width
                            height: 54
                            hoverEnabled: true
                            leftPadding: 12
                            rightPadding: 10
                            Accessible.name: qsTr("%1, %2").arg(modelData.title).arg(root.categories[modelData.section].title)
                            onClicked: root.openResult(modelData.section)
                            background: Rectangle {
                                radius: Theme.radiusMd
                                color: hit.down ? Theme.edge : hit.hovered ? Qt.alpha(Theme.ink, Theme.light ? 0.06 : 0.08) : "transparent"
                                Behavior on color { MotionColor { role: "fast" } }
                                border.width: hit.activeFocus ? 2 : 0
                                border.color: Theme.highlight
                            }
                            contentItem: ColumnLayout {
                                spacing: 1
                                Text {
                                    Layout.fillWidth: true
                                    text: hit.modelData.title
                                    textFormat: Text.PlainText
                                    elide: Text.ElideRight
                                    color: Theme.ink
                                    font.family: Theme.fontFamily; font.pixelSize: 13; font.weight: Font.Medium
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: root.categories[hit.modelData.section].title
                                    textFormat: Text.PlainText
                                    elide: Text.ElideRight
                                    color: Theme.dim
                                    font.family: Theme.fontFamily; font.pixelSize: 11
                                }
                            }
                        }
                    }
                    Text {
                        objectName: "settingsSearchEmpty"
                        visible: root.searching && root.searchResults.length === 0
                        Layout.fillWidth: true
                        Layout.leftMargin: 4
                        Layout.rightMargin: 4
                        Layout.topMargin: 6
                        text: qsTr("Sin resultados para «%1»").arg(root.query)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Theme.dim
                        font.family: Theme.fontFamily
                        font.pixelSize: 13
                    }
                    Item { Layout.fillHeight: true; visible: !(root.searching && root.searchResults.length > 0) }
                    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                    Text { text: qsTr("OpenClient · %1").arg(Qt.application.version); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 11; font.features: Theme.tabular; Layout.leftMargin: 8; Layout.topMargin: 5 }
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                Layout.preferredHeight: 76
                Layout.leftMargin: 28
                Layout.rightMargin: 18
                spacing: 12
                Text {
                    objectName: "settingsTitle"
                    Layout.fillWidth: true
                    text: root.categories[root.section] ? root.categories[root.section].title : ""
                    elide: Text.ElideRight
                    color: Theme.ink
                    font.family: Theme.fontFamily
                    font.pixelSize: 28
                    font.weight: Font.DemiBold
                }
                ActionButton {
                    objectName: "settingsClose"
                    glyph: "close"
                    compact: true
                    ToolTip.text: qsTr("Cerrar ajustes")
                    onClicked: root.closeRequested()
                }
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
            ScrollView {
                id: panel
                Layout.fillWidth: true
                Layout.fillHeight: true
                leftPadding: 28; rightPadding: 28; topPadding: 22; bottomPadding: 20
                clip: true
                contentWidth: availableWidth
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                StackLayout {
                    id: sections
                    width: panel.availableWidth
                    height: children[currentIndex] ? children[currentIndex].implicitHeight : 0
                    currentIndex: root.section
                    onCurrentIndexChanged: {
                        if (panel.contentItem) panel.contentItem.contentY = 0
                        if (Theme.motion) {
                            sections.opacity = 0
                            Qt.callLater(() => sectionReveal.play(sections.children[sections.currentIndex]))
                        }
                    }

                    // 0 · Juego
                    ColumnLayout {
                        spacing: 16
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Text { text: qsTr("Memoria asignada"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                                Text { text: qsTr("Cantidad de RAM que OpenClient reserva para Minecraft al iniciar una partida."); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 13; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            ActionButton { glyph: "chip"; compact: true; enabled: !launcher.hardwareLoading; ToolTip.text: qsTr("Volver a detectar hardware"); onClicked: launcher.refreshHardware() }
                        }
                        Text {
                            text: launcher.hardwareLoading ? qsTr("Consultando el hardware…") : launcher.hardwareError
                            visible: launcher.hardwareLoading || launcher.hardwareError.length > 0
                            color: launcher.hardwareError.length > 0 ? Theme.danger : Theme.dim
                            font.family: Theme.fontFamily
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                        GlassPanel {
                            id: memoryPanel
                            Layout.fillWidth: true
                            implicitHeight: memoryBody.implicitHeight + padding * 2
                            frosted: false
                            ColumnLayout {
                                id: memoryBody
                                anchors.fill: parent
                                spacing: 10
                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 12
                                    Text { text: qsTr("Asignación para Minecraft"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 14; Layout.fillWidth: true }
                                    Rectangle {
                                        objectName: "memoryValueBox"
                                        implicitWidth: Math.max(92, memoryValue.implicitWidth + 28)
                                        implicitHeight: 38
                                        radius: Theme.radiusSm
                                        color: Theme.crystal ? Theme.fieldFill : Theme.base
                                        border.color: Theme.crystal ? Theme.fieldBorder : Theme.border
                                        Text {
                                            id: memoryValue
                                            anchors.centerIn: parent
                                            text: qsTr("%1 GB").arg(root.gbText(launcher.allocatedRamMb))
                                            color: Theme.highlight
                                            font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.Bold; font.features: Theme.tabular
                                        }
                                    }
                                }
                                Slider {
                                    id: memorySlider
                                    objectName: "memorySlider"
                                    Layout.fillWidth: true
                                    from: 1024; to: Math.max(1024, launcher.hardware.installed_ram_mb || 1024)
                                    stepSize: 256; value: launcher.allocatedRamMb
                                    enabled: !launcher.hardwareLoading && (launcher.hardware.installed_ram_mb || 0) >= 1024
                                    // Redondea al paso y nunca pasa de la RAM instalada; setAllocatedRamMb rechaza el resto.
                                    onMoved: launcher.allocatedRamMb = Math.min(Math.round(to), Math.round(value / stepSize) * stepSize)
                                    Accessible.name: "RAM asignada a Minecraft, en megabytes"
                                    palette.highlight: Theme.accent
                                }
                                RowLayout {
                                    Layout.fillWidth: true
                                    Text { text: qsTr("%1 GB").arg(root.gbText(memorySlider.from)); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 11; font.features: Theme.tabular }
                                    Item { Layout.fillWidth: true }
                                    Text { text: qsTr("%1 GB").arg(root.gbText(memorySlider.to)); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 11; font.features: Theme.tabular }
                                }
                                Text {
                                    objectName: "memoryFree"
                                    text: qsTr("Te quedan %1 GB libres para asignar.").arg(root.gbText(Math.max(0, (launcher.hardware.installed_ram_mb || 0) - launcher.allocatedRamMb)))
                                    color: Theme.ink
                                    font.family: Theme.fontFamily; font.pixelSize: 12; font.features: Theme.tabular
                                    wrapMode: Text.WordWrap; Layout.fillWidth: true
                                }
                                Text {
                                    text: qsTr("%1 GB de RAM instalada · %2 GB disponibles ahora").arg(((launcher.hardware.installed_ram_mb || 0) / 1024).toFixed(0)).arg(((launcher.hardware.available_ram_mb || 0) / 1024).toFixed(1))
                                    color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 12; font.features: Theme.tabular
                                    wrapMode: Text.WordWrap; Layout.fillWidth: true
                                }
                            }
                        }
                        Text { text: qsTr("Puedes seleccionar hasta la RAM instalada. Windows y las demás aplicaciones también necesitan memoria."); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        Text {
                            objectName: "memoryWarning"
                            visible: launcher.memoryWarning && (launcher.hardware.installed_ram_mb || 0) > 0
                                && launcher.allocatedRamMb > (launcher.hardware.installed_ram_mb || 0) * 0.75
                            text: qsTr("Asignas más del 75 % de la RAM instalada. Windows y otras aplicaciones pueden quedarse sin memoria y el juego irá peor.")
                            color: Theme.warning; font.family: Theme.fontFamily; font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            Label { text: qsTr("Avisar si la RAM asignada es muy alta"); color: Theme.dim; Layout.fillWidth: true }
                            GlassSwitch { checked: launcher.memoryWarning; onToggled: launcher.memoryWarning = checked; Accessible.name: "Avisar si la RAM asignada es muy alta" }
                        }
                        Text { text: launcher.hardware.cpu_brand || qsTr("Procesador pendiente de detectar"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true }

                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        Label { text: qsTr("Optimización de JVM"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            Label { text: qsTr("Recolector de memoria (GC)"); color: Theme.dim; Layout.preferredWidth: 190 }
                            GlassComboBox {
                                id: gcSelector
                                Layout.fillWidth: true
                                model: root.gcOptions
                                textRole: "label"
                                valueRole: "id"
                                currentIndex: Math.max(0, root.gcOptions.findIndex(item => item.id === launcher.gcPreset))
                                Accessible.name: "Perfil del recolector de basura de Java"
                                onActivated: index => launcher.gcPreset = root.gcOptions[index].id
                            }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Label { text: qsTr("Pre-asignar bloques de RAM (-XX:+AlwaysPreTouch)"); color: Theme.ink }
                                Label { text: qsTr("Reserva la memoria al arrancar Minecraft para reducir tirones durante la partida."); color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.preTouchMemory; onToggled: launcher.preTouchMemory = checked; Accessible.name: "Pre-asignar RAM al iniciar" }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Label { text: qsTr("Prioridad alta de proceso"); color: Theme.ink }
                                Label { text: qsTr("Inicia el proceso de Java con prioridad alta en el planificador de Windows."); color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.highPriority; onToggled: launcher.highPriority = checked; Accessible.name: "Prioridad alta de proceso" }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            Label { text: qsTr("Argumentos JVM adicionales (opcional)"); color: Theme.ink; font.pixelSize: 13 }
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                GlassField {
                                    id: jvmArgsField
                                    Layout.fillWidth: true
                                    placeholderText: qsTr("Ej. -XX:SoftMaxHeapSize=4G")
                                    text: launcher.customJvmArgs
                                    maximumLength: 512
                                    Accessible.name: "Argumentos JVM adicionales"
                                    onEditingFinished: launcher.customJvmArgs = text
                                }
                                ActionButton {
                                    text: qsTr("Deshacer"); compact: true
                                    enabled: jvmArgsField.text !== launcher.customJvmArgs
                                    onClicked: jvmArgsField.text = launcher.customJvmArgs
                                }
                                ActionButton {
                                    text: qsTr("Restablecer"); compact: true
                                    enabled: jvmArgsField.text.length > 0 || launcher.customJvmArgs.length > 0
                                    onClicked: {
                                        jvmArgsField.text = ""
                                        launcher.customJvmArgs = ""
                                    }
                                }
                            }
                            Rectangle {
                                Layout.fillWidth: true
                                implicitHeight: jvmPreviewLabel.implicitHeight + 18
                                radius: Theme.radiusSm
                                color: Theme.crystal ? Theme.fieldFill : Theme.base
                                border.color: Theme.crystal ? Theme.fieldBorder : Theme.border
                                Label {
                                    id: jvmPreviewLabel
                                    anchors.fill: parent
                                    anchors.margins: 9
                                    text: launcher.jvmFlagsPreview
                                    textFormat: Text.PlainText
                                    font.family: "Consolas"
                                    font.pixelSize: 11
                                    color: Theme.highlight
                                    wrapMode: Text.WrapAnywhere
                                }
                            }
                        }

                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        RowLayout {
                            Layout.fillWidth: true
                            Label { text: qsTr("Runtimes de Java"); color: Theme.ink; font.pixelSize: 16; font.weight: Font.DemiBold; Layout.fillWidth: true }
                            Rectangle {
                                visible: launcher.javaStatus.required_major === javaMajor.currentValue
                                radius: Theme.radiusSm
                                color: launcher.javaStatus.found ? Qt.alpha(Theme.success, 0.14) : Qt.alpha(Theme.warning, 0.14)
                                border.color: launcher.javaStatus.found ? Qt.alpha(Theme.success, 0.4) : Qt.alpha(Theme.warning, 0.4)
                                implicitWidth: statusBadge.implicitWidth + 14
                                implicitHeight: 24
                                Text {
                                    id: statusBadge
                                    anchors.centerIn: parent
                                    text: launcher.javaStatus.found
                                        ? qsTr("Java %1 listo").arg(launcher.javaStatus.installed_major || javaMajor.currentValue)
                                        : qsTr("No instalado")
                                    color: launcher.javaStatus.found ? Theme.success : Theme.warning
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 11
                                    font.weight: Font.Medium
                                    font.features: Theme.tabular
                                }
                            }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            GlassComboBox {
                                id: javaMajor
                                model: [8, 16, 17, 21, 25]
                                currentIndex: Math.max(0, model.indexOf(launcher.javaMajor))
                                Accessible.name: "Versión de Java"
                                onActivated: {
                                    launcher.javaMajor = currentValue
                                    javaError.localError = ""
                                    launcher.checkJava(currentValue)
                                }
                            }
                            GlassField {
                                Layout.fillWidth: true
                                readOnly: true
                                selectByMouse: true
                                text: launcher.javaPaths[String(javaMajor.currentValue)]
                                    || (launcher.javaStatus.required_major === javaMajor.currentValue && launcher.javaStatus.java_path ? launcher.javaStatus.java_path : qsTr("Automático (OpenClient / Sistema)"))
                                Accessible.name: "Ruta de Java"
                            }
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: 8
                            ActionButton {
                                text: qsTr("Verificar versión"); glyph: "search"; compact: true
                                enabled: !launcher.javaBusy
                                onClicked: {
                                    javaError.localError = ""
                                    launcher.checkJava(javaMajor.currentValue)
                                }
                            }
                            ActionButton {
                                text: qsTr("Instalar desde Adoptium"); glyph: "arrow"; compact: true; primary: true
                                enabled: !launcher.javaBusy && !launcher.gameBusy
                                onClicked: {
                                    javaError.localError = ""
                                    launcher.installJava(javaMajor.currentValue)
                                }
                            }
                            ActionButton {
                                text: qsTr("Elegir java.exe"); glyph: "folder"; compact: true
                                enabled: !launcher.javaBusy && !launcher.gameBusy
                                onClicked: javaPicker.open()
                            }
                            ActionButton {
                                text: qsTr("Usar automático"); compact: true
                                enabled: !launcher.javaBusy && !!launcher.javaPaths[String(javaMajor.currentValue)]
                                onClicked: {
                                    javaError.localError = ""
                                    launcher.setJavaPath(javaMajor.currentValue, "")
                                }
                            }
                        }
                        ProgressBar {
                            Layout.fillWidth: true
                            visible: launcher.javaBusy
                            indeterminate: true
                        }
                        Label {
                            id: javaError
                            property string localError: ""
                            visible: localError.length > 0 || launcher.javaMessage.length > 0
                            text: localError.length > 0 ? localError : launcher.javaMessage
                            textFormat: Text.PlainText
                            color: localError.length > 0 ? Theme.danger
                                 : (launcher.javaStatus.required_major === javaMajor.currentValue && launcher.javaStatus.found) ? Theme.success : Theme.dim
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                        NativeDialogs.FileDialog {
                            id: javaPicker
                            title: qsTr("Selecciona java.exe o javaw.exe")
                            nameFilters: ["Ejecutable Java (java.exe javaw.exe)"]
                            onAccepted: {
                                if (launcher.setJavaPath(javaMajor.currentValue, selectedFile))
                                    javaError.localError = ""
                                else
                                    javaError.localError = launcher.javaMessage.length ? launcher.javaMessage : qsTr("Selecciona un ejecutable de Java válido y compatible.")
                            }
                        }

                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        Text { text: qsTr("Ventana de Minecraft"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                Label { text: qsTr("Iniciar en pantalla completa"); color: Theme.ink }
                                Label { text: qsTr("Pasa el argumento --fullscreen al iniciar Minecraft."); color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.fullscreen; onToggled: launcher.fullscreen = checked; Accessible.name: "Iniciar en pantalla completa" }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Label { text: qsTr("Resolución de la ventana"); color: Theme.ink }
                                Label { text: qsTr("Ancho × alto, en píxeles."); color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            Rectangle {
                                objectName: "resolutionBox"
                                implicitWidth: resolutionRow.implicitWidth + 20
                                implicitHeight: resolutionRow.implicitHeight + 12
                                radius: Theme.radiusSm
                                color: Theme.crystal ? Theme.fieldFill : Theme.base
                                border.color: Theme.crystal ? Theme.fieldBorder : Theme.border
                                RowLayout {
                                    id: resolutionRow
                                    anchors.centerIn: parent
                                    spacing: 8
                                    SpinBox {
                                        from: 320; to: 7680; stepSize: 16; editable: true
                                        value: launcher.windowWidth
                                        onValueModified: launcher.windowWidth = value
                                        Accessible.name: "Ancho de ventana de Minecraft"
                                    }
                                    Text { text: qsTr("×"); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 16 }
                                    SpinBox {
                                        from: 240; to: 4320; stepSize: 16; editable: true
                                        value: launcher.windowHeight
                                        onValueModified: launcher.windowHeight = value
                                        Accessible.name: "Alto de ventana de Minecraft"
                                    }
                                }
                            }
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: 8
                            Repeater {
                                model: [
                                    { label: qsTr("854 × 480"), w: 854, h: 480 },
                                    { label: qsTr("1280 × 720 (HD)"), w: 1280, h: 720 },
                                    { label: qsTr("1600 × 900"), w: 1600, h: 900 },
                                    { label: qsTr("1920 × 1080 (FHD)"), w: 1920, h: 1080 }
                                ]
                                ActionButton {
                                    required property var modelData
                                    text: modelData.label; compact: true
                                    selected: launcher.windowWidth === modelData.w && launcher.windowHeight === modelData.h
                                    onClicked: {
                                        launcher.windowWidth = modelData.w
                                        launcher.windowHeight = modelData.h
                                    }
                                }
                            }
                        }

                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        Text { text: qsTr("Comandos de lanzamiento"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        Label { text: qsTr("Valen para todas las instancias. Cada instancia puede definir los suyos."); color: Theme.dim; font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        CommandsEditor {
                            id: globalCommands
                            Layout.fillWidth: true
                            allowSave: !launcher.gameBusy
                            pre: launcher.preLaunchHook
                            post: launcher.postExitHook
                            wrapper: launcher.wrapperCommand
                            onAccepted: (pre, post, wrapper) => {
                                const ok = launcher.setLaunchCommands(pre, post, wrapper)
                                globalCommands.feedbackError = !ok
                                globalCommands.feedback = ok ? qsTr("Comandos guardados.") : qsTr("Algún comando no es válido.")
                            }
                        }
                        Item { Layout.fillHeight: true }
                    }

                    // 1 · General
                    ColumnLayout {
                        spacing: 16
                        Text { text: qsTr("Temas de color"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        Text { text: qsTr("Elige la paleta de superficies y acento para OpenClient."); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 13 }
                        Flow {
                            id: themeFlow
                            Layout.fillWidth: true
                            Layout.preferredHeight: childrenRect.height
                            spacing: 12
                            Repeater {
                                model: Theme.themes
                                Rectangle {
                                    required property var modelData
                                    width: Math.floor((themeFlow.width - 24) / 3); height: 124; radius: Theme.radiusMd
                                    color: modelData.base
                                    border.color: launcher.themeId === modelData.id ? modelData.accent : (Theme.crystal ? Theme.glassBorder : Theme.border)
                                    border.width: launcher.themeId === modelData.id ? 2 : 1
                                    ColumnLayout {
                                        anchors.fill: parent; anchors.margins: 12; spacing: 10
                                        Rectangle {
                                            Layout.fillWidth: true; height: 50; radius: Theme.radiusSm
                                            clip: true
                                            gradient: Gradient {
                                                GradientStop { position: 0; color: modelData.crystal ? Qt.tint(modelData.card, modelData.heroA) : Qt.tint(modelData.card, Qt.alpha(modelData.accent, 0.22)) }
                                                GradientStop { position: 1; color: modelData.crystal ? Qt.tint(modelData.card, modelData.heroC) : modelData.card }
                                            }
                                            // Orbes de color detrás del cristal (solo temas Cristal).
                                            Rectangle { visible: modelData.crystal === true; width: 46; height: 46; radius: 23; x: -12; y: -16; color: modelData.crystal ? modelData.orbPrimary : "transparent" }
                                            Rectangle { visible: modelData.crystal === true; width: 38; height: 38; radius: 19; x: parent.width - 26; y: 20; color: modelData.crystal ? modelData.orbSecondary : "transparent" }
                                            Row { anchors.centerIn: parent; spacing: 7
                                                Rectangle {
                                                    width: 20; height: 20; radius: 5
                                                    gradient: Gradient {
                                                        GradientStop { position: 0; color: modelData.crystal ? modelData.btnFrom : modelData.accent }
                                                        GradientStop { position: 1; color: modelData.crystal ? modelData.btnTo : modelData.accent }
                                                    }
                                                }
                                                Column { spacing: 5; y: 3
                                                    Rectangle { width: 64; height: 6; radius: 3; color: modelData.ink; opacity: 0.85 }
                                                    Rectangle { width: 38; height: 5; radius: 3; color: modelData.dim }
                                                }
                                            }
                                        }
                                        RowLayout {
                                            Layout.fillWidth: true
                                            Text { text: modelData.name; color: modelData.ink; font.family: Theme.fontFamily; font.pixelSize: 12; font.weight: Font.DemiBold; Layout.fillWidth: true; elide: Text.ElideRight }
                                            Text { text: "✓"; visible: launcher.themeId === modelData.id; color: modelData.accent; font.pixelSize: 14 }
                                        }
                                    }
                                    Button {
                                        id: themeButton
                                        anchors.fill: parent; text: ""; hoverEnabled: true
                                        background: Rectangle {
                                            radius: Theme.radiusMd
                                            color: "transparent"
                                            border.width: themeButton.activeFocus ? 2 : 1
                                            border.color: themeButton.activeFocus || themeButton.hovered ? modelData.accent : "transparent"
                                        }
                                        Accessible.name: qsTr("Usar tema %1").arg(modelData.name)
                                        onClicked: launcher.themeId = modelData.id
                                    }
                                }
                            }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            Text { text: qsTr("Partículas y animaciones"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 13; Layout.fillWidth: true }
                            GlassSwitch { checked: launcher.particlesEnabled; enabled: !launcher.potatoMode; onToggled: launcher.particlesEnabled = checked; Accessible.name: "Partículas ambientales" }
                        }
                        Text { text: qsTr("Activa las partículas ambientales y las transiciones suaves. Se pausan al minimizar, cambiar de ventana o iniciar Minecraft."); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Text { text: qsTr("Brillo de cursor"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 13 }
                                Text { text: qsTr("Un resplandor suave sigue al cursor sobre las tarjetas."); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.spotlightEnabled; enabled: !launcher.potatoMode && launcher.particlesEnabled; onToggled: launcher.spotlightEnabled = checked; Accessible.name: qsTr("Brillo de cursor") }
                        }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Text { text: qsTr("Modo bajo consumo (Potato PC)"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 13 }
                                Text { text: qsTr("Desactiva el desenfoque por GPU, las partículas y las animaciones de interfaz, y prioriza el perfil de ahorro de memoria."); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.potatoMode; onToggled: launcher.potatoMode = checked; Accessible.name: "Modo bajo consumo Potato PC" }
                        }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Text { text: qsTr("Pre-instalar Sodium en nuevas instancias Fabric"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 13 }
                                Text { text: qsTr("Añade Sodium automáticamente al crear una instancia Fabric desde la biblioteca."); color: Theme.dim; font.family: Theme.fontFamily; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.autoSodium; onToggled: launcher.autoSodium = checked; Accessible.name: "Pre-instalar Sodium en nuevas instancias Fabric" }
                        }

                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        Text { text: qsTr("Inicio y cierre"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                Label { text: qsTr("Minimizar al jugar"); color: Theme.ink }
                                Label { text: qsTr("Minimiza OpenClient cuando se abre Minecraft."); color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.minimizeOnLaunch; onToggled: launcher.minimizeOnLaunch = checked; Accessible.name: "Minimizar al jugar" }
                        }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                Label { text: qsTr("Volver al launcher"); color: Theme.ink }
                                Label { text: qsTr("Vuelve a mostrar OpenClient cuando se cierra el juego."); color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.refocusOnExit; onToggled: launcher.refocusOnExit = checked; Accessible.name: "Volver al launcher" }
                        }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                Label { text: qsTr("Avisar al cerrar con el juego abierto"); color: Theme.ink }
                                Label { text: qsTr("Explica por qué OpenClient no se cierra mientras Minecraft o una copia están en curso."); color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.closeWarning; onToggled: launcher.closeWarning = checked; Accessible.name: "Avisar al cerrar con el juego abierto" }
                        }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                Label { text: qsTr("Mostrar el registro si Minecraft falla"); color: Theme.ink }
                                Label { text: qsTr("Si el juego se cierra con un error, abre sus últimas líneas para copiarlas o revisar la causa."); color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.crashReport; onToggled: launcher.crashReport = checked; Accessible.name: "Mostrar el registro si Minecraft falla" }
                        }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                Label { text: qsTr("Mostrar snapshots al crear instancias"); color: Theme.ink }
                                Label { text: qsTr("Añade las versiones de prueba de Mojang al selector. Pueden romper mundos; usa una instancia aparte."); color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: launcher.showSnapshots; onToggled: launcher.showSnapshots = checked; Accessible.name: "Mostrar snapshots al crear instancias" }
                        }

                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        Text { text: qsTr("Descargas"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        RowLayout {
                            Layout.fillWidth: true
                            Label { text: qsTr("Descargas simultáneas"); color: Theme.ink; Layout.fillWidth: true }
                            Text { text: launcher.downloadConcurrency; color: Theme.highlight; font.family: Theme.fontFamily; font.pixelSize: 18; font.weight: Font.Bold; font.features: Theme.tabular }
                        }
                        Slider {
                            objectName: "downloadConcurrency"
                            Layout.fillWidth: true
                            from: 1; to: 20; stepSize: 1
                            value: launcher.downloadConcurrency
                            onMoved: launcher.downloadConcurrency = Math.round(value)
                            Accessible.name: "Descargas simultáneas"
                            palette.highlight: Theme.accent
                        }
                        Label { text: qsTr("Con una conexión lenta, usa 3 o menos. Se aplica a la próxima descarga."); color: Theme.dim; font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border; Layout.topMargin: 6 }
                        ActionButton { text: qsTr("Ver la bienvenida otra vez"); glyph: "news"; onClicked: root.showWelcome() }

                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border; Layout.topMargin: 6 }
                        Text { text: qsTr("Actualizaciones"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            Label { text: qsTr("Versión %1").arg(maintenance.currentVersion); color: Theme.dim; font.features: Theme.tabular; Layout.fillWidth: true }
                            GlassComboBox {
                                id: channelSelector
                                model: [{ id: "stable", label: qsTr("Estable") }, { id: "beta", label: qsTr("Beta") }]
                                textRole: "label"
                                valueRole: "id"
                                currentIndex: maintenance.updateChannel === "beta" ? 1 : 0
                                Accessible.name: "Canal de actualizaciones"
                                onActivated: maintenance.updateChannel = currentValue
                            }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            visible: maintenance.updatesConfigured && !maintenance.portable
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Label { text: qsTr("Actualizar automáticamente"); color: Theme.ink }
                                Label { text: qsTr("Al abrir OpenClient, si hay una versión nueva se descarga e instala sola y el launcher se reinicia."); color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: maintenance.autoUpdate; onToggled: maintenance.autoUpdate = checked; Accessible.name: "Actualizar automáticamente" }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            ActionButton { text: qsTr("Buscar actualizaciones"); enabled: !maintenance.updateChecking && !maintenance.updateDownloading; onClicked: maintenance.checkForUpdates() }
                            ActionButton { text: qsTr("Descargar e instalar"); primary: true; visible: maintenance.updateInstallable; enabled: !maintenance.updateDownloading; onClicked: maintenance.installUpdate() }
                            ActionButton { text: qsTr("Cancelar descarga"); visible: maintenance.updateDownloading; onClicked: maintenance.cancelUpdateDownload() }
                            ActionButton { text: qsTr("Abrir la descarga"); primary: !maintenance.updateInstallable; visible: maintenance.updateAvailable && !maintenance.updateDownloading; onClicked: maintenance.openUpdate() }
                        }
                        Label {
                            objectName: "updateMessage"
                            visible: maintenance.updateMessage.length > 0
                            text: maintenance.updateMessage
                            textFormat: Text.PlainText
                            color: maintenance.updateAvailable ? Theme.success : Theme.dim
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                        Label { text: qsTr("Antes de instalar, OpenClient comprueba la huella SHA-256 del instalador publicado."); color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        Label {
                            objectName: "settingsCurseforgeStatus"
                            text: qsTr("CurseForge · Próximamente")
                            color: Theme.dim
                            font.pixelSize: 12
                        }
                        Item { Layout.fillHeight: true }
                    }

                    // 2 · Cuenta
                    ColumnLayout {
                        spacing: 16
                        Text { text: qsTr("Perfil activo"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        GlassPanel {
                            id: accountCard
                            objectName: "settingsAccountCard"
                            readonly property bool needsLogin: launcher.usingMicrosoft && launcher.microsoftNeedsLogin
                            readonly property color tone: needsLogin ? Theme.warning : launcher.accountName ? Theme.success : Theme.dim
                            readonly property string statusText: launcher.usingMicrosoft ? (needsLogin ? qsTr("Vuelve a entrar") : qsTr("Microsoft · Java"))
                                : launcher.accountName ? qsTr("Perfil offline") : qsTr("Elige un perfil")
                            readonly property bool hasFace: launcher.accountName.length > 0 && wardrobe.accountFace.length > 0
                            Layout.fillWidth: true
                            implicitHeight: 96
                            accented: true
                            frosted: false
                            RowLayout {
                                anchors.fill: parent
                                spacing: 16
                                Rectangle {
                                    Layout.preferredWidth: 56
                                    Layout.preferredHeight: 56
                                    radius: Theme.radiusMd
                                    color: Theme.crystal ? Theme.fieldFill : Theme.surface
                                    border.width: 1.5
                                    border.color: Qt.alpha(accountCard.tone, 0.75)
                                    clip: true
                                    Image {
                                        anchors.fill: parent
                                        anchors.margins: 6
                                        visible: accountCard.hasFace
                                        source: accountCard.hasFace ? wardrobe.accountFace : ""
                                        smooth: false
                                        fillMode: Image.PreserveAspectFit
                                    }
                                    Glyph { anchors.centerIn: parent; visible: !accountCard.hasFace; name: "user"; width: 26; height: 26; color: Theme.highlight }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Text {
                                        Layout.fillWidth: true
                                        text: launcher.accountName || qsTr("Sin perfil")
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                        color: Theme.ink
                                        font.family: Theme.fontFamily; font.pixelSize: 20; font.weight: Font.DemiBold
                                    }
                                    RowLayout {
                                        spacing: 8
                                        Rectangle { Layout.preferredWidth: 8; Layout.preferredHeight: 8; radius: 4; color: accountCard.tone; Accessible.ignored: true }
                                        Text {
                                            Layout.fillWidth: true
                                            text: accountCard.statusText
                                            elide: Text.ElideRight
                                            color: accountCard.needsLogin ? Theme.warning : Theme.dim
                                            font.family: Theme.fontFamily; font.pixelSize: 13
                                        }
                                    }
                                }
                            }
                        }
                        ActionButton {
                            objectName: "settingsManageAccounts"
                            text: qsTr("Gestionar cuentas"); glyph: "user"; primary: true
                            Layout.alignment: Qt.AlignLeft
                            onClicked: root.accountsRequested()
                        }
                        Item { Layout.fillHeight: true }
                    }

                    // 3 · Almacenamiento
                    ColumnLayout {
                        spacing: 14
                        Text { text: qsTr("Carpeta de OpenClient"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        Label { text: qsTr("Aquí se guardan las instancias, bibliotecas y preferencias de esta versión."); color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        Label {
                            objectName: "portableNotice"
                            visible: maintenance.portable
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            color: Theme.highlight
                            text: qsTr("Modo portable: todo vive en la carpeta «data» junto al programa. Las sesiones de Microsoft están protegidas para este usuario de Windows y habrá que iniciar sesión otra vez en otro equipo.")
                        }
                        GlassField {
                            text: launcher.dataDirectory
                            readOnly: true
                            selectByMouse: true
                            Layout.fillWidth: true
                            Accessible.name: "Carpeta de datos"
                        }
                        ActionButton { text: qsTr("Abrir carpeta"); glyph: "arrow"; onClicked: launcher.openDataDirectory() }
                        NativeDialogs.FolderDialog {
                            id: dataFolderPicker
                            title: qsTr("Elige una carpeta vacía para los datos de OpenClient")
                            onAccepted: maintenance.moveDataDirectory(selectedFolder)
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8
                            ActionButton { text: qsTr("Cambiar carpeta…"); glyph: "folder"; enabled: !maintenance.portable && !maintenance.moving && !maintenance.restartNeeded && !launcher.gameBusy && !launcher.catalogBusy; onClicked: dataFolderPicker.open() }
                            ActionButton {
                                text: qsTr("Usar la predeterminada")
                                enabled: !maintenance.portable && !maintenance.moving && !maintenance.restartNeeded && !launcher.gameBusy && !launcher.catalogBusy && maintenance.dataDirectory !== maintenance.defaultDirectory
                                onClicked: maintenance.useDefaultDirectory()
                            }
                            ActionButton { text: qsTr("Reiniciar ahora"); primary: true; visible: maintenance.restartNeeded; onClicked: maintenance.restart() }
                        }
                        Label {
                            objectName: "locationMessage"
                            visible: maintenance.locationMessage.length > 0
                            text: maintenance.locationMessage
                            textFormat: Text.PlainText
                            color: maintenance.locationFailed ? Theme.danger : Theme.success
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border; Layout.topMargin: 6 }
                        RowLayout {
                            Layout.fillWidth: true
                            Text { text: qsTr("Espacio en disco"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold; Layout.fillWidth: true }
                            ActionButton { glyph: "chip"; compact: true; enabled: !launcher.storageBusy; ToolTip.text: qsTr("Volver a calcular"); onClicked: launcher.refreshStorage() }
                        }
                        Repeater {
                            model: [
                                { label: qsTr("Instancias"), hint: qsTr("Mundos, mods y ajustes. No se limpian desde aquí."), key: "instances", kind: "" },
                                { label: qsTr("Java instalado por OpenClient"), hint: qsTr("Se vuelve a descargar si hace falta."), key: "runtimes", kind: "" },
                                { label: qsTr("Archivos del juego"), hint: qsTr("Librerías, recursos y perfiles de versión. Se descargan de nuevo al jugar."), key: "gameFiles", kind: "game_files" },
                                { label: qsTr("Instancias retiradas"), hint: qsTr("Copias de recuperación. Vaciar es permanente."), key: "trash", kind: "trash" }
                            ]
                            RowLayout {
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 12
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2
                                    Label { text: modelData.label; color: Theme.ink }
                                    Label { text: modelData.hint; color: Theme.dim; font.pixelSize: 11; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                                }
                                Label {
                                    text: launcher.storageBusy && launcher.storageInfo[modelData.key] === undefined ? "…" : root.sizeText(launcher.storageInfo[modelData.key])
                                    color: Theme.highlight; font.features: Theme.tabular; font.weight: Font.DemiBold
                                }
                                ActionButton {
                                    visible: modelData.kind.length > 0
                                    text: modelData.kind === "trash" ? qsTr("Vaciar") : qsTr("Limpiar"); compact: true
                                    enabled: !launcher.storageBusy && !launcher.gameBusy && !launcher.catalogBusy && (launcher.storageInfo[modelData.key] || 0) > 0
                                    onClicked: {
                                        cleanupDialog.kind = modelData.kind
                                        cleanupDialog.open()
                                    }
                                }
                            }
                        }
                        Label {
                            visible: launcher.storageMessage.length > 0
                            text: launcher.storageMessage
                            textFormat: Text.PlainText
                            color: launcher.storageMessage === "Limpieza completada." ? Theme.success : Theme.danger
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                        Item { Layout.fillHeight: true }
                    }

                    // 4 · Discord
                    ColumnLayout {
                        spacing: 16
                        Text { text: qsTr("Presencia"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                Label { text: qsTr("Mostrar actividad en Discord"); color: discord.available ? Theme.ink : Theme.dim }
                                Label {
                                    text: !discord.available ? qsTr("Esta compilación no tiene una aplicación de Discord configurada.")
                                        : !discord.enabled ? qsTr("Tu perfil no muestra nada de OpenClient.")
                                        : discord.connected ? qsTr("Conectado. Tu perfil muestra si estás en el launcher o jugando, con versión y tiempo de partida.")
                                        : qsTr("Se conectará cuando Discord esté abierto en este equipo.")
                                    color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true
                                }
                            }
                            Rectangle {
                                visible: discord.available && discord.enabled
                                width: 8; height: 8; radius: 4
                                color: discord.connected ? Theme.accent : Theme.border
                                Behavior on color { MotionColor { role: "fast" } }
                                Accessible.ignored: true
                            }
                            GlassSwitch {
                                enabled: discord.available
                                checked: discord.available && discord.enabled
                                onToggled: discord.enabled = checked
                                Accessible.name: "Mostrar actividad en Discord"
                            }
                        }
                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.crystal ? Theme.divider : Theme.border }
                        RowLayout {
                            Layout.fillWidth: true
                            enabled: discord.available && discord.enabled
                            opacity: enabled ? 1 : 0.55
                            ColumnLayout {
                                Layout.fillWidth: true
                                Label { text: qsTr("Mostrar el servidor"); color: Theme.ink }
                                Label { text: qsTr("Muestra la dirección del servidor público en el que juegas. Las partidas LAN y las IP privadas nunca se muestran."); color: Theme.dim; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                            }
                            GlassSwitch { checked: discord.showServer; onToggled: discord.showServer = checked; Accessible.name: "Mostrar el servidor en Discord" }
                        }
                        Item { Layout.fillHeight: true }
                    }

                    // 5 · Privacidad
                    ColumnLayout {
                        spacing: 14
                        Text { text: qsTr("Diagnóstico"); color: Theme.ink; font.family: Theme.fontFamily; font.pixelSize: 16; font.weight: Font.DemiBold }
                        Label { text: qsTr("Guarda un archivo con tu versión, tus ajustes y el último registro de Minecraft, sin tu carpeta de usuario ni claves. Se queda en tu equipo: tú decides si lo compartes."); color: Theme.dim; font.pixelSize: 12; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                        NativeDialogs.FileDialog {
                            id: reportPicker
                            title: qsTr("Guardar informe de diagnóstico")
                            fileMode: NativeDialogs.FileDialog.SaveFile
                            nameFilters: ["Texto (*.txt)"]
                            defaultSuffix: "txt"
                            onAccepted: maintenance.exportReport(selectedFile)
                        }
                        ActionButton { text: qsTr("Guardar informe de errores…"); glyph: "news"; onClicked: reportPicker.open() }
                        Label {
                            visible: maintenance.reportMessage.length > 0
                            text: maintenance.reportMessage
                            color: maintenance.reportFailed ? Theme.danger : Theme.success
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                        Item { Layout.fillHeight: true }
                    }
                }
            }
        }
    }
    GlassDialog {
        id: cleanupDialog
        objectName: "cleanupDialog"
        property string kind: ""
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(460, parent ? parent.width - 48 : 460)
        glyph: "close"
        title: kind === "trash" ? qsTr("Vaciar instancias retiradas") : qsTr("Limpiar archivos del juego")
        contentItem: ColumnLayout {
            spacing: 14
            Label {
                text: cleanupDialog.kind === "trash"
                    ? qsTr("Se borrarán de forma permanente las copias de las instancias retiradas, con sus mundos. No se puede deshacer.")
                    : qsTr("Se borrarán librerías, recursos y perfiles de versión. La próxima partida los descargará otra vez y necesitará conexión.")
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                ActionButton { text: qsTr("Cancelar"); onClicked: cleanupDialog.close() }
                ActionButton {
                    text: cleanupDialog.kind === "trash" ? qsTr("Borrar para siempre") : qsTr("Limpiar"); primary: true
                    onClicked: {
                        launcher.clearStorage(cleanupDialog.kind)
                        cleanupDialog.close()
                    }
                }
            }
        }
    }
}
