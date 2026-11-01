//! Word mixing: a subtitle (or any text) in a language the user knows gets one or two of its words replaced by the
//! word of the language being learned, for the words the user already learned ("Eu quero comer agora" ->
//! "Eu quero 食べる agora"). Seeing a learned word in a sentence they understand anyway helps it stick.
//!
//! The pieces are plain functions so the player, the Text reader and the Images reader can share them:
//! - `Lexicon::build` makes the table "word of the known language -> learned word" from the knowledge state
//!   (`knowledge.md`, only words recalled well enough) and the course pack (translations, readings);
//! - `swap_line` replaces whole words (case kept, content words first, at most 1-2 per line) and says where the new
//!   words are (`Span`, character offsets in the new text) so the caller can colour them;
//! - `picked` decides, the same way every time, whether a line gets mixed at a given rate;
//! - `ass_line` draws a mixed line as ASS for mpv's `osd-overlay`, in the style of the user's subtitles.
//!
//! Part of `controller` as a module (`crate::controller::wordmix`); nothing here needs `Core`.
use crate::knowledge::Knowledge;
use crate::learn::Pack;
use std::collections::HashMap;

/// The lowest recall chance (knowledge.md "strength") of a word for it to be mixed into subtitles.
pub const MIX_MIN_STRENGTH: f64 = 0.4;

/// A word of the learned language that a known word can be replaced with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    /// The learned word ("食べる").
    pub target: String,
    /// Its reading when it has one (Japanese kana: "たべる").
    pub reading: String,
    /// Its recall chance now (0..1).
    pub strength: f64,
}

/// A replaced word in the mixed text: `start..end` are character offsets (not bytes) in the new text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    /// The learned word put there.
    pub word: String,
    /// The word of the known language it replaced, as it was written.
    pub original: String,
    pub reading: String,
}

/// "known word (lower case) -> learned word" for one pair of languages (3-letter codes).
#[derive(Clone, Debug, Default)]
pub struct Lexicon {
    /// The language of the text being mixed (a known one) and the language of the words put in (the study one).
    pub from: String,
    pub to: String,
    map: HashMap<String, Entry>,
    /// The most words a key has ("ice cream" = 2), for the phrase lookup.
    max_words: usize,
    /// The longest key in characters (texts without spaces: Japanese, Chinese).
    max_chars: usize,
}

/// Words that carry grammar more than meaning: never replaced (they would make the line hard to read and teach
/// little).
fn stop_word(lang: &str, w: &str) -> bool {
    const POR: &[&str] = &[
        "a", "o", "as", "os", "um", "uma", "uns", "umas", "e", "é", "de", "da", "do", "das", "dos", "em", "no", "na", "nos", "nas", "que", "se",
        "por", "para", "pra", "com", "sem", "ao", "à", "eu", "tu", "ele", "ela", "nós", "eles", "elas", "você", "vocês", "me", "te", "lhe", "não",
        "sim", "mas", "ou", "isso", "isto", "aquilo", "este", "esse", "esta", "essa", "meu", "minha", "seu", "sua", "foi", "ser", "ter", "há", "já",
    ];
    const ENG: &[&str] = &[
        "a", "an", "the", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "by", "from", "is", "are", "was", "were", "be", "been",
        "am", "i", "you", "he", "she", "it", "we", "they", "me", "him", "her", "us", "them", "my", "your", "his", "its", "our", "their", "this",
        "that", "these", "those", "not", "no", "yes", "do", "does", "did", "have", "has", "had", "will", "would", "can", "so", "if", "as",
    ];
    const SPA: &[&str] = &[
        "a", "el", "la", "los", "las", "un", "una", "unos", "unas", "y", "e", "o", "u", "de", "del", "al", "en", "que", "se", "por", "para", "con",
        "sin", "yo", "tú", "él", "ella", "nosotros", "ellos", "ellas", "usted", "me", "te", "le", "lo", "no", "sí", "pero", "es", "son", "fue",
        "ser", "mi", "tu", "su", "este", "ese", "esta", "esa", "eso", "esto", "ya", "muy",
    ];
    let list: &[&str] = match lang {
        "por" => POR,
        "eng" => ENG,
        "spa" => SPA,
        _ => &[],
    };
    list.contains(&w)
}

/// Languages written without spaces between words.
fn no_spaces(lang: &str) -> bool {
    matches!(lang, "jpn" | "cmn" | "zho" | "tha")
}

/// A character of a word (letters, digits, and the apostrophe / hyphen inside words like "don't", "guarda-chuva").
fn word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// The ways a translation can be met in a sentence: "to eat; to have (a meal)" -> ["eat", "have"]. Parentheses go,
/// the senses split at , ; /, and the article or the "to" of an English verb is dropped. Keys are lower case.
pub fn keys_of(lang: &str, translation: &str) -> Vec<String> {
    // without anything in parentheses or brackets
    let mut clean = String::new();
    let mut depth = 0i32;
    for c in translation.chars() {
        match c {
            '(' | '[' | '（' => depth += 1,
            ')' | ']' | '）' => depth = (depth - 1).max(0),
            _ if depth == 0 => clean.push(c),
            _ => {}
        }
    }
    let lead: &[&str] = match lang {
        "eng" => &["to ", "a ", "an ", "the "],
        "por" => &["o ", "a ", "os ", "as ", "um ", "uma "],
        "spa" => &["el ", "la ", "los ", "las ", "un ", "una "],
        _ => &[],
    };
    let mut out: Vec<String> = vec![];
    for sense in clean.split([',', ';', '/', '；', '、']) {
        let mut k = sense.trim().to_lowercase();
        for l in lead {
            if let Some(r) = k.strip_prefix(l) {
                k = r.trim().to_string();
            }
        }
        let k: String = k.trim_matches(|c: char| !word_char(c)).to_string();
        let words = k.split_whitespace().count();
        if k.chars().count() < 2 || words == 0 || words > 3 || (words == 1 && stop_word(lang, &k)) || out.contains(&k) {
            continue;
        }
        out.push(k);
    }
    out
}

impl Lexicon {
    pub fn new(from: &str, to: &str) -> Lexicon {
        Lexicon { from: from.to_string(), to: to.to_string(), ..Lexicon::default() }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Adds a learned word under every way its translation can be met. When two learned words share a key, the
    /// better known one stays (it is the one the user will recognise).
    pub fn add(&mut self, translation: &str, target: &str, reading: &str, strength: f64) {
        if target.trim().is_empty() {
            return;
        }
        for k in keys_of(&self.from, translation) {
            if self.map.get(&k).map(|e| e.strength >= strength).unwrap_or(false) {
                continue;
            }
            self.max_words = self.max_words.max(k.split_whitespace().count());
            self.max_chars = self.max_chars.max(k.chars().count());
            let reading = if reading == target { String::new() } else { reading.to_string() };
            self.map.insert(k, Entry { target: target.trim().to_string(), reading, strength });
        }
    }

    /// The learned word for a known word or phrase (any case).
    pub fn lookup(&self, key: &str) -> Option<&Entry> {
        self.map.get(&key.to_lowercase())
    }

    /// The table for mixing `study` words (3-letter code) into `from` texts: the words of the knowledge state
    /// recalled with at least `min_strength` (their translation from knowledge.md, else from the course pack), and
    /// with `all_pack` every word of the pack too (strength 0; for naming a mined word, not for mixing).
    pub fn build(k: &Knowledge, pack: Option<&Pack>, study: &str, from: &str, min_strength: f64, all_pack: bool, now: i64) -> Lexicon {
        let mut lex = Lexicon::new(from, study);
        if study.is_empty() || from.is_empty() || study == from {
            return lex;
        }
        let pack = pack.filter(|p| p.lang == study);
        // particles and the like teach nothing in a sentence of another language
        let grammar = |pos: &str| {
            let p = pos.to_lowercase();
            p.contains("particle") || p.starts_with("prt") || p.contains("aux") || p.contains("conj") || p.contains("suffix") || p.contains("prefix")
        };
        for r in k.rows(study) {
            let st = r.strength(now);
            if r.right == 0 || st < min_strength {
                continue;
            }
            let pw = pack.and_then(|p| p.word(&r.word));
            if pw.map(|w| grammar(&w.pos)).unwrap_or(false) {
                continue;
            }
            let mut tr = r.tr(from).to_string();
            if tr.is_empty() {
                tr = pw.map(|w| w.tr.iter().find(|(l, _)| l == from).map(|(_, t)| t.clone()).unwrap_or_default()).unwrap_or_default();
            }
            let reading = if r.reading.is_empty() { pw.map(|w| w.r.clone()).unwrap_or_default() } else { r.reading.clone() };
            lex.add(&tr, &r.word, &reading, st);
        }
        if all_pack {
            if let Some(p) = pack {
                for w in &p.words {
                    if grammar(&w.pos) {
                        continue;
                    }
                    let tr = w.tr.iter().find(|(l, _)| l == from).map(|(_, t)| t.clone()).unwrap_or_default();
                    lex.add(&tr, &w.w, &w.r, 0.0);
                }
            }
        }
        lex
    }
}

/// The new word in the case of the one it replaces: "Comer" -> "Eat", "COMER" -> "EAT"; scripts without case
/// (Japanese) stay as they are.
fn with_case(original: &str, target: &str) -> String {
    let letters: Vec<char> = original.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() || !target.chars().any(|c| c.is_lowercase() || c.is_uppercase()) {
        return target.to_string();
    }
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return target.to_uppercase();
    }
    if letters[0].is_uppercase() {
        let mut cs = target.chars();
        return match cs.next() {
            Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
            None => String::new(),
        };
    }
    target.to_string()
}

/// A word of the text: its byte range.
fn word_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut start: Option<usize> = None;
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        // an apostrophe or hyphen between two letters belongs to the word ("don't", "guarda-chuva")
        let inner = matches!(c, '\'' | '’' | '-') && start.is_some() && it.peek().map(|(_, n)| word_char(*n)).unwrap_or(false);
        if word_char(c) || inner {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            out.push((s, i));
        }
    }
    if let Some(s) = start {
        out.push((s, text.len()));
    }
    out
}

/// The words of a text written with spaces, as written (punctuation around them dropped).
pub fn words(text: &str) -> Vec<String> {
    word_ranges(text).into_iter().map(|(a, b)| text[a..b].to_string()).collect()
}

/// A candidate replacement: byte range in the text, the entry, how many words it covers.
struct Hit<'a> {
    a: usize,
    b: usize,
    entry: &'a Entry,
    chars: usize,
}

/// Replaces up to `max` words of `text` (a line in `lex.from`) with learned words. Whole words or phrases only,
/// never a grammar word, the longest (most meaningful) words first and, among equals, the one learned less well
/// (seeing it again helps most). A line of a single word is left alone (no sentence around it to guess from).
/// Returns the new text and where the new words are.
pub fn swap_line(lex: &Lexicon, text: &str, max: usize) -> (String, Vec<Span>) {
    if lex.is_empty() || max == 0 || text.trim().is_empty() {
        return (text.to_string(), vec![]);
    }
    let mut hits: Vec<Hit> = vec![];
    if no_spaces(&lex.from) {
        // no spaces: the longest key at each place (two characters at least)
        let idx: Vec<(usize, char)> = text.char_indices().collect();
        let mut i = 0;
        while i < idx.len() {
            let mut found = false;
            for len in (2..=lex.max_chars.min(idx.len() - i)).rev() {
                let a = idx[i].0;
                let b = if i + len < idx.len() { idx[i + len].0 } else { text.len() };
                if let Some(e) = lex.map.get(&text[a..b]) {
                    hits.push(Hit { a, b, entry: e, chars: len });
                    i += len;
                    found = true;
                    break;
                }
            }
            if !found {
                i += 1;
            }
        }
    } else {
        let words = word_ranges(text);
        if words.len() < 2 {
            return (text.to_string(), vec![]);
        }
        let mut i = 0;
        while i < words.len() {
            let mut step = 1;
            for n in (1..=lex.max_words.max(1).min(words.len() - i)).rev() {
                // the words of a phrase are separated by spaces only
                if (i..i + n - 1).any(|j| !text[words[j].1..words[j + 1].0].chars().all(char::is_whitespace)) {
                    continue;
                }
                let (a, b) = (words[i].0, words[i + n - 1].1);
                let key = text[a..b].split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
                if n == 1 && stop_word(&lex.from, &key) {
                    continue;
                }
                if let Some(e) = lex.map.get(&key) {
                    hits.push(Hit { a, b, entry: e, chars: key.chars().count() });
                    step = n;
                    break;
                }
            }
            i += step;
        }
    }
    if hits.is_empty() {
        return (text.to_string(), vec![]);
    }
    // the best ones, then back in the order of the text
    let mut order: Vec<usize> = (0..hits.len()).collect();
    order.sort_by(|&x, &y| {
        hits[y].chars.cmp(&hits[x].chars).then(hits[x].entry.strength.partial_cmp(&hits[y].entry.strength).unwrap_or(std::cmp::Ordering::Equal)).then(x.cmp(&y))
    });
    let mut chosen: Vec<usize> = order.into_iter().take(max).collect();
    chosen.sort_by_key(|&i| hits[i].a);
    let mut out = String::new();
    let mut spans = vec![];
    let mut at = 0;
    for i in chosen {
        let h = &hits[i];
        out.push_str(&text[at..h.a]);
        let original = &text[h.a..h.b];
        let word = with_case(original, &h.entry.target);
        let start = out.chars().count();
        out.push_str(&word);
        spans.push(Span { start, end: start + word.chars().count(), word, original: original.to_string(), reading: h.entry.reading.clone() });
        at = h.b;
    }
    out.push_str(&text[at..]);
    (out, spans)
}

/// The mixed text cut into pieces for a reader that draws them itself: (text, None) for plain text, (word, Some(span))
/// for a learned word.
pub fn pieces(text: &str, spans: &[Span]) -> Vec<(String, Option<Span>)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut at = 0;
    for s in spans {
        if s.start < at || s.end > chars.len() {
            continue;
        }
        if s.start > at {
            out.push((chars[at..s.start].iter().collect(), None));
        }
        out.push((chars[s.start..s.end].iter().collect(), Some(s.clone())));
        at = s.end;
    }
    if at < chars.len() {
        out.push((chars[at..].iter().collect(), None));
    }
    out
}

/// How many words a line may get: one, two in a long line (six words or more).
pub fn max_for(text: &str) -> usize {
    if text.split_whitespace().count() >= 6 {
        2
    } else {
        1
    }
}

/// Whether a line is mixed at `rate` (0..100 % of the lines): decided from the text itself, so the same line gets
/// the same answer on every tick, after a seek and when watched again.
pub fn picked(text: &str, rate: u32) -> bool {
    if rate >= 100 {
        return true;
    }
    // FNV-1a
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.trim().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    (h % 100) < rate as u64
}

/// The look of the subtitles, read from mpv's options so the mixed line looks like the ones mpv draws.
#[derive(Clone, Debug)]
pub struct Look {
    pub font: String,
    /// In the 720-line space of mpv's subtitles.
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    /// mpv colours "#AARRGGBB".
    pub color: String,
    pub border_color: String,
    pub border: f64,
    pub shadow_color: String,
    pub shadow: f64,
    /// Background box instead of an outline: its colour.
    pub back: Option<String>,
    /// 0 top .. 100 bottom.
    pub pos: f64,
    pub margin_y: f64,
    pub margin_x: f64,
    /// "left" | "center" | "right"
    pub align: String,
    /// The colour of the mixed words (#RRGGBB).
    pub highlight: String,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            font: "Sans".into(),
            size: 55.0,
            bold: false,
            italic: false,
            color: "#FFFFFFFF".into(),
            border_color: "#FF000000".into(),
            border: 3.0,
            shadow_color: "#CC000000".into(),
            shadow: 0.0,
            back: None,
            pos: 100.0,
            margin_y: 22.0,
            margin_x: 25.0,
            align: "center".into(),
            highlight: "#FFD966".into(),
        }
    }
}

/// "#AARRGGBB" or "#RRGGBB" (mpv) to the ASS colour "&HBBGGRR&" and alpha "&HAA&" (ASS alpha: 00 = opaque).
pub fn ass_color(c: &str) -> (String, String) {
    let h = c.trim().trim_start_matches('#');
    let (a, rgb) = if h.len() >= 8 && h.is_ascii() { (u8::from_str_radix(&h[0..2], 16).unwrap_or(255), &h[2..8]) } else { (255, h) };
    let (r, g, b) = crate::substyle::parse_color(rgb);
    (format!("&H{:02X}{:02X}{:02X}&", b, g, r), format!("&H{:02X}&", 255 - a))
}

/// Text made safe for ASS: no override blocks, line breaks as \N.
fn ass_escape(s: &str) -> String {
    s.replace('\\', "\u{29F5}").replace('{', "｛").replace('}', "｝").replace("\r\n", "\\N").replace('\n', "\\N")
}

/// The mixed line as one ASS event for `osd-overlay` (`res_x` x 720), in the user's subtitle look, the learned words
/// in `look.highlight` and, with `reading`, their reading small after them ("食べる(たべる)").
pub fn ass_line(text: &str, spans: &[Span], look: &Look, res_x: f64, reading: bool) -> String {
    let (c1, a1) = ass_color(&look.color);
    let (c3, _) = ass_color(look.back.as_deref().unwrap_or(&look.border_color));
    let (c4, a4) = ass_color(&look.shadow_color);
    let (hl, _) = ass_color(&look.highlight);
    let (an, x) = match look.align.as_str() {
        "left" => (1, look.margin_x),
        "right" => (3, res_x - look.margin_x),
        _ => (2, res_x / 2.0),
    };
    let y = (720.0 * look.pos.clamp(0.0, 100.0) / 100.0 - look.margin_y).max(look.size);
    // a background box is drawn as a thick border in its colour (an overlay event cannot change the box style)
    let border = if look.back.is_some() { (look.size / 6.0).max(look.border) } else { look.border };
    let font = look.font.replace(['{', '}', '\\'], "");
    let mut o = format!(
        "{{\\an{}\\pos({:.0},{:.0})\\q0\\fn{}\\fs{:.0}\\b{}\\i{}\\bord{:.1}\\shad{:.1}\\1c{}\\1a{}\\3c{}\\4c{}\\4a{}}}",
        an,
        x,
        y,
        font,
        look.size,
        u8::from(look.bold),
        u8::from(look.italic),
        border,
        look.shadow,
        c1,
        a1,
        c3,
        c4,
        a4
    );
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    for s in spans {
        if s.start < at || s.end > chars.len() {
            continue;
        }
        o.push_str(&ass_escape(&chars[at..s.start].iter().collect::<String>()));
        o.push_str(&format!("{{\\1c{}}}", hl));
        o.push_str(&ass_escape(&chars[s.start..s.end].iter().collect::<String>()));
        if reading && !s.reading.is_empty() {
            o.push_str(&format!("{{\\fs{:.0}}}({}){{\\fs{:.0}}}", (look.size * 0.55).max(10.0), ass_escape(&s.reading), look.size));
        }
        o.push_str(&format!("{{\\1c{}}}", c1));
        at = s.end;
    }
    o.push_str(&ass_escape(&chars[at..].iter().collect::<String>()));
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex() -> Lexicon {
        let mut l = Lexicon::new("por", "jpn");
        l.add("comer", "食べる", "たべる", 0.9);
        l.add("a casa; o lar", "家", "いえ", 0.7);
        l.add("sorvete", "アイスクリーム", "", 0.8);
        l.add("de (preposição)", "の", "", 0.9);
        l
    }

    #[test]
    fn keys_drop_articles_and_senses() {
        assert_eq!(keys_of("eng", "to eat; to drink (a liquid)"), vec!["eat", "drink"]);
        // "to have" is a grammar word in English: no key
        assert_eq!(keys_of("eng", "to have"), Vec::<String>::new());
        assert_eq!(keys_of("por", "a casa, o lar"), vec!["casa", "lar"]);
        assert_eq!(keys_of("eng", "ice cream"), vec!["ice cream"]);
        // grammar words and too long phrases are no keys
        assert!(keys_of("por", "de").is_empty());
        assert!(keys_of("eng", "a thing that is said").is_empty());
    }

    #[test]
    fn swaps_whole_words_and_keeps_case() {
        let l = lex();
        let (t, s) = swap_line(&l, "Eu quero comer agora", 1);
        assert_eq!(t, "Eu quero 食べる agora");
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].start, s[0].end, s[0].original.as_str(), s[0].reading.as_str()), (9, 12, "comer", "たべる"));
        let p = pieces(&t, &s);
        assert_eq!(p.iter().map(|(x, sp)| (x.as_str(), sp.is_some())).collect::<Vec<_>>(), vec![("Eu quero ", false), ("食べる", true), (" agora", false)]);
        // inside another word: no ("comercial" is not "comer")
        assert_eq!(swap_line(&l, "Um anúncio comercial", 1).1.len(), 0);
        // grammar words are never swapped, even with a key
        assert_eq!(swap_line(&l, "A casa de Ana", 2).0, "A 家 de Ana");
        // case of a Latin target follows the original
        let mut e = Lexicon::new("por", "eng");
        e.add("comer", "eat", "", 0.9);
        e.add("casa", "house", "", 0.9);
        assert_eq!(swap_line(&e, "Comer agora!", 1).0, "Eat agora!");
        assert_eq!(swap_line(&e, "VAMOS COMER JÁ", 1).0, "VAMOS EAT JÁ");
        // two at most, the longer words first, in the order of the line
        let (t, s) = swap_line(&e, "eu vou comer em casa hoje à noite", 2);
        assert_eq!(t, "eu vou eat em house hoje à noite");
        assert_eq!(s.iter().map(|x| x.word.as_str()).collect::<Vec<_>>(), vec!["eat", "house"]);
        let (t, _) = swap_line(&e, "eu vou comer em casa", 1);
        assert_eq!(t, "eu vou eat em casa");
        // one word alone is left as it is
        assert_eq!(swap_line(&e, "Comer!", 1).1.len(), 0);
    }

    #[test]
    fn phrases_and_spaceless_text() {
        let mut e = Lexicon::new("eng", "jpn");
        e.add("ice cream", "アイス", "", 0.9);
        e.add("cream", "クリーム", "", 0.9);
        assert_eq!(swap_line(&e, "I want ice cream now", 1).0, "I want アイス now");
        assert_eq!(swap_line(&e, "I want ice, cream now", 1).0, "I want ice, クリーム now");
        let mut j = Lexicon::new("jpn", "eng");
        j.add("食べる", "eat", "", 0.9);
        let (t, s) = swap_line(&j, "ご飯を食べる。", 1);
        assert_eq!(t, "ご飯をeat。");
        assert_eq!((s[0].start, s[0].end), (3, 6));
    }

    #[test]
    fn lexicon_from_knowledge_keeps_learned_words_only() {
        let mut k = Knowledge::default();
        let now = 1_760_000_000;
        k.record_at("jpn", "食べる", true, now);
        k.set_info("jpn", "食べる", "たべる", &[("por".into(), "comer".into())]);
        k.record_at("jpn", "飲む", false, now);
        k.set_info("jpn", "飲む", "のむ", &[("por".into(), "beber".into())]);
        let l = Lexicon::build(&k, None, "jpn", "por", MIX_MIN_STRENGTH, false, now);
        assert_eq!(l.lookup("Comer").map(|e| e.target.as_str()), Some("食べる"));
        assert!(l.lookup("beber").is_none(), "never answered right: not learned");
        // a month later the word faded below the threshold
        assert!(Lexicon::build(&k, None, "jpn", "por", MIX_MIN_STRENGTH, false, now + 86400 * 30).is_empty());
    }

    #[test]
    fn rate_and_overlay() {
        let n = (0..1000).filter(|i| picked(&format!("line {}", i), 30)).count();
        assert!((200..400).contains(&n), "{}", n);
        assert!(picked("x", 100) && !picked("x", 0));
        assert_eq!(picked("same line", 50), picked("same line", 50));
        let (t, s) = swap_line(&lex(), "Eu quero comer agora", 1);
        let a = ass_line(&t, &s, &Look::default(), 1280.0, true);
        assert!(a.starts_with("{\\an2\\pos(640,698)"));
        assert!(a.contains("Eu quero {\\1c&H66D9FF&}食べる{\\fs30}(たべる){\\fs55}{\\1c&HFFFFFF&} agora"), "{}", a);
        assert_eq!(ass_color("#80FF0000"), ("&H0000FF&".to_string(), "&H7F&".to_string()));
        // braces of the text cannot open an override block
        assert!(!ass_line("a {b} c", &[], &Look::default(), 1280.0, false).contains("{b}"));
    }
}
