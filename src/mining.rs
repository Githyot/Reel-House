//! Mining while watching, two ways (see `plan`):
//! - **Sentences** (like asbplayer / mpvacious / Migaku): the subtitle is in the language being learned (Japanese
//!   subtitles on a Japanese anime). Each line with something new to learn (a word not known yet, fewest unknown
//!   words first) becomes a card with its audio clip (the line ± 0.3 s) and a picture of the video; the back shows the
//!   unknown words and, when a second subtitle in a known language is shown (mpv's secondary subtitle), its text.
//! - **Single words**: the subtitle is in a language the user knows. Moments where a character says one word, with a
//!   silence before and after it ("Obrigado!" alone on screen, 1.5 s of nothing around it), make listening cards.
//!
//! Cards go to the app's deck "Seu Reels". A subtitle file is read whole when the episode starts; a subtitle inside the
//! video (embedded track) is collected line by line from mpv's `sub-text` while it plays. A track without a language
//! tag is told by its text (kana = Japanese), and an audio track without a tag is accepted (most fansub releases have
//! none); only an audio track tagged in another language (a dub) turns mining off.
//! Single-word cards show the line of the audio's own subtitle (Japanese) with the clip; the known-language word
//! comes only with the answer. Its lines are the ones word mixing looks for (`wordmix_orig_step`): a cut waits for
//! that search a little.
//! At most `general.study_mining_max` cards per episode, never a word / line already in Seu Reels (from any episode), one clip
//! cut at a time in the background; a banner with the macaw says how many were made. Every decision writes a
//! `[study] mining:` line to the debug log.
//! Part of `controller` (it needs the private parts of `Core`).
use super::studyplay::{read_lines, Line};
use super::wordmix::{self, Lexicon};
use super::*;
use crate::animeskip::Kind;

/// Seconds of silence (no subtitle) needed before and after the word.
pub const GAP: f64 = 1.5;
/// Seconds of video kept around the line in the clip.
pub const PAD: f64 = 0.3;

/// The note type of the mined cards: the clip on the front; the word of the subtitle, the learned word and its
/// reading on the back.
pub const MINED: i64 = 1_000_000_006;
/// The note type of mined sentences: the picture, the line and its clip on the front; the unknown words and the
/// translation (the second subtitle) on the back.
pub const MINED_SENTENCE: i64 = 1_000_000_007;
/// The cutting waits this long after the episode starts, so it does not compete with the start of the video.
const START_WAIT_SECS: u64 = 8;
/// A single word waits at most this long (since the episode started) for the audio's subtitle to be found.
const ORIG_WAIT_SECS: u64 = 60;

/// What is mined from the subtitle on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// not decided yet (a subtitle inside the video without a language tag: its first lines tell)
    #[default]
    Undecided,
    /// single words between silences (a subtitle in a known language)
    Word,
    /// whole lines (a subtitle in the language studied)
    Sentence,
}

/// Why `plan` cannot decide from the tags alone: the subtitle has no language tag.
pub const UNTAGGED: &str = "the subtitle has no language tag";

/// A moment to cut: the line and its word.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub start: f64,
    pub end: f64,
    /// The word (or two short words) as the subtitle writes it, without the punctuation around it.
    pub word: String,
    pub line: String,
    /// sentences: the line of the second subtitle (a known language) shown at the same time, if any
    pub translation: String,
    pub mode: Mode,
}

/// A clip that was cut, back from the background job.
#[derive(Clone, Debug)]
pub struct Mined {
    pub cand: Candidate,
    /// single words: the line of the audio's subtitle (the study language) said at that moment, "" when none
    pub original: String,
    pub target: String,
    pub reading: String,
    pub video: String,
    pub picture: Option<String>,
    pub source: String,
    pub tag: String,
}

/// The state of mining in the episode playing.
#[derive(Default)]
pub struct MiningState {
    /// The session these belong to.
    session: u64,
    /// The tracks checked ("" = not yet, "-" = mining does not apply to them).
    key: String,
    last_check: Option<Instant>,
    /// The subtitle is inside the video: lines are collected while they show.
    progressive: bool,
    lines: Vec<Line>,
    /// Progressive: the second subtitle's text for each line of `lines` (sentences), "" when none.
    trans: Vec<String>,
    mode: Mode,
    /// The language of the study as the knowledge file writes it ("jpn").
    study3: String,
    /// The session whose "mining is off" reason was logged already.
    off_logged: u64,
    /// Progressive: how many lines were already looked at, and the start of the last line seen.
    looked: usize,
    seen_start: f64,
    last_pos: f64,
    queue: VecDeque<Candidate>,
    busy: bool,
    /// Cards made, and moments taken (made + queued + cutting), this episode.
    made: u32,
    taken: u32,
    words: HashSet<String>,
    /// The tag of the episode on its cards (so a word is not mined twice from the same episode).
    tag: String,
    lexicon: Option<Lexicon>,
    /// Sentences from a subtitle file: the lines of a subtitle file in a known language loaded too (the translation on
    /// the back of the card).
    known_lines: Vec<Line>,
    announced: bool,
    /// When the episode started playing: the cutting waits a little so it does not compete with the start.
    playing_since: Option<Instant>,
}

/// What mining does with these languages (two-letter or three-letter codes, or "" / "und" when the track has no
/// tag): `Sentence` when the subtitle is in the study language, `Word` when it is in a known language (any language
/// but the study one when no known language was set). An audio track without a tag is accepted; one tagged in another
/// language (a dub) is not. Err(`UNTAGGED`): the subtitle's own text must tell (`guess_lang`).
pub fn plan(audio: &str, sub: &str, study: &str, known: &[String]) -> Result<Mode, &'static str> {
    let (a, s, st) = (util::lang_base(audio), util::lang_base(sub), util::lang_base(study));
    if st.is_empty() {
        return Err("no study language (Study > languages)");
    }
    if !a.is_empty() && a != "und" && a != "mul" && a != st {
        return Err("the audio track is in another language (a dub?)");
    }
    if s.is_empty() || s == "und" {
        return Err(UNTAGGED);
    }
    if s == st {
        return Ok(Mode::Sentence);
    }
    if known.is_empty() || known.iter().any(|k| util::lang_base(k) == s) {
        return Ok(Mode::Word);
    }
    Err("the subtitle is neither in the study language nor in a known one")
}

/// The language of subtitle lines without a tag, from their letters: kana = Japanese, hangul = Korean, Chinese
/// characters alone = Chinese; Latin text is "" (cannot tell which), unless the study language is written otherwise,
/// in which case it is surely not that one ("xx").
pub fn guess_lang<'a>(texts: impl Iterator<Item = &'a str>, study: &str) -> String {
    let (mut n, mut kana, mut hangul, mut han) = (0, 0, 0, 0);
    for t in texts.take(60) {
        if t.trim().is_empty() {
            continue;
        }
        n += 1;
        if t.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF)) {
            kana += 1;
        } else if t.chars().any(|c| matches!(c as u32, 0xAC00..=0xD7AF)) {
            hangul += 1;
        } else if t.chars().any(|c| matches!(c as u32, 0x4E00..=0x9FFF)) {
            han += 1;
        }
    }
    if n == 0 {
        return String::new();
    }
    let most = |k: i32| k * 10 >= n * 3;
    if most(kana) {
        return "ja".into();
    }
    if most(hangul) {
        return "ko".into();
    }
    if most(han) {
        return "zh".into();
    }
    if matches!(util::lang_base(study).as_str(), "ja" | "ko" | "zh" | "yue") {
        return "xx".into();
    }
    String::new()
}

/// A line worth a sentence card: real speech (no sound note, no song), not too short or too long.
pub fn sentence_line(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.starts_with(['(', '[', '（', '【', '<', '{']) || t.contains(['♪', '♫']) {
        return false;
    }
    if t.chars().any(crate::knowledge::is_cjk) {
        let n = t.chars().filter(|c| c.is_alphanumeric()).count();
        (3..=40).contains(&n)
    } else {
        let n = wordmix::words(t).len();
        (3..=20).contains(&n)
    }
}

/// A line as a sentence moment (outside `avoid`), with the second subtitle's text.
fn sentence_moment(l: &Line, translation: &str, avoid: &[(f64, f64)]) -> Option<Candidate> {
    let dur = l.end - l.start;
    if !(0.4..=12.0).contains(&dur) || avoid.iter().any(|(a, b)| l.start < *b && l.end > *a) || !sentence_line(&l.text) {
        return None;
    }
    let text = l.text.trim().to_string();
    Some(Candidate { start: l.start, end: l.end, word: text.clone(), line: text, translation: translation.trim().to_string(), mode: Mode::Sentence })
}

/// The sentence moments of a subtitle, each line once, in time order (the caller orders them by what is new).
pub fn select_sentences(lines: &[Line], avoid: &[(f64, f64)]) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = vec![];
    for l in lines {
        if let Some(c) = sentence_moment(l, "", avoid) {
            if !out.iter().any(|o| o.word == c.word) {
                out.push(c);
            }
        }
    }
    out
}

/// The word of a line that says one word (or two short ones, "Muito bem!"), without the punctuation around it.
/// Sound notes ("[risos]", "(suspira)") and songs ("♪ ...") are not words.
pub fn isolated_word(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.starts_with(['(', '[', '（', '【', '<', '{']) || t.contains(['♪', '♫']) {
        return None;
    }
    let cjk = t.chars().any(crate::knowledge::is_cjk);
    let words = wordmix::words(t);
    let ok = if cjk {
        // no spaces to count words: a very short line
        let n: usize = t.chars().filter(|c| c.is_alphanumeric()).count();
        (1..=4).contains(&n)
    } else {
        let chars: usize = words.iter().map(|w| w.chars().count()).sum();
        (words.len() == 1 || (words.len() == 2 && chars <= 10)) && chars >= 2
    };
    if !ok || !words.iter().all(|w| w.chars().any(char::is_alphabetic)) {
        return None;
    }
    Some(if cjk { t.trim_matches(|c: char| !c.is_alphanumeric()).to_string() } else { words.join(" ") })
}

/// A line is a moment to mine: one word, `gap` seconds without subtitles before it (since `prev_end`) and after it
/// (until `next_start`), a sensible length, outside `avoid` (openings, endings).
fn moment(l: &Line, prev_end: f64, next_start: f64, gap: f64, avoid: &[(f64, f64)]) -> Option<Candidate> {
    let dur = l.end - l.start;
    if !(0.25..=4.0).contains(&dur) || avoid.iter().any(|(a, b)| l.start < *b && l.end > *a) {
        return None;
    }
    if l.start - prev_end < gap || next_start - l.end < gap {
        return None;
    }
    let word = isolated_word(&l.text)?;
    Some(Candidate { start: l.start, end: l.end, word, line: l.text.clone(), translation: String::new(), mode: Mode::Word })
}

/// The lines of a subtitle (in time order) that are one word with `gap` seconds without subtitles before and after,
/// outside `avoid` (openings, endings), each word once (the first time). Any line overlapping counts as a subtitle
/// around it.
pub fn select(lines: &[Line], gap: f64, avoid: &[(f64, f64)]) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = vec![];
    for (i, l) in lines.iter().enumerate() {
        let before = lines[..i].iter().map(|x| x.end).fold(0.0, f64::max);
        let after = lines[i + 1..].iter().map(|x| x.start).fold(f64::MAX, f64::min);
        let Some(c) = moment(l, before, after, gap, avoid) else { continue };
        if !out.iter().any(|o| o.word.to_lowercase() == c.word.to_lowercase()) {
            out.push(c);
        }
    }
    out
}

/// A short stable name for the episode (its tag on the cards).
pub fn ep_tag(sid: &str, title: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in sid.bytes().chain(b"|".iter().copied()).chain(title.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("ep_{:012x}", h & 0xFFFF_FFFF_FFFF)
}

/// The front of a mined word card: the picture, the line of the audio's subtitle (Japanese) and the clip.
const MINED_Q: &str = "{{Image}}<br>{{#Original}}<big><big>{{Original}}</big></big><br>{{/Original}}{{Clip}}";
/// The front before 2026-11 (no Japanese line), changed to `MINED_Q` when found.
const MINED_Q_OLD: &str = "{{Image}}<br>{{Clip}}";
/// The back: the word of the known language (the translation), the learned word and its reading.
const MINED_A: &str = "{{FrontSide}}<hr id=answer><big>{{Word}}</big>{{#Target}}<br><big><big>{{Target}}</big></big>{{/Target}}{{#Reading}} <small>{{Reading}}</small>{{/Reading}}<br><small>{{Source}}</small>";

/// Adds the note type of mined cards when it is missing, and gives an older one the "Original" field (the Japanese
/// line on the front). True when something changed.
pub fn ensure_mined(col: &mut crate::study::Collection) -> bool {
    if let Some(m) = col.models.iter_mut().find(|m| m.id == MINED) {
        if m.fields.iter().any(|f| f == "Original") {
            return false;
        }
        m.fields.push("Original".into());
        for t in m.tmpls.iter_mut().filter(|t| t.q == MINED_Q_OLD) {
            t.q = MINED_Q.into();
        }
        return true;
    }
    let fields = ["Word", "Target", "Reading", "Clip", "Image", "Source", "Line", "Original"];
    col.models.push(crate::study::Model {
        id: MINED,
        name: "Reelhouse mined word".into(),
        cloze: false,
        fields: fields.iter().map(|f| f.to_string()).collect(),
        tmpls: vec![crate::study::Tmpl { name: "Listen".into(), q: MINED_Q.into(), a: MINED_A.into() }],
    });
    true
}

/// A word or line as a key to compare cards: lowercase, without the punctuation and spaces around it.
pub fn reel_key(s: &str) -> String {
    let plain = crate::study::strip_html(s);
    plain.trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// What a card of Seu Reels teaches, as keys (`reel_key`): a mined word its learned word (else the Japanese line
/// heard, else the subtitle's word), a swapped word the word put in, a mined sentence its line. Two cards with a key
/// in common teach the same thing.
pub fn reel_keys(n: &crate::study::Note) -> Vec<String> {
    let f = |i: usize| n.fields.get(i).map(|s| reel_key(s)).unwrap_or_default();
    let keys = if n.mid == MINED {
        let (target, original) = (f(1), f(7));
        if target.is_empty() && original.is_empty() {
            vec![f(0)]
        } else {
            vec![target, original]
        }
    } else if n.mid == MINED_SENTENCE && n.tags.split_whitespace().any(|t| t == "swapped") {
        // "食べる (たべる) = comer [ある]": the word before the reading / the "="
        let w = n.fields.get(2).map(|s| s.as_str()).unwrap_or("");
        let w = w.split(" = ").next().unwrap_or("").split(" (").next().unwrap_or("");
        vec![reel_key(w)]
    } else if n.mid == MINED_SENTENCE {
        vec![f(0)]
    } else {
        vec![]
    };
    keys.into_iter().filter(|k| !k.is_empty()).collect()
}

/// Every key of the cards of Seu Reels (`reel_keys`).
pub fn reel_key_set(col: &crate::study::Collection) -> HashSet<String> {
    col.notes.iter().flat_map(reel_keys).collect()
}

/// Removes the repeated cards of Seu Reels (the same word or line saved more than once): of each group, the one
/// studied most stays (the oldest when even). How many notes went.
pub fn dedupe_reels(col: &mut crate::study::Collection) -> usize {
    let reps = |nid: i64, col: &crate::study::Collection| col.cards.iter().filter(|c| c.nid == nid).map(|c| c.reps as i64 + if c.kind > 0 { 1 } else { 0 }).sum::<i64>();
    let mut order: Vec<(i64, i64, Vec<String>)> = col.notes.iter().map(|n| (n.id, reps(n.id, col), reel_keys(n))).filter(|x| !x.2.is_empty()).collect();
    order.sort_by_key(|x| (-x.1, x.0));
    let mut seen: HashSet<String> = HashSet::new();
    let mut gone: Vec<i64> = vec![];
    for (id, _, keys) in order {
        if keys.iter().any(|k| seen.contains(k)) {
            gone.push(id);
        } else {
            seen.extend(keys);
        }
    }
    for id in &gone {
        col.purge_note(*id);
    }
    gone.len()
}

/// Text of a subtitle as HTML for a card field (its lines on lines of their own).
fn html_text(s: &str) -> String {
    s.trim().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\n', "<br>")
}

/// Adds the note type of mined sentences when it is missing.
fn ensure_mined_sentence(col: &mut crate::study::Collection) {
    if col.models.iter().any(|m| m.id == MINED_SENTENCE) {
        return;
    }
    let fields = ["Sentence", "Translation", "Words", "Clip", "Image", "Source"];
    col.models.push(crate::study::Model {
        id: MINED_SENTENCE,
        name: "Reelhouse mined sentence".into(),
        cloze: false,
        fields: fields.iter().map(|f| f.to_string()).collect(),
        tmpls: vec![crate::study::Tmpl {
            name: "Sentence".into(),
            q: "{{Image}}<br><big><big>{{Sentence}}</big></big><br>{{Clip}}".into(),
            a: "{{FrontSide}}<hr id=answer>{{#Words}}<big>{{Words}}</big><br>{{/Words}}{{#Translation}}{{Translation}}<br>{{/Translation}}<small>{{Source}}</small>".into(),
        }],
    });
}

/// Saves a word that word mixing swapped into the subtitle (the macaw's button in the player) as a card in
/// "Seu Reels": the mixed line on the front, the learned word, its reading and the word it replaced on the back.
/// False when that word of that episode is already there.
pub fn add_swapped(col: &mut crate::study::Collection, tag: &str, w: &super::studyplay::SwapSeen, source: &str, now: i64) -> Result<bool, String> {
    col.ensure_builtin();
    ensure_mined_sentence(col);
    let mut words = if w.reading.is_empty() { format!("{} = {}", w.word, w.original) } else { format!("{} ({}) = {}", w.word, w.reading, w.original) };
    if !w.base.is_empty() {
        // the form said in the audio, and the word it is a form of
        words.push_str(&format!(" [{}]", w.base));
    }
    // that word already in Seu Reels (from any episode)
    let key = reel_key(&w.word);
    if key.is_empty() || col.notes.iter().any(|n| reel_keys(n).contains(&key)) {
        return Ok(false);
    }
    let did = col.app_deck().ok_or("could not make the deck")?;
    let fields = vec![w.line.clone(), w.plain.clone(), words, String::new(), String::new(), source.to_string()];
    col.add_note(did, MINED_SENTENCE, fields, now)?;
    if let Some(n) = col.notes.last_mut() {
        n.tags = format!("seu_reels swapped {}", tag);
    }
    Ok(true)
}

/// The keys (`reel_keys`) the card of a mined moment would have.
pub fn mined_keys(cand: &Candidate, target: &str, original: &str) -> Vec<String> {
    let keys = if cand.mode == Mode::Sentence {
        vec![reel_key(&cand.line)]
    } else if target.trim().is_empty() && original.trim().is_empty() {
        vec![reel_key(&cand.word)]
    } else {
        vec![reel_key(target), reel_key(original)]
    };
    keys.into_iter().filter(|k| !k.is_empty()).collect()
}

/// Saves a mined clip as a card in "Seu Reels", unless that word (or line) is already there, from any episode.
pub fn add_mined(col: &mut crate::study::Collection, m: &Mined, now: i64) -> Result<bool, String> {
    col.ensure_builtin();
    let keys = mined_keys(&m.cand, &m.target, &m.original);
    if keys.is_empty() || col.notes.iter().any(|n| reel_keys(n).iter().any(|k| keys.contains(k))) {
        return Ok(false);
    }
    if m.cand.mode == Mode::Sentence {
        ensure_mined_sentence(col);
        let did = col.app_deck().ok_or("could not make the deck")?;
        let clip = format!("<a href=\"clip:{}\">▶ Play the clip</a>", m.video);
        let img = m.picture.as_ref().map(|p| format!("<img src=\"{}\">", p)).unwrap_or_default();
        let fields = vec![m.cand.line.clone(), m.cand.translation.clone(), m.target.clone(), clip, img, m.source.clone()];
        col.add_note(did, MINED_SENTENCE, fields, now)?;
        if let Some(n) = col.notes.last_mut() {
            n.tags = format!("seu_reels mined sentence {}", m.tag);
        }
        return Ok(true);
    }
    ensure_mined(col);
    let did = col.app_deck().ok_or("could not make the deck")?;
    let clip = format!("<a href=\"clip:{}\">▶ Play the clip</a>", m.video);
    let img = m.picture.as_ref().map(|p| format!("<img src=\"{}\">", p)).unwrap_or_default();
    let fields = vec![m.cand.word.clone(), m.target.clone(), m.reading.clone(), clip, img, m.source.clone(), m.cand.line.clone(), html_text(&m.original)];
    col.add_note(did, MINED, fields, now)?;
    if let Some(n) = col.notes.last_mut() {
        n.tags = format!("seu_reels mined {}", m.tag);
    }
    Ok(true)
}

fn stamp(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

impl Core {
    /// Called with the player's tick (from `studyplay_step`): checks the languages, finds the moments and cuts them.
    pub(super) fn mining_step(&mut self, mpv: &Mpv) {
        let g = &self.settings.general;
        let on = g.study_in_player && g.study_mining && !g.study_lang.is_empty();
        let playing = self.session.as_ref().map(|s| s.stage == Stage::Playing).unwrap_or(false);
        let sid = self.session.as_ref().map(|s| s.id).unwrap_or(0);
        if !on && playing && self.mining.off_logged != sid {
            // said once per episode, so the log tells why no card came
            let why = if !g.study_in_player {
                "\"Study in the player\" is off"
            } else if !g.study_mining {
                "\"Cards from the episode\" (mining) is off"
            } else {
                "no study language is set"
            };
            crate::debug::log("study", &format!("mining: off ({})", why));
            self.mining.off_logged = sid;
        }
        if self.mining.session != sid || !on {
            self.mining_finish();
            if !on {
                return;
            }
            let logged = self.mining.off_logged;
            self.mining = MiningState { session: sid, off_logged: logged, ..MiningState::default() };
        }
        if !playing {
            return;
        }
        let since = *self.mining.playing_since.get_or_insert_with(Instant::now);
        if self.mining.last_check.map(|t| t.elapsed().as_secs_f64() >= 3.0).unwrap_or(true) {
            self.mining.last_check = Some(Instant::now());
            self.mining_check_tracks(mpv);
            // single words: their cards show the audio's own subtitle, looked for by word mixing; when that is off,
            // mining looks for it
            let g = &self.settings.general;
            if self.mining.mode == Mode::Word && !(g.study_wordmix && g.study_wordmix_rate > 0) {
                self.wordmix_orig_step(mpv);
            }
        }
        if self.mining.key.is_empty() || self.mining.key.starts_with('-') {
            return;
        }
        if self.mining.progressive {
            self.mining_collect(mpv);
        }
        if since.elapsed().as_secs() >= START_WAIT_SECS {
            self.mining_next();
        }
    }

    /// The tracks playing: when they changed, decides what mining does with them (`plan`) and reads the subtitle file.
    fn mining_check_tracks(&mut self, mpv: &Mpv) {
        let g = &self.settings.general;
        let (study, known) = (g.study_lang.clone(), g.known_langs.clone());
        let aid = mpv.get_i64("aid");
        let sid = mpv.get_i64("sid");
        let audio = mpv.tracks("audio").into_iter().find(|t| Some(t.id) == aid);
        let sub = mpv.tracks("sub").into_iter().find(|t| Some(t.id) == sid);
        let Some(s) = sub else {
            if self.mining.key != "-nosub" {
                crate::debug::log("study", "mining: waiting for a subtitle track to be shown (none is on)");
                self.mining.key = "-nosub".into();
            }
            return;
        };
        let (a_id, a_lang) = audio.map(|a| (a.id, a.lang)).unwrap_or((0, String::new()));
        let key = format!("{}|{}|{}|{}|{}", a_id, a_lang, s.id, s.lang, s.file);
        if key == self.mining.key || (self.mining.key.starts_with('-') && self.mining.key[1..] == key) {
            return;
        }
        let file = s.external && !s.file.is_empty();
        let file_lines = if file { read_lines(&s.file) } else { vec![] };
        // a track without a tag: its text tells (a file now; a track inside the video from its first lines)
        let mut sub_lang = s.lang.clone();
        if file && matches!(util::lang_base(&sub_lang).as_str(), "" | "und") {
            sub_lang = guess_lang(file_lines.iter().map(|l| l.text.as_str()), &study);
        }
        let mode = match plan(&a_lang, &sub_lang, &study, &known) {
            Ok(m) => m,
            Err(why) if why == UNTAGGED && !file => Mode::Undecided,
            Err(why) => {
                crate::debug::log("study", &format!("mining: off for this episode ({}: audio '{}', subtitle '{}' {})", why, a_lang, sub_lang, if file { "file" } else { "inside the video" }));
                self.mining.key = format!("-{}", key);
                return;
            }
        };
        let Some(sess) = self.session.as_ref() else { return };
        let tag = ep_tag(&sess.sid, &sess.title);
        let study3 = super::studyplay::lang3(&study);
        let from3 = super::studyplay::lang3(if mode == Mode::Word { &sub_lang } else { known.first().map(|k| k.as_str()).unwrap_or("en") });
        let lex = Lexicon::build(&self.knowledge, self.learn.pack.as_deref(), &study3, &from3, 0.0, true, Self::now_secs());
        let st = &mut self.mining;
        st.key = key;
        st.tag = tag.clone();
        st.lexicon = Some(lex);
        st.mode = mode;
        st.study3 = study3;
        st.lines.clear();
        st.trans.clear();
        st.looked = 0;
        st.queue.clear();
        // the words (lines) already in Seu Reels, from any episode
        st.words = reel_key_set(&self.study);
        st.known_lines.clear();
        st.progressive = !file;
        crate::debug::log(
            "study",
            &format!(
                "mining: on, {} (audio '{}', subtitle '{}' {}, study {})",
                match mode {
                    Mode::Sentence => "sentence cards from the lines",
                    Mode::Word => "single words between silences",
                    Mode::Undecided => "the subtitle's first lines will tell its language",
                },
                a_lang,
                sub_lang,
                if file { "file" } else { "inside the video, read while it plays" },
                study
            ),
        );
        if st.progressive {
            return;
        }
        if mode == Mode::Sentence {
            // a subtitle file in a known language loaded too: the translation on the back of the cards
            let known_base: Vec<String> = known.iter().map(|k| util::lang_base(k)).collect();
            let st_base = util::lang_base(&study);
            if let Some(t) = mpv.tracks("sub").into_iter().find(|t| t.external && !t.file.is_empty() && { let b = util::lang_base(&t.lang); !b.is_empty() && b != st_base && (known_base.is_empty() || known_base.contains(&b)) }) {
                self.mining.known_lines = read_lines(&t.file);
            }
        }
        let avoid = self.mining_avoid();
        let cands = match mode {
            Mode::Sentence => {
                let mut c = select_sentences(&file_lines, &avoid);
                // what has something new first (one unknown word = the easiest to learn), then in time order
                c.retain(|x| self.mining_unknown(&x.line).1 > 0);
                c.sort_by_key(|x| (self.mining_unknown(&x.line).1, (x.start * 10.0) as i64));
                c
            }
            _ => {
                let mut c = select(&file_lines, GAP, &avoid);
                // words with a learned word for them first (the back of the card teaches it), then in time order
                let lex = self.mining.lexicon.as_ref();
                c.sort_by_key(|c| (lex.and_then(|l| l.lookup(&c.word)).is_none(), (c.start * 10.0) as i64));
                c
            }
        };
        for c in cands {
            self.mining_take(c);
        }
        crate::debug::log("study", &format!("mining: {} lines in the file, {} moments queued", file_lines.len(), self.mining.queue.len()));
    }

    /// The words of a line not known yet (knowledge state), and how many: what a sentence card would teach.
    fn mining_unknown(&self, line: &str) -> (Vec<String>, usize) {
        let lang = &self.mining.study3;
        let mut out: Vec<String> = vec![];
        for w in self.knowledge.tokens(lang, line) {
            if w.chars().any(char::is_alphabetic) && !self.knowledge.knows(lang, &w) && !out.contains(&w) {
                out.push(w);
            }
        }
        let n = out.len();
        (out, n)
    }

    /// Openings and endings (no mining in songs).
    fn mining_avoid(&self) -> Vec<(f64, f64)> {
        let Some(s) = self.session.as_ref() else { return vec![] };
        s.segments.iter().filter(|x| matches!(x.kind, Kind::Intro | Kind::Credits)).map(|x| (x.start, x.end.unwrap_or(f64::MAX))).collect()
    }

    /// Queues a moment, unless its word was taken (in this episode or a card of Seu Reels) or the episode has enough.
    fn mining_take(&mut self, c: Candidate) {
        let max = self.settings.general.study_mining_max;
        let target = if c.mode == Mode::Word { self.mining.lexicon.as_ref().and_then(|l| l.lookup(&c.word)).map(|e| e.target.clone()).unwrap_or_default() } else { String::new() };
        let sid = self.mining.session;
        let original = if c.mode == Mode::Word { super::studyplay::heard_at(self.study_play.orig_lines(sid).0, c.start, c.end) } else { String::new() };
        let mut keys = mined_keys(&c, &target, &original);
        // the subtitle's own word too: not twice from this episode
        keys.push(format!("word:{}", reel_key(&c.word)));
        let st = &mut self.mining;
        if st.taken >= max || keys.iter().any(|k| st.words.contains(k)) {
            return;
        }
        st.words.extend(keys);
        st.taken += 1;
        st.queue.push_back(c);
    }

    /// A subtitle inside the video: keeps each line as it shows (with the second subtitle's text), and looks at a line
    /// once the next one started or the silence after it is long enough. A jump forgets the lines (the silence around
    /// them is not known). A track without a language tag is told by its first lines.
    fn mining_collect(&mut self, mpv: &Mpv) {
        let pos = mpv.get_f64("time-pos").unwrap_or(0.0);
        let st = &mut self.mining;
        let prev = std::mem::replace(&mut st.last_pos, pos);
        if (pos - prev).abs() > 2.0 {
            st.lines.clear();
            st.trans.clear();
            st.looked = 0;
        }
        if let (Some(a), Some(b)) = (mpv.get_f64("sub-start"), mpv.get_f64("sub-end")) {
            if (a - st.seen_start).abs() > 0.01 && b > a {
                let text = super::studyplay::plain(&mpv.get_str("sub-text").unwrap_or_default());
                if !text.is_empty() {
                    st.seen_start = a;
                    st.lines.push(Line { start: a, end: b, text });
                    st.trans.push(super::studyplay::plain(&mpv.get_str("secondary-sub-text").unwrap_or_default()));
                }
            }
        }
        if st.mode == Mode::Undecided {
            if st.lines.len() < 4 {
                return;
            }
            let g = &self.settings.general;
            let lang = guess_lang(self.mining.lines.iter().map(|l| l.text.as_str()), &g.study_lang);
            match plan("", &lang, &g.study_lang, &g.known_langs) {
                Ok(m) => {
                    crate::debug::log("study", &format!("mining: the subtitle reads as '{}': {}", if lang.is_empty() { "?" } else { lang.as_str() }, if m == Mode::Sentence { "sentence cards" } else { "single words" }));
                    self.mining.mode = m;
                    if m == Mode::Word {
                        let from3 = super::studyplay::lang3(&lang);
                        self.mining.lexicon = Some(Lexicon::build(&self.knowledge, self.learn.pack.as_deref(), &self.mining.study3, &from3, 0.0, true, Self::now_secs()));
                    }
                }
                Err(why) => {
                    crate::debug::log("study", &format!("mining: off for this episode (the untagged subtitle reads as '{}': {})", lang, why));
                    self.mining.key = format!("-{}", self.mining.key);
                    return;
                }
            }
        }
        let avoid = self.mining_avoid();
        let mut found = vec![];
        let sentences = self.mining.mode == Mode::Sentence;
        let st = &self.mining;
        let mut looked = st.looked;
        while looked < st.lines.len() {
            let i = looked;
            let l = &st.lines[i];
            if sentences {
                // a line is whole once it ended
                if pos < l.end {
                    break;
                }
                found.extend(sentence_moment(l, st.trans.get(i).map(|s| s.as_str()).unwrap_or(""), &avoid));
                looked += 1;
                continue;
            }
            // the silence after it: until the next line, or until now when none came yet
            let next = match st.lines.get(i + 1) {
                Some(n) => n.start,
                None if pos >= l.end + GAP => pos,
                None => break,
            };
            // the first line seen (at the start or after a jump): what came before it is not known
            if i > 0 {
                let before = st.lines[..i].iter().map(|x| x.end).fold(0.0, f64::max);
                // the second subtitle shown with it (the audio's language, when the viewer shows one)
                let second = st.trans.get(i).cloned().unwrap_or_default();
                found.extend(moment(l, before, next, GAP, &avoid).map(|c| Candidate { translation: second, ..c }));
            }
            looked += 1;
        }
        self.mining.looked = looked;
        if sentences {
            // only a line with something new to learn
            found.retain(|c| self.mining_unknown(&c.line).1 > 0);
        }
        for c in found {
            self.mining_take(c);
        }
    }

    /// Cuts the next moment in the background (one at a time).
    fn mining_next(&mut self) {
        if self.mining.busy {
            return;
        }
        // a single word: its card shows the line of the audio's subtitle; wait (a little) while it is looked for
        let sid = self.mining.session;
        if self.mining.queue.front().map(|c| c.mode == Mode::Word).unwrap_or(false) {
            let waited = self.mining.playing_since.map(|t| t.elapsed().as_secs()).unwrap_or(0);
            if self.study_play.orig_lines(sid).1 && waited < ORIG_WAIT_SECS {
                return;
            }
        }
        let Some(c) = self.mining.queue.pop_front() else {
            // a subtitle file: every moment was made; tell the user once
            if !self.mining.progressive && self.mining.made > 0 && !self.mining.announced {
                self.mining_announce();
            }
            return;
        };
        let Some(mpv) = self.mpv.clone() else { return };
        let Some(sess) = self.session.as_ref() else { return };
        let url = mpv.get_str("path").unwrap_or_default();
        if url.is_empty() {
            return;
        }
        let headers = sess.streams.get(sess.cur).map(|s| s.headers.clone()).unwrap_or_default();
        let source = format!("{} · {}", sess.title, stamp(c.start));
        let (target, reading) = if c.mode == Mode::Sentence {
            (self.mining_unknown(&c.line).0.join("、"), String::new())
        } else {
            self.mining.lexicon.as_ref().and_then(|l| l.lookup(&c.word)).map(|e| (e.target.clone(), e.reading.clone())).unwrap_or_default()
        };
        let mut c = c;
        let original = if c.mode == Mode::Word {
            // the audio's subtitle at that moment; else the second subtitle shown, when it is in the study language
            let heard = super::studyplay::heard_at(self.study_play.orig_lines(sid).0, c.start, c.end);
            let study = util::lang_base(&self.settings.general.study_lang);
            if !heard.trim().is_empty() {
                heard
            } else if !c.translation.is_empty() && guess_lang(std::iter::once(c.translation.as_str()), &study) == study {
                c.translation.clone()
            } else {
                String::new()
            }
        } else {
            if c.translation.is_empty() && !self.mining.known_lines.is_empty() {
                c.translation = super::studyplay::heard_at(&self.mining.known_lines, c.start, c.end);
            }
            String::new()
        };
        // already in Seu Reels (found now that the Japanese line is known): not cut
        let keys = mined_keys(&c, &target, &original);
        let have = reel_key_set(&self.study);
        if keys.iter().any(|k| have.contains(k)) {
            crate::debug::log("study", &format!("mining: '{}' is already in Seu Reels, not saved again", c.word));
            return;
        }
        crate::debug::log("study", &format!("mining: cutting '{}'{}", c.word, if original.is_empty() { " (no line of the audio's subtitle for it)".to_string() } else { format!(" with the audio's line '{}'", original.replace('\n', " ")) }));
        let tag = self.mining.tag.clone();
        let session = self.mining.session;
        let libs = self.rt.libmpv_candidates();
        let media = self.study_media_dir();
        let _ = std::fs::create_dir_all(&media);
        let now = Self::now_secs();
        self.mining.busy = true;
        self.spawn(move |tx, _http| {
            let r = (|| -> Result<Mined, String> {
                let base = format!("mine_{}_{}", now, (c.start * 100.0) as i64);
                let video = format!("{}.mp4", base);
                let pic = format!("{}.jpg", base);
                let (a, b) = ((c.start - PAD).max(0.0), c.end + PAD);
                Mpv::encode(&libs, &url, &headers, a, b, &media.join(&video).display().to_string(), false, 120)?;
                let picture = Mpv::encode(&libs, &url, &headers, (c.start + c.end) / 2.0, (c.start + c.end) / 2.0 + 0.3, &media.join(&pic).display().to_string(), true, 60).ok().map(|_| pic);
                Ok(Mined { cand: c, original, target, reading, video, picture, source, tag })
            })();
            send(tx, Msg::Mined(session, r));
        });
    }

    pub(super) fn on_mined(&mut self, session: u64, r: Result<Mined, String>) {
        if session == self.mining.session {
            self.mining.busy = false;
        }
        match r {
            Ok(m) => match add_mined(&mut self.study, &m, Self::now_secs()) {
                Ok(true) => {
                    self.save_study();
                    self.events.push(Event::Study);
                    if session == self.mining.session {
                        self.mining.made += 1;
                    } else {
                        // the episode ended while it was cut: count it in a banner of its own
                        self.mining_banner(1);
                    }
                }
                Ok(false) => crate::debug::log("study", &format!("mining: '{}' is already in Seu Reels, not saved again", m.cand.word)),
                Err(e) => crate::debug::log("study", &format!("mining: the card could not be saved: {}", e)),
            },
            Err(e) => crate::debug::log("study", &format!("mining: the clip could not be cut: {}", e)),
        }
    }

    /// The episode ended (or mining was turned off): says how many cards it gave, if not said yet.
    fn mining_finish(&mut self) {
        if self.mining.made > 0 && !self.mining.announced {
            self.mining_announce();
        }
        self.mining.queue.clear();
    }

    fn mining_announce(&mut self) {
        self.mining.announced = true;
        let n = self.mining.made;
        self.mining_banner(n);
    }

    fn mining_banner(&mut self, n: u32) {
        let t = |s: &str| {
            let l = &self.ui_lang();
            let lang = if l.is_empty() || l == "system" { self.system_lang.replace('_', "-") } else { l.clone() };
            crate::i18n::translate(&lang, s).map(|x| x.to_string()).unwrap_or_else(|| s.to_string())
        };
        let title = if n == 1 { t("1 new card from this episode") } else { t("%1 new cards from this episode").replace("%1", &n.to_string()) };
        let body = t("Lines and words of the episode, saved with their clips and pictures in the deck Seu Reels.");
        self.banner("info", &title, &body, "cheer", Some(Json::obj(vec![("cmd", Json::Str("set_mode".into())), ("mode", Json::Str("study".into()))])));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ln(a: f64, b: f64, t: &str) -> Line {
        Line { start: a, end: b, text: t.into() }
    }

    #[test]
    fn single_words_between_silences() {
        let lines = vec![
            ln(10.0, 12.0, "Vamos embora daqui agora."),
            ln(14.0, 14.8, "Obrigado!"),       // 2 s before, 2.2 s after: yes
            ln(17.0, 18.0, "Muito bem!"),      // two short words, 2.2 s before, 1.0 s after: no
            ln(19.0, 20.0, "Certo."),          // 1.0 s before: no
            ln(30.0, 31.0, "[risos]"),         // a sound note: no
            ln(40.0, 40.9, "Muito bem!"),      // two short words, silences: yes
            ln(50.0, 50.6, "obrigado"),        // the same word again: no
            ln(60.0, 61.0, "Desculpa"),        // inside the ending: no
            ln(70.0, 71.0, "Adeus"),           // the last line, silence after: yes
        ];
        let c = select(&lines, GAP, &[(55.0, 65.0)]);
        let words: Vec<&str> = c.iter().map(|c| c.word.as_str()).collect();
        assert_eq!(words, vec!["Obrigado", "Muito bem", "Adeus"]);
        assert_eq!((c[0].start, c[0].end, c[0].line.as_str()), (14.0, 14.8, "Obrigado!"));
        // an overlapping line counts as subtitles around it
        let ov = vec![ln(5.0, 9.0, "Uma frase longa aqui"), ln(8.5, 9.2, "Sim!"), ln(20.0, 21.0, "x y z w")];
        assert!(select(&ov, GAP, &[]).is_empty());
    }

    #[test]
    fn isolated_words() {
        assert_eq!(isolated_word("Obrigado!").as_deref(), Some("Obrigado"));
        assert_eq!(isolated_word("- Don't!").as_deref(), Some("Don't"));
        assert_eq!(isolated_word("Muito bem!").as_deref(), Some("Muito bem"));
        assert_eq!(isolated_word("Extraordinariamente bom"), None);
        assert_eq!(isolated_word("Eu não sei"), None);
        assert_eq!(isolated_word("♪ La la ♪"), None);
        assert_eq!(isolated_word("(suspira)"), None);
        assert_eq!(isolated_word("123"), None);
        assert_eq!(isolated_word("はい！").as_deref(), Some("はい"));
    }

    #[test]
    fn language_planning() {
        let known = vec!["pt".to_string(), "en".to_string()];
        // Japanese audio, Portuguese subtitle, learning Japanese: single words (mkv tracks use 3-letter codes)
        assert_eq!(plan("jpn", "por", "ja", &known), Ok(Mode::Word));
        assert_eq!(plan("ja", "pt-BR", "ja", &known), Ok(Mode::Word));
        // Japanese subtitles on a Japanese anime: sentence cards, also when the audio has no tag (fansub releases)
        assert_eq!(plan("jpn", "jpn", "ja", &known), Ok(Mode::Sentence));
        assert_eq!(plan("", "ja", "ja", &known), Ok(Mode::Sentence));
        assert_eq!(plan("und", "jpn", "ja", &known), Ok(Mode::Sentence));
        assert_eq!(plan("und", "por", "ja", &known), Ok(Mode::Word));
        // a dub is not mined
        assert!(plan("por", "por", "ja", &known).is_err());
        assert!(plan("eng", "jpn", "ja", &known).is_err());
        // a subtitle without a tag: its text must tell
        assert_eq!(plan("jpn", "", "ja", &known), Err(UNTAGGED));
        // a subtitle in a language not known; with no known language set, any other language is fine
        assert!(plan("jpn", "fre", "ja", &known).is_err());
        assert_eq!(plan("jpn", "fre", "ja", &[]), Ok(Mode::Word));
        // no study language
        assert!(plan("jpn", "por", "", &known).is_err());
    }

    #[test]
    fn untagged_subtitles_are_told_by_their_text() {
        let ja = ["ねえ、どこ行くの？", "学校だよ", "そうか", "Ok"];
        assert_eq!(guess_lang(ja.iter().copied(), "ja"), "ja");
        let pt = ["Aonde você vai?", "Para a escola.", "Entendi."];
        assert_eq!(guess_lang(pt.iter().copied(), "ja"), "xx", "Latin text is surely not Japanese");
        assert_eq!(guess_lang(pt.iter().copied(), "es"), "", "Latin text: cannot tell which");
        assert_eq!(plan("", "xx", "ja", &[]), Ok(Mode::Word));
        assert_eq!(guess_lang(["안녕하세요"].iter().copied(), "ko"), "ko");
    }

    #[test]
    fn sentence_lines_are_mined_whole() {
        let lines = vec![
            ln(10.0, 12.0, "ねえ、どこに行くの？"),
            ln(12.5, 13.0, "え"),                   // too short
            ln(14.0, 16.0, "（ため息）"),            // a sound note
            ln(20.0, 23.0, "♪ 夢を見ていた ♪"),      // a song
            ln(30.0, 32.0, "学校に行くんだ"),
            ln(40.0, 42.0, "ねえ、どこに行くの？"),  // the same line again
            ln(60.0, 62.0, "また明日ね"),           // inside the ending
        ];
        let c = select_sentences(&lines, &[(55.0, 65.0)]);
        let got: Vec<&str> = c.iter().map(|c| c.line.as_str()).collect();
        assert_eq!(got, vec!["ねえ、どこに行くの？", "学校に行くんだ"]);
        assert!(c.iter().all(|c| c.mode == Mode::Sentence));
        assert!(sentence_line("Where are you going now?"));
        assert!(!sentence_line("Yes!"));
    }

    #[test]
    fn swapped_words_go_to_seu_reels_once() {
        let mut col = crate::study::Collection::default();
        let w = super::super::studyplay::SwapSeen { word: "食べる".into(), reading: "たべる".into(), original: "comer".into(), base: String::new(), line: "Eu quero 食べる agora".into(), plain: "Eu quero comer agora".into(), at: 12.0 };
        let tag = ep_tag("tt1:1:2", "Show");
        assert_eq!(add_swapped(&mut col, &tag, &w, "Show 1x2", 100), Ok(true));
        assert_eq!(add_swapped(&mut col, &tag, &w, "Show 1x2", 101), Ok(false));
        // another episode with the same word: not again
        assert_eq!(add_swapped(&mut col, &ep_tag("tt1:1:3", "Show"), &w, "Show 1x3", 102), Ok(false));
        let n = col.notes.iter().find(|n| n.mid == MINED_SENTENCE).unwrap();
        assert_eq!(n.fields[0], "Eu quero 食べる agora");
        assert_eq!(n.fields[2], "食べる (たべる) = comer");
        assert!(n.tags.contains("swapped") && n.tags.contains("seu_reels"));
    }

    #[test]
    fn mined_sentences_go_to_seu_reels_once() {
        let mut col = crate::study::Collection::default();
        let m = Mined {
            cand: Candidate { start: 10.0, end: 12.0, word: "学校に行くんだ".into(), line: "学校に行くんだ".into(), translation: "Vou para a escola".into(), mode: Mode::Sentence },
            original: String::new(),
            target: "学校".into(),
            reading: String::new(),
            video: "mine_2.mp4".into(),
            picture: Some("mine_2.jpg".into()),
            source: "Show · 0:10".into(),
            tag: ep_tag("tt1:1:2", "Show"),
        };
        assert_eq!(add_mined(&mut col, &m, 1000), Ok(true));
        assert_eq!(add_mined(&mut col, &m, 1001), Ok(false));
        assert_eq!(col.notes.len(), 1);
        assert_eq!(col.notes[0].mid, MINED_SENTENCE);
        assert_eq!(col.deck_name(col.cards[0].did), crate::study::APP_DECK);
        let (q, a) = col.render(&col.cards[0], std::path::Path::new("/m"));
        assert!(q.contains("学校に行くんだ") && q.contains("mine_2.mp4"));
        assert!(a.contains("Vou para a escola"));
    }

    #[test]
    fn mined_cards_go_to_seu_reels_once() {
        let mut col = crate::study::Collection::default();
        let m = Mined {
            cand: Candidate { start: 14.0, end: 14.8, word: "Obrigado".into(), line: "Obrigado!".into(), translation: String::new(), mode: Mode::Word },
            original: "ありがとう！".into(),
            target: "ありがとう".into(),
            reading: String::new(),
            video: "mine_1.mp4".into(),
            picture: Some("mine_1.jpg".into()),
            source: "Show · 0:14".into(),
            tag: ep_tag("tt1:1:2", "Show"),
        };
        assert_eq!(add_mined(&mut col, &m, 1000), Ok(true));
        assert_eq!(add_mined(&mut col, &Mined { cand: Candidate { word: "obrigado".into(), ..m.cand.clone() }, ..m.clone() }, 1001), Ok(false));
        // another episode with the same word: not again (the same learned word, or the same Japanese line)
        assert_eq!(add_mined(&mut col, &Mined { tag: ep_tag("tt1:1:3", "Show"), ..m.clone() }, 1002), Ok(false));
        let other = Mined { tag: ep_tag("tt1:1:4", "Show"), target: String::new(), cand: Candidate { word: "Valeu".into(), ..m.cand.clone() }, ..m.clone() };
        assert_eq!(add_mined(&mut col, &other, 1003), Ok(false));
        // the same subtitle word for another Japanese word is a card of its own
        let yes = Mined { tag: ep_tag("tt1:1:5", "Show"), target: "どうも".into(), original: "どうも".into(), ..m.clone() };
        assert_eq!(add_mined(&mut col, &yes, 1004), Ok(true));
        assert_eq!(col.notes.len(), 2);
        let did = col.cards[0].did;
        assert_eq!(col.deck_name(did), crate::study::APP_DECK);
        // the Japanese line with the clip on the front; the known language only on the back
        let (q, a) = col.render(&col.cards[0], std::path::Path::new("/m"));
        assert!(q.contains("mine_1.mp4") && q.contains("ありがとう！") && !q.contains("Obrigado"));
        let back = a.split("<hr id=answer>").nth(1).unwrap();
        assert!(back.contains("Obrigado") && back.contains("ありがとう"));
    }

    #[test]
    fn older_mined_cards_get_the_japanese_line_and_repeats_go() {
        let mut col = crate::study::Collection::default();
        col.ensure_builtin();
        // the note type as it was before (no "Original")
        col.models.push(crate::study::Model {
            id: MINED,
            name: "Reelhouse mined word".into(),
            cloze: false,
            fields: ["Word", "Target", "Reading", "Clip", "Image", "Source", "Line"].iter().map(|f| f.to_string()).collect(),
            tmpls: vec![crate::study::Tmpl { name: "Listen".into(), q: MINED_Q_OLD.into(), a: MINED_A.into() }],
        });
        let did = col.app_deck().unwrap();
        let old = |w: &str, t: &str| vec![w.to_string(), t.to_string(), String::new(), "<a href=\"clip:a.mp4\">▶</a>".to_string(), String::new(), "src".to_string(), format!("{}!", w)];
        col.add_note(did, MINED, old("Obrigado", "ありがとう"), 100).unwrap();
        col.add_note(did, MINED, old("obrigado", "ありがとう"), 101).unwrap();
        col.add_note(did, MINED, old("Sim", ""), 102).unwrap();
        col.add_note(did, MINED, old("Sim!", ""), 103).unwrap();
        col.add_note(did, MINED, old("Adeus", "さようなら"), 104).unwrap();
        // the second "obrigado" was studied: it stays, the first goes
        let second = col.notes[1].id;
        col.cards.iter_mut().find(|c| c.nid == second).unwrap().reps = 3;
        assert!(ensure_mined(&mut col));
        assert!(!ensure_mined(&mut col));
        assert_eq!(dedupe_reels(&mut col), 2);
        assert_eq!(dedupe_reels(&mut col), 0);
        assert_eq!(col.notes.len(), 3);
        assert!(col.notes.iter().any(|n| n.id == second));
        // an older card renders as before (no "Original" value)
        let c = col.cards.iter().find(|c| c.nid == second).unwrap().clone();
        let (q, _) = col.render(&c, std::path::Path::new("/m"));
        assert!(q.contains("a.mp4") && !q.contains("ありがとう"));
    }
}
