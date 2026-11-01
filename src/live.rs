//! Gemini Live: a spoken conversation with Gemini over one WebSocket (the Live API, `BidiGenerateContent`), for Study >
//! Conversation (the Kaiwa layer that steers it is `kaiwa_live`). Desktop only, like the rest of Conversation.
//!
//! Up the socket: one `setup` (the model, the answer as AUDIO in a voice, the system instruction, the tools, how the
//! learner's turns are cut, transcription of both sides, session resumption and a sliding context window), then the
//! microphone as `realtimeInput.audio` (16 kHz 16-bit mono PCM, base64), typed text as `realtimeInput.text`, the notes
//! of the app as `clientContent`, and the answers to the tools as `toolResponse`.
//! Down: `serverContent` (the model's voice, 24 kHz 16-bit mono PCM, and the transcripts), `toolCall`, `goAway` (the
//! connection ends soon) and `sessionResumptionUpdate` (a handle to come back to the same session with).
//!
//! The sound never passes through the Core: the Qt side (qt/src/liveaudio.cpp) moves PCM in and out of the `Hub`
//! (`hub()`), and the worker thread here moves it between the hub and the socket. What the Core needs to know comes as
//! `Event`s. The socket sits behind `Wire` / `Dialer`, so the tests run whole sessions against a scripted one.
use crate::json::Json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const HOST: &str = "generativelanguage.googleapis.com";
pub const PATH: &str = "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
/// The Live model asked when the person chose none: the stable one of the docs (ai.google.dev, October 2026), on the
/// free tier of a Google AI Studio key.
pub const DEFAULT_MODEL: &str = "gemini-3.8-live";
/// Tried in turn when the model asked is refused (not offered to this key or region), only when none was chosen.
pub const FALLBACK_MODELS: &[&str] = &["gemini-2.5-flash-native-audio-preview-12-2025", "gemini-3.1-flash-live-preview"];
/// Prebuilt voices of the Live API (all of them speak every language of the native-audio models).
pub const VOICES: &[&str] = &["Kore", "Aoede", "Leda", "Zephyr", "Puck", "Charon", "Fenrir", "Orus"];
/// The microphone goes up at this rate; the model's voice comes down at OUT_RATE.
pub const IN_RATE: u32 = 16_000;
pub const OUT_RATE: u32 = 24_000;
/// The microphone is sent in pieces of 40 ms.
const MIC_PIECE: usize = (IN_RATE as usize) / 25;
/// After the speaker falls silent the room still echoes a little: the open microphone stays muted this long.
const ECHO_TAIL_MS: u64 = 350;

/// How the learner's turns are cut. `manual`: push-to-talk (the app says when a turn starts and ends); otherwise
/// Gemini's voice activity detection, with `silence_ms` of silence ending a turn (learners pause to think: longer than
/// the 800 ms default) and `prefix_ms` of speech needed before a turn starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Vad {
    pub manual: bool,
    pub silence_ms: u32,
    pub prefix_ms: u32,
}

impl Default for Vad {
    fn default() -> Vad {
        Vad { manual: false, silence_ms: 1200, prefix_ms: 200 }
    }
}

/// Everything a call needs. `models` are tried in order until one takes the setup.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub key: String,
    pub models: Vec<String>,
    pub voice: String,
    /// BCP-47 code of the language spoken ("ja-JP"), sent as `speechConfig.languageCode`; dropped (with the hints
    /// below) when the model refuses it (the native-audio models pick the language themselves).
    pub language_code: String,
    /// Languages the learner may speak (the one studied first), as hints for the transcription of their voice.
    pub hear_langs: Vec<String>,
    pub system: String,
    /// The function declarations (an array), `[]` for none.
    pub tools: Json,
    pub vad: Vad,
    /// Sent once after the first setup, so the tutor speaks first ("" for nothing).
    pub opening: String,
    /// Push-to-talk / open microphone with headphones (the hub's echo guard).
    pub headset: bool,
}

// ---------------------------------------------------------------- messages up

fn s(v: &str) -> Json {
    Json::Str(v.to_string())
}

/// The first message of a connection. `handle`: resume that session; `extras`: also send the language code and the
/// transcription hints (left out after a model refused them).
pub fn setup_message(c: &Config, model: &str, handle: Option<&str>, extras: bool) -> Json {
    let mut speech = vec![("voiceConfig", Json::obj(vec![("prebuiltVoiceConfig", Json::obj(vec![("voiceName", s(&c.voice))]))]))];
    if extras && !c.language_code.is_empty() {
        speech.push(("languageCode", s(&c.language_code)));
    }
    let gen = Json::obj(vec![("responseModalities", Json::Arr(vec![s("AUDIO")])), ("speechConfig", Json::obj(speech))]);
    let detect = if c.vad.manual {
        Json::obj(vec![("disabled", Json::Bool(true))])
    } else {
        Json::obj(vec![
            ("disabled", Json::Bool(false)),
            // a learner stops to think in the middle of a sentence: end the turn only on a clear, long silence
            ("endOfSpeechSensitivity", s("END_SENSITIVITY_LOW")),
            ("prefixPaddingMs", Json::Num(c.vad.prefix_ms as f64)),
            ("silenceDurationMs", Json::Num(c.vad.silence_ms as f64)),
        ])
    };
    let realtime = Json::obj(vec![("automaticActivityDetection", detect), ("activityHandling", s("START_OF_ACTIVITY_INTERRUPTS"))]);
    let mut hear = vec![];
    if extras && !c.hear_langs.is_empty() {
        hear.push(("languageCodes", Json::Arr(c.hear_langs.iter().map(|l| s(l)).collect())));
    }
    let name = if model.starts_with("models/") { model.to_string() } else { format!("models/{}", model) };
    let mut setup = vec![
        ("model", Json::Str(name)),
        ("generationConfig", gen),
        ("systemInstruction", Json::obj(vec![("parts", Json::Arr(vec![Json::obj(vec![("text", s(&c.system))])]))])),
    ];
    if !c.tools.items().is_empty() {
        setup.push(("tools", Json::Arr(vec![Json::obj(vec![("functionDeclarations", c.tools.clone())])])));
    }
    setup.push(("realtimeInputConfig", realtime));
    setup.push(("inputAudioTranscription", Json::obj(hear)));
    setup.push(("outputAudioTranscription", Json::obj(vec![])));
    setup.push(("sessionResumption", match handle {
        Some(h) => Json::obj(vec![("handle", s(h))]),
        None => Json::obj(vec![]),
    }));
    // without it an audio session ends after 15 minutes
    setup.push(("contextWindowCompression", Json::obj(vec![("slidingWindow", Json::obj(vec![]))])));
    Json::obj(vec![("setup", Json::obj(setup))])
}

/// A piece of the microphone (16 kHz mono).
pub fn audio_message(pcm: &[i16]) -> String {
    format!("{{\"realtimeInput\":{{\"audio\":{{\"data\":\"{}\",\"mimeType\":\"audio/pcm;rate={}\"}}}}}}", b64_encode(&pcm_to_le(pcm)), IN_RATE)
}

/// Text typed by the learner (a turn of theirs, like speech).
pub fn text_message(text: &str) -> String {
    Json::obj(vec![("realtimeInput", Json::obj(vec![("text", s(text))]))]).dump()
}

/// Push-to-talk: the learner's turn starts / ends. Open microphone muted: the stream pauses.
pub fn activity_message(start: bool) -> String {
    Json::obj(vec![("realtimeInput", Json::obj(vec![(if start { "activityStart" } else { "activityEnd" }, Json::obj(vec![]))]))]).dump()
}

pub fn stream_end_message() -> String {
    Json::obj(vec![("realtimeInput", Json::obj(vec![("audioStreamEnd", Json::Bool(true))]))]).dump()
}

/// A note of the app added to the conversation. `respond`: the model answers now (else it only remembers it).
pub fn note_message(text: &str, respond: bool) -> String {
    Json::obj(vec![(
        "clientContent",
        Json::obj(vec![
            ("turns", Json::Arr(vec![Json::obj(vec![("role", s("user")), ("parts", Json::Arr(vec![Json::obj(vec![("text", s(text))])]))])])),
            ("turnComplete", Json::Bool(respond)),
        ]),
    )])
    .dump()
}

/// The answers to tool calls: (id, name, response object).
pub fn tool_message(answers: &[(String, String, Json)]) -> String {
    let list = answers.iter().map(|(id, name, r)| Json::obj(vec![("id", s(id)), ("name", s(name)), ("response", r.clone())])).collect();
    Json::obj(vec![("toolResponse", Json::obj(vec![("functionResponses", Json::Arr(list))]))]).dump()
}

// ---------------------------------------------------------------- messages down

#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub args: Json,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Server {
    SetupComplete,
    /// The model's voice and its sample rate.
    Audio(Vec<i16>, u32),
    /// What the learner said / what the model said, a piece at a time.
    InputText(String),
    OutputText(String),
    /// The learner spoke over the model: its voice must stop at once.
    Interrupted,
    TurnComplete,
    GenerationComplete,
    WaitingForInput,
    ToolCall(Vec<Call>),
    ToolCancel(Vec<String>),
    /// Seconds before the server closes the connection.
    GoAway(f64),
    /// A handle to resume the session with (only sent when it can be resumed).
    Resume(String),
    VoiceStart,
    VoiceEnd,
    Usage(i64),
    Error(String),
}

fn short(t: &str) -> String {
    let mut o: String = t.chars().take(200).collect();
    if t.chars().count() > 200 {
        o.push('…');
    }
    o
}

/// "10s", "1.5s" or {"seconds": 10, "nanos": 0}.
pub fn duration_secs(j: &Json) -> f64 {
    match j {
        Json::Str(t) => t.trim().trim_end_matches('s').parse().unwrap_or(0.0),
        Json::Num(n) => *n,
        Json::Obj(_) => j.get("seconds").as_f64().unwrap_or(0.0) + j.get("nanos").as_f64().unwrap_or(0.0) / 1e9,
        _ => 0.0,
    }
}

fn rate_of(mime: &str) -> Option<u32> {
    mime.split(';').find_map(|p| p.trim().strip_prefix("rate=")).and_then(|r| r.trim().parse().ok())
}

/// One message of the server, as the things it says (in the order they matter: the learner's words, then the voice
/// and words of the model, then the end of the turn).
pub fn parse(text: &str) -> Vec<Server> {
    let j = match Json::parse(text) {
        Ok(j) => j,
        Err(_) => return vec![Server::Error(format!("unreadable message: {}", short(text)))],
    };
    let mut out = vec![];
    if !j.get("setupComplete").is_null() {
        out.push(Server::SetupComplete);
    }
    let sc = j.get("serverContent");
    if !sc.is_null() {
        let t = sc.path("inputTranscription.text").str();
        if !t.is_empty() {
            out.push(Server::InputText(t));
        }
        for p in sc.path("modelTurn.parts").items() {
            if p.get("thought").as_bool() == Some(true) {
                continue;
            }
            let d = p.get("inlineData");
            if d.is_null() {
                continue;
            }
            let mime = d.get("mimeType").str();
            if mime.is_empty() || mime.starts_with("audio/pcm") || mime.starts_with("audio/L16") {
                let pcm = pcm_from_le(&b64_decode(d.get("data").as_str().unwrap_or("")));
                if !pcm.is_empty() {
                    out.push(Server::Audio(pcm, rate_of(&mime).unwrap_or(OUT_RATE)));
                }
            }
        }
        let t = sc.path("outputTranscription.text").str();
        if !t.is_empty() {
            out.push(Server::OutputText(t));
        }
        if sc.get("interrupted").as_bool() == Some(true) {
            out.push(Server::Interrupted);
        }
        if sc.get("generationComplete").as_bool() == Some(true) {
            out.push(Server::GenerationComplete);
        }
        if sc.get("waitingForInput").as_bool() == Some(true) {
            out.push(Server::WaitingForInput);
        }
        if sc.get("turnComplete").as_bool() == Some(true) {
            out.push(Server::TurnComplete);
        }
    }
    let calls: Vec<Call> = j
        .path("toolCall.functionCalls")
        .items()
        .iter()
        .map(|c| Call { id: c.get("id").str(), name: c.get("name").str(), args: c.get("args").clone() })
        .filter(|c| !c.name.is_empty())
        .collect();
    if !calls.is_empty() {
        out.push(Server::ToolCall(calls));
    }
    let cancel = j.path("toolCallCancellation.ids").strings();
    if !cancel.is_empty() {
        out.push(Server::ToolCancel(cancel));
    }
    if !j.get("goAway").is_null() {
        out.push(Server::GoAway(duration_secs(j.path("goAway.timeLeft"))));
    }
    let r = j.get("sessionResumptionUpdate");
    if r.get("resumable").as_bool() == Some(true) && !r.get("newHandle").str().is_empty() {
        out.push(Server::Resume(r.get("newHandle").str()));
    }
    match j.path("voiceActivity.type").str().as_str() {
        "ACTIVITY_START" => out.push(Server::VoiceStart),
        "ACTIVITY_END" => out.push(Server::VoiceEnd),
        _ => {}
    }
    if let Some(n) = j.path("usageMetadata.totalTokenCount").as_i64() {
        out.push(Server::Usage(n));
    }
    if !j.get("error").is_null() {
        let e = j.get("error");
        let m = if e.get("message").str().is_empty() { e.str() } else { e.get("message").str() };
        out.push(Server::Error(m));
    }
    out
}

// ---------------------------------------------------------------- PCM and base64

pub fn pcm_to_le(pcm: &[i16]) -> Vec<u8> {
    let mut b = Vec::with_capacity(pcm.len() * 2);
    for x in pcm {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

pub fn pcm_from_le(b: &[u8]) -> Vec<i16> {
    b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(data: &[u8]) -> String {
    let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63]);
        out.push(B64[(n >> 12) as usize & 63]);
        out.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] } else { b'=' });
        out.push(if c.len() > 2 { B64[n as usize & 63] } else { b'=' });
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Standard or URL-safe base64, padding optional; anything else is skipped.
pub fn b64_decode(t: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(t.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in t.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// Linear resampling of a stream of mono PCM, piece by piece (good enough for a voice).
#[derive(Clone, Debug)]
pub struct Resampler {
    from: u32,
    to: u32,
    /// where the next output sample falls, in input samples, counted from `prev` (index 0; the new piece starts at 1)
    t: f64,
    prev: f32,
    primed: bool,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Resampler {
        Resampler { from: from.max(1), to: to.max(1), t: 1.0, prev: 0.0, primed: false }
    }

    pub fn rates(&self) -> (u32, u32) {
        (self.from, self.to)
    }

    pub fn push(&mut self, input: &[i16], out: &mut Vec<i16>) {
        if self.from == self.to {
            out.extend_from_slice(input);
            return;
        }
        let n = input.len();
        if n == 0 {
            return;
        }
        if !self.primed {
            self.primed = true;
            self.prev = input[0] as f32;
        }
        let step = self.from as f64 / self.to as f64;
        let prev = self.prev;
        let at = |i: usize| if i == 0 { prev } else { input[i - 1] as f32 };
        while self.t <= n as f64 {
            let i = self.t.floor() as usize;
            let frac = (self.t - i as f64) as f32;
            let a = at(i);
            let b = if i < n { at(i + 1) } else { a };
            out.push((a + (b - a) * frac).round().clamp(-32768.0, 32767.0) as i16);
            self.t += step;
        }
        self.t -= n as f64;
        self.prev = input[n - 1] as f32;
    }
}

// ---------------------------------------------------------------- the sound between Qt and the socket

/// The PCM exchanged with the sound devices of the window (qt/src/liveaudio.cpp through the C functions of the
/// bridge) and the state that decides whether the microphone goes up: push-to-talk held / open microphone on, and the
/// echo guard (no headphones: the open microphone sends silence while the model's voice plays, so the model does not
/// hear and interrupt itself).
pub struct Hub {
    epoch: Instant,
    /// the window can play and record (built with Qt Multimedia)
    present: AtomicBool,
    /// a call is up: the window keeps the devices open
    wanted: AtomicBool,
    /// which call owns the hub (a new call takes it over; the old worker then leaves it alone)
    owner: AtomicU64,
    /// loudness of the microphone, 0-100
    level: AtomicU32,
    /// milliseconds since `epoch`: until then the speaker (probably) still sounds
    busy_until: AtomicU64,
    /// milliseconds since `epoch` of the last loud piece of the microphone
    last_voice: AtomicU64,
    mic: Mutex<MicSide>,
    spk: Mutex<SpeakerSide>,
    device_error: Mutex<String>,
    /// the microphone chosen (setting `voice_mic`, a Qt audio input id; "" = the system's default)
    device: Mutex<String>,
}

struct MicSide {
    manual: bool,
    /// push-to-talk held, or the open microphone switched on
    open: bool,
    headset: bool,
    buf: Vec<i16>,
    rs: Resampler,
    frames: u64,
    loud_seen: bool,
}

struct SpeakerSide {
    queue: VecDeque<i16>,
    flush: bool,
    rs: Resampler,
    spare: Vec<i16>,
}

static HUB: OnceLock<Arc<Hub>> = OnceLock::new();

/// The hub of the app (one call at a time).
pub fn hub() -> Arc<Hub> {
    HUB.get_or_init(|| Arc::new(Hub::new())).clone()
}

impl Default for Hub {
    fn default() -> Hub {
        Hub::new()
    }
}

impl Hub {
    pub fn new() -> Hub {
        Hub {
            epoch: Instant::now(),
            present: AtomicBool::new(false),
            wanted: AtomicBool::new(false),
            owner: AtomicU64::new(0),
            level: AtomicU32::new(0),
            busy_until: AtomicU64::new(0),
            last_voice: AtomicU64::new(0),
            mic: Mutex::new(MicSide { manual: false, open: false, headset: false, buf: vec![], rs: Resampler::new(IN_RATE, IN_RATE), frames: 0, loud_seen: false }),
            spk: Mutex::new(SpeakerSide { queue: VecDeque::new(), flush: false, rs: Resampler::new(OUT_RATE, OUT_RATE), spare: vec![] }),
            device_error: Mutex::new(String::new()),
            device: Mutex::new(String::new()),
        }
    }

    /// The microphone the window opens for the next call.
    pub fn set_device(&self, id: &str) {
        if let Ok(mut d) = self.device.lock() {
            *d = id.to_string();
        }
    }
    pub fn device(&self) -> String {
        self.device.lock().map(|d| d.clone()).unwrap_or_default()
    }

    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    pub fn instant_of(&self, ms: u64) -> Instant {
        self.epoch + Duration::from_millis(ms)
    }

    pub fn set_present(&self, on: bool) {
        self.present.store(on, Ordering::Relaxed);
    }
    pub fn present(&self) -> bool {
        self.present.load(Ordering::Relaxed)
    }
    pub fn wanted(&self) -> bool {
        self.wanted.load(Ordering::Relaxed)
    }
    pub fn level(&self) -> u32 {
        self.level.load(Ordering::Relaxed)
    }
    pub fn last_voice_ms(&self) -> u64 {
        self.last_voice.load(Ordering::Relaxed)
    }

    /// A call takes the hub: devices wanted, queues empty. Returns its owner number (for `finish`).
    pub fn begin(&self, manual: bool, headset: bool) -> u64 {
        let id = self.owner.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut m) = self.mic.lock() {
            m.manual = manual;
            m.headset = headset;
            m.open = !manual;
            m.buf.clear();
            m.frames = 0;
            m.loud_seen = false;
        }
        if let Ok(mut s) = self.spk.lock() {
            s.queue.clear();
            s.spare.clear();
            s.flush = false;
        }
        if let Ok(mut e) = self.device_error.lock() {
            e.clear();
        }
        self.level.store(0, Ordering::Relaxed);
        self.busy_until.store(0, Ordering::Relaxed);
        self.wanted.store(true, Ordering::SeqCst);
        id
    }

    /// The call `owner` is over: the devices close (unless another call took the hub meanwhile).
    pub fn finish(&self, owner: u64) {
        if self.owner.load(Ordering::SeqCst) != owner {
            return;
        }
        self.wanted.store(false, Ordering::SeqCst);
        if let Ok(mut m) = self.mic.lock() {
            m.buf.clear();
            m.open = false;
        }
        self.flush_speaker();
        self.level.store(0, Ordering::Relaxed);
    }

    pub fn set_mode(&self, manual: bool, headset: bool) {
        if let Ok(mut m) = self.mic.lock() {
            if m.manual != manual {
                m.open = !manual;
                m.buf.clear();
            }
            m.manual = manual;
            m.headset = headset;
        }
    }

    /// Push-to-talk held / released, open microphone on / off.
    pub fn set_open(&self, on: bool) {
        if let Ok(mut m) = self.mic.lock() {
            m.open = on;
        }
    }

    pub fn is_open(&self) -> bool {
        self.mic.lock().map(|m| m.open).unwrap_or(false)
    }

    /// The model's voice is (probably) playing now, or ended a moment ago.
    pub fn speaking(&self) -> bool {
        let queued = self.spk.lock().map(|s| !s.queue.is_empty()).unwrap_or(false);
        let until = self.busy_until.load(Ordering::Relaxed);
        queued || (until > 0 && self.now_ms() < until + ECHO_TAIL_MS)
    }

    /// The echo guard is closing the microphone now.
    pub fn muted(&self) -> bool {
        let (manual, headset) = self.mic.lock().map(|m| (m.manual, m.headset)).unwrap_or((false, false));
        !manual && !headset && self.speaking()
    }

    /// A piece of the microphone (mono, at `rate`), from the window's sound thread.
    pub fn mic(&self, pcm: &[i16], rate: u32) {
        if !self.wanted() || pcm.is_empty() {
            return;
        }
        // loudness: -60 dBFS .. 0 dBFS as 0-100
        let sum: f64 = pcm.iter().map(|&x| (x as f64) * (x as f64)).sum();
        let rms = (sum / pcm.len() as f64).sqrt().max(1.0);
        let db = 20.0 * (rms / 32768.0).log10();
        let level = (((db + 60.0) / 60.0) * 100.0).clamp(0.0, 100.0) as u32;
        self.level.store(level, Ordering::Relaxed);
        let loud = db > -40.0;
        let mute = self.muted();
        let Ok(mut m) = self.mic.lock() else { return };
        m.frames += pcm.len() as u64;
        if pcm.iter().any(|&x| x.unsigned_abs() > 64) {
            m.loud_seen = true;
        }
        if loud && !mute {
            self.last_voice.store(self.now_ms(), Ordering::Relaxed);
        }
        if m.rs.rates() != (rate, IN_RATE) {
            m.rs = Resampler::new(rate, IN_RATE);
        }
        let mut at16 = Vec::with_capacity(pcm.len() * IN_RATE as usize / rate.max(1) as usize + 2);
        m.rs.push(pcm, &mut at16);
        if !m.open {
            return;
        }
        if mute {
            // silence keeps the stream (and the voice detection of the server) going without the echo
            let n = m.buf.len() + at16.len();
            m.buf.resize(n, 0);
        } else {
            m.buf.extend_from_slice(&at16);
        }
        // never more than 2 s waiting (the socket stalled)
        let over = m.buf.len().saturating_sub(IN_RATE as usize * 2);
        if over > 0 {
            m.buf.drain(..over);
        }
    }

    /// What the microphone gathered (16 kHz), once there is at least `min` samples (`0`: anything left).
    pub fn take_mic(&self, min: usize) -> Option<Vec<i16>> {
        let mut m = self.mic.lock().ok()?;
        if m.buf.is_empty() || m.buf.len() < min {
            return None;
        }
        Some(std::mem::take(&mut m.buf))
    }

    /// Samples seen from the microphone since the call began, and whether any was more than a whisper.
    pub fn mic_stats(&self) -> (u64, bool) {
        self.mic.lock().map(|m| (m.frames, m.loud_seen)).unwrap_or((0, false))
    }

    /// The model's voice (24 kHz) to play.
    pub fn push_speaker(&self, pcm: &[i16]) {
        if let Ok(mut s) = self.spk.lock() {
            s.queue.extend(pcm.iter().copied());
        }
    }

    /// Stop the model's voice now (the learner interrupted it, or pressed to talk).
    pub fn flush_speaker(&self) {
        if let Ok(mut s) = self.spk.lock() {
            s.queue.clear();
            s.spare.clear();
            s.flush = true;
        }
        self.busy_until.store(0, Ordering::Relaxed);
    }

    /// Samples queued for the speaker (24 kHz).
    pub fn queued(&self) -> usize {
        self.spk.lock().map(|s| s.queue.len()).unwrap_or(0)
    }

    /// Fills `out` with up to `out.len()` samples at `rate` for the speaker. `queued_ms`: what the device still holds.
    /// Returns (samples written, whether the device should drop what it holds first).
    pub fn speaker(&self, out: &mut [i16], rate: u32, queued_ms: u32) -> (usize, bool) {
        let Ok(mut s) = self.spk.lock() else { return (0, false) };
        if s.flush {
            s.flush = false;
            s.rs = Resampler::new(OUT_RATE, rate.max(1));
            s.spare.clear();
            return (0, true);
        }
        if s.rs.rates() != (OUT_RATE, rate) {
            s.rs = Resampler::new(OUT_RATE, rate.max(1));
        }
        let mut got = std::mem::take(&mut s.spare);
        if got.len() < out.len() && !s.queue.is_empty() {
            let need = ((out.len() - got.len()) as u64 * OUT_RATE as u64 / rate.max(1) as u64) as usize + 1;
            let take = need.min(s.queue.len());
            let piece: Vec<i16> = s.queue.drain(..take).collect();
            s.rs.push(&piece, &mut got);
        }
        let n = got.len().min(out.len());
        out[..n].copy_from_slice(&got[..n]);
        s.spare = got[n..].to_vec();
        let now = self.now_ms();
        if n > 0 || queued_ms > 0 {
            let until = now + queued_ms as u64 + (n as u64 * 1000 / rate.max(1) as u64);
            self.busy_until.store(until, Ordering::Relaxed);
        }
        (n, false)
    }

    pub fn device_error(&self, text: &str) {
        if let Ok(mut e) = self.device_error.lock() {
            *e = text.to_string();
        }
    }

    pub fn take_device_error(&self) -> String {
        self.device_error.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default()
    }
}

// ---------------------------------------------------------------- the socket

/// Why a connection ended: the close code and reason of the server, or code 0 and the error.
#[derive(Clone, Debug, PartialEq)]
pub struct Closed {
    pub code: u16,
    pub reason: String,
}

/// One WebSocket connection.
pub trait Wire: Send {
    fn send(&mut self, text: &str) -> Result<(), String>;
    /// The next message, or None after a short wait (about 10 ms) without one.
    fn recv(&mut self) -> Result<Option<String>, Closed>;
    fn close(&mut self);
}

pub trait Dialer: Send {
    fn dial(&mut self, key: &str) -> Result<Box<dyn Wire>, String>;
}

/// A TCP stream whose read timeout looks like WouldBlock on every system (Windows says TimedOut), so tungstenite and
/// rustls keep their place and the next read goes on.
struct Sock(std::net::TcpStream);

impl std::io::Read for Sock {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self.0.read(b) {
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Err(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
            r => r,
        }
    }
}

impl std::io::Write for Sock {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.write(b)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

struct WsWire {
    ws: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<Sock>>,
}

fn quiet(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
}

impl Wire for WsWire {
    fn send(&mut self, text: &str) -> Result<(), String> {
        match self.ws.send(tungstenite::Message::text(text.to_string())) {
            Ok(()) => Ok(()),
            // the frame waits in the write buffer: flush it
            Err(tungstenite::Error::Io(e)) if quiet(&e) => loop {
                match self.ws.flush() {
                    Ok(()) => break Ok(()),
                    Err(tungstenite::Error::Io(e)) if quiet(&e) => std::thread::sleep(Duration::from_millis(2)),
                    Err(e) => break Err(e.to_string()),
                }
            },
            Err(e) => Err(e.to_string()),
        }
    }

    fn recv(&mut self) -> Result<Option<String>, Closed> {
        use tungstenite::Message;
        match self.ws.read() {
            Ok(Message::Text(t)) => Ok(Some(t.as_str().to_string())),
            Ok(Message::Binary(b)) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
            Ok(Message::Close(f)) => Err(match f {
                Some(f) => Closed { code: u16::from(f.code), reason: f.reason.as_str().to_string() },
                None => Closed { code: 1005, reason: String::new() },
            }),
            Ok(_) => Ok(None),
            Err(tungstenite::Error::Io(e)) if quiet(&e) => Ok(None),
            Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => Err(Closed { code: 1006, reason: "the connection closed".into() }),
            Err(e) => Err(Closed { code: 0, reason: e.to_string() }),
        }
    }

    fn close(&mut self) {
        let _ = self.ws.close(None);
        // a moment for the server's answer, so the session ends cleanly
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            match self.ws.read() {
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if quiet(&e) => {}
                Err(_) => break,
            }
        }
    }
}

/// The real connection: TCP, TLS (rustls, the certificates of webpki-roots, as ureq does) and the WebSocket handshake.
pub struct TlsDialer;

impl Dialer for TlsDialer {
    fn dial(&mut self, key: &str) -> Result<Box<dyn Wire>, String> {
        use std::net::ToSocketAddrs;
        let hide = |t: String| if key.is_empty() { t } else { t.replace(key, "***") };
        let addrs: Vec<std::net::SocketAddr> = (HOST, 443).to_socket_addrs().map_err(|e| format!("{} could not be found ({})", HOST, e))?.collect();
        let mut last = String::from("no address");
        let mut tcp = None;
        for a in &addrs {
            match std::net::TcpStream::connect_timeout(a, Duration::from_secs(10)) {
                Ok(t) => {
                    tcp = Some(t);
                    break;
                }
                Err(e) => last = e.to_string(),
            }
        }
        let tcp = tcp.ok_or_else(|| format!("no connection to {} ({})", HOST, last))?;
        let _ = tcp.set_nodelay(true);
        let _ = tcp.set_read_timeout(Some(Duration::from_secs(15)));
        let ctl = tcp.try_clone().map_err(|e| e.to_string())?;
        let url = format!("wss://{}{}?key={}", HOST, PATH, crate::util::url_encode(key));
        let (ws, _) = tungstenite::client_tls_with_config(url.as_str(), Sock(tcp), None, None).map_err(|e| {
            let t = match e {
                tungstenite::HandshakeError::Failure(tungstenite::Error::Http(r)) => {
                    let body = r.body().as_ref().map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default();
                    format!("the server answered {} ({})", r.status(), short(&body))
                }
                other => other.to_string(),
            };
            hide(t)
        })?;
        // from now on a read waits at most 10 ms, so the microphone keeps flowing up while nothing comes down
        let _ = ctl.set_read_timeout(Some(Duration::from_millis(10)));
        Ok(Box::new(WsWire { ws }))
    }
}

// ---------------------------------------------------------------- the call

pub enum Cmd {
    /// Text typed by the learner.
    Text(String),
    /// A note of the app (the Kaiwa coach); `respond`: the model answers it now.
    Note { text: String, respond: bool },
    /// Answers to tool calls: (id, name, response).
    Tools(Vec<(String, String, Json)>),
    /// Push-to-talk held (true) / released; open microphone on / off.
    Talk(bool),
    /// Push-to-talk or open microphone, headphones, the pause that ends a turn. A change of the first or the third
    /// reconnects (resuming the session), since the setup holds them.
    Mode { vad: Vad, headset: bool },
    Stop,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// "connecting", "live", "reconnecting"
    Phase(String),
    /// The setup was taken: the model, whether the old session goes on, milliseconds from dialling to setupComplete.
    Ready { model: String, resumed: bool, ms: u64 },
    InputText(String),
    OutputText(String),
    /// The first sound of a turn of the model, and how long after the learner stopped (when known).
    FirstAudio(Option<u64>),
    Interrupted,
    TurnComplete,
    ToolCall(Call),
    ToolCancel(Vec<String>),
    /// A line for the log (`[live] …`).
    Log(String),
    /// The call is over: None when asked to stop, else why.
    Closed(Option<String>),
}

/// The thread of a call. Dropping it stops the call.
pub struct Handle {
    tx: Sender<Cmd>,
}

impl Handle {
    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

pub type Sink = Box<dyn Fn(Event) + Send>;

/// Starts a call on its own thread, over the real connection.
pub fn start(cfg: Config, hub: Arc<Hub>, sink: Sink) -> Handle {
    start_with(cfg, hub, sink, Box::new(TlsDialer))
}

pub fn start_with(cfg: Config, hub: Arc<Hub>, sink: Sink, dialer: Box<dyn Dialer>) -> Handle {
    let (tx, rx) = mpsc::channel();
    let owner = hub.begin(cfg.vad.manual, cfg.headset);
    std::thread::Builder::new()
        .name("gemini-live".into())
        .spawn(move || {
            let mut run = Run::new(cfg, hub.clone(), sink);
            let end = run.main(dialer, &rx);
            hub.finish(owner);
            (run.sink)(Event::Closed(end.err()));
        })
        .ok();
    Handle { tx }
}

#[cfg(not(test))]
const BACKOFF_MS: [u64; 3] = [500, 1500, 3000];
#[cfg(test)]
const BACKOFF_MS: [u64; 3] = [1, 1, 1];
#[cfg(not(test))]
const SETUP_WAIT: Duration = Duration::from_secs(20);
#[cfg(test)]
const SETUP_WAIT: Duration = Duration::from_millis(400);

enum SetupEnd {
    Ready,
    Stop,
    Closed(Closed),
    Timeout,
}

enum TalkEnd {
    Stop,
    GoAway,
    Reconfigure,
    Dropped(Closed),
}

/// The key was refused: no point trying again.
fn key_refused(c: &Closed) -> bool {
    let r = c.reason.to_lowercase();
    r.contains("api key") || r.contains("api_key") || r.contains("permission denied") || r.contains("unauthenticated") || r.contains("the server answered 401") || r.contains("the server answered 403")
}

/// The setup carried something this model does not take (the language code, the transcription hints).
fn extras_refused(c: &Closed) -> bool {
    let r = c.reason.to_lowercase();
    r.contains("language") || r.contains("speech_config") || r.contains("speechconfig") || r.contains("transcription") || r.contains("unknown name")
}

/// The model is not offered (name, region, this method).
fn model_refused(c: &Closed) -> bool {
    let r = c.reason.to_lowercase();
    r.contains("not found") || r.contains("is not supported") || r.contains("not available") || (r.contains("model") && (r.contains("invalid") || r.contains("unsupported")))
}

struct Run {
    cfg: Config,
    hub: Arc<Hub>,
    sink: Sink,
    turn: u32,
    /// when (and how we know) the learner stopped talking, for the latency of the answer
    user_end: Option<(Instant, &'static str)>,
    tokens: i64,
}

impl Run {
    fn new(cfg: Config, hub: Arc<Hub>, sink: Sink) -> Run {
        Run { cfg, hub, sink, turn: 0, user_end: None, tokens: 0 }
    }

    fn log(&self, t: String) {
        crate::debug::log("live", &t);
        (self.sink)(Event::Log(t));
    }

    fn user_stopped(&mut self, how: &'static str) {
        let now = Instant::now();
        if self.user_end.map(|(t, _)| t <= now).unwrap_or(true) {
            self.user_end = Some((now, how));
        }
    }

    /// Dial, set up and talk, again after a drop or a goAway (resuming the session with its handle), until stopped.
    fn main(&mut self, mut dialer: Box<dyn Dialer>, rx: &Receiver<Cmd>) -> Result<(), String> {
        let mut models: Vec<String> = self.cfg.models.iter().filter(|m| !m.trim().is_empty()).cloned().collect();
        if models.is_empty() {
            models.push(DEFAULT_MODEL.to_string());
        }
        let (mut mi, mut extras, mut tries, mut opened) = (0usize, true, 0usize, false);
        let mut handle: Option<String> = None;
        let mut last_drop = String::new();
        loop {
            if tries > BACKOFF_MS.len() {
                return Err(format!("Gemini Live lost the connection and could not get it back ({}).", last_drop));
            }
            if tries > 0 {
                std::thread::sleep(Duration::from_millis(BACKOFF_MS[(tries - 1).min(BACKOFF_MS.len() - 1)]));
                if matches!(rx.try_recv(), Ok(Cmd::Stop) | Err(TryRecvError::Disconnected)) {
                    return Ok(());
                }
            }
            let model = models[mi].clone();
            (self.sink)(Event::Phase(if tries > 0 || opened { "reconnecting" } else { "connecting" }.into()));
            let t0 = Instant::now();
            let mut wire = match dialer.dial(&self.cfg.key) {
                Ok(w) => w,
                Err(e) => {
                    self.log(format!("connection failed: {}", e));
                    last_drop = e;
                    tries += 1;
                    continue;
                }
            };
            let dial_ms = t0.elapsed().as_millis();
            let setup = setup_message(&self.cfg, &model, handle.as_deref(), extras).dump();
            if let Err(e) = wire.send(&setup) {
                self.log(format!("the setup could not be sent: {}", e));
                last_drop = e;
                tries += 1;
                continue;
            }
            match self.await_setup(&mut *wire, rx) {
                SetupEnd::Ready => {}
                SetupEnd::Stop => {
                    wire.close();
                    return Ok(());
                }
                SetupEnd::Timeout => {
                    wire.close();
                    self.log(format!("{} did not answer the setup in {} s", model, SETUP_WAIT.as_secs()));
                    last_drop = "no answer to the setup".into();
                    tries += 1;
                    continue;
                }
                SetupEnd::Closed(c) => {
                    self.log(format!("{} refused the setup (close {}): {}", model, c.code, short(&c.reason)));
                    if key_refused(&c) {
                        return Err(format!("The Gemini key was refused ({}). Check it, or make a new free one at aistudio.google.com/apikey.", short(&c.reason)));
                    }
                    if handle.is_some() && (c.reason.to_lowercase().contains("handle") || c.reason.to_lowercase().contains("resum")) {
                        self.log("the session could not be resumed: starting a new one".into());
                        handle = None;
                        continue;
                    }
                    if extras && extras_refused(&c) {
                        self.log("trying again without speechConfig.languageCode and the transcription hints (this model picks the language itself)".into());
                        extras = false;
                        continue;
                    }
                    if model_refused(&c) && mi + 1 < models.len() {
                        mi += 1;
                        handle = None;
                        self.log(format!("trying the model {}", models[mi]));
                        continue;
                    }
                    last_drop = short(&c.reason);
                    tries += 1;
                    continue;
                }
            }
            let resumed = handle.is_some();
            let ms = t0.elapsed().as_millis() as u64;
            self.log(format!("{} ready in {} ms (connection {} ms){}; voice {}, {}", model, ms, dial_ms, if resumed { ", session resumed" } else { "" }, self.cfg.voice, if self.cfg.vad.manual { "push-to-talk".to_string() } else { format!("open microphone, {} ms pause ends a turn", self.cfg.vad.silence_ms) }));
            (self.sink)(Event::Ready { model: model.clone(), resumed, ms });
            (self.sink)(Event::Phase("live".into()));
            tries = 0;
            if !opened {
                opened = true;
                if !self.cfg.opening.is_empty() {
                    let _ = wire.send(&note_message(&self.cfg.opening, true));
                }
            }
            match self.talk(&mut *wire, rx, &mut handle) {
                TalkEnd::Stop => {
                    wire.close();
                    self.log(format!("call closed after {} turns ({} tokens)", self.turn, self.tokens));
                    return Ok(());
                }
                TalkEnd::GoAway => {
                    wire.close();
                    self.log(format!("the server asked to reconnect (goAway): {}", if handle.is_some() { "resuming the session" } else { "no handle, a new session" }));
                }
                TalkEnd::Reconfigure => {
                    wire.close();
                    self.log("reconnecting with the new microphone settings".into());
                }
                TalkEnd::Dropped(c) => {
                    self.log(format!("connection dropped (close {}): {}", c.code, short(&c.reason)));
                    if key_refused(&c) {
                        return Err(format!("The Gemini key was refused ({}).", short(&c.reason)));
                    }
                    last_drop = if c.reason.is_empty() { format!("close {}", c.code) } else { short(&c.reason) };
                    tries += 1;
                }
            }
        }
    }

    fn await_setup(&mut self, wire: &mut dyn Wire, rx: &Receiver<Cmd>) -> SetupEnd {
        let until = Instant::now() + SETUP_WAIT;
        loop {
            match rx.try_recv() {
                Ok(Cmd::Stop) | Err(TryRecvError::Disconnected) => return SetupEnd::Stop,
                // the rest waits for the call (typed text, notes, the button: they make no sense before it)
                _ => {}
            }
            match wire.recv() {
                Ok(Some(t)) => {
                    for m in parse(&t) {
                        match m {
                            Server::SetupComplete => return SetupEnd::Ready,
                            Server::Error(e) => self.log(format!("error before the setup: {}", e)),
                            _ => {}
                        }
                    }
                }
                Ok(None) => {}
                Err(c) => return SetupEnd::Closed(c),
            }
            if Instant::now() >= until {
                return SetupEnd::Timeout;
            }
        }
    }

    fn talk(&mut self, wire: &mut dyn Wire, rx: &Receiver<Cmd>, handle: &mut Option<String>) -> TalkEnd {
        let mut in_turn = false;
        let mut go_away: Option<Instant> = None;
        let started = Instant::now();
        let mut mic_checked = false;
        macro_rules! up {
            ($m:expr) => {
                if let Err(e) = wire.send(&$m) {
                    return TalkEnd::Dropped(Closed { code: 0, reason: e });
                }
            };
        }
        loop {
            loop {
                match rx.try_recv() {
                    Ok(Cmd::Stop) | Err(TryRecvError::Disconnected) => return TalkEnd::Stop,
                    Err(TryRecvError::Empty) => break,
                    Ok(Cmd::Text(t)) => {
                        self.user_stopped("text sent");
                        up!(text_message(&t));
                    }
                    Ok(Cmd::Note { text, respond }) => up!(note_message(&text, respond)),
                    Ok(Cmd::Tools(r)) => up!(tool_message(&r)),
                    Ok(Cmd::Talk(on)) => {
                        if self.cfg.vad.manual {
                            if on {
                                // pressing to talk stops the model's voice (the server interrupts it too)
                                self.hub.flush_speaker();
                                self.hub.set_open(true);
                                up!(activity_message(true));
                            } else {
                                self.hub.set_open(false);
                                if let Some(pcm) = self.hub.take_mic(0) {
                                    up!(audio_message(&pcm));
                                }
                                up!(activity_message(false));
                                self.user_stopped("button released");
                            }
                        } else {
                            self.hub.set_open(on);
                            if !on {
                                if let Some(pcm) = self.hub.take_mic(0) {
                                    up!(audio_message(&pcm));
                                }
                                up!(stream_end_message());
                            }
                        }
                    }
                    Ok(Cmd::Mode { vad, headset }) => {
                        self.hub.set_mode(vad.manual, headset);
                        self.cfg.headset = headset;
                        if vad != self.cfg.vad {
                            self.cfg.vad = vad;
                            return TalkEnd::Reconfigure;
                        }
                    }
                }
            }
            while let Some(pcm) = self.hub.take_mic(MIC_PIECE) {
                up!(audio_message(&pcm));
            }
            match wire.recv() {
                Ok(None) => {}
                Ok(Some(text)) => {
                    for m in parse(&text) {
                        match m {
                            Server::SetupComplete | Server::GenerationComplete | Server::WaitingForInput | Server::VoiceStart => {}
                            Server::InputText(t) => {
                                self.user_stopped("last words heard");
                                (self.sink)(Event::InputText(t));
                            }
                            Server::Audio(pcm, rate) => {
                                let mut first = None;
                                if !in_turn {
                                    in_turn = true;
                                    self.turn += 1;
                                    let voice = self.hub.instant_of(self.hub.last_voice_ms());
                                    let mut end = self.user_end.take();
                                    if !self.cfg.vad.manual && self.hub.last_voice_ms() > 0 && end.map(|(t, _)| voice > t).unwrap_or(true) {
                                        end = Some((voice, "microphone went quiet"));
                                    }
                                    let lat = end.map(|(t, _)| Instant::now().saturating_duration_since(t).as_millis() as u64);
                                    match (lat, end) {
                                        (Some(ms), Some((_, how))) => self.log(format!("turn {}: first audio {} ms after the learner stopped ({})", self.turn, ms, how)),
                                        _ => self.log(format!("turn {}: first audio (the model spoke first)", self.turn)),
                                    }
                                    first = Some(lat);
                                }
                                if rate == OUT_RATE {
                                    self.hub.push_speaker(&pcm);
                                } else {
                                    let mut v = vec![];
                                    Resampler::new(rate, OUT_RATE).push(&pcm, &mut v);
                                    self.hub.push_speaker(&v);
                                }
                                // told once the voice is queued: whoever hears of it finds it there
                                if let Some(lat) = first {
                                    (self.sink)(Event::FirstAudio(lat));
                                }
                            }
                            Server::OutputText(t) => {
                                in_turn = true;
                                (self.sink)(Event::OutputText(t));
                            }
                            Server::Interrupted => {
                                self.hub.flush_speaker();
                                if in_turn {
                                    self.log(format!("turn {}: the learner interrupted the model", self.turn));
                                }
                                in_turn = false;
                                (self.sink)(Event::Interrupted);
                            }
                            Server::TurnComplete => {
                                in_turn = false;
                                self.user_end = None;
                                (self.sink)(Event::TurnComplete);
                            }
                            Server::ToolCall(calls) => {
                                for c in calls {
                                    self.log(format!("tool call {}({})", c.name, short(&c.args.dump())));
                                    (self.sink)(Event::ToolCall(c));
                                }
                            }
                            Server::ToolCancel(ids) => {
                                self.log(format!("tool calls cancelled: {}", ids.join(", ")));
                                (self.sink)(Event::ToolCancel(ids));
                            }
                            Server::GoAway(secs) => {
                                self.log(format!("goAway: the server closes this connection in {:.1} s", secs));
                                go_away = Some(Instant::now() + Duration::from_secs_f64((secs - 2.0).max(0.0)));
                            }
                            Server::Resume(h) => *handle = Some(h),
                            Server::VoiceEnd => self.user_stopped("voice activity ended"),
                            Server::Usage(n) => self.tokens = n.max(self.tokens),
                            Server::Error(e) => self.log(format!("server error: {}", e)),
                        }
                    }
                }
                Err(c) => return TalkEnd::Dropped(c),
            }
            if let Some(at) = go_away {
                // between two turns of the model, or at the last moment
                if !in_turn || Instant::now() >= at {
                    return TalkEnd::GoAway;
                }
            }
            if !mic_checked && started.elapsed() > Duration::from_secs(5) && self.hub.wanted() {
                mic_checked = true;
                let (frames, loud) = self.hub.mic_stats();
                if self.hub.present() && frames == 0 {
                    self.log("no sound came from the microphone in 5 s (no device, or it did not start)".into());
                } else if frames > 0 && !loud {
                    self.log("the microphone sends only silence (muted, or blocked in the system's privacy settings?)".into());
                }
            }
        }
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
pub mod mock {
    //! A scripted socket for the tests: each connection gets its own list of what the server says.
    use super::*;

    pub type Script = VecDeque<Result<Option<String>, Closed>>;

    pub struct MockWire {
        pub script: Script,
        pub sent: Arc<Mutex<Vec<String>>>,
        /// after the script: wait for these messages before saying more (for tests that answer the client)
        pub feed: Option<Receiver<String>>,
    }

    impl Wire for MockWire {
        fn send(&mut self, text: &str) -> Result<(), String> {
            self.sent.lock().unwrap().push(text.to_string());
            Ok(())
        }
        fn recv(&mut self) -> Result<Option<String>, Closed> {
            if let Some(next) = self.script.pop_front() {
                return next;
            }
            if let Some(f) = &self.feed {
                if let Ok(t) = f.try_recv() {
                    return Ok(Some(t));
                }
            }
            std::thread::sleep(Duration::from_millis(1));
            Ok(None)
        }
        fn close(&mut self) {
            self.sent.lock().unwrap().push("<close>".into());
        }
    }

    pub struct MockDialer {
        pub scripts: VecDeque<Script>,
        pub sent: Arc<Mutex<Vec<String>>>,
        pub dials: Arc<Mutex<u32>>,
        pub feed: Option<Receiver<String>>,
    }

    impl Dialer for MockDialer {
        fn dial(&mut self, _key: &str) -> Result<Box<dyn Wire>, String> {
            *self.dials.lock().unwrap() += 1;
            let script = self.scripts.pop_front().ok_or_else(|| "no more connections".to_string())?;
            let feed = if self.scripts.is_empty() { self.feed.take() } else { None };
            self.sent.lock().unwrap().push("<dial>".into());
            Ok(Box::new(MockWire { script, sent: self.sent.clone(), feed }))
        }
    }

    pub fn ok(t: &str) -> Result<Option<String>, Closed> {
        Ok(Some(t.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::mock::*;
    use super::*;

    fn cfg() -> Config {
        Config {
            key: "k".into(),
            models: vec![DEFAULT_MODEL.into()],
            voice: "Kore".into(),
            language_code: "ja-JP".into(),
            hear_langs: vec!["ja-JP".into(), "pt-BR".into()],
            system: "be a tutor".into(),
            tools: Json::Arr(vec![Json::obj(vec![("name", "give_hint".into())])]),
            vad: Vad::default(),
            opening: "start".into(),
            headset: false,
        }
    }

    struct Rig {
        events: Receiver<Event>,
        sent: Arc<Mutex<Vec<String>>>,
        dials: Arc<Mutex<u32>>,
        handle: Handle,
        hub: Arc<Hub>,
    }

    fn rig(c: Config, scripts: Vec<Vec<Result<Option<String>, Closed>>>, feed: Option<Receiver<String>>) -> Rig {
        let (etx, events) = mpsc::channel();
        let etx = Mutex::new(etx);
        let sent = Arc::new(Mutex::new(vec![]));
        let dials = Arc::new(Mutex::new(0));
        let hub = Arc::new(Hub::new());
        let dialer = MockDialer { scripts: scripts.into_iter().map(VecDeque::from).collect(), sent: sent.clone(), dials: dials.clone(), feed };
        let handle = start_with(c, hub.clone(), Box::new(move |e| { let _ = etx.lock().unwrap().send(e); }), Box::new(dialer));
        Rig { events, sent, dials, handle, hub }
    }

    /// Events until `stop` matches one (or 3 s pass).
    fn until(r: &Rig, stop: impl Fn(&Event) -> bool) -> Vec<Event> {
        let mut got = vec![];
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end {
            if let Ok(e) = r.events.recv_timeout(Duration::from_millis(20)) {
                let done = stop(&e);
                got.push(e);
                if done {
                    break;
                }
            }
        }
        got
    }

    fn sent(r: &Rig) -> Vec<String> {
        r.sent.lock().unwrap().clone()
    }

    fn audio_b64(pcm: &[i16]) -> String {
        b64_encode(&pcm_to_le(pcm))
    }

    #[test]
    fn base64_and_pcm_round_trip() {
        for data in [&b""[..], b"a", b"ab", b"abc", b"\x00\xff\x10\x80"] {
            assert_eq!(b64_decode(&b64_encode(data)), data);
        }
        assert_eq!(b64_encode(b"user:pass"), crate::suwayomi::base64(b"user:pass"));
        assert_eq!(b64_decode("_-8"), vec![0xff, 0xef]);
        assert_eq!(b64_decode("/+8="), vec![0xff, 0xef]);
        let pcm = vec![0i16, 1, -1, 32767, -32768];
        assert_eq!(pcm_from_le(&pcm_to_le(&pcm)), pcm);
    }

    #[test]
    fn resampling_keeps_the_length_in_proportion() {
        let input: Vec<i16> = (0..4800).map(|i| ((i as f32 / 10.0).sin() * 8000.0) as i16).collect();
        let mut r = Resampler::new(48_000, 16_000);
        let mut out = vec![];
        for piece in input.chunks(441) {
            r.push(piece, &mut out);
        }
        assert!((out.len() as i64 - 1600).abs() <= 2, "{}", out.len());
        let mut up = Resampler::new(24_000, 48_000);
        let mut o2 = vec![];
        up.push(&[0, 100, 200, 300], &mut o2);
        assert_eq!(o2[..5], [0, 50, 100, 150, 200]);
        let mut same = Resampler::new(16_000, 16_000);
        let mut o3 = vec![];
        same.push(&[1, 2, 3], &mut o3);
        assert_eq!(o3, vec![1, 2, 3]);
    }

    #[test]
    fn the_setup_carries_the_kaiwa_layer() {
        let c = cfg();
        let j = setup_message(&c, DEFAULT_MODEL, None, true);
        let s = j.get("setup");
        assert_eq!(s.get("model").str(), "models/gemini-3.8-live");
        assert_eq!(s.path("generationConfig.responseModalities.0").str(), "AUDIO");
        assert_eq!(s.path("generationConfig.speechConfig.voiceConfig.prebuiltVoiceConfig.voiceName").str(), "Kore");
        assert_eq!(s.path("generationConfig.speechConfig.languageCode").str(), "ja-JP");
        assert_eq!(s.path("systemInstruction.parts.0.text").str(), "be a tutor");
        assert_eq!(s.path("tools.0.functionDeclarations.0.name").str(), "give_hint");
        assert_eq!(s.path("inputAudioTranscription.languageCodes").strings(), vec!["ja-JP", "pt-BR"]);
        assert!(matches!(s.get("outputAudioTranscription"), Json::Obj(v) if v.is_empty()));
        let vad = s.path("realtimeInputConfig.automaticActivityDetection");
        assert_eq!(vad.get("disabled").as_bool(), Some(false));
        assert_eq!(vad.get("silenceDurationMs").as_i64(), Some(1200));
        assert_eq!(vad.get("endOfSpeechSensitivity").str(), "END_SENSITIVITY_LOW");
        assert_eq!(s.path("realtimeInputConfig.activityHandling").str(), "START_OF_ACTIVITY_INTERRUPTS");
        assert!(!s.path("contextWindowCompression.slidingWindow").is_null());
        assert!(matches!(s.get("sessionResumption"), Json::Obj(v) if v.is_empty()));
        // resumed, without the extras, push-to-talk
        let mut m = cfg();
        m.vad.manual = true;
        let j = setup_message(&m, "models/x", Some("h1"), false);
        let s = j.get("setup");
        assert_eq!(s.get("model").str(), "models/x");
        assert_eq!(s.path("sessionResumption.handle").str(), "h1");
        assert!(s.path("generationConfig.speechConfig.languageCode").is_null());
        assert!(s.path("inputAudioTranscription.languageCodes").is_null());
        assert_eq!(s.path("realtimeInputConfig.automaticActivityDetection.disabled").as_bool(), Some(true));
        // the other messages
        let a = Json::parse(&audio_message(&[1, -1])).unwrap();
        assert_eq!(a.path("realtimeInput.audio.mimeType").str(), "audio/pcm;rate=16000");
        assert_eq!(pcm_from_le(&b64_decode(&a.path("realtimeInput.audio.data").str())), vec![1, -1]);
        assert_eq!(Json::parse(&text_message("やあ")).unwrap().path("realtimeInput.text").str(), "やあ");
        assert!(!Json::parse(&activity_message(true)).unwrap().path("realtimeInput.activityStart").is_null());
        assert!(!Json::parse(&activity_message(false)).unwrap().path("realtimeInput.activityEnd").is_null());
        let n = Json::parse(&note_message("hi", false)).unwrap();
        assert_eq!(n.path("clientContent.turns.0.parts.0.text").str(), "hi");
        assert_eq!(n.path("clientContent.turnComplete").as_bool(), Some(false));
        let t = Json::parse(&tool_message(&[("c1".into(), "give_hint".into(), Json::obj(vec![("result", "ok".into())]))])).unwrap();
        assert_eq!(t.path("toolResponse.functionResponses.0.id").str(), "c1");
        assert_eq!(t.path("toolResponse.functionResponses.0.response.result").str(), "ok");
    }

    #[test]
    fn server_messages_are_read() {
        let audio = audio_b64(&[5, 6, 7]);
        let m = format!(
            r#"{{"serverContent":{{"inputTranscription":{{"text":"すみません"}},"modelTurn":{{"parts":[{{"thought":true,"text":"hmm"}},{{"inlineData":{{"mimeType":"audio/pcm;rate=24000","data":"{}"}}}}]}},"outputTranscription":{{"text":"はい"}},"turnComplete":true}},"usageMetadata":{{"totalTokenCount":42}}}}"#,
            audio
        );
        assert_eq!(
            parse(&m),
            vec![Server::InputText("すみません".into()), Server::Audio(vec![5, 6, 7], 24_000), Server::OutputText("はい".into()), Server::TurnComplete, Server::Usage(42)]
        );
        let t = parse(r#"{"toolCall":{"functionCalls":[{"id":"a1","name":"mark_goal_done","args":{"goal":2}}]}}"#);
        assert_eq!(t, vec![Server::ToolCall(vec![Call { id: "a1".into(), name: "mark_goal_done".into(), args: Json::parse(r#"{"goal":2}"#).unwrap() }])]);
        assert_eq!(parse(r#"{"goAway":{"timeLeft":"9.5s"}}"#), vec![Server::GoAway(9.5)]);
        assert_eq!(parse(r#"{"sessionResumptionUpdate":{"newHandle":"h","resumable":true}}"#), vec![Server::Resume("h".into())]);
        assert!(parse(r#"{"sessionResumptionUpdate":{"newHandle":"h","resumable":false}}"#).is_empty());
        assert_eq!(parse(r#"{"serverContent":{"interrupted":true}}"#), vec![Server::Interrupted]);
        assert_eq!(parse(r#"{"toolCallCancellation":{"ids":["a1"]}}"#), vec![Server::ToolCancel(vec!["a1".into()])]);
        assert_eq!(parse(r#"{"setupComplete":{}}"#), vec![Server::SetupComplete]);
        assert_eq!(parse(r#"{"voiceActivity":{"type":"ACTIVITY_END"}}"#), vec![Server::VoiceEnd]);
        assert!(matches!(parse("not json")[0], Server::Error(_)));
        assert_eq!(duration_secs(&Json::parse(r#"{"seconds":3,"nanos":500000000}"#).unwrap()), 3.5);
    }

    #[test]
    fn the_hub_guards_against_echo_and_follows_push_to_talk() {
        let h = Hub::new();
        // nothing is gathered before a call
        h.mic(&[1000; 320], 16_000);
        assert!(h.take_mic(0).is_none());
        let owner = h.begin(false, false);
        h.mic(&[1000; 320], 16_000);
        assert_eq!(h.take_mic(0).unwrap(), vec![1000; 320]);
        assert!(h.level() > 0);
        // the model speaks: the open microphone sends silence
        h.push_speaker(&[9; 2400]);
        assert!(h.speaking() && h.muted());
        h.mic(&[1000; 320], 16_000);
        assert_eq!(h.take_mic(0).unwrap(), vec![0; 320]);
        // with headphones the voice goes up (the learner can interrupt by voice)
        h.set_mode(false, true);
        h.mic(&[1000; 320], 16_000);
        assert_eq!(h.take_mic(0).unwrap(), vec![1000; 320]);
        // the speaker side: 24 kHz pulled at 48 kHz, then a flush
        let mut out = vec![0i16; 960];
        let (n, flush) = h.speaker(&mut out, 48_000, 0);
        assert!(!flush && n > 900, "{}", n);
        h.flush_speaker();
        assert_eq!(h.speaker(&mut out, 48_000, 0), (0, true));
        assert_eq!(h.queued(), 0);
        // push-to-talk: nothing until held
        h.set_mode(true, false);
        h.mic(&[1000; 320], 16_000);
        assert!(h.take_mic(0).is_none());
        h.set_open(true);
        h.mic(&[1000; 640], 32_000);
        assert_eq!(h.take_mic(0).unwrap().len(), 320);
        // pieces wait until there is enough
        h.mic(&[1; 100], 16_000);
        assert!(h.take_mic(MIC_PIECE).is_none());
        // a newer call owns the hub: the old one's finish changes nothing
        let newer = h.begin(false, false);
        h.finish(owner);
        assert!(h.wanted());
        h.finish(newer);
        assert!(!h.wanted());
    }

    #[test]
    fn a_call_sets_up_speaks_and_hears() {
        let audio = audio_b64(&[1, 2, 3]);
        let script = vec![
            ok(r#"{"setupComplete":{}}"#),
            ok(r#"{"serverContent":{"inputTranscription":{"text":"こんにちは"}}}"#),
            ok(&format!(r#"{{"serverContent":{{"modelTurn":{{"parts":[{{"inlineData":{{"mimeType":"audio/pcm;rate=24000","data":"{}"}}}}]}}}}}}"#, audio)),
            ok(r#"{"serverContent":{"outputTranscription":{"text":"いらっしゃい"}}}"#),
            ok(r#"{"serverContent":{"turnComplete":true}}"#),
        ];
        let r = rig(cfg(), vec![script], None);
        let ev = until(&r, |e| *e == Event::TurnComplete);
        assert!(ev.iter().any(|e| matches!(e, Event::Ready { resumed: false, .. })));
        assert!(ev.contains(&Event::InputText("こんにちは".into())));
        assert!(ev.contains(&Event::OutputText("いらっしゃい".into())));
        assert!(ev.iter().any(|e| matches!(e, Event::FirstAudio(Some(_)))));
        assert_eq!(r.hub.queued(), 3);
        let s = sent(&r);
        assert_eq!(s[0], "<dial>");
        assert!(s[1].contains("\"setup\"") && s[1].contains("ja-JP"));
        assert!(s[2].contains("clientContent") && s[2].contains("start"));
        // the microphone goes up in pieces
        r.hub.mic(&vec![500i16; MIC_PIECE], IN_RATE);
        let end = Instant::now() + Duration::from_secs(2);
        while !sent(&r).iter().any(|m| m.contains("audio/pcm;rate=16000")) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(sent(&r).iter().any(|m| m.contains("audio/pcm;rate=16000")));
        r.handle.send(Cmd::Stop);
        let ev = until(&r, |e| matches!(e, Event::Closed(_)));
        assert_eq!(ev.last(), Some(&Event::Closed(None)));
        assert!(sent(&r).contains(&"<close>".to_string()));
        assert!(!r.hub.wanted());
    }

    #[test]
    fn tool_calls_go_out_and_answers_come_back() {
        let script = vec![ok(r#"{"setupComplete":{}}"#), ok(r#"{"toolCall":{"functionCalls":[{"id":"c7","name":"give_hint","args":{"text":"ラーメンをください"}}]}}"#)];
        let r = rig(cfg(), vec![script], None);
        let ev = until(&r, |e| matches!(e, Event::ToolCall(_)));
        let Some(Event::ToolCall(c)) = ev.last() else { panic!("{:?}", ev) };
        assert_eq!((c.id.as_str(), c.name.as_str(), c.args.get("text").str()), ("c7", "give_hint", "ラーメンをください".to_string()));
        r.handle.send(Cmd::Tools(vec![(c.id.clone(), c.name.clone(), Json::obj(vec![("result", "shown".into())]))]));
        r.handle.send(Cmd::Note { text: "[Kaiwa coach] shorter".into(), respond: false });
        r.handle.send(Cmd::Text("ラーメン".into()));
        r.handle.send(Cmd::Stop);
        until(&r, |e| matches!(e, Event::Closed(_)));
        let s = sent(&r);
        assert!(s.iter().any(|m| m.contains("toolResponse") && m.contains("c7") && m.contains("shown")));
        assert!(s.iter().any(|m| m.contains("clientContent") && m.contains("shorter") && m.contains("\"turnComplete\":false")));
        assert!(s.iter().any(|m| m.contains("\"realtimeInput\":{\"text\":\"ラーメン\"}")));
    }

    #[test]
    fn push_to_talk_marks_the_turns() {
        let mut c = cfg();
        c.vad.manual = true;
        let r = rig(c, vec![vec![ok(r#"{"setupComplete":{}}"#)]], None);
        until(&r, |e| matches!(e, Event::Ready { .. }));
        r.hub.push_speaker(&[1; 100]);
        r.handle.send(Cmd::Talk(true));
        let end = Instant::now() + Duration::from_secs(2);
        while !sent(&r).iter().any(|m| m.contains("activityStart")) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
        // pressing stopped the model's voice
        assert_eq!(r.hub.queued(), 0);
        r.hub.mic(&[700; 200], IN_RATE);
        r.handle.send(Cmd::Talk(false));
        r.handle.send(Cmd::Stop);
        until(&r, |e| matches!(e, Event::Closed(_)));
        let s = sent(&r);
        let a = s.iter().position(|m| m.contains("activityStart")).unwrap();
        let au = s.iter().position(|m| m.contains("audio/pcm")).unwrap();
        let b = s.iter().position(|m| m.contains("activityEnd")).unwrap();
        assert!(a < au && au < b, "{:?}", s);
    }

    #[test]
    fn a_refused_language_code_is_dropped_and_the_setup_tried_again() {
        let refused = Err(Closed { code: 1007, reason: "Invalid JSON payload received. Unknown name \"languageCode\" at 'setup.generation_config.speech_config'".into() });
        let r = rig(cfg(), vec![vec![refused], vec![ok(r#"{"setupComplete":{}}"#)]], None);
        let ev = until(&r, |e| matches!(e, Event::Ready { .. }));
        assert!(ev.iter().any(|e| matches!(e, Event::Log(t) if t.contains("without speechConfig.languageCode"))));
        let setups: Vec<String> = sent(&r).into_iter().filter(|m| m.contains("\"setup\"")).collect();
        assert_eq!(setups.len(), 2);
        assert!(setups[0].contains("languageCode") && !setups[1].contains("languageCode"));
        drop(r.handle);
    }

    #[test]
    fn go_away_resumes_the_session_with_its_handle() {
        let first = vec![
            ok(r#"{"setupComplete":{}}"#),
            ok(r#"{"sessionResumptionUpdate":{"newHandle":"resume-me","resumable":true}}"#),
            ok(r#"{"goAway":{"timeLeft":"0s"}}"#),
        ];
        let r = rig(cfg(), vec![first, vec![ok(r#"{"setupComplete":{}}"#)]], None);
        let ev = until(&r, |e| matches!(e, Event::Ready { resumed: true, .. }));
        assert!(matches!(ev.last(), Some(Event::Ready { resumed: true, .. })), "{:?}", ev);
        let s = sent(&r);
        let setups: Vec<&String> = s.iter().filter(|m| m.contains("\"setup\"")).collect();
        assert!(setups[1].contains("resume-me"));
        // the opening is said once, not again after the reconnection
        assert_eq!(s.iter().filter(|m| m.contains("clientContent")).count(), 1);
        assert_eq!(*r.dials.lock().unwrap(), 2);
    }

    #[test]
    fn a_refused_key_ends_the_call_without_retrying() {
        let r = rig(cfg(), vec![vec![Err(Closed { code: 1008, reason: "API key not valid. Please pass a valid API key.".into() })], vec![]], None);
        let ev = until(&r, |e| matches!(e, Event::Closed(_)));
        assert!(matches!(ev.last(), Some(Event::Closed(Some(t))) if t.contains("key was refused") && t.contains("aistudio")));
        assert_eq!(*r.dials.lock().unwrap(), 1);
    }

    #[test]
    fn a_drop_reconnects_and_a_refused_model_falls_back() {
        let mut c = cfg();
        c.models = vec![DEFAULT_MODEL.into(), FALLBACK_MODELS[0].into()];
        let refused = Err(Closed { code: 1008, reason: "models/gemini-3.8-live is not found for API version v1beta, or is not supported for bidiGenerateContent".into() });
        let dropped = vec![ok(r#"{"setupComplete":{}}"#), Err(Closed { code: 1011, reason: "Internal error".into() })];
        let r = rig(c, vec![vec![refused], dropped, vec![ok(r#"{"setupComplete":{}}"#)]], None);
        let ev = until(&r, |e| matches!(e, Event::Phase(p) if p == "reconnecting"));
        assert!(ev.iter().any(|e| matches!(e, Event::Ready { model, .. } if model == FALLBACK_MODELS[0])), "{:?}", ev);
        let ev = until(&r, |e| matches!(e, Event::Ready { .. }));
        assert!(matches!(ev.last(), Some(Event::Ready { resumed: false, .. })));
        assert_eq!(*r.dials.lock().unwrap(), 3);
    }

    #[test]
    fn interruptions_stop_the_voice_at_once() {
        let audio = audio_b64(&[3; 480]);
        let script = vec![
            ok(r#"{"setupComplete":{}}"#),
            ok(&format!(r#"{{"serverContent":{{"modelTurn":{{"parts":[{{"inlineData":{{"mimeType":"audio/pcm;rate=24000","data":"{}"}}}}]}}}}}}"#, audio)),
        ];
        let (ftx, frx) = mpsc::channel();
        let r = rig(cfg(), vec![script], Some(frx));
        until(&r, |e| matches!(e, Event::FirstAudio(_)));
        assert_eq!(r.hub.queued(), 480);
        ftx.send(r#"{"serverContent":{"interrupted":true}}"#.to_string()).unwrap();
        until(&r, |e| *e == Event::Interrupted);
        assert_eq!(r.hub.queued(), 0);
        let mut out = [0i16; 10];
        assert!(r.hub.speaker(&mut out, 24_000, 0).1, "the device is told to drop what it holds");
    }
}
