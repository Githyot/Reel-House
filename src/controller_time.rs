//! Study time and the Pomodoro timer inside the core (the counting rules, the log file, sounds and banners).
//! The arithmetic lives in timetrack.rs; this file is part of `controller` (it needs the private parts of `Core`).
//!
//! What counts as study time, one second at a time, for the most specific activity going on:
//! - a quiz card of the player on screen ("quiz");
//! - in the Study mode, a Learn lesson ("lesson") or a flashcard review ("review");
//! - a Pomodoro focus session running, in any mode ("pomodoro");
//! - the Study mode open otherwise ("study").
//!
//! Except for the Pomodoro, the user must have done something in the last two minutes (a Study or Learn command,
//! or a ping of the window saying the pointer moved over the Study page) and the window must have the focus.
use super::*;
use timetrack::{PomoConfig, PomoEnd, Pomodoro, TimeLog};

/// Without input for this long, the time is not counted (the user went away).
const IDLE_AFTER: i64 = 120;
/// The window's focus report is trusted this long; after that (no Study page on screen) it is assumed focused.
const PING_TRUST: i64 = 90;
/// A gap longer than this between two ticks (the computer slept, the app hung) is not study time.
const MAX_GAP_MS: i64 = 10_000;
/// The log is written at most this often while time is being counted.
const SAVE_EVERY: i64 = 30;

/// The built-in sounds, kept inside the program and written to `<data dir>/sounds/` the first time they play.
const SOUNDS: [(&str, &[u8]); 3] = [
    ("pomo_focus_end", include_bytes!("../assets/sounds/pomo_focus_end.ogg")),
    ("pomo_break_end", include_bytes!("../assets/sounds/pomo_break_end.ogg")),
    ("pomo_tick", include_bytes!("../assets/sounds/pomo_tick.ogg")),
];

pub struct TimeState {
    pub log: TimeLog,
    pub pomo: Pomodoro,
    loaded: bool,
    /// The last tick (unix ms) and the milliseconds not yet counted.
    last_ms: i64,
    carry_ms: i64,
    /// The last thing the user did in Study (unix seconds).
    pub last_input: i64,
    /// What the window said about its focus, and when.
    focused: bool,
    ping_at: i64,
    /// The specific activity seen at the last tick: a new one counts as input, so a quiz card counts from the start.
    last_activity: &'static str,
    dirty: bool,
    saved_at: i64,
    /// The report the Time page shows: (period, offset).
    pub view: (String, i64),
    /// The Pomodoro settings seen last (new lengths reach a phase not started yet).
    cfg: PomoConfig,
    /// Players of the chimes and of the ticking (separate from the card sounds, which they would cut).
    chime: Option<Mpv>,
    ticker: Option<Mpv>,
    ticking: bool,
}

impl Default for TimeState {
    fn default() -> Self {
        let cfg = PomoConfig::default();
        TimeState {
            log: TimeLog::default(),
            pomo: Pomodoro::new(&cfg),
            loaded: false,
            last_ms: 0,
            carry_ms: 0,
            last_input: 0,
            focused: true,
            ping_at: 0,
            last_activity: "",
            dirty: false,
            saved_at: 0,
            view: ("week".into(), 0),
            cfg,
            chime: None,
            ticker: None,
            ticking: false,
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Core {
    fn time_file(&self) -> PathBuf {
        crate::study::dir(&self.profile_dir).join("time.json")
    }

    fn time_ensure(&mut self) {
        if !self.time.loaded {
            self.time.log = TimeLog::load(&self.time_file());
            self.time.loaded = true;
            self.time.last_ms = now_ms();
            let cfg = self.pomo_config();
            self.time.cfg = cfg;
            self.time.pomo = Pomodoro::new(&cfg);
        }
    }

    fn time_save(&mut self) {
        if self.time.loaded {
            self.time.log.save(&self.time_file());
            self.time.dirty = false;
            self.time.saved_at = now_ms() / 1000;
        }
    }

    pub fn pomo_config(&self) -> PomoConfig {
        let g = &self.settings.general;
        PomoConfig { focus: g.pomo_focus, short: g.pomo_short, long: g.pomo_long, rounds: g.pomo_rounds }
    }

    /// The window's time zone (minutes east of UTC); the day boundaries follow it.
    pub fn time_set_tz(&mut self, tz: Option<i64>) {
        self.time_ensure();
        if let Some(tz) = tz {
            let tz = tz.clamp(-14 * 60, 14 * 60);
            if tz != self.time.log.tz {
                self.time.log.tz = tz;
                self.time.dirty = true;
            }
        }
    }

    /// The user did something in Study (any Study or Learn command goes through here).
    pub fn time_input(&mut self) {
        self.time.last_input = now_ms() / 1000;
    }

    /// The Study page reports now and then: window focus, whether the pointer moved since the last report.
    pub fn time_ping(&mut self, focused: bool, input: bool, tz: Option<i64>) {
        self.time_set_tz(tz);
        let now = now_ms() / 1000;
        self.time.focused = focused;
        self.time.ping_at = now;
        if input && focused {
            self.time.last_input = now;
        }
    }

    /// Seconds studied today (for the reminders and the Study home).
    pub fn study_seconds_today(&self) -> u64 {
        let now = now_ms() / 1000;
        self.time.log.day_total(timetrack::day_of(now, self.time.log.tz))
    }

    /// What the current second is spent on, if it is study.
    fn time_activity(&mut self, now: i64) -> Option<&'static str> {
        let study_mode = self.mode() == "study";
        let specific: Option<&'static str> = if self.study_play.quiz.is_some() {
            Some("quiz")
        } else if study_mode && self.learn.session.is_some() {
            Some("lesson")
        } else if study_mode && self.study_session.is_some() {
            Some("review")
        } else {
            None
        };
        // a quiz card popping up, a lesson or review opening: the user is there
        let id = specific.unwrap_or("");
        if !id.is_empty() && id != self.time.last_activity {
            self.time.last_input = now;
        }
        self.time.last_activity = id;
        let focused = self.time.focused || now - self.time.ping_at > PING_TRUST;
        let active = focused && now - self.time.last_input <= IDLE_AFTER;
        match specific {
            Some(a) if active => Some(a),
            _ if self.time.pomo.in_focus() => Some("pomodoro"),
            _ if study_mode && active => Some("study"),
            _ => None,
        }
    }

    /// Called from `poll` (every few tens of milliseconds): the Pomodoro, then the seconds of study.
    pub(super) fn time_tick(&mut self) {
        self.time_ensure();
        let now_ms = now_ms();
        let now = now_ms / 1000;
        // the settings of the timer changed (Preferences or its popover)
        let cfg = self.pomo_config();
        if cfg != self.time.cfg {
            self.time.cfg = cfg;
            self.time.pomo.configure(&cfg);
            self.events.push(Event::Study);
        }
        if let Some(end) = self.time.pomo.tick(now_ms, &cfg, self.settings.general.pomo_auto) {
            self.pomo_ended(end, now);
        }
        self.pomo_sync_ticker();

        let delta = now_ms - self.time.last_ms;
        if delta < 1000 {
            return;
        }
        self.time.last_ms = now_ms;
        if !(0..=MAX_GAP_MS).contains(&delta) {
            self.time.carry_ms = 0;
            return;
        }
        self.time.carry_ms += delta;
        let secs = (self.time.carry_ms / 1000) as u64;
        self.time.carry_ms %= 1000;
        if let Some(a) = self.time_activity(now) {
            self.time.log.add(now, secs, a);
            self.time.dirty = true;
        }
        if self.time.dirty && now - self.time.saved_at >= SAVE_EVERY {
            self.time_save();
        }
    }

    /// The app is closing: keep the last seconds.
    pub fn time_flush(&mut self) {
        if self.time.dirty {
            self.time_save();
        }
    }

    /// Another profile is in use (`time_flush` kept the last seconds of the one before): its own log is read when
    /// time is next counted.
    pub(super) fn time_profile_changed(&mut self) {
        self.time.loaded = false;
        self.time.log = TimeLog::default();
        self.time.dirty = false;
    }

    // ---------- reports ----------

    /// Which report the Time page shows: period "day" | "week" | "month" | "year", offset 0 = now, -1 = before…
    pub fn studytime_view(&mut self, period: &str, offset: i64, tz: Option<i64>) {
        self.time_set_tz(tz);
        let period = if ["day", "week", "month", "year"].contains(&period) { period } else { "week" };
        self.time.view = (period.to_string(), offset.clamp(-1000, 0));
    }

    /// State "studytime": totals of today / this week / month / year, the streak, the last 7 days and the report
    /// chosen with `studytime_view`.
    pub fn studytime_json(&self) -> Json {
        let now = now_ms() / 1000;
        let log = &self.time.log;
        let today = timetrack::day_of(now, log.tz);
        let (y, m, _) = timetrack::ymd(today);
        let week0 = today - timetrack::weekday(today);
        let month0 = timetrack::day_from_ymd(y, m, 1);
        let year0 = timetrack::day_from_ymd(y, 1, 1);
        let num = |v: u64| Json::Num(v as f64);
        let last7: Vec<Json> = (today - 6..=today).map(|d| Json::obj(vec![("date", Json::Str(timetrack::date_key(d))), ("secs", num(log.day_total(d)))])).collect();
        Json::obj(vec![
            ("tz", Json::Num(log.tz as f64)),
            ("todayDate", Json::Str(timetrack::date_key(today))),
            ("today", num(log.day_total(today))),
            ("week", num(log.sum(week0, today).total)),
            ("month", num(log.sum(month0, today).total)),
            ("year", num(log.sum(year0, today).total)),
            ("all", num(log.days.values().map(|d| d.total()).sum())),
            ("streak", num(log.streak(today) as u64)),
            ("longest", num(log.longest_streak() as u64)),
            ("todayPomos", num(log.days.get(&today).map(|d| d.pomos as u64).unwrap_or(0))),
            ("last7", Json::Arr(last7)),
            ("view", log.report(now, &self.time.view.0, self.time.view.1)),
        ])
    }

    /// Writes the whole log as CSV (one line per day).
    pub fn studytime_export(&mut self, path: &Path) {
        self.time_ensure();
        let path = if path.extension().is_none() { path.with_extension("csv") } else { path.to_path_buf() };
        match std::fs::write(&path, self.time.log.csv()) {
            Ok(_) => self.toast("Study time exported."),
            Err(e) => self.toast(&format!("The file could not be written: {}", e)),
        }
    }

    // ---------- the Pomodoro ----------

    /// `pomo {op}`: start | pause | toggle | reset | skip (any other op, e.g. "sync", only passes the time zone).
    pub fn pomo_cmd(&mut self, op: &str, tz: Option<i64>) {
        self.time_set_tz(tz);
        let now = now_ms();
        let cfg = self.pomo_config();
        let p = &mut self.time.pomo;
        match op {
            "start" => p.start(now, &cfg),
            "pause" => p.pause(now),
            "toggle" => {
                if p.running {
                    p.pause(now)
                } else {
                    p.start(now, &cfg)
                }
            }
            "reset" => p.reset(&cfg),
            "skip" => p.skip(now, &cfg),
            _ => return,
        }
        self.pomo_sync_ticker();
        self.events.push(Event::Study);
    }

    /// State "pomodoro". Times are unix ms: the window counts down from `endsAt` by itself.
    pub fn pomo_json(&self) -> Json {
        let p = &self.time.pomo;
        let g = &self.settings.general;
        let cfg = self.pomo_config();
        let now = now_ms();
        let today = timetrack::day_of(now / 1000, self.time.log.tz);
        Json::obj(vec![
            ("phase", Json::Str(p.phase.id().into())),
            ("running", Json::Bool(p.running)),
            ("fresh", Json::Bool(p.fresh)),
            ("endsAt", Json::Num(if p.running { p.ends_at as f64 } else { 0.0 })),
            ("left", Json::Num(p.remaining(now) as f64)),
            ("length", Json::Num(cfg.ms(p.phase) as f64)),
            ("done", Json::Num(p.done as f64)),
            ("rounds", Json::Num(cfg.rounds as f64)),
            ("focus", Json::Num(cfg.focus as f64)),
            ("short", Json::Num(cfg.short as f64)),
            ("long", Json::Num(cfg.long as f64)),
            ("auto", Json::Bool(g.pomo_auto)),
            ("tick", Json::Bool(g.pomo_tick)),
            ("sound", Json::Bool(g.pomo_sound)),
            ("todayPomos", Json::Num(self.time.log.days.get(&today).map(|d| d.pomos as f64).unwrap_or(0.0))),
        ])
    }

    /// The UI text in the app's language.
    fn time_tr(&self, english: &str) -> String {
        let lang = match self.ui_lang().as_str() {
            "" | "system" => util::lang_base(&self.system_lang),
            other => other.to_string(),
        };
        crate::i18n::translate(&lang, english).unwrap_or(english).to_string()
    }

    /// A phase ended: count the focus session, ring, and show the macaw.
    fn pomo_ended(&mut self, end: PomoEnd, now: i64) {
        let cfg = self.pomo_config();
        let auto = self.settings.general.pomo_auto;
        // clicking the banner starts the next phase (when it does not start by itself)
        let action = if auto { None } else { Some(Json::obj(vec![("cmd", "pomo".into()), ("op", "start".into())])) };
        let (title, body, mood, sound) = match end {
            PomoEnd::Focus { long } => {
                self.time.log.add_pomo(now);
                self.time.dirty = true;
                if long {
                    (self.time_tr("Time for a long break"), self.time_tr("{n} focus sessions done. Rest for {m} minutes.").replace("{n}", &cfg.rounds.to_string()).replace("{m}", &cfg.long.to_string()), "tired", "pomo_focus_end")
                } else {
                    (self.time_tr("Focus session done!"), self.time_tr("Well done. Take a {m}-minute break.").replace("{m}", &cfg.short.to_string()), "cheer", "pomo_focus_end")
                }
            }
            PomoEnd::Break => (self.time_tr("Break is over"), self.time_tr("Ready for {m} minutes of focus?").replace("{m}", &cfg.focus.to_string()), "wave", "pomo_break_end"),
        };
        if self.settings.general.pomo_sound {
            self.play_sound(sound);
        }
        if self.settings.general.banners {
            self.banner("pomodoro", &title, &body, mood, action);
        } else {
            self.toast(&title);
        }
        self.time_save();
        self.events.push(Event::Study);
    }

    /// The ticking plays during a focus session when it is on, never over a video.
    fn pomo_sync_ticker(&mut self) {
        let want = self.settings.general.pomo_tick && self.time.pomo.in_focus() && self.session.is_none();
        if want == self.time.ticking {
            return;
        }
        self.time.ticking = want;
        if !want {
            if let Some(m) = &self.time.ticker {
                let _ = m.cmd(&["stop"]);
            }
            return;
        }
        let Some(file) = self.sound_file("pomo_tick") else { return };
        if self.time.ticker.is_none() {
            match Mpv::new_audio(&self.rt.libmpv_candidates()) {
                Ok(m) => self.time.ticker = Some(m),
                Err(e) => {
                    crate::debug::log("pomodoro", &format!("the ticking could not start: {}", e));
                    return;
                }
            }
        }
        if let Some(m) = &self.time.ticker {
            let _ = m.set("loop-file", "inf");
            let _ = m.set("volume", "60");
            let _ = m.cmd(&["loadfile", &file.to_string_lossy(), "replace"]);
        }
    }

    /// A built-in sound as a file in `<data dir>/sounds/` (written when missing or out of date).
    fn sound_file(&self, name: &str) -> Option<PathBuf> {
        let (_, bytes) = SOUNDS.iter().find(|(n, _)| *n == name)?;
        let dir = self.data_dir.join("sounds");
        let path = dir.join(format!("{}.ogg", name));
        let fresh = std::fs::metadata(&path).map(|m| m.len() == bytes.len() as u64).unwrap_or(false);
        if !fresh {
            let _ = std::fs::create_dir_all(&dir);
            if let Err(e) = std::fs::write(&path, bytes) {
                crate::debug::log("pomodoro", &format!("the sound {} could not be written: {}", name, e));
                return None;
            }
        }
        Some(path)
    }

    /// `play_sound {name}` (a built-in sound: pomo_focus_end, pomo_break_end, pomo_tick) or `{path}` (an audio file).
    pub fn play_sound(&mut self, name_or_path: &str) {
        let file = match self.sound_file(name_or_path) {
            Some(f) => f,
            None => {
                let p = PathBuf::from(name_or_path);
                if !p.is_file() {
                    return;
                }
                p
            }
        };
        if self.time.chime.is_none() {
            match Mpv::new_audio(&self.rt.libmpv_candidates()) {
                Ok(m) => self.time.chime = Some(m),
                Err(e) => {
                    crate::debug::log("pomodoro", &format!("the sound player could not start: {}", e));
                    return;
                }
            }
        }
        if let Some(m) = &self.time.chime {
            let _ = m.cmd(&["loadfile", &file.to_string_lossy(), "replace"]);
        }
    }
}

