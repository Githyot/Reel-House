//! Study while watching (docs/learn-contract.md §3). When the subtitle on screen is in the language being learned
//! (`general.study_lang`) and `general.study_in_player` is on, a few lines the user should already understand (every
//! word known, see `Knowledge::line_known`) are chosen over the episode:
//! - "card": after the line ends the episode pauses and a short question about it shows (its translation to
//!   choose, the meaning of one of its words, its words to put in order, or the matching flashcard);
//! - "listen": a 20-40 s part plays without subtitles (a notice says so first), then the user types the last line
//!   heard.
//! Answers go to the knowledge state (`record_line`). The marks are drawn on the seek bar (`player.studyMarks`),
//! the question is `player.studyQuiz`. Part of `controller` (it needs the private parts of `Core`).
//! The subtitle lines come from the subtitle file when it is one (downloaded, translated or added); a subtitle
//! inside the video can only be read while it plays, so then the lines are collected as they show and only "card"
//! questions are made.
use super::*;
use crate::animeskip::Kind;
use crate::knowledge::CardRef;
use crate::subtitles::{decode, parse_cues};
use super::wordmix::{self, Heard, Lexicon, Span};

/// A line of the subtitle: when it shows and its text (one line, no tags).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// A planned question: `t..end` is the line ("card") or the part without subtitles ("listen"); `line` is the index
/// of the line asked about.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub t: f64,
    pub end: f64,
    pub kind: &'static str,
    pub line: usize,
    pub done: bool,
    /// "listen": the notice before it was shown.
    pub noticed: bool,
}

/// The answer to a question, checked.
#[derive(Clone, Debug, Default)]
pub struct QuizResult {
    pub correct: bool,
    pub expected: String,
    pub given: String,
    /// The expected text in pieces, each marked right (found in the answer) or missed.
    pub diff: Vec<(String, bool)>,
}

/// The question on screen.
#[derive(Clone, Debug, Default)]
pub struct Quiz {
    pub id: u64,
    /// "card" or "listen" (the mark)
    pub kind: String,
    /// "translate" (choose the translation), "word" (choose the meaning of a word), "build" (put the words in order),
    /// "type" (type what was heard), "card" (a flashcard of the user, graded by the user)
    pub challenge: String,
    pub line: Line,
    /// The line is shown with the question (hidden when it is the answer).
    pub show_text: bool,
    pub options: Vec<String>,
    pub answer_idx: usize,
    pub bank: Vec<String>,
    pub word: String,
    pub expected: String,
    pub translation: String,
    pub card: Option<CardRef>,
    pub result: Option<QuizResult>,
    /// The dictionary form of `word` when it is another form of it (a swapped word copied from the line heard): the
    /// answer counts for it.
    pub base: String,
}

#[derive(Default)]
pub struct StudyPlay {
    /// The session these lines belong to (its id).
    session: u64,
    /// The subtitle track the lines were read from, and the settings used ("" = not read yet).
    key: String,
    lang3: String,
    main3: String,
    lines: Vec<Line>,
    translations: Vec<Line>,
    /// The subtitle is inside the video: lines are collected as they show.
    progressive: bool,
    pub marks: Vec<Mark>,
    last_pos: f64,
    last_track_check: Option<Instant>,
    pub quiz: Option<Quiz>,
    serial: u64,
    /// The subtitles were hidden by a "listen" part or a replay (sub-visibility is set back when done).
    hidden: bool,
    /// Playing the asked line again; the episode pauses at this time.
    replay_until: Option<f64>,
    pub notice: Option<(String, Instant)>,
    /// Embedded subtitles: the start of the line seen last.
    seen_start: f64,
    /// Learned words mixed into the subtitle in a known language (kept when the questions reset).
    mix: Mix,
    /// The macaw's last cue for the window (`player.studyCue`): serial, cue, milliseconds; and whether its "shh"
    /// is showing (a part without subtitles).
    cue: Option<(u64, &'static str, u64)>,
    cue_serial: u64,
    shh: bool,
    /// The learned words mixed into the subtitle this episode: asked by the macaw at the credits and listed by its
    /// button in the player (kept when the questions reset).
    pub swapped: Swapped,
}

/// A learned word that word mixing put into a line of the episode.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SwapSeen {
    /// The learned word ("食べる"), its reading, the word of the known language it replaced ("comer").
    pub word: String,
    pub reading: String,
    pub original: String,
    /// The dictionary form when `word` is another form of it, as said in the audio ("あった" of ある).
    pub base: String,
    /// The line as shown (mixed) and as it was.
    pub line: String,
    pub plain: String,
    /// When it showed (seconds into the episode).
    pub at: f64,
}

/// The words swapped in the episode playing, and the macaw's quiz on them at the credits.
#[derive(Clone, Debug, Default)]
pub struct Swapped {
    /// The session they belong to.
    pub session: u64,
    pub words: Vec<SwapSeen>,
    /// The credits quiz was already asked (once per episode).
    pub asked: bool,
    /// Words still to ask in the credits quiz, and how many it has in all.
    pub queue: Vec<SwapSeen>,
    pub total: usize,
}

/// How many words the macaw asks at the credits.
pub const CREDITS_QUIZ_MAX: usize = 3;

/// Keeps a swapped word once (the same learned word in the episode counts once; the first line it showed in).
pub fn note_swap(list: &mut Vec<SwapSeen>, seen: SwapSeen) -> bool {
    if seen.word.trim().is_empty() || list.iter().any(|x| x.word == seen.word) {
        return false;
    }
    list.push(seen);
    true
}

/// Where the credits begin: the ending of the skip data, else near the end of a long enough video.
pub fn credits_start(segments: &[(f64, bool)], dur: f64) -> Option<f64> {
    if let Some(t) = segments.iter().filter(|(_, credits)| *credits).map(|(t, _)| *t).fold(None, |a: Option<f64>, t| Some(a.map_or(t, |a| a.max(t)))) {
        return Some(t);
    }
    if dur < 300.0 {
        return None;
    }
    Some(dur - (dur * 0.05).clamp(60.0, 300.0))
}

/// The words for the credits quiz: the least known first (lowest recall), at most `max`.
pub fn credits_pick(words: &[SwapSeen], strength: impl Fn(&str) -> f64, max: usize) -> Vec<SwapSeen> {
    let mut v: Vec<(f64, usize)> = words.iter().enumerate().map(|(i, w)| (strength(&w.word), i)).collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    v.into_iter().take(max).map(|(_, i)| words[i].clone()).collect()
}

/// Word mixing in the player: the subtitle on screen is in a language the user knows; lines picked at the rate get
/// one or two learned words (wordmix.rs). mpv's own subtitle is hidden while a mixed line shows and the line is drawn
/// with mpv's `osd-overlay` in the same look, so it stays inside the video (full screen, every renderer).
#[derive(Default)]
pub struct Mix {
    /// The subtitle track and settings it was prepared for ("" = not checked yet, "-" = mixing does not apply).
    key: String,
    last_check: Option<Instant>,
    lex: Option<Lexicon>,
    /// When the learned words were read (they change slowly: read again every few minutes).
    built: Option<Instant>,
    look: wordmix::Look,
    /// A subtitle file: its lines, so a mixed line is known a moment before it shows (no flash of the original).
    lines: Vec<Line>,
    /// Mixed lines already worked out: text -> (new text, spans) (no spans = not mixed).
    cache: HashMap<String, (String, Vec<Span>)>,
    /// The ASS drawn now ("" = nothing), and whether mpv's subtitle was hidden for it.
    drawn: String,
    hidden: bool,
    /// The mixed text the macaw last peeked at.
    peeked: String,
    /// The subtitle in the language of the audio (the study one), not shown: a word is swapped only when it is said
    /// in the line heard at that moment, and as it is said there (wordmix `swap_heard`).
    orig: Vec<Line>,
    /// The episode and language it was looked for ("" = not yet), a search running, searches made, where it came from.
    orig_key: String,
    orig_busy: bool,
    orig_tries: u32,
    orig_from: String,
    /// The audio playing is in the study language (or untagged): the original subtitle matches what is heard.
    audio_ok: bool,
    /// The viewer was told that no original subtitle was found (once per episode).
    orig_told: bool,
}

/// How many times (every 2 s) the original subtitle is looked for before giving up: the addons answer late.
const ORIG_TRIES: u32 = 15;

/// The text of the lines of `orig` said during `start..end` (a line of the other subtitle): those that overlap it by
/// a good part, with a little room for subtitles timed by different people.
pub fn heard_at(orig: &[Line], start: f64, end: f64) -> String {
    let (s, e) = (start - 0.3, end.max(start) + 0.3);
    orig.iter()
        .filter(|l| {
            let over = e.min(l.end) - s.max(l.start);
            let short = (l.end - l.start).min(e - s).max(0.1);
            over > 0.0 && over >= 0.35 * short
        })
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The id of the overlay with the mixed line (any number no other overlay of the app uses).
const MIX_OVERLAY: &str = "47";

impl StudyPlay {
    /// The subtitle in the language of the audio found for the episode `sid` (empty when none yet), and whether it is
    /// still being looked for (mining waits for it: its cards show that line).
    pub(super) fn orig_lines(&self, sid: u64) -> (&[Line], bool) {
        let mx = &self.mix;
        let ours = mx.orig_key.split('|').next() == Some(sid.to_string().as_str());
        if !ours {
            return (&[], true);
        }
        (&mx.orig, mx.orig.is_empty() && (mx.orig_busy || mx.orig_tries < ORIG_TRIES))
    }

    fn active(&self) -> bool {
        !self.key.is_empty() || self.quiz.is_some() || self.hidden
    }

    /// Tells the window to show a cue of the macaw (docs/mascot-contract.md, `Core.mascotCue`).
    fn send_cue(&mut self, cue: &'static str, ms: u64) {
        self.cue_serial += 1;
        self.cue = Some((self.cue_serial, cue, ms));
    }
}

/// 2-letter code to the 3-letter one of the course packs and the knowledge file ("ja" -> "jpn").
pub fn lang3(l: &str) -> String {
    match util::lang_base(l).as_str() {
        "ja" => "jpn".into(),
        "en" => "eng".into(),
        "pt" => "por".into(),
        "es" => "spa".into(),
        "fr" => "fra".into(),
        "de" => "deu".into(),
        "it" => "ita".into(),
        "ko" => "kor".into(),
        "zh" => "cmn".into(),
        "ru" => "rus".into(),
        other => other.to_string(),
    }
}

/// "0:01:02.50" (ASS) or "00:01:02,500" (SRT) in seconds.
pub fn secs(t: &str) -> f64 {
    let t = t.trim().replace(',', ".");
    let p: Vec<f64> = t.split(':').map(|x| x.trim().parse::<f64>().unwrap_or(0.0)).collect();
    match p.len() {
        3 => p[0] * 3600.0 + p[1] * 60.0 + p[2],
        2 => p[0] * 60.0 + p[1],
        _ => p.first().copied().unwrap_or(0.0),
    }
}

/// The text of a subtitle line as one plain line: no HTML / ASS tags, no line breaks.
pub fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut depth_tag = false;
    let mut depth_ass = false;
    for c in text.replace("\\N", " ").replace("\\n", " ").chars() {
        match c {
            '<' => depth_tag = true,
            '>' if depth_tag => depth_tag = false,
            '{' => depth_ass = true,
            '}' if depth_ass => depth_ass = false,
            _ if depth_tag || depth_ass => {}
            '\n' => out.push(' '),
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The dialogue lines of an ASS / SSA file.
pub fn parse_ass(text: &str) -> Vec<Line> {
    let mut fmt: Vec<String> = vec![];
    let mut out = vec![];
    let mut in_events = false;
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            in_events = l.eq_ignore_ascii_case("[events]");
            continue;
        }
        if !in_events {
            continue;
        }
        if let Some(f) = l.strip_prefix("Format:") {
            fmt = f.split(',').map(|x| x.trim().to_lowercase()).collect();
            continue;
        }
        let Some(body) = l.strip_prefix("Dialogue:") else { continue };
        let n = if fmt.is_empty() { 10 } else { fmt.len() };
        let parts: Vec<&str> = body.splitn(n, ',').collect();
        let at = |name: &str, d: usize| fmt.iter().position(|f| f == name).unwrap_or(d);
        let (s, e, tx) = (at("start", 1), at("end", 2), at("text", n - 1));
        if parts.len() < n {
            continue;
        }
        let text = plain(parts[tx]);
        // drawings and signs ("{\p1}m 0 0 l ...") give no words
        if text.is_empty() || parts[tx].contains("\\p1") {
            continue;
        }
        out.push(Line { start: secs(parts[s]), end: secs(parts[e]), text });
    }
    out.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Every line of a subtitle file (SRT, VTT or ASS), in time order.
pub fn read_lines(file: &str) -> Vec<Line> {
    let path = util::file_url_path(file);
    let Ok(bytes) = std::fs::read(&path) else { return vec![] };
    let text = decode(&bytes);
    if text.contains("[Events]") || text.contains("[events]") {
        return parse_ass(&text);
    }
    let mut v: Vec<Line> = parse_cues(&text)
        .into_iter()
        .map(|c| Line { start: secs(&c.start), end: secs(&c.end), text: plain(&c.text) })
        .filter(|l| !l.text.is_empty() && l.end > l.start)
        .collect();
    v.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    v
}

/// Seconds between two questions for a rate of 0..100: 100 gives one every 2 minutes, 50 one every 7, 1 about one
/// every 12. 0 is off.
pub fn gap_for(rate: u32) -> f64 {
    120.0 + (100.0 - rate.min(100) as f64) * 6.0
}

/// Chooses the questions of an episode. `known[i]`: every word of line i is known; `words[i]`: its number of words;
/// `avoid`: openings and endings (songs make poor questions). The card lines are spread over the episode, one per
/// stretch of `gap_for(rate)` seconds (the line with the most words, up to 8, nearest the middle of the stretch);
/// the parts without subtitles come about every 2.5 gaps (5 minutes at least) and need 3 lines or more, most of them
/// known, the last one known.
pub fn plan(lines: &[Line], known: &[bool], words: &[usize], rate: u32, dur: f64, avoid: &[(f64, f64)]) -> Vec<Mark> {
    if rate == 0 || lines.is_empty() {
        return vec![];
    }
    let gap = gap_for(rate);
    let total = dur.max(lines.last().map(|l| l.end).unwrap_or(0.0));
    let clear = |a: f64, b: f64| !avoid.iter().any(|(x, y)| a < *y && b > *x);
    let good = |i: usize| {
        let l = &lines[i];
        known.get(i).copied().unwrap_or(false) && words.get(i).copied().unwrap_or(0) >= 2 && (0.7..=10.0).contains(&(l.end - l.start)) && clear(l.start, l.end)
    };
    let mut marks: Vec<Mark> = vec![];

    // cards
    let mut w0 = 60.0;
    let mut last = f64::MIN;
    while w0 < total - 30.0 {
        let center = w0 + gap / 2.0;
        let mut best: Option<(f64, usize)> = None;
        for i in 0..lines.len() {
            let l = &lines[i];
            if l.start < w0 || l.start >= w0 + gap || l.end > total - 30.0 || l.start - last < 120.0 || !good(i) {
                continue;
            }
            let score = words[i].min(8) as f64 - ((l.start + l.end) / 2.0 - center).abs() / gap * 4.0;
            if best.map(|(s, _)| score > s).unwrap_or(true) {
                best = Some((score, i));
            }
        }
        if let Some((_, i)) = best {
            marks.push(Mark { t: lines[i].start, end: lines[i].end, kind: "card", line: i, done: false, noticed: false });
            last = lines[i].start;
        }
        w0 += gap;
    }

    // parts without subtitles
    let lgap = (gap * 2.5).max(300.0);
    let mut w0 = 90.0;
    while w0 < total - 60.0 {
        let mut best: Option<(usize, usize, usize)> = None; // (known lines, first line, asked = last line)
        for i in 0..lines.len() {
            if lines[i].start < w0 || lines[i].start >= w0 + lgap {
                continue;
            }
            let mut j = i;
            while j + 1 < lines.len() && lines[j + 1].end - lines[i].start <= 40.0 {
                j += 1;
            }
            // the part ends on its last line that can be asked
            let Some(asked) = (i..=j).rev().find(|&k| good(k)) else { continue };
            let (a, b) = (lines[i].start, lines[asked].end);
            let n = asked + 1 - i;
            let k = (i..=asked).filter(|&x| known.get(x).copied().unwrap_or(false)).count();
            if b - a < 20.0 || n < 3 || (k as f64) < 0.7 * n as f64 || !clear(a, b) || b > total - 20.0 {
                continue;
            }
            // not next to a card question
            if marks.iter().any(|m| m.t < b + 30.0 && m.end > a - 30.0) {
                continue;
            }
            if best.map(|(bk, ..)| k > bk).unwrap_or(true) {
                best = Some((k, i, asked));
            }
        }
        if let Some((_, i, asked)) = best {
            marks.push(Mark { t: (lines[i].start - 0.2).max(0.0), end: lines[asked].end + 0.3, kind: "listen", line: asked, done: false, noticed: false });
        }
        w0 += lgap;
    }
    marks.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    marks
}

/// The text reduced to what is compared: letters and digits only, lower case, full-width Latin as ASCII, katakana
/// as hiragana (so "ネコ" and "ねこ" match), no spaces or punctuation.
pub fn norm_char(c: char) -> Option<char> {
    let c = match c as u32 {
        0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        0x30A1..=0x30F6 => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        _ => c,
    };
    if c.is_alphanumeric() {
        c.to_lowercase().next()
    } else {
        None
    }
}

pub fn norm(s: &str) -> Vec<char> {
    s.chars().filter_map(norm_char).collect()
}

/// 0..1, how close two answers are (1 - edit distance / longer length), on the reduced text.
pub fn similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (norm(a), norm(b));
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    1.0 - prev[b.len()] as f64 / a.len().max(b.len()) as f64
}

/// The answer is right: close enough to the expected text (short answers must match exactly).
pub fn answer_ok(expected: &str, given: &str) -> bool {
    let n = norm(expected).len();
    let s = similarity(expected, given);
    if n <= 4 {
        s >= 0.999
    } else {
        s >= 0.85
    }
}

/// The expected text in pieces marked found (true) or missed (false) in the answer (longest common subsequence on
/// the reduced text; spaces and punctuation count as found).
pub fn diff(expected: &str, given: &str) -> Vec<(String, bool)> {
    let chars: Vec<char> = expected.chars().collect();
    let idx: Vec<usize> = (0..chars.len()).filter(|&i| norm_char(chars[i]).is_some()).collect();
    let a: Vec<char> = idx.iter().map(|&i| norm_char(chars[i]).unwrap_or(' ')).collect();
    let b = norm(given);
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let mut ok = vec![true; chars.len()];
    for &i in &idx {
        ok[i] = false;
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            ok[idx[i]] = true;
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    let mut out: Vec<(String, bool)> = vec![];
    for (c, k) in chars.into_iter().zip(ok) {
        match out.last_mut() {
            Some((s, f)) if *f == k => s.push(c),
            _ => out.push((c.to_string(), k)),
        }
    }
    out
}

/// A small random number generator (no crate needed): the order of the options and the distractors.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

/// Up to `n` items of `pool` (not equal to `not`, no repeats), at random.
fn pick_others(pool: &[String], not: &str, n: usize, rng: &mut Rng) -> Vec<String> {
    let mut v: Vec<String> = vec![];
    for p in pool {
        let t = p.trim();
        if !t.is_empty() && norm(t) != norm(not) && !v.iter().any(|x| norm(x) == norm(t)) {
            v.push(t.to_string());
        }
    }
    rng.shuffle(&mut v);
    v.truncate(n);
    v
}

/// The translation shown at the same time as a line (the cue that overlaps it most).
fn translation_of(tr: &[Line], l: &Line) -> String {
    let mut best: Option<(f64, &Line)> = None;
    for x in tr {
        let o = x.end.min(l.end) - x.start.max(l.start);
        if o > 0.2 && best.map(|(b, _)| o > b).unwrap_or(true) {
            best = Some((o, x));
        }
    }
    best.map(|(_, x)| x.text.clone()).unwrap_or_default()
}

/// The look of mpv's subtitles now (the user's style, the position set in the player), for the mixed line.
fn mix_look(mpv: &Mpv) -> wordmix::Look {
    let d = wordmix::Look::default();
    let num = |names: &[&str], dflt: f64| names.iter().find_map(|n| mpv.get_f64(n)).unwrap_or(dflt);
    let text = |names: &[&str], dflt: &str| names.iter().find_map(|n| mpv.get_str(n).filter(|v| !v.is_empty())).unwrap_or_else(|| dflt.to_string());
    let boxed = mpv.get_str("sub-border-style").map(|v| v == "background-box" || v == "opaque-box").unwrap_or(false);
    wordmix::Look {
        font: text(&["sub-font"], &d.font),
        size: num(&["sub-font-size"], d.size) * num(&["sub-scale"], 1.0),
        bold: mpv.get_flag("sub-bold").unwrap_or(false),
        italic: mpv.get_flag("sub-italic").unwrap_or(false),
        color: text(&["sub-color"], &d.color),
        border_color: text(&["sub-outline-color", "sub-border-color"], &d.border_color),
        border: num(&["sub-outline-size", "sub-border-size"], d.border),
        shadow_color: text(&["sub-shadow-color", "sub-back-color"], &d.shadow_color),
        shadow: num(&["sub-shadow-offset"], d.shadow),
        back: if boxed { Some(text(&["sub-back-color"], "#99000000")) } else { None },
        pos: num(&["sub-pos"], d.pos),
        margin_y: num(&["sub-margin-y"], d.margin_y),
        margin_x: num(&["sub-margin-x"], d.margin_x),
        align: text(&["sub-align-x"], &d.align),
        highlight: d.highlight,
    }
}

impl Core {
    /// Called with the player's tick: reads the subtitle when it changes, plans the questions, and runs them.
    pub(super) fn studyplay_step(&mut self) {
        let Some(mpv) = self.mpv.clone() else { return };
        // the other ways to study while watching, each with its own switch
        self.mining_step(&mpv);
        self.wordmix_step(&mpv);
        // the macaw's quiz on the swapped words at the credits has its own clock
        if self.credits_quiz_step(&mpv) {
            return;
        }
        let g = &self.settings.general;
        let on = g.study_in_player && !g.study_lang.is_empty() && g.study_quiz_rate > 0 && (g.study_quiz_cards || g.study_quiz_listen);
        let playing = self.session.as_ref().map(|s| s.stage == Stage::Playing).unwrap_or(false);
        if !on || !playing {
            if self.study_play.active() {
                self.studyplay_reset(&mpv);
            }
            return;
        }
        let sid = self.session.as_ref().map(|s| s.id).unwrap_or(0);
        if self.study_play.session != sid {
            self.studyplay_reset(&mpv);
            self.study_play.session = sid;
        }
        // which subtitle shows (every 2 s: the track list costs a few calls into mpv)
        if self.study_play.last_track_check.map(|t| t.elapsed().as_secs_f64() >= 2.0).unwrap_or(true) {
            self.study_play.last_track_check = Some(Instant::now());
            self.studyplay_read_track(&mpv);
        }
        if self.study_play.key.is_empty() || self.study_play.key == "-" {
            return;
        }
        let pos = mpv.get_f64("time-pos").unwrap_or(0.0);
        let prev = std::mem::replace(&mut self.study_play.last_pos, pos);
        if self.study_play.notice.as_ref().map(|(_, t)| t.elapsed().as_secs() >= 5).unwrap_or(false) {
            self.study_play.notice = None;
        }

        // a question is on screen: only the replay of its line runs
        if self.study_play.quiz.is_some() {
            if let Some(until) = self.study_play.replay_until {
                if pos >= until || mpv.get_flag("eof-reached") == Some(true) {
                    let _ = mpv.set("pause", "yes");
                    self.study_play.replay_until = None;
                    self.studyplay_show_subs(&mpv);
                }
            } else if mpv.get_flag("pause") == Some(false) {
                // the viewer played on without answering: the question goes away
                self.studyplay_close(&mpv, false);
            }
            return;
        }

        if self.study_play.progressive && self.settings.general.study_quiz_cards {
            self.studyplay_collect(&mpv, pos);
        }

        let seeked = (pos - prev).abs() > 2.0;
        let mut fire: Option<usize> = None;
        let mut shh: Option<u64> = None;
        for (i, m) in self.study_play.marks.iter_mut().enumerate() {
            if m.done {
                continue;
            }
            if m.kind == "listen" {
                if !m.noticed && pos >= m.t - 5.0 && pos < m.t {
                    m.noticed = true;
                    self.study_play.notice = Some(("The next part plays without subtitles".into(), Instant::now()));
                    self.events.push(Event::Session);
                }
                let inside = pos >= m.t && pos < m.end;
                if inside && !self.study_play.hidden && !seeked {
                    let _ = mpv.set("sub-visibility", "no");
                    self.study_play.hidden = true;
                    // the macaw keeps the secret in a corner while the part plays
                    shh = Some(((m.end - pos).max(1.0) * 1000.0) as u64);
                }
            }
            // the line (or the part) just ended while playing on, not jumped over
            if !seeked && prev < m.end && pos >= m.end && pos - m.end < 1.5 && (m.kind == "card" || self.study_play.hidden) {
                m.done = true;
                fire = Some(i);
                break;
            }
        }
        if let Some(ms) = shh {
            if self.settings.general.study_mascot_cues {
                self.study_play.send_cue("shh", ms);
                self.study_play.shh = true;
            }
        }
        // a jump out of a part without subtitles brings them back
        if self.study_play.hidden && fire.is_none() && !self.study_play.marks.iter().any(|m| m.kind == "listen" && !m.done && pos >= m.t && pos < m.end) {
            self.studyplay_show_subs(&mpv);
        }
        if let Some(i) = fire {
            self.studyplay_open(&mpv, i);
        }
    }

    /// Reads the lines of the subtitle on screen when it changed, and plans the questions.
    fn studyplay_read_track(&mut self, mpv: &Mpv) {
        let g = &self.settings.general;
        let study = util::lang_base(&g.study_lang);
        let main = g.known_langs.first().cloned().unwrap_or_else(|| g.ui_lang(&self.system_lang));
        let rate = g.study_quiz_rate;
        let (cards, listen) = (g.study_quiz_cards, g.study_quiz_listen);
        let tracks = mpv.tracks("sub");
        let sid = mpv.get_i64("sid");
        let track = tracks.iter().find(|t| Some(t.id) == sid);
        let key = match track {
            Some(t) if util::lang_base(&t.lang) == study => format!("{}|{}|{}|{}|{}|{}", t.id, t.file, t.lang, rate, cards, listen),
            _ => "-".to_string(),
        };
        if key == self.study_play.key {
            return;
        }
        let sp = &mut self.study_play;
        sp.key = key.clone();
        sp.marks.clear();
        sp.lines.clear();
        sp.translations.clear();
        sp.progressive = false;
        self.events.push(Event::Session);
        let Some(t) = track.filter(|_| key != "-") else { return };
        let (lang3, main3) = (lang3(&study), lang3(&main));
        sp.lang3 = lang3.clone();
        sp.main3 = main3;
        if !(t.external && !t.file.is_empty()) {
            sp.progressive = true;
            return;
        }
        sp.lines = read_lines(&t.file);
        // the translation for the questions: a second subtitle in the main language (shown or not), else a
        // translation made for this episode
        let tr_file = tracks
            .iter()
            .find(|x| x.id != t.id && x.external && !x.file.is_empty() && util::lang_base(&x.lang) == main)
            .map(|x| x.file.clone())
            .or_else(|| {
                let s = self.session.as_ref()?;
                self.forced.iter().find(|f| f.sid == s.sid && util::lang_base(&f.lang) == main).map(|f| f.path.clone())
            });
        let translations = tr_file.map(|f| read_lines(&f)).unwrap_or_default();
        let known: Vec<bool> = self.study_play.lines.iter().map(|l| self.knowledge.line_known(&lang3, &l.text)).collect();
        let words: Vec<usize> = self.study_play.lines.iter().map(|l| self.knowledge.tokens(&lang3, &l.text).len()).collect();
        let dur = mpv.get_f64("duration").unwrap_or(0.0);
        let avoid = self.studyplay_avoid();
        let sp = &mut self.study_play;
        sp.translations = translations;
        sp.marks = plan(&sp.lines, &known, &words, rate, dur, &avoid);
        // only the kinds the user wants
        sp.marks.retain(|m| if m.kind == "card" { cards } else { listen });
        crate::debug::log("study", &format!("player: {} lines, {} known, {} questions", sp.lines.len(), known.iter().filter(|k| **k).count(), sp.marks.len()));
    }

    /// Openings and endings of the episode (no questions there).
    fn studyplay_avoid(&self) -> Vec<(f64, f64)> {
        let Some(s) = self.session.as_ref() else { return vec![] };
        s.segments.iter().filter(|x| matches!(x.kind, Kind::Intro | Kind::Credits)).map(|x| (x.start, x.end.unwrap_or(f64::MAX))).collect()
    }

    /// Subtitles inside the video: each line is kept when it shows, and a known one becomes a question when the last
    /// one is far enough behind.
    fn studyplay_collect(&mut self, mpv: &Mpv, pos: f64) {
        let (Some(a), Some(b)) = (mpv.get_f64("sub-start"), mpv.get_f64("sub-end")) else { return };
        if (a - self.study_play.seen_start).abs() < 0.01 || b <= a {
            return;
        }
        let text = plain(&mpv.get_str("sub-text").unwrap_or_default());
        if text.is_empty() {
            return;
        }
        self.study_play.seen_start = a;
        let lang3 = self.study_play.lang3.clone();
        let gap = gap_for(self.settings.general.study_quiz_rate);
        let known = self.knowledge.line_known(&lang3, &text);
        let words = self.knowledge.tokens(&lang3, &text).len();
        let avoid = self.studyplay_avoid();
        let sp = &mut self.study_play;
        sp.lines.push(Line { start: a, end: b, text });
        let last = sp.marks.iter().filter(|m| m.kind == "card").map(|m| m.t).fold(f64::MIN, f64::max);
        let clear = !avoid.iter().any(|(x, y)| a < *y && b > *x);
        if known && words >= 2 && (0.7..=10.0).contains(&(b - a)) && clear && a >= 45.0 && a - last >= gap && pos < b {
            let line = sp.lines.len() - 1;
            sp.marks.push(Mark { t: a, end: b, kind: "card", line, done: false, noticed: false });
            self.events.push(Event::Session);
        }
    }

    /// Pauses the episode and builds the question of mark `i`.
    fn studyplay_open(&mut self, mpv: &Mpv, i: usize) {
        let Some(m) = self.study_play.marks.get(i).cloned() else { return };
        let Some(line) = self.study_play.lines.get(m.line).cloned() else { return };
        let _ = mpv.set("pause", "yes");
        let sp = &mut self.study_play;
        sp.serial += 1;
        let lang = sp.lang3.clone();
        let main = sp.main3.clone();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (line.start * 1000.0) as u64 ^ crate::util::now_unix().wrapping_mul(2_654_435_761));
        let tokens = self.knowledge.tokens(&lang, &line.text);
        let translation = translation_of(&sp.translations, &line);
        let mut q = Quiz { id: sp.serial, kind: m.kind.to_string(), line: line.clone(), translation: translation.clone(), ..Quiz::default() };
        // a bank of the line's words plus two others of the episode, shuffled
        let make_bank = |rng: &mut Rng, lines: &[Line]| {
            let mut bank = tokens.clone();
            let others: Vec<String> = lines.iter().flat_map(|l| self.knowledge.tokens(&lang, &l.text)).collect();
            bank.extend(pick_others(&others, "", 12, rng).into_iter().filter(|w| !tokens.contains(w)).take(2));
            rng.shuffle(&mut bank);
            bank
        };
        if m.kind == "listen" {
            q.challenge = "type".into();
            q.expected = line.text.clone();
            q.bank = make_bank(&mut rng, &sp.lines);
        } else if let Some(card) = self.knowledge.card_for_line(&line.text) {
            q.challenge = "card".into();
            q.show_text = true;
            q.card = Some(card);
        } else {
            // the kinds this line allows, taken in turn over the episode
            let mut kinds: Vec<&str> = vec![];
            let others_tr: Vec<String> = sp.translations.iter().map(|l| l.text.clone()).collect();
            if !translation.is_empty() && pick_others(&others_tr, &translation, 3, &mut Rng(7)).len() == 3 {
                kinds.push("translate");
            }
            if tokens.len() >= 2 {
                kinds.push("build");
            }
            // a word of the line with a translation, the least known first
            let now = crate::util::now_unix() as i64;
            let mut word: Option<(f64, String, String)> = None;
            for w in &tokens {
                if let Some(r) = self.knowledge.row(&lang, w) {
                    let tr = r.tr(&main).to_string();
                    let st = r.strength(now);
                    if !tr.is_empty() && word.as_ref().map(|(s, ..)| st < *s).unwrap_or(true) {
                        word = Some((st, w.clone(), tr));
                    }
                }
            }
            let word_pool: Vec<String> = self.knowledge.rows(&lang).iter().map(|r| r.tr(&main).to_string()).filter(|t| !t.is_empty()).collect();
            if let Some((_, _, tr)) = &word {
                if pick_others(&word_pool, tr, 3, &mut Rng(7)).len() == 3 {
                    kinds.push("word");
                }
            }
            let pick = kinds.get(sp.serial as usize % kinds.len().max(1)).copied().unwrap_or("build");
            q.challenge = pick.into();
            match pick {
                "translate" => {
                    q.show_text = true;
                    let mut opts = pick_others(&others_tr, &translation, 3, &mut rng);
                    opts.push(translation.clone());
                    rng.shuffle(&mut opts);
                    q.answer_idx = opts.iter().position(|o| *o == translation).unwrap_or(0);
                    q.options = opts;
                    q.expected = translation.clone();
                }
                "word" => {
                    let (_, w, tr) = word.clone().unwrap_or_default();
                    q.show_text = true;
                    q.word = w;
                    let mut opts = pick_others(&word_pool, &tr, 3, &mut rng);
                    opts.push(tr.clone());
                    rng.shuffle(&mut opts);
                    q.answer_idx = opts.iter().position(|o| *o == tr).unwrap_or(0);
                    q.options = opts;
                    q.expected = tr;
                }
                _ => {
                    q.challenge = "build".into();
                    q.expected = line.text.clone();
                    q.bank = make_bank(&mut rng, &sp.lines);
                }
            }
        }
        self.study_play.quiz = Some(q);
        self.study_play.notice = None;
        self.events.push(Event::Session);
    }

    /// Plays the line of the question again (its start to its end), then pauses. The subtitle stays hidden when the
    /// line is the answer.
    pub fn study_quiz_replay(&mut self, id: u64) {
        let Some(mpv) = self.mpv.clone() else { return };
        let Some(q) = self.study_play.quiz.as_ref().filter(|q| q.id == id) else { return };
        let (a, b, show) = (q.line.start, q.line.end, q.show_text && q.result.is_none());
        if !show || q.challenge == "type" || q.challenge == "build" {
            if !self.study_play.hidden {
                let _ = mpv.set("sub-visibility", "no");
                self.study_play.hidden = true;
            }
        }
        let _ = mpv.cmd(&["seek", &format!("{:.2}", (a - 0.15).max(0.0)), "absolute", "exact"]);
        self.study_play.replay_until = Some(b + 0.15);
        self.study_play.last_pos = a;
        let _ = mpv.set("pause", "no");
        self.events.push(Event::Session);
    }

    /// Checks an answer: for choices the index of the option, for "build" / "type" the text, for "card" "right" or
    /// "wrong" (the user grades a flashcard). The words of the line are recorded in the knowledge state.
    pub fn study_quiz_answer(&mut self, id: u64, answer: &str) {
        let credits_lang = lang3(&self.settings.general.study_lang);
        let Some(q) = self.study_play.quiz.as_mut().filter(|q| q.id == id && q.result.is_none()) else { return };
        let lang = if q.kind == "credits" { credits_lang } else { self.study_play.lang3.clone() };
        let (correct, expected) = match q.challenge.as_str() {
            "translate" | "word" => {
                let i = answer.trim().parse::<usize>().ok();
                let ok = i == Some(q.answer_idx) || (i.is_none() && norm(answer) == norm(&q.expected));
                (ok, q.expected.clone())
            }
            "card" => (answer == "right", q.card.as_ref().map(|c| c.back.clone()).unwrap_or_default()),
            _ => {
                // kana answers count for words written in kanji when their readings are known
                let tokens = self.knowledge.tokens(&lang, &q.expected);
                let reading: String = tokens
                    .iter()
                    .map(|t| self.knowledge.row(&lang, t).map(|r| r.reading.clone()).filter(|r| !r.is_empty()).unwrap_or_else(|| t.clone()))
                    .collect::<Vec<_>>()
                    .join(if lang == "jpn" { "" } else { " " });
                let by_text = similarity(&q.expected, answer);
                let by_reading = if lang == "jpn" { similarity(&reading, answer) } else { 0.0 };
                if by_reading > by_text {
                    (answer_ok(&reading, answer), reading)
                } else {
                    (answer_ok(&q.expected, answer), q.expected.clone())
                }
            }
        };
        let d = if matches!(q.challenge.as_str(), "build" | "type") { diff(&expected, answer) } else { vec![] };
        q.result = Some(QuizResult { correct, expected, given: answer.to_string(), diff: d });
        let word = if q.base.is_empty() { q.word.clone() } else { q.base.clone() };
        let text = q.line.text.clone();
        if q.challenge == "word" {
            self.knowledge.record(&lang, &word, correct);
        } else {
            self.knowledge.record_line(&lang, &text, correct);
        }
        self.knowledge.save();
        self.events.push(Event::Session);
    }

    /// Closes the question (skipped, or Continue after the answer) and plays on.
    pub fn study_quiz_skip(&mut self, id: u64) {
        let Some(mpv) = self.mpv.clone() else { return };
        let Some(kind) = self.study_play.quiz.as_ref().filter(|q| q.id == id).map(|q| q.kind.clone()) else { return };
        if kind == "credits" && !self.study_play.swapped.queue.is_empty() {
            self.credits_quiz_next(&mpv);
            return;
        }
        if kind == "credits" && self.settings.general.study_mascot_cues {
            self.study_play.send_cue("cheer", 2500);
        }
        self.studyplay_close(&mpv, true);
    }

    fn studyplay_close(&mut self, mpv: &Mpv, resume: bool) {
        self.study_play.quiz = None;
        self.study_play.replay_until = None;
        self.studyplay_show_subs(mpv);
        // a replay may have moved the clock back: the marks before now are not asked again
        self.study_play.last_pos = mpv.get_f64("time-pos").unwrap_or(0.0);
        if resume {
            let _ = mpv.set("pause", "no");
        }
        self.events.push(Event::Session);
    }

    fn studyplay_show_subs(&mut self, mpv: &Mpv) {
        if self.study_play.hidden {
            let _ = mpv.set("sub-visibility", "yes");
            self.study_play.hidden = false;
        }
        if self.study_play.shh {
            self.study_play.shh = false;
            self.study_play.send_cue("hide", 0);
        }
    }

    /// Forgets the lines and questions (another episode, the option turned off, the player closed).
    fn studyplay_reset(&mut self, mpv: &Mpv) {
        self.studyplay_show_subs(mpv);
        // the word mixing and the cue numbering go on (they have their own switches)
        let mix = std::mem::take(&mut self.study_play.mix);
        let swapped = std::mem::take(&mut self.study_play.swapped);
        let (cue, cue_serial) = (self.study_play.cue, self.study_play.cue_serial);
        // the question serial goes on too: a new question never takes the id of the one the window still shows
        let serial = self.study_play.serial;
        self.study_play = StudyPlay { mix, swapped, cue, cue_serial, serial, ..StudyPlay::default() };
        self.events.push(Event::Session);
    }

    // ---------- the swapped words of the episode, and the macaw's quiz at the credits ----------

    /// Keeps the learned words of a mixed line that just showed.
    fn note_swaps(&mut self, mixed: &str, spans: &[Span], plain_line: &str, at: f64) {
        let sid = self.session.as_ref().map(|s| s.id).unwrap_or(0);
        let study3 = lang3(&self.settings.general.study_lang);
        let sw = &mut self.study_play.swapped;
        if sw.session != sid {
            *sw = Swapped { session: sid, ..Swapped::default() };
        }
        let mut added = false;
        for sp in spans {
            let reading = if !sp.reading.is_empty() || !sp.base.is_empty() {
                sp.reading.clone()
            } else {
                self.knowledge.row(&study3, &sp.word).map(|r| r.reading.clone()).filter(|r| *r != sp.word).unwrap_or_default()
            };
            let seen = SwapSeen {
                word: sp.word.clone(),
                reading,
                original: sp.original.clone(),
                base: sp.base.clone(),
                line: mixed.to_string(),
                plain: plain(plain_line),
                at,
            };
            added |= note_swap(&mut self.study_play.swapped.words, seen);
        }
        if added {
            self.events.push(Event::Session);
        }
    }

    /// Opens the credits quiz when the ending begins (once per episode, only when words were swapped) and keeps it
    /// while it shows. True while a credits question is on screen (the other questions wait).
    fn credits_quiz_step(&mut self, mpv: &Mpv) -> bool {
        let sid = self.session.as_ref().map(|s| s.id).unwrap_or(0);
        let playing = self.session.as_ref().map(|s| s.stage == Stage::Playing).unwrap_or(false);
        if self.study_play.swapped.session != sid {
            self.study_play.swapped = Swapped { session: sid, ..Swapped::default() };
        }
        let showing = self.study_play.quiz.as_ref().map(|q| q.kind == "credits").unwrap_or(false);
        if showing {
            if !playing {
                self.study_play.swapped.queue.clear();
                self.studyplay_close(mpv, false);
                return false;
            }
            // the viewer played on without answering: the questions go away
            if mpv.get_flag("pause") == Some(false) {
                self.study_play.swapped.queue.clear();
                self.studyplay_close(mpv, false);
                return false;
            }
            return true;
        }
        let g = &self.settings.general;
        if !playing || !g.study_in_player || g.study_lang.is_empty() || self.study_play.quiz.is_some() {
            return false;
        }
        let sw = &self.study_play.swapped;
        if sw.asked || sw.words.is_empty() {
            return false;
        }
        let dur = mpv.get_f64("duration").unwrap_or(0.0);
        let segs: Vec<(f64, bool)> = self.session.as_ref().map(|s| s.segments.iter().map(|x| (x.start, matches!(x.kind, Kind::Credits))).collect()).unwrap_or_default();
        let Some(start) = credits_start(&segs, dur) else { return false };
        let pos = mpv.get_f64("time-pos").unwrap_or(0.0);
        // the last swapped word showed before the credits (a jump back to the start does not ask)
        let last_seen = sw.words.iter().map(|w| w.at).fold(0.0, f64::max);
        if pos < start || last_seen > pos + 1.0 {
            return false;
        }
        let study3 = lang3(&g.study_lang);
        let now = crate::util::now_unix() as i64;
        let knowledge = &self.knowledge;
        let picks = credits_pick(&sw.words, |w| knowledge.row(&study3, w).map(|r| r.strength(now)).unwrap_or(0.0), CREDITS_QUIZ_MAX);
        crate::debug::log("study", &format!("credits quiz: {} of {} swapped words, at {:.0} s", picks.len(), sw.words.len(), pos));
        let sw = &mut self.study_play.swapped;
        sw.asked = true;
        sw.total = picks.len();
        sw.queue = picks;
        let _ = mpv.set("pause", "yes");
        self.credits_quiz_next(mpv);
        true
    }

    /// The next word of the credits quiz: the mixed line with the learned word, which word it replaced?
    fn credits_quiz_next(&mut self, mpv: &Mpv) {
        if self.study_play.swapped.queue.is_empty() {
            self.studyplay_close(mpv, true);
            return;
        }
        let w = self.study_play.swapped.queue.remove(0);
        let left = self.study_play.swapped.queue.len();
        let total = self.study_play.swapped.total;
        let _ = mpv.set("pause", "yes");
        let sp = &mut self.study_play;
        sp.serial += 1;
        let mut rng = Rng(0x2545_F491_4F6C_DD1D ^ (w.at * 1000.0) as u64 ^ crate::util::now_unix().wrapping_mul(2_654_435_761));
        let answer = w.original.to_lowercase();
        // the wrong options: the other swapped words of the episode first, then other learned words
        let mut pool: Vec<String> = sp.swapped.words.iter().map(|x| x.original.to_lowercase()).collect();
        if let Some(lex) = &sp.mix.lex {
            pool.extend(lex.keys().into_iter().filter(|k| !k.contains(' ') || k.len() < 16));
        }
        pool.sort();
        pool.dedup();
        let mut opts: Vec<String> = pick_others(&pool, &answer, 3, &mut rng);
        opts.push(answer.clone());
        rng.shuffle(&mut opts);
        let q = Quiz {
            id: sp.serial,
            kind: "credits".into(),
            challenge: "word".into(),
            line: Line { start: w.at, end: w.at, text: w.line.clone() },
            show_text: true,
            answer_idx: opts.iter().position(|o| *o == answer).unwrap_or(0),
            options: opts,
            word: w.word.clone(),
            base: w.base.clone(),
            expected: answer,
            translation: w.plain.clone(),
            ..Quiz::default()
        };
        sp.quiz = Some(q);
        sp.notice = None;
        crate::debug::log("study", &format!("credits quiz: asking {} ({} left of {})", w.word, left, total));
        self.events.push(Event::Session);
    }

    /// `player.studySwaps`: the learned words swapped into the subtitle this episode (the macaw's button).
    pub fn study_swaps_json(&self) -> Json {
        let sid = self.session.as_ref().map(|s| s.id).unwrap_or(0);
        let sw = &self.study_play.swapped;
        if sw.session != sid {
            return Json::Arr(vec![]);
        }
        Json::Arr(
            sw.words
                .iter()
                .map(|w| {
                    Json::obj(vec![
                        ("word", Json::Str(w.word.clone())),
                        ("reading", Json::Str(w.reading.clone())),
                        ("original", Json::Str(w.original.clone())),
                        ("base", Json::Str(w.base.clone())),
                        ("line", Json::Str(w.line.clone())),
                        ("plain", Json::Str(w.plain.clone())),
                        ("at", Json::Num(w.at)),
                    ])
                })
                .collect(),
        )
    }

    /// The macaw's button: the swapped words of the episode as flashcards in "Seu Reels" (once each).
    pub fn study_swaps_to_cards(&mut self) {
        let sid = self.session.as_ref().map(|s| s.id).unwrap_or(0);
        let words = if self.study_play.swapped.session == sid { self.study_play.swapped.words.clone() } else { vec![] };
        let Some(sess) = self.session.as_ref() else { return };
        if words.is_empty() {
            self.toast("No swapped words in this episode yet.");
            return;
        }
        let tag = mining::ep_tag(&sess.sid, &sess.title);
        let source = sess.title.clone();
        let now = Self::now_secs();
        let mut added = 0;
        for w in &words {
            match mining::add_swapped(&mut self.study, &tag, w, &source, now) {
                Ok(true) => added += 1,
                Ok(false) => {}
                Err(e) => {
                    self.toast(&e);
                    return;
                }
            }
        }
        if added > 0 {
            self.save_study();
        }
        crate::debug::log("study", &format!("swapped words: {} cards added of {}", added, words.len()));
        self.toast(if added == 0 { "These words are already in Seu Reels." } else { "Flashcards added to Seu Reels." });
        self.events.push(Event::Study);
    }

    /// The credits quiz: which question this is ("1 of 3"), 0/0 when none.
    pub fn credits_quiz_index(&self) -> (usize, usize) {
        match &self.study_play.quiz {
            Some(q) if q.kind == "credits" => {
                let sw = &self.study_play.swapped;
                (sw.total - sw.queue.len(), sw.total)
            }
            _ => (0, 0),
        }
    }

    // ---------- word mixing (wordmix.rs) ----------

    /// Called with the player's tick: mixes learned words into the subtitle shown when it is in a known language.
    fn wordmix_step(&mut self, mpv: &Mpv) {
        let g = &self.settings.general;
        let playing = self.session.as_ref().map(|s| s.stage == Stage::Playing).unwrap_or(false);
        let on = g.study_in_player && g.study_wordmix && !g.study_lang.is_empty() && g.study_wordmix_rate > 0 && playing;
        if !on {
            if !self.study_play.mix.key.is_empty() {
                self.wordmix_clear(mpv);
                self.study_play.mix = Mix::default();
            }
            return;
        }
        if self.study_play.mix.last_check.map(|t| t.elapsed().as_secs_f64() >= 2.0).unwrap_or(true) {
            self.study_play.mix.last_check = Some(Instant::now());
            self.wordmix_prepare(mpv);
            self.wordmix_orig_step(mpv);
        }
        let mx = &self.study_play.mix;
        if mx.key.is_empty() || mx.key == "-" || mx.lex.as_ref().map(|l| l.is_empty()).unwrap_or(true) {
            return;
        }
        let we_hid = mx.hidden;
        // a part without subtitles, a replay or a question: the subtitle belongs to them
        if self.study_play.hidden || self.study_play.quiz.is_some() {
            self.wordmix_clear(mpv);
            return;
        }
        // the user turned the subtitles off
        if !we_hid && mpv.get_flag("sub-visibility") == Some(false) {
            self.wordmix_clear(mpv);
            return;
        }
        let rate = self.settings.general.study_wordmix_rate;
        let reading = self.settings.general.study_wordmix_reading;
        // the text on screen now, and (a subtitle file) the one about to show
        let pos = mpv.get_f64("time-pos").unwrap_or(0.0) - mpv.get_f64("sub-delay").unwrap_or(0.0);
        // each text with the time it shows (start, end), to find the line heard then
        let (now_text, next_text, ends, now_at, next_at) = if self.study_play.mix.lines.is_empty() {
            let t = mpv.get_str("sub-text").unwrap_or_default();
            let end = mpv.get_f64("sub-end").unwrap_or(pos + 2.0);
            let start = mpv.get_f64("sub-start").unwrap_or(pos);
            (t.clone(), t, end, (start, end), (start, end))
        } else {
            let lines = &self.study_play.mix.lines;
            let at = |t: f64| lines.iter().filter(|l| l.start <= t && t < l.end).map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
            let span = |t: f64| {
                let on: Vec<&Line> = lines.iter().filter(|l| l.start <= t && t < l.end).collect();
                (on.iter().map(|l| l.start).fold(t, f64::min), on.iter().map(|l| l.end).fold(t, f64::max))
            };
            let end = lines.iter().filter(|l| l.start <= pos && pos < l.end).map(|l| l.end).fold(pos, f64::max);
            (at(pos), at(pos + 0.08), end, span(pos), span(pos + 0.08))
        };
        // with the subtitle of the audio, only words said in it are swapped; without one, and the viewer wants
        // that, none are; else the old way (the dictionary form)
        let mx = &self.study_play.mix;
        let heard_mode = mx.audio_ok && !mx.orig.is_empty();
        if !heard_mode && mx.audio_ok && self.settings.general.study_wordmix_confirm {
            self.wordmix_clear(mpv);
            return;
        }
        let now_mix = self.wordmix_of(&now_text, rate, heard_mode.then_some(now_at));
        let next_mix = self.wordmix_of(&next_text, rate, heard_mode.then_some(next_at));
        let hide = !now_mix.1.is_empty() || !next_mix.1.is_empty();
        if !hide {
            self.wordmix_clear(mpv);
            return;
        }
        // mpv's subtitle hidden: the line on screen is drawn here (mixed, or as it is just before a mixed one)
        let look = self.study_play.mix.look.clone();
        let res_x = match (mpv.get_f64("osd-width"), mpv.get_f64("osd-height")) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => 720.0 * w / h,
            _ => 1280.0,
        };
        let ass = if now_text.trim().is_empty() { String::new() } else { wordmix::ass_line(&now_mix.0, &now_mix.1, &look, res_x, reading) };
        let mx = &mut self.study_play.mix;
        if !mx.hidden {
            let _ = mpv.set("sub-visibility", "no");
            mx.hidden = true;
        }
        if ass != mx.drawn {
            let _ = if ass.is_empty() {
                mpv.cmd(&["osd-overlay", MIX_OVERLAY, "none", ""])
            } else {
                mpv.cmd(&["osd-overlay", MIX_OVERLAY, "ass-events", &ass, &format!("{:.0}", res_x), "720", "0", "no", "no"])
            };
            mx.drawn = ass;
        }
        // the macaw peeks at a new mixed line, from the corner, while it shows
        if !now_mix.1.is_empty() && mx.peeked != now_mix.0 {
            mx.peeked = now_mix.0.clone();
            self.note_swaps(&now_mix.0, &now_mix.1, &now_text, pos);
            if self.settings.general.study_mascot_cues {
                let ms = ((ends - pos).clamp(1.0, 8.0) * 1000.0) as u64;
                self.study_play.send_cue("peek", ms);
            }
        }
    }

    /// A text mixed (from the cache): the new text and the learned words in it (none when the line is not picked).
    /// `at`: when the line shows (start, end), to check it against the subtitle of the audio; None = the old way.
    fn wordmix_of(&mut self, text: &str, rate: u32, at: Option<(f64, f64)>) -> (String, Vec<Span>) {
        if text.trim().is_empty() {
            return (text.to_string(), vec![]);
        }
        let said = at.map(|(a, b)| heard_at(&self.study_play.mix.orig, a, b));
        let key = match &said {
            Some(h) => format!("{}\u{1}{}", text, h),
            None => text.to_string(),
        };
        if let Some(r) = self.study_play.mix.cache.get(&key) {
            return r.clone();
        }
        let study3 = lang3(&self.settings.general.study_lang);
        let heard: Option<Vec<Heard>> =
            said.map(|h| self.knowledge.heard_words(&study3, &h).into_iter().map(|(surface, base)| Heard { surface, base }).collect());
        let mx = &mut self.study_play.mix;
        let r = match (&mx.lex, &heard) {
            (Some(lex), Some(h)) if wordmix::picked(text, rate) => wordmix::swap_heard(lex, text, wordmix::max_for(text), h),
            (Some(lex), None) if wordmix::picked(text, rate) => wordmix::swap_line(lex, text, wordmix::max_for(text)),
            _ => (text.to_string(), vec![]),
        };
        if mx.cache.len() > 4000 {
            mx.cache.clear();
        }
        mx.cache.insert(key, r.clone());
        r
    }

    /// Looks for the subtitle in the language of the audio (the study language) to check the swaps against: a
    /// subtitle file loaded in the player, a track built into the video (read out of it in the background), or one
    /// the user's subtitle addons offer (downloaded). Tried every 2 s for a while: the addons answer late.
    pub(super) fn wordmix_orig_step(&mut self, mpv: &Mpv) {
        let study = util::lang_base(&self.settings.general.study_lang);
        let sid = self.session.as_ref().map(|s| s.id).unwrap_or(0);
        let okey = format!("{}|{}", sid, study);
        // the audio playing: the subtitle of the study language matches it only when it is in that language
        let aid = mpv.get_i64("aid");
        let audio_lang = mpv.tracks("audio").into_iter().find(|t| Some(t.id) == aid).map(|t| util::lang_base(&t.lang)).unwrap_or_default();
        let mx = &mut self.study_play.mix;
        if mx.orig_key != okey {
            mx.orig_key = okey.clone();
            mx.orig.clear();
            mx.orig_busy = false;
            mx.orig_tries = 0;
            mx.orig_from.clear();
            mx.orig_told = false;
            mx.cache.clear();
        }
        let ok = audio_lang.is_empty() || audio_lang == study;
        if ok != mx.audio_ok {
            mx.audio_ok = ok;
            mx.cache.clear();
        }
        if !mx.orig.is_empty() || mx.orig_busy || study.is_empty() {
            return;
        }
        if mx.orig_tries >= ORIG_TRIES {
            // only word mixing tells it (mining also looks for this subtitle, for the Japanese line of its cards)
            let g = &self.settings.general;
            if mx.audio_ok && !mx.orig_told && g.study_wordmix_confirm && g.study_wordmix && g.study_wordmix_rate > 0 {
                mx.orig_told = true;
                crate::debug::log("study", "word mixing: no subtitle in the language of the audio, no words swapped");
                self.toast("No subtitle in the language of the audio: words are swapped only when one is found (Preferences > Study).");
            }
            return;
        }
        mx.orig_tries += 1;
        let tracks: Vec<crate::mpv::Track> = mpv.tracks("sub").into_iter().filter(|t| util::lang_base(&t.lang) == study).collect();
        // a subtitle file loaded in the player
        for t in tracks.iter().filter(|t| t.external && !t.file.is_empty()) {
            let lines = read_lines(&t.file);
            if !lines.is_empty() {
                self.on_orig_lines(&okey, lines, &format!("file {}", t.file));
                return;
            }
        }
        let cache = self.data_dir.join("subtitles");
        // a text track built into the video
        if let Some(e) = tracks.iter().filter(|t| !t.external).find_map(|t| self.embedded_at(t.id).filter(|e| !crate::mkvsubs::is_picture_codec(&e.codec))) {
            self.study_play.mix.orig_busy = true;
            let key = okey.clone();
            self.spawn(move |tx, _| {
                let cancel = AtomicBool::new(false);
                let r = super::controller_sub::read_embedded(&e, &cache, &cancel, &|_| {});
                send(tx, Msg::OrigSubs(key, "built-in track".into(), r));
            });
            return;
        }
        // one of the subtitles the addons offer
        let cand = self.session.as_ref().and_then(|s| s.sub_cands.iter().find(|c| !c.url.is_empty() && util::lang_base(&c.lang) == study).cloned());
        if let Some(c) = cand {
            self.study_play.mix.orig_busy = true;
            let key = okey.clone();
            self.spawn(move |tx, http| {
                let r = subtitles::fetch_srt(&*http, &c.url, &cache);
                send(tx, Msg::OrigSubs(key, "addon".into(), r));
            });
        }
    }

    fn on_orig_lines(&mut self, key: &str, lines: Vec<Line>, from: &str) {
        let mx = &mut self.study_play.mix;
        if mx.orig_key != key {
            return;
        }
        crate::debug::log("study", &format!("word mixing: {} lines of the audio's subtitle ({}), words checked against them", lines.len(), from));
        mx.orig = lines;
        mx.orig_from = from.to_string();
        mx.orig_busy = false;
        mx.cache.clear();
    }

    /// The subtitle of the audio read out of the video or downloaded (`wordmix_orig_step`).
    pub(super) fn on_orig_subs(&mut self, key: String, from: String, r: Result<std::path::PathBuf, String>) {
        let lines = match r {
            Ok(p) => read_lines(&p.to_string_lossy()),
            Err(e) => {
                crate::debug::log("study", &format!("word mixing: the audio's subtitle ({}) could not be read: {}", from, e));
                vec![]
            }
        };
        if lines.is_empty() {
            if self.study_play.mix.orig_key == key {
                self.study_play.mix.orig_busy = false;
                // that source gives nothing: stop trying it
                self.study_play.mix.orig_tries = ORIG_TRIES;
            }
            return;
        }
        self.on_orig_lines(&key, lines, &from);
    }

    /// Checks the subtitle shown: in a known language (not the study one) it can be mixed; then the learned words,
    /// the lines of the file and the look of the subtitles are read.
    fn wordmix_prepare(&mut self, mpv: &Mpv) {
        let g = &self.settings.general;
        let study = util::lang_base(&g.study_lang);
        let known: Vec<String> = g.known_langs.iter().map(|l| util::lang_base(l)).collect();
        let rate = g.study_wordmix_rate;
        let sid = mpv.get_i64("sid");
        let track = mpv.tracks("sub").into_iter().find(|t| Some(t.id) == sid);
        let key = match &track {
            Some(t) if !util::lang_base(&t.lang).is_empty() && util::lang_base(&t.lang) != study && known.contains(&util::lang_base(&t.lang)) => {
                // the session too: a new episode loads with mpv's subtitle shown again
                format!("{}|{}|{}|{}|{}|{}", self.session.as_ref().map(|s| s.id).unwrap_or(0), t.id, t.file, t.lang, rate, g.study_wordmix_reading)
            }
            _ => "-".to_string(),
        };
        let mx = &self.study_play.mix;
        // the learned words change slowly (quizzes, lessons): read them again every few minutes
        let stale = mx.built.map(|t| t.elapsed().as_secs() >= 300).unwrap_or(true);
        if key == mx.key && !stale {
            self.study_play.mix.look = mix_look(mpv);
            return;
        }
        if key != mx.key {
            self.wordmix_clear(mpv);
        }
        let mx = &mut self.study_play.mix;
        mx.key = key.clone();
        mx.cache.clear();
        mx.lines.clear();
        mx.lex = None;
        let Some(t) = track.filter(|_| key != "-") else { return };
        let (study3, from3) = (lang3(&study), lang3(&t.lang));
        let lex = Lexicon::build(&self.knowledge, self.learn.pack.as_deref(), &study3, &from3, wordmix::MIX_MIN_STRENGTH, false, Self::now_secs());
        crate::debug::log("study", &format!("word mixing: {} learned words for {} subtitles", lex.len(), from3));
        let lines = if t.external && !t.file.is_empty() { read_lines(&t.file) } else { vec![] };
        let mx = &mut self.study_play.mix;
        mx.lex = Some(lex);
        mx.built = Some(Instant::now());
        mx.lines = lines;
        mx.look = mix_look(mpv);
    }

    /// Takes the mixed line away and gives mpv its subtitle back.
    fn wordmix_clear(&mut self, mpv: &Mpv) {
        let mx = &mut self.study_play.mix;
        if !mx.drawn.is_empty() {
            let _ = mpv.cmd(&["osd-overlay", MIX_OVERLAY, "none", ""]);
            mx.drawn.clear();
        }
        if mx.hidden {
            mx.hidden = false;
            // a part without subtitles keeps them hidden
            if !self.study_play.hidden {
                let _ = mpv.set("sub-visibility", "yes");
            }
        }
        mx.peeked.clear();
    }

    /// Mixes learned words into a text in a known language (`from_lang`, 2- or 3-letter code), for the readers
    /// (Text and Images modes): the text with one or two learned words, and where they are. Nothing changes when
    /// the language is the study language or nothing was learned yet. The table of learned words is kept until the
    /// language changes or `wordmix_forget` is called.
    pub fn wordmix_swap(&mut self, text: &str, from_lang: &str) -> (String, Vec<Span>) {
        let study3 = lang3(&self.settings.general.study_lang);
        let from3 = lang3(from_lang);
        if study3.is_empty() || from3.is_empty() || study3 == from3 {
            return (text.to_string(), vec![]);
        }
        let fresh = self.wordmix_readers.as_ref().map(|l| l.from == from3 && l.to == study3).unwrap_or(false);
        if !fresh {
            self.wordmix_readers = Some(Lexicon::build(&self.knowledge, self.learn.pack.as_deref(), &study3, &from3, wordmix::MIX_MIN_STRENGTH, false, Self::now_secs()));
        }
        match &self.wordmix_readers {
            Some(lex) => wordmix::swap_line(lex, text, wordmix::max_for(text)),
            None => (text.to_string(), vec![]),
        }
    }

    /// Forgets the readers' table of learned words (after a lesson or a review changed them).
    pub fn wordmix_forget(&mut self) {
        self.wordmix_readers = None;
    }

    /// `player.studyMarks`: where the questions are, for the seek bar.
    pub fn study_marks_json(&self) -> Json {
        Json::Arr(
            self.study_play
                .marks
                .iter()
                .map(|m| Json::obj(vec![("t", Json::Num(m.t)), ("end", Json::Num(m.end)), ("kind", Json::Str(m.kind.into())), ("done", Json::Bool(m.done))]))
                .collect(),
        )
    }

    /// `player.studyNotice`: the notice before a part without subtitles ("" when none).
    pub fn study_notice(&self) -> String {
        self.study_play.notice.as_ref().map(|(t, _)| t.clone()).unwrap_or_default()
    }

    /// `player.studyQuiz`: the question on screen (null when none).
    pub fn study_quiz_json(&self) -> Json {
        let Some(q) = &self.study_play.quiz else { return Json::Null };
        let strs = |v: &[String]| Json::Arr(v.iter().map(|s| Json::Str(s.clone())).collect());
        let result = match &q.result {
            Some(r) => Json::obj(vec![
                ("correct", Json::Bool(r.correct)),
                ("expected", Json::Str(r.expected.clone())),
                ("given", Json::Str(r.given.clone())),
                ("diff", Json::Arr(r.diff.iter().map(|(t, ok)| Json::obj(vec![("t", Json::Str(t.clone())), ("ok", Json::Bool(*ok))])).collect())),
            ]),
            None => Json::Null,
        };
        let lang = &self.study_play.lang3;
        Json::obj(vec![
            ("id", Json::Num(q.id as f64)),
            ("kind", Json::Str(q.kind.clone())),
            ("challenge", Json::Str(q.challenge.clone())),
            // the line itself only when it is not the answer (or once answered)
            ("text", Json::Str(if q.show_text || q.result.is_some() { q.line.text.clone() } else { String::new() })),
            ("line", Json::Str(if q.result.is_some() { q.line.text.clone() } else { String::new() })),
            ("translation", Json::Str(if q.result.is_some() || q.challenge == "card" { q.translation.clone() } else { String::new() })),
            ("word", Json::Str(q.word.clone())),
            ("options", strs(&q.options)),
            ("answerIdx", Json::Num(if q.result.is_some() { q.answer_idx as f64 } else { -1.0 })),
            ("bank", strs(&q.bank)),
            ("joiner", Json::Str(if lang == "jpn" || lang == "cmn" { String::new() } else { " ".into() })),
            ("cardFront", Json::Str(q.card.as_ref().map(|c| crate::knowledge::strip_tags(&c.front)).unwrap_or_default())),
            ("cardBack", Json::Str(q.card.as_ref().map(|c| crate::knowledge::strip_tags(&c.back)).unwrap_or_default())),
            ("cardDeck", Json::Str(q.card.as_ref().map(|c| c.deck.clone()).unwrap_or_default())),
            ("replaying", Json::Bool(self.study_play.replay_until.is_some())),
            // the macaw brings the card in and takes it away (StudyQuiz.qml)
            // the credits quiz is the macaw's own: it always brings it
            ("mascot", Json::Bool(self.settings.general.study_mascot_cues || q.kind == "credits")),
            ("step", Json::Num(self.credits_quiz_index().0 as f64)),
            ("steps", Json::Num(self.credits_quiz_index().1 as f64)),
            ("result", result),
        ])
    }

    /// `player.studyCue`: the macaw's last cue ({n, cue, ms}; null when none). The window calls `Core.mascotCue`
    /// when `n` changes.
    pub fn study_cue_json(&self) -> Json {
        match self.study_play.cue {
            Some((n, cue, ms)) => Json::obj(vec![("n", Json::Num(n as f64)), ("cue", Json::Str(cue.into())), ("ms", Json::Num(ms as f64))]),
            None => Json::Null,
        }
    }

    /// A question is on screen (the X-Ray does not open on its pause).
    pub fn study_quiz_open(&self) -> bool {
        self.study_play.quiz.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ln(a: f64, b: f64, t: &str) -> Line {
        Line { start: a, end: b, text: t.into() }
    }

    #[test]
    fn lines_are_read_from_srt_and_ass() {
        let ass = "[Script Info]\nTitle: x\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:03.50,0:00:05.00,Default,,0,0,0,,{\\an8}行こう、\\Nみんな\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,はい, そうです\nDialogue: 0,0:00:06.00,0:00:07.00,Sign,,0,0,0,,{\\p1}m 0 0 l 1 1{\\p0}\n";
        let v = parse_ass(ass);
        assert_eq!(v, vec![ln(1.0, 2.0, "はい, そうです"), ln(3.5, 5.0, "行こう、 みんな")]);
        assert_eq!(plain("<i>Hello</i>\nthere"), "Hello there");
        assert!((secs("00:01:02,500") - 62.5).abs() < 1e-9);
    }

    #[test]
    fn questions_are_spread_and_spaced() {
        // a line every 5 s for 24 minutes, all known, 3 words each
        let lines: Vec<Line> = (0..288).map(|i| ln(i as f64 * 5.0, i as f64 * 5.0 + 2.5, "a b c")).collect();
        let known = vec![true; lines.len()];
        let words = vec![3; lines.len()];
        let marks = plan(&lines, &known, &words, 100, 1440.0, &[(0.0, 90.0)]);
        let cards: Vec<&Mark> = marks.iter().filter(|m| m.kind == "card").collect();
        assert!(cards.len() >= 9 && cards.len() <= 12, "{} cards", cards.len());
        for w in cards.windows(2) {
            assert!(w[1].t - w[0].t >= 120.0);
        }
        // nothing inside the opening
        assert!(marks.iter().all(|m| m.t >= 90.0));
        let listens: Vec<&Mark> = marks.iter().filter(|m| m.kind == "listen").collect();
        assert!(!listens.is_empty());
        for l in &listens {
            assert!(l.end - l.t >= 20.0 && l.end - l.t <= 41.0);
            assert!(cards.iter().all(|c| c.t >= l.end + 30.0 || c.end <= l.t - 30.0));
        }
        // a lower rate asks less often; rate 0 never
        assert!(plan(&lines, &known, &words, 30, 1440.0, &[]).iter().filter(|m| m.kind == "card").count() < cards.len());
        assert!(plan(&lines, &known, &words, 0, 1440.0, &[]).is_empty());
        // unknown lines are never asked
        assert!(plan(&lines, &vec![false; lines.len()], &words, 100, 1440.0, &[]).is_empty());
    }

    #[test]
    fn answers_are_compared_tolerantly() {
        assert!(answer_ok("Where are you going?", "where are you going"));
        assert!(answer_ok("Where are you going?", "Where are yuo going"));
        assert!(!answer_ok("Where are you going?", "who is going"));
        // katakana typed as hiragana (kanji typed as kana is checked against the readings, in study_quiz_answer)
        assert!(answer_ok("ネコが好き。", "ねこが好き"));
        assert!(!answer_ok("ネコが好き。", "いぬが好き"));
        assert!(answer_ok("ＡＢＣ", "abc"));
        assert!(!answer_ok("はい", "はいい"));
        let d = diff("I am a cat.", "I am cat");
        let missed: String = d.iter().filter(|(_, ok)| !ok).map(|(t, _)| t.clone()).collect();
        assert_eq!(missed, "a");
        assert_eq!(d.iter().map(|(t, _)| t.as_str()).collect::<String>(), "I am a cat.");
    }

    #[test]
    fn translations_match_by_time() {
        let tr = vec![ln(0.0, 2.0, "Hello"), ln(2.0, 6.0, "Let's go")];
        assert_eq!(translation_of(&tr, &ln(2.5, 5.5, "行こう")), "Let's go");
        assert_eq!(translation_of(&tr, &ln(10.0, 11.0, "x")), "");
        assert_eq!(lang3("ja"), "jpn");
        assert_eq!(lang3("pt-BR"), "por");
    }

    fn seen(word: &str, original: &str, at: f64) -> SwapSeen {
        SwapSeen { word: word.into(), original: original.into(), line: format!("Eu quero {} agora", word), plain: format!("Eu quero {} agora", original), at, ..SwapSeen::default() }
    }

    #[test]
    fn swapped_words_are_kept_once() {
        let mut v = vec![];
        assert!(note_swap(&mut v, seen("食べる", "comer", 10.0)));
        assert!(!note_swap(&mut v, seen("食べる", "comer", 50.0)));
        assert!(note_swap(&mut v, seen("猫", "gato", 60.0)));
        assert!(!note_swap(&mut v, seen(" ", "x", 70.0)));
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].at, 10.0);
    }

    #[test]
    fn credits_begin_at_the_ending_or_near_the_end() {
        // the ending of the skip data
        assert_eq!(credits_start(&[(80.0, false), (1300.0, true)], 1420.0), Some(1300.0));
        // none: near the end of a long video (5 %, between 1 and 5 minutes)
        assert_eq!(credits_start(&[(80.0, false)], 1420.0), Some(1420.0 - 71.0));
        assert_eq!(credits_start(&[], 7200.0), Some(6900.0));
        assert_eq!(credits_start(&[], 600.0), Some(540.0));
        // a short clip has no credits
        assert_eq!(credits_start(&[], 120.0), None);
    }

    #[test]
    fn the_credits_quiz_asks_the_least_known_words() {
        let words = vec![seen("a", "1", 1.0), seen("b", "2", 2.0), seen("c", "3", 3.0), seen("d", "4", 4.0)];
        let st = |w: &str| match w { "a" => 0.9, "b" => 0.2, "c" => 0.5, _ => 0.2 };
        let got: Vec<String> = credits_pick(&words, st, 3).into_iter().map(|w| w.word).collect();
        assert_eq!(got, vec!["b", "d", "c"]);
        assert_eq!(credits_pick(&words, st, 10).len(), 4);
    }

    #[test]
    fn line_heard_is_the_one_said_at_the_same_time() {
        let l = |a: f64, b: f64, t: &str| Line { start: a, end: b, text: t.into() };
        let orig = vec![l(10.0, 12.0, "猫があった"), l(12.1, 14.0, "食べた"), l(20.0, 22.0, "遠い")];
        assert_eq!(heard_at(&orig, 10.2, 12.3), "猫があった");
        // a line cut in two by the other subtitle: both halves
        assert_eq!(heard_at(&orig, 10.0, 14.0), "猫があった\n食べた");
        assert_eq!(heard_at(&orig, 15.0, 18.0), "");
    }
}
