#pragma once
#include <QQuickFramebufferObject>

// The video: the core's mpv draws into the framebuffer of this item (OpenGL). Controls are ordinary QML items on top of it.
class VideoItem : public QQuickFramebufferObject {
    Q_OBJECT
public:
    explicit VideoItem(QQuickItem *parent = nullptr);
    Renderer *createRenderer() const override;
public slots:
    void frameReady();
};
