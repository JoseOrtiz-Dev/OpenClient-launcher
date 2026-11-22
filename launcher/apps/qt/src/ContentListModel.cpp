#include "ContentListModel.h"
#include <QDir>
#include <QFileInfo>
#include <QRegularExpression>
#include <QSet>
#include <QUrl>

namespace {
const QStringList kinds = {QStringLiteral("mod"), QStringLiteral("resourcepack"), QStringLiteral("shader")};
}

ContentListModel::ContentListModel(QObject *parent) : QAbstractListModel(parent) {}

void ContentListModel::setRoot(const QString &root) {
    m_iconDirectory = root.isEmpty() ? QString() : QDir::cleanPath(QDir(root).filePath("cache/content-icons"));
}

int ContentListModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : static_cast<int>(m_items.size());
}

QHash<int, QByteArray> ContentListModel::roleNames() const {
    return {{KeyRole, "entryKey"}, {FileNameRole, "fileName"}, {FileEnabledRole, "fileEnabled"}, {PresentRole, "present"},
            {ManagedRole, "managed"}, {DependencyRole, "dependency"}, {KindRole, "kind"}, {TitleRole, "title"},
            {AuthorRole, "author"}, {VersionRole, "version"}, {DescriptionRole, "description"},
            {ProjectIdRole, "projectId"}, {VersionIdRole, "versionId"}, {IconRole, "iconUrl"}, {SourceRole, "source"},
            {FileSizeRole, "fileSize"}, {ModifiedRole, "modified"}, {FlagsRole, "flags"},
            {UpdateVersionRole, "updateVersion"}, {SearchRole, "searchText"}};
}

QVariant ContentListModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_items.size()) return {};
    const auto &item = m_items.at(index.row());
    switch (role) {
    case KeyRole: return item.key;
    case FileNameRole: return item.fileName;
    case FileEnabledRole: return item.enabled;
    case PresentRole: return item.present;
    case ManagedRole: return item.managed;
    case DependencyRole: return item.dependency;
    case KindRole: return item.kind;
    case TitleRole: return item.title;
    case AuthorRole: return item.author;
    case VersionRole: return item.version;
    case DescriptionRole: return item.description;
    case ProjectIdRole: return item.projectId;
    case VersionIdRole: return item.versionId;
    case IconRole: return item.icon;
    case SourceRole: return item.source;
    case FileSizeRole: return item.size;
    case ModifiedRole: return item.modified;
    case FlagsRole: return item.flags;
    case UpdateVersionRole: return item.update;
    case SearchRole: return item.search;
    default: return {};
    }
}

QString ContentListModel::fold(const QString &text) {
    const QString decomposed = text.normalized(QString::NormalizationForm_D);
    QString out;
    out.reserve(decomposed.size());
    for (const QChar c : decomposed)
        if (c.category() != QChar::Mark_NonSpacing) out.append(c);
    return out.toCaseFolded();
}

// «cache:<sha1>» se traduce a la ruta local del PNG; solo https://cdn.modrinth.com/ pasa tal cual.
// Cualquier otra cosa se descarta: la interfaz nunca maneja rutas ni destinos arbitrarios.
QString ContentListModel::iconUrl(const QString &icon) const {
    if (icon.startsWith(QLatin1String("https://cdn.modrinth.com/"))) return icon;
    static const QRegularExpression cached(QStringLiteral("^cache:([0-9a-f]{40})$"));
    const auto match = cached.match(icon);
    if (!match.hasMatch() || m_iconDirectory.isEmpty()) return {};
    const auto path = m_iconDirectory + QLatin1Char('/') + match.captured(1) + QStringLiteral(".png");
    return QFileInfo::exists(path) ? QUrl::fromLocalFile(path).toString() : QString();
}

ContentListModel::Item ContentListModel::parse(const QVariantMap &map, const QString &fallbackKind) const {
    Item item;
    item.key = map.value("key").toString();
    item.kind = kinds.contains(map.value("kind").toString()) ? map.value("kind").toString() : fallbackKind;
    item.id = item.kind + QLatin1Char('|') + item.key;
    item.fileName = map.value("fileName").toString();
    item.title = map.value("title").toString();
    item.author = map.value("author").toString();
    item.version = map.value("version").toString();
    item.description = map.value("description").toString();
    item.projectId = map.value("projectId").toString();
    item.versionId = map.value("versionId").toString();
    item.icon = iconUrl(map.value("icon").toString());
    item.source = map.value("source").toString();
    item.enabled = map.value("enabled", true).toBool();
    item.present = map.value("present", true).toBool();
    item.managed = map.value("managed").toBool();
    item.dependency = map.value("dependency").toBool();
    item.size = map.value("size").toLongLong();
    item.modified = map.value("modified").toLongLong();
    item.flags = map.value("flags").toStringList();
    // updateAvailable es {versionId, version}; mientras sea null no hay etiqueta.
    item.update = map.value("updateAvailable").toMap().value("version").toString();
    const auto title = item.title.isEmpty() ? item.fileName : item.title;
    item.search = fold(title + QLatin1Char(' ') + item.author + QLatin1Char(' ') + item.fileName);
    item.titleKey = fold(title);
    return item;
}

QList<int> ContentListModel::changedRoles(const Item &a, const Item &b) {
    QList<int> roles;
    const auto note = [&roles](bool same, int role) { if (!same) roles.append(role); };
    note(a.fileName == b.fileName, FileNameRole);
    note(a.enabled == b.enabled, FileEnabledRole);
    note(a.present == b.present, PresentRole);
    note(a.managed == b.managed, ManagedRole);
    note(a.dependency == b.dependency, DependencyRole);
    note(a.title == b.title, TitleRole);
    note(a.author == b.author, AuthorRole);
    note(a.version == b.version, VersionRole);
    note(a.description == b.description, DescriptionRole);
    note(a.projectId == b.projectId, ProjectIdRole);
    note(a.versionId == b.versionId, VersionIdRole);
    note(a.icon == b.icon, IconRole);
    note(a.source == b.source, SourceRole);
    note(a.size == b.size, FileSizeRole);
    note(a.modified == b.modified, ModifiedRole);
    note(a.flags == b.flags, FlagsRole);
    note(a.update == b.update, UpdateVersionRole);
    note(a.search == b.search, SearchRole);
    return roles;
}

void ContentListModel::rebuildIndex() {
    m_index.clear();
    m_index.reserve(m_items.size());
    for (int row = 0; row < m_items.size(); ++row) m_index.insert(m_items.at(row).id, row);
}

void ContentListModel::applyItems(const QString &kind, const QVariantList &items, bool replace) {
    QVector<Item> incoming;
    QSet<QString> seen;
    incoming.reserve(items.size());
    for (const auto &entry : items) {
        auto item = parse(entry.toMap(), kind);
        if (item.key.isEmpty() || item.kind != kind || seen.contains(item.id)) continue;
        seen.insert(item.id);
        incoming.append(std::move(item));
    }
    bool countsDirty = false;
    if (replace) {
        bool removed = false;
        for (int row = static_cast<int>(m_items.size()) - 1; row >= 0; --row) {
            if (m_items.at(row).kind != kind || seen.contains(m_items.at(row).id)) continue;
            beginRemoveRows({}, row, row);
            m_items.removeAt(row);
            endRemoveRows();
            removed = true;
        }
        if (removed) {
            rebuildIndex();
            countsDirty = true;
        }
    }
    QVector<Item> added;
    for (auto &item : incoming) {
        const auto found = m_index.constFind(item.id);
        if (found == m_index.constEnd()) {
            added.append(std::move(item));
            continue;
        }
        const int row = found.value();
        if (m_items.at(row) == item) continue;
        const auto roles = changedRoles(m_items.at(row), item);
        m_items[row] = std::move(item);
        emit dataChanged(index(row), index(row), roles);
    }
    if (!added.isEmpty()) {
        const int first = static_cast<int>(m_items.size());
        beginInsertRows({}, first, first + static_cast<int>(added.size()) - 1);
        for (auto &item : added) {
            m_index.insert(item.id, static_cast<int>(m_items.size()));
            m_items.append(std::move(item));
        }
        endInsertRows();
        countsDirty = true;
    }
    if (countsDirty) emit countsChanged();
}

void ContentListModel::clear() {
    if (m_items.isEmpty()) return;
    beginResetModel();
    m_items.clear();
    m_index.clear();
    endResetModel();
    emit countsChanged();
}

int ContentListModel::countOf(const QString &kind) const {
    if (kind.isEmpty()) return static_cast<int>(m_items.size());
    int count = 0;
    for (const auto &item : m_items)
        if (item.kind == kind) ++count;
    return count;
}

// ---------------------------------------------------------------- filtro y orden

ContentFilterModel::ContentFilterModel(ContentListModel *source, QObject *parent)
    : QSortFilterProxyModel(parent), m_source(source) {
    m_collator.setNumericMode(true);
    m_collator.setCaseSensitivity(Qt::CaseInsensitive);
    setSourceModel(source);
    setDynamicSortFilter(true);
    sort(0, Qt::AscendingOrder);
    connect(source, &ContentListModel::countsChanged, this, &ContentFilterModel::countsChanged);
    connect(this, &QAbstractItemModel::rowsInserted, this, [this] { notifyVisible(); });
    connect(this, &QAbstractItemModel::rowsRemoved, this, [this] { notifyVisible(); });
    connect(this, &QAbstractItemModel::modelReset, this, [this] { notifyVisible(); });
    connect(this, &QAbstractItemModel::layoutChanged, this, [this] { notifyVisible(); });
}

void ContentFilterModel::notifyVisible() {
    const int now = rowCount();
    if (now == m_visible) return;
    m_visible = now;
    emit visibleCountChanged();
}

QVariantMap ContentFilterModel::counts() const {
    return {{"all", m_source->countOf({})}, {"mod", m_source->countOf(QStringLiteral("mod"))},
            {"resourcepack", m_source->countOf(QStringLiteral("resourcepack"))},
            {"shader", m_source->countOf(QStringLiteral("shader"))}};
}

void ContentFilterModel::setQuery(const QString &value) {
    if (value == m_query) return;
    m_query = value;
    m_tokens = ContentListModel::fold(value).split(QLatin1Char(' '), Qt::SkipEmptyParts);
    invalidateFilter();
    emit queryChanged();
}

void ContentFilterModel::setKind(const QString &value) {
    const auto next = value == QLatin1String("mod") || value == QLatin1String("resourcepack") || value == QLatin1String("shader")
        ? value : QString();
    if (next == m_kind) return;
    m_kind = next;
    invalidateFilter();
    emit kindChanged();
}

void ContentFilterModel::setOrder(const QString &value) {
    static const QStringList orders = {QStringLiteral("name"), QStringLiteral("nameDesc"), QStringLiteral("recent"),
                                       QStringLiteral("enabled")};
    const auto next = orders.contains(value) ? value : QStringLiteral("name");
    if (next == m_order) return;
    m_order = next;
    invalidate();
    emit orderChanged();
}

bool ContentFilterModel::filterAcceptsRow(int sourceRow, const QModelIndex &sourceParent) const {
    if (sourceParent.isValid() || sourceRow < 0 || sourceRow >= m_source->rowCount()) return false;
    const auto &item = m_source->itemAt(sourceRow);
    if (!m_kind.isEmpty() && item.kind != m_kind) return false;
    for (const auto &token : m_tokens)
        if (!item.search.contains(token)) return false;
    return true;
}

bool ContentFilterModel::lessThan(const QModelIndex &left, const QModelIndex &right) const {
    const auto &a = m_source->itemAt(left.row());
    const auto &b = m_source->itemAt(right.row());
    const auto byName = [this, &a, &b] {
        const int result = m_collator.compare(a.titleKey, b.titleKey);
        return result != 0 ? result : a.id.compare(b.id);
    };
    if (m_order == QLatin1String("recent") && a.modified != b.modified) return a.modified > b.modified;
    if (m_order == QLatin1String("enabled") && a.enabled != b.enabled) return a.enabled;
    if (m_order == QLatin1String("nameDesc")) return byName() > 0;
    return byName() < 0;
}
