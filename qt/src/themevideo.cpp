#include "themevideo.h"
#include "corebridge.h"
#include <QPainter>
#include <QPainterPath>
#include <QQuickWindow>

extern "C" {
#include "bridge.h"
}

ThemeVideo::ThemeVideo(QQuickItem *parent) : QQuickPaintedItem(parent) {
    setOpaquePainting(false);
    setAntialiasing(true);
    // ~30 pictures a second: YouTube videos are 24-30 fps, and mpv only gives a picture when it has a new one
    m_timer.setInterval(33);
    connect(&m_timer, &QTimer::timeout, this, &ThemeVideo::tick);
    // the timer only runs while the item can be seen
    connect(this, &QQuickItem::visibleChanged, this, &ThemeVideo::updateTimer);
}

void ThemeVideo::setRunning(bool r) {
    if (r == m_running) return;
    m_running = r;
    m_force = true;
    updateTimer();
    emit runningChanged();
}

void ThemeVideo::setRadius(qreal r) {
    if (qFuzzyCompare(r, m_radius)) return;
    m_radius = r;
    update();
    emit radiusChanged();
}

void ThemeVideo::setSource(const QString &s) {
    const bool clip = s == QLatin1String("clip");
    if (clip == m_clip) return;
    m_clip = clip;
    clearFrame();
    emit sourceChanged();
}

void ThemeVideo::clearFrame() {
    const bool had = !m_img.isNull();
    m_img = QImage();
    m_force = true;
    update();
    if (had) emit hasFrameChanged();
}

void ThemeVideo::updateTimer() {
    if (m_running && isVisible()) {
        m_force = true;
        if (!m_timer.isActive()) m_timer.start();
    } else {
        m_timer.stop();
    }
}

void ThemeVideo::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickPaintedItem::geometryChange(newGeometry, oldGeometry);
    // a sharper (or smaller) picture for the new size
    if (newGeometry.size() != oldGeometry.size()) m_force = true;
}

void ThemeVideo::tick() {
    Bridge *b = CoreBridge::handle();
    if (!b || width() < 16 || height() < 16) return;
    // pixels of the screen, at most 640x360: the panel is small and the pictures are drawn by the processor
    const qreal dpr = window() ? window()->effectiveDevicePixelRatio() : 1.0;
    const int mw = qMin(640, int(width() * dpr));
    const int mh = qMin(360, int(height() * dpr));
    int w = 0, h = 0;
    size_t len = 0;
    unsigned char *px = (m_clip ? rh_clip_frame : rh_theme_frame)(b, mw, mh, m_force ? 1 : 0, &w, &h, &len);
    if (!px) return;
    m_force = false;
    const bool had = !m_img.isNull();
    if (w > 0 && h > 0 && len >= size_t(w) * size_t(h) * 4)
        m_img = QImage(px, w, h, w * 4, QImage::Format_RGB32).copy();
    rh_free_pixels(px, len);
    update();
    if (!had && !m_img.isNull()) emit hasFrameChanged();
}

void ThemeVideo::paint(QPainter *p) {
    if (m_img.isNull()) return;
    // fitted inside the item, keeping the shape of the video; the rest stays as the QML background
    const QSizeF s = QSizeF(m_img.size()).scaled(QSizeF(width(), height()), Qt::KeepAspectRatio);
    const QRectF r((width() - s.width()) / 2, (height() - s.height()) / 2, s.width(), s.height());
    p->setRenderHint(QPainter::SmoothPixmapTransform, true);
    p->setRenderHint(QPainter::Antialiasing, true);
    if (m_radius > 0) {
        QPainterPath clip;
        clip.addRoundedRect(QRectF(0, 0, width(), height()), m_radius, m_radius);
        p->setClipPath(clip);
    }
    p->drawImage(r, m_img);
}
