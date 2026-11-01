//! The Learn area of the Study mode: Duolingo-style lessons built from the course packs (src/learn.rs), and the
//! knowledge state (src/knowledge.rs) that every part of Study feeds.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::knowledge::{strip_tags, CardRef, LessonRow};
use crate::learn::{self, kana, Pack, Progress, Session};

/// Everything the Learn page shows: the loaded course, the saved progress and the lesson in progress.
#[derive(Default)]
pub struct LearnState {
    pub pack: Option<Arc<Pack>>,
    /// The language of `pack`, and the one being read in the background ("" when none).
    pub pack_lang: String,
    pub loading: String,
    pub error: String,
    pub progress: Progress,
    loaded: bool,
    pub session: Option<Session>,
    /// The kana sounds that have a file (assets/kana), read when Learn first opens.
    pub kana_sounds: Option<Arc<HashSet<String>>>,
}

fn seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7)
}

fn n(v: f64) -> Json {
    Json::Num(v)
}

impl Core {
    fn learn_file(&self) -> PathBuf {
        crate::study::dir(&self.profile_dir).join("learn.json")
    }

    // (also used by the Study review, which gives XP: controller_study.rs)
    pub(super) fn learn_save(&self) {
        self.learn.progress.save(&self.learn_file());
    }

    /// The course being learned (3-letter code): the study language of the preferences, else the one chosen here.
    pub fn learn_course(&self) -> String {
        let s = &self.settings.general.study_lang;
        if !s.is_empty() {
            learn::lang3(s)
        } else {
            self.learn.progress.course.clone()
        }
    }

    /// The languages the user knows, as 3-letter codes, main first (the app language when none are set).
    fn learn_known(&self) -> Vec<String> {
        let mut v: Vec<String> = self.settings.general.known_langs.iter().map(|l| learn::lang3(l)).collect();
        if v.is_empty() {
            v.push(learn::lang3(&self.ui_lang()));
        }
        let course = self.learn_course();
        v.retain(|l| *l != course && !l.is_empty());
        v.dedup();
        v
    }

    /// The language lessons are translated into: the first known language the pack has, else English / Portuguese.
    pub fn learn_tl(&self) -> String {
        let known = self.learn_known();
        let course = self.learn_course();
        let have: Vec<String> = self.learn.pack.as_ref().map(|p| p.translation_langs()).unwrap_or_default();
        known
            .iter()
            .find(|l| have.is_empty() || have.contains(l))
            .cloned()
            .or_else(|| have.iter().find(|l| **l != course).cloned())
            .unwrap_or_else(|| if course == "eng" { "por".into() } else { "eng".into() })
    }

    /// Reads learn.json once and starts reading the course pack when it is not loaded.
    pub fn learn_ensure(&mut self) {
        if !self.learn.loaded {
            self.learn.progress = Progress::load(&self.learn_file());
            self.learn.loaded = true;
        }
        let course = self.learn_course();
        if course.is_empty() || course == self.learn.pack_lang || course == self.learn.loading {
            return;
        }
        self.learn.loading = course.clone();
        self.learn.error.clear();
        let data = self.data_dir.clone();
        self.spawn(move |tx, _| {
            let r = learn::load_pack(&data, &course);
            send(tx, Msg::LearnPack(course, r));
        });
    }

    /// Another profile is in use: its Learn progress is read now (the course itself is shared by everyone); a lesson
    /// in progress is left.
    pub(super) fn learn_profile_changed(&mut self) {
        self.learn.loaded = false;
        self.learn.session = None;
        self.learn_ensure();
    }

    pub(super) fn on_learn_pack(&mut self, lang: String, r: Result<Pack, String>) {
        if self.learn.loading == lang {
            self.learn.loading.clear();
        }
        // a file named for one language holding another is not that course
        let r = r.and_then(|p| if p.lang == lang { Ok(p) } else { Err(format!("learn/{}.json holds the course of {}", lang, p.lang)) });
        match r {
            Ok(p) => {
                self.knowledge.set_dictionary(&p.lang, &learn::dictionary(&p), &learn::forms(&p));
                self.learn.pack_lang = lang;
                self.learn.pack = Some(Arc::new(p));
                self.knowledge_sync();
            }
            Err(e) => {
                crate::debug::log("learn", &e);
                self.learn.error = e;
                self.learn.pack_lang = lang;
                self.learn.pack = None;
            }
        }
        self.events.push(Event::Study);
    }

    /// A text of the Learn area in the language of the interface (banners are made here, not in the window).
    fn learn_t(&self, s: &str) -> String {
        let l = &self.ui_lang();
        let lang = if l.is_empty() || l == "system" { self.system_lang.replace('_', "-") } else { l.clone() };
        crate::i18n::translate(&lang, s).map(|x| x.to_string()).unwrap_or_else(|| s.to_string())
    }

    fn kana_sounds(&mut self) -> Arc<HashSet<String>> {
        if self.learn.kana_sounds.is_none() {
            self.learn.kana_sounds = Some(Arc::new(kana::sounds(&self.data_dir)));
        }
        self.learn.kana_sounds.clone().unwrap_or_default()
    }

    /// The Learn page opened: the time zone of the window (minutes east of UTC), a new day, hearts that came back.
    pub fn learn_open(&mut self, tz: Option<i64>) {
        self.learn_ensure();
        self.kana_sounds();
        let now = Self::now_secs();
        if let Some(tz) = tz {
            self.learn.progress.tz = tz.clamp(-14 * 60, 14 * 60);
        }
        self.learn.progress.roll(now);
        self.learn.progress.refill(now);
        self.learn_save();
        self.events.push(Event::Study);
    }

    pub fn learn_set_course(&mut self, lang: &str) {
        let l = learn::lang3(lang);
        if l.is_empty() {
            return;
        }
        self.settings.general.study_lang = learn::lang2(&l);
        self.save_settings();
        self.learn.progress.course = l;
        self.learn.session = None;
        self.learn_save();
        self.learn_ensure();
        self.events.push(Event::Study);
    }

    /// Settings > Study > "Where to start in Learn": the path of the course picks up at `lesson`.
    pub fn learn_start_from(&mut self, lesson: &str) {
        self.learn_ensure();
        let course = self.learn_course();
        let Some(pack) = self.learn.pack.clone().filter(|p| p.lang == course) else {
            self.toast("The course is still loading; try again in a moment.");
            return;
        };
        match self.learn.progress.start_at(&pack, lesson) {
            Some(_) => {
                // a lesson half done belongs to the old place in the path
                if self.learn.session.as_ref().map(|s| s.kind == "lesson").unwrap_or(false) {
                    self.learn.session = None;
                }
                self.learn_save();
                self.toast("Learn now starts at the lesson you chose.");
                self.events.push(Event::Study);
            }
            None => self.toast("That lesson is not in the course."),
        }
    }

    pub fn learn_set_goal(&mut self, xp: u32) {
        if learn::GOALS.contains(&xp) {
            self.learn.progress.goal = xp;
            // the XP quest of today follows a bigger goal
            for q in self.learn.progress.quests.iter_mut().filter(|q| q.kind == "xp" && !q.done) {
                q.target = q.target.max(xp);
            }
            self.learn_save();
        }
        self.events.push(Event::Study);
    }

    pub fn learn_buy(&mut self, item: &str) {
        let now = Self::now_secs();
        match self.learn.progress.buy(item, now) {
            Ok(()) => {
                self.toast(match item {
                    "refill" => "Hearts refilled.",
                    "boost" => "XP boost on: double XP for 15 minutes.",
                    _ => "Streak freeze equipped.",
                });
                // out of hearts in a lesson: go on with it
                if let Some(s) = self.learn.session.as_mut() {
                    if s.status == "no_hearts" && item == "refill" {
                        s.status = if s.pos >= s.queue.len() { "done".into() } else { "question".into() };
                    }
                }
                self.learn_save();
            }
            Err(e) => self.toast(&e),
        }
        self.events.push(Event::Study);
    }

    fn learn_unlimited(&self) -> bool {
        self.settings.general.learn_unlimited_hearts
    }

    /// Lesson ids done in the course and the first one not done (the current lesson).
    fn learn_path_state(&self, pack: &Pack) -> (HashSet<String>, Option<String>) {
        let done: HashSet<String> = self.learn.progress.done_in(&pack.lang).iter().cloned().collect();
        let current = pack.lesson_ids().into_iter().find(|id| !done.contains(id));
        (done, current)
    }

    /// Starts a lesson of the path (a finished one can be played again), a characters lesson ("kana-h-3") or the
    /// legendary round of a finished unit ("legendary-2").
    pub fn learn_start(&mut self, id: &str) {
        if id.starts_with("kana-") {
            return self.learn_kana_start(id);
        }
        if let Some(u) = id.strip_prefix("legendary-").and_then(|u| u.parse::<i64>().ok()) {
            return self.learn_legendary(u);
        }
        let Some(pack) = self.learn.pack.clone() else { return };
        let now = Self::now_secs();
        self.learn.progress.refill(now);
        if !self.learn_unlimited() && self.learn.progress.hearts == 0 {
            self.toast("You have no hearts left. Practise to earn one, or refill them in the shop.");
            self.events.push(Event::Study);
            return;
        }
        let (done, current) = self.learn_path_state(&pack);
        if !done.contains(id) && current.as_deref() != Some(id) {
            self.toast("Finish the lessons before this one first.");
            return;
        }
        let Some((u, l)) = pack.find_lesson(id) else { return };
        let lesson = &pack.units[u].lessons[l];
        let lang = pack.lang.clone();
        let new_words = lesson.words.iter().filter(|w| self.knowledge.row(&lang, w).map(|r| r.right == 0).unwrap_or(true)).cloned().collect();
        let inp = learn::LessonInput {
            pack: &pack,
            words: lesson.words.clone(),
            new_words,
            sentences: lesson.sentences.clone(),
            pool: pack.words_until(id),
            tl: self.learn_tl(),
            max: 12,
            seed: seed(),
        };
        let ch = learn::build_lesson(&inp);
        if ch.is_empty() {
            self.toast("This lesson has nothing to ask in your language yet.");
            return;
        }
        self.learn.session = Some(Session::new("lesson", id, ch, now));
        self.events.push(Event::Study);
    }

    /// A practice: the weakest words (`mode` "weak") or the words answered wrong lately ("mistakes").
    pub fn learn_practice(&mut self, mode: &str) {
        if let Some(script) = mode.strip_prefix("kana-").and_then(|s| s.chars().next()) {
            return self.learn_kana_practice(script);
        }
        let Some(pack) = self.learn.pack.clone() else { return };
        let now = Self::now_secs();
        let (done, current) = self.learn_path_state(&pack);
        // the words of the lessons done so far
        let mut learned: Vec<String> = vec![];
        for u in &pack.units {
            for l in &u.lessons {
                if done.contains(&l.id) {
                    learned.extend(l.words.iter().cloned());
                }
            }
        }
        let words: Vec<String> = if mode == "mistakes" {
            self.learn.progress.mistakes.iter().rev().filter(|w| pack.word(w).is_some()).take(8).cloned().collect()
        } else {
            learn::practice_words(&self.knowledge, &pack, &learned, 8, now)
        };
        if words.is_empty() {
            self.toast(if mode == "mistakes" { "No mistakes to review. Well done!" } else { "Finish a lesson first: practice uses the words you learned." });
            return;
        }
        let mut allowed: HashSet<String> = learned.iter().cloned().collect();
        allowed.extend(words.iter().cloned());
        if let Some(c) = current.as_ref() {
            allowed.extend(pack.words_until(c));
        }
        let inp = learn::LessonInput {
            pack: &pack,
            words: words.clone(),
            new_words: HashSet::new(),
            sentences: learn::sentences_with(&pack, &words, &allowed, 6),
            pool: learned,
            tl: self.learn_tl(),
            max: 10,
            seed: seed(),
        };
        let ch = learn::build_lesson(&inp);
        if ch.is_empty() {
            self.toast("Nothing to practise in your language yet.");
            return;
        }
        let kind = if mode == "mistakes" { "mistakes" } else { "practice" };
        self.learn.session = Some(Session::new(kind, "", ch, now));
        self.events.push(Event::Study);
    }

    /// The legendary round of a finished unit: every word and sentence of the unit, more challenges, more XP.
    pub fn learn_legendary(&mut self, unit: i64) {
        let Some(pack) = self.learn.pack.clone() else { return };
        let now = Self::now_secs();
        self.learn.progress.refill(now);
        if !self.learn_unlimited() && self.learn.progress.hearts == 0 {
            self.toast("You have no hearts left. Practise to earn one, or refill them in the shop.");
            self.events.push(Event::Study);
            return;
        }
        let Some(u) = pack.units.iter().find(|u| u.id == unit) else { return };
        let (done, _) = self.learn_path_state(&pack);
        if !u.lessons.iter().all(|l| done.contains(&l.id)) {
            self.toast("Finish every lesson of the unit to unlock its legendary round.");
            return;
        }
        let mut words: Vec<String> = vec![];
        let mut sentences: Vec<i64> = vec![];
        for l in &u.lessons {
            for w in &l.words {
                if !words.contains(w) && !crate::knowledge::is_punct(w) {
                    words.push(w.clone());
                }
            }
            sentences.extend(l.sentences.iter().copied());
        }
        let last = u.lessons.last().map(|l| l.id.clone()).unwrap_or_default();
        let inp = learn::LessonInput { pack: &pack, words, new_words: HashSet::new(), sentences, pool: pack.words_until(&last), tl: self.learn_tl(), max: 16, seed: seed() };
        let ch = learn::build_lesson(&inp);
        if ch.is_empty() {
            self.toast("This lesson has nothing to ask in your language yet.");
            return;
        }
        self.learn.session = Some(Session::new("legendary", &format!("legendary-{}", unit), ch, now));
        self.events.push(Event::Study);
    }

    /// A characters lesson: one row of the hiragana or katakana chart, with a few characters learned before.
    pub fn learn_kana_start(&mut self, id: &str) {
        let Some((script, n)) = kana::parse_lesson_id(id) else { return };
        let now = Self::now_secs();
        let sounds = self.kana_sounds();
        // the weakest characters of the lessons before this one come back
        let mut earlier: Vec<(f64, kana::Kana)> = (0..n).flat_map(kana::group).map(|k| (self.knowledge.char_strength("jpn", &k.in_script(script), now), k)).collect();
        earlier.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let review: Vec<kana::Kana> = earlier.into_iter().take(5).map(|(_, k)| k).collect();
        let inp = kana::KanaInput { script, new: kana::group(n), review, sounds: &sounds, max: 12, seed: seed() };
        let ch = kana::build_lesson(&inp);
        if ch.is_empty() {
            return;
        }
        self.learn.session = Some(Session::new("kana", id, ch, now));
        self.events.push(Event::Study);
    }

    /// Practises the weakest characters of a script met so far.
    pub fn learn_kana_practice(&mut self, script: char) {
        let now = Self::now_secs();
        let sounds = self.kana_sounds();
        let mut met: Vec<(f64, kana::Kana)> = kana::all()
            .into_iter()
            .filter_map(|k| {
                let ch = k.in_script(script);
                self.knowledge.char_row("jpn", &ch).filter(|r| r.seen > 0).map(|r| (r.strength(now), k))
            })
            .collect();
        if met.len() < 4 {
            self.toast("Learn a few characters first: practice uses the ones you met.");
            return;
        }
        met.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let review: Vec<kana::Kana> = met.into_iter().take(8).map(|(_, k)| k).collect();
        let inp = kana::KanaInput { script, new: vec![], review, sounds: &sounds, max: 12, seed: seed() };
        let ch = kana::build_lesson(&inp);
        if ch.is_empty() {
            return;
        }
        self.learn.session = Some(Session::new("kana", "", ch, now));
        self.events.push(Event::Study);
    }

    /// Opens the daily chest (the daily goal is reached): gems, and sometimes an XP boost.
    pub fn learn_chest(&mut self) {
        let now = Self::now_secs();
        match self.learn.progress.open_chest(now, seed()) {
            Ok((gems, boost)) => {
                let body = if boost { self.learn_t("+%1 gems and an XP boost: double XP for 15 minutes!") } else { self.learn_t("+%1 gems!") };
                let title = self.learn_t("Daily chest");
                self.banner("goal", &title, &body.replace("%1", &gems.to_string()), "cheer", None);
                self.learn_save();
            }
            Err(e) => self.toast(&e),
        }
        self.events.push(Event::Study);
    }

    /// The answer of the current challenge. `missed` lists the words of a match challenge paired wrong first.
    pub fn learn_answer(&mut self, answer: &str, missed: &[String]) {
        let now = Self::now_secs();
        let unlimited = self.learn_unlimited();
        let pack = self.learn.pack.clone();
        let Some(s) = self.learn.session.as_mut() else { return };
        let Some(ch) = s.current().cloned() else { return };
        let Some(v) = s.answer(answer) else { return };
        let kind = s.kind.clone();
        if kind == "kana" {
            // characters go to their own table, with their romaji
            for c in &ch.words {
                let romaji = kana::find(c).map(|(k, _)| k.r).unwrap_or("");
                let right = if ch.kind == "match" { !missed.contains(c) } else { v.correct };
                self.knowledge.record_char_at("jpn", c, romaji, right, now);
            }
            self.events.push(Event::Study);
            return;
        }
        let Some(pack) = pack else { return };
        let lang = pack.lang.clone();
        // every word of the challenge goes to the knowledge state, with its reading and translations: the words of
        // the sentence (word bank, listening, fill-in, translation), the word asked (select) or each pair (match)
        for w in &ch.words {
            if let Some(word) = pack.word(w) {
                self.knowledge.set_info(&lang, w, &word.r, &word.tr);
            }
            let right = if ch.kind == "match" { !missed.contains(w) } else { v.correct };
            self.knowledge.record_at(&lang, w, right, now);
        }
        let loses_hearts = (kind == "lesson" || kind == "legendary") && !unlimited;
        if ch.kind == "match" {
            self.learn.progress.add_mistakes(missed);
        } else if !v.correct {
            self.learn.progress.add_mistakes(&ch.words);
            if loses_hearts {
                self.learn.progress.lose_heart(now);
            }
        } else if kind == "mistakes" {
            self.learn.progress.mistakes.retain(|m| !ch.words.contains(m));
        }
        self.learn_save();
        self.events.push(Event::Study);
    }

    /// Continue after the feedback, or close the finish screen.
    pub fn learn_continue(&mut self) {
        let now = Self::now_secs();
        let unlimited = self.learn_unlimited();
        let hearts = {
            self.learn.progress.refill(now);
            self.learn.progress.hearts
        };
        let Some(s) = self.learn.session.as_mut() else { return };
        match s.status.as_str() {
            "done" => {
                self.learn.session = None;
                self.events.push(Event::Study);
                return;
            }
            "no_hearts" => return,
            _ => {}
        }
        let was_wrong = s.status == "wrong";
        let over = s.advance();
        if was_wrong && (s.kind == "lesson" || s.kind == "legendary") && !unlimited && hearts == 0 {
            s.status = "no_hearts".into();
            self.events.push(Event::Study);
            return;
        }
        if over {
            self.learn_finish(now);
        }
        self.events.push(Event::Study);
    }

    fn learn_finish(&mut self, now: i64) {
        let pack = self.learn.pack.clone();
        // a characters lesson needs no course pack; the others do
        if pack.is_none() && self.learn.session.as_ref().map(|s| s.kind != "kana").unwrap_or(true) {
            return;
        }
        let Some(mut s) = self.learn.session.take() else { return };
        s.ended = now;
        let kana_lesson = kana::parse_lesson_id(&s.lesson_id);
        match (s.kind.as_str(), pack.as_ref()) {
            ("lesson", Some(p)) => self.learn.progress.mark_done(&p.lang, &s.lesson_id),
            ("legendary", Some(p)) => {
                if let Some(u) = s.lesson_id.strip_prefix("legendary-").and_then(|u| u.parse::<i64>().ok()) {
                    self.learn.progress.mark_legendary(&p.lang, u);
                }
            }
            ("kana", _) if kana_lesson.is_some() => {
                if let Some((script, _)) = kana_lesson {
                    self.learn.progress.mark_done(&kana::done_key(script), &s.lesson_id);
                }
            }
            // practising earns a heart back
            _ => self.learn.progress.gain_heart(now),
        }
        let (words, units) = match pack.as_ref() {
            Some(p) => {
                let done: HashSet<String> = self.learn.progress.done_in(&p.lang).iter().cloned().collect();
                (self.knowledge.learned(&p.lang), p.units.iter().filter(|u| u.lessons.iter().all(|l| done.contains(&l.id))).count())
            }
            None => (0, 0),
        };
        let (xp, gems, msgs) = self.learn.progress.finish(&s, now, words, units);
        s.xp = xp;
        s.gems = gems;
        s.messages = msgs.clone();
        s.status = "done".into();
        self.learn_record_lesson(&s, pack.as_deref(), now);
        self.learn.session = Some(s);
        self.learn_save();
        // knowledge.md is written right away (knowledge_sync saves it)
        self.knowledge_sync();
        self.learn_banners(&msgs);
    }

    /// The finished lesson goes to the "Lessons" table of knowledge.md, and the words it introduces get their rows.
    fn learn_record_lesson(&mut self, s: &Session, pack: Option<&Pack>, now: i64) {
        let row = match s.kind.as_str() {
            "lesson" | "legendary" => {
                let Some(p) = pack else { return };
                let tl = self.learn_tl();
                let (unit, title, words) = if s.kind == "lesson" {
                    let Some((u, l)) = p.find_lesson(&s.lesson_id) else { return };
                    let words: Vec<String> = p.units[u].lessons[l].words.iter().filter(|w| !crate::knowledge::is_punct(w)).cloned().collect();
                    (u, format!("{} · {}", p.units[u].title(&tl), l + 1), words)
                } else {
                    let id = s.lesson_id.strip_prefix("legendary-").and_then(|u| u.parse::<i64>().ok()).unwrap_or(-1);
                    let Some(u) = p.units.iter().position(|u| u.id == id) else { return };
                    (u, format!("{} · {}", p.units[u].title(&tl), self.learn_t("Legendary")), vec![])
                };
                for w in &words {
                    if let Some(word) = p.word(w) {
                        self.knowledge.set_info(&p.lang, w, &word.r, &word.tr);
                    }
                    self.knowledge.introduce(&p.lang, w);
                }
                (p.lang.clone(), LessonRow { unit: (unit + 1).to_string(), id: s.lesson_id.clone(), title, done: now, plays: 0, xp: s.xp, accuracy: s.accuracy(), words })
            }
            "kana" => {
                let Some((script, n)) = kana::parse_lesson_id(&s.lesson_id) else { return };
                let chars: Vec<String> = kana::group(n).iter().map(|k| k.in_script(script)).collect();
                ("jpn".to_string(), LessonRow { unit: kana::script_name(script).into(), id: s.lesson_id.clone(), title: chars.join(" "), done: now, plays: 0, xp: s.xp, accuracy: s.accuracy(), words: chars })
            }
            _ => return,
        };
        self.knowledge.record_lesson(&row.0, row.1);
    }

    /// Banners for what the lesson changed: the streak (and its milestones), the daily goal, achievements.
    pub(super) fn learn_banners(&mut self, msgs: &[String]) {
        let milestone = msgs.iter().find_map(|m| m.strip_prefix("milestone:")).map(|d| d.to_string());
        for m in msgs {
            if let Some(days) = m.strip_prefix("streak:") {
                if let Some(d) = &milestone {
                    let reward = learn::STREAK_MILESTONES.iter().find(|(x, _)| x.to_string() == *d).map(|(_, g)| *g).unwrap_or(0);
                    let title = self.learn_t("%1-day streak!").replace("%1", d);
                    let body = self.learn_t("A streak milestone! +%1 gems.").replace("%1", &reward.to_string());
                    self.banner("streak", &title, &body, "cheer", None);
                } else {
                    let title = self.learn_t("Streak extended!");
                    let body = if days == "1" { self.learn_t("Day one of your streak. Come back tomorrow!") } else { self.learn_t("%1 days in a row. Keep it going!").replace("%1", days) };
                    self.banner("streak", &title, &body, "happy", None);
                }
            } else if m == "goal" {
                let title = self.learn_t("Daily goal reached!");
                let body = self.learn_t("Your daily chest is waiting in Learn.");
                self.banner("goal", &title, &body, "cheer", None);
            } else if let Some(id) = m.strip_prefix("achievement:") {
                let name = learn::ACHIEVEMENTS.iter().find(|a| a.0 == id).map(|a| a.1).unwrap_or(id);
                let title = self.learn_t("Achievement unlocked");
                let body = self.learn_t(name);
                self.banner("achievement", &title, &body, "happy", None);
            }
        }
    }

    pub fn learn_quit(&mut self) {
        self.learn.session = None;
        self.learn_save();
        self.knowledge.save();
        self.events.push(Event::Study);
    }

    /// Plays the sentence audio of a challenge (a Tatoeba address), slower when asked.
    pub fn learn_play(&mut self, url: &str, slow: bool) {
        // a kana sound ("kana:ka") is a file of assets/kana
        let local = url.strip_prefix("kana:").map(|snd| kana::sound_file(&self.data_dir, snd));
        let url = match &local {
            Some(Some(p)) => p.to_string_lossy().into_owned(),
            Some(None) => return,
            None if url.starts_with("https://tatoeba.org/") => url.to_string(),
            None => return,
        };
        let url = url.as_str();
        if self.study_audio.is_none() {
            match Mpv::new_audio(&self.rt.libmpv_candidates()) {
                Ok(m) => self.study_audio = Some(m),
                Err(e) => {
                    self.toast(&format!("The sound could not be played: {}", e));
                    return;
                }
            }
        }
        if let Some(m) = &self.study_audio {
            let _ = m.set("speed", if slow { "0.7" } else { "1" });
            let _ = m.cmd(&["loadfile", url, "replace"]);
        }
    }

    // ---------- the knowledge state ----------

    /// Updates knowledge.md: the languages, translations of the words from the pack, and the flashcards index.
    pub fn knowledge_sync(&mut self) {
        let course = self.learn_course();
        let mut known = self.learn_known();
        if known.is_empty() {
            known.push("eng".into());
        }
        let tl = self.learn_tl();
        self.knowledge.set_langs(&course, &tl, &known);
        // words met in flashcards get the translations the course knows
        if let Some(pack) = self.learn.pack.clone() {
            let lang = pack.lang.clone();
            let missing: Vec<String> = self.knowledge.rows(&lang).iter().filter(|r| r.tr.is_empty()).map(|r| r.word.clone()).collect();
            for w in missing {
                if let Some(word) = pack.word(&w) {
                    self.knowledge.set_info(&lang, &w, &word.r, &word.tr);
                }
            }
        }
        let cards = self.flashcard_index(&course);
        self.knowledge.set_cards(cards);
        self.knowledge.save();
    }

    /// The notes of the Study collection in the study language, as the index the player uses to find a card.
    fn flashcard_index(&self, lang: &str) -> Vec<CardRef> {
        let col = &self.study;
        // the deck of each note (its first card), found once
        let mut deck_of: HashMap<i64, i64> = HashMap::new();
        for c in &col.cards {
            deck_of.entry(c.nid).or_insert(c.did);
        }
        let mut out = vec![];
        for note in &col.notes {
            let Some(m) = col.models.iter().find(|m| m.id == note.mid) else { continue };
            let field = |name: &str| m.fields.iter().position(|f| f == name).and_then(|i| note.fields.get(i)).map(|s| strip_tags(s)).unwrap_or_default();
            let clip = m.id == crate::study::CLIP || m.id == super::controller_clip::CLIP_VIDEO;
            let front = if clip { field("Sentence") } else { note.fields.first().map(|s| strip_tags(s)).unwrap_or_default() };
            if front.is_empty() || !in_lang(&front, lang) {
                continue;
            }
            let back = if clip { field("Meaning") } else { note.fields.get(1).map(|s| strip_tags(s)).unwrap_or_default() };
            let deck = deck_of.get(&note.id).map(|d| col.deck_name(*d)).unwrap_or_default();
            let location = if clip {
                let raw = m.fields.iter().position(|f| f == "Clip").and_then(|i| note.fields.get(i)).cloned().unwrap_or_default();
                raw.split("clip:").nth(1).and_then(|r| r.split('"').next()).map(|f| format!("study/media/{}", f)).unwrap_or_else(|| "study/collection".into())
            } else {
                "study/collection".into()
            };
            let short = |s: &str| s.chars().take(160).collect::<String>();
            out.push(CardRef {
                id: if clip { format!("clip:{}", note.id) } else { format!("a:{}", note.id) },
                kind: if clip { "clip".into() } else { "anki".into() },
                deck,
                note: note.id,
                location,
                front: short(&front),
                back: short(&back),
            });
            if out.len() >= 20_000 {
                break;
            }
        }
        out
    }

    /// A flashcard was answered (Study review): its words in the study language go to the knowledge state.
    pub(super) fn knowledge_from_card(&mut self, idx: usize, ease: u8) {
        let lang = self.learn_course();
        if lang.is_empty() {
            return;
        }
        let Some(card) = self.study.cards.get(idx) else { return };
        let Some(note) = self.study.notes.iter().find(|n| n.id == card.nid) else { return };
        let clip = note.mid == crate::study::CLIP || note.mid == super::controller_clip::CLIP_VIDEO;
        let front = note.fields.first().map(|s| strip_tags(s)).unwrap_or_default();
        if front.is_empty() || !in_lang(&front, &lang) {
            return;
        }
        let now = Self::now_secs();
        let words = self.knowledge.tokens(&lang, &front);
        // a vocabulary card is one word; a sentence card (or a clip) gives all its words
        let words = if !clip && words.len() > 1 && front.chars().count() <= 12 && !front.contains(' ') { vec![front.clone()] } else { words };
        for w in words.iter().take(40) {
            self.knowledge.record_at(&lang, w, ease > 1, now);
        }
        // a one-word card: its back is the word's meaning in the user's language, when the course has none
        if words.len() == 1 && !clip {
            let back = note.fields.get(1).map(|s| strip_tags(s)).unwrap_or_default();
            let tl = self.learn_tl();
            let has = self.knowledge.row(&lang, &words[0]).map(|r| !r.tr(&tl).is_empty()).unwrap_or(false);
            if !has && !back.is_empty() && back.chars().count() <= 60 && !in_lang(&back, &lang) {
                self.knowledge.set_info(&lang, &words[0], "", &[(tl, back)]);
            }
        }
    }

    // ---------- the state for the window ----------

    pub fn learn_json(&self) -> Json {
        let now = Self::now_secs();
        let mut p = self.learn.progress.clone();
        p.roll(now);
        p.refill(now);
        let today = p.today(now);
        let course = self.learn_course();
        let tl = self.learn_tl();
        let courses: Vec<Json> = learn::available_courses(&self.data_dir)
            .into_iter()
            .map(|l| Json::obj(vec![("lang", l.as_str().into()), ("name", crate::util::lang_name(&learn::lang2(&l)).into())]))
            .collect();
        let mut units: Vec<Json> = vec![];
        let mut lessons_done = 0usize;
        let mut lessons_total = 0usize;
        if let Some(pack) = self.learn.pack.as_ref().filter(|p| p.lang == course) {
            let (done, current) = self.learn_path_state(pack);
            for (ui, u) in pack.units.iter().enumerate() {
                let ls: Vec<Json> = u
                    .lessons
                    .iter()
                    .enumerate()
                    .map(|(li, l)| {
                        let st = if done.contains(&l.id) { "done" } else if current.as_deref() == Some(l.id.as_str()) { "current" } else { "locked" };
                        Json::obj(vec![("id", l.id.as_str().into()), ("n", n(li as f64 + 1.0)), ("state", st.into()), ("words", Json::from(l.words.iter().filter(|w| !crate::knowledge::is_punct(w)).cloned().collect::<Vec<String>>()))])
                    })
                    .collect();
                let d = u.lessons.iter().filter(|l| done.contains(&l.id)).count();
                lessons_done += d;
                lessons_total += u.lessons.len();
                let finished = d == u.lessons.len();
                let legendary = p.is_legendary(&pack.lang, u.id);
                units.push(Json::obj(vec![
                    ("id", n(u.id as f64)),
                    // the badge of the unit: 0 not finished, 1 finished, 2 legendary
                    ("level", learn::unit_level(finished, legendary).into()),
                    ("legendary", legendary.into()),
                    ("index", n(ui as f64)),
                    ("title", u.title(&tl).into()),
                    ("subtitle", u.title(&course).into()),
                    ("done", n(d as f64)),
                    ("total", n(u.lessons.len() as f64)),
                    ("lessons", Json::Arr(ls)),
                ]));
            }
        }
        let quests: Vec<Json> = p
            .quests
            .iter()
            .map(|q| Json::obj(vec![("kind", q.kind.as_str().into()), ("target", q.target.into()), ("progress", q.progress.into()), ("reward", q.reward.into()), ("done", q.done.into())]))
            .collect();
        let words = self.knowledge.learned(&course);
        let units_done = units.iter().filter(|u| u.get("done").as_f64() == u.get("total").as_f64()).count() as u32;
        let achievements: Vec<Json> = learn::ACHIEVEMENTS
            .iter()
            .map(|(id, name, what, target)| {
                let have = match *what {
                    "lessons" => p.lessons,
                    "streak" => p.longest,
                    "words" => words as u32,
                    "perfect" => p.perfect,
                    "xp" => p.xp_total,
                    "listens" => p.listens,
                    "units" => units_done,
                    _ => 0,
                };
                let earned = p.achievements.iter().any(|(a, _)| a == id);
                Json::obj(vec![("id", (*id).into()), ("name", (*name).into()), ("have", have.min(*target).into()), ("target", (*target).into()), ("earned", earned.into())])
            })
            .collect();
        // the last five weeks, Monday first, for the streak calendar
        let monday = learn::week_of(today) * 7 - 3;
        let calendar: Vec<Json> = (monday - 28..monday + 7)
            .map(|d| {
                let (_, _, dom, ..) = crate::util::civil((d.max(0) * 86400) as u64);
                Json::obj(vec![("day", n(dom as f64)), ("xp", p.xp_on(d).into()), ("frozen", p.frozen.contains(&d).into()), ("today", (d == today).into()), ("future", (d > today).into())])
            })
            .collect();
        let weeks = p.weeks(now, 8);
        let this_week = *weeks.last().unwrap_or(&0);
        let rank = 1 + weeks.iter().filter(|w| **w > this_week).count();
        let session = self.learn.session.as_ref().map(|s| self.learn_session_json(s, &p)).unwrap_or(Json::Null);
        let kana = if course == "jpn" { self.kana_json(&p, now) } else { Json::Null };
        Json::obj(vec![
            ("loading", (!self.learn.loading.is_empty()).into()),
            // an error of another course (the study language changed) is not this one's
            ("error", if self.learn.pack_lang == course { self.learn.error.as_str().into() } else { Json::from("") }),
            ("courses", Json::Arr(courses)),
            ("course", course.as_str().into()),
            ("course2", learn::lang2(&course).into()),
            ("course_name", if course.is_empty() { Json::from("") } else { crate::util::lang_name(&learn::lang2(&course)).into() }),
            ("tl", tl.as_str().into()),
            ("tl_name", crate::util::lang_name(&learn::lang2(&tl)).into()),
            ("ready", self.learn.pack.as_ref().map(|p| p.lang == course).unwrap_or(false).into()),
            ("credits", self.learn.pack.as_ref().map(|p| p.credits.clone()).unwrap_or_default().into()),
            ("units", Json::Arr(units)),
            ("lessons_done", n(lessons_done as f64)),
            ("lessons_total", n(lessons_total as f64)),
            ("xp_total", p.xp_total.into()),
            ("today_xp", p.xp_on(today).into()),
            ("goal", p.goal.into()),
            ("goals", Json::Arr(learn::GOALS.iter().map(|g| (*g).into()).collect())),
            ("streak", p.streak.into()),
            ("streak_today", (p.streak_day == today && p.streak > 0).into()),
            ("longest", p.longest.into()),
            ("freezes", p.freezes.into()),
            ("max_freezes", learn::MAX_FREEZES.into()),
            ("hearts", p.hearts.into()),
            ("max_hearts", learn::MAX_HEARTS.into()),
            ("next_heart", n(p.next_heart_in(now) as f64)),
            ("unlimited", self.learn_unlimited().into()),
            ("gems", p.gems.into()),
            ("price_refill", learn::PRICE_REFILL.into()),
            ("price_freeze", learn::PRICE_FREEZE.into()),
            ("price_boost", learn::PRICE_BOOST.into()),
            ("boost_left", n(p.boost_left(now) as f64)),
            ("chest_ready", p.chest_ready(now).into()),
            ("chest_opened", (p.chest_day == today).into()),
            ("gem_rules", learn::gem_rules_json()),
            ("kana", kana),
            ("quests", Json::Arr(quests)),
            ("achievements", Json::Arr(achievements)),
            ("calendar", Json::Arr(calendar)),
            ("week_days", Json::Arr(p.week_days(now).into_iter().map(|x| x.into()).collect())),
            ("weeks", Json::Arr(weeks.iter().map(|x| (*x).into()).collect())),
            ("week_rank", n(rank as f64)),
            ("words", n(words as f64)),
            ("mistakes", n(p.mistakes.len() as f64)),
            ("can_practice", (lessons_done > 0).into()),
            ("session", session),
        ])
    }

    /// The characters view (Japanese): both charts with the strength of each character, and the lessons.
    fn kana_json(&self, p: &Progress, now: i64) -> Json {
        let empty = HashSet::new();
        let sounds: &HashSet<String> = self.learn.kana_sounds.as_deref().unwrap_or(&empty);
        let strength = |c: &str| self.knowledge.char_row("jpn", c).map(|r| (r.strength(now), r.seen > 0)).unwrap_or((0.0, false));
        let lessons = |script: char| -> Json {
            let done = p.done_in(&kana::done_key(script));
            let current = (0..kana::group_count()).map(|g| kana::lesson_id(script, g)).find(|id| !done.contains(id));
            Json::Arr(
                (0..kana::group_count())
                    .map(|g| {
                        let id = kana::lesson_id(script, g);
                        let chars: Vec<String> = kana::group(g).iter().map(|k| k.in_script(script)).collect();
                        let st = if done.contains(&id) { "done" } else if current.as_deref() == Some(id.as_str()) { "current" } else { "todo" };
                        Json::obj(vec![("id", id.as_str().into()), ("n", n(g as f64 + 1.0)), ("chars", Json::from(chars)), ("state", st.into())])
                    })
                    .collect(),
            )
        };
        let learned = |script: char| kana::all().iter().filter(|k| self.knowledge.char_row("jpn", &k.in_script(script)).map(|r| r.right > 0).unwrap_or(false)).count();
        Json::obj(vec![
            ("hira", kana::chart_json('h', &strength, sounds)),
            ("kata", kana::chart_json('k', &strength, sounds)),
            ("lessons_h", lessons('h')),
            ("lessons_k", lessons('k')),
            ("learned_h", n(learned('h') as f64)),
            ("learned_k", n(learned('k') as f64)),
            ("total", n(kana::all().len() as f64)),
            ("sounds", n(sounds.len() as f64)),
        ])
    }

    fn learn_session_json(&self, s: &Session, p: &Progress) -> Json {
        let ch = s.current().map(|c| {
            let answered = s.status != "question";
            Json::obj(vec![
                ("kind", c.kind.as_str().into()),
                ("prompt", c.prompt.as_str().into()),
                ("reading", c.reading.as_str().into()),
                ("prompt_lang", c.prompt_lang.as_str().into()),
                ("hints", Json::Arr(c.hints.iter().map(|h| Json::obj(vec![("t", h.t.as_str().into()), ("hint", h.hint.as_str().into())])).collect())),
                ("help", c.help.as_str().into()),
                ("options", Json::from(c.options.clone())),
                ("option_readings", Json::from(c.option_readings.clone())),
                ("bank", Json::from(c.bank.clone())),
                ("pairs", Json::Arr(c.pairs.iter().map(|(a, b)| Json::Arr(vec![a.as_str().into(), b.as_str().into()])).collect())),
                ("audio", c.audio.as_str().into()),
                ("answer_lang", c.answer_lang.as_str().into()),
                ("new_word", c.new_word.into()),
                // the answer only after it was checked
                ("answer", if answered { c.answer.as_str().into() } else { Json::from("") }),
            ])
        });
        let verdict = s.verdict.as_ref().map(|v| Json::obj(vec![("correct", v.correct.into()), ("note", v.note.as_str().into()), ("shown", v.shown.as_str().into()), ("given", s.given.as_str().into())])).unwrap_or(Json::Null);
        let secs = if s.ended > 0 { s.ended - s.started } else { Self::now_secs() - s.started };
        Json::obj(vec![
            ("kind", s.kind.as_str().into()),
            ("lesson", s.lesson_id.as_str().into()),
            ("status", s.status.as_str().into()),
            // which challenge this is (the window clears its answer when it changes)
            ("pos", n(s.pos as f64)),
            ("started", n(s.started as f64)),
            ("progress", n(s.progress())),
            ("total", n(s.total as f64)),
            ("solved", n(s.solved as f64)),
            ("mistakes", s.mistakes.into()),
            ("combo", s.combo.into()),
            ("best_combo", s.best_combo.into()),
            ("boost", (p.boost_until > Self::now_secs()).into()),
            ("hearts", p.hearts.into()),
            ("challenge", ch.unwrap_or(Json::Null)),
            ("verdict", verdict),
            (
                "result",
                Json::obj(vec![
                    ("xp", s.xp.into()),
                    ("gems", s.gems.into()),
                    ("accuracy", s.accuracy().into()),
                    ("secs", n(secs.max(0) as f64)),
                    ("best_combo", s.best_combo.into()),
                    ("messages", Json::from(s.messages.clone())),
                ]),
            ),
        ])
    }
}

/// The text is written in the script of the language: kana / kanji for Japanese, Latin letters for the others.
fn in_lang(text: &str, lang: &str) -> bool {
    let cjk = text.chars().any(crate::knowledge::is_cjk);
    if lang == "jpn" {
        cjk
    } else {
        !cjk && text.chars().any(|c| c.is_alphabetic())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers every challenge of the session right, then closes the finish screen. Returns the learn state seen
    /// on the finish screen.
    fn play_through(c: &mut Core) -> Json {
        for _ in 0..200 {
            let Some(s) = c.learn.session.as_ref() else { break };
            match s.status.as_str() {
                "question" => {
                    let ch = s.current().cloned().expect("a challenge");
                    let answer = match ch.kind.as_str() {
                        "match" => String::new(),
                        "kana_trace" => "ok".into(),
                        _ => ch.answer.clone(),
                    };
                    c.learn_answer(&answer, &[]);
                    assert_eq!(c.learn.session.as_ref().unwrap().status, "right", "{:?}", ch);
                }
                "done" => {
                    let j = c.learn_json();
                    c.learn_continue();
                    return j;
                }
                _ => c.learn_continue(),
            }
        }
        panic!("the lesson did not end")
    }

    /// A real lesson and a characters lesson through the core: knowledge.md gets the Lessons and Characters tables
    /// and every word of the lesson right after it ends. With LEARN_DUMP set, the learn state of a few moments is
    /// written there (for trying the window's QML offline).
    #[test]
    fn lessons_go_to_knowledge() {
        let dir = std::env::temp_dir().join(format!("rh_learn_core_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("REELHOUSE_DATA", &dir);
        let mut c = Core::new();
        c.knowledge = crate::knowledge::Knowledge::load_from(&crate::study::dir(&dir).join("knowledge.md"));
        c.settings.general.study_lang = "ja".into();
        c.settings.general.known_langs = vec!["en".into()];
        let Ok(pack) = learn::load_pack(&c.data_dir, "jpn") else { return };
        c.on_learn_pack("jpn".into(), Ok(pack.clone()));
        c.learn_open(Some(0));
        let mut dump = vec![];
        let first = pack.lesson_ids()[0].clone();
        c.learn_start(&first);
        dump.push(("lesson", c.learn_json()));
        let fin = play_through(&mut c);
        dump.push(("finish", fin));
        let text = std::fs::read_to_string(crate::study::dir(&dir).join("knowledge.md")).unwrap();
        assert!(text.contains("## Lessons — jpn"), "{}", text);
        assert!(text.contains(&format!("| 1 | {} |", first)), "{}", text);
        let (u, l) = pack.find_lesson(&first).unwrap();
        for w in pack.units[u].lessons[l].words.iter().filter(|w| !crate::knowledge::is_punct(w)) {
            assert!(c.knowledge.row("jpn", w).is_some(), "{} is not in knowledge.md", w);
        }
        // a characters lesson
        c.learn_start("kana-h-1");
        assert_eq!(c.learn.session.as_ref().map(|s| s.kind.as_str()), Some("kana"));
        dump.push(("kana", c.learn_json()));
        play_through(&mut c);
        let text = std::fs::read_to_string(crate::study::dir(&dir).join("knowledge.md")).unwrap();
        assert!(text.contains("## Characters — jpn"));
        assert!(text.contains("| Hiragana | kana-h-1 | あ い う え お |"), "{}", text);
        assert!(c.knowledge.char_row("jpn", "あ").map(|r| r.right > 0).unwrap_or(false));
        let j = c.learn_json();
        assert!(j.get("kana").get("lessons_h").at(0).get("state").str() == "done");
        assert_eq!(j.get("kana").get("hira").at(0).get("rows").at(0).at(0).get("c").str(), "あ");
        dump.push(("path", j));
        // the legendary round of a finished unit is refused until the unit is done
        let unit = pack.units[0].id;
        c.learn_start(&format!("legendary-{}", unit));
        assert!(c.learn.session.is_none());
        for id in pack.units[0].lessons.iter().map(|l| l.id.clone()).collect::<Vec<_>>() {
            c.learn.progress.mark_done("jpn", &id);
        }
        c.learn.progress.hearts = learn::MAX_HEARTS;
        c.learn_start(&format!("legendary-{}", unit));
        assert_eq!(c.learn.session.as_ref().map(|s| s.kind.as_str()), Some("legendary"));
        play_through(&mut c);
        assert!(c.learn.progress.is_legendary("jpn", unit));
        assert_eq!(c.learn_json().get("units").at(0).get("level").as_u64(), Some(2));
        if let Ok(out) = std::env::var("LEARN_DUMP") {
            let all = Json::obj(dump.iter().map(|(k, v)| (*k, v.clone())).collect());
            let _ = std::fs::write(out, all.pretty());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
