//! The Japanese characters of Learn: the hiragana and katakana charts (gojūon, dakuten / handakuten, yōon), the
//! lessons that teach them a row at a time, and their sounds (assets/kana/<romaji>.ogg, see assets/kana/CREDITS.md).
//! Katakana is not written out: each katakana is its hiragana moved by 0x60 in Unicode (か -> カ).
use super::{Challenge, Rng};
use crate::json::Json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// One character: hiragana, romaji (Hepburn) and the name of its sound file when it differs from the romaji.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kana {
    pub h: &'static str,
    pub r: &'static str,
    pub snd: &'static str,
}

const fn k(h: &'static str, r: &'static str) -> Option<Kana> {
    Some(Kana { h, r, snd: "" })
}
/// A character that sounds like another one (を = o, ぢ = ji, づ = zu).
const fn ks(h: &'static str, r: &'static str, snd: &'static str) -> Option<Kana> {
    Some(Kana { h, r, snd })
}
const X: Option<Kana> = None;

/// The gojūon chart, five columns (a i u e o), empty cells where the row has no character.
pub const GOJUON: [[Option<Kana>; 5]; 11] = [
    [k("あ", "a"), k("い", "i"), k("う", "u"), k("え", "e"), k("お", "o")],
    [k("か", "ka"), k("き", "ki"), k("く", "ku"), k("け", "ke"), k("こ", "ko")],
    [k("さ", "sa"), k("し", "shi"), k("す", "su"), k("せ", "se"), k("そ", "so")],
    [k("た", "ta"), k("ち", "chi"), k("つ", "tsu"), k("て", "te"), k("と", "to")],
    [k("な", "na"), k("に", "ni"), k("ぬ", "nu"), k("ね", "ne"), k("の", "no")],
    [k("は", "ha"), k("ひ", "hi"), k("ふ", "fu"), k("へ", "he"), k("ほ", "ho")],
    [k("ま", "ma"), k("み", "mi"), k("む", "mu"), k("め", "me"), k("も", "mo")],
    [k("や", "ya"), X, k("ゆ", "yu"), X, k("よ", "yo")],
    [k("ら", "ra"), k("り", "ri"), k("る", "ru"), k("れ", "re"), k("ろ", "ro")],
    [k("わ", "wa"), X, X, X, ks("を", "wo", "o")],
    [k("ん", "n"), X, X, X, X],
];

/// Dakuten and handakuten (voiced and p- sounds).
pub const DAKUTEN: [[Option<Kana>; 5]; 5] = [
    [k("が", "ga"), k("ぎ", "gi"), k("ぐ", "gu"), k("げ", "ge"), k("ご", "go")],
    [k("ざ", "za"), k("じ", "ji"), k("ず", "zu"), k("ぜ", "ze"), k("ぞ", "zo")],
    [k("だ", "da"), ks("ぢ", "ji", "ji"), ks("づ", "zu", "zu"), k("で", "de"), k("ど", "do")],
    [k("ば", "ba"), k("び", "bi"), k("ぶ", "bu"), k("べ", "be"), k("ぼ", "bo")],
    [k("ぱ", "pa"), k("ぴ", "pi"), k("ぷ", "pu"), k("ぺ", "pe"), k("ぽ", "po")],
];

/// Yōon: a consonant with a small や / ゆ / よ, three columns (ya yu yo).
pub const YOON: [[Option<Kana>; 3]; 11] = [
    [k("きゃ", "kya"), k("きゅ", "kyu"), k("きょ", "kyo")],
    [k("しゃ", "sha"), k("しゅ", "shu"), k("しょ", "sho")],
    [k("ちゃ", "cha"), k("ちゅ", "chu"), k("ちょ", "cho")],
    [k("にゃ", "nya"), k("にゅ", "nyu"), k("にょ", "nyo")],
    [k("ひゃ", "hya"), k("ひゅ", "hyu"), k("ひょ", "hyo")],
    [k("みゃ", "mya"), k("みゅ", "myu"), k("みょ", "myo")],
    [k("りゃ", "rya"), k("りゅ", "ryu"), k("りょ", "ryo")],
    [k("ぎゃ", "gya"), k("ぎゅ", "gyu"), k("ぎょ", "gyo")],
    [k("じゃ", "ja"), k("じゅ", "ju"), k("じょ", "jo")],
    [k("びゃ", "bya"), k("びゅ", "byu"), k("びょ", "byo")],
    [k("ぴゃ", "pya"), k("ぴゅ", "pyu"), k("ぴょ", "pyo")],
];

/// The lessons of a script, in order: which rows of the charts each one teaches (section, first row, last row).
/// Section 0 = gojūon, 1 = dakuten, 2 = yōon.
const GROUPS: [(usize, usize, usize); 15] = [
    (0, 0, 0),
    (0, 1, 1),
    (0, 2, 2),
    (0, 3, 3),
    (0, 4, 4),
    (0, 5, 5),
    (0, 6, 6),
    (0, 7, 7),
    (0, 8, 8),
    (0, 9, 10),
    (1, 0, 1),
    (1, 2, 3),
    (1, 4, 4),
    (2, 0, 3),
    (2, 4, 10),
];

impl Kana {
    /// The character in a script: 'h' hiragana, 'k' katakana.
    pub fn in_script(&self, script: char) -> String {
        if script == 'k' {
            to_katakana(self.h)
        } else {
            self.h.to_string()
        }
    }
    /// The name of its sound file (without the extension).
    pub fn sound(&self) -> &'static str {
        if self.snd.is_empty() {
            self.r
        } else {
            self.snd
        }
    }
}

/// Hiragana to katakana (か -> カ); other characters stay.
pub fn to_katakana(s: &str) -> String {
    s.chars().map(|c| if ('\u{3041}'..='\u{3096}').contains(&c) { char::from_u32(c as u32 + 0x60).unwrap_or(c) } else { c }).collect()
}

fn section_rows(sec: usize) -> Vec<Vec<Option<Kana>>> {
    match sec {
        0 => GOJUON.iter().map(|r| r.to_vec()).collect(),
        1 => DAKUTEN.iter().map(|r| r.to_vec()).collect(),
        _ => YOON.iter().map(|r| r.to_vec()).collect(),
    }
}

/// Every character, chart order.
pub fn all() -> Vec<Kana> {
    (0..3).flat_map(section_rows).flatten().flatten().collect()
}

/// Finds a character written in either script; returns it with the script it is written in.
pub fn find(ch: &str) -> Option<(Kana, char)> {
    all().into_iter().find_map(|k| {
        if k.h == ch {
            Some((k, 'h'))
        } else if to_katakana(k.h) == ch {
            Some((k, 'k'))
        } else {
            None
        }
    })
}

/// How many lessons a script has.
pub fn group_count() -> usize {
    GROUPS.len()
}

/// The characters a lesson teaches.
pub fn group(n: usize) -> Vec<Kana> {
    let Some(&(sec, a, b)) = GROUPS.get(n) else { return vec![] };
    section_rows(sec).into_iter().skip(a).take(b + 1 - a).flatten().flatten().collect()
}

/// The id of a characters lesson ("kana-h-3") and back.
pub fn lesson_id(script: char, n: usize) -> String {
    format!("kana-{}-{}", script, n + 1)
}

pub fn parse_lesson_id(id: &str) -> Option<(char, usize)> {
    let rest = id.strip_prefix("kana-")?;
    let (s, n) = rest.split_once('-')?;
    let script = match s {
        "h" => 'h',
        "k" => 'k',
        _ => return None,
    };
    let n: usize = n.parse().ok()?;
    (n >= 1 && n <= GROUPS.len()).then(|| (script, n - 1))
}

/// The key under which the finished characters lessons of a script are kept in learn.json ("done").
pub fn done_key(script: char) -> String {
    format!("jpn-kana-{}", script)
}

pub fn script_name(script: char) -> &'static str {
    if script == 'k' {
        "Katakana"
    } else {
        "Hiragana"
    }
}

// ---------------------------------------------------------------- sounds

/// Where the sound files are looked for: next to the program, the repository (development), the data folder.
pub fn sound_dirs(data: &Path) -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        v.push(exe.join("kana"));
        let mut d = exe.clone();
        for _ in 0..4 {
            if let Some(p) = d.parent() {
                d = p.to_path_buf();
                v.push(d.join("assets").join("kana"));
            }
        }
    }
    v.push(data.join("kana"));
    v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("kana"));
    v
}

const SOUND_EXT: [&str; 4] = ["ogg", "opus", "mp3", "oga"];

/// The sound file of a romaji ("ka"), when there is one.
pub fn sound_file(data: &Path, sound: &str) -> Option<PathBuf> {
    if sound.is_empty() || !sound.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    sound_dirs(data).into_iter().flat_map(|d| SOUND_EXT.iter().map(move |e| d.join(format!("{}.{}", sound, e)))).find(|p| p.is_file())
}

/// The sounds that have a file (romaji names), read once.
pub fn sounds(data: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    for d in sound_dirs(data) {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if let (Some(stem), Some(ext)) = (p.file_stem().and_then(|s| s.to_str()), p.extension().and_then(|s| s.to_str())) {
                if SOUND_EXT.contains(&ext) {
                    out.insert(stem.to_string());
                }
            }
        }
    }
    out
}

/// The address a challenge plays ("kana:ka"); learn_play turns it into the file.
pub fn sound_url(k: &Kana, sounds: &HashSet<String>) -> String {
    if sounds.contains(k.sound()) {
        format!("kana:{}", k.sound())
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------- the charts for the window

/// The charts of a script as JSON: sections of rows of cells {c, r, s (strength 0..1), seen, snd}; an empty cell is
/// null. `strength` gives the strength of a character as written.
pub fn chart_json(script: char, strength: &dyn Fn(&str) -> (f64, bool), sounds: &HashSet<String>) -> Json {
    let names = ["gojuon", "dakuten", "yoon"];
    let secs: Vec<Json> = (0..3)
        .map(|sec| {
            let rows: Vec<Json> = section_rows(sec)
                .into_iter()
                .map(|row| {
                    Json::Arr(
                        row.into_iter()
                            .map(|c| match c {
                                Some(k) => {
                                    let ch = k.in_script(script);
                                    let (s, seen) = strength(&ch);
                                    Json::obj(vec![("c", ch.as_str().into()), ("r", k.r.into()), ("s", Json::Num((s * 100.0).round() / 100.0)), ("seen", seen.into()), ("snd", sound_url(&k, sounds).into())])
                                }
                                None => Json::Null,
                            })
                            .collect(),
                    )
                })
                .collect();
            Json::obj(vec![("id", names[sec].into()), ("cols", Json::Num(if sec == 2 { 3.0 } else { 5.0 })), ("rows", Json::Arr(rows))])
        })
        .collect();
    Json::Arr(secs)
}

// ---------------------------------------------------------------- lessons

fn options_from(rng: &mut Rng, right: &str, cands: &[String], n: usize) -> Vec<String> {
    let mut c: Vec<String> = cands.iter().filter(|x| x.as_str() != right).cloned().collect();
    c.sort();
    c.dedup();
    rng.shuffle(&mut c);
    let mut out: Vec<String> = c.into_iter().take(n).collect();
    out.push(right.to_string());
    rng.shuffle(&mut out);
    out
}

/// Distractors close to the answer: the same lesson first, then the rest of the script, never one that reads the
/// same (じ and ぢ are both "ji").
fn near(target: &Kana, near: &[Kana], all: &[Kana], rng: &mut Rng) -> Vec<Kana> {
    let mut a: Vec<Kana> = near.iter().copied().filter(|k| k.r != target.r).collect();
    rng.shuffle(&mut a);
    let mut b: Vec<Kana> = all.iter().copied().filter(|k| k.r != target.r && !a.contains(k)).collect();
    rng.shuffle(&mut b);
    a.extend(b);
    let mut seen = HashSet::new();
    a.retain(|k| seen.insert(k.r));
    a.truncate(3);
    a
}

/// What the choice cards of a lesson pick from: the script, the characters of the lesson (distractors come from them
/// first), every character, and the sounds that have a file.
struct Pick<'a> {
    script: char,
    lesson: &'a [Kana],
    all: &'a [Kana],
    sounds: &'a HashSet<String>,
}

fn kana_choice(kind: &str, k: &Kana, p: &Pick, rng: &mut Rng, new: bool) -> Option<Challenge> {
    let (script, sounds) = (p.script, p.sounds);
    let others = near(k, p.lesson, p.all, rng);
    if others.len() < 2 {
        return None;
    }
    let ch = k.in_script(script);
    let audio = sound_url(k, sounds);
    let (prompt, answer, cands, prompt_lang, answer_lang): (String, String, Vec<String>, &str, &str) = match kind {
        "kana_romaji" => (ch.clone(), k.r.to_string(), others.iter().map(|o| o.r.to_string()).collect(), "jpn", "romaji"),
        "romaji_kana" => (k.r.to_string(), ch.clone(), others.iter().map(|o| o.in_script(script)).collect(), "romaji", "jpn"),
        _ => {
            if audio.is_empty() {
                return None;
            }
            (String::new(), ch.clone(), others.iter().map(|o| o.in_script(script)).collect(), "jpn", "jpn")
        }
    };
    let options = options_from(rng, &answer, &cands, 3);
    Some(Challenge {
        kind: kind.into(),
        prompt,
        prompt_lang: prompt_lang.into(),
        option_readings: vec![String::new(); options.len()],
        options,
        audio,
        answer,
        answer_lang: answer_lang.into(),
        words: vec![ch],
        new_word: new,
        ..Challenge::default()
    })
}

fn kana_match(items: &[Kana], script: char, rng: &mut Rng) -> Option<Challenge> {
    let mut seen = HashSet::new();
    let mut pick: Vec<Kana> = items.iter().copied().filter(|k| seen.insert(k.r)).collect();
    rng.shuffle(&mut pick);
    pick.truncate(5);
    if pick.len() < 4 {
        return None;
    }
    let pairs: Vec<(String, String)> = pick.iter().map(|k| (k.in_script(script), k.r.to_string())).collect();
    let mut right: Vec<String> = pairs.iter().map(|(_, r)| r.clone()).collect();
    rng.shuffle(&mut right);
    Some(Challenge {
        kind: "match".into(),
        words: pairs.iter().map(|(c, _)| c.clone()).collect(),
        option_readings: vec![String::new(); pairs.len()],
        pairs,
        options: right,
        prompt_lang: "jpn".into(),
        answer_lang: "romaji".into(),
        ..Challenge::default()
    })
}

/// What a characters lesson is made of.
pub struct KanaInput<'a> {
    pub script: char,
    /// The characters it teaches (each gets a first "what does it read" card and a tracing card).
    pub new: Vec<Kana>,
    /// Characters learned before, to go over again.
    pub review: Vec<Kana>,
    pub sounds: &'a HashSet<String>,
    pub max: usize,
    pub seed: u64,
}

/// A characters lesson: a first look at each new character (pick its romaji), then the new and the review ones
/// asked in turns (romaji -> character, listen -> character, character -> romaji), one match-the-pairs and a
/// tracing card or two near the end.
pub fn build_lesson(inp: &KanaInput) -> Vec<Challenge> {
    let mut rng = Rng::new(inp.seed ^ 0x6b61_6e61);
    let script = inp.script;
    let all_set = all();
    let mut lesson_set: Vec<Kana> = inp.new.clone();
    lesson_set.extend(inp.review.iter().copied().filter(|k| !inp.new.contains(k)));
    let max = inp.max.max(4);
    let pick = Pick { script, lesson: &lesson_set, all: &all_set, sounds: inp.sounds };
    let mut intro: Vec<Challenge> = vec![];
    for k in &inp.new {
        if let Some(c) = kana_choice("kana_romaji", k, &pick, &mut rng, true) {
            intro.push(c);
        }
    }
    // every character of the lesson once more, in another way (the new ones twice)
    let mut drill_targets: Vec<Kana> = inp.new.iter().chain(inp.new.iter()).chain(inp.review.iter()).copied().collect();
    rng.shuffle(&mut drill_targets);
    let ways = ["romaji_kana", "kana_listen", "kana_romaji"];
    let mut w = rng.below(ways.len());
    let mut drills: Vec<Challenge> = vec![];
    for k in &drill_targets {
        for step in 0..ways.len() {
            let kind = ways[(w + step) % ways.len()];
            if let Some(c) = kana_choice(kind, k, &pick, &mut rng, false) {
                drills.push(c);
                w = (w + step + 1) % ways.len();
                break;
            }
        }
    }
    let matcher = kana_match(&lesson_set, script, &mut rng);
    let traces: Vec<Challenge> = inp
        .new
        .iter()
        .take(2)
        .map(|k| Challenge {
            kind: "kana_trace".into(),
            prompt: k.in_script(script),
            reading: k.r.to_string(),
            prompt_lang: "jpn".into(),
            audio: sound_url(k, inp.sounds),
            answer: "ok".into(),
            answer_lang: "jpn".into(),
            words: vec![k.in_script(script)],
            ..Challenge::default()
        })
        .collect();
    let tail = usize::from(matcher.is_some()) + traces.len();
    let mut out: Vec<Challenge> = intro;
    out.extend(drills.into_iter().take(max.saturating_sub(out.len() + tail)));
    out.truncate(max.saturating_sub(tail).max(1));
    if let Some(m) = matcher {
        let at = out.len().saturating_sub(1);
        out.insert(at, m);
    }
    out.extend(traces);
    out
}

/// The tracing card is checked by the window (how much of the faint character the strokes covered): it sends
/// "ok" when the drawing is close enough.
pub fn trace_ok(answer: &str) -> bool {
    answer.trim() == "ok"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charts() {
        assert_eq!(all().len(), 46 + 25 + 33);
        assert_eq!(to_katakana("きゃ"), "キャ");
        assert_eq!(find("カ").map(|(k, s)| (k.r, s)), Some(("ka", 'k')));
        assert_eq!(find("を").map(|(k, _)| k.sound()), Some("o"));
        assert!(find("x").is_none());
        // the lessons cover every character once
        let mut n = 0;
        let mut seen = HashSet::new();
        for g in 0..group_count() {
            for k in group(g) {
                assert!(seen.insert(k.h), "{} twice", k.h);
                n += 1;
            }
        }
        assert_eq!(n, all().len());
        assert_eq!(parse_lesson_id(&lesson_id('k', 3)), Some(('k', 3)));
        assert_eq!(parse_lesson_id("kana-h-0"), None);
        assert_eq!(parse_lesson_id("kana-h-99"), None);
        assert_eq!(parse_lesson_id("1-1"), None);
        let j = chart_json('k', &|c| (if c == "ア" { 0.5 } else { 0.0 }, c == "ア"), &HashSet::new());
        assert_eq!(j.at(0).get("rows").at(0).at(0).get("c").str(), "ア");
        assert_eq!(j.at(0).get("rows").at(0).at(0).get("s").as_f64(), Some(0.5));
        assert!(matches!(j.at(0).get("rows").at(7).at(1), Json::Null));
    }

    #[test]
    fn kana_lessons() {
        let sounds: HashSet<String> = ["a", "i", "u", "ka"].iter().map(|s| s.to_string()).collect();
        let mut kinds = HashSet::new();
        for seed in 0..30 {
            let inp = KanaInput { script: 'h', new: group(1), review: group(0), sounds: &sounds, max: 12, seed };
            let ch = build_lesson(&inp);
            assert!(ch.len() >= 8 && ch.len() <= 12, "{}", ch.len());
            // a first look at a new character comes first
            assert!(ch[0].kind == "kana_romaji" && ch[0].new_word);
            for c in &ch {
                kinds.insert(c.kind.clone());
                assert_eq!(c.words.len(), if c.kind == "match" { c.pairs.len() } else { 1 }, "{:?}", c);
                match c.kind.as_str() {
                    "kana_romaji" | "romaji_kana" | "kana_listen" => {
                        if c.kind == "kana_listen" {
                            assert!(c.audio.starts_with("kana:"));
                        }
                        assert!(c.options.contains(&c.answer));
                        assert_eq!(c.options.len(), 4);
                        assert!(c.check(&c.answer).correct);
                        let wrong = c.options.iter().find(|o| **o != c.answer).unwrap();
                        assert!(!c.check(wrong).correct);
                        // no two options read the same
                        let reads: HashSet<String> = c.options.iter().map(|o| find(o).map(|(k, _)| k.r.to_string()).unwrap_or(o.clone())).collect();
                        assert_eq!(reads.len(), 4);
                    }
                    "kana_trace" => {
                        assert!(c.check("ok").correct && !c.check("").correct);
                    }
                    "match" => assert!(c.pairs.len() >= 4),
                    _ => panic!("unexpected {}", c.kind),
                }
            }
        }
        for k in ["kana_romaji", "romaji_kana", "kana_listen", "match", "kana_trace"] {
            assert!(kinds.contains(k), "never made {}", k);
        }
        // katakana, without any sound: no listening
        let none = HashSet::new();
        let ch = build_lesson(&KanaInput { script: 'k', new: group(13), review: vec![], sounds: &none, max: 12, seed: 4 });
        assert!(ch.iter().all(|c| c.kind != "kana_listen"));
        assert!(ch.iter().any(|c| c.words.iter().any(|w| w == "キャ")));
    }
}
