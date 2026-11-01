#include "editedpage.h"
#include "imageprovider.h"
#include <QCache>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMutex>
#include <QPainter>
#include <QPainterPath>
#include <QUrl>
#include <QVector>

namespace {
QMutex g_lock;
QHash<QString, QString> &opsMap() { static QHash<QString, QString> m; return m; }
// the original pages (decoding is the slow part) and the finished ones, by address and by address + edits
QCache<QString, QImage> &baseCache() { static QCache<QString, QImage> c(6); return c; }
QCache<QString, QImage> &doneCache() { static QCache<QString, QImage> c(6); return c; }

QColor colorOf(const QJsonObject &o) {
    QColor c(o.value("c").toString("#ffffff"));
    c.setAlphaF(qBound(0.0, o.value("a").toDouble(1.0), 1.0));
    return c;
}

QPainterPath pathOf(const QJsonArray &p, double w, double h) {
    QPainterPath path;
    for (int i = 0; i + 1 < p.size(); i += 2) {
        const QPointF pt(p[i].toDouble() * w, p[i + 1].toDouble() * h);
        if (i == 0) path.moveTo(pt); else path.lineTo(pt);
    }
    if (p.size() == 2) path.lineTo(path.currentPosition() + QPointF(0.01, 0)); // a dot
    return path;
}

// Scanline flood fill on what is seen (page + edits), from the seed, over pixels close to the seed colour.
void bucket(const QImage &seen, QImage &layer, int sx, int sy, const QColor &paint, int tol) {
    const int w = seen.width(), h = seen.height();
    if (sx < 0 || sy < 0 || sx >= w || sy >= h) return;
    const QRgb seed = seen.pixel(sx, sy);
    auto near = [&](QRgb c) {
        return qAbs(qRed(c) - qRed(seed)) <= tol && qAbs(qGreen(c) - qGreen(seed)) <= tol && qAbs(qBlue(c) - qBlue(seed)) <= tol;
    };
    QVector<quint8> done(w * h, 0);
    QVector<QPoint> stack{QPoint(sx, sy)};
    const QRgb out = paint.rgba();
    // premultiplied target: write through a painter-free path
    while (!stack.isEmpty()) {
        QPoint p = stack.takeLast();
        int x = p.x();
        const int y = p.y();
        while (x >= 0 && !done[y * w + x] && near(seen.pixel(x, y))) --x;
        ++x;
        bool up = false, down = false;
        while (x < w && !done[y * w + x] && near(seen.pixel(x, y))) {
            done[y * w + x] = 1;
            layer.setPixel(x, y, out);
            if (y > 0) {
                const bool ok = !done[(y - 1) * w + x] && near(seen.pixel(x, y - 1));
                if (ok && !up) { stack.append(QPoint(x, y - 1)); up = true; } else if (!ok) up = false;
            }
            if (y < h - 1) {
                const bool ok = !done[(y + 1) * w + x] && near(seen.pixel(x, y + 1));
                if (ok && !down) { stack.append(QPoint(x, y + 1)); down = true; } else if (!ok) down = false;
            }
            ++x;
        }
    }
}

// `localOnly`: never waits for the network (the interface thread); a page not on the device comes back null.
QImage base(const QString &url, bool localOnly = false) {
    {
        QMutexLocker l(&g_lock);
        if (QImage *b = baseCache().object(url)) return *b;
    }
    QImage img = (localOnly ? loadPictureLocal(url) : loadPictureBlocking(url)).convertToFormat(QImage::Format_ARGB32);
    if (!img.isNull()) {
        QMutexLocker l(&g_lock);
        baseCache().insert(url, new QImage(img));
    }
    return img;
}

QImage apply(const QImage &page, const QString &opsJson, bool crop) {
    const QJsonArray ops = QJsonDocument::fromJson(opsJson.toUtf8()).array();
    const double w = page.width(), h = page.height();
    QImage layer(page.size(), QImage::Format_ARGB32);
    layer.fill(Qt::transparent);
    QRectF cut(0, 0, 1, 1);
    for (const QJsonValue &v : ops) {
        const QJsonObject o = v.toObject();
        const QString t = o.value("t").toString();
        if (t == "stroke") {
            QPainter p(&layer);
            p.setRenderHint(QPainter::Antialiasing);
            const bool eraser = o.value("tool").toString() == "eraser";
            QPen pen(eraser ? QColor(0, 0, 0, 255) : colorOf(o), qMax(1.0, o.value("s").toDouble(0.01) * w), Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
            p.setPen(pen);
            if (eraser) {
                // the eraser takes the edits away where it passes (opacity = how much)
                p.setCompositionMode(QPainter::CompositionMode_DestinationOut);
                QColor k(0, 0, 0);
                k.setAlphaF(qBound(0.0, o.value("a").toDouble(1.0), 1.0));
                pen.setColor(k);
                p.setPen(pen);
            }
            p.drawPath(pathOf(o.value("p").toArray(), w, h));
        } else if (t == "fill") {
            QImage seen = page;
            { QPainter p(&seen); p.drawImage(0, 0, layer); }
            QImage mask(page.size(), QImage::Format_ARGB32);
            mask.fill(Qt::transparent);
            bucket(seen, mask, int(o.value("x").toDouble() * w), int(o.value("y").toDouble() * h), colorOf(o), qBound(0, o.value("tol").toInt(40), 255));
            QPainter p(&layer);
            p.drawImage(0, 0, mask);
        } else if (t == "crop") {
            cut = QRectF(o.value("x").toDouble(), o.value("y").toDouble(), o.value("w").toDouble(1), o.value("h").toDouble(1)).intersected(QRectF(0, 0, 1, 1));
        }
    }
    QImage out = page.copy();
    { QPainter p(&out); p.drawImage(0, 0, layer); }
    if (crop && cut != QRectF(0, 0, 1, 1) && cut.width() > 0.01 && cut.height() > 0.01)
        out = out.copy(QRect(int(cut.x() * w), int(cut.y() * h), int(cut.width() * w), int(cut.height() * h)));
    return out;
}
} // namespace

void EditedPageProvider::setOps(const QString &url, const QString &opsJson) {
    QMutexLocker l(&g_lock);
    opsMap().insert(url, opsJson);
}

QString EditedPageProvider::ops(const QString &url) {
    QMutexLocker l(&g_lock);
    return opsMap().value(url);
}

QRectF EditedPageProvider::cropOf(const QString &opsJson) {
    QRectF cut(0, 0, 1, 1);
    for (const QJsonValue &v : QJsonDocument::fromJson(opsJson.toUtf8()).array()) {
        const QJsonObject o = v.toObject();
        if (o.value("t").toString() == "crop")
            cut = QRectF(o.value("x").toDouble(), o.value("y").toDouble(), o.value("w").toDouble(1), o.value("h").toDouble(1)).intersected(QRectF(0, 0, 1, 1));
    }
    return cut;
}

QImage EditedPageProvider::cutOut(const QString &url, const QRectF &r) {
    const QImage page = base(url);
    if (page.isNull()) return page;
    // the edits are painted, but the page's own crop is not applied: `r` is measured on the whole original page
    const QImage seen = apply(page, ops(url), false);
    const QRectF n = r.intersected(QRectF(0, 0, 1, 1));
    const QRect px = QRect(int(n.x() * seen.width()), int(n.y() * seen.height()), int(n.width() * seen.width()), int(n.height() * seen.height())).intersected(seen.rect());
    if (px.width() < 2 || px.height() < 2) return QImage();
    return seen.copy(px);
}

QImage EditedPageProvider::render(const QString &url) {
    const QString o = ops(url);
    const QString key = url + '\n' + o;
    {
        QMutexLocker l(&g_lock);
        if (QImage *d = doneCache().object(key)) return *d;
    }
    const QImage page = base(url);
    if (page.isNull()) return page;
    QImage out = apply(page, o, true);
    QMutexLocker l(&g_lock);
    doneCache().insert(key, new QImage(out));
    return out;
}

QString EditedPageProvider::pickColor(const QString &url, double x, double y) {
    // Called from QML on the interface thread: only what is already on the device, no nested event loop.
    const QImage page = base(url, true);
    if (page.isNull()) return "#ffffff";
    const QImage seen = apply(page, ops(url), false);
    const int px = qBound(0, int(x * seen.width()), seen.width() - 1), py = qBound(0, int(y * seen.height()), seen.height() - 1);
    return QColor(seen.pixel(px, py)).name();
}

QImage EditedPageProvider::requestImage(const QString &id, QSize *size, const QSize &requestedSize) {
    // id: <encoded address>@<revision>
    const QString url = QUrl::fromPercentEncoding(id.section('@', 0, 0).toUtf8());
    QImage img = render(url);
    if (size) *size = img.size();
    if (!img.isNull() && requestedSize.width() > 0 && img.width() > requestedSize.width())
        img = img.scaledToWidth(requestedSize.width(), Qt::SmoothTransformation);
    return img;
}
