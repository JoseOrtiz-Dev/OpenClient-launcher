cmake_minimum_required(VERSION 3.24)
project(OpenClientQt VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
find_package(Qt6 6.11.3 REQUIRED COMPONENTS Quick QuickControls2 Concurrent Svg Network LinguistTools)
qt_standard_project_setup(REQUIRES 6.8)

set(REPOSITORY_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../..")
qt_add_executable(openclient-qt WIN32 src/main.cpp src/LauncherController.cpp src/LauncherController.h src/MaintenanceController.cpp src/MaintenanceController.h src/WindowFrame.cpp src/WindowFrame.h src/DiscordPresence.cpp src/DiscordPresence.h src/ContentListModel.cpp src/ContentListModel.h src/InstanceLinks.cpp src/InstanceLinks.h src/InstanceBanners.cpp src/InstanceBanners.h)
target_include_directories(openclient-qt PRIVATE src)
set_source_files_properties(qml/Theme.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
qt_add_qml_module(openclient-qt
    URI OpenClient
    SOURCES
        src/SkinPreview.cpp
        src/SkinPreview.h
        src/WardrobeController.cpp
        src/WardrobeController.h
    QML_FILES
        qml/Main.qml
        qml/Theme.qml
        qml/components/Glyph.qml
        qml/components/FrostedBackdrop.qml
        qml/components/ActionButton.qml
        qml/components/PillButton.qml
        qml/components/MotionNumber.qml
        qml/components/MotionColor.qml
        qml/components/GlassCard.qml
        qml/components/Orbs.qml
        qml/components/BannerArt.qml
        qml/components/BannerBackdrop.qml
        qml/components/CrossfadeImage.qml
        qml/components/InstanceBanner.qml
        qml/components/Crystals.qml
        qml/components/GlassField.qml
        qml/components/GlassSwitch.qml
        qml/components/GlassComboBox.qml
        qml/components/ProjectIcon.qml
        qml/components/ContentRow.qml
        qml/components/ContentToolbar.qml
        qml/components/GlassPanel.qml
        qml/components/GlassDialog.qml
        qml/components/GlassMenu.qml
        qml/components/GlassMenuItem.qml
        qml/components/Reveal.qml
        qml/components/CommandsEditor.qml
        qml/components/InstanceAdvanced.qml
        qml/components/SectionHeader.qml
        qml/components/AmbientParticles.qml
        qml/components/CrystalArt.qml
        qml/components/InstanceBlock.qml
        qml/components/StartupIntro.qml
        qml/components/UpdateDialog.qml
        qml/components/WelcomeDialog.qml
        qml/components/ReleaseNotes.qml
        qml/components/AccountDialog.qml
        qml/components/ProjectDialog.qml
        qml/components/OperationBar.qml
        qml/components/ModpackVersionDialog.qml
        qml/components/SnapshotDialog.qml
        qml/components/WorldBackups.qml
        qml/components/PlayButton.qml
        qml/components/PlayButtonGallery.qml
        qml/components/AccountChip.qml
        qml/components/Sidebar.qml
        qml/components/SidebarButton.qml
        qml/pages/HomePage.qml
        qml/pages/HomeContent.qml
        qml/pages/OfficialPage.qml
        qml/pages/InstancesPage.qml
        qml/pages/InstanceDetails.qml
        qml/pages/DownloadsPage.qml
        qml/pages/WardrobePage.qml
        qml/pages/SettingsPage.qml
        qml/pages/CreditsPage.qml
)

# Traducciones: el español está en el código; cada idioma nuevo es un .ts en translations/.
qt_add_translations(openclient-qt
    TS_FILES translations/openclient_en.ts
    RESOURCE_PREFIX "/i18n"
    LUPDATE_OPTIONS -no-obsolete)

set_source_files_properties("${REPOSITORY_ROOT}/src/assets/openclient-cube.svg"
    PROPERTIES QT_RESOURCE_ALIAS "assets/cube.svg")
set_source_files_properties("${REPOSITORY_ROOT}/src/assets/openclient-logo.svg"
    PROPERTIES QT_RESOURCE_ALIAS "assets/openclient-logo.svg")
set_source_files_properties("${REPOSITORY_ROOT}/src/assets/patito.gif"
    PROPERTIES QT_RESOURCE_ALIAS "assets/patito.gif")
set_source_files_properties("${REPOSITORY_ROOT}/src/assets/enderhosting-logo.png"
    PROPERTIES QT_RESOURCE_ALIAS "assets/enderhosting.png")
qt_add_resources(openclient-qt shared_assets PREFIX "/"
    FILES
        "${REPOSITORY_ROOT}/src/assets/openclient-cube.svg"
        "${REPOSITORY_ROOT}/src/assets/openclient-logo.svg"
        "${REPOSITORY_ROOT}/src/assets/patito.gif"
        "${REPOSITORY_ROOT}/src/assets/enderhosting-logo.png")
qt_add_resources(openclient-qt particle_sprites PREFIX "/assets" BASE "resources"
    FILES
        resources/particles/dark-obsidian.svg
        resources/particles/crystal-ruby.svg
        resources/particles/crystal-amethyst.svg
        resources/particles/crystal-obsidian.svg)
qt_add_resources(openclient-qt glass_assets PREFIX "/assets" BASE "resources"
    FILES
        resources/glass/noise.png
        resources/glass/crystal-a.svg
        resources/glass/crystal-b.svg
        resources/glass/crystal-c.svg)
qt_add_resources(openclient-qt ui_fonts PREFIX "/" BASE "resources"
    FILES
        resources/fonts/PlusJakartaSans.ttf
        resources/fonts/Inter.ttf)
foreach(texture moss_block dirt stone)
    set(image "${REPOSITORY_ROOT}/src/assets/block-textures/blocks--${texture}.png")
    set_source_files_properties("${image}" PROPERTIES QT_RESOURCE_ALIAS "assets/blocks/${texture}.png")
    qt_add_resources(openclient-qt "block_${texture}" PREFIX "/" FILES "${image}")
endforeach()
if(WIN32)
    configure_file(resources/windows.rc.in "${CMAKE_CURRENT_BINARY_DIR}/windows.rc" @ONLY)
    target_sources(openclient-qt PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/windows.rc")
endif()
target_link_libraries(openclient-qt PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Concurrent Qt6::Svg Qt6::Network)
if(WIN32)
    target_link_libraries(openclient-qt PRIVATE dwmapi)
endif()
if(MSVC)
    target_compile_options(openclient-qt PRIVATE /W4 /utf-8 /permissive-)
endif()

set(OPENCLIENT_UPDATE_URL "" CACHE STRING "URL HTTPS del manifiesto de actualizaciones. Vacío desactiva la comprobación")
target_compile_definitions(openclient-qt PRIVATE OPENCLIENT_UPDATE_URL="${OPENCLIENT_UPDATE_URL}")
set(OPENCLIENT_BRIDGE "" CACHE FILEPATH "DLL del puente Rust compilada para esta arquitectura")
if(OPENCLIENT_BRIDGE)
    if(NOT EXISTS "${OPENCLIENT_BRIDGE}")
        message(FATAL_ERROR "La biblioteca Rust indicada no existe")
    endif()
    add_custom_command(TARGET openclient-qt POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different "${OPENCLIENT_BRIDGE}" "$<TARGET_FILE_DIR:openclient-qt>")
endif()
set(OPENCLIENT_OFFICIAL_PACK "" CACHE FILEPATH "Modpack .mrpack que se instala junto al launcher como modpack oficial")
if(OPENCLIENT_OFFICIAL_PACK AND NOT EXISTS "${OPENCLIENT_OFFICIAL_PACK}")
    message(FATAL_ERROR "El modpack oficial indicado no existe")
endif()
include(GNUInstallDirs)
install(TARGETS openclient-qt RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
if(OPENCLIENT_OFFICIAL_PACK)
    install(FILES "${OPENCLIENT_OFFICIAL_PACK}" DESTINATION ${CMAKE_INSTALL_BINDIR}/modpack RENAME official.mrpack)
endif()
install(FILES resources/octicons-LICENSE.txt DESTINATION ${CMAKE_INSTALL_DATADIR}/openclient/licenses)
install(FILES resources/licenses/PlusJakartaSans-OFL.txt resources/licenses/Inter-OFL.txt DESTINATION ${CMAKE_INSTALL_DATADIR}/openclient/licenses)
install(FILES ../../../LICENSE DESTINATION ${CMAKE_INSTALL_DATADIR}/openclient/licenses RENAME OpenClient-GPL-3.0.txt)
if(OPENCLIENT_BRIDGE)
    install(FILES "${OPENCLIENT_BRIDGE}" DESTINATION ${CMAKE_INSTALL_BINDIR})
endif()
qt_generate_deploy_qml_app_script(TARGET openclient-qt OUTPUT_SCRIPT deploy_script)
install(SCRIPT ${deploy_script})
if(WIN32)
    # El despliegue normal solo incluye qwindows; la prueba usa el plugin sin ventana.
    install(FILES "${Qt6_DIR}/../../../plugins/platforms/qoffscreen.dll" DESTINATION plugins/platforms)
    # Modrinth sirve sus iconos y capturas en WebP; sin este plugin el catálogo sale sin imágenes.
    set(OPENCLIENT_QWEBP "${Qt6_DIR}/../../../plugins/imageformats/qwebp.dll")
    if(NOT EXISTS "${OPENCLIENT_QWEBP}")
        message(FATAL_ERROR "Falta qwebp.dll en ${OPENCLIENT_QWEBP}. Instala el componente \"Qt Image Formats\" de Qt con el Maintenance Tool.")
    endif()
    install(FILES "${OPENCLIENT_QWEBP}" DESTINATION plugins/imageformats)
endif()

enable_testing()
add_test(NAME native-smoke COMMAND openclient-qt --smoke-test)
set_tests_properties(native-smoke PROPERTIES TIMEOUT 30 ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software;QT_FORCE_STDERR_LOGGING=1")
