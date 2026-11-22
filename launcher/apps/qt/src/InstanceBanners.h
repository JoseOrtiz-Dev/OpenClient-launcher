#pragma once
#include <QFutureWatcher>
#include <QHash>
#include <QObject>
#include <QSet>
#include <QStringList>
#include <QUrl>
#include <QVariantMap>

class QNetworkAccessManager;
class QNetworkReply;

// Banner (imagen de cabecera) de cada instancia: personalizado > modpack (galería de Modrinth) > última captura.
// El procesado de imágenes y la lectura de disco van en hilos; el estado vive solo en el hilo de la interfaz.
class InstanceBanners : public QObject {
    Q_OBJECT
public:
    struct Processed {
        bool ok = false;
        QString key, sharp, soft, accent;
    };
    struct Outcome {
        QVariantMap info;
        QString key;
        bool wantModpack = false;
    };

    explicit InstanceBanners(const QString &root, QObject *parent = nullptr);
    ~InstanceBanners() override;

    int revision() const { return m_revision; }
    // Devuelve false si la instancia aún no se ha pedido. Nunca bloquea: el trabajo pendiente va en un hilo.
    bool lookup(const QString &id, QVariantMap &info);
    QString project(const QString &id) const { return m_entries.value(id).project; }
    QStringList ids() const { return m_entries.keys(); }
    // Registra o actualiza las entradas de la instancia y (re)calcula su banner en segundo plano.
    void request(const QString &id, const QString &screenshotsDir, const QString &project);
    // Devuelve el motivo del rechazo, o vacío si el archivo se copió como banner personalizado.
    QString setCustom(const QString &id, const QUrl &file);
    void clearCustom(const QString &id);
    void remove(const QString &id);
    bool waitIdle(int timeoutMs);

    static QVariantMap generated();
    static bool validProject(const QString &project);
    static QString customPath(const QString &root, const QString &id);
    static QString cacheDirectory(const QString &root);
    static QString latestScreenshot(const QString &directory);
    static Processed processImage(const QString &root, const QString &sourcePath);
    static Outcome resolve(const QString &root, const QString &id, const QString &screenshots, const QString &project);

signals:
    void changed();

private:
    struct Entry {
        QString screenshots, project, key;
        QVariantMap info = generated();
        bool pending = false;
        bool dirty = false;
        bool gone = false;
    };
    void launch(const QString &id);
    void finish(const QString &id, const Outcome &outcome);
    void sweep();
    void fetchProject(const QString &project);
    void fetchImage(const QString &project, const QUrl &url);
    void markNone(const QString &project);
    void refreshProject(const QString &project);
    QNetworkReply *download(const QUrl &url, qint64 limit);
    static void removeDerivatives(const QString &root, const QString &key);

    QString m_root;
    QHash<QString, Entry> m_entries;
    QSet<QString> m_stale;
    QSet<QString> m_tried;
    QList<QFutureWatcher<Outcome> *> m_watchers;
    QNetworkAccessManager *m_network = nullptr;
    int m_pending = 0;
    int m_revision = 0;
};
