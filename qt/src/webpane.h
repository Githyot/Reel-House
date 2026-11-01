#pragma once
#include <QElapsedTimer>
#include <QHash>
#include <QImage>
#include <QPointer>
#include <QQuickItem>
#include <QTimer>
#include <QVariantList>
#include <vector>

#include "cef_host.h"

// A web page area for QML (Research mode, the quick search over the video, the hidden Lens probe of X-Ray).
// Pages are drawn by Chromium (CEF, cef_host.cpp) off screen: each picture becomes a texture of this item, so QML
// draws over it like over anything else (menus, banners, the mascot, sheets), clipped to `radius`. Mouse, wheel,
// keyboard and input methods given to the item go to the page. Without Chromium next to the program `available` is
// false and addresses open in the system browser.
class WebPane : public QQuickItem, public cefh::View {
    Q_OBJECT
    Q_PROPERTY(bool available READ available NOTIFY availableChanged)
    Q_PROPERTY(QString problem READ problem NOTIFY availableChanged)
    // "Chromium 154.0.8037.98 (CEF 154.0.34)" once the engine runs.
    Q_PROPERTY(QString runtimeVersion READ runtimeVersion NOTIFY availableChanged)
    // The profile folder (from Core.state("browser").profileDir) and the language of the pages; read when the engine
    // starts (the first page of the run), shared by every pane.
    Q_PROPERTY(QString profile READ profile WRITE setProfile NOTIFY profileChanged)
    Q_PROPERTY(QString language READ language WRITE setLanguage NOTIFY profileChanged)
    // Kept for older QML: pages are drawn in the scene now, so a sheet over the page needs nothing special.
    Q_PROPERTY(bool covered READ covered WRITE setCovered NOTIFY coveredChanged)
    // One tab only (the quick search): links that want a new window open in place.
    Q_PROPERTY(bool singleTab READ singleTab WRITE setSingleTab NOTIFY singleTabChanged)
    // Keeps the page running and drawing while the item is not visible (the hidden Lens probe).
    Q_PROPERTY(bool keepRendering READ keepRendering WRITE setKeepRendering NOTIFY keepRenderingChanged)
    // Rounded corners of the page (logical pixels).
    Q_PROPERTY(qreal radius MEMBER m_radius NOTIFY radiusChanged)
    Q_PROPERTY(QVariantList tabs READ tabs NOTIFY tabsChanged)
    Q_PROPERTY(int currentTab READ currentTab WRITE setCurrentTab NOTIFY currentChanged)
    Q_PROPERTY(QString url READ url NOTIFY currentChanged)
    Q_PROPERTY(QString title READ title NOTIFY currentChanged)
    Q_PROPERTY(QString icon READ icon NOTIFY currentChanged)
    Q_PROPERTY(bool loading READ loading NOTIFY currentChanged)
    Q_PROPERTY(bool canGoBack READ canGoBack NOTIFY currentChanged)
    Q_PROPERTY(bool canGoForward READ canGoForward NOTIFY currentChanged)
    // The page failed to load: {url, error, code}; empty while it is fine.
    Q_PROPERTY(QVariantMap failure READ failure NOTIFY currentChanged)
    // The address of the link under the pointer (the status bubble of browsers).
    Q_PROPERTY(QString statusText READ statusText NOTIFY statusTextChanged)
    // A page element (a video) asked for full screen.
    Q_PROPERTY(bool fullScreen READ fullScreen NOTIFY fullScreenChanged)
    // CEF (Chromium Embedded) has no Chrome extension support in the versions that draw off screen: always empty.
    Q_PROPERTY(QVariantList extensions READ extensions NOTIFY extensionsChanged)
    Q_PROPERTY(bool extensionsSupported READ extensionsSupported NOTIFY extensionsChanged)
    // The current tab has drawn a picture (false while a new pane or tab is still opening: show a loading state).
    Q_PROPERTY(bool hasPicture READ hasPicture NOTIFY hasPictureChanged)
    // Google Lens of lens(): 0 idle, 1 opening Lens and preparing the picture, 2 picture sent, 3 on the upload address.
    Q_PROPERTY(int lensStage READ lensStage NOTIFY lensStageChanged)
public:
    explicit WebPane(QQuickItem *parent = nullptr);
    ~WebPane() override;

    bool available() const { return m_available; }
    QString problem() const { return m_problem; }
    QString runtimeVersion() const { return m_version; }
    QString profile() const { return m_profile; }
    void setProfile(const QString &p);
    QString language() const { return m_language; }
    void setLanguage(const QString &l);
    bool covered() const { return m_covered; }
    void setCovered(bool c);
    bool singleTab() const { return m_singleTab; }
    void setSingleTab(bool s);
    bool keepRendering() const { return m_keepRendering; }
    void setKeepRendering(bool k);
    QVariantList tabs() const;
    int currentTab() const { return m_current; }
    void setCurrentTab(int id);
    QString url() const;
    QString title() const;
    QString icon() const;
    bool loading() const;
    bool canGoBack() const;
    bool canGoForward() const;
    QVariantMap failure() const;
    QString statusText() const { return m_status; }
    bool fullScreen() const { return m_fullScreen; }
    QVariantList extensions() const { return {}; }
    bool extensionsSupported() const { return false; }
    bool hasPicture() const { return !m_frame.isNull() && m_frameTab == m_current && m_current > 0; }
    int lensStage() const { return m_lensStage; }

    // Starts Chromium and opens an empty, hidden tab ahead of time (the quick search calls it when its bar opens, so
    // the first search does not wait for the engine). Does nothing when a tab is already open.
    Q_INVOKABLE void warmUp();

    // Opens a tab (shown at once unless `show` is false); returns its id.
    Q_INVOKABLE int openTab(const QString &url, bool show = true);
    Q_INVOKABLE void closeTab(int id);
    // Moves a tab in the list (the sidebar's drag and drop).
    Q_INVOKABLE void moveTab(int id, int toIndex);
    // In the current tab (a new one when there is none).
    Q_INVOKABLE void navigate(const QString &url);
    Q_INVOKABLE void back();
    Q_INVOKABLE void forward();
    Q_INVOKABLE void reload();
    Q_INVOKABLE void stop();
    Q_INVOKABLE void zoom(double factor);
    Q_INVOKABLE void focusPage();
    // Google Lens with this picture, in the current tab. When Lens refuses it, Google Images opens instead with the
    // picture on the clipboard (Ctrl+V there); `quiet` (the Lens probe) only emits lensFailed() instead.
    Q_INVOKABLE void lens(const QString &imagePath, bool quiet = false);
    // Runs a script in the page of the current tab (or of `tab`).
    Q_INVOKABLE void runScript(const QString &code, int tab = 0);
    // Runs `body` as the body of an async function in the page and returns a number; evaluated(number, result) brings
    // back what it returned (JSON-like values; {error} when it threw).
    Q_INVOKABLE int evaluate(const QString &body, int tab = 0);
    // "copy", "cut", "paste", "selectAll", "undo", "redo", "delete" in the page.
    Q_INVOKABLE void editCommand(const QString &what);
    // Saves an address (a picture, a link) into the Downloads folder.
    Q_INVOKABLE void downloadUrl(const QString &url);
    Q_INVOKABLE void copyText(const QString &text);
    // The answer to a jsDialog (alert / confirm / prompt) of the page.
    Q_INVOKABLE void answerDialog(int id, bool ok, const QString &text = QString());
    // Find in page; an empty text stops it.
    Q_INVOKABLE void find(const QString &text, bool forward = true);
    Q_INVOKABLE void refreshExtensions();
    Q_INVOKABLE void addExtension(const QString &folder);
    Q_INVOKABLE void removeExtension(const QString &id);
    Q_INVOKABLE void enableExtension(const QString &id, bool on);
    Q_INVOKABLE void cancelDownload(const QString &id);
    Q_INVOKABLE void pauseDownload(const QString &id);
    Q_INVOKABLE void resumeDownload(const QString &id);
    // The download's folder in the file manager, with the file selected (Windows).
    Q_INVOKABLE void showInFolder(const QString &path);
    Q_INVOKABLE void openFile(const QString &path);

    // cefh::View
    QSize cefViewSize() const override;
    qreal cefScale() const override;
    QPoint cefToScreen(const QPoint &viewPoint) const override;
    QRect cefScreenRect() const override;
    quintptr cefNativeWindow() const override;
    void cefPaint(int tab, bool popup, const uchar *bgra, int w, int h) override;
    void cefEvent(const cefh::Event &e) override;

signals:
    void availableChanged();
    void profileChanged();
    void coveredChanged();
    void singleTabChanged();
    void keepRenderingChanged();
    void radiusChanged();
    void tabsChanged();
    void currentChanged();
    void statusTextChanged();
    void fullScreenChanged();
    void extensionsChanged();
    // A page finished loading (for the history).
    void visited(const QString &url, const QString &title);
    // The title of a page arrived after it loaded.
    void titleSeen(const QString &url, const QString &title);
    // A download started or changed: {id, url, path, received, total, state}.
    void download(const QVariantMap &d);
    void message(const QString &text);
    // Escape pressed while the page had the keyboard (the other browser keys reach QML's Shortcuts directly).
    void keyPressed(const QString &name);
    // `status` is Chromium's error code (negative), `http` 0.
    void navFailed(const QString &url, int status, int http);
    // A right click in the page: {x, y (item coordinates), link, image, selection, editable, canCopy, canCut,
    // canPaste, canUndo, canRedo, page}. Without a handler nothing shows.
    void contextMenu(const QVariantMap &info);
    // A page's alert / confirm / prompt: {kind, message, value, origin}; answer with answerDialog(id, …). Without a
    // handler alerts are dismissed and questions answered "Cancel".
    void jsDialog(int id, const QVariantMap &info);
    // The page took the dialog back (it navigated away): hide it.
    void jsDialogClosed(int id);
    void evaluated(int id, const QVariant &result);
    // Google Lens did not take the picture (quiet mode of lens()).
    void lensFailed();
    void hasPictureChanged();
    void lensStageChanged();

protected:
    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override;
    void itemChange(ItemChange change, const ItemChangeData &value) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    bool event(QEvent *e) override;
    void mousePressEvent(QMouseEvent *e) override;
    void mouseReleaseEvent(QMouseEvent *e) override;
    void mouseMoveEvent(QMouseEvent *e) override;
    void mouseDoubleClickEvent(QMouseEvent *e) override;
    void mouseUngrabEvent() override;
    void hoverEnterEvent(QHoverEvent *e) override;
    void hoverMoveEvent(QHoverEvent *e) override;
    void hoverLeaveEvent(QHoverEvent *e) override;
    void wheelEvent(QWheelEvent *e) override;
    void keyPressEvent(QKeyEvent *e) override;
    void keyReleaseEvent(QKeyEvent *e) override;
    void focusInEvent(QFocusEvent *e) override;
    void focusOutEvent(QFocusEvent *e) override;
    void inputMethodEvent(QInputMethodEvent *e) override;
    QVariant inputMethodQuery(Qt::InputMethodQuery query) const override;

private:
    struct TabInfo {
        int id = 0;
        QString url, title, icon;
        QString visitedUrl;   // the last address written to the history
        bool loading = false, back = false, forward = false;
        QVariantMap failure;
    };
    TabInfo *find(int id);
    const TabInfo *findConst(int id) const;
    bool startEngine();
    void openOutside(const QString &url);
    void lensFallback();
    void lensSubmit();
    void setLensStage(int s);
    void lensPrepared(int gen, const QString &data, const QSize &size, qint64 ms);
    void notePicture();
    QString dlKey(int n) const;
    int dlNum(const QString &key) const;
    void syncShown();
    void checkScale();
    void sendKey(QKeyEvent *e, bool down);
    void sendMouse(QMouseEvent *e, bool press, bool release);
    uint modifiers(Qt::KeyboardModifiers m, Qt::MouseButtons b) const;
    void watchWindow(QQuickWindow *w);

    bool m_available = false;
    QString m_problem, m_version, m_profile, m_language, m_status;
    qreal m_radius = 0;
    bool m_covered = false, m_singleTab = false, m_keepRendering = false, m_fullScreen = false;
    int m_current = 0;
    std::vector<TabInfo> m_tabs;
    // the picture of the current tab, and the drop-down list of a <select> over it
    QImage m_frame, m_popup;
    int m_frameTab = 0;
    QRect m_popupRect;
    bool m_popupShown = false;
    bool m_textureDirty = false;
    // what Chromium was last told (to tell it only changes)
    QSize m_sentSize;
    qreal m_sentScale = 0;
    bool m_shown = false;
    QRect m_imeRect;
    // the mouse: clicks counted by hand (double and triple clicks)
    QElapsedTimer m_clickClock;
    QPointF m_clickPos;
    Qt::MouseButton m_clickButton = Qt::NoButton;
    int m_clicks = 0;
    QPointer<QQuickWindow> m_window;
    QList<QMetaObject::Connection> m_windowConns;
    // downloads: the latest state of each (for the throttled progress), keyed by Chromium's id
    QHash<int, QVariantMap> m_dl;
    QHash<int, qint64> m_dlLastSent;
    QElapsedTimer m_clock;
    qint64 m_runStamp = 0;
    // Google Lens: the tab, the picture, and how far it went (1 opening Lens, 2 picture sent, 3 on the results)
    int m_lensTab = 0, m_lensStage = 0;
    bool m_lensQuiet = false;
    QString m_lensImage;
    QTimer m_lensWatch;
    // the picture for Lens, made small and encoded on a worker thread (base64 JPEG and its size); m_lensGen tells a
    // late answer from an old request; lensSubmit() waits for it when the Lens page is faster
    QString m_lensData;
    QSize m_lensSize;
    bool m_lensDataReady = false, m_lensSubmitWanted = false;
    int m_lensGen = 0;
    QElapsedTimer m_lensClock;
    // timings for the log: from a tab's opening to its first picture
    QElapsedTimer m_firstPaintClock;
    bool m_hadPicture = false;
    // the frame rate Chromium was last told for the current tab (a hidden page kept running draws slowly)
    int m_sentFps = 0, m_fpsTab = 0;
    int m_nextEval = 1;
};
