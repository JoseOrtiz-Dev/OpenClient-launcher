#pragma once

#include <QJsonObject>
#include <QLocalSocket>
#include <QObject>
#include <QSettings>
#include <QTimer>

class LauncherController;

// Rich Presence de Discord por la tubería local discord-ipc-N: sin SDK ni conexiones a internet.
// Si Discord no está abierto, reintenta en segundo plano sin molestar.
class DiscordPresence final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool available READ available CONSTANT)
    Q_PROPERTY(bool enabled READ enabled WRITE setEnabled NOTIFY changed)
    Q_PROPERTY(bool showServer READ showServer WRITE setShowServer NOTIFY changed)
    Q_PROPERTY(bool connected READ connected NOTIFY changed)

public:
    DiscordPresence(LauncherController *launcher, const QString &settingsFile, const QString &clientId,
                    const QString &inviteUrl, QObject *parent = nullptr);
    static QString defaultClientId();

    bool available() const { return !m_clientId.isEmpty(); }
    bool enabled() const { return m_enabled; }
    bool showServer() const { return m_showServer; }
    bool connected() const { return m_ready; }
    void setEnabled(bool value);
    void setShowServer(bool value);
    Q_INVOKABLE void setPage(const QString &page);

    enum class World { Menu, Singleplayer, Server };
    struct Session {
        World world = World::Menu;
        QString host;
    };
    // Lee una línea del log de Minecraft; devuelve true si cambió dónde está el jugador.
    static bool readLogLine(const QString &line, Session &session);
    static QString describe(const Session &session, bool showServer);
    static QByteArray frame(quint32 opcode, const QJsonObject &payload);

signals:
    void changed();

private:
    void connectPipe();
    void readFrames();
    void send(quint32 opcode, const QJsonObject &payload);
    void schedule();
    void push();
    void followGame();
    void tailLog();
    QJsonObject activity() const;

    LauncherController *m_launcher;
    QSettings m_settings;
    QString m_clientId;
    QString m_inviteUrl;
    QLocalSocket m_socket;
    QTimer m_retry;
    QTimer m_debounce;
    QTimer m_logTimer;
    QByteArray m_buffer;
    QJsonObject m_sent;
    QString m_page = "menu";
    QString m_logPath;
    QByteArray m_logRest;
    qint64 m_logOffset = 0;
    qint64 m_openedAt = 0;
    qint64 m_playingSince = 0;
    Session m_session;
    int m_pipe = 0;
    bool m_enabled = true;
    bool m_showServer = true;
    bool m_connecting = false;
    bool m_ready = false;
    bool m_running = false;
};
