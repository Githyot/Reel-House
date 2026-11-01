//! The microphone's side of the voice of Conversation: the samples the window records (any rate, any number of
//! channels) are mixed down to mono 16 kHz (what Whisper and the cloud models want), kept, and watched by a small
//! energy voice detector (VAD) that tells when the learner starts and stops talking. With `auto` the recording
//! then ends by itself after a pause, and the file sent holds only the speech (a little before, a little after).
//!
//! Tuned for learners, and used in every turn of Conversation (not only in a voice call): the pause that ends a
//! turn is 1.2 s or the user's own choice (Settings > Study > Microphone), and a short noise (a click, a cough) is not
//! speech: it must last `min_speech_ms`. The noise of the room is measured in the first 300 ms of live sound and
//! followed afterwards; speech is what stays `start_db` over it (the "cut sensitivity"). No speech in 8 s: nothing is
//! sent; a turn never passes 30 s. Pure code (no audio API): the window feeds it through `rh_voice_*` in qtbridge.
use crate::json::Json;

/// The rate of what is kept and sent.
pub const RATE: u32 = 16_000;
/// One analysis frame: 20 ms at 16 kHz.
pub const FRAME: usize = 320;
const FRAME_MS: u32 = 20;
/// Below this (dBFS) nothing counts as speech, however quiet the room. Low on purpose: a laptop or headset microphone
/// turned down gives a voice near -55 dBFS, still 10 dB and more over its room (the test is relative to the room).
const ABS_MIN_DB: f32 = -62.0;
/// At most two minutes are kept.
const MAX_SAMPLES: usize = RATE as usize * 120;
/// A peak under this (about -84 dBFS, the last bits of 16-bit sound) is a closed microphone, not a quiet one.
const DEAD_PEAK: f32 = 2.0 / 32768.0;
/// A clip whose peak reaches this (about -9 dBFS) is left as it is.
const LOUD_ENOUGH: f32 = 0.35;
/// Where a quiet clip is raised to (about -3 dBFS), and by at most how much (+40 dB).
const TARGET_PEAK: f32 = 0.7;
const MAX_GAIN: f32 = 100.0;
/// The share of the loudest samples the gain ignores, so one click (the device opening, a tap on the desk) does not
/// keep a quiet voice quiet; those few samples are clipped instead.
const GAIN_IGNORE: f64 = 0.001;

/// The pause that ends a turn when the user chose none.
pub const DEFAULT_PAUSE_MS: u32 = 1200;

/// How the detector behaves. `auto`: the recording ends by itself (`Ended` after a pause, `Nothing` when no speech
/// came); otherwise it only measures (the push-to-talk button ends the recording).
#[derive(Clone, Debug, PartialEq)]
pub struct VadCfg {
    pub auto: bool,
    /// how far above the noise of the room (dB) a frame must be to count as speech
    pub start_db: f32,
    /// speech must last this long before the turn starts (a click is not a word)
    pub min_speech_ms: u32,
    /// the pause that ends a turn
    pub silence_ms: u32,
    /// no speech at all for this long: give up (`Nothing`)
    pub max_wait_ms: u32,
    /// a turn never lasts longer (Whisper reads 30 s at a time)
    pub max_turn_ms: u32,
}

impl Default for VadCfg {
    fn default() -> VadCfg {
        VadCfg::for_lang("", 0)
    }
}

/// Languages whose learners usually need longer to find the next word.
fn slow_lang(lang: &str) -> bool {
    matches!(crate::util::lang_base(lang).as_str(), "ja" | "zh" | "ko" | "yue")
}

impl VadCfg {
    /// For a learner speaking `lang`. `pause_ms` is the user's choice of the pause that ends a turn (0: automatic).
    pub fn for_lang(lang: &str, pause_ms: u32) -> VadCfg {
        VadCfg::for_user(lang, pause_ms, 0)
    }

    /// With the user's settings: the pause that ends a turn (`pause_ms`, 0 = automatic, 1.2 s) and how far over the
    /// room's noise speech must be (`margin_db`, Settings > Study > Microphone > cut sensitivity; 0 = 10 dB).
    pub fn for_user(lang: &str, pause_ms: u32, margin_db: u32) -> VadCfg {
        let slow = slow_lang(lang);
        VadCfg {
            auto: true,
            start_db: if margin_db > 0 { (margin_db as f32).clamp(3.0, 30.0) } else { 10.0 },
            min_speech_ms: if slow { 300 } else { 250 },
            silence_ms: if pause_ms > 0 { pause_ms.clamp(500, 6000) } else { DEFAULT_PAUSE_MS },
            max_wait_ms: 8_000,
            max_turn_ms: 30_000,
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("auto", Json::Bool(self.auto)),
            ("startDb", Json::Num(self.start_db as f64)),
            ("minSpeechMs", Json::Num(self.min_speech_ms as f64)),
            ("silenceMs", Json::Num(self.silence_ms as f64)),
            ("maxWaitMs", Json::Num(self.max_wait_ms as f64)),
            ("maxTurnMs", Json::Num(self.max_turn_ms as f64)),
        ])
    }

    /// What the page sends back (`to_json`, maybe with `auto` changed); missing values keep the defaults.
    pub fn from_json(j: &Json) -> VadCfg {
        let d = VadCfg::default();
        let n = |k: &str, def: u32, lo: u32, hi: u32| j.get(k).as_f64().map(|v| (v.max(0.0) as u32).clamp(lo, hi)).unwrap_or(def);
        VadCfg {
            auto: j.get("auto").as_bool().unwrap_or(false),
            start_db: j.get("startDb").as_f64().map(|v| (v as f32).clamp(3.0, 30.0)).unwrap_or(d.start_db),
            min_speech_ms: n("minSpeechMs", d.min_speech_ms, 60, 2000),
            silence_ms: n("silenceMs", d.silence_ms, 300, 8000),
            max_wait_ms: n("maxWaitMs", d.max_wait_ms, 1000, 60_000),
            max_turn_ms: n("maxTurnMs", d.max_turn_ms, 2000, 120_000),
        }
    }
}

/// What a frame (or a chunk of samples) changed. The numbers are the C interface (`rh_voice_feed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    None = 0,
    /// the learner started talking
    Started = 1,
    /// the turn ended (a pause after speech, or the longest turn): the recording should stop and be sent
    Ended = 2,
    /// nobody spoke (only with `auto`)
    Nothing = 3,
}

/// Live frames that measure the room when the recording starts (300 ms).
const CAL_FRAMES: usize = 15;
/// A frame this quiet is the device sending zeros (a microphone waking up), not the room: it never sets the floor.
/// (Taking the quietest frame of the start as the room, as before, put the floor at -80 dBFS whenever the device
/// began with zeros, so a fan or the hum of a laptop counted as speech and the turn never ended.)
const DEAD_DB: f32 = -90.0;
/// The end of a turn is looked for this far under the start threshold, so a voice that fades does not flicker.
const HYST_DB: f32 = 3.0;
/// During a turn the room is still followed: the quietest frame of the last 2 s, approached at most 3 dB a second.
/// Real speech has quiet gaps between words, so this stays at the room; a steady noise taken for speech does not,
/// and stops counting after a few seconds.
const FLOOR_WIN: usize = 100;
const FLOOR_RISE: f32 = 0.06;

/// The detector: fed one frame level (dBFS) at a time. The room's noise floor is measured in the first 300 ms of
/// live sound (a low percentile, so a word or a click there does not count), speech starts when frames stay over
/// floor + `start_db` for `min_speech_ms`, and the turn ends after `silence_ms` under that threshold (minus a little
/// hysteresis), after `max_turn_ms` in all, or with `Nothing` when no speech came within `max_wait_ms`. Each decision
/// is written to `note` (with `note_seq` growing) for the `[voice]` log.
#[derive(Clone, Debug)]
pub struct Vad {
    cfg: VadCfg,
    floor: f32,
    calibrated: bool,
    cal: Vec<f32>,
    recent: std::collections::VecDeque<f32>,
    frames: u32,
    /// loud frames of the run that may become speech, the quiet frames inside it, and its first frame
    run: u32,
    gap: u32,
    run_start: u32,
    speaking: bool,
    /// quiet frames since the last loud one, while speaking
    quiet: u32,
    /// the levels of the frames of the current turn (to tell speech from a steady noise at its end)
    turn: Vec<f32>,
    /// first and last frame of the speech (1-based; 0: none yet)
    pub first: u32,
    pub last: u32,
    level: f32,
    loudest: f32,
    done: bool,
    /// the last decision, for the log
    pub note: String,
    pub note_seq: u32,
}

impl Vad {
    pub fn new(cfg: VadCfg) -> Vad {
        Vad {
            cfg,
            floor: -60.0,
            calibrated: false,
            cal: Vec::with_capacity(CAL_FRAMES),
            recent: std::collections::VecDeque::with_capacity(FLOOR_WIN + 1),
            frames: 0,
            run: 0,
            gap: 0,
            run_start: 0,
            speaking: false,
            quiet: 0,
            turn: Vec::new(),
            first: 0,
            last: 0,
            level: 0.0,
            loudest: -100.0,
            done: false,
            note: String::new(),
            note_seq: 0,
        }
    }

    /// The loudness of the last frame for a meter, 0-1 (-60 dBFS to -10 dBFS).
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Speech was heard (the turn started).
    pub fn heard(&self) -> bool {
        self.first > 0
    }

    pub fn speaking(&self) -> bool {
        self.speaking && !self.done
    }

    pub fn finished(&self) -> bool {
        self.done
    }

    pub fn floor(&self) -> f32 {
        self.floor
    }

    /// Over this (dBFS) a frame is speech.
    pub fn start_threshold(&self) -> f32 {
        (self.floor + self.cfg.start_db).max(ABS_MIN_DB)
    }

    /// Under this (dBFS) a frame of a turn is silence.
    pub fn stop_threshold(&self) -> f32 {
        (self.start_threshold() - HYST_DB).max(self.floor + 2.0)
    }

    fn say(&mut self, text: String) {
        self.note = text;
        self.note_seq = self.note_seq.wrapping_add(1);
    }

    /// How long it listened (it stops counting when it decides).
    pub fn elapsed_ms(&self) -> u32 {
        self.frames * FRAME_MS
    }

    fn secs(&self) -> f32 {
        (self.frames * FRAME_MS) as f32 / 1000.0
    }

    /// The turn held real speech: enough frames over the threshold of the room as it is known now.
    fn real_speech(&self) -> bool {
        let thr = self.start_threshold();
        self.turn.iter().filter(|d| **d > thr).count() as u32 * FRAME_MS >= self.cfg.min_speech_ms
    }

    /// One 20 ms frame of loudness `db` (dBFS).
    pub fn frame(&mut self, db: f32) -> Event {
        if self.done {
            return Event::None;
        }
        self.frames += 1;
        self.level = ((db + 60.0) / 50.0).clamp(0.0, 1.0);
        let ms = |frames: u32| frames * FRAME_MS;
        let live = db > DEAD_DB;
        let wait = self.cfg.max_wait_ms.min(self.cfg.max_turn_ms);
        if live {
            self.loudest = self.loudest.max(db);
            self.recent.push_back(db);
            if self.recent.len() > FLOOR_WIN {
                self.recent.pop_front();
            }
        }
        // the first 300 ms of live sound: the room
        if !self.calibrated {
            if live {
                self.cal.push(db);
            }
            if self.cal.len() >= CAL_FRAMES {
                let mut v = self.cal.clone();
                v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                self.floor = v[v.len() / 4].clamp(-85.0, -25.0);
                self.calibrated = true;
                self.say(format!(
                    "room noise {:.0} dBFS (first {} ms), speech above {:.0} dBFS (margin {:.0} dB), silence to end {} ms",
                    self.floor,
                    CAL_FRAMES as u32 * FRAME_MS,
                    self.start_threshold(),
                    self.cfg.start_db,
                    self.cfg.silence_ms
                ));
            } else if self.cfg.auto && ms(self.frames) >= wait {
                self.done = true;
                self.say(format!("no sound at all in {:.1} s: stopped without sending", self.secs()));
                return Event::Nothing;
            }
            return Event::None;
        }
        // follow the room: down at once and up slowly while nobody talks; during a possible turn, towards the
        // quietest frame of the last 2 s
        if live {
            if !self.speaking && self.run == 0 && db < self.start_threshold() {
                self.floor = if db < self.floor { db } else { self.floor * 0.97 + db * 0.03 };
            } else {
                let low = self.recent.iter().cloned().fold(f32::INFINITY, f32::min);
                if low < self.floor {
                    self.floor = low;
                } else if self.recent.len() >= FLOOR_WIN {
                    self.floor += (low - self.floor).min(FLOOR_RISE);
                }
            }
            self.floor = self.floor.clamp(-85.0, -25.0);
        }
        let start = self.start_threshold();
        if !self.speaking {
            let loud = live && db > start;
            if loud {
                if self.run == 0 {
                    self.run_start = self.frames;
                    self.turn.clear();
                }
                self.run += 1;
                self.gap = 0;
                self.turn.push(db);
            } else if self.run > 0 {
                // a short dip inside a word is still the same run
                self.gap += 1;
                self.turn.push(db);
                if self.gap > 5 {
                    self.run = 0;
                    self.gap = 0;
                    self.turn.clear();
                }
            }
            if self.run > 0 && ms(self.run) >= self.cfg.min_speech_ms {
                self.speaking = true;
                self.first = self.run_start;
                self.last = self.frames;
                self.quiet = 0;
                self.say(format!("speech started at {:.1} s: {:.0} dBFS, room {:.0} dBFS, threshold {:.0} dBFS", self.secs(), db, self.floor, start));
                return Event::Started;
            }
            if self.cfg.auto && ms(self.frames) >= wait {
                self.done = true;
                self.say(format!(
                    "no speech in {:.1} s (loudest {:.0} dBFS, speech needs over {:.0} dBFS): stopped without sending",
                    self.secs(),
                    self.loudest,
                    start
                ));
                return Event::Nothing;
            }
            return Event::None;
        }
        self.turn.push(db);
        if live && db > self.stop_threshold() {
            self.quiet = 0;
            self.last = self.frames;
        } else {
            self.quiet += 1;
        }
        let paused = ms(self.quiet) >= self.cfg.silence_ms;
        let capped = ms(self.frames) >= self.cfg.max_turn_ms;
        if self.cfg.auto && (paused || capped) {
            if !self.real_speech() {
                // the "speech" was a steady noise the floor has caught up with: wait for real speech
                self.speaking = false;
                self.first = 0;
                self.last = 0;
                self.run = 0;
                self.gap = 0;
                self.quiet = 0;
                self.turn.clear();
                if capped || ms(self.frames) >= wait {
                    self.done = true;
                    self.say(format!("only steady noise in {:.1} s (room now {:.0} dBFS): stopped without sending", self.secs(), self.floor));
                    return Event::Nothing;
                }
                self.say(format!("false start at {:.1} s: a steady noise, not speech (room now {:.0} dBFS)", self.secs(), self.floor));
                return Event::None;
            }
            self.done = true;
            self.speaking = false;
            if capped {
                self.say(format!("turn ended at the longest turn ({} s)", self.cfg.max_turn_ms / 1000));
            } else {
                self.say(format!(
                    "end of speech at {:.1} s: {} ms under {:.0} dBFS (room {:.0} dBFS)",
                    self.secs(),
                    ms(self.quiet),
                    self.stop_threshold(),
                    self.floor
                ));
            }
            return Event::Ended;
        }
        Event::None
    }
}

/// Loudness of a frame in dBFS.
pub fn frame_db(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return -100.0;
    }
    let e: f32 = frame.iter().map(|x| x * x).sum::<f32>() / frame.len() as f32;
    10.0 * (e + 1e-10).log10()
}

/// A recording in progress: mixes down, resamples to 16 kHz, keeps the samples and runs the detector.
#[derive(Clone, Debug)]
pub struct Capture {
    pub vad: Vad,
    pcm: Vec<i16>,
    frame: Vec<f32>,
    // resampling: box average when the input is faster than 16 kHz, linear interpolation when slower
    rate: u32,
    phase: f64,
    acc: f32,
    n: u32,
    prev: f32,
    t: f64,
    /// the loudest sample of the device (0-1), before any gain
    peak: f32,
}

impl Capture {
    pub fn new(cfg: VadCfg) -> Capture {
        Capture { vad: Vad::new(cfg), pcm: Vec::new(), frame: Vec::with_capacity(FRAME), rate: 0, phase: 0.0, acc: 0.0, n: 0, prev: 0.0, t: 0.0, peak: 0.0 }
    }

    /// `samples` interleaved (-1..1), `channels` per frame, at `rate` Hz. Returns the first event of the chunk.
    pub fn feed(&mut self, samples: &[f32], channels: usize, rate: u32) -> Event {
        if channels == 0 || rate == 0 {
            return Event::None;
        }
        if self.rate == 0 {
            self.rate = rate;
        }
        let mut ev = Event::None;
        for f in samples.chunks(channels) {
            for x in f {
                self.peak = self.peak.max(x.abs().min(1.0));
            }
            let mono = f.iter().sum::<f32>() / f.len() as f32;
            let e = self.resample(mono, rate);
            if ev == Event::None {
                ev = e;
            }
        }
        ev
    }

    fn resample(&mut self, x: f32, rate: u32) -> Event {
        let mut ev = Event::None;
        if rate == RATE {
            return self.push(x);
        }
        if rate > RATE {
            let step = rate as f64 / RATE as f64;
            self.acc += x;
            self.n += 1;
            self.phase += 1.0;
            if self.phase >= step {
                self.phase -= step;
                let y = self.acc / self.n as f32;
                self.acc = 0.0;
                self.n = 0;
                ev = self.push(y);
            }
            return ev;
        }
        // slower than 16 kHz: points between the last sample and this one
        let step = rate as f64 / RATE as f64;
        while self.t < 1.0 {
            let y = self.prev + (x - self.prev) * self.t as f32;
            let e = self.push(y);
            if ev == Event::None {
                ev = e;
            }
            self.t += step;
        }
        self.t -= 1.0;
        self.prev = x;
        ev
    }

    fn push(&mut self, y: f32) -> Event {
        if self.pcm.len() >= MAX_SAMPLES {
            return Event::None;
        }
        let y = y.clamp(-1.0, 1.0);
        self.pcm.push((y * 32767.0) as i16);
        self.frame.push(y);
        if self.frame.len() < FRAME {
            return Event::None;
        }
        let db = frame_db(&self.frame);
        self.frame.clear();
        self.vad.frame(db)
    }

    pub fn level(&self) -> f32 {
        self.vad.level()
    }

    pub fn samples(&self) -> &[i16] {
        &self.pcm
    }

    pub fn seconds(&self) -> f32 {
        self.pcm.len() as f32 / RATE as f32
    }

    /// The loudest sample the device gave, in dBFS (-100 for none).
    pub fn peak_db(&self) -> f32 {
        if self.peak <= 0.0 {
            -100.0
        } else {
            20.0 * self.peak.log10()
        }
    }

    /// The device gave only zeros (or the last bit of noise): not a quiet room, a closed microphone. Windows does this
    /// when desktop apps may not use the microphone, and a muted or wrong input does it too.
    pub fn dead_silent(&self) -> bool {
        !self.pcm.is_empty() && self.peak < DEAD_PEAK
    }

    /// What is sent: from 400 ms before the speech to 300 ms after it; the whole recording when no speech was
    /// found (a soft voice may stay under the detector: Whisper's own VAD and the cloud models decide then).
    pub fn clip(&self) -> &[i16] {
        if !self.vad.heard() {
            return &self.pcm;
        }
        let at = |frame: u32| (frame.saturating_sub(1) as usize) * FRAME;
        let pre = RATE as usize * 400 / 1000;
        let post = RATE as usize * 300 / 1000;
        let a = at(self.vad.first).saturating_sub(pre);
        let b = (at(self.vad.last) + FRAME + post).min(self.pcm.len());
        if a >= b {
            return &self.pcm;
        }
        &self.pcm[a..b]
    }

    /// How much the clip is raised: a quiet microphone (a laptop's, a headset turned down) gives a voice Whisper and
    /// its own detector often take for silence, so the clip is brought up to about -3 dBFS (at most +40 dB, never
    /// down). The loudest 0.1 % of the samples are not counted (`robust_peak`): a click must not decide it.
    pub fn gain(&self) -> f32 {
        let peak = robust_peak(self.clip());
        if peak < DEAD_PEAK || peak >= LOUD_ENOUGH {
            return 1.0;
        }
        (TARGET_PEAK / peak).clamp(1.0, MAX_GAIN)
    }

    /// The loudness of the clip sent, before any gain: RMS in dBFS (-100 for none).
    pub fn rms_db(&self) -> f32 {
        let c = self.clip();
        if c.is_empty() {
            return -100.0;
        }
        let e: f64 = c.iter().map(|s| (*s as f64 / 32768.0).powi(2)).sum::<f64>() / c.len() as f64;
        if e <= 0.0 {
            -100.0
        } else {
            (10.0 * e.log10()) as f32
        }
    }

    /// The clip as a WAV file (mono, 16 bit, 16 kHz), raised by `gain`, with what was measured before the gain in a
    /// small chunk after the sound (`Measure`, read back by `wav_info` for the log and the microphone test).
    pub fn wav(&self) -> Vec<u8> {
        let g = self.gain();
        let m = Measure { peak_db: self.peak_db(), rms_db: self.rms_db(), gain: g, speech: self.vad.heard() };
        if g <= 1.0 {
            return wav_with(self.clip(), RATE, Some(m));
        }
        let up: Vec<i16> = self.clip().iter().map(|s| (*s as f32 * g).round().clamp(-32768.0, 32767.0) as i16).collect();
        wav_with(&up, RATE, Some(m))
    }
}

/// The level under which all but the loudest `GAIN_IGNORE` share of the samples stay (0-1).
pub fn robust_peak(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return 0.0;
    }
    let mut hist = vec![0u32; 32769];
    for s in pcm {
        hist[(*s as i32).unsigned_abs().min(32768) as usize] += 1;
    }
    let skip = (pcm.len() as f64 * GAIN_IGNORE) as u64;
    let mut seen = 0u64;
    for (v, n) in hist.iter().enumerate().rev() {
        seen += *n as u64;
        if seen > skip {
            return v as f32 / 32768.0;
        }
    }
    0.0
}

/// What the microphone gave, measured before the gain (kept in the WAV file sent).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    pub peak_db: f32,
    pub rms_db: f32,
    pub gain: f32,
    /// the detector found speech
    pub speech: bool,
}

/// What a WAV file holds: its format, its length, its loudness as sent, and the microphone's own `Measure` when the
/// app wrote it.
#[derive(Clone, Debug, PartialEq)]
pub struct WavInfo {
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    /// 1 = integer PCM, 3 = float
    pub format: u16,
    pub seconds: f32,
    pub peak_db: f32,
    pub rms_db: f32,
    pub measure: Option<Measure>,
}

/// Reads the chunks of a WAV file (PCM 8/16/24/32 bit or float 32, any rate and channels, extensible headers too).
/// None when it is not a WAV file.
pub fn wav_info(b: &[u8]) -> Option<WavInfo> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let f32_at = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let (mut rate, mut channels, mut bits, mut format) = (0u32, 0u16, 0u16, 0u16);
    let mut data: &[u8] = &[];
    let mut measure = None;
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32_at(i + 4) as usize;
        let body = &b[i + 8..(i + 8 + len).min(b.len())];
        match id {
            b"fmt " if body.len() >= 16 => {
                format = u16_at(i + 8);
                channels = u16_at(i + 10);
                rate = u32_at(i + 12);
                bits = u16_at(i + 22);
                // WAVE_FORMAT_EXTENSIBLE: the real format is the first two bytes of the sub-format
                if format == 0xFFFE && body.len() >= 26 {
                    format = u16_at(i + 32);
                }
            }
            b"data" => data = body,
            b"rhmm" if body.len() >= 16 => {
                measure = Some(Measure { peak_db: f32_at(i + 8), rms_db: f32_at(i + 12), gain: f32_at(i + 16), speech: f32_at(i + 20) > 0.5 })
            }
            _ => {}
        }
        i += 8 + len + (len & 1);
    }
    if rate == 0 || channels == 0 || bits == 0 {
        return None;
    }
    let width = (bits as usize).div_ceil(8);
    let frame = width * channels as usize;
    let n = if width == 0 { 0 } else { data.len() / width };
    let sample = |k: usize| -> f64 {
        let p = &data[k * width..k * width + width];
        match (format, width) {
            (3, 4) => f32::from_le_bytes([p[0], p[1], p[2], p[3]]) as f64,
            (_, 1) => (p[0] as f64 - 128.0) / 128.0,
            (_, 2) => i16::from_le_bytes([p[0], p[1]]) as f64 / 32768.0,
            (_, 3) => (((p[0] as i32) << 8 | (p[1] as i32) << 16 | (p[2] as i32) << 24) >> 8) as f64 / 8_388_608.0,
            (_, 4) => i32::from_le_bytes([p[0], p[1], p[2], p[3]]) as f64 / 2_147_483_648.0,
            _ => 0.0,
        }
    };
    let (mut peak, mut e) = (0f64, 0f64);
    for k in 0..n {
        let x = sample(k);
        if x.is_finite() {
            peak = peak.max(x.abs());
            e += x * x;
        }
    }
    Some(WavInfo {
        rate,
        channels,
        bits,
        format,
        seconds: if frame == 0 { 0.0 } else { (data.len() / frame) as f32 / rate as f32 },
        peak_db: if peak > 0.0 { (20.0 * peak.log10()) as f32 } else { -100.0 },
        rms_db: if n > 0 && e > 0.0 { (10.0 * (e / n as f64).log10()) as f32 } else { -100.0 },
        measure,
    })
}

/// A mono 16-bit PCM WAV file.
pub fn wav_bytes(pcm: &[i16], rate: u32) -> Vec<u8> {
    wav_with(pcm, rate, None)
}

/// A mono 16-bit PCM WAV file; `m` goes in an "rhmm" chunk after the sound (players and the speech engines skip
/// chunks they do not know).
pub fn wav_with(pcm: &[i16], rate: u32, m: Option<Measure>) -> Vec<u8> {
    let data = (pcm.len() * 2) as u32;
    let extra = if m.is_some() { 8 + 16 } else { 0 };
    let mut out = Vec::with_capacity(44 + data as usize + extra as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data + extra).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    if let Some(m) = m {
        out.extend_from_slice(b"rhmm");
        out.extend_from_slice(&16u32.to_le_bytes());
        for v in [m.peak_db, m.rms_db, m.gain, if m.speech { 1.0 } else { 0.0 }] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ms` of a tone (speech stand-in) or of quiet noise, at `rate` Hz with `ch` channels.
    fn sound(ms: u32, rate: u32, ch: usize, amp: f32) -> Vec<f32> {
        let n = (rate as u64 * ms as u64 / 1000) as usize;
        let mut out = Vec::with_capacity(n * ch);
        let mut seed = 12345u32;
        for i in 0..n {
            // a 220 Hz tone plus a little noise
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let noise = ((seed >> 16) as f32 / 32768.0 - 1.0) * 0.002;
            let v = (i as f32 * 2.0 * std::f32::consts::PI * 220.0 / rate as f32).sin() * amp + noise;
            for _ in 0..ch {
                out.push(v);
            }
        }
        out
    }

    fn run(c: &mut Capture, parts: &[(u32, f32)], rate: u32, ch: usize) -> Vec<Event> {
        let mut evs = vec![];
        for (ms, amp) in parts {
            // fed in 10 ms chunks, like an audio device
            let s = sound(*ms, rate, ch, *amp);
            for chunk in s.chunks((rate as usize / 100) * ch) {
                let e = c.feed(chunk, ch, rate);
                if e != Event::None {
                    evs.push(e);
                }
            }
        }
        evs
    }

    #[test]
    fn a_turn_ends_after_the_learners_pause() {
        let mut c = Capture::new(VadCfg::for_lang("ja", 0));
        // quiet room, a sentence, a 1 s thinking pause, more speech, then silence
        let evs = run(&mut c, &[(400, 0.0), (900, 0.3), (1000, 0.0), (700, 0.3), (2500, 0.0)], 48_000, 2);
        assert_eq!(evs, vec![Event::Started, Event::Ended], "a 1 s pause must not end the turn");
        // the clip keeps the speech and a little around it, not the long silences
        let secs = c.clip().len() as f32 / RATE as f32;
        assert!(secs > 2.6 && secs < 3.6, "clip of {} s", secs);
        assert!(c.seconds() > 4.0 && c.seconds() < 5.6, "recorded {} s", c.seconds());
        // a shorter pause setting ends at the thinking pause
        let mut fast = Capture::new(VadCfg::for_lang("pt", 800));
        let evs = run(&mut fast, &[(400, 0.0), (900, 0.3), (1000, 0.0), (700, 0.3)], 16_000, 1);
        assert_eq!(evs, vec![Event::Started, Event::Ended]);
    }

    #[test]
    fn clicks_are_not_speech_and_silence_gives_up() {
        let mut cfg = VadCfg::for_lang("en", 0);
        cfg.max_wait_ms = 3000;
        let mut c = Capture::new(cfg);
        // two 100 ms clicks: shorter than min_speech_ms
        let evs = run(&mut c, &[(400, 0.0), (100, 0.5), (500, 0.0), (100, 0.5), (3000, 0.0)], 44_100, 1);
        assert_eq!(evs, vec![Event::Nothing]);
        assert!(!c.vad.heard());
        // without auto it only measures: nothing ends by itself
        let mut cfg = VadCfg::for_lang("en", 0);
        cfg.auto = false;
        let mut m = Capture::new(cfg);
        let evs = run(&mut m, &[(300, 0.0), (600, 0.3), (5000, 0.0)], 16_000, 1);
        assert_eq!(evs, vec![Event::Started]);
        assert!(m.level() < 0.2);
    }

    #[test]
    fn a_noisy_room_still_finds_the_voice() {
        let mut c = Capture::new(VadCfg::for_lang("es", 0));
        // a fan at about -35 dBFS, then a voice 15 dB louder
        let evs = run(&mut c, &[(600, 0.025), (800, 0.14), (2000, 0.025)], 48_000, 1);
        assert_eq!(evs, vec![Event::Started, Event::Ended]);
        assert!(c.vad.floor() > -45.0);
    }

    /// `ms` of exact zeros (a device waking up)
    fn zeros(c: &mut Capture, ms: u32) {
        let s = vec![0.0f32; 16 * ms as usize];
        for chunk in s.chunks(160) {
            assert_eq!(c.feed(chunk, 1, 16_000), Event::None);
        }
    }

    #[test]
    fn a_device_that_starts_with_zeros_still_ends_the_turn() {
        // the old floor (quietest frame of the start) was -80 dBFS here, so the room (-59 dBFS) was "speech" for ever
        let mut c = Capture::new(VadCfg::for_lang("ja", 0));
        zeros(&mut c, 400);
        let evs = run(&mut c, &[(500, 0.0), (900, 0.3), (2000, 0.0)], 16_000, 1);
        assert_eq!(evs, vec![Event::Started, Event::Ended]);
        assert!(c.vad.floor() > -65.0 && c.vad.floor() < -50.0, "floor {}", c.vad.floor());
        assert!(c.vad.note.starts_with("end of speech"), "{}", c.vad.note);
        // it ended about 1.2 s after the speech, not at the longest turn
        assert!(c.vad.elapsed_ms() < 3400, "ended at {} ms", c.vad.elapsed_ms());
    }

    #[test]
    fn a_steady_noise_is_not_a_turn() {
        // a quiet room, then a fan 20 dB louder that never stops: no turn is sent
        let mut c = Capture::new(VadCfg::for_lang("en", 0));
        let evs = run(&mut c, &[(500, 0.0), (14_000, 0.014)], 16_000, 1);
        assert_eq!(evs.last(), Some(&Event::Nothing), "{:?} / {}", evs, c.vad.note);
        assert!(!c.vad.heard());
        assert!(c.vad.elapsed_ms() < 12_000, "ended at {} ms", c.vad.elapsed_ms());
    }

    #[test]
    fn the_cut_sensitivity_and_the_longest_turn() {
        // a voice only 8 dB over the room: heard with a margin of 6 dB, not with 14 dB
        let mut c = Capture::new(VadCfg::for_user("pt", 0, 6));
        let evs = run(&mut c, &[(500, 0.003), (900, 0.0075), (2000, 0.003)], 16_000, 1);
        assert_eq!(evs, vec![Event::Started, Event::Ended], "{}", c.vad.note);
        let mut c = Capture::new(VadCfg::for_user("pt", 0, 14));
        let evs = run(&mut c, &[(500, 0.003), (900, 0.0075), (8000, 0.003)], 16_000, 1);
        assert_eq!(evs, vec![Event::Nothing], "{}", c.vad.note);
        // someone who never pauses is cut at 30 s
        let mut c = Capture::new(VadCfg::for_lang("pt", 0));
        let mut parts = vec![(400u32, 0.0f32)];
        for _ in 0..40 {
            parts.push((700, 0.3));
            parts.push((300, 0.05));
        }
        let evs = run(&mut c, &parts, 16_000, 1);
        assert_eq!(evs, vec![Event::Started, Event::Ended]);
        assert!(c.vad.note.contains("longest"), "{}", c.vad.note);
    }

    #[test]
    fn resampling_keeps_the_duration() {
        for (rate, ch) in [(48_000u32, 2usize), (44_100, 1), (16_000, 1), (8_000, 1), (22_050, 2)] {
            let mut cfg = VadCfg::default();
            cfg.auto = false;
            let mut c = Capture::new(cfg);
            let s = sound(1000, rate, ch, 0.2);
            c.feed(&s, ch, rate);
            let n = c.samples().len() as i64;
            assert!((n - 16_000).abs() <= 2, "{} Hz x{}: {} samples", rate, ch, n);
            // the tone is still there (not averaged away)
            let peak = c.samples().iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
            assert!(peak > 5000, "{} Hz: peak {}", rate, peak);
        }
    }

    #[test]
    fn wav_header_is_right() {
        let w = wav_bytes(&[0, 1, -1], 16_000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([w[4], w[5], w[6], w[7]]), 36 + 6);
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes([w[24], w[25], w[26], w[27]]), 16_000);
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]), 6);
        assert_eq!(w.len(), 44 + 6);
        assert_eq!(&w[44..46], &0i16.to_le_bytes());
    }

    #[test]
    fn a_quiet_voice_is_raised_and_a_closed_microphone_is_told() {
        // a laptop microphone far away: the voice peaks near -40 dBFS
        let mut c = Capture::new(VadCfg { auto: false, ..VadCfg::default() });
        run(&mut c, &[(300, 0.0), (1200, 0.01), (300, 0.0)], 48_000, 2);
        assert!(!c.dead_silent());
        assert!(c.peak_db() < -35.0 && c.peak_db() > -45.0, "{}", c.peak_db());
        let g = c.gain();
        assert!(g > 20.0 && g <= MAX_GAIN, "{}", g);
        let w = c.wav();
        let loudest = w[44..].chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs()).max().unwrap();
        assert!(loudest > 6000, "{}", loudest);
        // a normal voice is left alone
        let mut n = Capture::new(VadCfg { auto: false, ..VadCfg::default() });
        run(&mut n, &[(300, 0.0), (1200, 0.5), (300, 0.0)], 16_000, 1);
        assert_eq!(n.gain(), 1.0);
        // zeros only: Windows' answer when desktop apps may not use the microphone
        let mut z = Capture::new(VadCfg { auto: false, ..VadCfg::default() });
        for _ in 0..100 {
            z.feed(&[0.0; 960], 2, 48_000);
        }
        assert!(z.dead_silent());
        assert_eq!(z.peak_db(), -100.0);
        assert_eq!(z.gain(), 1.0);
    }

    #[test]
    fn a_very_quiet_microphone_is_still_heard_and_raised() {
        // a headset turned down: room near -75 dBFS, the voice peaking near -45 dBFS (RMS about -48)
        let mut c = Capture::new(VadCfg::for_lang("ja", 0));
        let evs = run(&mut c, &[(400, 0.0), (1500, 0.0056), (2500, 0.0)], 48_000, 1);
        assert_eq!(evs, vec![Event::Started, Event::Ended], "a quiet voice well above its room is speech");
        // one loud click must not keep the rest quiet: the gain ignores the loudest 0.1 %
        let mut k = Capture::new(VadCfg { auto: false, ..VadCfg::default() });
        run(&mut k, &[(300, 0.0), (1500, 0.005), (300, 0.0)], 16_000, 1);
        let mut click = vec![0f32; 160];
        click[3] = 0.6;
        k.feed(&click, 1, 16_000);
        assert!(k.peak_db() > -6.0);
        let g = k.gain();
        assert!(g > 50.0, "gain {} (the click decided it)", g);
        assert!(k.rms_db() < -45.0 && k.rms_db() > -60.0, "{}", k.rms_db());
    }

    #[test]
    fn the_wav_file_tells_what_the_microphone_gave() {
        let mut c = Capture::new(VadCfg { auto: false, ..VadCfg::default() });
        run(&mut c, &[(300, 0.0), (1000, 0.01), (300, 0.0)], 48_000, 2);
        let w = c.wav();
        let info = wav_info(&w).unwrap();
        assert_eq!((info.rate, info.channels, info.bits, info.format), (16_000, 1, 16, 1));
        assert!((info.seconds - c.clip().len() as f32 / 16_000.0).abs() < 0.01);
        // as sent: raised; as measured: the quiet voice
        assert!(info.peak_db > -6.0, "{}", info.peak_db);
        let m = info.measure.unwrap();
        assert!(m.peak_db < -35.0 && m.gain > 20.0, "{:?}", m);
        assert!(m.rms_db < m.peak_db);
        // the chunk counts in the RIFF size
        assert_eq!(u32::from_le_bytes([w[4], w[5], w[6], w[7]]) as usize, w.len() - 8);
        // a float stereo file (Qt's own recorder on some systems) is read too
        let mut f = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        f.extend_from_slice(&16u32.to_le_bytes());
        f.extend_from_slice(&3u16.to_le_bytes());
        f.extend_from_slice(&2u16.to_le_bytes());
        f.extend_from_slice(&48_000u32.to_le_bytes());
        f.extend_from_slice(&(48_000u32 * 8).to_le_bytes());
        f.extend_from_slice(&8u16.to_le_bytes());
        f.extend_from_slice(&32u16.to_le_bytes());
        f.extend_from_slice(b"data");
        f.extend_from_slice(&(48_000u32 * 8).to_le_bytes());
        for i in 0..48_000 {
            let v = if i % 2 == 0 { 0.5f32 } else { -0.5 };
            f.extend_from_slice(&v.to_le_bytes());
            f.extend_from_slice(&v.to_le_bytes());
        }
        let i = wav_info(&f).unwrap();
        assert_eq!((i.rate, i.channels, i.format), (48_000, 2, 3));
        assert!((i.seconds - 1.0).abs() < 0.001);
        assert!((i.peak_db + 6.02).abs() < 0.1 && (i.rms_db + 6.02).abs() < 0.1, "{:?}", i);
        assert!(i.measure.is_none());
        assert!(wav_info(b"nope").is_none());
    }

    #[test]
    fn the_page_round_trips_the_settings() {
        let c = VadCfg::for_lang("ja", 0);
        assert_eq!(c.silence_ms, DEFAULT_PAUSE_MS);
        assert_eq!((c.max_wait_ms, c.max_turn_ms), (8_000, 30_000));
        assert_eq!(VadCfg::for_lang("pt", 2500).silence_ms, 2500);
        assert_eq!(VadCfg::for_user("pt", 0, 16).start_db, 16.0);
        assert_eq!(VadCfg::for_user("pt", 0, 0).start_db, 10.0);
        let back = VadCfg::from_json(&c.to_json());
        assert_eq!(back, c);
        let mut j = c.to_json();
        j.set("auto", Json::Bool(false));
        j.set("silenceMs", Json::Num(99999.0));
        let b = VadCfg::from_json(&j);
        assert!(!b.auto);
        assert_eq!(b.silence_ms, 8000);
        assert!(!VadCfg::from_json(&Json::Null).auto);
    }
}
