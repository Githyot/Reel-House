//! Kanji, readings and word lookup for the clips saved from the player (Study mode).
//! The kanji table is a trimmed copy of davidluzgouveia/kanji-data (meanings, on/kun readings, JLPT level, strokes);
//! words and their readings come from Jisho's public API, a short description from the Japanese Wikipedia.
use crate::http::{Http, Request};
use crate::json::Json;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Debug, Default)]
pub struct Kanji {
    pub strokes: u32,
    pub jlpt: u32,
    pub meanings: Vec<String>,
    pub on: Vec<String>,
    pub kun: Vec<String>,
}

static TABLE: OnceLock<HashMap<char, Kanji>> = OnceLock::new();

fn table() -> &'static HashMap<char, Kanji> {
    TABLE.get_or_init(|| {
        let mut m = HashMap::new();
        if let Ok(j) = Json::parse(include_str!("../assets/kanji-min.json")) {
            if let Json::Obj(items) = &j {
                for (k, v) in items.iter() {
                    let Some(c) = k.chars().next() else { continue };
                    m.insert(
                        c,
                        Kanji { strokes: v.get("s").as_i64().unwrap_or(0) as u32, jlpt: v.get("j").as_i64().unwrap_or(0) as u32, meanings: v.get("m").strings(), on: v.get("o").strings(), kun: v.get("k").strings() },
                    );
                }
            }
        }
        m
    })
}

pub fn info(c: char) -> Option<&'static Kanji> {
    table().get(&c)
}

pub fn is_kanji(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c) || c == '々'
}

pub fn to_katakana(s: &str) -> String {
    s.chars().map(|c| if ('\u{3041}'..='\u{3096}').contains(&c) { char::from_u32(c as u32 + 0x60).unwrap_or(c) } else { c }).collect()
}

pub fn to_hiragana(s: &str) -> String {
    s.chars().map(|c| if ('\u{30a1}'..='\u{30f6}').contains(&c) { char::from_u32(c as u32 - 0x60).unwrap_or(c) } else { c }).collect()
}

/// The distinct kanji of a text, in order.
pub fn kanji_of(text: &str) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    for c in text.chars() {
        if is_kanji(c) && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// The words of a sentence that have kanji: each run of kanji, with the hiragana that follow it (up to three) as a
/// second try (食べ, 行く). Without a morphological dictionary this is a good approximation.
pub fn candidates(text: &str) -> Vec<(String, String)> {
    let cs: Vec<char> = text.chars().collect();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        if is_kanji(cs[i]) {
            let a = i;
            while i < cs.len() && is_kanji(cs[i]) {
                i += 1;
            }
            let run: String = cs[a..i].iter().collect();
            let mut j = i;
            while j < cs.len() && j - i < 3 && ('\u{3041}'..='\u{3096}').contains(&cs[j]) {
                j += 1;
            }
            let with: String = cs[a..j].iter().collect();
            if !out.iter().any(|(r, _)| *r == run) {
                out.push((run, with));
            }
        } else {
            i += 1;
        }
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct Word {
    pub word: String,
    pub reading: String,
    pub meanings: Vec<String>,
}

/// Looks a word up on Jisho. `with_okurigana` is tried first (the dictionary form of a verb is found from 食べ by Jisho's own
/// search), then the bare run of kanji.
pub fn lookup_word(http: &dyn Http, run: &str, with_okurigana: &str) -> Option<Word> {
    for q in [with_okurigana, run] {
        let url = format!("https://jisho.org/api/v1/search/words?keyword={}", crate::util::url_encode(q));
        let Ok(r) = http.send(&Request::get(&url)) else { continue };
        if !r.ok() {
            continue;
        }
        let Ok(j) = r.json() else { continue };
        for e in j.get("data").items().iter().take(6) {
            for jp in e.get("japanese").items() {
                let w = jp.get("word").str();
                let reading = jp.get("reading").str();
                // the written form must contain the kanji we looked for
                if !w.is_empty() && !reading.is_empty() && w.contains(run) {
                    let meanings: Vec<String> = e.get("senses").items().iter().take(2).map(|s| s.get("english_definitions").strings().into_iter().take(3).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty()).collect();
                    return Some(Word { word: w, reading, meanings });
                }
            }
        }
    }
    None
}

/// A short description from the Japanese Wikipedia (empty when there is no page).
pub fn describe(http: &dyn Http, word: &str) -> String {
    let url = format!("https://ja.wikipedia.org/api/rest_v1/page/summary/{}", crate::util::url_encode(word));
    let Ok(r) = http.send(&Request::get(&url).header("User-Agent", "Reelhouse/0.3 (clip cards; https://github.com/reelhouse)")) else { return String::new() };
    if !r.ok() {
        return String::new();
    }
    let Ok(j) = r.json() else { return String::new() };
    if j.get("type").str() == "disambiguation" {
        return String::new();
    }
    let t = j.get("extract").str();
    let mut s: String = t.chars().take(280).collect();
    if t.chars().count() > 280 {
        s.push('…');
    }
    s
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The fields of a clip card, for a sentence (the subtitle of the clip).
pub struct ClipText {
    pub hiragana: String,
    pub katakana: String,
    pub kanji: String,
    pub meaning: String,
    pub description: String,
}

pub fn build(http: &dyn Http, sentence: &str) -> ClipText {
    let mut hira: Vec<String> = vec![];
    let mut kata: Vec<String> = vec![];
    let mut meaning: Vec<String> = vec![];
    let mut desc = String::new();
    let mut words: Vec<Word> = vec![];
    for (run, with) in candidates(sentence).into_iter().take(8) {
        if let Some(w) = lookup_word(http, &run, &with) {
            if !words.iter().any(|x| x.word == w.word) {
                words.push(w);
            }
        }
    }
    for w in &words {
        let r = to_hiragana(&w.reading);
        hira.push(format!("{} 【{}】", esc(&w.word), esc(&r)));
        kata.push(format!("{} 【{}】", esc(&w.word), esc(&to_katakana(&r))));
        if !w.meanings.is_empty() {
            meaning.push(format!("<b>{}</b>: {}", esc(&w.word), esc(&w.meanings.join(" · "))));
        }
    }
    for w in words.iter().take(2) {
        let d = describe(http, &w.word);
        if !d.is_empty() {
            if !desc.is_empty() {
                desc.push_str("<br>");
            }
            desc.push_str(&format!("<b>{}</b>: {}", esc(&w.word), esc(&d)));
        }
    }
    if desc.is_empty() {
        if let Some(w) = words.first() {
            desc = esc(&w.meanings.join(" · "));
        }
    }
    let mut kj: Vec<String> = vec![];
    for c in kanji_of(sentence) {
        if let Some(k) = info(c) {
            let mut line = format!("<b>{}</b> — {}", c, esc(&k.meanings.join(", ")));
            if !k.on.is_empty() {
                line.push_str(&format!("; 音 {}", esc(&k.on.iter().map(|x| to_katakana(x)).collect::<Vec<_>>().join("・"))));
            }
            if !k.kun.is_empty() {
                line.push_str(&format!("; 訓 {}", esc(&k.kun.join("・"))));
            }
            if k.jlpt > 0 {
                line.push_str(&format!("; N{}", k.jlpt));
            }
            if k.strokes > 0 {
                line.push_str(&format!("; {}画", k.strokes));
            }
            kj.push(line);
        } else {
            kj.push(format!("<b>{}</b>", c));
        }
    }
    ClipText { hiragana: hira.join("<br>"), katakana: kata.join("<br>"), kanji: kj.join("<br>"), meaning: meaning.join("<br>"), description: desc }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn encode_clip() {
        let libs = vec![std::path::PathBuf::from("libmpv.so.2")];
        let r = crate::mpv::Mpv::encode(&libs, "/tmp/enc/in.mp4", &[], 1.0, 3.0, "/tmp/enc/out.mp4", false, 60);
        println!("video: {:?}", r);
        let p = crate::mpv::Mpv::encode(&libs, "/tmp/enc/in.mp4", &[], 1.3, 1.6, "/tmp/enc/out.jpg", true, 60);
        println!("picture: {:?}", p);
        assert!(r.is_ok() && p.is_ok());
    }
    use super::*;
    #[test]
    fn table_and_runs() {
        assert!(info('語').unwrap().on.contains(&"ご".to_string()));
        assert_eq!(to_katakana("がっこう"), "ガッコウ");
        assert_eq!(candidates("私は学校に行きます"), vec![("私".to_string(), "私は".to_string()), ("学校".to_string(), "学校に".to_string()), ("行".to_string(), "行きます".to_string())]);
        assert_eq!(kanji_of("学校学"), vec!['学', '校']);
    }
}
