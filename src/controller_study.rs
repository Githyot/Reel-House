//! The Study mode: decks, the review session and the import of Anki packages.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::study::{self, Collection};

/// A deck being studied.
#[derive(Clone, Debug)]
pub struct StudySession {
    pub did: i64,
    /// the index (in `Core::study.cards`) of the card on screen
    pub card: Option<usize>,
    pub answer_shown: bool,
    pub done: u32,
    /// the combo and XP of this session (the XP also goes to Learn's daily total and goal)
    pub game: study::ReviewGame,
}

impl Core {
    pub(super) fn load_study(dir: &Path) -> Collection {
        let mut c = Collection::load(dir);
        c.ensure_builtin();
        c
    }

    pub fn save_study(&mut self) {
        self.study_dirty = 0;
        self.hint_cache = None;
        self.study.save(&self.profile_dir);
        // knowledge.md follows: the words reviewed and where the flashcards are
        self.knowledge_sync();
    }

    pub(super) fn now_secs() -> i64 {
        crate::util::now_unix() as i64
    }

    /// While a video plays: when a new subtitle appears and a word of it is in a Study deck, show its flash card for 5 s.
    /// It runs on the player's tick and reads what the player shows, so the card comes up with the subtitle.
    pub(super) fn sub_hint(&mut self, mpv: &Mpv) {
        if matches!(&self.flash, Some((_, t)) if t.elapsed().as_secs_f32() >= 5.0) {
            self.flash = None;
        }
        if !self.settings.general.study_hints || self.study.notes.is_empty() {
            return;
        }
        let text = mpv.get_str("sub-text").unwrap_or_default();
        if text == self.hint_text {
            return;
        }
        self.hint_text = text.clone();
        if text.trim().is_empty() || !text.chars().any(crate::kanji::is_kanji) {
            return;
        }
        if self.hint_cache.is_none() {
            self.hint_cache = Some(self.study.hint_index());
        }
        let hit = self.hint_cache.as_ref().and_then(|idx| {
            idx.iter().find(|h| text.contains(&h.term) && self.hint_cool.get(&h.nid).map(|t| t.elapsed().as_secs() >= 45).unwrap_or(true)).cloned()
        });
        if let Some(h) = hit {
            self.hint_cool.insert(h.nid, Instant::now());
            self.flash = Some((h, Instant::now()));
            self.events.push(Event::Study);
        }
    }

    pub fn study_open(&mut self, did: i64) {
        if !self.study.decks.iter().any(|d| d.id == did) {
            return;
        }
        let now = Self::now_secs();
        self.study.roll_day(now);
        let card = self.study.next_card(did, now);
        self.study_session = Some(StudySession { did, card, answer_shown: false, done: 0, game: study::ReviewGame::default() });
        self.events.push(Event::Study);
    }

    pub fn study_close(&mut self) {
        if self.study_session.take().is_some() {
            self.save_study();
            self.events.push(Event::Study);
        }
    }

    pub fn study_show(&mut self) {
        if let Some(s) = self.study_session.as_mut() {
            if s.card.is_some() {
                s.answer_shown = true;
                self.events.push(Event::Study);
            }
        }
    }

    /// Run an action on the card on screen and move to the next one when it leaves the queue.
    pub fn study_card_action(&mut self, what: &str, arg: i64) {
        let now = Self::now_secs();
        let Some(s) = self.study_session.as_ref() else { return };
        let (did, Some(idx)) = (s.did, s.card) else { return };
        let mut leaves = true;
        match what {
            "suspend" => self.study.toggle_suspend(idx, false),
            "suspend_note" => self.study.toggle_suspend(idx, true),
            "bury" => self.study.bury(idx, false, now),
            "bury_note" => self.study.bury(idx, true, now),
            "reset" => self.study.reset_card(idx),
            "set_due" => self.study.set_due(idx, arg, now),
            "flag" => {
                self.study.set_flag(idx, arg as u8);
                leaves = false;
            }
            "mark" => {
                self.study.toggle_mark(idx);
                leaves = false;
            }
            _ => return,
        }
        self.study_dirty += 1;
        if leaves {
            let next = self.study.next_card(did, now);
            if let Some(s) = self.study_session.as_mut() {
                s.card = next;
                s.answer_shown = false;
            }
        }
        self.save_study();
        self.events.push(Event::Study);
    }

    pub fn study_undo(&mut self) {
        if let Some(idx) = self.study.undo_answer() {
            let mut xp = 0;
            if let Some(s) = self.study_session.as_mut() {
                s.card = Some(idx);
                s.answer_shown = false;
                s.done = s.done.saturating_sub(1);
                xp = s.game.undo();
            }
            // the XP of the undone answer leaves today's total again
            if xp > 0 {
                self.learn.progress.remove_xp(xp, Self::now_secs());
                self.learn_save();
            }
            self.save_study();
            self.events.push(Event::Study);
        } else {
            self.toast("Nothing to undo.");
        }
    }

    pub fn study_unbury(&mut self) {
        let now = Self::now_secs();
        let Some(did) = self.study_session.as_ref().map(|s| s.did) else { return };
        self.study.unbury_deck(did);
        let next = self.study.next_card(did, now);
        if let Some(s) = self.study_session.as_mut() {
            s.card = next;
            s.answer_shown = false;
        }
        self.save_study();
        self.events.push(Event::Study);
    }

    pub fn study_answer(&mut self, ease: u8) {
        let now = Self::now_secs();
        let Some(s) = self.study_session.as_mut() else { return };
        let Some(idx) = s.card else { return };
        if !(1..=4).contains(&ease) {
            return;
        }
        let did = s.did;
        // the words of the card in the study language go to the knowledge state (knowledge.md)
        self.learn_ensure();
        self.knowledge_from_card(idx, ease);
        self.study.answer(idx, ease, now);
        // the game: the combo, and XP for the same daily total, goal and streak as the Learn lessons
        let xp = self.study_session.as_mut().map(|s| s.game.answer(ease)).unwrap_or(0);
        if xp > 0 {
            let (given, msgs) = self.learn.progress.add_xp(xp, now);
            if let Some(s) = self.study_session.as_mut() {
                s.game.set_last_xp(given);
            }
            self.learn_save();
            self.learn_banners(&msgs);
        }
        self.fire_hook("onStudyAnswer", Json::obj(vec![("deck", self.study.deck_name(did).into()), ("ease", Json::Num(ease as f64))]));
        self.study_dirty += 1;
        let next = self.study.next_card(did, now);
        if let Some(s) = self.study_session.as_mut() {
            s.card = next;
            s.answer_shown = false;
            s.done += 1;
        }
        if self.study_dirty >= 5 || next.is_none() {
            self.save_study();
        }
        self.events.push(Event::Study);
    }

    /// The game numbers the Study pages show (shared with Learn): today's XP and the daily goal, the streak, and the
    /// review session's combo and XP.
    pub fn study_game_json(&self) -> Json {
        let now = Self::now_secs();
        let mut p = self.learn.progress.clone();
        p.roll(now);
        let today = p.today(now);
        let g = self.study_session.as_ref().map(|s| s.game.clone()).unwrap_or_default();
        Json::obj(vec![
            ("xpToday", p.xp_on(today).into()),
            ("goal", p.goal.into()),
            ("streak", p.streak.into()),
            ("streakToday", (p.streak_day == today && p.streak > 0).into()),
            ("boost", Json::Num(p.boost_left(now) as f64)),
            ("combo", g.combo.into()),
            ("bestCombo", g.best_combo.into()),
            ("xp", g.xp.into()),
            ("answered", g.answered.into()),
            ("correct", g.correct.into()),
            ("accuracy", g.accuracy().into()),
            ("fire", study::REVIEW_FIRE.into()),
        ])
    }

    pub fn study_create_deck(&mut self, name: &str) {
        if self.study.create_deck(name).is_some() {
            self.save_study();
            self.events.push(Event::Study);
        } else {
            self.toast("That name is empty or already used.");
        }
    }

    pub fn study_rename_deck(&mut self, did: i64, name: &str) {
        if self.study.rename_deck(did, name) {
            self.save_study();
            self.events.push(Event::Study);
        } else {
            self.toast("That name is empty or already used.");
        }
    }

    pub fn study_delete_deck(&mut self, did: i64) {
        if self.study_session.as_ref().map(|s| s.did == did).unwrap_or(false) {
            self.study_session = None;
        }
        self.study.delete_deck(did);
        self.save_study();
        self.events.push(Event::Study);
    }

    pub fn study_import(&mut self, path: PathBuf) {
        if self.study_importing {
            return;
        }
        self.study_importing = true;
        self.toast("Reading the package…");
        self.events.push(Event::Study);
        // the media of the package go to the study folder of the profile in use
        let data = self.profile_dir.clone();
        self.spawn(move |tx, _| {
            let r = std::fs::read(&path).map_err(|e| format!("Could not read the file: {}", e)).and_then(|bytes| {
                let mut fresh = Collection::default();
                study::import_apkg(&mut fresh, &bytes, &data).map(|_| fresh)
            });
            send(tx, Msg::StudyImported(r));
        });
    }

    pub(super) fn on_study_imported(&mut self, r: Result<Collection, String>) {
        self.study_importing = false;
        match r {
            Ok(fresh) => {
                let (d, n, c) = self.study.merge(fresh);
                self.save_study();
                if n == 0 && c == 0 {
                    self.toast("Nothing new in this package (it may be already imported).");
                } else {
                    self.toast(&format!("Added {} decks, {} notes and {} cards.", d, n, c));
                }
            }
            Err(e) => self.toast(&e),
        }
        self.events.push(Event::Study);
    }

    pub fn study_media_dir(&self) -> PathBuf {
        study::media_dir(&self.profile_dir)
    }

    pub fn study_add_note(&mut self, did: i64, mid: i64, fields: Vec<String>) {
        match self.study.add_note(did, mid, fields, Self::now_secs()) {
            Ok(n) => {
                self.save_study();
                self.toast(&if n == 1 { "Card added.".to_string() } else { format!("{} cards added.", n) });
            }
            Err(e) => self.toast(&e),
        }
        self.events.push(Event::Study);
    }

    pub fn study_edit_note(&mut self, nid: i64, fields: Vec<String>) {
        match self.study.edit_note(nid, fields, Self::now_secs()) {
            Ok(()) => self.save_study(),
            Err(e) => self.toast(&e),
        }
        self.events.push(Event::Study);
    }

    pub fn study_delete_note(&mut self, nid: i64) {
        self.study.delete_note(nid);
        self.save_study();
        self.events.push(Event::Study);
    }

    pub fn study_browse(&mut self, did: i64, query: String) {
        self.study_browse = Some((did, query));
        self.events.push(Event::Study);
    }

    pub fn study_browse_close(&mut self) {
        self.study_browse = None;
        self.events.push(Event::Study);
    }

    pub fn study_stats_open(&mut self, did: Option<i64>) {
        self.study_stats = did;
        self.events.push(Event::Study);
    }

    // ---------- sounds and clips of the cards ----------

    /// Volume of the card sounds and clips (0-100 %): kept in the settings and given to both players.
    pub fn study_set_volume(&mut self, v: u32) {
        self.settings.general.study_volume = v.min(100);
        self.save_settings();
        self.apply_study_volume();
    }

    pub(super) fn apply_study_volume(&self) {
        let v = self.settings.general.study_volume.min(100).to_string();
        if let Some(m) = &self.study_audio {
            let _ = m.set("volume", &v);
        }
        if let Some(m) = &self.study_clip {
            let _ = m.set("volume", &v);
        }
    }

    /// Plays the sound files (names in the media folder) one after the other, stopping what was playing.
    pub fn study_play_sounds(&mut self, names: &[String]) {
        let dir = self.study_media_dir();
        let files: Vec<String> = names.iter().filter(|n| crate::util::safe_media_name(n)).map(|n| dir.join(n)).filter(|p| p.exists()).map(|p| p.to_string_lossy().into_owned()).collect();
        if self.study_audio.is_none() {
            match Mpv::new_audio(&self.rt.libmpv_candidates()) {
                Ok(m) => {
                    self.study_audio = Some(m);
                    self.apply_study_volume();
                }
                Err(e) => {
                    crate::debug::log("study", &format!("the sound player could not start: {}", e));
                    return;
                }
            }
        }
        let Some(m) = &self.study_audio else { return };
        if files.is_empty() {
            let _ = m.cmd(&["stop"]);
            if !names.is_empty() {
                crate::debug::log("study", &format!("sound files not in the media folder: {}", names.join(", ")));
            }
            return;
        }
        for (i, f) in files.iter().enumerate() {
            let _ = m.cmd(&["loadfile", f, if i == 0 { "replace" } else { "append-play" }]);
        }
    }

    /// A new card or the end of a review: sounds and the clip stop.
    pub fn study_stop_sounds(&mut self) {
        self.study_stop_sounds_only();
        self.study_stop_clip();
    }

    fn study_stop_sounds_only(&self) {
        if let Some(m) = &self.study_audio {
            let _ = m.cmd(&["stop"]);
        }
    }

    /// The clip of a clip card, played inside the card (the interface draws the pictures, see `study_clip_source`).
    pub fn study_play_clip(&mut self, name: &str) {
        let name = name.trim_start_matches("clip:");
        if !crate::util::safe_media_name(name) {
            return;
        }
        let path = self.study_media_dir().join(name);
        if !path.exists() {
            self.toast("The clip file is not in the Study media folder.");
            return;
        }
        self.study_stop_sounds_only();
        if self.study_clip.is_none() {
            let fresh = self.study_clip_fresh.clone();
            let made = Mpv::new_embedded(&self.rt.libmpv_candidates()).and_then(|mut m| {
                m.init_render_sw(Box::new(move || fresh.store(true, std::sync::atomic::Ordering::SeqCst)))?;
                Ok(m)
            });
            match made {
                Ok(m) => {
                    self.study_clip = Some(std::sync::Arc::new(m));
                    self.apply_study_volume();
                }
                Err(e) => {
                    self.toast(&format!("The clip could not be played: {}", e));
                    return;
                }
            }
        }
        let file = path.to_string_lossy().into_owned();
        if let Some(m) = &self.study_clip {
            let _ = m.set("pause", "no");
            if m.cmd(&["loadfile", &file, "replace"]).is_err() {
                crate::debug::log("study", "the clip player did not take the file");
            }
            self.study_clip_fresh.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Pauses or resumes the clip (toggles when `pause` is not given); at the end it starts again from the beginning.
    pub fn study_clip_pause(&mut self, pause: Option<bool>) {
        let Some(m) = &self.study_clip else { return };
        let want = pause.unwrap_or(!m.get_flag("pause").unwrap_or(false));
        if !want && m.get_flag("eof-reached") == Some(true) {
            let _ = m.cmd(&["seek", "0", "absolute"]);
        }
        let _ = m.set("pause", if want { "yes" } else { "no" });
    }

    pub fn study_stop_clip(&mut self) {
        if let Some(m) = &self.study_clip {
            let _ = m.cmd(&["stop"]);
        }
    }

    /// The clip player and its "new picture" flag, for the interface to draw the video (see `rh_clip_frame`).
    pub fn study_clip_source(&self) -> Option<(std::sync::Arc<Mpv>, std::sync::Arc<std::sync::atomic::AtomicBool>)> {
        Some((self.study_clip.clone()?, self.study_clip_fresh.clone()))
    }

    /// Where the clip is: `{playing, paused, ended, pos, duration}` for the clip area of the card.
    pub fn study_clip_json(&self) -> Json {
        let Some(m) = &self.study_clip else { return Json::obj(vec![("playing", false.into())]) };
        let idle = m.get_flag("idle-active").unwrap_or(true);
        Json::obj(vec![
            ("playing", (!idle).into()),
            ("paused", m.get_flag("pause").unwrap_or(false).into()),
            ("ended", m.get_flag("eof-reached").unwrap_or(false).into()),
            ("pos", m.get_f64("time-pos").unwrap_or(0.0).into()),
            ("duration", m.get_f64("duration").unwrap_or(0.0).into()),
        ])
    }

    // ---------- AnkiWeb ----------

    pub fn ankiweb_login(&mut self, user: &str, pass: &str) {
        let (user, pass) = (user.trim().to_string(), pass.to_string());
        if user.is_empty() || pass.is_empty() || self.ankiweb_busy {
            return;
        }
        self.ankiweb_busy = true;
        self.ankiweb_status = "Logging in to AnkiWeb…".into();
        self.events.push(Event::Study);
        self.events.push(Event::Info(MsgKind::AnkiWeb, self.ankiweb_status.clone()));
        self.spawn(move |tx, http| {
            let r = crate::ankiweb::login(&*http, &user, &pass).map(|k| (user, k));
            send(tx, Msg::AnkiWebLogin(r));
        });
    }

    pub(super) fn on_ankiweb_login(&mut self, r: Result<(String, String), String>) {
        self.ankiweb_busy = false;
        match r {
            Ok((user, key)) => {
                self.settings.general.ankiweb_user = user;
                self.settings.general.ankiweb_hkey = key;
                self.save_settings();
                self.ankiweb_status = "Logged in.".into();
                self.toast("Logged in to AnkiWeb.");
            }
            Err(e) => {
                self.ankiweb_status = e.clone();
                self.toast(&e);
            }
        }
        self.events.push(Event::Study);
        self.events.push(Event::Info(MsgKind::AnkiWeb, self.ankiweb_status.clone()));
    }

    pub fn ankiweb_logout(&mut self) {
        self.settings.general.ankiweb_hkey.clear();
        self.save_settings();
        self.ankiweb_status = String::new();
        self.events.push(Event::Study);
        self.events.push(Event::Info(MsgKind::AnkiWeb, self.ankiweb_status.clone()));
    }

    /// Brings the AnkiWeb collection here: new decks, notes and cards are added; cards already here take AnkiWeb's
    /// scheduling and notes its fields (AnkiWeb is the reference). Nothing is sent to AnkiWeb.
    pub fn ankiweb_sync(&mut self) {
        let key = self.settings.general.ankiweb_hkey.clone();
        if key.is_empty() {
            self.toast("Log in to AnkiWeb first (Preferences > General > Study).");
            return;
        }
        if self.ankiweb_busy {
            return;
        }
        self.ankiweb_busy = true;
        self.ankiweb_status = "Downloading the collection from AnkiWeb…".into();
        self.events.push(Event::Study);
        self.spawn(move |tx, http| {
            let r = crate::ankiweb::meta(&*http, &key).and_then(|m| {
                if m.empty {
                    return Err("The AnkiWeb collection is empty.".to_string());
                }
                crate::ankiweb::download(&*http, &key).map(|b| (b, m.message))
            });
            // Reading the SQLite file of a big collection takes a while: done here, not on the interface's thread.
            // Only adding it to the collection (fast) is left for `on_ankiweb_pulled`.
            let r = r.and_then(|(bytes, msg)| study::parse_db(&bytes).map(|db| (db, msg)));
            send(tx, Msg::AnkiWebPulled(r));
        });
    }

    pub(super) fn on_ankiweb_pulled(&mut self, r: Result<(study::AnkiDb, String), String>) {
        self.ankiweb_busy = false;
        match r.map(|(db, msg)| (study::merge_db(&mut self.study, &db, true), msg)) {
            Ok((o, msg)) => {
                self.study.ensure_builtin();
                self.save_study();
                self.ankiweb_last = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
                self.ankiweb_status = format!("{} new decks, {} new notes, {} new cards, {} updated.", o.decks, o.notes, o.cards, o.updated);
                if !msg.trim().is_empty() {
                    self.ankiweb_status.push_str(&format!(" AnkiWeb: {}", msg.trim()));
                }
                self.toast(&format!("AnkiWeb: {}", self.ankiweb_status));
            }
            Err(e) => {
                if e.contains("expired") {
                    self.settings.general.ankiweb_hkey.clear();
                    self.save_settings();
                }
                self.ankiweb_status = e.clone();
                self.toast(&e);
            }
        }
        self.events.push(Event::Study);
    }

}
