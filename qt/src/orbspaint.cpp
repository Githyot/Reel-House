#include "orbspaint.h"

#include <QPainter>
#include <QPolygonF>
#include <QRadialGradient>
#include <algorithm>
#include <cmath>

OrbsStyle OrbsStyle::forDark(bool dark) {
    if (dark) return {QColor(245, 252, 255), QColor(110, 190, 255), QColor(60, 140, 255), QColor(20, 50, 175)};
    return {QColor(40, 90, 160), QColor(47, 95, 154), QColor(47, 95, 170), QColor(80, 120, 200)};
}

// The glow of one orb: a white core in a soft halo, drawn once per style and scaled.
static const QImage &sprite(const OrbsStyle &st) {
    static QImage img;
    static QRgb key = 0;
    if (img.isNull() || key != st.halo.rgb()) {
        key = st.halo.rgb();
        img = QImage(128, 128, QImage::Format_ARGB32_Premultiplied);
        img.fill(Qt::transparent);
        QPainter p(&img);
        p.setRenderHint(QPainter::Antialiasing);
        QRadialGradient g(QPointF(64, 64), 64);
        auto c = [](QColor col, double a) { col.setAlphaF(float(a)); return col; };
        g.setColorAt(0.0, c(st.core, 1.0));
        g.setColorAt(0.13, c(st.core, 1.0));
        g.setColorAt(0.22, c(st.halo, 0.85));
        g.setColorAt(0.42, c(st.halo, 0.36));
        g.setColorAt(0.70, c(st.halo, 0.10));
        g.setColorAt(1.0, c(st.halo, 0.0));
        p.setPen(Qt::NoPen);
        p.setBrush(g);
        p.drawEllipse(QRectF(0, 0, 128, 128));
    }
    return img;
}

static QColor mix(const QColor &a, const QColor &b, double k, double alpha) {
    QColor c;
    c.setRgbF(float(a.redF() + (b.redF() - a.redF()) * k), float(a.greenF() + (b.greenF() - a.greenF()) * k),
              float(a.blueF() + (b.blueF() - a.blueF()) * k), float(std::clamp(alpha, 0.0, 1.0)));
    return c;
}

void paintOrbs(QImage &img, const float *pts, int orbs, int trail, float radius, const OrbsStyle &st) {
    img.fill(Qt::transparent);
    if (!pts || orbs <= 0 || trail <= 0) return;
    QPainter p(&img);
    p.setRenderHint(QPainter::Antialiasing);
    p.setRenderHint(QPainter::SmoothPixmapTransform);
    p.setCompositionMode(QPainter::CompositionMode_Plus);
    const QPointF c(img.width() / 2.0, img.height() / 2.0);
    auto at = [&](int o, int k) {
        const float *q = pts + (size_t(o) * trail + k) * 5;
        return QPointF(c.x() + q[0] * radius, c.y() + q[1] * radius);
    };
    auto bright = [&](int o, int k) { return double(pts[(size_t(o) * trail + k) * 5 + 3]); };

    // trails: a wide faint glow under a thin bright line, light blue at the orb, deep blue at the end. Drawn in
    // short polylines of one colour each (so the joints are not added twice).
    const int chunk = 6;
    const double glowW = std::max(2.0, radius * 0.075);
    const double lineW = std::max(1.0, radius * 0.017);
    for (int pass = 0; pass < 2; ++pass) {
        for (int o = 0; o < orbs; ++o) {
            for (int k0 = 0; k0 < trail - 1; k0 += chunk) {
                const int k1 = std::min(trail - 1, k0 + chunk);
                QPolygonF line;
                line.reserve(k1 - k0 + 1);
                for (int k = k0; k <= k1; ++k) line << at(o, k);
                const double b = (bright(o, k0) + bright(o, k1)) / 2;
                const double along = double(k0) / trail;
                QPen pen(mix(st.trailHead, st.trailTail, along, pass == 0 ? b * 0.16 : b * 0.4));
                pen.setWidthF(pass == 0 ? glowW : lineW);
                pen.setCapStyle(Qt::FlatCap);
                pen.setJoinStyle(Qt::RoundJoin);
                p.setPen(pen);
                p.drawPolyline(line);
            }
        }
    }

    // the orbs: the glow drawn a few times between now and a moment ago, so a fast orb is a little stretched
    const QImage &spr = sprite(st);
    const int blur = 4;
    for (int o = 0; o < orbs; ++o) {
        const float *head = pts + size_t(o) * trail * 5;
        const double d = radius * 0.6 * head[2];
        const double op = 0.55 + 0.45 * head[4];
        const QPointF a = at(o, 0);
        const QPointF b = trail > 1 ? at(o, 1) : a;
        for (int i = 0; i < blur; ++i) {
            const double t = double(i) / blur;
            const QPointF q = a + (b - a) * t;
            p.setOpacity(op * (i == 0 ? 0.85 : 0.6 / (blur - 1)));
            p.drawImage(QRectF(q.x() - d / 2, q.y() - d / 2, d, d), spr);
        }
    }
}
