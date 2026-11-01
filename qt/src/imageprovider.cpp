#include "imageprovider.h"
#include <QEventLoop>
#include <QMutex>
#include <QCache>
#include "bridge.h"
#include <QQuickTextureFactory>
#include <QQuickImageResponse>
#include <QNetworkReply>
#include <QNetworkDiskCache>
#include <QAbstractNetworkCache>
#include <QStandardPaths>
#include <QFile>
#include <QUrl>
#include <QDebug>
#include <QAtomicInt>
#include <QCryptographicHash>
#include <QDir>
#include <QTimer>
#include <QImageWriter>
#include <QFileInfo>
#include <QtConcurrent>
#include <QFutureWatcher>
#include <utility>

namespace {
QAtomicInt g_asked(0), g_done(0), g_gen(0);
// Where the pictures are kept on this device as WebP (smaller than the originals): the data folder, "pictures".
QString picturesDir() {
    static const QString dir = [] {
        QString r;
        if (char *p = rh_data_dir()) {
            r = QString::fromUtf8(p) + "/pictures";
            rh_free_str(p);
        }
        return r;
    }();
    return dir;
}

QString webpPath(const QString &url) {
    const QString d = picturesDir();
    if (d.isEmpty()) return {};
    // The manga server of the app has another port at each start: its pictures are kept by their path only.
    QString key = url;
    for (const QString pre : {QStringLiteral("http://127.0.0.1:"), QStringLiteral("http://localhost:")}) {
        if (url.startsWith(pre)) { const int i = url.indexOf('/', pre.size()); if (i > 0) key = QStringLiteral("local") + url.mid(i); }
    }
    return d + "/" + QString::fromLatin1(QCryptographicHash::hash(key.toUtf8(), QCryptographicHash::Md5).toHex()) + ".webp";
}

// Keeps a picture as WebP (quality 86) the first time it is loaded from the web, so the next time it is read from
// the device and not downloaded again.
void storeWebp(const QString &url, const QImage &img) {
    const QString path = webpPath(url);
    if (path.isEmpty() || img.isNull() || QFileInfo::exists(path)) return;
    QDir().mkpath(picturesDir());
    QImageWriter w(path, "webp");
    w.setQuality(86);
    if (!w.write(img)) qWarning() << "picture not kept as WebP:" << url << w.errorString();
}

QImage decodeFull(const QByteArray &bytes) {
    QImage img;
    if (bytes.size() > 12 && bytes.startsWith("RIFF") && bytes.mid(8, 4) == "WEBP") {
        int w = 0, h = 0; size_t len = 0;
        uchar *px = rh_decode_webp(reinterpret_cast<const uchar *>(bytes.constData()), size_t(bytes.size()), &w, &h, &len);
        if (px) {
            img = QImage(px, w, h, w * 4, QImage::Format_RGBA8888).copy();
            rh_free_pixels(px, len);
        }
    } else {
        img.loadFromData(bytes);
    }
    // A graphics card cannot hold a picture bigger than its texture limit, and the WebP writer refuses sides over 16383:
    // shrink such a page (a very long webtoon strip) first, keeping the proportion.
    constexpr int kMax = 8192;
    if (!img.isNull() && (img.width() > kMax || img.height() > kMax))
        img = img.scaled(kMax, kMax, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    return img;
}

QImage fit(QImage img, const QSize &want) {
    if (!img.isNull() && want.isValid() && want.width() > 0 && img.width() > want.width())
        img = img.scaledToWidth(want.width(), Qt::SmoothTransformation);
    // A graphics card cannot hold a picture bigger than its texture limit (often 8192 or 16384 on Windows): a very long
    // webtoon page would stay blank or take the program down. Keep the proportion, shrink to fit the limit.
    constexpr int kMax = 8192;
    if (!img.isNull() && (img.width() > kMax || img.height() > kMax))
        img = img.scaled(kMax, kMax, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    return img;
}

QImage decode(const QByteArray &bytes, const QSize &want) { return fit(decodeFull(bytes), want); }

// Local pictures already made smaller (the upscaled pages): kept decoded in memory, so turning back to a page or
// scrolling a strip does not read and shrink a large PNG again. Cost in KB, about 320 MB at most.
QCache<QString, QImage> &localCache() { static QCache<QString, QImage> c(320 * 1024); return c; }
QMutex &localCacheLock() { static QMutex m; return m; }

QImage decodeLocal(const QString &path, const QSize &want) {
    const QString key = path + '|' + QString::number(want.width());
    {
        QMutexLocker l(&localCacheLock());
        if (QImage *hit = localCache().object(key)) return *hit;
    }
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly)) return {};
    const QByteArray bytes = f.readAll();
    f.close();
    const QImage full = decodeFull(bytes);
    QImage img = fit(full, want);
    // An upscaled page is written back once as lossy WebP (quality 88) at the drawn size: the engine writes lossless
    // files, and 4x models make pages larger than what is drawn. Only files of the upscaled folder are touched.
    const bool lossless = bytes.size() > 16 && bytes.mid(12, 4) == "VP8L";
    const bool png = bytes.startsWith("\x89PNG");
    if (!img.isNull() && path.contains("/upscaled/") && (img.width() < full.width() || lossless || png)) {
        const QString tmp = path + ".tmp";
        QImageWriter w(tmp, "webp");
        w.setQuality(88);
        if (w.write(img)) { QFile::remove(path); QFile::rename(tmp, path); } else QFile::remove(tmp);
    }
    if (!img.isNull()) {
        QMutexLocker l(&localCacheLock());
        localCache().insert(key, new QImage(img), int(img.sizeInBytes() / 1024) + 1);
    }
    return img;
}

bool isLocalUrl(const QString &url) { return url.startsWith("file:") || (url.size() > 2 && url[1] == ':') || url.startsWith('/'); }

// The picture from the device only: a local file or the WebP kept for a web address. Never touches the network.
QImage loadPictureLocalImpl(const QString &url) {
    if (isLocalUrl(url)) {
        QFile f(url.startsWith("file:") ? QUrl(url).toLocalFile() : url);
        return f.open(QIODevice::ReadOnly) ? decodeFull(f.readAll()) : QImage();
    }
    if (const QString kept = webpPath(url); !kept.isEmpty() && QFileInfo::exists(kept)) {
        QFile f(kept);
        if (f.open(QIODevice::ReadOnly)) return decodeFull(f.readAll());
    }
    return {};
}

QImage loadPictureBlockingImpl(const QString &url) {
    if (const QImage img = loadPictureLocalImpl(url); !img.isNull() || isLocalUrl(url)) return img;
    QNetworkAccessManager nam;
    QNetworkRequest rq{QUrl(url)};
    rq.setHeader(QNetworkRequest::UserAgentHeader, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36");
    rq.setTransferTimeout(30000);
    QNetworkReply *r = nam.get(rq);
    QEventLoop loop;
    QObject::connect(r, &QNetworkReply::finished, &loop, &QEventLoop::quit);
    loop.exec();
    QImage img;
    if (r->error() == QNetworkReply::NoError) {
        img = decodeFull(r->readAll());
        storeWebp(url, img);
    }
    r->deleteLater();
    return img;
}

class Response : public QQuickImageResponse {
public:
    Response(WebImageProvider *p, const QString &url, const QString &title, const QSize &want, bool fresh = false) : m_want(want), m_url(url), m_title(title), m_counted(fresh) {
        if (m_counted) g_asked.fetchAndAddRelaxed(1);
        if (fresh) { const QString old = webpPath(m_url); if (!old.isEmpty()) QFile::remove(old); }
        if (url.startsWith("file:") || (url.size() > 2 && url[1] == ':') || url.startsWith('/')) {
            QString path = url.startsWith("file:") ? QUrl(url).toLocalFile() : url;
            m_image = decodeLocal(QDir::fromNativeSeparators(path), want);
            QMetaObject::invokeMethod(this, &Response::done, Qt::QueuedConnection);
            return;
        }
        Q_UNUSED(p);
        // A picture kept on this device as WebP is read from there, without going to the web.
        if (const QString kept = webpPath(m_url); !kept.isEmpty() && QFileInfo::exists(kept)) {
            QFile f(kept);
            if (f.open(QIODevice::ReadOnly)) m_image = decode(f.readAll(), want);
            QMetaObject::invokeMethod(this, &Response::done, Qt::QueuedConnection);
            return;
        }
        start(fresh);
    }
    // `fresh`: skip the disk cache. A reply cut short (the picture scrolled away) can leave a broken copy there; such a copy
    // is thrown away and the picture asked for again, once.
    void start(bool fresh) {
        QNetworkRequest rq{QUrl(m_url)};
        rq.setAttribute(QNetworkRequest::CacheLoadControlAttribute, fresh ? QNetworkRequest::AlwaysNetwork : QNetworkRequest::PreferCache);
        rq.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
        // Some picture hosts refuse requests that do not look like a browser (same as the GTK version).
        rq.setHeader(QNetworkRequest::UserAgentHeader, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36");
        rq.setRawHeader("Accept", "image/png,image/jpeg,image/gif,image/*;q=0.8");
        rq.setTransferTimeout(25000);
        // Some picture servers drop HTTP/2 streams when many pictures are asked for at once.
        rq.setAttribute(QNetworkRequest::Http2AllowedAttribute, false);
        m_reply = WebImageProvider::nam()->get(rq);
        QObject::connect(m_reply, &QNetworkReply::finished, this, [this, fresh] {
            m_http = m_reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            const bool cached = m_reply->attribute(QNetworkRequest::SourceIsFromCacheAttribute).toBool();
            const bool ok = m_reply->error() == QNetworkReply::NoError;
            if (ok) {
                const QByteArray body = m_reply->readAll();
                // breadcrumbs: when the program dies without a message, the last line of the log says which step it was in
                qInfo() << "picture: got" << body.size() << "bytes of" << m_url;
                const QImage full = decodeFull(body);
                qInfo() << "picture: decoded" << full.width() << "x" << full.height();
                storeWebp(m_url, full);
                qInfo() << "picture: kept";
                m_image = fit(full, m_want);
                if (full.isNull()) m_err = QString("HTTP %1, %2 bytes, type %3, starts with %4").arg(m_http).arg(body.size())
                    .arg(m_reply->header(QNetworkRequest::ContentTypeHeader).toString(), QString::fromLatin1(body.left(12).toHex(' ')));
            }
            else m_err = m_reply->errorString();
            const bool canceled = m_reply->error() == QNetworkReply::OperationCanceledError;
            m_reply->deleteLater();
            m_reply = nullptr;
            if (m_image.isNull() && !canceled && !ok && m_tries < 2 && m_http != 404) {
                // A server that drops a request (the local manga server while it wakes up, a busy host): try again, as GTK does.
                ++m_tries;
                QTimer::singleShot(700 * m_tries, this, [this] { start(true); });
                return;
            }
            if (m_image.isNull() && !canceled && !ok && m_http != 404 && !m_byCore && m_url.startsWith("http")) {
                // Qt's own network failed (on a phone that is mostly its TLS): the core asks for the same picture with
                // its own HTTPS, which also fetches the lists and the streams (HP: covers grey on Android).
                m_byCore = true;
                auto *w = new QFutureWatcher<QByteArray>(this);
                const QByteArray u = m_url.toUtf8();
                connect(w, &QFutureWatcher<QByteArray>::finished, this, [this, w, fresh] {
                    const QByteArray b = w->result();
                    w->deleteLater();
                    if (!b.isEmpty()) {
                        const QImage full = decodeFull(b);
                        if (!full.isNull()) {
                            storeWebp(m_url, full);
                            m_image = fit(full, m_want);
                            done();
                            return;
                        }
                    }
                    qWarning() << "picture: the core could not fetch it either:" << m_url << m_err;
                    finishFailed(fresh);
                });
                w->setFuture(QtConcurrent::run([u] {
                    size_t len = 0;
                    uchar *px = rh_fetch_picture(u.constData(), &len);
                    if (!px) return QByteArray();
                    QByteArray out(reinterpret_cast<const char *>(px), qsizetype(len));
                    rh_free_pixels(px, len);
                    return out;
                }));
                return;
            }
            finishFailed(fresh, canceled, ok, cached);
        });
    }
    // What is left after a failed or unreadable reply: the cover found by the title, a fresh try, or giving up.
    void finishFailed(bool fresh, bool canceled = false, bool ok = false, bool cached = false) {
        {
            if (m_image.isNull() && !canceled && !ok && !m_title.isEmpty() && !m_byTitle) {
                m_byTitle = true;
                auto *w = new QFutureWatcher<QByteArray>(this);
                const QByteArray t = m_title.toUtf8();
                connect(w, &QFutureWatcher<QByteArray>::finished, this, [this, w] {
                    const QByteArray b = w->result();
                    w->deleteLater();
                    if (!b.isEmpty()) {
                        const QImage full = decodeFull(b);
                        storeWebp(m_url, full);
                        m_image = fit(full, m_want);
                    }
                    if (m_image.isNull()) qWarning() << "picture not shown:" << m_url << m_err;
                    done();
                });
                w->setFuture(QtConcurrent::run([t] {
                    size_t len = 0;
                    uchar *px = rh_title_cover(t.constData(), &len);
                    if (!px) return QByteArray();
                    QByteArray out(reinterpret_cast<const char *>(px), qsizetype(len));
                    rh_free_pixels(px, len);
                    return out;
                }));
                return;
            }
            if (m_image.isNull() && !canceled && !fresh && (cached || ok)) {
                if (auto *c = WebImageProvider::nam()->cache()) c->remove(QUrl(m_url));
                m_err.clear();
                start(true);
                return;
            }
            if (m_image.isNull() && !canceled) qWarning() << "picture not shown:" << m_url << (m_err.isEmpty() ? "unreadable data" : m_err);
            done();
        }
    }
    ~Response() override {
        if (m_counted) g_done.fetchAndAddRelaxed(1);
        // abort() emits finished() synchronously: disconnect first so the handler (which uses this object) never runs.
        if (QNetworkReply *r = std::exchange(m_reply, nullptr)) {
            QObject::disconnect(r, nullptr, this, nullptr);
            r->abort();
            r->deleteLater();
        }
    }
    QQuickTextureFactory *textureFactory() const override { return QQuickTextureFactory::textureFactoryForImage(m_image); }
    QString errorString() const override { return m_err; }
    void cancel() override { if (m_reply) m_reply->abort(); }
private:
    void done() { if (m_counted) { m_counted = false; g_done.fetchAndAddRelaxed(1); } emit finished(); }
    QSize m_want; QString m_url, m_title, m_err; int m_tries = 0, m_http = 0; bool m_byTitle = false, m_byCore = false; bool m_counted = false; QImage m_image; QNetworkReply *m_reply = nullptr;
};
}

// Declared in imageprovider.h for the page editor; the body lives with the other picture helpers above.
QImage loadPictureBlocking(const QString &url) { return loadPictureBlockingImpl(url); }
QImage loadPictureLocal(const QString &url) { return loadPictureLocalImpl(url); }

WebImageProvider::WebImageProvider() {}

int WebImageProvider::startReload() {
    g_asked.storeRelaxed(0);
    g_done.storeRelaxed(0);
    return g_gen.fetchAndAddRelaxed(1) + 1;
}

void WebImageProvider::stats(int &asked, int &done) {
    asked = g_asked.loadRelaxed();
    done = g_done.loadRelaxed();
}

QNetworkAccessManager *WebImageProvider::nam() {
    static thread_local QNetworkAccessManager *m = nullptr;
    if (!m) {
        m = new QNetworkAccessManager;
        auto *cache = new QNetworkDiskCache(m);
        static QAtomicInt n = 0;
        const int id = n.fetchAndAddRelaxed(1);
        cache->setCacheDirectory(QStandardPaths::writableLocation(QStandardPaths::CacheLocation) + "/images" + (id ? QString::number(id) : QString()));
        cache->setMaximumCacheSize(1024LL * 1024 * 1024);
        m->setCache(cache);
    }
    return m;
}

QQuickImageResponse *WebImageProvider::requestImageResponse(const QString &id, const QSize &requestedSize) {
    // "<encoded address>@@<encoded title>": the title lets the core look for a cover by name when the address fails.
    const int at = id.indexOf("@@");
    // A trailing "~~<n>" is the generation of a reload of the covers.
    QString rest = id;
    bool fresh = false;
    if (const int g = rest.lastIndexOf("~~"); g >= 0 && rest.mid(g + 2).toInt() > 0) { fresh = rest.mid(g + 2).toInt() == g_gen.loadRelaxed(); rest = rest.left(g); }
    const int at2 = rest.indexOf("@@");
    const QString url = QUrl::fromPercentEncoding((at2 < 0 ? rest : rest.left(at2)).toUtf8());
    const QString title = at2 < 0 ? QString() : QUrl::fromPercentEncoding(rest.mid(at2 + 2).toUtf8());
    Q_UNUSED(at);
    return new Response(this, url, title, requestedSize, fresh);
}
