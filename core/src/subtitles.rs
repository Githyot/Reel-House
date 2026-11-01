//! Subtitle files: parse SRT/WebVTT, pick the best source, translate in batches, cache the result.

use crate::addons::Subtitle;
use crate::error::{Error, Result};
use crate::translators::Translate;
use futures_util::stream::{self, StreamExt};
use regex::Regex;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: String,
    pub end: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PreparedSub {
    pub file: PathBuf,
    pub lang: String,
    pub translated: bool,
    pub cached: bool,
    /// True when no translator was available and the file is still in its original language.
    pub untranslated: bool,
}

/// ISO 639-2 (what Stremio subtitle addons send) to ISO 639-1 (what translators and locales use).
const ISO3: &[(&str, &str)] = &[
    ("eng", "en"), ("por", "pt"), ("pob", "pt"), ("spa", "es"), ("fre", "fr"), ("fra", "fr"), ("ger", "de"), ("deu", "de"),
    ("ita", "it"), ("rus", "ru"), ("jpn", "ja"), ("kor", "ko"), ("chi", "zh"), ("zho", "zh"), ("ara", "ar"), ("hin", "hi"),
    ("tur", "tr"), ("pol", "pl"), ("dut", "nl"), ("nld", "nl"), ("swe", "sv"), ("nor", "no"), ("dan", "da"), ("fin", "fi"),
    ("cze", "cs"), ("ces", "cs"), ("hun", "hu"), ("rum", "ro"), ("ron", "ro"), ("gre", "el"), ("ell", "el"), ("heb", "he"),
    ("tha", "th"), ("vie", "vi"), ("ind", "id"), ("ukr", "uk"), ("bul", "bg"), ("hrv", "hr"), ("srp", "sr"), ("slv", "sl"), ("cat", "ca"),
];

pub fn lang_base(code: &str) -> String {
    let lower = code.trim().to_lowercase().replace('_', "-");
    let head = lower.split('-').next().unwrap_or("");
    ISO3.iter().find(|(k, _)| *k == head).map(|(_, v)| v.to_string()).unwrap_or_else(|| head.to_string())
}

pub fn decode(bytes: &[u8]) -> String {
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    };
    text.trim_start_matches('\u{feff}').to_string()
}

static TIMES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([\d:.,]+)\s*-->\s*([\d:.,]+)").unwrap());
static ASS_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\\[^}]*\}").unwrap());

pub fn to_srt_time(raw: &str) -> String {
    let normal = raw.trim().replace(',', ".");
    let mut parts: Vec<&str> = normal.split(':').collect();
    while parts.len() < 3 {
        parts.insert(0, "0");
    }
    let (h, m, rest) = (parts[0], parts[1], parts[2]);
    let (s, ms) = rest.split_once('.').unwrap_or((rest, "0"));
    let p2 = |x: &str| format!("{:0>2}", x);
    format!("{}:{}:{},{}", p2(h), p2(m), p2(s), format!("{ms}00").chars().take(3).collect::<String>())
}

/// Reads SRT or WebVTT into cues with SRT-style timestamps.
pub fn parse_cues(input: &str) -> Vec<Cue> {
    let text = input.replace("\r\n", "\n").replace('\r', "\n");
    let text = match text.strip_prefix("WEBVTT") {
        Some(rest) => rest.split_once('\n').map(|(_, r)| r.to_string()).unwrap_or_default(),
        None => text,
    };
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.trim_matches('\n').split('\n').collect();
        if lines.first().map_or(true, |l| l.starts_with("NOTE") || l.starts_with("STYLE") || l.starts_with("REGION")) {
            continue;
        }
        let Some(t) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some(m) = TIMES.captures(lines[t]) else { continue };
        let body = ASS_TAGS.replace_all(&lines[t + 1..].join("\n"), "").trim().to_string();
        if body.is_empty() {
            continue;
        }
        cues.push(Cue { start: to_srt_time(&m[1]), end: to_srt_time(&m[2]), text: body });
    }
    cues
}

pub fn cues_to_srt(cues: &[Cue]) -> String {
    cues.iter().enumerate().map(|(i, c)| format!("{}\n{} --> {}\n{}\n", i + 1, c.start, c.end, c.text)).collect::<Vec<_>>().join("\n")
}

/// Translates cue text in batches, a few at a time. A batch whose answer has the wrong length is split
/// until it fits, and a single line that cannot be translated keeps its original text.
pub async fn translate_cues(
    cues: &[Cue],
    translator: &dyn Translate,
    source: &str,
    target: &str,
    batch_size: usize,
    on_progress: &(dyn Fn(f32) + Send + Sync),
) -> Result<Vec<Cue>> {
    let total = cues.len().max(1);
    let ranges: Vec<(usize, usize)> = (0..cues.len()).step_by(batch_size).map(|i| (i, (i + batch_size).min(cues.len()))).collect();
    let done = std::sync::atomic::AtomicUsize::new(0);

    let results: Vec<Result<Vec<(usize, String)>>> = stream::iter(ranges)
        .map(|(from, to)| {
            let done = &done;
            async move {
                let mut out: Vec<(usize, String)> = Vec::new();
                let mut stack = vec![(from, to)];
                while let Some((a, b)) = stack.pop() {
                    let texts: Vec<String> = cues[a..b].iter().map(|c| c.text.clone()).collect();
                    match translator.translate(&texts, source, target).await {
                        Ok(r) if r.len() == texts.len() => out.extend(r.into_iter().enumerate().map(|(k, t)| (a + k, t))),
                        Ok(_) | Err(_) if b - a > 1 => {
                            let mid = a + (b - a + 1) / 2;
                            stack.push((mid, b));
                            stack.push((a, mid));
                        }
                        Ok(_) => out.push((a, texts[0].clone())),
                        Err(e) => return Err(e),
                    }
                }
                let n = done.fetch_add(to - from, std::sync::atomic::Ordering::SeqCst) + (to - from);
                on_progress(n as f32 / total as f32);
                Ok(out)
            }
        })
        .buffer_unordered(3)
        .collect()
        .await;

    let mut texts: Vec<Option<String>> = vec![None; cues.len()];
    for r in results {
        for (i, t) in r? {
            texts[i] = Some(t);
        }
    }
    Ok(cues.iter().zip(texts).map(|(c, t)| Cue { text: t.unwrap_or_else(|| c.text.clone()), ..c.clone() }).collect())
}

/// Chooses which subtitle to use. The bool says whether it still needs translating.
pub fn pick_candidate<'a>(candidates: &'a [Subtitle], target: &str, preferred: &[String]) -> Option<(&'a Subtitle, bool)> {
    let target = lang_base(target);
    let usable: Vec<&Subtitle> = candidates.iter().filter(|c| !c.url.is_empty()).collect();
    if let Some(c) = usable.iter().find(|c| lang_base(&c.lang) == target) {
        return Some((c, false));
    }
    for pref in preferred {
        if let Some(c) = usable.iter().find(|c| lang_base(&c.lang) == lang_base(pref)) {
            return Some((c, true));
        }
    }
    usable.first().map(|c| (*c, true))
}

async fn download(http: &reqwest::Client, url: &str) -> Result<String> {
    let res = http.get(url).timeout(std::time::Duration::from_secs(30)).send().await?;
    if !res.status().is_success() {
        return Err(Error::msg(format!("Subtitle download failed ({})", res.status().as_u16())));
    }
    Ok(decode(&res.bytes().await?))
}

/// Produces a local .srt in the user's language, or None when no subtitle exists at all.
/// Results are cached by source URL and language, so playing the same title again is instant.
pub async fn prepare_subtitle(
    http: &reqwest::Client,
    candidates: &[Subtitle],
    target: &str,
    preferred: &[String],
    translator: Option<&dyn Translate>,
    cache_dir: &Path,
    on_status: &(dyn Fn(String) + Send + Sync),
) -> Result<Option<PreparedSub>> {
    let Some((cand, needs)) = pick_candidate(candidates, target, preferred) else { return Ok(None) };
    let target = lang_base(target);
    let src_lang = lang_base(&cand.lang);
    let key = hex::encode(Sha1::digest(format!("{}|{}", cand.url, if needs { target.as_str() } else { "orig" }).as_bytes()))[..16].to_string();
    std::fs::create_dir_all(cache_dir)?;
    let file = cache_dir.join(format!("{key}.{}.srt", if needs { target.clone() } else if src_lang.is_empty() { "und".into() } else { src_lang.clone() }));
    if file.exists() {
        return Ok(Some(PreparedSub { file, lang: target, translated: needs, cached: true, untranslated: false }));
    }

    on_status("Downloading subtitles".into());
    let cues = parse_cues(&download(http, &cand.url).await?);
    if cues.is_empty() {
        return Err(Error::msg("The subtitle file had no readable lines."));
    }
    if !needs {
        std::fs::write(&file, cues_to_srt(&cues))?;
        return Ok(Some(PreparedSub { file, lang: target, translated: false, cached: false, untranslated: false }));
    }
    let Some(translator) = translator else {
        // No translator set up: still show the subtitle in its own language.
        let raw = cache_dir.join(format!("{key}.{}.raw.srt", if src_lang.is_empty() { "und" } else { &src_lang }));
        std::fs::write(&raw, cues_to_srt(&cues))?;
        return Ok(Some(PreparedSub { file: raw, lang: src_lang, translated: false, cached: false, untranslated: true }));
    };
    let from = if src_lang.is_empty() { "auto".to_string() } else { src_lang };
    let progress = |p: f32| on_status(format!("Translating subtitles {}%", (p * 100.0).round() as u32));
    let translated = translate_cues(&cues, translator, &from, &target, 40, &progress).await?;
    std::fs::write(&file, cues_to_srt(&translated))?;
    Ok(Some(PreparedSub { file, lang: target, translated: true, cached: false, untranslated: false }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn sub(lang: &str, url: &str) -> Subtitle {
        Subtitle { id: None, url: url.into(), lang: lang.into(), addon_name: String::new() }
    }

    #[test]
    fn parses_srt_and_webvtt_into_srt_timestamps() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\n{\\an8}Bye\r\n";
        let vtt = "WEBVTT\n\nNOTE hi\n\ncue1\n00:01.000 --> 00:02.500 align:start\nHello\n\n00:03.000 --> 00:04.000\n<i>Bye</i>\n";
        let a = parse_cues(srt);
        assert_eq!((a[0].start.as_str(), a[0].end.as_str(), a[0].text.as_str()), ("00:00:01,000", "00:00:02,500", "Hello"));
        assert_eq!(a[1].text, "Bye");
        let b = parse_cues(vtt);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].start, "00:00:01,000");
        assert_eq!(b[1].text, "<i>Bye</i>");
        assert!(cues_to_srt(&a).starts_with("1\n00:00:01,000 --> 00:00:02,500\nHello\n\n2\n"));
    }

    #[test]
    fn language_codes_are_normalised() {
        assert_eq!(lang_base("eng"), "en");
        assert_eq!(lang_base("pt-BR"), "pt");
        assert_eq!(lang_base("pob"), "pt");
        assert_eq!(lang_base("pt_BR"), "pt");
        assert_eq!(lang_base(""), "");
    }

    #[test]
    fn decode_falls_back_to_windows_1252() {
        assert_eq!(decode("olá".as_bytes()), "olá");
        assert_eq!(decode(&[b'o', b'l', 0xE1]), "olá");
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'a']), "a");
    }

    #[test]
    fn picks_target_language_then_preferred_sources() {
        let c = vec![sub("spa", "u1"), sub("eng", "u2"), sub("por", "u3")];
        let (p, needs) = pick_candidate(&c, "pt-BR", &["en".into()]).unwrap();
        assert_eq!((p.url.as_str(), needs), ("u3", false));
        let (p, needs) = pick_candidate(&c[..2], "pt", &["en".into()]).unwrap();
        assert_eq!((p.url.as_str(), needs), ("u2", true));
        assert!(pick_candidate(&[sub("eng", "")], "pt", &["en".into()]).is_none());
    }

    struct Upper(AtomicUsize);
    impl Translate for Upper {
        fn name(&self) -> &str { "upper" }
        fn translate<'a>(&'a self, texts: &'a [String], _s: &'a str, _t: &'a str) -> futures_util::future::BoxFuture<'a, Result<Vec<String>>> {
            Box::pin(async move {
                self.0.fetch_add(1, Ordering::SeqCst);
                if texts.len() > 2 {
                    return Ok(texts.iter().skip(1).map(|t| t.to_uppercase()).collect()); // wrong length on purpose
                }
                Ok(texts.iter().map(|t| t.to_uppercase()).collect())
            })
        }
    }

    #[tokio::test]
    async fn a_batch_with_the_wrong_length_is_split_until_it_fits() {
        let cues: Vec<Cue> = (0..7).map(|i| Cue { start: "s".into(), end: "e".into(), text: format!("line {i}") }).collect();
        let t = Upper(AtomicUsize::new(0));
        let out = translate_cues(&cues, &t, "en", "pt", 7, &|_| {}).await.unwrap();
        assert_eq!(out.iter().map(|c| c.text.clone()).collect::<Vec<_>>(), cues.iter().map(|c| c.text.to_uppercase()).collect::<Vec<_>>());
        assert!(t.0.load(Ordering::SeqCst) > 1);
    }

    struct Prefix;
    impl Translate for Prefix {
        fn name(&self) -> &str { "prefix" }
        fn translate<'a>(&'a self, texts: &'a [String], _s: &'a str, _t: &'a str) -> futures_util::future::BoxFuture<'a, Result<Vec<String>>> {
            Box::pin(async move { Ok(texts.iter().map(|t| format!("[pt] {t}")).collect()) })
        }
    }

    #[tokio::test]
    async fn downloads_translates_writes_an_srt_and_reuses_the_cache() {
        let server = MockServer::start();
        let m = server.mock(|w, t| { w.method(GET).path("/en.srt"); t.status(200).body("1\n00:00:01,000 --> 00:00:02,000\nHello there\n\n"); });
        let dir = tempfile::tempdir().unwrap();
        let http = reqwest::Client::new();
        let cands = vec![sub("eng", &server.url("/en.srt"))];
        let r1 = prepare_subtitle(&http, &cands, "pt-BR", &["en".into()], Some(&Prefix), dir.path(), &|_| {}).await.unwrap().unwrap();
        assert!(r1.translated && !r1.cached);
        assert!(std::fs::read_to_string(&r1.file).unwrap().contains("[pt] Hello there"));
        let r2 = prepare_subtitle(&http, &cands, "pt-BR", &["en".into()], Some(&Prefix), dir.path(), &|_| {}).await.unwrap().unwrap();
        assert!(r2.cached);
        assert_eq!(m.hits(), 1);
    }

    #[tokio::test]
    async fn skips_translation_when_the_target_language_exists_and_handles_no_translator() {
        let server = MockServer::start();
        server.mock(|w, t| { w.method(GET).path("/pt.srt"); t.status(200).body("1\n00:00:01,000 --> 00:00:02,000\nOlá\n\n"); });
        server.mock(|w, t| { w.method(GET).path("/en.srt"); t.status(200).body("1\n00:00:01,000 --> 00:00:02,000\nHi\n\n"); });
        let dir = tempfile::tempdir().unwrap();
        let http = reqwest::Client::new();
        let both = vec![sub("por", &server.url("/pt.srt")), sub("eng", &server.url("/en.srt"))];
        let r = prepare_subtitle(&http, &both, "pt", &["en".into()], Some(&Prefix), dir.path(), &|_| {}).await.unwrap().unwrap();
        assert!(!r.translated);
        assert!(std::fs::read_to_string(&r.file).unwrap().contains("Olá"));
        let r = prepare_subtitle(&http, &both[1..], "pt", &["en".into()], None, dir.path(), &|_| {}).await.unwrap().unwrap();
        assert!(r.untranslated);
        assert!(prepare_subtitle(&http, &[], "pt", &[], None, dir.path(), &|_| {}).await.unwrap().is_none());
    }
}
