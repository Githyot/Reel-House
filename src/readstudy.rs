//! Study tricks of the player brought to the readers (docs/study-in-readers.md):
//! - word mixing: in a text written in a language the user knows, the words already learned in the study language
//!   are written in the study language (highlighted, the original word in a tooltip);
//! - quiz cards: in a text written in the study language, a sentence whose words are all known becomes a small card
//!   ("do you understand this?") every few paragraphs; in the Images mode, a speech bubble that is all known offers
//!   the same card.
//!
//! What "known" means comes from the knowledge file (`crate::knowledge`). The word mixing itself is the player's
//! (`Core::wordmix_swap`, `controller::wordmix`); this module turns its spans into pieces the reader draws.
use crate::controller::wordmix::Span;
use crate::json::Json;
use crate::knowledge::Knowledge;

/// A piece of a paragraph: plain text, or a word written in the study language.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    Swap { word: String, reading: String, original: String },
}

/// The pieces of a mixed text (`wordmix::pieces`), plain runs joined.
pub fn from_wordmix(parts: Vec<(String, Option<Span>)>, out: &mut Vec<Piece>) {
    for (t, sp) in parts {
        match sp {
            Some(sp) => out.push(Piece::Swap { word: sp.word, reading: sp.reading, original: sp.original }),
            None => match out.last_mut() {
                Some(Piece::Text(prev)) => prev.push_str(&t),
                _ => out.push(Piece::Text(t)),
            },
        }
    }
}

/// A paragraph cut into its sentences with the spaces after each kept, so joining them gives the paragraph back.
pub fn sentence_parts(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        let end = matches!(c, '。' | '！' | '？' | '!' | '?') || (c == '.' && chars.peek().map(|n| n.is_whitespace()).unwrap_or(true));
        if end {
            while let Some(&n) = chars.peek() {
                if matches!(n, '」' | '』' | '"' | '”' | '’' | ')' | '）') || n.is_whitespace() {
                    cur.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn pieces_json(p: &[Piece]) -> Json {
    Json::Arr(
        p.iter()
            .map(|x| match x {
                Piece::Text(t) => Json::obj(vec![("t", t.as_str().into())]),
                Piece::Swap { word, reading, original } => Json::obj(vec![("w", word.as_str().into()), ("r", reading.as_str().into()), ("o", original.as_str().into())]),
            })
            .collect(),
    )
}

/// The sentences of a paragraph, with their end marks.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        let end = matches!(c, '。' | '！' | '？' | '!' | '?') || (c == '.' && chars.peek().map(|n| n.is_whitespace()).unwrap_or(true));
        if end {
            // closing quotes stay with their sentence
            while let Some(&n) = chars.peek() {
                if matches!(n, '」' | '』' | '"' | '”' | '’' | ')' | '）') {
                    cur.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            let t = cur.trim().to_string();
            if !t.is_empty() {
                out.push(t);
            }
            cur.clear();
        }
    }
    let t = cur.trim().to_string();
    if !t.is_empty() {
        out.push(t);
    }
    out
}

/// Sentences for quiz cards: in every run of `every` paragraphs, the first sentence whose words are all known
/// (3 to 12 words, docs/study-in-readers.md §2). Returns (paragraph index, sentence).
pub fn quiz_picks(k: &Knowledge, lang: &str, paras: &[String], every: usize) -> Vec<(usize, String)> {
    let every = every.max(1);
    let mut out = vec![];
    let mut i = 0;
    while i < paras.len() {
        let end = (i + every).min(paras.len());
        'run: for (p, text) in paras.iter().enumerate().take(end).skip(i) {
            for s in sentences(text) {
                let n = k.tokens(lang, &s).len();
                if (3..=12).contains(&n) && k.line_known(lang, &s) {
                    out.push((p, s));
                    break 'run;
                }
            }
        }
        i = end;
    }
    out
}

/// How many paragraphs apart the quiz cards are, from `study_quiz_rate` (100: every 4 paragraphs; 1: every 30).
pub fn quiz_every(rate: u32) -> usize {
    let r = rate.clamp(1, 100) as usize;
    30 - (r * 26) / 100
}

/// For the bubbles of a page: which texts are in the study language and all known.
pub fn known_texts(k: &Knowledge, lang: &str, texts: &[String]) -> Vec<bool> {
    texts.iter().map(|t| !lang.is_empty() && k.line_known(lang, t)).collect()
}

/// The 3-letter code of a language as a plugin of the Text mode names it ("English", "Português", "日本語",
/// "Japanese", "pt-BR"…), or "" when it is not known.
pub fn lang3_of(label: &str) -> String {
    let l = label.trim();
    if l.is_empty() {
        return String::new();
    }
    let low = l.to_lowercase();
    let english: &[(&str, &str)] = &[("english", "en"), ("portuguese", "pt"), ("spanish", "es"), ("japanese", "ja"), ("french", "fr"), ("german", "de"), ("italian", "it"), ("korean", "ko"), ("chinese", "zh"), ("russian", "ru")];
    for (name, code) in english {
        if low.starts_with(name) {
            return crate::learn::lang3(code);
        }
    }
    for code in ["en", "pt", "es", "ja", "fr", "de", "it", "ko", "zh", "ru"] {
        if crate::util::lang_name(code).to_lowercase() == low {
            return crate::learn::lang3(code);
        }
    }
    // a code ("pt-BR", "jpn", "en")
    let base = crate::util::lang_base(l);
    if base.len() == 2 && base.chars().all(|c| c.is_ascii_alphabetic()) {
        return crate::learn::lang3(&base);
    }
    String::new()
}

/// A text without a known language: Japanese is told by its kana.
pub fn guess_lang3(sample: &str) -> String {
    let kana = sample.chars().filter(|c| crate::knowledge::is_kana(*c)).count();
    if kana >= 10 {
        "jpn".to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_spans_become_pieces() {
        let sp = Span { start: 2, end: 3, word: "猫".into(), original: "gato".into(), reading: "ねこ".into() };
        let mut out = vec![Piece::Text("Um ".into())];
        from_wordmix(vec![("O ".into(), None), ("猫".into(), Some(sp)), (" dorme.".into(), None)], &mut out);
        assert_eq!(out, vec![Piece::Text("Um O ".into()), Piece::Swap { word: "猫".into(), reading: "ねこ".into(), original: "gato".into() }, Piece::Text(" dorme.".into())]);
    }

    #[test]
    fn sentence_parts_join_back() {
        let t = "Ele veio.  Ela não! «Sim?» disse 2.5 vezes";
        let parts = sentence_parts(t);
        assert_eq!(parts.concat(), t);
        assert_eq!(parts[0], "Ele veio.  ");
        assert_eq!(sentence_parts("猫です。犬です").len(), 2);
        assert_eq!(quiz_every(100), 4);
        assert_eq!(quiz_every(1), 30);
        assert_eq!(quiz_every(0), 30);
    }

    #[test]
    fn sentences_split_with_their_marks() {
        assert_eq!(sentences("Hi there. How are you? Fine!"), vec!["Hi there.", "How are you?", "Fine!"]);
        assert_eq!(sentences("「猫です。」犬です"), vec!["「猫です。」", "犬です"]);
        assert_eq!(sentences("Version 2.5 is out."), vec!["Version 2.5 is out."]);
    }

    #[test]
    fn quiz_cards_come_from_fully_known_sentences() {
        let mut k = Knowledge::default();
        for w in ["i", "like", "cats", "dogs"] {
            k.record_at("eng", w, true, 1_000);
        }
        let paras: Vec<String> = vec!["I like cats. Something unknown here.".into(), "I like dogs.".into(), "Nothing.".into(), "I like cats.".into()];
        let picks = quiz_picks(&k, "eng", &paras, 2);
        assert_eq!(picks, vec![(0, "I like cats.".to_string()), (3, "I like cats.".to_string())]);
        assert_eq!(known_texts(&k, "eng", &["I like dogs".into(), "I like birds".into()]), vec![true, false]);
        assert_eq!(known_texts(&k, "", &["I like dogs".into()]), vec![false]);
    }

    #[test]
    fn plugin_languages_become_codes() {
        assert_eq!(lang3_of("English"), "eng");
        assert_eq!(lang3_of("Português"), "por");
        assert_eq!(lang3_of("日本語"), "jpn");
        assert_eq!(lang3_of("Spanish"), "spa");
        assert_eq!(lang3_of("pt-BR"), "por");
        assert_eq!(lang3_of(""), "");
        assert_eq!(lang3_of("Multi"), "");
        assert_eq!(guess_lang3("これはねこです。あれはいぬです。"), "jpn");
        assert_eq!(guess_lang3("plain text"), "");
    }
}
