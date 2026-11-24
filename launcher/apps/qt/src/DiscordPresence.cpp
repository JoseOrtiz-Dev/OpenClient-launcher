#include "DiscordPresence.h"
#include "LauncherController.h"

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QHostAddress>
#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QUuid>
#include <QtEndian>

namespace {
// ID de la aplicación de Discord (discord.com/developers → Applications → OAuth2 → Client ID).
// No es un secreto. Vacío desactiva la función; OPENCLIENT_DISCORD_CLIENT_ID lo sustituye para pruebas.
constexpr auto BuiltInClientId = "1566250662910631976";

constexpr quint32 Handshake = 0, Frame = 1, Close = 2, Ping = 3, Pong = 4;

const QHash<QString, QString> pageStates = {
    {"menu", "En el menú principal"},
    {"official", "Viendo el modpack oficial"},
    {"instances", "Eligiendo instancia"},
    {"downloads", "Explorando mods y modpacks"},
    {"wardrobe", "Cambiando de skin"},
    {"settings", "En ajustes"},
    {"credits", "Viendo los créditos"},
};

// Discord exige entre 2 y 128 caracteres en details y state.
QString fit(const QString &text) {
    auto value = text.simplified();
    if (value.size() > 128) value = value.left(127) + QChar(0x2026);
    return value.size() < 2 ? QStringLiteral("OpenClient") : value;
}

bool privateHost(const QString &host) {
    if (host == "localhost" || host.endsWith(".local") || host.endsWith(".lan")) return true;
    const QHostAddress address(host);
    return !address.isNull() && (address.isLoopback() || address.isPrivateUse() || address.isLinkLocal());
}
}

QString DiscordPresence::defaultClientId() {
    const auto value = qEnvironmentVariable("OPENCLIENT_DISCORD_CLIENT_ID", QString::fromLatin1(BuiltInClientId)).trimmed();
    static const QRegularExpression snowflake(QStringLiteral("^\\d{15,21}$"));
    return snowflake.match(value).hasMatch() ? value : QString();
}

DiscordPresence::DiscordPresence(LauncherController *launcher, const QString &settingsFile, const QString &clientId,
                                 const QString &inviteUrl, QObject *parent)
    : QObject(parent), m_launcher(launcher), m_settings(settingsFile, QSettings::IniFormat), m_clientId(clientId),
      m_inviteUrl(inviteUrl), m_openedAt(QDateTime::currentSecsSinceEpoch()) {
    m_enabled = m_settings.value("discord/enabled", true).toBool();
    m_showServer = m_settings.value("discord/show_server", true).toBool();

    m_retry.setSingleShot(true);
    m_retry.setInterval(20000);
    connect(&m_retry, &QTimer::timeout, this, &DiscordPresence::connectPipe);
    // Discord limita las actualizaciones; se agrupan los cambios seguidos en uno solo.
    m_debounce.setSingleShot(true);
    m_debounce.setInterval(1200);
    connect(&m_debounce, &QTimer::timeout, this, &DiscordPresence::push);
    m_logTimer.setInterval(3000);
    connect(&m_logTimer, &QTimer::timeout, this, &DiscordPresence::tailLog);

    connect(&m_socket, &QLocalSocket::connected, this, [this] {
        m_connecting = false;
        send(Handshake, {{"v", 1}, {"client_id", m_clientId}});
    });
    connect(&m_socket, &QLocalSocket::readyRead, this, &DiscordPresence::readFrames);
    connect(&m_socket, &QLocalSocket::errorOccurred, this, [this] {
        if (!m_connecting) return;
        m_connecting = false;
        // Discord abre la primera tubería libre entre discord-ipc-0 y discord-ipc-9.
        QTimer::singleShot(0, this, [this] {
            m_socket.abort();
            if (++m_pipe < 10) connectPipe();
            else {
                m_pipe = 0;
                if (m_enabled) m_retry.start();
            }
        });
    });
    connect(&m_socket, &QLocalSocket::disconnected, this, [this] {
        const bool wasReady = m_ready;
        m_ready = false;
        m_buffer.clear();
        m_sent = {};
        m_pipe = 0;
        if (m_enabled) m_retry.start();
        if (wasReady) emit changed();
    });

    if (m_launcher) connect(m_launcher, &LauncherController::gameChanged, this, &DiscordPresence::followGame);
    connectPipe();
}

void DiscordPresence::setEnabled(bool value) {
    if (m_enabled == value) return;
    m_enabled = value;
    m_settings.setValue("discord/enabled", value);
    if (value) connectPipe();
    else {
        m_retry.stop();
        m_debounce.stop();
        m_connecting = false;
        // Al cerrar la tubería, Discord retira la actividad.
        m_socket.abort();
    }
    emit changed();
}

void DiscordPresence::setShowServer(bool value) {
    if (m_showServer == value) return;
    m_showServer = value;
    m_settings.setValue("discord/show_server", value);
    emit changed();
    schedule();
}

void DiscordPresence::setPage(const QString &page) {
    if (m_page == page) return;
    m_page = page;
    schedule();
}

void DiscordPresence::connectPipe() {
    if (!m_enabled || !available() || m_socket.state() != QLocalSocket::UnconnectedState) return;
    m_connecting = true;
    m_socket.connectToServer(QStringLiteral("discord-ipc-%1").arg(m_pipe));
}

QByteArray DiscordPresence::frame(quint32 opcode, const QJsonObject &payload) {
    const auto body = QJsonDocument(payload).toJson(QJsonDocument::Compact);
    QByteArray header(8, Qt::Uninitialized);
    qToLittleEndian<quint32>(opcode, header.data());
    qToLittleEndian<quint32>(quint32(body.size()), header.data() + 4);
    return header + body;
}

void DiscordPresence::send(quint32 opcode, const QJsonObject &payload) {
    if (m_socket.state() == QLocalSocket::ConnectedState) m_socket.write(frame(opcode, payload));
}

void DiscordPresence::readFrames() {
    m_buffer += m_socket.readAll();
    while (m_buffer.size() >= 8) {
        const auto opcode = qFromLittleEndian<quint32>(m_buffer.constData());
        const auto length = qFromLittleEndian<quint32>(m_buffer.constData() + 4);
        if (length > 65536) {
            m_socket.abort();
            return;
        }
        if (m_buffer.size() < 8 + qsizetype(length)) return;
        const auto body = QJsonDocument::fromJson(m_buffer.mid(8, length)).object();
        m_buffer.remove(0, 8 + length);
        if (opcode == Ping) send(Pong, body);
        else if (opcode == Close) {
            // Suele indicar un Client ID inválido; se reintenta más tarde.
            m_socket.disconnectFromServer();
            return;
        } else if (opcode == Frame && body.value("evt").toString() == "READY") {
            m_ready = true;
            m_sent = {};
            emit changed();
            push();
        }
    }
}

void DiscordPresence::schedule() {
    if (m_ready) m_debounce.start();
}

void DiscordPresence::push() {
    if (!m_ready) return;
    const auto current = activity();
    if (current == m_sent) return;
    m_sent = current;
    send(Frame, {{"cmd", "SET_ACTIVITY"},
                 {"args", QJsonObject{{"pid", qint64(QCoreApplication::applicationPid())}, {"activity", current}}},
                 {"nonce", QUuid::createUuid().toString(QUuid::WithoutBraces)}});
}

void DiscordPresence::followGame() {
    const auto id = m_launcher->gameRunning() ? m_launcher->playingInstanceId() : QString();
    if (!id.isEmpty() && id != m_playingInstanceId) {
        m_playingSince = QDateTime::currentSecsSinceEpoch();
        m_session = {};
        m_logOffset = 0;
        m_logRest.clear();
        m_logPath = QDir(m_launcher->dataDirectory())
            .filePath(QStringLiteral("instances/%1/logs/openclient-latest.log").arg(id));
        m_logTimer.start();
    } else if (id.isEmpty() && !m_playingInstanceId.isEmpty()) {
        m_logTimer.stop();
        m_session = {};
    }
    m_playingInstanceId = id;
    schedule();
}

void DiscordPresence::tailLog() {
    QFile file(m_logPath);
    if (!file.open(QIODevice::ReadOnly)) return;
    if (file.size() < m_logOffset) {
        m_logOffset = 0;
        m_logRest.clear();
    }
    if (!file.seek(m_logOffset)) return;
    const auto chunk = file.read(512 * 1024);
    m_logOffset += chunk.size();
    m_logRest += chunk;
    const auto end = m_logRest.lastIndexOf('\n');
    if (end < 0) {
        if (m_logRest.size() > 64 * 1024) m_logRest.clear();
        return;
    }
    bool moved = false;
    for (const auto &line : m_logRest.left(end).split('\n'))
        moved |= readLogLine(QString::fromUtf8(line), m_session);
    m_logRest.remove(0, end + 1);
    if (moved) schedule();
}

bool DiscordPresence::readLogLine(const QString &line, Session &session) {
    static const QRegularExpression connecting(QStringLiteral("Connecting to ([^\\s,]+), ?\\d+"));
    static const QRegularExpression safeHost(QStringLiteral("^[a-z0-9._:-]{1,80}$"));
    Session next = session;
    if (const auto match = connecting.match(line); match.hasMatch()) {
        const auto host = match.captured(1).toLower();
        next = {World::Server, safeHost.match(host).hasMatch() ? host : QString()};
    } else if (line.contains("Starting integrated minecraft server")) {
        next = {World::Singleplayer, {}};
    } else if (line.contains("Couldn't connect to server")
               || (session.world == World::Singleplayer && line.contains("Stopping server"))) {
        next = {};
    }
    if (next.world == session.world && next.host == session.host) return false;
    session = next;
    return true;
}

QString DiscordPresence::describe(const Session &session, bool showServer) {
    switch (session.world) {
    case World::Singleplayer: return QStringLiteral("Un jugador");
    case World::Server:
        if (session.host.isEmpty() || !showServer) return QStringLiteral("En multijugador");
        if (privateHost(session.host)) return QStringLiteral("En una partida LAN");
        return QStringLiteral("En ") + session.host;
    case World::Menu: break;
    }
    return QStringLiteral("En el menú del juego");
}

QJsonObject DiscordPresence::activity() const {
    QJsonObject assets{{"large_image", "logo"}, {"large_text", "OpenClient " + QCoreApplication::applicationVersion()}};
    QJsonObject result;
    const auto id = m_launcher ? m_launcher->playingInstanceId() : QString();
    if (id.isEmpty()) {
        result["details"] = QStringLiteral("En el launcher");
        result["state"] = fit(pageStates.value(m_page, pageStates.value("menu")));
        result["timestamps"] = QJsonObject{{"start", m_openedAt}};
    } else {
        QVariantMap entry;
        for (const auto &item : m_launcher->instances())
            if (item.toMap().value("id").toString() == id) entry = item.toMap();
        const auto category = entry.value("category").toString();
        const auto loader = entry.value("loaderLabel").toString();
        auto edition = "Minecraft " + entry.value("mcVersion").toString();
        if (category != "vanilla" && !loader.isEmpty()) edition += " · " + loader;
        if (!m_playingInstanceId.isEmpty()) {
            result["details"] = fit(edition);
            result["state"] = fit(describe(m_session, m_showServer));
            result["timestamps"] = QJsonObject{{"start", m_playingSince}};
        } else {
            result["details"] = fit("Preparando " + edition);
            result["state"] = QStringLiteral("Descargando archivos");
        }
        assets["large_text"] = fit(entry.value("name").toString());
        static const QStringList loaders = {"vanilla", "fabric", "quilt", "forge", "neoforge"};
        if (loaders.contains(category)) {
            assets["small_image"] = category;
            assets["small_text"] = fit(loader.isEmpty() ? category : loader);
        }
    }
    result["assets"] = assets;
    if (m_inviteUrl.startsWith("https://"))
        result["buttons"] = QJsonArray{QJsonObject{{"label", "Únete a la comunidad"}, {"url", m_inviteUrl}}};
    return result;
}
