//! Lining a downloaded subtitle up with the speech of the video, by sound alone (no speech model).
//!
//! The idea of syncing a subtitle by itself comes from NuvioMobile's AutoSync (GPL-3.0), which compares the timing of
//! an add-on subtitle with a subtitle track inside the video. This is written anew and works differently: it needs no
//! subtitle inside the file. A stretch of the video's own audio (8 kHz mono, see `Mpv::encode_pcm`) is cut into 10 ms
//! frames and each frame is marked "voice" or "quiet" by its loudness in the voice band, relative to the quiet parts
//! of that stretch. The subtitle lines of that stretch are then slid along it: the best delay is the one where the
//! lines cover the most voice and the least quiet. How far that best place stands above all the others (and above
//! the best place at least half a second away) says how sure the match is. Only a constant delay is found.

/// Samples per second of the decoded sound.
pub const RATE: usize = 8000;
/// Samples per frame: 10 ms.
const HOP: usize = 80;
/// Frames per second.
const FPS: f64 = (RATE / HOP) as f64;

/// The delay found for a subtitle.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    /// mpv's `sub-delay`: seconds the subtitle must be shown later (negative: earlier).
    pub delay: f64,
    /// How many standard deviations the best delay stands above the average one.
    pub z: f64,
    /// Best score divided by the best score at least 0.5 s away (1 = no clear winner).
    pub ratio: f64,
    /// Subtitle lines that took part.
    pub lines: usize,
}

impl Match {
    /// "good", "probable" or "unsure".
    pub fn quality(&self) -> &'static str {
        if self.lines < 6 {
            "unsure"
        } else if self.z >= 4.0 && self.ratio >= 1.3 {
            "good"
        } else if self.z >= 3.0 && self.ratio >= 1.1 {
            "probable"
        } else {
            "unsure"
        }
    }
}

/// One flag per 10 ms frame: true where someone seems to be talking.
pub fn voice_frames(pcm: &[f32]) -> Vec<bool> {
    if pcm.len() < HOP {
        return vec![];
    }
    // a simple high-pass (pre-emphasis) keeps the voice band and drops most of the bass of music and noise
    let mut prev = 0.0f32;
    let db: Vec<f32> = pcm
        .chunks(HOP)
        .map(|c| {
            let mut e = 0.0f64;
            for &x in c {
                let y = x - 0.95 * prev;
                prev = x;
                e += (y as f64) * (y as f64);
            }
            (10.0 * ((e / c.len() as f64) + 1e-12).log10()) as f32
        })
        .collect();
    let mut sorted = db.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let floor = sorted[sorted.len() / 5];
    let top = sorted[sorted.len() * 95 / 100];
    // between the quiet floor and the loud top; never below a level that is silence anyway
    let thr = (floor + (top - floor) * 0.35).max(floor + 6.0).max(-60.0);
    let mut v: Vec<bool> = db.iter().map(|&d| d > thr).collect();
    // join gaps under 150 ms (between words), then drop bursts under 80 ms (a click, a hit)
    fill_runs(&mut v, false, 15);
    fill_runs(&mut v, true, 8);
    v
}

/// Flips every run of `value` shorter than `min` frames that sits between two runs of the other value.
fn fill_runs(v: &mut [bool], value: bool, min: usize) {
    let mut i = 0;
    while i < v.len() {
        if v[i] != value {
            i += 1;
            continue;
        }
        let start = i;
        while i < v.len() && v[i] == value {
            i += 1;
        }
        if i - start < min && start > 0 && i < v.len() {
            for x in &mut v[start..i] {
                *x = !value;
            }
        }
    }
}

/// The best `sub-delay` for `cues` (start and end, in seconds of the video), given the voice flags of the stretch of
/// audio that starts at `voice_start` seconds. Delays from -`max_shift` to +`max_shift` seconds are tried; only lines
/// that stay inside the stretch for every delay count. None when too few lines or too little voice.
pub fn find(voice: &[bool], voice_start: f64, cues: &[(f64, f64)], max_shift: f64) -> Option<Match> {
    if voice.is_empty() {
        return None;
    }
    let voice_end = voice_start + voice.len() as f64 / FPS;
    let lines: Vec<(usize, usize)> = cues
        .iter()
        .filter(|(a, b)| b > a && *b - *a < 15.0 && *a - max_shift >= voice_start && *b + max_shift <= voice_end)
        .map(|(a, b)| (((a - voice_start) * FPS).round() as usize, ((b - voice_start) * FPS).round() as usize))
        .collect();
    if lines.len() < 3 || voice.iter().filter(|x| **x).count() < (2.0 * FPS) as usize {
        return None;
    }
    let mut sum = vec![0i64; voice.len() + 1];
    for (i, &x) in voice.iter().enumerate() {
        sum[i + 1] = sum[i] + x as i64;
    }
    let shift_max = (max_shift * FPS).round() as i64;
    let mut scores: Vec<(i64, f64)> = Vec::with_capacity((2 * shift_max + 1) as usize);
    for k in -shift_max..=shift_max {
        let mut sc = 0i64;
        for &(a, b) in &lines {
            let (a, b) = ((a as i64 + k).clamp(0, voice.len() as i64) as usize, (b as i64 + k).clamp(0, voice.len() as i64) as usize);
            let talk = sum[b] - sum[a];
            // voice under a line counts for it, quiet under a line against it
            sc += 2 * talk - (b - a) as i64;
        }
        scores.push((k, sc as f64));
    }
    let n = scores.len() as f64;
    let mean = scores.iter().map(|s| s.1).sum::<f64>() / n;
    let sd = (scores.iter().map(|s| (s.1 - mean).powi(2)).sum::<f64>() / n).sqrt().max(1e-9);
    let &(best_k, best) = scores.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    let away = (0.5 * FPS) as i64;
    let second = scores.iter().filter(|s| (s.0 - best_k).abs() >= away).map(|s| s.1).fold(f64::MIN, f64::max);
    // scores can be negative; compare their rise over the average
    let ratio = if second > mean { (best - mean) / (second - mean) } else { 10.0 };
    Some(Match { delay: best_k as f64 / FPS, z: (best - mean) / sd, ratio, lines: lines.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cues spread over a stretch, of uneven length and spacing.
    fn cues() -> Vec<(f64, f64)> {
        let mut c = Vec::new();
        let mut t = 40.0;
        let mut i = 0;
        while t < 140.0 {
            let len = 1.2 + (i % 4) as f64 * 0.6;
            c.push((t, t + len));
            t += len + 0.7 + (i % 5) as f64 * 0.9;
            i += 1;
        }
        c
    }

    /// Sound for a stretch starting at `start`: a voice-like buzz where `talk` says, a low hum elsewhere.
    fn sound(start: f64, secs: f64, talk: &[(f64, f64)]) -> Vec<f32> {
        let n = (secs * RATE as f64) as usize;
        (0..n)
            .map(|i| {
                let t = start + i as f64 / RATE as f64;
                let on = talk.iter().any(|(a, b)| t >= *a && t < *b);
                let hum = 0.02 * (2.0 * std::f64::consts::PI * 60.0 * t).sin();
                let voice = if on { 0.3 * (2.0 * std::f64::consts::PI * 900.0 * t).sin() * (1.0 + 0.5 * (2.0 * std::f64::consts::PI * 5.0 * t).sin()) } else { 0.0 };
                (hum + voice) as f32
            })
            .collect()
    }

    #[test]
    fn finds_a_late_subtitle() {
        let c = cues();
        // the speech comes 1.7 s after the subtitle says
        let talk: Vec<(f64, f64)> = c.iter().map(|(a, b)| (a + 1.7, b + 1.7)).collect();
        let pcm = sound(10.0, 160.0, &talk);
        let v = voice_frames(&pcm);
        let m = find(&v, 10.0, &c, 25.0).unwrap();
        assert!((m.delay - 1.7).abs() <= 0.05, "{:?}", m);
        assert_eq!(m.quality(), "good", "{:?}", m);
    }

    #[test]
    fn finds_an_early_subtitle() {
        let c = cues();
        let talk: Vec<(f64, f64)> = c.iter().map(|(a, b)| (a - 3.25, b - 3.25)).collect();
        let pcm = sound(10.0, 160.0, &talk);
        let m = find(&voice_frames(&pcm), 10.0, &c, 25.0).unwrap();
        assert!((m.delay + 3.25).abs() <= 0.05, "{:?}", m);
        assert_ne!(m.quality(), "unsure");
    }

    #[test]
    fn no_voice_is_no_match() {
        let c = cues();
        let pcm = sound(10.0, 160.0, &[]);
        let r = find(&voice_frames(&pcm), 10.0, &c, 25.0);
        assert!(r.map(|m| m.quality() == "unsure").unwrap_or(true));
        assert!(find(&[], 0.0, &c, 25.0).is_none());
    }

    #[test]
    fn short_gaps_are_joined() {
        let mut v = vec![true, true, true, false, false, true, true, true];
        fill_runs(&mut v, false, 3);
        assert!(v.iter().all(|x| *x));
    }
}
