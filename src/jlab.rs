//! Jlab mode of the Study: Japanese sentence decks studied the way the Jlab add-on for desktop Anki does it
//! (AnkiWeb add-on 2110939339, "Japanese like a breeze"). That add-on is Python for desktop Anki and cannot run
//! here, so its behaviour is rebuilt natively from its public manual; no code of it is used.
//!
//! - A deck in Jlab mode has two phases per note: first a listening card (the audio, the sentence in romaji or
//!   kana), and once it is remembered for `retire_days` days it turns into a reading card (the sentence in kanji,
//!   the looked-up word as a gap with its meaning as the hint). A reading card remembered for as long retires
//!   (it is suspended), so reviews do not pile up.
//! - Four levels of reading help, set apart for listening, reading and the dictionary: romaji, kana,
//!   kana with the kanji the user knows, kanji with furigana only on the kanji the user does not know.
//! - Kana trainer: kana the user has learned show as kana inside the romaji. Kanji trainer: the number of kanji
//!   the user knows (in the order JLPT N5 → N1, fewer strokes first).
//! - A tap on a word looks it up (Jisho), adds its meaning to the back of the card and makes it the gap of the
//!   reading card.
//! - Romaji that maps one to one to kana: は is always "ha", を "wo", ぢ "dji", づ "dzu", ん before a vowel "n'",
//!   small っ doubles the next consonant, きゃ is "kya".
//!
//! The state lives in `<profile>/study/jlab.json` (decks in Jlab mode, settings, per note: phase, gap, lookups).
use crate::http::{Http, Request};
use crate::json::Json;
use crate::kanji::{is_kanji, to_hiragana};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Levels of reading help.
pub const ROMAJI: u8 = 0;
pub const KANA: u8 = 1;
pub const KANA_KANJI: u8 = 2;
pub const FURIGANA: u8 = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// reading help on listening cards, reading cards and in the dictionary (ROMAJI..FURIGANA)
    pub listen: u8,
    pub read: u8,
    pub dict: u8,
    /// kana shown as kana inside romaji: "none", "hiragana", "all", "learned" (those learned in Learn)
    pub kana: String,
    /// how many kanji the user knows (kanji trainer)
    pub kanji: usize,
    /// days a card must be remembered for before it moves on (listening → reading → retired)
    pub retire_days: i64,
    /// the sentence also shows on the front of listening cards (off = audio only until the answer)
    pub listen_text: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { listen: ROMAJI, read: FURIGANA, dict: KANA, kana: "none".into(), kanji: 0, retire_days: 10, listen_text: true }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub word: String,
    pub reading: String,
    pub meaning: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteData {
    /// 0 listening, 1 reading, 2 retired
    pub phase: u8,
    /// the word asked as a gap on the reading card ("" = none)
    pub cloze: String,
    pub looks: Vec<Look>,
}

#[derive(Clone, Debug, Default)]
pub struct Jlab {
    pub decks: Vec<i64>,
    pub settings: Settings,
    pub notes: HashMap<i64, NoteData>,
    /// the note data before the last answer, for Undo (not saved)
    pub undo: Option<(i64, Option<NoteData>)>,
}

fn file(data: &Path) -> std::path::PathBuf {
    crate::study::dir(data).join("jlab.json")
}

impl Jlab {
    pub fn load(data: &Path) -> Jlab {
        std::fs::read_to_string(file(data)).ok().and_then(|t| Json::parse(&t).ok()).map(|j| Jlab::from_json(&j)).unwrap_or_default()
    }

    pub fn save(&self, data: &Path) {
        let f = file(data);
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = f.with_extension("json.tmp");
        if std::fs::write(&tmp, self.to_json().dump()).is_ok() {
            let _ = std::fs::rename(&tmp, &f);
        }
    }

    pub fn from_json(j: &Json) -> Jlab {
        let s = j.get("settings");
        let d = Settings::default();
        let lv = |k: &str, dflt: u8| s.get(k).as_i64().map(|v| v.clamp(0, 3) as u8).unwrap_or(dflt);
        let settings = Settings {
            listen: lv("listen", d.listen),
            read: lv("read", d.read),
            dict: lv("dict", d.dict),
            kana: s.get("kana").as_str().filter(|k| ["none", "hiragana", "all", "learned"].contains(k)).unwrap_or("none").to_string(),
            kanji: s.get("kanji").as_i64().unwrap_or(0).max(0) as usize,
            retire_days: s.get("retire_days").as_i64().unwrap_or(10).clamp(1, 365),
            listen_text: s.get("listen_text").as_bool().unwrap_or(true),
        };
        let mut notes = HashMap::new();
        for (k, v) in j.get("notes").entries() {
            let Ok(nid) = k.parse::<i64>() else { continue };
            let looks = v.get("looks").items().iter().map(|l| Look { word: l.get("word").str(), reading: l.get("reading").str(), meaning: l.get("meaning").str() }).collect();
            notes.insert(nid, NoteData { phase: v.get("phase").as_i64().unwrap_or(0).clamp(0, 2) as u8, cloze: v.get("cloze").str(), looks });
        }
        Jlab { decks: j.get("decks").items().iter().filter_map(|x| x.as_i64()).collect(), settings, notes, undo: None }
    }

    pub fn to_json(&self) -> Json {
        let s = &self.settings;
        let mut ids: Vec<&i64> = self.notes.keys().collect();
        ids.sort();
        let notes: Vec<(String, Json)> = ids
            .into_iter()
            .map(|id| {
                let d = &self.notes[id];
                let looks: Vec<Json> = d.looks.iter().map(|l| Json::obj(vec![("word", l.word.as_str().into()), ("reading", l.reading.as_str().into()), ("meaning", l.meaning.as_str().into())])).collect();
                (id.to_string(), Json::obj(vec![("phase", (d.phase as i64).into()), ("cloze", d.cloze.as_str().into()), ("looks", Json::Arr(looks))]))
            })
            .collect();
        Json::obj(vec![
            ("decks", Json::Arr(self.decks.iter().map(|d| (*d).into()).collect())),
            (
                "settings",
                Json::obj(vec![
                    ("listen", (s.listen as i64).into()),
                    ("read", (s.read as i64).into()),
                    ("dict", (s.dict as i64).into()),
                    ("kana", s.kana.as_str().into()),
                    ("kanji", (s.kanji as i64).into()),
                    ("retire_days", s.retire_days.into()),
                    ("listen_text", s.listen_text.into()),
                ]),
            ),
            ("notes", Json::Obj(notes)),
        ])
    }

    /// True when the deck `did` (named `name`) is in Jlab mode, itself or through a parent deck.
    pub fn is_on(&self, did: i64, name: &str, deck_name: &dyn Fn(i64) -> String) -> bool {
        self.decks.iter().any(|&d| d == did || name.starts_with(&format!("{}::", deck_name(d))))
    }

    pub fn set_deck(&mut self, did: i64, on: bool) {
        self.decks.retain(|&d| d != did);
        if on {
            self.decks.push(did);
        }
    }

    pub fn note(&self, nid: i64) -> NoteData {
        self.notes.get(&nid).cloned().unwrap_or_default()
    }

    /// A setting changed from the page: `key` and its value.
    pub fn set(&mut self, key: &str, v: &Json) {
        let s = &mut self.settings;
        let lv = |v: &Json| v.as_i64().unwrap_or(0).clamp(0, 3) as u8;
        match key {
            "listen" => s.listen = lv(v),
            "read" => s.read = lv(v),
            "dict" => s.dict = lv(v),
            "kana" => {
                if let Some(k) = v.as_str().filter(|k| ["none", "hiragana", "all", "learned"].contains(k)) {
                    s.kana = k.to_string()
                }
            }
            "kanji" => s.kanji = v.as_i64().unwrap_or(0).clamp(0, kanji_order().len() as i64) as usize,
            "retire_days" => s.retire_days = v.as_i64().unwrap_or(10).clamp(1, 365),
            "listen_text" => s.listen_text = v.as_bool().unwrap_or(true),
            _ => {}
        }
    }

    /// A word was looked up: its meaning goes to the back and it becomes the gap of the reading card.
    pub fn add_look(&mut self, nid: i64, l: Look) {
        let d = self.notes.entry(nid).or_default();
        d.looks.retain(|x| x.word != l.word);
        d.cloze = l.word.clone();
        d.looks.push(l);
    }

    pub fn remove_look(&mut self, nid: i64, word: &str) {
        if let Some(d) = self.notes.get_mut(&nid) {
            d.looks.retain(|x| x.word != word);
            if d.cloze == word {
                d.cloze = d.looks.last().map(|l| l.word.clone()).unwrap_or_default();
            }
        }
    }

    /// After an answer: a card remembered for `retire_days` moves on. Returns the new phase when it changed
    /// (1 = turn into a reading card: reset it to new; 2 = retire it: suspend it).
    pub fn after_answer(&mut self, nid: i64, kind: u8, ivl: i64) -> Option<u8> {
        self.undo = Some((nid, self.notes.get(&nid).cloned()));
        if kind != 2 || ivl < self.settings.retire_days {
            return None;
        }
        let d = self.notes.entry(nid).or_default();
        if d.phase >= 2 {
            return None;
        }
        d.phase += 1;
        Some(d.phase)
    }

    /// Undo of the last answer: the phase it had.
    pub fn undo_answer(&mut self) {
        if let Some((nid, old)) = self.undo.take() {
            match old {
                Some(d) => {
                    self.notes.insert(nid, d);
                }
                None => {
                    self.notes.remove(&nid);
                }
            }
        }
    }
}

// ---------- the kanji the user knows ----------

/// Every kanji of the table in the order of the kanji trainer: JLPT N5 first up to N1, then the others; fewer
/// strokes first.
pub fn kanji_order() -> &'static [char] {
    static O: std::sync::OnceLock<Vec<char>> = std::sync::OnceLock::new();
    O.get_or_init(|| {
        let mut v: Vec<(u32, u32, char)> = crate::kanji::all().map(|(c, k)| (if k.jlpt == 0 { 9 } else { 6 - k.jlpt.min(5) }, k.strokes, c)).collect();
        v.sort();
        v.into_iter().map(|x| x.2).collect()
    })
}

pub fn known_kanji(count: usize) -> HashSet<char> {
    kanji_order().iter().take(count).copied().collect()
}

/// The kana shown as kana in romaji, for the setting `kana` and the kana learned in Learn.
pub fn kana_set(kana: &str, learned: &[String]) -> HashSet<char> {
    let hira = ('\u{3041}'..='\u{3096}').chain(['ー']);
    let kata = ('\u{30a1}'..='\u{30f6}').chain(['ー']);
    match kana {
        "hiragana" => hira.collect(),
        "all" => hira.chain(kata).collect(),
        "learned" => learned.iter().flat_map(|s| s.chars()).collect(),
        _ => HashSet::new(),
    }
}

// ---------- the sentence: segments, words ----------

/// A piece of the sentence: kanji with their reading (hiragana), or other text with no reading ("").
#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub text: String,
    pub reading: String,
}

/// A word of the sentence.
#[derive(Clone, Debug, PartialEq)]
pub struct Tok {
    pub segs: Vec<Seg>,
}

pub fn is_kana(c: char) -> bool {
    ('\u{3041}'..='\u{3096}').contains(&c) || ('\u{30a1}'..='\u{30fa}').contains(&c) || c == 'ー' || c == 'ヽ' || c == 'ゝ'
}

fn is_word_char(c: char) -> bool {
    is_kana(c) || is_kanji(c) || c.is_alphanumeric()
}

impl Tok {
    pub fn text(&self) -> String {
        self.segs.iter().map(|s| s.text.as_str()).collect()
    }
    pub fn has_kanji(&self) -> bool {
        self.segs.iter().any(|s| s.text.chars().any(is_kanji))
    }
    /// Punctuation and other marks: no word to look up.
    pub fn punct(&self) -> bool {
        !self.text().chars().any(is_word_char)
    }
    /// The reading in hiragana; kanji without a reading are given as written.
    pub fn reading(&self) -> String {
        self.segs.iter().map(|s| if s.reading.is_empty() { to_hiragana(&s.text) } else { s.reading.clone() }).collect()
    }
    /// True when every kanji of the word has a reading.
    pub fn read_ok(&self) -> bool {
        self.segs.iter().all(|s| !s.reading.is_empty() || !s.text.chars().any(is_kanji))
    }
}

/// A field as plain text: ruby becomes the Anki furigana form `漢字[かんじ]`, sounds, tags and entities go.
pub fn plain(html: &str) -> String {
    let mut s = html.to_string();
    // <ruby>漢字<rt>かんじ</rt></ruby> -> " 漢字[かんじ]"
    for (a, b) in [("<rt>", "["), ("</rt>", "]"), ("<RT>", "["), ("</RT>", "]")] {
        s = s.replace(a, b);
    }
    for t in ["<rp>(</rp>", "<rp>)</rp>", "<rp>（</rp>", "<rp>）</rp>"] {
        s = s.replace(t, "");
    }
    s = s.replace("<ruby>", " ").replace("<br>", " ").replace("<br/>", " ").replace("<br />", " ").replace("<div>", " ");
    // [sound:x.mp3]
    while let Some(p) = s.find("[sound:") {
        match s[p..].find(']') {
            Some(e) => s.replace_range(p..p + e + 1, ""),
            None => break,
        }
    }
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
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").trim().to_string()
}

pub fn has_brackets(s: &str) -> bool {
    let Some(a) = s.find('[') else { return false };
    s[a..].find(']').is_some_and(|b| s[a + 1..a + b].chars().any(is_kana))
}

/// The segments of a text in the Anki furigana form ("私[わたし]は 学生[がくせい]です"). The reading belongs to the
/// text after the last space, or to the run of kanji just before the bracket when there is no space.
/// A plain space becomes a segment " " (a word boundary that is not shown).
pub fn parse_brackets(s: &str) -> Vec<Seg> {
    let cs: Vec<char> = s.chars().collect();
    let mut out: Vec<Seg> = vec![];
    let mut buf: Vec<char> = vec![];
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '[' {
            if let Some(close) = cs[i + 1..].iter().position(|&x| x == ']') {
                let reading = to_hiragana(&cs[i + 1..i + 1 + close].iter().collect::<String>());
                let mut start = buf.iter().rposition(|&x| x == ' ').map(|p| p + 1).unwrap_or(0);
                let mut k = buf.len();
                while k > start && is_kanji(buf[k - 1]) {
                    k -= 1;
                }
                if k < buf.len() && k > start {
                    // "お茶[おちゃ]" keeps its kana; "は学生[がくせい]" gives the reading to 学生 only
                    let prefix = to_hiragana(&buf[start..k].iter().collect::<String>());
                    if !reading.starts_with(&prefix) {
                        start = k;
                    }
                }
                push_plain(&mut out, &buf[..start]);
                let base: String = buf[start..].iter().collect();
                if !base.is_empty() {
                    out.push(Seg { text: base, reading: if reading.chars().any(is_kana) { reading } else { String::new() } });
                }
                buf.clear();
                i += close + 2;
                continue;
            }
        }
        buf.push(cs[i]);
        i += 1;
    }
    push_plain(&mut out, &buf);
    out
}

fn push_plain(out: &mut Vec<Seg>, cs: &[char]) {
    for &c in cs {
        out.push(Seg { text: if c.is_whitespace() { " ".into() } else { c.to_string() }, reading: String::new() });
    }
}

/// Gives the readings of a kana sentence (`reading`) to the kanji of `text`, matching the kana they share.
/// None when they do not fit.
pub fn align(text: &str, reading: &str) -> Option<Vec<Seg>> {
    let t: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let r: Vec<char> = to_hiragana(reading).chars().filter(|&c| is_kana(c)).collect();
    if t.len() > 200 || r.len() > 400 {
        return None;
    }
    let mut failed: HashSet<(usize, usize)> = HashSet::new();
    let mut out = vec![];
    if go(&t, &r, 0, 0, &mut failed, &mut out) {
        out.reverse();
        Some(out)
    } else {
        None
    }
}

fn go(t: &[char], r: &[char], ti: usize, ri: usize, failed: &mut HashSet<(usize, usize)>, out: &mut Vec<Seg>) -> bool {
    if ti == t.len() {
        return ri == r.len();
    }
    if failed.contains(&(ti, ri)) {
        return false;
    }
    let c = t[ti];
    let ok = if is_kanji(c) {
        let mut e = ti;
        while e < t.len() && is_kanji(t[e]) {
            e += 1;
        }
        let len = e - ti;
        let mut found = false;
        for take in 1..=(len * 5).min(r.len().saturating_sub(ri)) {
            if go(t, r, e, ri + take, failed, out) {
                out.push(Seg { text: t[ti..e].iter().collect(), reading: r[ri..ri + take].iter().collect() });
                found = true;
                break;
            }
        }
        found
    } else if is_kana(c) {
        let h = to_hiragana(&c.to_string());
        if ri < r.len() && (r[ri].to_string() == h || c == 'ー') && go(t, r, ti + 1, ri + 1, failed, out) {
            out.push(Seg { text: c.to_string(), reading: String::new() });
            true
        } else {
            false
        }
    } else if go(t, r, ti + 1, ri, failed, out) {
        out.push(Seg { text: c.to_string(), reading: String::new() });
        true
    } else {
        false
    };
    if !ok {
        failed.insert((ti, ri));
    }
    ok
}

/// The words of the sentence: `split` (the tokenizer of the knowledge state, or None) gives the word boundaries of
/// the plain text; spaces and punctuation are boundaries too, and a reading never splits.
/// A tokenizer: the words of a text, in order.
pub type Split<'a> = &'a dyn Fn(&str) -> Vec<String>;

pub fn words(segs: &[Seg], split: Option<Split>) -> Vec<Tok> {
    let text: String = segs.iter().filter(|s| s.text != " ").map(|s| s.text.as_str()).collect();
    let chars: Vec<char> = text.chars().collect();
    // boundaries as char positions in `text`
    let mut cut: HashSet<usize> = HashSet::new();
    let mut pos = 0;
    // a segment's char span; spaces cut
    let mut spans: Vec<(usize, usize, &Seg)> = vec![];
    for s in segs {
        if s.text == " " {
            cut.insert(pos);
            continue;
        }
        let n = s.text.chars().count();
        spans.push((pos, pos + n, s));
        pos += n;
    }
    match split {
        Some(f) => {
            let mut at = 0;
            for w in f(&text) {
                let wc: Vec<char> = w.chars().collect();
                if wc.is_empty() {
                    continue;
                }
                if let Some(p) = (at..chars.len().saturating_sub(wc.len() - 1)).find(|&p| chars[p..p + wc.len()] == wc[..]) {
                    cut.insert(p);
                    cut.insert(p + wc.len());
                    at = p + wc.len();
                }
            }
        }
        None => {
            // kana after kanji stays with them (okurigana); kanji after kana start a word
            for i in 1..chars.len() {
                let (a, b) = (chars[i - 1], chars[i]);
                if is_kanji(b) && !is_kanji(a) || is_kana(a) != is_kana(b) && !is_kanji(a) && !is_kanji(b) {
                    cut.insert(i);
                }
            }
        }
    }
    for i in 1..chars.len() {
        if !is_word_char(chars[i]) || !is_word_char(chars[i - 1]) {
            cut.insert(i);
        }
    }
    let mut out: Vec<Tok> = vec![];
    let mut cur: Vec<Seg> = vec![];
    for (a, _, s) in spans {
        if cut.contains(&a) && !cur.is_empty() {
            out.push(Tok { segs: std::mem::take(&mut cur) });
        }
        // a cut inside a segment with a reading is ignored
        cur.push(s.clone());
    }
    if !cur.is_empty() {
        out.push(Tok { segs: cur });
    }
    // neighbouring plain characters of one word join into one segment
    for t in out.iter_mut() {
        let mut segs: Vec<Seg> = vec![];
        for s in t.segs.drain(..) {
            match segs.last_mut() {
                Some(l) if l.reading.is_empty() && s.reading.is_empty() && !l.text.chars().any(is_kanji) && !s.text.chars().any(is_kanji) => l.text.push_str(&s.text),
                _ => segs.push(s),
            }
        }
        t.segs = segs;
    }
    out
}

/// The note's sentence as segments, from its sentence field and (when there is one) its reading field.
pub fn sentence(sentence_html: &str, reading_html: &str) -> Vec<Seg> {
    let s = plain(sentence_html);
    if has_brackets(&s) {
        return parse_brackets(&s);
    }
    let r = plain(reading_html);
    if has_brackets(&r) {
        return parse_brackets(&r);
    }
    if !r.is_empty() && s.chars().any(is_kanji) {
        if let Some(a) = align(&s, &r) {
            return a;
        }
    }
    parse_brackets(&s)
}

// ---------- which field is what ----------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fields {
    pub sentence: usize,
    pub reading: Option<usize>,
    pub translation: Option<usize>,
}

fn japanese(s: &str) -> bool {
    s.chars().any(|c| is_kana(c) || is_kanji(c))
}

/// The sentence, reading and translation fields of a note, by their names and what they hold.
pub fn pick_fields(names: &[String], values: &[String]) -> Fields {
    let low: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let val = |i: usize| values.get(i).map(|v| plain(v)).unwrap_or_default();
    let has = |i: usize, keys: &[&str]| keys.iter().any(|k| low[i].contains(k));
    let reading_keys = ["furigana", "reading", "hiragana", "kana", "読み", "yomi"];
    let tr_keys = ["translation", "meaning", "english", "tradu", "definition", "意味", "english", "português", "portugues", "back"];
    let sentence = (0..names.len())
        .find(|&i| has(i, &["jlab-kanji", "expression", "sentence", "japanese", "kanji", "target", "文", "front", "word"]) && !has(i, &reading_keys) && !has(i, &tr_keys) && japanese(&val(i)))
        .or_else(|| (0..names.len()).find(|&i| !has(i, &reading_keys) && japanese(&val(i))))
        .unwrap_or(0);
    let reading = (0..names.len()).find(|&i| i != sentence && has(i, &reading_keys) && japanese(&val(i)));
    let translation = (0..names.len())
        .find(|&i| i != sentence && Some(i) != reading && has(i, &tr_keys) && !val(i).is_empty())
        .or_else(|| (0..names.len()).find(|&i| i != sentence && Some(i) != reading && !val(i).is_empty() && !japanese(&val(i)) && !values[i].contains("[sound:") && !values[i].contains("<img")));
    Fields { sentence, reading, translation }
}

// ---------- romaji ----------

fn mora(h: char) -> &'static str {
    const T: &[(char, &str)] = &[
        ('あ', "a"), ('い', "i"), ('う', "u"), ('え', "e"), ('お', "o"),
        ('か', "ka"), ('き', "ki"), ('く', "ku"), ('け', "ke"), ('こ', "ko"),
        ('が', "ga"), ('ぎ', "gi"), ('ぐ', "gu"), ('げ', "ge"), ('ご', "go"),
        ('さ', "sa"), ('し', "shi"), ('す', "su"), ('せ', "se"), ('そ', "so"),
        ('ざ', "za"), ('じ', "ji"), ('ず', "zu"), ('ぜ', "ze"), ('ぞ', "zo"),
        ('た', "ta"), ('ち', "chi"), ('つ', "tsu"), ('て', "te"), ('と', "to"),
        ('だ', "da"), ('ぢ', "dji"), ('づ', "dzu"), ('で', "de"), ('ど', "do"),
        ('な', "na"), ('に', "ni"), ('ぬ', "nu"), ('ね', "ne"), ('の', "no"),
        ('は', "ha"), ('ひ', "hi"), ('ふ', "fu"), ('へ', "he"), ('ほ', "ho"),
        ('ば', "ba"), ('び', "bi"), ('ぶ', "bu"), ('べ', "be"), ('ぼ', "bo"),
        ('ぱ', "pa"), ('ぴ', "pi"), ('ぷ', "pu"), ('ぺ', "pe"), ('ぽ', "po"),
        ('ま', "ma"), ('み', "mi"), ('む', "mu"), ('め', "me"), ('も', "mo"),
        ('や', "ya"), ('ゆ', "yu"), ('よ', "yo"),
        ('ら', "ra"), ('り', "ri"), ('る', "ru"), ('れ', "re"), ('ろ', "ro"),
        ('わ', "wa"), ('ゐ', "wi"), ('ゑ', "we"), ('を', "wo"), ('ん', "n"), ('ゔ', "vu"),
        ('ぁ', "a"), ('ぃ', "i"), ('ぅ', "u"), ('ぇ', "e"), ('ぉ', "o"), ('ゃ', "ya"), ('ゅ', "yu"), ('ょ', "yo"), ('ゎ', "wa"),
        ('ゕ', "ka"), ('ゖ', "ke"),
    ];
    T.iter().find(|(k, _)| *k == h).map(|(_, v)| *v).unwrap_or("")
}

fn punct_romaji(c: char) -> Option<&'static str> {
    Some(match c {
        '。' => ".",
        '、' => ",",
        '？' => "?",
        '！' => "!",
        '「' | '」' | '『' | '』' => "\"",
        '・' => "·",
        '〜' | '～' => "~",
        '…' => "…",
        '（' => "(",
        '）' => ")",
        '　' => " ",
        _ => return None,
    })
}

/// One unit of kana: a mora with its small kana (きゃ, ファ), as written, and its romaji.
fn units(kana: &str) -> Vec<(String, String)> {
    let cs: Vec<char> = kana.chars().collect();
    let mut out: Vec<(String, String)> = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        let h = to_hiragana(&c.to_string()).chars().next().unwrap_or(c);
        let base = mora(h);
        if base.is_empty() || matches!(h, 'っ') {
            let r = match c {
                'ー' => out.last().and_then(|(_, r)| r.chars().last()).filter(|v| "aiueo".contains(*v)).map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                _ if h == 'っ' => "\u{1}".into(),
                _ => punct_romaji(c).map(|s| s.to_string()).unwrap_or_else(|| c.to_string()),
            };
            out.push((c.to_string(), r));
            i += 1;
            continue;
        }
        let mut written = c.to_string();
        let mut r = base.to_string();
        if let Some(&n) = cs.get(i + 1) {
            let nh = to_hiragana(&n.to_string()).chars().next().unwrap_or(n);
            let small_y = matches!(nh, 'ゃ' | 'ゅ' | 'ょ');
            let small_v = matches!(nh, 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ');
            if small_y && r.ends_with('i') && r.len() > 1 {
                let v = &mora(nh)[1..];
                r.pop();
                // し/ち/じ/ぢ + ゃ = sha, cha, ja, dja; the others ki + ゃ = kya
                if !(r.ends_with("sh") || r.ends_with("ch") || r.ends_with('j')) {
                    r.push('y');
                }
                r.push_str(v);
                written.push(n);
                i += 1;
            } else if small_v && h != 'ん' {
                let v = mora(nh);
                if r == "u" {
                    r = "w".into();
                } else {
                    r.pop();
                }
                r.push_str(v);
                written.push(n);
                i += 1;
            }
        }
        out.push((written, r));
        i += 1;
    }
    // ん before a vowel or y is "n'"; small っ doubles the next consonant
    for k in 0..out.len() {
        let next = out.get(k + 1).map(|(_, r)| r.clone()).unwrap_or_default();
        if out[k].1 == "n" && out[k].0 != "n" && next.starts_with(['a', 'i', 'u', 'e', 'o', 'y']) {
            out[k].1 = "n'".into();
        }
        if out[k].1 == "\u{1}" {
            out[k].1 = match next.chars().next() {
                Some(c) if c.is_ascii_lowercase() && !"aiueo".contains(c) => c.to_string(),
                _ => "'".into(),
            };
        }
    }
    out
}

/// Romaji of kana, one to one (Jlab rules).
pub fn romaji(kana: &str) -> String {
    units(kana).into_iter().map(|(_, r)| r).collect()
}

/// Romaji where the kana in `learned` stay as kana (kana trainer).
pub fn romaji_mixed(kana: &str, learned: &HashSet<char>) -> String {
    if learned.is_empty() {
        return romaji(kana);
    }
    units(kana).into_iter().map(|(w, r)| if w.chars().all(|c| learned.contains(&c)) && w.chars().any(is_kana) { w } else { r }).collect()
}

// ---------- what a word shows ----------

/// What a word shows at `level`: (the text, the furigana above it).
pub fn show(t: &Tok, level: u8, kana: &HashSet<char>, known: &HashSet<char>) -> (String, String) {
    if t.punct() {
        let s = t.text();
        return if level == ROMAJI { (s.chars().map(|c| punct_romaji(c).map(|x| x.to_string()).unwrap_or_else(|| c.to_string())).collect(), String::new()) } else { (s, String::new()) };
    }
    let read_ok = t.read_ok();
    match level {
        ROMAJI if read_ok => {
            // katakana stays katakana in the input so the trainer can keep it
            let k: String = t.segs.iter().map(|s| if s.reading.is_empty() { s.text.clone() } else { s.reading.clone() }).collect();
            (romaji_mixed(&k, kana), String::new())
        }
        KANA if read_ok => (t.segs.iter().map(|s| if s.reading.is_empty() { s.text.clone() } else { s.reading.clone() }).collect(), String::new()),
        KANA_KANJI if read_ok => (t.segs.iter().map(|s| if s.reading.is_empty() || s.text.chars().filter(|c| is_kanji(*c)).all(|c| known.contains(&c)) { s.text.clone() } else { s.reading.clone() }).collect(), String::new()),
        _ => {
            let unknown = t.segs.iter().any(|s| s.text.chars().any(|c| is_kanji(c) && !known.contains(&c)));
            (t.text(), if unknown && read_ok && t.has_kanji() { t.reading() } else { String::new() })
        }
    }
}

/// A word looked up gives its reading to the same word in the sentence that had none.
pub fn fill_readings(toks: &mut [Tok], looks: &[Look]) {
    for t in toks.iter_mut() {
        if t.read_ok() {
            continue;
        }
        let text = t.text();
        if let Some(l) = looks.iter().find(|l| l.word == text && !l.reading.is_empty()) {
            let segs = align(&text, &l.reading).unwrap_or_else(|| vec![Seg { text: text.clone(), reading: to_hiragana(&l.reading) }]);
            t.segs = segs;
        }
    }
}

// ---------- the dictionary ----------

/// Looks a word up on Jisho (the same public API the add-on's parser follows): an exact match of the written form or
/// the reading first, then a word that contains it.
pub fn lookup(http: &dyn Http, word: &str) -> Option<Look> {
    let url = format!("https://jisho.org/api/v1/search/words?keyword={}", crate::util::url_encode(word));
    let r = http.send(&Request::get(&url)).ok()?;
    if !r.ok() {
        return None;
    }
    let j = r.json().ok()?;
    look_from(&j, word)
}

pub fn look_from(j: &Json, word: &str) -> Option<Look> {
    let h = to_hiragana(word);
    let data = j.get("data").items();
    let meaning = |e: &Json| -> String {
        e.get("senses").items().iter().take(3).map(|s| s.get("english_definitions").strings().into_iter().take(3).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("; ")
    };
    for exact in [true, false] {
        for e in data.iter().take(8) {
            for jp in e.get("japanese").items() {
                let w = jp.get("word").str();
                let r = jp.get("reading").str();
                let hit = if exact { w == word || (w.is_empty() && to_hiragana(&r) == h) || (!word.chars().any(is_kanji) && to_hiragana(&r) == h) } else { !w.is_empty() && w.contains(word) };
                if hit {
                    let m = meaning(e);
                    if m.is_empty() {
                        continue;
                    }
                    return Some(Look { word: word.to_string(), reading: to_hiragana(&r), meaning: m });
                }
            }
        }
    }
    None
}

/// Strings of the Jlab pages (English, Portuguese, Spanish), joined to the app's table by i18n.rs.
pub const I18N: &[(&str, &str, &str)] = &[
    ("Jlab mode", "Modo Jlab", "Modo Jlab"),
    ("Jlab settings…", "Ajustes do Jlab…", "Ajustes de Jlab…"),
    ("Jlab mode on", "Modo Jlab ligado", "Modo Jlab activado"),
    ("Jlab mode off", "Modo Jlab desligado", "Modo Jlab desactivado"),
    ("Listening", "Escuta", "Escucha"),
    ("Reading", "Leitura", "Lectura"),
    ("Retired", "Aposentado", "Retirado"),
    ("Dictionary", "Dicionário", "Diccionario"),
    ("Romaji", "Romaji", "Romaji"),
    ("Kana", "Kana", "Kana"),
    ("Kana + known kanji", "Kana + kanji que sei", "Kana + kanji que sé"),
    ("Kanji + furigana", "Kanji + furigana", "Kanji + furigana"),
    ("Reading help on listening cards", "Ajuda de leitura nos cartões de escuta", "Ayuda de lectura en las tarjetas de escucha"),
    ("Reading help on reading cards", "Ajuda de leitura nos cartões de leitura", "Ayuda de lectura en las tarjetas de lectura"),
    ("Reading help in the dictionary", "Ajuda de leitura no dicionário", "Ayuda de lectura en el diccionario"),
    ("Kana trainer", "Treino de kana", "Entrenador de kana"),
    ("Kana that show as kana inside the romaji", "Kana que aparecem como kana no meio do romaji", "Kana que se muestran como kana dentro del romaji"),
    ("None", "Nenhum", "Ninguno"),
    ("Hiragana", "Hiragana", "Hiragana"),
    ("Hiragana and katakana", "Hiragana e katakana", "Hiragana y katakana"),
    ("Those I learned in Learn", "Os que aprendi no Aprender", "Los que aprendí en Aprender"),
    ("Kanji trainer", "Treino de kanji", "Entrenador de kanji"),
    ("Kanji I know", "Kanji que sei", "Kanji que sé"),
    ("Counted from JLPT N5 up to N1, fewer strokes first. Known kanji show without furigana.", "Contados do JLPT N5 até o N1, com menos traços primeiro. Os kanji que você sabe aparecem sem furigana.", "Contados desde el JLPT N5 hasta el N1, con menos trazos primero. Los kanji que sabes salen sin furigana."),
    ("Days to move on", "Dias para avançar", "Días para avanzar"),
    ("A listening card remembered for this many days becomes a reading card; a reading card remembered as long retires.", "Um cartão de escuta lembrado por tantos dias vira cartão de leitura; um cartão de leitura lembrado o mesmo tempo se aposenta.", "Una tarjeta de escucha recordada tantos días pasa a lectura; una de lectura recordada lo mismo se retira."),
    ("Sentence on the front of listening cards", "Frase na frente dos cartões de escuta", "Frase en el anverso de las tarjetas de escucha"),
    ("Off: only the audio until you show the answer", "Desligado: só o áudio até mostrar a resposta", "Desactivado: solo el audio hasta mostrar la respuesta"),
    ("Study Japanese sentences in two steps: first you listen, then you read. Tap a word to look it up; it becomes the gap of the reading card.", "Estude frases em japonês em duas etapas: primeiro você escuta, depois lê. Toque numa palavra para ver o significado; ela vira a lacuna do cartão de leitura.", "Estudia frases en japonés en dos pasos: primero escuchas, luego lees. Toca una palabra para buscarla; se vuelve el hueco de la tarjeta de lectura."),
    ("Use Jlab mode in this deck", "Usar o modo Jlab neste baralho", "Usar el modo Jlab en este mazo"),
    ("Tap a word to look it up", "Toque numa palavra para ver o significado", "Toca una palabra para buscarla"),
    ("Looking up…", "Procurando…", "Buscando…"),
    ("Word not found in the dictionary.", "Palavra não encontrada no dicionário.", "Palabra no encontrada en el diccionario."),
    ("Hint", "Dica", "Pista"),
    ("Remove", "Remover", "Quitar"),
    ("This listening card is now a reading card.", "Este cartão de escuta virou cartão de leitura.", "Esta tarjeta de escucha ahora es de lectura."),
    ("You know this sentence: the card retires.", "Você já sabe esta frase: o cartão se aposenta.", "Ya sabes esta frase: la tarjeta se retira."),
    ("No reading for this sentence: look a word up to get one.", "Sem leitura para esta frase: procure uma palavra para obter uma.", "Sin lectura para esta frase: busca una palabra para obtenerla."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn romaji_follows_jlab_rules() {
        assert_eq!(romaji("わたしはがくせいです"), "watashihagakuseidesu");
        assert_eq!(romaji("ほんをよむ"), "honwoyomu");
        assert_eq!(romaji("ろっく"), "rokku");
        assert_eq!(romaji("きゃく"), "kyaku");
        assert_eq!(romaji("しゃしん"), "shashin");
        assert_eq!(romaji("ちゃ"), "cha");
        assert_eq!(romaji("じゃ"), "ja");
        assert_eq!(romaji("ぢづ"), "djidzu");
        assert_eq!(romaji("きんえん"), "kin'en");
        assert_eq!(romaji("こんや"), "kon'ya");
        assert_eq!(romaji("ファン"), "fan");
        assert_eq!(romaji("コーヒー"), "koohii");
        assert_eq!(romaji("まっちゃ"), "maccha");
        assert_eq!(romaji("あっ。"), "a'.");
    }

    #[test]
    fn kana_trainer_keeps_learned_kana() {
        let k: HashSet<char> = "あいうえお".chars().collect();
        assert_eq!(romaji_mixed("あかい", &k), "あkaい");
        // a digraph stays kana only when both are learned
        let k2: HashSet<char> = "き".chars().collect();
        assert_eq!(romaji_mixed("きゃき", &k2), "kyaき");
    }

    #[test]
    fn furigana_form_is_parsed() {
        let s = parse_brackets("私[わたし]は 学生[がくせい]です。");
        assert_eq!(s[0], Seg { text: "私".into(), reading: "わたし".into() });
        assert!(s.iter().any(|x| x.text == "学生" && x.reading == "がくせい"));
        let s = parse_brackets("お茶[おちゃ]を 飲[の]む");
        assert_eq!(s[0].text, "お茶");
        let s = parse_brackets("は学生[がくせい]");
        assert_eq!(s.last().unwrap().text, "学生");
    }

    #[test]
    fn kana_reading_is_aligned() {
        let a = align("私は学生です", "わたしはがくせいです").unwrap();
        assert_eq!(a[0], Seg { text: "私".into(), reading: "わたし".into() });
        assert!(a.iter().any(|x| x.text == "学生" && x.reading == "がくせい"));
        assert!(align("猫", "").is_none());
    }

    #[test]
    fn words_and_levels() {
        let segs = parse_brackets("夏休[なつやす]み に 東京[とうきょう] へ 行[い]った。");
        let toks = words(&segs, None);
        let texts: Vec<String> = toks.iter().map(|t| t.text()).collect();
        assert_eq!(texts, vec!["夏休み", "に", "東京", "へ", "行った", "。"]);
        let none = HashSet::new();
        let known: HashSet<char> = "東京".chars().collect();
        assert_eq!(show(&toks[0], ROMAJI, &none, &none).0, "natsuyasumi");
        assert_eq!(show(&toks[2], KANA, &none, &none).0, "とうきょう");
        assert_eq!(show(&toks[2], KANA_KANJI, &none, &known).0, "東京");
        assert_eq!(show(&toks[0], KANA_KANJI, &none, &known).0, "なつやすみ");
        assert_eq!(show(&toks[2], FURIGANA, &none, &known), ("東京".into(), String::new()));
        assert_eq!(show(&toks[4], FURIGANA, &none, &known), ("行った".into(), "いった".into()));
        assert_eq!(show(&toks[5], ROMAJI, &none, &none).0, ".");
    }

    #[test]
    fn tokenizer_boundaries_are_used() {
        let segs = parse_brackets("私[わたし]は学生[がくせい]です");
        let split = |_: &str| vec!["私".to_string(), "は".into(), "学生".into(), "です".into()];
        let toks = words(&segs, Some(&split));
        let texts: Vec<String> = toks.iter().map(|t| t.text()).collect();
        assert_eq!(texts, vec!["私", "は", "学生", "です"]);
    }

    #[test]
    fn fields_are_found() {
        let names: Vec<String> = ["Expression", "Reading", "Meaning", "Audio"].iter().map(|s| s.to_string()).collect();
        let values: Vec<String> = ["猫が好き", "ねこがすき", "I like cats", "[sound:a.mp3]"].iter().map(|s| s.to_string()).collect();
        assert_eq!(pick_fields(&names, &values), Fields { sentence: 0, reading: Some(1), translation: Some(2) });
        let names: Vec<String> = ["Front", "Back"].iter().map(|s| s.to_string()).collect();
        let values: Vec<String> = ["<b>猫[ねこ]</b>が好[す]き[sound:x.mp3]", "Gosto de gatos"].iter().map(|s| s.to_string()).collect();
        let f = pick_fields(&names, &values);
        assert_eq!((f.sentence, f.translation), (0, Some(1)));
        assert_eq!(plain(&values[0]), "猫[ねこ]が好[す]き");
    }

    #[test]
    fn phases_move_on_and_undo() {
        let mut j = Jlab::default();
        assert_eq!(j.after_answer(7, 2, 3), None);
        assert_eq!(j.after_answer(7, 2, 12), Some(1));
        assert_eq!(j.after_answer(7, 2, 12), Some(2));
        j.undo_answer();
        assert_eq!(j.note(7).phase, 1);
        assert_eq!(j.after_answer(7, 2, 12), Some(2));
        assert_eq!(j.after_answer(7, 2, 40), None);
        let back = Jlab::from_json(&Json::parse(&j.to_json().dump()).unwrap());
        assert_eq!(back.note(7).phase, 2);
    }

    #[test]
    fn looks_become_the_gap() {
        let mut j = Jlab::default();
        j.add_look(1, Look { word: "猫".into(), reading: "ねこ".into(), meaning: "cat".into() });
        j.add_look(1, Look { word: "好き".into(), reading: "すき".into(), meaning: "liked".into() });
        assert_eq!(j.note(1).cloze, "好き");
        j.remove_look(1, "好き");
        assert_eq!(j.note(1).cloze, "猫");
        let mut toks = words(&parse_brackets("猫 が 好き"), None);
        fill_readings(&mut toks, &j.note(1).looks);
        assert_eq!(toks[0].reading(), "ねこ");
    }

    #[test]
    fn jisho_answer_is_read() {
        let j = Json::parse(r#"{"data":[{"japanese":[{"word":"夏休み","reading":"なつやすみ"}],"senses":[{"english_definitions":["summer vacation","summer holiday"]}]}]}"#).unwrap();
        let l = look_from(&j, "夏休み").unwrap();
        assert_eq!((l.reading.as_str(), l.meaning.as_str()), ("なつやすみ", "summer vacation, summer holiday"));
        assert_eq!(look_from(&j, "夏").unwrap().reading, "なつやすみ");
        let k = Json::parse(r#"{"data":[{"japanese":[{"reading":"すごい"}],"senses":[{"english_definitions":["amazing"]}]}]}"#).unwrap();
        assert_eq!(look_from(&k, "すごい").unwrap().meaning, "amazing");
    }

    #[test]
    fn kanji_order_starts_with_n5() {
        let o = kanji_order();
        assert!(o.len() > 2000);
        let first: Vec<u32> = o.iter().take(50).filter_map(|c| crate::kanji::info(*c)).map(|k| k.jlpt).collect();
        assert!(first.iter().all(|&j| j == 5));
    }
}
