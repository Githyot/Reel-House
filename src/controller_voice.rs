//! The voice of the Study mode, run (voice.rs decides): hearing a recording with the engines of the plan in order
//! (Gemini / OpenAI's Whisper / Whisper on this computer, the next one when one fails), reading a text aloud with
//! Google's voice or, when it cannot be reached, a voice of the system (spoken by the page with Qt TextToSpeech),
//! and Whisper on this computer (`whisper_local`). Shared by Conversation and anything else that listens or speaks:
//! `voice_hear` and `voice_say` take the language and hand the result back as the caller's own message.
//! Every step writes a `[voice]` line: in the debug report, and through the page into reelhouse-qt.log.
use super::*;
use crate::http::Request;
use crate::llm;
use crate::voice::{self, Stt, Tts};
use std::time::{Duration, Instant};

pub enum VoiceMsg {
    /// Whisper on this computer changed its status
    Whisper,
    /// what was heard: the engine (shown), how long it took, the text, a note about the engines that failed first
    Heard { engine: String, ms: u64, text: String, note: String },
    /// a piece of the online voice is on disk: generation, index, file
    Piece(u64, usize, PathBuf),
    /// the online voice could not be fetched: generation, the text for the system voice ("" = no fallback),
    /// language, why
    OnlineFailed(u64, String, String, String),
    /// a line for the logs
    Log(String),
    /// what the microphone test of the page heard
    Test(Result<String, String>),
    /// what was measured in the last recording and what the transcriber gave (`measure_json`), sent before the
    /// caller's own message
    Measure(Json),
}

#[derive(Default)]
pub struct VoiceState {
    /// what was heard last: {seq, engine, ms, text, note, kanji}
    pub last: Json,
    last_seq: u64,
    /// what the page reads with a system voice: {seq, text, lang, voice}; an empty text stops it
    say: Json,
    say_seq: u64,
    /// the online voice being fetched (the pieces of an older text are dropped)
    tts_gen: u64,
    /// the voices of the system as the page found them: {available, engine, voices: [{name, locale}]}
    pub local: Json,
    /// the microphone test of the page: {seq, busy} while it is written down, then {seq, text} or {seq, error}
    test: Json,
    test_seq: u64,
    /// the latest `[voice]` lines with their number (the page writes the new ones to its log)
    log: Vec<(u64, String)>,
    log_seq: u64,
    /// the last recording heard: its level, length and what the transcriber gave (`measure_json`)
    pub measure: Json,
}

/// The last recording, for the log, the microphone test and Conversation's "nothing heard": the level the microphone
/// gave (before the app raised it), the level and length sent, the transcriber and its raw answer.
pub fn measure_json(info: Option<&crate::vad::WavInfo>, engine: &str, raw: &str, ms: u64) -> Json {
    let n = |v: f32| Json::Num((v as f64 * 10.0).round() / 10.0);
    let mut o = vec![("engine", Json::Str(engine.into())), ("raw", Json::Str(raw.chars().take(200).collect())), ("ms", Json::Num(ms as f64))];
    if let Some(i) = info {
        o.push(("secs", n(i.seconds)));
        o.push(("peakDb", n(i.peak_db)));
        o.push(("rmsDb", n(i.rms_db)));
        o.push(("rate", Json::Num(i.rate as f64)));
        o.push(("channels", Json::Num(i.channels as f64)));
        if let Some(m) = i.measure {
            o.push(("micPeakDb", n(m.peak_db)));
            o.push(("micRmsDb", n(m.rms_db)));
            o.push(("gain", n(m.gain)));
            o.push(("speech", Json::Bool(m.speech)));
        }
    }
    Json::obj(o)
}

/// One line about a measure, the same in every language: "mic peak -48 dBFS, RMS -61 dBFS, raised x31 · sent 2.9 s,
/// peak -3 dBFS · gemini: (nothing)".
pub fn measure_note(m: &Json) -> String {
    let f = |k: &str| m.get(k).as_f64();
    let mut parts: Vec<String> = vec![];
    if let (Some(p), Some(r)) = (f("micPeakDb"), f("micRmsDb")) {
        let mut t = format!("mic peak {:.0} dBFS, RMS {:.0} dBFS", p, r);
        if let Some(g) = f("gain").filter(|g| *g > 1.05) {
            t.push_str(&format!(", raised x{:.0}", g));
        }
        if m.get("speech").as_bool() == Some(false) {
            t.push_str(", no speech found by the detector");
        }
        parts.push(t);
    }
    if let Some(s) = f("secs") {
        parts.push(format!("sent {:.1} s, peak {:.0} dBFS, RMS {:.0} dBFS", s, f("peakDb").unwrap_or(-100.0), f("rmsDb").unwrap_or(-100.0)));
    }
    let e = m.get("engine").str();
    if !e.is_empty() {
        let raw = m.get("raw").str();
        parts.push(format!("{}: {}", e, if raw.trim().is_empty() { "(nothing)".to_string() } else { format!("\"{}\"", raw.trim()) }));
    }
    parts.join(" · ")
}

/// Why nothing came out of a recording, from its measure (a sentence of the interface, translated by the page).
pub fn nothing_heard_why(m: &Json) -> &'static str {
    let mic = m.get("micPeakDb").as_f64().or(m.get("peakDb").as_f64()).unwrap_or(0.0);
    if mic < -50.0 {
        return "The microphone gave almost no sound: it may be the wrong microphone, or turned down. Choose another one or raise its volume in Settings > Study > Microphone.";
    }
    if m.get("secs").as_f64().map(|s| s < 0.6).unwrap_or(false) {
        return "The recording was too short. Hold the button a little longer while you speak.";
    }
    if m.get("speech").as_bool() == Some(false) {
        return "Sound came in, but no voice was found in it. Speak closer to the microphone, or choose another one in Settings > Study > Microphone.";
    }
    "The voice reached the transcriber, but it wrote nothing. Speak in the language you study, or choose another voice input in Conversation > Voice."
}

/// A browser's name: Google's voice answers it.
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0 Safari/537.36";

/// Whether this system may run Whisper itself.
fn local_ok() -> bool {
    crate::platform::DESKTOP_AI && crate::platform::local_processes_allowed()
}

impl Core {
    /// A `[voice]` line.
    pub(super) fn voice_log(&mut self, line: &str) {
        crate::debug::log("voice", line);
        self.voice.log_seq += 1;
        let seq = self.voice.log_seq;
        self.voice.log.push((seq, format!("[voice] {}", line)));
        if self.voice.log.len() > 30 {
            self.voice.log.remove(0);
        }
    }

    fn voice_study_lang(&self) -> String {
        crate::util::lang_base(&self.settings.general.study_lang)
    }

    fn voice_tutor(&self) -> llm::Cfg {
        llm::pick(&self.settings.general.kaiwa_ai, &self.settings.subtitles, &self.settings.general.kaiwa_ollama)
    }

    /// The local Whisper model for `lang` (setting `voice_model`).
    pub fn voice_model(&self, lang: &str) -> &'static voice::WhisperModel {
        voice::whisper_model(&self.settings.general.voice_model, lang)
    }

    /// The engines that will hear `lang`, in order.
    pub fn voice_plan(&self, tutor: &llm::Cfg, lang: &str) -> Vec<Stt> {
        let choice = &self.settings.general.voice_input;
        let cloud = llm::cloud_transcriber(choice, tutor, &self.settings.subtitles);
        let m = self.voice_model(lang);
        let installed = local_ok() && crate::whisper_local::installed(&self.data_dir, m);
        voice::stt_plan(choice, cloud, local_ok(), installed, m.id)
    }

    /// Whether the voice can be heard now, and why not (no file is looked at: cheap for every refresh).
    pub fn voice_can_hear(&self, tutor: &llm::Cfg) -> (bool, String) {
        let choice = &self.settings.general.voice_input;
        let cloud = llm::cloud_transcriber(choice, tutor, &self.settings.subtitles);
        let ok = !voice::stt_plan(choice, cloud, local_ok(), false, "").is_empty();
        (ok, if ok { String::new() } else { voice::hear_missing(choice, local_ok()) })
    }

    /// Turns a recording (a WAV file, removed afterwards) into text in the background, trying the engines of the plan
    /// in order. `prompt` guides the local Whisper (`voice::stt_prompt`); `done` makes the caller's message with the
    /// text ("" when nothing was said). Err (at once): no engine can hear, with the reason.
    pub fn voice_hear(&mut self, wav: PathBuf, lang: &str, prompt: String, tutor: &llm::Cfg, done: impl FnOnce(Result<String, String>) -> Msg + Send + 'static) -> Result<(), String> {
        let plan = self.voice_plan(tutor, lang);
        if plan.is_empty() {
            let _ = std::fs::remove_file(&wav);
            let why = voice::hear_missing(&self.settings.general.voice_input, local_ok());
            return Err(if why.is_empty() { "Nothing can hear the voice now.".into() } else { why });
        }
        // the local Whisper first: its setup starts now (the first time, the download) and the call waits for it
        if let Some(Stt::Local(id)) = plan.first() {
            self.whisper_setup_for(id);
        }
        self.voice_log(&format!("hearing {} with {}", lang, plan.iter().map(|e| e.label()).collect::<Vec<_>>().join(" then ")));
        let whisper = self.whisper.clone();
        let data_dir = self.data_dir.clone();
        let lang = lang.to_string();
        self.spawn(move |tx, http| {
            let bytes = match std::fs::read(&wav) {
                Ok(b) => b,
                Err(e) => {
                    send(tx, done(Err(format!("The recording could not be read: {}", e))));
                    return;
                }
            };
            let _ = std::fs::remove_file(&wav);
            // what the file holds (the app's own WAV, or Qt's recorder's in any format)
            let info = crate::vad::wav_info(&bytes);
            let secs = info.as_ref().map(|i| i.seconds as f64).unwrap_or(bytes.len().saturating_sub(44) as f64 / 32_000.0);
            let level = match &info {
                Some(i) => {
                    let mic = i.measure.map(|m| format!(", mic before gain peak={:.1} dBFS rms={:.1} dBFS gain=x{:.1} speech={}", m.peak_db, m.rms_db, m.gain, if m.speech { "yes" } else { "no" })).unwrap_or_default();
                    format!("{:.2}s {} Hz {} ch {}-bit{} peak={:.1} dBFS rms={:.1} dBFS{}", i.seconds, i.rate, i.channels, i.bits, if i.format == 3 { " float" } else { "" }, i.peak_db, i.rms_db, mic)
                }
                None => format!("{} bytes, not a WAV file the app can read", bytes.len()),
            };
            send(tx, Msg::Voice(VoiceMsg::Log(format!("recording: {} lang={} transcribers: {}", level, lang, plan.iter().map(|e| e.label()).collect::<Vec<_>>().join(" then ")))));
            // a header alone and very short clicks: nothing was said
            if secs < 0.25 {
                send(tx, Msg::Voice(VoiceMsg::Log(format!("recording of {:.2} s: too short to hear", secs))));
                send(tx, Msg::Voice(VoiceMsg::Measure(measure_json(info.as_ref(), "", "", 0))));
                send(tx, done(Ok(String::new())));
                return;
            }
            let mut errors: Vec<String> = vec![];
            for (i, eng) in plan.iter().enumerate() {
                let t = Instant::now();
                let r = match eng {
                    Stt::Cloud(cfg) => llm::transcribe(&*http, cfg, &bytes, &lang),
                    Stt::Local(id) => {
                        // a fallback starts the installed Whisper here (nothing to download)
                        if whisper.begin(id) {
                            whisper.run(&*http, &data_dir, &|| send(tx, Msg::Voice(VoiceMsg::Whisper)));
                        }
                        match whisper.wait_ready(id, Duration::from_secs(8 * 3600)) {
                            Ok((root, vad)) => match voice::whisper_transcribe(&*http, &root, &bytes, &lang, &prompt, vad) {
                                // Whisper's own detector found no speech in a soft voice: once more without it
                                Ok(t) if vad && t.trim().is_empty() => {
                                    send(tx, Msg::Voice(VoiceMsg::Log(format!("stt={} heard nothing with its speech detector, again without it", eng.label()))));
                                    voice::whisper_transcribe(&*http, &root, &bytes, &lang, &prompt, false).inspect_err(|_| whisper.invalidate())
                                }
                                r => r.inspect_err(|_| whisper.invalidate()),
                            },
                            Err(e) => Err(e),
                        }
                    }
                };
                let ms = t.elapsed().as_millis() as u64;
                match r {
                    // nothing heard by one engine: the next one listens too (a cloud model may miss a soft voice)
                    Ok(raw) if voice::clean_transcript(&raw, &lang).is_empty() && i + 1 < plan.len() => {
                        let dropped = if raw.trim().is_empty() { String::new() } else { format!(" (dropped {:?})", raw.trim()) };
                        send(tx, Msg::Voice(VoiceMsg::Measure(measure_json(info.as_ref(), &eng.label(), &raw, ms))));
                        send(tx, Msg::Voice(VoiceMsg::Log(format!("stt={} heard nothing in {:.1}s of audio{} -> trying {}", eng.label(), secs, dropped, plan[i + 1].label()))));
                    }
                    Ok(raw) => {
                        let text = voice::clean_transcript(&raw, &lang);
                        let note = if i > 0 { format!("after {} failed", plan[..i].iter().map(|e| e.label()).collect::<Vec<_>>().join(", ")) } else { String::new() };
                        let dropped = if text.is_empty() && !raw.trim().is_empty() { format!(" (dropped {:?})", raw.trim()) } else { String::new() };
                        send(
                            tx,
                            Msg::Voice(VoiceMsg::Log(format!(
                                "stt={} lang={} audio={:.1}s ms={} chars={}{}{}",
                                eng.label(),
                                lang,
                                secs,
                                ms,
                                text.chars().count(),
                                if note.is_empty() { String::new() } else { format!(" ({})", note) },
                                dropped
                            ))),
                        );
                        send(tx, Msg::Voice(VoiceMsg::Measure(measure_json(info.as_ref(), &eng.label(), &raw, ms))));
                        send(tx, Msg::Voice(VoiceMsg::Heard { engine: eng.pretty(), ms, text: text.clone(), note }));
                        send(tx, done(Ok(text)));
                        return;
                    }
                    Err(e) => {
                        send(tx, Msg::Voice(VoiceMsg::Measure(measure_json(info.as_ref(), &eng.label(), &format!("error: {}", e), ms))));
                        let next = plan.get(i + 1).map(|n| format!(" -> falling back to {}", n.label())).unwrap_or_default();
                        send(tx, Msg::Voice(VoiceMsg::Log(format!("stt={} failed after {} ms: {}{}", eng.label(), ms, e, next))));
                        errors.push(e);
                    }
                }
            }
            let mut msg = errors.first().cloned().unwrap_or_else(|| "The voice could not be written down.".into());
            if plan.len() == 1 && matches!(plan[0], Stt::Cloud(_)) && local_ok() {
                msg.push_str(" To keep talking without the internet, turn on Whisper on this computer (Conversation > Voice).");
            }
            send(tx, done(Err(msg)));
        });
        Ok(())
    }

    /// Reads `text` aloud in `lang`: Google's online voice, or a voice of the system (setting `voice_output`; "auto"
    /// falls back to the system when Google cannot be reached). An empty text stops the voice (the microphone opens).
    pub fn voice_say(&mut self, text: &str, lang: &str) {
        self.voice.tts_gen += 1;
        let gen = self.voice.tts_gen;
        let text = text.trim().to_string();
        if text.is_empty() {
            if let Some(m) = &self.study_audio {
                let _ = m.cmd(&["stop"]);
            }
            if self.voice.say.get("text").as_str().map(|t| !t.is_empty()).unwrap_or(false) {
                self.voice_say_local("", lang);
            }
            return;
        }
        let plan = voice::tts_plan(&self.settings.general.voice_output);
        if plan.first() == Some(&Tts::Local) {
            self.voice_say_local(&text, lang);
            return;
        }
        let fallback = plan.contains(&Tts::Local);
        let dir = self.data_dir.join("voice");
        // the pieces of older texts go (one still playing on Windows stays until the next time)
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let lang = lang.to_string();
        self.spawn(move |tx, http| {
            let _ = std::fs::create_dir_all(&dir);
            let t0 = Instant::now();
            for (i, piece) in voice::speech_pieces(&text, 180).iter().enumerate() {
                let req = Request::get(&voice::google_tts_url(&lang, piece)).header("User-Agent", BROWSER_UA).header("Referer", "https://translate.google.com/").timeout(15);
                let got = match http.send(&req) {
                    Ok(r) if r.ok() && r.body.len() > 256 => Ok(r.body),
                    Ok(r) => Err(format!("Google answered {}", r.status)),
                    Err(e) => Err(e),
                };
                match got {
                    Ok(body) => {
                        let p = dir.join(format!("tts-{}-{}.mp3", gen, i));
                        if let Err(e) = std::fs::write(&p, &body) {
                            send(tx, Msg::Voice(VoiceMsg::Log(format!("tts=google: could not save the voice: {}", e))));
                            return;
                        }
                        if i == 0 {
                            send(tx, Msg::Voice(VoiceMsg::Log(format!("tts=google lang={} first piece in {} ms", lang, t0.elapsed().as_millis()))));
                        }
                        send(tx, Msg::Voice(VoiceMsg::Piece(gen, i, p)));
                    }
                    Err(e) => {
                        if i == 0 {
                            send(tx, Msg::Voice(VoiceMsg::OnlineFailed(gen, if fallback { text.clone() } else { String::new() }, lang.clone(), e)));
                        } else {
                            send(tx, Msg::Voice(VoiceMsg::Log(format!("tts=google: piece {} failed: {}", i + 1, e))));
                        }
                        return;
                    }
                }
            }
        });
    }

    /// Asks the page to read `text` with a voice of the system ("" stops it).
    fn voice_say_local(&mut self, text: &str, lang: &str) {
        let name = self.settings.general.voice_local.clone();
        if !text.is_empty() {
            if voice::has_system_voice(&self.voice.local, lang) == Some(false) {
                self.voice_log(&format!("tts=local: no system voice for {}", lang));
                let msg = "No voice of this computer speaks the language studied. On Windows: Settings > Time & language > Speech > Add voices.";
                let ui = self.ui_lang();
                self.toast(crate::i18n::translate(&ui, msg).unwrap_or(msg));
                return;
            }
            self.voice_log(&format!("tts=local lang={} voice={}", lang, if name.is_empty() { "(first of the language)" } else { name.as_str() }));
        }
        self.voice.say_seq += 1;
        self.voice.say = Json::obj(vec![("seq", Json::Num(self.voice.say_seq as f64)), ("text", Json::Str(text.into())), ("lang", Json::Str(lang.into())), ("voice", Json::Str(name))]);
        self.events.push(Event::Study);
    }

    /// The page's "Test the voice": a sentence in the language studied, with `engine` ("local", "google" or "" = the
    /// setting).
    pub fn voice_test(&mut self, engine: &str) {
        let lang = self.voice_study_lang();
        let lang = if lang.is_empty() { "en".to_string() } else { lang };
        let text = voice::sample_sentence(&lang);
        if engine == "local" {
            self.voice.tts_gen += 1;
            if let Some(m) = &self.study_audio {
                let _ = m.cmd(&["stop"]);
            }
            self.voice_say_local(text, &lang);
        } else {
            self.voice_say(text, &lang);
        }
    }

    /// The page's "Test the microphone": a few seconds recorded by the page (a WAV file, removed after) written down
    /// by the engines of the plan, in the language studied; the answer is `test` in `voice_json`.
    pub fn voice_hear_test(&mut self, path: &str) {
        let path = crate::util::file_url_path(path);
        let lang = self.voice_study_lang();
        let lang = if lang.is_empty() { "en".to_string() } else { lang };
        let tutor = self.voice_tutor();
        let prompt = voice::stt_prompt(&lang, &[], &[], &[]);
        self.voice.test_seq += 1;
        let seq = Json::Num(self.voice.test_seq as f64);
        self.voice.test = match self.voice_hear(path.into(), &lang, prompt, &tutor, |r| Msg::Voice(VoiceMsg::Test(r))) {
            Ok(()) => Json::obj(vec![("seq", seq), ("busy", Json::Bool(true))]),
            Err(e) => Json::obj(vec![("seq", seq), ("error", Json::Str(e))]),
        };
        self.events.push(Event::Study);
    }

    /// What the page found about the voices of the system (Qt TextToSpeech).
    pub fn voice_local_info(&mut self, j: &Json) {
        let voices: Vec<Json> = j
            .get("voices")
            .items()
            .iter()
            .take(300)
            .map(|v| Json::obj(vec![("name", Json::Str(v.get("name").str())), ("locale", Json::Str(v.get("locale").str()))]))
            .collect();
        let available = j.get("available").as_bool().unwrap_or(false);
        let engine = j.get("engine").str();
        let info = Json::obj(vec![("available", Json::Bool(available)), ("engine", Json::Str(engine.clone())), ("voices", Json::Arr(voices))]);
        if info == self.voice.local {
            return;
        }
        let lang = self.voice_study_lang();
        let mine: Vec<String> = info.get("voices").items().iter().filter(|v| crate::util::lang_base(&v.get("locale").str()) == lang).map(|v| v.get("name").str()).collect();
        self.voice_log(&format!(
            "system voices: {} (engine {:?}), {} in all, for {}: {}",
            if available { "available" } else { "Qt TextToSpeech missing" },
            engine,
            info.get("voices").items().len(),
            lang,
            if mine.is_empty() { "none".to_string() } else { mine.join(", ") }
        ));
        self.voice.local = info;
        self.events.push(Event::Study);
    }

    /// The page could not speak (no voice, an error of the engine).
    pub fn voice_local_failed(&mut self, why: &str) {
        self.voice_log(&format!("tts=local failed: {}", why));
    }

    /// Gets Whisper ready for `id` (finds or downloads the program and the model, starts the server) in the background.
    pub fn whisper_setup_for(&mut self, id: &str) {
        if self.whisper.begin(id) {
            self.voice_log(&format!("starting local whisper (model {})", id));
            let local = self.whisper.clone();
            let dir = self.data_dir.clone();
            self.spawn(move |tx, http| {
                local.run(&*http, &dir, &|| send(tx, Msg::Voice(VoiceMsg::Whisper)));
            });
        }
        self.events.push(Event::Study);
    }

    /// The page's button: Whisper with the model of the language studied.
    pub fn whisper_setup(&mut self) {
        let lang = self.voice_study_lang();
        let id = self.voice_model(&lang).id;
        self.whisper_setup_for(id);
    }

    /// Stops the Whisper server Reelhouse started.
    pub fn whisper_stop(&mut self) {
        self.whisper.stop();
        self.voice_log("local whisper stopped");
        self.events.push(Event::Study);
    }

    /// Checks that a ready Whisper still answers (the page asks when it shows).
    pub fn whisper_check(&mut self) {
        let st = self.whisper.status();
        if st.phase == "ready" {
            let local = self.whisper.clone();
            self.spawn(move |tx, http| {
                if crate::whisper_local::health(&*http, &st.root) != Some(true) {
                    local.invalidate();
                    send(tx, Msg::Voice(VoiceMsg::Whisper));
                }
            });
        }
        self.events.push(Event::Study);
    }

    pub(super) fn on_voice(&mut self, msg: VoiceMsg) {
        match msg {
            VoiceMsg::Whisper => {}
            VoiceMsg::Log(line) => {
                self.voice_log(&line);
            }
            VoiceMsg::Test(r) => {
                let seq = Json::Num(self.voice.test_seq as f64);
                let m = self.voice.measure.clone();
                let note = Json::Str(measure_note(&m));
                self.voice.test = match r {
                    Ok(t) if t.trim().is_empty() => Json::obj(vec![("seq", seq), ("error", Json::Str(nothing_heard_why(&m).into())), ("measure", m), ("note", note)]),
                    Ok(t) => Json::obj(vec![("seq", seq), ("text", Json::Str(t)), ("measure", m), ("note", note)]),
                    Err(e) => Json::obj(vec![("seq", seq), ("error", Json::Str(e)), ("measure", m), ("note", note)]),
                };
            }
            VoiceMsg::Measure(m) => {
                self.voice.measure = m;
                return;
            }
            VoiceMsg::Heard { engine, ms, text, note } => {
                self.voice.last_seq += 1;
                let lang = self.voice_study_lang();
                let kanji = if lang == "ja" { voice::kanji_dict(&text) } else { Json::Null };
                self.voice.last = Json::obj(vec![
                    ("seq", Json::Num(self.voice.last_seq as f64)),
                    ("engine", Json::Str(engine)),
                    ("ms", Json::Num(ms as f64)),
                    ("text", Json::Str(text)),
                    ("note", Json::Str(note)),
                    ("lang", Json::Str(lang)),
                    ("kanji", kanji),
                ]);
            }
            VoiceMsg::Piece(gen, i, path) => {
                if gen != self.voice.tts_gen {
                    let _ = std::fs::remove_file(&path);
                    return;
                }
                if self.study_audio.is_none() {
                    match Mpv::new_audio(&self.rt.libmpv_candidates()) {
                        Ok(m) => {
                            self.study_audio = Some(m);
                            self.apply_study_volume();
                        }
                        Err(e) => {
                            self.voice_log(&format!("the sound player could not start: {}", e));
                            return;
                        }
                    }
                }
                if let Some(m) = &self.study_audio {
                    let p = path.to_string_lossy().into_owned();
                    let _ = m.cmd(&["loadfile", &p, if i == 0 { "replace" } else { "append-play" }]);
                }
                return;
            }
            VoiceMsg::OnlineFailed(gen, text, lang, why) => {
                if gen != self.voice.tts_gen {
                    return;
                }
                if text.is_empty() {
                    self.voice_log(&format!("tts=google failed: {}", why));
                    let msg = "Google's voice could not be reached. Choose the voice of this computer in Conversation > Voice.";
                    let ui = self.ui_lang();
                    self.toast(crate::i18n::translate(&ui, msg).unwrap_or(msg));
                } else {
                    self.voice_log(&format!("tts=google failed: {} -> falling back to the system voice", why));
                    self.voice_say_local(&text, &lang);
                }
                return;
            }
        }
        self.events.push(Event::Study);
    }

    /// The voice for the interface (`Core.state("voice")`): the choices, the plan for the language studied, Whisper on
    /// this computer (status, models, where its files go), the last text heard, the request for the system voice,
    /// the settings of the microphone's detector and the latest `[voice]` lines.
    pub fn voice_json(&self) -> Json {
        let lang = self.voice_study_lang();
        let g = &self.settings.general;
        let tutor = self.voice_tutor();
        let plan = self.voice_plan(&tutor, &lang);
        let (can, missing) = self.voice_can_hear(&tutor);
        let m = self.voice_model(&lang);
        let models: Vec<Json> = voice::WHISPER_MODELS
            .iter()
            .filter(|x| x.only.is_empty() || x.only == lang)
            .map(|x| Json::obj(vec![("id", Json::Str(x.id.into())), ("label", Json::Str(x.label.into())), ("mib", Json::Num(x.mib as f64)), ("note", Json::Str(x.note.into()))]))
            .collect();
        let mut whisper = self.whisper.status().to_json();
        if let Json::Obj(o) = &mut whisper {
            o.push(("dir".into(), Json::Str(crate::whisper_local::home(&self.data_dir).to_string_lossy().into_owned())));
            o.push(("installed".into(), Json::Bool(local_ok() && crate::whisper_local::installed(&self.data_dir, m))));
            o.push(("inUse".into(), Json::Bool(plan.iter().any(|e| matches!(e, Stt::Local(_))))));
        }
        Json::obj(vec![
            ("available", Json::Bool(local_ok())),
            ("lang", Json::Str(lang.clone())),
            ("input", Json::Str(g.voice_input.clone())),
            ("output", Json::Str(g.voice_output.clone())),
            ("modelChoice", Json::Str(g.voice_model.clone())),
            ("localVoice", Json::Str(g.voice_local.clone())),
            ("pause", Json::Num(g.voice_pause as f64)),
            ("mic", Json::Str(g.voice_mic.clone())),
            ("test", self.voice.test.clone()),
            ("testFile", Json::Str(self.data_dir.join("voice-test.wav").to_string_lossy().into_owned())),
            ("plan", Json::Arr(plan.iter().map(|e| Json::Str(e.pretty())).collect())),
            ("canHear", Json::Bool(can)),
            ("hearMissing", Json::Str(missing)),
            ("model", Json::obj(vec![("id", Json::Str(m.id.into())), ("label", Json::Str(m.label.into())), ("mib", Json::Num(m.mib as f64)), ("note", Json::Str(m.note.into()))])),
            ("models", Json::Arr(models)),
            ("whisper", whisper),
            ("last", self.voice.last.clone()),
            ("say", self.voice.say.clone()),
            ("local", self.voice.local.clone()),
            ("vad", crate::vad::VadCfg::for_user(&lang, g.voice_pause, g.voice_margin).to_json()),
            ("log", Json::Arr(self.voice.log.iter().map(|(n, l)| Json::obj(vec![("seq", Json::Num(*n as f64)), ("line", Json::Str(l.clone()))])).collect())),
        ])
    }
}

/// Conversation's prompt for the local Whisper: the tutor's last two lines (the scene's words), the hints on screen
/// (what the learner may say next) and the words learned in this language.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Core {
    pub(super) fn kaiwa_voice_prompt(&self, lang: &str) -> String {
        let tutor: Vec<String> = self
            .kaiwa
            .store
            .sessions
            .iter()
            .find(|s| s.id == self.kaiwa.open)
            .map(|s| {
                let mut t: Vec<String> = s.lines.iter().rev().filter(|l| l.role == "assistant").take(2).map(|l| l.text.clone()).collect();
                t.reverse();
                t
            })
            .unwrap_or_default();
        let phrases: Vec<String> = self.kaiwa.hints.items().iter().map(|h| h.get("text").str()).filter(|t| !t.is_empty()).collect();
        let words: Vec<String> = self.kaiwa.store.words.iter().filter(|w| w.2 == lang).take(8).map(|w| w.0.clone()).collect();
        voice::stt_prompt(lang, &tutor, &phrases, &words)
    }
}
