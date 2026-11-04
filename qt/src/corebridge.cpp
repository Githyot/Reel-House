#include "corebridge.h"
#include <QTimer>
#include "videoitem.h"
#include "bridge.h"
#include "imageprovider.h"
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QDesktopServices>
#include <QUrl>
#include <QDebug>
#include <QStringList>
#include <QImage>
#include <QImageReader>
#include <QImageWriter>
#include <QDir>
#include <QFileInfo>
#include <QCryptographicHash>
#include <QDateTime>
#include <QStandardPaths>
#include <QRectF>
#include <cstring>
#include <thread>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QWheelEvent>
#include <QCursor>
#ifdef Q_OS_WIN
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#endif
#include <QColor>
#include <QGuiApplication>
#include <cmath>
#ifdef Q_OS_ANDROID
#include <QJniObject>
#include <QJniEnvironment>
#include <QCoreApplication>
#include <QtCore/qcoreapplication_platform.h>
#endif
#include <QWindow>
#include <QCoreApplication>
#include <QBuffer>
#include <QFile>
#include <QNetworkAccessManager>
#include <QNetworkProxy>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QQuickItem>
#include <QQuickItemGrabResult>
#include <QSharedPointer>
#include "bridge.h"
#ifdef REELHOUSE_LIVE_AUDIO
#include "liveaudio.h"
#endif

#include <atomic>

// Read from mpv's thread (frame callback) and the scene graph's render thread (VideoRenderer::render).
static std::atomic<CoreBridge *> g_inst{nullptr};

static QVariantMap parseMap(char *raw) {
    if (!raw) return {};
    const QByteArray bytes(raw);
    rh_free_str(raw);
    return QJsonDocument::fromJson(bytes).object().toVariantMap();
}

CoreBridge::CoreBridge(QObject *parent) : QObject(parent) {
    m_b = rh_new();
    // IP protection: Qt's own requests (pictures, QML) go through the core's gateway before the first one is made
    applyPrivacyProxy(rh_privacy_port());
    g_inst.store(this);
    if (!m_b) qCritical() << "the core could not start";
    else {
        rh_set_frame_callback(m_b, [](void *) { if (CoreBridge *c = g_inst.load()) c->frameFromMpv(); }, nullptr);
#ifdef REELHOUSE_LIVE_AUDIO
        // Conversation by voice (Gemini Live) can record and play here
        rh_live_audio_present(1);
#endif
        m_settings = fetch("settings");
        m_notices = fetch("notices");
        // the Study state (every deck's numbers) just after start, not before the window shows
        QTimer::singleShot(0, this, [this] { m_study = fetch("study"); emit studyChanged(); });
        m_profiles = fetch("profiles");
        cmd({{"cmd", "load_home"}});
#ifdef Q_OS_ANDROID
        // The Images mode's extensions run inside the app on Android (docs/android-extensions.md): the Kotlin host
        // loads the installed ones and listens on 127.0.0.1, off the main thread since loading takes a moment.
        std::thread([this] {
            const QString dir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/extensions";
            QDir().mkpath(dir);
            QJniObject ctx = QNativeInterface::QAndroidApplication::context();
            const jint port = QJniObject::callStaticMethod<jint>("org/reelhouse/ext/ExtensionHost", "start", "(Landroid/content/Context;Ljava/lang/String;)I",
                                                                 ctx.object(), QJniObject::fromString(dir).object<jstring>());
            QJniEnvironment env;
            if (env.checkAndClearExceptions()) qWarning() << "extension host: Java exception while starting";
            qInfo() << "extension host port" << port;
            QMetaObject::invokeMethod(this, [this, port, dir] { cmd({{"cmd", "ext_host"}, {"port", int(port)}, {"dir", dir}}); }, Qt::QueuedConnection);
        }).detach();
#endif
        // Testing aid: REELHOUSE_TEST_PLAY=<file or address> plays it a moment after start.
        const QString testPlay = qEnvironmentVariable("REELHOUSE_TEST_PLAY");
        if (!testPlay.isEmpty())
            QTimer::singleShot(3000, this, [this, testPlay] { cmd({{"cmd", "play_url"}, {"url", testPlay}, {"title", "Test"}}); });
        // Testing aid: REELHOUSE_TEST_CMDS=[{"cmd":...},...] sends those commands one per second, starting 3 s after start.
        const QByteArray testCmds = qgetenv("REELHOUSE_TEST_CMDS");
        if (!testCmds.isEmpty()) {
            const QJsonArray arr = QJsonDocument::fromJson(testCmds).array();
            for (int i = 0; i < arr.size(); ++i)
                QTimer::singleShot(3000 + i * 1000, this, [this, c = arr[i].toObject().toVariantMap()] { cmd(c); });
        }
    }
    connect(&m_timer, &QTimer::timeout, this, &CoreBridge::tick);
    m_timer.start(50);
}

CoreBridge::~CoreBridge() {
    // First: no frame callback or renderer reaches this object (or the core) any more.
    g_inst.store(nullptr);
    m_timer.stop();
    m_picTimer.stop();
#ifdef REELHOUSE_LIVE_AUDIO
    LiveAudio::shutdown();
#endif
    if (m_b) {
        rh_set_frame_callback(m_b, nullptr, nullptr);
        rh_free(m_b);
        m_b = nullptr;
    }
}

CoreBridge *CoreBridge::instance() { return g_inst.load(); }
Bridge *CoreBridge::handle() {
    CoreBridge *c = g_inst.load();
    return c ? c->m_b : nullptr;
}

void CoreBridge::setVideoItem(VideoItem *v) { m_video = v; }

void CoreBridge::frameFromMpv() {
    // mpv's thread: m_video is only read on the interface thread (the item may be gone by then).
    QMetaObject::invokeMethod(this, [this] { if (m_video) m_video->frameReady(); }, Qt::QueuedConnection);
}

void CoreBridge::reloadPictures() {
    m_picGen = WebImageProvider::startReload();
    m_picLastDone = -1;
    m_picQuiet = 0;
    m_picReload = QVariantMap{{"active", true}, {"asked", 0}, {"done", 0}};
    emit picReloadChanged();
    if (!m_picTimer.isActive()) {
        m_picTimer.setInterval(250);
        connect(&m_picTimer, &QTimer::timeout, this, [this] {
            int asked = 0, done = 0;
            WebImageProvider::stats(asked, done);
            // Finished when everything asked for is done and nothing new was asked for during a moment.
            m_picQuiet = (done == m_picLastDone && done >= asked) ? m_picQuiet + 1 : 0;
            m_picLastDone = done;
            const bool end = m_picQuiet >= 6 || (asked == 0 && ++m_picQuiet >= 12);
            m_picReload = QVariantMap{{"active", !end}, {"asked", asked}, {"done", qMin(done, asked)}};
            emit picReloadChanged();
            if (end) m_picTimer.stop();
        });
    }
    m_picTimer.start();
    cmd({{"cmd", "load_home"}});
}

QVariantMap CoreBridge::fetch(const char *name) { return parseMap(rh_state(m_b, name)); }

QVariantMap CoreBridge::state(const QString &name) { return m_b ? fetch(name.toUtf8().constData()) : QVariantMap{}; }

void CoreBridge::refreshTorrents() {
    if (!m_b) return;
    m_torrents = fetch("torrents"); emit torrentsChanged();
}

void CoreBridge::refreshPrefs() {
    if (!m_b) return;
    m_prefs = fetch("prefs"); emit prefsChanged();
    m_settings = fetch("settings"); emit settingsChanged();
    const QString lang = uiLang();
    if (!m_uiLangSent || lang != m_lastUiLang) {
        m_uiLangSent = true;
        m_lastUiLang = lang;
        emit uiLangChanged();
    }
}

namespace {

QByteArray jpegOf(const QImage &im) {
    QByteArray ba;
    QBuffer buf(&ba);
    buf.open(QIODevice::WriteOnly);
    QImageWriter w(&buf, "jpeg");
    w.setQuality(82);
    w.write(im.convertToFormat(QImage::Format_RGB888));
    return ba;
}

// The page as the OCR takes it, the same rule as the GTK reader: a big picture is made smaller, and a long strip
// (a webtoon) is cut into slices that overlap a little, because the text would be too small to read if shrunk.
QVariantList pieces(QImage img) {
    QVariantList out;
    const int maxSide = 2200;
    if (qMax(img.width(), img.height()) > maxSide) {
        const double k = double(maxSide) / qMax(img.width(), img.height());
        img = img.scaled(qMax(1, int(img.width() * k)), qMax(1, int(img.height() * k)), Qt::KeepAspectRatio, Qt::SmoothTransformation);
    }
    const int w = img.width(), h = img.height();
    auto put = [&out](const QImage &part, double y0, double y1) {
        QVariantMap t;
        t["b64"] = QString::fromLatin1(jpegOf(part).toBase64());
        t["w"] = part.width();
        t["h"] = part.height();
        t["y0"] = y0;
        t["y1"] = y1;
        out << t;
    };
    if (h <= w * 2.4) {
        put(img, 0.0, 1.0);
        return out;
    }
    int slice = qMax(int(w * 1.6), 900);
    slice = qMax(slice, (h + 13) / 14);
    const int overlap = slice / 12;
    for (int y = 0; y < h;) {
        const int end = qMin(y + slice, h);
        put(img.copy(0, y, w, end - y), double(y) / h, double(end) / h);
        if (end >= h) break;
        y = end - overlap;
    }
    return out;
}

QNetworkAccessManager *pageNet() {
    static QNetworkAccessManager *net = new QNetworkAccessManager(qApp);
    return net;
}

}  // namespace

void CoreBridge::readPage(const QString &url) {
    auto finish = [this, url](const QByteArray &data) {
        QImage img = QImage::fromData(data);
        if (img.isNull() && !data.isEmpty()) {
            // WebP pages (the picture reader of Qt may not have the plugin) go through the core's decoder.
            int w = 0, h = 0; size_t len = 0;
            uchar *px = rh_decode_webp(reinterpret_cast<const uchar *>(data.constData()), size_t(data.size()), &w, &h, &len);
            if (px) { img = QImage(px, w, h, w * 4, QImage::Format_RGBA8888).copy(); rh_free_pixels(px, len); }
        }
        if (img.isNull()) qWarning() << "page not readable for the text:" << url << data.size() << "bytes, starts with" << data.left(12).toHex(' ');
        // An empty list tells the core that the page could not be read.
        cmd(QVariantMap{{"cmd", "page_text"}, {"url", url}, {"tiles", img.isNull() ? QVariantList() : pieces(img)}});
    };
    if (url.startsWith("http://") || url.startsWith("https://")) {
        QNetworkRequest req{QUrl(url)};
        // Picture hosts often refuse a client that does not look like a browser.
        req.setHeader(QNetworkRequest::UserAgentHeader, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36");
        req.setRawHeader("Accept", "image/png,image/jpeg,image/webp,image/*;q=0.8");
        req.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
        req.setTransferTimeout(30000);
        QNetworkReply *reply = pageNet()->get(req);
        QObject::connect(reply, &QNetworkReply::finished, this, [reply, finish, url]() {
            if (reply->error() != QNetworkReply::NoError) qWarning() << "page download failed:" << url << reply->errorString();
            const QByteArray data = reply->error() == QNetworkReply::NoError ? reply->readAll() : QByteArray();
            reply->deleteLater();
            finish(data);
        });
    } else {
        QFile f(localPath(url));
        finish(f.open(QIODevice::ReadOnly) ? f.readAll() : QByteArray());
    }
}

QString CoreBridge::keepAsWebp(const QString &pathIn) {
    const QString path = localPath(pathIn);
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly)) return path;
    const QByteArray data = f.readAll();
    f.close();
    if (path.endsWith(".webp", Qt::CaseInsensitive) && data.size() < 600 * 1024) return path;    // already small
    QImage img = QImage::fromData(data);
    if (img.isNull()) {
        int w = 0, h = 0; size_t len = 0;
        uchar *px = rh_decode_webp(reinterpret_cast<const uchar *>(data.constData()), size_t(data.size()), &w, &h, &len);
        if (px) { img = QImage(px, w, h, w * 4, QImage::Format_RGBA8888).copy(); rh_free_pixels(px, len); }
    }
    if (img.isNull()) return path;
    QString dir;
    if (char *d = rh_data_dir()) { dir = QString::fromUtf8(d) + "/pictures/custom"; rh_free_str(d); }
    if (dir.isEmpty()) return path;
    QDir().mkpath(dir);
    const QString out = dir + "/" + QString::fromLatin1(QCryptographicHash::hash(data, QCryptographicHash::Md5).toHex()) + ".webp";
    if (QFileInfo::exists(out)) return out;
    QImageWriter w(out, "webp");
    w.setQuality(90);
    if (!w.write(img)) { qWarning() << "picture not kept as WebP:" << path << w.errorString(); return path; }
    return out;
}

QString CoreBridge::localPath(const QString &url) {
    const QUrl u(url);
    return u.isLocalFile() ? u.toLocalFile() : url;
}

QString CoreBridge::importFile(const QString &url, const QString &dir) {
    const QUrl u(url);
    if (u.isLocalFile()) return u.toLocalFile();
    if (!url.startsWith(QLatin1String("content:"))) return url;
    QFile in(url);
    if (!in.open(QIODevice::ReadOnly)) { qWarning() << "[import] could not open" << url << in.errorString(); return QString(); }
    const QString base = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/" + dir;
    QDir().mkpath(base);
    // the document's own name (Android gives its display name), else one made from the address
    QString name = QFileInfo(url).fileName();
    static const QString bad = QStringLiteral("/\\:*?\"<>|");
    for (QChar &ch : name) if (bad.contains(ch) || ch.unicode() < 32) ch = '_';
    if (name.isEmpty() || name.startsWith('.') || name.contains(QLatin1String("%2F")))
        name = "file-" + QString::fromLatin1(QCryptographicHash::hash(url.toUtf8(), QCryptographicHash::Md5).toHex().left(12));
    const QString out = base + "/" + name;
    if (QFileInfo(out).size() > 0 && QFileInfo(out).size() == in.size()) return out;
    QFile o(out + ".part");
    if (!o.open(QIODevice::WriteOnly | QIODevice::Truncate)) { qWarning() << "[import] could not write" << out; return QString(); }
    QByteArray buf;
    while (!(buf = in.read(1 << 20)).isEmpty()) {
        if (o.write(buf) != buf.size()) { o.remove(); qWarning() << "[import] copy failed" << out; return QString(); }
    }
    o.close();
    QFile::remove(out);
    if (!QFile::rename(out + ".part", out)) return QString();
    qInfo().noquote() << "[import]" << url << "->" << out;
    return out;
}

QVariantMap CoreBridge::cmd(const QVariantMap &c) {
    if (!m_b) return {{"ok", false}};
    const QByteArray j = QJsonDocument(QJsonObject::fromVariantMap(c)).toJson(QJsonDocument::Compact);
    const QVariantMap r = parseMap(rh_cmd(m_b, j.constData()));
    // Some commands change a page without the core sending an event (closing a title page, a mark…): read again now.
    const QString n = c.value("cmd").toString();
    static const QStringList pageCmds = {"open_detail", "close_detail", "set_season", "toggle_favorite", "toggle_library",
                                         "play", "play_best", "resume", "close_player", "toggle_watched", "mark", "open_notice",
                                         "retry_mal", "set_my_status", "remove_my_status", "go_forward", "play_episode", "set_art"};
    static const QStringList genreCmds = {"open_genre", "genre_more", "close_genre"};
    static const QStringList dlCmds = {"dl_open", "dl_load_streams", "dl_start", "dl_close"};
    static const QStringList torrentCmds = {"remove_torrent", "clear_torrent_cache"};
    static const QStringList prefCmds = {"set_shortcut", "reset_shortcuts", "test_translator", "set_setting", "toggle_addon", "install_addon", "remove_addon", "anime_skip_logout",
                                         "mal_logout", "download_shaders", "backup_now", "restore_backup", "save_settings"};
    // Player settings first: "upscale_mode" would otherwise be taken for a manga command ("upscale_").
    static const QStringList playerPrefCmds = {"upscale", "upscale_mode", "karaoke_on", "karaoke_set_song_delay", "set_sub_layout", "video_scale"};
    if (playerPrefCmds.contains(n)) {
        m_player = fetch("player"); emit playerChanged();
        refreshPrefs();
    } else if (pageCmds.contains(n)) {
        m_detail = fetch("detail"); emit detailChanged();
        m_player = fetch("player"); emit playerChanged();
        if (n == "toggle_favorite" || n == "toggle_library" || n == "toggle_watched" || n == "mark") {
            m_home = fetch("home"); emit homeChanged();
            m_library = fetch("library"); emit libraryChanged();
            m_calendar = fetch("calendar"); emit calendarChanged();
        }
        if (n == "open_notice") { m_notices = fetch("notices"); emit noticesChanged(); }
    } else if (dlCmds.contains(n)) {
        m_dlpick = fetch("dlpick"); emit dlpickChanged();
        if (n == "dl_start") { m_tasks = fetch("tasks"); emit tasksChanged(); }
    } else if (n.startsWith("manga_") || n.startsWith("pagedb_") || n.startsWith("suwayomi_") || n.startsWith("novel_") || n == "page_text" || n == "translate_texts" || n.startsWith("upscale_")) {
        refreshManga();
        if (n.startsWith("suwayomi_") || n == "novel_install" || n == "novel_remove_plugin") refreshPrefs();
    } else if (n.startsWith("study_") || n.startsWith("ankiweb_")) {
        // playing a card's sounds or the volume change nothing in the Study state: no rebuild of every deck's
        // numbers on the GUI thread at each card shown
        static const QStringList studyQuiet = {"study_play_sounds", "study_stop_sounds", "study_set_volume", "study_play_clip"};
        if (!studyQuiet.contains(n)) { m_study = fetch("study"); emit studyChanged(); }
        if (n.startsWith("ankiweb_")) refreshPrefs();
    } else if (n == "extensions_reload" || n == "extension_toggle" || n.startsWith("cloud_")) {
        refreshPrefs();
    } else if (n == "set_mode") {
        refreshPrefs();
        refreshManga();
        // only opening Study needs its state again (leaving it, or the other modes, do not)
        if (c.value("mode").toString() == "study") { m_study = fetch("study"); emit studyChanged(); }
    } else if (n.startsWith("profile_")) {
        // the list at once; after a switch every page follows with the core's events (library, home, study…)
        m_profiles = fetch("profiles"); emit profilesChanged();
    } else if (torrentCmds.contains(n)) {
        refreshTorrents();
        m_detail = fetch("detail"); emit detailChanged();
    } else if (n == "cancel_task") {
        m_tasks = fetch("tasks"); emit tasksChanged();
    } else if (genreCmds.contains(n)) {
        m_genre = fetch("genre"); emit genreChanged();
    } else if (prefCmds.contains(n)) {
        refreshPrefs();
        if (n == "set_setting") {
            const QString path = c.value("path").toString();
            if (path.startsWith("general.manga_") || path.startsWith("general.novel_")) { refreshManga(); }
        }
        if (n == "toggle_addon" || n == "remove_addon") { m_home = fetch("home"); emit homeChanged(); }
    } else if (n == "clear_all_notices" || n == "check_new_episodes") {
        m_notices = fetch("notices"); emit noticesChanged();
    } else if (n == "load_calendar") {
        m_calendar = fetch("calendar"); emit calendarChanged();
    } else if (n == "request_thumb") {
        // The picture comes back as an event; nothing to read now.
    } else if (m_player.value("active").toBool()) {
        // A player command (track, speed, pause...): show its effect at once.
        m_player = fetch("player"); emit playerChanged();
    }
    return r;
}

// A handler that runs while the state is being handed out (a page changing, a button...) may send a command that wants the
// state again: that second read waits until the first is finished (it caused "Binding loop detected" and slowed the reader).
void CoreBridge::refreshManga() {
    if (m_inManga) {
        if (!m_mangaPending) { m_mangaPending = true; QTimer::singleShot(0, this, [this] { m_mangaPending = false; refreshManga(); }); }
        return;
    }
    m_inManga = true;
    m_manga = fetch("manga");
    emit mangaChanged();
    m_inManga = false;
}

void CoreBridge::tick() {
    if (!m_b) return;
#ifdef REELHOUSE_LIVE_AUDIO
    // the microphone and speaker of a Gemini Live call open and close with it
    LiveAudio::sync();
#endif
    // Game controller (GTK gamepad.rs): the presses since the last tick.
    if (char *raw = rh_pad_poll()) {
        const QString names = QString::fromUtf8(raw);
        rh_free_str(raw);
        for (const QString &n : names.split(',', Qt::SkipEmptyParts)) {
            if (n.startsWith('@')) {
                const QStringList f = n.mid(1).split('|');
                if (f.size() >= 3) emit padConnection(f[0] == "connected", f[1], f[2]);
            } else {
                emit padPressed(n);
            }
        }
    }
    ++m_tickCount;
    const QVariantMap r = parseMap(rh_poll(m_b));
    bool home = false, search = false, library = false, detail = false, player = false;
    bool notices = false, genre = false, calendar = false, prefs = false, opened = false;
    for (const QVariant &ev : r.value("events").toList()) {
        const QVariantMap e = ev.toMap();
        const QString n = e.value("e").toString();
        if (n == "home") home = true;
        else if (n == "search") search = true;
        else if (n == "library") library = true;
        else if (n == "detail" || n == "mal") detail = true;
        else if (n == "player") player = true;
        else if (n == "progress") home = true;
        else if (n == "notices") { notices = true; home = true; library = true; }
        else if (n == "genre") genre = true;
        else if (n == "calendar") calendar = true;
        else if (n == "open_detail") opened = true;
        else if (n == "dlpick") { m_dlpick = fetch("dlpick"); emit dlpickChanged(); }
        else if (n == "downloads") { refreshTorrents(); detail = true; }
        else if (n == "manga") { refreshManga(); }
        else if (n == "mode") { refreshManga(); prefs = true; }
        else if (n == "study") { m_study = fetch("study"); emit studyChanged(); }
        else if (n == "xray") { m_xray = fetch("xray"); emit xrayChanged(); }
        else if (n == "playback") {
            m_playback = QVariantMap{{"pos", e.value("pos")}, {"paused", e.value("paused")}};
            emit playbackEvent(m_playback);
        }
        // A real file URL (spaces, '#', '%' and drive letters of Windows paths escaped properly), not a hand-made "file://" string.
        else if (n == "thumb") emit thumb(e.value("t").toDouble(), QUrl::fromLocalFile(e.value("path").toString()).toString());
        else if (n == "raise") emit raiseWindow();
        // an update replaces the program: its helper waits for this process to end (controller_update.rs)
        else if (n == "quit") QTimer::singleShot(300, QCoreApplication::instance(), &QCoreApplication::quit);
        else if (n == "lens_frame") { QVariantMap f = e; f.remove("e"); emit lensFrame(f); }
        else if (n == "restored") { home = library = detail = notices = prefs = true; }
        else if (n == "profiles") { m_profiles = fetch("profiles"); emit profilesChanged(); }
        else if (n == "privacy") {
            QVariantMap m = e; m.remove("e");
            applyPrivacyProxy(m.value("proxy_port").toInt());
            if (m != m_privacy) { m_privacy = m; emit privacyChanged(); }
        }
        else if (n == "net") { QVariantMap m = e; m.remove("e"); if (m != m_net) { m_net = m; emit netChanged(); } }
        else if (n == "info") {
            m_infos.insert(e.value("kind").toString(), e.value("text").toString());
            emit infosChanged();
            prefs = true;
        }
        else if (n == "toast") emit toast(e.value("text").toString());
        else if (n == "banner") { QVariantMap b = e; b.remove("e"); emit banner(b); }
        else if (n == "open_url") QDesktopServices::openUrl(QUrl(e.value("url").toString()));
        // the core's lines for this log (the "[live]" lines of Gemini Live)
        else if (n == "log") qInfo().noquote() << e.value("text").toString();
    }
    // The MyAnimeList lookup offers "Try again" after 20 s: look again now and then while it runs.
    if (!detail && m_tickCount % 100 == 0 && m_detail.value("open").toBool() && m_detail.value("mal").toMap().value("loading").toBool()) detail = true;
    if (home) { m_home = fetch("home"); emit homeChanged(); }
    if (search) { m_search = fetch("search"); emit searchChanged(); }
    if (library || m_library.isEmpty()) { m_library = fetch("library"); emit libraryChanged(); calendar = true; }
    if (detail) { m_detail = fetch("detail"); emit detailChanged(); }
    if (notices) { m_notices = fetch("notices"); emit noticesChanged(); }
    if (genre) { m_genre = fetch("genre"); emit genreChanged(); }
    if (calendar) { m_calendar = fetch("calendar"); emit calendarChanged(); }
    if (prefs) refreshPrefs();
    // Background work (downloads, translations) twice a second; only told when it changed.
    if (m_tickCount % 10 == 0) {
        char *raw = rh_state(m_b, "tasks");
        const QByteArray bytes(raw ? raw : "");
        if (raw) rh_free_str(raw);
        if (bytes != m_tasksRaw) { m_tasksRaw = bytes; m_tasks = QJsonDocument::fromJson(bytes).object().toVariantMap(); emit tasksChanged(); }
    }
    if (opened) emit detailOpened();
    // The play position moves all the time: refresh it 4 times a second while something plays.
    if (player || (r.value("playing").toBool() && m_tickCount % 5 == 0)) { m_player = fetch("player"); emit playerChanged(); }
}

static QString localPath(const QString &f) {
    return f.startsWith("file:") ? QUrl(f).toLocalFile() : f;
}

QUrl CoreBridge::navBlipUrl(int kind) {
    static QUrl urls[2];
    QUrl &url = urls[kind == 1 ? 1 : 0];
    if (!url.isEmpty()) return url;
    size_t len = 0;
    unsigned char *p = rh_nav_blip(kind == 1 ? 1 : 0, &len);
    if (!p) return url;
    const QString path = QDir(QStandardPaths::writableLocation(QStandardPaths::TempLocation)).filePath(kind == 1 ? "reelhouse-select-blip.wav" : "reelhouse-nav-blip.wav");
    QFile f(path);
    if (f.open(QIODevice::WriteOnly) && f.write(reinterpret_cast<const char *>(p), qint64(len)) == qint64(len)) url = QUrl::fromLocalFile(path);
    rh_free_pixels(p, len);
    return url;
}

QVariantMap CoreBridge::imageInfo(const QString &file) {
    QImageReader r(localPath(file));
    r.setAutoTransform(true);
    const QSize sz = r.size();
    if (!sz.isValid()) return {};
    const bool turned = r.transformation() & QImageIOHandler::TransformationRotate90;
    return {{"w", turned ? sz.height() : sz.width()}, {"h", turned ? sz.width() : sz.height()}};
}

QVariantMap CoreBridge::squareAvatar(const QString &file) {
    QImageReader r(localPath(file));
    r.setAutoTransform(true);
    // A big photo is decoded smaller: 1024 px on its short side is plenty for a 256 px picture.
    const QSize sz = r.size();
    if (sz.isValid() && qMin(sz.width(), sz.height()) > 1024) {
        const double f = 1024.0 / qMin(sz.width(), sz.height());
        r.setScaledSize(QSize(qMax(1, qRound(sz.width() * f)), qMax(1, qRound(sz.height() * f))));
    }
    const QImage img = r.read();
    if (img.isNull()) return {{"ok", false}, {"error", "Could not read that image: " + r.errorString()}};
    // the middle square, at 256 x 256 (a small picture is scaled up, so every profile picture has the same size)
    const int side = qMin(img.width(), img.height());
    const QImage square = img.copy((img.width() - side) / 2, (img.height() - side) / 2, side, side)
                              .scaled(256, 256, Qt::IgnoreAspectRatio, Qt::SmoothTransformation)
                              .convertToFormat(QImage::Format_ARGB32);
    const QString dir = QStandardPaths::writableLocation(QStandardPaths::TempLocation);
    QDir().mkpath(dir);
    // the core deletes it after copying it; the name tells it which files it may delete
    const QString path = QDir(dir).filePath(QString("reelhouse-avatar-%1.png").arg(QDateTime::currentMSecsSinceEpoch()));
    if (!square.save(path, "PNG")) return {{"ok", false}, {"error", "Could not save the picture."}};
    return {{"ok", true}, {"path", path}};
}

QVariantMap CoreBridge::saveArt(const QString &file, const QString &which, const QString &dir, const QRectF &crop) {
    QImageReader r(localPath(file));
    r.setAutoTransform(true);
    QImage img = r.read();
    if (img.isNull()) {
        return {{"ok", false}, {"error", "Could not read that image: " + r.errorString()}};
    }
    const bool poster = which == "poster";
    const int tw = poster ? 600 : 1920, th = poster ? 900 : 1080;
    const double ratio = double(tw) / th;
    QRect cut;
    if (crop.isEmpty()) {
        // Automatic: cut the edges evenly to the right shape.
        const int iw = img.width(), ih = img.height();
        int cw = iw, ch = ih;
        if (double(iw) / ih > ratio) cw = qRound(ih * ratio); else ch = qRound(iw / ratio);
        cut = QRect((iw - cw) / 2, (ih - ch) / 2, qMax(1, cw), qMax(1, ch));
    } else {
        cut = crop.toAlignedRect().intersected(img.rect());
    }
    QImage out = img.copy(cut);
    if (out.width() > tw || out.height() > th) out = out.scaled(tw, th, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    QDir().mkpath(dir);
    const QString id = m_detail.value("meta").toMap().value("id").toString();
    QString safe;
    for (const QChar c : id) safe += (c.isLetterOrNumber() && c.unicode() < 128) ? c : QChar('_');
    // WebP keeps the pictures small on the device; PNG or JPEG when the WebP writer is not there.
    const bool webp = QImageWriter::supportedImageFormats().contains("webp");
    const bool png = out.hasAlphaChannel();
    const QString ext = webp ? "webp" : (png ? "png" : "jpg");
    const QString path = QDir(dir).filePath(QString("%1-%2-%3.%4").arg(safe, which).arg(QDateTime::currentSecsSinceEpoch()).arg(ext));
    QImageWriter w(path, webp ? "webp" : (png ? "png" : "jpg"));
    w.setQuality(webp ? 88 : (png ? -1 : 92));
    if (!w.write(out)) return {{"ok", false}, {"error", "Could not save the picture in " + dir + ": " + w.errorString()}};
    cmd({{"cmd", "set_art"}, {"which", which}, {"value", QDir::toNativeSeparators(path)}});
    return {{"ok", true}, {"error", ""}};
}

void CoreBridge::sampleMood(QObject *obj, const QString &tag) {
    auto *item = qobject_cast<QQuickItem *>(obj);
    if (!item) return;
    auto grab = item->grabToImage(QSize(40, 60));
    if (!grab) return;
    // The grab is ready on the next frame; the colours are computed in the Rust core (palette::mood).
    connect(grab.data(), &QQuickItemGrabResult::ready, this, [this, grab, tag] {
        QImage img = grab->image().convertToFormat(QImage::Format_RGB888);
        QByteArray px(img.width() * img.height() * 3, Qt::Uninitialized);
        for (int y = 0; y < img.height(); ++y) {
            const uchar *line = img.constScanLine(y);
            memcpy(px.data() + size_t(y) * img.width() * 3, line, size_t(img.width()) * 3);
        }
        unsigned char out[12] = {0};
        rh_mood(reinterpret_cast<const unsigned char *>(px.constData()), size_t(img.width() * img.height()), out);
        QStringList cols;
        for (int i = 0; i < 4; ++i)
            cols << QString("#%1%2%3").arg(out[i * 3], 2, 16, QChar('0')).arg(out[i * 3 + 1], 2, 16, QChar('0')).arg(out[i * 3 + 2], 2, 16, QChar('0'));
        emit moodReady(tag, cols);
    });
}

void CoreBridge::padConfig(const QVariantMap &c) {
    rh_pad_deadzone(float(c.value("deadzone", 0.3).toDouble()));
    const bool on = c.value("on").toBool();
    const QColor light(c.value("light").toString());
    const double k = qBound(0, c.value("brightness", 100).toInt(), 100) / 100.0;
    rh_ds4_config(light.isValid() ? (unsigned char)(light.red() * k) : 0, light.isValid() ? (unsigned char)(light.green() * k) : 0,
                  light.isValid() ? (unsigned char)(light.blue() * k) : 0, c.value("touchMouse", true).toBool() ? 1 : 0,
                  float(c.value("touchSpeed", 1.0).toDouble()));
    rh_ds4_enable(on ? 1 : 0);
    if (on && !m_hidTimer.isActive()) {
        m_hidTimer.setInterval(8);
        m_hidTimer.setTimerType(Qt::PreciseTimer);
        connect(&m_hidTimer, &QTimer::timeout, this, &CoreBridge::pollHid, Qt::UniqueConnection);
        m_hidTimer.start();
    } else if (!on && m_hidTimer.isActive()) {
        m_hidTimer.stop();
        if (!m_padHid.isEmpty()) { m_padHid.clear(); emit padHidChanged(); }
    }
}

// The touchpad of a PlayStation controller as a mouse of this app: one finger moves the system pointer, a press of the
// pad is a left click and two fingers turn the wheel, all sent to the window of the app under the pointer (or the one
// in front). Only while the app is in front.
void CoreBridge::pollHid() {
    float v[10] = {0};
    if (rh_ds4_poll(v, 10) < 10) return;
    QVariantMap st;
    if (v[0] > 0.5f) {
        st = {{"connected", true}, {"kind", v[1] > 4.5f ? "ps5" : "ps4"}, {"bluetooth", v[2] > 0.5f},
              {"battery", int(v[3])}, {"charging", v[4] > 0.5f}};
    }
    if (st != m_padHid) { m_padHid = st; emit padHidChanged(); }
    if (QGuiApplication::applicationState() != Qt::ApplicationActive) { m_hidRest = {}; m_hidWheel = 0; return; }
    m_hidRest += QPointF(v[5], v[6]);
    const QPoint step(int(m_hidRest.x()), int(m_hidRest.y()));
    m_hidRest -= QPointF(step);
    QPoint global = QCursor::pos();
    if (!step.isNull()) {
        global += step;
        QCursor::setPos(global);
    }
    const bool click = v[8] > 0.5f;
    m_hidWheel += v[7];
    int wheel = 0;
    if (std::abs(m_hidWheel) >= 8) {
        wheel = int(m_hidWheel);
        m_hidWheel -= wheel;
    }
#ifdef Q_OS_WIN
    // Windows: a real click and wheel turn of the system (SendInput), like a mouse. HP: the clicks posted to the Qt
    // window did not reach everything (the web pages of Chromium and the video are native windows of their own).
    if (click != m_hidClick) {
        m_hidClick = click;
        INPUT in = {};
        in.type = INPUT_MOUSE;
        in.mi.dwFlags = click ? MOUSEEVENTF_LEFTDOWN : MOUSEEVENTF_LEFTUP;
        SendInput(1, &in, sizeof(INPUT));
    }
    if (wheel != 0) {
        INPUT in = {};
        in.type = INPUT_MOUSE;
        in.mi.dwFlags = MOUSEEVENTF_WHEEL;
        in.mi.mouseData = DWORD(wheel);   // eighths of a degree, the same unit as WHEEL_DELTA (120 a notch)
        SendInput(1, &in, sizeof(INPUT));
    }
#else
    QWindow *w = QGuiApplication::focusWindow();
    if (!w) return;
    const QPointF local = w->mapFromGlobal(global);
    if (click != m_hidClick) {
        m_hidClick = click;
        QGuiApplication::postEvent(w, new QMouseEvent(click ? QEvent::MouseButtonPress : QEvent::MouseButtonRelease, local, local, QPointF(global),
                                                      Qt::LeftButton, click ? Qt::LeftButton : Qt::NoButton, Qt::NoModifier));
    }
    if (wheel != 0)
        QGuiApplication::postEvent(w, new QWheelEvent(local, QPointF(global), QPoint(0, wheel), QPoint(0, wheel * 3), Qt::NoButton, Qt::NoModifier,
                                                      Qt::NoScrollPhase, false));
#endif
}

void CoreBridge::sendKey(QObject *obj, int key) {
    auto *w = qobject_cast<QWindow *>(obj);
    if (!w) return;
    QCoreApplication::postEvent(w, new QKeyEvent(QEvent::KeyPress, key, Qt::NoModifier));
    QCoreApplication::postEvent(w, new QKeyEvent(QEvent::KeyRelease, key, Qt::NoModifier));
}

QString CoreBridge::uiLang() const {
    // The core resolves it: "UI language" when one is picked, else the main language of "Languages I know"
    // (when the interface has it), else the system's (REELHOUSE_SYSTEM_LANG, set in main.cpp).
    const QString r = m_prefs.value("uiLang").toString();
    if (!r.isEmpty()) return r;
    const QString l = m_prefs.value("general").toMap().value("language").toString();
    if (l.isEmpty() || l == "system") return QLocale::system().name();
    return l;
}

QString CoreBridge::tr(const QString &lang, const QString &text) {
    if (lang.isEmpty() || lang == "en" || text.isEmpty()) return text;
    const QString key = lang + '\x1f' + text;
    auto it = m_trCache.constFind(key);
    if (it != m_trCache.constEnd()) return it.value();
    const QByteArray l = lang.toUtf8(), t = text.toUtf8();
    char *raw = rh_tr(l.constData(), t.constData());
    const QString out = raw ? QString::fromUtf8(raw) : text;
    if (raw) rh_free_str(raw);
    m_trCache.insert(key, out);
    return out;
}

QString CoreBridge::webCookies(const QString &url) {
#ifdef Q_OS_ANDROID
    QJniObject cm = QJniObject::callStaticObjectMethod("android/webkit/CookieManager", "getInstance", "()Landroid/webkit/CookieManager;");
    if (!cm.isValid()) return QString();
    cm.callMethod<void>("flush");
    QJniObject c = cm.callObjectMethod("getCookie", "(Ljava/lang/String;)Ljava/lang/String;", QJniObject::fromString(url).object<jstring>());
    return c.isValid() ? c.toString() : QString();
#else
    Q_UNUSED(url);
    return QString();
#endif
}

QString CoreBridge::webUserAgent() {
#ifdef Q_OS_ANDROID
    QJniObject ctx = QNativeInterface::QAndroidApplication::context();
    QJniObject ua = QJniObject::callStaticObjectMethod("android/webkit/WebSettings", "getDefaultUserAgent", "(Landroid/content/Context;)Ljava/lang/String;", ctx.object());
    return ua.isValid() ? ua.toString() : QString();
#else
    return QString();
#endif
}

// IP protection (src/privacy.rs): every QNetworkAccessManager of the app (pictures, pages) uses the application
// proxy, so with the protection on they go through the core's local gateway as SOCKS5 (names resolved there, no DNS
// from here). Qt never proxies localhost, so the core's own servers are reached directly.
void CoreBridge::applyPrivacyProxy(int port) {
    if (port == m_privacyPort) return;
    m_privacyPort = port;
    if (port > 0) QNetworkProxy::setApplicationProxy(QNetworkProxy(QNetworkProxy::Socks5Proxy, QStringLiteral("127.0.0.1"), quint16(port)));
    else QNetworkProxy::setApplicationProxy(QNetworkProxy(QNetworkProxy::NoProxy));
    qInfo().noquote() << "[privacy] Qt proxy:" << (port > 0 ? QStringLiteral("socks5://127.0.0.1:%1").arg(port) : QStringLiteral("none"));
}
