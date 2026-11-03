#pragma once
#include <QObject>
#include <QUrl>
#include <QTimer>
#include <QPointF>
#include "bridge.h"
#include <QVariantMap>
#include <QRectF>
#include <QPointer>

#include <QQuickTextDocument>

struct Bridge;
class VideoItem;

// The Rust core as a QObject: pages read `home`, `search`, `library`, `detail`, `player` and call `cmd({...})`.
class CoreBridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap home READ home NOTIFY homeChanged)
    Q_PROPERTY(QVariantMap search READ search NOTIFY searchChanged)
    Q_PROPERTY(QVariantMap library READ library NOTIFY libraryChanged)
    Q_PROPERTY(QVariantMap detail READ detail NOTIFY detailChanged)
    Q_PROPERTY(QVariantMap player READ player NOTIFY playerChanged)
    // X-Ray of the video playing: {status, data: {items: [{type, start_time, end_time, name, role, voice, image, text, tag}], sources, notes}, pos, paused}
    Q_PROPERTY(QVariantMap xray READ xray NOTIFY xrayChanged)
    // the playback clock observed from mpv: {pos, paused}
    Q_PROPERTY(QVariantMap playback READ playback NOTIFY playbackEvent)
    Q_PROPERTY(QVariantMap settings READ settings NOTIFY settingsChanged)
    Q_PROPERTY(QVariantMap notices READ notices NOTIFY noticesChanged)
    Q_PROPERTY(QVariantMap genre READ genre NOTIFY genreChanged)
    Q_PROPERTY(QVariantMap calendar READ calendar NOTIFY calendarChanged)
    Q_PROPERTY(QVariantMap prefs READ prefs NOTIFY prefsChanged)
    Q_PROPERTY(QVariantMap infos READ infos NOTIFY infosChanged)
    Q_PROPERTY(QVariantMap tasks READ tasks NOTIFY tasksChanged)
    Q_PROPERTY(QVariantMap study READ study NOTIFY studyChanged)
    Q_PROPERTY(QVariantMap dlpick READ dlpick NOTIFY dlpickChanged)
    Q_PROPERTY(QVariantMap torrents READ torrents NOTIFY torrentsChanged)
    Q_PROPERTY(QVariantMap manga READ manga NOTIFY mangaChanged)
    // The people who use the app on this computer: {active, presets, max, list: [{id, name, preset, photo, main, active}]}.
    Q_PROPERTY(QVariantMap profiles READ profiles NOTIFY profilesChanged)
    // the connection: {online, network, forced} (src/connectivity.rs); offline, only what is on disk opens
    Q_PROPERTY(QVariantMap net READ net NOTIFY netChanged)
    // IP protection (src/privacy.rs): {mode, phase, message, exit_ip, progress, proxy_port, proxy_url, wg_name...}
    Q_PROPERTY(QVariantMap privacy READ privacy NOTIFY privacyChanged)
    // The language of the menus and buttons ("en", "pt-BR", "es"): the preference, or the system's when it says so.
    Q_PROPERTY(QString uiLang READ uiLang NOTIFY uiLangChanged)
    // "Reload the covers": the generation pictures are asked with, and how far the reload is ({active, asked, done}).
    Q_PROPERTY(int picGen READ picGen NOTIFY picReloadChanged)
    Q_PROPERTY(QVariantMap picReload READ picReload NOTIFY picReloadChanged)
    // The PlayStation controller read over HID: {connected, kind: "ps4"|"ps5", bluetooth, battery (-1 unknown), charging}
    Q_PROPERTY(QVariantMap padHid READ padHid NOTIFY padHidChanged)
public:
    explicit CoreBridge(QObject *parent = nullptr);
    ~CoreBridge() override;
    static CoreBridge *instance();
    static Bridge *handle();
    void setVideoItem(VideoItem *v);
    void frameFromMpv();  // called from mpv's thread; the item is reached on the interface thread

    QVariantMap home() const { return m_home; }
    QVariantMap search() const { return m_search; }
    QVariantMap library() const { return m_library; }
    QVariantMap detail() const { return m_detail; }
    QVariantMap player() const { return m_player; }
    QVariantMap xray() const { return m_xray; }
    QVariantMap playback() const { return m_playback; }
    QVariantMap settings() const { return m_settings; }
    QVariantMap notices() const { return m_notices; }
    QVariantMap profiles() const { return m_profiles; }
    QVariantMap net() const { return m_net; }
    QVariantMap privacy() const { return m_privacy; }
    QVariantMap genre() const { return m_genre; }
    QVariantMap calendar() const { return m_calendar; }
    QVariantMap prefs() const { return m_prefs; }
    QVariantMap infos() const { return m_infos; }
    QVariantMap tasks() const { return m_tasks; }
    QVariantMap study() const { return m_study; }
    QVariantMap dlpick() const { return m_dlpick; }
    QVariantMap torrents() const { return m_torrents; }
    QVariantMap manga() const { return m_manga; }
    Q_INVOKABLE void refreshTorrents();
    int picGen() const { return m_picGen; }
    QVariantMap picReload() const { return m_picReload; }
    // Throws away the pictures kept on the device and loads every picture on screen again, counting them.
    Q_INVOKABLE void reloadPictures();
    QString uiLang() const;
    // A text of the interface in `lang` (GTK i18n table, the core's). Bindings call it as Core.tr(Core.uiLang, "Play"),
    // so they follow a change of language at once.
    Q_INVOKABLE QString tr(const QString &lang, const QString &text);

    Q_INVOKABLE QVariantMap cmd(const QVariantMap &c);
    // A state read once (lists that never change, such as the categories).
    Q_INVOKABLE QVariantMap state(const QString &name);
    Q_INVOKABLE void refreshPrefs();
    // "Cover and background": size of a local image file ({w, h}, empty when unreadable).
    Q_INVOKABLE QVariantMap imageInfo(const QString &file);
    // A made-up interface sound (kind 0 = focus moved, 1 = select), made by the core and written once as a temporary
    // WAV ("" when it cannot).
    Q_INVOKABLE QUrl navBlipUrl(int kind);
    // Cuts `crop` (image pixels; an empty rectangle = centred automatic crop) to the shape of a cover (2:3, up to
    // 600x900) or a wallpaper (16:9, up to 1920x1080), saves it in `dir` and makes it the picture of the title.
    // Returns {ok, error}.
    Q_INVOKABLE QVariantMap saveArt(const QString &file, const QString &which, const QString &dir, const QRectF &crop);
    // A photo for a profile: the middle square of the picture at 256 x 256, written as a temporary PNG that the core
    // then keeps in the profile's folder (`profile_avatar` / `profile_add` with `photo`). Returns {ok, path, error}.
    Q_INVOKABLE QVariantMap squareAvatar(const QString &file);
    // The four main colours of a picture (moving background). `item` is a picture already drawn on screen: it is
    // read at 40x60 and the colours come back in moodReady(tag, colours) as "#rrggbb" strings.
    Q_INVOKABLE void sampleMood(QObject *item, const QString &tag);
    // A key press sent to the window (the game controller maps its buttons onto keys outside the player).
    Q_INVOKABLE void sendKey(QObject *window, int key);
    // The reader's text: the page is fetched here, cut into the pieces the OCR reads, and sent to the core (the result comes as state).
    Q_INVOKABLE void readPage(const QString &url);
    // A file path from a file: address (the path itself when it is already one).
    Q_INVOKABLE QString localPath(const QString &url);
    // A file picked in a dialog as a path the core and mpv can open: a local file as it is; an Android content://
    // document is copied into <app data>/<dir>/ first (neither can read those). Empty when the copy failed.
    Q_INVOKABLE QString importFile(const QString &url, const QString &dir);
    // The in-app browser (Android System WebView): its cookies for a page and its User-Agent, kept by the core after
    // a site's bot check so the text sources can read it (empty where there is no such browser).
    Q_INVOKABLE QString webCookies(const QString &url);
    Q_INVOKABLE QString webUserAgent();
    // A local path as a file URL (spaces, # and drive letters escaped).
    Q_INVOKABLE QString fileUrl(const QString &path) { return QUrl::fromLocalFile(path).toString(); }
    // ---- page editor (Images mode) ----
    // The edits of a page (JSON array, as kept in the chapter's .md); the picture comes from image://edited/.
    Q_INVOKABLE void setEditOps(const QString &url, const QString &opsJson);
    Q_INVOKABLE QString pickColor(const QString &url, double x, double y);
    Q_INVOKABLE QRectF cropOf(const QString &opsJson);
    // Cuts the rectangle `r` (0..1 of the original page) out of the page `url`, with its picture edits, on another
    // thread (the page may still have to be downloaded) and writes it as a PNG in the temporary folder. The answer comes
    // with pageCut(tag, path, error).
    Q_INVOKABLE void cutPage(const QString &url, const QRectF &r, const QString &tag);
    // ---- bubble text with formatting ----
    // kind: bold, italic, underline, strike, highlight (value = colour or "" to remove), color (value = colour), clear
    Q_INVOKABLE void richFormat(QQuickTextDocument *doc, int start, int end, const QString &kind, const QString &value);
    // The text of a rich text editor reduced to the tags a bubble keeps (b, i, u, s, mark, span colour, br).
    Q_INVOKABLE QString richSimple(QQuickTextDocument *doc);
    // Which formats the text at `pos` has: {bold, italic, underline, strike, highlight}
    Q_INVOKABLE QVariantMap richState(QQuickTextDocument *doc, int pos);
    // The highlighted words of a text edit as rectangles, one per line ({ox, oy, pills: [{x, y, w, h, c}]}, document
    // coordinates; ox/oy is where position 0 is), so they can be drawn with round ends behind the text.
    Q_INVOKABLE QVariantMap richPills(QQuickTextDocument *doc);
    // A picture the person chose (cover, wallpaper) is kept on the device as WebP; returns the file to use.
    Q_INVOKABLE QString keepAsWebp(const QString &path);
    // A banner born in QML (the Pomodoro timer…): {kind, title, body, mood, action}; shown like the core's banners.
    Q_INVOKABLE void showBanner(const QVariantMap &b) { emit banner(b); }
    // The macaw comes in at a corner of the window: {cue: shh|peek|cheer|wave|happy|sad|think|hide, corner, ms, text}
    // (MascotStage.qml shows it; docs/mascot-contract.md).
    Q_INVOKABLE void mascotCue(const QVariantMap &c) { emit mascotCued(c); }
    // Controller settings (Main.qml, from Preferences > Controller): {on (controller mode), deadzone (0-1), light
    // (colour, "" = off), brightness (0-100), touchMouse, touchSpeed (1 = normal)}. While `on`, PlayStation
    // controllers are read over HID: the touchpad moves the pointer of this app's window that is in front.
    Q_INVOKABLE void padConfig(const QVariantMap &c);
    // A short vibration of a PlayStation controller (0-255 each motor).
    Q_INVOKABLE void padRumble(int weak, int strong, int ms) { rh_ds4_rumble((unsigned char)qBound(0, weak, 255), (unsigned char)qBound(0, strong, 255), (unsigned)qMax(0, ms)); }
    QVariantMap padHid() const { return m_padHid; }
signals:
    void moodReady(const QString &tag, const QStringList &colours);
    // A game controller button was pressed: "A", "B", "Start", "Left"...; "~Name" a release; "RS:<y>" / "RSX:<x>" the
    // right stick (-1..1, 0 in the deadzone; see rh_pad_poll)
    void padPressed(const QString &button);
    // A controller plugged in or out: kind "xbox", "ps4", "ps5" or "pad", and the name the driver gives.
    void padConnection(bool connected, const QString &kind, const QString &name);
    void padHidChanged();
    void homeChanged();
    void searchChanged();
    void libraryChanged();
    void detailChanged();
    void playerChanged();
    void xrayChanged();
    // emitted when mpv reports a new position (at most 4 times a second) or a pause / play
    void playbackEvent(const QVariantMap &state);
    void settingsChanged();
    void noticesChanged();
    void genreChanged();
    void calendarChanged();
    void prefsChanged();
    // Only when the language of the interface really changed (every Core.tr binding is evaluated again).
    void uiLangChanged();
    void infosChanged();
    void tasksChanged();
    void studyChanged();
    void dlpickChanged();
    void torrentsChanged();
    void mangaChanged();
    void profilesChanged();
    void netChanged();
    void privacyChanged();
    void picReloadChanged();
    // A preview picture of the video at `t` seconds is ready (the seek bar).
    // `t`: time of the picture's slot; `url`: file URL of the JPEG.
    void thumb(double t, const QString &url);
    // A title page was opened by the core (a card found in the catalog, a notification…).
    void detailOpened();
    void raiseWindow();
    // The frame of browser_grab_frame_async (the quick search's Google Lens): {token, frame, error, ms}.
    void lensFrame(const QVariantMap &f);
    void toast(const QString &text);
    // A phone-style pop-up with the macaw: {kind, title, body, mood, action} (docs/mascot-contract.md).
    void banner(const QVariantMap &b);
    // A cue for the macaw of MascotStage.qml (from mascotCue).
    void mascotCued(const QVariantMap &c);
    // The answer of cutPage: the PNG written, or an error (path empty).
    void pageCut(const QString &tag, const QString &path, const QString &error);
    void openUrl(const QString &url);
private slots:
    void tick();
private:
    QVariantMap fetch(const char *name);
    Bridge *m_b = nullptr;
    QTimer m_timer;
    // Guarded: the QML engine (and the item) goes away before this object does.
    QPointer<VideoItem> m_video;
    QString m_lastUiLang;
    bool m_uiLangSent = false;
    QHash<QString, QString> m_trCache;
    void refreshManga();
    bool m_inManga = false, m_mangaPending = false;
    QVariantMap m_home, m_search, m_library, m_detail, m_player, m_settings, m_notices, m_genre, m_calendar, m_prefs, m_infos, m_tasks, m_dlpick, m_torrents, m_manga, m_study, m_xray, m_playback;
    QVariantMap m_profiles;
    QVariantMap m_net{{"online", true}, {"network", true}, {"forced", false}};
    QVariantMap m_privacy;
    int m_privacyPort = -1;
    void applyPrivacyProxy(int port);
    QByteArray m_tasksRaw;
    int m_tickCount = 0;
    int m_picGen = 0;
    QVariantMap m_picReload;
    QTimer m_picTimer;
    int m_picLastDone = -1, m_picQuiet = 0;
    // PlayStation controller over HID: polled every 8 ms while controller mode is on
    void pollHid();
    QTimer m_hidTimer;
    QVariantMap m_padHid;
    QPointF m_hidRest;          // the fractions of pixels not moved yet
    bool m_hidClick = false;
    double m_hidWheel = 0;
    int m_hidIdle = 0;
};
