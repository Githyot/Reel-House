#pragma once
// Chromium (CEF, the Chromium Embedded Framework, BSD licence) for Research mode and the player's quick search.
// The pages are drawn off screen: Chromium hands over each finished picture and WebPane (webpane.cpp) draws it as a
// texture of the Qt Quick scene, so menus, banners, the mascot and dialogs can sit on top of a page like on anything
// else, the same way on Windows and Linux.
// This file is the small engine API WebPane sees; cef_host.cpp talks to libcef through its plain C API (no MSVC-only
// C++ wrapper, so MinGW builds it) and loads libcef at run time: without it next to the program the app still starts
// and Research mode opens pages in the system browser. Every call must come from the GUI thread; Chromium's UI thread
// is this thread too (its message loop is pumped from a Qt timer), so the View callbacks arrive here as well.
#include <QImage>
#include <QPoint>
#include <QRect>
#include <QSize>
#include <QString>
#include <QVariantMap>
#include <QVector>

namespace cefh {

enum class Ev {
    Failed,       // the engine could not start: text = why
    Url,          // text = address of the main frame
    Title,        // text = title, text2 = the address it belongs to
    Icon,         // text = favicon address
    Loading,      // n1 = 1 while a page loads, 0 when done
    NavState,     // n1 = can go back, n2 = can go forward
    Visited,      // the main frame finished loading: text = address, text2 = title, n1 = HTTP status
    NavFailed,    // text = address, text2 = Chromium's error name, n1 = error code
    NewTab,       // a page asked for a new window, or a middle / Ctrl click: text = address, n1 = 1 in the background
    External,     // a link the browser does not open itself (mailto:, magnet:…): text = address
    FullScreen,   // n1 = 1 when a page element (a video) went full screen, 0 when it left
    Cursor,       // n1 = the CEF cursor type
    Status,       // text = the status line (the address of the link under the pointer), "" = none
    Console,      // text = a console message of the page (WebPane looks for its own script answers in them)
    ContextMenu,  // map = {x, y, link, image, selection, editable, canCopy, canPaste, canCut, canUndo, canRedo, page}
    JsDialog,     // n1 = dialog id; map = {kind: alert|confirm|prompt, message, value, origin}
    JsDialogGone, // n1 = dialog id: the page took the dialog back (it navigated away)
    PopupShow,    // n1 = 1 when a drop-down list (a <select>) opens, 0 when it closes
    PopupRect,    // rect = where the drop-down list is drawn (view coordinates)
    ImeRect,      // rect = the text being composed with an input method (view coordinates)
    Crashed,      // the page's process stopped: text = why
    DlStart,      // n1 = download id; text = address, text2 = file path; n2 = total bytes (0 = unknown)
    DlProgress,   // n1 = download id; n2 = bytes received
    DlState,      // n1 = download id; text = "progress", "paused", "done", "failed" or "cancelled"; text2 = file path
    Closed,       // the tab's page is gone (after closeTab, or by itself)
    Message,      // text = something to tell the user
};

struct Event {
    explicit Event(Ev k, int t = 0) : kind(k), tab(t) {}
    Ev kind;
    int tab = 0;
    QString text, text2;
    double n1 = 0, n2 = 0;
    QRect rect;
    QVariantMap map;
};

// What a tab draws into and reports to: a WebPane.
class View {
public:
    virtual ~View() = default;
    // The page size in logical pixels (CEF's "device independent pixels") and the device pixels per logical pixel
    // (the screen's scale times the app's "Size in general").
    virtual QSize cefViewSize() const = 0;
    virtual qreal cefScale() const = 0;
    // A point of the page on the screen (logical screen coordinates), and the screen itself.
    virtual QPoint cefToScreen(const QPoint &viewPoint) const = 0;
    virtual QRect cefScreenRect() const = 0;
    // The native window the page belongs to (an HWND or an X11 window), 0 when not known yet.
    virtual quintptr cefNativeWindow() const = 0;
    // A new picture: BGRA, premultiplied, `w`×`h` device pixels, valid only during the call. `popup` = the drop-down
    // list of a <select>, drawn over the page at the PopupRect.
    virtual void cefPaint(int tab, bool popup, const uchar *bgra, int w, int h) = 0;
    virtual void cefEvent(const Event &e) = 0;
};

// Whether the engine is there (libcef and its helper next to the program); `info` gets what is missing. Only looks
// at the files: libcef is loaded by start().
bool probe(QString &info);
// "Chromium 154.0.8037.98 (CEF 154.0.34)" once started.
QString version();
// Before the first tab: the profile folder (cookies, logins, caches; a "cef" folder is made inside) and the language.
void configure(const QString &profileDir, const QString &language);
// Starts Chromium the first time (it stays up until the app quits). False with the reason when it cannot.
bool start(QString &problem);
bool started();

// A tab: a page of its own drawn into `view`. Returns its id, 0 when the engine is not running.
int openTab(View *view, const QString &url, bool hidden);
// Closes the page; the view gets nothing more from it (a last Closed event comes when Chromium let it go).
void closeTab(int tab);
// The view is going away: its tabs close and nothing more is sent to it.
void forgetView(View *view);

void navigate(int tab, const QString &url);
void goBack(int tab);
void goForward(int tab);
void reload(int tab);
void stop(int tab);
void setZoom(int tab, double factor);
// Shown or hidden (a hidden page stops drawing and slows its timers).
void setHidden(int tab, bool hidden);
// The size or the scale changed: Chromium asks the view again and draws anew.
void resized(int tab);
void screenChanged(int tab);
void invalidate(int tab);
// Pictures a second the page is drawn at (a hidden page kept running needs few).
void setFrameRate(int tab, int fps);
void setFocus(int tab, bool focus);

// Input, in view coordinates (logical pixels). Mouse buttons: 0 left, 1 middle, 2 right. `mods` are CEF EVENTFLAG_* bits.
void mouseMove(int tab, int x, int y, uint mods, bool leave);
void mouseClick(int tab, int x, int y, uint mods, int button, bool up, int clicks);
void mouseWheel(int tab, int x, int y, uint mods, int dx, int dy);
// kind: 0 raw key down, 2 key up, 3 character (CEF's cef_key_event_type_t).
void key(int tab, int kind, int windowsKey, int nativeKey, uint mods, ushort character, ushort unmodified, bool systemKey);
void imeComposition(int tab, const QString &text, int cursor);
void imeCommit(int tab, const QString &text);
void imeCancel(int tab);
void captureLost(int tab);

// Runs a script in the page (main frame).
void runScript(int tab, const QString &code);
// Edit commands of the focused frame: "copy", "cut", "paste", "selectAll", "undo", "redo", "delete".
void editCommand(int tab, const QString &what);
// Saves the address as a download (the "Save image" / "Save link" of the page menu).
void download(int tab, const QString &url);
// Answers a page's alert / confirm / prompt.
void answerDialog(int dialog, bool ok, const QString &text);
// Find in page; an empty text stops it.
void find(int tab, const QString &text, bool forward, bool next);

void cancelDownload(int id);
void pauseDownload(int id);
void resumeDownload(int id);

// Closes every page and stops Chromium (the app is quitting).
void shutdown();

}  // namespace cefh
