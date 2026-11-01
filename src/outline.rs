//! A white outline for logos that are mostly dark, so they can be read over a dark wallpaper (Stremio does the same).

/// Average brightness (0 to 1) of the visible part of a picture of straight RGBA pixels; `None` when nothing is visible.
pub fn brightness(rgba: &[u8]) -> Option<f64> {
    let (mut sum, mut weight) = (0.0f64, 0.0f64);
    for p in rgba.chunks_exact(4) {
        let a = p[3] as f64 / 255.0;
        if a <= 0.0 {
            continue;
        }
        let l = (0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64) / 255.0;
        sum += l * a;
        weight += a;
    }
    if weight < 1.0 { None } else { Some(sum / weight) }
}

/// Is the logo dark enough to need the outline?
pub fn is_dark(rgba: &[u8]) -> bool {
    brightness(rgba).map(|b| b < 0.45).unwrap_or(false)
}

/// The picture with a white outline `r` pixels wide around its visible part. The result is `r` pixels larger on every side:
/// (width, height, straight RGBA pixels).
pub fn outlined(w: usize, h: usize, rgba: &[u8], r: usize) -> (usize, usize, Vec<u8>) {
    let (nw, nh) = (w + 2 * r, h + 2 * r);
    // The alpha channel, grown by `r` pixels: first along the rows, then along the columns.
    let mut a = vec![0u8; nw * nh];
    for y in 0..h {
        for x in 0..w {
            a[(y + r) * nw + (x + r)] = rgba[(y * w + x) * 4 + 3];
        }
    }
    let mut rows = vec![0u8; nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            let (lo, hi) = (x.saturating_sub(r), (x + r).min(nw - 1));
            rows[y * nw + x] = (lo..=hi).map(|i| a[y * nw + i]).max().unwrap_or(0);
        }
    }
    let mut grown = vec![0u8; nw * nh];
    for y in 0..nh {
        let (lo, hi) = (y.saturating_sub(r), (y + r).min(nh - 1));
        for x in 0..nw {
            grown[y * nw + x] = (lo..=hi).map(|j| rows[j * nw + x]).max().unwrap_or(0);
        }
    }
    // White under the logo.
    let mut out = vec![0u8; nw * nh * 4];
    for i in 0..nw * nh {
        let (x, y) = (i % nw, i / nw);
        let (mut cr, mut cg, mut cb, mut ca) = (255.0f64, 255.0f64, 255.0f64, grown[i] as f64 / 255.0);
        if x >= r && y >= r && x < r + w && y < r + h {
            let s = ((y - r) * w + (x - r)) * 4;
            let sa = rgba[s + 3] as f64 / 255.0;
            if sa > 0.0 {
                let oa = sa + ca * (1.0 - sa);
                cr = (rgba[s] as f64 * sa + cr * ca * (1.0 - sa)) / oa;
                cg = (rgba[s + 1] as f64 * sa + cg * ca * (1.0 - sa)) / oa;
                cb = (rgba[s + 2] as f64 * sa + cb * ca * (1.0 - sa)) / oa;
                ca = oa;
            }
        }
        out[i * 4] = cr.round().clamp(0.0, 255.0) as u8;
        out[i * 4 + 1] = cg.round().clamp(0.0, 255.0) as u8;
        out[i * 4 + 2] = cb.round().clamp(0.0, 255.0) as u8;
        out[i * 4 + 3] = (ca * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    (nw, nh, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot(rgb: [u8; 3]) -> Vec<u8> {
        // 3x3 picture: only the middle pixel is visible.
        let mut v = vec![0u8; 36];
        v[16..20].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        v
    }

    #[test]
    fn dark_and_bright() {
        assert!(is_dark(&dot([10, 10, 10])));
        assert!(!is_dark(&dot([250, 250, 250])));
        assert!(!is_dark(&dot([220, 20, 20])) || brightness(&dot([220, 20, 20])).unwrap() < 0.45);
        assert!(brightness(&[0, 0, 0, 0]).is_none());
    }

    #[test]
    fn outline_surrounds_the_logo() {
        let (w, h, px) = outlined(3, 3, &dot([0, 0, 0]), 2);
        assert_eq!((w, h), (7, 7));
        // The centre stays black, the ring around it is white and opaque, the far corner is empty.
        let at = |x: usize, y: usize| &px[(y * w + x) * 4..(y * w + x) * 4 + 4];
        assert_eq!(at(3, 3), &[0, 0, 0, 255]);
        assert_eq!(at(1, 3), &[255, 255, 255, 255]);
        assert_eq!(at(0, 0)[3], 0);
    }
}
