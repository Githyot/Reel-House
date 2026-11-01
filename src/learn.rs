//! Learn: Duolingo-style lessons. Course packs (assets/learn/<lang3>.json, docs/learn-contract.md §1), the lesson
//! generator (six kinds of challenges), answer checking that forgives case, punctuation, accents and kana / kanji,
//! and the game around it (XP, daily goal, streak, hearts, gems, quests, achievements) kept in study/learn.json.
//! Everything here is plain data and functions, so it is tested without the app.
use crate::json::Json;
use crate::knowledge::{is_punct, Knowledge};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[path = "learn_kana.rs"]
pub mod kana;

// ---------------------------------------------------------------- languages

/// The app's 2-letter code to the 3-letter code of the packs ("pt" -> "por").
pub fn lang3(code: &str) -> String {
    let c = code.trim().to_lowercase();
    let base = c.split(['-', '_']).next().unwrap_or("");
    match base {
        "ja" => "jpn",
        "en" => "eng",
        "pt" => "por",
        "es" => "spa",
        "fr" => "fra",
        "de" => "deu",
        "it" => "ita",
        "ko" => "kor",
        "zh" => "cmn",
        "ru" => "rus",
        _ => return base.to_string(),
    }
    .to_string()
}

/// The 3-letter code of the packs to the app's 2-letter code ("por" -> "pt").
pub fn lang2(code: &str) -> String {
    match code.trim() {
        "jpn" => "ja",
        "eng" => "en",
        "por" => "pt",
        "spa" => "es",
        "fra" => "fr",
        "deu" => "de",
        "ita" => "it",
        "kor" => "ko",
        "cmn" => "zh",
        "rus" => "ru",
        other => return other.to_string(),
    }
    .to_string()
}

// ---------------------------------------------------------------- course packs

#[derive(Clone, Debug, Default)]
pub struct Word {
    pub w: String,
    pub r: String,
    pub rank: u32,
    pub pos: String,
    pub tr: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct Sentence {
    pub id: i64,
    pub t: String,
    pub r: String,
    pub tok: Vec<String>,
    /// The dictionary form of each token ("急い" -> "急ぐ"); "" or missing = the token itself.
    pub lem: Vec<String>,
    pub tr: Vec<(String, String)>,
    pub audio: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct Lesson {
    pub id: String,
    pub words: Vec<String>,
    pub sentences: Vec<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct Unit {
    pub id: i64,
    pub title: Vec<(String, String)>,
    pub lessons: Vec<Lesson>,
}

#[derive(Clone, Debug, Default)]
pub struct Pack {
    pub lang: String,
    pub version: i64,
    pub credits: String,
    pub words: Vec<Word>,
    pub sentences: Vec<Sentence>,
    pub units: Vec<Unit>,
    word_idx: HashMap<String, usize>,
    sent_idx: HashMap<i64, usize>,
}

fn tr_map(j: &Json) -> Vec<(String, String)> {
    j.entries().iter().map(|(k, v)| (k.clone(), v.str())).filter(|(_, v)| !v.trim().is_empty()).collect()
}

fn find_tr<'a>(tr: &'a [(String, String)], lang: &str) -> &'a str {
    tr.iter().find(|(l, _)| l == lang).map(|(_, t)| t.as_str()).unwrap_or("")
}

/// The first sense of a translation ("to eat; to have" -> "to eat"): short enough for an option card.
pub fn first_sense(t: &str) -> String {
    t.split([';', '；']).next().unwrap_or("").trim().to_string()
}

impl Word {
    pub fn tr(&self, lang: &str) -> String {
        first_sense(find_tr(&self.tr, lang))
    }
}

impl Sentence {
    pub fn tr(&self, lang: &str) -> &str {
        find_tr(&self.tr, lang)
    }
    pub fn audio_url(&self) -> String {
        self.audio.map(|a| format!("https://tatoeba.org/audio/download/{}", a)).unwrap_or_default()
    }
    /// The tokens that are words (punctuation dropped), as written in the sentence.
    pub fn words(&self) -> Vec<String> {
        self.tok.iter().filter(|t| !is_punct(t)).cloned().collect()
    }
    /// The dictionary form of token `i`.
    pub fn lemma(&self, i: usize) -> &str {
        match self.lem.get(i) {
            Some(l) if !l.is_empty() => l,
            _ => self.tok.get(i).map(|t| t.as_str()).unwrap_or(""),
        }
    }
    /// The dictionary forms of the words of the sentence.
    pub fn lemmas(&self) -> Vec<String> {
        (0..self.tok.len()).filter(|&i| !is_punct(&self.tok[i])).map(|i| self.lemma(i).to_string()).collect()
    }
}

impl Unit {
    pub fn title(&self, lang: &str) -> String {
        for l in [lang, "eng"] {
            let t = find_tr(&self.title, l);
            if !t.is_empty() {
                return t.to_string();
            }
        }
        self.title.first().map(|(_, t)| t.clone()).unwrap_or_else(|| format!("Unit {}", self.id))
    }
}

impl Pack {
    pub fn parse(text: &str) -> Result<Pack, String> {
        let j = Json::parse(text)?;
        let mut p = Pack { lang: j.get("lang").str(), version: j.get("version").as_i64().unwrap_or(1), credits: j.get("credits").str(), ..Pack::default() };
        if p.lang.is_empty() {
            return Err("the course file has no language".into());
        }
        for w in j.get("words").items() {
            let word = Word { w: w.get("w").str(), r: w.get("r").str(), rank: w.get("rank").as_u64().unwrap_or(0) as u32, pos: w.get("pos").str(), tr: tr_map(w.get("tr")) };
            if !word.w.is_empty() {
                p.word_idx.entry(word.w.clone()).or_insert(p.words.len());
                p.words.push(word);
            }
        }
        for s in j.get("sentences").items() {
            let mut sen = Sentence { id: s.get("id").as_i64().unwrap_or(0), t: s.get("t").str(), r: s.get("r").str(), tok: s.get("tok").strings(), lem: s.get("lem").strings(), tr: tr_map(s.get("tr")), audio: s.get("audio").as_i64() };
            if sen.t.is_empty() {
                continue;
            }
            if sen.tok.is_empty() {
                sen.tok = sen.t.split_whitespace().map(|x| x.to_string()).collect();
            }
            p.sent_idx.insert(sen.id, p.sentences.len());
            p.sentences.push(sen);
        }
        for u in j.get("units").items() {
            let lessons = u
                .get("lessons")
                .items()
                .iter()
                .map(|l| Lesson { id: l.get("id").str(), words: l.get("words").strings(), sentences: l.get("sentences").items().iter().filter_map(|x| x.as_i64()).collect() })
                .filter(|l| !l.id.is_empty())
                .collect::<Vec<_>>();
            if !lessons.is_empty() {
                p.units.push(Unit { id: u.get("id").as_i64().unwrap_or(p.units.len() as i64 + 1), title: tr_map(u.get("title")), lessons });
            }
        }
        if p.units.is_empty() {
            return Err("the course file has no lessons".into());
        }
        Ok(p)
    }

    pub fn word(&self, w: &str) -> Option<&Word> {
        self.word_idx.get(w).map(|&i| &self.words[i])
    }

    pub fn sentence(&self, id: i64) -> Option<&Sentence> {
        self.sent_idx.get(&id).map(|&i| &self.sentences[i])
    }

    /// (unit index, lesson index) of a lesson id.
    pub fn find_lesson(&self, id: &str) -> Option<(usize, usize)> {
        self.units.iter().enumerate().find_map(|(u, unit)| unit.lessons.iter().position(|l| l.id == id).map(|i| (u, i)))
    }

    /// Lesson ids in course order.
    pub fn lesson_ids(&self) -> Vec<String> {
        self.units.iter().flat_map(|u| u.lessons.iter().map(|l| l.id.clone())).collect()
    }

    /// Every word introduced up to and including lesson `id` (the words the user may meet in it).
    pub fn words_until(&self, id: &str) -> Vec<String> {
        let mut out = vec![];
        for u in &self.units {
            for l in &u.lessons {
                out.extend(l.words.iter().cloned());
                if l.id == id {
                    return out;
                }
            }
        }
        out
    }

    /// The translation languages this pack has (most common first).
    pub fn translation_langs(&self) -> Vec<String> {
        let mut n: HashMap<&str, usize> = HashMap::new();
        for w in self.words.iter().take(500) {
            for (l, _) in &w.tr {
                *n.entry(l.as_str()).or_default() += 1;
            }
        }
        let mut v: Vec<(&str, usize)> = n.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        v.into_iter().map(|(l, _)| l.to_string()).collect()
    }
}

/// Where the packs are looked for, in order: next to the program, the data folder, the repository (development).
pub fn pack_dirs(data: &Path) -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        v.push(exe.join("learn"));
        // a development build runs from qt/build or target/debug: the repository is a few folders up
        let mut d = exe.clone();
        for _ in 0..4 {
            if let Some(p) = d.parent() {
                d = p.to_path_buf();
                v.push(d.join("assets").join("learn"));
            }
        }
    }
    v.push(data.join("learn"));
    v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("learn"));
    v
}

/// The pack file of a language, when there is one.
pub fn pack_file(data: &Path, lang: &str) -> Option<PathBuf> {
    // the exe folder first, then the data folder, then the repository
    let dirs = pack_dirs(data);
    let order: Vec<&PathBuf> = dirs.iter().take(1).chain(dirs.iter().filter(|d| d.starts_with(data))).chain(dirs.iter().skip(1).filter(|d| !d.starts_with(data))).collect();
    order.into_iter().map(|d| d.join(format!("{}.json", lang))).find(|p| p.is_file())
}

/// Languages that have a pack (3-letter codes, sorted).
pub fn available_courses(data: &Path) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for d in pack_dirs(data) {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(l) = name.strip_suffix(".json") {
                if l.len() == 3 && !out.iter().any(|x| x == l) {
                    out.push(l.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

pub fn load_pack(data: &Path, lang: &str) -> Result<Pack, String> {
    let f = pack_file(data, lang).ok_or_else(|| format!("No course file for {} (learn/{}.json).", lang, lang))?;
    let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {}", f.display(), e))?;
    Pack::parse(&text)
}

// ---------------------------------------------------------------- random numbers

/// A small random generator (xorshift64*), seeded so lessons can be made again in tests.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

// ---------------------------------------------------------------- answer checking

fn fold_char(c: char) -> &'static str {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'ç' | 'ć' | 'č' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
        'ñ' | 'ń' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        'œ' => "oe",
        'æ' => "ae",
        _ => "",
    }
}

/// Text for comparing answers: lower case, punctuation gone, single spaces; katakana as hiragana and full-width
/// letters as plain ones. With `accents` false, accents are dropped too (é -> e). Japanese loses every space.
pub fn fold(s: &str, lang: &str, accents: bool) -> String {
    let mut o = String::new();
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        // full-width ASCII (ａ, １) to plain
        let c = if ('\u{FF01}'..='\u{FF5E}').contains(&c) { char::from_u32(c as u32 - 0xFEE0).unwrap_or(c) } else { c };
        // katakana to hiragana (ア -> あ), the long-vowel mark stays
        let c = if ('\u{30A1}'..='\u{30F6}').contains(&c) { char::from_u32(c as u32 - 0x60).unwrap_or(c) } else { c };
        if c.is_alphanumeric() || c == 'ー' {
            let f = if accents { "" } else { fold_char(c) };
            if f.is_empty() {
                o.push(c);
            } else {
                o.push_str(f);
            }
        } else if c == '\'' || c == '’' {
            // "don't" and "dont" are the same answer
        } else {
            o.push(' ');
        }
    }
    o.split_whitespace().collect::<Vec<_>>().join(if lang == "jpn" { "" } else { " " })
}

/// Edit distance (insert, delete, change one character), stopped early above 2.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > 2 {
        return 3;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// The answer, cut by the tokens of a Japanese sentence where each token may be written in kanji or in kana
/// (its reading): "わたしは学生です" matches 私|は|学生|です with 私 = わたし.
fn matches_tokens(answer: &str, alts: &[Vec<String>]) -> bool {
    let ans: Vec<char> = fold(answer, "jpn", false).chars().collect();
    let mut reach: HashSet<usize> = [0].into_iter().collect();
    for opts in alts {
        let forms: Vec<Vec<char>> = opts.iter().map(|o| fold(o, "jpn", false).chars().collect::<Vec<char>>()).filter(|f| !f.is_empty()).collect();
        if forms.is_empty() {
            continue; // punctuation
        }
        let mut next = HashSet::new();
        for &p in &reach {
            for f in &forms {
                if ans.len() >= p + f.len() && ans[p..p + f.len()] == f[..] {
                    next.insert(p + f.len());
                }
            }
        }
        if next.is_empty() {
            return false;
        }
        reach = next;
    }
    reach.contains(&ans.len())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Verdict {
    pub correct: bool,
    /// "typo" (one letter off), "accents" (right but without the accents) or "".
    pub note: String,
    /// The correct answer to show.
    pub shown: String,
}

// ---------------------------------------------------------------- challenges

/// A word of a sentence prompt, with its meaning for the hover hint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hint {
    pub t: String,
    pub hint: String,
}

#[derive(Clone, Debug, Default)]
pub struct Challenge {
    /// select | arrange | assist | listen | match | fill, and for the characters kana_romaji | romaji_kana |
    /// kana_listen | kana_trace
    pub kind: String,
    /// The text shown (a word or a sentence) and its reading.
    pub prompt: String,
    pub reading: String,
    pub prompt_lang: String,
    /// Words of the prompt with their meanings (sentence prompts in the study language).
    pub hints: Vec<Hint>,
    /// A help line under the prompt (the translation of a fill-in sentence).
    pub help: String,
    /// The choices (select, assist, fill) and their readings.
    pub options: Vec<String>,
    pub option_readings: Vec<String>,
    /// The word bank (arrange, listen).
    pub bank: Vec<String>,
    /// Match: (word, meaning).
    pub pairs: Vec<(String, String)>,
    pub audio: String,
    /// The right answer, and other ways to write it.
    pub answer: String,
    pub alts: Vec<String>,
    /// Japanese: each token of the answer written as itself or as its reading.
    pub token_alts: Vec<Vec<String>>,
    pub answer_lang: String,
    /// Words of the study language the challenge asks about (they go to the knowledge state).
    pub words: Vec<String>,
    pub new_word: bool,
}

impl Challenge {
    /// Checks an answer: the chosen option, the chips of the word bank joined by spaces, or typed text.
    pub fn check(&self, answer: &str) -> Verdict {
        let shown = self.answer.clone();
        let lang = self.answer_lang.as_str();
        match self.kind.as_str() {
            "kana_trace" => Verdict { correct: kana::trace_ok(answer), note: String::new(), shown },
            "select" | "assist" | "fill" | "kana_romaji" | "romaji_kana" | "kana_listen" => {
                let ok = answer.trim() == self.answer.trim() || fold(answer, lang, true) == fold(&self.answer, lang, true);
                Verdict { correct: ok, note: String::new(), shown }
            }
            "match" => Verdict { correct: true, note: String::new(), shown },
            _ => {
                // arrange and listen: the chips or typed text
                let a = fold(answer, lang, true);
                if a.is_empty() {
                    return Verdict { correct: false, note: String::new(), shown };
                }
                let targets: Vec<&String> = std::iter::once(&self.answer).chain(self.alts.iter()).collect();
                if targets.iter().any(|t| fold(t, lang, true) == a) {
                    return Verdict { correct: true, note: String::new(), shown };
                }
                if lang == "jpn" {
                    let ok = !self.token_alts.is_empty() && matches_tokens(answer, &self.token_alts) || targets.iter().any(|t| fold(t, lang, false) == fold(answer, lang, false));
                    return Verdict { correct: ok, note: String::new(), shown };
                }
                let a2 = fold(answer, lang, false);
                if targets.iter().any(|t| fold(t, lang, false) == a2) {
                    return Verdict { correct: true, note: "accents".into(), shown };
                }
                // one letter off in a longer answer is a typo, not a mistake (typed answers only)
                if self.kind == "listen" && a2.chars().count() >= 6 && targets.iter().any(|t| distance(&fold(t, lang, false), &a2) <= 1) {
                    return Verdict { correct: true, note: "typo".into(), shown };
                }
                Verdict { correct: false, note: String::new(), shown }
            }
        }
    }
}

/// Splits a translation into word-bank chips: words with the punctuation around them removed.
pub fn split_words(s: &str) -> Vec<String> {
    s.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’').to_string()).filter(|w| !w.is_empty()).collect()
}

/// A sentence for display from its tokens: Japanese joined, others with spaces and the punctuation attached.
pub fn join_tokens(tok: &[String], lang: &str) -> String {
    if lang == "jpn" {
        return tok.concat();
    }
    let mut o = String::new();
    for t in tok {
        if !o.is_empty() && !(is_punct(t) && !t.starts_with(['¿', '¡', '"', '('])) && !o.ends_with(['¿', '¡', '(']) {
            o.push(' ');
        }
        o.push_str(t);
    }
    o
}

/// What the lesson generator works from.
pub struct LessonInput<'a> {
    pub pack: &'a Pack,
    /// The words the lesson teaches (new words first get a "select the meaning" card).
    pub words: Vec<String>,
    /// Which of them are new to the user.
    pub new_words: HashSet<String>,
    pub sentences: Vec<i64>,
    /// Words the user has met so far (for distractors and match pairs).
    pub pool: Vec<String>,
    /// The translation language.
    pub tl: String,
    pub max: usize,
    pub seed: u64,
}

fn word_reading(pack: &Pack, w: &str) -> String {
    pack.word(w).map(|x| x.r.clone()).filter(|r| r != w).unwrap_or_default()
}

/// Up to `n` translations other than `right` (pool words first, then words of the pack close in rank).
fn distractor_meanings(inp: &LessonInput, rng: &mut Rng, right: &str, n: usize, pos: &str) -> Vec<String> {
    let mut seen: HashSet<String> = [fold(right, &inp.tl, false)].into_iter().collect();
    let mut cands: Vec<String> = vec![];
    let mut pool: Vec<&Word> = inp.pool.iter().filter_map(|w| inp.pack.word(w)).collect();
    rng.shuffle(&mut pool);
    // then the whole pack, same part of speech first
    let mut rest: Vec<&Word> = inp.pack.words.iter().take(3000).collect();
    rng.shuffle(&mut rest);
    rest.sort_by_key(|w| if w.pos == pos { 0 } else { 1 });
    for w in pool.into_iter().chain(rest) {
        let t = w.tr(&inp.tl);
        if t.is_empty() || !seen.insert(fold(&t, &inp.tl, false)) {
            continue;
        }
        cands.push(t);
        if cands.len() >= n {
            break;
        }
    }
    cands
}

/// Up to `n` words of the study language other than `right` (for fill-in options and reversed selects).
fn distractor_words(inp: &LessonInput, rng: &mut Rng, right: &str, n: usize, need_tr: bool) -> Vec<String> {
    let pos = inp.pack.word(right).map(|w| w.pos.clone()).unwrap_or_default();
    let mut seen: HashSet<String> = [right.to_string()].into_iter().collect();
    let mut pool: Vec<String> = inp.pool.clone();
    rng.shuffle(&mut pool);
    let mut rest: Vec<String> = inp.pack.words.iter().take(3000).map(|w| w.w.clone()).collect();
    rng.shuffle(&mut rest);
    let mut all: Vec<String> = pool.into_iter().chain(rest).collect();
    // same part of speech first, so a particle is not offered against a noun
    all.sort_by_key(|w| if inp.pack.word(w).map(|x| x.pos == pos).unwrap_or(false) { 0 } else { 1 });
    let mut out = vec![];
    for w in all {
        if is_punct(&w) || !seen.insert(w.clone()) {
            continue;
        }
        if need_tr && inp.pack.word(&w).map(|x| x.tr(&inp.tl).is_empty()).unwrap_or(true) {
            continue;
        }
        out.push(w);
        if out.len() >= n {
            break;
        }
    }
    out
}

fn hints(inp: &LessonInput, s: &Sentence) -> Vec<Hint> {
    s.tok.iter().enumerate().map(|(i, t)| Hint { t: t.clone(), hint: inp.pack.word(s.lemma(i)).map(|w| w.tr(&inp.tl)).unwrap_or_default() }).collect()
}

/// The words of the course in a sentence (dictionary forms), for the knowledge state.
fn known_words_of(inp: &LessonInput, s: &Sentence) -> Vec<String> {
    let mut seen = HashSet::new();
    s.lemmas().into_iter().filter(|w| inp.pack.word(w).is_some() && seen.insert(w.clone())).collect()
}

fn token_alts(pack: &Pack, s: &Sentence) -> Vec<Vec<String>> {
    s.tok
        .iter()
        .map(|t| {
            let mut v = vec![t.clone()];
            // the reading of a word written as in the dictionary (a conjugated one has none of its own)
            if let Some(w) = pack.word(t) {
                if !w.r.is_empty() {
                    v.push(w.r.clone());
                }
            }
            v
        })
        .collect()
}

fn select_challenge(inp: &LessonInput, rng: &mut Rng, w: &str, new_word: bool) -> Option<Challenge> {
    let word = inp.pack.word(w)?;
    let right = word.tr(&inp.tl);
    if right.is_empty() {
        return None;
    }
    // a word met before is asked the other way round: the meaning, pick the word
    if !new_word && rng.below(2) == 0 {
        let mut opts = distractor_words(inp, rng, w, 3, false);
        if opts.len() < 2 {
            return None;
        }
        opts.push(w.to_string());
        rng.shuffle(&mut opts);
        let readings = opts.iter().map(|o| word_reading(inp.pack, o)).collect();
        return Some(Challenge {
            kind: "select".into(),
            prompt: right,
            prompt_lang: inp.tl.clone(),
            options: opts,
            option_readings: readings,
            answer: w.to_string(),
            answer_lang: inp.pack.lang.clone(),
            words: vec![w.to_string()],
            ..Challenge::default()
        });
    }
    let mut opts = distractor_meanings(inp, rng, &right, 3, &word.pos);
    if opts.len() < 2 {
        return None;
    }
    opts.push(right.clone());
    rng.shuffle(&mut opts);
    Some(Challenge {
        kind: "select".into(),
        prompt: w.to_string(),
        reading: word_reading(inp.pack, w),
        prompt_lang: inp.pack.lang.clone(),
        option_readings: vec![String::new(); opts.len()],
        options: opts,
        answer: right,
        answer_lang: inp.tl.clone(),
        words: vec![w.to_string()],
        new_word,
        ..Challenge::default()
    })
}

fn match_challenge(inp: &LessonInput, rng: &mut Rng) -> Option<Challenge> {
    let mut cands: Vec<String> = inp.words.clone();
    let mut more = inp.pool.clone();
    rng.shuffle(&mut more);
    cands.extend(more);
    let mut pairs: Vec<(String, String)> = vec![];
    let mut used: HashSet<String> = HashSet::new();
    for w in cands {
        if pairs.len() >= 5 || is_punct(&w) || pairs.iter().any(|(x, _)| *x == w) {
            continue;
        }
        let Some(word) = inp.pack.word(&w) else { continue };
        let t = word.tr(&inp.tl);
        if t.is_empty() || !used.insert(fold(&t, &inp.tl, false)) {
            continue;
        }
        pairs.push((w, t));
    }
    if pairs.len() < 4 {
        return None;
    }
    let words = pairs.iter().map(|(w, _)| w.clone()).collect();
    let readings = pairs.iter().map(|(w, _)| word_reading(inp.pack, w)).collect();
    // the right column comes shuffled; the left keeps its order
    let mut right: Vec<String> = pairs.iter().map(|(_, t)| t.clone()).collect();
    rng.shuffle(&mut right);
    Some(Challenge { kind: "match".into(), pairs, options: right, option_readings: readings, answer_lang: inp.tl.clone(), prompt_lang: inp.pack.lang.clone(), words, ..Challenge::default() })
}

/// Chips of other sentences to mix into a word bank.
fn bank_distractors(inp: &LessonInput, rng: &mut Rng, skip: i64, answer: &[String], to_tl: bool, n: usize) -> Vec<String> {
    let have: HashSet<String> = answer.iter().map(|a| a.to_lowercase()).collect();
    let mut ids: Vec<i64> = inp.sentences.iter().copied().filter(|&i| i != skip).collect();
    // sentences of the whole pack too, when the lesson has few
    let mut others: Vec<i64> = inp.pack.sentences.iter().take(400).map(|s| s.id).collect();
    rng.shuffle(&mut others);
    ids.extend(others);
    let mut out: Vec<String> = vec![];
    for id in ids {
        let Some(s) = inp.pack.sentence(id) else { continue };
        let words = if to_tl { split_words(s.tr(&inp.tl)) } else { s.words() };
        for w in words {
            if !have.contains(&w.to_lowercase()) && !out.iter().any(|o| o.to_lowercase() == w.to_lowercase()) {
                out.push(w);
                break; // one per sentence, so they vary
            }
        }
        if out.len() >= n {
            break;
        }
    }
    out
}

fn arrange_challenge(inp: &LessonInput, rng: &mut Rng, s: &Sentence, to_tl: bool) -> Option<Challenge> {
    let tr = s.tr(&inp.tl).to_string();
    if tr.is_empty() {
        return None;
    }
    // a Japanese translation has no tokens to cut it with: then the chips are in the study language
    let to_tl = to_tl && inp.tl != "jpn";
    let (answer_chips, prompt, prompt_lang, answer_lang, answer_text) =
        if to_tl { (split_words(&tr), s.t.clone(), inp.pack.lang.clone(), inp.tl.clone(), tr.clone()) } else { (s.words(), tr.clone(), inp.tl.clone(), inp.pack.lang.clone(), s.t.clone()) };
    if answer_chips.is_empty() || answer_chips.len() > 14 {
        return None;
    }
    let mut bank = answer_chips.clone();
    bank.extend(bank_distractors(inp, rng, s.id, &answer_chips, to_tl, (answer_chips.len() / 2).clamp(2, 4)));
    rng.shuffle(&mut bank);
    Some(Challenge {
        kind: "arrange".into(),
        prompt,
        reading: if to_tl { s.r.clone() } else { String::new() },
        prompt_lang,
        hints: if to_tl { hints(inp, s) } else { vec![] },
        bank,
        audio: if to_tl { s.audio_url() } else { String::new() },
        answer: answer_text,
        token_alts: if to_tl { vec![] } else { token_alts(inp.pack, s) },
        answer_lang,
        words: known_words_of(inp, s),
        ..Challenge::default()
    })
}

fn assist_challenge(inp: &LessonInput, rng: &mut Rng, s: &Sentence) -> Option<Challenge> {
    let right = s.tr(&inp.tl).to_string();
    if right.is_empty() {
        return None;
    }
    let mut opts: Vec<String> = vec![right.clone()];
    let mut ids: Vec<i64> = inp.sentences.iter().copied().filter(|&i| i != s.id).collect();
    let mut more: Vec<i64> = inp.pack.sentences.iter().take(400).map(|x| x.id).collect();
    rng.shuffle(&mut more);
    ids.extend(more);
    for id in ids {
        if let Some(o) = inp.pack.sentence(id).map(|x| x.tr(&inp.tl).to_string()) {
            if !o.is_empty() && !opts.iter().any(|x| fold(x, &inp.tl, false) == fold(&o, &inp.tl, false)) {
                opts.push(o);
            }
        }
        if opts.len() >= 3 {
            break;
        }
    }
    if opts.len() < 2 {
        return None;
    }
    rng.shuffle(&mut opts);
    Some(Challenge {
        kind: "assist".into(),
        prompt: s.t.clone(),
        reading: s.r.clone(),
        prompt_lang: inp.pack.lang.clone(),
        hints: hints(inp, s),
        option_readings: vec![String::new(); opts.len()],
        options: opts,
        audio: s.audio_url(),
        answer: right,
        answer_lang: inp.tl.clone(),
        words: known_words_of(inp, s),
        ..Challenge::default()
    })
}

fn listen_challenge(inp: &LessonInput, rng: &mut Rng, s: &Sentence) -> Option<Challenge> {
    s.audio?;
    let words = s.words();
    if words.is_empty() || words.len() > 14 {
        return None;
    }
    // the word bank is there for those who cannot type the language (Duolingo's "use keyboard" switch)
    let mut bank = words.clone();
    bank.extend(bank_distractors(inp, rng, s.id, &words, false, 3));
    rng.shuffle(&mut bank);
    let mut alts = vec![];
    if !s.r.is_empty() {
        alts.push(s.r.clone());
    }
    Some(Challenge {
        kind: "listen".into(),
        prompt_lang: inp.pack.lang.clone(),
        help: s.tr(&inp.tl).to_string(),
        bank,
        audio: s.audio_url(),
        answer: s.t.clone(),
        alts,
        token_alts: token_alts(inp.pack, s),
        answer_lang: inp.pack.lang.clone(),
        words: known_words_of(inp, s),
        ..Challenge::default()
    })
}

fn fill_challenge(inp: &LessonInput, rng: &mut Rng, s: &Sentence) -> Option<Challenge> {
    // blank a word the lesson teaches (new words first), else any word of the pack in the sentence; only words
    // written in their dictionary form, so the options (dictionary forms) do not give the answer away
    let idx: Vec<usize> = (0..s.tok.len()).filter(|&i| !is_punct(&s.tok[i]) && s.lemma(i) == s.tok[i] && inp.pack.word(&s.tok[i]).is_some()).collect();
    let pick = idx.iter().copied().find(|&i| inp.new_words.contains(&s.tok[i])).or_else(|| idx.iter().copied().find(|&i| inp.words.contains(&s.tok[i]))).or_else(|| idx.first().copied())?;
    let right = s.tok[pick].clone();
    let mut opts = distractor_words(inp, rng, &right, 3, false);
    if opts.len() < 2 {
        return None;
    }
    opts.push(right.clone());
    rng.shuffle(&mut opts);
    let mut tok = s.tok.clone();
    tok[pick] = "____".into();
    let readings = opts.iter().map(|o| word_reading(inp.pack, o)).collect();
    Some(Challenge {
        kind: "fill".into(),
        prompt: join_tokens(&tok, &inp.pack.lang),
        prompt_lang: inp.pack.lang.clone(),
        help: s.tr(&inp.tl).to_string(),
        options: opts,
        option_readings: readings,
        answer: right.clone(),
        answer_lang: inp.pack.lang.clone(),
        words: known_words_of(inp, s),
        ..Challenge::default()
    })
}

/// Makes the challenges of a lesson: a "what does it mean" card for each new word, then the sentences in several
/// ways (translate with the word bank both ways, pick the translation, fill the gap, listen), one match-the-pairs,
/// and more word cards when there are few sentences. New words come before the sentences that use them.
pub fn build_lesson(inp: &LessonInput) -> Vec<Challenge> {
    let mut rng = Rng::new(inp.seed);
    let max = inp.max.max(4);
    let mut word_cards: Vec<Challenge> = vec![];
    for w in &inp.words {
        if let Some(c) = select_challenge(inp, &mut rng, w, inp.new_words.contains(w)) {
            word_cards.push(c);
        }
    }
    let mut sentence_cards: Vec<Challenge> = vec![];
    let mut sents: Vec<&Sentence> = inp.sentences.iter().filter_map(|&id| inp.pack.sentence(id)).collect();
    rng.shuffle(&mut sents);
    // each sentence in the next way of the cycle, so a lesson shows every kind
    let kinds = ["assist", "arrange_tl", "fill", "listen", "arrange_back"];
    let mut k = rng.below(kinds.len());
    for s in &sents {
        let mut made = None;
        for step in 0..kinds.len() {
            let kind = kinds[(k + step) % kinds.len()];
            made = match kind {
                "assist" => assist_challenge(inp, &mut rng, s),
                "arrange_tl" => arrange_challenge(inp, &mut rng, s, true),
                "arrange_back" => arrange_challenge(inp, &mut rng, s, false),
                "fill" => fill_challenge(inp, &mut rng, s),
                _ => listen_challenge(inp, &mut rng, s),
            };
            if made.is_some() {
                k = (k + step + 1) % kinds.len();
                break;
            }
        }
        if let Some(c) = made {
            sentence_cards.push(c);
        }
    }
    let matcher = match_challenge(inp, &mut rng);
    // a second round of sentences, other ways, when the lesson is short
    let mut i = 0;
    while word_cards.len() + sentence_cards.len() + usize::from(matcher.is_some()) < max && i < sents.len() * 2 && !sents.is_empty() {
        let s = sents[i % sents.len()];
        let c = match (i + k) % 3 {
            0 => arrange_challenge(inp, &mut rng, s, i % 2 == 0),
            1 => fill_challenge(inp, &mut rng, s),
            _ => listen_challenge(inp, &mut rng, s),
        };
        if let Some(c) = c {
            sentence_cards.push(c);
        }
        i += 1;
    }
    // reversed word cards fill what is left
    let mut j = 0;
    while word_cards.len() + sentence_cards.len() + usize::from(matcher.is_some()) < max && j < inp.words.len() * 2 {
        if let Some(c) = select_challenge(inp, &mut rng, &inp.words[j % inp.words.len()], false) {
            word_cards.push(c);
        }
        j += 1;
    }
    // order: word cards spread among the sentences (a new word before its first sentence), the match near the end
    let mut out: Vec<Challenge> = vec![];
    let mut wi = word_cards.into_iter();
    let mut si = sentence_cards.into_iter();
    loop {
        let mut any = false;
        for _ in 0..2 {
            if let Some(c) = wi.next() {
                out.push(c);
                any = true;
            }
        }
        if let Some(c) = si.next() {
            out.push(c);
            any = true;
        }
        if !any {
            break;
        }
    }
    out.truncate(if matcher.is_some() { max - 1 } else { max });
    if let Some(m) = matcher {
        let at = out.len().saturating_sub(2).max(out.len().min(3));
        out.insert(at.min(out.len()), m);
    }
    out
}

// ---------------------------------------------------------------- the lesson being played

/// A lesson in progress: the challenges, which come next (missed ones come back at the end) and the score.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// lesson | practice | mistakes | legendary (a finished unit again, harder, for more XP) | kana (characters)
    pub kind: String,
    pub lesson_id: String,
    pub challenges: Vec<Challenge>,
    pub queue: Vec<usize>,
    pub pos: usize,
    pub total: usize,
    pub solved: usize,
    pub mistakes: u32,
    pub combo: u32,
    pub best_combo: u32,
    pub listens: u32,
    pub started: i64,
    pub ended: i64,
    /// question | right | wrong | done | no_hearts
    pub status: String,
    pub verdict: Option<Verdict>,
    pub given: String,
    /// What the finish screen shows.
    pub xp: u32,
    pub gems: u32,
    pub messages: Vec<String>,
}

impl Session {
    pub fn new(kind: &str, lesson_id: &str, challenges: Vec<Challenge>, now: i64) -> Session {
        let n = challenges.len();
        Session { kind: kind.into(), lesson_id: lesson_id.into(), challenges, queue: (0..n).collect(), total: n, started: now, status: "question".into(), ..Session::default() }
    }

    pub fn current(&self) -> Option<&Challenge> {
        self.queue.get(self.pos).and_then(|&i| self.challenges.get(i))
    }

    /// Checks the answer of the current challenge. Returns the verdict (None when nothing is being asked).
    pub fn answer(&mut self, answer: &str) -> Option<Verdict> {
        if self.status != "question" {
            return None;
        }
        let ch = self.current()?;
        let v = ch.check(answer);
        let listen = ch.kind == "listen";
        self.given = answer.to_string();
        if v.correct {
            self.solved += 1;
            self.combo += 1;
            self.best_combo = self.best_combo.max(self.combo);
            if listen {
                self.listens += 1;
            }
            self.status = "right".into();
        } else {
            self.mistakes += 1;
            self.combo = 0;
            self.status = "wrong".into();
        }
        self.verdict = Some(v.clone());
        Some(v)
    }

    /// Goes to the next challenge; a missed one is asked again at the end. Returns true when the lesson is over.
    pub fn advance(&mut self) -> bool {
        if self.status == "wrong" {
            if let Some(&i) = self.queue.get(self.pos) {
                self.queue.push(i);
            }
        }
        if self.status == "right" || self.status == "wrong" {
            self.pos += 1;
        }
        self.verdict = None;
        self.given.clear();
        if self.pos >= self.queue.len() {
            self.status = "done".into();
            true
        } else {
            self.status = "question".into();
            false
        }
    }

    /// 0..1 of the lesson done.
    pub fn progress(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            (self.solved as f64 / self.total as f64).min(1.0)
        }
    }

    /// XP of a finished lesson: 10, +5 with no mistakes, +1 for every 5 right in a row (up to 5). A legendary round
    /// gives 40 and +10 with no mistakes.
    pub fn xp_earned(&self) -> u32 {
        let (base, bonus) = if self.kind == "legendary" { (XP_LEGENDARY, 10) } else { (10, 5) };
        let perfect = if self.mistakes == 0 { bonus } else { 0 };
        base + perfect + (self.best_combo / 5).min(5)
    }

    pub fn accuracy(&self) -> u32 {
        let tries = self.solved as u32 + self.mistakes;
        (self.solved as u32 * 100 + tries / 2).checked_div(tries).unwrap_or(100)
    }
}

// ---------------------------------------------------------------- the game: XP, streak, hearts, gems, quests

pub const MAX_HEARTS: u32 = 5;
/// A heart comes back every 5 hours.
pub const HEART_SECS: i64 = 5 * 3600;
pub const PRICE_REFILL: u32 = 60;
pub const PRICE_FREEZE: u32 = 50;
pub const MAX_FREEZES: u32 = 2;
pub const GOALS: [u32; 4] = [10, 20, 30, 50];
/// Gems for every finished lesson or practice, and more when it had no mistakes.
pub const GEMS_LESSON: u32 = 5;
pub const GEMS_PERFECT: u32 = 3;
/// Gems of an achievement.
pub const GEMS_ACHIEVEMENT: u32 = 20;
/// Gems of the daily quests (each quest gives one of these).
pub const QUEST_GEMS: [u32; 3] = [10, 15, 20];
/// Streak milestones: (days in a row, gems).
pub const STREAK_MILESTONES: [(u32, u32); 3] = [(7, 25), (30, 50), (100, 100)];
/// The daily chest, opened once the daily goal is reached: gems between these, and sometimes an XP boost.
pub const CHEST_MIN: u32 = 5;
pub const CHEST_MAX: u32 = 20;
/// One chest in this many also holds an XP boost.
pub const CHEST_BOOST_ONE_IN: usize = 4;
/// The XP boost: double XP for 15 minutes.
pub const BOOST_SECS: i64 = 15 * 60;
pub const PRICE_BOOST: u32 = 80;
/// XP of a legendary round of a finished unit.
pub const XP_LEGENDARY: u32 = 40;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quest {
    /// xp | lessons | perfect | listen | combo | goal | practice
    pub kind: String,
    pub target: u32,
    pub progress: u32,
    pub reward: u32,
    pub done: bool,
}

/// What is kept in study/learn.json.
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    /// The course chosen here (3-letter code) when the settings name none.
    pub course: String,
    /// Finished lessons per course.
    pub done: Vec<(String, Vec<String>)>,
    pub xp_total: u32,
    /// XP per day (day number, XP), oldest first.
    pub days: Vec<(i64, u32)>,
    pub streak: u32,
    /// The last day the streak counted (a lesson, or a freeze).
    pub streak_day: i64,
    pub longest: u32,
    pub freezes: u32,
    /// Days a streak freeze saved.
    pub frozen: Vec<i64>,
    pub hearts: u32,
    /// When the heart clock started (refills count from here).
    pub hearts_at: i64,
    pub gems: u32,
    pub goal: u32,
    pub quest_day: i64,
    pub quests: Vec<Quest>,
    /// (achievement id, unix time it was earned)
    pub achievements: Vec<(String, i64)>,
    /// Words answered wrong, for the "mistakes" review.
    pub mistakes: Vec<String>,
    pub lessons: u32,
    pub perfect: u32,
    pub listens: u32,
    pub practices: u32,
    pub best_combo: u32,
    /// The local time zone offset in minutes (from the window), for where a day starts.
    pub tz: i64,
    /// The day the daily chest was opened.
    pub chest_day: i64,
    /// Double XP until this unix time.
    pub boost_until: i64,
    /// Units played as legendary, per course (unit ids).
    pub legendary: Vec<(String, Vec<i64>)>,
}

impl Default for Progress {
    fn default() -> Progress {
        Progress {
            course: String::new(),
            done: vec![],
            xp_total: 0,
            days: vec![],
            streak: 0,
            streak_day: 0,
            longest: 0,
            freezes: 0,
            frozen: vec![],
            hearts: MAX_HEARTS,
            hearts_at: 0,
            gems: 50,
            goal: 20,
            quest_day: 0,
            quests: vec![],
            achievements: vec![],
            mistakes: vec![],
            lessons: 0,
            perfect: 0,
            listens: 0,
            practices: 0,
            best_combo: 0,
            tz: 0,
            chest_day: 0,
            boost_until: 0,
            legendary: vec![],
        }
    }
}

/// The day number of a moment in the user's time zone.
pub fn day_of(now: i64, tz_minutes: i64) -> i64 {
    (now + tz_minutes * 60).div_euclid(86400)
}

/// The week (Monday first) of a day number; 1970-01-01 was a Thursday.
pub fn week_of(day: i64) -> i64 {
    (day + 3).div_euclid(7)
}

/// Achievements: (id, name, what is counted, target).
pub const ACHIEVEMENTS: &[(&str, &str, &str, u32)] = &[
    ("first_lesson", "First steps", "lessons", 1),
    ("lessons_25", "Committed", "lessons", 25),
    ("streak_7", "Week warrior", "streak", 7),
    ("streak_30", "Monthly habit", "streak", 30),
    ("words_100", "Wordsmith", "words", 100),
    ("perfect_10", "Perfectionist", "perfect", 10),
    ("xp_1000", "Scholar", "xp", 1000),
    ("listen_50", "Good ears", "listens", 50),
    ("unit_1", "Unit complete", "units", 1),
];

fn n(v: i64) -> Json {
    Json::Num(v as f64)
}

impl Progress {
    pub fn load(path: &Path) -> Progress {
        std::fs::read_to_string(path).ok().and_then(|t| Json::parse(&t).ok()).map(|j| Progress::from_json(&j)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, self.to_json().pretty()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    pub fn from_json(j: &Json) -> Progress {
        let d = Progress::default();
        let u = |k: &str, def: u32| j.get(k).as_u64().map(|x| x as u32).unwrap_or(def);
        let i = |k: &str| j.get(k).as_i64().unwrap_or(0);
        Progress {
            course: j.get("course").str(),
            done: j.get("done").entries().iter().map(|(k, v)| (k.clone(), v.strings())).collect(),
            xp_total: u("xp_total", 0),
            days: j.get("days").items().iter().map(|x| (x.at(0).as_i64().unwrap_or(0), x.at(1).as_u64().unwrap_or(0) as u32)).collect(),
            streak: u("streak", 0),
            streak_day: i("streak_day"),
            longest: u("longest", 0),
            freezes: u("freezes", 0),
            frozen: j.get("frozen").items().iter().filter_map(|x| x.as_i64()).collect(),
            hearts: u("hearts", d.hearts).min(MAX_HEARTS),
            hearts_at: i("hearts_at"),
            gems: u("gems", d.gems),
            goal: u("goal", d.goal),
            quest_day: i("quest_day"),
            quests: j
                .get("quests")
                .items()
                .iter()
                .map(|q| Quest { kind: q.get("kind").str(), target: q.get("target").as_u64().unwrap_or(1) as u32, progress: q.get("progress").as_u64().unwrap_or(0) as u32, reward: q.get("reward").as_u64().unwrap_or(10) as u32, done: q.get("done").as_bool().unwrap_or(false) })
                .collect(),
            achievements: j.get("achievements").items().iter().map(|x| (x.at(0).str(), x.at(1).as_i64().unwrap_or(0))).collect(),
            mistakes: j.get("mistakes").strings(),
            lessons: u("lessons", 0),
            perfect: u("perfect", 0),
            listens: u("listens", 0),
            practices: u("practices", 0),
            best_combo: u("best_combo", 0),
            tz: i("tz"),
            chest_day: i("chest_day"),
            boost_until: i("boost_until"),
            legendary: j.get("legendary").entries().iter().map(|(k, v)| (k.clone(), v.items().iter().filter_map(|x| x.as_i64()).collect())).collect(),
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("course", self.course.as_str().into()),
            ("done", Json::Obj(self.done.iter().map(|(k, v)| (k.clone(), Json::from(v.clone()))).collect())),
            ("xp_total", self.xp_total.into()),
            ("days", Json::Arr(self.days.iter().map(|(d, x)| Json::Arr(vec![n(*d), n(*x as i64)])).collect())),
            ("streak", self.streak.into()),
            ("streak_day", n(self.streak_day)),
            ("longest", self.longest.into()),
            ("freezes", self.freezes.into()),
            ("frozen", Json::Arr(self.frozen.iter().map(|d| n(*d)).collect())),
            ("hearts", self.hearts.into()),
            ("hearts_at", n(self.hearts_at)),
            ("gems", self.gems.into()),
            ("goal", self.goal.into()),
            ("quest_day", n(self.quest_day)),
            (
                "quests",
                Json::Arr(
                    self.quests
                        .iter()
                        .map(|q| Json::obj(vec![("kind", q.kind.as_str().into()), ("target", q.target.into()), ("progress", q.progress.into()), ("reward", q.reward.into()), ("done", q.done.into())]))
                        .collect(),
                ),
            ),
            ("achievements", Json::Arr(self.achievements.iter().map(|(a, t)| Json::Arr(vec![a.as_str().into(), n(*t)])).collect())),
            ("mistakes", Json::from(self.mistakes.clone())),
            ("lessons", self.lessons.into()),
            ("perfect", self.perfect.into()),
            ("listens", self.listens.into()),
            ("practices", self.practices.into()),
            ("best_combo", self.best_combo.into()),
            ("tz", n(self.tz)),
            ("chest_day", n(self.chest_day)),
            ("boost_until", n(self.boost_until)),
            ("legendary", Json::Obj(self.legendary.iter().map(|(k, v)| (k.clone(), Json::Arr(v.iter().map(|u| n(*u)).collect()))).collect())),
        ])
    }

    pub fn today(&self, now: i64) -> i64 {
        day_of(now, self.tz)
    }

    pub fn done_in(&self, course: &str) -> &[String] {
        self.done.iter().find(|(c, _)| c == course).map(|(_, v)| v.as_slice()).unwrap_or(&[])
    }

    pub fn mark_done(&mut self, course: &str, lesson: &str) {
        if let Some(e) = self.done.iter_mut().find(|(c, _)| c == course) {
            if !e.1.iter().any(|l| l == lesson) {
                e.1.push(lesson.to_string());
            }
        } else {
            self.done.push((course.to_string(), vec![lesson.to_string()]));
        }
    }

    /// Settings > Study > "Where to start in Learn": the course picks up at `lesson`. Every lesson before it counts as
    /// finished and the ones from it on as not, so the path shows it as the current lesson. For someone who lost their
    /// progress and knows where they were. Returns how many lessons now count as finished (None: no such lesson).
    pub fn start_at(&mut self, pack: &Pack, lesson: &str) -> Option<usize> {
        let ids = pack.lesson_ids();
        let at = ids.iter().position(|id| id == lesson)?;
        self.done.retain(|(c, _)| *c != pack.lang);
        if at > 0 {
            self.done.push((pack.lang.clone(), ids[..at].to_vec()));
        }
        Some(at)
    }

    pub fn xp_on(&self, day: i64) -> u32 {
        self.days.iter().find(|(d, _)| *d == day).map(|(_, x)| *x).unwrap_or(0)
    }

    /// Hearts that came back with time.
    pub fn refill(&mut self, now: i64) {
        if self.hearts >= MAX_HEARTS {
            self.hearts = MAX_HEARTS;
            return;
        }
        if self.hearts_at <= 0 {
            self.hearts_at = now;
        }
        while self.hearts < MAX_HEARTS && now - self.hearts_at >= HEART_SECS {
            self.hearts += 1;
            self.hearts_at += HEART_SECS;
        }
        if self.hearts >= MAX_HEARTS {
            self.hearts_at = now;
        }
    }

    /// Seconds until the next heart comes back (0 when full).
    pub fn next_heart_in(&self, now: i64) -> i64 {
        if self.hearts >= MAX_HEARTS {
            0
        } else {
            (HEART_SECS - (now - self.hearts_at)).max(0)
        }
    }

    pub fn lose_heart(&mut self, now: i64) {
        self.refill(now);
        if self.hearts == MAX_HEARTS {
            self.hearts_at = now;
        }
        self.hearts = self.hearts.saturating_sub(1);
    }

    pub fn gain_heart(&mut self, now: i64) {
        self.refill(now);
        self.hearts = (self.hearts + 1).min(MAX_HEARTS);
    }

    /// A new day: the streak breaks when a day was missed (a streak freeze covers one missed day each), and new
    /// quests are drawn.
    pub fn roll(&mut self, now: i64) {
        let today = self.today(now);
        if self.streak > 0 && self.streak_day < today - 1 {
            let missed = (today - 1 - self.streak_day) as u32;
            if self.freezes >= missed {
                self.freezes -= missed;
                for d in self.streak_day + 1..today {
                    self.frozen.push(d);
                }
                self.streak_day = today - 1;
            } else {
                self.streak = 0;
            }
        }
        if self.quest_day != today {
            self.quest_day = today;
            self.quests = daily_quests(today, self.goal);
        }
        self.days.retain(|(d, _)| *d > today - 120);
        self.frozen.retain(|d| *d > today - 120);
    }

    fn quest_event(&mut self, kind: &str, amount: u32, set: bool) -> u32 {
        let mut gems = 0;
        for q in self.quests.iter_mut().filter(|q| q.kind == kind && !q.done) {
            q.progress = if set { q.progress.max(amount) } else { q.progress + amount };
            if q.progress >= q.target {
                q.progress = q.target;
                q.done = true;
                gems += q.reward;
            }
        }
        gems
    }

    /// Adds a finished lesson: XP, the day's total, the streak, quests, gems. `words` is how many words are known
    /// (for achievements), `units` how many units are finished. Returns the messages for the finish screen and the
    /// gems won.
    pub fn finish(&mut self, s: &Session, now: i64, words: usize, units: usize) -> (u32, u32, Vec<String>) {
        self.roll(now);
        let today = self.today(now);
        let mut xp = s.xp_earned();
        let mut msgs = vec![];
        // the XP boost doubles what the lesson gives
        if now < self.boost_until {
            xp *= 2;
            msgs.push("boost".into());
        }
        let before = self.xp_on(today);
        match self.days.iter_mut().find(|(d, _)| *d == today) {
            Some(e) => e.1 += xp,
            None => self.days.push((today, xp)),
        }
        self.xp_total += xp;
        let mut gems = GEMS_LESSON + if s.mistakes == 0 { GEMS_PERFECT } else { 0 };
        if self.streak_day != today {
            self.streak = if self.streak_day == today - 1 { self.streak + 1 } else { 1 };
            self.streak_day = today;
            self.longest = self.longest.max(self.streak);
            msgs.push(format!("streak:{}", self.streak));
            // the streak grows once a day, so a milestone is reached once per streak
            if let Some((days, reward)) = STREAK_MILESTONES.iter().find(|(d, _)| *d == self.streak) {
                gems += reward;
                msgs.push(format!("milestone:{}", days));
            }
        }
        self.lessons += 1;
        if s.mistakes == 0 {
            self.perfect += 1;
        }
        self.listens += s.listens;
        self.best_combo = self.best_combo.max(s.best_combo);
        if s.kind == "practice" || s.kind == "mistakes" {
            self.practices += 1;
            gems += self.quest_event("practice", 1, false);
        }
        gems += self.quest_event("xp", xp, false);
        gems += self.quest_event("lessons", 1, false);
        if s.mistakes == 0 {
            gems += self.quest_event("perfect", 1, false);
        }
        gems += self.quest_event("listen", s.listens, false);
        gems += self.quest_event("combo", s.best_combo, true);
        let after = before + xp;
        if before < self.goal && after >= self.goal {
            msgs.push("goal".into());
            if self.chest_day != today {
                msgs.push("chest".into());
            }
            gems += self.quest_event("goal", 1, false);
        }
        for (id, _, what, target) in ACHIEVEMENTS {
            if self.achievements.iter().any(|(a, _)| a == id) {
                continue;
            }
            let have = match *what {
                "lessons" => self.lessons,
                "streak" => self.longest,
                "words" => words as u32,
                "perfect" => self.perfect,
                "xp" => self.xp_total,
                "listens" => self.listens,
                "units" => units as u32,
                _ => 0,
            };
            if have >= *target {
                self.achievements.push((id.to_string(), now));
                gems += GEMS_ACHIEVEMENT;
                msgs.push(format!("achievement:{}", id));
            }
        }
        self.gems += gems;
        (xp, gems, msgs)
    }

    /// Buys an item of the shop. Err holds the reason it cannot be bought.
    pub fn buy(&mut self, item: &str, now: i64) -> Result<(), String> {
        self.refill(now);
        let (price, ok, why) = match item {
            "refill" => (PRICE_REFILL, self.hearts < MAX_HEARTS, "Your hearts are already full."),
            "freeze" => (PRICE_FREEZE, self.freezes < MAX_FREEZES, "You already have the most streak freezes."),
            "boost" => (PRICE_BOOST, true, ""),
            _ => return Err("Unknown item.".into()),
        };
        if !ok {
            return Err(why.into());
        }
        if self.gems < price {
            return Err("Not enough gems.".into());
        }
        self.gems -= price;
        match item {
            "refill" => {
                self.hearts = MAX_HEARTS;
                self.hearts_at = now;
            }
            "boost" => self.add_boost(now),
            _ => self.freezes += 1,
        }
        Ok(())
    }

    /// XP earned outside a lesson (a flashcard review in Study): it counts for the day, the daily goal, the XP quest
    /// and the streak just as a lesson's XP does (the boost doubles it too). Returns the XP really added and the same
    /// messages as `finish` ("streak:N", "milestone:N", "goal", "chest") for the banners; streak milestone gems are
    /// given here.
    pub fn add_xp(&mut self, xp: u32, now: i64) -> (u32, Vec<String>) {
        if xp == 0 {
            return (0, vec![]);
        }
        self.roll(now);
        let today = self.today(now);
        let xp = if now < self.boost_until { xp * 2 } else { xp };
        let mut msgs = vec![];
        let before = self.xp_on(today);
        match self.days.iter_mut().find(|(d, _)| *d == today) {
            Some(e) => e.1 += xp,
            None => self.days.push((today, xp)),
        }
        self.xp_total += xp;
        let mut gems = 0;
        if self.streak_day != today {
            self.streak = if self.streak_day == today - 1 { self.streak + 1 } else { 1 };
            self.streak_day = today;
            self.longest = self.longest.max(self.streak);
            msgs.push(format!("streak:{}", self.streak));
            if let Some((days, reward)) = STREAK_MILESTONES.iter().find(|(d, _)| *d == self.streak) {
                gems += reward;
                msgs.push(format!("milestone:{}", days));
            }
        }
        gems += self.quest_event("xp", xp, false);
        if before < self.goal && before + xp >= self.goal {
            msgs.push("goal".into());
            if self.chest_day != today {
                msgs.push("chest".into());
            }
            gems += self.quest_event("goal", 1, false);
        }
        self.gems += gems;
        (xp, msgs)
    }

    /// Takes back XP given by `add_xp` today (an undone flashcard answer). The streak and quests stay as they are.
    pub fn remove_xp(&mut self, xp: u32, now: i64) {
        let today = self.today(now);
        if let Some(e) = self.days.iter_mut().find(|(d, _)| *d == today) {
            e.1 = e.1.saturating_sub(xp);
        }
        self.xp_total = self.xp_total.saturating_sub(xp);
    }

    /// Fifteen more minutes of double XP (from now, or after the boost already running).
    pub fn add_boost(&mut self, now: i64) {
        self.boost_until = self.boost_until.max(now) + BOOST_SECS;
    }

    /// Seconds of XP boost left.
    pub fn boost_left(&self, now: i64) -> i64 {
        (self.boost_until - now).max(0)
    }

    /// The daily chest can be opened: the goal of today is reached and it was not opened today.
    pub fn chest_ready(&self, now: i64) -> bool {
        let today = self.today(now);
        self.chest_day != today && self.xp_on(today) >= self.goal
    }

    /// Opens the daily chest: gems between CHEST_MIN and CHEST_MAX, and one time in CHEST_BOOST_ONE_IN an XP boost
    /// too. Returns (gems, boost).
    pub fn open_chest(&mut self, now: i64, seed: u64) -> Result<(u32, bool), String> {
        self.roll(now);
        if self.chest_day == self.today(now) {
            return Err("You already opened today's chest. Come back tomorrow!".into());
        }
        if !self.chest_ready(now) {
            return Err("Reach your daily goal to open the chest.".into());
        }
        let mut rng = Rng::new(seed);
        let gems = CHEST_MIN + rng.below((CHEST_MAX - CHEST_MIN + 1) as usize) as u32;
        let boost = rng.below(CHEST_BOOST_ONE_IN) == 0;
        self.gems += gems;
        if boost {
            self.add_boost(now);
        }
        self.chest_day = self.today(now);
        Ok((gems, boost))
    }

    pub fn is_legendary(&self, course: &str, unit: i64) -> bool {
        self.legendary.iter().any(|(c, v)| c == course && v.contains(&unit))
    }

    pub fn mark_legendary(&mut self, course: &str, unit: i64) {
        match self.legendary.iter_mut().find(|(c, _)| c == course) {
            Some(e) => {
                if !e.1.contains(&unit) {
                    e.1.push(unit);
                }
            }
            None => self.legendary.push((course.to_string(), vec![unit])),
        }
    }

    /// XP of the last `count` weeks, oldest first (the last one is this week).
    pub fn weeks(&self, now: i64, count: usize) -> Vec<u32> {
        let w = week_of(self.today(now));
        (0..count as i64).rev().map(|k| self.days.iter().filter(|(d, _)| week_of(*d) == w - k).map(|(_, x)| *x).sum()).collect()
    }

    /// XP of each day of this week, Monday first.
    pub fn week_days(&self, now: i64) -> Vec<u32> {
        let today = self.today(now);
        let monday = week_of(today) * 7 - 3;
        (0..7).map(|i| self.xp_on(monday + i)).collect()
    }

    pub fn add_mistakes(&mut self, words: &[String]) {
        for w in words {
            if !self.mistakes.contains(w) {
                self.mistakes.push(w.clone());
            }
        }
        if self.mistakes.len() > 200 {
            let cut = self.mistakes.len() - 200;
            self.mistakes.drain(..cut);
        }
    }
}

/// Three quests of a day, drawn from the pool with the day as the seed (always one about XP).
pub fn daily_quests(day: i64, goal: u32) -> Vec<Quest> {
    let mut rng = Rng::new(day as u64 ^ 0x51_7cc1_b727_220a);
    let [low, mid, high] = QUEST_GEMS;
    let xp_target = [20, 30, 50][rng.below(3)].max(goal);
    let mut out = vec![Quest { kind: "xp".into(), target: xp_target, progress: 0, reward: if xp_target >= 50 { high } else { low }, done: false }];
    let mut pool: Vec<Quest> = vec![
        Quest { kind: "lessons".into(), target: 2 + rng.below(2) as u32, reward: low, ..Quest::default() },
        Quest { kind: "perfect".into(), target: 1 + rng.below(2) as u32, reward: mid, ..Quest::default() },
        Quest { kind: "listen".into(), target: 3 + rng.below(3) as u32, reward: low, ..Quest::default() },
        Quest { kind: "combo".into(), target: [5, 10][rng.below(2)], reward: mid, ..Quest::default() },
        Quest { kind: "goal".into(), target: 1, reward: low, ..Quest::default() },
        Quest { kind: "practice".into(), target: 1, reward: low, ..Quest::default() },
    ];
    rng.shuffle(&mut pool);
    out.extend(pool.into_iter().take(2));
    out
}

/// How gems are earned, with the real numbers, for the shop's explanation.
pub fn gem_rules_json() -> Json {
    Json::obj(vec![
        ("lesson", GEMS_LESSON.into()),
        ("perfect", GEMS_PERFECT.into()),
        ("quest_min", QUEST_GEMS[0].into()),
        ("quest_max", QUEST_GEMS[2].into()),
        ("achievement", GEMS_ACHIEVEMENT.into()),
        ("milestones", Json::Arr(STREAK_MILESTONES.iter().map(|(d, g)| Json::obj(vec![("days", (*d).into()), ("gems", (*g).into())])).collect())),
        ("chest_min", CHEST_MIN.into()),
        ("chest_max", CHEST_MAX.into()),
        ("boost_minutes", ((BOOST_SECS / 60) as u32).into()),
        ("price_boost", PRICE_BOOST.into()),
        ("xp_legendary", XP_LEGENDARY.into()),
    ])
}

/// The level of a unit for its badge: 0 not finished, 1 finished, 2 legendary.
pub fn unit_level(finished: bool, legendary: bool) -> u32 {
    if !finished {
        0
    } else if legendary {
        2
    } else {
        1
    }
}

/// Builds the knowledge words of a pack for tokenising (word, part of speech).
pub fn dictionary(pack: &Pack) -> Vec<(String, String)> {
    pack.words.iter().map(|w| (w.w.clone(), w.pos.clone())).collect()
}

/// Written forms of the words found in the pack's sentences and their dictionary forms ("急い" -> "急ぐ").
pub fn forms(pack: &Pack) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    for s in &pack.sentences {
        for (i, t) in s.tok.iter().enumerate() {
            let l = s.lemma(i);
            if l != t && !is_punct(t) && pack.word(l).is_some() && seen.insert(t.clone()) {
                out.push((t.clone(), l.to_string()));
            }
        }
    }
    out
}

/// Sentences of the pack that use one of `words` and only words of `allowed` (plus punctuation), up to `max`.
pub fn sentences_with(pack: &Pack, words: &[String], allowed: &HashSet<String>, max: usize) -> Vec<i64> {
    let want: HashSet<&String> = words.iter().collect();
    let mut out = vec![];
    for s in &pack.sentences {
        let ws = s.lemmas();
        if ws.iter().any(|w| want.contains(w)) && ws.iter().all(|w| allowed.contains(w) || pack.word(w).is_none()) {
            out.push(s.id);
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

/// The words of a practice: the weakest known words of the course (by the knowledge state).
pub fn practice_words(k: &Knowledge, pack: &Pack, learned: &[String], n: usize, now: i64) -> Vec<String> {
    let mut v: Vec<(f64, &String)> = learned.iter().filter(|w| pack.word(w).is_some() && !is_punct(w)).map(|w| (k.strength(&pack.lang, w, now), w)).collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen = HashSet::new();
    v.into_iter().filter(|(_, w)| seen.insert(w.to_string())).take(n).map(|(_, w)| w.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny Japanese course: enough words and sentences for every kind of challenge.
    pub const SAMPLE: &str = r#"{
      "lang": "jpn", "version": 1, "credits": "test",
      "words": [
        {"w": "私", "r": "わたし", "rank": 1, "pos": "pron", "tr": {"eng": "I", "por": "eu", "spa": "yo"}},
        {"w": "は", "r": "は", "rank": 2, "pos": "particle", "tr": {"eng": "(topic)", "por": "(tópico)", "spa": "(tema)"}},
        {"w": "学生", "r": "がくせい", "rank": 3, "pos": "noun", "tr": {"eng": "student", "por": "estudante", "spa": "estudiante"}},
        {"w": "です", "r": "です", "rank": 4, "pos": "aux", "tr": {"eng": "to be", "por": "ser", "spa": "ser"}},
        {"w": "先生", "r": "せんせい", "rank": 5, "pos": "noun", "tr": {"eng": "teacher", "por": "professor", "spa": "profesor"}},
        {"w": "猫", "r": "ねこ", "rank": 6, "pos": "noun", "tr": {"eng": "cat", "por": "gato", "spa": "gato"}},
        {"w": "犬", "r": "いぬ", "rank": 7, "pos": "noun", "tr": {"eng": "dog", "por": "cachorro", "spa": "perro"}},
        {"w": "食べる", "r": "たべる", "rank": 8, "pos": "verb", "tr": {"eng": "to eat; to have", "por": "comer", "spa": "comer"}}
      ],
      "sentences": [
        {"id": 4707, "t": "私は学生です。", "r": "わたしはがくせいです。", "tok": ["私", "は", "学生", "です", "。"],
         "tr": {"eng": "I am a student.", "por": "Eu sou estudante.", "spa": "Soy estudiante."}, "audio": 12345},
        {"id": 2032, "t": "私は先生です。", "r": "わたしはせんせいです。", "tok": ["私", "は", "先生", "です", "。"],
         "tr": {"eng": "I am a teacher.", "por": "Eu sou professor.", "spa": "Soy profesor."}},
        {"id": 3000, "t": "猫です。", "r": "ねこです。", "tok": ["猫", "です", "。"],
         "tr": {"eng": "It is a cat.", "por": "É um gato.", "spa": "Es un gato."}, "audio": 777}
      ],
      "units": [
        {"id": 1, "title": {"eng": "Basics 1", "por": "Básico 1", "spa": "Básico 1", "jpn": "きほん 1"},
         "lessons": [ {"id": "1-1", "words": ["私", "は", "学生", "です"], "sentences": [4707]},
                      {"id": "1-2", "words": ["先生", "猫", "犬"], "sentences": [2032, 3000]} ]},
        {"id": 2, "title": {"eng": "Food"}, "lessons": [ {"id": "2-1", "words": ["食べる"], "sentences": []} ]}
      ]
    }"#;

    fn pack() -> Pack {
        Pack::parse(SAMPLE).unwrap()
    }

    #[test]
    fn languages() {
        assert_eq!(lang3("pt"), "por");
        assert_eq!(lang3("pt-BR"), "por");
        assert_eq!(lang2("jpn"), "ja");
        assert_eq!(lang2(&lang3("es")), "es");
    }

    #[test]
    fn loads_pack() {
        let p = pack();
        assert_eq!(p.lang, "jpn");
        assert_eq!(p.words.len(), 8);
        assert_eq!(p.units.len(), 2);
        assert_eq!(p.units[0].title("por"), "Básico 1");
        assert_eq!(p.units[1].title("por"), "Food");
        assert_eq!(p.word("食べる").unwrap().tr("eng"), "to eat");
        assert_eq!(p.sentence(4707).unwrap().audio_url(), "https://tatoeba.org/audio/download/12345");
        assert_eq!(p.find_lesson("1-2"), Some((0, 1)));
        assert_eq!(p.words_until("1-1").len(), 4);
        assert!(Pack::parse("{\"lang\": \"jpn\"}").is_err());
        assert_eq!(join_tokens(&["I".into(), "am".into(), "here".into(), ".".into()], "eng"), "I am here.");
    }

    #[test]
    fn the_path_can_start_where_the_user_was() {
        let p = pack();
        let mut g = Progress::default();
        g.mark_done("jpn", "1-1");
        g.mark_done("eng", "x-1");
        // lost progress: start at the first lesson of unit 2
        assert_eq!(g.start_at(&p, "2-1"), Some(2));
        assert_eq!(g.done_in("jpn"), ["1-1".to_string(), "1-2".to_string()]);
        // back to an earlier lesson: the ones from it on count as not done
        assert_eq!(g.start_at(&p, "1-2"), Some(1));
        assert_eq!(g.done_in("jpn"), ["1-1".to_string()]);
        assert_eq!(g.start_at(&p, "1-1"), Some(0));
        assert!(g.done_in("jpn").is_empty());
        // other courses are left alone; a lesson not in the course changes nothing
        assert_eq!(g.done_in("eng"), ["x-1".to_string()]);
        assert_eq!(g.start_at(&p, "9-9"), None);
        // it is kept in learn.json like any progress
        g.start_at(&p, "2-1");
        assert_eq!(Progress::from_json(&g.to_json()).done_in("jpn").len(), 2);
    }

    fn input<'a>(p: &'a Pack, lesson: &str, tl: &str, seed: u64) -> LessonInput<'a> {
        let (u, l) = p.find_lesson(lesson).unwrap();
        let les = &p.units[u].lessons[l];
        LessonInput { pack: p, words: les.words.clone(), new_words: les.words.iter().cloned().collect(), sentences: les.sentences.clone(), pool: p.words_until(lesson), tl: tl.into(), max: 12, seed }
    }

    #[test]
    fn generates_every_kind() {
        let p = pack();
        let mut kinds: HashSet<String> = HashSet::new();
        for seed in 0..40 {
            let ch = build_lesson(&input(&p, "1-2", "por", seed));
            assert!(!ch.is_empty() && ch.len() <= 12);
            for c in &ch {
                kinds.insert(c.kind.clone());
                match c.kind.as_str() {
                    "select" | "assist" | "fill" => {
                        assert!(c.options.len() >= 3, "{:?}", c);
                        assert!(c.options.contains(&c.answer), "the right answer is an option: {:?}", c);
                        let mut o = c.options.clone();
                        o.dedup();
                        assert_eq!(o.len(), c.options.len());
                        assert!(c.check(&c.answer).correct);
                    }
                    "arrange" => {
                        // the chips of the answer are all in the bank, and arranging them is right
                        let chips = if c.answer_lang == "jpn" { p.sentences.iter().find(|s| s.t == c.answer).unwrap().words() } else { split_words(&c.answer) };
                        for x in &chips {
                            assert!(c.bank.contains(x), "{} in {:?}", x, c.bank);
                        }
                        assert!(c.bank.len() > chips.len());
                        assert!(c.check(&chips.join(" ")).correct, "{:?}", c);
                    }
                    "listen" => {
                        assert!(c.audio.starts_with("https://tatoeba.org/audio/download/"));
                        assert!(c.check(&c.answer).correct);
                    }
                    "match" => assert!(c.pairs.len() >= 4 && c.pairs.len() <= 5),
                    _ => {}
                }
            }
        }
        for k in ["select", "arrange", "assist", "listen", "match", "fill"] {
            assert!(kinds.contains(k), "never made a {} challenge", k);
        }
        // the first lesson starts by teaching a new word
        let ch = build_lesson(&input(&p, "1-1", "eng", 3));
        assert!(ch[0].kind == "select" && ch[0].new_word);
        // no audio: no listening challenge
        let ch = build_lesson(&input(&p, "2-1", "eng", 1));
        assert!(ch.iter().all(|c| c.kind != "listen"));
    }

    #[test]
    fn checks_answers() {
        let c = Challenge { kind: "listen".into(), answer: "Eu sou estudante.".into(), answer_lang: "por".into(), ..Challenge::default() };
        assert!(c.check("eu sou estudante").correct);
        assert_eq!(c.check("Eu sou estudante").note, "");
        let c2 = Challenge { kind: "listen".into(), answer: "Você é médico.".into(), answer_lang: "por".into(), ..Challenge::default() };
        let v = c2.check("voce e medico");
        assert!(v.correct);
        assert_eq!(v.note, "accents");
        let v = c2.check("voce e medica");
        assert!(v.correct && v.note == "typo");
        assert!(!c2.check("voce e dentista").correct);
        assert!(!c2.check("").correct);
        // Japanese: kanji, kana, a mix, katakana, full-width and spaces
        let p = pack();
        let s = p.sentence(4707).unwrap();
        let j = Challenge { kind: "listen".into(), answer: s.t.clone(), alts: vec![s.r.clone()], token_alts: token_alts(&p, s), answer_lang: "jpn".into(), ..Challenge::default() };
        assert!(j.check("私は学生です").correct);
        assert!(j.check("わたしはがくせいです。").correct);
        assert!(j.check("わたしは学生です").correct);
        assert!(j.check("ワタシハ ガクセイデス").correct);
        assert!(j.check("私は 学生です！").correct);
        assert!(!j.check("私は先生です").correct);
        // arrange: chips joined
        let a = Challenge { kind: "arrange".into(), answer: "I am a student.".into(), answer_lang: "eng".into(), ..Challenge::default() };
        assert!(a.check("I am a student").correct);
        assert!(!a.check("a am I student").correct);
        // select: the option itself
        let s = Challenge { kind: "select".into(), answer: "gato".into(), answer_lang: "por".into(), ..Challenge::default() };
        assert!(s.check("gato").correct && !s.check("cachorro").correct);
    }

    #[test]
    fn session_requeues_mistakes() {
        let c = |a: &str| Challenge { kind: "select".into(), answer: a.into(), answer_lang: "eng".into(), ..Challenge::default() };
        let mut s = Session::new("lesson", "1-1", vec![c("a"), c("b")], 0);
        assert!(s.answer("x").map(|v| !v.correct).unwrap());
        assert!(!s.advance());
        assert!(s.answer("b").unwrap().correct);
        assert!(!s.advance());
        // the missed one comes back
        assert_eq!(s.current().unwrap().answer, "a");
        assert!(s.answer("a").unwrap().correct);
        assert!(s.advance());
        assert_eq!(s.status, "done");
        assert_eq!(s.mistakes, 1);
        assert_eq!(s.xp_earned(), 10);
        assert_eq!(s.accuracy(), 67);
    }

    #[test]
    fn streak_and_days() {
        let day = 86400;
        let t0 = 20_000 * day + 3600; // 01:00 UTC
        let mut p = Progress { tz: 0, ..Progress::default() };
        let won = |m: u32| Session { kind: "lesson".into(), mistakes: m, ..Session::default() };
        p.finish(&won(0), t0, 0, 0);
        assert_eq!(p.streak, 1);
        assert_eq!(p.xp_on(20_000), 15);
        assert!(p.achievements.iter().any(|(a, _)| a == "first_lesson"));
        // same day: no change of streak
        p.finish(&won(1), t0 + 3600, 0, 0);
        assert_eq!(p.streak, 1);
        assert_eq!(p.xp_on(20_000), 25);
        // next day
        p.finish(&won(0), t0 + day, 0, 0);
        assert_eq!(p.streak, 2);
        // a missed day breaks it
        p.roll(t0 + 3 * day);
        assert_eq!(p.streak, 0);
        // ... unless a freeze covers it
        let mut q = Progress::default();
        q.finish(&won(0), t0, 0, 0);
        q.freezes = 1;
        q.roll(t0 + 2 * day);
        assert_eq!(q.streak, 1);
        assert_eq!(q.freezes, 0);
        assert_eq!(q.frozen, vec![20_001]);
        q.finish(&won(0), t0 + 2 * day, 0, 0);
        assert_eq!(q.streak, 2);
        // the time zone moves the day: 01:00 UTC is still the day before at UTC-3
        assert_eq!(day_of(t0, -180), 19_999);
        // quests change with the day and the XP quest moves
        assert_eq!(q.quests.len(), 3);
        assert_eq!(q.quests[0].kind, "xp");
        assert_eq!(daily_quests(5, 20), daily_quests(5, 20));
        // round trip
        let back = Progress::from_json(&Json::parse(&q.to_json().dump()).unwrap());
        assert_eq!(back, q);
    }

    #[test]
    fn hearts_refill() {
        let mut p = Progress::default();
        let t = 1_000_000;
        p.lose_heart(t);
        p.lose_heart(t + 10);
        assert_eq!(p.hearts, 3);
        p.refill(t + HEART_SECS - 1);
        assert_eq!(p.hearts, 3);
        p.refill(t + HEART_SECS);
        assert_eq!(p.hearts, 4);
        assert_eq!(p.next_heart_in(t + HEART_SECS), HEART_SECS);
        p.refill(t + 2 * HEART_SECS);
        assert_eq!(p.hearts, 5);
        p.refill(t + 10 * HEART_SECS);
        assert_eq!(p.hearts, 5);
        // the shop
        p.gems = 100;
        assert!(p.buy("refill", t).is_err(), "full hearts");
        p.lose_heart(t);
        assert!(p.buy("refill", t).is_ok());
        assert_eq!((p.hearts, p.gems), (5, 100 - PRICE_REFILL));
        assert!(p.buy("freeze", t).is_err(), "not enough gems");
        p.gems = 500;
        assert!(p.buy("freeze", t).is_ok() && p.buy("freeze", t).is_ok());
        assert!(p.buy("freeze", t).is_err(), "two at most");
    }

    #[test]
    fn every_challenge_names_its_words() {
        // the knowledge state is fed from `words`: every kind of challenge (word bank, match, select, listen, fill,
        // assist) must say which words it asked about
        let p = pack();
        for seed in 0..40 {
            for c in build_lesson(&input(&p, "1-2", "eng", seed)) {
                assert!(!c.words.is_empty(), "{} has no words: {:?}", c.kind, c);
                assert!(c.words.iter().all(|w| p.word(w).is_some()), "{:?}", c.words);
            }
        }
    }

    #[test]
    fn chest_boost_and_milestones() {
        let day = 86400;
        let t0 = 20_000 * day + 3600;
        let won = |m: u32| Session { kind: "lesson".into(), mistakes: m, ..Session::default() };
        let mut p = Progress { goal: 20, gems: 0, ..Progress::default() };
        // the chest waits for the daily goal
        assert!(!p.chest_ready(t0));
        assert!(p.open_chest(t0, 1).is_err());
        let (_, _, m) = p.finish(&won(0), t0, 0, 0);
        assert!(!m.contains(&"goal".to_string()));
        let (_, _, m) = p.finish(&won(0), t0 + 60, 0, 0);
        assert!(m.contains(&"goal".to_string()) && m.contains(&"chest".to_string()), "{:?}", m);
        assert!(p.chest_ready(t0 + 120));
        let before = p.gems;
        let (g, _) = p.open_chest(t0 + 120, 7).unwrap();
        assert!((CHEST_MIN..=CHEST_MAX).contains(&g));
        assert_eq!(p.gems, before + g);
        assert!(p.open_chest(t0 + 130, 8).is_err(), "once a day");
        assert!(!p.chest_ready(t0 + 130));
        // over many days the chest gives every amount in the range and sometimes a boost
        let mut amounts = HashSet::new();
        let mut boosts = 0;
        for seed in 0..400u64 {
            let mut q = Progress { goal: 10, ..Progress::default() };
            q.days.push((q.today(t0), 10));
            let (g, b) = q.open_chest(t0, seed).unwrap();
            amounts.insert(g);
            boosts += usize::from(b);
            assert_eq!(b, q.boost_left(t0) == BOOST_SECS);
        }
        assert_eq!(amounts.len() as u32, CHEST_MAX - CHEST_MIN + 1);
        assert!(boosts > 40 && boosts < 200, "{}", boosts);
        // the boost from the shop doubles XP for 15 minutes, and stacks
        let mut b = Progress { gems: 200, ..Progress::default() };
        assert!(b.buy("boost", t0).is_ok());
        assert_eq!(b.gems, 200 - PRICE_BOOST);
        assert_eq!(b.boost_left(t0), BOOST_SECS);
        assert!(b.buy("boost", t0 + 60).is_ok());
        assert_eq!(b.boost_left(t0 + 60), 2 * BOOST_SECS - 60);
        let (xp, _, m) = b.finish(&won(0), t0 + 120, 0, 0);
        assert_eq!(xp, 30);
        assert!(m.contains(&"boost".to_string()));
        let (xp, _, _) = b.finish(&won(0), t0 + 3 * BOOST_SECS, 0, 0);
        assert_eq!(xp, 15);
        // streak milestones: 7 days in a row give their gems once
        let mut s = Progress { gems: 0, ..Progress::default() };
        let mut got = vec![];
        for d in 0..8 {
            let (_, gems, m) = s.finish(&won(1), t0 + d * day, 0, 0);
            if m.contains(&"milestone:7".to_string()) {
                got.push((d, gems));
            }
        }
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, 6);
        // (the quests of the day may add theirs)
        assert!(got[0].1 >= GEMS_LESSON + STREAK_MILESTONES[0].1);
        // legendary rounds give more XP and mark the unit
        let leg = Session { kind: "legendary".into(), mistakes: 0, ..Session::default() };
        assert_eq!(leg.xp_earned(), XP_LEGENDARY + 10);
        s.mark_legendary("jpn", 1);
        s.mark_legendary("jpn", 1);
        assert!(s.is_legendary("jpn", 1) && !s.is_legendary("jpn", 2) && !s.is_legendary("eng", 1));
        assert_eq!((unit_level(false, false), unit_level(true, false), unit_level(true, true)), (0, 1, 2));
        // all of it survives learn.json
        let back = Progress::from_json(&Json::parse(&s.to_json().dump()).unwrap());
        assert_eq!(back, s);
        let back = Progress::from_json(&Json::parse(&b.to_json().dump()).unwrap());
        assert_eq!(back.boost_until, b.boost_until);
    }

    #[test]
    fn gem_rules_are_the_real_numbers() {
        let j = gem_rules_json();
        assert_eq!(j.get("lesson").as_u64(), Some(GEMS_LESSON as u64));
        assert_eq!(j.get("perfect").as_u64(), Some(GEMS_PERFECT as u64));
        assert_eq!(j.get("achievement").as_u64(), Some(GEMS_ACHIEVEMENT as u64));
        assert_eq!(j.get("chest_min").as_u64(), Some(5));
        assert_eq!(j.get("chest_max").as_u64(), Some(20));
        assert_eq!(j.get("boost_minutes").as_u64(), Some(15));
        let ms: Vec<u64> = j.get("milestones").items().iter().map(|m| m.get("days").as_u64().unwrap()).collect();
        assert_eq!(ms, vec![7, 30, 100]);
        // the quests really pay between quest_min and quest_max
        let (lo, hi) = (j.get("quest_min").as_u64().unwrap() as u32, j.get("quest_max").as_u64().unwrap() as u32);
        let mut seen = HashSet::new();
        for d in 0..200 {
            for q in daily_quests(d, 20) {
                assert!(q.reward >= lo && q.reward <= hi);
                seen.insert(q.reward);
            }
        }
        assert!(seen.contains(&lo) && seen.contains(&hi));
        // a finished lesson pays lesson (+ perfect) gems
        let mut p = Progress { gems: 0, quests: vec![], quest_day: day_of(1_000_000, 0), achievements: ACHIEVEMENTS.iter().map(|a| (a.0.to_string(), 0)).collect(), streak_day: day_of(1_000_000, 0), streak: 3, ..Progress::default() };
        let (_, gems, _) = p.finish(&Session { kind: "lesson".into(), mistakes: 0, ..Session::default() }, 1_000_000, 0, 0);
        assert_eq!(gems, GEMS_LESSON + GEMS_PERFECT);
    }

    /// The real course packs (assets/learn), when the data builder has made them: every lesson gives challenges
    /// whose own answers are accepted.
    #[test]
    fn real_packs() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("learn");
        for lang in ["jpn", "eng", "por", "spa"] {
            let Ok(text) = std::fs::read_to_string(dir.join(format!("{}.json", lang))) else { continue };
            let p = Pack::parse(&text).unwrap_or_else(|e| panic!("{}.json: {}", lang, e));
            let ids = p.lesson_ids();
            for tl in p.translation_langs().into_iter().filter(|l| *l != lang).take(3) {
                for (i, id) in ids.iter().enumerate() {
                    if i > 10 && i % 25 != 0 {
                        continue;
                    }
                    let ch = build_lesson(&input(&p, id, &tl, i as u64));
                    assert!(!ch.is_empty(), "{} lesson {} in {} has no challenges", lang, id, tl);
                    for c in ch.iter().filter(|c| c.kind != "match" && c.kind != "arrange") {
                        assert!(c.check(&c.answer).correct, "{} {}: {:?}", lang, id, c);
                    }
                }
            }
        }
    }

    #[test]
    fn practice_from_weak_words() {
        let p = pack();
        let mut k = Knowledge::default();
        k.record_at("jpn", "学生", true, 0);
        k.record_at("jpn", "先生", true, 1_000_000);
        k.record_at("jpn", "猫", false, 1_000_000);
        let learned: Vec<String> = ["学生", "先生", "猫", "。"].iter().map(|s| s.to_string()).collect();
        let w = practice_words(&k, &p, &learned, 2, 1_000_000);
        assert_eq!(w.len(), 2);
        assert!(!w.contains(&"先生".to_string()), "the strongest word is left out: {:?}", w);
        let allowed: HashSet<String> = p.words_until("1-2").into_iter().collect();
        let ids = sentences_with(&p, &w, &allowed, 5);
        assert!(!ids.is_empty());
    }
}
