//! The main colours of a picture, for the moving background behind a title.

pub type Rgb = (u8, u8, u8);

/// Colours used when a picture has nothing to offer (grey, black or white pictures).
pub const FALLBACK: [Rgb; 3] = [(96, 112, 200), (160, 96, 200), (72, 168, 176)];

fn hsv(c: Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

fn from_hsv(h: f64, s: f64, v: f64) -> Rgb {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (((r + m) * 255.0).round() as u8, ((g + m) * 255.0).round() as u8, ((b + m) * 255.0).round() as u8)
}

/// Up to `n` distinct vivid colours, the most present first. Dull pixels (grey, near black or white) are ignored.
pub fn dominant(pixels: &[Rgb], n: usize) -> Vec<Rgb> {
    const BINS: usize = 12;
    let mut weight = [0.0f64; BINS];
    let mut sum = [(0.0f64, 0.0f64); BINS]; // hue vector (cos, sin) to average hues across the 0/360 seam
    let mut sat = [0.0f64; BINS];
    let mut val = [0.0f64; BINS];
    for p in pixels {
        let (h, s, v) = hsv(*p);
        if v < 0.16 || s < 0.14 || (v > 0.96 && s < 0.25) {
            continue;
        }
        let w = s * v;
        let bin = ((h / 360.0 * BINS as f64) as usize).min(BINS - 1);
        weight[bin] += w;
        let rad = h.to_radians();
        sum[bin].0 += rad.cos() * w;
        sum[bin].1 += rad.sin() * w;
        sat[bin] += s * w;
        val[bin] += v * w;
    }
    let mut order: Vec<usize> = (0..BINS).filter(|b| weight[*b] > 0.0).collect();
    order.sort_by(|a, b| weight[*b].partial_cmp(&weight[*a]).unwrap_or(std::cmp::Ordering::Equal));
    let mut chosen: Vec<usize> = Vec::new();
    for b in order {
        // Neighbouring hues look alike: skip them unless nothing else is left.
        let near = |x: usize| chosen.iter().any(|c| (*c as i32 - x as i32).rem_euclid(BINS as i32).min((x as i32 - *c as i32).rem_euclid(BINS as i32)) <= 1);
        if !near(b) {
            chosen.push(b);
        }
        if chosen.len() == n {
            break;
        }
    }
    chosen
        .into_iter()
        .map(|b| {
            let h = sum[b].1.atan2(sum[b].0).to_degrees().rem_euclid(360.0);
            // Livelier than the picture: the colours are only a mood behind the text.
            from_hsv(h, (sat[b] / weight[b]).clamp(0.45, 0.95), (val[b] / weight[b]).clamp(0.55, 0.95))
        })
        .collect()
}

/// Four colours for the moving background: the dominant ones, completed from the fallback ones.
pub fn mood(pixels: &[Rgb]) -> [Rgb; 4] {
    let mut v = dominant(pixels, 4);
    let mut i = 0;
    while v.len() < 4 {
        let base = if v.is_empty() { FALLBACK[i % 3] } else { v[i % v.len()] };
        // A shifted copy of what we have, or the fallback.
        let (h, s, val) = hsv(base);
        v.push(if v.is_empty() { base } else { from_hsv((h + 35.0 * (i as f64 + 1.0)) % 360.0, s.clamp(0.45, 0.9), val.clamp(0.55, 0.95)) });
        i += 1;
    }
    [v[0], v[1], v[2], v[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_red_and_blue() {
        let mut px = vec![(220, 30, 30); 60];
        px.extend(vec![(30, 40, 220); 40]);
        px.extend(vec![(128, 128, 128); 200]); // grey is ignored
        let d = dominant(&px, 3);
        assert_eq!(d.len(), 2);
        assert!(d[0].0 > d[0].2, "red first: {:?}", d);
        assert!(d[1].2 > d[1].0, "blue second: {:?}", d);
    }

    #[test]
    fn grey_picture_uses_the_fallback() {
        let px = vec![(120, 120, 120); 100];
        assert!(dominant(&px, 3).is_empty());
        let m = mood(&px);
        assert_eq!(m[0], FALLBACK[0]);
        assert_eq!(m.len(), 4);
    }

    #[test]
    fn hue_seam_is_averaged() {
        // Reds on both sides of 0 degrees are one colour, not two.
        let px = vec![(255, 20, 40), (255, 40, 20), (255, 30, 30), (255, 20, 45)];
        let d = dominant(&px, 3);
        assert_eq!(d.len(), 1);
        assert!(d[0].0 > 200);
    }
}
