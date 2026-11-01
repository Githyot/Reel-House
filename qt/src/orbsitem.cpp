#include "orbsitem.h"

#include "bridge.h"
#include "orbspaint.h"

#include <QDateTime>
#include <QQuickWindow>
#include <QSGSimpleTextureNode>
#include <QSGTexture>
#include <algorithm>
#include <vector>

namespace {
// 7 orbs x 72 trail points x 5 floats (reelhouse::orbs::ORBS * TRAIL), with room to spare
constexpr int kOrbs = 7;
constexpr size_t kCap = 7 * 128 * 5;

// The node keeps its texture and frees it with itself (and when the next frame replaces it).
struct OrbsNode : QSGSimpleTextureNode {
    QSGTexture *tex = nullptr;
    ~OrbsNode() override { delete tex; }
};

// Local wall clock in seconds: the formations follow the seconds of the clock, like on the console.
double localNow() {
    static qint64 offsetAt = 0;
    static int offset = 0;
    const qint64 ms = QDateTime::currentMSecsSinceEpoch();
    if (ms - offsetAt > 60000 || offsetAt == 0) {
        offset = QDateTime::currentDateTime().offsetFromUtc();
        offsetAt = ms;
    }
    return ms / 1000.0 + offset;
}
} // namespace

OrbsItem::OrbsItem(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents, true);
}

void OrbsItem::setRunning(bool on) {
    if (m_running == on) return;
    m_running = on;
    connectWindow();
    if (on) update();
    emit runningChanged();
}

void OrbsItem::setDark(bool on) {
    if (m_dark == on) return;
    m_dark = on;
    emit darkChanged();
    update();
}

void OrbsItem::setRing(qreal r) {
    if (qFuzzyCompare(m_ring, r)) return;
    m_ring = r;
    emit ringChanged();
    update();
}

void OrbsItem::itemChange(ItemChange change, const ItemChangeData &value) {
    if (change == ItemSceneChange) connectWindow();
    QQuickItem::itemChange(change, value);
}

// One frame per frame of the window while running: afterAnimating comes on the GUI thread before each frame is
// synchronised, and update() asks for the next one. Not running: no connection, no work at all.
void OrbsItem::connectWindow() {
    QObject::disconnect(m_conn);
    if (m_running && window()) m_conn = connect(window(), &QQuickWindow::afterAnimating, this, &OrbsItem::tick);
}

// The square the orbs live in: the sphere and its glow, centred in the item.
QRectF OrbsItem::area() const {
    const qreal side = std::min(width(), height()) * m_ring * 2 * 1.3;
    return QRectF((width() - side) / 2, (height() - side) / 2, side, side);
}

void OrbsItem::tick() {
    if (!m_running || !isVisible() || width() < 8 || height() < 8) return;
    const QRectF a = area();
    const qreal scale = std::min<qreal>(1.0, 640.0 / a.width()) * (window() ? std::min<qreal>(window()->devicePixelRatio(), 1.5) : 1.0);
    const int px = std::max(16, int(a.width() * scale));
    if (m_img.width() != px) m_img = QImage(px, px, QImage::Format_ARGB32_Premultiplied);
    static std::vector<float> buf(kCap);
    const size_t n = rh_orbs(localNow(), buf.data(), buf.size());
    const int trail = int(n / kOrbs);
    const float radius = float(std::min(width(), height()) * m_ring * (px / a.width()));
    paintOrbs(m_img, buf.data(), trail > 0 ? kOrbs : 0, trail, radius, OrbsStyle::forDark(m_dark));
    m_fresh = true;
    update();
}

QSGNode *OrbsItem::updatePaintNode(QSGNode *old, UpdatePaintNodeData *) {
    auto *node = static_cast<OrbsNode *>(old);
    if (m_img.isNull() || !window()) {
        delete node;
        return nullptr;
    }
    if (!node) {
        node = new OrbsNode;
        node->setFiltering(QSGTexture::Linear);
        node->setOwnsTexture(false);
        m_fresh = true;
    }
    if (m_fresh || !node->tex) {
        QSGTexture *t = window()->createTextureFromImage(m_img, QQuickWindow::TextureHasAlphaChannel);
        node->setTexture(t);
        delete node->tex;
        node->tex = t;
        m_fresh = false;
    }
    node->setRect(area());
    return node;
}
