#include "platform.h"
#include <QGuiApplication>
#include <QInputDevice>
#include <QJsonDocument>
#include <QJsonObject>
#include <QScreen>
#include <QStandardPaths>
#include <QDebug>

extern "C" {
#include "bridge.h"
}

#ifdef Q_OS_ANDROID
#include <QJniObject>
#include <QJniEnvironment>
#include <QtCore/qnativeinterface.h>
#endif

#ifdef Q_OS_IOS
// platform_ios.mm (Objective-C++): the safe area of the UIWindow and the idle timer.
QMarginsF reelhouseIosSafeArea();
void reelhouseIosKeepScreenOn(bool on);
#endif

Platform::Platform(QObject *parent) : QObject(parent) {
    // What the core can do here (helper programs, Chromium...), from qtbridge: platform::caps().
    if (char *j = rh_platform_caps()) {
        const QJsonDocument d = QJsonDocument::fromJson(QByteArray(j));
        rh_free_str(j);
        if (d.isObject()) m_caps = d.object().toVariantMap();
    }
    m_safeArea = {{"top", 0.0}, {"left", 0.0}, {"right", 0.0}, {"bottom", 0.0}};
#ifdef Q_OS_ANDROID
    // Android TV: the system says "leanback" and usually has no touch screen.
    {
        QJniObject activity = QNativeInterface::QAndroidApplication::context();
        if (activity.isValid()) {
            QJniObject pm = activity.callObjectMethod("getPackageManager", "()Landroid/content/pm/PackageManager;");
            QJniObject feature = QJniObject::fromString("android.software.leanback");
            if (pm.isValid()) m_tv = pm.callMethod<jboolean>("hasSystemFeature", "(Ljava/lang/String;)Z", feature.object<jstring>());
        }
    }
#endif
    detectTouch();
    m_safeTimer.setSingleShot(true);
    m_safeTimer.setInterval(50);
    connect(&m_safeTimer, &QTimer::timeout, this, &Platform::refreshSafeArea);
    qInfo() << "platform:" << os() << (isMobile() ? "mobile" : "desktop") << "touch" << m_touch << "tv" << m_tv << "caps" << m_caps;
}

QString Platform::os() const {
    if (isAndroid()) return QStringLiteral("android");
    if (isIOS()) return QStringLiteral("ios");
    if (isWindows()) return QStringLiteral("windows");
    if (isMac()) return QStringLiteral("macos");
    if (isLinux()) return QStringLiteral("linux");
    return QSysInfo::productType();
}

void Platform::detectTouch() {
    bool t = isMobile() && !m_tv;
    if (!t) {
        for (const QInputDevice *d : QInputDevice::devices())
            if (d->type() == QInputDevice::DeviceType::TouchScreen) { t = true; break; }
    }
    if (t != m_touch) { m_touch = t; emit touchChanged(); }
}

void Platform::attach(QWindow *w) {
    if (m_window == w) return;
    if (m_window) m_window->disconnect(this);
    m_window = w;
    if (!w) return;
    connect(w, &QWindow::widthChanged, &m_safeTimer, qOverload<>(&QTimer::start));
    connect(w, &QWindow::heightChanged, &m_safeTimer, qOverload<>(&QTimer::start));
    connect(w, &QWindow::screenChanged, &m_safeTimer, qOverload<>(&QTimer::start));
    connect(w, &QWindow::visibilityChanged, &m_safeTimer, qOverload<>(&QTimer::start));
    if (QScreen *s = w->screen()) connect(s, &QScreen::orientationChanged, &m_safeTimer, qOverload<>(&QTimer::start));
    refreshSafeArea();
}

void Platform::refreshSafeArea() {
    QVariantMap m = {{"top", 0.0}, {"left", 0.0}, {"right", 0.0}, {"bottom", 0.0}};
#if defined(Q_OS_IOS)
    const QMarginsF i = reelhouseIosSafeArea();
    m = {{"top", i.top()}, {"left", i.left()}, {"right", i.right()}, {"bottom", i.bottom()}};
#elif defined(Q_OS_ANDROID)
    // The insets of the decor view (status bar, navigation bar, cutout), in device pixels: API 28 names.
    QJniObject activity = QNativeInterface::QAndroidApplication::context();
    if (activity.isValid()) {
        QJniObject window = activity.callObjectMethod("getWindow", "()Landroid/view/Window;");
        QJniObject decor = window.isValid() ? window.callObjectMethod("getDecorView", "()Landroid/view/View;") : QJniObject();
        QJniObject insets = decor.isValid() ? decor.callObjectMethod("getRootWindowInsets", "()Landroid/view/WindowInsets;") : QJniObject();
        if (insets.isValid()) {
            const qreal dpr = m_window ? m_window->devicePixelRatio() : (QGuiApplication::primaryScreen() ? QGuiApplication::primaryScreen()->devicePixelRatio() : 1.0);
            auto get = [&](const char *name) { return insets.callMethod<jint>(name) / dpr; };
            m = {{"top", get("getSystemWindowInsetTop")}, {"left", get("getSystemWindowInsetLeft")}, {"right", get("getSystemWindowInsetRight")},
                 {"bottom", get("getSystemWindowInsetBottom")}};
        }
    }
#endif
    if (m != m_safeArea) { m_safeArea = m; emit safeAreaChanged(); }
}

void Platform::keepScreenOn(bool on) {
#if defined(Q_OS_ANDROID)
    // FLAG_KEEP_SCREEN_ON on the activity's window; must run on the Android UI thread.
    QNativeInterface::QAndroidApplication::runOnAndroidMainThread([on] {
        QJniObject activity = QNativeInterface::QAndroidApplication::context();
        if (!activity.isValid()) return;
        QJniObject window = activity.callObjectMethod("getWindow", "()Landroid/view/Window;");
        if (!window.isValid()) return;
        const jint FLAG_KEEP_SCREEN_ON = 128;
        if (on) window.callMethod<void>("addFlags", "(I)V", FLAG_KEEP_SCREEN_ON);
        else window.callMethod<void>("clearFlags", "(I)V", FLAG_KEEP_SCREEN_ON);
    });
#elif defined(Q_OS_IOS)
    reelhouseIosKeepScreenOn(on);
#else
    Q_UNUSED(on);
#endif
}

QVariantMap Platform::folders() const {
    auto loc = [](QStandardPaths::StandardLocation l) { return QStandardPaths::writableLocation(l); };
    return {{"data", loc(QStandardPaths::AppDataLocation)},     {"cache", loc(QStandardPaths::CacheLocation)},
            {"temp", loc(QStandardPaths::TempLocation)},        {"downloads", loc(QStandardPaths::DownloadLocation)},
            {"movies", loc(QStandardPaths::MoviesLocation)},    {"pictures", loc(QStandardPaths::PicturesLocation)},
            {"documents", loc(QStandardPaths::DocumentsLocation)}};
}
