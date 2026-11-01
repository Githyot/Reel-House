#include "videoitem.h"
#include "corebridge.h"
#include <QOpenGLFramebufferObject>
#include <QQuickWindow>
#include <QQuickOpenGLUtils>
#include <QDebug>
#include <QOpenGLContext>

extern "C" {
#include "bridge.h"
}

namespace {
// mpv asks the context that is current right now for its functions (desktop OpenGL or Qt's software OpenGL).
void *qtGlProc(const char *name) {
    QOpenGLContext *ctx = QOpenGLContext::currentContext();
    return ctx ? reinterpret_cast<void *>(ctx->getProcAddress(name)) : nullptr;
}

class VideoRenderer : public QQuickFramebufferObject::Renderer {
public:
    QOpenGLFramebufferObject *createFramebufferObject(const QSize &size) override {
        QOpenGLFramebufferObjectFormat fmt;
        fmt.setAttachment(QOpenGLFramebufferObject::NoAttachment);
        return new QOpenGLFramebufferObject(size, fmt);
    }
    void synchronize(QQuickFramebufferObject *item) override { m_window = item->window(); }
    void render() override {
        Bridge *b = CoreBridge::handle();
        if (!b) return;
        if (!m_inited) {
            m_inited = true;
            rh_set_gl_loader(&qtGlProc);
            const QOpenGLContext *ctx = QOpenGLContext::currentContext();
            qInfo() << "video init: OpenGL" << (ctx ? ctx->format().majorVersion() : 0) << "." << (ctx ? ctx->format().minorVersion() : 0)
                    << (ctx && ctx->isOpenGLES() ? "ES" : "desktop");
            int rc = rh_video_init(b);
            if (rc != 0) {
                char *e = rh_video_error(b);
                qWarning() << "video init failed:" << rc << QString::fromUtf8(e ? e : "");
                if (e) rh_free_str(e);
            } else {
                qInfo() << "video init: ok";
            }
        }
        QOpenGLFramebufferObject *fbo = framebufferObject();
        // Hand mpv a clean OpenGL state: a layer or effect drawn earlier in the frame (the shadows of the menus) can
        // leave a scissor box, a colour mask or another framebuffer bound, and mpv then drew only black.
        QQuickOpenGLUtils::resetOpenGLState();
        rh_video_render(b, int(fbo->handle()), fbo->width(), fbo->height());
        QQuickOpenGLUtils::resetOpenGLState();
    }
private:
    bool m_inited = false;
    QQuickWindow *m_window = nullptr;
};
}

VideoItem::VideoItem(QQuickItem *parent) : QQuickFramebufferObject(parent) {
    setMirrorVertically(false);
    CoreBridge::instance()->setVideoItem(this);
}

QQuickFramebufferObject::Renderer *VideoItem::createRenderer() const { return new VideoRenderer; }

void VideoItem::frameReady() { update(); }
