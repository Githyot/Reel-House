#include "cef_host.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QStandardPaths>

#if !defined(REELHOUSE_CEF) && !defined(REELHOUSE_WEBVIEW)

// Built without Chromium (qt/third_party/cef was missing at build time): nothing here works, and WebPane opens
// addresses in the system browser.
namespace cefh {
bool probe(QString &info) {
    info = QStringLiteral("This build of Reelhouse has no built-in browser engine (Chromium).");
    return false;
}
QString version() { return QString(); }
void configure(const QString &, const QString &) {}
bool start(QString &problem) { probe(problem); return false; }
bool started() { return false; }
int openTab(View *, const QString &, bool) { return 0; }
void closeTab(int) {}
void forgetView(View *) {}
void navigate(int, const QString &) {}
void goBack(int) {}
void goForward(int) {}
void reload(int) {}
void stop(int) {}
void setZoom(int, double) {}
void setHidden(int, bool) {}
void resized(int) {}
void screenChanged(int) {}
void invalidate(int) {}
void setFrameRate(int, int) {}
void setFocus(int, bool) {}
void mouseMove(int, int, int, uint, bool) {}
void mouseClick(int, int, int, uint, int, bool, int) {}
void mouseWheel(int, int, int, uint, int, int) {}
void key(int, int, int, int, uint, ushort, ushort, bool) {}
void imeComposition(int, const QString &, int) {}
void imeCommit(int, const QString &) {}
void imeCancel(int) {}
void captureLost(int) {}
void runScript(int, const QString &) {}
void editCommand(int, const QString &) {}
void download(int, const QString &) {}
void answerDialog(int, bool, const QString &) {}
void find(int, const QString &, bool, bool) {}
void cancelDownload(int) {}
void pauseDownload(int) {}
void resumeDownload(int) {}
void shutdown() {}
}  // namespace cefh

#elif defined(REELHOUSE_CEF)

#include <QElapsedTimer>
#include <QHash>
#include <QLibrary>
#include <QPointer>
#include <QRegularExpression>
#include <QSet>
#include <QThread>
#include <QTimer>
#include <atomic>
#include <cmath>
#include <cstring>

#ifdef Q_OS_WIN
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

#include "include/cef_api_hash.h"
#include "include/cef_version_info.h"
#include "include/capi/cef_app_capi.h"
#include "include/capi/cef_browser_capi.h"
#include "include/capi/cef_browser_process_handler_capi.h"
#include "include/capi/cef_client_capi.h"
#include "include/capi/cef_command_line_capi.h"
#include "include/capi/cef_context_menu_handler_capi.h"
#include "include/capi/cef_display_handler_capi.h"
#include "include/capi/cef_download_handler_capi.h"
#include "include/capi/cef_download_item_capi.h"
#include "include/capi/cef_frame_capi.h"
#include "include/capi/cef_jsdialog_handler_capi.h"
#include "include/capi/cef_life_span_handler_capi.h"
#include "include/capi/cef_load_handler_capi.h"
#include "include/capi/cef_render_handler_capi.h"
#include "include/capi/cef_request_capi.h"
#include "include/capi/cef_request_handler_capi.h"
#include "include/capi/cef_resource_request_handler_capi.h"

#include <QJsonArray>
#include <QJsonDocument>

#include "bridge.h"   // the ad blocker of the core (rh_adblock_check / rh_adblock_css)
// The IP protection gateway's port Chromium was started with (its proxy cannot change later); -1 = not started.
static int g_cefProxyPort = -1;

// How references work in CEF's C API (the rules the C++ wrapper follows):
//  - a struct that CEF passes to one of our callbacks comes with a reference for us: we release it when done;
//  - a struct a CEF function returns comes with a reference: we release it;
//  - a struct we hand to CEF (an argument, or the answer of get_*_handler) must carry a reference for CEF: add_ref first.
// The `self` argument of a call is never counted.

namespace cefh {
namespace {

// ---- the libcef functions, loaded at run time ----

struct Api {
    decltype(&cef_api_hash) api_hash = nullptr;
    decltype(&cef_initialize) initialize = nullptr;
    decltype(&cef_shutdown) shutdown = nullptr;
    decltype(&cef_do_message_loop_work) do_work = nullptr;
    decltype(&cef_browser_host_create_browser_sync) create_browser = nullptr;
    decltype(&cef_string_userfree_utf16_free) userfree_free = nullptr;
    decltype(&cef_string_list_size) list_size = nullptr;
    decltype(&cef_string_list_value) list_value = nullptr;
    decltype(&cef_string_utf16_clear) str_clear = nullptr;
    decltype(&cef_version_info) version_info = nullptr;
};

QString fromCef(const cef_string_t *s) {
    return s && s->str ? QString::fromUtf16(s->str, qsizetype(s->length)) : QString();
}

// A string to hand to CEF without copying: it points into a QString that must outlive the call.
struct CStr {
    QString q;
    cef_string_t c{};
    explicit CStr(const QString &s) : q(s) {
        c.str = reinterpret_cast<char16_t *>(const_cast<ushort *>(reinterpret_cast<const ushort *>(q.utf16())));
        c.length = size_t(q.size());
        c.dtor = nullptr;
    }
    const cef_string_t *p() const { return &c; }
};

template <class T> void rel(T *p) {
    if (p) p->base.release(&p->base);
}

// ---- our objects: several CEF structs sharing one reference count ----

struct Shared {
    std::atomic<int> refs{1};
    virtual ~Shared() = default;
    void addRef() { refs.fetch_add(1); }
    bool release() {
        if (refs.fetch_sub(1) == 1) {
            delete this;
            return true;
        }
        return false;
    }
};

template <class T> struct Part {
    T cef;             // first: a pointer to the struct is a pointer to the Part
    Shared *owner = nullptr;
};

template <class T> void initPart(Part<T> &p, Shared *owner) {
    std::memset(&p.cef, 0, sizeof(T));
    p.cef.base.size = sizeof(T);
    p.owner = owner;
    p.cef.base.add_ref = [](cef_base_ref_counted_t *b) { reinterpret_cast<Part<T> *>(b)->owner->addRef(); };
    p.cef.base.release = [](cef_base_ref_counted_t *b) { return reinterpret_cast<Part<T> *>(b)->owner->release() ? 1 : 0; };
    p.cef.base.has_one_ref = [](cef_base_ref_counted_t *b) { return reinterpret_cast<Part<T> *>(b)->owner->refs.load() == 1 ? 1 : 0; };
    p.cef.base.has_at_least_one_ref = [](cef_base_ref_counted_t *b) { return reinterpret_cast<Part<T> *>(b)->owner->refs.load() >= 1 ? 1 : 0; };
}

template <class T, class O> O *ownerOf(T *self) {
    return static_cast<O *>(reinterpret_cast<Part<T> *>(self)->owner);
}

// Structs that live for the whole run (the app and its browser-process handler): counting does nothing.
template <class T> void initStatic(T &s) {
    std::memset(&s, 0, sizeof(T));
    s.base.size = sizeof(T);
    s.base.add_ref = [](cef_base_ref_counted_t *) {};
    s.base.release = [](cef_base_ref_counted_t *) { return 0; };
    s.base.has_one_ref = [](cef_base_ref_counted_t *) { return 1; };
    s.base.has_at_least_one_ref = [](cef_base_ref_counted_t *) { return 1; };
}

struct Tab;

// The engine: one per process.
struct Engine {
    QLibrary lib;
    Api api;
    QString dir;                 // where libcef and its files are
    QString version;
    bool probed = false, present = false, isStarted = false, failed = false, down = false;
    QString problem;
    QString profile, language;
    QHash<int, Tab *> tabs;
    int nextTab = 1;
    QSet<View *> views;
    QTimer *pumpTimer = nullptr;      // the next piece of work Chromium asked for
    QTimer *idleTimer = nullptr;      // and a steady beat in case it asked for nothing
    QObject *pumpTarget = nullptr;
    bool inWork = false;
    // downloads: id -> the view that started it and Chromium's handle to cancel / pause / resume it
    struct Dl { View *view = nullptr; cef_download_item_callback_t *cb = nullptr; QString path; };
    QHash<quint32, Dl> downloads;
    // pages' alert / confirm / prompt waiting for an answer
    struct Dlg { int tab = 0; cef_jsdialog_callback_t *cb = nullptr; };
    QHash<int, Dlg> dialogs;
    int nextDialog = 1;
    cef_app_t app;
    cef_browser_process_handler_t bph;
    cef_resource_request_handler_t blocked;   // handed out for the requests the ad blocker cancels
};
Engine *E = nullptr;

Engine &eng() {
    if (!E) E = new Engine;
    return *E;
}

QString freeStr(cef_string_userfree_t s) {
    QString r = fromCef(s);
    if (s && E && E->api.userfree_free) E->api.userfree_free(s);
    return r;
}

void doWork();

void schedule(qint64 delayMs) {
    Engine &e = eng();
    if (!e.pumpTimer || e.down) return;
    if (delayMs <= 0) {
        // as soon as the event loop is free (never from inside the work itself)
        QMetaObject::invokeMethod(e.pumpTarget, [] { doWork(); }, Qt::QueuedConnection);
        return;
    }
    const int ms = int(qMin<qint64>(delayMs, 1000 / 30));
    if (!e.pumpTimer->isActive() || e.pumpTimer->remainingTime() > ms) e.pumpTimer->start(ms);
}

void doWork() {
    Engine &e = eng();
    if (!e.isStarted || e.down || e.inWork) return;
    e.inWork = true;
    e.api.do_work();
    e.inWork = false;
}

// ---- a tab: one browser and its handlers ----

struct Tab : Shared {
    int id = 0;
    View *view = nullptr;              // nulled when the tab closes or the view goes away
    cef_browser_t *browser = nullptr;  // our reference, let go in on_before_close
    bool closing = false;
    bool hidden = false;
    QString url, title;
    Part<cef_client_t> client;
    Part<cef_render_handler_t> render;
    Part<cef_life_span_handler_t> life;
    Part<cef_load_handler_t> load;
    Part<cef_display_handler_t> display;
    Part<cef_download_handler_t> dl;
    Part<cef_context_menu_handler_t> menu;
    Part<cef_jsdialog_handler_t> js;
    Part<cef_request_handler_t> request;

    Tab();
    void send(const Event &ev) const {
        if (view && !closing) view->cefEvent(ev);
    }
    cef_browser_host_t *host() const { return browser ? browser->get_host(browser) : nullptr; }
};

// (nothing once Chromium has stopped: a pane destroyed after the app quit must not call into it)
Tab *tabOf(int id) {
    return E && !E->down ? E->tabs.value(id, nullptr) : nullptr;
}

// Runs `f` with the browser host of a tab (and lets the host go after).
template <class F> void withHost(int id, F f) {
    Tab *t = tabOf(id);
    if (!t || !t->browser || t->closing) return;
    cef_browser_host_t *h = t->host();
    if (!h) return;
    f(t, h);
    rel(h);
}

template <class F> void withFrame(int id, bool focused, F f) {
    Tab *t = tabOf(id);
    if (!t || !t->browser || t->closing) return;
    cef_frame_t *fr = focused ? t->browser->get_focused_frame(t->browser) : nullptr;
    if (!fr) fr = t->browser->get_main_frame(t->browser);
    if (!fr) return;
    f(t, fr);
    rel(fr);
}

bool isMain(cef_frame_t *frame) { return frame && frame->is_main(frame); }

// ---- the ad blocker (uBlock's filter lists, matched in the core: reelhouse::adblock) ----

// The request type the way the filter lists name it (the $script, $image... options).
const char *adblockKind(cef_resource_type_t t) {
    switch (t) {
    case RT_SUB_FRAME: return "subdocument";
    case RT_STYLESHEET: return "stylesheet";
    case RT_SCRIPT: case RT_WORKER: case RT_SHARED_WORKER: case RT_SERVICE_WORKER: return "script";
    case RT_IMAGE: case RT_FAVICON: return "image";
    case RT_FONT_RESOURCE: return "font";
    case RT_MEDIA: return "media";
    case RT_XHR: return "xmlhttprequest";
    case RT_PING: case RT_CSP_REPORT: return "ping";
    case RT_OBJECT: case RT_PLUGIN_RESOURCE: return "object";
    default: return "other";
    }
}

// Adds the element hiding rules of the page in `frame` (one <style> of ours, filled again on each call).
void adblockCss(cef_frame_t *f) {
    if (!f) return;
    const QString url = freeStr(f->get_url(f));
    if (!url.startsWith(QLatin1String("http"))) return;
    char *css = rh_adblock_css(url.toUtf8().constData());
    const QString text = css ? QString::fromUtf8(css) : QString();
    if (css) rh_free_str(css);
    if (text.isEmpty()) return;
    const QString lit = QString::fromUtf8(QJsonDocument(QJsonArray{text}).toJson(QJsonDocument::Compact));
    const QString code = QStringLiteral(
        "(function(){var c=%1[0],d=document,s=d.getElementById('__reelhouse_adblock');"
        "if(!s){s=d.createElement('style');s.id='__reelhouse_adblock';}"
        "s.textContent=c;var p=d.head||d.documentElement;if(p&&s.parentNode!==p)p.appendChild(s);})();").arg(lit);
    CStr c(code), where(QStringLiteral("reelhouse://adblock"));
    f->execute_java_script(f, c.p(), where.p(), 0);
}

// The first free name in the Downloads folder ("file (2).pdf" when "file.pdf" is there).
QString downloadPath(const QString &suggested) {
    QString dir = QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
    if (dir.isEmpty()) dir = QDir::homePath();
    QDir().mkpath(dir);
    QString name = suggested.trimmed();
    name.replace(QRegularExpression(QStringLiteral("[\\\\/:*?\"<>|\\x00-\\x1f]")), QStringLiteral("_"));
    if (name.isEmpty()) name = QStringLiteral("download");
    const QFileInfo fi(name);
    const QString base = fi.completeBaseName(), ext = fi.suffix().isEmpty() ? QString() : "." + fi.suffix();
    QString path = QDir(dir).filePath(name);
    for (int n = 2; QFileInfo::exists(path) && n < 1000; ++n) path = QDir(dir).filePath(QStringLiteral("%1 (%2)%3").arg(base).arg(n).arg(ext));
    return path;
}

View *viewForDownload(cef_browser_t *browser) {
    if (browser) {
        const int bid = browser->get_identifier(browser);
        for (Tab *t : std::as_const(E->tabs))
            if (t->browser && t->browser->get_identifier(t->browser) == bid && t->view) return t->view;
    }
    return E->views.isEmpty() ? nullptr : *E->views.begin();
}

void sendDl(quint32 id, const Event &ev) {
    Engine::Dl &d = E->downloads[id];
    if (!d.view || !E->views.contains(d.view)) d.view = E->views.isEmpty() ? nullptr : *E->views.begin();
    if (d.view) d.view->cefEvent(ev);
}

Tab::Tab() {
    initPart(client, this);
    initPart(render, this);
    initPart(life, this);
    initPart(load, this);
    initPart(display, this);
    initPart(dl, this);
    initPart(menu, this);
    initPart(js, this);
    initPart(request, this);

    // -- client: hands out the handlers (each answer carries a reference for CEF) --
    client.cef.get_render_handler = [](cef_client_t *self) { Tab *t = ownerOf<cef_client_t, Tab>(self); t->addRef(); return &t->render.cef; };
    client.cef.get_life_span_handler = [](cef_client_t *self) { Tab *t = ownerOf<cef_client_t, Tab>(self); t->addRef(); return &t->life.cef; };
    client.cef.get_load_handler = [](cef_client_t *self) { Tab *t = ownerOf<cef_client_t, Tab>(self); t->addRef(); return &t->load.cef; };
    client.cef.get_display_handler = [](cef_client_t *self) { Tab *t = ownerOf<cef_client_t, Tab>(self); t->addRef(); return &t->display.cef; };
    client.cef.get_download_handler = [](cef_client_t *self) { Tab *t = ownerOf<cef_client_t, Tab>(self); t->addRef(); return &t->dl.cef; };
    client.cef.get_context_menu_handler = [](cef_client_t *self) { Tab *t = ownerOf<cef_client_t, Tab>(self); t->addRef(); return &t->menu.cef; };
    client.cef.get_jsdialog_handler = [](cef_client_t *self) { Tab *t = ownerOf<cef_client_t, Tab>(self); t->addRef(); return &t->js.cef; };
    client.cef.get_request_handler = [](cef_client_t *self) { Tab *t = ownerOf<cef_client_t, Tab>(self); t->addRef(); return &t->request.cef; };

    // -- drawing off screen --
    render.cef.get_view_rect = [](cef_render_handler_t *self, cef_browser_t *b, cef_rect_t *r) {
        Tab *t = ownerOf<cef_render_handler_t, Tab>(self);
        const QSize s = t->view ? t->view->cefViewSize() : QSize(800, 600);
        r->x = 0;
        r->y = 0;
        r->width = qMax(1, s.width());
        r->height = qMax(1, s.height());
        rel(b);
    };
    render.cef.get_screen_info = [](cef_render_handler_t *self, cef_browser_t *b, cef_screen_info_t *info) {
        Tab *t = ownerOf<cef_render_handler_t, Tab>(self);
        rel(b);
        if (!t->view) return 0;
        info->size = sizeof(cef_screen_info_t);
        info->device_scale_factor = float(t->view->cefScale());
        info->depth = 24;
        info->depth_per_component = 8;
        info->is_monochrome = 0;
        const QRect s = t->view->cefScreenRect();
        info->rect = cef_rect_t{s.x(), s.y(), s.width(), s.height()};
        info->available_rect = info->rect;
        return 1;
    };
    render.cef.get_screen_point = [](cef_render_handler_t *self, cef_browser_t *b, int x, int y, int *sx, int *sy) {
        Tab *t = ownerOf<cef_render_handler_t, Tab>(self);
        rel(b);
        if (!t->view) return 0;
        const QPoint p = t->view->cefToScreen(QPoint(x, y));
        *sx = p.x();
        *sy = p.y();
        return 1;
    };
    render.cef.on_popup_show = [](cef_render_handler_t *self, cef_browser_t *b, int show) {
        Tab *t = ownerOf<cef_render_handler_t, Tab>(self);
        rel(b);
        Event ev(Ev::PopupShow, t->id);
        ev.n1 = show ? 1 : 0;
        t->send(ev);
    };
    render.cef.on_popup_size = [](cef_render_handler_t *self, cef_browser_t *b, const cef_rect_t *r) {
        Tab *t = ownerOf<cef_render_handler_t, Tab>(self);
        rel(b);
        Event ev(Ev::PopupRect, t->id);
        ev.rect = QRect(r->x, r->y, r->width, r->height);
        t->send(ev);
    };
    render.cef.on_paint = [](cef_render_handler_t *self, cef_browser_t *b, cef_paint_element_type_t type, size_t, const cef_rect_t *, const void *buffer, int w, int h) {
        Tab *t = ownerOf<cef_render_handler_t, Tab>(self);
        rel(b);
        if (t->view && !t->closing) t->view->cefPaint(t->id, type == PET_POPUP, static_cast<const uchar *>(buffer), w, h);
    };
    render.cef.on_ime_composition_range_changed = [](cef_render_handler_t *self, cef_browser_t *b, const cef_range_t *, size_t n, const cef_rect_t *bounds) {
        Tab *t = ownerOf<cef_render_handler_t, Tab>(self);
        rel(b);
        if (!n || !bounds) return;
        QRect r;
        for (size_t i = 0; i < n; ++i) r = r.united(QRect(bounds[i].x, bounds[i].y, bounds[i].width, bounds[i].height));
        Event ev(Ev::ImeRect, t->id);
        ev.rect = r;
        t->send(ev);
    };
    // Dragging out of the page (a link, a picture) is not supported off screen: CEF is told it ended at once.
    render.cef.start_dragging = [](cef_render_handler_t *, cef_browser_t *b, cef_drag_data_t *data, cef_drag_operations_mask_t, int, int) {
        rel(data);
        rel(b);
        return 0;
    };

    // -- tabs and windows --
    life.cef.on_before_popup = [](cef_life_span_handler_t *self, cef_browser_t *b, cef_frame_t *f, int, const cef_string_t *url, const cef_string_t *,
                                  cef_window_open_disposition_t disp, int, const cef_popup_features_t *, cef_window_info_t *, cef_client_t **,
                                  cef_browser_settings_t *, cef_dictionary_value_t **, int *) {
        Tab *t = ownerOf<cef_life_span_handler_t, Tab>(self);
        rel(f);
        rel(b);
        // No pop-up windows: the address opens in a tab of our own instead.
        const QString u = fromCef(url);
        if (!u.isEmpty()) {
            Event ev(Ev::NewTab, t->id);
            ev.text = u;
            ev.n1 = disp == CEF_WOD_NEW_BACKGROUND_TAB ? 1 : 0;
            t->send(ev);
        }
        return 1;
    };
    life.cef.on_before_close = [](cef_life_span_handler_t *self, cef_browser_t *b) {
        Tab *t = ownerOf<cef_life_span_handler_t, Tab>(self);
        rel(b);
        if (t->browser) {
            cef_browser_t *mine = t->browser;
            t->browser = nullptr;
            rel(mine);
        }
        View *v = t->view;
        t->view = nullptr;
        if (E) {
            E->tabs.remove(t->id);
            if (v && E->views.contains(v)) v->cefEvent(Event(Ev::Closed, t->id));
        }
        t->release();   // the reference the engine kept for the tab
    };

    // -- loading --
    load.cef.on_loading_state_change = [](cef_load_handler_t *self, cef_browser_t *b, int loading, int back, int fwd) {
        Tab *t = ownerOf<cef_load_handler_t, Tab>(self);
        rel(b);
        Event l(Ev::Loading, t->id);
        l.n1 = loading ? 1 : 0;
        t->send(l);
        Event n(Ev::NavState, t->id);
        n.n1 = back ? 1 : 0;
        n.n2 = fwd ? 1 : 0;
        t->send(n);
    };
    // the ad blocker's hiding rules: as soon as the page starts (ads not seen for a moment) and again when it is done
    load.cef.on_load_start = [](cef_load_handler_t *, cef_browser_t *b, cef_frame_t *f, cef_transition_type_t) {
        adblockCss(f);
        rel(f);
        rel(b);
    };
    load.cef.on_load_end = [](cef_load_handler_t *self, cef_browser_t *b, cef_frame_t *f, int status) {
        Tab *t = ownerOf<cef_load_handler_t, Tab>(self);
        adblockCss(f);
        if (isMain(f)) {
            Event ev(Ev::Visited, t->id);
            ev.text = freeStr(f->get_url(f));
            ev.text2 = t->title;
            ev.n1 = status;
            t->send(ev);
        }
        rel(f);
        rel(b);
    };
    load.cef.on_load_error = [](cef_load_handler_t *self, cef_browser_t *b, cef_frame_t *f, cef_errorcode_t code, const cef_string_t *text, const cef_string_t *url) {
        Tab *t = ownerOf<cef_load_handler_t, Tab>(self);
        // ERR_ABORTED: the user stopped it, or a download took the address over
        if (isMain(f) && code != ERR_ABORTED) {
            Event ev(Ev::NavFailed, t->id);
            ev.text = fromCef(url);
            ev.text2 = fromCef(text);
            ev.n1 = int(code);
            t->send(ev);
        }
        rel(f);
        rel(b);
    };

    // -- what the page shows about itself --
    display.cef.on_address_change = [](cef_display_handler_t *self, cef_browser_t *b, cef_frame_t *f, const cef_string_t *url) {
        Tab *t = ownerOf<cef_display_handler_t, Tab>(self);
        if (isMain(f)) {
            t->url = fromCef(url);
            Event ev(Ev::Url, t->id);
            ev.text = t->url;
            t->send(ev);
        }
        rel(f);
        rel(b);
    };
    display.cef.on_title_change = [](cef_display_handler_t *self, cef_browser_t *b, const cef_string_t *title) {
        Tab *t = ownerOf<cef_display_handler_t, Tab>(self);
        rel(b);
        t->title = fromCef(title);
        Event ev(Ev::Title, t->id);
        ev.text = t->title;
        ev.text2 = t->url;
        t->send(ev);
    };
    display.cef.on_favicon_urlchange = [](cef_display_handler_t *self, cef_browser_t *b, cef_string_list_t icons) {
        Tab *t = ownerOf<cef_display_handler_t, Tab>(self);
        rel(b);
        if (!icons || !E->api.list_size(icons)) return;
        cef_string_t s{};
        if (E->api.list_value(icons, 0, &s)) {
            Event ev(Ev::Icon, t->id);
            ev.text = fromCef(&s);
            E->api.str_clear(&s);
            t->send(ev);
        }
    };
    display.cef.on_fullscreen_mode_change = [](cef_display_handler_t *self, cef_browser_t *b, int on) {
        Tab *t = ownerOf<cef_display_handler_t, Tab>(self);
        rel(b);
        Event ev(Ev::FullScreen, t->id);
        ev.n1 = on ? 1 : 0;
        t->send(ev);
    };
    display.cef.on_status_message = [](cef_display_handler_t *self, cef_browser_t *b, const cef_string_t *value) {
        Tab *t = ownerOf<cef_display_handler_t, Tab>(self);
        rel(b);
        Event ev(Ev::Status, t->id);
        ev.text = fromCef(value);
        t->send(ev);
    };
    display.cef.on_console_message = [](cef_display_handler_t *self, cef_browser_t *b, cef_log_severity_t, const cef_string_t *msg, const cef_string_t *, int) {
        Tab *t = ownerOf<cef_display_handler_t, Tab>(self);
        rel(b);
        Event ev(Ev::Console, t->id);
        ev.text = fromCef(msg);
        t->send(ev);
        // our own answers (WebPane.evaluate) stay out of Chromium's log
        return ev.text.startsWith(QLatin1String("\x01rh:")) ? 1 : 0;
    };
    display.cef.on_cursor_change = [](cef_display_handler_t *self, cef_browser_t *b, cef_cursor_handle_t, cef_cursor_type_t type, const cef_cursor_info_t *) {
        Tab *t = ownerOf<cef_display_handler_t, Tab>(self);
        rel(b);
        Event ev(Ev::Cursor, t->id);
        ev.n1 = int(type);
        t->send(ev);
        return 1;
    };

    // -- downloads: straight into the Downloads folder, no dialog --
    dl.cef.can_download = [](cef_download_handler_t *, cef_browser_t *b, const cef_string_t *, const cef_string_t *) {
        rel(b);
        return 1;
    };
    dl.cef.on_before_download = [](cef_download_handler_t *, cef_browser_t *b, cef_download_item_t *item, const cef_string_t *suggested, cef_before_download_callback_t *cb) {
        const quint32 id = item->get_id(item);
        const QString path = downloadPath(fromCef(suggested));
        Engine::Dl &d = E->downloads[id];
        d.view = viewForDownload(b);
        d.path = path;
        Event ev(Ev::DlStart);
        ev.n1 = id;
        ev.text = freeStr(item->get_url(item));
        ev.text2 = path;
        ev.n2 = double(qMax<int64_t>(0, item->get_total_bytes(item)));
        CStr p(path);
        cb->cont(cb, p.p(), 0);
        rel(cb);
        rel(item);
        rel(b);
        sendDl(id, ev);
        return 1;
    };
    dl.cef.on_download_updated = [](cef_download_handler_t *, cef_browser_t *b, cef_download_item_t *item, cef_download_item_callback_t *cb) {
        const quint32 id = item->get_id(item);
        rel(b);
        if (!E->downloads.contains(id)) {   // not started through on_before_download (should not happen)
            rel(cb);
            rel(item);
            return;
        }
        Engine::Dl &d = E->downloads[id];
        // keep the newest handle to cancel / pause / resume with
        if (d.cb) rel(d.cb);
        d.cb = cb;
        const QString path = freeStr(item->get_full_path(item));
        if (!path.isEmpty()) d.path = path;
        QString state = QStringLiteral("progress");
        if (item->is_complete(item)) state = QStringLiteral("done");
        else if (item->is_canceled(item)) state = QStringLiteral("cancelled");
        else if (item->is_interrupted(item)) state = QStringLiteral("failed");
        else if (item->is_paused(item)) state = QStringLiteral("paused");
        Event p(Ev::DlProgress);
        p.n1 = id;
        p.n2 = double(qMax<int64_t>(0, item->get_received_bytes(item)));
        Event s(Ev::DlState);
        s.n1 = id;
        s.text = state;
        s.text2 = d.path;
        const double total = double(qMax<int64_t>(0, item->get_total_bytes(item)));
        rel(item);
        if (total > 0) s.map.insert(QStringLiteral("total"), total);
        sendDl(id, p);
        sendDl(id, s);
        if (state != QLatin1String("progress") && state != QLatin1String("paused")) {
            Engine::Dl &done = E->downloads[id];
            if (done.cb) rel(done.cb);
            done.cb = nullptr;
        }
    };

    // -- the page menu: shown by QML (ContextMenu event); Chromium's own is cancelled --
    menu.cef.run_context_menu = [](cef_context_menu_handler_t *self, cef_browser_t *b, cef_frame_t *f, cef_context_menu_params_t *p, cef_menu_model_t *model, cef_run_context_menu_callback_t *cb) {
        Tab *t = ownerOf<cef_context_menu_handler_t, Tab>(self);
        Event ev(Ev::ContextMenu, t->id);
        const int flags = int(p->get_type_flags(p));
        const int edit = int(p->get_edit_state_flags(p));
        ev.map.insert(QStringLiteral("x"), p->get_xcoord(p));
        ev.map.insert(QStringLiteral("y"), p->get_ycoord(p));
        ev.map.insert(QStringLiteral("link"), (flags & CM_TYPEFLAG_LINK) ? freeStr(p->get_link_url(p)) : QString());
        ev.map.insert(QStringLiteral("image"), (flags & CM_TYPEFLAG_MEDIA) && p->get_media_type(p) == CM_MEDIATYPE_IMAGE ? freeStr(p->get_source_url(p)) : QString());
        ev.map.insert(QStringLiteral("selection"), (flags & CM_TYPEFLAG_SELECTION) ? freeStr(p->get_selection_text(p)) : QString());
        ev.map.insert(QStringLiteral("editable"), p->is_editable(p) != 0);
        ev.map.insert(QStringLiteral("canCopy"), (edit & CM_EDITFLAG_CAN_COPY) != 0);
        ev.map.insert(QStringLiteral("canCut"), (edit & CM_EDITFLAG_CAN_CUT) != 0);
        ev.map.insert(QStringLiteral("canPaste"), (edit & CM_EDITFLAG_CAN_PASTE) != 0);
        ev.map.insert(QStringLiteral("canUndo"), (edit & CM_EDITFLAG_CAN_UNDO) != 0);
        ev.map.insert(QStringLiteral("canRedo"), (edit & CM_EDITFLAG_CAN_REDO) != 0);
        ev.map.insert(QStringLiteral("page"), freeStr(p->get_page_url(p)));
        cb->cancel(cb);
        rel(cb);
        rel(model);
        rel(p);
        rel(f);
        rel(b);
        t->send(ev);
        return 1;
    };

    // -- alert / confirm / prompt: answered by a QML dialog --
    js.cef.on_jsdialog = [](cef_jsdialog_handler_t *self, cef_browser_t *b, const cef_string_t *origin, cef_jsdialog_type_t type, const cef_string_t *msg, const cef_string_t *def,
                            cef_jsdialog_callback_t *cb, int *suppress) {
        Tab *t = ownerOf<cef_jsdialog_handler_t, Tab>(self);
        rel(b);
        if (!t->view) {
            *suppress = 1;
            rel(cb);
            return 0;
        }
        const int id = E->nextDialog++;
        E->dialogs.insert(id, Engine::Dlg{t->id, cb});   // its reference is kept until the answer
        Event ev(Ev::JsDialog, t->id);
        ev.n1 = id;
        ev.map.insert(QStringLiteral("kind"), type == JSDIALOGTYPE_ALERT ? QStringLiteral("alert") : (type == JSDIALOGTYPE_CONFIRM ? QStringLiteral("confirm") : QStringLiteral("prompt")));
        ev.map.insert(QStringLiteral("message"), fromCef(msg));
        ev.map.insert(QStringLiteral("value"), fromCef(def));
        ev.map.insert(QStringLiteral("origin"), fromCef(origin));
        t->send(ev);
        return 1;
    };
    // "Leave this page?": always yes (closing a tab must not hang on a question)
    js.cef.on_before_unload_dialog = [](cef_jsdialog_handler_t *, cef_browser_t *b, const cef_string_t *, int, cef_jsdialog_callback_t *cb) {
        rel(b);
        cb->cont(cb, 1, nullptr);
        rel(cb);
        return 1;
    };
    js.cef.on_reset_dialog_state = [](cef_jsdialog_handler_t *self, cef_browser_t *b) {
        Tab *t = ownerOf<cef_jsdialog_handler_t, Tab>(self);
        rel(b);
        for (auto it = E->dialogs.begin(); it != E->dialogs.end();) {
            if (it->tab == t->id) {
                Event ev(Ev::JsDialogGone, t->id);
                ev.n1 = it.key();
                rel(it->cb);
                it = E->dialogs.erase(it);
                t->send(ev);
            } else {
                ++it;
            }
        }
    };

    // -- navigation rules --
    request.cef.on_before_browse = [](cef_request_handler_t *self, cef_browser_t *b, cef_frame_t *f, cef_request_t *req, int, int) {
        Tab *t = ownerOf<cef_request_handler_t, Tab>(self);
        const QString url = freeStr(req->get_url(req));
        rel(req);
        rel(f);
        rel(b);
        // mailto:, magnet:, tel:… belong to other programs
        const QString scheme = url.section(':', 0, 0).toLower();
        static const QStringList own = {"http", "https", "about", "data", "blob", "file", "chrome", "devtools", "javascript", "view-source", "chrome-error"};
        if (!scheme.isEmpty() && !own.contains(scheme)) {
            Event ev(Ev::External, t->id);
            ev.text = url;
            t->send(ev);
            return 1;
        }
        return 0;
    };
    // The ad blocker: each request of a page (Chromium's network thread) is checked against the filter lists; a
    // blocked one gets the handler that cancels it, the others none (Chromium's default).
    request.cef.get_resource_request_handler = [](cef_request_handler_t *, cef_browser_t *b, cef_frame_t *f, cef_request_t *req, int, int isDownload, const cef_string_t *initiator, int *) -> cef_resource_request_handler_t * {
        bool block = false;
        const cef_resource_type_t type = req->get_resource_type(req);
        // IP protection changed after Chromium started (its proxy is fixed at start): nothing goes out until the
        // app is opened again, so the address is never shown (kill switch)
        if (E && !E->down && rh_privacy_port() != g_cefProxyPort) block = true;
        else if (!isDownload && type != RT_MAIN_FRAME && E && !E->down) {
            const QString url = freeStr(req->get_url(req));
            // the page the request belongs to: the tab's page (an allowed site is a whole page, as in uBlock)
            QString source;
            if (cef_frame_t *main = b ? b->get_main_frame(b) : nullptr) {
                source = freeStr(main->get_url(main));
                rel(main);
            }
            if (source.isEmpty()) source = fromCef(initiator);
            if (!source.isEmpty())
                block = rh_adblock_check(url.toUtf8().constData(), source.toUtf8().constData(), adblockKind(type)) != 0;
        }
        rel(req);
        rel(f);
        rel(b);
        return block ? &E->blocked : nullptr;
    };
    // Middle click, Ctrl+click: a new tab
    request.cef.on_open_urlfrom_tab = [](cef_request_handler_t *self, cef_browser_t *b, cef_frame_t *f, const cef_string_t *url, cef_window_open_disposition_t disp, int) {
        Tab *t = ownerOf<cef_request_handler_t, Tab>(self);
        rel(f);
        rel(b);
        Event ev(Ev::NewTab, t->id);
        ev.text = fromCef(url);
        ev.n1 = disp == CEF_WOD_NEW_BACKGROUND_TAB ? 1 : 0;
        t->send(ev);
        return 1;
    };
    request.cef.on_render_process_terminated = [](cef_request_handler_t *self, cef_browser_t *b, cef_termination_status_t, int code, const cef_string_t *why) {
        Tab *t = ownerOf<cef_request_handler_t, Tab>(self);
        rel(b);
        Event ev(Ev::Crashed, t->id);
        ev.text = fromCef(why);
        ev.n1 = code;
        t->send(ev);
    };
}

// ---- the app (switches of Chromium) and the browser process (the message pump) ----

void CEF_CALLBACK onBeforeCommandLine(cef_app_t *, const cef_string_t *processType, cef_command_line_t *cl) {
    if (processType && processType->length) return;   // only the browser process
    auto sw = [cl](const char *name) { CStr n(QString::fromLatin1(name)); cl->append_switch(cl, n.p()); };
    auto swv = [cl](const char *name, const QString &v) { CStr n(QString::fromLatin1(name)), val(v); cl->append_switch_with_value(cl, n.p(), val.p()); };
    // IP protection (src/privacy.rs): the pages go through the core's gateway (SOCKS5, names resolved there), and
    // WebRTC may not open UDP around it (it would show the real address)
    g_cefProxyPort = rh_privacy_port();
    if (g_cefProxyPort > 0) {
        swv("proxy-server", QStringLiteral("socks5://127.0.0.1:%1").arg(g_cefProxyPort));
        swv("proxy-bypass-list", QStringLiteral("<-loopback>"));
        swv("force-webrtc-ip-handling-policy", QStringLiteral("disable_non_proxied_udp"));
    }
    // without the sandbox (it needs a special build of the program on Windows)
    sw("no-sandbox");
#ifdef Q_OS_LINUX
    // no prompt of the desktop keyring for saved passwords
    swv("password-store", QStringLiteral("basic"));
#endif
    // a machine without a usable GPU (a virtual machine, or the variable set by hand): software drawing
    if (qEnvironmentVariableIsSet("REELHOUSE_CEF_NO_GPU")) {
        sw("disable-gpu");
        sw("disable-gpu-compositing");
    }
    // Chrome's own "Translate this page?", sign-in and first-run prompts have no window here; its AI models and
    // components (hundreds of MB in the profile) are not needed by these pages
    swv("disable-features", QStringLiteral("Translate,MediaRouter,DialMediaRouteProvider,OptimizationGuideModelDownloading,OptimizationHintsFetching,OptimizationTargetPrediction,OptimizationHints"));
    sw("disable-component-update");
    sw("no-first-run");
    sw("no-default-browser-check");
    // extra switches for testing: REELHOUSE_CEF_ARGS="--a --b=c"
    for (const QString &a : qEnvironmentVariable("REELHOUSE_CEF_ARGS").split(' ', Qt::SkipEmptyParts)) {
        const QString s = a.startsWith(QLatin1String("--")) ? a.mid(2) : a;
        const int eq = s.indexOf('=');
        if (eq > 0) swv(s.left(eq).toLatin1().constData(), s.mid(eq + 1));
        else sw(s.toLatin1().constData());
    }
}

cef_browser_process_handler_t *CEF_CALLBACK getBrowserProcessHandler(cef_app_t *) { return &E->bph; }

void CEF_CALLBACK onScheduleWork(cef_browser_process_handler_t *, int64_t delayMs) {
    // may come from any thread of Chromium: the work itself always runs on the GUI thread
    if (!E || !E->pumpTarget) return;
    if (QThread::currentThread() == E->pumpTarget->thread()) {
        schedule(delayMs);
    } else {
        const qint64 d = delayMs;
        QMetaObject::invokeMethod(E->pumpTarget, [d] { schedule(d); }, Qt::QueuedConnection);
    }
}

QString libName() {
#ifdef Q_OS_WIN
    return QStringLiteral("libcef.dll");
#else
    return QStringLiteral("libcef.so");
#endif
}

QString helperName() {
#ifdef Q_OS_WIN
    return QStringLiteral("reelhouse-cef-helper.exe");
#else
    return QStringLiteral("reelhouse-cef-helper");
#endif
}

// Chromium's language name for ours ("pt" -> "pt-BR").
QString chromeLocale(const QString &lang) {
    const QString l = lang.toLower();
    if (l.startsWith(QLatin1String("pt"))) return QStringLiteral("pt-BR");
    if (l.startsWith(QLatin1String("es"))) return QStringLiteral("es");
    if (l.startsWith(QLatin1String("ja"))) return QStringLiteral("ja");
    return QStringLiteral("en-US");
}

QString acceptLanguages(const QString &lang) {
    const QString loc = chromeLocale(lang);
    if (loc == QLatin1String("pt-BR")) return QStringLiteral("pt-BR,pt,en-US,en");
    if (loc == QLatin1String("es")) return QStringLiteral("es,es-419,en-US,en");
    if (loc == QLatin1String("ja")) return QStringLiteral("ja,en-US,en");
    return QStringLiteral("en-US,en");
}

void setStr(cef_string_t &dst, const QString &value, QStringList &keep) {
    keep.append(value);   // the text must stay alive while CEF reads the settings
    const QString &q = keep.last();
    dst.str = reinterpret_cast<char16_t *>(const_cast<ushort *>(reinterpret_cast<const ushort *>(q.utf16())));
    dst.length = size_t(q.size());
    dst.dtor = nullptr;
}

}  // namespace

// ---- the API ----

// Whether libcef and the helper are next to the program: only the files are looked at (libcef is big and loaded
// when the first page opens).
bool probe(QString &info) {
    Engine &e = eng();
    if (!e.probed) {
        e.probed = true;
        // Next to the program; REELHOUSE_CEF_DIR points elsewhere for development.
        e.dir = qEnvironmentVariable("REELHOUSE_CEF_DIR");
        if (e.dir.isEmpty()) e.dir = QCoreApplication::applicationDirPath();
        if (!QFileInfo::exists(QDir(e.dir).filePath(libName()))) {
            e.problem = QStringLiteral("The Chromium engine (libcef) is not next to the program.");
            e.failed = true;
        } else if (!QFileInfo::exists(QDir(e.dir).filePath(helperName()))) {
            e.problem = QStringLiteral("The Chromium helper (reelhouse-cef-helper) is not next to the program.");
            e.failed = true;
        } else {
            e.present = true;
        }
    }
    info = e.failed ? e.problem : e.version;
    return !e.failed;
}

namespace {
// Loads libcef and checks it is the version this program was built for.
bool loadLibrary(Engine &e) {
    e.lib.setFileName(QDir(e.dir).filePath(libName()));
    if (!e.lib.load()) {
        e.problem = QStringLiteral("The Chromium engine could not be loaded: %1").arg(e.lib.errorString());
        return false;
    }
    Api &a = e.api;
    a.api_hash = reinterpret_cast<decltype(a.api_hash)>(e.lib.resolve("cef_api_hash"));
    a.initialize = reinterpret_cast<decltype(a.initialize)>(e.lib.resolve("cef_initialize"));
    a.shutdown = reinterpret_cast<decltype(a.shutdown)>(e.lib.resolve("cef_shutdown"));
    a.do_work = reinterpret_cast<decltype(a.do_work)>(e.lib.resolve("cef_do_message_loop_work"));
    a.create_browser = reinterpret_cast<decltype(a.create_browser)>(e.lib.resolve("cef_browser_host_create_browser_sync"));
    a.userfree_free = reinterpret_cast<decltype(a.userfree_free)>(e.lib.resolve("cef_string_userfree_utf16_free"));
    a.list_size = reinterpret_cast<decltype(a.list_size)>(e.lib.resolve("cef_string_list_size"));
    a.list_value = reinterpret_cast<decltype(a.list_value)>(e.lib.resolve("cef_string_list_value"));
    a.str_clear = reinterpret_cast<decltype(a.str_clear)>(e.lib.resolve("cef_string_utf16_clear"));
    a.version_info = reinterpret_cast<decltype(a.version_info)>(e.lib.resolve("cef_version_info"));
    if (!a.api_hash || !a.initialize || !a.shutdown || !a.do_work || !a.create_browser || !a.userfree_free || !a.list_size || !a.list_value || !a.str_clear) {
        e.problem = QStringLiteral("This libcef is not the version Reelhouse was built for.");
        return false;
    }
    // Tells libcef which version of its API we were compiled against; it answers with its hash.
    const char *hash = a.api_hash(CEF_API_VERSION, 0);
    if (!hash || std::strcmp(hash, CEF_API_HASH_PLATFORM) != 0) {
        e.problem = QStringLiteral("This libcef is not the version Reelhouse was built for.");
        return false;
    }
    if (a.version_info)
        e.version = QStringLiteral("Chromium %1.%2.%3.%4 (CEF %5.%6.%7)").arg(a.version_info(4)).arg(a.version_info(5)).arg(a.version_info(6)).arg(a.version_info(7))
                        .arg(a.version_info(0)).arg(a.version_info(1)).arg(a.version_info(2));
    return true;
}
}  // namespace

QString version() { return E ? E->version : QString(); }

void configure(const QString &profileDir, const QString &language) {
    Engine &e = eng();
    if (e.isStarted) return;   // fixed once Chromium runs
    if (!profileDir.isEmpty()) e.profile = profileDir;
    if (!language.isEmpty()) e.language = language;
}

bool started() { return E && E->isStarted && !E->down; }

bool start(QString &problem) {
    Engine &e = eng();
    if (e.isStarted && !e.down) return true;
    QString info;
    if (e.down || !probe(info) || e.failed) {
        problem = e.down ? QStringLiteral("The browser engine has stopped.") : e.problem;
        return false;
    }
    QElapsedTimer clock;   // the first page waits for this: the log says how long (the owner's Windows logs)
    clock.start();
    if (!loadLibrary(e)) {
        e.failed = true;
        problem = e.problem;
        return false;
    }
    initStatic(e.app);
    e.app.on_before_command_line_processing = onBeforeCommandLine;
    e.app.get_browser_process_handler = getBrowserProcessHandler;
    initStatic(e.bph);
    e.bph.on_schedule_message_pump_work = onScheduleWork;
    initStatic(e.blocked);
    e.blocked.on_before_resource_load = [](cef_resource_request_handler_t *, cef_browser_t *b, cef_frame_t *f, cef_request_t *req, cef_callback_t *cb) {
        rel(cb);
        rel(req);
        rel(f);
        rel(b);
        return RV_CANCEL;
    };

    QString root = e.profile;
    if (root.isEmpty()) root = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/browser";
    root = QDir(root).absoluteFilePath(QStringLiteral("cef"));
    QDir().mkpath(root + "/Default");

    static QStringList keep;   // texts the settings point into
    cef_settings_t s;
    std::memset(&s, 0, sizeof(s));
    s.size = sizeof(s);
    s.no_sandbox = 1;
    setStr(s.browser_subprocess_path, QDir::toNativeSeparators(QDir(e.dir).filePath(helperName())), keep);
    s.multi_threaded_message_loop = 0;
    s.external_message_pump = 1;
    s.windowless_rendering_enabled = 1;
    // the app's own arguments (a file to open…) are not Chromium's
    s.command_line_args_disabled = 1;
    setStr(s.root_cache_path, QDir::toNativeSeparators(root), keep);
    setStr(s.cache_path, QDir::toNativeSeparators(root + "/Default"), keep);
    s.persist_session_cookies = 1;
    setStr(s.locale, chromeLocale(e.language), keep);
    setStr(s.accept_language_list, acceptLanguages(e.language), keep);
    setStr(s.log_file, QDir::toNativeSeparators(QDir(QStandardPaths::writableLocation(QStandardPaths::TempLocation)).filePath(QStringLiteral("reelhouse-cef.log"))), keep);
    s.log_severity = qEnvironmentVariableIsSet("REELHOUSE_CEF_LOG") ? LOGSEVERITY_INFO : LOGSEVERITY_ERROR;
    setStr(s.resources_dir_path, QDir::toNativeSeparators(e.dir), keep);
    setStr(s.locales_dir_path, QDir::toNativeSeparators(QDir(e.dir).filePath(QStringLiteral("locales"))), keep);
    s.background_color = 0xFFFFFFFF;
    // Qt and the core keep their own signal handling (Ctrl+C in a terminal, child processes)
    s.disable_signal_handlers = 1;

    cef_main_args_t args;
    std::memset(&args, 0, sizeof(args));
#ifdef Q_OS_WIN
    args.instance = GetModuleHandleW(nullptr);
#else
    static QByteArray arg0 = QCoreApplication::applicationFilePath().toLocal8Bit();
    static char *argv[] = {arg0.data(), nullptr};
    args.argc = 1;
    args.argv = argv;
#endif

    // The pump: Chromium's work runs on this (the GUI) thread, between Qt's events.
    e.pumpTarget = new QObject(QCoreApplication::instance());
    e.pumpTimer = new QTimer(e.pumpTarget);
    e.pumpTimer->setSingleShot(true);
    e.pumpTimer->setTimerType(Qt::PreciseTimer);
    QObject::connect(e.pumpTimer, &QTimer::timeout, e.pumpTarget, [] { doWork(); });
    e.idleTimer = new QTimer(e.pumpTarget);
    e.idleTimer->setInterval(1000 / 30);
    QObject::connect(e.idleTimer, &QTimer::timeout, e.pumpTarget, [] { doWork(); });
    QObject::connect(qApp, &QCoreApplication::aboutToQuit, e.pumpTarget, [] { shutdown(); });

    const int ok = e.api.initialize(&args, &s, &e.app, nullptr);
    if (!ok) {
        e.failed = true;
        e.problem = QStringLiteral("The Chromium engine could not start (is Reelhouse already open with the same profile?).");
        problem = e.problem;
        return false;
    }
    e.isStarted = true;
    e.idleTimer->start();
    qInfo("[web] Chromium engine started in %lld ms (libcef load + cef_initialize, GUI thread)", (long long)clock.elapsed());
    return true;
}

int openTab(View *view, const QString &url, bool hidden) {
    QString why;
    if (!view || !start(why)) return 0;
    Engine &e = eng();
    e.views.insert(view);
    Tab *t = new Tab;   // its first reference is the engine's, given back in on_before_close
    t->id = e.nextTab++;
    t->view = view;
    t->hidden = hidden;

    cef_window_info_t wi;
    std::memset(&wi, 0, sizeof(wi));
    wi.size = sizeof(wi);
    wi.windowless_rendering_enabled = 1;
    wi.shared_texture_enabled = 0;
    wi.runtime_style = CEF_RUNTIME_STYLE_ALLOY;   // the only style that draws off screen
    // the app's window: Chromium places IME windows and native dialogs (file choosers) from it
#ifdef Q_OS_WIN
    wi.parent_window = reinterpret_cast<cef_window_handle_t>(view->cefNativeWindow());
#else
    wi.parent_window = static_cast<cef_window_handle_t>(view->cefNativeWindow());
#endif

    cef_browser_settings_t bs;
    std::memset(&bs, 0, sizeof(bs));
    bs.size = sizeof(bs);
    bs.windowless_frame_rate = 60;
    bs.background_color = 0xFFFFFFFF;

    e.tabs.insert(t->id, t);
    const QString start = url.isEmpty() ? QStringLiteral("about:blank") : url;
    CStr u(start);
    t->addRef();   // the reference CEF takes with the client
    QElapsedTimer clock;
    clock.start();
    cef_browser_t *b = e.api.create_browser(&wi, &t->client.cef, u.p(), &bs, nullptr, nullptr);
    qInfo("[web] page view %d created in %lld ms", t->id, (long long)clock.elapsed());
    if (!b) {
        e.tabs.remove(t->id);
        t->release();
        return 0;
    }
    t->browser = b;
    t->url = start;
    if (hidden) setHidden(t->id, true);
    return t->id;
}

void closeTab(int id) {
    Tab *t = tabOf(id);
    if (!t || t->closing) return;
    cef_browser_host_t *h = t->host();
    t->closing = true;
    t->view = nullptr;
    // pending dialogs of the tab: no answer will come
    for (auto it = E->dialogs.begin(); it != E->dialogs.end();) {
        if (it->tab == id) {
            it->cb->cont(it->cb, 0, nullptr);
            rel(it->cb);
            it = E->dialogs.erase(it);
        } else {
            ++it;
        }
    }
    if (h) {
        h->close_browser(h, 1);
        rel(h);
    }
}

void forgetView(View *view) {
    if (!E) return;
    QList<int> ids;
    for (Tab *t : std::as_const(E->tabs))
        if (t->view == view) ids.append(t->id);
    for (int id : ids) closeTab(id);
    E->views.remove(view);
    for (Engine::Dl &d : E->downloads)
        if (d.view == view) d.view = nullptr;
}

void navigate(int id, const QString &url) {
    withFrame(id, false, [&](Tab *t, cef_frame_t *f) {
        t->url = url;
        CStr u(url);
        f->load_url(f, u.p());
    });
}

void goBack(int id) { Tab *t = tabOf(id); if (t && t->browser && !t->closing) t->browser->go_back(t->browser); }
void goForward(int id) { Tab *t = tabOf(id); if (t && t->browser && !t->closing) t->browser->go_forward(t->browser); }
void reload(int id) { Tab *t = tabOf(id); if (t && t->browser && !t->closing) t->browser->reload(t->browser); }
void stop(int id) { Tab *t = tabOf(id); if (t && t->browser && !t->closing) t->browser->stop_load(t->browser); }

void setZoom(int id, double factor) {
    // Chromium's zoom levels: each step is 20 % (level = log base 1.2 of the factor)
    withHost(id, [&](Tab *, cef_browser_host_t *h) { h->set_zoom_level(h, std::log(qBound(0.25, factor, 5.0)) / std::log(1.2)); });
}

void setHidden(int id, bool hidden) {
    withHost(id, [&](Tab *t, cef_browser_host_t *h) {
        t->hidden = hidden;
        h->was_hidden(h, hidden ? 1 : 0);
        if (!hidden) h->invalidate(h, PET_VIEW);
    });
}

void resized(int id) { withHost(id, [](Tab *, cef_browser_host_t *h) { h->was_resized(h); }); }
void screenChanged(int id) { withHost(id, [](Tab *, cef_browser_host_t *h) { h->notify_screen_info_changed(h); h->was_resized(h); }); }
void invalidate(int id) { withHost(id, [](Tab *, cef_browser_host_t *h) { h->invalidate(h, PET_VIEW); }); }
void setFrameRate(int id, int fps) { withHost(id, [&](Tab *, cef_browser_host_t *h) { h->set_windowless_frame_rate(h, qBound(1, fps, 60)); }); }
void setFocus(int id, bool focus) { withHost(id, [&](Tab *, cef_browser_host_t *h) { h->set_focus(h, focus ? 1 : 0); }); }

void mouseMove(int id, int x, int y, uint mods, bool leave) {
    withHost(id, [&](Tab *, cef_browser_host_t *h) {
        cef_mouse_event_t ev{x, y, mods};
        h->send_mouse_move_event(h, &ev, leave ? 1 : 0);
    });
}

void mouseClick(int id, int x, int y, uint mods, int button, bool up, int clicks) {
    withHost(id, [&](Tab *, cef_browser_host_t *h) {
        cef_mouse_event_t ev{x, y, mods};
        h->send_mouse_click_event(h, &ev, button == 2 ? MBT_RIGHT : (button == 1 ? MBT_MIDDLE : MBT_LEFT), up ? 1 : 0, qBound(1, clicks, 3));
    });
}

void mouseWheel(int id, int x, int y, uint mods, int dx, int dy) {
    withHost(id, [&](Tab *, cef_browser_host_t *h) {
        cef_mouse_event_t ev{x, y, mods};
        h->send_mouse_wheel_event(h, &ev, dx, dy);
    });
}

void key(int id, int kind, int windowsKey, int nativeKey, uint mods, ushort character, ushort unmodified, bool systemKey) {
    withHost(id, [&](Tab *, cef_browser_host_t *h) {
        cef_key_event_t ev;
        std::memset(&ev, 0, sizeof(ev));
        ev.size = sizeof(ev);
        ev.type = cef_key_event_type_t(kind);
        ev.modifiers = mods;
        ev.windows_key_code = windowsKey;
        ev.native_key_code = nativeKey;
        ev.is_system_key = systemKey ? 1 : 0;
        ev.character = char16_t(character);
        ev.unmodified_character = char16_t(unmodified);
        h->send_key_event(h, &ev);
    });
}

void imeComposition(int id, const QString &text, int cursor) {
    withHost(id, [&](Tab *, cef_browser_host_t *h) {
        CStr t(text);
        cef_composition_underline_t line;
        std::memset(&line, 0, sizeof(line));
        line.size = sizeof(line);
        line.range = cef_range_t{0, uint32_t(text.size())};
        line.color = 0xFF000000;
        line.background_color = 0;
        line.thick = 0;
        const uint32_t c = uint32_t(qBound(0, cursor, int(text.size())));
        cef_range_t replace{UINT32_MAX, UINT32_MAX};   // "invalid": replace the current composition
        cef_range_t sel{c, c};
        h->ime_set_composition(h, t.p(), 1, &line, &replace, &sel);
    });
}

void imeCommit(int id, const QString &text) {
    withHost(id, [&](Tab *, cef_browser_host_t *h) {
        CStr t(text);
        cef_range_t replace{UINT32_MAX, UINT32_MAX};
        h->ime_commit_text(h, t.p(), &replace, 0);
    });
}

void imeCancel(int id) { withHost(id, [](Tab *, cef_browser_host_t *h) { h->ime_cancel_composition(h); }); }
void captureLost(int id) { withHost(id, [](Tab *, cef_browser_host_t *h) { h->send_capture_lost_event(h); }); }

void runScript(int id, const QString &code) {
    withFrame(id, false, [&](Tab *, cef_frame_t *f) {
        CStr c(code), where(QStringLiteral("reelhouse://script"));
        f->execute_java_script(f, c.p(), where.p(), 1);
    });
}

void editCommand(int id, const QString &what) {
    withFrame(id, true, [&](Tab *, cef_frame_t *f) {
        if (what == QLatin1String("copy")) f->copy(f);
        else if (what == QLatin1String("cut")) f->cut(f);
        else if (what == QLatin1String("paste")) f->paste(f);
        else if (what == QLatin1String("selectAll")) f->select_all(f);
        else if (what == QLatin1String("undo")) f->undo(f);
        else if (what == QLatin1String("redo")) f->redo(f);
        else if (what == QLatin1String("delete")) f->del(f);
    });
}

void download(int id, const QString &url) {
    withHost(id, [&](Tab *, cef_browser_host_t *h) {
        CStr u(url);
        h->start_download(h, u.p());
    });
}

void answerDialog(int dialog, bool ok, const QString &text) {
    if (!E || !E->dialogs.contains(dialog)) return;
    Engine::Dlg d = E->dialogs.take(dialog);
    CStr t(text);
    d.cb->cont(d.cb, ok ? 1 : 0, t.p());
    rel(d.cb);
}

void find(int id, const QString &text, bool forward, bool next) {
    withHost(id, [&](Tab *, cef_browser_host_t *h) {
        if (text.isEmpty()) {
            h->stop_finding(h, 1);
            return;
        }
        CStr t(text);
        h->find(h, t.p(), forward ? 1 : 0, 0, next ? 1 : 0);
    });
}

void cancelDownload(int id) {
    if (E && E->downloads.contains(quint32(id)) && E->downloads[quint32(id)].cb) {
        cef_download_item_callback_t *cb = E->downloads[quint32(id)].cb;
        cb->cancel(cb);
    }
}
void pauseDownload(int id) {
    if (E && E->downloads.contains(quint32(id)) && E->downloads[quint32(id)].cb) {
        cef_download_item_callback_t *cb = E->downloads[quint32(id)].cb;
        cb->pause(cb);
    }
}
void resumeDownload(int id) {
    if (E && E->downloads.contains(quint32(id)) && E->downloads[quint32(id)].cb) {
        cef_download_item_callback_t *cb = E->downloads[quint32(id)].cb;
        cb->resume(cb);
    }
}

void shutdown() {
    if (!E || !E->isStarted || E->down) return;
    Engine &e = *E;
    // every page closes, then Chromium gets a little time to let them go before it stops
    for (Tab *t : e.tabs.values()) {
        t->view = nullptr;
        if (!t->closing) {
            t->closing = true;
            if (cef_browser_host_t *h = t->host()) {
                h->close_browser(h, 1);
                rel(h);
            }
        }
    }
    for (auto it = e.dialogs.begin(); it != e.dialogs.end(); ++it) {
        it->cb->cont(it->cb, 0, nullptr);
        rel(it->cb);
    }
    e.dialogs.clear();
    for (Engine::Dl &d : e.downloads) {
        if (d.cb) rel(d.cb);
        d.cb = nullptr;
    }
    e.views.clear();
    QElapsedTimer clock;
    clock.start();
    while (!e.tabs.isEmpty() && clock.elapsed() < 3000) {
        e.inWork = true;
        e.api.do_work();
        e.inWork = false;
        QThread::msleep(5);
    }
    for (int i = 0; i < 10; ++i) e.api.do_work();
    e.down = true;
    e.pumpTimer->stop();
    e.idleTimer->stop();
    e.api.shutdown();
}

}  // namespace cefh

#endif  // REELHOUSE_CEF
