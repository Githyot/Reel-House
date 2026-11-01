#pragma once
#include <QImage>
#include <QQuickItem>

// The orbs of time (PS2 menu background) as a native item: each frame the core gives the orbs and their long trails
// (rh_orbs, reelhouse::orbs), paintOrbs adds them into a small image (at most 640 px wide: the glow is soft, so a
// smaller picture scaled up looks the same and costs little), and the scene graph shows it as one texture centred in
// the item. It only runs while `running` is true (QML: visible, window in front and not minimized); then it draws
// once per frame of the window (afterAnimating), else it stops completely.
class OrbsItem : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(bool running READ running WRITE setRunning NOTIFY runningChanged)
    // dark windows: white and cyan orbs; light windows: deep blue
    Q_PROPERTY(bool dark READ dark WRITE setDark NOTIFY darkChanged)
    // radius of the sphere as a share of the smaller side of the item
    Q_PROPERTY(qreal ring READ ring WRITE setRing NOTIFY ringChanged)
public:
    explicit OrbsItem(QQuickItem *parent = nullptr);
    bool running() const { return m_running; }
    void setRunning(bool on);
    bool dark() const { return m_dark; }
    void setDark(bool on);
    qreal ring() const { return m_ring; }
    void setRing(qreal r);

signals:
    void runningChanged();
    void darkChanged();
    void ringChanged();

protected:
    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override;
    void itemChange(ItemChange change, const ItemChangeData &value) override;

private:
    void tick();
    void connectWindow();
    QRectF area() const;
    bool m_running = false;
    bool m_dark = true;
    qreal m_ring = 0.36;
    QImage m_img;
    bool m_fresh = false;
    QMetaObject::Connection m_conn;
};
