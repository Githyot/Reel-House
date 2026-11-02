//! The "current knowledge" of the Study mode: every word the user saw (in Learn lessons, flashcard reviews and the
//! player's quizzes), how well it is known, and where the flashcards are. Kept in <data dir>/study/knowledge.md so
//! people can read it and other parts of the app can use it (layout: docs/learn-contract.md §2).
//!
//! How well a word is known follows half-life regression (Settles & Meeder, Duolingo 2016): the chance to recall a
//! word is p = 2^(-Δ/h), Δ the time since it was last seen and h its half-life. h grows with right answers and
//! shrinks with wrong ones, so it is computed from the counts (no extra state to keep beside the table).
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A flashcard that matches a line of text, for the player to show.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardRef {
    pub id: String,
    pub kind: String,
    pub deck: String,
    pub note: i64,
    pub location: String,
    pub front: String,
    pub back: String,
}

/// One word the user met.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WordRow {
    pub word: String,
    pub reading: String,
    pub seen: u32,
    pub right: u32,
    pub wrong: u32,
    /// Unix seconds of the last time it was seen (0 = never answered).
    pub last: i64,
    /// Translations: (language, text).
    pub tr: Vec<(String, String)>,
}

impl WordRow {
    /// Half-life in hours, from the counts. A word seen once and answered right lasts about a day; twenty right
    /// answers make it about a month; every wrong answer pulls it back.
    pub fn half_life(&self) -> f64 {
        let x = -1.0 + 1.6 * (1.0 + self.right as f64).sqrt() - 1.2 * (1.0 + self.wrong as f64).sqrt();
        (24.0 * 2f64.powf(x)).clamp(0.25, 24.0 * 365.0)
    }

    /// Chance (0..1) to recall the word at `now` (unix seconds).
    pub fn strength(&self, now: i64) -> f64 {
        // never answered right: not known yet, however recent
        if self.right == 0 {
            return 0.0;
        }
        let hours = ((now - self.last).max(0) as f64) / 3600.0;
        2f64.powf(-hours / self.half_life())
    }

    /// The translation into `lang`, or "".
    pub fn tr(&self, lang: &str) -> &str {
        self.tr.iter().find(|(l, _)| l == lang).map(|(_, t)| t.as_str()).unwrap_or("")
    }
}

/// A lesson the user finished in Learn (a lesson of the path, a legendary round or a characters lesson).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LessonRow {
    /// The unit: its number on the path, or "Hiragana" / "Katakana" for the characters.
    pub unit: String,
    pub id: String,
    pub title: String,
    /// Unix seconds of the last time it was finished.
    pub done: i64,
    pub plays: u32,
    /// XP earned in it over every play.
    pub xp: u32,
    /// Right answers out of all answers, in percent, the last time.
    pub accuracy: u32,
    /// The words (or characters) it introduces.
    pub words: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Knowledge {
    path: PathBuf,
    pub study: String,
    pub translation: String,
    pub known: Vec<String>,
    pub updated: i64,
    /// Words per language (language code, rows), in the order they were first met.
    pub words: Vec<(String, Vec<WordRow>)>,
    /// Characters of a writing system per language (Japanese kana): the character, its romaji as the reading.
    pub chars: Vec<(String, Vec<WordRow>)>,
    /// Lessons finished in Learn per language, in the order they were first finished.
    pub lessons: Vec<(String, Vec<LessonRow>)>,
    pub cards: Vec<CardRef>,
    /// Words of the course packs per language, for cutting lines into words: word -> dictionary form.
    dict: HashMap<String, HashMap<String, String>>,
    /// Longest entry of `dict` per language, in characters.
    dict_max: HashMap<String, usize>,
    /// Per flashcard: its front without spaces or punctuation, and its words (kept so the player can look up
    /// hundreds of lines quickly).
    card_idx: Vec<(String, HashSet<String>)>,
}

/// Particles and endings of Japanese that every line has: they are always "known" and cut lines apart.
const JA_GRAMMAR: &[&str] = &[
    "は", "が", "を", "に", "で", "と", "へ", "も", "の", "や", "か", "ね", "よ", "な", "わ", "さ", "ぞ", "ぜ", "から", "まで", "より", "って", "けど",
    "けれど", "ので", "のに", "だけ", "しか", "ばかり", "ほど", "など", "でも", "とか", "し", "ば", "たら", "なら", "です", "でした", "でしょう", "ます",
    "ました", "ません", "ませんでした", "ましょう", "ない", "なかった", "なくて", "た", "だ", "だった", "だろう", "て", "ている", "ています", "てる", "たい",
    "たかった", "れる", "られる", "せる", "させる", "う", "よう", "ください", "ちゃう", "じゃない", "じゃ", "では", "には", "とは", "への", "での", "との",
    "からの", "ん", "の", "こと", "もの",
];

/// Endings written after the stem of a Japanese verb or adjective that belong to the word as it is said
/// (`Knowledge::heard_words`).
const JA_ENDINGS: &[&str] = &[
    "ます", "ました", "ません", "ませんでした", "ましょう", "ない", "なかった", "なくて", "た", "だ", "て", "で", "ている", "ています", "てる",
    "でる", "たい", "たかった", "れる", "られる", "せる", "させる", "う", "よう", "ちゃう", "ちゃった", "たら", "だら", "ば", "ねえ", "ねぇ",
];

pub fn is_kana(c: char) -> bool {
    ('\u{3041}'..='\u{309F}').contains(&c) || ('\u{30A0}'..='\u{30FF}').contains(&c)
}

pub fn is_cjk(c: char) -> bool {
    crate::kanji::is_kanji(c) || is_kana(c) || c == '々' || c == 'ー'
}

/// The class of a Japanese character, for cutting unknown runs: kanji, hiragana, katakana, other.
fn ja_class(c: char) -> u8 {
    if crate::kanji::is_kanji(c) || c == '々' {
        0
    } else if ('\u{3041}'..='\u{309F}').contains(&c) {
        1
    } else if ('\u{30A0}'..='\u{30FF}').contains(&c) {
        2
    } else {
        3
    }
}

/// True for a token made only of punctuation and symbols (no letters or digits).
pub fn is_punct(t: &str) -> bool {
    !t.chars().any(|c| c.is_alphanumeric())
}

/// Lower case for the Latin-script languages; Japanese is kept as it is.
fn key(lang: &str, w: &str) -> String {
    if lang == "jpn" {
        w.trim().to_string()
    } else {
        w.trim().to_lowercase()
    }
}

/// Forms of a Japanese verb or adjective that appear in sentences (the polite stem, the te / ta stem…), so
/// "食べました" finds 食べる. `pos` comes from the course pack ("verb", "adj-i"…).
pub fn ja_forms(w: &str, pos: &str) -> Vec<String> {
    let chars: Vec<char> = w.chars().collect();
    if chars.len() < 2 {
        return vec![];
    }
    let stem: String = chars[..chars.len() - 1].iter().collect();
    let last = chars[chars.len() - 1];
    let mut out = vec![];
    if pos.contains("verb") || pos.starts_with('v') {
        // ichidan (食べる -> 食べ) and godan (書く -> 書き, 書い)
        let godan: &[(char, &str, &str)] = &[
            ('う', "い", "っ"),
            ('つ', "ち", "っ"),
            ('る', "り", "っ"),
            ('く', "き", "い"),
            ('ぐ', "ぎ", "い"),
            ('す', "し", "し"),
            ('ぬ', "に", "ん"),
            ('ぶ', "び", "ん"),
            ('む', "み", "ん"),
        ];
        if last == 'る' {
            out.push(stem.clone());
        }
        if let Some((_, i, te)) = godan.iter().find(|(e, _, _)| *e == last) {
            out.push(format!("{}{}", stem, i));
            out.push(format!("{}{}", stem, te));
            // and the negative stem (書か)
            let a = match last {
                'う' => "わ",
                'つ' => "た",
                'る' => "ら",
                'く' => "か",
                'ぐ' => "が",
                'す' => "さ",
                'ぬ' => "な",
                'ぶ' => "ば",
                'む' => "ま",
                _ => "",
            };
            if !a.is_empty() {
                out.push(format!("{}{}", stem, a));
            }
        }
        if w.ends_with("する") {
            let s: String = chars[..chars.len() - 2].iter().collect();
            for e in ["し", "さ", "せ"] {
                out.push(format!("{}{}", s, e));
            }
        }
    } else if (pos.contains("adj-i") || pos == "adj") && last == 'い' {
        for e in ["く", "かった", "くない", "くて", "けれ", ""] {
            out.push(format!("{}{}", stem, e));
        }
    }
    out.retain(|f| f.chars().count() >= 1 && f != w);
    out
}

/// Days since 1970-01-01 of a date "YYYY-MM-DD".
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn parse_date(s: &str) -> i64 {
    let p: Vec<&str> = s.trim().split(['-', 'T']).collect();
    if p.len() < 3 {
        return 0;
    }
    let (Ok(y), Ok(m), Ok(d)) = (p[0].parse::<i64>(), p[1].parse::<u32>(), p[2].parse::<u32>()) else { return 0 };
    let mut secs = days_from_civil(y, m, d) * 86400;
    // the time of the "Updated" line, when there is one
    if let Some(t) = s.split('T').nth(1) {
        let hms: Vec<i64> = t.trim_end_matches('Z').split(':').filter_map(|x| x.parse().ok()).collect();
        secs += hms.first().copied().unwrap_or(0) * 3600 + hms.get(1).copied().unwrap_or(0) * 60 + hms.get(2).copied().unwrap_or(0);
    } else {
        // a date alone: the middle of that day, so the half-life is not off by a whole day
        secs += 12 * 3600;
    }
    secs
}

fn fmt_date(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    let (y, mo, d, ..) = crate::util::civil(secs as u64);
    format!("{:04}-{:02}-{:02}", y, mo, d)
}

fn fmt_stamp(secs: i64) -> String {
    let (y, mo, d, h, mi, s) = crate::util::civil(secs.max(0) as u64);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, s)
}

/// A table cell: no bars or line breaks inside.
fn cell(s: &str) -> String {
    s.replace('|', "/").replace(['\n', '\r'], " ").trim().to_string()
}

fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// Removes HTML tags and the common entities (Anki fields are HTML).
pub fn strip_tags(s: &str) -> String {
    let s = s.replace("<br>", " ").replace("<br/>", " ").replace("<br />", " ").replace("<div>", " ");
    let mut out = String::new();
    let mut inside = false;
    for ch in s.chars() {
        match ch {
            '<' => inside = true,
            '>' if inside => inside = false,
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    let out = out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"");
    // [sound:x.mp3] tags of Anki
    let mut clean = String::new();
    let mut rest = out.as_str();
    while let Some(i) = rest.find("[sound:") {
        clean.push_str(&rest[..i]);
        rest = rest[i..].find(']').map(|j| &rest[i + j + 1..]).unwrap_or("");
    }
    clean.push_str(rest);
    clean.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Knowledge {
    pub fn path() -> PathBuf {
        crate::study::dir(&crate::util::data_dir()).join("knowledge.md")
    }

    /// Reads knowledge.md (empty if missing).
    pub fn load() -> Knowledge {
        Knowledge::load_from(&Knowledge::path())
    }

    pub fn load_from(path: &Path) -> Knowledge {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let mut k = Knowledge::parse(&text);
        k.path = path.to_path_buf();
        k.index_cards();
        k
    }

    /// Reads the Markdown layout written by `render`.
    pub fn parse(text: &str) -> Knowledge {
        let mut k = Knowledge::default();
        // what the table being read holds: words of a language (with its column names) or the flashcards
        enum Sec {
            None,
            Words(String, Vec<String>),
            Chars(String, Vec<String>),
            Lessons(String, Vec<String>),
            Cards,
        }
        // "Words — jpn" -> "jpn"
        fn lang_of(h: &str, name: &str) -> Option<String> {
            h.strip_prefix(name).map(|r| r.trim_start_matches([' ', '—', '-']).trim().to_string())
        }
        let mut sec = Sec::None;
        for line in text.lines() {
            let l = line.trim();
            if let Some(v) = l.strip_prefix("- Study language:") {
                k.study = v.trim().to_string();
            } else if let Some(v) = l.strip_prefix("- Translation language:") {
                k.translation = v.trim().to_string();
            } else if let Some(v) = l.strip_prefix("- Known languages:") {
                k.known = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            } else if let Some(v) = l.strip_prefix("- Updated:") {
                k.updated = parse_date(v);
            } else if let Some(h) = l.strip_prefix("## ") {
                sec = if h.trim() == "Flashcards" {
                    Sec::Cards
                } else if let Some(lang) = lang_of(h, "Words") {
                    Sec::Words(lang, vec![])
                } else if let Some(lang) = lang_of(h, "Characters") {
                    Sec::Chars(lang, vec![])
                } else if let Some(lang) = lang_of(h, "Lessons") {
                    Sec::Lessons(lang, vec![])
                } else {
                    Sec::None
                };
            } else if l.starts_with('|') {
                let c = cells(l);
                if c.iter().all(|x| x.chars().all(|ch| ch == '-' || ch == ':')) {
                    continue; // the line under the column names
                }
                match &mut sec {
                    Sec::Words(lang, cols) => {
                        if cols.is_empty() {
                            *cols = c;
                            continue;
                        }
                        let get = |name: &str| cols.iter().position(|x| x == name).and_then(|i| c.get(i)).cloned().unwrap_or_default();
                        let num = |name: &str| get(name).parse::<u32>().unwrap_or(0);
                        let mut row = WordRow { word: get("word"), reading: get("reading"), seen: num("seen"), right: num("right"), wrong: num("wrong"), last: parse_date(&get("last seen")), tr: vec![] };
                        if row.word.is_empty() {
                            continue;
                        }
                        if get("last seen").is_empty() {
                            row.last = 0;
                        }
                        for (i, name) in cols.iter().enumerate().skip(7) {
                            if let Some(v) = c.get(i).filter(|v| !v.is_empty()) {
                                row.tr.push((name.clone(), v.clone()));
                            }
                        }
                        let lang = lang.clone();
                        k.rows_mut(&lang).push(row);
                    }
                    Sec::Chars(lang, cols) => {
                        if cols.is_empty() {
                            *cols = c;
                            continue;
                        }
                        let get = |name: &str| cols.iter().position(|x| x == name).and_then(|i| c.get(i)).cloned().unwrap_or_default();
                        let num = |name: &str| get(name).parse::<u32>().unwrap_or(0);
                        let last = if get("last seen").is_empty() { 0 } else { parse_date(&get("last seen")) };
                        let row = WordRow { word: get("character"), reading: get("romaji"), seen: num("seen"), right: num("right"), wrong: num("wrong"), last, tr: vec![] };
                        if !row.word.is_empty() {
                            let lang = lang.clone();
                            k.chars_mut(&lang).push(row);
                        }
                    }
                    Sec::Lessons(lang, cols) => {
                        if cols.is_empty() {
                            *cols = c;
                            continue;
                        }
                        let get = |name: &str| cols.iter().position(|x| x == name).and_then(|i| c.get(i)).cloned().unwrap_or_default();
                        let num = |name: &str| get(name).trim_end_matches('%').parse::<u32>().unwrap_or(0);
                        let row = LessonRow {
                            unit: get("unit"),
                            id: get("lesson"),
                            title: get("title"),
                            done: parse_date(&get("completed")),
                            plays: num("plays").max(1),
                            xp: num("xp"),
                            accuracy: num("accuracy"),
                            words: get("words").split(", ").map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect(),
                        };
                        if !row.id.is_empty() {
                            let lang = lang.clone();
                            k.lessons_mut(&lang).push(row);
                        }
                    }
                    Sec::Cards => {
                        if c.first().map(|x| x == "id").unwrap_or(false) {
                            continue;
                        }
                        let g = |i: usize| c.get(i).cloned().unwrap_or_default();
                        k.cards.push(CardRef { id: g(0), kind: g(1), deck: g(2), note: g(3).parse().unwrap_or(0), location: g(4), front: g(5), back: g(6) });
                    }
                    Sec::None => {}
                }
            }
        }
        k
    }

    /// The Markdown text of the file.
    pub fn render(&self, now: i64) -> String {
        let mut o = String::new();
        o.push_str("# Current knowledge\n");
        o.push_str(&format!("- Study language: {}\n", self.study));
        o.push_str(&format!("- Translation language: {}\n", self.translation));
        o.push_str(&format!("- Known languages: {}\n", self.known.join(", ")));
        o.push_str(&format!("- Updated: {}\n", fmt_stamp(now)));
        // the study language first, then any other language with words
        let mut langs: Vec<&(String, Vec<WordRow>)> = self.words.iter().collect();
        langs.sort_by_key(|(l, _)| if *l == self.study { 0 } else { 1 });
        for (lang, rows) in langs {
            // one column per known language (not the language of the words)
            let mut tcols: Vec<String> = self.known.iter().filter(|l| *l != lang).cloned().collect();
            for r in rows {
                for (l, _) in &r.tr {
                    if l != lang && !tcols.contains(l) {
                        tcols.push(l.clone());
                    }
                }
            }
            o.push_str(&format!("\n## Words — {}\n", lang));
            let mut head = vec!["word", "reading", "seen", "right", "wrong", "strength", "last seen"];
            head.extend(tcols.iter().map(|s| s.as_str()));
            o.push_str(&format!("| {} |\n", head.join(" | ")));
            o.push_str(&format!("|{}\n", "---|".repeat(head.len())));
            for r in rows {
                let mut c = vec![cell(&r.word), cell(&r.reading), r.seen.to_string(), r.right.to_string(), r.wrong.to_string(), format!("{:.2}", r.strength(now)), fmt_date(r.last)];
                c.extend(tcols.iter().map(|l| cell(r.tr(l))));
                o.push_str(&format!("| {} |\n", c.join(" | ")));
            }
        }
        // the characters (Japanese kana), then the lessons, each language on its own
        for (lang, rows) in &self.chars {
            o.push_str(&format!("\n## Characters — {}\n", lang));
            o.push_str("| character | romaji | seen | right | wrong | strength | last seen |\n|---|---|---|---|---|---|---|\n");
            for r in rows {
                o.push_str(&format!("| {} | {} | {} | {} | {} | {:.2} | {} |\n", cell(&r.word), cell(&r.reading), r.seen, r.right, r.wrong, r.strength(now), fmt_date(r.last)));
            }
        }
        for (lang, rows) in &self.lessons {
            o.push_str(&format!("\n## Lessons — {}\n", lang));
            o.push_str("| unit | lesson | title | completed | plays | xp | accuracy | words |\n|---|---|---|---|---|---|---|---|\n");
            for r in rows {
                let words: Vec<String> = r.words.iter().map(|w| cell(w)).collect();
                o.push_str(&format!("| {} | {} | {} | {} | {} | {} | {}% | {} |\n", cell(&r.unit), cell(&r.id), cell(&r.title), fmt_date(r.done), r.plays, r.xp, r.accuracy, words.join(", ")));
            }
        }
        o.push_str("\n## Flashcards\n| id | kind | deck | note | where | front | back |\n|---|---|---|---|---|---|---|\n");
        for c in &self.cards {
            o.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} |\n", cell(&c.id), cell(&c.kind), cell(&c.deck), c.note, cell(&c.location), cell(&c.front), cell(&c.back)));
        }
        o
    }

    /// Writes knowledge.md.
    pub fn save(&self) {
        let path = if self.path.as_os_str().is_empty() { Knowledge::path() } else { self.path.clone() };
        self.save_to(&path);
    }

    pub fn save_to(&self, path: &Path) {
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = path.with_extension("md.tmp");
        if std::fs::write(&tmp, self.render(crate::util::now_unix() as i64)).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    pub fn study_lang(&self) -> String {
        self.study.clone()
    }

    /// Sets the languages of the header (3-letter codes).
    pub fn set_langs(&mut self, study: &str, translation: &str, known: &[String]) {
        let changed = self.study != study;
        self.study = study.to_string();
        if changed {
            self.index_cards();
        }
        self.translation = translation.to_string();
        self.known = known.to_vec();
    }

    pub fn rows(&self, lang: &str) -> &[WordRow] {
        self.words.iter().find(|(l, _)| l == lang).map(|(_, r)| r.as_slice()).unwrap_or(&[])
    }

    fn rows_mut(&mut self, lang: &str) -> &mut Vec<WordRow> {
        if let Some(i) = self.words.iter().position(|(l, _)| l == lang) {
            return &mut self.words[i].1;
        }
        self.words.push((lang.to_string(), vec![]));
        &mut self.words.last_mut().expect("just pushed").1
    }

    fn row_mut(&mut self, lang: &str, word: &str) -> &mut WordRow {
        let k = key(lang, word);
        let rows = self.rows_mut(lang);
        if let Some(i) = rows.iter().position(|r| key(lang, &r.word) == k) {
            return &mut rows[i];
        }
        rows.push(WordRow { word: word.trim().to_string(), ..WordRow::default() });
        rows.last_mut().expect("just pushed")
    }

    pub fn row(&self, lang: &str, word: &str) -> Option<&WordRow> {
        let k = key(lang, word);
        self.rows(lang).iter().find(|r| key(lang, &r.word) == k)
    }

    fn chars_mut(&mut self, lang: &str) -> &mut Vec<WordRow> {
        if let Some(i) = self.chars.iter().position(|(l, _)| l == lang) {
            return &mut self.chars[i].1;
        }
        self.chars.push((lang.to_string(), vec![]));
        &mut self.chars.last_mut().expect("just pushed").1
    }

    fn lessons_mut(&mut self, lang: &str) -> &mut Vec<LessonRow> {
        if let Some(i) = self.lessons.iter().position(|(l, _)| l == lang) {
            return &mut self.lessons[i].1;
        }
        self.lessons.push((lang.to_string(), vec![]));
        &mut self.lessons.last_mut().expect("just pushed").1
    }

    /// The characters of a language met so far (kana with their romaji).
    pub fn char_rows(&self, lang: &str) -> &[WordRow] {
        self.chars.iter().find(|(l, _)| l == lang).map(|(_, r)| r.as_slice()).unwrap_or(&[])
    }

    pub fn char_row(&self, lang: &str, ch: &str) -> Option<&WordRow> {
        self.char_rows(lang).iter().find(|r| r.word == ch)
    }

    /// A character was answered right or wrong (a characters lesson).
    pub fn record_char_at(&mut self, lang: &str, ch: &str, romaji: &str, correct: bool, now: i64) {
        if ch.trim().is_empty() {
            return;
        }
        let rows = self.chars_mut(lang);
        let i = match rows.iter().position(|r| r.word == ch) {
            Some(i) => i,
            None => {
                rows.push(WordRow { word: ch.to_string(), ..WordRow::default() });
                rows.len() - 1
            }
        };
        let r = &mut rows[i];
        if !romaji.is_empty() {
            r.reading = romaji.to_string();
        }
        r.seen += 1;
        if correct {
            r.right += 1;
        } else {
            r.wrong += 1;
        }
        r.last = now;
    }

    /// Recall chance of a character now (0 when never answered right).
    pub fn char_strength(&self, lang: &str, ch: &str, now: i64) -> f64 {
        self.char_row(lang, ch).map(|r| r.strength(now)).unwrap_or(0.0)
    }

    pub fn lesson_rows(&self, lang: &str) -> &[LessonRow] {
        self.lessons.iter().find(|(l, _)| l == lang).map(|(_, r)| r.as_slice()).unwrap_or(&[])
    }

    /// A lesson was finished: its row is added, or updated when it is played again (one more play, its XP added,
    /// the date and accuracy of this time).
    pub fn record_lesson(&mut self, lang: &str, row: LessonRow) {
        let rows = self.lessons_mut(lang);
        match rows.iter_mut().find(|r| r.id == row.id) {
            Some(r) => {
                r.unit = row.unit;
                r.title = row.title;
                r.done = row.done;
                r.plays += 1;
                r.xp += row.xp;
                r.accuracy = row.accuracy;
                if !row.words.is_empty() {
                    r.words = row.words;
                }
            }
            None => rows.push(LessonRow { plays: 1, ..row }),
        }
    }

    /// A word a lesson introduces gets its row (with no answers yet), so the file lists every word met.
    pub fn introduce(&mut self, lang: &str, word: &str) {
        if !word.trim().is_empty() && !is_punct(word) {
            self.row_mut(lang, word);
        }
    }

    /// The words of the course pack of a language, with their part of speech, and the written forms met in its
    /// sentences with their dictionary forms: they help cut lines into words.
    pub fn set_dictionary(&mut self, lang: &str, words: &[(String, String)], forms: &[(String, String)]) {
        let mut map: HashMap<String, String> = HashMap::new();
        for (f, w) in forms {
            map.insert(key(lang, f), key(lang, w));
        }
        for (w, pos) in words {
            let w = key(lang, w);
            if w.is_empty() {
                continue;
            }
            if lang == "jpn" {
                for f in ja_forms(&w, pos) {
                    map.entry(f).or_insert_with(|| w.clone());
                }
            }
            map.insert(w.clone(), w);
        }
        let max = map.keys().map(|k| k.chars().count()).max().unwrap_or(1);
        self.dict_max.insert(lang.to_string(), max);
        self.dict.insert(lang.to_string(), map);
        if lang == self.card_lang() {
            self.index_cards();
        }
    }

    /// Words of a line (no punctuation). Japanese has no spaces: the longest known word at each place wins (known
    /// words, course words, particles and endings); a run of unknown kana or kanji becomes one token.
    pub fn tokens(&self, lang: &str, text: &str) -> Vec<String> {
        self.split(lang, text).into_iter().map(|(_, w)| w).collect()
    }

    /// The words of a line in the original language as they are said there, each with its dictionary form, for
    /// matching the subtitle in a known language with the audio (wordmix `swap_heard`). Japanese verbs and
    /// adjectives keep the endings written after them ("あっ" + "た" -> "あった" of ある, "あり" + "ます"), and a
    /// lone kana that is only the stem of a word ("あ" of ある) is dropped: it is far more often something else.
    pub fn heard_words(&self, lang: &str, text: &str) -> Vec<(String, String)> {
        let parts = self.split(lang, text);
        if lang != "jpn" {
            return parts;
        }
        let mut out: Vec<(String, String)> = vec![];
        let mut i = 0;
        while i < parts.len() {
            let (mut surface, base) = parts[i].clone();
            i += 1;
            if surface != base && !Knowledge::grammar(lang, &base) {
                let mut n = 0;
                while i < parts.len() && n < 3 && JA_ENDINGS.contains(&parts[i].0.as_str()) {
                    surface.push_str(&parts[i].0);
                    i += 1;
                    n += 1;
                }
            }
            if surface != base && surface.chars().count() == 1 && surface.chars().all(is_kana) {
                continue;
            }
            out.push((surface, base));
        }
        out
    }

    /// `tokens` with the text of each word as written: (written, dictionary form).
    fn split(&self, lang: &str, text: &str) -> Vec<(String, String)> {
        if lang != "jpn" {
            return text
                .split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’' || c == '-'))
                .map(|w| w.trim_matches(|c: char| c == '\'' || c == '’' || c == '-').to_lowercase())
                .filter(|w| !w.is_empty() && !is_punct(w))
                .map(|w| (w.clone(), w))
                .collect();
        }
        let chars: Vec<char> = text.chars().collect();
        let empty = HashMap::new();
        let dict = self.dict.get(lang).unwrap_or(&empty);
        let known: HashSet<&str> = self.rows(lang).iter().map(|r| r.word.as_str()).collect();
        let max = self.dict_max.get(lang).copied().unwrap_or(1).max(self.rows(lang).iter().map(|r| r.word.chars().count()).max().unwrap_or(1)).clamp(6, 16);
        // the word that starts at `i`: (length, dictionary form)
        let lookup = |i: usize| -> Option<(usize, String)> {
            for len in (1..=max.min(chars.len() - i)).rev() {
                let s: String = chars[i..i + len].iter().collect();
                if known.contains(s.as_str()) {
                    return Some((len, s));
                }
                if let Some(d) = dict.get(&s) {
                    return Some((len, d.clone()));
                }
                if JA_GRAMMAR.contains(&s.as_str()) {
                    return Some((len, s));
                }
            }
            None
        };
        let mut out = vec![];
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if !is_cjk(c) {
                // Latin letters or digits inside a Japanese line make one token per run; punctuation is dropped
                if c.is_alphanumeric() {
                    let st = i;
                    while i < chars.len() && chars[i].is_alphanumeric() && !is_cjk(chars[i]) {
                        i += 1;
                    }
                    let w = chars[st..i].iter().collect::<String>().to_lowercase();
                    out.push((w.clone(), w));
                } else {
                    i += 1;
                }
                continue;
            }
            if let Some((len, w)) = lookup(i) {
                out.push((chars[i..i + len].iter().collect(), w));
                i += len;
                continue;
            }
            // an unknown run of one kind of character, ended where a longer known word starts
            let cls = ja_class(c);
            let st = i;
            i += 1;
            while i < chars.len() && ja_class(chars[i]) == cls && is_cjk(chars[i]) {
                if let Some((len, _)) = lookup(i) {
                    if len >= 2 || cls == 0 {
                        break;
                    }
                }
                i += 1;
            }
            let w: String = chars[st..i].iter().collect();
            out.push((w.clone(), w));
        }
        out
    }

    /// Particles and endings count as known: everyone who reads a sentence meets them.
    fn grammar(lang: &str, word: &str) -> bool {
        lang == "jpn" && JA_GRAMMAR.contains(&word)
    }

    /// The word was answered right at least once (or it is a particle / ending).
    pub fn knows(&self, lang: &str, word: &str) -> bool {
        Knowledge::grammar(lang, word) || self.row(lang, word).map(|r| r.right > 0).unwrap_or(false)
    }

    /// Every word of the line is known (and it has at least one word).
    pub fn line_known(&self, lang: &str, text: &str) -> bool {
        let t = self.tokens(lang, text);
        !t.is_empty() && t.iter().all(|w| self.knows(lang, w))
    }

    /// The user saw the word and answered right or wrong.
    pub fn record(&mut self, lang: &str, word: &str, correct: bool) {
        self.record_at(lang, word, correct, crate::util::now_unix() as i64);
    }

    pub fn record_at(&mut self, lang: &str, word: &str, correct: bool, now: i64) {
        if word.trim().is_empty() || is_punct(word) {
            return;
        }
        let r = self.row_mut(lang, word);
        r.seen += 1;
        if correct {
            r.right += 1;
        } else {
            r.wrong += 1;
        }
        r.last = now;
    }

    /// Records every word of a line.
    pub fn record_line(&mut self, lang: &str, text: &str, correct: bool) {
        let now = crate::util::now_unix() as i64;
        for w in self.tokens(lang, text) {
            if !Knowledge::grammar(lang, &w) {
                self.record_at(lang, &w, correct, now);
            }
        }
    }

    /// The reading and translations of a word (kept with its row; added when it is first met).
    pub fn set_info(&mut self, lang: &str, word: &str, reading: &str, tr: &[(String, String)]) {
        let r = self.row_mut(lang, word);
        if !reading.is_empty() {
            r.reading = reading.to_string();
        }
        for (l, t) in tr {
            if t.is_empty() || l == lang {
                continue;
            }
            match r.tr.iter_mut().find(|(x, _)| x == l) {
                Some(e) => e.1 = t.clone(),
                None => r.tr.push((l.clone(), t.clone())),
            }
        }
    }

    /// Recall chance of a word now (0 when never seen).
    pub fn strength(&self, lang: &str, word: &str, now: i64) -> f64 {
        self.row(lang, word).map(|r| r.strength(now)).unwrap_or(0.0)
    }

    /// The `n` answered words most likely forgotten, weakest first.
    pub fn weakest(&self, lang: &str, n: usize, now: i64) -> Vec<String> {
        let mut v: Vec<(f64, &WordRow)> = self.rows(lang).iter().filter(|r| r.right + r.wrong > 0).map(|r| (r.strength(now), r)).collect();
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter().take(n).map(|(_, r)| r.word.clone()).collect()
    }

    /// How many words of a language were answered right at least once.
    pub fn learned(&self, lang: &str) -> usize {
        self.rows(lang).iter().filter(|r| r.right > 0).count()
    }

    /// Replaces the flashcards table.
    pub fn set_cards(&mut self, cards: Vec<CardRef>) {
        // the same cards again (most saves of the collection): keep the index
        if cards == self.cards && self.card_idx.len() == self.cards.len() {
            return;
        }
        self.cards = cards;
        self.index_cards();
    }

    /// The language of the flashcards (the study language; Japanese when none is set).
    fn card_lang(&self) -> String {
        if self.study.is_empty() {
            "jpn".into()
        } else {
            self.study.clone()
        }
    }

    /// Text for "is this front inside the line": Japanese without spaces or punctuation; other languages as
    /// " word word " so a front only matches whole words ("eat" is not inside "great").
    fn compact(lang: &str, s: &str) -> String {
        if lang == "jpn" {
            return key(lang, s).chars().filter(|c| c.is_alphanumeric()).collect();
        }
        let words: Vec<String> = s.split(|c: char| !(c.is_alphanumeric() || c == '\'')).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect();
        if words.is_empty() {
            String::new()
        } else {
            format!(" {} ", words.join(" "))
        }
    }

    fn index_cards(&mut self) {
        let lang = self.card_lang();
        let idx = self
            .cards
            .iter()
            .map(|c| (Knowledge::compact(&lang, &c.front), self.tokens(&lang, &c.front).into_iter().filter(|w| !Knowledge::grammar(&lang, w)).collect()))
            .collect();
        self.card_idx = idx;
    }

    /// The best flashcard for a line: a card whose front is inside the line (the longest one), or else the card
    /// that shares the most words with it (two at least).
    pub fn card_for_line(&self, text: &str) -> Option<CardRef> {
        let lang = self.card_lang();
        let line = Knowledge::compact(&lang, text);
        if line.is_empty() || self.card_idx.len() != self.cards.len() {
            return None;
        }
        let mut best: Option<(usize, usize)> = None;
        for (i, (front, _)) in self.card_idx.iter().enumerate() {
            // the front inside the line, or a long enough line inside the front
            let n = front.trim().chars().count();
            if n >= 1 && (line.contains(front.as_str()) || (line.chars().count() >= 4 && front.contains(line.as_str()))) {
                // whole match: weigh it above any word overlap; prefer clips (they have the video)
                let score = 1000 + n * 2 + usize::from(self.cards[i].kind == "clip");
                if best.map(|(s, _)| score > s).unwrap_or(true) {
                    best = Some((score, i));
                }
            }
        }
        if best.is_none() {
            let words: HashSet<String> = self.tokens(&lang, text).into_iter().filter(|w| !Knowledge::grammar(&lang, w)).collect();
            for (i, (_, ws)) in self.card_idx.iter().enumerate() {
                let shared = ws.iter().filter(|w| words.contains(*w)).count();
                if shared >= 2 && best.map(|(s, _)| shared > s).unwrap_or(true) {
                    best = Some((shared, i));
                }
            }
        }
        best.map(|(_, i)| self.cards[i].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rh_knowledge_{}_{}", name, std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d.join("knowledge.md")
    }

    #[test]
    fn round_trip() {
        let p = tmp("rt");
        let mut k = Knowledge::load_from(&p);
        k.set_langs("jpn", "por", &["por".into(), "eng".into(), "spa".into()]);
        let now = 1_760_000_000;
        k.record_at("jpn", "食べる", true, now);
        k.record_at("jpn", "食べる", false, now);
        k.set_info("jpn", "食べる", "たべる", &[("por".into(), "comer".into()), ("eng".into(), "to eat".into())]);
        k.set_cards(vec![CardRef { id: "a:1".into(), kind: "anki".into(), deck: "Japanese | V2K".into(), note: 1, location: "study/collection".into(), front: "食べる".into(), back: "to eat".into() }]);
        k.save_to(&p);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("## Words — jpn"));
        assert!(text.contains("| word | reading | seen | right | wrong | strength | last seen | por | eng | spa |"));
        let k2 = Knowledge::load_from(&p);
        assert_eq!(k2.study, "jpn");
        assert_eq!(k2.known, vec!["por", "eng", "spa"]);
        let r = k2.row("jpn", "食べる").unwrap();
        assert_eq!((r.seen, r.right, r.wrong), (2, 1, 1));
        assert_eq!(r.reading, "たべる");
        assert_eq!(r.tr("eng"), "to eat");
        assert_eq!(k2.cards.len(), 1);
        assert_eq!(k2.cards[0].deck, "Japanese / V2K");
        // the same text again
        assert_eq!(Knowledge::parse(&k2.render(now)).render(now), k2.render(now));
    }

    #[test]
    fn lessons_and_characters_round_trip() {
        let p = tmp("lessons");
        let mut k = Knowledge::load_from(&p);
        k.set_langs("jpn", "por", &["por".into()]);
        let now = 1_760_000_000;
        k.record_at("jpn", "学生", true, now);
        k.introduce("jpn", "先生");
        k.introduce("jpn", "。");
        let row = |xp| LessonRow { unit: "1".into(), id: "1-1".into(), title: "Básico 1 · 1".into(), done: now, plays: 0, xp, accuracy: 90, words: vec!["私".into(), "学生".into()] };
        k.record_lesson("jpn", row(15));
        k.record_lesson("jpn", LessonRow { accuracy: 100, ..row(10) });
        k.record_lesson("jpn", LessonRow { unit: "Hiragana".into(), id: "kana-h-1".into(), title: "あ い う え お".into(), done: now, plays: 0, xp: 12, accuracy: 80, words: vec!["あ".into(), "い".into()] });
        k.record_char_at("jpn", "か", "ka", true, now);
        k.record_char_at("jpn", "か", "ka", false, now);
        k.record_char_at("jpn", "カ", "ka", true, now);
        k.save_to(&p);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("## Lessons — jpn"));
        assert!(text.contains("| unit | lesson | title | completed | plays | xp | accuracy | words |"));
        assert!(text.contains("| 1 | 1-1 | Básico 1 · 1 | 2025-10-09 | 2 | 25 | 100% | 私, 学生 |"), "{}", text);
        assert!(text.contains("## Characters — jpn"));
        let k2 = Knowledge::load_from(&p);
        assert_eq!(k2.lesson_rows("jpn").len(), 2);
        let l = &k2.lesson_rows("jpn")[0];
        assert_eq!((l.plays, l.xp, l.accuracy), (2, 25, 100));
        assert_eq!(l.words, vec!["私", "学生"]);
        let c = k2.char_row("jpn", "か").unwrap();
        assert_eq!((c.reading.as_str(), c.seen, c.right, c.wrong), ("ka", 2, 1, 1));
        assert!(k2.char_strength("jpn", "カ", now) > 0.9);
        assert_eq!(k2.char_strength("jpn", "き", now), 0.0);
        // an introduced word has its row but is not known; punctuation gets none
        assert!(k2.row("jpn", "先生").is_some() && !k2.knows("jpn", "先生"));
        assert!(k2.row("jpn", "。").is_none());
        // the words table still reads the same, and the whole file is stable
        assert_eq!(k2.row("jpn", "学生").unwrap().right, 1);
        assert_eq!(Knowledge::parse(&k2.render(now)).render(now), k2.render(now));
    }

    #[test]
    fn hlr_strength() {
        let mut r = WordRow { word: "x".into(), right: 1, last: 0, ..Default::default() };
        let h1 = r.half_life();
        assert!((r.strength(0) - 1.0).abs() < 1e-9);
        assert!((r.strength((h1 * 3600.0) as i64) - 0.5).abs() < 0.01);
        r.right = 10;
        assert!(r.half_life() > h1, "right answers make the half-life longer");
        let h10 = r.half_life();
        r.wrong = 3;
        assert!(r.half_life() < h10, "wrong answers make it shorter");
        // weakest first
        let mut k = Knowledge::default();
        k.record_at("eng", "old", true, 0);
        k.record_at("eng", "new", true, 1_000_000);
        assert_eq!(k.weakest("eng", 1, 1_000_000), vec!["old".to_string()]);
    }

    #[test]
    fn japanese_tokens() {
        let mut k = Knowledge::default();
        k.set_dictionary("jpn", &[("私".into(), "pron".into()), ("学生".into(), "noun".into()), ("食べる".into(), "verb".into()), ("ご飯".into(), "noun".into()), ("急ぐ".into(), "verb".into())], &[("急い".into(), "急ぐ".into())]);
        assert_eq!(k.tokens("jpn", "私は学生です。"), vec!["私", "は", "学生", "です"]);
        assert_eq!(k.tokens("jpn", "ご飯を食べました"), vec!["ご飯", "を", "食べる", "ました"]);
        assert_eq!(k.tokens("jpn", "急いで！"), vec!["急ぐ", "で"]);
        k.record_at("jpn", "私", true, 0);
        k.record_at("jpn", "学生", true, 0);
        assert!(k.line_known("jpn", "私は学生です。"));
        assert!(!k.line_known("jpn", "ご飯を食べました"));
        assert!(!k.line_known("jpn", "。"));
        assert_eq!(k.tokens("eng", "I don't know, Tom!"), vec!["i", "don't", "know", "tom"]);
    }

    #[test]
    fn card_lookup() {
        let mut k = Knowledge::default();
        k.study = "jpn".into();
        k.set_cards(vec![
            CardRef { id: "a:1".into(), kind: "anki".into(), front: "食べる".into(), ..Default::default() },
            CardRef { id: "clip:2".into(), kind: "clip".into(), front: "ご飯を食べる".into(), ..Default::default() },
        ]);
        assert_eq!(k.card_for_line("毎日ご飯を食べる。").unwrap().id, "clip:2");
        assert_eq!(k.card_for_line("パンを食べる").unwrap().id, "a:1");
        assert!(k.card_for_line("こんにちは").is_none());
        // other languages match whole words only
        k.study = "eng".into();
        k.set_cards(vec![CardRef { id: "a:3".into(), kind: "anki".into(), front: "eat".into(), ..Default::default() }]);
        assert_eq!(k.card_for_line("I eat rice.").unwrap().id, "a:3");
        assert!(k.card_for_line("That is great!").is_none());
    }

    #[test]
    fn words_heard_keep_their_endings() {
        let mut k = Knowledge::default();
        k.set_dictionary("jpn", &[("ある".into(), "verb".into()), ("食べる".into(), "verb".into()), ("猫".into(), "noun".into())], &[]);
        let w = k.heard_words("jpn", "猫があった。食べますか？あの…");
        assert!(w.contains(&("あった".to_string(), "ある".to_string())), "{:?}", w);
        assert!(w.contains(&("食べます".to_string(), "食べる".to_string())), "{:?}", w);
        assert!(w.contains(&("猫".to_string(), "猫".to_string())));
        // a lone stem kana ("あ" of ある) is not taken for the verb
        assert!(!w.iter().any(|(s, b)| s == "あ" && b == "ある"), "{:?}", w);
        assert_eq!(k.heard_words("jpn", "ありません")[0], ("ありません".to_string(), "ある".to_string()));
    }
}
