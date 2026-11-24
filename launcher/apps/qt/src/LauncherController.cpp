#include "LauncherController.h"
#include "WindowFrame.h"
#include "InstanceLinks.h"
#include <QWindow>
#include <QClipboard>
#include <QCoreApplication>
#include <QDesktopServices>
#include <QDir>
#include <QJsonDocument>
#include <QJsonObject>
#include <QCryptographicHash>
#include <QFile>
#include <QLibrary>
#include <QLockFile>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QMap>
#include <QScopeGuard>
#include <QStandardPaths>
#include <QFileInfo>
#include <QGuiApplication>
#include <QUrl>
#include <QUuid>
#include <QSet>
#include <QTextDocument>
#include <QRegularExpression>
#include <QtConcurrent>
#include <algorithm>
#include <atomic>
#include <cstdio>
#include <utility>

namespace {
std::atomic<int> downloadConcurrencyShared{16};
const QList<int> javaMajors = {8, 16, 17, 21, 25};
bool commandValid(const QString &value) {
    if (value.size() > 512) return false;
    return std::none_of(value.begin(), value.end(), [](QChar c) { return c.isNonCharacter() || c.unicode() < 0x20 || c.unicode() == 0x7f; });
}
// Acciones que devuelven el resultado de una operación de modpack (para `modpackResult`).
const QSet<QString> modpackWrites = {"modpack_update", "modpack_restore", "modpack_link", "modpack_export",
                                     "import_curseforge", "import_mmc"};
const QStringList themes = {"crystal-ruby", "crystal-amethyst", "crystal-obsidian", "dark-obsidian"};
const QStringList gcPresets = {"automatic", "g1gc", "generational_zgc", "low_memory"};
const QStringList links = {
    "https://openclient.josgpe.com/", "https://discord.gg/6wGeNxTNS6", QString(),
    "https://github.com/JoseOrtiz-Dev/OpenClient-launcher",
    "https://enderhosting.pro/?a=toamso10", "https://discord.gg/jJMcN9uJq"
};
}

LauncherController::LauncherController(const QString &settingsFile, QObject *parent)
    : QObject(parent), m_settings(settingsFile, QSettings::IniFormat),
      m_libraryRoot(QFileInfo(settingsFile).absolutePath()) {
    m_contentModel->setRoot(m_libraryRoot);
    m_banners = new InstanceBanners(m_libraryRoot, this);
    connect(m_banners, &InstanceBanners::changed, this, &LauncherController::bannersChanged);
    connect(this, &LauncherController::libraryChanged, this, [this] {
        for (const auto &id : m_banners->ids()) {
            const auto entry = instanceEntry(id);
            if (entry.isEmpty()) continue;
            const auto modpack = entry.value("modpack").toMap();
            const auto project = modpack.value("source").toString() == "modrinth" ? modpack.value("projectId").toString() : QString();
            if (project != m_banners->project(id)) requestBanner(id);
        }
    });
    // Al cerrar el juego hay capturas nuevas.
    connect(this, &LauncherController::gameChanged, this, [this] {
        const bool busy = gameBusy();
        if (m_bannerGameBusy && !busy) refreshBanners();
        m_bannerGameBusy = busy;
    });
    m_accountName = m_settings.value("account/offline_name").toString();
    m_offlineProfiles = m_settings.value("account/offline_profiles").toStringList();
    // Versiones anteriores guardaban un solo perfil offline: pasa a ser el primero de la lista.
    if (!m_accountName.isEmpty() && !m_offlineProfiles.contains(m_accountName)) m_offlineProfiles.prepend(m_accountName);
    m_useMicrosoft = m_settings.value("account/use_microsoft", false).toBool();
    m_microsoftId = m_settings.value("account/microsoft_id").toString();
    m_instanceAccounts = QJsonDocument::fromJson(m_settings.value("account/instance_accounts").toString().toUtf8())
        .object().toVariantMap();
    reloadMicrosoftAccounts();
    m_authTimer.setInterval(250);
    connect(&m_authTimer, &QTimer::timeout, this, &LauncherController::pollMicrosoftPrompt);
    connect(&m_auth, &QFutureWatcher<QVariantMap>::finished, this, [this] {
        m_authTimer.stop();
        m_authAttempt = 0;
        m_microsoftPrompt.clear();
        const auto result = m_auth.result();
        m_microsoftMessage = result.value("error").toString();
        if (m_microsoftMessage.startsWith("SESSION_")) {
            m_microsoftNeedsLogin = m_microsoftMessage.startsWith("SESSION_REAUTH:");
            m_microsoftMessage = m_microsoftMessage.section(':', 1).trimmed();
        }
        if (m_microsoftMessage.isEmpty()) {
            const auto profile = result.value("profile").toMap();
            if (const auto id = profile.value("id").toString(); !id.isEmpty()) {
                m_microsoftId = id;
                m_settings.setValue("account/microsoft_id", id);
            }
            m_microsoftMessage = result.value("message", "Cuenta de Minecraft Java conectada.").toString();
            reloadMicrosoftAccounts();
            m_microsoftNeedsLogin = false;
            if (m_microsoftProfile.isEmpty()) m_microsoftProfile = profile;
            if (result.value("select").toBool()) selectMicrosoft(true);
        } else if (m_authCancelled) {
            m_microsoftMessage = "Inicio de sesión cancelado.";
        }
        m_authLock.reset();
        emit accountChanged();
    });
    connect(&m_catalog, &QFutureWatcher<QVariantMap>::finished, this, [this] {
        const auto response = m_catalog.result();
        m_catalogPending = false;
        const auto action = response.value("action").toString();
        const auto error = response.value("error").toString();
        const bool cancelled = error.startsWith("CANCELLED:");
        const auto id = response.value("id").toString();
        const auto result = response.value("result");
        const auto linkingId = std::exchange(m_officialLinkingId, QString());
        if (response.value("write").toBool()) endOperation(cancelled);
        if (action == "world_backup" || action == "world_restore") {
            const auto map = result.toMap();
            m_worldFailed = !cancelled && !error.isEmpty();
            if (cancelled) {
                m_worldMessage = "Operación cancelada.";
            } else if (!error.isEmpty()) {
                m_worldMessage = error;
            } else if (action == "world_backup") {
                const auto backup = map.value("backup").toMap();
                m_worldMessage = QString("Copia guardada: %1 mundo(s), %2 MB.")
                                     .arg(backup.value("worlds").toList().size())
                                     .arg(backup.value("bytes").toLongLong() / 1048576);
            } else {
                m_worldMessage = "Mundo restaurado. Antes se guardó una copia de cómo estaba.";
            }
            emit catalogChanged();
            loadWorlds(id);
            return;
        }
        const bool modpackAction = modpackWrites.contains(action);
        if (modpackAction && !cancelled) {
            m_modpackError = error;
            if (error.isEmpty()) m_modpackResult = result.toMap();
            emit modpackResultChanged();
        }
        if (m_copyingInstance) {
            m_copyingInstance = false;
            m_copyMessage = cancelled ? QString("Operación cancelada.") : error;
            if (m_copyMessage.isEmpty()) {
                if (action == "export") {
                    auto folder = result.toMap().value("folder").toString();
                    if (folder.startsWith("\\\\?\\UNC\\")) folder = "\\\\" + folder.mid(8);
                    else if (folder.startsWith("\\\\?\\")) folder = folder.mid(4);
                    m_copyMessage = "Instancia exportada en " + folder;
                } else if (modpackAction) {
                    applyInstances(result);
                    m_copyMessage = modpackWriteMessage(action, result.toMap());
                    if (action == "modpack_link" && linkingId == id && !id.isEmpty()) {
                        // Oficial antiguo: tras vincularlo se actualiza al pack incluido.
                        startOfficialUpdate(id);
                        return;
                    }
                    if (action == "modpack_update") requestModpackCheck(id, false);
                } else {
                    m_instances = withDiskMods(result.toList());
                    m_libraryError.clear();
                    m_copyMessage = action == "duplicate" ? "Copia creada en la biblioteca." :
                        action == "modpack_import" ? "Modpack importado como una instancia nueva." :
                        "Instancia importada en la biblioteca.";
                    emit libraryChanged();
                }
            }
            emit copyChanged();
            emit catalogChanged();
            return;
        }
        m_catalogMessage = cancelled ? QString("Operación cancelada.") : error;
        if (m_catalogMessage.isEmpty()) {
            if (response.value("write").toBool()) {
                if (action == "content_sync") {
                    m_catalogMessage = "Contenido sincronizado.";
                } else if (action == "content_update") {
                    reloadLibrary();
                    const int updated = result.toMap().value("updated").toInt();
                    m_catalogMessage = updated > 0
                        ? QString("%1 proyecto(s) actualizado(s) y sincronizado(s).").arg(updated)
                        : "Todo el contenido compatible ya está en su última versión.";
                } else if (modpackAction) {
                    if (action != "modpack_export") applyInstances(result);
                    m_catalogMessage = modpackWriteMessage(action, result.toMap());
                    if (action == "modpack_update" || action == "modpack_restore" || action == "modpack_link") {
                        requestModpackCheck(id, false);
                        if (m_snapshotsId == id) listSnapshots(id);
                    }
                } else {
                    reloadLibrary();
                    m_catalogMessage = action == "modpack_install" ? "Modpack instalado en una instancia nueva." :
                        "Guardado. Los archivos se sincronizarán al jugar.";
                    // content_add devuelve un array de instancias o {instances, skippedOptional}.
                    if (action == "content_add" && result.metaType().id() == QMetaType::QVariantMap) {
                        QStringList skipped;
                        for (const auto &entry : result.toMap().value("skippedOptional").toList()) {
                            const auto item = entry.toMap();
                            skipped.append(item.value("reason").toString().isEmpty()
                                ? item.value("project").toString()
                                : QString("%1 (%2)").arg(item.value("project").toString(), item.value("reason").toString()));
                            if (skipped.size() >= 5) break;
                        }
                        const int count = result.toMap().value("skippedOptional").toList().size();
                        if (count > 0)
                            m_catalogMessage += QString(" %1 complemento(s) opcional(es) no se añadieron: %2.")
                                .arg(count).arg(skipped.join(", "));
                    }
                }
            } else {
                const auto results = result.toMap();
                m_catalogResults = results.value("projects").toList();
                m_catalogPage = results.value("page").toInt();
                m_catalogTotal = results.value("total").toInt();
                m_catalogFilters = response.value("filters").toMap();
                m_catalogInstance = response.value("id").toString();
                m_catalogKind = response.value("kind").toString();
                m_catalogMessage = m_catalogResults.isEmpty() ? "No se encontró contenido compatible." : QString();
            }
        }
        emit catalogChanged();
        // Las escrituras (alternar, quitar, sincronizar, actualizar, añadir) cambian la lista que se está viendo.
        if (response.value("write").toBool() && m_contentActive && !m_contentId.isEmpty()) loadContent(m_contentId);
    });
    connect(&m_project, &QFutureWatcher<QVariantMap>::finished, this, [this] {
        const auto response = m_project.result();
        m_projectPending = false;
        m_projectError = response.value("error").toString();
        m_projectInfo = response.value("result").toMap();
        QTextDocument description;
        description.setMarkdown(m_projectInfo.value("body").toString());
        auto plainDescription = description.toPlainText().remove(QChar::ObjectReplacementCharacter);
        plainDescription.replace(QRegularExpression("\\n[\\t \\n]*\\n"), "\n\n");
        m_projectInfo.insert("body", plainDescription.trimmed());
        emit projectChanged();
    });
    connect(&m_official, &QFutureWatcher<QVariantMap>::finished, this, [this] {
        const auto response = m_official.result();
        m_officialPending = false;
        m_officialError = response.value("error").toString();
        m_officialSummary = response.value("result").toMap();
        if (!m_officialError.isEmpty()) m_officialPath.clear();
        emit officialChanged();
    });
    connect(this, &LauncherController::libraryChanged, this, &LauncherController::officialChanged);
    // Cualquier cambio de la lista de instancias (arranque, crear, renombrar, duplicar, importar, eliminar) rehace los accesos.
    connect(this, &LauncherController::libraryChanged, this, &LauncherController::scheduleLinkSync);
    m_operationTimer.setInterval(250);
    connect(&m_operationTimer, &QTimer::timeout, this, &LauncherController::pollOperation);
    m_gameTimer.setInterval(350);
    connect(&m_gameTimer, &QTimer::timeout, this, &LauncherController::pollGame);
    connect(&m_launch, &QFutureWatcher<QString>::finished, this, [this] {
        m_launchPending = false;
        const auto error = m_launch.result();
        if (!error.isEmpty()) {
            m_gameMessage = error.startsWith("CANCELLED:") ? QString("Operación cancelada.") : error;
            if (error.startsWith("SESSION_REAUTH:") || error.startsWith("SESSION_NETWORK:")) {
                m_gameMessage = error.section(':', 1).trimmed();
                if (m_useMicrosoft && m_launchAccountId == m_microsoftId) {
                    m_microsoftNeedsLogin = error.startsWith("SESSION_REAUTH:");
                    m_microsoftMessage = m_gameMessage;
                    emit accountChanged();
                }
            }
            QString ignored;
            requestEngine({{"action", "game_status"}, {"id", m_preparingId}, {"consume", true}}, ignored);
            m_preparingId.clear();
            releaseGameLockIfIdle();
            emit gameChanged();
            return;
        }
        pollGame();
    });
    const auto saved = m_settings.value("appearance/theme", m_themeId).toString();
    if (themes.contains(saved)) m_themeId = saved;
    const auto savedGc = m_settings.value("game/gc_preset", m_gcPreset).toString();
    if (gcPresets.contains(savedGc)) m_gcPreset = savedGc;
    m_particles = m_settings.value("appearance/particles", true).toBool();
    m_spotlight = m_settings.value("appearance/spotlight", true).toBool();
    m_potatoMode = m_settings.value("appearance/potato_mode", false).toBool();
    m_autoSodium = m_settings.value("game/auto_sodium", false).toBool();
    m_windowWidth = std::clamp(m_settings.value("game/window_width", 854).toInt(), 320, 7680);
    m_windowHeight = std::clamp(m_settings.value("game/window_height", 480).toInt(), 240, 4320);
    m_fullscreen = m_settings.value("game/fullscreen", false).toBool();
    m_customJvmArgs = m_settings.value("game/custom_jvm_args").toString();
    m_downloadConcurrency = std::clamp(m_settings.value("game/download_concurrency", 16).toInt(), 1, 20);
    downloadConcurrencyShared = m_downloadConcurrency;
    if (const int savedJava = m_settings.value("game/java_major", 21).toInt(); javaMajors.contains(savedJava)) m_javaMajor = savedJava;
    m_closeWarning = m_settings.value("ui/close_warning", true).toBool();
    m_memoryWarning = m_settings.value("ui/memory_warning", true).toBool();
    m_showSnapshots = m_settings.value("game/show_snapshots", false).toBool();
    m_crashReport = m_settings.value("ui/crash_report", true).toBool();
    m_compactLibrary = m_settings.value("ui/compact_library", false).toBool();
    connect(&m_versionsWatcher, &QFutureWatcher<QVariantMap>::finished, this, [this] {
        const auto response = m_versionsWatcher.result();
        m_versionsLoading = false;
        m_versionsError = response.value("error").toString();
        if (m_versionsError.isEmpty()) m_minecraftVersions = response.value("result").toMap();
        emit versionsChanged();
    });
    const auto savedCommand = [this](const char *key) {
        const auto value = m_settings.value(key).toString().trimmed();
        return commandValid(value) ? value : QString();
    };
    m_preLaunchHook = savedCommand("game/pre_launch_hook");
    m_postExitHook = savedCommand("game/post_exit_hook");
    m_wrapperCommand = savedCommand("game/wrapper_command");
    connect(&m_storage, &QFutureWatcher<QVariantMap>::finished, this, [this] {
        const auto response = m_storage.result();
        m_storagePending = false;
        const auto error = response.value("error").toString();
        if (error.isEmpty()) {
            m_storageInfo = response.value("result").toMap();
            m_storageMessage = response.value("done").toString();
            if (!m_storageMessage.isEmpty()) refreshBanners();
        } else {
            m_storageMessage = error;
        }
        emit storageChanged();
    });
    if (m_settings.contains("catalog/curseforge_api_key")) {
        m_settings.remove("catalog/curseforge_api_key");
        m_settings.sync();
    }
    m_minimizeOnLaunch = m_settings.value("game/minimize_on_launch", false).toBool();
    m_refocusOnExit = m_settings.value("game/refocus_on_exit", false).toBool();
    m_preTouchMemory = m_settings.value("game/pre_touch_memory", false).toBool();
    m_highPriority = m_settings.value("game/high_priority", false).toBool();
    m_javaPaths = m_settings.value("game/java_paths").toMap();
    m_ram = std::max(1024, m_settings.value("game/ram_mb", 2048).toInt());
    connect(&m_java, &QFutureWatcher<QVariantMap>::finished, this, [this] {
        const auto response = m_java.result();
        m_javaPending = false;
        const auto error = response.value("error").toString();
        if (!error.isEmpty()) {
            m_javaMessage = error;
            m_javaStatus.clear();
        } else {
            m_javaStatus = response.value("result").toMap();
            const int major = m_javaStatus.value("required_major").toInt();
            if (response.value("action").toString() == "java_install" && m_javaStatus.value("found").toBool()) {
                m_javaPaths.remove(QString::number(major));
                persist();
            }
            const auto source = m_javaStatus.value("source").toString();
            const int installed = (m_javaStatus.value("installed_major").toInt() ? m_javaStatus.value("installed_major").toInt() : major);
            if (!m_javaStatus.value("found").toBool()) {
                m_javaMessage = QString("Java %1 no está instalado todavía. Puedes instalarlo desde Adoptium o se descargará al iniciar Minecraft.").arg(major);
            } else if (source == "launcher" || source == "downloaded") {
                m_javaMessage = QString("Java %1 listo (runtime aislado de OpenClient).").arg(installed);
            } else if (source == "system") {
                m_javaMessage = QString("Java %1 detectado en el sistema.").arg(installed);
            } else if (source == "manual") {
                m_javaMessage = QString("Java %1 verificado desde la ruta manual.").arg(installed);
            } else {
                m_javaMessage = QString("Java %1 listo.").arg(installed);
            }
        }
        emit javaChanged();
    });
    connect(&m_probe, &QFutureWatcher<QVariantMap>::finished, this, [this] {
        const auto result = m_probe.result();
        m_loading = false;
        m_hardwareError = result.value("error").toString();
        if (m_hardwareError.isEmpty()) {
            m_hardware = result;
            const int maximum = m_hardware.value("installed_ram_mb").toInt();
            if (maximum >= 1024) m_ram = std::clamp(m_ram, 1024, maximum);
            emit preferencesChanged();
        }
        emit hardwareChanged();
    });
}

LauncherController::~LauncherController() {
    if (m_authAttempt) cancelMicrosoftLogin();
    m_auth.waitForFinished();
    m_catalog.waitForFinished();
    for (auto *query : std::as_const(m_queries)) query->waitForFinished();
    m_project.waitForFinished();
    m_launch.waitForFinished();
    m_java.waitForFinished();
    m_probe.waitForFinished();
}

bool LauncherController::loadProject(const QString &id, const QString &project, const QString &kind) {
    if (projectBusy()) return false;
    m_projectPending = true;
    m_projectInfo.clear();
    m_projectError.clear();
    emit projectChanged();
    const QVariantMap request{{"action", "catalog_project"}, {"root", m_libraryRoot},
                              {"id", id}, {"project", project}, {"kind", kind}};
    m_project.setFuture(QtConcurrent::run([request] {
        QString error;
        const auto result = requestEngine(request, error);
        return QVariantMap{{"result", result}, {"error", error}};
    }));
    return true;
}

void LauncherController::searchCatalog(const QString &id, const QString &query, const QString &kind,
                                       const QString &category, const QString &sort, int page) {
    startCatalogRequest({{"action", "catalog_discover"}, {"id", id}, {"query", query}, {"kind", kind},
                         {"category", category}, {"sort", sort}, {"page", page}}, false);
}

bool LauncherController::setupPending() const {
    return !m_settings.value("welcome/completed", false).toBool();
}

bool LauncherController::releasePending() const {
    return m_settings.value("welcome/seen_release").toString() != QCoreApplication::applicationVersion();
}

bool LauncherController::completeSetup() {
    m_settings.setValue("welcome/completed", true);
    m_settings.sync();
    emit introductionChanged();
    return m_settings.status() == QSettings::NoError;
}

bool LauncherController::acknowledgeRelease() {
    m_settings.setValue("welcome/seen_release", QCoreApplication::applicationVersion());
    m_settings.sync();
    emit introductionChanged();
    return m_settings.status() == QSettings::NoError;
}

void LauncherController::changeContent(const QString &id, const QString &project, const QString &action, const QString &kind) {
    if (action != "content_add" && action != "content_toggle" && action != "content_remove") return;
    startCatalogRequest({{"action", action}, {"id", id}, {"project", project}, {"kind", kind}}, true);
}

void LauncherController::syncContent(const QString &id) {
    startCatalogRequest({{"action", "content_sync"}, {"id", id}}, true);
}

void LauncherController::updateInstanceContent(const QString &id) {
    startCatalogRequest({{"action", "content_update"}, {"id", id}}, true);
}

void LauncherController::installModpack(const QString &project) {
    startCatalogRequest({{"action", "modpack_install"}, {"project", project}}, true);
}

void LauncherController::installProjectVersion(const QString &id, const QString &project, const QString &kind,
                                               const QString &version, const QStringList &optional) {
    if (version.isEmpty()) return;
    QVariantMap request{{"action", kind == "modpack" ? "modpack_install" : "content_add"},
        {"id", id}, {"project", project}, {"kind", kind}, {"version", version}};
    // Con `optional` el motor responde {instances, skippedOptional}; sin él, el array de siempre.
    if (kind != "modpack" && !optional.isEmpty()) request.insert("optional", optional);
    startCatalogRequest(request, true);
}

bool LauncherController::setInstanceRam(const QString &id, int ramMb) {
    return updateLibrary({{"action", "instance_ram"}, {"id", id}, {"ram", ramMb}});
}

void LauncherController::duplicateInstance(const QString &id) {
    startFileOperation({{"action", "duplicate"}, {"id", id}}, "Copiando los archivos de la instancia…");
}

void LauncherController::exportInstance(const QString &id, const QUrl &directory) {
    if (!directory.isLocalFile()) return;
    startFileOperation({{"action", "export"}, {"id", id}, {"directory", directory.toLocalFile()}}, "Exportando instancia…");
}

void LauncherController::importInstance(const QUrl &directory) {
    if (!directory.isLocalFile()) return;
    startFileOperation({{"action", "import"}, {"directory", directory.toLocalFile()}}, "Importando instancia…");
}

void LauncherController::importModpack(const QUrl &file) {
    if (!file.isLocalFile()) return;
    startFileOperation({{"action", "modpack_import"}, {"file", file.toLocalFile()}}, "Importando archivos del modpack…");
}

void LauncherController::startFileOperation(const QVariantMap &request, const QString &message) {
    if (catalogBusy()) return;
    if (gameBusy()) {
        m_copyMessage = "Cierra Minecraft antes de continuar.";
        emit copyChanged();
        return;
    }
    m_copyingInstance = true;
    m_copyMessage = message;
    emit copyChanged();
    startCatalogRequest(request, true);
}

void LauncherController::startCatalogRequest(QVariantMap request, bool writesLibrary) {
    if (catalogBusy() || (writesLibrary && gameBusy())) return;
    m_catalogPending = true;
    m_catalogMessage.clear();
    // content_toggle/content_remove no registran operación en el motor: no hay progreso que mostrar.
    const auto action = request.value("action").toString();
    if (writesLibrary && action != "content_toggle" && action != "content_remove") beginOperation(action);
    if (!writesLibrary) {
        m_catalogResults.clear();
        m_catalogInstance.clear();
        m_catalogKind.clear();
        m_catalogPage = 0;
        m_catalogTotal = 0;
        m_catalogFilters.clear();
    }
    emit catalogChanged();
    request.insert("root", m_libraryRoot);
    m_catalog.setFuture(QtConcurrent::run([request, writesLibrary] {
        QLockFile lock(QDir(request.value("root").toString()).filePath("library.lock"));
        lock.setStaleLockTime(0);
        QString error;
        QVariant result;
        if (writesLibrary && !lock.tryLock())
            error = "La biblioteca está en uso.";
        else
            result = requestEngine(request, error);
        const QVariantMap filters{{"category", request.value("category")}, {"sort", request.value("sort")},
                                  {"query", request.value("query")}};
        return QVariantMap{{"result", result}, {"error", error}, {"write", writesLibrary},
                           {"id", request.value("id")}, {"action", request.value("action")},
                           {"kind", request.value("kind")}, {"filters", filters}};
    }));
}

void LauncherController::persist() {
    m_settings.setValue("game/minimize_on_launch", m_minimizeOnLaunch);
    m_settings.setValue("game/refocus_on_exit", m_refocusOnExit);
    m_settings.setValue("game/gc_preset", m_gcPreset);
    m_settings.setValue("game/pre_touch_memory", m_preTouchMemory);
    m_settings.setValue("game/high_priority", m_highPriority);
    m_settings.setValue("game/auto_sodium", m_autoSodium);
    m_settings.setValue("game/window_width", m_windowWidth);
    m_settings.setValue("game/window_height", m_windowHeight);
    m_settings.setValue("game/fullscreen", m_fullscreen);
    m_settings.setValue("game/custom_jvm_args", m_customJvmArgs);
    m_settings.setValue("game/download_concurrency", m_downloadConcurrency);
    m_settings.setValue("game/java_major", m_javaMajor);
    m_settings.setValue("ui/close_warning", m_closeWarning);
    m_settings.setValue("ui/memory_warning", m_memoryWarning);
    m_settings.setValue("game/show_snapshots", m_showSnapshots);
    m_settings.setValue("ui/crash_report", m_crashReport);
    m_settings.setValue("ui/compact_library", m_compactLibrary);
    m_settings.setValue("game/pre_launch_hook", m_preLaunchHook);
    m_settings.setValue("game/post_exit_hook", m_postExitHook);
    m_settings.setValue("game/wrapper_command", m_wrapperCommand);
    m_settings.setValue("game/java_paths", m_javaPaths);
    m_settings.setValue("appearance/theme", m_themeId);
    m_settings.setValue("appearance/particles", m_particles);
    m_settings.setValue("appearance/spotlight", m_spotlight);
    m_settings.setValue("appearance/potato_mode", m_potatoMode);
    m_settings.setValue("game/ram_mb", m_ram);
    m_settings.sync();
    if (m_settings.status() != QSettings::NoError) {
        m_linkError = "No se pudieron guardar los ajustes de esta vista previa.";
        emit linkErrorChanged();
    }
    emit preferencesChanged();
}

QString LauncherController::jvmFlagsPreview() const {
    const int ram = std::max(1024, m_ram);
    const int initial = m_gcPreset == "low_memory" ? 512 : 1024;
    QStringList flags{QString("-Xms%1M").arg(std::min(initial, ram)), QString("-Xmx%1M").arg(ram)};
    if (m_preTouchMemory) flags.append("-XX:+AlwaysPreTouch");
    if (m_gcPreset == "generational_zgc") {
        flags.append("-XX:+UseZGC");
        flags.append("-XX:+ZGenerational");
    } else if (m_gcPreset == "low_memory") {
        flags.append("-XX:+UseG1GC");
        flags.append("-XX:+UseStringDeduplication");
    } else if (m_gcPreset != "automatic") {
        flags.append("-XX:+UseG1GC");
        flags.append("-XX:+ParallelRefProcEnabled");
        flags.append("-XX:MaxGCPauseMillis=200");
    }
    for (const auto &token : m_customJvmArgs.split(QRegularExpression("\\s+"), Qt::SkipEmptyParts)) {
        const auto lower = token.toLower();
        if (token.startsWith('-') && !lower.contains("formatmsgnolookups") && !lower.contains("log4j.configurationfile"))
            flags.append(token);
    }
    return flags.join(' ');
}

void LauncherController::setPotatoMode(bool value) {
    if (m_potatoMode == value) return;
    m_potatoMode = value;
    if (value) {
        m_particles = false;
        if (m_gcPreset == "automatic") m_gcPreset = "low_memory";
    }
    persist();
}

void LauncherController::setAutoSodium(bool value) {
    if (m_autoSodium == value) return;
    m_autoSodium = value;
    persist();
}

void LauncherController::setWindowWidth(int value) {
    const int clamped = std::clamp(value, 320, 7680);
    if (m_windowWidth == clamped) return;
    m_windowWidth = clamped;
    persist();
}

void LauncherController::setWindowHeight(int value) {
    const int clamped = std::clamp(value, 240, 4320);
    if (m_windowHeight == clamped) return;
    m_windowHeight = clamped;
    persist();
}

void LauncherController::setFullscreen(bool value) {
    if (m_fullscreen == value) return;
    m_fullscreen = value;
    persist();
}

void LauncherController::setCustomJvmArgs(const QString &value) {
    const auto trimmed = value.left(512);
    if (m_customJvmArgs == trimmed) return;
    m_customJvmArgs = trimmed;
    persist();
}

void LauncherController::setDownloadConcurrency(int value) {
    const int clamped = std::clamp(value, 1, 20);
    if (m_downloadConcurrency == clamped) return;
    m_downloadConcurrency = clamped;
    downloadConcurrencyShared = clamped;
    persist();
}

void LauncherController::setJavaMajor(int value) {
    if (m_javaMajor == value || !javaMajors.contains(value)) return;
    m_javaMajor = value;
    persist();
}

void LauncherController::setCloseWarning(bool value) {
    if (m_closeWarning == value) return;
    m_closeWarning = value;
    persist();
}

void LauncherController::setMemoryWarning(bool value) {
    if (m_memoryWarning == value) return;
    m_memoryWarning = value;
    persist();
}

bool LauncherController::setLaunchCommands(const QString &preLaunch, const QString &postExit, const QString &wrapper) {
    const auto pre = preLaunch.trimmed(), post = postExit.trimmed(), wrap = wrapper.trimmed();
    if (!commandValid(pre) || !commandValid(post) || !commandValid(wrap)) return false;
    if (pre == m_preLaunchHook && post == m_postExitHook && wrap == m_wrapperCommand) return true;
    m_preLaunchHook = pre;
    m_postExitHook = post;
    m_wrapperCommand = wrap;
    persist();
    return true;
}

bool LauncherController::setInstanceSettings(const QString &id, const QVariantMap &fields) {
    QVariantMap request = fields;
    if (const auto java = fields.value("javaPath"); java.metaType().id() == QMetaType::QUrl)
        request.insert("javaPath", QDir::toNativeSeparators(java.toUrl().toLocalFile()));
    request.insert("action", "instance_settings");
    request.insert("id", id);
    return updateLibrary(request);
}

void LauncherController::setShowSnapshots(bool value) {
    if (m_showSnapshots == value) return;
    m_showSnapshots = value;
    persist();
}

void LauncherController::setCrashReport(bool value) {
    if (m_crashReport == value) return;
    m_crashReport = value;
    persist();
}

void LauncherController::setCompactLibrary(bool value) {
    if (m_compactLibrary == value) return;
    m_compactLibrary = value;
    persist();
}

void LauncherController::loadMinecraftVersions(bool refresh) {
    if (m_versionsLoading) return;
    if (!refresh && !m_minecraftVersions.isEmpty()) return;
    m_versionsLoading = true;
    m_versionsError.clear();
    emit versionsChanged();
    const QVariantMap request = {{"action", "minecraft_versions"}, {"root", m_libraryRoot}, {"refresh", refresh}};
    m_versionsWatcher.setFuture(QtConcurrent::run([request] {
        QString error;
        const auto result = requestEngine(request, error);
        return QVariantMap{{"error", error}, {"result", result}};
    }));
}

void LauncherController::styleWindowFrame(QObject *window, const QColor &caption, const QColor &text, const QColor &border, bool dark) {
    ::styleWindowFrame(qobject_cast<QWindow *>(window), caption, text, border, dark);
}

void LauncherController::copyText(const QString &text) {
    QGuiApplication::clipboard()->setText(text.left(200000));
}

void LauncherController::refreshStorage() {
    if (m_storagePending) return;
    m_storagePending = true;
    m_storageMessage.clear();
    emit storageChanged();
    const QVariantMap request = {{"action", "storage_usage"}, {"root", m_libraryRoot}};
    m_storage.setFuture(QtConcurrent::run([request] {
        QString error;
        const auto result = requestEngine(request, error);
        return QVariantMap{{"error", error}, {"result", result}};
    }));
}

void LauncherController::clearStorage(const QString &kind) {
    if (m_storagePending || gameBusy() || catalogBusy()) {
        m_storageMessage = "Espera a que termine la operación en curso.";
        emit storageChanged();
        return;
    }
    m_storagePending = true;
    m_storageMessage.clear();
    emit storageChanged();
    const QVariantMap request = {{"action", "storage_clear"}, {"root", m_libraryRoot}, {"kind", kind}};
    m_storage.setFuture(QtConcurrent::run([request] {
        QLockFile lock(QDir(request.value("root").toString()).filePath("library.lock"));
        lock.setStaleLockTime(0);
        if (!lock.tryLock()) return QVariantMap{{"error", QString("La biblioteca está en uso en otra ventana.")}};
        QString error;
        const auto result = requestEngine(request, error);
        return QVariantMap{{"error", error}, {"result", result}, {"done", QString("Limpieza completada.")}};
    }));
}

void LauncherController::setMinimizeOnLaunch(bool value) {
    if (m_minimizeOnLaunch == value) return;
    m_minimizeOnLaunch = value;
    persist();
}

void LauncherController::setRefocusOnExit(bool value) {
    if (m_refocusOnExit == value) return;
    m_refocusOnExit = value;
    persist();
}

void LauncherController::setGcPreset(const QString &value) {
    if (m_gcPreset == value || !gcPresets.contains(value)) return;
    m_gcPreset = value;
    persist();
}

void LauncherController::setPreTouchMemory(bool value) {
    if (m_preTouchMemory == value) return;
    m_preTouchMemory = value;
    persist();
}

void LauncherController::setHighPriority(bool value) {
    if (m_highPriority == value) return;
    m_highPriority = value;
    persist();
}

bool LauncherController::setJavaPath(int major, const QUrl &url) {
    if (major != 8 && major != 16 && major != 17 && major != 21 && major != 25) return false;
    const auto key = QString::number(major);
    if (url.isEmpty()) {
        m_javaPaths.remove(key);
        persist();
        checkJava(major);
        return true;
    }
    const QFileInfo file(url.toLocalFile());
    const auto name = file.fileName().toLower();
#ifdef Q_OS_WIN
    const bool validJava = name == "java.exe" || name == "javaw.exe";
#else
    const bool validJava = name == "java" && file.isExecutable();
#endif
    if (!url.isLocalFile() || !file.isFile() || !validJava) return false;
    QString error;
    const auto verified = requestEngine({{"action", "java_check"}, {"root", m_libraryRoot},
                                         {"major", major}, {"path", file.absoluteFilePath()}}, error);
    if (!error.isEmpty()) {
        m_javaMessage = error;
        emit javaChanged();
        return false;
    }
    m_javaStatus = verified.toMap();
    m_javaMessage = QString("Java %1 verificado desde la ruta manual.").arg((m_javaStatus.value("installed_major").toInt() ? m_javaStatus.value("installed_major").toInt() : major));
    m_javaPaths.insert(key, file.absoluteFilePath());
    persist();
    emit javaChanged();
    return true;
}

void LauncherController::checkJava(int major) {
    if (m_javaPending || (major != 8 && major != 16 && major != 17 && major != 21 && major != 25)) return;
    m_javaPending = true;
    m_javaMessage = QString("Verificando Java %1…").arg(major);
    emit javaChanged();
    QVariantMap request{{"action", "java_check"}, {"root", m_libraryRoot}, {"major", major}};
    const auto custom = m_javaPaths.value(QString::number(major)).toString();
    if (!custom.isEmpty()) request.insert("path", custom);
    m_java.setFuture(QtConcurrent::run([request] {
        QString error;
        const auto result = requestEngine(request, error);
        return QVariantMap{{"result", result}, {"error", error}, {"action", "java_check"}};
    }));
}

void LauncherController::installJava(int major) {
    if (m_javaPending || gameBusy() || (major != 8 && major != 16 && major != 17 && major != 21 && major != 25)) return;
    m_javaPending = true;
    m_javaMessage = QString("Descargando e instalando Java %1 oficial desde Adoptium…").arg(major);
    emit javaChanged();
    const QVariantMap request{{"action", "java_install"}, {"root", m_libraryRoot}, {"major", major}};
    m_java.setFuture(QtConcurrent::run([request] {
        QString error;
        const auto result = requestEngine(request, error);
        return QVariantMap{{"result", result}, {"error", error}, {"action", "java_install"}};
    }));
}

void LauncherController::openDataDirectory() {
    m_linkError = QDesktopServices::openUrl(QUrl::fromLocalFile(m_libraryRoot)) ? QString() : "No se pudo abrir la carpeta de OpenClient.";
    emit linkErrorChanged();
}

void LauncherController::setThemeId(const QString &requested) {
    // Solo para capturas de prueba: OPENCLIENT_QA_THEME fuerza el tema en todas las superficies.
    const auto forced = qEnvironmentVariable("OPENCLIENT_QA_THEME");
    const auto &value = (!forced.isEmpty() && QCoreApplication::arguments().contains("--smoke-test") && themes.contains(forced)) ? forced : requested;
    if (value == m_themeId || !themes.contains(value)) return;
    m_themeId = value;
    persist();
}

void LauncherController::setParticlesEnabled(bool value) {
    if (value == m_particles) return;
    m_particles = value;
    persist();
}

void LauncherController::setSpotlightEnabled(bool value) {
    if (value == m_spotlight) return;
    m_spotlight = value;
    persist();
}

void LauncherController::setAllocatedRamMb(int value) {
    const int maximum = m_hardware.value("installed_ram_mb").toInt();
    if (maximum < 1024 || value < 1024 || value > maximum || value == m_ram) return;
    m_ram = value;
    persist();
}

namespace {
QString engineLibraryPath() {
    const QDir application(QCoreApplication::applicationDirPath());
#ifdef Q_OS_WIN
    return application.filePath("openclient_ffi.dll");
#elif defined(Q_OS_MACOS)
    const auto bundled = application.filePath("../Frameworks/libopenclient_ffi.dylib");
    return QFileInfo(bundled).isFile() ? QDir::cleanPath(bundled)
                                       : application.filePath("libopenclient_ffi.dylib");
#else
    return application.filePath("libopenclient_ffi.so");
#endif
}
}

QVariantMap LauncherController::readHardware() {
    // Cargar solo la biblioteca del propio directorio, nunca una ruta suministrada por QML.
    QLibrary library(engineLibraryPath());
    library.setLoadHints(QLibrary::PreventUnloadHint);
    if (!library.load()) return {{"error", "No se pudo cargar el motor Rust de esta compilación."}};
    using Version = unsigned int (*)();
    using Hardware = char *(*)();
    using Free = void (*)(char *);
    const auto version = reinterpret_cast<Version>(library.resolve("openclient_abi_version"));
    const auto hardware = reinterpret_cast<Hardware>(library.resolve("openclient_hardware_json"));
    const auto release = reinterpret_cast<Free>(library.resolve("openclient_free_string"));
    if (!version || !hardware || !release || version() != 3) return {{"error", "El puente Rust y la interfaz tienen versiones incompatibles."}};
    char *raw = hardware();
    if (!raw) return {{"error", "El motor no pudo consultar el hardware."}};
    const auto document = QJsonDocument::fromJson(QByteArray(raw));
    release(raw);
    const auto object = document.object();
    if (!object.value("ok").toBool() || !object.value("hardware").isObject()) {
        return {{"error", "Respuesta de hardware no válida."}};
    }
    const auto result = object.value("hardware").toObject().toVariantMap();
    if (result.value("installed_ram_mb").toLongLong() <= 0) return {{"error", "No se pudo determinar la memoria instalada."}};
    return result;
}

bool LauncherController::updateLibrary(QVariantMap request) {
    QLockFile lock(QDir(m_libraryRoot).filePath("library.lock"));
    lock.setStaleLockTime(0);
    const bool readingUnderGameLock = gameBusy() && request.value("action").toString() == "list";
    if (!readingUnderGameLock && (gameBusy() || !lock.tryLock())) {
        m_libraryError = "La biblioteca está ocupada. Inténtalo de nuevo.";
        emit libraryChanged();
        return false;
    }
    request.insert("root", m_libraryRoot);
    const auto result = requestEngine(request, m_libraryError);
    if (m_libraryError.isEmpty()) m_instances = withDiskMods(result.toList());
    emit libraryChanged();
    return m_libraryError.isEmpty();
}

QVariant LauncherController::requestEngine(const QVariantMap &request, QString &error) {
    error.clear();
    auto fail = [&error](const QString &message) -> QVariant {
        error = message;
        return {};
    };
    QLibrary library(engineLibraryPath());
    library.setLoadHints(QLibrary::PreventUnloadHint);
    if (!library.load()) return fail("No se pudo cargar el motor Rust.");
    using Operation = char *(*)(const char *, size_t);
    using Free = void (*)(char *);
    using Version = unsigned int (*)();
    const auto operation = reinterpret_cast<Operation>(library.resolve("openclient_request_json"));
    const auto release = reinterpret_cast<Free>(library.resolve("openclient_free_string"));
    const auto version = reinterpret_cast<Version>(library.resolve("openclient_abi_version"));
    if (!operation || !release || !version || version() != 3) return fail("El motor Rust es incompatible.");
    QVariantMap payload = request;
    payload.insert("downloadConcurrency", downloadConcurrencyShared.load());
    const auto input = QJsonDocument::fromVariant(payload).toJson(QJsonDocument::Compact);
    char *raw = operation(input.constData(), static_cast<size_t>(input.size()));
    if (!raw) return fail("No se pudo procesar la solicitud.");
    const auto result = QJsonDocument::fromJson(QByteArray(raw)).object();
    release(raw);
    if (!result.value("ok").toBool()) return fail(result.value("error").toString("No se pudo leer la biblioteca."));
    return result.value("result").toVariant();
}

void LauncherController::requestPlayInstance(const QString &id) {
    if (gameRunning() && !id.isEmpty() && m_preparingId.isEmpty() && !m_runningIds.contains(id) && !catalogBusy()) {
        emit launchAccountRequested(id);
    } else {
        playInstance(id);
    }
}

void LauncherController::playInstance(const QString &id) {
    launchInstance(id, m_instanceAccounts.value(id).toMap());
}

void LauncherController::playInstanceWithAccount(const QString &id, const QString &kind, const QString &key) {
    launchInstance(id, {{"kind", kind}, {"key", key}});
}

void LauncherController::launchInstance(const QString &id, const QVariantMap &account) {
    if (id.isEmpty() || !m_preparingId.isEmpty() || m_runningIds.contains(id)) return;
    if (catalogBusy()) {
        m_gameMessage = "Espera a que termine la operación de contenido.";
        emit gameChanged();
        return;
    }
    // Cuenta elegida para este lanzamiento, asignada a la instancia o activa del launcher.
    bool useMicrosoft = m_useMicrosoft;
    QString username = m_accountName;
    QString microsoftId = m_microsoftId;
    if (!account.isEmpty()) {
        const auto kind = account.value("kind").toString();
        const auto key = account.value("key").toString();
        QVariantMap entry;
        for (const auto &candidate : std::as_const(m_microsoftAccounts))
            if (candidate.toMap().value("id").toString() == key) entry = candidate.toMap();
        if (kind == "offline" && m_offlineProfiles.contains(key)) {
            useMicrosoft = false;
            username = key;
        } else if (kind == "microsoft" && entry.value("status").toString() == "ok" && !microsoftBusy()) {
            useMicrosoft = true;
            username = entry.value("username").toString();
            microsoftId = key;
        } else {
            m_gameMessage = "La cuenta elegida para esta instancia no está disponible. Cámbiala en Detalles o vuelve a conectarla.";
            emit gameChanged();
            return;
        }
    } else if (accountName().isEmpty() || (m_useMicrosoft && (m_microsoftNeedsLogin || microsoftBusy()))) {
        m_gameMessage = m_useMicrosoft ? "Revisa la sesión de Microsoft en Cuenta." :
            "Guarda un nombre de jugador en Cuenta para iniciar Minecraft.";
        emit gameChanged();
        return;
    }
    if (!m_gameLock) {
        auto lock = std::make_unique<QLockFile>(QDir(m_libraryRoot).filePath("library.lock"));
        lock->setStaleLockTime(0);
        if (!lock->tryLock()) {
            m_gameMessage = "La biblioteca está en uso por otra ventana.";
            emit gameChanged();
            return;
        }
        m_gameLock = std::move(lock);
    }
    m_preparingId = id;
    m_launchAccountId = useMicrosoft ? microsoftId : QString();
    m_gameMessage.clear();
    m_launchMessage = "Preparando Minecraft…";
    m_gamePercent = 0;
    m_launchPending = true;
    emit gameChanged();
    const QVariantMap request = {{"action", "launch"}, {"root", m_libraryRoot},
                                 {"id", id}, {"username", username}, {"ram", m_ram}, {"javaPaths", m_javaPaths},
                                 {"gcPreset", m_gcPreset}, {"preTouchMemory", m_preTouchMemory}, {"highPriority", m_highPriority},
                                 {"windowWidth", m_windowWidth}, {"windowHeight", m_windowHeight},
                                 {"fullscreen", m_fullscreen}, {"customJvmArgs", m_customJvmArgs},
                                 {"minimizeOnLaunch", m_minimizeOnLaunch}, {"refocusOnExit", m_refocusOnExit},
                                 {"preLaunchHook", m_preLaunchHook}, {"postExitHook", m_postExitHook}, {"wrapperCommand", m_wrapperCommand},
                                 {"accountType", useMicrosoft ? "microsoft" : "offline"}, {"accountId", microsoftId}};
    m_launch.setFuture(QtConcurrent::run([request] {
        QLockFile accountLock(QDir(request.value("root").toString()).filePath("account.lock"));
        accountLock.setStaleLockTime(0);
        if (request.value("accountType").toString() == "microsoft" && !accountLock.tryLock())
            return QString("La sesión está en uso en otra ventana. Intenta de nuevo.");
        QString error;
        requestEngine(request, error);
        return error;
    }));
    m_gameTimer.start();
}

void LauncherController::pollGame() {
    if (!gameBusy()) return;
    auto ids = m_runningIds;
    if (!m_preparingId.isEmpty()) ids.append(m_preparingId);
    bool startedGame = false;
    QStringList exitedGames;
    QList<QPair<QString, int>> crashes;
    for (const auto &id : std::as_const(ids)) {
        const bool preparing = id == m_preparingId;
        const bool pending = preparing && m_launchPending;
        QString error;
        const auto state = requestEngine({{"action", "game_status"}, {"id", id}, {"consume", !pending}}, error).toMap();
        if (!error.isEmpty() || state.value("instanceId").toString() != id) continue;
        const auto phase = state.value("phase").toString();
        if (preparing && phase == "preparing") {
            m_launchMessage = state.value("message").toString();
            m_gamePercent = state.value("percent").toInt();
        } else if (preparing && !pending && phase == "running") {
            m_preparingId.clear();
            m_runningIds.append(id);
            m_gamePercent = 100;
            startedGame = true;
        } else if (!pending && phase == "idle" && state.contains("exitCode")) {
            if (preparing) m_preparingId.clear();
            m_runningIds.removeOne(id);
            m_gameMessage = state.value("message").toString();
            if (ids.size() > 1) {
                const auto name = instanceEntry(id).value("name").toString();
                if (!name.isEmpty()) m_gameMessage.prepend(name + " · ");
            }
            exitedGames.append(id);
            const auto exitCode = state.value("exitCode");
            if (m_crashReport && !exitCode.isNull() && exitCode.toInt() != 0)
                crashes.append({id, exitCode.toInt()});
        }
    }
    releaseGameLockIfIdle();
    if (!exitedGames.isEmpty()) {
        reloadLibrary();
        if (gameBusy()) {
            for (const auto &id : std::as_const(exitedGames)) requestBanner(id);
        }
    }
    emit gameChanged();
    if (startedGame && m_minimizeOnLaunch) emit minimizeRequested();
    else if (!exitedGames.isEmpty() && m_refocusOnExit) emit refocusRequested();
    for (const auto &[id, code] : std::as_const(crashes)) {
        analyzeCrash(id);
        emit gameCrashed(id, code);
    }
}

void LauncherController::releaseGameLockIfIdle() {
    if (!m_preparingId.isEmpty() || gameRunning()) return;
    m_gameTimer.stop();
    m_gameLock.reset();
}

QString LauncherController::instanceLog(const QString &id) {
    QString error;
    const auto result = requestEngine({{"action", "game_log"}, {"root", m_libraryRoot}, {"id", id}}, error);
    return error.isEmpty() ? result.toString() : error;
}

void LauncherController::saveOfflineProfiles() {
    m_settings.setValue("account/offline_profiles", m_offlineProfiles);
}

bool LauncherController::persistAccount(const QString &username) {
    const auto previousProfiles = m_offlineProfiles;
    // Un perfil offline usado pasa a la lista (máximo 20); el activo queda en `offline_name`.
    if (!username.isEmpty() && !m_offlineProfiles.contains(username) && m_offlineProfiles.size() < 20) m_offlineProfiles.append(username);
    m_settings.setValue("account/offline_name", username);
    saveOfflineProfiles();
    m_settings.sync();
    if (m_settings.status() != QSettings::NoError) {
        m_offlineProfiles = previousProfiles;
        m_settings.setValue("account/offline_name", m_accountName);
        saveOfflineProfiles();
        m_accountError = "No se pudo guardar la cuenta local.";
    } else {
        m_accountName = username;
        m_accountError.clear();
    }
    emit accountChanged();
    return m_accountError.isEmpty();
}

bool LauncherController::setOfflineAccount(const QString &username) {
    if (gameBusy() || microsoftBusy()) return false;
    const auto result = requestEngine({{"action", "offline"}, {"username", username}}, m_accountError);
    if (m_accountError.isEmpty()) {
        if (!persistAccount(result.toMap().value("username").toString())) return false;
        return selectMicrosoft(false);
    }
    emit accountChanged();
    return false;
}

bool LauncherController::selectOfflineProfile(const QString &username) {
    if (gameBusy() || microsoftBusy() || !m_offlineProfiles.contains(username)) return false;
    if (!persistAccount(username)) return false;
    return selectMicrosoft(false);
}

// Quita un perfil de la lista; si era el activo, deja de haber perfil offline activo.
void LauncherController::removeOfflineProfile(const QString &username) {
    if (gameBusy() || microsoftBusy() || !m_offlineProfiles.contains(username)) return;
    const auto previous = m_offlineProfiles;
    m_offlineProfiles.removeAll(username);
    saveOfflineProfiles();
    dropInstanceAccounts("offline", username);
    if (username == m_accountName) {
        persistAccount({});
        return;
    }
    m_settings.sync();
    if (m_settings.status() != QSettings::NoError) {
        m_offlineProfiles = previous;
        m_accountError = "No se pudo guardar la cuenta local.";
    } else {
        m_accountError.clear();
    }
    emit accountChanged();
}

void LauncherController::saveInstanceAccounts() {
    m_settings.setValue("account/instance_accounts",
                        QString::fromUtf8(QJsonDocument(QJsonObject::fromVariantMap(m_instanceAccounts)).toJson(QJsonDocument::Compact)));
    m_settings.sync();
}

void LauncherController::dropInstanceAccounts(const QString &kind, const QString &key) {
    bool changed = false;
    for (auto it = m_instanceAccounts.begin(); it != m_instanceAccounts.end();) {
        const auto own = it.value().toMap();
        if (own.value("kind").toString() == kind && own.value("key").toString() == key) {
            it = m_instanceAccounts.erase(it);
            changed = true;
        } else ++it;
    }
    if (changed) saveInstanceAccounts();
}

bool LauncherController::setInstanceAccount(const QString &instanceId, const QString &kind, const QString &key) {
    if (instanceId.isEmpty() || m_preparingId == instanceId || m_runningIds.contains(instanceId)) return false;
    const auto previous = m_instanceAccounts;
    if (kind.isEmpty()) {
        m_instanceAccounts.remove(instanceId);
    } else {
        const bool valid = kind == "offline" ? m_offlineProfiles.contains(key)
            : kind == "microsoft" && std::any_of(m_microsoftAccounts.cbegin(), m_microsoftAccounts.cend(),
                [&key](const QVariant &entry) { return entry.toMap().value("id").toString() == key; });
        if (!valid) return false;
        m_instanceAccounts.insert(instanceId, QVariantMap{{"kind", kind}, {"key", key}});
    }
    saveInstanceAccounts();
    if (m_settings.status() != QSettings::NoError) {
        m_instanceAccounts = previous;
        m_accountError = "No se pudo guardar la cuenta de la instancia.";
    } else {
        m_accountError.clear();
    }
    emit accountChanged();
    return m_accountError.isEmpty();
}

void LauncherController::clearOfflineAccount() {
    if (!gameBusy() && !microsoftBusy()) removeOfflineProfile(m_accountName);
}

bool LauncherController::selectMicrosoft(bool value) {
    m_settings.setValue("account/use_microsoft", value);
    m_settings.sync();
    if (m_settings.status() != QSettings::NoError) {
        m_settings.setValue("account/use_microsoft", m_useMicrosoft);
        m_accountError = "No se pudo guardar la cuenta activa.";
    } else {
        m_useMicrosoft = value;
        m_accountError.clear();
    }
    emit accountChanged();
    return m_accountError.isEmpty();
}

void LauncherController::startMicrosoftLogin(bool directBrowser) {
    if (gameBusy() || microsoftBusy() || m_diagnosing) return;
    auto lock = std::make_unique<QLockFile>(QDir(m_libraryRoot).filePath("account.lock"));
    lock->setStaleLockTime(0);
    if (!lock->tryLock()) {
        m_microsoftMessage = "La sesión está en uso en otra ventana.";
        emit accountChanged();
        return;
    }
    m_authAttempt = requestEngine({{"action", "auth_prepare"}}, m_microsoftMessage).toULongLong();
    if (!m_authAttempt) {
        emit accountChanged();
        return;
    }
    m_authLock = std::move(lock);
    m_authCancelled = false;
    m_microsoftPrompt.clear();
    m_microsoftMessage = directBrowser
        ? "Abriendo Microsoft en tu navegador…"
        : "Generando código y abriendo Microsoft en tu navegador…";
    emit accountChanged();
    const QVariantMap request{{"action", "auth_login"}, {"root", m_libraryRoot},
                              {"mode", directBrowser ? "loopback" : "device"},
                              {"attempt", QVariant::fromValue(m_authAttempt)}};
    m_auth.setFuture(QtConcurrent::run([request] {
        QString error;
        const auto profile = requestEngine(request, error);
        return QVariantMap{{"profile", profile}, {"error", error}, {"select", true},
                           {"message", "Cuenta de Minecraft Java conectada."}};
    }));
    m_authTimer.start();
}

void LauncherController::pollMicrosoftPrompt() {
    if (m_authCancelled || !m_microsoftPrompt.isEmpty()) return;
    QString error;
    const auto prompt = requestEngine({{"action", "auth_prompt"},
        {"attempt", QVariant::fromValue(m_authAttempt)}}, error).toMap();
    if (prompt.isEmpty()) return;
    m_microsoftPrompt = prompt;
    if (!microsoftCode().isEmpty()) {
        copyMicrosoftCode();
        openMicrosoftVerification();
        m_microsoftMessage = "Código copiado al portapapeles y navegador abierto. Pégalo (Ctrl+V) en Microsoft para confirmar.";
    } else {
        openMicrosoftVerification();
        m_microsoftMessage = "Completa el acceso en la ventana de Microsoft que se abrió en tu navegador. OpenClient continúa solo al terminar.";
    }
    emit accountChanged();
}

void LauncherController::cancelMicrosoftLogin() {
    if (!m_authAttempt || m_authCancelled) return;
    m_authCancelled = true;
    QString error;
    requestEngine({{"action", "auth_cancel"}, {"attempt", QVariant::fromValue(m_authAttempt)}}, error);
    m_microsoftPrompt.clear();
    m_microsoftMessage = "Cancelando el inicio de sesión…";
    emit accountChanged();
}

void LauncherController::openMicrosoftVerification() {
    const QUrl url(m_microsoftPrompt.value("verification_uri").toString());
    const bool isDevice = (url.host() == "microsoft.com" || url.host() == "www.microsoft.com")
        && (url.path() == "/link" || url.path() == "/devicelogin") && !url.hasQuery();
    const bool isLoopback = url.host() == "login.microsoftonline.com"
        && url.path() == "/consumers/oauth2/v2.0/authorize";
    if (url.scheme() != "https" || (!isDevice && !isLoopback) || url.hasFragment()
        || !url.userInfo().isEmpty() || url.port() != -1) return;
    if (!QDesktopServices::openUrl(url)) {
        m_microsoftMessage = "No se pudo abrir el navegador.";
        emit accountChanged();
    }
}

void LauncherController::refreshMicrosoftProfile() {
    if (gameBusy() || microsoftBusy() || m_microsoftProfile.isEmpty()) return;
    const QVariantMap request{{"action", "auth_refresh"}, {"root", m_libraryRoot}, {"accountId", m_microsoftId}};
    m_auth.setFuture(QtConcurrent::run([request] {
        QString error;
        const auto profile = requestEngine(request, error);
        return QVariantMap{{"profile", profile}, {"error", error},
                           {"message", "Skin y capas sincronizadas con tu cuenta de Minecraft Java."}};
    }));
}

void LauncherController::equipMicrosoftCape(const QString &capeId) {
    if (gameBusy() || microsoftBusy() || m_microsoftProfile.isEmpty()) return;
    m_microsoftMessage = capeId.isEmpty() ? "Ocultando capa oficial…" : "Equipando capa oficial en tu cuenta de Minecraft…";
    emit accountChanged();
    const QVariantMap request{{"action", "auth_equip_cape"}, {"root", m_libraryRoot}, {"accountId", m_microsoftId},
                              {"capeId", capeId}};
    m_auth.setFuture(QtConcurrent::run([request, capeId] {
        QString error;
        const auto profile = requestEngine(request, error);
        return QVariantMap{{"profile", profile}, {"error", error},
                           {"message", capeId.isEmpty() ? "Capa oficial desactivada." : "Capa equipada en tu cuenta de Minecraft Java."}};
    }));
}

void LauncherController::copyMicrosoftCode() {
    if (!microsoftCode().isEmpty()) QGuiApplication::clipboard()->setText(microsoftCode());
}

void LauncherController::useMicrosoftProfile() {
    if (!gameBusy() && !microsoftBusy() && !m_microsoftNeedsLogin && !m_microsoftProfile.isEmpty())
        selectMicrosoft(true);
}

void LauncherController::forgetMicrosoftProfile() {
    forgetMicrosoftAccount(m_microsoftId);
}

void LauncherController::forgetMicrosoftAccount(const QString &id) {
    if (gameBusy() || microsoftBusy() || m_diagnosing || id.isEmpty()) return;
    QLockFile lock(QDir(m_libraryRoot).filePath("account.lock"));
    lock.setStaleLockTime(0);
    if (!lock.tryLock()) {
        m_microsoftMessage = "La sesión está en uso en otra ventana.";
    } else {
        requestEngine({{"action", "auth_forget"}, {"root", m_libraryRoot}, {"accountId", id}}, m_microsoftMessage);
        if (m_microsoftMessage.isEmpty()) {
            if (id == m_microsoftId) m_microsoftId.clear();
            dropInstanceAccounts("microsoft", id);
            reloadMicrosoftAccounts();
            m_microsoftNeedsLogin = false;
            if (m_microsoftProfile.isEmpty()) selectMicrosoft(false);
        }
    }
    emit accountChanged();
}

void LauncherController::selectMicrosoftAccount(const QString &id) {
    if (gameBusy() || microsoftBusy() || m_diagnosing || id.isEmpty()) return;
    const bool known = std::any_of(m_microsoftAccounts.cbegin(), m_microsoftAccounts.cend(),
                                   [&id](const QVariant &entry) { return entry.toMap().value("id").toString() == id; });
    if (!known) return;
    m_microsoftId = id;
    m_microsoftMessage.clear();
    reloadMicrosoftAccounts();
    if (!m_microsoftProfile.isEmpty()) selectMicrosoft(true);
    emit accountChanged();
}

void LauncherController::diagnoseMicrosoftAccount(const QString &id) {
    if (gameBusy() || microsoftBusy() || m_diagnosing || id.isEmpty()) return;
    m_diagnosing = true;
    m_diagnosisId = id;
    m_diagnosis.clear();
    emit diagnosisChanged();
    runQuery({{"action", "auth_diagnose"}, {"accountId", id}}, [this, id](const QVariantMap &response) {
        m_diagnosing = false;
        const auto error = response.value("error").toString();
        m_diagnosis = error.isEmpty()
            ? response.value("result").toMap()
            : QVariantMap{{"code", "error"}, {"action", "retry"}, {"detail", error}, {"steps", QVariantList()}};
        m_diagnosis.insert("accountId", id);
        m_diagnosis.remove("profile");
        if (m_diagnosis.value("code").toString() == "ok") {
            reloadMicrosoftAccounts();
            emit accountChanged();
        }
        emit diagnosisChanged();
    });
}

// Solo acciones fijas: la interfaz nunca pasa una dirección.
void LauncherController::runDiagnosisAction(const QString &action) {
    if (action == "reconnect") {
        startMicrosoftLogin(true);
    } else if (action == "retry") {
        diagnoseMicrosoftAccount(m_diagnosisId);
    } else if (action == "open_xbox") {
        QDesktopServices::openUrl(QUrl("https://account.xbox.com/"));
    } else if (action == "open_minecraft") {
        QDesktopServices::openUrl(QUrl("https://www.minecraft.net/store/minecraft-java-bedrock-edition-pc"));
    }
}

void LauncherController::clearDiagnosis() {
    if (m_diagnosing) return;
    m_diagnosis.clear();
    emit diagnosisChanged();
}

// Lee las cuentas guardadas y fija la activa: la elegida si sigue ahí, si no la primera lista.
void LauncherController::reloadMicrosoftAccounts() {
    QString error;
    m_microsoftAccounts = requestEngine({{"action", "auth_accounts"}, {"root", m_libraryRoot}}, error).toList();
    if (!error.isEmpty()) m_microsoftMessage = error;
    QVariantMap selected;
    for (const auto &entry : std::as_const(m_microsoftAccounts)) {
        if (entry.toMap().value("id").toString() == m_microsoftId) selected = entry.toMap();
    }
    if (selected.isEmpty()) {
        for (const auto &entry : std::as_const(m_microsoftAccounts)) {
            if (entry.toMap().value("status").toString() == "ok") { selected = entry.toMap(); break; }
        }
        m_microsoftId = selected.value("id").toString();
    }
    m_settings.setValue("account/microsoft_id", m_microsoftId);
    const bool ready = selected.value("status").toString() == "ok";
    m_microsoftProfile = ready ? selected : QVariantMap();
    m_microsoftNeedsLogin = m_useMicrosoft && !ready;
}

bool LauncherController::reloadLibrary() {
    return updateLibrary({{"action", "list"}});
}

bool LauncherController::saveInstance(const QString &id, const QString &name, const QString &version, const QString &loader) {
    return updateLibrary({{"action", id.isEmpty() ? "create" : "edit"}, {"id", id},
                          {"name", name}, {"version", version}, {"loader", loader}, {"autoSodium", m_autoSodium}});
}

namespace { bool validInstanceId(const QString &id); }

bool LauncherController::deleteInstance(const QString &id) {
    const bool deleted = updateLibrary({{"action", "delete"}, {"id", id}});
    if (deleted && m_instanceAccounts.remove(id)) saveInstanceAccounts();
    if (deleted && validInstanceId(id)) m_banners->remove(id);
    return deleted;
}

bool LauncherController::toggleFavorite(const QString &id) {
    return updateLibrary({{"action", "favorite"}, {"id", id}});
}

QString LauncherController::instanceDirectory(const QString &id, const QString &category, bool create) const {
    if (QUuid::fromString(id).toString(QUuid::WithoutBraces) != id ||
        std::none_of(m_instances.cbegin(), m_instances.cend(), [&id](const QVariant &entry) {
            return entry.toMap().value("id").toString() == id;
        })) return {};
    QString suffix;
    if (category == "worlds") suffix = "/saves";
    else if (category == "screenshots") suffix = "/screenshots";
    else if (category == "mods" || category == "resourcepacks" || category == "shaderpacks" || category == "logs") suffix = "/" + category;
    else if (!category.isEmpty()) return {};
    const auto root = QDir::fromNativeSeparators(QFileInfo(m_libraryRoot).canonicalFilePath());
    if (root.isEmpty()) return {};
    const auto expected = QDir(root).filePath("instances/" + id + suffix);
    // Una instancia recién creada aún no tiene carpeta (Minecraft la crea al jugar): se crea solo al abrirla.
    if (create && !QFileInfo::exists(expected)) QDir().mkpath(expected);
    const QFileInfo directory(expected);
    if (!directory.isDir() || QDir::fromNativeSeparators(directory.canonicalFilePath()).compare(expected, Qt::CaseInsensitive) != 0)
        return {};
    return expected;
}

QVariantList LauncherController::instanceFiles(const QString &id, const QString &category) const {
    if (category.isEmpty()) return {};
    const auto path = instanceDirectory(id, category);
    if (path.isEmpty()) return {};
    const QDir directory(path);
    const bool worlds = category == "worlds";
    const bool captures = category == "screenshots";
    const auto filters = (worlds ? QDir::Dirs : QDir::Files) | QDir::NoDotAndDotDot | QDir::NoSymLinks;
    const auto names = captures ? QStringList{"*.png", "*.jpg", "*.jpeg"} :
        category == "mods" ? QStringList{"*.jar", "*.jar.disabled"} :
        worlds ? QStringList{} : QStringList{"*.zip"};
    QVariantList result;
    for (const auto &entry : directory.entryInfoList(names, filters, captures ? QDir::Time : QDir::Name)) {
        if (result.size() >= 100) break;
        if (QDir::fromNativeSeparators(entry.canonicalFilePath()).compare(directory.filePath(entry.fileName()), Qt::CaseInsensitive) != 0)
            continue;
        if (worlds && !QFileInfo(QDir(entry.filePath()).filePath("level.dat")).isFile()) continue;
        QVariantMap item{{"name", entry.fileName()}};
        if (captures) item.insert("url", QUrl::fromLocalFile(entry.filePath()));
        result.append(item);
    }
    return result;
}

QVariantMap LauncherController::instanceEntry(const QString &id) const {
    for (const auto &entry : m_instances) {
        if (entry.toMap().value("id").toString() == id) return entry.toMap();
    }
    return {};
}

void LauncherController::requestBanner(const QString &id) {
    const auto modpack = instanceEntry(id).value("modpack").toMap();
    const auto project = modpack.value("source").toString() == "modrinth" ? modpack.value("projectId").toString() : QString();
    m_banners->request(id, instanceDirectory(id, "screenshots"), project);
}

void LauncherController::refreshBanners() {
    for (const auto &id : m_banners->ids()) requestBanner(id);
}

QVariantMap LauncherController::bannerInfo(const QString &instanceId) {
    QVariantMap info;
    if (m_banners->lookup(instanceId, info)) return info;
    if (!validInstanceId(instanceId) || instanceEntry(instanceId).isEmpty()) return InstanceBanners::generated();
    requestBanner(instanceId);
    return InstanceBanners::generated();
}

void LauncherController::setBannerImage(const QString &instanceId, const QUrl &file) {
    if (!validInstanceId(instanceId) || instanceEntry(instanceId).isEmpty()) return;
    const auto error = m_banners->setCustom(instanceId, file);
    if (!error.isEmpty()) {
        emit bannerFailed(error);
        return;
    }
    requestBanner(instanceId);
}

void LauncherController::clearBanner(const QString &instanceId) {
    if (!validInstanceId(instanceId) || instanceEntry(instanceId).isEmpty()) return;
    m_banners->clearCustom(instanceId);
    requestBanner(instanceId);
}

void LauncherController::openInstanceFolder(const QString &id, const QString &category) {
    const auto real = instanceDirectory(id, category, true);
    const auto path = namedPath(id, real);
    if (!real.isEmpty() && path == real) scheduleLinkSync();
    m_linkError = !path.isEmpty() && QDesktopServices::openUrl(QUrl::fromLocalFile(path))
        ? QString() : "La carpeta no está disponible. Minecraft la crea al usar la instancia.";
    emit linkErrorChanged();
}

void LauncherController::refreshHardware() {
    if (m_loading) return;
    m_loading = true;
    m_hardwareError.clear();
    emit hardwareChanged();
    m_probe.setFuture(QtConcurrent::run(&LauncherController::readHardware));
}

QString LauncherController::linkUrl(int id) const {
    return linkAvailable(id) ? links.at(id) : QString();
}

bool LauncherController::linkAvailable(int id) const {
    return id >= 0 && id < links.size() && !links.at(id).isEmpty();
}

void LauncherController::openLink(int id) {
    if (!linkAvailable(id)) return;
    const QUrl url(links.at(id));
    if (url.scheme() != "https" || !url.userInfo().isEmpty()) return;
    m_linkError = QDesktopServices::openUrl(url) ? QString() : "No se pudo abrir el navegador del sistema.";
    emit linkErrorChanged();
}

namespace {
constexpr auto kOfficialPackId = "diosesmc";
// El modpack oficial se descarga la primera vez y se comprueba con SHA-256.
// Al publicar un pack nuevo hay que cambiar estas tres constantes (la URL es el archivo adjunto del Release de GitHub).
constexpr auto kOfficialPackUrl = "https://github.com/JoseOrtiz-Dev/OpenClient-launcher/releases/download/modpack-1.0.0/official.mrpack";
constexpr qint64 kOfficialPackSize = 147038359;
constexpr auto kOfficialPackSha256 = "e2b982562d543fd12b713679223229c2306b1bf8e7161c6dd4c892963ef97d43";
}

// Descarga ya verificada, en la carpeta cache de la biblioteca (respeta el modo portable y «Mover carpeta de datos»).
QString LauncherController::officialPackCachePath() const {
    return m_libraryRoot.isEmpty() ? QString() : QDir(m_libraryRoot).filePath("cache/official.mrpack");
}

QString LauncherController::officialPackPath() const {
    // Un pack junto al programa o indicado por variable de entorno tiene prioridad sobre la descarga.
    const QDir application(QCoreApplication::applicationDirPath());
#ifdef Q_OS_MACOS
    const auto bundledPack = application.filePath("../Resources/modpack/official.mrpack");
#else
    const auto bundledPack = application.filePath("modpack/official.mrpack");
#endif
    const QStringList candidates = {qEnvironmentVariable("OPENCLIENT_OFFICIAL_PACK"), bundledPack};
    for (const auto &candidate : candidates) {
        const QFileInfo info(candidate);
        if (!candidate.isEmpty() && info.isFile() && info.suffix().compare("mrpack", Qt::CaseInsensitive) == 0)
            return info.absoluteFilePath();
    }
    // La descarga solo se guarda con el nombre final si el hash coincidió; aquí basta comprobar el tamaño.
    const QFileInfo cached(officialPackCachePath());
    if (cached.isFile() && cached.size() == kOfficialPackSize) return cached.absoluteFilePath();
    return {};
}

QVariantMap LauncherController::officialInstance() const {
    for (auto it = m_instances.crbegin(); it != m_instances.crend(); ++it) {
        const auto entry = it->toMap();
        if (entry.value("official").toString() == kOfficialPackId) return entry;
    }
    return {};
}

QVariantMap LauncherController::officialPack() const {
    QVariantMap pack = m_officialSummary;
    const bool available = !officialPackPath().isEmpty();
    const auto installed = officialInstance();
    const auto version = pack.value("version").toString();
    const auto installedVersion = installed.value("modpackVersion").toString();
    pack.insert("available", available);
    pack.insert("loading", m_officialPending);
    pack.insert("error", m_officialError);
    pack.insert("installedId", installed.value("id"));
    pack.insert("installedVersion", installedVersion);
    const bool downloadable = !available && !officialPackCachePath().isEmpty();
    pack.insert("downloadable", downloadable);
    pack.insert("downloading", m_packReply != nullptr);
    pack.insert("downloadSizeMb", static_cast<int>(kOfficialPackSize / (1024 * 1024)));
    pack.insert("state", installed.isEmpty() ? (available ? "ready" : downloadable ? "download" : "missing")
                         : (available && !version.isEmpty() && version != installedVersion) ? "update" : "installed");
    return pack;
}

void LauncherController::inspectOfficialPack() {
    const auto path = officialPackPath();
    if (path.isEmpty() || m_officialPending || path == m_officialPath) return;
    m_officialPath = path;
    m_officialSummary.clear();
    m_officialError.clear();
    m_officialPending = true;
    emit officialChanged();
    m_official.setFuture(QtConcurrent::run([path] {
        QString error;
        const auto result = requestEngine({{"action", "modpack_inspect"}, {"file", path}}, error);
        return QVariantMap{{"result", result}, {"error", error}};
    }));
}

void LauncherController::installOfficialPack() {
    const auto path = officialPackPath();
    if (path.isEmpty()) {
        downloadOfficialPack(); // al terminar vuelve aquí con el archivo ya verificado
        return;
    }
    const auto installed = officialInstance();
    if (!installed.isEmpty()) {
        const auto id = installed.value("id").toString();
        if (!installed.value("modpack").toMap().isEmpty()) {
            startOfficialUpdate(id);
        } else {
            // Instancia oficial sin modpack vinculado: se vincula al pack oficial y después se actualiza.
            if (!catalogBusy() && !gameBusy()) m_officialLinkingId = id;
            startFileOperation({{"action", "modpack_link"}, {"id", id}, {"file", path}},
                               "Vinculando el modpack oficial…");
        }
        return;
    }
    startFileOperation({{"action", "modpack_import"}, {"file", path}, {"official", kOfficialPackId}},
                       "Instalando el modpack oficial…");
}

void LauncherController::startOfficialUpdate(const QString &id) {
    const auto path = officialPackPath();
    if (path.isEmpty()) {
        downloadOfficialPack(); // al terminar, installOfficialPack() retoma la actualización
        return;
    }
    startFileOperation({{"action", "modpack_update"}, {"id", id}, {"file", path}, {"official", kOfficialPackId}},
                       "Actualizando el modpack oficial…");
}

// Descarga el modpack oficial por HTTPS a un .part, lo verifica (tamaño y SHA-256) y solo entonces lo renombra.
// La seguridad descansa en el hash fijado en el programa, no en el servidor ni en las redirecciones.
void LauncherController::downloadOfficialPack() {
    if (m_packReply || catalogBusy()) return;
    if (gameBusy()) {
        m_copyMessage = "Cierra Minecraft antes de continuar.";
        emit copyChanged();
        return;
    }
    const auto target = officialPackCachePath();
    const auto partPath = target + ".part";
    if (target.isEmpty() || !QDir().mkpath(QFileInfo(target).absolutePath())) {
        m_copyMessage = "No se pudo preparar la carpeta de descargas.";
        emit copyChanged();
        return;
    }
    auto part = std::make_shared<QFile>(partPath);
    if (!part->open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        m_copyMessage = "No se pudo crear el archivo de descarga.";
        emit copyChanged();
        return;
    }
    const QUrl url{QString::fromLatin1(kOfficialPackUrl)};
    if (url.scheme() != "https") {
        part->remove();
        return;
    }
    if (!m_packNetwork) m_packNetwork = new QNetworkAccessManager(this);
    QNetworkRequest request(url);
    request.setHeader(QNetworkRequest::UserAgentHeader, "OpenClient/" + QCoreApplication::applicationVersion());
    request.setTransferTimeout(30000);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
    request.setMaximumRedirectsAllowed(5);

    m_copyingInstance = true;
    m_copyMessage = "Descargando el modpack oficial…";
    emit copyChanged();
    beginOperation("official_pack_download");
    m_operationMessage = m_copyMessage;
    m_operationPercent = 0;
    m_operationCancellable = true;
    emit operationChanged();

    auto hash = std::make_shared<QCryptographicHash>(QCryptographicHash::Sha256);
    auto written = std::make_shared<qint64>(0);
    auto oversize = std::make_shared<bool>(false);
    auto *reply = m_packNetwork->get(request);
    m_packReply = reply;
    connect(reply, &QNetworkReply::readyRead, this, [reply, part, hash, written, oversize] {
        const auto chunk = reply->readAll();
        if (*oversize) return;
        if (*written + chunk.size() > kOfficialPackSize || part->write(chunk) != chunk.size()) {
            *oversize = true;
            reply->abort();
            return;
        }
        hash->addData(chunk);
        *written += chunk.size();
    });
    connect(reply, &QNetworkReply::downloadProgress, this, [this](qint64 received, qint64) {
        const int percent = static_cast<int>(std::clamp<qint64>(received * 100 / kOfficialPackSize, 0, 99));
        if (percent == m_operationPercent) return;
        m_operationPercent = percent;
        m_operationMessage = QString("Descargando el modpack oficial… %1 %").arg(percent);
        emit operationChanged();
    });
    connect(reply, &QNetworkReply::finished, this, [this, reply, part, hash, written, oversize, target, partPath] {
        reply->deleteLater();
        m_packReply = nullptr;
        part->close();
        const bool cancelled = !*oversize && reply->error() == QNetworkReply::OperationCanceledError;
        const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        QString failure;
        if (cancelled) failure = "Descarga cancelada.";
        else if (*oversize) failure = "El archivo descargado es más grande de lo esperado; se descartó.";
        else if (reply->error() != QNetworkReply::NoError || status != 200)
            failure = "No se pudo descargar el modpack oficial. Revisa tu conexión.";
        else if (*written != kOfficialPackSize) failure = "La descarga quedó incompleta.";
        else if (hash->result().toHex() != QByteArray(kOfficialPackSha256))
            failure = "El modpack descargado no coincide con el esperado; se descartó.";
        else if (QFile::exists(target) && !QFile::remove(target)) failure = "No se pudo reemplazar el modpack anterior.";
        else if (!QFile::rename(partPath, target)) failure = "No se pudo guardar el modpack descargado.";
        if (!failure.isEmpty()) QFile::remove(partPath);
        m_copyingInstance = false;
        m_copyMessage = cancelled ? QString() : failure;
        emit copyChanged();
        endOperation(cancelled);
        if (!failure.isEmpty()) return;
        m_officialPath.clear();
        emit officialChanged();
        inspectOfficialPack();
        installOfficialPack();
    });
}

// ---------------------------------------------------------------- operación global

void LauncherController::beginOperation(const QString &kind) {
    m_operationActive = true;
    m_operationKind = kind;
    m_operationMessage = "Preparando…";
    m_operationPercent = -1;
    m_operationCancellable = false;
    m_operationId = 0;
    m_operationTimer.start();
    emit operationChanged();
}

void LauncherController::endOperation(bool cancelled) {
    m_operationTimer.stop();
    m_operationActive = false;
    m_operationCancellable = false;
    m_operationPercent = -1;
    m_operationId = 0;
    m_operationKind.clear();
    m_operationMessage = cancelled ? QString("Operación cancelada.") : QString();
    emit operationChanged();
}

void LauncherController::pollOperation() {
    if (!m_operationActive) return;
    QString error;
    const auto state = requestEngine({{"action", "operation_status"}}, error).toMap();
    if (!error.isEmpty() || !state.value("active").toBool()) return;
    const auto percent = state.value("percent");
    const int value = percent.isValid() && !percent.isNull() ? std::clamp(percent.toInt(), 0, 100) : -1;
    const auto message = state.value("message").toString();
    const bool cancellable = state.value("cancellable").toBool();
    const auto kind = state.value("kind").toString();
    const auto id = state.value("id").toULongLong();
    if (id == m_operationId && value == m_operationPercent && cancellable == m_operationCancellable
        && message == m_operationMessage && kind == m_operationKind) return;
    m_operationId = id;
    m_operationPercent = value;
    m_operationCancellable = cancellable;
    m_operationMessage = message;
    if (!kind.isEmpty()) m_operationKind = kind;
    emit operationChanged();
}

void LauncherController::cancelOperation() {
    if (!m_operationActive || !m_operationCancellable) return;
    if (m_packReply) { // descarga del modpack oficial: no pasa por el motor
        m_packReply->abort();
        return;
    }
    QVariantMap request{{"action", "operation_cancel"}, {"target", "content"}};
    if (m_operationId) request.insert("id", QVariant::fromValue(m_operationId));
    QString error;
    const auto result = requestEngine(request, error).toMap();
    if (error.isEmpty() && result.value("cancelled").toBool()) {
        m_operationCancellable = false;
        m_operationMessage = "Cancelando…";
        emit operationChanged();
    }
}

void LauncherController::cancelLaunch() {
    if (m_preparingId.isEmpty()) return;
    QString error;
    const auto result = requestEngine({{"action", "operation_cancel"}, {"target", "game"}}, error).toMap();
    if (error.isEmpty() && result.value("cancelled").toBool()) {
        m_launchMessage = "Cancelando…";
        emit gameChanged();
    }
}

// ---------------------------------------------------------------- modpack

namespace {
bool validInstanceId(const QString &id) {
    return QUuid::fromString(id).toString(QUuid::WithoutBraces) == id;
}

QString mrpackPath(const QUrl &url) {
    auto path = url.toLocalFile();
    if (!path.isEmpty() && !path.endsWith(".mrpack", Qt::CaseInsensitive)) path += ".mrpack";
    return path;
}
}

// Carpeta de accesos: fuera de la raíz de datos para que el motor, «Mover carpeta de datos» y el cálculo de disco no la recorran.
// Vacía en modo portable (sin accesos); en la prueba solo se admite OPENCLIENT_LINKS_DIR.
QString LauncherController::instanceLinksDirectory() const {
    if (QCoreApplication::arguments().contains("--smoke-test")) return qEnvironmentVariable("OPENCLIENT_LINKS_DIR");
    if (QFileInfo(QDir(QCoreApplication::applicationDirPath()).filePath("portable.txt")).isFile()) return {};
    const auto base = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation);
    return base.isEmpty() ? QString() : base + "/OpenClient/Instancias";
}

void LauncherController::scheduleLinkSync() {
    if (m_linksPending) return;
    m_linksPending = true;
    QTimer::singleShot(0, this, [this] { m_linksPending = false; syncInstanceLinks(); });
}

// Idempotente y no reentrante. Quita solo el acceso y conserva la carpeta de destino.
void LauncherController::syncInstanceLinks() {
    if (m_linksSyncing) return;
    m_linksSyncing = true;
    const auto done = qScopeGuard([this] { m_linksSyncing = false; });
    m_instanceLinks.clear();
    const auto dir = instanceLinksDirectory();
    const auto root = QDir::fromNativeSeparators(QFileInfo(m_libraryRoot).canonicalFilePath());
    if (dir.isEmpty() || root.isEmpty() || !QDir().mkpath(dir)) return;
    const auto same = [](const QString &a, const QString &b) {
        return !a.isEmpty() && QDir::cleanPath(QDir::fromNativeSeparators(a)).compare(QDir::cleanPath(QDir::fromNativeSeparators(b)), Qt::CaseInsensitive) == 0;
    };
    // Mapa deseado nombre -> id, en orden por id para que los sufijos « (2)» no se intercambien entre ejecuciones.
    QMap<QString, QString> names;
    for (const auto &entry : std::as_const(m_instances)) {
        const auto id = entry.toMap().value("id").toString();
        const auto real = root + "/instances/" + id;
        if (validInstanceId(id) && QFileInfo(real).isDir() && same(QFileInfo(real).canonicalFilePath(), real)) names.insert(id, entry.toMap().value("name").toString());
    }
    QHash<QString, QString> desired;
    QSet<QString> used;
    for (auto it = names.cbegin(); it != names.cend(); ++it) {
        const auto base = InstanceLinks::sanitize(it.value());
        auto name = base;
        for (int n = 2; used.contains(name.toLower()); ++n) {
            const auto suffix = QString(" (%1)").arg(n);
            name = base.left(60 - suffix.size()).trimmed() + suffix;
        }
        used.insert(name.toLower());
        desired.insert(name, it.key());
    }
    const QDir links(dir);
    for (const auto &entry : links.entryInfoList(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System)) {
        if (!entry.isJunction() && !entry.isSymLink()) continue;   // carpetas y archivos del usuario: intactos
        const auto id = desired.value(entry.fileName());
        if (!id.isEmpty() && same(InstanceLinks::target(entry), root + "/instances/" + id)) continue;
#ifdef Q_OS_WIN
        const bool removed = QDir().rmdir(entry.filePath());
#else
        const bool removed = QFile::remove(entry.filePath());
#endif
        if (!removed) std::fprintf(stderr, "No se pudo quitar el acceso obsoleto %s\n", qPrintable(entry.filePath()));
    }
    for (auto it = desired.cbegin(); it != desired.cend(); ++it) {
        const auto path = links.filePath(it.key());
        const auto real = root + "/instances/" + it.value();
        const QFileInfo info(path);
        if ((info.isJunction() || info.isSymLink()) && same(InstanceLinks::target(info), real)) m_instanceLinks.insert(it.value(), path);
        else if (info.exists()) std::fprintf(stderr, "El acceso %s está ocupado por otra carpeta o archivo\n", qPrintable(path));
        else if (InstanceLinks::createJunction(path, QDir::toNativeSeparators(real))) m_instanceLinks.insert(it.value(), path);
        else std::fprintf(stderr, "No se pudo crear el acceso %s\n", qPrintable(path));
    }
}

// Ruta con el nombre de la instancia equivalente a una ruta real de `instances/<id>`; si no hay acceso, la misma ruta real.
QString LauncherController::namedPath(const QString &id, const QString &realPath) const {
    const auto link = m_instanceLinks.value(id);
    const auto base = QDir::fromNativeSeparators(QFileInfo(m_libraryRoot).canonicalFilePath()) + "/instances/" + id;
    if (link.isEmpty() || !realPath.startsWith(base)) return realPath;
    const auto candidate = link + realPath.mid(base.size());
    return QFileInfo(candidate).isDir() ? candidate : realPath;
}

bool LauncherController::applyInstances(const QVariant &result) {
    QVariantList list;
    if (result.metaType().id() == QMetaType::QVariantList) {
        list = result.toList();
    } else if (result.metaType().id() == QMetaType::QVariantMap && result.toMap().value("instances").isValid()) {
        list = result.toMap().value("instances").toList();
    } else {
        return false;
    }
    m_instances = withDiskMods(list);
    m_libraryError.clear();
    emit libraryChanged();
    return true;
}

// Mods que hay de verdad en `instances/<id>/mods` (activos y `.disabled`). Un modpack los instala ahí sin
// añadirlos a installedContent, así que sin esto su tarjeta diría «Sin mods».
QVariantList LauncherController::withDiskMods(QVariantList list) const {
    for (auto &item : list) {
        auto entry = item.toMap();
        const auto id = entry.value("id").toString();
        if (QUuid::fromString(id).isNull()) continue;
        const QDir mods(QDir(m_libraryRoot).filePath("instances/" + id + "/mods"));
        const auto files = mods.entryList({"*.jar", "*.jar.disabled"}, QDir::Files);
        entry.insert("diskMods", files.size());
        entry.insert("diskModsActive", std::count_if(files.cbegin(), files.cend(),
            [](const QString &name) { return name.endsWith(".jar", Qt::CaseInsensitive); }));
        item = entry;
    }
    return list;
}

QString LauncherController::modpackWriteMessage(const QString &action, const QVariantMap &r) {
    QString text;
    const int kept = r.value("keptCount").toInt();
    const QString keptText = kept > 0 ? QString(" Se conservaron %1 archivo(s) tuyos.").arg(kept) : QString();
    if (action == "modpack_update") {
        text = QString("Modpack actualizado: %1 archivo(s) añadidos, %2 reemplazados y %3 eliminados.")
                   .arg(r.value("added").toInt()).arg(r.value("replaced").toInt()).arg(r.value("removed").toInt()) + keptText;
        if (!r.value("worldBackup").toString().isEmpty()) text += " Se guardó una copia de tus mundos.";
        else if (r.value("worldBackupSkipped").toLongLong() > 0) text += " No se copiaron tus mundos porque ocupan más de 2 GB.";
        if (r.value("loaderChanged").toBool()) text += " Cambió el cargador o la versión del juego.";
    } else if (action == "modpack_restore") {
        text = "Versión anterior del modpack restaurada." + keptText;
    } else if (action == "modpack_link") {
        text = QString("Modpack vinculado: %1 archivo(s) reconocidos; %2 no coinciden y se consideran tuyos.")
                   .arg(r.value("adopted").toInt()).arg(r.value("missing").toInt());
    } else if (action == "modpack_export") {
        text = QString("Modpack exportado en %1 (%2 archivo(s), %3 MB).")
                   .arg(QDir::toNativeSeparators(r.value("file").toString()))
                   .arg(r.value("files").toInt()).arg(r.value("bytes").toLongLong() / 1048576);
        if (const int embedded = r.value("embeddedCount").toInt(); embedded > 0)
            text += QString(" %1 mod(s) sin proyecto en Modrinth se incluyeron dentro del paquete.").arg(embedded);
    } else if (action == "import_curseforge") {
        text = "Modpack de CurseForge importado como una instancia nueva.";
        if (const int unresolved = r.value("unresolved").toInt(); unresolved > 0)
            text += QString(" %1 mod(s) no se pudieron resolver y no se incluyeron.").arg(unresolved);
    } else if (action == "import_mmc") {
        text = "Instancia de MultiMC/Prism importada en la biblioteca.";
    }
    if (const int warnings = r.value("warnings").toList().size(); warnings > 0)
        text += QString(" (%1 aviso(s))").arg(warnings);
    return text;
}

void LauncherController::startModpackWrite(QVariantMap request) {
    if (!validInstanceId(request.value("id").toString())) return;
    if (gameBusy()) {
        m_catalogMessage = "Cierra Minecraft antes de continuar.";
        emit catalogChanged();
        return;
    }
    startCatalogRequest(std::move(request), true);
}

void LauncherController::runQuery(QVariantMap request, std::function<void(const QVariantMap &)> done) {
    request.insert("root", m_libraryRoot);
    auto *watcher = new QFutureWatcher<QVariantMap>(this);
    m_queries.append(watcher);
    connect(watcher, &QFutureWatcher<QVariantMap>::finished, this, [this, watcher, done = std::move(done)] {
        m_queries.removeOne(watcher);
        done(watcher->result());
        watcher->deleteLater();
    });
    watcher->setFuture(QtConcurrent::run([request] {
        QString error;
        const auto result = requestEngine(request, error);
        return QVariantMap{{"result", result}, {"error", error}};
    }));
}

void LauncherController::analyzeCrash(const QString &id) {
    const int token = ++m_crashToken;
    m_crashFindings.clear();
    m_crashAnalyzing = validInstanceId(id);
    emit crashFindingsChanged();
    if (!m_crashAnalyzing) return;
    runQuery({{"action", "crash_analyze"}, {"id", id}, {"ram", m_ram}}, [this, token](const QVariantMap &response) {
        if (token != m_crashToken) return;
        const bool ok = response.value("error").toString().isEmpty();
        m_crashFindings = ok ? response.value("result").toMap().value("findings").toList() : QVariantList();
        m_crashAnalyzing = false;
        emit crashFindingsChanged();
    });
}

void LauncherController::checkModpack(const QString &id, bool force) {
    if (!validInstanceId(id) || m_modpackChecking.contains(id)) return;
    if (!force && m_modpackChecked.contains(id)) return;
    m_modpackChecked.insert(id);
    requestModpackCheck(id, force);
}

void LauncherController::requestModpackCheck(const QString &id, bool force) {
    if (!validInstanceId(id) || m_modpackChecking.contains(id)) return;
    m_modpackChecked.insert(id);
    m_modpackChecking.insert(id);
    auto pending = m_modpackInfo.value(id).toMap();
    pending.insert("checking", true);
    m_modpackInfo.insert(id, pending);
    emit modpackInfoChanged(id);
    runQuery({{"action", "modpack_check"}, {"id", id}, {"force", force}}, [this, id](const QVariantMap &response) {
        m_modpackChecking.remove(id);
        const auto error = response.value("error").toString();
        // Si falla (p. ej. sin red) se conserva lo último conocido y el error queda en `error`, sin reintentar.
        auto info = error.isEmpty() ? response.value("result").toMap() : m_modpackInfo.value(id).toMap();
        info.insert("checking", false);
        info.insert("error", error);
        m_modpackInfo.insert(id, info);
        emit modpackInfoChanged(id);
    });
}

void LauncherController::updateModpack(const QString &id, const QString &version) {
    if (version.isEmpty()) return;
    startModpackWrite({{"action", "modpack_update"}, {"id", id}, {"version", version}});
}

void LauncherController::updateModpackFromFile(const QString &id, const QUrl &file) {
    if (!file.isLocalFile()) return;
    QVariantMap request{{"action", "modpack_update"}, {"id", id}, {"file", file.toLocalFile()}};
    // Una instancia del modpack oficial solo se actualiza desde un archivo declarado como oficial.
    for (const auto &entry : std::as_const(m_instances)) {
        const auto instance = entry.toMap();
        if (instance.value("id").toString() == id && instance.value("official").toString() == kOfficialPackId) {
            request.insert("official", kOfficialPackId);
            break;
        }
    }
    startModpackWrite(request);
}

void LauncherController::listSnapshots(const QString &id) {
    if (!validInstanceId(id)) return;
    m_snapshots.clear();
    m_snapshotsId = id;
    emit snapshotsChanged();
    runQuery({{"action", "modpack_snapshots"}, {"id", id}}, [this, id](const QVariantMap &response) {
        if (m_snapshotsId != id) return;
        const auto error = response.value("error").toString();
        m_snapshots = error.isEmpty() ? response.value("result").toList() : QVariantList();
        emit snapshotsChanged();
        if (!error.isEmpty()) {
            m_modpackError = error;
            emit modpackResultChanged();
        }
    });
}

void LauncherController::restoreSnapshot(const QString &id, const QString &snapshot) {
    if (snapshot.isEmpty()) return;
    startModpackWrite({{"action", "modpack_restore"}, {"id", id}, {"snapshot", snapshot}});
}

void LauncherController::loadWorlds(const QString &id) {
    if (!validInstanceId(id)) return;
    if (m_worldsId != id) {
        m_worlds.clear();
        m_worldBackups.clear();
        m_worldMessage.clear();
        m_worldFailed = false;
    }
    m_worldsId = id;
    m_worldsLoading = true;
    emit worldsChanged();
    runQuery({{"action", "world_list"}, {"id", id}}, [this, id](const QVariantMap &response) {
        if (m_worldsId != id) return;
        const auto error = response.value("error").toString();
        const auto result = response.value("result").toMap();
        m_worlds = error.isEmpty() ? result.value("worlds").toList() : QVariantList();
        m_worldBackups = error.isEmpty() ? result.value("backups").toList() : QVariantList();
        m_worldsLoading = false;
        if (!error.isEmpty()) {
            m_worldMessage = error;
            m_worldFailed = true;
        }
        emit worldsChanged();
    });
}

void LauncherController::backupWorlds(const QString &id, const QStringList &names) {
    if (!validInstanceId(id)) return;
    m_worldMessage.clear();
    m_worldFailed = false;
    QVariantMap request{{"action", "world_backup"}, {"id", id}};
    if (!names.isEmpty()) request.insert("worlds", names);
    startModpackWrite(request);
}

void LauncherController::restoreWorldBackup(const QString &id, const QString &backup) {
    if (backup.isEmpty()) return;
    m_worldMessage.clear();
    m_worldFailed = false;
    startModpackWrite({{"action", "world_restore"}, {"id", id}, {"backup", backup}});
}

void LauncherController::deleteWorldBackup(const QString &id, const QString &backup) {
    if (!validInstanceId(id) || backup.isEmpty()) return;
    runQuery({{"action", "world_backup_delete"}, {"id", id}, {"backup", backup}}, [this, id](const QVariantMap &response) {
        if (m_worldsId != id) return;
        const auto error = response.value("error").toString();
        if (error.isEmpty()) {
            m_worldBackups = response.value("result").toMap().value("backups").toList();
            m_worldMessage = "Copia eliminada.";
            m_worldFailed = false;
        } else {
            m_worldMessage = error;
            m_worldFailed = true;
        }
        emit worldsChanged();
    });
}

void LauncherController::loadLinkCandidates(const QString &id) {
    if (!validInstanceId(id)) return;
    m_linkCandidates.clear();
    m_linkCandidatesId = id;
    emit linkCandidatesChanged();
    runQuery({{"action", "modpack_link_candidates"}, {"id", id}}, [this, id](const QVariantMap &response) {
        if (m_linkCandidatesId != id) return;
        const auto error = response.value("error").toString();
        m_linkCandidates = error.isEmpty() ? response.value("result").toList() : QVariantList();
        emit linkCandidatesChanged();
        if (!error.isEmpty()) {
            m_modpackError = error;
            emit modpackResultChanged();
        }
    });
}

void LauncherController::linkModpack(const QString &id, const QString &project, const QString &version) {
    if (project.isEmpty() || version.isEmpty()) return;
    startModpackWrite({{"action", "modpack_link"}, {"id", id}, {"project", project}, {"version", version}});
}

void LauncherController::linkModpackFromFile(const QString &id, const QUrl &file) {
    if (!file.isLocalFile()) return;
    startModpackWrite({{"action", "modpack_link"}, {"id", id}, {"file", file.toLocalFile()}});
}

void LauncherController::exportMrpack(const QString &id, const QUrl &file, bool includeSaves) {
    if (!file.isLocalFile()) return;
    startModpackWrite({{"action", "modpack_export"}, {"id", id}, {"file", mrpackPath(file)},
                       {"includeSaves", includeSaves}});
}

void LauncherController::importCurseForge(const QUrl &file, bool confirm) {
    if (!file.isLocalFile()) return;
    if (confirm) {
        startFileOperation({{"action", "import_curseforge"}, {"file", file.toLocalFile()}, {"confirm", true}},
                           "Importando el modpack de CurseForge…");
        return;
    }
    // Vista previa: solo lee el .zip, sin bloqueo de biblioteca ni operación registrada.
    m_curseForgePreview.clear();
    emit curseForgePreviewChanged();
    runQuery({{"action", "import_curseforge"}, {"file", file.toLocalFile()}, {"confirm", false}},
             [this](const QVariantMap &response) {
        const auto error = response.value("error").toString();
        m_curseForgePreview = error.isEmpty() ? response.value("result").toMap() : QVariantMap{{"error", error}};
        emit curseForgePreviewChanged();
    });
}

void LauncherController::importMultiMC(const QUrl &fileOrFolder) {
    if (!fileOrFolder.isLocalFile()) return;
    startFileOperation({{"action", "import_mmc"}, {"path", fileOrFolder.toLocalFile()}},
                       "Importando la instancia de MultiMC/Prism…");
}

void LauncherController::contentConflicts(const QString &id) {
    if (!validInstanceId(id)) return;
    m_conflictsId = id;
    m_conflicts.clear();
    m_conflictCount = 0;
    m_conflictsCacheMissing = false;
    emit conflictsChanged();
    runQuery({{"action", "content_conflicts"}, {"id", id}}, [this, id](const QVariantMap &response) {
        if (m_conflictsId != id) return;
        const auto result = response.value("result").toMap();
        const bool ok = response.value("error").toString().isEmpty();
        m_conflicts = ok ? result.value("conflicts").toList() : QVariantList();
        m_conflictCount = ok ? result.value("count").toInt() : 0;
        m_conflictsCacheMissing = ok && result.value("cacheMissing").toBool();
        emit conflictsChanged();
    });
}

// ---------------------------------------------------------------- contenido de la instancia

namespace {
struct ContentCategory {
    const char *folder;
    const char *kind;
};
constexpr ContentCategory contentCategories[] = {{"mods", "mod"}, {"resourcepacks", "resourcepack"}, {"shaderpacks", "shader"}};
// Máximo de llamadas seguidas a content_resolve sin que baje `pending` antes de rendirse.
constexpr int contentStallLimit = 3;
}

void LauncherController::loadContent(const QString &id, bool force) {
    if (!validInstanceId(id)) return;
    const int token = ++m_contentToken;
    if (id != m_contentId) {
        m_contentModel->clear();
        m_contentOffline = false;
        m_contentTruncated = false;
    }
    m_contentId = id;
    m_contentActive = true;
    m_contentError.clear();
    m_contentLoading = true;
    m_contentResolving = false;
    m_contentStall = 0;
    m_contentListsLeft = static_cast<int>(std::size(contentCategories));
    m_loadOffline = false;
    m_loadTruncated = false;
    for (int i = 0; i < 3; ++i) {
        m_contentPendingBy[i] = 0;
        m_contentForceBy[i] = force;
    }
    emit contentChanged();
    for (int i = 0; i < 3; ++i) {
        runQuery({{"action", "content_list"}, {"id", id}, {"category", contentCategories[i].folder}},
                 [this, token, i](const QVariantMap &response) {
            if (token != m_contentToken) return;
            const auto error = response.value("error").toString();
            const auto result = response.value("result").toMap();
            m_contentModel->applyItems(contentCategories[i].kind, error.isEmpty() ? result.value("items").toList() : QVariantList(), true);
            if (!error.isEmpty()) {
                if (m_contentError.isEmpty()) m_contentError = error;
            } else {
                m_contentPendingBy[i] = result.value("pending").toInt();
                m_loadOffline = m_loadOffline || result.value("offline").toBool();
                m_loadTruncated = m_loadTruncated || result.value("truncated").toBool();
            }
            if (--m_contentListsLeft > 0) return;
            m_contentLoading = false;
            m_contentOffline = m_loadOffline;
            m_contentTruncated = m_loadTruncated;
            emit contentChanged();
            resolveContentStep(token);
        });
    }
}

void LauncherController::releaseContent() {
    ++m_contentToken;
    m_contentActive = false;
    m_contentLoading = false;
    m_contentResolving = false;
    m_contentOffline = false;
    m_contentTruncated = false;
    m_contentError.clear();
    m_contentId.clear();
    for (int i = 0; i < 3; ++i) {
        m_contentPendingBy[i] = 0;
        m_contentForceBy[i] = false;
    }
    m_contentModel->clear();
    emit contentChanged();
}

// Una llamada a content_resolve por categoría con pendientes; repite con una pausa breve hasta que no quede
// nada, la red falle (offline) o `pending` no baje en contentStallLimit llamadas seguidas.
void LauncherController::resolveContentStep(int token) {
    if (token != m_contentToken || !m_contentActive) return;
    int index = -1;
    for (int i = 0; i < 3 && index < 0; ++i)
        if (m_contentPendingBy[i] > 0 || m_contentForceBy[i]) index = i;
    if (index < 0) {
        if (m_contentResolving) {
            m_contentResolving = false;
            emit contentChanged();
        }
        return;
    }
    const auto stop = [this] {
        m_contentResolving = false;
        emit contentChanged();
    };
    if (!m_contentResolving) {
        m_contentResolving = true;
        emit contentChanged();
    }
    m_contentForceBy[index] = false;
    const int before = m_contentPendingBy[index];
    runQuery({{"action", "content_resolve"}, {"id", m_contentId}, {"category", contentCategories[index].folder},
              {"budgetMs", 1200}},
             [this, token, index, before, stop](const QVariantMap &response) {
        if (token != m_contentToken) return;
        const auto error = response.value("error").toString();
        const auto result = response.value("result").toMap();
        if (!error.isEmpty()) {
            if (m_contentError.isEmpty()) m_contentError = error;
            m_contentPendingBy[index] = 0;
        } else {
            m_contentModel->applyItems(contentCategories[index].kind, result.value("items").toList(), false);
            const int pending = result.value("pending").toInt();
            m_contentStall = before > 0 && pending >= before ? m_contentStall + 1 : 0;
            m_contentPendingBy[index] = pending;
            if (result.value("offline").toBool()) {
                m_contentOffline = true;
                stop();
                return;
            }
            if (m_contentStall >= contentStallLimit) {
                stop();
                return;
            }
        }
        emit contentChanged();
        QTimer::singleShot(120, this, [this, token] { resolveContentStep(token); });
    });
}
