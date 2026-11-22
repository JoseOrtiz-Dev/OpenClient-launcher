#pragma once

#include <QImage>
#include <QNetworkAccessManager>
#include <QObject>
#include <QUrl>
#include <QVariantList>
#include <QVariantMap>
#include <functional>

class WardrobeController final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QImage skin READ skin NOTIFY previewChanged)
    Q_PROPERTY(QImage cape READ cape NOTIFY previewChanged)
    Q_PROPERTY(QString faceIcon READ faceIcon NOTIFY previewChanged)
    Q_PROPERTY(QString accountFace READ accountFace NOTIFY previewChanged)
    Q_PROPERTY(QVariantList capes READ capes NOTIFY previewChanged)
    Q_PROPERTY(QString selectedCapeId READ selectedCapeId NOTIFY previewChanged)
    Q_PROPERTY(QString savedSkinId READ savedSkinId NOTIFY previewChanged)
    Q_PROPERTY(QString name READ name NOTIFY previewChanged)
    Q_PROPERTY(bool slim READ slim WRITE setSlim NOTIFY previewChanged)
    Q_PROPERTY(bool legacy READ legacy NOTIFY previewChanged)
    Q_PROPERTY(bool hasSkin READ hasSkin NOTIFY previewChanged)
    Q_PROPERTY(bool hasCape READ hasCape NOTIFY previewChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY statusChanged)
    Q_PROPERTY(QString message READ message NOTIFY statusChanged)
    Q_PROPERTY(QVariantList collection READ collection NOTIFY collectionChanged)

public:
    explicit WardrobeController(const QString &root);
    QImage skin() const { return m_skin; }
    QImage cape() const { return m_cape; }
    QString faceIcon() const { return m_faceIcon; }
    QString accountFace() const { return m_accountFace.isEmpty() ? m_faceIcon : m_accountFace; }
    QVariantList capes() const { return m_capes; }
    QString selectedCapeId() const { return m_selectedCapeId; }
    QString savedSkinId() const { return m_savedSkinId; }
    QString name() const { return m_name; }
    bool slim() const { return m_slim; }
    bool legacy() const { return m_skin.size() == QSize(64, 32); }
    bool hasSkin() const { return !m_skin.isNull(); }
    bool hasCape() const { return !m_cape.isNull(); }
    bool busy() const { return m_busy; }
    QString message() const { return m_message; }
    QVariantList collection() const { return m_collection; }
    void setSlim(bool value);
    Q_INVOKABLE void lookup(const QString &player);
    Q_INVOKABLE void syncAccount(const QString &username, const QVariantMap &microsoftProfile = {});
    Q_INVOKABLE void selectCape(const QString &capeId);
    Q_INVOKABLE bool importPng(const QUrl &file);
    Q_INVOKABLE bool exportPng(const QUrl &file);
    Q_INVOKABLE bool saveSkin();
    Q_INVOKABLE bool openSkin(const QString &id);
    Q_INVOKABLE bool removeSkin(const QString &id);

signals:
    void previewChanged();
    void statusChanged();
    void collectionChanged();

private:
    using Response = std::function<void(QByteArray, int)>;
    void get(const QUrl &url, Response response);
    void loadCapeIcons();
    void loadProfile(const QString &player, const QString &id, bool background = false);
    void finishLookup(const QString &message);
    bool fail(const QString &message);
    void showSkin(QImage image, const QString &name, bool slim, const QString &collectionId = {});
    void reload();
    QString writeSkin(const QImage &skin, const QString &name, bool slim, QString id = {});
    QString directory(bool create) const;
    QString savedPath(const QString &id) const;

    QString m_root;
    QNetworkAccessManager m_network;
    QImage m_skin;
    QImage m_cape;
    QString m_faceIcon;
    QString m_accountFace;
    QString m_accountName;
    QVariantList m_accountCapes;
    QVariantList m_capes;
    QString m_selectedCapeId;
    QString m_savedSkinId;
    QString m_name;
    QString m_message;
    QVariantList m_collection;
    bool m_slim = false;
    bool m_busy = false;
};
