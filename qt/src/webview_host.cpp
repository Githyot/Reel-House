// Research mode on Android: the pages are drawn by the Android System WebView (Qt WebView), not by Chromium (CEF has
// no build for phones). This is the same small engine API WebPane uses on desktop (cef_host.h), so WebPane, the
// sidebar, the tabs and the history work as they do there. The difference: a WebView is a native view placed over the
// app's window at the WebPane's place, so it is not part of the Qt Quick picture. QML drawn over the page (the side
// sheets) only shows while WebPane hides it (`covered`). Touches and the keyboard go to the page directly.
#include "cef_host.h"
#include "bridge.h"

#ifdef REELHOUSE_WEBVIEW

#include <QDesktopServices>
#include <QLocale>
#include <QHash>
#include <QPointer>
#include <QQmlComponent>
#include <QQmlContext>
#include <QtQml/qqml.h>
#include <QQmlEngine>
#include <QQuickItem>
#include <QUrl>

namespace cefh {
namespace {

// What the QML WebView reports, passed on to its WebPane as the events Chromium would send.
class Bridge : public QObject {
    Q_OBJECT
public:
    Q_INVOKABLE void state(int tab, const QString &url, const QString &title, bool loading, bool back, bool forward);
    Q_INVOKABLE void loaded(int tab, int status, const QString &url, const QString &error);
};

struct Tab {
    View *view = nullptr;
    QPointer<QQuickItem> item;
    QString url, title;
};

QHash<int, Tab> g_tabs;
int g_next = 1;
Bridge *g_bridge = nullptr;

void send(int tab, Event e) {
    auto it = g_tabs.find(tab);
    if (it != g_tabs.end() && it->view) it->view->cefEvent(e);
}

void Bridge::state(int tab, const QString &url, const QString &title, bool loading, bool back, bool forward) {
    auto it = g_tabs.find(tab);
    if (it == g_tabs.end()) return;
    if (url != it->url) {
        it->url = url;
        Event e(Ev::Url, tab);
        e.text = url;
        send(tab, e);
    }
    if (title != it->title) {
        it->title = title;
        Event e(Ev::Title, tab);
        e.text = title;
        e.text2 = url;
        send(tab, e);
    }
    Event l(Ev::Loading, tab);
    l.n1 = loading ? 1 : 0;
    send(tab, l);
    Event n(Ev::NavState, tab);
    n.n1 = back ? 1 : 0;
    n.n2 = forward ? 1 : 0;
    send(tab, n);
}

// status: Qt WebView's LoadStatus (0 started, 1 stopped, 2 succeeded, 3 failed)
void Bridge::loaded(int tab, int status, const QString &url, const QString &error) {
    auto it = g_tabs.find(tab);
    if (it == g_tabs.end()) return;
    if (status == 2) {
        Event e(Ev::Visited, tab);
        e.text = url;
        e.text2 = it->title;
        e.n1 = 200;
        send(tab, e);
    } else if (status == 3) {
        Event e(Ev::NavFailed, tab);
        e.text = url;
        e.text2 = error;
        e.n1 = -1;
        send(tab, e);
    }
}


QQuickItem *item(int tab) {
    auto it = g_tabs.find(tab);
    return it == g_tabs.end() ? nullptr : it->item.data();
}

// IP protection (src/privacy.rs) on: the Android System WebView cannot be sent through the app's gateway, so pages
// from the internet are not opened at all (they would show the real address); a short page says why.
QUrl toUrl(const QString &s) {
    if (s.isEmpty()) return QUrl(QStringLiteral("about:blank"));
    const QUrl u = QUrl::fromUserInput(s);
    const QString host = u.host();
    const bool local = u.isLocalFile() || u.scheme() == QLatin1String("about") || u.scheme() == QLatin1String("data") || host == QLatin1String("localhost") || host == QLatin1String("127.0.0.1");
    if (!local && rh_privacy_port() != 0) {
        char *raw = rh_tr(QLocale::system().name().left(2).toUtf8().constData(), "IP protection is on: on Android the research browser cannot go through it, so pages do not open here. Turn the protection off in Preferences to browse.");
        const QString text = QString::fromUtf8(raw ? raw : "");
        if (raw) rh_free_str(raw);
        return QUrl(QStringLiteral("data:text/html;charset=utf-8,") + QString::fromLatin1(QUrl::toPercentEncoding(
            QStringLiteral("<html><body style='font-family:sans-serif;padding:24px;color:#888'><p>%1</p></body></html>").arg(text.toHtmlEscaped()))));
    }
    return u;
}

}  // namespace

// Google Lens (and the image search it falls back to) answers a phone's browser with a "Download app" page instead of
// taking the picture: those pages get a desktop Chrome's name, the others keep the WebView's own.
static void uaFor(QQuickItem *it, const QString &url) {
    static const QString desktop = QStringLiteral(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36");
    const bool lens = url.contains(QLatin1String("lens.google.")) || url.contains(QLatin1String("/searchbyimage"))
        || url.contains(QLatin1String("images.google.")) || url.contains(QLatin1String("google.com/imghp"));
    const QString cur = it->property("httpUserAgent").toString();
    if (lens && cur != desktop) {
        it->setProperty("mobileUa", cur);
        it->setProperty("httpUserAgent", desktop);
    } else if (!lens && cur == desktop) {
        it->setProperty("httpUserAgent", it->property("mobileUa").toString());
    }
}

bool probe(QString &info) { info.clear(); return true; }
QString version() { return QStringLiteral("Android System WebView"); }
void configure(const QString &, const QString &) {}
bool start(QString &problem) { problem.clear(); return true; }
bool started() { return true; }

int openTab(View *view, const QString &url, bool hidden) {
    auto *parent = dynamic_cast<QQuickItem *>(view);
    QQmlEngine *engine = parent ? qmlEngine(parent) : nullptr;
    if (!engine) return 0;
    if (!g_bridge) g_bridge = new Bridge;
    static QPointer<QQmlComponent> comp;
    if (!comp) {
        comp = new QQmlComponent(engine, QUrl(QStringLiteral("qrc:/qml-android/WebViewTab.qml")), engine);
    }
    const int id = g_next++;
    QObject *o = comp->beginCreate(qmlContext(parent) ? qmlContext(parent) : engine->rootContext());
    auto *it = qobject_cast<QQuickItem *>(o);
    if (!it) {
        qWarning() << "[web] Android WebView could not be made:" << comp->errorString();
        delete o;
        return 0;
    }
    it->setProperty("tabId", id);
    it->setProperty("host", QVariant::fromValue<QObject *>(g_bridge));
    it->setParentItem(parent);
    it->setVisible(!hidden);
    comp->completeCreate();
    Tab t;
    t.view = view;
    t.item = it;
    g_tabs.insert(id, t);
    if (!url.isEmpty()) {
        uaFor(it, url);
        it->setProperty("url", toUrl(url));
    }
    return id;
}

void closeTab(int tab) {
    auto it = g_tabs.find(tab);
    if (it == g_tabs.end()) return;
    View *v = it->view;
    if (it->item) it->item->deleteLater();
    g_tabs.erase(it);
    if (v) v->cefEvent(Event(Ev::Closed, tab));
}

void forgetView(View *view) {
    for (auto it = g_tabs.begin(); it != g_tabs.end();) {
        if (it->view == view) {
            if (it->item) it->item->deleteLater();
            it = g_tabs.erase(it);
        } else {
            ++it;
        }
    }
}

void navigate(int tab, const QString &url) {
    if (QQuickItem *i = item(tab)) {
        uaFor(i, url);
        i->setProperty("url", toUrl(url));
    }
}
void goBack(int tab) { if (QQuickItem *i = item(tab)) QMetaObject::invokeMethod(i, "goBack"); }
void goForward(int tab) { if (QQuickItem *i = item(tab)) QMetaObject::invokeMethod(i, "goForward"); }
void reload(int tab) { if (QQuickItem *i = item(tab)) QMetaObject::invokeMethod(i, "reload"); }
void stop(int tab) { if (QQuickItem *i = item(tab)) QMetaObject::invokeMethod(i, "stop"); }
void setZoom(int, double) {}
void setHidden(int tab, bool hidden) { if (QQuickItem *i = item(tab)) i->setVisible(!hidden); }
void resized(int) {}
void screenChanged(int) {}
void invalidate(int) {}
void setFrameRate(int, int) {}
void setFocus(int tab, bool focus) { if (QQuickItem *i = item(tab)) { if (focus) i->forceActiveFocus(); } }
void mouseMove(int, int, int, uint, bool) {}
void mouseClick(int, int, int, uint, int, bool, int) {}
void mouseWheel(int, int, int, uint, int, int) {}
void key(int, int, int, int, uint, ushort, ushort, bool) {}
void imeComposition(int, const QString &, int) {}
void imeCommit(int, const QString &) {}
void imeCancel(int) {}
void captureLost(int) {}
void runScript(int tab, const QString &code) {
    if (QQuickItem *i = item(tab)) QMetaObject::invokeMethod(i, "runJavaScript", Q_ARG(QString, code));
}
void editCommand(int, const QString &) {}
// The WebView saves nothing itself: the system (its browser or download manager) takes the address.
void download(int, const QString &url) { QDesktopServices::openUrl(QUrl(url)); }
void answerDialog(int, bool, const QString &) {}
void find(int, const QString &, bool, bool) {}
void cancelDownload(int) {}
void pauseDownload(int) {}
void resumeDownload(int) {}
void shutdown() {
    for (auto &t : g_tabs)
        if (t.item) t.item->deleteLater();
    g_tabs.clear();
}

}  // namespace cefh

#include "webview_host.moc"

#endif  // REELHOUSE_WEBVIEW
