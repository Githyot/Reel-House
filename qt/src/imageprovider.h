#pragma once
#include <QQuickAsyncImageProvider>
#include <QNetworkAccessManager>

// image://web/<encoded address>: downloads a picture (cached on disk) and decodes it. WebP is decoded by the Rust core,
// so the covers show even when Qt's own WebP plugin is missing.
class WebImageProvider : public QQuickAsyncImageProvider {
public:
    WebImageProvider();
    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &requestedSize) override;
    // Qt asks for pictures on its own picture-loading thread: each thread gets its own network manager (a manager
    // may only be used on the thread that made it).
    static QNetworkAccessManager *nam();
    // "Reload the covers": a new generation makes every picture asked for with it come from the web again (not from the
    // disk cache or the WebP kept on the device); `stats` counts the pictures asked for in this generation and the ones done.
    static int startReload();
    static void stats(int &asked, int &done);
};

// The picture of `url` decoded at full size, for the page editor: a local file, the WebP kept on this device, or the web
// (waits for the download). Safe to call from any thread.
QImage loadPictureBlocking(const QString &url);
// Only what is on this device (a local file or the WebP kept for a web address); a null picture otherwise. Never waits
// for the network, so it is safe on the interface thread.
QImage loadPictureLocal(const QString &url);
