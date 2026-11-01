#pragma once
#include <QQuickImageProvider>
#include <QImage>
#include <QString>
#include <QRectF>

// image://edited/<encoded page address>@<revision>: the page with the edits of the page editor applied (brush, eraser,
// bucket, crop). The edits are a JSON array, the same text kept in the chapter's translation file (.md), so a page reads
// the same on every computer. Operations, coordinates normalised to the original page (0..1):
//   {"t":"stroke","tool":"brush"|"eraser","c":"#rrggbb","a":0..1,"s":<width / page width>,"p":[x0,y0,x1,y1,...]}
//   {"t":"fill","x":..,"y":..,"c":"#rrggbb","a":0..1,"tol":0..255}
//   {"t":"crop","x":..,"y":..,"w":..,"h":..}
// The eraser removes edits (brush and bucket paint), showing the page as it was under them.
class EditedPageProvider : public QQuickImageProvider {
public:
    EditedPageProvider() : QQuickImageProvider(QQuickImageProvider::Image) {}
    QImage requestImage(const QString &id, QSize *size, const QSize &requestedSize) override;

    static void setOps(const QString &url, const QString &opsJson);
    static QString ops(const QString &url);
    // The finished page (cached for the last few pages edited).
    static QImage render(const QString &url);
    // The colour at (x, y) of the finished page, normalised to the original page.
    static QString pickColor(const QString &url, double x, double y);
    // The crop of the page (0,0,1,1 when there is none).
    static QRectF cropOf(const QString &opsJson);
    // The rectangle `r` (0..1 of the original page) of the page with its edits; may wait for the download, so it is
    // called off the interface thread. A null picture when the page cannot be loaded.
    static QImage cutOut(const QString &url, const QRectF &r);
};
