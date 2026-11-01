#pragma once
#include <QQuickPaintedItem>
#include <QImage>
#include <QTimer>

// The opening / ending video playing inside the X-Ray. A second mpv in the core draws small pictures into memory;
// this item takes the newest one about 30 times a second (only while `running` and visible) and paints it, fitted and
// with rounded corners. The controls and the state (Core.state("theme_video")) are ordinary QML around it.
class ThemeVideo : public QQuickPaintedItem {
    Q_OBJECT
    // poll for pictures (the panel is open and a video is loaded)
    Q_PROPERTY(bool running READ running WRITE setRunning NOTIFY runningChanged)
    Q_PROPERTY(qreal radius READ radius WRITE setRadius NOTIFY radiusChanged)
    // where the pictures come from: "theme" (the X-Ray video, default) or "clip" (the clip of a Study card)
    Q_PROPERTY(QString source READ source WRITE setSource NOTIFY sourceChanged)
    // a picture has arrived since the video started (until then the QML shows the thumbnail)
    Q_PROPERTY(bool hasFrame READ hasFrame NOTIFY hasFrameChanged)
public:
    explicit ThemeVideo(QQuickItem *parent = nullptr);
    void paint(QPainter *p) override;
    bool running() const { return m_running; }
    void setRunning(bool r);
    qreal radius() const { return m_radius; }
    QString source() const { return m_clip ? QStringLiteral("clip") : QStringLiteral("theme"); }
    void setSource(const QString &s);
    void setRadius(qreal r);
    bool hasFrame() const { return !m_img.isNull(); }
    // forget the last picture (a new video starts)
    Q_INVOKABLE void clearFrame();
signals:
    void runningChanged();
    void radiusChanged();
    void sourceChanged();
    void hasFrameChanged();
protected:
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
private:
    void tick();
    void updateTimer();
    QTimer m_timer;
    QImage m_img;
    bool m_running = false;
    bool m_force = true;   // ask for a picture even if mpv drew nothing new (after a resize or a restart)
    qreal m_radius = 0;
    bool m_clip = false;
};
