//! Time spent studying, and the Pomodoro timer.
//!
//! The log keeps, for every local day, the seconds of each activity, the seconds of each hour (for the day chart)
//! and the Pomodoro focus sessions finished. It lives in `<data dir>/study/time.json`:
//! `{"tz": -180, "days": {"2026-10-14": {"review": 300, "lesson": 120, "quiz": 0, "pomodoro": 600, "study": 60,
//! "pomos": 1, "h": [0, 0, …24 numbers]}}}`.
//! No calendar crate: days are counted from 1970-01-01 in the user's time zone (minutes east of UTC, sent by the
//! window), the way learn.rs does it.
//! This file is part of `controller` (see controller_time.rs for the parts that need `Core`).

use crate::json::Json;
use std::collections::BTreeMap;
use std::path::Path;

/// What the time was spent on. Each second counts once, for the most specific activity going on.
pub const ACTIVITIES: [&str; 5] = ["review", "lesson", "quiz", "pomodoro", "study"];

/// A day counts for the streak from this many seconds of study.
pub const STREAK_MIN_SECS: u64 = 60;

/// The day number of a moment (unix seconds) in the time zone `tz` (minutes east of UTC).
pub fn day_of(now: i64, tz: i64) -> i64 {
    (now + tz * 60).div_euclid(86_400)
}

/// The local hour (0-23) of a moment.
pub fn hour_of(now: i64, tz: i64) -> usize {
    ((now + tz * 60).rem_euclid(86_400) / 3600) as usize
}

/// Calendar date of a day number (Howard Hinnant's algorithm).
pub fn ymd(day: i64) -> (i64, u32, u32) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

/// Day number of a calendar date (the inverse of `ymd`).
pub fn day_from_ymd(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn days_in_month(y: i64, m: u32) -> i64 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    day_from_ymd(ny, nm, 1) - day_from_ymd(y, m, 1)
}

/// 0 = Monday … 6 = Sunday (1970-01-01 was a Thursday).
pub fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

pub fn date_key(day: i64) -> String {
    let (y, m, d) = ymd(day);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

pub fn parse_key(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 || d as i64 > days_in_month(y, m) {
        return None;
    }
    Some(day_from_ymd(y, m, d))
}

/// One local day of the log.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Day {
    /// Seconds per activity, in the order of `ACTIVITIES`.
    pub secs: [u64; 5],
    /// Seconds per local hour.
    pub hours: [u64; 24],
    /// Pomodoro focus sessions finished.
    pub pomos: u32,
}

impl Day {
    pub fn total(&self) -> u64 {
        self.secs.iter().sum()
    }
}

#[derive(Clone, Debug, Default)]
pub struct TimeLog {
    pub days: BTreeMap<i64, Day>,
    /// The last time zone the window told (minutes east of UTC).
    pub tz: i64,
}

/// Sums over a range of days.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sum {
    pub total: u64,
    pub split: [u64; 5],
    pub pomos: u32,
    /// Days with at least `STREAK_MIN_SECS`.
    pub studied: u32,
}

impl TimeLog {
    pub fn load(path: &Path) -> TimeLog {
        std::fs::read_to_string(path).ok().and_then(|t| Json::parse(&t).ok()).map(|j| TimeLog::from_json(&j)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        // write then rename, so a crash never leaves half a file
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, self.to_json().pretty()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    pub fn from_json(j: &Json) -> TimeLog {
        let mut log = TimeLog { days: BTreeMap::new(), tz: j.get("tz").as_i64().unwrap_or(0).clamp(-14 * 60, 14 * 60) };
        for (k, v) in j.get("days").entries() {
            let Some(day) = parse_key(k) else { continue };
            let mut d = Day::default();
            for (i, a) in ACTIVITIES.iter().enumerate() {
                d.secs[i] = v.get(a).as_u64().unwrap_or(0);
            }
            for (i, h) in v.get("h").items().iter().take(24).enumerate() {
                d.hours[i] = h.as_u64().unwrap_or(0);
            }
            d.pomos = v.get("pomos").as_u64().unwrap_or(0) as u32;
            log.days.insert(day, d);
        }
        log
    }

    pub fn to_json(&self) -> Json {
        let days = self
            .days
            .iter()
            .map(|(k, d)| {
                let mut pairs: Vec<(&str, Json)> = ACTIVITIES.iter().zip(d.secs.iter()).map(|(a, s)| (*a, Json::Num(*s as f64))).collect();
                pairs.push(("pomos", Json::Num(d.pomos as f64)));
                pairs.push(("h", Json::Arr(d.hours.iter().map(|h| Json::Num(*h as f64)).collect())));
                (date_key(*k), Json::obj(pairs))
            })
            .collect();
        Json::obj(vec![("tz", Json::Num(self.tz as f64)), ("days", Json::Obj(days))])
    }

    /// Adds `secs` seconds of `activity` at the moment `now` (unix seconds).
    pub fn add(&mut self, now: i64, secs: u64, activity: &str) {
        let Some(i) = ACTIVITIES.iter().position(|a| *a == activity) else { return };
        let d = self.days.entry(day_of(now, self.tz)).or_default();
        d.secs[i] += secs;
        d.hours[hour_of(now, self.tz)] += secs;
    }

    /// A Pomodoro focus session was finished.
    pub fn add_pomo(&mut self, now: i64) {
        self.days.entry(day_of(now, self.tz)).or_default().pomos += 1;
    }

    pub fn day_total(&self, day: i64) -> u64 {
        self.days.get(&day).map(|d| d.total()).unwrap_or(0)
    }

    /// Sums of the days `from..=to`.
    pub fn sum(&self, from: i64, to: i64) -> Sum {
        let mut s = Sum::default();
        for (_, d) in self.days.range(from..=to) {
            let t = d.total();
            s.total += t;
            for i in 0..5 {
                s.split[i] += d.secs[i];
            }
            s.pomos += d.pomos;
            if t >= STREAK_MIN_SECS {
                s.studied += 1;
            }
        }
        s
    }

    /// Days in a row with some study, ending today (or yesterday, when nothing was done yet today).
    pub fn streak(&self, today: i64) -> u32 {
        let mut d = if self.day_total(today) >= STREAK_MIN_SECS { today } else { today - 1 };
        let mut n = 0;
        while self.day_total(d) >= STREAK_MIN_SECS {
            n += 1;
            d -= 1;
        }
        n
    }

    /// The longest run of study days ever.
    pub fn longest_streak(&self) -> u32 {
        let (mut best, mut run, mut prev) = (0u32, 0u32, i64::MIN);
        for (day, d) in &self.days {
            if d.total() < STREAK_MIN_SECS {
                continue;
            }
            run = if prev != i64::MIN && *day == prev + 1 { run + 1 } else { 1 };
            prev = *day;
            best = best.max(run);
        }
        best
    }

    /// The report of one period: "day", "week" (Monday first), "month" or "year"; `offset` 0 = the current one,
    /// -1 = the one before… The comparison is with the previous period up to the same point (this week until
    /// Wednesday against last week until Wednesday), so a period in progress is not compared with a whole one.
    pub fn report(&self, now: i64, period: &str, offset: i64) -> Json {
        let today = day_of(now, self.tz);
        let offset = offset.min(0);
        let (y, m, _) = ymd(today);
        let num = |v: u64| Json::Num(v as f64);
        let mut bars: Vec<Json> = Vec::new();
        let (start, end, elapsed, prev, best);
        match period {
            "day" => {
                let d = today + offset;
                start = d;
                end = d;
                elapsed = 1;
                let day = self.days.get(&d).cloned().unwrap_or_default();
                let yest = self.days.get(&(d - 1)).cloned().unwrap_or_default();
                // today: yesterday until the same hour
                let upto = if offset == 0 { hour_of(now, self.tz) } else { 23 };
                prev = yest.hours[..=upto].iter().sum::<u64>();
                for (h, s) in day.hours.iter().enumerate() {
                    bars.push(Json::obj(vec![("secs", num(*s)), ("hour", num(h as u64)), ("current", Json::Bool(offset == 0 && h == upto))]));
                }
                let (bh, bs) = day.hours.iter().enumerate().max_by_key(|(h, s)| (**s, std::cmp::Reverse(*h))).map(|(h, s)| (h, *s)).unwrap_or((0, 0));
                best = if bs > 0 { Json::obj(vec![("hour", num(bh as u64)), ("secs", num(bs))]) } else { Json::Null };
            }
            "month" => {
                let mi = y * 12 + (m as i64 - 1) + offset;
                let (yy, mm) = (mi.div_euclid(12), (mi.rem_euclid(12) + 1) as u32);
                start = day_from_ymd(yy, mm, 1);
                let len = days_in_month(yy, mm);
                end = start + len - 1;
                elapsed = if offset == 0 { today - start + 1 } else { len };
                let pi = mi - 1;
                let (py, pm) = (pi.div_euclid(12), (pi.rem_euclid(12) + 1) as u32);
                let ps = day_from_ymd(py, pm, 1);
                prev = self.sum(ps, ps + elapsed.min(days_in_month(py, pm)) - 1).total;
                for d in start..=end {
                    bars.push(Json::obj(vec![("secs", num(self.day_total(d))), ("date", Json::Str(date_key(d))), ("current", Json::Bool(d == today))]));
                }
                best = self.best_day(start, end);
            }
            "year" => {
                let yy = y + offset;
                start = day_from_ymd(yy, 1, 1);
                let len = day_from_ymd(yy + 1, 1, 1) - start;
                end = start + len - 1;
                elapsed = if offset == 0 { today - start + 1 } else { len };
                let ps = day_from_ymd(yy - 1, 1, 1);
                prev = self.sum(ps, ps + elapsed.min(start - ps) - 1).total;
                for mo in 1..=12u32 {
                    let a = day_from_ymd(yy, mo, 1);
                    let s = self.sum(a, a + days_in_month(yy, mo) - 1).total;
                    bars.push(Json::obj(vec![("secs", num(s)), ("month", num(mo as u64)), ("current", Json::Bool(offset == 0 && mo == m))]));
                }
                best = self.best_day(start, end);
            }
            _ => {
                // the week, Monday first
                start = today - weekday(today) + 7 * offset;
                end = start + 6;
                elapsed = if offset == 0 { weekday(today) + 1 } else { 7 };
                prev = self.sum(start - 7, start - 7 + elapsed - 1).total;
                for d in start..=end {
                    bars.push(Json::obj(vec![("secs", num(self.day_total(d))), ("date", Json::Str(date_key(d))), ("current", Json::Bool(d == today))]));
                }
                best = self.best_day(start, end);
            }
        }
        let s = self.sum(start, end);
        let change = if prev > 0 { Json::Num(((s.total as f64 - prev as f64) * 100.0 / prev as f64).round()) } else { Json::Null };
        let split = ACTIVITIES.iter().zip(s.split.iter()).map(|(a, v)| Json::obj(vec![("id", Json::Str(a.to_string())), ("secs", num(*v))])).collect();
        Json::obj(vec![
            ("period", Json::Str(if ["day", "month", "year"].contains(&period) { period.to_string() } else { "week".into() })),
            ("offset", Json::Num(offset as f64)),
            ("start", Json::Str(date_key(start))),
            ("end", Json::Str(date_key(end))),
            ("total", num(s.total)),
            ("prev", num(prev)),
            ("change", change),
            ("elapsed", num(elapsed.max(1) as u64)),
            ("avg", num(s.total / elapsed.max(1) as u64)),
            ("studied", num(s.studied as u64)),
            ("pomos", num(s.pomos as u64)),
            ("best", best),
            ("split", Json::Arr(split)),
            ("bars", Json::Arr(bars)),
        ])
    }

    fn best_day(&self, from: i64, to: i64) -> Json {
        let mut best: Option<(i64, u64)> = None;
        for (d, v) in self.days.range(from..=to) {
            let t = v.total();
            if t > 0 && best.map(|(_, b)| t > b).unwrap_or(true) {
                best = Some((*d, t));
            }
        }
        match best {
            Some((d, t)) => Json::obj(vec![("date", Json::Str(date_key(d))), ("secs", Json::Num(t as f64))]),
            None => Json::Null,
        }
    }

    /// One line per day with study: date, total and each activity in seconds, and the Pomodoros.
    pub fn csv(&self) -> String {
        let mut out = String::from("date,total_seconds,");
        out.push_str(&ACTIVITIES.iter().map(|a| format!("{}_seconds", a)).collect::<Vec<_>>().join(","));
        out.push_str(",pomodoros\n");
        for (k, d) in &self.days {
            out.push_str(&format!("{},{},", date_key(*k), d.total()));
            out.push_str(&d.secs.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","));
            out.push_str(&format!(",{}\n", d.pomos));
        }
        out
    }
}

// ---------------- the Pomodoro timer ----------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Focus,
    Short,
    Long,
}

impl Phase {
    pub fn id(self) -> &'static str {
        match self {
            Phase::Focus => "focus",
            Phase::Short => "short",
            Phase::Long => "long",
        }
    }
}

/// Lengths in minutes, and how many focus sessions come before a long break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PomoConfig {
    pub focus: u32,
    pub short: u32,
    pub long: u32,
    pub rounds: u32,
}

impl Default for PomoConfig {
    fn default() -> Self {
        PomoConfig { focus: 25, short: 5, long: 15, rounds: 4 }
    }
}

impl PomoConfig {
    pub fn ms(&self, p: Phase) -> i64 {
        let min = match p {
            Phase::Focus => self.focus,
            Phase::Short => self.short,
            Phase::Long => self.long,
        };
        min.max(1) as i64 * 60_000
    }
}

/// What happened when a phase ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PomoEnd {
    /// A focus session finished; `long` = the break that comes is the long one.
    Focus { long: bool },
    /// A break finished.
    Break,
}

/// The timer. Times are unix milliseconds, so the window can count down by itself from `ends_at`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pomodoro {
    pub phase: Phase,
    pub running: bool,
    /// While running: when the phase ends.
    pub ends_at: i64,
    /// While stopped: what is left of the phase.
    pub left: i64,
    /// Focus sessions finished in this cycle (the long break comes after `rounds` of them).
    pub done: u32,
    /// The phase has not been started yet (its length follows the settings).
    pub fresh: bool,
}

impl Pomodoro {
    pub fn new(cfg: &PomoConfig) -> Pomodoro {
        Pomodoro { phase: Phase::Focus, running: false, ends_at: 0, left: cfg.ms(Phase::Focus), done: 0, fresh: true }
    }

    /// Milliseconds left at `now`.
    pub fn remaining(&self, now: i64) -> i64 {
        if self.running {
            (self.ends_at - now).max(0)
        } else {
            self.left.max(0)
        }
    }

    pub fn in_focus(&self) -> bool {
        self.running && self.phase == Phase::Focus
    }

    pub fn start(&mut self, now: i64, cfg: &PomoConfig) {
        if self.running {
            return;
        }
        if self.left <= 0 {
            self.left = cfg.ms(self.phase);
        }
        self.ends_at = now + self.left;
        self.running = true;
        self.fresh = false;
    }

    pub fn pause(&mut self, now: i64) {
        if !self.running {
            return;
        }
        self.left = (self.ends_at - now).max(0);
        self.running = false;
    }

    /// Back to the start of the phase; when it already was there, back to a new cycle (focus, round 1).
    pub fn reset(&mut self, cfg: &PomoConfig) {
        if self.fresh && !self.running {
            *self = Pomodoro::new(cfg);
            return;
        }
        self.running = false;
        self.left = cfg.ms(self.phase);
        self.fresh = true;
    }

    /// The settings changed: a phase not started yet takes the new length.
    pub fn configure(&mut self, cfg: &PomoConfig) {
        if self.fresh && !self.running {
            self.left = cfg.ms(self.phase);
        }
        if self.phase != Phase::Long {
            self.done = self.done.min(cfg.rounds.max(1) - 1);
        }
    }

    /// Goes to the next phase. A skipped focus session moves the cycle on but is not counted as finished
    /// (the caller decides what to log); the timer keeps running if it was.
    pub fn skip(&mut self, now: i64, cfg: &PomoConfig) {
        let was = self.running;
        self.advance(cfg);
        if was {
            self.start(now, cfg);
        }
    }

    fn advance(&mut self, cfg: &PomoConfig) -> PomoEnd {
        let end = match self.phase {
            Phase::Focus => {
                self.done += 1;
                let long = self.done >= cfg.rounds.max(1);
                self.phase = if long { Phase::Long } else { Phase::Short };
                PomoEnd::Focus { long }
            }
            Phase::Short => {
                self.phase = Phase::Focus;
                PomoEnd::Break
            }
            Phase::Long => {
                self.phase = Phase::Focus;
                self.done = 0;
                PomoEnd::Break
            }
        };
        self.running = false;
        self.left = cfg.ms(self.phase);
        self.fresh = true;
        end
    }

    /// Call often: when the running phase is over, moves to the next one (started at once when `auto`, else
    /// waiting for Start) and says what ended.
    pub fn tick(&mut self, now: i64, cfg: &PomoConfig, auto: bool) -> Option<PomoEnd> {
        if !self.running || now < self.ends_at {
            return None;
        }
        let end = self.advance(cfg);
        if auto {
            self.start(now, cfg);
        }
        Some(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-14 (a Wednesday) 12:00 UTC
    const NOON: i64 = 1_791_979_200;

    #[test]
    fn calendar_round_trip() {
        assert_eq!(ymd(0), (1970, 1, 1));
        assert_eq!(day_from_ymd(1970, 1, 1), 0);
        let d = day_of(NOON, 0);
        assert_eq!(ymd(d), (2026, 10, 14));
        assert_eq!(weekday(d), 2); // Wednesday
        assert_eq!(parse_key("2026-10-14"), Some(d));
        assert_eq!(date_key(d), "2026-10-14");
        assert_eq!(parse_key("2026-02-30"), None);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2026, 12), 31);
        for day in [-800i64, -1, 0, 59, 11_000, 20_740, 30_000] {
            let (y, m, dd) = ymd(day);
            assert_eq!(day_from_ymd(y, m, dd), day);
        }
    }

    #[test]
    fn time_zone_moves_the_day() {
        // 01:30 UTC on the 15th is still the 14th in Brazil (UTC-3)
        let t = NOON + 13 * 3600 + 1800;
        assert_eq!(ymd(day_of(t, -180)), (2026, 10, 14));
        assert_eq!(hour_of(t, -180), 22);
        assert_eq!(ymd(day_of(t, 0)), (2026, 10, 15));
        let mut log = TimeLog { tz: -180, ..Default::default() };
        log.add(t, 120, "review");
        let d = day_of(t, -180);
        assert_eq!(log.days[&d].secs[0], 120);
        assert_eq!(log.days[&d].hours[22], 120);
    }

    #[test]
    fn aggregates_day_week_month_year() {
        let mut log = TimeLog::default();
        let today = day_of(NOON, 0);
        log.add(NOON, 600, "review"); // Wed 14th, 12h
        log.add(NOON - 3600, 300, "lesson"); // Wed 11h
        log.add(NOON - 86_400, 900, "quiz"); // Tue 13th
        log.add(NOON - 2 * 86_400, 60, "study"); // Mon 12th
        log.add(NOON - 3 * 86_400, 1200, "pomodoro"); // Sun 11th: last week
        log.add(NOON - 14 * 86_400, 400, "review"); // 30 Sep: last month
        log.add(NOON - 400 * 86_400, 100, "review"); // 9 Sep 2025: last year
        log.add_pomo(NOON);

        let day = log.report(NOON, "day", 0);
        assert_eq!(day.get("total").as_u64(), Some(900));
        assert_eq!(day.get("bars").items().len(), 24);
        assert_eq!(day.get("bars").at(12).get("secs").as_u64(), Some(600));
        assert_eq!(day.get("best").get("hour").as_u64(), Some(12));
        assert_eq!(day.get("pomos").as_u64(), Some(1));
        // yesterday: all of it was at 12h, before this hour's end
        assert_eq!(day.get("prev").as_u64(), Some(900));
        assert_eq!(day.get("change").as_f64(), Some(0.0));

        let week = log.report(NOON, "week", 0);
        assert_eq!(week.get("total").as_u64(), Some(900 + 900 + 60));
        assert_eq!(week.get("start").str(), "2026-10-12");
        assert_eq!(week.get("bars").items().len(), 7);
        assert_eq!(week.get("bars").at(2).get("current").as_bool(), Some(true));
        assert_eq!(week.get("elapsed").as_u64(), Some(3));
        assert_eq!(week.get("avg").as_u64(), Some(620));
        assert_eq!(week.get("studied").as_u64(), Some(3));
        // Tuesday and Wednesday tie at 900 s: the first one wins
        assert_eq!(week.get("best").get("date").str(), "2026-10-13");
        // last week Mon..Wed had nothing (Sunday is outside the same span)
        assert_eq!(week.get("prev").as_u64(), Some(0));
        assert!(week.get("change").is_null());
        let split = week.get("split");
        assert_eq!(split.at(0).get("secs").as_u64(), Some(600));
        assert_eq!(split.at(1).get("secs").as_u64(), Some(300));
        assert_eq!(split.at(2).get("secs").as_u64(), Some(900));
        assert_eq!(split.at(4).get("secs").as_u64(), Some(60));

        let last_week = log.report(NOON, "week", -1);
        assert_eq!(last_week.get("total").as_u64(), Some(1200));
        assert_eq!(last_week.get("start").str(), "2026-10-05");

        let month = log.report(NOON, "month", 0);
        assert_eq!(month.get("total").as_u64(), Some(900 + 900 + 60 + 1200));
        assert_eq!(month.get("bars").items().len(), 31);
        // September 1..14 had 400
        assert_eq!(month.get("prev").as_u64(), Some(0));
        let sep = log.report(NOON, "month", -1);
        assert_eq!(sep.get("total").as_u64(), Some(400));
        assert_eq!(sep.get("bars").items().len(), 30);
        assert_eq!(sep.get("start").str(), "2026-09-01");
        assert_eq!(sep.get("avg").as_u64(), Some(400 / 30));

        let year = log.report(NOON, "year", 0);
        assert_eq!(year.get("total").as_u64(), Some(900 + 900 + 60 + 1200 + 400));
        assert_eq!(year.get("bars").items().len(), 12);
        assert_eq!(year.get("bars").at(9).get("secs").as_u64(), Some(3060));
        assert_eq!(year.get("bars").at(8).get("secs").as_u64(), Some(400));
        // 2025 up to the same day of the year
        assert_eq!(year.get("prev").as_u64(), Some(100));
        assert_eq!(year.get("change").as_f64(), Some(3360.0));
        assert_eq!(year.get("elapsed").as_u64(), Some((today - day_from_ymd(2026, 1, 1) + 1) as u64));
        let y2025 = log.report(NOON, "year", -1);
        assert_eq!(y2025.get("total").as_u64(), Some(100));
        assert_eq!(y2025.get("elapsed").as_u64(), Some(365));
    }

    #[test]
    fn streaks() {
        let mut log = TimeLog::default();
        let today = day_of(NOON, 0);
        assert_eq!(log.streak(today), 0);
        log.add(NOON - 86_400, 120, "review");
        log.add(NOON - 2 * 86_400, 120, "review");
        log.add(NOON - 3 * 86_400, 30, "review"); // too little to count
        log.add(NOON - 4 * 86_400, 120, "review");
        // nothing yet today: the streak still counts until yesterday
        assert_eq!(log.streak(today), 2);
        log.add(NOON, 61, "study");
        assert_eq!(log.streak(today), 3);
        assert_eq!(log.longest_streak(), 3);
    }

    #[test]
    fn json_round_trip_and_csv() {
        let mut log = TimeLog { tz: 60, ..Default::default() };
        log.add(NOON, 42, "lesson");
        log.add_pomo(NOON);
        let back = TimeLog::from_json(&Json::parse(&log.to_json().dump()).unwrap());
        assert_eq!(back.tz, 60);
        assert_eq!(back.days, log.days);
        let csv = log.csv();
        assert!(csv.starts_with("date,total_seconds,review_seconds,"));
        assert!(csv.contains("2026-10-14,42,0,42,0,0,0,1"));
    }

    #[test]
    fn pomodoro_cycle() {
        let cfg = PomoConfig { focus: 25, short: 5, long: 15, rounds: 2 };
        let mut p = Pomodoro::new(&cfg);
        let mut now = 1_000_000;
        assert_eq!(p.remaining(now), 25 * 60_000);
        assert_eq!(p.tick(now, &cfg, false), None);
        p.start(now, &cfg);
        assert!(p.in_focus());
        now += 10 * 60_000;
        assert_eq!(p.tick(now, &cfg, false), None);
        p.pause(now);
        assert_eq!(p.remaining(now + 999_999), 15 * 60_000);
        now += 60_000;
        p.start(now, &cfg);
        now += 15 * 60_000;
        assert_eq!(p.tick(now, &cfg, false), Some(PomoEnd::Focus { long: false }));
        assert_eq!(p.phase, Phase::Short);
        assert!(!p.running);
        assert_eq!(p.remaining(now), 5 * 60_000);
        // automatic start of the next phase
        p.start(now, &cfg);
        now += 5 * 60_000;
        assert_eq!(p.tick(now, &cfg, true), Some(PomoEnd::Break));
        assert_eq!(p.phase, Phase::Focus);
        assert!(p.running);
        now += 25 * 60_000;
        // second focus of two: the long break
        assert_eq!(p.tick(now, &cfg, true), Some(PomoEnd::Focus { long: true }));
        assert_eq!(p.phase, Phase::Long);
        assert_eq!(p.done, 2);
        now += 15 * 60_000;
        assert_eq!(p.tick(now, &cfg, false), Some(PomoEnd::Break));
        assert_eq!((p.phase, p.done, p.running), (Phase::Focus, 0, false));
    }

    #[test]
    fn pomodoro_skip_reset_configure() {
        let cfg = PomoConfig::default();
        let mut p = Pomodoro::new(&cfg);
        p.start(0, &cfg);
        p.skip(1000, &cfg);
        assert_eq!(p.phase, Phase::Short);
        assert!(p.running);
        assert_eq!(p.remaining(1000), 5 * 60_000);
        p.skip(2000, &cfg);
        assert_eq!((p.phase, p.done), (Phase::Focus, 1));
        // reset once: the phase starts over; twice: a new cycle
        p.pause(5000);
        p.reset(&cfg);
        assert_eq!((p.phase, p.done, p.remaining(0)), (Phase::Focus, 1, 25 * 60_000));
        p.reset(&cfg);
        assert_eq!(p, Pomodoro::new(&cfg));
        // new lengths reach a phase not started yet, not one under way
        let short = PomoConfig { focus: 50, ..cfg };
        p.configure(&short);
        assert_eq!(p.remaining(0), 50 * 60_000);
        p.start(0, &short);
        p.configure(&cfg);
        assert_eq!(p.remaining(0), 50 * 60_000);
    }
}
