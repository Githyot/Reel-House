//! Study reminders shown as banners with the macaw ("Arara"): streak at risk in the evening, daily goal not met,
//! flashcards due, a daily quest nearly done, and "the macaw is waiting" after a few days away.
//! This file is part of `controller` (it needs the private parts of `Core`). See docs/mascot-contract.md.
//!
//! How a reminder is chosen (a small version of what Duolingo describes for Duo):
//! - Once a minute the facts are gathered (study/learn.json, the Anki collection, the local hour).
//! - Nothing is shown in the quiet hours, while a video plays, during a lesson or review, or when banners are off.
//! - At most `MAX_PER_DAY` reminders a day, at least `MIN_GAP` apart, and each kind has its own cooldown.
//! - Each kind has a pool of messages; the ones shown recently are skipped, so the macaw does not repeat itself.
//! - The log of what was shown lives in study/nudges.json, so a restart does not bring the same reminder back.
use super::*;
use crate::learn::{self, Progress};

/// How often the facts are looked at.
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// After the app opens, the greeting has the stage for a while.
const STARTUP_GRACE: i64 = 3 * 60;
/// Two reminders (of any kind) are at least this far apart.
const MIN_GAP: i64 = 45 * 60;
/// Reminders in one day (the greeting does not count).
const MAX_PER_DAY: u32 = 3;
/// How many shown messages are remembered so they are not picked again soon.
const RECENT_KEEP: usize = 16;
/// Flashcards due before the macaw mentions them.
const DUE_MIN: u32 = 10;
/// The evening, when a streak with no lesson today is "at risk".
const EVENING: u32 = 18;
/// No "daily goal" reminder in the morning: the day is still young.
const GOAL_FROM: u32 = 13;

/// The same kind is not repeated within this many seconds.
pub fn cooldown(kind: &str) -> i64 {
    match kind {
        "streak" => 3 * 3600,
        "goal" => 4 * 3600,
        "review" => 5 * 3600,
        "quest" => 3 * 3600,
        // once a day is plenty for "I miss you"
        "idle" => 20 * 3600,
        _ => 3600,
    }
}

/// The macaw's face for each kind of reminder.
pub fn mood_of(kind: &str) -> &'static str {
    match kind {
        "streak" => "tired",
        "goal" => "cheer",
        "review" => "think",
        "quest" => "happy",
        "idle" => "sad",
        "greet" => "wave",
        _ => "idle",
    }
}

/// The banner `kind` of the contract for each reminder (the inactivity one is a plain "info" banner).
fn banner_kind(kind: &str) -> &'static str {
    match kind {
        "streak" => "streak",
        "goal" => "goal",
        "review" => "review",
        "quest" => "quest",
        _ => "info",
    }
}

/// The messages, (title, body) in English; "{n}" is replaced by a number. Translated through crate::i18n.
/// The macaw is a little sleepy and runs on energy drinks, but kind: light humour, never heavy guilt.
pub fn pool(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "streak" => &[
            ("Your {n}-day streak is yawning", "One quick lesson keeps it awake. I'll hold the energy drink."),
            ("Streak check!", "{n} days in a row, and it's getting late. Five minutes tonight?"),
            ("Don't let the fire go out", "Your {n}-day streak needs one small lesson before midnight. I believe in you (and in caffeine)."),
            ("Psst… your streak", "I stayed up guarding your {n} days. Give me a hand?"),
            ("Still time tonight", "A short lesson saves your {n}-day streak. Then we can both get some sleep."),
        ],
        "goal" => &[
            ("Daily goal: {n} XP to go", "A short lesson gets you there. I'll be right here, sipping."),
            ("Almost there!", "Only {n} XP left for today's goal. You've got this."),
            ("Today's goal is waiting", "{n} XP and we can call it a day. Deal?"),
            ("A little nudge", "Your daily goal is {n} XP away. One lesson should do it."),
        ],
        "review" => &[
            ("{n} cards are ready", "Your flashcards miss you. A quick review keeps them fresh."),
            ("Review time", "{n} cards are due. Past you made them, present you can finish them."),
            ("Cards are piling up", "{n} reviews today. Small batches are easier than big ones, trust me."),
            ("Memory refresh", "{n} cards are about to slip away. Let's catch them."),
        ],
        "quest" => &[
            ("Quest almost done", "Just {n} more to finish a daily quest. Gems are calling."),
            ("So close!", "A daily quest needs only {n} more. Let's wrap it up."),
            ("Finish line ahead", "{n} more and today's quest is yours."),
        ],
        "idle" => &[
            ("The macaw is waiting", "It's been {n} days. I saved you a seat (and a can)."),
            ("Long time no see", "{n} days without studying. Even one lesson counts. No pressure."),
            ("I'm still here", "Just checking in. A two-minute lesson is a great way back."),
            ("Miss you!", "{n} days off. Want to start small today?"),
        ],
        "greet_morning" => &[
            ("Good morning!", "Coffee for you, energy drink for me. Ready when you are."),
            ("Morning!", "I'm awake. Mostly. Let's have a good day."),
        ],
        "greet_afternoon" => &[
            ("Good afternoon!", "Nice to see you. Want to learn something today?"),
            ("Hey there!", "Perfect time for a show… or a quick lesson."),
        ],
        "greet_evening" => &[
            ("Good evening!", "Relax, watch something, maybe a quick lesson?"),
            ("Evening!", "Long day? I've got snacks and flashcards."),
        ],
        "greet_night" => &[
            ("Up late too?", "Me too. Don't forget to rest, OK?"),
            ("Night owl mode", "I'm more of a night macaw. Just one episode, right?"),
        ],
        "greet_streak" => &[
            ("Welcome back!", "Day {n} of your streak. Let's keep it going."),
            ("Hi again!", "{n} days in a row. I'm proud of you (and a bit sleepy)."),
        ],
        _ => &[],
    }
}

/// How many messages a pool has: its own (title, body) pairs, then the macaw's extra lines of crate::mascot_lines
/// (shown under one of the pool's titles; they carry their own three languages).
pub fn pool_len(kind: &str) -> usize {
    pool(kind).len() + crate::mascot_lines::lines(kind).len()
}

/// Which greeting pool fits this hour.
pub fn greet_pool(hour: u32) -> &'static str {
    match hour {
        5..=11 => "greet_morning",
        12..=17 => "greet_afternoon",
        18..=22 => "greet_evening",
        _ => "greet_night",
    }
}

/// True when `hour` is inside the quiet time from `from` to `to` (it may cross midnight; equal = never quiet).
pub fn is_quiet(hour: u32, from: u32, to: u32) -> bool {
    if from == to {
        false
    } else if from < to {
        hour >= from && hour < to
    } else {
        hour >= from || hour < to
    }
}

/// What was shown, kept in study/nudges.json.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NudgeLog {
    /// (kind, unix time it was last shown)
    pub last: Vec<(String, i64)>,
    /// "kind:index" of the messages shown lately, newest last.
    pub recent: Vec<String>,
    /// The local day `count` belongs to, and how many reminders were shown that day.
    pub day: i64,
    pub count: u32,
    /// When the last reminder (of any kind) was shown.
    pub last_any: i64,
    /// The local day of the last greeting.
    pub greeted_day: i64,
}

impl NudgeLog {
    pub fn last_of(&self, kind: &str) -> i64 {
        self.last.iter().find(|(k, _)| k == kind).map(|(_, t)| *t).unwrap_or(0)
    }

    /// Remembers a reminder shown now.
    pub fn record(&mut self, kind: &str, index: usize, now: i64, today: i64) {
        match self.last.iter_mut().find(|(k, _)| k == kind) {
            Some(e) => e.1 = now,
            None => self.last.push((kind.to_string(), now)),
        }
        self.remember(kind, index);
        if self.day != today {
            self.day = today;
            self.count = 0;
        }
        self.count += 1;
        self.last_any = now;
    }

    fn remember(&mut self, pool: &str, index: usize) {
        self.recent.push(format!("{}:{}", pool, index));
        if self.recent.len() > RECENT_KEEP {
            let cut = self.recent.len() - RECENT_KEEP;
            self.recent.drain(..cut);
        }
    }

    fn from_json(j: &Json) -> NudgeLog {
        NudgeLog {
            last: j.get("last").entries().iter().map(|(k, v)| (k.clone(), v.as_i64().unwrap_or(0))).collect(),
            recent: j.get("recent").strings(),
            day: j.get("day").as_i64().unwrap_or(0),
            count: j.get("count").as_i64().unwrap_or(0).max(0) as u32,
            last_any: j.get("last_any").as_i64().unwrap_or(0),
            greeted_day: j.get("greeted_day").as_i64().unwrap_or(0),
        }
    }

    fn to_json(&self) -> Json {
        Json::obj(vec![
            ("last", Json::obj(self.last.iter().map(|(k, t)| (k.as_str(), Json::from(*t))).collect())),
            ("recent", Json::Arr(self.recent.iter().map(|r| r.clone().into()).collect())),
            ("day", self.day.into()),
            ("count", self.count.into()),
            ("last_any", self.last_any.into()),
            ("greeted_day", self.greeted_day.into()),
        ])
    }
}

/// Picks a message of a pool, skipping the ones shown recently (when every one was, the oldest shown wins).
pub fn pick_message(pool_name: &str, log: &NudgeLog, seed: u64) -> usize {
    let len = pool_len(pool_name);
    if len <= 1 {
        return 0;
    }
    let shown_at = |i: usize| log.recent.iter().rposition(|r| *r == format!("{}:{}", pool_name, i));
    let fresh: Vec<usize> = (0..len).filter(|i| shown_at(*i).is_none()).collect();
    if !fresh.is_empty() {
        return fresh[(seed % fresh.len() as u64) as usize];
    }
    (0..len).min_by_key(|i| shown_at(*i)).unwrap_or(0)
}

/// What the reminder rules look at, gathered once a minute.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// Local hour (0-23) and local day number.
    pub hour: u32,
    pub today: i64,
    /// A Learn lesson or an Anki review happened today.
    pub studied_today: bool,
    /// The Learn streak still alive (0 when there is none).
    pub streak: u32,
    pub xp_today: u32,
    pub goal: u32,
    /// The person has used Learn at all (no goal reminders for someone who only watches videos).
    pub learns: bool,
    /// Anki cards due now (learning + reviews; new cards do not count).
    pub due: u32,
    /// What is left of the daily quest closest to done (only when at least half is done).
    pub quest_left: Option<u32>,
    /// Days since the last study (None when the person never studied).
    pub idle_days: Option<i64>,
}

/// The reminder to show now (with its number for "{n}"), or None. `log` says what was shown before.
pub fn choose(f: &Facts, log: &NudgeLog, now: i64) -> Option<(&'static str, i64)> {
    if log.day == f.today && log.count >= MAX_PER_DAY {
        return None;
    }
    if now - log.last_any < MIN_GAP {
        return None;
    }
    let mut wanted: Vec<(&'static str, i64)> = vec![];
    // Saving a streak matters most, and only makes sense in the evening.
    if f.streak > 0 && !f.studied_today && f.hour >= EVENING {
        wanted.push(("streak", f.streak as i64));
    }
    if let Some(left) = f.quest_left {
        wanted.push(("quest", left as i64));
    }
    if f.learns && f.goal > 0 && f.xp_today < f.goal && f.hour >= GOAL_FROM {
        wanted.push(("goal", (f.goal - f.xp_today) as i64));
    }
    if f.due >= DUE_MIN {
        wanted.push(("review", f.due as i64));
    }
    // "The macaw is waiting": two days or more away, but not forever (after a month it stops asking).
    if let Some(d) = f.idle_days {
        if (2..=30).contains(&d) && !f.studied_today {
            wanted.push(("idle", d));
        }
    }
    wanted.into_iter().find(|(k, _)| now - log.last_of(k) >= cooldown(k))
}

/// The reminder machinery's state inside `Core`.
#[derive(Default)]
pub struct Nudges {
    /// Minutes east of UTC and the language of the window (from `banner_hello`); nothing is shown before it comes.
    pub tz: Option<i64>,
    pub lang: String,
    /// When the window said hello (unix seconds).
    started: i64,
    last_check: Option<Instant>,
    log: NudgeLog,
    loaded: bool,
}

fn seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as u64 ^ d.as_secs()).unwrap_or(3)
}

impl Core {
    fn nudge_file(&self) -> PathBuf {
        crate::study::dir(&self.profile_dir).join("nudges.json")
    }

    fn nudge_load(&mut self) {
        if !self.nudges.loaded {
            let text = std::fs::read_to_string(self.nudge_file()).unwrap_or_default();
            self.nudges.log = Json::parse(&text).map(|j| NudgeLog::from_json(&j)).unwrap_or_default();
            self.nudges.loaded = true;
        }
    }

    fn nudge_save(&self) {
        let path = self.nudge_file();
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(&path, self.nudges.log.to_json().pretty());
    }

    /// A text of a pool in the window's language, with "{n}" filled in.
    fn nudge_text(&self, english: &str, n: i64) -> String {
        let t = crate::i18n::translate(&self.nudge_lang(), english).unwrap_or(english);
        t.replace("{n}", &n.to_string())
    }

    /// The window's language (the app's setting until the window says hello).
    fn nudge_lang(&self) -> String {
        if self.nudges.lang.is_empty() {
            self.ui_lang()
        } else {
            self.nudges.lang.clone()
        }
    }

    fn nudge_show(&mut self, pool_name: &str, kind: &str, index: usize, n: i64) {
        let own = pool(pool_name);
        let (title, body) = match own.get(index) {
            Some((title, body)) => (self.nudge_text(title, n), self.nudge_text(body, n)),
            // one of the macaw's extra lines (crate::mascot_lines), under one of the pool's titles
            None => {
                let Some(q) = crate::mascot_lines::lines(pool_name).get(index - own.len()) else { return };
                let Some((title, _)) = own.get(index % own.len().max(1)) else { return };
                let body = crate::mascot_lines::in_lang(q, &self.nudge_lang()).replace("{n}", &n.to_string());
                (self.nudge_text(title, n), body)
            }
        };
        // Every study reminder opens the Study mode; the greeting just goes away.
        let action = if kind == "greet" { None } else { Some(Json::obj(vec![("cmd", "set_mode".into()), ("mode", "study".into())])) };
        self.banner(banner_kind(kind), &title, &body, mood_of(kind), action);
    }

    fn local_now(&self, now: i64) -> (u32, i64) {
        let tz = self.nudges.tz.unwrap_or(0);
        let hour = ((now + tz * 60).rem_euclid(86400) / 3600) as u32;
        (hour, learn::day_of(now, tz))
    }

    /// Everything the rules look at, from study/learn.json and the Anki collection.
    fn nudge_facts(&self, now: i64) -> Facts {
        let (hour, today) = self.local_now(now);
        // The file is the truth (the Learn page saves after every lesson); it is small and read once a minute.
        let p = Progress::load(&crate::study::dir(&self.profile_dir).join("learn.json"));
        let xp_today = p.xp_on(today);
        // learn.json is only "rolled" to a new day when the Learn page opens: a streak whose last day is older than
        // yesterday (and not covered by the freezes) is already lost.
        let streak = if p.streak > 0 && p.streak_day + p.freezes as i64 >= today - 1 { p.streak } else { 0 };
        let anki_today = crate::study::today(now);
        let reviewed_today = self.study.history.iter().any(|h| h[0] == anki_today && h[1] > 0);
        let studied_today = xp_today > 0 || p.streak_day == today || reviewed_today;
        let due: u32 = self
            .study
            .decks
            .iter()
            .filter(|d| !d.name.contains("::"))
            .map(|d| {
                let (_, ln, rv) = self.study.counts(d.id, now);
                ln + rv
            })
            .sum();
        let quest_left = if p.quest_day == today {
            p.quests
                .iter()
                .filter(|q| !q.done && q.target > 0 && q.progress > 0 && q.progress * 2 >= q.target && q.progress < q.target)
                .map(|q| q.target - q.progress)
                .min()
        } else {
            None
        };
        let last_learn = p.days.iter().filter(|(_, x)| *x > 0).map(|(d, _)| *d).max();
        // Anki days start at 04:00 UTC; close enough to the local day for "days away".
        let last_anki = self.study.history.iter().filter(|h| h[1] > 0).map(|h| h[0]).max();
        let idle_days = last_learn.max(last_anki).map(|d| (today - d).max(0));
        Facts { hour, today, studied_today, streak, xp_today, goal: p.goal, learns: !p.days.is_empty(), due, quest_left, idle_days }
    }

    /// The window is up: its time zone and language. Greets once a day (the corner macaw waves along).
    pub fn banner_hello(&mut self, tz: Option<i64>, lang: &str) {
        if let Some(tz) = tz {
            self.nudges.tz = Some(tz.clamp(-14 * 60, 14 * 60));
        }
        if !lang.is_empty() {
            self.nudges.lang = lang.to_string();
        }
        let now = util::now_unix() as i64;
        if self.nudges.started == 0 {
            self.nudges.started = now;
        }
        self.nudge_load();
        if !self.settings.general.banners {
            return;
        }
        let f = self.nudge_facts(now);
        if self.nudges.log.greeted_day == f.today {
            return;
        }
        self.nudges.log.greeted_day = f.today;
        let pool_name = if f.streak >= 2 { "greet_streak" } else { greet_pool(f.hour) };
        let i = pick_message(pool_name, &self.nudges.log, seed());
        self.nudges.log.remember(pool_name, i);
        self.nudge_save();
        self.nudge_show(pool_name, "greet", i, f.streak as i64);
    }

    /// Shows an example of a reminder now, whatever the rules say (Preferences, to see how it looks).
    pub fn banner_test(&mut self, kind: &str) {
        let kind = if pool(kind).is_empty() { "streak" } else { kind };
        let i = (seed() % pool_len(kind) as u64) as usize;
        self.nudge_show(kind, kind, i, 3);
    }

    /// Called from `poll`: looks at the reminders once a minute.
    pub(super) fn nudge_tick(&mut self) {
        if self.nudges.last_check.map(|t| t.elapsed() < CHECK_EVERY).unwrap_or(false) {
            return;
        }
        self.nudges.last_check = Some(Instant::now());
        // Nothing before the window says hello (no time zone yet), and the greeting goes first.
        if self.nudges.tz.is_none() || !self.settings.general.banners {
            return;
        }
        let now = util::now_unix() as i64;
        if now - self.nudges.started < STARTUP_GRACE {
            return;
        }
        // Not over a video, a lesson or a review: the person is busy (it is tried again next minute).
        if self.session.is_some() || self.learn.session.is_some() || self.study_session.is_some() {
            return;
        }
        let g = &self.settings.general;
        let (hour, _) = self.local_now(now);
        if is_quiet(hour, g.banner_quiet_from, g.banner_quiet_to) {
            return;
        }
        self.nudge_load();
        let f = self.nudge_facts(now);
        let Some((kind, n)) = choose(&f, &self.nudges.log, now) else { return };
        let i = pick_message(kind, &self.nudges.log, seed());
        self.nudges.log.record(kind, i, now, f.today);
        self.nudge_save();
        self.nudge_show(kind, kind, i, n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts { hour: 19, today: 100, goal: 20, learns: true, ..Facts::default() }
    }

    #[test]
    fn quiet_hours_cross_midnight() {
        assert!(is_quiet(23, 22, 8));
        assert!(is_quiet(3, 22, 8));
        assert!(!is_quiet(8, 22, 8));
        assert!(!is_quiet(12, 22, 8));
        assert!(is_quiet(14, 13, 15));
        assert!(!is_quiet(15, 13, 15));
        // the same hour on both ends turns the quiet time off
        assert!(!is_quiet(5, 5, 5));
    }

    #[test]
    fn streak_only_in_the_evening_and_before_goal() {
        let now = 1_000_000;
        let mut f = Facts { streak: 4, ..facts() };
        assert_eq!(choose(&f, &NudgeLog::default(), now), Some(("streak", 4)));
        f.hour = 15;
        // in the afternoon the streak is not "at risk" yet: the goal reminder comes instead
        assert_eq!(choose(&f, &NudgeLog::default(), now), Some(("goal", 20)));
        f.studied_today = true;
        f.xp_today = 20;
        assert_eq!(choose(&f, &NudgeLog::default(), now), None);
    }

    #[test]
    fn goal_not_for_people_who_never_learn() {
        let f = Facts { learns: false, ..facts() };
        assert_eq!(choose(&f, &NudgeLog::default(), 1_000_000), None);
        let f = Facts { learns: false, due: 25, ..facts() };
        assert_eq!(choose(&f, &NudgeLog::default(), 1_000_000), Some(("review", 25)));
        let f = Facts { learns: false, due: 3, ..facts() };
        assert_eq!(choose(&f, &NudgeLog::default(), 1_000_000), None);
    }

    #[test]
    fn cooldown_gap_and_daily_cap() {
        let now = 1_000_000;
        let f = Facts { streak: 4, due: 30, ..facts() };
        let mut log = NudgeLog::default();
        log.record("streak", 0, now, f.today);
        // right after one reminder: nothing (the gap between any two)
        assert_eq!(choose(&f, &log, now + 60), None);
        // after the gap, the streak is still cooling down, so the next kind in line comes
        assert_eq!(choose(&f, &log, now + MIN_GAP), Some(("goal", 20)));
        // after its cooldown, the streak comes back first
        assert_eq!(choose(&f, &log, now + cooldown("streak")), Some(("streak", 4)));
        // the daily cap
        log.record("goal", 0, now + 1, f.today);
        log.record("review", 0, now + 2, f.today);
        assert_eq!(log.count, 3);
        assert_eq!(choose(&f, &log, now + 10 * 3600), None);
        // a new day starts the count again
        let tomorrow = Facts { today: f.today + 1, ..f.clone() };
        assert!(choose(&tomorrow, &log, now + 10 * 3600).is_some());
    }

    #[test]
    fn idle_and_quest() {
        let now = 1_000_000;
        let f = Facts { hour: 10, idle_days: Some(3), ..facts() };
        assert_eq!(choose(&f, &NudgeLog::default(), now), Some(("idle", 3)));
        let f = Facts { hour: 10, idle_days: Some(1), ..facts() };
        assert_eq!(choose(&f, &NudgeLog::default(), now), None);
        // after a month away the macaw stops asking
        let f = Facts { hour: 10, idle_days: Some(45), ..facts() };
        assert_eq!(choose(&f, &NudgeLog::default(), now), None);
        let f = Facts { hour: 10, quest_left: Some(2), studied_today: true, ..facts() };
        assert_eq!(choose(&f, &NudgeLog::default(), now), Some(("quest", 2)));
    }

    #[test]
    fn messages_do_not_repeat_soon() {
        let mut log = NudgeLog::default();
        // the macaw's extra lines make this pool bigger than what is remembered
        let len = pool_len("review");
        assert!(len > RECENT_KEEP);
        let mut seen = vec![];
        for s in 0..len as u64 {
            let i = pick_message("review", &log, s * 7 + 1);
            assert!(i < len);
            assert!(!log.recent.contains(&format!("review:{}", i)), "message {} repeated while still remembered", i);
            if seen.len() < RECENT_KEEP {
                assert!(!seen.contains(&i), "message {} repeated soon", i);
            }
            seen.push(i);
            log.record("review", i, 0, 0);
        }
        // a pool that fits in the memory: once all were shown, the oldest comes back
        let mut log = NudgeLog::default();
        let len = pool_len("quest");
        assert!(len <= RECENT_KEEP);
        for i in 0..len {
            log.remember("quest", i);
        }
        assert_eq!(pick_message("quest", &log, 5), 0);
    }

    #[test]
    fn log_round_trip() {
        let mut log = NudgeLog::default();
        log.record("goal", 2, 500, 9);
        log.greeted_day = 9;
        assert_eq!(NudgeLog::from_json(&log.to_json()), log);
    }

    #[test]
    fn every_pool_has_messages() {
        for k in ["streak", "goal", "review", "quest", "idle", "greet_morning", "greet_afternoon", "greet_evening", "greet_night", "greet_streak"] {
            assert!(pool(k).len() >= 2, "{}", k);
            assert!(crate::mascot_lines::lines(k).len() >= 6, "{}", k);
        }
        assert_eq!(greet_pool(7), "greet_morning");
        assert_eq!(greet_pool(2), "greet_night");
    }
}
