#include "webpane.h"

#include <QBuffer>
#include <QClipboard>
#include <QCursor>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImageReader>
#include <QInputMethod>
#include <QJsonArray>
#include <QJsonDocument>
#include <QPainter>
#ifdef Q_OS_WIN
#include <QProcess>  // showInFolder: Explorer with the file selected (no QProcess on iOS)
#endif
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QSGTexture>
#include <QSGTextureMaterial>
#include <QScreen>
#include <QStyleHints>
#include <QThreadPool>
#include <QUrl>
#include <QtMath>
#include <cstring>
#include <memory>

// CEF's EVENTFLAG_* bits (include/internal/cef_types.h), repeated here so this file needs no CEF header.
enum : uint {
    kShift = 1 << 1, kControl = 1 << 2, kAlt = 1 << 3, kLeft = 1 << 4, kMiddle = 1 << 5, kRight = 1 << 6,
    kCommand = 1 << 7, kKeyPad = 1 << 9, kRepeat = 1 << 13,
};
// cef_key_event_type_t
enum { kRawKeyDown = 0, kKeyUp = 2, kChar = 3 };

// The Windows virtual-key code of a Qt key: Chromium reads keys this way on every system. (On Windows Qt gives the
// real one, nativeVirtualKey(); this table serves Linux, and keys typed through an input method.)
static int windowsKeyCode(int key, Qt::KeyboardModifiers mods) {
    const bool pad = mods & Qt::KeypadModifier;
    if (key >= Qt::Key_A && key <= Qt::Key_Z) return key;   // same codes: 0x41..0x5A
    if (key >= Qt::Key_0 && key <= Qt::Key_9) return pad ? 0x60 + (key - Qt::Key_0) : key;
    if (key >= Qt::Key_F1 && key <= Qt::Key_F24) return 0x70 + (key - Qt::Key_F1);
    switch (key) {
    case Qt::Key_Backspace: return 0x08;
    case Qt::Key_Tab: case Qt::Key_Backtab: return 0x09;
    case Qt::Key_Clear: return 0x0C;
    case Qt::Key_Return: case Qt::Key_Enter: return 0x0D;
    case Qt::Key_Shift: return 0x10;
    case Qt::Key_Control: return 0x11;
    case Qt::Key_Alt: case Qt::Key_AltGr: return 0x12;
    case Qt::Key_Pause: return 0x13;
    case Qt::Key_CapsLock: return 0x14;
    case Qt::Key_Escape: return 0x1B;
    case Qt::Key_Space: return 0x20;
    case Qt::Key_PageUp: return 0x21;
    case Qt::Key_PageDown: return 0x22;
    case Qt::Key_End: return 0x23;
    case Qt::Key_Home: return 0x24;
    case Qt::Key_Left: return 0x25;
    case Qt::Key_Up: return 0x26;
    case Qt::Key_Right: return 0x27;
    case Qt::Key_Down: return 0x28;
    case Qt::Key_Print: return 0x2C;
    case Qt::Key_Insert: return 0x2D;
    case Qt::Key_Delete: return 0x2E;
    case Qt::Key_Meta: return 0x5B;
    case Qt::Key_Menu: return 0x5D;
    case Qt::Key_NumLock: return 0x90;
    case Qt::Key_ScrollLock: return 0x91;
    case Qt::Key_Asterisk: return pad ? 0x6A : 0x38;
    case Qt::Key_Plus: return pad ? 0x6B : 0xBB;
    case Qt::Key_Minus: return pad ? 0x6D : 0xBD;
    case Qt::Key_Period: return pad ? 0x6E : 0xBE;
    case Qt::Key_Slash: return pad ? 0x6F : 0xBF;
    case Qt::Key_Semicolon: case Qt::Key_Colon: return 0xBA;
    case Qt::Key_Equal: return 0xBB;
    case Qt::Key_Comma: case Qt::Key_Less: return 0xBC;
    case Qt::Key_Underscore: return 0xBD;
    case Qt::Key_Greater: return 0xBE;
    case Qt::Key_Question: return 0xBF;
    case Qt::Key_QuoteLeft: case Qt::Key_AsciiTilde: return 0xC0;
    case Qt::Key_BracketLeft: case Qt::Key_BraceLeft: return 0xDB;
    case Qt::Key_Backslash: case Qt::Key_Bar: return 0xDC;
    case Qt::Key_BracketRight: case Qt::Key_BraceRight: return 0xDD;
    case Qt::Key_Apostrophe: case Qt::Key_QuoteDbl: return 0xDE;
    // shifted digits of a US keyboard
    case Qt::Key_Exclam: return '1';
    case Qt::Key_At: return '2';
    case Qt::Key_NumberSign: return '3';
    case Qt::Key_Dollar: return '4';
    case Qt::Key_Percent: return '5';
    case Qt::Key_AsciiCircum: return '6';
    case Qt::Key_Ampersand: return '7';
    case Qt::Key_ParenLeft: return '9';
    case Qt::Key_ParenRight: return '0';
    case Qt::Key_Back: return 0xA6;
    case Qt::Key_Forward: return 0xA7;
    case Qt::Key_VolumeMute: return 0xAD;
    case Qt::Key_VolumeDown: return 0xAE;
    case Qt::Key_VolumeUp: return 0xAF;
    case Qt::Key_MediaNext: return 0xB0;
    case Qt::Key_MediaPrevious: return 0xB1;
    case Qt::Key_MediaStop: return 0xB2;
    case Qt::Key_MediaTogglePlayPause: case Qt::Key_MediaPlay: return 0xB3;
    default: return 0;
    }
}

static Qt::CursorShape cursorShape(int cefType) {
    // cef_cursor_type_t, in its order: CT_POINTER = 0, CT_CROSS, CT_HAND, CT_IBEAM…
    switch (cefType) {
    case 1: case 31: return Qt::CrossCursor;                 // cross, cell
    case 2: return Qt::PointingHandCursor;
    case 3: case 30: return Qt::IBeamCursor;                 // text, vertical text
    case 4: return Qt::WaitCursor;
    case 5: return Qt::WhatsThisCursor;
    case 6: case 13: case 15: return Qt::SizeHorCursor;      // east, west, east-west
    case 7: case 10: case 14: return Qt::SizeVerCursor;      // north, south, north-south
    case 8: case 12: case 16: return Qt::SizeBDiagCursor;    // north-east, south-west
    case 9: case 11: case 17: return Qt::SizeFDiagCursor;    // north-west, south-east
    case 18: return Qt::SplitHCursor;                        // column resize
    case 19: return Qt::SplitVCursor;                        // row resize
    case 29: return Qt::SizeAllCursor;                       // move
    case 33: return Qt::DragLinkCursor;                      // alias
    case 34: return Qt::BusyCursor;                          // progress
    case 35: case 38: return Qt::ForbiddenCursor;            // no drop, not allowed
    case 36: return Qt::DragCopyCursor;
    case 37: return Qt::BlankCursor;                         // none
    case 41: return Qt::OpenHandCursor;                      // grab
    case 42: return Qt::ClosedHandCursor;                    // grabbing
    default:
        if (cefType >= 20 && cefType <= 28) return Qt::SizeAllCursor;   // panning
        return Qt::ArrowCursor;
    }
}

WebPane::WebPane(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents);
    setFlag(ItemAcceptsInputMethod);
    setAcceptedMouseButtons(Qt::AllButtons);
    setAcceptHoverEvents(true);
    QString info;
    m_available = cefh::probe(info);
    if (!m_available) m_problem = info;
    m_clock.start();
    // one stamp for the whole run: download ids are shared by every pane (any pane can cancel any download)
    static const qint64 stamp = QDateTime::currentSecsSinceEpoch();
    m_runStamp = stamp;
    connect(this, &WebPane::radiusChanged, this, [this] { update(); });
    m_lensWatch.setSingleShot(true);
    connect(&m_lensWatch, &QTimer::timeout, this, [this] {
        // Lens never got the picture through (or sat on its upload address): the other way
        if (m_lensTab) {
            qInfo("[lens] no answer from Google Lens after %lld ms (stage %d): falling back", (long long)m_lensClock.elapsed(), m_lensStage);
            lensFallback();
        }
    });
}

// The current tab drew its first picture (or the pane lost it): tells QML, and logs how long the page took.
void WebPane::notePicture() {
    const bool has = hasPicture();
    if (has == m_hadPicture) return;
    m_hadPicture = has;
    if (has && m_firstPaintClock.isValid()) {
        qInfo("[web] tab %d: first picture %lld ms after it opened", m_current, (long long)m_firstPaintClock.elapsed());
        m_firstPaintClock.invalidate();
    }
    emit hasPictureChanged();
}

void WebPane::setLensStage(int s) {
    if (s == m_lensStage) return;
    m_lensStage = s;
    emit lensStageChanged();
}

void WebPane::warmUp() {
    if (!m_available || m_current != 0) return;
    QElapsedTimer clock;
    clock.start();
    openTab(QString(), true);
    qInfo("[web] warm-up: engine and an empty page ready in %lld ms", (long long)clock.elapsed());
}

WebPane::~WebPane() {
    cefh::forgetView(this);
}

// ---- properties ----

void WebPane::setProfile(const QString &p) {
    if (p == m_profile) return;
    m_profile = p;
    emit profileChanged();
}

void WebPane::setLanguage(const QString &l) {
    if (l == m_language) return;
    m_language = l;
    emit profileChanged();
}

void WebPane::setCovered(bool c) {
    if (c == m_covered) return;
    m_covered = c;
    emit coveredChanged();
#ifdef REELHOUSE_WEBVIEW
    syncShown();   // the Android WebView is a native view over the app: hidden while QML is drawn over the page
#endif
}

void WebPane::setSingleTab(bool s) {
    if (s == m_singleTab) return;
    m_singleTab = s;
    emit singleTabChanged();
}

void WebPane::setKeepRendering(bool k) {
    if (k == m_keepRendering) return;
    m_keepRendering = k;
    emit keepRenderingChanged();
    syncShown();
}

WebPane::TabInfo *WebPane::find(int id) {
    for (TabInfo &t : m_tabs)
        if (t.id == id) return &t;
    return nullptr;
}

const WebPane::TabInfo *WebPane::findConst(int id) const {
    for (const TabInfo &t : m_tabs)
        if (t.id == id) return &t;
    return nullptr;
}

QVariantList WebPane::tabs() const {
    QVariantList out;
    for (const TabInfo &t : m_tabs)
        out.append(QVariantMap{{"id", t.id}, {"url", t.url}, {"title", t.title}, {"icon", t.icon}, {"loading", t.loading}});
    return out;
}

void WebPane::setCurrentTab(int id) {
    if (!findConst(id) || id == m_current) return;
    const int old = m_current;
    m_current = id;
    if (old > 0) cefh::setHidden(old, true);
    m_shown = false;   // the new one is shown (and asked for a fresh picture) by syncShown
    m_popupShown = false;
    syncShown();
    emit currentChanged();
    notePicture();
}

QString WebPane::url() const { const TabInfo *t = findConst(m_current); return t ? t->url : QString(); }
QString WebPane::title() const { const TabInfo *t = findConst(m_current); return t ? t->title : QString(); }
QString WebPane::icon() const { const TabInfo *t = findConst(m_current); return t ? t->icon : QString(); }
bool WebPane::loading() const { const TabInfo *t = findConst(m_current); return t && t->loading; }
bool WebPane::canGoBack() const { const TabInfo *t = findConst(m_current); return t && t->back; }
bool WebPane::canGoForward() const { const TabInfo *t = findConst(m_current); return t && t->forward; }
QVariantMap WebPane::failure() const { const TabInfo *t = findConst(m_current); return t ? t->failure : QVariantMap(); }

// ---- tabs and pages ----

bool WebPane::startEngine() {
    if (!m_available) return false;
    if (cefh::started()) return true;
    qInfo("[web] starting Chromium");
    cefh::configure(m_profile, m_language);
    QString why;
    if (!cefh::start(why)) {
        m_available = false;
        m_problem = why;
        emit availableChanged();
        return false;
    }
    m_version = cefh::version();
    emit availableChanged();
    return true;
}

void WebPane::openOutside(const QString &url) {
    const QUrl u(url);
    if (u.scheme() == "http" || u.scheme() == "https" || u.scheme() == "file") {
        QDesktopServices::openUrl(u);
        emit visited(url, QString());   // still in the history of Research mode
    }
}

int WebPane::openTab(const QString &url, bool show) {
    int id = 0;
    QElapsedTimer clock;
    clock.start();
    if (startEngine()) {
        if (m_version.isEmpty()) {
            m_version = cefh::version();
            emit availableChanged();
        }
        id = cefh::openTab(this, url, true);   // hidden until syncShown shows it
    }
    if (!id) {
        // No page view here: the address opens in the system browser, the tab stays in the list.
        static int fake = -1;
        id = fake--;
        openOutside(url);
    }
    TabInfo t;
    t.id = id;
    t.url = url;
    t.loading = id > 0 && !url.isEmpty();
    m_tabs.push_back(t);
    emit tabsChanged();
    if (id > 0) {
        qInfo("[web] tab %d opened in %lld ms (GUI thread)", id, (long long)clock.elapsed());
        m_firstPaintClock.start();
    }
    if (show || m_current == 0) {
        const int old = m_current;
        m_current = id;
        if (old > 0) cefh::setHidden(old, true);
        m_shown = false;
        syncShown();
        emit currentChanged();
        notePicture();
    }
    return id;
}

void WebPane::closeTab(int id) {
    int at = -1;
    for (int i = 0; i < (int)m_tabs.size(); ++i)
        if (m_tabs[i].id == id) at = i;
    if (at < 0) return;
    if (id > 0) cefh::closeTab(id);
    if (id == m_lensTab) {
        m_lensTab = 0;
        m_lensWatch.stop();
        setLensStage(0);
    }
    m_tabs.erase(m_tabs.begin() + at);
    emit tabsChanged();
    if (id == m_current) {
        // the neighbour below, else the one above (like Zen's vertical tabs)
        m_current = 0;
        m_shown = false;
        if (!m_tabs.empty()) m_current = m_tabs[qMin(at, (int)m_tabs.size() - 1)].id;
        if (!m_current) {
            m_frame = QImage();
            m_frameTab = 0;
            update();
        }
        syncShown();
        emit currentChanged();
        notePicture();
    }
}

void WebPane::moveTab(int id, int toIndex) {
    int at = -1;
    for (int i = 0; i < (int)m_tabs.size(); ++i)
        if (m_tabs[i].id == id) at = i;
    if (at < 0) return;
    toIndex = qBound(0, toIndex, (int)m_tabs.size() - 1);
    if (toIndex == at) return;
    TabInfo t = m_tabs[at];
    m_tabs.erase(m_tabs.begin() + at);
    m_tabs.insert(m_tabs.begin() + toIndex, t);
    emit tabsChanged();
}

void WebPane::navigate(const QString &url) {
    if (!m_current) {
        openTab(url, true);
        return;
    }
    if (TabInfo *t = find(m_current)) {
        t->url = url;
        t->failure.clear();
        emit tabsChanged();
        emit currentChanged();
    }
    if (m_current > 0) cefh::navigate(m_current, url);
    else openOutside(url);
}

void WebPane::back() { if (m_current > 0) cefh::goBack(m_current); }
void WebPane::forward() { if (m_current > 0) cefh::goForward(m_current); }
void WebPane::reload() {
    if (TabInfo *t = find(m_current)) t->failure.clear();
    if (m_current > 0) cefh::reload(m_current);
}
void WebPane::stop() { if (m_current > 0) cefh::stop(m_current); }
void WebPane::zoom(double factor) { if (m_current > 0) cefh::setZoom(m_current, qBound(0.25, factor, 5.0)); }
void WebPane::focusPage() {
    forceActiveFocus(Qt::OtherFocusReason);
    if (m_current > 0) cefh::setFocus(m_current, true);
}

void WebPane::runScript(const QString &code, int tab) {
    const int id = tab ? tab : m_current;
    if (id > 0) cefh::runScript(id, code);
}

int WebPane::evaluate(const QString &body, int tab) {
    const int n = m_nextEval++;
    const int id = tab ? tab : m_current;
    if (id <= 0) {
        // answered later, like a real one, so callers can rely on the order
        QTimer::singleShot(0, this, [this, n] { emit evaluated(n, QVariantMap{{"error", "no page"}}); });
        return n;
    }
    // the answer comes back as a console message that starts with \x01rh:<n>: (cef_host.cpp keeps it out of the log)
    const QString code = QStringLiteral(
        "(async () => { let r; try { r = await (async () => { %1\n })(); } catch (e) { r = { error: String(e) }; }"
        " let j; try { j = JSON.stringify(r === undefined ? null : r); } catch (e) { j = JSON.stringify({ error: String(e) }); }"
        " console.log('\\u0001rh:%2:' + j); })();").arg(body).arg(n);
    cefh::runScript(id, code);
    return n;
}

void WebPane::editCommand(const QString &what) { if (m_current > 0) cefh::editCommand(m_current, what); }
void WebPane::downloadUrl(const QString &url) { if (m_current > 0) cefh::download(m_current, url); }
void WebPane::copyText(const QString &text) { QGuiApplication::clipboard()->setText(text); }
void WebPane::answerDialog(int id, bool ok, const QString &text) { cefh::answerDialog(id, ok, text); }
void WebPane::find(const QString &text, bool forward) { if (m_current > 0) cefh::find(m_current, text, forward, true); }

// ---- Google Lens ----

void WebPane::lens(const QString &imagePath, bool quiet) {
    if (!QFileInfo::exists(imagePath)) {
        if (quiet) emit lensFailed();
        else emit message(tr("The picture could not be read."));
        return;
    }
    m_lensImage = imagePath;
    m_lensQuiet = quiet;
    m_lensClock.start();
    // each step is logged before it runs, so a crash in it shows where (HP 2026-10-16: the app closed after the frame)
    qInfo("[lens] start (%s, engine %s)", quiet ? "X-Ray" : "quick search", cefh::started() ? "running" : "not started yet");
    // The picture is made small (Lens works on about 1000 px anyway) and encoded off the GUI thread, while the Lens
    // page opens: a full-size 4K frame as base64 was megabytes of text built and sent from this thread.
    const int gen = ++m_lensGen;
    m_lensData.clear();
    m_lensSize = QSize();
    m_lensDataReady = false;
    m_lensSubmitWanted = false;
    {
        QPointer<WebPane> self(this);
        const QString path = imagePath;
        QThreadPool::globalInstance()->start([self, gen, path] {
            QElapsedTimer clock;
            clock.start();
            QImage img(path);
            QString data;
            QSize size;
            if (!img.isNull()) {
                const int maxSide = 1600;
                if (qMax(img.width(), img.height()) > maxSide) img = img.scaled(maxSide, maxSide, Qt::KeepAspectRatio, Qt::SmoothTransformation);
                QByteArray jpg;
                QBuffer buf(&jpg);
                buf.open(QIODevice::WriteOnly);
                if (img.save(&buf, "JPEG", 85)) {
                    data = QString::fromLatin1(jpg.toBase64());
                    size = img.size();
                }
            }
            const qint64 ms = clock.elapsed();
            QMetaObject::invokeMethod(qApp, [self, gen, data, size, ms] {
                if (self) self->lensPrepared(gen, data, size, ms);
            }, Qt::QueuedConnection);
        });
    }
    if (!m_available || !startEngine()) {
        qInfo("[lens] no page view (%s): falling back", qPrintable(m_problem));
        lensFallback();
        return;
    }
    qInfo("[lens] engine ready, %s", m_current <= 0 ? "opening the Lens tab" : "using the open tab");
    if (m_current <= 0) openTab(QString(), true);
    m_lensTab = m_current;
    if (m_lensTab <= 0) {
        lensFallback();
        return;
    }
    // Lens takes the picture as a form upload. Chromium only sends a form for a page of the same site, so the tab
    // first opens Lens itself (its cookies and consent then apply too); lensSubmit() sends the picture from there.
    setLensStage(1);
    if (TabInfo *t = find(m_lensTab)) {
        t->title = tr("Google Lens");
        t->failure.clear();
        emit tabsChanged();
        emit currentChanged();
    }
    const QString lang = m_language.isEmpty() ? QStringLiteral("en") : m_language;
    cefh::navigate(m_lensTab, QStringLiteral("https://lens.google.com/?hl=%1").arg(lang));
    m_lensWatch.start(25000);
}

// The picture is ready (worker thread of lens()): sent now if the Lens page is already open.
void WebPane::lensPrepared(int gen, const QString &data, const QSize &size, qint64 ms) {
    if (gen != m_lensGen) return;
    if (data.isEmpty()) {
        qInfo("[lens] the picture could not be read (%lld ms)", (long long)ms);
        if (m_lensTab || m_lensStage) lensFallback();
        return;
    }
    qInfo("[lens] picture prepared in %lld ms off the GUI thread (%dx%d, %d KB)", (long long)ms, size.width(), size.height(), int(data.size() * 3 / 4 / 1024));
    m_lensData = data;
    m_lensSize = size;
    m_lensDataReady = true;
    if (m_lensSubmitWanted && m_lensTab && m_lensStage == 1) lensSubmit();
}

// Builds the upload form in the Lens page with the picture (a File made from base64) and sends it.
void WebPane::lensSubmit() {
    if (!m_lensDataReady) {
        // the Lens page was faster than the picture: lensPrepared() sends it
        m_lensSubmitWanted = true;
        return;
    }
    m_lensSubmitWanted = false;
    const QSize size = m_lensSize;
    const bool png = false;   // always the JPEG made by lens()
    const QString lang = m_language.isEmpty() ? QStringLiteral("en") : m_language;
    const QString action = QStringLiteral("https://lens.google.com/v3/upload?hl=%1&re=df&st=%2&ep=gsbubb").arg(lang).arg(QDateTime::currentMSecsSinceEpoch());
    const QString js = QStringLiteral(
        "(function(){var b=atob('%1');var a=new Uint8Array(b.length);for(var i=0;i<b.length;i++)a[i]=b.charCodeAt(i);"
        "var f=document.createElement('form');f.method='POST';f.enctype='multipart/form-data';f.action='%2';f.style.display='none';"
        "var p=document.createElement('input');p.type='file';p.name='encoded_image';var d=new DataTransfer();"
        "d.items.add(new File([a],'%3',{type:'%4'}));p.files=d.files;f.appendChild(p);"
        "var s=document.createElement('input');s.type='hidden';s.name='processed_image_dimensions';s.value='%5,%6';f.appendChild(s);"
        "(document.body||document.documentElement).appendChild(f);f.submit();})();")
        .arg(m_lensData, action, png ? QStringLiteral("frame.png") : QStringLiteral("frame.jpg"),
             png ? QStringLiteral("image/png") : QStringLiteral("image/jpeg"))
        .arg(size.width()).arg(size.height());
    setLensStage(2);
    qInfo("[lens] upload sent %lld ms after the request", (long long)m_lensClock.elapsed());
    cefh::runScript(m_lensTab, js);
}

// Lens did not take the picture (its upload address changed, or no page view): Google Images, with the picture on
// the clipboard to paste into its search box.
void WebPane::lensFallback() {
    qInfo("[lens] Lens did not take the picture (stage %d, %lld ms): %s", m_lensStage, (long long)m_lensClock.elapsed(),
          m_lensQuiet ? "lensFailed" : "Google Images with the picture on the clipboard");
    m_lensTab = 0;
    setLensStage(0);
    m_lensSubmitWanted = false;
    ++m_lensGen;   // a picture still being made is not wanted any more
    m_lensWatch.stop();
    if (m_lensQuiet) {
        emit lensFailed();
        return;
    }
    const QImage img(m_lensImage);
    if (!img.isNull()) QGuiApplication::clipboard()->setImage(img);
    const QString images = QStringLiteral("https://images.google.com/");
    if (m_current > 0) cefh::navigate(m_current, images);
    else openOutside(images);
    emit message(tr("Google Lens did not take the picture. It is on the clipboard: press Ctrl+V in the Google Images search box."));
}

// ---- extensions and downloads ----

// CEF dropped its Chrome extension support (the cef_extension API) in the versions that can draw pages off screen,
// so there is nothing to list; the sheet says so.
void WebPane::refreshExtensions() { emit extensionsChanged(); }
void WebPane::addExtension(const QString &) { emit message(tr("Browser extensions are not supported by the built-in Chromium engine.")); }
void WebPane::removeExtension(const QString &) {}
void WebPane::enableExtension(const QString &, bool) {}

// Download ids start again at 1 each run; the core keeps them across runs, so they carry the start time of the run.
QString WebPane::dlKey(int n) const { return QStringLiteral("%1-%2").arg(m_runStamp).arg(n); }
int WebPane::dlNum(const QString &key) const {
    const QString prefix = QString::number(m_runStamp) + "-";
    return key.startsWith(prefix) ? key.mid(prefix.size()).toInt() : 0;
}

void WebPane::cancelDownload(const QString &id) { if (int n = dlNum(id)) cefh::cancelDownload(n); }
void WebPane::pauseDownload(const QString &id) { if (int n = dlNum(id)) cefh::pauseDownload(n); }
void WebPane::resumeDownload(const QString &id) { if (int n = dlNum(id)) cefh::resumeDownload(n); }

void WebPane::showInFolder(const QString &path) {
    const QFileInfo fi(path);
#ifdef Q_OS_WIN
    if (fi.exists()) {
        QProcess p;
        p.setProgram("explorer.exe");
        p.setNativeArguments("/select,\"" + QDir::toNativeSeparators(fi.absoluteFilePath()) + "\"");
        if (p.startDetached()) return;
    }
#endif
    QDesktopServices::openUrl(QUrl::fromLocalFile(fi.absolutePath()));
}

void WebPane::openFile(const QString &path) { QDesktopServices::openUrl(QUrl::fromLocalFile(path)); }

// ---- what the engine asks of the view ----

QSize WebPane::cefViewSize() const {
    return QSize(qMax(1, qCeil(width())), qMax(1, qCeil(height())));
}

// Device pixels per logical pixel of the page: the screen's scale times the scale of the scene ("Size in general"
// scales the whole content item), so pages follow the app's size and stay sharp.
qreal WebPane::cefScale() const {
    const qreal dpr = window() ? window()->effectiveDevicePixelRatio() : qApp->devicePixelRatio();
    qreal scene = 1;
    if (width() > 0) {
        const QRectF r = mapRectToScene(QRectF(0, 0, width(), height()));
        scene = r.width() / width();
    }
    return qBound<qreal>(0.25, dpr * scene, 8.0);
}

QPoint WebPane::cefToScreen(const QPoint &p) const {
    return mapToGlobal(QPointF(p)).toPoint();
}

QRect WebPane::cefScreenRect() const {
    const QScreen *s = window() && window()->screen() ? window()->screen() : QGuiApplication::primaryScreen();
    return s ? s->geometry() : QRect(0, 0, 1920, 1080);
}

quintptr WebPane::cefNativeWindow() const {
    return window() ? quintptr(window()->winId()) : 0;
}

void WebPane::cefPaint(int tab, bool popup, const uchar *bgra, int w, int h) {
    if (tab != m_current || w <= 0 || h <= 0) return;
    // a hidden page kept running (the Lens probe) is never looked at: no copy of its pictures on the GUI thread
    if (!isVisible()) return;
    if (popup) {
        m_popup = QImage(bgra, w, h, w * 4, QImage::Format_ARGB32_Premultiplied).copy();
    } else {
        // the same size: copied over the old picture (bits() makes it our own if the renderer still holds it)
        if (m_frame.width() != w || m_frame.height() != h || m_frame.format() != QImage::Format_ARGB32_Premultiplied)
            m_frame = QImage(w, h, QImage::Format_ARGB32_Premultiplied);
        std::memcpy(m_frame.bits(), bgra, size_t(w) * size_t(h) * 4);
        m_frameTab = tab;
    }
    m_textureDirty = true;
    update();
    if (!popup && !m_hadPicture) notePicture();
}

void WebPane::cefEvent(const cefh::Event &e) {
    using cefh::Ev;
    TabInfo *t = e.tab ? find(e.tab) : nullptr;
    const bool cur = e.tab != 0 && e.tab == m_current;
    switch (e.kind) {
    case Ev::Failed:
        m_available = false;
        m_problem = e.text;
        for (TabInfo &x : m_tabs) x.loading = false;
        emit availableChanged();
        emit tabsChanged();
        emit currentChanged();
        break;
    case Ev::Url:
        if (t && t->url != e.text) {
            t->url = e.text;
            emit tabsChanged();
            if (cur) emit currentChanged();
        }
        if (e.tab == m_lensTab && m_lensStage == 2 && e.text.contains("/v3/upload")) setLensStage(3);
        break;
    case Ev::Title:
        if (t) {
            t->title = e.text;
            emit tabsChanged();
            if (cur) emit currentChanged();
        }
        if (!e.text2.isEmpty()) emit titleSeen(e.text2, e.text);
        break;
    case Ev::Icon:
        if (t) {
            t->icon = e.text;
            emit tabsChanged();
            if (cur) emit currentChanged();
        }
        break;
    case Ev::Loading:
        if (t) {
            t->loading = e.n1 > 0;
            if (t->loading) t->failure.clear();
            emit tabsChanged();
            if (cur) emit currentChanged();
        }
        break;
    case Ev::NavState:
        if (t) {
            t->back = e.n1 > 0;
            t->forward = e.n2 > 0;
            if (cur) emit currentChanged();
            // a page that changes its address without loading again (YouTube, most web apps): a visit too
            if (!t->loading && !t->url.isEmpty() && t->url != t->visitedUrl && !t->visitedUrl.isEmpty() && !t->url.startsWith("data:")) {
                t->visitedUrl = t->url;
                emit visited(t->url, t->title);
            }
        }
        break;
    case Ev::Visited:
        if (e.tab == m_lensTab && m_lensTab) {
            if (m_lensStage == 1) {
                // Lens (or Google's consent page) is open: the picture goes from here
                if (e.text.startsWith("https://") && e.text.contains("google.")) lensSubmit();
                break;
            }
            // Still on the upload address: Lens usually moves on to its results by itself (a redirect, or a script
            // in the page). If it has not after a few seconds, it did not take the picture.
            if (e.text.contains("/v3/upload")) {
                setLensStage(3);
                m_lensWatch.start(6000);
                const int tab = e.tab;
                QPointer<WebPane> self(this);
                QTimer::singleShot(4000, this, [self, tab] {
                    if (!self || self->m_lensTab != tab) return;
                    const TabInfo *t = self->findConst(tab);
                    if (t && t->url.contains("/v3/upload")) self->lensFallback();
                });
                break;
            }
            if (m_lensStage >= 2) {
                qInfo("[lens] results page %lld ms after the request", (long long)m_lensClock.elapsed());
                m_lensTab = 0;
                setLensStage(0);
                m_lensWatch.stop();
            }
        }
        if (e.text.startsWith("data:") || e.text == QLatin1String("about:blank")) break;
        if (t) t->visitedUrl = e.text;
        emit visited(e.text, e.text2);
        break;
    case Ev::NavFailed:
        if (e.tab == m_lensTab && m_lensTab) {
            lensFallback();
            break;
        }
        if (t) {
            t->failure = QVariantMap{{"url", e.text}, {"error", e.text2}, {"code", int(e.n1)}};
            if (cur) emit currentChanged();
        }
        emit navFailed(e.text, int(e.n1), 0);
        break;
    case Ev::NewTab:
        if (m_singleTab) navigate(e.text);
        else openTab(e.text, e.n1 == 0);
        break;
    case Ev::External: {
        const QUrl u(e.text);
        if (u.isValid()) QDesktopServices::openUrl(u);
        break;
    }
    case Ev::FullScreen:
        m_fullScreen = e.n1 > 0;
        emit fullScreenChanged();
        break;
    case Ev::Cursor:
        if (cur) setCursor(QCursor(cursorShape(int(e.n1))));
        break;
    case Ev::Status:
        if (cur && m_status != e.text) {
            m_status = e.text;
            emit statusTextChanged();
        }
        break;
    case Ev::Console:
        if (e.text.startsWith(QLatin1String("\x01rh:"))) {
            const int colon = e.text.indexOf(':', 4);
            if (colon > 4) {
                const int n = e.text.mid(4, colon - 4).toInt();
                const QJsonDocument doc = QJsonDocument::fromJson("[" + e.text.mid(colon + 1).toUtf8() + "]");
                emit evaluated(n, doc.isArray() && !doc.array().isEmpty() ? doc.array().at(0).toVariant() : QVariant());
            }
        }
        break;
    case Ev::ContextMenu:
        if (cur) emit contextMenu(e.map);
        break;
    case Ev::JsDialog: {
        static const QMetaMethod sig = QMetaMethod::fromSignal(&WebPane::jsDialog);
        if (cur && isSignalConnected(sig)) emit jsDialog(int(e.n1), e.map);
        else cefh::answerDialog(int(e.n1), e.map.value("kind").toString() == QLatin1String("alert"), QString());
        break;
    }
    case Ev::JsDialogGone:
        emit jsDialogClosed(int(e.n1));
        break;
    case Ev::PopupShow:
        if (cur) {
            m_popupShown = e.n1 > 0;
            if (!m_popupShown) m_popup = QImage();
            m_textureDirty = true;
            update();
        }
        break;
    case Ev::PopupRect:
        if (cur) m_popupRect = e.rect;
        break;
    case Ev::ImeRect:
        if (cur) {
            m_imeRect = e.rect;
            if (hasActiveFocus()) QGuiApplication::inputMethod()->update(Qt::ImCursorRectangle);
        }
        break;
    case Ev::Crashed:
        if (t) {
            t->loading = false;
            t->failure = QVariantMap{{"url", t->url}, {"error", tr("The page stopped working.")}, {"code", -1000}};
            emit tabsChanged();
            if (cur) emit currentChanged();
        }
        break;
    case Ev::Closed:
        // the page closed itself (window.close()): its tab goes too
        if (t) closeTab(e.tab);
        break;
    case Ev::DlStart: {
        const int n = int(e.n1);
        QVariantMap d{{"id", dlKey(n)}, {"url", e.text}, {"path", e.text2}, {"received", 0.0}, {"total", e.n2}, {"state", "progress"}};
        m_dl.insert(n, d);
        m_dlLastSent.insert(n, m_clock.elapsed());
        emit download(d);
        break;
    }
    case Ev::DlProgress: {
        const int n = int(e.n1);
        if (!m_dl.contains(n)) break;
        m_dl[n]["received"] = e.n2;
        break;
    }
    case Ev::DlState: {
        const int n = int(e.n1);
        if (!m_dl.contains(n)) break;
        const QString before = m_dl[n].value("state").toString();
        m_dl[n]["state"] = e.text;
        if (!e.text2.isEmpty()) m_dl[n]["path"] = e.text2;
        if (e.map.contains("total")) m_dl[n]["total"] = e.map.value("total");
        if (e.text == QLatin1String("done")) m_dl[n]["received"] = qMax(m_dl[n]["received"].toDouble(), m_dl[n]["total"].toDouble());
        // progress changes many times a second: a few updates a second are enough for the list
        const qint64 now = m_clock.elapsed();
        if (e.text != before || e.text != QLatin1String("progress") || now - m_dlLastSent.value(n) >= 300) {
            m_dlLastSent[n] = now;
            emit download(m_dl.value(n));
        }
        break;
    }
    case Ev::Message:
        emit message(e.text);
        break;
    }
}

// ---- drawing ----

namespace {
// The page as a textured rounded rectangle (a triangle strip from top to bottom: left and right edge in turns).
struct PageNode : QSGGeometryNode {
    QSGTextureMaterial material;
    QSGOpaqueTextureMaterial opaque;
    std::unique_ptr<QSGTexture> texture;
    QSGGeometry geometry{QSGGeometry::defaultAttributes_TexturedPoint2D(), 0};
    QSizeF size;
    qreal radius = -1;
    PageNode() {
        geometry.setDrawingMode(QSGGeometry::DrawTriangleStrip);
        setGeometry(&geometry);
        setMaterial(&material);
        setOpaqueMaterial(&opaque);
    }
    void shape(const QSizeF &s, qreal r) {
        if (s == size && r == radius) return;
        size = s;
        radius = r;
        const qreal w = s.width(), h = s.height();
        r = qBound<qreal>(0, r, qMin(w, h) / 2);
        const int steps = r > 0 ? qBound(3, int(r / 2), 12) : 0;
        // rows: the top arc (or edge), then the bottom arc (or edge); each row gives a left and a right point
        QVector<QPointF> rows;   // (y, inset)
        for (int i = 0; i <= steps; ++i) {   // top: from the edge (y 0, inset r) down to y r (inset 0)
            const qreal a = steps ? (M_PI / 2) * i / steps : 0;
            rows.append(QPointF(r - r * std::cos(a), r - r * std::sin(a)));
        }
        for (int i = 0; i <= steps; ++i) {   // bottom: from y h - r (inset 0) down to the edge (y h, inset r)
            const qreal a = steps ? (M_PI / 2) * i / steps : 0;
            rows.append(QPointF(h - r + r * std::sin(a), r - r * std::cos(a)));
        }
        geometry.allocate(rows.size() * 2);
        QSGGeometry::TexturedPoint2D *v = geometry.vertexDataAsTexturedPoint2D();
        for (int i = 0; i < rows.size(); ++i) {
            const qreal y = rows[i].x(), inset = rows[i].y();
            v[2 * i].set(float(inset), float(y), float(inset / w), float(y / h));
            v[2 * i + 1].set(float(w - inset), float(y), float((w - inset) / w), float(y / h));
        }
        markDirty(QSGNode::DirtyGeometry);
    }
};
}  // namespace

QSGNode *WebPane::updatePaintNode(QSGNode *old, UpdatePaintNodeData *) {
    auto *node = static_cast<PageNode *>(old);
    // no picture yet (a page still opening, or no engine): nothing is drawn and the QML under the item shows
    if (m_frame.isNull() || m_frameTab != m_current || width() < 1 || height() < 1 || !window()) {
        delete node;
        return nullptr;
    }
    if (!node) {
        node = new PageNode;
        m_textureDirty = true;
    }
    if (m_textureDirty || !node->texture) {
        m_textureDirty = false;
        QImage img = m_frame;
        if (m_popupShown && !m_popup.isNull() && m_popupRect.isValid() && m_sentSize.width() > 0) {
            // the drop-down list is drawn into the picture where Chromium placed it
            const qreal s = qreal(m_frame.width()) / m_sentSize.width();
            img = m_frame.copy();
            QPainter p(&img);
            p.drawImage(QPointF(m_popupRect.x() * s, m_popupRect.y() * s), m_popup);
        }
        std::unique_ptr<QSGTexture> tex(window()->createTextureFromImage(img));
        tex->setFiltering(QSGTexture::Linear);
        node->material.setTexture(tex.get());
        node->opaque.setTexture(tex.get());
        node->texture = std::move(tex);
        node->markDirty(QSGNode::DirtyMaterial);
    }
    node->shape(QSizeF(width(), height()), m_radius);
    return node;
}

// ---- size, scale and visibility ----

// Shows the current tab's page while the item can be seen (or keepRendering), hides it otherwise; and tells Chromium
// about a new size or scale.
void WebPane::syncShown() {
    if (m_current <= 0) return;
    const bool minimized = m_window && (m_window->visibility() == QWindow::Minimized || !m_window->isVisible());
    bool want = m_keepRendering || (isVisible() && !minimized && width() > 2 && height() > 2);
#ifdef REELHOUSE_WEBVIEW
    want = want && !m_covered && !m_overlaid && isVisible();
#endif
    checkScale();
    if (want != m_shown) {
        m_shown = want;
        cefh::setHidden(m_current, !want);
    }
    // a page kept running while it cannot be seen (keepRendering) is still drawn by Chromium: at a few pictures a
    // second instead of 60, so it does not load the GUI thread (and the video under it) for nothing
    const bool seen = isVisible() && !minimized;
    const int fps = want && !seen ? 5 : 60;
    if (fps != m_sentFps || m_fpsTab != m_current) {
        m_sentFps = fps;
        m_fpsTab = m_current;
        cefh::setFrameRate(m_current, fps);
    }
}

void WebPane::checkScale() {
    const QSize s = cefViewSize();
    const qreal sc = cefScale();
    if (s == m_sentSize && qFuzzyCompare(sc, m_sentScale)) return;
    const bool scaleChanged = !qFuzzyCompare(sc, m_sentScale);
    m_sentSize = s;
    m_sentScale = sc;
    for (const TabInfo &t : m_tabs) {
        if (t.id <= 0) continue;
        if (scaleChanged) cefh::screenChanged(t.id);
        else cefh::resized(t.id);
    }
}

void WebPane::watchWindow(QQuickWindow *w) {
    if (w == m_window) return;
    for (const auto &c : m_windowConns) disconnect(c);
    m_windowConns.clear();
    m_window = w;
    if (!w) return;
    // every frame: a change of the scene's scale ("Size in general") or of the screen moves nothing in this item
    m_windowConns << connect(w, &QQuickWindow::afterAnimating, this, &WebPane::checkScale);
    m_windowConns << connect(w, &QWindow::visibilityChanged, this, &WebPane::syncShown);
    m_windowConns << connect(w, &QWindow::screenChanged, this, &WebPane::checkScale);
    m_overlay = nullptr;
#ifdef REELHOUSE_WEBVIEW
    m_windowConns << connect(w, &QQuickWindow::afterAnimating, this, &WebPane::checkOverlay);
#endif
}

// The Android WebView is a native view drawn above the whole Qt Quick scene, so a Menu, Popover, Drawer or Dialog
// opened over the page (the "Research ▾" mode menu, the bottom sheets…) was cut off by it. Popups live in the
// window's overlay: while one that can be seen overlaps this pane, the page is hidden, as with `covered`.
// Only popup items count (not the toast or other plain items placed in the overlay), and not tooltips.
// On desktop Chromium is drawn into the scene, so nothing is needed there.
void WebPane::checkOverlay() {
#ifdef REELHOUSE_WEBVIEW
    bool over = false;
    if (m_window && isVisible() && m_current > 0) {
        if (!m_overlay && m_window->contentItem()) {
            const auto kids = m_window->contentItem()->childItems();
            for (QQuickItem *k : kids)
                if (k->inherits("QQuickOverlay")) { m_overlay = k; break; }
        }
        if (m_overlay) {
            const QRectF mine = mapRectToScene(QRectF(0, 0, width(), height()));
            const auto kids = m_overlay->childItems();
            for (QQuickItem *k : kids) {
                if (!k->isVisible() || k->opacity() < 0.01 || !k->inherits("QQuickPopupItem")) continue;
                if (k->parent() && k->parent()->inherits("QQuickToolTip")) continue;
                if (k->mapRectToScene(QRectF(0, 0, k->width(), k->height())).intersects(mine)) { over = true; break; }
            }
        }
    }
    if (over != m_overlaid) {
        m_overlaid = over;
        qInfo("[web] popup over the page: %s", over ? "hiding the WebView" : "showing the WebView again");
        syncShown();
    }
#endif
}

void WebPane::itemChange(ItemChange change, const ItemChangeData &value) {
    QQuickItem::itemChange(change, value);
    if (change == ItemSceneChange) watchWindow(value.window);
    if (change == ItemVisibleHasChanged || change == ItemSceneChange) syncShown();
    // shown again while it kept running hidden (no pictures were copied then): ask for a fresh one
    if (change == ItemVisibleHasChanged && value.boolValue && m_current > 0) cefh::invalidate(m_current);
    if (change == ItemDevicePixelRatioHasChanged) checkScale();
}

void WebPane::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (newGeometry.size() != oldGeometry.size()) syncShown();
    update();
}

// ---- input ----

uint WebPane::modifiers(Qt::KeyboardModifiers m, Qt::MouseButtons b) const {
    uint f = 0;
    if (m & Qt::ShiftModifier) f |= kShift;
    if (m & Qt::ControlModifier) f |= kControl;
    if (m & Qt::AltModifier) f |= kAlt;
    if (m & Qt::MetaModifier) f |= kCommand;
    if (m & Qt::KeypadModifier) f |= kKeyPad;
    if (b & Qt::LeftButton) f |= kLeft;
    if (b & Qt::MiddleButton) f |= kMiddle;
    if (b & Qt::RightButton) f |= kRight;
    return f;
}

void WebPane::sendMouse(QMouseEvent *e, bool press, bool release) {
    if (m_current <= 0) return;
    const QPointF p = e->position();
    const int button = e->button() == Qt::RightButton ? 2 : (e->button() == Qt::MiddleButton ? 1 : 0);
    if (press || release) {
        cefh::mouseClick(m_current, qRound(p.x()), qRound(p.y()), modifiers(e->modifiers(), e->buttons()), button, release, m_clicks);
    } else {
        cefh::mouseMove(m_current, qRound(p.x()), qRound(p.y()), modifiers(e->modifiers(), e->buttons()), false);
    }
}

void WebPane::mousePressEvent(QMouseEvent *e) {
    // the mouse's back and forward buttons: the browser's own
    if (e->button() == Qt::BackButton) { back(); e->accept(); return; }
    if (e->button() == Qt::ForwardButton) { forward(); e->accept(); return; }
    forceActiveFocus(Qt::MouseFocusReason);
    // double and triple clicks: the same button again, soon and close by
    const bool again = e->button() == m_clickButton && m_clickClock.isValid() && m_clickClock.elapsed() < QGuiApplication::styleHints()->mouseDoubleClickInterval()
                       && QLineF(e->position(), m_clickPos).length() < 5;
    m_clicks = again ? qMin(3, m_clicks + 1) : 1;
    m_clickButton = e->button();
    m_clickPos = e->position();
    m_clickClock.start();
    sendMouse(e, true, false);
    e->accept();
}

void WebPane::mouseDoubleClickEvent(QMouseEvent *e) { mousePressEvent(e); }

void WebPane::mouseReleaseEvent(QMouseEvent *e) {
    if (e->button() == Qt::BackButton || e->button() == Qt::ForwardButton) { e->accept(); return; }
    sendMouse(e, false, true);
    e->accept();
}

void WebPane::mouseMoveEvent(QMouseEvent *e) {
    sendMouse(e, false, false);
    e->accept();
}

void WebPane::mouseUngrabEvent() {
    if (m_current > 0) cefh::captureLost(m_current);
}

void WebPane::hoverEnterEvent(QHoverEvent *e) { hoverMoveEvent(e); }

void WebPane::hoverMoveEvent(QHoverEvent *e) {
    if (m_current > 0) cefh::mouseMove(m_current, qRound(e->position().x()), qRound(e->position().y()), modifiers(e->modifiers(), Qt::NoButton), false);
}

void WebPane::hoverLeaveEvent(QHoverEvent *e) {
    if (m_current > 0) cefh::mouseMove(m_current, qRound(e->position().x()), qRound(e->position().y()), modifiers(e->modifiers(), Qt::NoButton), true);
    if (!m_status.isEmpty()) {
        m_status.clear();
        emit statusTextChanged();
    }
}

void WebPane::wheelEvent(QWheelEvent *e) {
    if (m_current <= 0) { e->ignore(); return; }
    // Ctrl + wheel: the page's zoom (Chromium does it from the modifier)
    QPoint d = e->pixelDelta();
    if (d.isNull()) d = e->angleDelta();   // 120 per notch, as Chromium expects on Windows
    cefh::mouseWheel(m_current, qRound(e->position().x()), qRound(e->position().y()), modifiers(e->modifiers(), e->buttons()), d.x(), d.y());
    e->accept();
}

// The keys a page gets even when a QML Shortcut uses them: typing, editing and moving the caret. Ctrl+T, Ctrl+W,
// Ctrl+L, Ctrl+Tab, F5, Alt+←… stay the app's (Research mode's Shortcuts); keys no Shortcut wants still reach the
// page through keyPressEvent.
static bool pageKey(const QKeyEvent *k) {
    const Qt::KeyboardModifiers m = k->modifiers() & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier);
    const int key = k->key();
    if (key >= Qt::Key_F1 && key <= Qt::Key_F35) return false;
    if (m == Qt::NoModifier) return true;
    if (m == Qt::ControlModifier) {
        switch (key) {
        case Qt::Key_A: case Qt::Key_C: case Qt::Key_V: case Qt::Key_X: case Qt::Key_Z: case Qt::Key_Y:
        case Qt::Key_Left: case Qt::Key_Right: case Qt::Key_Up: case Qt::Key_Down: case Qt::Key_Home: case Qt::Key_End:
        case Qt::Key_Backspace: case Qt::Key_Delete: case Qt::Key_Insert: case Qt::Key_B: case Qt::Key_I: case Qt::Key_U:
            return true;
        default:
            return false;
        }
    }
    // AltGr (Ctrl+Alt on Windows) makes characters on many keyboards
    if (m == (Qt::ControlModifier | Qt::AltModifier) && !k->text().isEmpty() && k->text().at(0).isPrint()) return true;
    return false;
}

bool WebPane::event(QEvent *e) {
    if (e->type() == QEvent::ShortcutOverride) {
        auto *k = static_cast<QKeyEvent *>(e);
        if (m_current > 0 && pageKey(k)) {
            e->accept();
            return true;
        }
    }
    return QQuickItem::event(e);
}

void WebPane::sendKey(QKeyEvent *e, bool down) {
    if (m_current <= 0) return;
    int vk = 0;
#ifdef Q_OS_WIN
    vk = int(e->nativeVirtualKey());
#endif
    if (!vk) vk = windowsKeyCode(e->key(), e->modifiers());
    int native = int(e->nativeScanCode());
#ifdef Q_OS_WIN
    // Chromium wants the lParam of WM_KEYDOWN: the scan code (with its "extended" bit) in bits 16..24
    native = int(((e->nativeScanCode() & 0x1FF) << 16) | 1u | (down ? 0u : 0xC0000000u));
#endif
    uint mods = modifiers(e->modifiers(), Qt::NoButton);
    if (e->isAutoRepeat()) mods |= kRepeat;
    const bool system = (e->modifiers() & Qt::AltModifier) && !(e->modifiers() & Qt::ControlModifier);
    const QString text = e->text();
    const ushort ch = text.isEmpty() ? 0 : text.at(0).unicode();
    cefh::key(m_current, down ? kRawKeyDown : kKeyUp, vk, native, mods, ch, ch, system);
    if (!down) return;
    // the characters it types (Enter as "\r", as Windows does; no other control characters). On Windows CEF rebuilds
    // a WM_CHAR message from the event, whose wParam (windows_key_code) is the character itself.
    for (const QChar c : text) {
        const ushort u = c.unicode();
        if (!((u >= 0x20 && u != 0x7F) || u == '\r')) continue;
#ifdef Q_OS_WIN
        const int code = u;
#else
        const int code = u == '\r' ? 0x0D : vk;
#endif
        cefh::key(m_current, kChar, code, native, mods, u, u, system);
    }
}

void WebPane::keyPressEvent(QKeyEvent *e) {
    if (m_current <= 0) { e->ignore(); return; }
    sendKey(e, true);
    if (e->key() == Qt::Key_Escape) emit keyPressed(QStringLiteral("Escape"));
    e->accept();
}

void WebPane::keyReleaseEvent(QKeyEvent *e) {
    if (m_current <= 0) { e->ignore(); return; }
    sendKey(e, false);
    e->accept();
}

void WebPane::focusInEvent(QFocusEvent *e) {
    QQuickItem::focusInEvent(e);
    if (m_current > 0) cefh::setFocus(m_current, true);
}

void WebPane::focusOutEvent(QFocusEvent *e) {
    QQuickItem::focusOutEvent(e);
    if (m_current > 0) cefh::setFocus(m_current, false);
}

void WebPane::inputMethodEvent(QInputMethodEvent *e) {
    if (m_current <= 0) { e->ignore(); return; }
    if (!e->commitString().isEmpty()) cefh::imeCommit(m_current, e->commitString());
    if (!e->preeditString().isEmpty()) {
        int cursor = e->preeditString().size();
        for (const QInputMethodEvent::Attribute &a : e->attributes())
            if (a.type == QInputMethodEvent::Cursor) cursor = a.start;
        cefh::imeComposition(m_current, e->preeditString(), cursor);
    } else if (e->commitString().isEmpty()) {
        cefh::imeCancel(m_current);
    }
    e->accept();
}

QVariant WebPane::inputMethodQuery(Qt::InputMethodQuery query) const {
    switch (query) {
    case Qt::ImEnabled: return m_current > 0;
    case Qt::ImCursorRectangle: return QRectF(m_imeRect.isValid() ? m_imeRect : QRect(0, 0, 1, 20));
    case Qt::ImHints: return int(Qt::ImhNone);
    default: return QQuickItem::inputMethodQuery(query);
    }
}
