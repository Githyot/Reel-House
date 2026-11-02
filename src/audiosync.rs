//! Lining up an audio track loaded from outside (a dub, say Brazilian Portuguese) with the audio built into the video.
//!
//! A dub replaces the voices but keeps the music and the sound effects, so the two tracks share their *rhythm*: the
//! moments where sound suddenly rises (a door, a hit, a note). Both tracks are decoded to 8 kHz mono (`Mpv::encode_pcm`),
//! cut into 10 ms frames, and for each frame the rise of loudness in five frequency bands is measured ("onsets").
//! The onsets of a stretch of the video's track are then slid along a longer stretch of the dub; where they line up
//! best is the offset. How far that best place stands above all the others says how sure the match is.
//!
//! Sign convention: `offset` is `dub time − video time` for the same moment. mpv plays the dub `audio-delay` seconds
//! later, so the delay that fixes it is `-offset` (see `delay_for`).

/// Sample rate of the decoded sound.
pub const RATE: usize = 8000;
/// Samples between frames: 10 ms.
pub const HOP: usize = 80;
/// Samples looked at for each frame.
const WIN: usize = 256;
/// Frequency bands (FFT bins of 31.25 Hz): ~95-190, 190-400, 400-800, 800-1600, 1600-3250 Hz.
const BANDS: [(usize, usize); 5] = [(3, 6), (6, 13), (13, 26), (26, 52), (52, 104)];
const NB: usize = BANDS.len();

/// Seconds per frame.
pub const FRAME_SECS: f64 = HOP as f64 / RATE as f64;

/// Where two tracks line up.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    /// `dub time − video time`, in seconds.
    pub offset: f64,
    /// Correlation at the best place (about 0..1).
    pub score: f64,
    /// How many standard deviations the best place stands above the average place.
    pub z: f64,
    /// Best place divided by the best place at least 0.5 s away from it (1 = no clear winner).
    pub ratio: f64,
}

impl Match {
    /// "good", "probable" or "unsure".
    pub fn quality(&self) -> &'static str {
        if self.z >= 9.0 && self.ratio >= 1.25 {
            "good"
        } else if self.z >= 6.0 && self.ratio >= 1.08 {
            "probable"
        } else {
            "unsure"
        }
    }
}

/// The `audio-delay` for mpv that makes the dub play in time with the picture.
pub fn delay_for(offset: f64) -> f64 {
    -offset
}

/// 16-bit little-endian samples from a raw file, or from a WAV file (its "data" chunk).
pub fn read_pcm(bytes: &[u8], kind: &str) -> Vec<f32> {
    let data: &[u8] = if kind == "wav" { wav_data(bytes).unwrap_or(&[]) } else { bytes };
    data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect()
}

fn wav_data(b: &[u8]) -> Option<&[u8]> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let start = i + 8;
        if id == b"data" {
            // a streamed WAV may carry 0 or 0xFFFFFFFF as its length: take what is there
            let end = if len == 0 || start.saturating_add(len) > b.len() { b.len() } else { start + len };
            return Some(&b[start..end]);
        }
        i = start + len + (len & 1);
    }
    None
}

/// In-place radix-2 FFT of `re`/`im` (length a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

/// The onsets of `pcm`: for each 10 ms frame and band, how much the loudness rose since the frame before.
/// Returned band by band (`out[band][frame]`).
pub fn onsets(pcm: &[f32]) -> Vec<Vec<f32>> {
    let frames = if pcm.len() >= WIN { (pcm.len() - WIN) / HOP + 1 } else { 0 };
    let window: Vec<f32> = (0..WIN).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WIN as f32).cos()).collect();
    let mut out = vec![Vec::with_capacity(frames); NB];
    let mut prev = [0f32; NB];
    let (mut re, mut im) = (vec![0f32; WIN], vec![0f32; WIN]);
    for f in 0..frames {
        let s = &pcm[f * HOP..f * HOP + WIN];
        for i in 0..WIN {
            re[i] = s[i] * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for (b, &(lo, hi)) in BANDS.iter().enumerate() {
            let e: f32 = (lo..hi).map(|k| re[k] * re[k] + im[k] * im[k]).sum();
            let l = (e + 1e-7).ln();
            out[b].push(if f == 0 { 0.0 } else { (l - prev[b]).max(0.0) });
            prev[b] = l;
        }
    }
    out
}

/// Slides the onsets of the video's stretch `vid` along the dub's longer stretch `dub` (both from `onsets`) and returns
/// the best lag in frames (`dub[lag + i]` matches `vid[i]`), with its score, z and ratio. None when `dub` is not
/// longer than `vid` or a stretch is silent.
pub fn best_lag(vid: &[Vec<f32>], dub: &[Vec<f32>]) -> Option<(f64, f64, f64, f64)> {
    let n = vid.first()?.len();
    let m = dub.first()?.len();
    if n < 100 || m <= n {
        return None;
    }
    let lags = m - n + 1;
    let mut curve = vec![0f64; lags];
    let mut used = 0usize;
    for b in 0..NB.min(vid.len()).min(dub.len()) {
        let v = &vid[b];
        let d = &dub[b];
        // the video's stretch, centred and scaled to unit length
        let mean = v.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
        let vn: Vec<f64> = v.iter().map(|&x| x as f64 - mean).collect();
        let vnorm = vn.iter().map(|x| x * x).sum::<f64>().sqrt();
        if vnorm < 1e-9 {
            continue;
        }
        // sliding sums of the dub for its mean and spread under each lag
        let mut ps = vec![0f64; m + 1];
        let mut ps2 = vec![0f64; m + 1];
        for i in 0..m {
            ps[i + 1] = ps[i] + d[i] as f64;
            ps2[i + 1] = ps2[i] + (d[i] as f64) * (d[i] as f64);
        }
        used += 1;
        for (k, c) in curve.iter_mut().enumerate() {
            let sum = ps[k + n] - ps[k];
            let var = (ps2[k + n] - ps2[k]) - sum * sum / n as f64;
            if var <= 1e-12 {
                continue;
            }
            let dot: f64 = vn.iter().zip(&d[k..k + n]).map(|(a, &b)| a * b as f64).sum();
            *c += dot / (vnorm * var.sqrt());
        }
    }
    if used == 0 {
        return None;
    }
    for c in curve.iter_mut() {
        *c /= used as f64;
    }
    let (best, &peak) = curve.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    let mean = curve.iter().sum::<f64>() / lags as f64;
    let sd = (curve.iter().map(|c| (c - mean) * (c - mean)).sum::<f64>() / lags as f64).sqrt().max(1e-9);
    let gap = (0.5 / FRAME_SECS) as usize;
    let second = curve
        .iter()
        .enumerate()
        .filter(|(k, _)| (*k as isize - best as isize).unsigned_abs() > gap)
        .map(|(_, &c)| c)
        .fold(f64::MIN, f64::max);
    let ratio = if second > 1e-9 { peak / second } else { 99.0 };
    // between frames: the top of a parabola through the best place and its neighbours
    let mut frac = 0.0;
    if best > 0 && best + 1 < lags {
        let (a, b, c) = (curve[best - 1], peak, curve[best + 1]);
        let den = a - 2.0 * b + c;
        if den.abs() > 1e-12 {
            frac = (0.5 * (a - c) / den).clamp(-0.5, 0.5);
        }
    }
    Some((best as f64 + frac, peak, (peak - mean) / sd, ratio))
}

/// The match between the video's sound `vid` starting at `vid_start` seconds and the dub's sound `dub` starting at
/// `dub_start` seconds (samples at `RATE`).
pub fn find(vid: &[f32], vid_start: f64, dub: &[f32], dub_start: f64) -> Option<Match> {
    let (lag, score, z, ratio) = best_lag(&onsets(vid), &onsets(dub))?;
    let offset = dub_start + lag * FRAME_SECS - vid_start;
    Some(Match { offset: (offset * 1000.0).round() / 1000.0, score, z, ratio })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A noise generator that gives the same numbers every run.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % 20000) as f32 / 10000.0 - 1.0
        }
    }

    /// "Music and effects": bursts of tones and noise at random moments, `secs` long.
    fn effects(secs: f64, seed: u64) -> Vec<f32> {
        let mut r = Rng(seed);
        let n = (secs * RATE as f64) as usize;
        let mut out = vec![0f32; n];
        let mut t = 0usize;
        while t < n {
            t += 1200 + ((r.next() + 1.0) * 6000.0) as usize;
            let len = 800 + ((r.next() + 1.0) * 2000.0) as usize;
            let freq = 150.0 + (r.next() + 1.0) * 1200.0;
            let amp = 0.2 + (r.next() + 1.0) * 0.2;
            for i in 0..len {
                if t + i >= n {
                    break;
                }
                let env = (-(i as f32) / (len as f32 / 3.0)).exp();
                let tone = (2.0 * std::f32::consts::PI * freq * (i as f32) / RATE as f32).sin();
                out[t + i] += amp * env * (0.7 * tone + 0.3 * r.next());
            }
        }
        out
    }

    /// "Voices": noise switched on and off like speech, different for each language.
    fn voices(n: usize, seed: u64) -> Vec<f32> {
        let mut r = Rng(seed);
        let mut out = vec![0f32; n];
        let mut on = false;
        let mut left = 0usize;
        for x in out.iter_mut() {
            if left == 0 {
                on = !on;
                left = 1500 + ((r.next() + 1.0) * 3000.0) as usize;
            }
            left -= 1;
            *x = if on { 0.25 * r.next() } else { 0.0 };
        }
        out
    }

    fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
        a.iter().zip(b).map(|(x, y)| x + y).collect()
    }

    fn slice(v: &[f32], from_s: f64, secs: f64) -> Vec<f32> {
        let a = (from_s * RATE as f64) as usize;
        let b = ((from_s + secs) * RATE as f64) as usize;
        v[a.min(v.len())..b.min(v.len())].to_vec()
    }

    #[test]
    fn finds_the_offset_of_a_dub() {
        let total = 120.0;
        let me = effects(total, 7);
        let original = mix(&me, &voices(me.len(), 11));
        // the dub starts 2.37 s later: 2.37 s of silence, then the same effects with other voices
        let shift = (2.37 * RATE as f64) as usize;
        let mut dub_me = vec![0f32; shift];
        dub_me.extend_from_slice(&me);
        let dub = mix(&dub_me, &voices(dub_me.len(), 23));
        let vid = slice(&original, 40.0, 30.0);
        let dub_part = slice(&dub, 20.0, 70.0);
        let m = find(&vid, 40.0, &dub_part, 20.0).expect("a match");
        assert!((m.offset - 2.37).abs() < 0.02, "offset {:?}", m);
        assert_eq!(m.quality(), "good", "{:?}", m);
        assert!((delay_for(m.offset) + 2.37).abs() < 0.02);
    }

    #[test]
    fn finds_a_dub_that_starts_earlier() {
        let me = effects(100.0, 3);
        let original = mix(&me, &voices(me.len(), 5));
        // the dub lacks the first 1.5 s
        let cut = (1.5 * RATE as f64) as usize;
        let dub = mix(&me[cut..], &voices(me.len() - cut, 9));
        let vid = slice(&original, 30.0, 30.0);
        let dub_part = slice(&dub, 0.0, 90.0);
        let m = find(&vid, 30.0, &dub_part, 0.0).expect("a match");
        assert!((m.offset + 1.5).abs() < 0.02, "offset {:?}", m);
    }

    #[test]
    fn unrelated_sound_is_unsure() {
        let a = mix(&effects(60.0, 1), &voices(480000, 2));
        let b = mix(&effects(90.0, 99), &voices(720000, 98));
        let m = find(&slice(&a, 10.0, 30.0), 10.0, &b, 0.0).expect("a curve");
        assert_eq!(m.quality(), "unsure", "{:?}", m);
    }

    /// End to end with a real libmpv: `RH_SYNC_VIDEO`, `RH_SYNC_DUB` (files) and `RH_SYNC_EXPECT` (offset in seconds).
    /// Run with `cargo test --release --lib real_files -- --ignored`.
    #[test]
    #[ignore]
    fn real_files() {
        let (Ok(v), Ok(d), Ok(e)) = (std::env::var("RH_SYNC_VIDEO"), std::env::var("RH_SYNC_DUB"), std::env::var("RH_SYNC_EXPECT")) else { return };
        let libs = vec![std::path::PathBuf::from("libmpv.so.2"), std::path::PathBuf::from("libmpv.so")];
        let dir = std::env::temp_dir();
        let (vf, df) = (dir.join("rh-v.pcm").to_string_lossy().to_string(), dir.join("rh-d.pcm").to_string_lossy().to_string());
        let vk = crate::mpv::Mpv::encode_pcm(&libs, &v, &[], "1", 60.0, 105.0, &vf, 60).expect("video audio");
        let dk = crate::mpv::Mpv::encode_pcm(&libs, &d, &[], "auto", 0.0, 165.0, &df, 60).expect("dub audio");
        let vid = read_pcm(&std::fs::read(&vf).unwrap(), vk);
        let dub = read_pcm(&std::fs::read(&df).unwrap(), dk);
        assert!((vid.len() as f64 / RATE as f64 - 45.0).abs() < 0.5, "video {} samples ({})", vid.len(), vk);
        let m = find(&vid, 60.0, &dub, 0.0).expect("a match");
        eprintln!("{:?} via {}/{}", m, vk, dk);
        assert!((m.offset - e.parse::<f64>().unwrap()).abs() < 0.05, "{:?}", m);
    }

    #[test]
    fn reads_raw_and_wav() {
        let raw = [0u8, 0x40, 0x00, 0xC0];
        assert_eq!(read_pcm(&raw, "s16le"), vec![0.5, -0.5]);
        let mut wav = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0".to_vec();
        wav.extend_from_slice(&[0u8; 16]);
        wav.extend_from_slice(b"LIST\x02\0\0\0ab");
        wav.extend_from_slice(b"data\x04\0\0\0");
        wav.extend_from_slice(&raw);
        assert_eq!(read_pcm(&wav, "wav"), vec![0.5, -0.5]);
        assert!(best_lag(&onsets(&[]), &onsets(&[])).is_none());
    }
}
