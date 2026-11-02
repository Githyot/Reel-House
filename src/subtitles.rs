//! Finding, downloading, translating and caching subtitles.
use crate::addons::SubCand;
use crate::http::{Http, Request};
use crate::translate::{translate_all, Translator};
use crate::util::{fnv_hex, lang_base};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: String,
    pub end: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct SubFile {
    pub path: PathBuf,
    pub lang: String,
    pub translated: bool,
    /// True when no translator was available, so the file stays in its own language.
    pub untranslated: bool,
    pub cached: bool,
}

const CP1252: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—',
    '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// UTF-8 when valid, otherwise Windows-1252 (the usual encoding of older subtitle files).
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes
            .iter()
            .map(|&b| if (0x80..0xA0).contains(&b) { CP1252[(b - 0x80) as usize] } else { b as char })
            .collect(),
    }
}

fn srt_time(raw: &str) -> String {
    let t = raw.trim().replace(',', ".");
    let mut parts: Vec<&str> = t.split(':').collect();
    while parts.len() < 3 {
        parts.insert(0, "0");
    }
    let (h, m, rest) = (parts[0], parts[1], parts[2]);
    let (s, ms) = rest.split_once('.').unwrap_or((rest, "0"));
    // three digits, digits only: a stray letter or accent in a downloaded stamp must not cut a character in half
    let ms: String = ms.chars().filter(|c| c.is_ascii_digit()).chain(std::iter::repeat('0')).take(3).collect();
    format!("{:02}:{:02}:{:02},{}", h.parse::<u32>().unwrap_or(0), m.parse::<u32>().unwrap_or(0), s.parse::<u32>().unwrap_or(0), ms)
}

fn strip_style_tags(line: &str) -> String {
    // Remove ASS override blocks such as {\an8}; keep <i> and other HTML tags.
    let mut out = String::new();
    let mut depth = 0;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' && chars.get(i + 1) == Some(&'\\') {
            depth += 1;
        } else if chars[i] == '}' && depth > 0 {
            depth -= 1;
        } else if depth == 0 {
            out.push(chars[i]);
        }
        i += 1;
    }
    out
}

/// Reads SRT or WebVTT into cues with SRT-style timestamps.
pub fn parse_cues(input: &str) -> Vec<Cue> {
    let text = input.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.strip_prefix("WEBVTT").map(|r| r.split_once('\n').map(|x| x.1).unwrap_or("")).unwrap_or(&text);
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.trim_matches('\n').split('\n').collect();
        if lines.first().map(|l| l.starts_with("NOTE") || l.starts_with("STYLE") || l.starts_with("REGION")).unwrap_or(true) {
            continue;
        }
        let Some(t) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some((a, b)) = lines[t].split_once("-->") else { continue };
        let end = b.trim().split_whitespace().next().unwrap_or("");
        let body = strip_style_tags(&lines[t + 1..].join("\n")).trim().to_string();
        if body.is_empty() || a.trim().is_empty() || end.is_empty() {
            continue;
        }
        cues.push(Cue { start: srt_time(a), end: srt_time(end), text: body });
    }
    cues
}

/// Removes the notes for the hearing impaired from one cue: `[music]`, `(laughs)` and speaker labels such as
/// `JOHN:` or `- [MAN]:`. None when nothing is left. Ported from NuvioMobile's SubtitleSdhFilter.kt
/// (GPL-3.0, NuvioMedia and contributors), modified 2026-11: the regex crate has no look-around, so the checks
/// the Kotlin patterns made with it are done in code, and a speaker label must be written in capitals.
pub fn strip_sdh(text: &str) -> Option<String> {
    use std::sync::OnceLock;
    static RE: OnceLock<(regex::Regex, regex::Regex, regex::Regex)> = OnceLock::new();
    let (speaker, brackets, parens) = RE.get_or_init(|| {
        (
            regex::Regex::new(r"(?m)^([ \t]*-[ \t]*)?(?:[A-Z0-9 ()'#.,]*[A-Z][A-Z0-9 ()'#.,]*|\[[^\]\r\n]*\]):([ \t]+|$)").unwrap(),
            regex::Regex::new(r"\[[^\]]*\][ \t]*").unwrap(),
            regex::Regex::new(r"\(([^)]*)\)[ \t]*|（([^）]*)）[ \t]*").unwrap(),
        )
    });
    let t = speaker.replace_all(text, |c: &regex::Captures| c.get(1).map(|m| m.as_str()).unwrap_or("").to_string());
    let t = brackets.replace_all(&t, "");
    // only notes written in plain letters, and not a bare number such as "(2)"
    let t = parens.replace_all(&t, |c: &regex::Captures| {
        let inner = c.get(1).or_else(|| c.get(2)).map(|m| m.as_str()).unwrap_or("");
        let note = inner.chars().all(|ch| ch.is_ascii_alphanumeric() || " '#.,\"\\-\r\n".contains(ch));
        let number = inner.chars().all(|ch| ch.is_ascii_digit());
        if note && !number { String::new() } else { c[0].to_string() }
    });
    let lines: Vec<&str> = t.lines().filter(|l| l.chars().any(|ch| !ch.is_whitespace() && ch != '-')).map(|l| l.trim_end()).collect();
    let out = lines.join("\n");
    if out.trim().is_empty() { None } else { Some(out) }
}

pub fn cues_to_srt(cues: &[Cue]) -> String {
    cues.iter().enumerate().map(|(i, c)| format!("{}\n{} --> {}\n{}\n", i + 1, c.start, c.end, c.text)).collect::<Vec<_>>().join("\n")
}

/// Exact language match first, then the preferred source languages in order, then anything.
pub fn pick_candidate<'a>(cands: &'a [SubCand], target: &str, preferred: &[String]) -> Option<(&'a SubCand, bool)> {
    let target = lang_base(target);
    let usable: Vec<&SubCand> = cands.iter().filter(|c| !c.url.is_empty()).collect();
    if let Some(c) = usable.iter().find(|c| lang_base(&c.lang) == target) {
        return Some((c, false));
    }
    for p in preferred {
        let p = lang_base(p);
        if let Some(c) = usable.iter().find(|c| lang_base(&c.lang) == p) {
            return Some((c, true));
        }
    }
    usable.first().map(|c| (*c, true))
}

/// Every usable subtitle in the order they should be tried: the target language first, then the preferred
/// source languages, then the rest.
pub fn ordered_candidates<'a>(cands: &'a [SubCand], target: &str, preferred: &[String]) -> Vec<(&'a SubCand, bool)> {
    let target = lang_base(target);
    let usable: Vec<&SubCand> = cands.iter().filter(|c| !c.url.is_empty()).collect();
    let mut out: Vec<(&SubCand, bool)> = Vec::new();
    let push = |c: &'a SubCand, needs: bool, out: &mut Vec<(&'a SubCand, bool)>| {
        if !out.iter().any(|(o, _)| o.url == c.url) {
            out.push((c, needs));
        }
    };
    for c in usable.iter().filter(|c| lang_base(&c.lang) == target) {
        push(c, false, &mut out);
    }
    for p in preferred {
        let p = lang_base(p);
        for c in usable.iter().filter(|c| lang_base(&c.lang) == p) {
            push(c, true, &mut out);
        }
    }
    for c in usable.iter() {
        push(c, true, &mut out);
    }
    out
}

pub struct Wanted<'a> {
    pub candidates: &'a [SubCand],
    pub target: &'a str,
    pub preferred: &'a [String],
    /// Remove the notes for the hearing impaired (see `strip_sdh`).
    pub strip_sdh: bool,
}

/// Produces a local `.srt` in the target language, or None when no subtitle exists.
/// Results are cached by source URL and language, so playing the same title again is instant.
pub fn prepare(
    http: &dyn Http,
    wanted: &Wanted,
    translator: Option<&dyn Translator>,
    cache_dir: &Path,
    status: &(dyn Fn(String) + Sync),
) -> Result<Option<SubFile>, String> {
    let ordered = ordered_candidates(wanted.candidates, wanted.target, wanted.preferred);
    if ordered.is_empty() {
        return Ok(None);
    }
    // One broken subtitle (a dead link, an empty file) must not end the search: try the next ones.
    let mut last = String::new();
    for (cand, needs) in ordered.into_iter().take(6) {
        match prepare_one(http, cand, needs, wanted, translator, cache_dir, status) {
            Ok(r) => return Ok(r),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn prepare_one(
    http: &dyn Http,
    cand: &SubCand,
    needs: bool,
    wanted: &Wanted,
    translator: Option<&dyn Translator>,
    cache_dir: &Path,
    status: &(dyn Fn(String) + Sync),
) -> Result<Option<SubFile>, String> {
    let target = lang_base(wanted.target);
    let src_lang = lang_base(&cand.lang);
    fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
    let key = fnv_hex(&format!("{}|{}{}", cand.url, if needs { target.as_str() } else { "orig" }, if wanted.strip_sdh { "|nosdh" } else { "" }));

    let (file_name, lang) = if needs && translator.is_some() {
        (format!("{}.{}.srt", key, target), target.clone())
    } else if needs {
        (format!("{}.{}.raw.srt", key, if src_lang.is_empty() { "und" } else { &src_lang }), src_lang.clone())
    } else {
        (format!("{}.{}.srt", key, target), target.clone())
    };
    let path = cache_dir.join(&file_name);
    let translated = needs && translator.is_some();
    let untranslated = needs && translator.is_none();
    if path.exists() {
        return Ok(Some(SubFile { path, lang, translated, untranslated, cached: true }));
    }

    status("Downloading subtitles".to_string());
    let r = http.send(&Request::get(&cand.url).timeout(40))?;
    if !r.ok() {
        return Err(format!("The subtitle download failed ({}).", r.status));
    }
    let mut cues = parse_cues(&decode(&r.body));
    if wanted.strip_sdh {
        // before translating: fewer lines to send, and the notes are not translated for nothing
        cues = cues.into_iter().filter_map(|c| strip_sdh(&c.text).map(|text| Cue { text, ..c })).collect();
    }
    if cues.is_empty() {
        return Err("The subtitle file had no readable lines.".to_string());
    }
    let out_cues = match (needs, translator) {
        (true, Some(tr)) => {
            let texts: Vec<String> = cues.iter().map(|c| c.text.clone()).collect();
            let progress = |p: f32| status(format!("Translating subtitles {}%", (p * 100.0).round() as u32));
            let tx = translate_all(tr, &texts, if src_lang.is_empty() { "auto" } else { &src_lang }, &target, 40, 3, &progress)?;
            cues.into_iter().zip(tx).map(|(c, t)| Cue { text: t, ..c }).collect()
        }
        _ => cues,
    };
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, cues_to_srt(&out_cues)).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(Some(SubFile { path, lang, translated, untranslated, cached: false }))
}

/// Downloads one subtitle as it is (no translation) and keeps it as a local .srt. Cached by URL.
pub fn fetch_srt(http: &dyn Http, url: &str, cache_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
    let path = cache_dir.join(format!("{}.variant.srt", fnv_hex(url)));
    if path.exists() {
        return Ok(path);
    }
    let r = http.send(&Request::get(url).timeout(40))?;
    if !r.ok() {
        return Err(format!("The subtitle download failed ({}).", r.status));
    }
    let cues = parse_cues(&decode(&r.body));
    if cues.is_empty() {
        return Err("The subtitle file had no readable lines.".to_string());
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, cues_to_srt(&cues)).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn strips_hearing_impaired_notes() {
        assert_eq!(strip_sdh("[MUSIC PLAYING]").as_deref(), None);
        assert_eq!(strip_sdh("(laughs) That's funny.").as_deref(), Some("That's funny."));
        assert_eq!(strip_sdh("JOHN: Where are you?").as_deref(), Some("Where are you?"));
        assert_eq!(strip_sdh("- [MAN]: Run!\n- (gasps) Okay.").as_deref(), Some("- Run!\n- Okay."));
        // ordinary text stays
        assert_eq!(strip_sdh("Meet me at 10:30, okay?").as_deref(), Some("Meet me at 10:30, okay?"));
        assert_eq!(strip_sdh("Note: this is fine").as_deref(), Some("Note: this is fine"));
        assert_eq!(strip_sdh("Chapter (2) begins").as_deref(), Some("Chapter (2) begins"));
        assert_eq!(strip_sdh("お元気ですか（笑）").as_deref(), Some("お元気ですか（笑）"));
        assert_eq!(strip_sdh("（laughs）こんにちは").as_deref(), Some("こんにちは"));
    }

    #[test]
    fn srt_time_survives_odd_stamps() {
        assert_eq!(srt_time("00:01:02,5"), "00:01:02,500");
        assert_eq!(srt_time("1:02.345"), "00:01:02,345");
        assert_eq!(srt_time("00:00:01,12345"), "00:00:01,123");
        // non-ASCII in the fraction used to panic on a byte slice
        assert_eq!(srt_time("00:00:01,ñé"), "00:00:01,000");
        assert_eq!(srt_time("00:00:01,1ñ"), "00:00:01,100");
    }

    fn cand(lang: &str, url: &str) -> SubCand {
        SubCand { lang: lang.into(), url: url.into(), addon: "t".into() }
    }

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("rh-sub-{}-{}", std::process::id(), fnv_hex(&format!("{:?}", std::time::Instant::now()))));
        let _ = fs::remove_dir_all(&d);
        d
    }

    struct Prefix;
    impl Translator for Prefix {
        fn name(&self) -> String {
            "p".into()
        }
        fn translate(&self, t: &[String], _: &str, target: &str) -> Result<Vec<String>, String> {
            Ok(t.iter().map(|x| format!("[{}] {}", target, x)).collect())
        }
    }

    #[test]
    fn parses_srt_and_vtt() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\n{\\an8}Bye\r\n";
        let vtt = "WEBVTT\n\nNOTE hi\n\ncue1\n00:01.000 --> 00:02.500 align:start\nHello\n\n00:03.000 --> 00:04.000\n<i>Bye</i>\n";
        let a = parse_cues(srt);
        let b = parse_cues(vtt);
        assert_eq!(a.iter().map(|c| (c.start.as_str(), c.end.as_str(), c.text.as_str())).collect::<Vec<_>>(), vec![("00:00:01,000", "00:00:02,500", "Hello"), ("00:00:03,000", "00:00:04,000", "Bye")]);
        assert_eq!(b[0].start, "00:00:01,000");
        assert_eq!(b[1].text, "<i>Bye</i>");
        assert!(cues_to_srt(&a).starts_with("1\n00:00:01,000 --> 00:00:02,500\nHello\n\n2\n"));
    }

    #[test]
    fn decodes_utf8_and_windows_1252() {
        assert_eq!(decode("\u{FEFF}olá".as_bytes()), "olá");
        assert_eq!(decode(&[b'a', 0xE9, 0x93, b'z']), "aé“z");
    }

    #[test]
    fn picks_target_then_preferred_then_any() {
        let c = vec![cand("spa", "u1"), cand("eng", "u2"), cand("por", "u3")];
        let pref = vec!["en".to_string()];
        assert_eq!(pick_candidate(&c, "pt-BR", &pref).map(|(x, n)| (x.url.as_str(), n)), Some(("u3", false)));
        assert_eq!(pick_candidate(&c[..2], "pt", &pref).map(|(x, n)| (x.url.as_str(), n)), Some(("u2", true)));
        assert_eq!(pick_candidate(&c[..1], "pt", &pref).map(|(x, n)| (x.url.as_str(), n)), Some(("u1", true)));
        assert!(pick_candidate(&[], "pt", &pref).is_none());
    }

    #[test]
    fn translates_writes_and_caches() {
        let dir = tmp();
        let http = Fake::new().route("GET http://x/sub.srt", 200, "1\n00:00:01,000 --> 00:00:02,000\nHello there\n\n");
        let c = vec![cand("eng", "http://x/sub.srt")];
        let pref = vec!["en".to_string()];
        let w = Wanted { candidates: &c, target: "pt-BR", preferred: &pref, strip_sdh: false };
        let first = prepare(&http, &w, Some(&Prefix), &dir, &|_| {}).unwrap().unwrap();
        assert!(first.translated && !first.cached);
        assert!(fs::read_to_string(&first.path).unwrap().contains("[pt] Hello there"));
        let second = prepare(&http, &w, Some(&Prefix), &dir, &|_| {}).unwrap().unwrap();
        assert!(second.cached);
        assert_eq!(http.requests().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_translation_when_the_language_exists() {
        let dir = tmp();
        let http = Fake::new().route("GET http://x/p.srt", 200, "1\n00:00:01,000 --> 00:00:02,000\nOlá\n\n");
        let c = vec![cand("por", "http://x/p.srt"), cand("eng", "http://x/e.srt")];
        let pref = vec!["en".to_string()];
        let w = Wanted { candidates: &c, target: "pt", preferred: &pref, strip_sdh: false };
        let r = prepare(&http, &w, Some(&Prefix), &dir, &|_| {}).unwrap().unwrap();
        assert!(!r.translated);
        assert!(fs::read_to_string(&r.path).unwrap().contains("Olá"));
        assert!(!fs::read_to_string(&r.path).unwrap().contains("[pt]"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_the_original_language_without_a_translator() {
        let dir = tmp();
        let http = Fake::new().route("GET http://x/e.srt", 200, "1\n00:00:01,000 --> 00:00:02,000\nHi\n\n");
        let c = vec![cand("eng", "http://x/e.srt")];
        let pref = vec!["en".to_string()];
        let w = Wanted { candidates: &c, target: "pt", preferred: &pref, strip_sdh: false };
        let r = prepare(&http, &w, None, &dir, &|_| {}).unwrap().unwrap();
        assert!(r.untranslated && r.lang == "en");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reports_bad_downloads() {
        let dir = tmp();
        let http = Fake::new().route("GET http://x/e.srt", 404, "");
        let c = vec![cand("eng", "http://x/e.srt")];
        let pref = vec!["en".to_string()];
        let w = Wanted { candidates: &c, target: "pt", preferred: &pref, strip_sdh: false };
        assert!(prepare(&http, &w, Some(&Prefix), &dir, &|_| {}).unwrap_err().contains("404"));
        assert!(prepare(&http, &Wanted { candidates: &[], target: "pt", preferred: &pref, strip_sdh: false }, None, &dir, &|_| {}).unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
