#pragma once

#include <QFutureWatcher>
#include <QObject>
#include <QSettings>
#include <QVariantList>
#include <QTimer>
#include <QLockFile>
#include <QVariantMap>
#include <QColor>
#include <QUrl>
#include <QSet>
#include <QHash>
#include "ContentListModel.h"
#include "InstanceBanners.h"
#include <QStringList>
#include <functional>
#include <memory>

class QNetworkAccessManager;
class QNetworkReply;

class LauncherController final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString themeId READ themeId WRITE setThemeId NOTIFY preferencesChanged)
    Q_PROPERTY(bool particlesEnabled READ particlesEnabled WRITE setParticlesEnabled NOTIFY preferencesChanged)
    Q_PROPERTY(bool spotlightEnabled READ spotlightEnabled WRITE setSpotlightEnabled NOTIFY preferencesChanged)
    Q_PROPERTY(int allocatedRamMb READ allocatedRamMb WRITE setAllocatedRamMb NOTIFY preferencesChanged)
    Q_PROPERTY(bool minimizeOnLaunch READ minimizeOnLaunch WRITE setMinimizeOnLaunch NOTIFY preferencesChanged)
    Q_PROPERTY(bool refocusOnExit READ refocusOnExit WRITE setRefocusOnExit NOTIFY preferencesChanged)
    Q_PROPERTY(QString gcPreset READ gcPreset WRITE setGcPreset NOTIFY preferencesChanged)
    Q_PROPERTY(bool preTouchMemory READ preTouchMemory WRITE setPreTouchMemory NOTIFY preferencesChanged)
    Q_PROPERTY(bool highPriority READ highPriority WRITE setHighPriority NOTIFY preferencesChanged)
    Q_PROPERTY(bool potatoMode READ potatoMode WRITE setPotatoMode NOTIFY preferencesChanged)
    Q_PROPERTY(bool autoSodium READ autoSodium WRITE setAutoSodium NOTIFY preferencesChanged)
    Q_PROPERTY(int windowWidth READ windowWidth WRITE setWindowWidth NOTIFY preferencesChanged)
    Q_PROPERTY(int windowHeight READ windowHeight WRITE setWindowHeight NOTIFY preferencesChanged)
    Q_PROPERTY(bool fullscreen READ fullscreen WRITE setFullscreen NOTIFY preferencesChanged)
    Q_PROPERTY(QString customJvmArgs READ customJvmArgs WRITE setCustomJvmArgs NOTIFY preferencesChanged)
    Q_PROPERTY(int downloadConcurrency READ downloadConcurrency WRITE setDownloadConcurrency NOTIFY preferencesChanged)
    Q_PROPERTY(int javaMajor READ javaMajor WRITE setJavaMajor NOTIFY preferencesChanged)
    Q_PROPERTY(bool closeWarning READ closeWarning WRITE setCloseWarning NOTIFY preferencesChanged)
    Q_PROPERTY(bool memoryWarning READ memoryWarning WRITE setMemoryWarning NOTIFY preferencesChanged)
    Q_PROPERTY(QString preLaunchHook READ preLaunchHook NOTIFY preferencesChanged)
    Q_PROPERTY(QString postExitHook READ postExitHook NOTIFY preferencesChanged)
    Q_PROPERTY(QString wrapperCommand READ wrapperCommand NOTIFY preferencesChanged)
    Q_PROPERTY(QVariantMap storageInfo READ storageInfo NOTIFY storageChanged)
    Q_PROPERTY(bool showSnapshots READ showSnapshots WRITE setShowSnapshots NOTIFY preferencesChanged)
    Q_PROPERTY(bool crashReport READ crashReport WRITE setCrashReport NOTIFY preferencesChanged)
    Q_PROPERTY(bool compactLibrary READ compactLibrary WRITE setCompactLibrary NOTIFY preferencesChanged)
    Q_PROPERTY(QVariantMap minecraftVersions READ minecraftVersions NOTIFY versionsChanged)
    Q_PROPERTY(bool versionsLoading READ versionsLoading NOTIFY versionsChanged)
    Q_PROPERTY(QString versionsError READ versionsError NOTIFY versionsChanged)
    Q_PROPERTY(bool storageBusy READ storageBusy NOTIFY storageChanged)
    Q_PROPERTY(QString storageMessage READ storageMessage NOTIFY storageChanged)
    Q_PROPERTY(QString jvmFlagsPreview READ jvmFlagsPreview NOTIFY preferencesChanged)
    Q_PROPERTY(QVariantMap javaPaths READ javaPaths NOTIFY preferencesChanged)
    Q_PROPERTY(bool javaBusy READ javaBusy NOTIFY javaChanged)
    Q_PROPERTY(QVariantMap javaStatus READ javaStatus NOTIFY javaChanged)
    Q_PROPERTY(QString javaMessage READ javaMessage NOTIFY javaChanged)
    Q_PROPERTY(QString dataDirectory READ dataDirectory CONSTANT)
    Q_PROPERTY(QVariantMap hardware READ hardware NOTIFY hardwareChanged)
    Q_PROPERTY(bool hardwareLoading READ hardwareLoading NOTIFY hardwareChanged)
    Q_PROPERTY(QString hardwareError READ hardwareError NOTIFY hardwareChanged)
    Q_PROPERTY(QString linkError READ linkError NOTIFY linkErrorChanged)
    Q_PROPERTY(QVariantList instances READ instances NOTIFY libraryChanged)
    Q_PROPERTY(QString libraryError READ libraryError NOTIFY libraryChanged)
    Q_PROPERTY(QString copyMessage READ copyMessage NOTIFY copyChanged)
    Q_PROPERTY(bool copyingInstance READ copyingInstance NOTIFY copyChanged)
    Q_PROPERTY(QVariantList crashFindings READ crashFindings NOTIFY crashFindingsChanged)
    Q_PROPERTY(bool crashAnalyzing READ crashAnalyzing NOTIFY crashFindingsChanged)
    Q_PROPERTY(QString accountName READ accountName NOTIFY accountChanged)
    Q_PROPERTY(QString accountError READ accountError NOTIFY accountChanged)
    Q_PROPERTY(QString offlineName READ offlineName NOTIFY accountChanged)
    Q_PROPERTY(QStringList offlineProfiles READ offlineProfiles NOTIFY accountChanged)
    // instanceId -> {kind: "offline"|"microsoft", key}: cuenta propia de una instancia; sin entrada usa la activa.
    Q_PROPERTY(QVariantMap instanceAccounts READ instanceAccounts NOTIFY accountChanged)
    Q_PROPERTY(QString microsoftName READ microsoftName NOTIFY accountChanged)
    Q_PROPERTY(QVariantMap microsoftProfile READ microsoftProfile NOTIFY accountChanged)
    Q_PROPERTY(QVariantList microsoftAccounts READ microsoftAccounts NOTIFY accountChanged)
    Q_PROPERTY(QString microsoftAccountId READ microsoftAccountId NOTIFY accountChanged)
    Q_PROPERTY(QVariantMap accountDiagnosis READ accountDiagnosis NOTIFY diagnosisChanged)
    Q_PROPERTY(bool accountDiagnosing READ accountDiagnosing NOTIFY diagnosisChanged)
    Q_PROPERTY(QVariantList accountCapes READ accountCapes NOTIFY accountChanged)
    Q_PROPERTY(QString activeCapeId READ activeCapeId NOTIFY accountChanged)
    Q_PROPERTY(bool usingMicrosoft READ usingMicrosoft NOTIFY accountChanged)
    Q_PROPERTY(bool microsoftBusy READ microsoftBusy NOTIFY accountChanged)
    Q_PROPERTY(bool microsoftNeedsLogin READ microsoftNeedsLogin NOTIFY accountChanged)
    Q_PROPERTY(QString microsoftMessage READ microsoftMessage NOTIFY accountChanged)
    Q_PROPERTY(QString microsoftCode READ microsoftCode NOTIFY accountChanged)
    Q_PROPERTY(bool gameBusy READ gameBusy NOTIFY gameChanged)
    Q_PROPERTY(QString gameMessage READ gameMessage NOTIFY gameChanged)
    Q_PROPERTY(int gamePercent READ gamePercent NOTIFY gameChanged)
    Q_PROPERTY(bool gameRunning READ gameRunning NOTIFY gameChanged)
    Q_PROPERTY(QString playingInstanceId READ playingInstanceId NOTIFY gameChanged)
    Q_PROPERTY(bool catalogBusy READ catalogBusy NOTIFY catalogChanged)
    Q_PROPERTY(bool projectBusy READ projectBusy NOTIFY projectChanged)
    Q_PROPERTY(QVariantMap projectInfo READ projectInfo NOTIFY projectChanged)
    Q_PROPERTY(QString projectError READ projectError NOTIFY projectChanged)
    Q_PROPERTY(QVariantList catalogResults READ catalogResults NOTIFY catalogChanged)
    Q_PROPERTY(QString catalogInstance READ catalogInstance NOTIFY catalogChanged)
    Q_PROPERTY(QString catalogKind READ catalogKind NOTIFY catalogChanged)
    Q_PROPERTY(QString catalogMessage READ catalogMessage NOTIFY catalogChanged)
    Q_PROPERTY(int catalogPage READ catalogPage NOTIFY catalogChanged)
    Q_PROPERTY(int catalogTotal READ catalogTotal NOTIFY catalogChanged)
    Q_PROPERTY(QVariantMap catalogFilters READ catalogFilters NOTIFY catalogChanged)
    Q_PROPERTY(QVariantMap officialPack READ officialPack NOTIFY officialChanged)
    Q_PROPERTY(QString defaultInstanceId READ defaultInstanceId NOTIFY officialChanged)
    Q_PROPERTY(bool operationActive READ operationActive NOTIFY operationChanged)
    Q_PROPERTY(QString operationMessage READ operationMessage NOTIFY operationChanged)
    Q_PROPERTY(int operationPercent READ operationPercent NOTIFY operationChanged)
    Q_PROPERTY(bool operationCancellable READ operationCancellable NOTIFY operationChanged)
    Q_PROPERTY(QString operationKind READ operationKind NOTIFY operationChanged)
    Q_PROPERTY(QVariantMap modpackInfo READ modpackInfo NOTIFY modpackInfoChanged)
    Q_PROPERTY(QVariantMap modpackResult READ modpackResult NOTIFY modpackResultChanged)
    Q_PROPERTY(QString modpackError READ modpackError NOTIFY modpackResultChanged)
    Q_PROPERTY(QVariantList snapshots READ snapshots NOTIFY snapshotsChanged)
    Q_PROPERTY(QString snapshotsInstance READ snapshotsInstance NOTIFY snapshotsChanged)
    Q_PROPERTY(QVariantList worlds READ worlds NOTIFY worldsChanged)
    Q_PROPERTY(QVariantList worldBackups READ worldBackups NOTIFY worldsChanged)
    Q_PROPERTY(QString worldsInstance READ worldsInstance NOTIFY worldsChanged)
    Q_PROPERTY(bool worldsLoading READ worldsLoading NOTIFY worldsChanged)
    Q_PROPERTY(QString worldMessage READ worldMessage NOTIFY worldsChanged)
    Q_PROPERTY(bool worldFailed READ worldFailed NOTIFY worldsChanged)
    Q_PROPERTY(QVariantList linkCandidates READ linkCandidates NOTIFY linkCandidatesChanged)
    Q_PROPERTY(QVariantList conflicts READ conflicts NOTIFY conflictsChanged)
    Q_PROPERTY(int conflictCount READ conflictCount NOTIFY conflictsChanged)
    Q_PROPERTY(bool conflictsCacheMissing READ conflictsCacheMissing NOTIFY conflictsChanged)
    Q_PROPERTY(QString conflictsInstance READ conflictsInstance NOTIFY conflictsChanged)
    Q_PROPERTY(QVariantMap curseForgePreview READ curseForgePreview NOTIFY curseForgePreviewChanged)
    // Contenido de la instancia abierta: lista unificada (mods, texturas y shaders) ya filtrada y ordenada.
    Q_PROPERTY(ContentFilterModel *content READ content CONSTANT)
    Q_PROPERTY(QString contentInstance READ contentInstance NOTIFY contentChanged)
    Q_PROPERTY(bool contentLoading READ contentLoading NOTIFY contentChanged)
    Q_PROPERTY(bool contentResolving READ contentResolving NOTIFY contentChanged)
    Q_PROPERTY(int contentPending READ contentPending NOTIFY contentChanged)
    Q_PROPERTY(bool contentOffline READ contentOffline NOTIFY contentChanged)
    Q_PROPERTY(bool contentTruncated READ contentTruncated NOTIFY contentChanged)
    Q_PROPERTY(QString contentError READ contentError NOTIFY contentChanged)
    Q_PROPERTY(int bannerRevision READ bannerRevision NOTIFY bannersChanged)
    Q_PROPERTY(bool setupPending READ setupPending NOTIFY introductionChanged)
    Q_PROPERTY(bool releasePending READ releasePending NOTIFY introductionChanged)

public:
    explicit LauncherController(const QString &settingsFile, QObject *parent = nullptr);
    ~LauncherController() override;
    QString themeId() const { return m_themeId; }
    bool particlesEnabled() const { return m_particles; }
    bool spotlightEnabled() const { return m_spotlight; }
    int allocatedRamMb() const { return m_ram; }
    bool minimizeOnLaunch() const { return m_minimizeOnLaunch; }
    bool refocusOnExit() const { return m_refocusOnExit; }
    QString gcPreset() const { return m_gcPreset; }
    bool preTouchMemory() const { return m_preTouchMemory; }
    bool highPriority() const { return m_highPriority; }
    bool potatoMode() const { return m_potatoMode; }
    bool autoSodium() const { return m_autoSodium; }
    int windowWidth() const { return m_windowWidth; }
    int windowHeight() const { return m_windowHeight; }
    bool fullscreen() const { return m_fullscreen; }
    QString customJvmArgs() const { return m_customJvmArgs; }
    int downloadConcurrency() const { return m_downloadConcurrency; }
    int javaMajor() const { return m_javaMajor; }
    bool closeWarning() const { return m_closeWarning; }
    bool memoryWarning() const { return m_memoryWarning; }
    QString preLaunchHook() const { return m_preLaunchHook; }
    QString postExitHook() const { return m_postExitHook; }
    QString wrapperCommand() const { return m_wrapperCommand; }
    QVariantMap storageInfo() const { return m_storageInfo; }
    bool showSnapshots() const { return m_showSnapshots; }
    bool crashReport() const { return m_crashReport; }
    bool compactLibrary() const { return m_compactLibrary; }
    QVariantMap minecraftVersions() const { return m_minecraftVersions; }
    bool versionsLoading() const { return m_versionsLoading; }
    QString versionsError() const { return m_versionsError; }
    void setShowSnapshots(bool value);
    void setCrashReport(bool value);
    void setCompactLibrary(bool value);
    Q_INVOKABLE void loadMinecraftVersions(bool refresh);
    Q_INVOKABLE void copyText(const QString &text);
    Q_INVOKABLE void styleWindowFrame(QObject *window, const QColor &caption, const QColor &text, const QColor &border, bool dark);
    bool storageBusy() const { return m_storagePending; }
    QString storageMessage() const { return m_storageMessage; }
    QString jvmFlagsPreview() const;
    QVariantMap javaPaths() const { return m_javaPaths; }
    bool javaBusy() const { return m_javaPending; }
    QVariantMap javaStatus() const { return m_javaStatus; }
    QString javaMessage() const { return m_javaMessage; }
    QString dataDirectory() const { return m_libraryRoot; }
    Q_INVOKABLE bool setJavaPath(int major, const QUrl &url);
    Q_INVOKABLE void checkJava(int major);
    Q_INVOKABLE void installJava(int major);
    Q_INVOKABLE void openDataDirectory();
    QVariantMap hardware() const { return m_hardware; }
    bool hardwareLoading() const { return m_loading; }
    QString hardwareError() const { return m_hardwareError; }
    QString linkError() const { return m_linkError; }
    QVariantList instances() const { return m_instances; }
    QVariantList crashFindings() const { return m_crashFindings; }
    bool crashAnalyzing() const { return m_crashAnalyzing; }
    QString libraryError() const { return m_libraryError; }
    QString copyMessage() const { return m_copyMessage; }
    bool copyingInstance() const { return m_copyingInstance; }
    Q_INVOKABLE void duplicateInstance(const QString &id);
    Q_INVOKABLE void exportInstance(const QString &id, const QUrl &directory);
    Q_INVOKABLE void importInstance(const QUrl &directory);
    Q_INVOKABLE void importModpack(const QUrl &file);
    QString accountName() const { return m_useMicrosoft ? microsoftName() : m_accountName; }
    QString offlineName() const { return m_accountName; }
    QStringList offlineProfiles() const { return m_offlineProfiles; }
    QVariantMap instanceAccounts() const { return m_instanceAccounts; }
    QString microsoftName() const { return m_microsoftProfile.value("username").toString(); }
    QVariantMap microsoftProfile() const { return m_microsoftProfile; }
    QVariantList microsoftAccounts() const { return m_microsoftAccounts; }
    QString microsoftAccountId() const { return m_microsoftId; }
    QVariantMap accountDiagnosis() const { return m_diagnosis; }
    bool accountDiagnosing() const { return m_diagnosing; }
    QVariantList accountCapes() const { return m_microsoftProfile.value("capes").toList(); }
    QString activeCapeId() const { return m_microsoftProfile.value("capeId").toString(); }
    bool usingMicrosoft() const { return m_useMicrosoft; }
    bool microsoftBusy() const { return m_authAttempt != 0 || m_auth.isRunning(); }
    bool microsoftNeedsLogin() const { return m_microsoftNeedsLogin; }
    QString microsoftMessage() const { return m_microsoftMessage; }
    QString microsoftCode() const { return m_microsoftPrompt.value("user_code").toString(); }
    QString accountError() const { return m_accountError; }
    bool gameBusy() const { return static_cast<bool>(m_gameLock); }
    QString gameMessage() const { return m_gameMessage; }
    int gamePercent() const { return m_gamePercent; }
    QString playingInstanceId() const { return gameBusy() ? m_playingId : QString(); }
    bool gameRunning() const { return gameBusy() && m_gameWasRunning; }
    QString linkUrl(int id) const;
    bool catalogBusy() const { return m_catalogPending; }
    bool projectBusy() const { return m_projectPending; }
    QVariantMap projectInfo() const { return m_projectInfo; }
    QString projectError() const { return m_projectError; }
    Q_INVOKABLE bool loadProject(const QString &id, const QString &project, const QString &kind);
    QVariantList catalogResults() const { return m_catalogResults; }
    QString catalogInstance() const { return m_catalogInstance; }
    QString catalogKind() const { return m_catalogKind; }
    QString catalogMessage() const { return m_catalogMessage; }
    int catalogPage() const { return m_catalogPage; }
    int catalogTotal() const { return m_catalogTotal; }
    QVariantMap catalogFilters() const { return m_catalogFilters; }
    Q_INVOKABLE void searchCatalog(const QString &id, const QString &query, const QString &kind,
                                   const QString &category, const QString &sort, int page);
    QVariantMap officialPack() const;
    QString defaultInstanceId() const { return officialInstance().value("id").toString(); }
    Q_INVOKABLE void inspectOfficialPack();
    Q_INVOKABLE void installOfficialPack();
    bool setupPending() const;
    bool releasePending() const;
    Q_INVOKABLE bool completeSetup();
    Q_INVOKABLE bool acknowledgeRelease();
    Q_INVOKABLE void changeContent(const QString &id, const QString &project, const QString &action, const QString &kind);
    Q_INVOKABLE void syncContent(const QString &id);
    Q_INVOKABLE void updateInstanceContent(const QString &id);
    Q_INVOKABLE void installModpack(const QString &project);
    Q_INVOKABLE void installProjectVersion(const QString &id, const QString &project, const QString &kind, const QString &version,
                                           const QStringList &optional = {});
    bool operationActive() const { return m_operationActive; }
    QString operationMessage() const { return m_operationMessage; }
    int operationPercent() const { return m_operationPercent; }
    bool operationCancellable() const { return m_operationCancellable; }
    QString operationKind() const { return m_operationKind; }
    QVariantMap modpackInfo() const { return m_modpackInfo; }
    QVariantMap modpackResult() const { return m_modpackResult; }
    QString modpackError() const { return m_modpackError; }
    QVariantList snapshots() const { return m_snapshots; }
    QString snapshotsInstance() const { return m_snapshotsId; }
    QVariantList worlds() const { return m_worlds; }
    QVariantList worldBackups() const { return m_worldBackups; }
    QString worldsInstance() const { return m_worldsId; }
    bool worldsLoading() const { return m_worldsLoading; }
    QString worldMessage() const { return m_worldMessage; }
    bool worldFailed() const { return m_worldFailed; }
    QVariantList linkCandidates() const { return m_linkCandidates; }
    QVariantList conflicts() const { return m_conflicts; }
    int conflictCount() const { return m_conflictCount; }
    bool conflictsCacheMissing() const { return m_conflictsCacheMissing; }
    QString conflictsInstance() const { return m_conflictsId; }
    QVariantMap curseForgePreview() const { return m_curseForgePreview; }
    Q_INVOKABLE void cancelOperation();
    Q_INVOKABLE void cancelLaunch();
    Q_INVOKABLE void checkModpack(const QString &id, bool force = false);
    Q_INVOKABLE void updateModpack(const QString &id, const QString &version);
    Q_INVOKABLE void updateModpackFromFile(const QString &id, const QUrl &file);
    Q_INVOKABLE void listSnapshots(const QString &id);
    Q_INVOKABLE void restoreSnapshot(const QString &id, const QString &snapshot);
    Q_INVOKABLE void loadWorlds(const QString &id);
    Q_INVOKABLE void backupWorlds(const QString &id, const QStringList &names);
    Q_INVOKABLE void restoreWorldBackup(const QString &id, const QString &backup);
    Q_INVOKABLE void deleteWorldBackup(const QString &id, const QString &backup);
    Q_INVOKABLE void loadLinkCandidates(const QString &id);
    Q_INVOKABLE void linkModpack(const QString &id, const QString &project, const QString &version);
    Q_INVOKABLE void linkModpackFromFile(const QString &id, const QUrl &file);
    Q_INVOKABLE void exportMrpack(const QString &id, const QUrl &file, bool includeSaves);
    Q_INVOKABLE void importCurseForge(const QUrl &file, bool confirm);
    Q_INVOKABLE void importMultiMC(const QUrl &fileOrFolder);
    Q_INVOKABLE void contentConflicts(const QString &id);
    ContentFilterModel *content() const { return m_contentFilter; }
    QString contentInstance() const { return m_contentId; }
    bool contentLoading() const { return m_contentLoading; }
    bool contentResolving() const { return m_contentResolving; }
    int contentPending() const { return m_contentPendingBy[0] + m_contentPendingBy[1] + m_contentPendingBy[2]; }
    bool contentOffline() const { return m_contentOffline; }
    bool contentTruncated() const { return m_contentTruncated; }
    QString contentError() const { return m_contentError; }
    // Carga las tres categorías de la instancia (content_list) y completa los datos con content_resolve.
    // force = vuelve a resolver aunque no quede nada pendiente (botón «Actualizar»).
    Q_INVOKABLE void loadContent(const QString &id, bool force = false);
    // La vista de contenido se cerró: se descartan las respuestas en vuelo y se libera la lista.
    Q_INVOKABLE void releaseContent();
    Q_INVOKABLE bool setInstanceRam(const QString &id, int ramMb);
    Q_INVOKABLE void playInstance(const QString &id);
    Q_INVOKABLE QString instanceLog(const QString &id);
    Q_INVOKABLE void analyzeCrash(const QString &id);
    Q_INVOKABLE bool setOfflineAccount(const QString &username);
    Q_INVOKABLE void clearOfflineAccount();
    Q_INVOKABLE bool selectOfflineProfile(const QString &username);
    Q_INVOKABLE void removeOfflineProfile(const QString &username);
    // kind vacío = usar la cuenta activa del launcher.
    Q_INVOKABLE bool setInstanceAccount(const QString &instanceId, const QString &kind, const QString &key);
    Q_INVOKABLE void startMicrosoftLogin(bool directBrowser = false);
    Q_INVOKABLE void cancelMicrosoftLogin();
    Q_INVOKABLE void openMicrosoftVerification();
    Q_INVOKABLE void copyMicrosoftCode();
    Q_INVOKABLE void useMicrosoftProfile();
    Q_INVOKABLE void forgetMicrosoftProfile();
    Q_INVOKABLE void selectMicrosoftAccount(const QString &id);
    Q_INVOKABLE void forgetMicrosoftAccount(const QString &id);
    Q_INVOKABLE void diagnoseMicrosoftAccount(const QString &id);
    Q_INVOKABLE void runDiagnosisAction(const QString &action);
    Q_INVOKABLE void clearDiagnosis();
    Q_INVOKABLE void refreshMicrosoftProfile();
    Q_INVOKABLE void equipMicrosoftCape(const QString &capeId);
    Q_INVOKABLE bool reloadLibrary();
    Q_INVOKABLE bool saveInstance(const QString &id, const QString &name, const QString &version, const QString &loader);
    Q_INVOKABLE bool deleteInstance(const QString &id);
    Q_INVOKABLE bool toggleFavorite(const QString &id);
    Q_INVOKABLE QVariantList instanceFiles(const QString &id, const QString &category) const;
    int bannerRevision() const { return m_banners ? m_banners->revision() : 0; }
    // {source: "custom"|"modpack"|"screenshot"|"generated", sharp, soft, accent}; las rutas son file:/// o vacías.
    Q_INVOKABLE QVariantMap bannerInfo(const QString &instanceId);
    Q_INVOKABLE void setBannerImage(const QString &instanceId, const QUrl &file);
    Q_INVOKABLE void clearBanner(const QString &instanceId);
    Q_INVOKABLE void openInstanceFolder(const QString &id, const QString &category);
    void setThemeId(const QString &value);
    void setParticlesEnabled(bool value);
    void setSpotlightEnabled(bool value);
    void setAllocatedRamMb(int value);
    void setMinimizeOnLaunch(bool value);
    void setRefocusOnExit(bool value);
    void setGcPreset(const QString &value);
    void setPreTouchMemory(bool value);
    void setHighPriority(bool value);
    void setPotatoMode(bool value);
    void setAutoSodium(bool value);
    void setWindowWidth(int value);
    void setWindowHeight(int value);
    void setFullscreen(bool value);
    void setCustomJvmArgs(const QString &value);
    void setDownloadConcurrency(int value);
    void setJavaMajor(int value);
    void setCloseWarning(bool value);
    void setMemoryWarning(bool value);
    // Los comandos solo se guardan desde la interfaz, tras mostrar el texto completo al usuario.
    Q_INVOKABLE bool setLaunchCommands(const QString &preLaunch, const QString &postExit, const QString &wrapper);
    Q_INVOKABLE bool setInstanceSettings(const QString &id, const QVariantMap &fields);
    Q_INVOKABLE void refreshStorage();
    Q_INVOKABLE void clearStorage(const QString &kind);
    Q_INVOKABLE void refreshHardware();
    Q_INVOKABLE bool linkAvailable(int id) const;
    Q_INVOKABLE void openLink(int id);

signals:
    void preferencesChanged();
    void javaChanged();
    void storageChanged();
    void versionsChanged();
    void gameCrashed(const QString &instanceId, int exitCode);
    void crashFindingsChanged();
    void hardwareChanged();
    void linkErrorChanged();
    void libraryChanged();
    void accountChanged();
    void diagnosisChanged();
    void gameChanged();
    void catalogChanged();
    void projectChanged();
    void introductionChanged();
    void officialChanged();
    void copyChanged();
    void operationChanged();
    void modpackInfoChanged(const QString &id);
    void modpackResultChanged();
    void snapshotsChanged();
    void worldsChanged();
    void linkCandidatesChanged();
    void conflictsChanged();
    void contentChanged();
    void curseForgePreviewChanged();
    void bannersChanged();
    void bannerFailed(const QString &message);
    void minimizeRequested();
    void refocusRequested();

private:
    static QVariantMap readHardware();
    bool updateLibrary(QVariantMap request);
    QString instanceDirectory(const QString &id, const QString &category, bool create = false) const;
    // Accesos con el nombre de la instancia (junctions fuera de la carpeta de datos), ver InstanceLinks.
    QString instanceLinksDirectory() const;
    void scheduleLinkSync();
    void requestBanner(const QString &id);
    void refreshBanners();
    QVariantMap instanceEntry(const QString &id) const;
    void syncInstanceLinks();
    QString namedPath(const QString &id, const QString &realPath) const;
    friend int main(int, char **);
    static QVariant requestEngine(const QVariantMap &request, QString &error);
    bool persistAccount(const QString &username);
    void saveOfflineProfiles();
    void pollMicrosoftPrompt();
    bool selectMicrosoft(bool value);
    void reloadMicrosoftAccounts();
    void pollGame();
    void startCatalogRequest(QVariantMap request, bool writesLibrary);
    void startFileOperation(const QVariantMap &request, const QString &message);
    void startModpackWrite(QVariantMap request);
    void runQuery(QVariantMap request, std::function<void(const QVariantMap &)> done);
    void requestModpackCheck(const QString &id, bool force);
    void resolveContentStep(int token);
    void pollOperation();
    void beginOperation(const QString &kind);
    void endOperation(bool cancelled);
    bool applyInstances(const QVariant &result);
    QVariantList withDiskMods(QVariantList list) const;
    void startOfficialUpdate(const QString &id);
    QString modpackWriteMessage(const QString &action, const QVariantMap &result);
    void persist();
    QString officialPackPath() const;
    QString officialPackCachePath() const;
    void downloadOfficialPack();
    QNetworkAccessManager *m_packNetwork = nullptr;
    QNetworkReply *m_packReply = nullptr;
    QVariantMap officialInstance() const;
    QFutureWatcher<QVariantMap> m_official;
    QVariantMap m_officialSummary;
    QString m_officialError;
    QString m_officialPath;
    bool m_officialPending = false;
    QSettings m_settings;
    QString m_libraryRoot;
    QString m_libraryError;
    QString m_accountName;
    QStringList m_offlineProfiles;
    QVariantMap m_instanceAccounts;
    void saveInstanceAccounts();
    // Quita la cuenta propia de las instancias que usaban esa cuenta (o de una instancia concreta si kind está vacío).
    void dropInstanceAccounts(const QString &kind, const QString &key);
    QString m_accountError;
    QVariantMap m_microsoftProfile;
    QVariantList m_microsoftAccounts;
    QVariantMap m_diagnosis;
    QString m_diagnosisId;
    bool m_diagnosing = false;
    QString m_microsoftId;
    QVariantMap m_microsoftPrompt;
    QString m_microsoftMessage;
    bool m_useMicrosoft = false;
    bool m_microsoftNeedsLogin = false;
    bool m_authCancelled = false;
    quint64 m_authAttempt = 0;
    QTimer m_authTimer;
    QFutureWatcher<QVariantMap> m_auth;
    std::unique_ptr<QLockFile> m_authLock;
    QVariantList m_instances;
    InstanceBanners *m_banners = nullptr;
    bool m_bannerGameBusy = false;
    QHash<QString, QString> m_instanceLinks;
    bool m_linksSyncing = false;
    bool m_linksPending = false;
    QFutureWatcher<QVariantMap> m_probe;
    QFutureWatcher<QString> m_launch;
    QFutureWatcher<QVariantMap> m_catalog;
    QFutureWatcher<QVariantMap> m_project;
    QVariantMap m_projectInfo;
    QString m_projectError;
    bool m_projectPending = false;
    QVariantList m_catalogResults;
    QString m_catalogInstance;
    QString m_catalogKind;
    QString m_catalogMessage;
    int m_catalogPage = 0;
    int m_catalogTotal = 0;
    QVariantMap m_catalogFilters;
    bool m_catalogPending = false;
    bool m_copyingInstance = false;
    QString m_copyMessage;
    QTimer m_operationTimer;
    bool m_operationActive = false;
    QString m_operationMessage;
    int m_operationPercent = -1;
    bool m_operationCancellable = false;
    QString m_operationKind;
    quint64 m_operationId = 0;
    QVariantMap m_modpackInfo;
    QSet<QString> m_modpackChecked;
    QSet<QString> m_modpackChecking;
    QVariantMap m_modpackResult;
    QString m_modpackError;
    QVariantList m_worlds;
    QVariantList m_worldBackups;
    QString m_worldsId;
    QString m_worldMessage;
    bool m_worldsLoading = false;
    bool m_worldFailed = false;
    QVariantList m_snapshots;
    QString m_snapshotsId;
    QVariantList m_linkCandidates;
    QString m_linkCandidatesId;
    QVariantList m_conflicts;
    int m_conflictCount = 0;
    bool m_conflictsCacheMissing = false;
    QString m_conflictsId;
    ContentListModel *m_contentModel = new ContentListModel(this);
    ContentFilterModel *m_contentFilter = new ContentFilterModel(m_contentModel, this);
    QString m_contentId;
    QString m_contentError;
    bool m_contentActive = false;
    bool m_contentLoading = false;
    bool m_contentResolving = false;
    bool m_contentOffline = false;
    bool m_contentTruncated = false;
    bool m_loadOffline = false;
    bool m_loadTruncated = false;
    int m_contentToken = 0;
    int m_contentListsLeft = 0;
    int m_contentStall = 0;
    int m_contentPendingBy[3] = {0, 0, 0};
    bool m_contentForceBy[3] = {false, false, false};
    QVariantMap m_curseForgePreview;
    QString m_officialLinkingId;
    QList<QFutureWatcher<QVariantMap> *> m_queries;
    QTimer m_gameTimer;
    std::unique_ptr<QLockFile> m_gameLock;
    QString m_gameMessage;
    QString m_playingId;
    int m_gamePercent = 0;
    bool m_launchPending = false;
    bool m_gameWasRunning = false;
    bool m_minimizeOnLaunch = false;
    bool m_refocusOnExit = false;
    QString m_gcPreset = "automatic";
    bool m_preTouchMemory = false;
    bool m_highPriority = false;
    bool m_potatoMode = false;
    bool m_autoSodium = false;
    int m_windowWidth = 854;
    int m_windowHeight = 480;
    bool m_fullscreen = false;
    int m_downloadConcurrency = 16;
    int m_javaMajor = 21;
    bool m_closeWarning = true;
    bool m_memoryWarning = true;
    QString m_preLaunchHook;
    QString m_postExitHook;
    QString m_wrapperCommand;
    QFutureWatcher<QVariantMap> m_storage;
    QVariantMap m_storageInfo;
    QString m_storageMessage;
    bool m_storagePending = false;
    bool m_showSnapshots = false;
    bool m_crashReport = true;
    QVariantList m_crashFindings;
    bool m_crashAnalyzing = false;
    int m_crashToken = 0;
    bool m_compactLibrary = false;
    QFutureWatcher<QVariantMap> m_versionsWatcher;
    QVariantMap m_minecraftVersions;
    bool m_versionsLoading = false;
    QString m_versionsError;
    QString m_customJvmArgs;
    QVariantMap m_javaPaths;
    QFutureWatcher<QVariantMap> m_java;
    QVariantMap m_javaStatus;
    QString m_javaMessage;
    bool m_javaPending = false;
    QVariantMap m_hardware;
    QString m_themeId = "crystal-ruby";
    QString m_hardwareError;
    QString m_linkError;
    bool m_particles = true;
    bool m_spotlight = true;
    bool m_loading = false;
    int m_ram = 2048;
};
