#include "LauncherController.h"
#include "MaintenanceController.h"
#include "DiscordPresence.h"
#include "InstanceLinks.h"
#include <algorithm>
#include <QEventLoop>
#include <QTimer>
#include "SkinPreview.h"
#include "WardrobeController.h"
#include <QDir>
#include <QFile>
#include <QFontDatabase>
#include <QImage>
#include <QImageReader>
#include <QGuiApplication>
#include <QIcon>
#include <QPainter>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QQuickItem>
#include <QSettings>
#include <QStandardPaths>
#include <QTranslator>
#include <QLocale>
#include <QTemporaryDir>
#include <QTimer>
#include <cstdio>

static bool previewTexturesValid() {
    SkinPreview preview;
    preview.setSize(QSizeF(320, 320));
    QImage cape(64, 32, QImage::Format_ARGB32);
    cape.fill(Qt::transparent);
    {
        QPainter texture(&cape);
        texture.fillRect(QRect(1, 1, 5, 8), Qt::red);
        texture.fillRect(QRect(6, 1, 5, 8), Qt::green);
        texture.fillRect(QRect(1, 9, 10, 8), Qt::yellow);
        texture.fillRect(QRect(12, 1, 10, 16), Qt::blue);
    }
    const auto render = [&preview] {
        QImage frame(320, 320, QImage::Format_ARGB32_Premultiplied);
        frame.fill(Qt::transparent);
        QPainter painter(&frame);
        preview.paint(&painter);
        painter.end();
        return frame;
    };
    preview.setView(true);
    preview.orbit(32.0 / 0.6, -21.0 / 0.6);
    for (const auto &texture : {cape, cape.scaled(128, 64)}) {
        preview.setCape(texture);
        const auto frame = render();
        const auto left = frame.pixelColor(138, 132);
        const auto right = frame.pixelColor(182, 132);
        const auto bottom = frame.pixelColor(160, 190);
        if (left.red() <= 150 || left.green() != 0 || left.blue() != 0
            || right.green() <= 150 || right.red() != 0 || right.blue() != 0
            || bottom.red() <= 150 || bottom.green() <= 150 || bottom.blue() != 0) {
            std::fprintf(stderr, "Wardrobe preview: incorrect outer cape texture or orientation (%s, %s, %s)\n",
                qPrintable(left.name()), qPrintable(right.name()), qPrintable(bottom.name()));
            return false;
        }
    }
    preview.setView(false);
    preview.setCapeVisible(false);
    QImage skin(64, 64, QImage::Format_ARGB32);
    skin.fill(Qt::transparent);
    preview.setSkin(skin);
    const auto base = render();
    {
        QPainter texture(&skin);
        texture.fillRect(QRect(32, 0, 32, 16), Qt::magenta);
    }
    preview.setSkin(skin);
    const auto shell = render();
    for (int y = 0; y < 110; ++y)
        for (int x = 0; x < shell.width(); ++x)
            if (qAlpha(base.pixel(x, y)) == 0 && qAlpha(shell.pixel(x, y)) > 0) return true;
    std::fprintf(stderr, "Wardrobe preview: outer skin layer did not extend the head silhouette\n");
    return false;
}

int main(int argc, char *argv[]) {
    QGuiApplication app(argc, argv);
    QCoreApplication::setOrganizationName("OpenClient");
    QCoreApplication::setApplicationName("OpenClientQtPreview");
    QCoreApplication::setApplicationVersion("0.1.0-preview.27");
    // Idioma de la interfaz: el español es el texto fuente; las demás lenguas salen de :/i18n/openclient_<idioma>.qm.
    // OPENCLIENT_LANG (p. ej. "en") fuerza un idioma; si no, se usa el del sistema cuando hay traducción.
    QTranslator translator;
    const bool smokeRun = app.arguments().contains("--smoke-test");
    if (!smokeRun || !qEnvironmentVariable("OPENCLIENT_LANG").isEmpty()) {
        const auto forced = qEnvironmentVariable("OPENCLIENT_LANG");
        const QLocale locale = forced.isEmpty() ? QLocale::system() : QLocale(forced);
        if (locale.language() != QLocale::Spanish && translator.load(locale, "openclient", "_", ":/i18n"))
            QCoreApplication::installTranslator(&translator);
    }
    const QIcon appIcon(":/assets/cube.svg");
    QGuiApplication::setWindowIcon(appIcon);
    QQuickStyle::setStyle("Basic");
    const bool smoke = app.arguments().contains("--smoke-test");
    // Tipografías incluidas (licencia OFL, ver resources/licenses): se registran antes de crear el motor QML.
    // Theme.fontFamily y Theme.displayFamily dependen de estos nombres de familia.
    const auto registerFont = [](const char *file, const QString &family) {
        const int id = QFontDatabase::addApplicationFont(QString::fromLatin1(file));
        return id >= 0 && QFontDatabase::applicationFontFamilies(id).contains(family);
    };
    const bool interLoaded = registerFont(":/fonts/Inter.ttf", "Inter");
    const bool jakartaLoaded = registerFont(":/fonts/PlusJakartaSans.ttf", "Plus Jakarta Sans");
    if (!interLoaded || !jakartaLoaded) {
        std::fprintf(stderr, "Tipografías: Inter=%d Plus Jakarta Sans=%d (recursos :/fonts/*.ttf)\n", interLoaded, jakartaLoaded);
        if (smoke) return 38;
    }
    QTemporaryDir temporary;
    const auto data = smoke ? temporary.path() : MaintenanceController::resolveDataDirectory();
    if (data.isEmpty() || !QDir().mkpath(data)) return 2;
    // Modo portable: los ajustes de Qt (p. ej. la barra fijada) también van dentro de la carpeta de datos, no al registro.
    if (!smoke && !MaintenanceController::portableDirectory(QCoreApplication::applicationDirPath()).isEmpty()) {
        QSettings::setDefaultFormat(QSettings::IniFormat);
        QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, QDir(data).filePath("qt"));
    }
    const auto preferences = QDir(data).filePath("preview.ini");
    if (smoke) {
        QSettings oldPrefs(preferences, QSettings::IniFormat);
        oldPrefs.setValue("catalog/curseforge_api_key", "fixture-legacy-secret");
        oldPrefs.sync();
    }
    LauncherController launcher(preferences);
    MaintenanceController maintenance(&launcher, data, smoke ? QDir(data).filePath("location.ini") : MaintenanceController::bootstrapFile(),
                                      QStringLiteral(OPENCLIENT_UPDATE_URL));
    if (smoke && QSettings(preferences, QSettings::IniFormat).contains("catalog/curseforge_api_key")) return 26;
    WardrobeController wardrobe(data);
    DiscordPresence discord(&launcher, preferences, smoke ? QString() : DiscordPresence::defaultClientId(), launcher.linkUrl(1));
    launcher.reloadLibrary();
    QString lastAccountKey;
    if (!smoke) {
        // Carga la skin y capas de la cuenta activa al abrir y cuando cambie de perfil.
        const auto syncWardrobe = [&launcher, &wardrobe, &lastAccountKey] {
            const auto profile = launcher.usingMicrosoft() ? launcher.microsoftProfile() : QVariantMap{};
            const auto key = launcher.accountName() + '|' + profile.value("skinUrl").toString() + '|'
                + profile.value("capeId").toString() + '|' + QString::number(profile.value("capes").toList().size());
            if (key == lastAccountKey) return;
            lastAccountKey = key;
            wardrobe.syncAccount(launcher.accountName(), profile);
        };
        QObject::connect(&launcher, &LauncherController::accountChanged, &wardrobe, syncWardrobe);
        syncWardrobe();
        if (launcher.usingMicrosoft()) launcher.refreshMicrosoftProfile();
    }
    QQmlApplicationEngine engine;
    engine.setInitialProperties({{"interactiveStartup", !smoke}});
    engine.rootContext()->setContextProperty("launcher", &launcher);
    engine.rootContext()->setContextProperty("maintenance", &maintenance);
    engine.rootContext()->setContextProperty("wardrobe", &wardrobe);
    engine.rootContext()->setContextProperty("discord", &discord);
    bool qmlWarning = false;
    QObject::connect(&engine, &QQmlEngine::warnings, &app, [&qmlWarning](const QList<QQmlError> &) { qmlWarning = true; });
    engine.loadFromModule("OpenClient", "Main");
    if (engine.rootObjects().isEmpty()) return 3;
    if (smoke) {
        if (!previewTexturesValid()) return 18;
        for (const auto *id : {"crystal-ruby", "crystal-amethyst", "crystal-obsidian", "dark-obsidian"}) {
            QFile sprite(QStringLiteral(":/assets/particles/%1.svg").arg(QLatin1String(id)));
            if (!sprite.exists() || sprite.size() == 0) { std::fprintf(stderr, "Partículas: falta el sprite de %s\n", id); return 15; }
        }
        auto *account = engine.rootObjects().first()->findChild<QObject *>("mainAccountDialog");
        if (!account || !QMetaObject::invokeMethod(account, "open") ||
            !account->property("visible").toBool() || !QMetaObject::invokeMethod(account, "close")) return 19;
        if (appIcon.pixmap(32, 32).isNull() || QImage(":/assets/openclient-logo.svg").isNull()) return 16;
        const auto testSkin = QDir(data).filePath("Prueba de skin.png");
        QImage texture(64, 64, QImage::Format_ARGB32);
        texture.fill(Qt::transparent);
        {
            QPainter atlas(&texture);
            atlas.fillRect(0, 0, 32, 16, QColor("#d0a484"));
            atlas.fillRect(0, 0, 32, 8, QColor("#6a3428"));
            atlas.fillRect(38, 8, 2, 5, QColor("#6a3428"));
            atlas.fillRect(9, 11, 2, 1, QColor("#453329"));
            atlas.fillRect(13, 11, 2, 1, QColor("#453329"));
            atlas.fillRect(16, 16, 48, 16, QColor("#466d57"));
            atlas.fillRect(32, 48, 16, 16, QColor("#466d57"));
            atlas.fillRect(0, 16, 16, 16, QColor("#344457"));
            atlas.fillRect(16, 48, 16, 16, QColor("#344457"));
        }
        if (!wardrobe.collection().isEmpty() || !texture.save(testSkin) ||
            !wardrobe.importPng(QUrl::fromLocalFile(testSkin)) || !wardrobe.hasSkin() ||
            wardrobe.collection().size() != 1 || wardrobe.savedSkinId().isEmpty() || !QFile::remove(testSkin)) return 18;
        const auto importedId = wardrobe.savedSkinId();
        wardrobe.setSlim(true);
        if (!wardrobe.saveSkin() || wardrobe.collection().size() != 1 ||
            wardrobe.savedSkinId() != importedId) return 18;
        {
            WardrobeController reopened(data);
            const auto entry = reopened.collection().value(0).toMap();
            const auto skinId = entry.value("id").toString();
            if (entry.value("name") != "Prueba de skin" || !reopened.openSkin(skinId) ||
                !reopened.slim() || !reopened.hasSkin() || reopened.removeSkin("../" + skinId) ||
                !reopened.removeSkin(skinId) || !reopened.collection().isEmpty() || !wardrobe.saveSkin()) return 18;
        }
        QImage invalidSkin(256, 256, QImage::Format_ARGB32);
        invalidSkin.fill(Qt::red);
        const auto invalidPath = QDir(data).filePath("No es una skin.png");
        if (!invalidSkin.save(invalidPath) || wardrobe.importPng(QUrl::fromLocalFile(invalidPath)) ||
            !wardrobe.hasSkin() || !wardrobe.slim() || wardrobe.collection().size() != 1) return 18;
        const auto exportedSkin = QDir(data).filePath("Exportada.png");
        if (!wardrobe.exportPng(QUrl::fromLocalFile(exportedSkin)) || QImage(exportedSkin).size() != QSize(64, 64))
            return 18;
        if (!launcher.setupPending() || !launcher.releasePending() || !launcher.completeSetup()
            || !launcher.acknowledgeRelease()) return 17;
        {
            LauncherController restored(QDir(data).filePath("preview.ini"));
            if (restored.setupPending() || restored.releasePending()) return 17;
        }
        launcher.setMinimizeOnLaunch(true);
        launcher.setRefocusOnExit(true);
        launcher.setWindowWidth(1280);
        launcher.setWindowHeight(720);
        launcher.setFullscreen(true);
        launcher.setCustomJvmArgs("-XX:+UseStringDeduplication");
        launcher.setDownloadConcurrency(99);
        if (launcher.downloadConcurrency() != 20) return 10;
        launcher.setDownloadConcurrency(5);
        launcher.setJavaMajor(99);
        launcher.setJavaMajor(17);
        launcher.setCloseWarning(false);
        launcher.setMemoryWarning(false);
        launcher.setShowSnapshots(true);
        launcher.setCrashReport(false);
        launcher.setCompactLibrary(true);
        if (launcher.setLaunchCommands(QString("pre") + QChar(7) + ".exe", "", "")) return 10;
        if (!launcher.setLaunchCommands("  \"C:\\Scripts\\copia.exe\" --rapido ", "otro.exe", "")) return 10;
        if (launcher.setJavaPath(21, QUrl::fromLocalFile(QDir(data).filePath("missing.exe")))) return 10;
        {
            LauncherController restored(QDir(data).filePath("preview.ini"));
            if (!restored.minimizeOnLaunch() || !restored.refocusOnExit()
                || restored.windowWidth() != 1280 || restored.windowHeight() != 720
                || !restored.fullscreen()
                || restored.downloadConcurrency() != 5 || restored.javaMajor() != 17
                || restored.closeWarning() || restored.memoryWarning()
                || restored.preLaunchHook() != "\"C:\\Scripts\\copia.exe\" --rapido" || restored.postExitHook() != "otro.exe"
                || !restored.wrapperCommand().isEmpty()
                || !restored.showSnapshots() || restored.crashReport() || !restored.compactLibrary()
                || !restored.jvmFlagsPreview().contains("-XX:+UseStringDeduplication")) return 10;
        }
        {
            using D = DiscordPresence;
            D::Session session;
            if (!D::readLogLine("[12:00:01] [Render thread/INFO]: Connecting to Play.Example.NET, 25565", session)
                || D::describe(session, true) != "En play.example.net" || D::describe(session, false) != "En multijugador"
                || D::readLogLine("[12:00:02] [Render thread/INFO]: Connecting to play.example.net, 25565", session)) return 32;
            D::readLogLine("[Render thread/INFO]: Connecting to 192.168.1.20, 25565", session);
            if (D::describe(session, true) != "En una partida LAN") return 32;
            D::readLogLine("[Render thread/INFO]: Connecting to evil<b>host, 25565", session);
            if (D::describe(session, true) != "En multijugador") return 32;
            if (!D::readLogLine("[Server thread/INFO]: Starting integrated minecraft server version 1.21.4", session)
                || D::describe(session, true) != "Un jugador"
                || !D::readLogLine("[Server thread/INFO]: Stopping server", session) || D::describe(session, true) != "En el menú del juego") return 32;
            const auto packet = D::frame(1, {{"cmd", "SET_ACTIVITY"}});
            if (packet.size() != 8 + 22 || packet.at(0) != 1 || packet.at(4) != 22) return 32;
            if (discord.available() || discord.connected()) return 32;
        }
        {
            using M = MaintenanceController;
            if (M::compareVersions("0.1.0-preview.27", "0.1.0-preview.26") <= 0 || M::compareVersions("0.1.0", "0.1.0-preview.27") <= 0
                || M::compareVersions("0.2.0-preview.1", "0.1.9") <= 0 || M::compareVersions("0.1.0-preview.9", "0.1.0-preview.27") >= 0
                || M::compareVersions("1.0", "1.0.0") != 0) return 30;
            const QByteArray manifest = R"({"channels":{"stable":{"version":"0.1.0","url":"https://example.org/s"},
                "beta":{"version":"0.2.0-preview.3","url":"https://example.org/b","notes":"n"}}})";
            if (M::parseManifest(manifest, "stable", "0.1.0-preview.27").value("version").toString() != "0.1.0") return 30;
            if (M::parseManifest(manifest, "beta", "0.1.0").value("version").toString() != "0.2.0-preview.3"
                || !M::parseManifest(manifest, "beta", "0.1.0").value("available").toBool()
                || M::parseManifest(manifest, "stable", "0.1.0").value("available").toBool()) return 30;
            if (M::parseManifest(R"({"channels":{"stable":{"version":"9.9","url":"http://example.org/x"}}})", "stable", "0.1").value("ok").toBool()
                || M::parseManifest("no es json", "stable", "0.1").value("ok").toBool()) return 30;
            // sha256 y tamaño: válidos se conservan; con una huella mal formada o sin tamaño no se ofrece instalar.
            const auto verified = M::parseManifest(R"({"channels":{"stable":{"version":"0.2.0","url":"https://github.com/x/y/a.exe",
                "sha256":"e2b982562d543fd12b713679223229c2306b1bf8e7161c6dd4c892963ef97d43","size":1234}}})", "stable", "0.1.0");
            if (verified.value("sha256").toString().size() != 64 || verified.value("size").toLongLong() != 1234) return 30;
            if (!M::parseManifest(R"({"channels":{"stable":{"version":"0.2.0","url":"https://x.org/a","sha256":"zz","size":5}}})",
                                  "stable", "0.1.0").value("sha256").toString().isEmpty()
                || M::parseManifest(R"({"channels":{"stable":{"version":"0.2.0","url":"https://x.org/a",
                "sha256":"e2b982562d543fd12b713679223229c2306b1bf8e7161c6dd4c892963ef97d43"}}})", "stable", "0.1.0").value("size").toLongLong() != 0)
                return 30;
            const auto hidden = M::redact("C:\\Users\\Ana\\AppData en /Users/Ana accessToken=abc123 y eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlMTIz", "C:\\Users\\Ana");
            if (hidden.contains("Ana\\") || hidden.contains("abc123") || hidden.contains("eyJhbGci") || !hidden.contains("%USERPROFILE%")) return 30;
            const auto source = QDir(data).filePath("origen-movimiento");
            const auto target = QDir(data).filePath("destino-movimiento");
            QDir().mkpath(source + "/instances/a");
            { QFile f(source + "/instances/a/mundo.dat"); if (!f.open(QIODevice::WriteOnly)) return 30; }
            { QFile f(source + "/preview.ini"); if (!f.open(QIODevice::WriteOnly) || f.write("tema") < 0) return 30; }
            { QFile f(source + "/library.lock"); if (!f.open(QIODevice::WriteOnly)) return 30; }
            if (!M::validateTarget(source, source).size() || !M::validateTarget(source, source + "/dentro").size()
                || !M::validateTarget(source, data).size() || !M::validateTarget(source, "relativa").size()
                || !M::validateTarget(source, source + "/instances").size()) return 30;
            if (!M::validateTarget(data, source).size()) return 30;
            if (!M::validateTarget(source, target).isEmpty() || !M::copyTree(source, target).isEmpty()) return 30;
            if (!QFile::exists(target + "/instances/a/mundo.dat") || !QFile::exists(target + "/preview.ini") || QFile::exists(target + "/library.lock")) return 30;
            if (M::validateTarget(source, target).isEmpty()) return 30;
            const auto pointer = QDir(data).filePath("puntero.ini");
            if (!M::writeLocation(pointer, target) || QSettings(pointer, QSettings::IniFormat).value("location/data_directory").toString() != target
                || !M::writeLocation(pointer, QString()) || QSettings(pointer, QSettings::IniFormat).contains("location/data_directory")) return 30;
            // Modo portable: solo con portable.txt, y la carpeta `data` se crea junto al programa.
            {
                QTemporaryDir app;
                if (!app.isValid() || !M::portableDirectory(app.path()).isEmpty()) return 34;
                { QFile f(app.path() + "/portable.txt"); if (!f.open(QIODevice::WriteOnly)) return 34; }
                const auto portableData = M::portableDirectory(app.path());
                if (portableData != QDir::cleanPath(QFileInfo(app.path() + "/data").canonicalFilePath()) || !QFileInfo(portableData).isDir()) return 34;
            }
            const auto report = maintenance.buildReport();
            if (!report.contains("Versión:") || !report.contains("RAM global:") || report.contains("fixture-legacy-secret")) return 30;
            maintenance.checkForUpdates();
            if (maintenance.updatesConfigured() ? false : maintenance.updateMessage().isEmpty()) return 30;
            QTemporaryDir destination;
            const auto moved = QDir(destination.path()).filePath("datos-nuevos");
            maintenance.moveDataDirectory(QUrl::fromLocalFile(QDir(data).filePath("dentro-de-datos")));
            if (!maintenance.locationFailed() || maintenance.moving()) return 31;
            QEventLoop wait;
            QTimer limit;
            limit.setSingleShot(true);
            QObject::connect(&limit, &QTimer::timeout, &wait, &QEventLoop::quit);
            QObject::connect(&maintenance, &MaintenanceController::locationChanged, &wait, [&] { if (!maintenance.moving()) wait.quit(); });
            maintenance.moveDataDirectory(QUrl::fromLocalFile(moved));
            if (!maintenance.moving()) return 31;
            limit.start(20000);
            wait.exec();
            if (maintenance.moving() || maintenance.locationFailed() || !maintenance.restartNeeded() || maintenance.pendingDirectory() != QDir::cleanPath(moved)) return 31;
            if (!QFile::exists(moved + "/preview.ini") || QFile::exists(moved + "/library.lock")
                || QSettings(QDir(data).filePath("location.ini"), QSettings::IniFormat).value("location/data_directory").toString() != QDir::cleanPath(moved)) return 31;
            maintenance.moveDataDirectory(QUrl::fromLocalFile(QDir(destination.path()).filePath("otra")));
            if (!maintenance.locationFailed() || QFileInfo::exists(QDir(destination.path()).filePath("otra"))) return 31;
        }
        if (launcher.setOfflineAccount("nombre no válido") ||
            !launcher.setOfflineAccount("PruebaLocal") || launcher.accountName() != "PruebaLocal") return 8;
        auto *offlineInput = account->findChild<QObject *>("offlineProfileName");
        if (!offlineInput || !offlineInput->setProperty("text", "nombre no válido") ||
            !QMetaObject::invokeMethod(account, "saveOffline") || launcher.accountName() != "PruebaLocal") return 8;
        offlineInput->setProperty("text", "Prueba_UI");
        if (!QMetaObject::invokeMethod(account, "saveOffline") || launcher.accountName() != "Prueba_UI" ||
            account->property("offlineFeedback").toString().isEmpty() ||
            !launcher.setOfflineAccount("PruebaLocal")) return 8;
        {
            LauncherController reloaded(QDir(data).filePath("preview.ini"));
            if (reloaded.accountName() != "PruebaLocal") return 8;
        }
        // Cuenta propia por instancia: solo con perfiles que existen, persiste y se limpia al quitar el perfil.
        if (launcher.setInstanceAccount("inst-x", "offline", "NoExiste") || launcher.setInstanceAccount("inst-x", "microsoft", "nada") ||
            !launcher.setInstanceAccount("inst-x", "offline", "Prueba_UI") ||
            launcher.instanceAccounts().value("inst-x").toMap().value("key").toString() != "Prueba_UI") return 35;
        {
            LauncherController reloaded(QDir(data).filePath("preview.ini"));
            if (reloaded.instanceAccounts().value("inst-x").toMap().value("key").toString() != "Prueba_UI") return 35;
        }
        launcher.removeOfflineProfile("Prueba_UI");
        if (launcher.instanceAccounts().contains("inst-x") || launcher.offlineProfiles().contains("Prueba_UI")) return 35;
        if (!launcher.setOfflineAccount("PruebaLocal") || !launcher.setInstanceAccount("inst-x", "offline", "PruebaLocal") ||
            !launcher.setInstanceAccount("inst-x", "", "") || launcher.instanceAccounts().contains("inst-x")) return 35;
        // Los iconos de Modrinth son WebP: sin el plugin qwebp el catálogo se ve sin imágenes.
        if (!QImageReader::supportedImageFormats().contains("webp")) {
            std::fprintf(stderr, "Falta el plugin de imagen WebP (qwebp): los iconos del catálogo no cargarían.\n");
            return 36;
        }
        // Accesos con el nombre de la instancia (junctions): se crean, se renombran, se quitan y no tocan lo del usuario.
        {
            namespace L = InstanceLinks;
            if (L::sanitize("a:b*c") != "abc" || L::sanitize("CON") != "CON_" || L::sanitize("con.txt") != "con_.txt" ||
                L::sanitize(" . ") != "Instancia" || L::sanitize("x. ") != "x" || L::sanitize(QString(80, 'a')).size() != 60) {
                std::fprintf(stderr, "Accesos: sanitize -> [%s] [%s] [%s] [%s] [%s] %d\n", qPrintable(L::sanitize("a:b*c")), qPrintable(L::sanitize("CON")),
                    qPrintable(L::sanitize("con.txt")), qPrintable(L::sanitize(" . ")), qPrintable(L::sanitize("x. ")), int(L::sanitize(QString(80, 'a')).size()));
                return 37;
            }
            QTemporaryDir links;
            if (!links.isValid()) return 37;
            qputenv("OPENCLIENT_LINKS_DIR", links.path().toUtf8());
            const auto savedInstances = launcher.m_instances;
            const QStringList ids{"11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222",
                                  "33333333-3333-4333-8333-333333333333", "44444444-4444-4444-8444-444444444444"};
            const auto real = [&](int i) { return QDir(data).filePath("instances/" + ids.at(i)); };
            const auto library = [&](const QStringList &names) {
                QVariantList list;
                for (int i = 0; i < names.size(); ++i) list.append(QVariantMap{{"id", ids.at(i)}, {"name", names.at(i)}});
                return list;
            };
            const auto linked = [&](const QString &name, int i) {
                const QFileInfo info(links.filePath(name));
                return (info.isJunction() || info.isSymLink()) && !L::target(info).isEmpty() &&
                    QDir::cleanPath(L::target(info)).compare(QDir::cleanPath(QFileInfo(real(i)).canonicalFilePath()), Qt::CaseInsensitive) == 0;
            };
            const auto gone = [&](const QString &name) { return !QFileInfo(links.filePath(name)).isSymLink() && !QFileInfo(links.filePath(name)).isJunction() && !QFileInfo::exists(links.filePath(name)); };
            bool ok = true, reported = false;
            const auto stage = [&](const char *name) {
                if (!ok && !reported) { reported = true; std::fprintf(stderr, "Accesos: falla en %s (links=%s)\n", name, qPrintable(links.path())); }
            };
            for (int i = 0; i < ids.size(); ++i) ok = QDir().mkpath(real(i)) && ok;
            { QFile f(real(0) + "/conservar.txt"); ok = f.open(QIODevice::WriteOnly) && ok; }
            ok = QDir().mkpath(links.filePath("Carpeta del usuario/dentro")) && ok;
            launcher.m_instances = library({"Mi Mundo", "mi mundo", "a:b*c", "CON"});
            // Un acceso previo que apunta a otra carpeta (o mal formado) debe repararse solo.
            ok = L::createJunction(links.filePath("Mi Mundo"), QDir::toNativeSeparators(real(1))) && ok;
            launcher.syncInstanceLinks();
            launcher.syncInstanceLinks();   // idempotente
            stage("preparacion");
            ok = ok && linked("Mi Mundo", 0) && linked("mi mundo (2)", 1) && linked("abc", 2) && linked("CON_", 3);
            // Entrar de verdad por el acceso: el archivo de la carpeta real debe verse a través de él.
            ok = ok && QFile::exists(links.filePath("Mi Mundo") + "/conservar.txt") && QFileInfo(links.filePath("Mi Mundo")).isDir() &&
                 !QDir(links.filePath("Mi Mundo")).entryList(QDir::Files).isEmpty();
            stage("creacion");
            if (!ok) {
                for (const auto &entry : QDir(links.path()).entryInfoList(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::System | QDir::Hidden)) {
                    std::fprintf(stderr, "  entrada [%s] junction=%d symlink=%d dir=%d destino=[%s]\n", qPrintable(entry.fileName()), entry.isJunction(),
                        entry.isSymLink(), entry.isDir(), qPrintable(L::target(entry)));
                }
                std::fprintf(stderr, "  real0=[%s] canon=[%s]\n", qPrintable(real(0)), qPrintable(QFileInfo(real(0)).canonicalFilePath()));
            }
            launcher.m_instances = library({"Mi Mundo", "Otro", "a:b*c", "CON"});
            launcher.syncInstanceLinks();
            ok = ok && linked("Mi Mundo", 0) && linked("Otro", 1) && gone("mi mundo (2)") && linked("CON_", 3);
            stage("renombrar");
            launcher.m_instances = library({"Mi Mundo", "Otro", "a:b*c"});
            launcher.syncInstanceLinks();
            ok = ok && linked("abc", 2) && gone("CON_") && QFileInfo(real(3)).isDir();
            stage("eliminar");
            ok = ok && QFileInfo(links.filePath("Carpeta del usuario/dentro")).isDir() && QFile::exists(real(0) + "/conservar.txt");
            launcher.m_instances = {};
            launcher.syncInstanceLinks();
            ok = ok && gone("Mi Mundo") && gone("Otro") && gone("abc") && QFileInfo(links.filePath("Carpeta del usuario/dentro")).isDir()
                && QFile::exists(real(0) + "/conservar.txt");
            launcher.m_instances = savedInstances;
            qunsetenv("OPENCLIENT_LINKS_DIR");
            for (int i = 0; i < ids.size(); ++i) QDir(real(i)).removeRecursively();
            stage("final");
            if (!ok) return 37;
        }
        if (!launcher.instances().isEmpty() ||
            !launcher.saveInstance({}, "Prueba", "1.21.4", "fabric")) return 7;
        const auto id = launcher.instances().first().toMap().value("id").toString();
        if (!launcher.saveInstance(id, "Prueba Forge", "1.20.1", "forge") ||
            launcher.instances().first().toMap().value("loaderLabel").toString() != "Forge" ||
            !launcher.saveInstance(id, "Prueba NeoForge", "1.21.4", "neoforge") ||
            launcher.instances().first().toMap().value("loaderLabel").toString() != "NeoForge" ||
            !launcher.saveInstance(id, "Prueba Quilt", "1.21.4", "quilt") ||
            launcher.instances().first().toMap().value("loaderLabel").toString() != "Quilt" ||
            !launcher.saveInstance(id, "Prueba editada", "1.21.4", "vanilla") ||
            !launcher.reloadLibrary() ||
            launcher.instances().first().toMap().value("name").toString() != "Prueba editada" ||
            launcher.saveInstance(id, "Prueba", "../outside", "vanilla") ||
            !launcher.deleteInstance(id) || !launcher.reloadLibrary() ||
            !launcher.instances().isEmpty()) return 7;
        if (!launcher.saveInstance({}, "Prueba visual", "1.21.4", "fabric")) return 7;
        const auto demoId = launcher.instances().first().toMap().value("id").toString();
        if (!launcher.toggleFavorite(demoId) || !launcher.reloadLibrary() ||
            !launcher.instances().first().toMap().value("favorite").toBool()) return 11;
        const auto fixture = QDir(data).filePath("instances/" + demoId);
        if (!QDir().mkpath(fixture + "/saves/Mundo") || !QDir().mkpath(fixture + "/screenshots")) return 11;
        QFile world(fixture + "/saves/Mundo/level.dat");
        if (!world.open(QIODevice::WriteOnly)) return 11;
        world.close();
        // Captura de fixture con paisaje (cielo en degradado, sol, montañas y suelo) para poder juzgar el banner.
        QImage screenshot(1280, 720, QImage::Format_RGB32);
        {
            QPainter paint(&screenshot);
            QLinearGradient sky(0, 0, 0, 720);
            sky.setColorAt(0, QColor(18, 38, 92));
            sky.setColorAt(0.55, QColor(226, 128, 74));
            sky.setColorAt(1, QColor(250, 196, 120));
            paint.fillRect(screenshot.rect(), sky);
            paint.setPen(Qt::NoPen);
            paint.setBrush(QColor(255, 236, 190));
            paint.drawEllipse(QPoint(930, 330), 70, 70);
            paint.setBrush(QColor(86, 62, 96));
            paint.drawPolygon(QPolygon({QPoint(0, 560), QPoint(220, 300), QPoint(430, 520), QPoint(640, 250), QPoint(900, 540), QPoint(1280, 380), QPoint(1280, 720), QPoint(0, 720)}));
            paint.setBrush(QColor(34, 52, 44));
            paint.drawPolygon(QPolygon({QPoint(0, 640), QPoint(300, 520), QPoint(620, 620), QPoint(980, 500), QPoint(1280, 600), QPoint(1280, 720), QPoint(0, 720)}));
        }
        if (!screenshot.save(fixture + "/screenshots/Prueba.png") ||
            launcher.instanceFiles(demoId, "worlds").size() != 1 ||
            launcher.instanceFiles(demoId, "screenshots").size() != 1 ||
            !launcher.instanceFiles(demoId, "../").isEmpty()) return 11;
        // Banners por instancia: captura -> personalizado -> captura (la instancia demo no tiene modpack: sin red).
        {
            const auto bannerFail = [](const char *what) { std::fprintf(stderr, "Banners: %s\n", what); return 39; };
            const auto local = [](const QVariant &url) { return QUrl(url.toString()).toLocalFile(); };
            const auto valid = [&](const QVariantMap &info, const char *source) {
                const auto accent = info.value("accent").toString();
                return info.value("source").toString() == source && QImage(local(info.value("sharp"))).size() == QSize(1280, 400) &&
                    QImage(local(info.value("soft"))).size() == QSize(960, 300) && accent.size() == 7 && accent.startsWith('#') &&
                    QColor::isValidColorName(accent);
            };
            // La interfaz ya pidió el banner de la instancia al crearla (aún sin captura): se vuelve a pedir con la captura en disco.
            launcher.refreshBanners();
            const int revision = launcher.bannerRevision();
            if (launcher.bannerInfo(demoId).value("source").toString() != "generated" ||
                launcher.bannerInfo("../no-existe").value("source").toString() != "generated") return bannerFail("estado inicial");
            if (!launcher.m_banners->waitIdle(20000) || launcher.bannerRevision() <= revision ||
                !valid(launcher.bannerInfo(demoId), "screenshot")) {
                const auto info = launcher.bannerInfo(demoId);
                std::fprintf(stderr, "Banners: captura rev=%d/%d source=%s sharp=%s\n", launcher.bannerRevision(), revision,
                    qPrintable(info.value("source").toString()), qPrintable(info.value("sharp").toString()));
                return bannerFail("captura");
            }
            QTemporaryDir custom;
            QImage picture(640, 360, QImage::Format_RGB32);
            picture.fill(QColor(30, 90, 200));
            const auto picturePath = custom.filePath("banner.png");
            if (!custom.isValid() || !picture.save(picturePath)) return bannerFail("imagen de prueba");
            const auto stored = QDir(data).filePath("banners/" + demoId + ".png");
            launcher.setBannerImage(demoId, QUrl::fromLocalFile(custom.filePath("no-existe.png")));
            { QFile f(custom.filePath("texto.png")); if (!f.open(QIODevice::WriteOnly) || f.write("no es una imagen") < 0) return bannerFail("archivo falso"); }
            launcher.setBannerImage(demoId, QUrl::fromLocalFile(custom.filePath("texto.png")));
            if (QFileInfo::exists(stored)) return bannerFail("imagen no válida aceptada");
            launcher.setBannerImage(demoId, QUrl::fromLocalFile(picturePath));
            if (!launcher.m_banners->waitIdle(20000) || !QFileInfo::exists(stored) || !valid(launcher.bannerInfo(demoId), "custom"))
                return bannerFail("personalizado");
            launcher.clearBanner(demoId);
            if (!launcher.m_banners->waitIdle(20000) || QFileInfo::exists(stored) || !valid(launcher.bannerInfo(demoId), "screenshot"))
                return bannerFail("quitar personalizado");
        }
        launcher.playInstance("00000000-0000-0000-0000-000000000000");
        if (!launcher.gameBusy()) return 9;
        if (!launcher.property("gameRunning").isValid() || !launcher.property("playingInstanceId").isValid()) return 33;
        auto *root = engine.rootObjects().first();
        int page = 0;
        int theme = 0;
        int section = 0;
        int detailSection = -1;
        int visualStage = 0;
        const auto captureDirectory = qEnvironmentVariable("OPENCLIENT_QA_CAPTURE_DIR");
        if (qEnvironmentVariableIsSet("OPENCLIENT_QA_PLAY_GALLERY")) root->setProperty("qaPlayGallery", true);
        const bool captureCatalog = qEnvironmentVariable("OPENCLIENT_QA_SURFACE") == "catalog";
        const bool captureSidebar = qEnvironmentVariable("OPENCLIENT_QA_SURFACE") == "sidebar";
        const bool captureWardrobe = qEnvironmentVariable("OPENCLIENT_QA_SURFACE") == "wardrobe";
        const bool captureSettings = qEnvironmentVariable("OPENCLIENT_QA_SURFACE") == "settings";
        const bool captureContent = qEnvironmentVariable("OPENCLIENT_QA_SURFACE") == "content";
        const bool captureCrystal = qEnvironmentVariable("OPENCLIENT_QA_SURFACE") == "crystal";
        const bool captureBanner = qEnvironmentVariable("OPENCLIENT_QA_SURFACE") == "banner";
        const bool captureRoundTrip = qEnvironmentVariable("OPENCLIENT_QA_SURFACE") == "roundtrip";
        if (captureContent) {
            // Archivos falsos con nombres reales de mods: no son jars válidos, así que salen como «archivo» con monograma.
            const QStringList mods = {
                "sodium-fabric-0.6.0-mc1.21.4.jar", "lithium-fabric-0.14.3-mc1.21.4.jar", "iris-fabric-1.8.1-mc1.21.4.jar",
                "fabric-api-0.110.5-1.21.4.jar", "modmenu-13.0.0.jar", "ferritecore-7.1.1-fabric.jar",
                "entityculling-fabric-1.7.4-mc1.21.4.jar", "immediatelyfast-fabric-1.3.5-1.21.4.jar", "krypton-0.2.8.jar",
                "cloth-config-17.0.144-fabric.jar", "appleskin-fabric-mc1.21.4-3.0.5.jar",
                "reeses-sodium-options-fabric-1.8.1-mc1.21.4.jar.disabled", "mouse-tweaks-fabric-mc1.21.4-2.26.1.jar.disabled",
                "dynamic-fps-3.10.0.jar.disabled"};
            const QList<QPair<QString, QString>> files = [&mods] {
                QList<QPair<QString, QString>> list;
                for (const auto &name : mods) list.append({"mods", name});
                list.append(qMakePair(QStringLiteral("resourcepacks"), QStringLiteral("Faithful 64x - 1.21.4.zip")));
                list.append(qMakePair(QStringLiteral("shaderpacks"), QStringLiteral("ComplementaryReimagined_r5.4.zip")));
                return list;
            }();
            for (const auto &entry : files) {
                if (!QDir().mkpath(fixture + "/" + entry.first)) return 21;
                QFile file(fixture + "/" + entry.first + "/" + entry.second);
                if (!file.open(QIODevice::WriteOnly) || file.write("openclient-qa") < 0) return 21;
            }
        }
        const auto liveProject = qEnvironmentVariable("OPENCLIENT_QA_PROJECT");
        const bool captureProject = !liveProject.isEmpty();
        int projectStage = 0;
        int projectVisual = 0;
        QString pinnedVersion;
        const int captureCount = captureRoundTrip ? 6 : captureBanner ? 11 : captureCrystal ? 8 : captureContent ? 4 : captureWardrobe ? 2 : captureSettings ? 12 : captureCatalog ? 20 : 15;
        if (!captureDirectory.isEmpty() && !QDir().mkpath(captureDirectory)) return 21;
        bool copyStarted = false;
        bool catalogRouteChecked = false;
        int transferStage = 0;
        QTemporaryDir portable;
        if (!portable.isValid()) return 13;
        QTimer navigation;
        navigation.setInterval(std::max(captureSidebar ? 400 : captureCrystal || captureBanner ? 350 : 180, qEnvironmentVariableIntValue("OPENCLIENT_QA_INTERVAL")));
        const QStringList destinations = {"menu", "official", "instances", "downloads", "wardrobe", "credits"};
        const QStringList themes = {"crystal-ruby", "crystal-amethyst", "crystal-obsidian", "dark-obsidian"};
        QObject::connect(&navigation, &QTimer::timeout, &app, [&] {
            if (page < destinations.size()) {
                root->setProperty("activePage", destinations.at(page++));
                if (root->property("activePage").toString() == "downloads") {
                    auto *status = root->findChild<QObject *>("catalogCurseforgeStatus");
                    if (!status || (qEnvironmentVariable("OPENCLIENT_LANG").isEmpty() && !status->property("text").toString().contains("Próximamente"))) app.exit(26);
                }
            }
            else if (theme < themes.size()) {
                root->setProperty("settingsShown", true);
                const auto themeId = themes.at(theme++);
                launcher.setThemeId(themeId);
                auto *particles = root->findChild<QObject *>("workspaceParticles");
                const auto sprite = particles ? particles->property("spriteSource").toUrl() : QUrl();
                // Con OPENCLIENT_QA_THEME el tema está forzado y no coincide con el de la lista: no se compara.
                if (qEnvironmentVariable("OPENCLIENT_QA_THEME").isEmpty() &&
                    (sprite.fileName() != themeId + ".svg" || !QFile::exists(":" + sprite.path())))
                    app.exit(15);
            }
            else if (section < 6) {
                auto *settingsPage = root->findChild<QObject *>("settingsPage");
                if (!settingsPage || !settingsPage->setProperty("section", section++)) app.exit(6);
                else if (section == 6) {
                    auto *status = settingsPage->findChild<QObject *>("settingsCurseforgeStatus");
                    if (!status || (qEnvironmentVariable("OPENCLIENT_LANG").isEmpty() && !status->property("text").toString().contains("Próximamente"))) app.exit(26);
                }
            }
            else if (!captureProject && !captureDirectory.isEmpty() && visualStage <= captureCount) {
                auto *window = qobject_cast<QQuickWindow *>(root);
                auto *account = root->findChild<QObject *>("mainAccountDialog");
                if (!window || !account) { app.exit(21); return; }
                if (visualStage > 0) {
                    const auto file = QDir(captureDirectory).filePath(QString("view-%1.png").arg(visualStage - 1, 2, 10, QLatin1Char('0')));
                    if (!window->grabWindow().save(file)) { app.exit(21); return; }
                }
                QMetaObject::invokeMethod(account, "close");
                if (visualStage == captureCount) { ++visualStage; return; }
                root->setProperty("settingsShown", false);
                if (captureSettings) {
                    // Seis categorías en dos tamaños de ventana; la captura es siempre Ajustes abierto.
                    window->resize(visualStage < 6 ? 940 : 1260, visualStage < 6 ? 620 : 800);
                    launcher.setThemeId(themes.at(0));
                    root->setProperty("activePage", "menu");
                    root->setProperty("settingsShown", true);
                    if (auto *page = root->findChild<QObject *>("settingsPage")) page->setProperty("section", visualStage % 6);
                    ++visualStage;
                    return;
                }
                if (captureRoundTrip) {
                    // Inicio -> Instancias -> Inicio: el banner del héroe debe seguir viéndose al volver (captura 0 = inicio, 3 = vuelta).
                    static const QStringList route = {"menu", "instances", "instances", "menu", "menu", "menu"};
                    window->resize(1260, 800);
                    launcher.setThemeId("crystal-ruby");
                    root->setProperty("activePage", route.at(std::min<int>(visualStage, route.size() - 1)));
                    ++visualStage;
                    return;
                }
                if (captureBanner) {
                    // Inicio con el banner de la instancia demo en los cuatro temas a 940x620 (0-3) y 1260x800 (4-7);
                    // 8: biblioteca con la banda de banner en la tarjeta; 9: diálogo de editar (banner real);
                    // 10: diálogo de nueva instancia (arte generado, vista previa en vivo).
                    const QStringList bannerThemes = {"crystal-ruby", "crystal-amethyst", "crystal-obsidian", "dark-obsidian"};
                    window->resize(visualStage < 4 ? 940 : 1260, visualStage < 4 ? 620 : 800);
                    if (visualStage < 8) {
                        launcher.setThemeId(bannerThemes.at(visualStage % 4));
                        root->setProperty("activePage", "menu");
                    } else {
                        launcher.setThemeId(bannerThemes.at(visualStage == 10 ? 2 : 0));
                        root->setProperty("activePage", "instances");
                        launcher.setCompactLibrary(false);
                        if (visualStage >= 9) {
                            // La biblioteca ya existe desde la captura 8: el diálogo se abre en este mismo paso.
                            auto *library = root->findChild<QObject *>("instanceLibrary");
                            auto *editor = library ? library->findChild<QObject *>("instanceEditor") : nullptr;
                            if (!library || !editor) { std::fprintf(stderr, "Banner QA: sin biblioteca o editor\n"); app.exit(27); return; }
                            if (visualStage == 9)
                                QMetaObject::invokeMethod(library, "openEditor", Q_ARG(QVariant, launcher.instances().first()));
                            else
                                QMetaObject::invokeMethod(library, "openEditor", Q_ARG(QVariant, QVariant()));
                        }
                    }
                    ++visualStage;
                    return;
                }
                if (captureCrystal) {
                    // Inicio sin diálogos con los cuatro temas en 940x620 (capturas 0-3) y 1260x800 (4-7).
                    const QStringList crystalThemes = {"crystal-ruby", "crystal-amethyst", "crystal-obsidian", "dark-obsidian"};
                    window->resize(visualStage < 4 ? 940 : 1260, visualStage < 4 ? 620 : 800);
                    launcher.setThemeId(crystalThemes.at(visualStage % 4));
                    root->setProperty("activePage", "menu");
                    ++visualStage;
                    return;
                }
                if (captureContent) {
                    // Detalles de la instancia de prueba en Contenido: 940x620 y 1260x800, con Cristal Rubí y Obsidiana.
                    auto *library = root->findChild<QObject *>("instanceLibrary");
                    auto *details = root->findChild<QObject *>("instanceDetails");
                    if (!details) {
                        // La página y los detalles se crean en intervalos distintos.
                        if (!library) root->setProperty("activePage", "instances");
                        else if (!library->setProperty("detailsId", demoId)) app.exit(27);
                        return;
                    }
                    if (visualStage >= 1 && !launcher.contentLoading() && launcher.content()->rowCount() < 14) {
                        std::fprintf(stderr, "Content view: %d rows listed, expected at least 14 (%s)\n",
                            launcher.content()->rowCount(), qPrintable(launcher.contentError()));
                        app.exit(27); return;
                    }
                    if (!QMetaObject::invokeMethod(details, "selectSection", Q_ARG(QVariant, QVariant(1)))) { app.exit(27); return; }
                    window->resize(visualStage % 2 ? 1260 : 940, visualStage % 2 ? 800 : 620);
                    launcher.setThemeId(themes.at(visualStage < 2 ? 0 : 2));
                    // Con la ventana ya a su tamaño, la página se desplaza hasta que la cabecera de la lista queda arriba.
                    QTimer::singleShot(90, root, [root] {
                        auto *scroll = root->findChild<QObject *>("pageScroll");
                        auto *toolbar = root->findChild<QQuickItem *>("contentToolbar");
                        auto *flick = scroll ? scroll->property("contentItem").value<QQuickItem *>() : nullptr;
                        auto *surface = flick ? flick->property("contentItem").value<QQuickItem *>() : nullptr;
                        if (!toolbar || !surface) return;
                        const qreal top = toolbar->mapToItem(surface, QPointF(0, 0)).y();
                        const qreal limit = std::max<qreal>(0, flick->property("contentHeight").toReal() - flick->height());
                        flick->setProperty("contentY", std::clamp<qreal>(top - 12, 0, limit));
                    });
                    ++visualStage;
                    return;
                }
                if (captureWardrobe) {
                    window->resize(visualStage == 0 ? 1366 : 940, 840);
                    launcher.setThemeId(themes.at(0));
                    root->setProperty("activePage", "wardrobe");
                    ++visualStage;
                    return;
                }
                if (captureSidebar) {
                    const auto findNavigation = [](auto &&self, QQuickItem *item, const QString &name) -> QObject * {
                        if (item->objectName() == name) return item;
                        for (auto *child : item->childItems()) {
                            if (auto *found = self(self, child, name)) return found;
                        }
                        return nullptr;
                    };
                    // Tres tamaños de ventana x cuatro temas; la captura es siempre el inicio. Antes de volver a él se
                    // comprueba que el botón de cada destino navega y queda seleccionado.
                    const QStringList pages = {"menu", "instances", "credits"};
                    const int size = visualStage % 3;
                    window->resize(size == 0 ? 940 : size == 1 ? 1260 : 1536, size == 0 ? 620 : size == 1 ? 800 : 864);
                    launcher.setThemeId(themes.at((visualStage / 3) % themes.size()));
                    auto *button = findNavigation(findNavigation, window->contentItem(), "sidebar-" + pages.at(size));
                    if (!button || !QMetaObject::invokeMethod(button, "clicked") ||
                        root->property("activePage").toString() != pages.at(size) ||
                        !button->property("selected").toBool()) {
                        std::fprintf(stderr, "Sidebar navigation: page=%s button=%p current=%s selected=%d\n", qPrintable(pages.at(size)),
                            static_cast<void *>(button), qPrintable(root->property("activePage").toString()), button && button->property("selected").toBool());
                        app.exit(25); return;
                    }
                    auto *home = findNavigation(findNavigation, window->contentItem(), "sidebar-menu");
                    if (!home || !QMetaObject::invokeMethod(home, "clicked") || root->property("activePage").toString() != "menu") {
                        std::fprintf(stderr, "Sidebar home: button=%p\n", static_cast<void *>(home));
                        app.exit(25); return;
                    }
                    auto *accountButton = root->findChild<QObject *>("sidebar-account");
                    if (!accountButton || !QMetaObject::invokeMethod(accountButton, "clicked") ||
                        !account->property("visible").toBool()) {
                        std::fprintf(stderr, "Sidebar account: button=%p visible=%d\n", static_cast<void *>(accountButton), account->property("visible").toBool());
                        app.exit(25); return;
                    }
                    // Solo para la captura (sin tocar los ajustes guardados): a 1536 la barra sale fijada y desplegada.
                    if (auto *rail = root->findChild<QObject *>("navigationSidebar")) {
                        rail->setProperty("pinned", size == 2);
                        // A 1260 sale la etiqueta flotante sobre Instancias; en los demás tamaños no hay ninguna.
                        rail->setProperty("tipTarget", QVariant::fromValue<QObject *>(nullptr));
                        if (size == 1) {
                            // Con el ratón la posición ya es estable; aquí se espera a que termine el cambio de tamaño.
                            if (auto *tipButton = findNavigation(findNavigation, window->contentItem(), "sidebar-instances"))
                                QTimer::singleShot(150, rail, [rail, tipButton] {
                                    QMetaObject::invokeMethod(rail, "showTip", Q_ARG(QVariant, QVariant::fromValue(tipButton)),
                                                              Q_ARG(QVariant, QStringLiteral("Instancias")));
                                });
                        }
                    }
                    // Y a 1260 la cabeza de la cuenta sale desplegada.
                    // La página nueva tarda un poco en cargarse: se espera antes de buscar la cabeza.
                    QTimer::singleShot(120, root, [root, size] {
                        if (auto *chip = root->findChild<QObject *>("accountChip")) chip->setProperty("pinned", size == 1);
                    });
                    QMetaObject::invokeMethod(account, "close");
                    ++visualStage;
                    return;
                }
                if (captureCatalog) {
                    const QStringList kinds = {"modpack", "mod", "resourcepack", "shader"};
                    window->resize(visualStage % 2 ? 1260 : 940, 800);
                    launcher.setThemeId(themes.at((visualStage / 4) % themes.size()));
                    root->setProperty("activePage", "downloads");
                    auto *catalog = root->findChild<QObject *>("catalogPage");
                    if (!catalog || !QMetaObject::invokeMethod(catalog, "configure",
                        Q_ARG(QVariant, QVariant(kinds.at(visualStage % 4))), Q_ARG(QVariant, QVariant(demoId))) ||
                        catalog->property("kind").toString() != kinds.at(visualStage % 4) ||
                        catalog->property("chosenInstanceId").toString() != demoId ||
                        !catalog->property("category").toString().isEmpty()) { app.exit(22); return; }
                    auto *sort = catalog->findChild<QObject *>("catalogSort");
                    auto *popup = sort ? sort->property("popup").value<QObject *>() : nullptr;
                    if (!popup || !QMetaObject::invokeMethod(popup, visualStage % 4 == 3 ? "open" : "close")) {
                        app.exit(22); return;
                    }
                    ++visualStage;
                    return;
                }
                window->resize(940, 620);
                launcher.setThemeId(themes.at((visualStage / 3) % themes.size()));
                root->setProperty("activePage", "credits");
                if (visualStage % 3 != 0) {
                    QMetaObject::invokeMethod(account, "open");
                    account->setProperty("selectedTab", visualStage % 3 - 1);
                }
                ++visualStage;
            }
            else if (detailSection < 6) {
                if (detailSection < 0) {
                    root->setProperty("settingsShown", false);
                    root->setProperty("activePage", "instances");
                    auto *library = root->findChild<QObject *>("instanceLibrary");
                    if (!library || !library->setProperty("detailsId", demoId)) app.exit(11);
                    detailSection = 0;
                } else {
                    auto *details = root->findChild<QObject *>("instanceDetails");
                    if (!details || !QMetaObject::invokeMethod(details, "selectSection", Q_ARG(QVariant, QVariant(detailSection++))))
                        app.exit(11);
                }
            }
            else if (!catalogRouteChecked) {
                auto *details = root->findChild<QObject *>("instanceDetails");
                if (!details || !QMetaObject::invokeMethod(details, "openContent", Q_ARG(QVariant, QVariant("shader"))) ||
                    root->property("activePage").toString() != "downloads" ||
                    root->property("catalogKind").toString() != "shader" ||
                    root->property("catalogTargetId").toString() != demoId) {
                    std::fprintf(stderr, "Catalog route did not preserve shader category and instance.\n");
                    app.exit(20);
                    return;
                }
                catalogRouteChecked = true;
            }
            else if (projectStage < 4) {
                auto *catalog = root->findChild<QObject *>("catalogPage");
                if (!catalog) { app.exit(23); return; }
                if (projectStage == 3) {
                    if (launcher.catalogBusy()) return;
                    launcher.reloadLibrary();
                    const auto content = launcher.instances().first().toMap().value("installedContent").toList();
                    if (content.size() != 1 || content.first().toMap().value("id") != liveProject ||
                        content.first().toMap().value("versionId") != pinnedVersion) {
                        std::fprintf(stderr, "Pinned version not persisted: %s\n", qPrintable(launcher.catalogMessage()));
                        app.exit(24); return;
                    }
                    std::fprintf(stdout, "Selected version: persisted=ok\n");
                    projectStage = 4;
                    return;
                }
                if (projectStage == 0) {
                    QMetaObject::invokeMethod(catalog, "configure", Q_ARG(QVariant, QVariant("mod")), Q_ARG(QVariant, QVariant(demoId)));
                    catalog->setProperty("submittedQuery", "sodium");
                    catalog->setProperty("category", "optimization");
                    const QVariantMap project{{"id", captureProject ? liveProject : "../invalid"}, {"title", "Proyecto"}};
                    if (!QMetaObject::invokeMethod(catalog, "openProject", Q_ARG(QVariant, QVariant(project))) || !launcher.projectBusy()) {
                        app.exit(23); return;
                    }
                    ++projectStage;
                    return;
                }
                if (launcher.projectBusy()) return;
                auto *dialog = root->findChild<QObject *>("projectDialog");
                if (!dialog || !dialog->property("visible").toBool() || dialog->property("targetId").toString() != demoId) {
                    app.exit(23); return;
                }
                if (captureProject) {
                    if (!launcher.projectError().isEmpty() || launcher.projectInfo().value("id") != liveProject ||
                        launcher.projectInfo().value("versions").toList().isEmpty()) {
                        std::fprintf(stderr, "Project details: %s\n", qPrintable(launcher.projectError()));
                        app.exit(23); return;
                    }
                    auto *window = qobject_cast<QQuickWindow *>(root);
                    if (!window) { app.exit(23); return; }
                    if (projectStage == 1) {
                        if (pinnedVersion.isEmpty()) {
                            const auto choices = launcher.projectInfo().value("versions").toList();
                            pinnedVersion = choices.at(choices.size() > 1 ? 1 : 0).toMap().value("id").toString();
                            if (!QMetaObject::invokeMethod(dialog, "chooseVersion", Q_ARG(QVariant, QVariant(pinnedVersion))) ||
                                dialog->property("chosenVersion").toMap().value("id") != pinnedVersion) { app.exit(24); return; }
                        }
                        window->resize(projectVisual % 2 ? 1260 : 940, projectVisual % 2 ? 800 : 620);
                        launcher.setThemeId(themes.at((projectVisual / 4) % themes.size()));
                        dialog->setProperty("section", projectVisual % 4);
                        projectStage = 2;
                        return;
                    }
                    if (!captureDirectory.isEmpty() && !window->grabWindow().save(QDir(captureDirectory).filePath(
                        QString("project-%1.png").arg(projectVisual, 2, 10, QLatin1Char('0'))))) { app.exit(23); return; }
                    if (++projectVisual < 20) { projectStage = 1; return; }
                    if (!QMetaObject::invokeMethod(dialog, "installSelected") || !launcher.catalogBusy()) { app.exit(24); return; }
                } else if (launcher.projectError().isEmpty() || !launcher.projectInfo().value("id").toString().isEmpty()) {
                    app.exit(23); return;
                }
                QMetaObject::invokeMethod(dialog, "close");
                if (catalog->property("submittedQuery").toString() != "sodium" ||
                    catalog->property("category").toString() != "optimization" ||
                    catalog->property("chosenInstanceId").toString() != demoId) { app.exit(23); return; }
                std::fprintf(stdout, "Project dialog: context=preserved response=%s\n", captureProject ? "live" : "invalid-request-rejected");
                projectStage = captureProject ? 3 : 4;
            }
            else if (!launcher.hardwareLoading() && !launcher.gameBusy()) {
                if (!copyStarted) {
                    copyStarted = true;
                    launcher.duplicateInstance(demoId);
                    if (!launcher.copyingInstance()) app.exit(12);
                    return;
                }
                if (launcher.copyingInstance()) return;
                if (transferStage < 2 && launcher.instances().size() != 2) {
                    std::fprintf(stderr, "Duplicate: %s\n", qPrintable(launcher.copyMessage()));
                    app.exit(12);
                    return;
                }
                const auto copy = launcher.instances().value(1).toMap();
                const auto copyId = copy.value("id").toString();
                if (copyId == demoId || copy.value("favorite").toBool() ||
                    launcher.instanceFiles(copyId, "worlds").size() != 1 ||
                    launcher.instanceFiles(copyId, "screenshots").size() != 1 ||
                    launcher.instanceFiles(demoId, "worlds").size() != 1) {
                    app.exit(12);
                    return;
                }
                if (transferStage == 0) {
                    ++transferStage;
                    launcher.exportInstance(demoId, QUrl::fromLocalFile(portable.path()));
                    if (!launcher.copyingInstance()) app.exit(13);
                    return;
                }
                if (transferStage == 1) {
                    const QDir destination(portable.path());
                    const auto packages = destination.entryList({"OpenClient-*"}, QDir::Dirs | QDir::NoDotAndDotDot);
                    if (packages.size() != 1) {
                        std::fprintf(stderr, "Export: %s\n", qPrintable(launcher.copyMessage()));
                        app.exit(13);
                        return;
                    }
                    const auto bundle = destination.filePath(packages.first());
                    if (!QFile::exists(bundle + "/openclient-instance.json") ||
                        !QFile::exists(bundle + "/game/saves/Mundo/level.dat") ||
                        QFile::exists(bundle + "/game/preview.ini")) {
                        app.exit(13);
                        return;
                    }
                    ++transferStage;
                    launcher.importInstance(QUrl::fromLocalFile(bundle));
                    if (!launcher.copyingInstance()) app.exit(13);
                    return;
                }
                const auto importedId = launcher.instances().last().toMap().value("id").toString();
                if (launcher.instances().size() != 3 || importedId == demoId || importedId == copyId ||
                    launcher.instanceFiles(importedId, "worlds").size() != 1 ||
                    launcher.instanceFiles(importedId, "screenshots").size() != 1) {
                    std::fprintf(stderr, "Import: %s\n", qPrintable(launcher.copyMessage()));
                    app.exit(13);
                    return;
                }
                const bool launchRejected = launcher.gameMessage() == "La instancia ya no existe.";
                launcher.clearOfflineAccount();
                const int before = launcher.allocatedRamMb();
                launcher.setAllocatedRamMb(0);
                const bool invalidRamRejected = before == launcher.allocatedRamMb();
                const bool success = !qmlWarning && launcher.hardwareError().isEmpty() && !launcher.hardware().isEmpty()
                    && invalidRamRejected && launchRejected && launcher.accountName().isEmpty()
                    && launcher.linkAvailable(1) && !launcher.linkAvailable(0) && !launcher.linkAvailable(-1);
                std::fprintf(stdout, "Qt smoke: pages=%d themes=%d sections=%d details=%d duplicate=ok transfer=ok library=ok offline=ok hardware=%s qml=%s\n", page, theme, section, detailSection,
                    launcher.hardware().isEmpty() ? "unavailable" : "ok", qmlWarning ? "warnings" : "ok");
                app.exit(success ? 0 : 4);
            }
        });
        QTimer::singleShot(captureProject ? 120000 : 20000, &app, [&] { app.exit(5); });
        launcher.refreshHardware();
        navigation.start();
        return app.exec();
    }
    launcher.refreshHardware();
    return app.exec();
}
