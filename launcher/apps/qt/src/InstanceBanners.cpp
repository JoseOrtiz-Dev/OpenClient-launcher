#include "InstanceBanners.h"
#include <QBuffer>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QImageReader>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QPainter>
#include <QRegularExpression>
#include <QSaveFile>
#include <QThread>
#include <QUuid>
#include <QtConcurrent>
#include <algorithm>

namespace {
constexpr qint64 kCustomLimit = 20LL * 1024 * 1024;
constexpr qint64 kModpackLimit = 8LL * 1024 * 1024;
constexpr qint64 kProjectLimit = 2LL * 1024 * 1024;
constexpr int kMaxSide = 16384;
constexpr qint64 kMaxPixels = 100000000;
constexpr int kReadSide = 2560;
const QStringList imageExtensions = {"png", "jpg", "jpeg", "webp"};
const QStringList imageFormats = {"png", "jpeg", "webp"};

bool sizeAllowed(const QSize &size) {
    return size.isValid() && size.width() <= kMaxSide && size.height() <= kMaxSide
        && qint64(size.width()) * size.height() <= kMaxPixels;
}

bool saveJpeg(const QImage &image, const QString &path, int quality) {
    const auto temporary = path + "." + QUuid::createUuid().toString(QUuid::Id128).left(8) + ".tmp";
    if (!image.save(temporary, "JPG", quality)) {
        QFile::remove(temporary);
        return false;
    }
    QFile::remove(path);
    if (QFile::rename(temporary, path)) return true;
    QFile::remove(temporary);
    return QFile::exists(path);
}

QString accentOf(const QImage &sharp) {
    const auto tiny = sharp.scaled(16, 5, Qt::IgnoreAspectRatio, Qt::SmoothTransformation).convertToFormat(QImage::Format_RGB32);
    double r = 0, g = 0, b = 0, weight = 0, mr = 0, mg = 0, mb = 0;
    for (int y = 0; y < tiny.height(); ++y) {
        for (int x = 0; x < tiny.width(); ++x) {
            const QColor pixel = tiny.pixelColor(x, y);
            const double w = pixel.hslSaturationF();
            r += w * pixel.redF();
            g += w * pixel.greenF();
            b += w * pixel.blueF();
            weight += w;
            mr += pixel.redF();
            mg += pixel.greenF();
            mb += pixel.blueF();
        }
    }
    const double count = tiny.width() * tiny.height();
    // Una imagen casi gris no tiene color dominante: se usa la media simple y luego se fuerza la saturación.
    const QColor base = weight > 0.05 ? QColor::fromRgbF(r / weight, g / weight, b / weight)
                                      : QColor::fromRgbF(mr / count, mg / count, mb / count);
    float h = base.hslHueF(), s = base.hslSaturationF(), l = base.lightnessF();
    if (h < 0) h = 0.6f;
    s = std::max(s, 0.45f);
    l = std::clamp(l, 0.45f, 0.62f);
    return QColor::fromHslF(h, s, l).name();
}
}

InstanceBanners::InstanceBanners(const QString &root, QObject *parent) : QObject(parent), m_root(QDir::cleanPath(root)) {}

InstanceBanners::~InstanceBanners() {
    for (auto *watcher : std::as_const(m_watchers)) watcher->waitForFinished();
}

QVariantMap InstanceBanners::generated() {
    return {{"source", "generated"}, {"sharp", QString()}, {"soft", QString()}, {"accent", QString()}};
}

bool InstanceBanners::validProject(const QString &project) {
    static const QRegularExpression pattern("^[A-Za-z0-9_-]{1,64}$");
    return pattern.match(project).hasMatch();
}

QString InstanceBanners::cacheDirectory(const QString &root) {
    return QDir::cleanPath(root + "/cache/banners");
}

QString InstanceBanners::customPath(const QString &root, const QString &id) {
    for (const auto &extension : imageExtensions) {
        const auto path = QDir::cleanPath(root + "/banners/" + id + "." + extension);
        if (QFileInfo(path).isFile()) return path;
    }
    return {};
}

QString InstanceBanners::latestScreenshot(const QString &directory) {
    if (directory.isEmpty()) return {};
    const QDir dir(directory);
    const auto entries = dir.entryInfoList({"*.png", "*.jpg", "*.jpeg"}, QDir::Files | QDir::NoSymLinks, QDir::Time);
    for (const auto &entry : entries) {
        if (QDir::fromNativeSeparators(entry.canonicalFilePath()).compare(QDir::fromNativeSeparators(dir.filePath(entry.fileName())), Qt::CaseInsensitive) == 0)
            return entry.filePath();
    }
    return {};
}

InstanceBanners::Processed InstanceBanners::processImage(const QString &root, const QString &sourcePath) {
    Processed out;
    const QFileInfo source(sourcePath);
    if (!source.isFile()) return out;
    const auto sourceKey = QString("v1|%1|%2|%3").arg(source.absoluteFilePath()).arg(source.size()).arg(source.lastModified().toMSecsSinceEpoch());
    out.key = QCryptographicHash::hash(sourceKey.toUtf8(), QCryptographicHash::Sha1).toHex().left(24);
    const auto directory = cacheDirectory(root);
    const auto sharpPath = directory + "/" + out.key + "-sharp.jpg";
    const auto softPath = directory + "/" + out.key + "-soft.jpg";
    const auto jsonPath = directory + "/" + out.key + ".json";
    out.sharp = sharpPath;
    out.soft = softPath;
    if (QFileInfo(sharpPath).isFile() && QFileInfo(softPath).isFile()) {
        QFile file(jsonPath);
        if (file.open(QIODevice::ReadOnly)) {
            const auto saved = QJsonDocument::fromJson(file.read(4096)).object();
            const auto accent = saved.value("accent").toString();
            if (saved.value("sourceKey").toString() == sourceKey && QColor::isValidColorName(accent)) {
                out.accent = accent;
                out.ok = true;
                return out;
            }
        }
    }
    QImageReader reader(sourcePath);
    reader.setAutoTransform(true);
    const auto size = reader.size();
    if (size.isValid()) {
        if (!sizeAllowed(size)) return Processed{};
        if (size.width() > kReadSide || size.height() > kReadSide)
            reader.setScaledSize(size.scaled(kReadSide, kReadSide, Qt::KeepAspectRatio));
    }
    QImage image = reader.read();
    if (image.isNull() || !sizeAllowed(image.size())) return Processed{};
    QImage flat(image.size(), QImage::Format_RGB32);
    flat.fill(Qt::black);
    {
        QPainter painter(&flat);
        painter.drawImage(0, 0, image);
    }
    const auto filled = flat.scaled(1280, 400, Qt::KeepAspectRatioByExpanding, Qt::SmoothTransformation);
    const auto sharp = filled.copy((filled.width() - 1280) / 2, (filled.height() - 400) / 2, 1280, 400);
    const auto soft = sharp.scaled(48, 15, Qt::IgnoreAspectRatio, Qt::SmoothTransformation)
                          .scaled(960, 300, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
    out.accent = accentOf(sharp);
    if (!QDir().mkpath(directory) || !saveJpeg(sharp, sharpPath, 85) || !saveJpeg(soft, softPath, 80)) return Processed{};
    QSaveFile json(jsonPath);
    if (!json.open(QIODevice::WriteOnly)) return Processed{};
    json.write(QJsonDocument(QJsonObject{{"accent", out.accent}, {"sourceKey", sourceKey}}).toJson(QJsonDocument::Compact));
    if (!json.commit()) return Processed{};
    out.ok = true;
    return out;
}

InstanceBanners::Outcome InstanceBanners::resolve(const QString &root, const QString &id, const QString &screenshots, const QString &project) {
    Outcome outcome;
    outcome.info = generated();
    QList<QPair<QString, QString>> candidates;
    const auto custom = customPath(root, id);
    if (!custom.isEmpty()) candidates.append({"custom", custom});
    if (validProject(project)) {
        const auto base = cacheDirectory(root) + "/modpack-" + project;
        bool found = false;
        for (const auto &extension : imageExtensions) {
            if (QFileInfo(base + "." + extension).isFile()) {
                candidates.append({"modpack", base + "." + extension});
                found = true;
                break;
            }
        }
        const QFileInfo none(base + ".none");
        const bool recentFailure = none.isFile() && none.lastModified() > QDateTime::currentDateTime().addDays(-7);
        outcome.wantModpack = custom.isEmpty() && !found && !recentFailure;
    }
    if (const auto shot = latestScreenshot(screenshots); !shot.isEmpty()) candidates.append({"screenshot", shot});
    for (const auto &[kind, path] : std::as_const(candidates)) {
        const auto processed = processImage(root, path);
        if (!processed.ok) continue;
        outcome.key = processed.key;
        outcome.info = {{"source", kind},
                        {"sharp", QUrl::fromLocalFile(processed.sharp).toString()},
                        {"soft", QUrl::fromLocalFile(processed.soft).toString()},
                        {"accent", processed.accent}};
        break;
    }
    return outcome;
}

bool InstanceBanners::lookup(const QString &id, QVariantMap &info) {
    const auto it = m_entries.find(id);
    if (it == m_entries.end() || it->gone) return false;
    info = it->info;
    // «Limpiar caché» puede borrar los derivados: se regeneran y mientras tanto se muestra el banner generado.
    if (info.value("source").toString() != "generated" && !QFileInfo::exists(QUrl(info.value("sharp").toString()).toLocalFile())) {
        info = generated();
        if (!it->pending) launch(id);
    }
    return true;
}

void InstanceBanners::request(const QString &id, const QString &screenshotsDir, const QString &project) {
    auto &entry = m_entries[id];
    entry.gone = false;
    entry.screenshots = screenshotsDir;
    entry.project = project;
    launch(id);
}

void InstanceBanners::launch(const QString &id) {
    auto &entry = m_entries[id];
    if (entry.pending) {
        entry.dirty = true;
        return;
    }
    entry.pending = true;
    ++m_pending;
    auto *watcher = new QFutureWatcher<Outcome>(this);
    m_watchers.append(watcher);
    connect(watcher, &QFutureWatcher<Outcome>::finished, this, [this, watcher, id] {
        m_watchers.removeOne(watcher);
        const auto outcome = watcher->result();
        watcher->deleteLater();
        finish(id, outcome);
    });
    watcher->setFuture(QtConcurrent::run([root = m_root, id, shots = entry.screenshots, project = entry.project] {
        return resolve(root, id, shots, project);
    }));
}

void InstanceBanners::finish(const QString &id, const Outcome &outcome) {
    --m_pending;
    const auto it = m_entries.find(id);
    if (it == m_entries.end()) {
        removeDerivatives(m_root, outcome.key);
        return;
    }
    it->pending = false;
    if (it->gone) {
        m_stale.insert(outcome.key);
        m_entries.erase(it);
        sweep();
        return;
    }
    const bool different = it->info != outcome.info;
    if (it->key != outcome.key) {
        if (!it->key.isEmpty()) m_stale.insert(it->key);
        it->key = outcome.key;
    }
    it->info = outcome.info;
    const auto project = it->project;
    if (it->dirty) {
        it->dirty = false;
        launch(id);
    }
    if (outcome.wantModpack) fetchProject(project);
    sweep();
    if (different) {
        ++m_revision;
        emit changed();
    }
}

// Los derivados de una clave solo se borran sin trabajos en vuelo y si ninguna instancia los usa.
void InstanceBanners::sweep() {
    if (m_pending > 0) return;
    for (const auto &key : std::as_const(m_stale)) {
        const bool used = std::any_of(m_entries.cbegin(), m_entries.cend(), [&key](const Entry &entry) { return entry.key == key; });
        if (!used) removeDerivatives(m_root, key);
    }
    m_stale.clear();
}

void InstanceBanners::removeDerivatives(const QString &root, const QString &key) {
    if (key.isEmpty()) return;
    const auto base = cacheDirectory(root) + "/" + key;
    QFile::remove(base + "-sharp.jpg");
    QFile::remove(base + "-soft.jpg");
    QFile::remove(base + ".json");
}

QString InstanceBanners::setCustom(const QString &id, const QUrl &file) {
    if (!file.isLocalFile()) return "Elige un archivo de imagen del disco.";
    const QFileInfo info(file.toLocalFile());
    const auto extension = info.suffix().toLower();
    if (!info.isFile() || !imageExtensions.contains(extension)) return "La imagen debe ser PNG, JPG o WebP.";
    if (info.size() > kCustomLimit) return "La imagen pesa más de 20 MB.";
    QImageReader reader(info.filePath());
    reader.setAutoTransform(true);
    const auto size = reader.size();
    if (!imageFormats.contains(QString::fromLatin1(reader.format())) || !sizeAllowed(size)) return "No se pudo leer la imagen o es demasiado grande.";
    reader.setScaledSize(size.scaled(64, 64, Qt::KeepAspectRatio));
    if (reader.read().isNull()) return "No se pudo leer la imagen o es demasiado grande.";
    const auto directory = QDir::cleanPath(m_root + "/banners");
    const auto destination = directory + "/" + id + "." + extension;
    if (!QDir().mkpath(directory)) return "No se pudo crear la carpeta de banners.";
    if (QFileInfo(destination).exists() && QFileInfo(destination).canonicalFilePath() == info.canonicalFilePath()) return {};
    const auto temporary = destination + ".tmp";
    QFile::remove(temporary);
    if (!QFile::copy(info.filePath(), temporary)) return "No se pudo copiar la imagen.";
    for (const auto &other : imageExtensions) QFile::remove(directory + "/" + id + "." + other);
    if (!QFile::rename(temporary, destination)) {
        QFile::remove(temporary);
        return "No se pudo guardar la imagen.";
    }
    return {};
}

void InstanceBanners::clearCustom(const QString &id) {
    for (const auto &extension : imageExtensions) QFile::remove(QDir::cleanPath(m_root + "/banners/" + id + "." + extension));
}

void InstanceBanners::remove(const QString &id) {
    clearCustom(id);
    const auto it = m_entries.find(id);
    if (it == m_entries.end()) return;
    if (it->pending) {
        it->gone = true;
        return;
    }
    m_stale.insert(it->key);
    m_entries.erase(it);
    sweep();
}

bool InstanceBanners::waitIdle(int timeoutMs) {
    QElapsedTimer timer;
    timer.start();
    while (m_pending > 0 && timer.elapsed() < timeoutMs) {
        QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
        QThread::msleep(2);
    }
    return m_pending == 0;
}

// ---------------------------------------------------------------- modpack

QNetworkReply *InstanceBanners::download(const QUrl &url, qint64 limit) {
    if (!m_network) m_network = new QNetworkAccessManager(this);
    QNetworkRequest request(url);
    request.setHeader(QNetworkRequest::UserAgentHeader, "OpenClient/" + QCoreApplication::applicationVersion());
    request.setTransferTimeout(10000);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::ManualRedirectPolicy);
    auto *reply = m_network->get(request);
    connect(reply, &QNetworkReply::downloadProgress, reply, [reply, limit](qint64 received, qint64 total) {
        if (received > limit || total > limit) reply->abort();
    });
    return reply;
}

void InstanceBanners::fetchProject(const QString &project) {
    if (!validProject(project) || m_tried.contains(project)) return;
    m_tried.insert(project);
    auto *reply = download(QUrl("https://api.modrinth.com/v2/project/" + project), kProjectLimit);
    connect(reply, &QNetworkReply::finished, this, [this, reply, project] {
        reply->deleteLater();
        const bool ok = reply->error() == QNetworkReply::NoError && reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt() == 200;
        const auto data = ok ? reply->read(kProjectLimit + 1) : QByteArray();
        if (!ok || data.size() > kProjectLimit) return markNone(project);
        const auto gallery = QJsonDocument::fromJson(data).object().value("gallery").toArray();
        QString chosen;
        for (const auto &item : gallery) {
            if (item.toObject().value("featured").toBool()) { chosen = item.toObject().value("url").toString(); break; }
        }
        if (chosen.isEmpty() && !gallery.isEmpty()) chosen = gallery.first().toObject().value("url").toString();
        const QUrl url(chosen);
        // Solo el CDN de Modrinth: la URL viene de datos remotos.
        if (chosen.isEmpty() || url.scheme() != "https" || url.host() != "cdn.modrinth.com" || url.port() != -1
            || !url.userInfo().isEmpty() || !url.path().startsWith("/data/")) return markNone(project);
        fetchImage(project, url);
    });
}

void InstanceBanners::fetchImage(const QString &project, const QUrl &url) {
    const auto extension = QFileInfo(url.path()).suffix().toLower();
    if (!imageExtensions.contains(extension)) return markNone(project);
    auto *reply = download(url, kModpackLimit);
    connect(reply, &QNetworkReply::finished, this, [this, reply, project, extension] {
        reply->deleteLater();
        const bool ok = reply->error() == QNetworkReply::NoError && reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt() == 200;
        auto data = ok ? reply->read(kModpackLimit + 1) : QByteArray();
        if (!ok || data.isEmpty() || data.size() > kModpackLimit) return markNone(project);
        QBuffer buffer(&data);
        buffer.open(QIODevice::ReadOnly);
        QImageReader reader(&buffer);
        if (!imageFormats.contains(QString::fromLatin1(reader.format())) || !sizeAllowed(reader.size())) return markNone(project);
        const auto directory = cacheDirectory(m_root);
        QSaveFile file(directory + "/modpack-" + project + "." + extension);
        if (!QDir().mkpath(directory) || !file.open(QIODevice::WriteOnly) || file.write(data) != data.size() || !file.commit())
            return markNone(project);
        QFile::remove(directory + "/modpack-" + project + ".none");
        refreshProject(project);
    });
}

void InstanceBanners::markNone(const QString &project) {
    const auto directory = cacheDirectory(m_root);
    if (!QDir().mkpath(directory)) return;
    QFile marker(directory + "/modpack-" + project + ".none");
    if (marker.open(QIODevice::WriteOnly)) marker.write(QDateTime::currentDateTimeUtc().toString(Qt::ISODate).toUtf8());
}

void InstanceBanners::refreshProject(const QString &project) {
    for (const auto &id : m_entries.keys()) {
        if (m_entries.value(id).project == project) launch(id);
    }
}
