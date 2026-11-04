#include <QLocale>
#include <QWindow>
#include <QImageWriter>
#include <QImageReader>
#include <QGuiApplication>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QtQml/qqml.h>
#include <QQuickWindow>
#include <QQuickStyle>
#include <QSGRendererInterface>
#include <QSurfaceFormat>
#include <QNetworkAccessManager>
#include <QNetworkDiskCache>
#include <QQmlNetworkAccessManagerFactory>
#include <QStandardPaths>
#include <QFile>
#include <QFileInfo>
#include <QDir>
#include <QTextStream>
#include <QDateTime>
#include <QTimer>
#include <QPluginLoader>
#include <QInputMethod>
#ifdef REELHOUSE_WEBVIEW
#include <QtWebView/QtWebView>
#endif
#include <QKeyEvent>
#include <clocale>
#include <cstdio>
#if defined(__GLIBC__)
#include <malloc.h>
#endif
#include "corebridge.h"
#include "thememanager.h"
#include "videoitem.h"
#include "themevideo.h"
#include "imageprovider.h"
#include "editedpage.h"
#include "webpane.h"
#include "platform.h"
#include "orbsitem.h"
#include "bridge.h"
#ifdef REELHOUSE_VOICE_INPUT
#include "voiceinput.h"
#endif
#ifdef REELHOUSE_TTS
#include <QTextToSpeech>
#endif

#ifdef Q_OS_WIN
#include <windows.h>
#include <cstdio>
// A hard crash inside a native part (graphics driver, mpv) leaves no message: write its code, where it happened and the
// last step the core announced to crash.txt in the data folder (like the GTK version).
static char g_crashPath[1024] = {0};
static LONG WINAPI onCrash(EXCEPTION_POINTERS *p) {
    if (g_crashPath[0] && p && p->ExceptionRecord) {
        if (FILE *f = std::fopen(g_crashPath, "w")) {
            char *step = rh_last_step();
            std::fprintf(f, "crash: code 0x%08lx at %p\nlast step: %s\n", (unsigned long)p->ExceptionRecord->ExceptionCode, p->ExceptionRecord->ExceptionAddress, step ? step : "");
            if (step) rh_free_str(step);
            std::fclose(f);
        }
    }
    return EXCEPTION_CONTINUE_SEARCH;
}
#endif

// Any platform: an abort (Rust out of memory, qFatal) or a segfault that the Windows filter above never sees.
#include <csignal>
#include <exception>
static char g_sigPath[1024] = {0};
static void onSignal(int sig) {
    if (g_sigPath[0]) {
        if (FILE *f = std::fopen(g_sigPath, "a")) {
            std::fprintf(f, "signal %d (%s)\n", sig, sig == SIGABRT ? "abort" : sig == SIGSEGV ? "segfault" : sig == SIGFPE ? "arithmetic" : sig == SIGILL ? "illegal instruction" : "other");
            std::fclose(f);
        }
    }
    std::signal(sig, SIG_DFL);
    std::raise(sig);
}

#ifdef Q_OS_ANDROID
// Android's keyboards type a word as "composing" text and hand it over only when the word ends. Enter in a text field
// came before that hand-over, so the field's onAccepted saw the text without the word, and the keyboard then put the
// word back at the wrong place (typing "joj" + Enter gave "ojoj"). The word is handed over first, then Enter goes on.
class CommitBeforeEnter : public QObject {
public:
    using QObject::QObject;
    bool eventFilter(QObject *o, QEvent *e) override {
        if (e->type() == QEvent::KeyPress) {
            const int k = static_cast<QKeyEvent *>(e)->key();
            if ((k == Qt::Key_Return || k == Qt::Key_Enter) && QGuiApplication::inputMethod()->isVisible())
                QGuiApplication::inputMethod()->commit();
        }
        return QObject::eventFilter(o, e);
    }
};
#endif

static QFile *g_log = nullptr;
static void logHandler(QtMsgType, const QMessageLogContext &, const QString &msg) {
    if (!g_log) return;
    QTextStream(g_log) << QDateTime::currentDateTime().toString("hh:mm:ss.zzz ") << msg << "\n";
    g_log->flush();
}

int main(int argc, char *argv[]) {
    static QFile logFile;
    // Every log of the app goes to one folder, `debug/` next to the program, so it is easy to send them all.
    QString exeDir = ".";
    {
        QString exe = QString::fromLocal8Bit(argv[0]);
        int i = qMax(exe.lastIndexOf('\\'), exe.lastIndexOf('/'));
        if (i > 0) exeDir = exe.left(i);
    }
#ifdef REELHOUSE_MOBILE
    // A phone cannot write next to the program: the data, the logs and the temporary files go to the app's own
    // folders (Android: /data/data/<package>/files; iOS: the app container). The core reads REELHOUSE_DATA first.
    QCoreApplication::setApplicationName("Reelhouse");
    QCoreApplication::setOrganizationName("Reelhouse");
    const QString appData = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    QDir().mkpath(appData);
    if (!qEnvironmentVariableIsSet("REELHOUSE_DATA")) qputenv("REELHOUSE_DATA", appData.toUtf8());
    // The course packs and kana sounds come inside the app (:/bundled, see CMakeLists.txt): copied to the data
    // folder, where the core looks for learn/ and kana/, when missing or changed (a new version of the app).
    for (const char *sub : {"learn", "kana"}) {
        const QString from = QStringLiteral(":/bundled/") + sub, to = appData + '/' + sub;
        QDir().mkpath(to);
        for (const QFileInfo &fi : QDir(from).entryInfoList(QDir::Files)) {
            const QString dst = to + '/' + fi.fileName();
            if (QFileInfo(dst).size() == fi.size()) continue;
            QFile::remove(dst);
            if (QFile::copy(fi.filePath(), dst)) QFile::setPermissions(dst, QFile::ReadOwner | QFile::WriteOwner);
        }
    }
    // The system language for the core (Windows, Android and iOS have no LANG): "UI language > System default".
    if (!qEnvironmentVariableIsSet("REELHOUSE_SYSTEM_LANG")) qputenv("REELHOUSE_SYSTEM_LANG", QLocale::system().name().toUtf8());
    if (!qEnvironmentVariableIsSet("TMPDIR")) {
        const QString tmp = QStandardPaths::writableLocation(QStandardPaths::TempLocation);
        QDir().mkpath(tmp);
        qputenv("TMPDIR", tmp.toUtf8());
    }
#ifdef Q_OS_ANDROID
    // Subtitles (and the karaoke lyrics, a second ASS track) are drawn by libass inside mpv-android's libmpv, which
    // finds fonts through fontconfig. mpv hands libass a fonts.conf only from its config folder, and the player runs
    // with config=no, so fontconfig fell back to a path of the build machine: no fonts at all, and every subtitle drawn
    // by mpv stayed invisible. A fonts.conf of our own (the same one the mpv-android app writes) points it at the
    // phone's fonts; fontconfig reads FONTCONFIG_FILE when libass starts, long after this.
    if (!qEnvironmentVariableIsSet("FONTCONFIG_FILE")) {
        const QString cache = QStandardPaths::writableLocation(QStandardPaths::CacheLocation) + QStringLiteral("/fontconfig");
        QDir().mkpath(cache);
        const QString conf = appData + QStringLiteral("/fonts.conf");
        const QString want = QStringLiteral(
            "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"fonts.dtd\">\n<fontconfig>\n"
            "  <dir>/system/fonts</dir>\n  <dir>/product/fonts</dir>\n  <dir>/system/product/fonts</dir>\n"
            "  <cachedir>%1</cachedir>\n"
            "  <alias><family>serif</family><prefer><family>Noto Serif</family><family>Noto Serif CJK JP</family></prefer></alias>\n"
            "  <alias><family>sans-serif</family><prefer><family>Roboto</family><family>Noto Sans</family><family>Noto Sans CJK JP</family></prefer></alias>\n"
            "  <alias><family>Sans</family><prefer><family>Roboto</family><family>Noto Sans</family><family>Noto Sans CJK JP</family></prefer></alias>\n"
            "  <alias><family>monospace</family><prefer><family>Droid Sans Mono</family><family>Cutive Mono</family></prefer></alias>\n"
            "  <alias><family>Arial</family><accept><family>Roboto</family></accept></alias>\n"
            "  <alias><family>Helvetica</family><accept><family>Roboto</family></accept></alias>\n"
            "</fontconfig>\n").arg(cache.toHtmlEscaped());
        QFile f(conf);
        bool same = false;
        if (f.open(QIODevice::ReadOnly)) { same = QString::fromUtf8(f.readAll()) == want; f.close(); }
        if (!same && f.open(QIODevice::WriteOnly | QIODevice::Truncate)) { f.write(want.toUtf8()); f.close(); }
        qputenv("FONTCONFIG_FILE", conf.toUtf8());
    }
#endif
    exeDir = appData;
#elif !defined(Q_OS_WIN)
    // An installed copy (/opt/reelhouse from the .deb / .rpm, ReelHouse.app from the .dmg) sits in a folder the user
    // cannot write, or in a signed bundle that should stay as it is: its logs go to the core's data folder instead
    // (REELHOUSE_DATA, else $XDG_CONFIG_HOME/reelhouse, else ~/.config/reelhouse, as util::data_dir does).
    {
        QString realDir = exeDir;
#ifdef Q_OS_LINUX
        // started through the /usr/bin/reelhouse link or $PATH, argv[0] does not say where the program is
        const QString self = QFile::symLinkTarget("/proc/self/exe");
        if (!self.isEmpty()) realDir = QFileInfo(self).absolutePath();
#endif
        if (realDir.endsWith("/Contents/MacOS") || !QFileInfo(realDir).isWritable()) {
            QString data = qEnvironmentVariable("REELHOUSE_DATA");
            if (data.isEmpty()) {
                const QString xdg = qEnvironmentVariable("XDG_CONFIG_HOME");
                data = (xdg.isEmpty() ? QDir::homePath() + "/.config" : xdg) + "/reelhouse";
            }
            exeDir = data;
        }
    }
#endif
    const QString debugDir = exeDir + "/debug";
    QDir().mkpath(debugDir);
    {
        // The log of the run before stays as reelhouse-qt.old.log.
        const QString cur = debugDir + "/reelhouse-qt.log", old = debugDir + "/reelhouse-qt.old.log";
        QFile::remove(old);
        QFile::rename(cur, old);
        logFile.setFileName(cur);
    }
    if (logFile.open(QIODevice::WriteOnly | QIODevice::Truncate | QIODevice::Text)) { g_log = &logFile; qInstallMessageHandler(logHandler); }
    const QByteArray debugDirUtf8 = debugDir.toUtf8();
    rh_set_debug_dir(debugDirUtf8.constData());

    std::snprintf(g_sigPath, sizeof g_sigPath, "%s/crash-signal.txt", debugDir.toLocal8Bit().constData());
    for (int sg : {SIGABRT, SIGSEGV, SIGFPE, SIGILL}) std::signal(sg, onSignal);
    std::set_terminate([] {
        if (g_sigPath[0]) if (FILE *f = std::fopen(g_sigPath, "a")) { std::fputs("terminate: an uncaught C++ exception\n", f); std::fclose(f); }
        std::abort();
    });
#ifdef Q_OS_WIN
    std::snprintf(g_crashPath, sizeof g_crashPath, "%s\\crash.txt", QDir::toNativeSeparators(debugDir).toLocal8Bit().constData());
    SetUnhandledExceptionFilter(onCrash);
#endif
    // mpv draws with OpenGL, so Qt Quick runs on its OpenGL backend (Direct3D 11 is Qt's default on Windows, Metal on
    // iOS; on Android it is OpenGL ES already).
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
#ifdef Q_OS_LINUX
    // Uniform look with GTK/libadwaita (Arch wiki, "Uniform look for Qt and GTK"): on GNOME let Qt ask the desktop portal
    // for colours, accent and the native file dialogs, unless the user already chose a platform theme.
    if (!qEnvironmentVariableIsSet("QT_QPA_PLATFORMTHEME") && qgetenv("XDG_CURRENT_DESKTOP").toLower().contains("gnome"))
        qputenv("QT_QPA_PLATFORMTHEME", "xdgdesktopportal");
#endif
#ifdef REELHOUSE_WEBVIEW
    QtWebView::initialize();   // before the application object (Research mode on Android)
#endif
    QGuiApplication app(argc, argv);
    app.setApplicationName("Reelhouse");
#ifdef Q_OS_ANDROID
    app.installEventFilter(new CommitBeforeEnter(&app));
#endif
    {
        // The picture formats come from Qt plugins (imageformats/*.dll). When one cannot be loaded (a DLL it needs is
        // missing) pictures stay empty, so the log says why.
        qInfo() << "plugin folders:" << QCoreApplication::libraryPaths();
        qInfo() << "pictures: can read" << QImageReader::supportedImageFormats() << "can write" << QImageWriter::supportedImageFormats();
        const QDir pd(QCoreApplication::applicationDirPath() + "/imageformats");
        for (const QString &f : pd.entryList({"*.dll", "*.so"}, QDir::Files)) {
            QPluginLoader pl(pd.absoluteFilePath(f));
            if (!pl.load()) qWarning() << "picture plugin not loaded:" << f << pl.errorString();
        }
    }
    app.setWindowIcon(QIcon(":/icon/reelhouse-icon.png"));
    app.setOrganizationName("Reelhouse");
    std::setlocale(LC_NUMERIC, "C");
    // Basic, with the hover tips drawn by the theme (qt/style/ReelStyle: a style that falls back to Basic).
    QQuickStyle::setStyle("ReelStyle");

    ThemeManager theme;
    // The theme of the device's own system until the preferences say otherwise (HyperOS on Xiaomi phones, Android,
    // iOS; GNOME on desktop, as before).
    theme.load(theme.systemId());
    CoreBridge core;
    // Phone or desktop, safe area, what this build can do: `Platform` in QML.
    Platform platform;
    qmlRegisterType<VideoItem>("Reelhouse", 1, 0, "VideoItem");
    qmlRegisterType<ThemeVideo>("Reelhouse", 1, 0, "ThemeVideo");
    // the orbs of time behind the pages (OrbsBackground.qml)
    qmlRegisterType<OrbsItem>("Reelhouse", 1, 0, "OrbsItem");
    // Research mode: web pages (Chromium / CEF, drawn off screen; cef_host.cpp)
    qmlRegisterType<WebPane>("Reelhouse", 1, 0, "WebPane");
#ifdef REELHOUSE_VOICE_INPUT
    // the microphone of Conversation with the learner's turns (MicRecorder.qml; desktop, with Qt Multimedia)
    qmlRegisterType<VoiceInput>("Reelhouse", 1, 0, "VoiceInput");
#endif
#ifdef REELHOUSE_TTS
    // A real call into Qt6TextToSpeech.dll: without one, MinGW leaves it out of the .exe's imports and windeployqt6
    // then ships neither the DLL nor the texttospeech plugins (sapi), so LocalTts.qml could not load. The engines
    // found also go to the log.
    qInfo().noquote() << "[voice] system voice engines:" << QTextToSpeech::availableEngines().join(QStringLiteral(", "));
#else
    qInfo() << "[voice] system voice engines: none (this build has no Qt TextToSpeech)";
#endif
    qmlRegisterSingletonInstance("Reelhouse", 1, 0, "Theme", &theme);
    qmlRegisterSingletonInstance("Reelhouse", 1, 0, "Core", &core);
    qmlRegisterSingletonInstance("Reelhouse", 1, 0, "Platform", &platform);
    // Stop the video before the QML engine (and the video item) is destroyed: mpv stops asking for frames.
    QObject::connect(&app, &QCoreApplication::aboutToQuit, &core, [&core] { core.cmd({{"cmd", "close_player"}}); });

    QQmlApplicationEngine engine;
    engine.addImageProvider("web", new WebImageProvider);
    engine.addImageProvider("edited", new EditedPageProvider);
    QObject::connect(&engine, &QQmlApplicationEngine::warnings, [](const QList<QQmlError> &w) { for (auto &e : w) qWarning() << "QML:" << e.toString(); });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app, [] { qCritical() << "QML object creation failed"; QCoreApplication::exit(2); }, Qt::QueuedConnection);
    engine.addImportPath(QCoreApplication::applicationDirPath() + "/qml");
    engine.addImportPath("qrc:/");
    engine.load(QUrl("qrc:/qml/Main.qml"));
    // Testing aid: REELHOUSE_TEST_GEOMETRY=1920x1080 opens the window at that size.
    if (const QByteArray g = qgetenv("REELHOUSE_TEST_GEOMETRY"); !g.isEmpty() && !engine.rootObjects().isEmpty())
        if (auto *w = qobject_cast<QWindow *>(engine.rootObjects().first())) {
            const QList<QByteArray> p = g.split('x');
            if (p.size() == 2) { w->setPosition(0, 0); w->resize(p[0].toInt(), p[1].toInt()); }
        }
    if (engine.rootObjects().isEmpty()) { qCritical() << "QML failed to load"; return 1; }
    // The safe area follows the main window (orientation, keyboard).
    if (auto *w = qobject_cast<QWindow *>(engine.rootObjects().first())) platform.attach(w);
    // Memory: when the window is minimized or the app goes to the background (Android), the QML engine collects its
    // garbage, drops cached components and the scene graph's spare resources, and (Linux/glibc) freed memory goes back
    // to the system. Two seconds later, so a quick Alt+Tab does nothing.
    if (auto *qw = qobject_cast<QQuickWindow *>(engine.rootObjects().first())) {
        auto *trim = new QTimer(qw);
        trim->setSingleShot(true);
        trim->setInterval(2000);
        QObject::connect(trim, &QTimer::timeout, qw, [qw, &engine] {
            const bool away = qw->visibility() == QWindow::Minimized || qw->visibility() == QWindow::Hidden
                || QGuiApplication::applicationState() == Qt::ApplicationHidden || QGuiApplication::applicationState() == Qt::ApplicationSuspended;
            if (!away) return;
            engine.collectGarbage();
            engine.trimComponentCache();
            qw->releaseResources();
#if defined(__GLIBC__)
            malloc_trim(0);
#endif
            qInfo("[memory] window away: QML garbage collected, caches released");
        });
        QObject::connect(qw, &QWindow::visibilityChanged, trim, [trim](QWindow::Visibility) { trim->start(); });
        QObject::connect(qApp, &QGuiApplication::applicationStateChanged, trim, [trim](Qt::ApplicationState) { trim->start(); });
    }
#ifdef Q_OS_WIN
    // A full-screen OpenGL window covering the whole monitor gets the "independent flip" of Windows: the video
    // bypasses the desktop compositor, so recording or sharing the screen (OBS, Discord, Game Bar) shows a frozen
    // picture (HP). A one-pixel border keeps the window composed, like Qt's own setHasBorderInFullScreen.
    if (auto *w = qobject_cast<QWindow *>(engine.rootObjects().first())) {
        QObject::connect(w, &QWindow::visibilityChanged, w, [w](QWindow::Visibility v) {
            if (v != QWindow::FullScreen) return;
            QTimer::singleShot(0, w, [w] {
                if (w->visibility() != QWindow::FullScreen) return;
                HWND h = reinterpret_cast<HWND>(w->winId());
                const LONG_PTR st = GetWindowLongPtrW(h, GWL_STYLE);
                if (st & WS_BORDER) return;
                SetWindowLongPtrW(h, GWL_STYLE, st | WS_BORDER);
                SetWindowPos(h, nullptr, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
                qInfo("[window] full screen with a border (screen capture keeps working)");
            });
        });
    }
#endif
    qInfo() << "started, theme" << theme.id() << "on" << platform.os();
    // Testing aid: REELHOUSE_TEST_SHOT=<file.png> saves a picture of the window a few seconds after start and quits.
    if (const QString shot = qEnvironmentVariable("REELHOUSE_TEST_SHOT"); !shot.isEmpty()) {
        QTimer::singleShot(9000, &app, [&engine, shot] {
            if (auto *w = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0))) w->grabWindow().save(shot);
            QCoreApplication::quit();
        });
    }
    // REELHOUSE_TEST_SIZE=WxH resizes the window (to look at the narrow layouts).
    if (const QStringList wh = qEnvironmentVariable("REELHOUSE_TEST_SIZE").split('x'); wh.size() == 2)
        if (auto *w = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0))) w->resize(wh[0].toInt(), wh[1].toInt());
    // REELHOUSE_TEST_OPEN=about | prefs | prefs:<page> opens that window before the picture is taken.
    if (const QString what = qEnvironmentVariable("REELHOUSE_TEST_OPEN"); !what.isEmpty()) {
        QTimer::singleShot(8000, &app, [&engine, what] {
            if (auto *o = engine.rootObjects().value(0)) QMetaObject::invokeMethod(o, "testOpen", Q_ARG(QVariant, what));
        });
    }
    int rc = app.exec();
    qInfo() << "exit" << rc;
    return rc;
}
