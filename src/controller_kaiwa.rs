//! Conversation in the Study mode (see `kaiwa`): the sessions with the tutor, the checks of the learner's messages,
//! hints, words explained (and made into flashcards), reports, and the tutor's voice. Every call to the model runs in
//! the background; its answer comes back as `Msg::Kaiwa`.
use super::*;
use crate::kaiwa::{self, Ctx, Line, Session};
use crate::kaiwa_live::{self, Coach, Effect};
use crate::live;
use crate::llm;
use std::time::{Duration, Instant};

/// What a background call brings back. `sid` is the session it belongs to: an answer for a session that was closed
/// or deleted meanwhile is kept with that session (or dropped when it is gone).
pub enum KaiwaMsg {
    /// the tutor's answer: session, number of the request (a cancelled one is dropped), the answer
    Reply(i64, u64, Result<String, String>),
    /// the request moved to another stage ("thinking", "writing"): session, number of the request, stage
    Stage(i64, u64, &'static str),
    Check(i64, usize, Result<Json, String>),
    Hints(i64, Result<Json, String>),
    Word(String, Result<Json, String>),
    Report(i64, Result<Json, String>),
    /// what the microphone heard, for the session `sid`
    Heard(i64, u64, Result<String, String>),
    /// the local AI (ollama_local) changed its status
    Ollama,
    /// what the call of Gemini Live number `id` says (see `live::Event`; a call hung up keeps talking a moment)
    Live(u64, live::Event),
    /// the reading in kana, romaji and translation of a line of a spoken session
    Annot(i64, usize, Result<Json, String>),
}

#[derive(Default)]
pub struct KaiwaState {
    loaded: bool,
    pub store: kaiwa::Store,
    /// the session open on screen (0: the home of Conversation)
    pub open: i64,
    /// waiting for the tutor's turn / for the report
    pub busy: bool,
    pub reporting: bool,
    pub error: String,
    pub hints: Json,
    pub hints_busy: bool,
    /// the word explained: `{"key":…, "loading":true}` then the answer
    pub word: Json,
    /// the voice being turned into text, and the last text heard (`seq` grows with each one, so the page sees a
    /// new one even when the words are the same)
    pub listening: bool,
    pub heard: String,
    pub heard_seq: u64,
    /// the call of Gemini Live in progress (one at a time)
    pub live: Option<LiveCall>,
    /// what the page shows while it waits: "" (nothing), "hearing" (the voice is written down), "preparing" (the
    /// local AI starts), "thinking" (the tutor's model works), "writing" (the first answer was empty: asked again in
    /// plain text); since when (ms since 1970), and the number of the request (a cancelled one is dropped)
    pub stage: &'static str,
    pub stage_since: i64,
    pub gen: u64,
    pub hear_gen: u64,
    /// the technical detail under an error (model, seconds, the raw error, the microphone's level)
    pub detail: String,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The deck the words of the conversations go to.
pub const KAIWA_DECK: &str = "Kaiwa";

impl Core {
    /// The conversations, mistakes and words of the profile in use (each person has their own; see `profiles.rs`).
    fn kaiwa_path(&self) -> std::path::PathBuf {
        self.profile_dir.join("kaiwa.json")
    }

    /// Another profile is about to be used (`profile_dir` still names the one leaving): a call in progress hangs up,
    /// what was said is kept in this profile's file, and the next profile's conversations are read when next needed.
    pub(super) fn kaiwa_leave_profile(&mut self) {
        self.kaiwa_live_hangup();
        if self.kaiwa.loaded {
            self.kaiwa_save();
        }
        self.kaiwa = KaiwaState::default();
    }

    /// The conversations in memory are dropped without being written (a switch already kept them; a restored backup
    /// brought its own kaiwa.json): they are read again from the profile in use when next needed.
    pub(super) fn kaiwa_forget(&mut self) {
        if let Some(call) = self.kaiwa.live.take() {
            call.handle.send(live::Cmd::Stop);
        }
        self.kaiwa = KaiwaState::default();
    }

    fn kaiwa_load(&mut self) {
        if !self.kaiwa.loaded {
            self.kaiwa.loaded = true;
            self.kaiwa.store = kaiwa::Store::load(&self.kaiwa_path());
        }
    }

    fn kaiwa_save(&mut self) {
        let path = self.kaiwa_path();
        self.kaiwa.store.save(&path);
    }

    /// The language studied (two letters), or None with the reason shown.
    fn kaiwa_lang(&self) -> Option<String> {
        let l = crate::util::lang_base(&self.settings.general.study_lang);
        if l.is_empty() {
            None
        } else {
            Some(l)
        }
    }

    fn kaiwa_llm(&self) -> llm::Cfg {
        llm::pick(&self.settings.general.kaiwa_ai, &self.settings.subtitles, &self.settings.general.kaiwa_ollama)
    }

    fn kaiwa_ctx(&self, s: &Session) -> Ctx {
        let now = Self::now_secs();
        let tz = self.time.log.tz;
        let lang = s.lang.clone();
        Ctx {
            ui: crate::util::lang_base(&self.ui_lang()),
            level: s.level.clone(),
            name: self.settings.general.kaiwa_name.clone(),
            interests: self.settings.general.kaiwa_interests.clone(),
            hour: super::timetrack::hour_of(now, tz) as u32,
            mode: s.mode.clone(),
            scene_id: s.scene.clone(),
            custom: s.custom.clone(),
            topic: s.topic.clone(),
            script: s.script.clone(),
            mistakes: self.kaiwa.store.mistakes.iter().filter(|m| m.lang == lang).take(5).map(|m| (m.wrong.clone(), m.right.clone())).collect(),
            words: self.kaiwa.store.words.iter().filter(|w| w.2 == lang).take(10).map(|w| w.0.clone()).collect(),
            lang,
        }
    }

    fn kaiwa_session(&mut self, sid: i64) -> Option<&mut Session> {
        self.kaiwa.store.sessions.iter_mut().find(|s| s.id == sid)
    }

    /// Starts a session: `mode` "chat", "roleplay" (with `scene`), "custom" (`custom`: setting, the tutor's role, the
    /// learner's role), "lesson" (`scene` is the lesson) or "story" (`topic`, `script`). The tutor speaks first.
    pub fn kaiwa_start(&mut self, j: &Json) {
        self.kaiwa_load();
        let Some(lang) = self.kaiwa_lang() else {
            self.kaiwa.error = "Choose the language you study first (Preferences > Languages I know).".into();
            self.events.push(Event::Study);
            return;
        };
        let mode = match j.get("mode").str().as_str() {
            m @ ("roleplay" | "custom" | "lesson" | "story" | "call") => m.to_string(),
            _ => "chat".to_string(),
        };
        let scene_id = j.get("scene").str();
        let c = j.get("custom").strings();
        let custom = [c.first().cloned().unwrap_or_default(), c.get(1).cloned().unwrap_or_default(), c.get(2).cloned().unwrap_or_default()];
        if mode == "custom" && custom[0].trim().is_empty() {
            self.toast("Describe the scene first.");
            return;
        }
        let title = match mode.as_str() {
            "roleplay" => kaiwa::scene(&scene_id).map(|s| s.title.to_string()).unwrap_or_else(|| "Role-play".into()),
            "lesson" => kaiwa::lesson(&scene_id).map(|l| l.title.to_string()).unwrap_or_else(|| "Lesson".into()),
            "custom" => custom[0].chars().take(40).collect(),
            "story" => "Story time".into(),
            "call" => "Voice call".into(),
            _ => "Free conversation".into(),
        };
        // spoken with Gemini Live: the voice call always, scenes, lessons and free talk when "speak" is on
        let live = self.kaiwa_wants_live(&mode);
        // without Gemini Live the call still works with the voice of the Study mode (a cloud key, or Whisper on this
        // computer with the local AI): only when nothing at all can hear is it refused
        let (hears, why) = self.voice_can_hear(&self.kaiwa_llm());
        if mode == "call" && !live && !hears {
            self.kaiwa.error = if why.is_empty() { self.kaiwa_live_missing().unwrap_or(NEED_VOICE_KEY).to_string() } else { why };
            self.events.push(Event::Study);
            return;
        }
        let goals = if live { kaiwa_live::goals_for(&mode, &scene_id) } else { vec![] };
        let now = Self::now_secs();
        let mut id = now * 1000;
        while self.kaiwa.store.sessions.iter().any(|s| s.id == id) {
            id += 1;
        }
        let s = Session {
            id,
            mode,
            scene: scene_id,
            title,
            custom,
            topic: j.get("topic").str(),
            script: j.get("script").str(),
            level: kaiwa::level_for(&lang, &self.settings.general.kaiwa_level),
            lang,
            started: now,
            live,
            goals,
            ..Session::default()
        };
        self.kaiwa.store.sessions.insert(0, s);
        // only the last 100 sessions are kept
        self.kaiwa.store.sessions.truncate(100);
        self.kaiwa.open = id;
        self.kaiwa.hints = Json::Null;
        self.kaiwa.word = Json::Null;
        self.kaiwa_save();
        if live {
            self.kaiwa_live_begin(id);
        } else {
            self.kaiwa_ask(id);
        }
    }

    /// Asks the tutor for its next turn.
    fn kaiwa_ask(&mut self, sid: i64) {
        let Some(s) = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).cloned() else { return };
        let cfg = self.kaiwa_llm();
        let gate = self.kaiwa_gate(&cfg);
        let system = kaiwa::tutor_system(&self.kaiwa_ctx(&s));
        let mut turns = kaiwa::turns(&s);
        if turns.is_empty() {
            turns.push(llm::Turn::new("user", "(The session starts: say your first line.)"));
        }
        self.kaiwa.busy = true;
        self.kaiwa.error.clear();
        self.kaiwa.detail.clear();
        self.kaiwa.gen += 1;
        let gen = self.kaiwa.gen;
        self.kaiwa_stage(if gate.is_some() { "preparing" } else { "thinking" });
        crate::debug::log("kaiwa", &format!("asking {} (session {}, {} turns)", cfg.label(), sid, turns.len()));
        self.events.push(Event::Study);
        self.spawn(move |tx, http| {
            if let Some((local, model)) = &gate {
                if let Err(e) = local.wait_ready(model, std::time::Duration::from_secs(8 * 3600)) {
                    send(tx, Msg::Kaiwa(KaiwaMsg::Reply(sid, gen, Err(e))));
                    return;
                }
                send(tx, Msg::Kaiwa(KaiwaMsg::Stage(sid, gen, "thinking")));
            }
            let t0 = std::time::Instant::now();
            let r = match llm::chat(&*http, &cfg, &system, &turns, true) {
                // no message in it ("{}" from a small model in JSON mode): once more, in plain text
                Ok(t) if kaiwa::reply_of(&t, 0).is_none() => {
                    crate::debug::log("kaiwa", &format!("{} gave no message after {} ms ({:?}): asking again in plain text", cfg.label(), t0.elapsed().as_millis(), t.chars().take(80).collect::<String>()));
                    send(tx, Msg::Kaiwa(KaiwaMsg::Stage(sid, gen, "writing")));
                    let again = format!("{}\n\n{}", system, kaiwa::PLAIN_AGAIN);
                    match llm::chat(&*http, &cfg, &again, &turns, false) {
                        Ok(t2) if kaiwa::reply_of(&t2, 0).is_some() => Ok(t2),
                        Ok(t2) => Err(format!("{}|empty answer twice: {:?} then {:?}", kaiwa::ERR_EMPTY, t.chars().take(40).collect::<String>(), t2.chars().take(40).collect::<String>())),
                        Err(e) => Err(e),
                    }
                }
                r => r,
            };
            crate::debug::log("kaiwa", &format!("{} answered in {} ms: {}", cfg.label(), t0.elapsed().as_millis(), match &r { Ok(t) => format!("{} chars", t.chars().count()), Err(e) => format!("error {}", e) }));
            send(tx, Msg::Kaiwa(KaiwaMsg::Reply(sid, gen, r)));
        });
    }

    fn kaiwa_stage(&mut self, stage: &'static str) {
        if self.kaiwa.stage != stage {
            self.kaiwa.stage = stage;
            self.kaiwa.stage_since = now_ms();
        }
    }

    /// The page's "Cancel" while it waits: the answer (or the text heard) that comes later is dropped.
    pub fn kaiwa_cancel(&mut self) {
        if self.kaiwa.busy {
            self.kaiwa.gen += 1;
            self.kaiwa.busy = false;
            crate::debug::log("kaiwa", "the learner cancelled the tutor's answer");
        }
        if self.kaiwa.listening {
            self.kaiwa.hear_gen += 1;
            self.kaiwa.listening = false;
        }
        self.kaiwa.stage = "";
        self.kaiwa.error.clear();
        self.kaiwa.detail.clear();
        self.events.push(Event::Study);
    }

    /// Conversation was left (another Study page, another mode, or a video started): what it holds is let go so the
    /// rest of the app stays light. The answer or the text being waited for is dropped, the voice stops, a Gemini Live
    /// call hangs up, the local AI unloads its model (`keep_alive: 0`) and the server Reelhouse started stops, and the
    /// Whisper server Reelhouse started stops. The settings stay as they are: both start again by themselves the next
    /// time Conversation needs them ("Loading the local AI…"). A setup still downloading is left to finish.
    pub fn kaiwa_release(&mut self) {
        if self.kaiwa.busy || self.kaiwa.listening {
            self.kaiwa_cancel();
        }
        self.kaiwa.hints_busy = false;
        if self.kaiwa.live.is_some() {
            self.kaiwa_live_hangup();
        }
        self.voice_say("", "");
        let os = self.ollama.status();
        let ws = self.whisper.status();
        // the subtitles' translator may be the local AI too: then it stays for them
        let ollama_up = (os.ours || os.phase == "ready") && self.settings.subtitles.provider != "ollama";
        let whisper_up = ws.ours || ws.phase == "ready";
        if !ollama_up && !whisper_up {
            return;
        }
        let model = if os.model.is_empty() { self.ollama_model() } else { os.model.clone() };
        let (local, whisper) = (self.ollama.clone(), self.whisper.clone());
        let (o_busy, w_busy) = (os.busy, ws.busy);
        self.voice_log(&format!(
            "Conversation closed: releasing{}{}",
            if ollama_up { if o_busy { " (the local AI is still being set up: left to finish)" } else { " the local AI" } } else { "" },
            if whisper_up { if w_busy { " (Whisper is still being set up: left to finish)" } else { " and Whisper" } } else { "" }
        ));
        self.spawn(move |_tx, http| {
            if ollama_up && !o_busy {
                // the model leaves the memory (RAM / VRAM) even when the server is someone else's
                let body = Json::obj(vec![("model", Json::Str(model)), ("keep_alive", Json::Num(0.0))]).dump();
                let _ = http.send(&crate::http::Request::post(&format!("{}/api/generate", crate::ollama_local::ROOT)).json_body(body).timeout(5));
                local.stop();
            }
            if whisper_up && !w_busy {
                whisper.stop();
            }
        });
        self.events.push(Event::Study);
    }

    /// What the page shows while it waits, read every half second by it (cheap): the stage, since when, the tutor.
    pub fn kaiwa_status_json(&self) -> Json {
        // the online voice playing (the system voice is the page's own)
        let speaking = self.study_audio.as_ref().and_then(|m| m.get_flag("idle-active")).map(|idle| !idle).unwrap_or(false);
        Json::obj(vec![
            ("stage", Json::Str(self.kaiwa.stage.into())),
            ("since", Json::Num(self.kaiwa.stage_since as f64)),
            ("now", Json::Num(now_ms() as f64)),
            ("ai", Json::Str(self.kaiwa_llm().label())),
            ("speaking", Json::Bool(speaking)),
        ])
    }

    /// The learner's message: kept, checked (when on) and answered.
    pub fn kaiwa_send(&mut self, text: &str) {
        if self.kaiwa_live_open() {
            return self.kaiwa_live_text(text);
        }
        let text = text.trim().to_string();
        let sid = self.kaiwa.open;
        if text.is_empty() || sid == 0 || self.kaiwa.busy {
            return;
        }
        let now = Self::now_secs();
        let check_on = self.settings.general.kaiwa_check;
        let Some(s) = self.kaiwa_session(sid) else { return };
        if s.ended > 0 {
            return;
        }
        s.lines.push(Line { role: "user".into(), text: text.clone(), at: now, ..Line::default() });
        let idx = s.lines.len() - 1;
        let s = s.clone();
        self.kaiwa.hints = Json::Null;
        self.kaiwa_save();
        if check_on {
            let cfg = self.kaiwa_llm();
            let gate = self.kaiwa_gate(&cfg);
            let prompt = kaiwa::check_prompt(&self.kaiwa_ctx(&s), &text);
            self.spawn(move |tx, http| {
                let r = kaiwa_chat(&gate, &*http, &cfg, "You answer with JSON only.", &[llm::Turn::new("user", &prompt)], true).and_then(|t| llm::json_of(&t).ok_or_else(|| "The check was not readable.".to_string()));
                send(tx, Msg::Kaiwa(KaiwaMsg::Check(sid, idx, r)));
            });
        }
        self.kaiwa_ask(sid);
    }

    /// Asks again after an error (the last turn of the learner stays).
    pub fn kaiwa_retry(&mut self) {
        let sid = self.kaiwa.open;
        // a spoken session whose call ended: call again
        if self.kaiwa.live.is_none() && self.kaiwa.store.sessions.iter().any(|s| s.id == sid && s.live && s.ended == 0) {
            return self.kaiwa_live_begin(sid);
        }
        if sid != 0 && !self.kaiwa.busy {
            self.kaiwa_ask(sid);
        }
    }

    pub fn kaiwa_hint(&mut self) {
        if self.kaiwa_live_open() {
            // the tutor gives it through give_hint (and says a word of encouragement)
            self.kaiwa.hints_busy = true;
            self.kaiwa.hints = Json::Null;
            self.live_send(live::Cmd::Note { text: kaiwa_live::hint_note(), respond: true });
            self.events.push(Event::Study);
            return;
        }
        let sid = self.kaiwa.open;
        let Some(s) = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).cloned() else { return };
        if self.kaiwa.hints_busy {
            return;
        }
        let cfg = self.kaiwa_llm();
        let gate = self.kaiwa_gate(&cfg);
        let prompt = kaiwa::hint_prompt(&self.kaiwa_ctx(&s), &s.lines);
        self.kaiwa.hints_busy = true;
        self.kaiwa.hints = Json::Null;
        self.events.push(Event::Study);
        self.spawn(move |tx, http| {
            let r = kaiwa_chat(&gate, &*http, &cfg, "You answer with JSON only.", &[llm::Turn::new("user", &prompt)], true).and_then(|t| llm::json_of(&t).ok_or_else(|| "The hints were not readable.".to_string()));
            send(tx, Msg::Kaiwa(KaiwaMsg::Hints(sid, r)));
        });
    }

    /// A word clicked in the conversation: explained by the model (and remembered as looked up).
    pub fn kaiwa_word(&mut self, word: &str, sentence: &str) {
        let word = word.trim().to_string();
        if word.is_empty() {
            return;
        }
        self.kaiwa_load();
        let sid = self.kaiwa.open;
        let s = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).cloned().unwrap_or_else(|| Session {
            lang: self.kaiwa_lang().unwrap_or_default(),
            ..Session::default()
        });
        let key = format!("{}|{}", word, sentence);
        self.kaiwa.word = Json::obj(vec![("key", Json::Str(key.clone())), ("word", Json::Str(word.clone())), ("loading", Json::Bool(true))]);
        self.events.push(Event::Study);
        let cfg = self.kaiwa_llm();
        let gate = self.kaiwa_gate(&cfg);
        let prompt = kaiwa::word_prompt(&self.kaiwa_ctx(&s), &word, sentence);
        self.spawn(move |tx, http| {
            let r = kaiwa_chat(&gate, &*http, &cfg, "You answer with JSON only.", &[llm::Turn::new("user", &prompt)], true).and_then(|t| llm::json_of(&t).ok_or_else(|| "The explanation was not readable.".to_string()));
            send(tx, Msg::Kaiwa(KaiwaMsg::Word(key, r)));
        });
    }

    /// A word made into a flashcard of the deck "Kaiwa": the word (and its reading) on the front, the meaning and the
    /// example on the back.
    pub fn kaiwa_card(&mut self, j: &Json) {
        let word = j.get("word").str();
        if word.trim().is_empty() {
            return;
        }
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let reading = j.get("reading").str();
        let front = if reading.is_empty() || reading == word { format!("<big>{}</big>", esc(&word)) } else { format!("<big>{}</big><br><small>{}</small>", esc(&word), esc(&reading)) };
        let mut back = esc(&j.get("meaning").str());
        let ex = j.get("example").str();
        if !ex.is_empty() {
            back.push_str(&format!("<br><br>{}<br><i>{}</i>", esc(&ex), esc(&j.get("example_tr").str())));
        }
        self.study.ensure_builtin();
        let did = match self.study.decks.iter().find(|d| d.name == KAIWA_DECK) {
            Some(d) => d.id,
            None => match self.study.create_deck(KAIWA_DECK) {
                Some(d) => d,
                None => return,
            },
        };
        let lang = self.kaiwa_lang().unwrap_or_default();
        self.kaiwa_load();
        self.kaiwa.store.learned(&word, &j.get("meaning").str(), &lang, Self::now_secs());
        self.kaiwa_save();
        self.study_add_note(did, crate::study::BASIC, vec![front, back]);
    }

    /// Ends the session and asks for its report.
    pub fn kaiwa_end(&mut self) {
        let sid = self.kaiwa.open;
        self.kaiwa_end_session(sid);
    }

    fn kaiwa_end_session(&mut self, sid: i64) {
        if self.kaiwa.live.as_ref().map(|c| c.sid == sid).unwrap_or(false) {
            self.kaiwa_live_hangup();
        }
        let now = Self::now_secs();
        let Some(s) = self.kaiwa_session(sid) else { return };
        if s.ended == 0 {
            s.ended = now;
        }
        let s = s.clone();
        self.kaiwa_save();
        if !s.lines.iter().any(|l| l.role == "user") {
            // nothing said: nothing to report
            self.events.push(Event::Study);
            return;
        }
        // a spoken session has a Gemini key: its report comes from Gemini, whatever the written tutor is
        let cfg = if s.live { llm::gemini_text(&self.settings.subtitles).unwrap_or_else(|| self.kaiwa_llm()) } else { self.kaiwa_llm() };
        let gate = self.kaiwa_gate(&cfg);
        let prompt = kaiwa::report_prompt(&self.kaiwa_ctx(&s), &s);
        self.kaiwa.reporting = true;
        self.events.push(Event::Study);
        self.spawn(move |tx, http| {
            let r = kaiwa_chat(&gate, &*http, &cfg, "You answer with JSON only.", &[llm::Turn::new("user", &prompt)], true).and_then(|t| llm::json_of(&t).ok_or_else(|| "The report was not readable.".to_string()));
            send(tx, Msg::Kaiwa(KaiwaMsg::Report(sid, r)));
        });
    }

    pub fn kaiwa_open(&mut self, sid: i64) {
        self.kaiwa_load();
        // leaving a spoken session hangs up (it can be called again from it)
        if self.kaiwa.live.as_ref().map(|c| c.sid != sid).unwrap_or(false) {
            self.kaiwa_live_hangup();
        }
        if sid == 0 || self.kaiwa.store.sessions.iter().any(|s| s.id == sid) {
            self.kaiwa.open = sid;
            self.kaiwa.hints = Json::Null;
            self.kaiwa.word = Json::Null;
            if self.kaiwa.live.is_none() {
                self.kaiwa.error.clear();
                self.kaiwa.detail.clear();
            }
            if self.kaiwa.busy {
                // the answer of the session left is dropped
                self.kaiwa.gen += 1;
            }
            self.kaiwa.busy = false;
            self.kaiwa.stage = "";
        }
        self.events.push(Event::Study);
    }

    pub fn kaiwa_delete(&mut self, sid: i64) {
        self.kaiwa_load();
        if self.kaiwa.live.as_ref().map(|c| c.sid == sid).unwrap_or(false) {
            self.kaiwa_live_hangup();
        }
        self.kaiwa.store.sessions.retain(|s| s.id != sid);
        if self.kaiwa.open == sid {
            self.kaiwa.open = 0;
        }
        self.kaiwa_save();
        self.events.push(Event::Study);
    }

    /// A recording of the microphone (a WAV file written by the page): turned into text in the background by the
    /// voice of the Study mode (controller_voice.rs: a cloud model or Whisper on this computer, with fallbacks); the
    /// text comes back as `heard` (the page puts it in the box, or sends it during a call). The file is removed after.
    pub fn kaiwa_listen(&mut self, path: &str) {
        let path = crate::util::file_url_path(path);
        let sid = self.kaiwa.open;
        let lang = self.kaiwa_lang().unwrap_or_else(|| "en".into());
        let prompt = self.kaiwa_voice_prompt(&lang);
        let tutor = self.kaiwa_llm();
        self.kaiwa.hear_gen += 1;
        let gen = self.kaiwa.hear_gen;
        if let Err(e) = self.voice_hear(path.into(), &lang, prompt, &tutor, move |r| Msg::Kaiwa(KaiwaMsg::Heard(sid, gen, r))) {
            self.kaiwa.error = e;
            self.events.push(Event::Study);
            return;
        }
        self.kaiwa.listening = true;
        if !self.kaiwa.busy {
            self.kaiwa_stage("hearing");
        }
        self.kaiwa.error.clear();
        self.kaiwa.detail.clear();
        self.events.push(Event::Study);
    }

    /// Reads a text aloud in the language studied (controller_voice.rs: Google's voice, or a voice of the system).
    pub fn kaiwa_speak(&mut self, text: &str) {
        let lang = self.kaiwa_lang().unwrap_or_else(|| "en".into());
        self.voice_say(text, &lang);
    }

    pub(super) fn on_kaiwa(&mut self, msg: KaiwaMsg) {
        self.kaiwa_load();
        let now = Self::now_secs();
        match msg {
            KaiwaMsg::Stage(sid, gen, stage) => {
                if sid == self.kaiwa.open && gen == self.kaiwa.gen && self.kaiwa.busy {
                    self.kaiwa_stage(stage);
                }
            }
            KaiwaMsg::Reply(sid, gen, r) => {
                if gen != self.kaiwa.gen {
                    // cancelled, or another session was opened
                    return;
                }
                let waited = ((now_ms() - self.kaiwa.stage_since).max(0) / 1000) as u64;
                if self.kaiwa.open == sid {
                    self.kaiwa.busy = false;
                    self.kaiwa.stage = "";
                }
                match r {
                    Ok(text) => {
                        let line = kaiwa::tutor_line(&text, now);
                        let speak = line.text.clone();
                        let Some(s) = self.kaiwa_session(sid) else { return };
                        s.lines.push(line);
                        self.kaiwa_save();
                        let spoken = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).map(|s| kaiwa::spoken(&s.mode)).unwrap_or(false);
                        if (self.settings.general.kaiwa_voice || spoken) && self.kaiwa.open == sid && self.mode() == "study" {
                            self.kaiwa_speak(&speak);
                        }
                    }
                    Err(e) => {
                        // the local server may have stopped: the next turn checks it again
                        self.ollama.invalidate();
                        if self.kaiwa.open == sid {
                            // "sentence|detail" (kaiwa_ask), a known error made a sentence of the interface, or as it is
                            let (sentence, raw) = match e.split_once('|') {
                                Some((a, b)) => (a.to_string(), b.to_string()),
                                None => match kaiwa::explain_error(&e) {
                                    "" => (e.clone(), String::new()),
                                    known => (known.to_string(), e.clone()),
                                },
                            };
                            self.kaiwa.error = sentence;
                            let ai = self.kaiwa_llm().label();
                            self.kaiwa.detail = if raw.is_empty() { format!("{} · {} s", ai, waited) } else { format!("{} · {} s · {}", ai, waited, raw.chars().take(200).collect::<String>()) };
                        }
                    }
                }
            }
            KaiwaMsg::Check(sid, idx, r) => {
                let lang = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).map(|s| s.lang.clone()).unwrap_or_default();
                let check = match r {
                    Ok(j) => j,
                    Err(e) => Json::obj(vec![("error", Json::Str(e))]),
                };
                if let Some(s) = self.kaiwa_session(sid) {
                    if let Some(l) = s.lines.get_mut(idx) {
                        l.check = check.clone();
                    }
                }
                self.kaiwa.store.remember(&check, &lang, now);
                self.kaiwa_save();
            }
            KaiwaMsg::Hints(sid, r) => {
                if self.kaiwa.open != sid {
                    return;
                }
                self.kaiwa.hints_busy = false;
                self.kaiwa.hints = match r {
                    Ok(j) => j.get("hints").clone(),
                    Err(e) => Json::obj(vec![("error", Json::Str(e))]),
                };
            }
            KaiwaMsg::Word(key, r) => {
                if self.kaiwa.word.get("key").str() != key {
                    return;
                }
                let word = self.kaiwa.word.get("word").str();
                self.kaiwa.word = match r {
                    Ok(Json::Obj(mut o)) => {
                        o.push(("key".into(), Json::Str(key)));
                        if o.iter().all(|(k, _)| k != "word") {
                            o.push(("word".into(), Json::Str(word)));
                        }
                        Json::Obj(o)
                    }
                    Ok(_) => Json::obj(vec![("key", Json::Str(key)), ("word", Json::Str(word)), ("error", Json::Str("The explanation was not readable.".into()))]),
                    Err(e) => Json::obj(vec![("key", Json::Str(key)), ("word", Json::Str(word)), ("error", Json::Str(e))]),
                };
            }
            KaiwaMsg::Heard(sid, gen, r) => {
                if gen != self.kaiwa.hear_gen {
                    return;
                }
                self.kaiwa.listening = false;
                if self.kaiwa.stage == "hearing" {
                    self.kaiwa.stage = "";
                }
                if self.kaiwa.open != sid {
                    return;
                }
                let m = self.voice.measure.clone();
                match r {
                    Ok(t) if t.trim().is_empty() => {
                        self.kaiwa.error = super::controller_voice::nothing_heard_why(&m).into();
                        self.kaiwa.detail = super::controller_voice::measure_note(&m);
                    }
                    Ok(t) => {
                        self.kaiwa.heard = t;
                        self.kaiwa.heard_seq += 1;
                    }
                    Err(e) => self.kaiwa.error = e,
                }
            }
            KaiwaMsg::Ollama => {}
            KaiwaMsg::Live(id, ev) => return self.on_live(id, ev),
            KaiwaMsg::Annot(sid, idx, r) => {
                if let (Ok(j), Some(s)) = (r, self.kaiwa.store.sessions.iter_mut().find(|s| s.id == sid)) {
                    if let Some(l) = s.lines.get_mut(idx) {
                        l.reading = j.get("reading").str().trim().to_string();
                        l.romaji = j.get("romaji").str().trim().to_string();
                        l.translation = j.get("translation").str().trim().to_string();
                    }
                    self.kaiwa_save();
                }
            }
            KaiwaMsg::Report(sid, r) => {
                self.kaiwa.reporting = false;
                match r {
                    Ok(j) => {
                        let lang = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).map(|s| s.lang.clone()).unwrap_or_default();
                        for w in j.get("words").items() {
                            self.kaiwa.store.learned(&w.get("word").str(), &w.get("meaning").str(), &lang, now);
                        }
                        if let Some(s) = self.kaiwa_session(sid) {
                            s.report = j;
                        }
                        self.kaiwa_save();
                    }
                    Err(e) => self.kaiwa.error = e,
                }
            }
        }
        self.events.push(Event::Study);
    }

    /// The model the local AI should have: the one of the tutor when it is Ollama, else the one chosen for Ollama.
    fn ollama_model(&self) -> String {
        let m = self.settings.general.kaiwa_ollama.trim();
        if m.is_empty() {
            llm::OLLAMA_MODEL.to_string()
        } else {
            m.to_string()
        }
    }

    /// Gets the local AI ready (finds, downloads or starts Ollama, then pulls the model) in the background. Does
    /// nothing while a setup runs or when the model is already ready.
    pub fn ollama_setup(&mut self) {
        let model = self.ollama_model();
        self.ollama_setup_for(&model);
    }

    fn ollama_setup_for(&mut self, model: &str) {
        if self.ollama.begin(model) {
            let local = self.ollama.clone();
            let dir = self.data_dir.clone();
            self.spawn(move |tx, http| {
                local.run(&*http, &dir, &|| send(tx, Msg::Kaiwa(KaiwaMsg::Ollama)));
            });
        }
        self.events.push(Event::Study);
    }

    /// Checks that a ready local AI still answers (the Ollama app may have been closed); the page asks it when it
    /// shows the panel.
    pub fn ollama_status(&mut self) {
        if self.ollama.status().phase == "ready" {
            let local = self.ollama.clone();
            self.spawn(move |tx, http| {
                if !crate::ollama_local::alive(&*http) {
                    local.invalidate();
                    send(tx, Msg::Kaiwa(KaiwaMsg::Ollama));
                }
            });
        }
        self.events.push(Event::Study);
    }

    /// Stops the Ollama server Reelhouse started (one started by someone else keeps running).
    pub fn ollama_stop(&mut self) {
        self.ollama.stop();
        self.events.push(Event::Study);
    }

    /// The local AI for the interface: `phase` (idle, checking, downloading_runtime, starting, pulling_model, ready,
    /// error), `message`, `done` / `total` bytes, `percent`, `model`, `busy`, `ready`, `canDownload`, `dir` (where
    /// its files go), and `inUse` (the tutor is Ollama).
    pub fn ollama_json(&self) -> Json {
        let mut j = self.ollama.status().to_json();
        if let Json::Obj(o) = &mut j {
            if self.ollama.status().model.is_empty() {
                o.retain(|(k, _)| k != "model");
                o.push(("model".into(), Json::Str(self.ollama_model())));
            }
            o.push(("dir".into(), Json::Str(self.data_dir.join("ollama").to_string_lossy().into_owned())));
            o.push(("inUse".into(), Json::Bool(self.kaiwa_llm().provider == "ollama")));
        }
        j
    }

    /// When the tutor is the local AI: its setup starts (if needed) and the calls wait for it.
    fn kaiwa_gate(&mut self, cfg: &llm::Cfg) -> Gate {
        if cfg.provider != "ollama" {
            return None;
        }
        self.ollama_setup_for(&cfg.model);
        Some((self.ollama.clone(), cfg.model.clone()))
    }

    /// The state of Conversation for the interface.
    /// (The sessions are read from disk by the first command: the page sends `kaiwa_open` when it shows.)
    pub fn kaiwa_json(&self) -> Json {
        let lang = self.kaiwa_lang().unwrap_or_default();
        let cfg = self.kaiwa_llm();
        let now = Self::now_secs();
        let scenes = kaiwa::SCENES
            .iter()
            .map(|s| Json::obj(vec![("id", Json::Str(s.id.into())), ("icon", Json::Str(s.icon.into())), ("cat", Json::Str(s.cat.into())), ("title", Json::Str(s.title.into())), ("goal", Json::Str(s.goal.into()))]))
            .collect();
        let lessons = kaiwa::LESSONS.iter().map(|l| Json::obj(vec![("id", Json::Str(l.id.into())), ("icon", Json::Str(l.icon.into())), ("title", Json::Str(l.title.into())), ("goal", Json::Str(l.goals.into()))])).collect();
        let open = self.kaiwa.store.sessions.iter().find(|s| s.id == self.kaiwa.open).map(|s| {
            let mut j = s.to_json();
            if let Json::Obj(o) = &mut j {
                o.push(("minutes".into(), Json::Num(s.minutes() as f64)));
            }
            j
        });
        let history: Vec<Json> = self
            .kaiwa
            .store
            .sessions
            .iter()
            .filter(|s| s.lang == lang)
            .take(40)
            .map(|s| {
                Json::obj(vec![
                    ("id", Json::Num(s.id as f64)),
                    ("title", Json::Str(s.title.clone())),
                    ("mode", Json::Str(s.mode.clone())),
                    ("scene", Json::Str(s.scene.clone())),
                    ("started", Json::Num(s.started as f64)),
                    ("ended", Json::Num(s.ended as f64)),
                    ("minutes", Json::Num(s.minutes() as f64)),
                    ("lines", Json::Num(s.lines.len() as f64)),
                    ("report", s.report.clone()),
                ])
            })
            .collect();
        let mistakes: Vec<Json> = self
            .kaiwa
            .store
            .mistakes
            .iter()
            .filter(|m| m.lang == lang)
            .take(30)
            .map(|m| Json::obj(vec![("wrong", Json::Str(m.wrong.clone())), ("right", Json::Str(m.right.clone())), ("why", Json::Str(m.why.clone())), ("kind", Json::Str(m.kind.clone()))]))
            .collect();
        let words: Vec<Json> = self.kaiwa.store.words.iter().filter(|w| w.2 == lang).take(30).map(|w| Json::obj(vec![("word", Json::Str(w.0.clone())), ("meaning", Json::Str(w.1.clone()))])).collect();
        Json::obj(vec![
            ("lang", Json::Str(lang.clone())),
            ("langName", Json::Str(if lang.is_empty() { String::new() } else { crate::util::lang_name(&lang) })),
            ("level", Json::Str(kaiwa::level_for(&lang, &self.settings.general.kaiwa_level))),
            ("levels", Json::Arr(kaiwa::levels(&lang).iter().map(|l| Json::Str(l.to_string())).collect())),
            ("ai", Json::Str(cfg.label())),
            ("aiMissing", Json::Str(cfg.missing().unwrap_or_default())),
            ("scenes", Json::Arr(scenes)),
            ("lessons", Json::Arr(lessons)),
            ("session", open.unwrap_or(Json::Null)),
            ("busy", Json::Bool(self.kaiwa.busy)),
            ("reporting", Json::Bool(self.kaiwa.reporting)),
            ("error", Json::Str(self.kaiwa.error.clone())),
            ("errorDetail", Json::Str(self.kaiwa.detail.clone())),
            ("hints", self.kaiwa.hints.clone()),
            ("hintsBusy", Json::Bool(self.kaiwa.hints_busy)),
            ("word", self.kaiwa.word.clone()),
            ("listening", Json::Bool(self.kaiwa.listening)),
            ("heard", Json::obj(vec![("seq", Json::Num(self.kaiwa.heard_seq as f64)), ("text", Json::Str(self.kaiwa.heard.clone()))])),
            ("canHear", Json::Bool(self.voice_can_hear(&cfg).0)),
            ("hearMissing", Json::Str(self.voice_can_hear(&cfg).1)),
            // Conversation by voice (Gemini Live): see kaiwa_live_info; the call itself is the light state "kaiwa_live"
            ("live", self.kaiwa_live_info(&lang)),
            // where the page writes a recording
            ("micFile", Json::Str(self.data_dir.join("kaiwa-voice.wav").to_string_lossy().into_owned())),
            // the local AI (Ollama run by Reelhouse): see ollama_json
            ("ollama", self.ollama_json()),
            ("history", Json::Arr(history)),
            ("progress", self.kaiwa.store.progress(&lang, now, self.time.log.tz)),
            ("mistakes", Json::Arr(mistakes)),
            ("words", Json::Arr(words)),
        ])
    }
}

/// The local AI to wait for before a call to the tutor (None: another provider).
type Gate = Option<(crate::ollama_local::Local, String)>;

/// `llm::chat`, after the local AI is ready (a first setup downloads it: the call waits, it does not fail).
fn kaiwa_chat(gate: &Gate, http: &dyn crate::http::Http, cfg: &llm::Cfg, system: &str, turns: &[llm::Turn], json: bool) -> Result<String, String> {
    if let Some((local, model)) = gate {
        local.wait_ready(model, std::time::Duration::from_secs(8 * 3600))?;
    }
    llm::chat(http, cfg, system, turns, json)
}


// ================================================================ Conversation by voice (Gemini Live)

/// What a spoken session needs when there is no Gemini key.
pub const NEED_VOICE_KEY: &str = "Speaking needs a free Gemini key: make one at aistudio.google.com/apikey and paste it in Conversation > Conversation by voice, or in Preferences > Addons & accounts > Gemini.";
const NO_SOUND: &str = "This build of Reelhouse cannot record or play sound (Qt Multimedia is missing), and Gemini Live needs it.";
/// A learner quiet this long after the tutor spoke gets a nudge (the tutor asks an easy question or gives a hint).
const IDLE_NUDGE: Duration = Duration::from_secs(25);
/// After end_scene the call waits this long for the goodbye (and for it to play), then hangs up.
const GOODBYE_WAIT: Duration = Duration::from_secs(12);
/// The number of the next call.
static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A call of Gemini Live in progress, for the session `sid`. The words of the turn in progress live here until they
/// become lines of the session (the learner's when the tutor starts answering, the tutor's at the end of its turn).
pub struct LiveCall {
    /// the number of this call (the events of an older call of the same session are told apart by it)
    id: u64,
    pub sid: i64,
    handle: live::Handle,
    /// "connecting", "live", "reconnecting"
    pub phase: String,
    pub model: String,
    pub voice: String,
    pub user: String,
    pub tutor: String,
    /// the tutor is in its turn, and the learner's line of this turn (late words of the transcription go there)
    in_turn: bool,
    turn_user: Option<usize>,
    /// what the learner said before this turn of the tutor, for the coach
    heard: String,
    coach: Coach,
    pub latency: Option<u64>,
    last_tutor_end: Option<Instant>,
    nudged: bool,
    /// end_scene was called, and the tutor said its goodbye (the call hangs up once that has played)
    ending: Option<Instant>,
    bye_said: bool,
    ready_once: bool,
    pub started: Instant,
    /// a problem of the sound devices, shown in the call
    pub warn: String,
}

impl Core {
    fn live_log(&mut self, text: &str) {
        crate::debug::log("live", text);
        self.events.push(Event::Log(format!("[live] {}", text)));
    }

    fn live_send(&self, c: live::Cmd) {
        if let Some(call) = &self.kaiwa.live {
            call.handle.send(c);
        }
    }

    /// A call runs for the session on screen.
    fn kaiwa_live_open(&self) -> bool {
        self.kaiwa.live.as_ref().map(|c| c.sid == self.kaiwa.open).unwrap_or(false)
    }

    /// Why a spoken session cannot start now (None: it can).
    fn kaiwa_live_missing(&self) -> Option<&'static str> {
        if llm::gemini_key(&self.settings.subtitles).is_none() {
            return Some(NEED_VOICE_KEY);
        }
        if !live::hub().present() {
            return Some(NO_SOUND);
        }
        None
    }

    /// Whether a new session of this mode is spoken: the voice call always, a reading story never, the rest when
    /// "speak" is on; and only when Gemini Live can run.
    fn kaiwa_wants_live(&self, mode: &str) -> bool {
        let asked = match mode {
            "story" => false,
            "call" => true,
            _ => self.settings.general.kaiwa_speak,
        };
        asked && self.kaiwa_live_missing().is_none()
    }

    fn live_vad(&self) -> live::Vad {
        let g = &self.settings.general;
        live::Vad { manual: g.kaiwa_live_mic == "ptt", silence_ms: g.kaiwa_live_pause, prefix_ms: 200 }
    }

    /// Starts (or starts again) the call of the session `sid`.
    fn kaiwa_live_begin(&mut self, sid: i64) {
        if let Some(m) = self.kaiwa_live_missing() {
            self.kaiwa.error = m.into();
            self.events.push(Event::Study);
            return;
        }
        let Some(s) = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).cloned() else { return };
        let Some(key) = llm::gemini_key(&self.settings.subtitles) else { return };
        self.kaiwa_live_hangup();
        let ctx = self.kaiwa_ctx(&s);
        let (chosen, voice_set, headset, mic, pause) = {
            let g = &self.settings.general;
            (g.kaiwa_live_model.trim().to_string(), g.kaiwa_live_voice.clone(), g.kaiwa_live_headset, g.kaiwa_live_mic.clone(), g.kaiwa_live_pause)
        };
        let models: Vec<String> = if chosen.is_empty() {
            std::iter::once(live::DEFAULT_MODEL).chain(live::FALLBACK_MODELS.iter().copied()).map(String::from).collect()
        } else {
            vec![chosen]
        };
        let voice = kaiwa_live::voice_for(&s.lang, &voice_set);
        let code = kaiwa_live::bcp47(&s.lang);
        let mut hear = vec![code.clone()];
        let ui = kaiwa_live::bcp47(&ctx.ui);
        if !ui.is_empty() && ui != code {
            hear.push(ui);
        }
        let cfg = live::Config {
            key,
            models,
            voice: voice.clone(),
            language_code: code,
            hear_langs: hear,
            system: kaiwa_live::live_system(&ctx, &s.goals),
            tools: kaiwa_live::tools(&ctx),
            vad: self.live_vad(),
            opening: if s.lines.is_empty() { kaiwa_live::opening(&s.mode) } else { kaiwa_live::again_note(&s) },
            headset,
        };
        let tx = self.tx.clone();
        let id = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        live::hub().set_device(&self.settings.general.voice_mic);
        let handle = live::start(cfg, live::hub(), Box::new(move |ev| send(&tx, Msg::Kaiwa(KaiwaMsg::Live(id, ev)))));
        // the voice of the written tutor stops
        if let Some(m) = &self.study_audio {
            let _ = m.cmd(&["stop"]);
        }
        self.kaiwa.live = Some(LiveCall {
            id,
            sid,
            handle,
            phase: "connecting".into(),
            model: String::new(),
            voice: voice.clone(),
            user: String::new(),
            tutor: String::new(),
            in_turn: false,
            turn_user: None,
            heard: String::new(),
            coach: Coach::new(&s.lang, &ctx.ui, &s.level),
            latency: None,
            last_tutor_end: None,
            nudged: false,
            ending: None,
            bye_said: false,
            ready_once: false,
            started: Instant::now(),
            warn: String::new(),
        });
        self.kaiwa.error.clear();
        self.kaiwa.hints = Json::Null;
        let how = if mic == "ptt" { "push-to-talk".to_string() } else { format!("open microphone{}, a {} ms pause ends a turn", if headset { " with headphones" } else { " (muted while the tutor speaks)" }, pause) };
        let line = format!("call for \"{}\" ({}, {} {}, {} goals): voice {}, {}", s.title, s.mode, s.lang, s.level, s.goals.len(), voice, how);
        self.live_log(&line);
        self.events.push(Event::Study);
    }

    /// Hangs up (the session stays open: it can be called again, or ended for its report).
    fn kaiwa_live_hangup(&mut self) {
        if self.kaiwa.live.is_none() {
            return;
        }
        self.live_commit_user();
        self.live_commit_tutor(true);
        if let Some(call) = self.kaiwa.live.take() {
            call.handle.send(live::Cmd::Stop);
            let line = format!("hung up after {} s", call.started.elapsed().as_secs());
            self.live_log(&line);
        }
        self.kaiwa.hints_busy = false;
        self.kaiwa_save();
        self.events.push(Event::Study);
    }

    /// The app closes: the call stops (its thread closes the socket).
    pub fn kaiwa_live_shutdown(&mut self) {
        self.kaiwa.live = None;
    }

    /// The learner's words of this turn become a line of the session (when the tutor starts answering, or before a
    /// tool needs the line).
    fn live_commit_user(&mut self) {
        let now = Self::now_secs();
        let (sid, idx) = {
            let KaiwaState { live, store, .. } = &mut self.kaiwa;
            let Some(call) = live.as_mut() else { return };
            let text = call.user.trim().to_string();
            call.user.clear();
            if text.is_empty() {
                return;
            }
            let Some(s) = store.sessions.iter_mut().find(|s| s.id == call.sid) else { return };
            s.lines.push(Line { role: "user".into(), text: text.clone(), at: now, ..Line::default() });
            // a point for each turn the learner speaks
            s.score += 1;
            call.turn_user = Some(s.lines.len() - 1);
            call.heard = if call.heard.is_empty() { text } else { format!("{} {}", call.heard, text) };
            (s.id, s.lines.len() - 1)
        };
        self.live_annotate(sid, idx);
    }

    /// The tutor's words of this turn become a line (`cut`: it was interrupted).
    fn live_commit_tutor(&mut self, cut: bool) {
        let now = Self::now_secs();
        let (sid, idx) = {
            let KaiwaState { live, store, .. } = &mut self.kaiwa;
            let Some(call) = live.as_mut() else { return };
            let mut text = call.tutor.trim().to_string();
            call.tutor.clear();
            if text.is_empty() {
                return;
            }
            if cut {
                text.push_str(" …");
            }
            let Some(s) = store.sessions.iter_mut().find(|s| s.id == call.sid) else { return };
            s.lines.push(Line { role: "assistant".into(), text, at: now, ..Line::default() });
            (s.id, s.lines.len() - 1)
        };
        self.live_annotate(sid, idx);
    }

    /// The reading in kana, romaji and translation of a spoken line, when the page shows any of them (Gemini, text).
    fn live_annotate(&mut self, sid: i64, idx: usize) {
        let g = &self.settings.general;
        if !(g.kaiwa_reading || g.kaiwa_romaji || g.kaiwa_translation) {
            return;
        }
        let Some(cfg) = llm::gemini_text(&self.settings.subtitles) else { return };
        let Some(s) = self.kaiwa.store.sessions.iter().find(|s| s.id == sid) else { return };
        let Some(l) = s.lines.get(idx) else { return };
        if !l.translation.is_empty() || l.text.trim().is_empty() {
            return;
        }
        let prompt = kaiwa_live::annotate_prompt(&self.kaiwa_ctx(s), &l.text);
        self.spawn(move |tx, http| {
            let r = llm::chat(&*http, &cfg, "You answer with JSON only.", &[llm::Turn::new("user", &prompt)], true).and_then(|t| llm::json_of(&t).ok_or_else(|| "The reading was not readable.".to_string()));
            send(tx, Msg::Kaiwa(KaiwaMsg::Annot(sid, idx, r)));
        });
    }

    /// The readings of the lines of the spoken session on screen that have none yet (the page turned one of the
    /// options on).
    pub fn kaiwa_annotate(&mut self) {
        let sid = self.kaiwa.open;
        let todo: Vec<usize> = match self.kaiwa.store.sessions.iter().find(|s| s.id == sid && s.live) {
            Some(s) => s.lines.iter().enumerate().filter(|(_, l)| l.translation.is_empty() && !l.text.trim().is_empty()).map(|(i, _)| i).collect(),
            None => return,
        };
        let from = todo.len().saturating_sub(30);
        for &i in &todo[from..] {
            self.live_annotate(sid, i);
        }
    }

    /// The tutor starts its turn: what the learner said becomes a line.
    fn live_turn_starts(&mut self) {
        self.live_commit_user();
        if let Some(c) = self.kaiwa.live.as_mut() {
            c.in_turn = true;
            c.nudged = false;
        }
        self.events.push(Event::Study);
    }

    /// The tutor ended its turn: its words become a line, the coach reads the turn, a scene that ended hangs up.
    fn live_turn_ends(&mut self) {
        self.live_commit_user();
        let tutor_text = self.kaiwa.live.as_ref().map(|c| c.tutor.trim().to_string()).unwrap_or_default();
        self.live_commit_tutor(false);
        let Some(c) = self.kaiwa.live.as_mut() else { return };
        c.in_turn = false;
        c.turn_user = None;
        c.last_tutor_end = Some(Instant::now());
        c.nudged = false;
        let heard = std::mem::take(&mut c.heard);
        if c.ending.is_some() && !tutor_text.is_empty() {
            // the goodbye was said: the tick hangs up once it has played
            c.bye_said = true;
        }
        let sid = c.sid;
        let goals = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).map(|s| s.goals.clone()).unwrap_or_default();
        let note = self.kaiwa.live.as_mut().and_then(|c| c.coach.after_turn(&heard, &tutor_text, &goals));
        if let Some(n) = note {
            self.live_log(&format!("coach: {}", n));
            self.live_send(live::Cmd::Note { text: n, respond: false });
        }
        self.kaiwa.hints_busy = false;
        self.kaiwa_save();
        self.events.push(Event::Study);
    }

    /// A function the tutor called: applied to the session, answered at once.
    fn live_tool(&mut self, sid: i64, call: live::Call) {
        self.live_commit_user();
        let now = Self::now_secs();
        let lang = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).map(|s| s.lang.clone()).unwrap_or_default();
        let (answer, effect) = match self.kaiwa.store.sessions.iter_mut().find(|s| s.id == sid) {
            Some(s) => kaiwa_live::apply_tool(s, &call.name, &call.args, now),
            None => (Json::obj(vec![("error", Json::Str("the session is gone".into()))]), Effect::None),
        };
        match effect {
            Effect::Correction(e) => {
                let check = Json::obj(vec![("errors", Json::Arr(vec![e]))]);
                self.kaiwa.store.remember(&check, &lang, now);
            }
            Effect::Goal { index, all } => {
                if let Some(c) = self.kaiwa.live.as_mut() {
                    c.coach.goal_reached();
                }
                self.live_log(&format!("goal {} reached{}", index + 1, if all { " (all goals)" } else { "" }));
            }
            Effect::Hint(h) => {
                self.kaiwa.hints = Json::Arr(vec![h]);
                self.kaiwa.hints_busy = false;
            }
            Effect::End { success, summary } => {
                if let Some(c) = self.kaiwa.live.as_mut() {
                    c.ending = Some(Instant::now());
                }
                if let Some(s) = self.kaiwa.store.sessions.iter_mut().find(|s| s.id == sid) {
                    if s.report.is_null() && !summary.is_empty() {
                        s.report = Json::obj(vec![("summary", Json::Str(summary))]);
                    }
                }
                self.live_log(&format!("the tutor ends the scene ({})", if success { "goals reached" } else { "goals not reached" }));
            }
            Effect::None => {}
        }
        self.live_send(live::Cmd::Tools(vec![(call.id, call.name, answer)]));
        self.kaiwa_save();
        self.events.push(Event::Study);
    }

    fn on_live(&mut self, id: u64, ev: live::Event) {
        if let live::Event::Log(t) = &ev {
            // the worker wrote it to the debug report already
            self.events.push(Event::Log(format!("[live] {}", t)));
            return;
        }
        let Some(sid) = self.kaiwa.live.as_ref().filter(|c| c.id == id).map(|c| c.sid) else {
            // an old call (hung up): only its end matters, for the log
            if let live::Event::Closed(Some(e)) = ev {
                self.live_log(&format!("an earlier call ended: {}", e));
            }
            return;
        };
        match ev {
            live::Event::Log(_) | live::Event::ToolCancel(_) => {}
            live::Event::Phase(p) => {
                if let Some(c) = self.kaiwa.live.as_mut() {
                    c.phase = p;
                }
                self.events.push(Event::Study);
            }
            live::Event::Ready { model, resumed, .. } => {
                let fresh_again = match self.kaiwa.live.as_mut() {
                    Some(c) => {
                        let again = c.ready_once && !resumed;
                        c.ready_once = true;
                        c.model = model;
                        again
                    }
                    None => false,
                };
                if fresh_again {
                    // the old session could not be resumed: the new one hears the last lines to go on
                    if let Some(note) = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).map(kaiwa_live::recap_note) {
                        self.live_send(live::Cmd::Note { text: note, respond: false });
                    }
                }
                self.events.push(Event::Study);
            }
            live::Event::InputText(t) => {
                let KaiwaState { live, store, .. } = &mut self.kaiwa;
                if let Some(c) = live.as_mut() {
                    c.nudged = false;
                    let late = match (c.in_turn, c.turn_user) {
                        (true, Some(i)) => store.sessions.iter_mut().find(|s| s.id == sid).and_then(|s| s.lines.get_mut(i)),
                        _ => None,
                    };
                    match late {
                        // the transcription of the learner's turn came after the tutor began
                        Some(l) => {
                            l.text.push_str(&t);
                            c.heard.push_str(&t);
                        }
                        None => c.user.push_str(&t),
                    }
                }
            }
            live::Event::FirstAudio(lat) => {
                let starts = self.kaiwa.live.as_ref().map(|c| !c.in_turn).unwrap_or(false);
                if starts {
                    self.live_turn_starts();
                }
                if let Some(c) = self.kaiwa.live.as_mut() {
                    c.latency = lat.or(c.latency);
                }
            }
            live::Event::OutputText(t) => {
                let starts = self.kaiwa.live.as_ref().map(|c| !c.in_turn).unwrap_or(false);
                if starts {
                    self.live_turn_starts();
                }
                if let Some(c) = self.kaiwa.live.as_mut() {
                    c.tutor.push_str(&t);
                }
            }
            live::Event::Interrupted => {
                self.live_commit_tutor(true);
                if let Some(c) = self.kaiwa.live.as_mut() {
                    c.in_turn = false;
                    c.turn_user = None;
                }
                self.kaiwa_save();
                self.events.push(Event::Study);
            }
            live::Event::TurnComplete => self.live_turn_ends(),
            live::Event::ToolCall(call) => self.live_tool(sid, call),
            live::Event::Closed(err) => {
                self.live_commit_user();
                self.live_commit_tutor(true);
                let ending = self.kaiwa.live.as_ref().map(|c| c.ending.is_some()).unwrap_or(false);
                self.kaiwa.live = None;
                self.kaiwa.hints_busy = false;
                if let Some(e) = err {
                    self.live_log(&format!("call ended: {}", e));
                    if self.kaiwa.open == sid {
                        self.kaiwa.error = e;
                    }
                }
                self.kaiwa_save();
                if ending {
                    self.kaiwa_end_session(sid);
                }
                self.events.push(Event::Study);
            }
        }
    }

    /// Every tick: the goodbye of an ended scene, a nudge after a long silence, the sound devices' errors.
    pub fn kaiwa_live_tick(&mut self) {
        let Some(c) = self.kaiwa.live.as_ref() else { return };
        let sid = c.sid;
        let hub = live::hub();
        if let Some(t) = c.ending {
            // the scene is over: hang up once the goodbye has played (or it never came)
            let played = c.bye_said && !hub.speaking();
            let waited = t.elapsed() > GOODBYE_WAIT && !hub.speaking();
            if played || waited || t.elapsed() > GOODBYE_WAIT * 3 {
                self.kaiwa_end_session(sid);
            }
            return;
        }
        let err = hub.take_device_error();
        if !err.is_empty() {
            self.live_log(&format!("sound devices: {}", err));
            if let Some(c) = self.kaiwa.live.as_mut() {
                c.warn = err;
            }
            self.events.push(Event::Study);
        }
        let manual = self.settings.general.kaiwa_live_mic == "ptt";
        let Some(c) = self.kaiwa.live.as_ref() else { return };
        // held push-to-talk, or an open microphone the learner switched off: they are busy or paused on purpose
        let learner_busy = if manual { hub.is_open() } else { !hub.is_open() };
        let voice_quiet = hub.now_ms().saturating_sub(hub.last_voice_ms()) > 8000;
        let quiet = c.phase == "live" && !c.in_turn && !c.nudged && c.user.trim().is_empty() && !hub.speaking() && !learner_busy && voice_quiet && c.last_tutor_end.map(|t| t.elapsed() > IDLE_NUDGE).unwrap_or(false);
        if quiet {
            let goals = self.kaiwa.store.sessions.iter().find(|s| s.id == sid).map(|s| s.goals.clone()).unwrap_or_default();
            let note = match self.kaiwa.live.as_mut() {
                Some(c) => {
                    c.nudged = true;
                    c.coach.idle_note(&goals)
                }
                None => return,
            };
            self.live_log("the learner is quiet: the tutor is asked to help");
            self.live_send(live::Cmd::Note { text: note, respond: true });
        }
    }

    /// Push-to-talk pressed / released; the open microphone switched on / off.
    pub fn kaiwa_live_talk(&mut self, down: bool) {
        if let Some(c) = self.kaiwa.live.as_mut() {
            c.nudged = false;
        }
        self.live_send(live::Cmd::Talk(down));
    }

    /// Text typed during a call: a line of the learner, sent as their turn.
    pub fn kaiwa_live_text(&mut self, text: &str) {
        let text = text.trim().to_string();
        if text.is_empty() || self.kaiwa.live.is_none() {
            return;
        }
        self.live_commit_user();
        let now = Self::now_secs();
        let idx = {
            let KaiwaState { live, store, .. } = &mut self.kaiwa;
            let Some(c) = live.as_mut() else { return };
            let Some(s) = store.sessions.iter_mut().find(|s| s.id == c.sid) else { return };
            s.lines.push(Line { role: "user".into(), text: text.clone(), at: now, ..Line::default() });
            s.score += 1;
            c.heard = text.clone();
            c.turn_user = None;
            c.nudged = false;
            s.lines.len() - 1
        };
        self.live_send(live::Cmd::Text(text));
        self.kaiwa.hints = Json::Null;
        self.kaiwa_save();
        let sid = self.kaiwa.open;
        self.live_annotate(sid, idx);
        self.events.push(Event::Study);
    }

    /// Calls (again) the spoken session on screen.
    pub fn kaiwa_live_call(&mut self) {
        let sid = self.kaiwa.open;
        if self.kaiwa.store.sessions.iter().any(|s| s.id == sid && s.ended == 0) {
            if let Some(s) = self.kaiwa.store.sessions.iter_mut().find(|s| s.id == sid) {
                if !s.live {
                    // a written session goes on by voice
                    s.live = true;
                }
            }
            self.kaiwa_live_begin(sid);
        }
    }

    pub fn kaiwa_live_stop(&mut self) {
        self.kaiwa_live_hangup();
    }

    /// The options of Conversation by voice: `speak`, `mic` ("ptt" / "open"), `headset`, `pause` (ms), `voice`,
    /// `model`. A call in progress follows the microphone options at once (reconnecting when the setup must change).
    pub fn kaiwa_live_opts(&mut self, j: &Json) {
        {
            let g = &mut self.settings.general;
            if let Some(v) = j.get("speak").as_bool() {
                g.kaiwa_speak = v;
            }
            if let Some(m) = j.get("mic").as_str() {
                g.kaiwa_live_mic = if m == "ptt" { "ptt" } else { "open" }.into();
            }
            if let Some(h) = j.get("headset").as_bool() {
                g.kaiwa_live_headset = h;
            }
            if let Some(p) = j.get("pause").as_f64() {
                g.kaiwa_live_pause = p.clamp(500.0, 3000.0) as u32;
            }
            if let Some(v) = j.get("voice").as_str() {
                g.kaiwa_live_voice = v.trim().into();
            }
            if let Some(m) = j.get("model").as_str() {
                g.kaiwa_live_model = m.trim().into();
            }
        }
        self.save_settings();
        let (vad, headset) = (self.live_vad(), self.settings.general.kaiwa_live_headset);
        self.live_send(live::Cmd::Mode { vad, headset });
        self.events.push(Event::Study);
    }

    /// The options and the state of Conversation by voice, for the page (in `kaiwa_json`).
    fn kaiwa_live_info(&self, lang: &str) -> Json {
        let g = &self.settings.general;
        let hub = live::hub();
        Json::obj(vec![
            ("available", Json::Bool(self.kaiwa_live_missing().is_none())),
            ("missing", Json::Str(self.kaiwa_live_missing().unwrap_or("").into())),
            ("hasKey", Json::Bool(llm::gemini_key(&self.settings.subtitles).is_some())),
            ("audio", Json::Bool(hub.present())),
            ("speak", Json::Bool(g.kaiwa_speak)),
            ("model", Json::Str(if g.kaiwa_live_model.is_empty() { live::DEFAULT_MODEL.into() } else { g.kaiwa_live_model.clone() })),
            ("modelSet", Json::Str(g.kaiwa_live_model.clone())),
            ("voice", Json::Str(kaiwa_live::voice_for(lang, &g.kaiwa_live_voice))),
            ("voiceSet", Json::Str(g.kaiwa_live_voice.clone())),
            ("voices", Json::Arr(live::VOICES.iter().map(|v| Json::Str(v.to_string())).collect())),
            ("mic", Json::Str(g.kaiwa_live_mic.clone())),
            ("headset", Json::Bool(g.kaiwa_live_headset)),
            ("pause", Json::Num(g.kaiwa_live_pause as f64)),
            ("keyUrl", Json::Str(llm::GEMINI_KEY_URL.into())),
            ("running", Json::Bool(self.kaiwa.live.is_some())),
            ("callSid", Json::Num(self.kaiwa.live.as_ref().map(|c| c.sid as f64).unwrap_or(0.0))),
        ])
    }

    /// The call in progress, small enough to be read several times a second by the page: the phase, the words of
    /// the turn in progress, the microphone level, whether the tutor is speaking.
    pub fn kaiwa_live_json(&self) -> Json {
        let Some(c) = &self.kaiwa.live else {
            return Json::obj(vec![("running", Json::Bool(false)), ("error", Json::Str(self.kaiwa.error.clone()))]);
        };
        let hub = live::hub();
        let g = &self.settings.general;
        Json::obj(vec![
            ("running", Json::Bool(true)),
            ("sid", Json::Num(c.sid as f64)),
            ("phase", Json::Str(c.phase.clone())),
            ("model", Json::Str(c.model.clone())),
            ("voice", Json::Str(c.voice.clone())),
            ("mic", Json::Str(g.kaiwa_live_mic.clone())),
            ("headset", Json::Bool(g.kaiwa_live_headset)),
            ("open", Json::Bool(hub.is_open())),
            ("level", Json::Num(hub.level() as f64 / 100.0)),
            ("speaking", Json::Bool(hub.speaking())),
            ("muted", Json::Bool(hub.muted())),
            ("user", Json::Str(c.user.clone())),
            ("tutor", Json::Str(c.tutor.clone())),
            ("inTurn", Json::Bool(c.in_turn)),
            ("latency", c.latency.map(|l| Json::Num(l as f64)).unwrap_or(Json::Null)),
            ("seconds", Json::Num(c.started.elapsed().as_secs() as f64)),
            ("ending", Json::Bool(c.ending.is_some())),
            ("warn", Json::Str(c.warn.clone())),
            ("error", Json::Str(self.kaiwa.error.clone())),
        ])
    }
}

