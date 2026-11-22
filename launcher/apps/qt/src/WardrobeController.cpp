#include "WardrobeController.h"

#include <QBuffer>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImageReader>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkReply>
#include <QPainter>
#include <QRegularExpression>
#include <QSaveFile>
#include <QUuid>
#include <algorithm>

namespace {
constexpr qsizetype maxBytes = 1024 * 1024;
constexpr int maxSaved = 200;
const QRegularExpression savedId("^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$");
const QRegularExpression profileId("^[0-9a-f]{32}$");

QImage decodeTexture(QByteArray bytes, bool cape = false) {
    if (bytes.isEmpty() || bytes.size() > maxBytes) return {};
    QBuffer buffer(&bytes);
    if (!buffer.open(QIODevice::ReadOnly)) return {};
    QImageReader reader(&buffer, "png");
    const auto size = reader.size();
    if (!cape) {
        const bool valid = size == QSize(64, 64) || size == QSize(64, 32) || size == QSize(128, 128);
        return valid ? reader.read() : QImage();
    }
    if (size == QSize(64, 32) || size == QSize(128, 64)) return reader.read();
    if (size == QSize(46, 22) || size == QSize(92, 44) || size == QSize(22, 17)) {
        const auto raw = reader.read();
        if (raw.isNull()) return {};
        const int scale = size.width() > 64 ? 2 : 1;
        QImage padded(64 * scale, 32 * scale, QImage::Format_ARGB32);
        padded.fill(Qt::transparent);
        QPainter p(&padded);
        p.drawImage(0, 0, raw);
        return padded;
    }
    return {};
}

QImage readSkin(const QString &path) {
    QFile file(path);
    if (!QFileInfo(path).isFile() || !file.open(QIODevice::ReadOnly) || file.size() > maxBytes) return {};
    return decodeTexture(file.read(maxBytes + 1));
}

QString headIcon(const QImage &skin) {
    if (skin.isNull()) return {};
    const int scale = std::max(1, skin.width() / 64);
    QImage face(8 * scale, 8 * scale, QImage::Format_ARGB32_Premultiplied);
    face.fill(Qt::transparent);
    QPainter painter(&face);
    painter.drawImage(QPoint(0, 0), skin, QRect(8 * scale, 8 * scale, face.width(), face.height()));
    painter.drawImage(QPoint(0, 0), skin, QRect(40 * scale, 8 * scale, face.width(), face.height()));
    painter.end();
    QByteArray bytes;
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::WriteOnly);
    face.save(&buffer, "PNG");
    return "data:image/png;base64," + QString::fromLatin1(bytes.toBase64());
}

QString capeIcon(const QImage &cape) {
    if (cape.isNull()) return {};
    const int scale = std::max(1, cape.width() / 64);
    QImage front(10 * scale, 16 * scale, QImage::Format_ARGB32_Premultiplied);
    front.fill(Qt::transparent);
    QPainter painter(&front);
    painter.drawImage(QPoint(0, 0), cape, QRect(1 * scale, 1 * scale, front.width(), front.height()));
    painter.end();
    QByteArray bytes;
    QBuffer buffer(&bytes);
    buffer.open(QIODevice::WriteOnly);
    front.save(&buffer, "PNG");
    return "data:image/png;base64," + QString::fromLatin1(bytes.toBase64());
}

QImage cleanTexture(const QImage &skin) {
    QImage image(skin.size(), QImage::Format_ARGB32);
    image.fill(Qt::transparent);
    QPainter painter(&image);
    painter.setCompositionMode(QPainter::CompositionMode_Source);
    painter.drawImage(QPoint(), skin);
    return image;
}

QUrl textureUrl(const QJsonObject &entry) {
    auto url = QUrl(entry.value("url").toString(), QUrl::StrictMode);
    static const QRegularExpression texturePath("^/texture/[0-9a-fA-F]{32,64}$");
    if (!url.isValid() || (url.scheme() != "http" && url.scheme() != "https")
        || url.host() != "textures.minecraft.net" || url.port() != -1
        || !url.userInfo().isEmpty() || url.hasQuery() || url.hasFragment()
        || !texturePath.match(url.path()).hasMatch()) return {};
    url.setScheme("https");
    return url;
}
}

WardrobeController::WardrobeController(const QString &root) : m_root(root) {
    reload();
}

bool WardrobeController::fail(const QString &message) {
    m_message = message;
    emit statusChanged();
    return false;
}

void WardrobeController::showSkin(QImage image, const QString &name, bool slim, const QString &collectionId) {
    m_skin = std::move(image);
    m_faceIcon = headIcon(m_skin);
    m_name = name;
    m_slim = slim && !legacy();
    m_savedSkinId = collectionId;
    emit previewChanged();
}

void WardrobeController::setSlim(bool value) {
    if (m_busy || m_skin.isNull() || legacy() || m_slim == value) return;
    m_slim = value;
    emit previewChanged();
}

void WardrobeController::get(const QUrl &url, Response response) {
    const auto host = url.host();
    if (!url.isValid() || url.scheme() != "https" || url.port() != -1 || !url.userInfo().isEmpty()
        || (host != "api.mojang.com" && host != "sessionserver.mojang.com" && host != "textures.minecraft.net")) {
        response({}, 0);
        return;
    }
    QNetworkRequest request(url);
    request.setRawHeader("User-Agent", "OpenClient/0.1 (Minecraft desktop launcher)");
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::ManualRedirectPolicy);
    request.setTransferTimeout(8000);
    auto *reply = m_network.get(request);
    reply->setReadBufferSize(maxBytes + 1);
    connect(reply, &QIODevice::readyRead, this, [reply] {
        if (reply->bytesAvailable() > maxBytes) reply->abort();
    });
    connect(reply, &QNetworkReply::finished, this, [reply, response = std::move(response)] {
        const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const auto data = reply->error() == QNetworkReply::NoError && status == 200
            ? reply->read(maxBytes + 1) : QByteArray();
        reply->deleteLater();
        response(data, status);
    });
}

void WardrobeController::syncAccount(const QString &username, const QVariantMap &microsoftProfile) {
    static const QRegularExpression namePattern("^[A-Za-z0-9_]{3,16}$");
    const auto name = username.trimmed();
    m_accountName = name;
    if (!namePattern.match(name).hasMatch()) {
        m_accountFace.clear();
        m_accountCapes.clear();
        m_capes.clear();
        selectCape("none");
        return;
    }
    m_accountCapes.clear();
    for (const auto &entry : microsoftProfile.value("capes").toList()) {
        const auto map = entry.toMap();
        const auto url = textureUrl(QJsonObject{{"url", map.value("url").toString()}});
        if (url.isEmpty()) continue;
        const auto id = map.value("id", url.toString()).toString();
        const auto alias = map.value("alias").toString();
        const auto state = map.value("state").toString();
        m_accountCapes.append(QVariantMap{
            {"id", id},
            {"alias", alias.isEmpty() ? "Capa oficial" : alias},
            {"url", url.toString()},
            {"state", state},
            {"official", true},
            {"source", "Mojang"}
        });
    }
    m_capes = m_accountCapes;
    m_cape = {};
    const auto activeCapeId = microsoftProfile.value("capeId").toString();
    m_selectedCapeId = !activeCapeId.isEmpty() ? activeCapeId
        : (!m_capes.isEmpty() ? m_capes.first().toMap().value("id").toString() : QString());
    emit previewChanged();
    loadCapeIcons();

    const auto directSkinUrl = textureUrl(QJsonObject{{"url", microsoftProfile.value("skinUrl").toString()}});
    const bool slim = microsoftProfile.value("skinVariant").toString().compare("slim", Qt::CaseInsensitive) == 0;
    if (!directSkinUrl.isEmpty()) {
        get(directSkinUrl, [this, name, slim](const QByteArray &png, int) {
            auto image = decodeTexture(png);
            if (image.isNull() || m_accountName != name) return;
            m_accountFace = headIcon(image);
            const bool showAccountSkin = !m_busy && m_savedSkinId.isEmpty();
            if (showAccountSkin) showSkin(std::move(image), name, slim);
            else emit previewChanged();
            if (m_busy) return;
            if (!m_selectedCapeId.isEmpty()) selectCape(m_selectedCapeId);
            if (!showAccountSkin) return;
            m_message = m_capes.isEmpty()
                ? "Skin de tu cuenta cargada."
                : QString("Skin y %1 capa(s) de tu cuenta cargadas.").arg(m_capes.size());
            emit statusChanged();
        });
        return;
    }

    get(QUrl("https://api.mojang.com/users/profiles/minecraft/" + name), [this, name](const QByteArray &data, int) {
        if (m_accountName != name) return;
        const auto profile = QJsonDocument::fromJson(data).object();
        const auto id = profile.value("id").toString();
        if (!profileId.match(id).hasMatch()) return;
        loadProfile(profile.value("name").toString(name), id, true);
    });
}

// Cada capa necesita su textura para dibujar la miniatura; sin esto solo la seleccionada la tenía.
void WardrobeController::loadCapeIcons() {
    const auto account = m_accountName;
    for (const auto &entry : std::as_const(m_capes)) {
        const auto map = entry.toMap();
        if (!map.value("icon").toString().isEmpty()) continue;
        const auto id = map.value("id").toString();
        const QUrl url(map.value("url").toString(), QUrl::StrictMode);
        if (!url.isValid() || url.scheme() != "https") continue;
        get(url, [this, account, id, url](const QByteArray &bytes, int) {
            if (m_accountName != account) return;
            const auto icon = capeIcon(decodeTexture(bytes, true));
            if (icon.isEmpty()) return;
            // m_accountCapes se guarda aparte: sin esto la miniatura se perdería al volver a tu cuenta.
            bool changed = false;
            for (auto *list : {&m_capes, &m_accountCapes}) {
                for (int i = 0; i < list->size(); ++i) {
                    auto item = list->at(i).toMap();
                    if (item.value("id").toString() != id || QUrl(item.value("url").toString()) != url) continue;
                    item.insert("icon", icon);
                    (*list)[i] = item;
                    changed = true;
                }
            }
            if (changed) emit previewChanged();
        });
    }
}

void WardrobeController::selectCape(const QString &capeId) {
    if (capeId.isEmpty() || capeId == "none") {
        m_selectedCapeId.clear();
        m_cape = {};
        emit previewChanged();
        return;
    }
    QVariantMap chosen;
    for (const auto &entry : m_capes) {
        const auto map = entry.toMap();
        if (map.value("id").toString() == capeId || map.value("url").toString() == capeId) {
            chosen = map;
            break;
        }
    }
    if (chosen.isEmpty()) return;
    const QUrl url(chosen.value("url").toString(), QUrl::StrictMode);
    if (!url.isValid() || url.scheme() != "https") return;
    if (m_selectedCapeId != chosen.value("id").toString()) m_cape = {};
    m_selectedCapeId = chosen.value("id").toString();
    emit previewChanged();
    get(url, [this, id = m_selectedCapeId, url](const QByteArray &bytes, int) {
        auto decoded = decodeTexture(bytes, true);
        if (decoded.isNull()) return;
        const auto icon = capeIcon(decoded);
        for (int i = 0; i < m_capes.size(); ++i) {
            auto map = m_capes[i].toMap();
            if (map.value("id").toString() != id || QUrl(map.value("url").toString()) != url) continue;
            map.insert("icon", icon);
            m_capes[i] = map;
            if (m_selectedCapeId == id) m_cape = std::move(decoded);
            emit previewChanged();
            return;
        }
    });
}

void WardrobeController::lookup(const QString &player) {
    if (m_busy) return;
    static const QRegularExpression namePattern("^[A-Za-z0-9_]{3,16}$");
    const auto name = player.trimmed();
    if (!namePattern.match(name).hasMatch()) {
        fail("Escribe un nombre de Minecraft Java de 3 a 16 caracteres.");
        return;
    }
    m_busy = true;
    m_message = "Buscando el perfil público de Minecraft…";
    m_capes = name.compare(m_accountName, Qt::CaseInsensitive) == 0 ? m_accountCapes : QVariantList{};
    m_selectedCapeId = !m_capes.isEmpty() ? m_capes.first().toMap().value("id").toString() : QString();
    m_cape = {};
    emit previewChanged();
    emit statusChanged();
    get(QUrl("https://api.mojang.com/users/profiles/minecraft/" + name), [this, name](const QByteArray &data, int status) {
        const auto profile = QJsonDocument::fromJson(data).object();
        const auto id = profile.value("id").toString();
        if (!profileId.match(id).hasMatch()) {
            finishLookup(status == 404 || status == 204 ? "No se encontró ese jugador de Minecraft Java." :
                "No se pudo consultar Minecraft. Inténtalo de nuevo.");
            return;
        }
        loadProfile(profile.value("name").toString(name), id);
    });
}

void WardrobeController::loadProfile(const QString &player, const QString &id, bool background) {
    get(QUrl("https://sessionserver.mojang.com/session/minecraft/profile/" + id),
        [this, player, id, background](const QByteArray &data, int status) {
            if (background && (m_busy || m_accountName.compare(player, Qt::CaseInsensitive) != 0)) return;
            QJsonObject textures;
            const auto profile = QJsonDocument::fromJson(data).object();
            if (profile.value("id").toString() == id) {
                for (const auto &value : profile.value("properties").toArray()) {
                    const auto property = value.toObject();
                    if (property.value("name") != QStringLiteral("textures")) continue;
                    textures = QJsonDocument::fromJson(QByteArray::fromBase64(
                        property.value("value").toString().toLatin1())).object().value("textures").toObject();
                    break;
                }
            }
            const auto skin = textures.value("SKIN").toObject();
            const auto skinUrl = textureUrl(skin);
            if (skinUrl.isEmpty()) {
                if (!background) finishLookup(status == 200 ? "El perfil no tiene una skin pública personalizada." :
                    "No se pudieron consultar las texturas del perfil.");
                return;
            }
            const auto capeUrl = textureUrl(textures.value("CAPE").toObject());
            if (!capeUrl.isEmpty()) {
                const auto capeStr = capeUrl.toString();
                const bool exists = std::any_of(m_capes.cbegin(), m_capes.cend(), [&capeStr](const QVariant &v) {
                    return v.toMap().value("url").toString() == capeStr;
                });
                if (!exists) {
                    m_capes.prepend(QVariantMap{
                        {"id", "mojang-public-cape"},
                        {"alias", "Capa oficial equipada"},
                        {"url", capeStr},
                        {"state", "ACTIVE"},
                        {"official", false},
                        {"source", "Mojang"}
                    });
                }
                if (m_selectedCapeId.isEmpty()) {
                    m_selectedCapeId = m_capes.first().toMap().value("id").toString();
                }
            }
            const bool slim = skin.value("metadata").toObject().value("model") == QStringLiteral("slim");
            get(skinUrl, [this, player, slim, capeUrl, background](const QByteArray &png, int) {
                const bool ownProfile = m_accountName.compare(player, Qt::CaseInsensitive) == 0;
                if (background && (m_busy || !ownProfile)) return;
                auto image = decodeTexture(png);
                if (image.isNull()) {
                    if (!background) finishLookup("No se pudo cargar la skin pública.");
                    return;
                }
                if (ownProfile) m_accountFace = headIcon(image);
                if (background && !m_savedSkinId.isEmpty()) {
                    emit previewChanged();
                    if (!m_selectedCapeId.isEmpty()) selectCape(m_selectedCapeId);
                    return;
                }
                showSkin(std::move(image), player, slim);
                if (!m_selectedCapeId.isEmpty()) {
                    selectCape(m_selectedCapeId);
                    finishLookup("Skin y capas del perfil cargadas.");
                    return;
                }
                if (capeUrl.isEmpty()) {
                    finishLookup("Skin pública cargada.");
                    return;
                }
                get(capeUrl, [this](const QByteArray &cape, int) {
                    m_cape = decodeTexture(cape, true);
                    emit previewChanged();
                    finishLookup(m_cape.isNull() ? "Skin cargada. La capa pública no se pudo leer." :
                        "Skin y capa públicas cargadas.");
                });
            });
        });
}

void WardrobeController::finishLookup(const QString &message) {
    m_busy = false;
    fail(message);
}

bool WardrobeController::importPng(const QUrl &file) {
    if (m_busy || !file.isLocalFile()) return false;
    auto image = readSkin(file.toLocalFile());
    if (image.isNull()) return fail("Selecciona un PNG de 64×64, 64×32 o 128×128 píxeles.");
    const auto name = QFileInfo(file.toLocalFile()).completeBaseName().left(64);
    const bool slim = image.size() != QSize(64, 32) && image.text("OpenClientModel") == "slim";
    const auto id = writeSkin(image, name, slim);
    if (id.isEmpty()) return false;
    showSkin(std::move(image), name, slim, id);
    reload();
    m_message = "Skin añadida a tu colección.";
    emit statusChanged();
    return true;
}

bool WardrobeController::exportPng(const QUrl &file) {
    if (m_busy || !hasSkin() || !file.isLocalFile()) return false;
    const QFileInfo target(file.toLocalFile());
    if (target.suffix().compare("png", Qt::CaseInsensitive) != 0)
        return fail("Guarda el archivo con la extensión .png.");
    const auto folder = directory(false);
    if (!folder.isEmpty() && target.dir().canonicalPath() == folder)
        return fail("Elige una carpeta distinta de la colección de skins.");
    QSaveFile output(target.absoluteFilePath());
    if (!output.open(QIODevice::WriteOnly) || !cleanTexture(m_skin).save(&output, "PNG") || !output.commit())
        return fail("No se pudo exportar la skin.");
    m_message = "PNG exportado.";
    emit statusChanged();
    return true;
}

QString WardrobeController::directory(bool create) const {
    const auto root = QFileInfo(m_root).canonicalFilePath();
    if (root.isEmpty()) return {};
    const auto path = QDir(root).filePath("skins");
    if (create && !QDir().mkpath(path)) return {};
    const QFileInfo directory(path);
    return directory.isDir() && !directory.isSymLink() && directory.canonicalFilePath() == path ? path : QString();
}

QString WardrobeController::savedPath(const QString &id) const {
    if (!savedId.match(id).hasMatch()) return {};
    const auto folder = directory(false);
    if (folder.isEmpty()) return {};
    const QFileInfo file(QDir(folder).filePath(id + ".png"));
    return file.isFile() && !file.isSymLink() && file.canonicalFilePath() == file.absoluteFilePath()
        ? file.absoluteFilePath() : QString();
}

void WardrobeController::reload() {
    m_collection.clear();
    const auto folder = directory(false);
    if (!folder.isEmpty()) {
        const auto files = QDir(folder).entryInfoList({"*.png"}, QDir::Files | QDir::NoSymLinks, QDir::Time);
        for (const auto &file : files) {
            const auto id = file.completeBaseName();
            if (!savedId.match(id).hasMatch() || file.canonicalFilePath() != file.absoluteFilePath()) continue;
            const auto image = readSkin(file.absoluteFilePath());
            if (image.isNull()) continue;
            m_collection.append(QVariantMap{{"id", id}, {"name", image.text("OpenClientName").left(64)}, {"icon", headIcon(image)},
                {"slim", image.text("OpenClientModel") == "slim"}});
            if (m_collection.size() == maxSaved) break;
        }
    }
    emit collectionChanged();
}

QString WardrobeController::writeSkin(const QImage &skin, const QString &name, bool slim, QString id) {
    const auto folder = directory(true);
    if (folder.isEmpty()) {
        fail("No se pudo abrir la carpeta de skins.");
        return {};
    }
    auto path = savedPath(id);
    if (path.isEmpty()) {
        if (QDir(folder).entryList({"*.png"}, QDir::Files | QDir::NoSymLinks).size() >= maxSaved) {
            fail("Tu colección ya tiene 200 skins.");
            return {};
        }
        id = QUuid::createUuid().toString(QUuid::WithoutBraces);
        path = QDir(folder).filePath(id + ".png");
    }
    auto image = cleanTexture(skin);
    image.setText("OpenClientName", name);
    image.setText("OpenClientModel", slim ? "slim" : "classic");
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly) || !image.save(&file, "PNG") || !file.commit()) {
        fail("No se pudo guardar la skin.");
        return {};
    }
    return id;
}

bool WardrobeController::saveSkin() {
    if (m_busy || m_skin.isNull()) return false;
    const auto id = writeSkin(m_skin, m_name, m_slim, m_savedSkinId);
    if (id.isEmpty()) return false;
    m_savedSkinId = id;
    reload();
    emit previewChanged();
    m_message = "Skin guardada.";
    emit statusChanged();
    return true;
}

bool WardrobeController::openSkin(const QString &id) {
    if (m_busy) return false;
    auto image = readSkin(savedPath(id));
    if (image.isNull()) return fail("No se pudo abrir la skin guardada.");
    const auto name = image.text("OpenClientName").left(64);
    const bool slim = image.text("OpenClientModel") == "slim";
    showSkin(std::move(image), name.isEmpty() ? "Skin guardada" : name, slim, id);
    m_message.clear();
    emit statusChanged();
    return true;
}

bool WardrobeController::removeSkin(const QString &id) {
    if (m_busy) return false;
    const auto path = savedPath(id);
    if (path.isEmpty() || !QFile::remove(path)) return fail("No se pudo quitar la skin guardada.");
    if (m_savedSkinId == id) {
        m_savedSkinId.clear();
        emit previewChanged();
    }
    reload();
    m_message = "Skin retirada de la colección.";
    emit statusChanged();
    return true;
}
