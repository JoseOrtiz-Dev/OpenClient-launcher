#pragma once

#include <QAbstractListModel>
#include <QCollator>
#include <QHash>
#include <QSortFilterProxyModel>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>
#include <QVector>

// Filas de contenido de una instancia (mods, texturas y shaders fusionados). Cada fila es una de las
// que devuelven `content_list` y `content_resolve`; se identifican por `kind` + `key`, de modo que
// una actualización cambia solo la fila afectada (dataChanged) sin reiniciar el scroll de la vista.
class ContentListModel final : public QAbstractListModel {
    Q_OBJECT
public:
    enum Role {
        KeyRole = Qt::UserRole + 1, FileNameRole, FileEnabledRole, PresentRole, ManagedRole, DependencyRole,
        KindRole, TitleRole, AuthorRole, VersionRole, DescriptionRole, ProjectIdRole, VersionIdRole,
        IconRole, SourceRole, FileSizeRole, ModifiedRole, FlagsRole, UpdateVersionRole, SearchRole
    };

    struct Item {
        QString id;  // kind + "|" + key: única entre categorías
        QString key, fileName, kind, title, author, version, description, projectId, versionId, icon, source, update;
        QString search, titleKey;  // texto plegado (sin acentos ni mayúsculas) para filtrar y ordenar
        bool enabled = true, present = true, managed = false, dependency = false;
        qint64 size = 0, modified = 0;
        QStringList flags;
        bool operator==(const Item &other) const = default;
    };

    explicit ContentListModel(QObject *parent = nullptr);
    // Carpeta de datos del launcher: de ahí salen los iconos «cache:<sha1>» (cache/content-icons/<sha1>.png).
    void setRoot(const QString &root);
    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    const Item &itemAt(int row) const { return m_items.at(row); }
    // replace = true: la lista completa de ese tipo (se quitan las filas que ya no están);
    // replace = false: solo se actualizan o añaden las recibidas (respuesta de content_resolve).
    void applyItems(const QString &kind, const QVariantList &items, bool replace);
    void clear();
    int countOf(const QString &kind) const;  // kind vacío = todas
    static QString fold(const QString &text);

signals:
    void countsChanged();

private:
    Item parse(const QVariantMap &map, const QString &fallbackKind) const;
    QString iconUrl(const QString &icon) const;
    static QList<int> changedRoles(const Item &before, const Item &after);
    void rebuildIndex();
    QVector<Item> m_items;
    QHash<QString, int> m_index;
    QString m_iconDirectory;
};

// Filtro por texto (título, autor y archivo), por tipo y orden de la lista de contenido.
class ContentFilterModel final : public QSortFilterProxyModel {
    Q_OBJECT
    Q_PROPERTY(QString query READ query WRITE setQuery NOTIFY queryChanged)
    // "" = todos | "mod" | "resourcepack" | "shader"
    Q_PROPERTY(QString kind READ kind WRITE setKind NOTIFY kindChanged)
    // "name" (A-Z) | "nameDesc" (Z-A) | "recent" (modified, más nuevos primero) | "enabled" (activados primero)
    Q_PROPERTY(QString order READ order WRITE setOrder NOTIFY orderChanged)
    Q_PROPERTY(int visibleCount READ visibleCount NOTIFY visibleCountChanged)
    // {all, mod, resourcepack, shader}: filas totales, sin tener en cuenta el filtro.
    Q_PROPERTY(QVariantMap counts READ counts NOTIFY countsChanged)
public:
    explicit ContentFilterModel(ContentListModel *source, QObject *parent = nullptr);
    QString query() const { return m_query; }
    QString kind() const { return m_kind; }
    QString order() const { return m_order; }
    int visibleCount() const { return rowCount(); }
    QVariantMap counts() const;
    void setQuery(const QString &value);
    void setKind(const QString &value);
    void setOrder(const QString &value);

signals:
    void queryChanged();
    void kindChanged();
    void orderChanged();
    void visibleCountChanged();
    void countsChanged();

protected:
    bool filterAcceptsRow(int sourceRow, const QModelIndex &sourceParent) const override;
    bool lessThan(const QModelIndex &left, const QModelIndex &right) const override;

private:
    void notifyVisible();
    ContentListModel *m_source;
    QString m_query;
    QStringList m_tokens;
    QString m_kind;
    QString m_order = QStringLiteral("name");
    int m_visible = 0;
    QCollator m_collator;
};
