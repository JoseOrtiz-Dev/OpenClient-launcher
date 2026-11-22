#include "MaintenanceController.h"
#include "LauncherController.h"
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLockFile>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QProcess>
#include <QRegularExpression>
#include <QSaveFile>
#include <QStandardPaths>
#include <QSysInfo>
#include <QTimer>
#include <QTemporaryFile>
#include <QtConcurrent>
#include <algorithm>

namespace {
constexpr qint64 manifestLimit = 64 * 1024;
constexpr qint64 logTailLimit = 64 * 1024;

QString normalized(const QString &path) {
    auto value = QDir::cleanPath(QFileInfo(path).absoluteFilePath()).toLower();
    if (!value.endsWith('/')) value += '/';
    return value;
}

bool isLink(const QFileInfo &info) { return info.isSymLink() || info.isJunction(); }

void measure(const QDir &directory, qint64 &files, qint64 &bytes) {
    for (const auto &entry : directory.entryInfoList(QDir::Files | QDir::Dirs | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System)) {
        if (isLink(entry)) continue;
        if (entry.isDir()) {
            measure(QDir(entry.absoluteFilePath()), files, bytes);
        } else if (!entry.fileName().endsWith(".lock")) {
            ++files;
            bytes += entry.size();
        }
    }
}

QString copyDirectory(const QDir &from, const QDir &to) {
    for (const auto &entry : from.entryInfoList(QDir::Files | QDir::Dirs | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System)) {
        if (isLink(entry)) continue;
        const auto name = entry.fileName();
        if (entry.isDir()) {
            if (!to.mkpath(name)) return QString("No se pudo crear «%1».").arg(name);
            const auto error = copyDirectory(QDir(entry.absoluteFilePath()), QDir(to.filePath(name)));
            if (!error.isEmpty()) return error;
        } else if (!name.endsWith(".lock") && !QFile::copy(entry.absoluteFilePath(), to.filePath(name))) {
            return QString("No se pudo copiar «%1».").arg(name);
        }
    }
    return {};
}

void emptyDirectory(const QString &path) {
    const QDir directory(path);
    for (const auto &entry : directory.entryInfoList(QDir::Files | QDir::Dirs | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System)) {
        if (isLink(entry) || !entry.isDir()) QFile::remove(entry.absoluteFilePath());
        else QDir(entry.absoluteFilePath()).removeRecursively();
    }
}

QString tail(const QString &path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) return {};
    file.seek(std::max<qint64>(0, file.size() - logTailLimit));
    auto lines = QString::fromUtf8(file.readAll()).split('\n');
    if (lines.size() > 200) lines = lines.mid(lines.size() - 200);
    return lines.join('\n');
}

QList<int> numbers(const QString &core) {
    QList<int> result;
    for (const auto &part : core.split('.')) result.append(part.toInt());
    return result;
}
}

MaintenanceController::MaintenanceController(LauncherController *launcher, const QString &dataDirectory,
                                             const QString &bootstrapFile, const QString &manifestUrl, QObject *parent)
    : QObject(parent), m_launcher(launcher), m_settings(QDir(dataDirectory).filePath("maintenance.ini"), QSettings::IniFormat),
      m_bootstrapFile(bootstrapFile), m_manifestUrl(manifestUrl), m_dataDirectory(dataDirectory) {
    const auto saved = m_settings.value("updates/channel", "stable").toString();
    if (saved == "stable" || saved == "beta") m_channel = saved;
    const auto chosen = QSettings(m_bootstrapFile, QSettings::IniFormat).value("location/data_directory").toString();
    if (!chosen.isEmpty() && normalized(chosen) != normalized(m_dataDirectory)) {
        m_locationMessage = QString("No se encontró la carpeta elegida (%1). Se usa la predeterminada.").arg(QDir::toNativeSeparators(chosen));
        m_locationFailed = true;
    }
    connect(&m_move, &QFutureWatcher<QString>::finished, this, [this] {
        m_moving = false;
        const auto error = m_move.result();
        if (error.isEmpty()) {
            m_pending = m_moveTarget;
            m_locationFailed = false;
            m_locationMessage = QString("Copia terminada. Reinicia OpenClient para usar la carpeta nueva. La anterior no se borró: %1")
                                    .arg(QDir::toNativeSeparators(m_dataDirectory));
        } else {
            m_locationFailed = true;
            m_locationMessage = error;
        }
        emit locationChanged();
    });
}

QString MaintenanceController::portableDirectory(const QString &appDirectory) {
    if (!QFileInfo(QDir(appDirectory).filePath("portable.txt")).isFile()) return {};
    const auto data = QDir(appDirectory).filePath("data");
    if (QFileInfo(data).exists() ? false : !QDir().mkpath(data)) return {};
    const QFileInfo info(data);
    if (!info.isDir() || !info.isWritable() || isLink(info)) return {};
    return QDir::cleanPath(info.canonicalFilePath());
}

bool MaintenanceController::portable() const {
    const auto portableData = portableDirectory(QCoreApplication::applicationDirPath());
    return !portableData.isEmpty() && normalized(m_dataDirectory) == normalized(portableData);
}

QString MaintenanceController::defaultDataDirectory() {
    if (const auto portable = portableDirectory(QCoreApplication::applicationDirPath()); !portable.isEmpty()) return portable;
    return QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
}

QString MaintenanceController::bootstrapFile() {
    return QDir(defaultDataDirectory()).filePath("location.ini");
}

QString MaintenanceController::resolveDataDirectory() {
    const auto fallback = defaultDataDirectory();
    // En modo portable manda la carpeta `data` junto al programa; el puntero de ubicación se ignora.
    if (!portableDirectory(QCoreApplication::applicationDirPath()).isEmpty()) return fallback;
    const auto chosen = QSettings(bootstrapFile(), QSettings::IniFormat).value("location/data_directory").toString();
    if (chosen.isEmpty()) return fallback;
    const QFileInfo info(chosen);
    if (!info.isAbsolute() || !info.isDir() || !info.isWritable() || isLink(info)) return fallback;
    return QDir::cleanPath(info.canonicalFilePath());
}

bool MaintenanceController::writeLocation(const QString &file, const QString &directory) {
    QDir().mkpath(QFileInfo(file).absolutePath());
    QSettings pointer(file, QSettings::IniFormat);
    if (directory.isEmpty()) pointer.remove("location/data_directory");
    else pointer.setValue("location/data_directory", directory);
    pointer.sync();
    return pointer.status() == QSettings::NoError;
}

QString MaintenanceController::validateTarget(const QString &current, const QString &target) {
    const QFileInfo info(target);
    if (target.trimmed().isEmpty() || !info.isAbsolute()) return "Elige una carpeta con una ruta completa.";
    const auto from = normalized(current), to = normalized(target);
    if (from == to) return "Esa ya es la carpeta de datos actual.";
    if (to.startsWith(from) || from.startsWith(to)) return "La carpeta nueva no puede estar dentro de la actual ni contenerla.";
    if (info.exists()) {
        if (!info.isDir() || isLink(info)) return "Elige una carpeta normal, no un enlace.";
        if (!QDir(target).isEmpty(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System))
            return "La carpeta debe estar vacía. Crea una nueva o vacía la que elegiste.";
    } else if (!QFileInfo(info.absolutePath()).isDir()) {
        return "La carpeta superior no existe.";
    }
    return {};
}

QString MaintenanceController::copyTree(const QString &source, const QString &target) {
    if (!QDir().mkpath(target)) return "No se pudo crear la carpeta de destino.";
    {
        QTemporaryFile probe(QDir(target).filePath("openclient-escritura-XXXXXX"));
        if (!probe.open()) return "No se puede escribir en la carpeta de destino.";
    }
    const auto error = copyDirectory(QDir(source), QDir(target));
    if (!error.isEmpty()) return error;
    qint64 originalFiles = 0, originalBytes = 0, copiedFiles = 0, copiedBytes = 0;
    measure(QDir(source), originalFiles, originalBytes);
    measure(QDir(target), copiedFiles, copiedBytes);
    if (originalFiles != copiedFiles || originalBytes != copiedBytes) return "La copia no coincide con el original; no se cambió nada.";
    return {};
}

QString MaintenanceController::redact(const QString &text, const QString &home) {
    auto result = text;
    if (!home.isEmpty()) {
        for (const auto &variant : {QDir::toNativeSeparators(home), QDir::fromNativeSeparators(home)})
            result.replace(variant, "%USERPROFILE%", Qt::CaseInsensitive);
    }
    static const QRegularExpression jwt("eyJ[A-Za-z0-9_-]{10,}\\.[A-Za-z0-9_-]{10,}\\.[A-Za-z0-9_-]{10,}");
    static const QRegularExpression named("(access_?token|refresh_?token|--session|password|secret)([\"'\\s=:]+)[^\\s\"']+",
                                          QRegularExpression::CaseInsensitiveOption);
    result.replace(jwt, "[oculto]");
    result.replace(named, "\\1\\2[oculto]");
    return result;
}

int MaintenanceController::compareVersions(const QString &left, const QString &right) {
    const auto split = [](const QString &version) {
        const auto dash = version.indexOf('-');
        return std::pair{dash < 0 ? version : version.left(dash), dash < 0 ? QString() : version.mid(dash + 1)};
    };
    const auto [leftCore, leftPre] = split(left);
    const auto [rightCore, rightPre] = split(right);
    auto a = numbers(leftCore), b = numbers(rightCore);
    while (a.size() < b.size()) a.append(0);
    while (b.size() < a.size()) b.append(0);
    if (a != b) return a < b ? -1 : 1;
    if (leftPre == rightPre) return 0;
    if (leftPre.isEmpty()) return 1;
    if (rightPre.isEmpty()) return -1;
    const auto x = leftPre.split('.'), y = rightPre.split('.');
    for (int i = 0; i < std::min(x.size(), y.size()); ++i) {
        bool xn = false, yn = false;
        const int xv = x[i].toInt(&xn), yv = y[i].toInt(&yn);
        if (xn && yn) {
            if (xv != yv) return xv < yv ? -1 : 1;
        } else if (xn != yn) {
            return xn ? -1 : 1;
        } else if (x[i] != y[i]) {
            return x[i] < y[i] ? -1 : 1;
        }
    }
    return x.size() == y.size() ? 0 : (x.size() < y.size() ? -1 : 1);
}

QVariantMap MaintenanceController::parseManifest(const QByteArray &raw, const QString &channel, const QString &current) {
    static const QRegularExpression versionPattern("^[0-9]+(\\.[0-9]+){0,3}(-[0-9A-Za-z.]{1,40})?$");
    static const QRegularExpression sha256Pattern("^[0-9a-f]{64}$");
    QJsonParseError parse;
    const auto document = QJsonDocument::fromJson(raw, &parse);
    if (parse.error != QJsonParseError::NoError || !document.isObject())
        return {{"ok", false}, {"error", "La respuesta de actualizaciones no es válida."}};
    const auto channels = document.object().value("channels").toObject();
    QVariantMap best;
    for (const auto &name : channel == "beta" ? QStringList{"stable", "beta"} : QStringList{"stable"}) {
        const auto entry = channels.value(name).toObject();
        const auto version = entry.value("version").toString();
        const QUrl url(entry.value("url").toString());
        if (!versionPattern.match(version).hasMatch() || url.scheme() != "https" || url.host().isEmpty()) continue;
        // sha256 y size son opcionales: sin ellos solo se ofrece abrir la descarga en el navegador.
        const auto sha256 = entry.value("sha256").toString().toLower();
        const qint64 size = static_cast<qint64>(entry.value("size").toDouble());
        const bool verifiable = sha256Pattern.match(sha256).hasMatch() && size > 0 && size <= (qint64(1) << 30);
        if (best.isEmpty() || compareVersions(version, best.value("version").toString()) > 0)
            best = {{"version", version}, {"url", url.toString()}, {"notes", entry.value("notes").toString().left(500)},
                    {"sha256", verifiable ? sha256 : QString()}, {"size", verifiable ? size : qint64(0)}};
    }
    if (best.isEmpty()) return {{"ok", false}, {"error", "Este canal todavía no tiene versiones publicadas."}};
    best.insert("ok", true);
    best.insert("available", compareVersions(best.value("version").toString(), current) > 0);
    return best;
}

QString MaintenanceController::currentVersion() const { return QCoreApplication::applicationVersion(); }

void MaintenanceController::fail(QString &message, bool &flag, const QString &text) {
    message = text;
    flag = true;
}

void MaintenanceController::moveDataDirectory(const QUrl &folder) {
    if (!folder.isLocalFile()) return;
    startMove(QDir::cleanPath(folder.toLocalFile()));
}

void MaintenanceController::useDefaultDirectory() {
    startMove(QDir::cleanPath(defaultDataDirectory()));
}

void MaintenanceController::startMove(const QString &target) {
    if (m_moving || !m_launcher) return;
    if (portable()) {
        fail(m_locationMessage, m_locationFailed, "OpenClient está en modo portable: los datos viven en la carpeta «data» junto al programa. Borra portable.txt para usar otra ubicación.");
        return;
    }
    if (!m_pending.isEmpty()) {
        fail(m_locationMessage, m_locationFailed, "Ya hay un cambio pendiente. Reinicia OpenClient primero.");
    } else if (m_launcher->gameBusy() || m_launcher->catalogBusy() || m_launcher->copyingInstance()) {
        fail(m_locationMessage, m_locationFailed, "Espera a que termine la partida o la operación en curso.");
    } else if (const auto error = validateTarget(m_dataDirectory, target); !error.isEmpty()) {
        fail(m_locationMessage, m_locationFailed, error);
    } else {
        m_moving = true;
        m_locationFailed = false;
        m_locationMessage = "Copiando los datos…";
        m_moveTarget = target;
        const QString current = m_dataDirectory, bootstrap = m_bootstrapFile;
        const bool isDefault = normalized(target) == normalized(defaultDataDirectory());
        m_move.setFuture(QtConcurrent::run([current, target, bootstrap, isDefault] {
            QLockFile library(QDir(current).filePath("library.lock")), account(QDir(current).filePath("account.lock"));
            library.setStaleLockTime(0);
            account.setStaleLockTime(0);
            if (!library.tryLock() || !account.tryLock()) return QString("OpenClient está en uso en otra ventana.");
            const bool existed = QFileInfo::exists(target);
            const auto error = copyTree(current, target);
            if (!error.isEmpty()) {
                if (existed) emptyDirectory(target);
                else QDir(target).removeRecursively();
                return error;
            }
            if (!writeLocation(bootstrap, isDefault ? QString() : target)) return QString("No se pudo guardar la ubicación nueva.");
            return QString();
        }));
    }
    emit locationChanged();
}

void MaintenanceController::restart() {
    if (m_pending.isEmpty() || (m_launcher && m_launcher->gameBusy())) return;
    QProcess::startDetached(QCoreApplication::applicationFilePath(), {});
    QCoreApplication::quit();
}

QString MaintenanceController::buildReport() const {
    QStringList out;
    out << "Informe de diagnóstico de OpenClient"
        << "Revisa este archivo antes de compartirlo. Se han ocultado tu carpeta de usuario y cadenas que parecen claves.\n"
        << "Versión: " + QCoreApplication::applicationVersion()
        << QString("Qt: %1").arg(qVersion())
        << QString("Sistema: %1 (%2)").arg(QSysInfo::prettyProductName(), QSysInfo::currentCpuArchitecture());
    const auto home = QStandardPaths::writableLocation(QStandardPaths::HomeLocation);
    if (m_launcher) {
        const auto hardware = m_launcher->hardware();
        out << QString("RAM instalada: %1 MB").arg(hardware.value("installed_ram_mb").toLongLong())
            << "Procesador: " + hardware.value("cpu_brand").toString();
        out << "\nAjustes"
            << QString("Tema: %1 | Potato PC: %2 | Partículas: %3").arg(m_launcher->themeId()).arg(m_launcher->potatoMode()).arg(m_launcher->particlesEnabled())
            << QString("RAM global: %1 MB | GC: %2 | Pre-asignar: %3 | Prioridad alta: %4")
                   .arg(m_launcher->allocatedRamMb()).arg(m_launcher->gcPreset()).arg(m_launcher->preTouchMemory()).arg(m_launcher->highPriority())
            << QString("Descargas simultáneas: %1 | Java seleccionado: %2").arg(m_launcher->downloadConcurrency()).arg(m_launcher->javaMajor())
            << QString("Ventana: %1x%2 | Pantalla completa: %3").arg(m_launcher->windowWidth()).arg(m_launcher->windowHeight()).arg(m_launcher->fullscreen())
            << QString("Comandos configurados: %1")
                   .arg(!m_launcher->preLaunchHook().isEmpty() || !m_launcher->postExitHook().isEmpty() || !m_launcher->wrapperCommand().isEmpty() ? "sí" : "no")
            << QString("Argumentos JVM globales: %1").arg(m_launcher->customJvmArgs().isEmpty() ? "no" : "sí");
        const auto paths = m_launcher->javaPaths();
        for (auto it = paths.cbegin(); it != paths.cend(); ++it) out << QString("Java %1: %2").arg(it.key(), it.value().toString());
        const auto instances = m_launcher->instances();
        out << QString("\nInstancias: %1").arg(instances.size());
        for (const auto &item : instances) {
            const auto instance = item.toMap();
            out << QString("- %1 %2 | RAM propia: %3 | contenido: %4")
                       .arg(instance.value("mcVersion").toString(), instance.value("category").toString())
                       .arg(instance.value("ramMb").toInt()).arg(instance.value("installedContent").toList().size());
        }
    }
    QString newest;
    QDateTime newestTime;
    for (const auto &entry : QDir(QDir(m_dataDirectory).filePath("instances")).entryInfoList(QDir::Dirs | QDir::NoDotAndDotDot)) {
        const QFileInfo log(QDir(entry.absoluteFilePath()).filePath("logs/openclient-latest.log"));
        if (log.exists() && (newest.isEmpty() || log.lastModified() > newestTime)) {
            newest = log.absoluteFilePath();
            newestTime = log.lastModified();
        }
    }
    out << "\nÚltimo registro de Minecraft" << (newest.isEmpty() ? QString("(todavía no hay registros)") : tail(newest));
    return redact(out.join('\n'), home);
}

void MaintenanceController::exportReport(const QUrl &file) {
    if (!file.isLocalFile()) return;
    QSaveFile output(file.toLocalFile());
    const bool written = output.open(QIODevice::WriteOnly) && output.write(buildReport().toUtf8()) >= 0 && output.commit();
    m_reportFailed = !written;
    m_reportMessage = written ? "Informe guardado. Revísalo antes de compartirlo." : "No se pudo guardar el informe.";
    emit reportChanged();
}

void MaintenanceController::setUpdateChannel(const QString &value) {
    if ((value != "stable" && value != "beta") || value == m_channel) return;
    m_channel = value;
    m_settings.setValue("updates/channel", m_channel);
    m_settings.sync();
    m_updateUrl.clear();
    m_updateSha256.clear();
    m_updateSize = 0;
    m_updateMessage.clear();
    emit updateChanged();
}

void MaintenanceController::checkForUpdates() {
    if (m_checking || m_updateReply) return;
    const QUrl manifest(m_manifestUrl);
    m_updateUrl.clear();
    m_updateSha256.clear();
    m_updateSize = 0;
    if (!updatesConfigured() || manifest.scheme() != "https") {
        m_updateMessage = "Esta compilación no tiene un canal de actualizaciones configurado.";
        emit updateChanged();
        return;
    }
    m_checking = true;
    m_updateMessage = "Buscando…";
    emit updateChanged();
    QNetworkRequest request(manifest);
    request.setTransferTimeout(10000);
    // GitHub redirige releases/latest/download/manifest.json a su CDN: se siguen redirecciones solo a HTTPS.
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
    request.setMaximumRedirectsAllowed(5);
    request.setRawHeader("Accept", "application/json");
    auto *reply = m_network.get(request);
    connect(reply, &QNetworkReply::finished, this, [this, reply, manifest] {
        reply->deleteLater();
        m_checking = false;
        const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        if (reply->error() != QNetworkReply::NoError || status != 200) {
            m_updateMessage = "No se pudo consultar las actualizaciones. Revisa tu conexión.";
        } else if (reply->size() > manifestLimit) {
            m_updateMessage = "La respuesta de actualizaciones es demasiado grande.";
        } else {
            const auto result = parseManifest(reply->readAll(), m_channel, QCoreApplication::applicationVersion());
            if (!result.value("ok").toBool()) {
                m_updateMessage = result.value("error").toString();
            } else if (!result.value("available").toBool()) {
                m_updateMessage = QString("Tienes la versión más reciente (%1).").arg(QCoreApplication::applicationVersion());
            } else {
                const QUrl download(result.value("url").toString());
                if (download.host() == manifest.host() || download.host() == "github.com") {
                    m_updateUrl = download.toString();
                    m_updateSha256 = result.value("sha256").toString();
                    m_updateSize = result.value("size").toLongLong();
                    m_updateMessage = QString("Hay una versión nueva: %1.").arg(result.value("version").toString());
                } else {
                    m_updateMessage = "La descarga indicada no está en un sitio de confianza; no se ofrece.";
                }
            }
        }
        emit updateChanged();
    });
}

void MaintenanceController::openUpdate() {
    const QUrl url(m_updateUrl);
    if (url.scheme() == "https") QDesktopServices::openUrl(url);
}

// Descarga el instalador nuevo, comprueba tamaño y SHA-256 del manifiesto y solo entonces lo abre.
// Lo inicia el usuario con un botón; el instalador se abre con su asistente normal y el launcher se cierra.
void MaintenanceController::installUpdate() {
    if (m_updateReply || m_checking || !updateInstallable()) return;
    if (m_launcher && m_launcher->gameBusy()) {
        m_updateMessage = "Cierra Minecraft antes de actualizar.";
        emit updateChanged();
        return;
    }
    const QUrl url(m_updateUrl);
    const QUrl manifest(m_manifestUrl);
    if (url.scheme() != "https" || (url.host() != manifest.host() && url.host() != "github.com")) return;
    const QString directory = QDir(QStandardPaths::writableLocation(QStandardPaths::TempLocation)).filePath("OpenClient-update");
    const QString target = QDir(directory).filePath("Instalar-OpenClient.exe");
    const QString partPath = target + ".part";
    auto part = std::make_shared<QFile>(partPath);
    if (!QDir().mkpath(directory) || !part->open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        m_updateMessage = "No se pudo preparar la descarga de la actualización.";
        emit updateChanged();
        return;
    }
    QNetworkRequest request(url);
    request.setHeader(QNetworkRequest::UserAgentHeader, "OpenClient/" + QCoreApplication::applicationVersion());
    request.setTransferTimeout(30000);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
    request.setMaximumRedirectsAllowed(5);
    const qint64 expectedSize = m_updateSize;
    const QByteArray expectedHash = m_updateSha256.toLatin1();
    auto hash = std::make_shared<QCryptographicHash>(QCryptographicHash::Sha256);
    auto written = std::make_shared<qint64>(0);
    auto oversize = std::make_shared<bool>(false);
    m_updatePercent = 0;
    m_updateMessage = "Descargando la actualización… 0 %";
    auto *reply = m_network.get(request);
    m_updateReply = reply;
    emit updateChanged();
    connect(reply, &QNetworkReply::readyRead, this, [reply, part, hash, written, oversize, expectedSize] {
        const auto chunk = reply->readAll();
        if (*oversize) return;
        if (*written + chunk.size() > expectedSize || part->write(chunk) != chunk.size()) {
            *oversize = true;
            reply->abort();
            return;
        }
        hash->addData(chunk);
        *written += chunk.size();
    });
    connect(reply, &QNetworkReply::downloadProgress, this, [this, expectedSize](qint64 received, qint64) {
        const int percent = static_cast<int>(std::clamp<qint64>(received * 100 / expectedSize, 0, 99));
        if (percent == m_updatePercent) return;
        m_updatePercent = percent;
        m_updateMessage = QString("Descargando la actualización… %1 %").arg(percent);
        emit updateChanged();
    });
    connect(reply, &QNetworkReply::finished, this,
            [this, reply, part, hash, written, oversize, expectedSize, expectedHash, target, partPath] {
        reply->deleteLater();
        m_updateReply = nullptr;
        m_updatePercent = -1;
        part->close();
        const bool cancelled = !*oversize && reply->error() == QNetworkReply::OperationCanceledError;
        const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        QString failure;
        if (cancelled) failure = "Descarga cancelada.";
        else if (*oversize) failure = "El instalador descargado es más grande de lo esperado; se descartó.";
        else if (reply->error() != QNetworkReply::NoError || status != 200)
            failure = "No se pudo descargar la actualización. Revisa tu conexión.";
        else if (*written != expectedSize) failure = "La descarga quedó incompleta.";
        else if (hash->result().toHex() != expectedHash)
            failure = "El instalador descargado no coincide con el publicado; se descartó.";
        else if (QFile::exists(target) && !QFile::remove(target)) failure = "No se pudo reemplazar el instalador anterior.";
        else if (!QFile::rename(partPath, target)) failure = "No se pudo guardar el instalador.";
        if (!failure.isEmpty()) {
            QFile::remove(partPath);
            m_updateMessage = failure;
            emit updateChanged();
            return;
        }
        if (m_launcher && m_launcher->gameBusy()) {
            m_updateMessage = "Descarga verificada, pero Minecraft sigue abierto. Ciérralo y vuelve a pulsar Actualizar.";
            emit updateChanged();
            return;
        }
        if (!QProcess::startDetached(target, {})) {
            m_updateMessage = "La descarga se verificó, pero no se pudo abrir el instalador.";
            emit updateChanged();
            return;
        }
        m_updateMessage = "Abriendo el instalador…";
        emit updateChanged();
        QTimer::singleShot(400, qApp, &QCoreApplication::quit);
    });
}

void MaintenanceController::cancelUpdateDownload() {
    if (m_updateReply) m_updateReply->abort();
}
