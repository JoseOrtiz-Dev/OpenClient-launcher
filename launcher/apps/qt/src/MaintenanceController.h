#pragma once

#include <QFutureWatcher>
#include <QNetworkAccessManager>
#include <QObject>
#include <QPointer>
#include <QSettings>
#include <QUrl>
#include <QVariantMap>

class LauncherController;
class QNetworkReply;

// Carpeta de datos, informe de errores y actualizaciones (al arrancar si está activado, o con el botón).
class MaintenanceController final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString dataDirectory READ dataDirectory CONSTANT)
    Q_PROPERTY(QString defaultDirectory READ defaultDirectory CONSTANT)
    Q_PROPERTY(QString pendingDirectory READ pendingDirectory NOTIFY locationChanged)
    Q_PROPERTY(bool moving READ moving NOTIFY locationChanged)
    Q_PROPERTY(bool restartNeeded READ restartNeeded NOTIFY locationChanged)
    Q_PROPERTY(QString locationMessage READ locationMessage NOTIFY locationChanged)
    Q_PROPERTY(bool locationFailed READ locationFailed NOTIFY locationChanged)
    Q_PROPERTY(QString reportMessage READ reportMessage NOTIFY reportChanged)
    Q_PROPERTY(bool reportFailed READ reportFailed NOTIFY reportChanged)
    Q_PROPERTY(QString updateChannel READ updateChannel WRITE setUpdateChannel NOTIFY updateChanged)
    Q_PROPERTY(bool updatesConfigured READ updatesConfigured CONSTANT)
    Q_PROPERTY(bool updateChecking READ updateChecking NOTIFY updateChanged)
    Q_PROPERTY(bool updateAvailable READ updateAvailable NOTIFY updateChanged)
    Q_PROPERTY(QString updateMessage READ updateMessage NOTIFY updateChanged)
    Q_PROPERTY(bool updateInstallable READ updateInstallable NOTIFY updateChanged)
    Q_PROPERTY(bool updateDownloading READ updateDownloading NOTIFY updateChanged)
    Q_PROPERTY(int updatePercent READ updatePercent NOTIFY updateChanged)
    Q_PROPERTY(bool autoUpdate READ autoUpdate WRITE setAutoUpdate NOTIFY updateChanged)
    // "" sin actividad, "downloading", "installing" o "failed".
    Q_PROPERTY(QString updateStage READ updateStage NOTIFY updateChanged)
    Q_PROPERTY(QString updateVersion READ updateVersion NOTIFY updateChanged)
    Q_PROPERTY(double updateSizeMb READ updateSizeMb NOTIFY updateChanged)
    Q_PROPERTY(QString currentVersion READ currentVersion CONSTANT)
    Q_PROPERTY(bool portable READ portable CONSTANT)

public:
    MaintenanceController(LauncherController *launcher, const QString &dataDirectory, const QString &bootstrapFile,
                          const QString &manifestUrl, QObject *parent = nullptr);

    // Modo portable: si hay un `portable.txt` junto al ejecutable, los datos van a `<carpeta>/data`.
    // Devuelve esa carpeta (creada y escribible) o vacío si no hay modo portable.
    static QString portableDirectory(const QString &appDirectory);
    static QString defaultDataDirectory();
    static QString bootstrapFile();
    // Carpeta elegida por el usuario si sigue siendo válida; si no, la predeterminada.
    static QString resolveDataDirectory();
    static bool writeLocation(const QString &bootstrapFile, const QString &directory);
    static QString validateTarget(const QString &current, const QString &target);
    static QString copyTree(const QString &source, const QString &target);
    static QString redact(const QString &text, const QString &home);
    static int compareVersions(const QString &left, const QString &right);
    static QVariantMap parseManifest(const QByteArray &raw, const QString &channel, const QString &current);
    // Carpeta de una instalación hecha con el instalador (la que contiene bin/ y el desinstalador), o vacío.
    static QString installRoot(const QString &appDirectory);
    static QStringList updateArguments(qint64 pid, const QString &installDirectory);
    QString buildReport() const;

    QString dataDirectory() const { return m_dataDirectory; }
    bool portable() const;
    QString defaultDirectory() const { return defaultDataDirectory(); }
    QString pendingDirectory() const { return m_pending; }
    bool moving() const { return m_moving; }
    bool restartNeeded() const { return !m_pending.isEmpty(); }
    QString locationMessage() const { return m_locationMessage; }
    bool locationFailed() const { return m_locationFailed; }
    QString reportMessage() const { return m_reportMessage; }
    bool reportFailed() const { return m_reportFailed; }
    QString updateChannel() const { return m_channel; }
    bool updatesConfigured() const { return !m_manifestUrl.isEmpty(); }
    bool updateChecking() const { return m_checking; }
    bool updateAvailable() const { return !m_updateUrl.isEmpty(); }
    QString updateMessage() const { return m_updateMessage; }
    // Instalable desde el launcher solo si el manifiesto trae SHA-256 y tamaño del instalador.
    bool updateInstallable() const { return !m_updateUrl.isEmpty() && !m_updateSha256.isEmpty() && m_updateSize > 0; }
    bool updateDownloading() const { return m_updateReply != nullptr; }
    int updatePercent() const { return m_updatePercent; }
    bool autoUpdate() const { return m_autoUpdate; }
    QString updateStage() const { return m_stage; }
    QString updateVersion() const { return m_updateVersion; }
    double updateSizeMb() const { return m_updateSize / (1024.0 * 1024.0); }
    QString currentVersion() const;
    void setUpdateChannel(const QString &value);
    void setAutoUpdate(bool value);

    Q_INVOKABLE void moveDataDirectory(const QUrl &folder);
    Q_INVOKABLE void useDefaultDirectory();
    Q_INVOKABLE void restart();
    Q_INVOKABLE void exportReport(const QUrl &file);
    Q_INVOKABLE void checkForUpdates();
    Q_INVOKABLE void openUpdate();
    Q_INVOKABLE void installUpdate();
    Q_INVOKABLE void cancelUpdateDownload();
    // Al arrancar: busca y, si hay versión nueva verificable, la descarga e instala sin preguntar.
    Q_INVOKABLE void autoCheck();
    Q_INVOKABLE void dismissUpdate();

signals:
    void locationChanged();
    void reportChanged();
    void updateChanged();

private:
    void startMove(const QString &target);
    void fail(QString &message, bool &flag, const QString &text);

    QPointer<LauncherController> m_launcher;
    QSettings m_settings;
    QString m_bootstrapFile;
    QString m_manifestUrl;
    QString m_dataDirectory;
    QString m_pending;
    QString m_locationMessage;
    bool m_locationFailed = false;
    bool m_moving = false;
    QFutureWatcher<QString> m_move;
    QString m_moveTarget;
    QString m_reportMessage;
    bool m_reportFailed = false;
    QString m_channel = "stable";
    bool m_checking = false;
    QString m_updateMessage;
    QString m_updateUrl;
    QString m_updateSha256;
    qint64 m_updateSize = 0;
    QNetworkReply *m_updateReply = nullptr;
    int m_updatePercent = -1;
    bool m_autoUpdate = true;
    bool m_autoRun = false;
    QString m_stage;
    QString m_updateVersion;
    QNetworkAccessManager m_network;
};
