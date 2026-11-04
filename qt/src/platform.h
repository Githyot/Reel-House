#pragma once
#include <QObject>
#include <QPointer>
#include <QTimer>
#include <QVariantMap>
#include <QWindow>

// Where the app runs, for QML (`Platform`, a singleton of the Reelhouse module): phone or desktop, which system, touch,
// the safe area of a phone screen (notch, home bar) and what this build of the core can do (helper programs,
// Chromium, native file dialogs...), read once from the core (rh_platform_caps).
//
//   Platform.isMobile / isAndroid / isIOS / isDesktop / isWindows / isLinux / isMac, Platform.os
//   Platform.touch             a touch screen is the main input (always on a phone)
//   Platform.tv                Android TV (leanback): remote control, no touch
//   Platform.safeAreaInsets    {top, left, right, bottom} in logical pixels (0 on desktop). Qt 6.9+ also has the
//                              SafeArea attached property in QML; this works on any Qt version.
//   Platform.localProcesses / browser / fileDialogs / windowFrame, Platform.caps (the whole map)
//   Platform.keepScreenOn(true) while a video plays
#if defined(Q_OS_ANDROID) || defined(Q_OS_IOS)
#define REELHOUSE_MOBILE 1
#endif

class Platform : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool isMobile READ isMobile CONSTANT)
    Q_PROPERTY(bool isAndroid READ isAndroid CONSTANT)
    Q_PROPERTY(bool isIOS READ isIOS CONSTANT)
    Q_PROPERTY(bool isDesktop READ isDesktop CONSTANT)
    Q_PROPERTY(bool isWindows READ isWindows CONSTANT)
    Q_PROPERTY(bool isLinux READ isLinux CONSTANT)
    Q_PROPERTY(bool isMac READ isMac CONSTANT)
    Q_PROPERTY(QString os READ os CONSTANT)
    Q_PROPERTY(bool touch READ touch NOTIFY touchChanged)
    Q_PROPERTY(bool tv READ tv CONSTANT)
    Q_PROPERTY(QVariantMap safeAreaInsets READ safeAreaInsets NOTIFY safeAreaChanged)
    Q_PROPERTY(QVariantMap caps READ caps CONSTANT)
    Q_PROPERTY(bool localProcesses READ localProcesses CONSTANT)
    Q_PROPERTY(bool browser READ browser CONSTANT)
    Q_PROPERTY(bool fileDialogs READ fileDialogs CONSTANT)
    Q_PROPERTY(bool windowFrame READ windowFrame CONSTANT)
public:
    explicit Platform(QObject *parent = nullptr);

    static constexpr bool isAndroid() {
#ifdef Q_OS_ANDROID
        return true;
#else
        return false;
#endif
    }
    static constexpr bool isIOS() {
#ifdef Q_OS_IOS
        return true;
#else
        return false;
#endif
    }
    static constexpr bool isMobile() { return isAndroid() || isIOS(); }
    static constexpr bool isDesktop() { return !isMobile(); }
    static constexpr bool isWindows() {
#ifdef Q_OS_WIN
        return true;
#else
        return false;
#endif
    }
    static constexpr bool isLinux() {
#if defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
        return true;
#else
        return false;
#endif
    }
    static constexpr bool isMac() {
#if defined(Q_OS_MACOS)
        return true;
#else
        return false;
#endif
    }
    QString os() const;
    bool touch() const { return m_touch; }
    bool tv() const { return m_tv; }
    QVariantMap safeAreaInsets() const { return m_safeArea; }
    QVariantMap caps() const { return m_caps; }
    bool localProcesses() const { return m_caps.value("localProcesses", isDesktop()).toBool(); }
    bool browser() const { return m_caps.value("browser", isDesktop()).toBool(); }
    bool fileDialogs() const { return m_caps.value("fileDialogs", isDesktop()).toBool(); }
    bool windowFrame() const { return m_caps.value("windowFrame", isDesktop()).toBool(); }

    // Follows the main window: the safe area changes with the orientation and the keyboard.
    Q_INVOKABLE void attach(QWindow *w);
    Q_INVOKABLE void refreshSafeArea();
    // The screen does not dim while a video plays.
    Q_INVOKABLE void keepScreenOn(bool on);
    // Android: hands a downloaded APK (an update of the app, controller_update.rs) to the system installer through
    // the FileProvider of the manifest. False when it could not be started. Does nothing elsewhere.
    static bool installApk(const QString &path);
    // The folders of this system: {data, cache, temp, downloads, movies, pictures, documents}.
    Q_INVOKABLE QVariantMap folders() const;

signals:
    void touchChanged();
    void safeAreaChanged();

private:
    void detectTouch();
    QVariantMap m_caps, m_safeArea;
    bool m_touch = false, m_tv = false;
    QPointer<QWindow> m_window;
    QTimer m_safeTimer;
};
