#pragma once
#include <QColor>
#include <QImage>

// The look of the orbs of time (OrbsItem). Dark windows: white cores, cyan-blue halos and blue trails added on
// near-black, like the PS2. Light windows: the same in deep blue so the orbs still show.
struct OrbsStyle {
    QColor core;
    QColor halo;
    QColor trailHead;
    QColor trailTail;
    static OrbsStyle forDark(bool dark);
};

// Paints `orbs` orbs, each with `trail` points (5 floats per point: x, y in -1..1, size, brightness, depth; the orb
// first, then its trail going back in time, as reelhouse::orbs::frame gives them) into `img` (premultiplied ARGB,
// cleared first). The sphere has `radius` pixels around the centre of the image. Everything is added (Plus), so where
// orbs meet they merge into one brighter ball and crossing trails glow.
void paintOrbs(QImage &img, const float *pts, int orbs, int trail, float radius, const OrbsStyle &st);
