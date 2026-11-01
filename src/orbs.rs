//! "Orbs of time": seven glowing orbs flying round a sphere like the PS2 menu background (qt/src/orbsitem.cpp draws
//! them through `rh_orbs` in qtbridge). Studied from a capture of the console: the orbs run fast along great circles
//! of a sphere seen nearly face on, each on a slightly tilted circle of its own (so the trails draw a cage of rings),
//! the whole ring slowly rocks and turns, and every few seconds the orbs gather: all seven into one bright ball, a
//! "string of pearls", clusters of 4+3, 3+2+2, 2+2+2+1, then spread evenly again. The formation follows the second of
//! the clock (one formation per 5 seconds, the ball at the top of the minute). Each orb comes with a long trail
//! sampled from its own past positions, so the trail is exact and smooth at any frame rate.
//! Also the short "blip" played when the focus moves with a controller (made here, no recording).

use std::f64::consts::PI;

pub const ORBS: usize = 7;
/// Points per orb: the orb itself, then its trail going back in time.
pub const TRAIL: usize = 72;
/// Seconds of trail behind each orb.
const TRAIL_SECS: f64 = 1.35;
/// Seconds the orbs take to move from one formation to the next.
const BLEND: f64 = 1.2;
/// Radians per second along the orbit.
const SPEED: f64 = 2.1;
/// Camera distance in sphere radii.
const CAMERA: f64 = 4.0;
/// Largest projected distance from the centre (sqrt(1 - z^2) * CAMERA / (CAMERA - z), at most ~1.033 for CAMERA 4).
const FIT: f64 = 1.04;

/// How the seven orbs are arranged along their orbit.
#[derive(Clone, Debug, PartialEq)]
pub enum Formation {
    /// All seven in one ball.
    Ball,
    /// Close together in a row along the orbit.
    Chain,
    /// Spread evenly round the ring.
    Spread,
    /// Clusters of these sizes, spread round the ring.
    Groups(Vec<usize>),
}

/// The formation for a second of the minute: one per 5 seconds, the ball at second 0.
pub fn formation(sec: u32) -> Formation {
    match (sec % 60) / 5 {
        0 => Formation::Ball,
        1 | 6 => Formation::Chain,
        3 | 10 => Formation::Groups(vec![4, 3]),
        5 => Formation::Groups(vec![2, 2, 2, 1]),
        8 => Formation::Groups(vec![3, 2, 2]),
        _ => Formation::Spread,
    }
}

/// The group sizes of a formation (the ball is one group of 7, a chain one group, spread seven groups of 1).
pub fn groups(sec: u32) -> Vec<usize> {
    match formation(sec) {
        Formation::Ball | Formation::Chain => vec![ORBS],
        Formation::Spread => vec![1; ORBS],
        Formation::Groups(g) => g,
    }
}

/// Angle of each orb along its orbit, and how far the orbits fan out (0 = all on one circle, 1 = fully apart).
fn layout(f: &Formation) -> ([f64; ORBS], f64) {
    let mut off = [0.0; ORBS];
    match f {
        Formation::Ball => {
            for (o, a) in off.iter_mut().enumerate() {
                *a = (o as f64 - 3.0) * 0.012;
            }
            (off, 0.0)
        }
        Formation::Chain => {
            for (o, a) in off.iter_mut().enumerate() {
                *a = (o as f64 - 3.0) * 0.25;
            }
            (off, 0.12)
        }
        Formation::Spread => {
            for (o, a) in off.iter_mut().enumerate() {
                *a = o as f64 * 2.0 * PI / ORBS as f64;
            }
            (off, 1.0)
        }
        Formation::Groups(g) => {
            let mut o = 0;
            for (ci, &size) in g.iter().enumerate() {
                let centre = ci as f64 * 2.0 * PI / g.len() as f64;
                for i in 0..size {
                    off[o] = centre + (i as f64 - (size as f64 - 1.0) / 2.0) * 0.2;
                    o += 1;
                }
            }
            (off, 0.55)
        }
    }
}

fn smooth(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The shortest signed turn from angle a to angle b.
fn turn(a: f64, b: f64) -> f64 {
    (b - a + PI).rem_euclid(2.0 * PI) - PI
}

/// Rotates v about the unit axis k by angle t (Rodrigues).
fn rotate(v: [f64; 3], k: [f64; 3], t: f64) -> [f64; 3] {
    let (s, c) = t.sin_cos();
    let d = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
    let x = [k[1] * v[2] - k[2] * v[1], k[2] * v[0] - k[0] * v[2], k[0] * v[1] - k[1] * v[0]];
    [0, 1, 2].map(|i| v[i] * c + x[i] * s + k[i] * d * (1.0 - c))
}

/// 3D positions (on the unit sphere) of the orbs at wall time `now` (seconds, local), moving smoothly from the last
/// formation to the current one.
pub fn blended(now: f64) -> [[f64; 3]; ORBS] {
    let secs = now.rem_euclid(60.0);
    let sec = secs.floor() as u32;
    let slot = sec / 5 * 5;
    let (a, fa) = layout(&formation((slot + 55) % 60));
    let (b, fb) = layout(&formation(slot));
    let k = smooth((secs - slot as f64) / BLEND);
    let fan = fa + (fb - fa) * k;
    // the ring rocks and turns slowly, mostly facing the camera
    let rx = 0.55 * (now * 0.23).sin() + 0.2 * (now * 0.07 + 2.0).sin();
    let ry = 0.6 * (now * 0.17 + 1.0).sin();
    let mut out = [[0.0; 3]; ORBS];
    for o in 0..ORBS {
        let theta = now * SPEED + a[o] + turn(a[o], b[o]) * k;
        let mut p = [theta.cos(), theta.sin(), 0.0];
        // each orbit tilted a little about its own axis in the ring's plane
        let axis = o as f64 * 0.9;
        p = rotate(p, [axis.cos(), axis.sin(), 0.0], fan * 0.45 * (o as f64 - 3.0) / 3.0);
        p = rotate(p, [1.0, 0.0, 0.0], rx);
        p = rotate(p, [0.0, 1.0, 0.0], ry);
        out[o] = p;
    }
    out
}

/// One point on screen: x and y from -1 to 1 (the sphere fits in the unit circle), `size` (about 0.75 far to 1.25
/// near), `bright` (0-1, darker far away and along the trail) and `depth` (0 far, 1 near).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub bright: f32,
    pub depth: f32,
}

pub fn project(p: [f64; 3]) -> (f64, f64, f64) {
    let persp = CAMERA / (CAMERA - p[2]);
    (p[0] * persp / FIT, -p[1] * persp / FIT, (p[2] + 1.0) / 2.0)
}

/// All points to draw at `now`: TRAIL points per orb (the orb first, then its trail), ORBS * TRAIL in all.
pub fn frame(now: f64) -> Vec<Point> {
    let mut out = Vec::with_capacity(ORBS * TRAIL);
    let step = TRAIL_SECS / (TRAIL - 1) as f64;
    let steps: Vec<[[f64; 3]; ORBS]> = (0..TRAIL).map(|t| blended(now - t as f64 * step)).collect();
    for o in 0..ORBS {
        for (t, pos) in steps.iter().enumerate() {
            let (x, y, depth) = project(pos[o]);
            let fade = 1.0 - t as f64 / TRAIL as f64;
            out.push(Point {
                x: x as f32,
                y: y as f32,
                size: (0.75 + depth * 0.5) as f32,
                bright: ((0.5 + depth * 0.5) * fade.powf(1.6)) as f32,
                depth: depth as f32,
            });
        }
    }
    out
}

/// A short soft "blip" (16-bit mono WAV) for a focus move: a sine that glides up with a little FM shimmer and fades fast.
pub fn nav_blip_wav() -> Vec<u8> {
    wav(&blip(1250.0, 1950.0, 0.09, 45.0, 0.35))
}

/// The "select" sound: two quick rising notes, a little longer and fuller than the move blip.
pub fn select_blip_wav() -> Vec<u8> {
    let mut s = blip(880.0, 990.0, 0.06, 30.0, 0.4);
    s.extend(blip(1320.0, 1480.0, 0.14, 22.0, 0.4));
    wav(&s)
}

const RATE: u32 = 44100;

fn blip(f0: f64, f1: f64, dur: f64, decay: f64, gain: f64) -> Vec<i16> {
    let n = (RATE as f64 * dur) as usize;
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f64;
    for i in 0..n {
        let t = i as f64 / RATE as f64;
        let f = f0 + (f1 - f0) * (t / dur);
        let fm = (2.0 * PI * f * 2.0 * t).sin() * 0.35 * (-t * 60.0).exp();
        phase += 2.0 * PI * f / RATE as f64;
        let env = (t / 0.003).min(1.0) * (-t * decay).exp();
        out.push(((phase + fm).sin() * env * gain * i16::MAX as f64) as i16);
    }
    out
}

fn wav(samples: &[i16]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&(RATE * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groupings() {
        assert_eq!(formation(0), Formation::Ball);
        assert_eq!(groups(15), vec![4, 3]);
        assert_eq!(groups(25), vec![2, 2, 2, 1]);
        assert_eq!(groups(40), vec![3, 2, 2]);
        assert_eq!(groups(12), vec![1; 7]);
        for s in 0..60 {
            assert_eq!(groups(s).iter().sum::<usize>(), ORBS);
            assert_eq!(layout(&formation(s)).0.len(), ORBS);
        }
    }

    #[test]
    fn positions_on_sphere() {
        for k in 0..300 {
            for o in blended(1_700_000_000.0 + k as f64 * 0.21) {
                let r = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
                assert!((r - 1.0).abs() < 1e-9);
            }
        }
        // in the ball (late in second 0..4 of the minute) the orbs are close together
        let p = blended(60.0 * 1000.0 + 3.0);
        for o in 1..ORBS {
            let d: f64 = (0..3).map(|i| (p[o][i] - p[0][i]).powi(2)).sum::<f64>().sqrt();
            assert!(d < 0.1, "{d}");
        }
    }

    #[test]
    fn frame_fits_and_fades() {
        for k in 0..200 {
            let f = frame(1_700_000_000.0 + k as f64 * 0.37);
            assert_eq!(f.len(), ORBS * TRAIL);
            for p in &f {
                assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0, "{p:?}");
                assert!((0.0..=1.0).contains(&p.depth) && (0.0..=1.0).contains(&p.bright));
            }
            for o in 0..ORBS {
                assert!(f[o * TRAIL].bright > f[o * TRAIL + TRAIL - 1].bright);
            }
        }
        let near = project([0.0, 0.0, 1.0]);
        let far = project([0.0, 0.0, -1.0]);
        assert!(near.2 > far.2);
    }

    #[test]
    fn blend_is_continuous() {
        for t in [120.0, 125.0, 1_700_000_005.0, 1_700_000_060.0] {
            let a = blended(t - 0.001);
            let b = blended(t + 0.001);
            for o in 0..ORBS {
                for d in 0..3 {
                    assert!((a[o][d] - b[o][d]).abs() < 0.02, "{t} {o}");
                }
            }
        }
    }

    #[test]
    fn blip_is_wav() {
        let s = select_blip_wav();
        assert_eq!(u32::from_le_bytes([s[40], s[41], s[42], s[43]]) as usize, s.len() - 44);
        let w = nav_blip_wav();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]) as usize, w.len() - 44);
        assert!(w.len() > 44 + 2000);
    }
}
