//! Translation back ends and the batching that keeps subtitle lines aligned.
use crate::http::{Http, Request, Response};
use crate::json::Json;
use crate::settings::Subtitles;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

// Microsoft Translator (Azure), the free services one by one, and which translator serves each place of the app.
#[path = "translators/azure.rs"]
mod azure;
#[path = "translators/free.rs"]
mod free;
// the local AI (Ollama): desktop only; on Android and iOS "ollama" falls back to a cloud translator
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[path = "translators/local_ai.rs"]
mod local_ai;
#[path = "translators/routing.rs"]
mod routing;
pub use azure::{azure_batches, azure_code, Azure, AZURE_ENDPOINT, AZURE_MAX_CHARS, AZURE_MAX_ITEMS};
pub use free::{Edge, GoogleWeb, Lingva, MyMemory, Yandex};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use local_ai::{LocalAi, LOCAL_MAX_CHARS, LOCAL_MAX_ITEMS};
/// The batch size of the local AI; kept on Android and iOS only so the shared callers compile (the gate is never set).
#[cfg(any(target_os = "android", target_os = "ios"))]
pub const LOCAL_MAX_ITEMS: usize = 10;

/// The local AI as a translator with `model` ("" = the usual one); on Android and iOS, the cloud translator that
/// stands in for it (`Subtitles::cloud_fallback`), or None when none is set up.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn local_ai<'a>(s: &Subtitles, http: &'a dyn Http, model: &str) -> Option<Box<dyn Translator + 'a>> {
    let _ = s;
    Some(Box::new(LocalAi::new(http, model)))
}
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn local_ai<'a>(s: &Subtitles, http: &'a dyn Http, _model: &str) -> Option<Box<dyn Translator + 'a>> {
    let mut c = s.clone();
    c.enabled = true;
    c.provider = s.cloud_fallback().to_string();
    from_settings(&c, http)
}
pub use routing::{auto_name, build, cache_key, configured, for_use, local_model, name_of, timed_sample, uses_local_ai, Info, CATALOGUE, SAMPLE, USES};

pub trait Translator: Send + Sync {
    fn name(&self) -> String;
    /// Returns one translation per input text, in order.
    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String>;
    /// The same with a hint of what the texts are ("manga" or "novel"); translators that talk to a language model
    /// use it to write a better prompt, the others ignore it.
    fn translate_hinted(&self, texts: &[String], source: &str, target: &str, _hint: &str) -> Result<Vec<String>, String> {
        self.translate(texts, source, target)
    }
}

/// Public LibreTranslate servers that are tried when no server is set. Their availability changes, so the
/// list is only a first choice; the web translation below always answers.
pub const PUBLIC_LIBRE: &[&str] = &["https://translate.terraprint.co", "https://lt.vern.cc", "https://translate.fedilab.app"];

pub struct LibreTranslate<'a> {
    pub http: &'a dyn Http,
    pub url: String,
    pub key: String,
}

impl Translator for LibreTranslate<'_> {
    fn name(&self) -> String {
        "LibreTranslate".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        self.translate_within(texts, source, target, 60)
    }
}

impl LibreTranslate<'_> {
    fn translate_within(&self, texts: &[String], source: &str, target: &str, timeout: u64) -> Result<Vec<String>, String> {
        let mut body = Json::obj(vec![
            ("q", texts.to_vec().into()),
            ("source", if source.is_empty() { "auto" } else { source }.into()),
            ("target", target.into()),
            ("format", "html".into()),
        ]);
        if !self.key.is_empty() {
            body.set("api_key", self.key.clone().into());
        }
        let url = format!("{}/translate", self.url.trim_end_matches('/'));
        let r = self.http.send(&Request::post(&url).json_body(body.dump()).timeout(timeout))?;
        if !r.ok() {
            let t: String = r.text().chars().take(140).collect();
            return Err(format!("LibreTranslate answered {}: {}", r.status, t));
        }
        let j = r.json()?;
        match j.get("translatedText") {
            Json::Arr(v) => Ok(v.iter().map(|x| x.str()).collect()),
            Json::Str(s) if texts.len() == 1 => Ok(vec![s.clone()]),
            _ => Err("LibreTranslate returned an unexpected answer.".to_string()),
        }
    }
}

/// Translation that needs no setup: the server you typed (if any), then public LibreTranslate servers,
/// then the free web translation. The first one that works is remembered for the rest of the run.
pub struct Automatic<'a> {
    pub http: &'a dyn Http,
    pub custom: String,
    pub key: String,
    good: Mutex<Option<String>>,
    dead: Mutex<Vec<String>>,
    /// The free web translation is the last resort. The readers (Images, Text) keep it but leave Google out:
    /// Google answers 429 as soon as a whole chapter is asked, and the others (Lingva, MyMemory) are enough.
    google: bool,
    /// False: only LibreTranslate servers, no Edge or web translation after them (the "LibreTranslate" choice of
    /// Preferences > Translators).
    web: bool,
}

/// What the readers have learned about the public servers: a chapter is asked in many requests, and each one must not
/// wait again for servers that are down.
/// Microsoft Edge's built-in translator: free, no key, answers many texts in one request. The token it asks for is
/// handed out by Edge's own address and lasts several minutes.
static EDGE_TOKEN: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);

fn edge_code(c: &str) -> String {
    match c {
        "zh" | "chs" | "zh-CN" => "zh-Hans".to_string(),
        "zh-TW" | "cht" => "zh-Hant".to_string(),
        "jpn" => "ja".to_string(),
        "kor" => "ko".to_string(),
        "eng" => "en".to_string(),
        other => other.to_string(),
    }
}

static EDGE_DEAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Edge could not be reached (blocked network, TLS refused…): not asked again for ten minutes, so a chapter does not
/// wait 12 s per request for it, thousands of times.
static EDGE_PAUSE: Mutex<Option<std::time::Instant>> = Mutex::new(None);

fn edge_token(http: &dyn Http, fresh: bool) -> Result<String, String> {
    if paused(&EDGE_PAUSE) {
        return Err("The Edge translator could not be reached a moment ago.".to_string());
    }
    if !fresh {
        if let Some((t, at)) = EDGE_TOKEN.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(480) {
                return Ok(t.clone());
            }
        }
    }
    let r = match http.send(&Request::get("https://edge.microsoft.com/translate/auth").timeout(15)) {
        Ok(r) => r,
        Err(e) => {
            pause_for(&EDGE_PAUSE, 600);
            return Err(e);
        }
    };
    let t = r.text().trim().to_string();
    if r.status == 403 || r.status == 404 {
        // Edge no longer gives the token here. It is not asked again in this run.
        EDGE_DEAD.store(true, Ordering::Relaxed);
        return Err(format!("The Edge translator refused (answer {})", r.status));
    }
    if !r.ok() || t.len() < 20 {
        // too many requests, the service down for a moment (429, 503…): asked again in two minutes
        pause_for(&EDGE_PAUSE, 120);
        return Err(format!("The Edge translator is busy (answer {})", r.status));
    }
    *EDGE_TOKEN.lock().unwrap_or_else(|p| p.into_inner()) = Some((t.clone(), std::time::Instant::now()));
    Ok(t)
}

/// The texts translated by Edge's translator, 40 per request.
fn edge_translate(http: &dyn Http, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
    if EDGE_DEAD.load(Ordering::Relaxed) || paused(&EDGE_PAUSE) {
        return Err("The Edge translator is not available.".to_string());
    }
    let mut out: Vec<String> = Vec::with_capacity(texts.len());
    let from = if source.is_empty() || source == "auto" { String::new() } else { format!("&from={}", edge_code(source)) };
    let url = format!("https://api-edge.cognitive.microsofttranslator.com/translate?api-version=3.0&to={}{}", edge_code(target), from);
    for chunk in texts.chunks(40) {
        let body = Json::Arr(chunk.iter().map(|t| Json::obj(vec![("Text", t.as_str().into())])).collect()).dump();
        let mut fresh = false;
        let answer = loop {
            let token = edge_token(http, fresh)?;
            let r = match http.send(&Request::post(&url).header("Authorization", &format!("Bearer {}", token)).json_body(body.clone()).timeout(30)) {
                Ok(r) => r,
                Err(e) => {
                    // the service could not be reached: not asked again for ten minutes (as for the token)
                    pause_for(&EDGE_PAUSE, 600);
                    return Err(e);
                }
            };
            if (r.status == 401 || r.status == 403) && !fresh {
                fresh = true;
                continue;
            }
            if !r.ok() {
                let t: String = r.text().chars().take(120).collect();
                return Err(format!("The Edge translator answered {}: {}", r.status, t));
            }
            break r.json()?;
        };
        let items = answer.items();
        if items.len() != chunk.len() {
            return Err("The Edge translator returned an unexpected answer.".to_string());
        }
        for (it, orig) in items.iter().zip(chunk) {
            let t = it.get("translations").at(0).get("text").str();
            out.push(if t.is_empty() { orig.clone() } else { t });
        }
    }
    Ok(out)
}

static READER_DEAD: Mutex<Vec<String>> = Mutex::new(Vec::new());
static READER_GOOD: Mutex<Option<String>> = Mutex::new(None);

impl<'a> Automatic<'a> {
    fn is_dead(&self, url: &str) -> bool {
        if self.google { self.dead.lock().unwrap().iter().any(|d| d == url) } else { READER_DEAD.lock().unwrap_or_else(|p| p.into_inner()).iter().any(|d| d == url) }
    }

    fn mark_dead(&self, url: String) {
        if self.google { self.dead.lock().unwrap().push(url) } else { READER_DEAD.lock().unwrap_or_else(|p| p.into_inner()).push(url) }
    }

    fn remembered(&self) -> Option<String> {
        if self.google { self.good.lock().unwrap().clone() } else { READER_GOOD.lock().unwrap_or_else(|p| p.into_inner()).clone() }
    }

    fn remember(&self, url: String) {
        if self.google { *self.good.lock().unwrap() = Some(url) } else { *READER_GOOD.lock().unwrap_or_else(|p| p.into_inner()) = Some(url) }
    }

    pub fn new(http: &'a dyn Http, custom: &str, key: &str) -> Automatic<'a> {
        Automatic { http, custom: custom.trim().to_string(), key: key.to_string(), good: Mutex::new(None), dead: Mutex::new(vec![]), google: true, web: true }
    }

    /// LibreTranslate first, then the free web translation without Google: what the page readers use.
    pub fn without_google(mut self) -> Automatic<'a> {
        self.google = false;
        self
    }

    /// Only the LibreTranslate servers (yours, then the public ones), with nothing after them.
    pub fn libre_only(mut self) -> Automatic<'a> {
        self.web = false;
        self
    }

    fn web(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        let out: Mutex<Vec<Option<String>>> = Mutex::new(vec![None; texts.len()]);
        // Yandex takes several texts per request: the way to translate a chapter without being limited.
        let mut at = 0;
        while at < texts.len() && !paused(&YANDEX_PAUSE) {
            let mut end = at;
            let mut chars = 0;
            while end < texts.len() && end - at < 12 && chars + texts[end].len() < 4500 {
                chars += texts[end].len();
                end += 1;
            }
            if end == at {
                end = at + 1;
            }
            let chunk: Vec<String> = texts[at..end].iter().map(|t| if t.trim().is_empty() { " ".to_string() } else { t.clone() }).collect();
            match yandex_batch(self.http, &chunk, source, target) {
                Ok(v) => {
                    let mut o = out.lock().unwrap();
                    for (k, t) in v.into_iter().enumerate() {
                        o[at + k] = Some(if texts[at + k].trim().is_empty() { texts[at + k].clone() } else { t });
                    }
                }
                Err(e) => {
                    crate::debug::log("translate", &e);
                    break;
                }
            }
            at = end;
        }
        if out.lock().unwrap().iter().all(|x| x.is_some()) {
            return Ok(out.into_inner().unwrap().into_iter().map(|x| x.unwrap_or_default()).collect());
        }
        let first_err: Mutex<Option<String>> = Mutex::new(None);
        let next = AtomicUsize::new(0);
        // With no network every text would wait for every service in turn: after a few failures in a row with nothing
        // translated the rest is not tried.
        let misses = AtomicUsize::new(0);
        let any_ok = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|sc| {
            for _ in 0..(if self.google { 2 } else { 2 }).min(texts.len().max(1)) {
                sc.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= texts.len() || (!any_ok.load(Ordering::SeqCst) && misses.load(Ordering::SeqCst) >= 3) {
                        return;
                    }
                    if out.lock().unwrap()[i].is_some() {
                        continue;
                    }
                    match web_translate(self.http, &texts[i], source, target, self.google) {
                        Ok(t) => {
                            any_ok.store(true, Ordering::SeqCst);
                            out.lock().unwrap()[i] = Some(t)
                        }
                        Err(e) => {
                            misses.fetch_add(1, Ordering::SeqCst);
                            *first_err.lock().unwrap() = Some(e)
                        }
                    }
                });
            }
        });
        let got = out.into_inner().unwrap();
        // The readers want to know when most of a chapter was not translated, instead of showing it as it was.
        let translated = got.iter().filter(|x| x.is_some()).count();
        let failed_too_much = if self.google { translated == 0 } else { translated * 2 < got.len() };
        if failed_too_much && !texts.is_empty() {
            return Err(first_err.into_inner().unwrap().unwrap_or_else(|| "The web translation did not answer.".to_string()));
        }
        // A line that failed keeps its original text, so the subtitles stay in step.
        Ok(got.into_iter().zip(texts).map(|(t, orig)| t.unwrap_or_else(|| orig.clone())).collect())
    }
}

/// How many times in a row each free service failed in this run; one that failed several times is left alone, so a
/// chapter does not wait for a service that is down at every paragraph.
static FAILS_YANDEX: AtomicUsize = AtomicUsize::new(0); // kept for the tests of old; the pause above decides now
static FAILS_LINGVA: AtomicUsize = AtomicUsize::new(0);
const GIVE_UP: usize = 4;

fn usable(c: &AtomicUsize) -> bool {
    c.load(Ordering::Relaxed) < GIVE_UP
}

/// Cuts a long text at the ends of sentences (or words) into pieces of at most `max` characters.
pub fn split_pieces(text: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let push_word = |cur: &mut String, out: &mut Vec<String>, w: &str| {
        if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > max {
            out.push(std::mem::take(cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    };
    for w in text.split_whitespace() {
        push_word(&mut cur, &mut out, w);
        // A sentence ends here: start a new piece when the current one is already long.
        if (w.ends_with('.') || w.ends_with('!') || w.ends_with('?') || w.ends_with('”') || w.ends_with('。')) && cur.chars().count() > max * 2 / 3 {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

static YANDEX_LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);
static YANDEX_PAUSE: Mutex<Option<std::time::Instant>> = Mutex::new(None);

fn paused(m: &Mutex<Option<std::time::Instant>>) -> bool {
    m.lock().unwrap_or_else(|p| p.into_inner()).map(|t| std::time::Instant::now() < t).unwrap_or(false)
}

fn pause_for(m: &Mutex<Option<std::time::Instant>>, secs: u64) {
    *m.lock().unwrap() = Some(std::time::Instant::now() + std::time::Duration::from_secs(secs));
}

/// Yandex's translator (the one of its Android app): free, no key. Several texts go in one request (it answers them in
/// the same order), and the requests of the whole app keep a distance of a third of a second: it answers 429 ("limited")
/// to anything faster, and then it is left alone for a while instead of being asked again at every text.
fn yandex_batch(http: &dyn Http, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
    if paused(&YANDEX_PAUSE) {
        return Err("The Yandex translator is resting (it limited the requests).".to_string());
    }
    {
        // One request at a time through this gate: the wait is done holding the lock.
        let mut last = YANDEX_LAST.lock().unwrap();
        if let Some(t) = *last {
            let gap = std::time::Duration::from_millis(330);
            if t.elapsed() < gap {
                std::thread::sleep(gap - t.elapsed());
            }
        }
        *last = Some(std::time::Instant::now());
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
    let id = format!("{:032x}-0-0", nanos ^ ((std::process::id() as u128) << 64));
    let lang = if source.is_empty() || source == "auto" { target.to_string() } else { format!("{}-{}", source, target) };
    let url = format!("https://translate.yandex.net/api/v1/tr.json/translate?id={}&srv=android&lang={}", id, crate::util::url_encode(&lang));
    let mut form: Vec<(&str, &str)> = texts.iter().map(|t| ("text", t.as_str())).collect();
    form.push(("options", "4"));
    let r = http.send(&Request::post(&url).form(&form).timeout(25))?;
    if r.status == 429 {
        pause_for(&YANDEX_PAUSE, 25);
        return Err("The Yandex translator limited the requests (429).".to_string());
    }
    if !r.ok() {
        pause_for(&YANDEX_PAUSE, 8);
        return Err(format!("The Yandex translator answered {}", r.status));
    }
    let j = r.json()?;
    let out: Vec<String> = j.get("text").items().iter().map(|x| x.str()).collect();
    if out.len() != texts.len() || out.iter().all(|t| t.trim().is_empty()) {
        return Err("The Yandex translator returned an unexpected answer.".to_string());
    }
    Ok(out.into_iter().zip(texts).map(|(t, o)| if t.trim().is_empty() { o.clone() } else { t }).collect())
}

fn yandex_translate(http: &dyn Http, text: &str, source: &str, target: &str) -> Result<String, String> {
    yandex_batch(http, &[text.to_string()], source, target).map(|v| v.into_iter().next().unwrap_or_default())
}

/// MyMemory, one piece of at most about 480 characters. It limits how much a day it translates.
static MYMEMORY_PAUSE: Mutex<Option<std::time::Instant>> = Mutex::new(None);

fn mymemory_piece(http: &dyn Http, text: &str, sl: &str, target: &str) -> Result<String, String> {
    if paused(&MYMEMORY_PAUSE) {
        return Err("MyMemory used up its free translations of the day.".to_string());
    }
    let pair = format!("{}|{}", if sl == "auto" { "Autodetect" } else { sl }, target);
    let url = format!("https://api.mymemory.translated.net/get?q={}&langpair={}", crate::util::url_encode(text), crate::util::url_encode(&pair));
    let r = http.send(&Request::get(&url).timeout(20))?;
    if r.status == 429 || r.text().contains("MYMEMORY WARNING") {
        // Its answer is a warning text, never a translation; the day's quota is gone, so it is not asked again for a while.
        pause_for(&MYMEMORY_PAUSE, 3600);
        return Err("MyMemory used up its free translations of the day.".to_string());
    }
    if !r.ok() {
        return Err(format!("The web translation answered {}", r.status));
    }
    let j = r.json()?;
    let t = j.get("responseData").get("translatedText").str();
    if t.is_empty() || j.get("responseStatus").str() == "403" {
        return Err("The web translation returned nothing.".to_string());
    }
    Ok(t)
}

/// Google's free widget endpoints (the browser extension's and the "gtx" one): one text per request.
fn google_text(http: &dyn Http, text: &str, sl: &str, target: &str) -> Result<String, String> {
    let (esl, etl, eq) = (crate::util::url_encode(sl), crate::util::url_encode(target), crate::util::url_encode(text));
    let google = [
        Request::post(&format!("https://translate.googleapis.com/translate_a/single?client=gtx&sl={}&tl={}&dt=t", esl, etl)).form(&[("q", text)]).timeout(20),
        Request::get(&format!("https://clients5.google.com/translate_a/single?client=dict-chrome-ex&sl={}&tl={}&dt=t&q={}", esl, etl, eq)).timeout(20),
    ];
    let mut last = String::from("The web translation did not answer.");
    for req in &google {
        match http.send(req) {
            Ok(r) if r.ok() => {
                if let Ok(j) = r.json() {
                    let parts = j.at(0).items();
                    if !parts.is_empty() {
                        return Ok(parts.iter().map(|p| p.at(0).str()).collect::<String>());
                    }
                }
                last = "The web translation returned an unexpected answer.".to_string();
            }
            Ok(r) => last = format!("The web translation answered {}", r.status),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// The public hosts of Lingva (a free front end of Google Translate).
const LINGVA_HOSTS: &[&str] = &["https://lingva.ml", "https://lingva.thedaviddelta.com", "https://lingva.garudalinux.org", "https://lingva.lunar.icu", "https://translate.plausibility.cloud"];

/// Lingva, one text, each public host in turn. The error is empty when no host answered at all.
fn lingva_text(http: &dyn Http, text: &str, sl: &str, target: &str) -> Result<String, String> {
    let (esl, etl, eq) = (crate::util::url_encode(sl), crate::util::url_encode(target), crate::util::url_encode(text));
    let mut last = String::new();
    for host in LINGVA_HOSTS {
        let url = format!("{}/api/v1/{}/{}/{}", host, esl, etl, eq);
        if let Ok(r) = http.send(&Request::get(&url).timeout(12)) {
            if r.ok() {
                if let Ok(j) = r.json() {
                    let t = j.get("translation").str();
                    if !t.is_empty() {
                        return Ok(t);
                    }
                }
            } else {
                last = format!("The web translation answered {}", r.status);
            }
        }
    }
    Err(last)
}

/// MyMemory, one text: a long paragraph goes in pieces of about 450 characters.
fn mymemory_text(http: &dyn Http, text: &str, sl: &str, target: &str) -> Result<String, String> {
    let pieces = if text.chars().count() <= 480 { vec![text.to_string()] } else { split_pieces(text, 450) };
    let mut done: Vec<String> = Vec::with_capacity(pieces.len());
    for piece in &pieces {
        done.push(mymemory_piece(http, piece, sl, target)?);
    }
    Ok(done.join(" "))
}

/// Free web translation. Several public services in turn, because each one fails or answers 429 when it is asked
/// too much: (Google's widget endpoint, when allowed,) Yandex, Lingva (public hosts), then MyMemory, which takes a
/// long paragraph in pieces. One text per request.
fn web_translate(http: &dyn Http, text: &str, source: &str, target: &str, use_google: bool) -> Result<String, String> {
    if text.trim().is_empty() {
        return Ok(text.to_string());
    }
    if paused(&WEB_PAUSE) {
        return Err("The web translation could not be reached a moment ago (no network?).".to_string());
    }
    // Every request goes through `Answered`, which notes whether any service answered at all (even with an error).
    let watch = Answered { inner: http, any: AtomicBool::new(false) };
    let http: &dyn Http = &watch;
    let sl = if source.is_empty() { "auto" } else { source };
    let mut last = String::from("The web translation did not answer.");
    for attempt in 0..3 {
        if attempt > 0 {
            // No service answered (no network, everything blocked): one more try at most, not three rounds of
            // time-outs for every text of the chapter.
            if attempt >= 2 && !watch.any.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(900 * attempt as u64));
        }
        if use_google {
            match google_text(http, text, sl, target) {
                Ok(t) => return Ok(t),
                Err(e) => last = e,
            }
        }
        // Yandex.
        if !paused(&YANDEX_PAUSE) {
            match yandex_translate(http, text, source, target) {
                Ok(t) => {
                    FAILS_YANDEX.store(0, Ordering::Relaxed);
                    return Ok(t);
                }
                Err(e) => {
                    FAILS_YANDEX.fetch_add(1, Ordering::Relaxed);
                    last = e;
                }
            }
        }
        // Lingva (a free front end of Google Translate; several public hosts).
        if usable(&FAILS_LINGVA) && text.chars().count() <= 1500 {
            match lingva_text(http, text, sl, target) {
                Ok(t) => {
                    FAILS_LINGVA.store(0, Ordering::Relaxed);
                    return Ok(t);
                }
                Err(e) => {
                    if !e.is_empty() {
                        last = e;
                    }
                    FAILS_LINGVA.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        // MyMemory: a long paragraph goes in pieces of about 450 characters.
        match mymemory_text(http, text, sl, target) {
            Ok(t) => return Ok(t),
            // MyMemory resting after its daily quota says little; the error of the services before it says more
            Err(e) => {
                if !paused(&MYMEMORY_PAUSE) || last.is_empty() {
                    last = e
                }
            }
        }
    }
    if !watch.any.load(Ordering::SeqCst) {
        // the texts after this one fail at once for a minute instead of each waiting for the same time-outs
        pause_for(&WEB_PAUSE, 60);
    }
    Err(last)
}

/// The free web translation got no answer from any service: not tried again for a moment (see `web_translate`).
static WEB_PAUSE: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Passes the requests on and notes whether one got an HTTP answer (any status), to tell "no network" from
/// "the services refused".
struct Answered<'a> {
    inner: &'a dyn Http,
    any: AtomicBool,
}

impl Http for Answered<'_> {
    fn send(&self, req: &Request) -> Result<Response, String> {
        let r = self.inner.send(req);
        if r.is_ok() {
            self.any.store(true, Ordering::SeqCst);
        }
        r
    }
}

impl Translator for Automatic<'_> {
    fn name(&self) -> String {
        if !self.web {
            return "LibreTranslate".to_string();
        }
        match self.remembered().as_deref() {
            Some("web") => "LibreTranslate (automatic, web fallback)".to_string(),
            Some(u) => format!("LibreTranslate ({})", u.split('/').nth(2).unwrap_or(u)),
            None => "LibreTranslate (automatic)".to_string(),
        }
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        let mut order: Vec<String> = Vec::new();
        if let Some(g) = self.remembered() {
            order.push(g);
        }
        if !self.custom.is_empty() {
            order.push(self.custom.clone());
        }
        for u in PUBLIC_LIBRE {
            order.push(u.to_string());
        }
        order.dedup();
        for url in order {
            if url == "web" {
                break;
            }
            if self.is_dead(&url) {
                continue;
            }
            let lt = LibreTranslate { http: self.http, url: url.clone(), key: self.key.clone() };
            match lt.translate_within(texts, source, target, 15) {
                Ok(v) if v.len() == texts.len() => {
                    self.remember(url);
                    return Ok(v);
                }
                _ => self.mark_dead(url),
            }
        }
        if !self.web {
            return Err("No LibreTranslate server answered. Type your own server in Preferences > Translators.".to_string());
        }
        // Edge's translator answers whole chapters at once; the per-text web translations are what is left after it.
        match edge_translate(self.http, texts, source, target) {
            Ok(v) => {
                self.remember("web".to_string());
                return Ok(v);
            }
            Err(e) => crate::debug::log("translate", &e),
        }
        let v = self.web(texts, source, target)?;
        self.remember("web".to_string());
        Ok(v)
    }
}

pub struct Claude<'a> {
    pub http: &'a dyn Http,
    pub key: String,
    pub model: String,
}

impl Translator for Claude<'_> {
    fn name(&self) -> String {
        "Claude".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        self.translate_hinted(texts, source, target, "")
    }

    fn translate_hinted(&self, texts: &[String], source: &str, target: &str, hint: &str) -> Result<Vec<String>, String> {
        let system = system_prompt_for(source, target, hint);
        let body = Json::obj(vec![
            ("model", self.model.clone().into()),
            ("max_tokens", 8000i64.into()),
            ("system", system.into()),
            ("messages", Json::Arr(vec![Json::obj(vec![("role", "user".into()), ("content", Json::from(texts.to_vec()).dump().into())])])),
        ]);
        let r = self.http.send(
            &Request::post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.key)
                .header("anthropic-version", "2023-06-01")
                .json_body(body.dump())
                .timeout(120),
        )?;
        if r.status == 401 {
            return Err("The Claude API key was rejected. Check it in Settings > Subtitles.".to_string());
        }
        if !r.ok() {
            let t: String = r.text().chars().take(140).collect();
            return Err(format!("Claude API answered {}: {}", r.status, t));
        }
        let j = r.json()?;
        let text: String = j.get("content").items().iter().map(|b| b.get("text").str()).collect();
        let (a, b) = (text.find('['), text.rfind(']'));
        match (a, b) {
            (Some(a), Some(b)) if b > a => {
                let arr = Json::parse(&text[a..=b]).map_err(|_| "The translation reply was not a JSON array.".to_string())?;
                Ok(arr.items().iter().map(|x| x.str()).collect())
            }
            _ => Err("The translation reply was not a JSON array.".to_string()),
        }
    }
}


/// The instruction given to a language model. `hint` says what the texts are: subtitles by default, or the text of a manga
/// page ("manga"), or paragraphs of a novel ("novel"). A page is translated as a whole so the dialogue reads naturally.
fn system_prompt_for(source: &str, target: &str, hint: &str) -> String {
    let from = if source.is_empty() || source == "auto" { String::new() } else { format!(" from {}", source) };
    let rules = "The user message is a JSON array of strings. Reply with ONLY a JSON array of the same length, one translation per entry, \
                 in the same order. Never merge, split, drop or explain entries.";
    match hint {
        "manga" => format!(
            "You translate the text of the speech bubbles, thoughts and captions of ONE page of a manga or comic{} into the language with ISO code \"{}\". \
             The entries are in reading order and a sentence can continue in the next entry (a bubble split in two): use the whole page as context so the \
             dialogue sounds like natural speech by the same characters. Keep names, honorifics and the tone (rude, polite, childish) consistent. \
             Translate sound effects as short equivalents. Keep it short enough to fit a speech bubble. Text that is only noise or a fragment of the art can stay as it is. {}",
            from, target, rules
        ),
        "novel" => format!(
            "You translate paragraphs of a novel{} into the language with ISO code \"{}\". Write fluent literary prose in the target language, keep names, \
             honorifics, terms and the narrator's voice consistent, and keep dialogue quotes. Empty entries stay empty. {}",
            from, target, rules
        ),
        _ => format!(
            "You translate movie and series subtitle lines{} into the language with ISO code \"{}\". {} Keep line breaks, HTML tags such as <i>, music symbols and speaker dashes.",
            from, target, rules
        ),
    }
}

/// Pulls the JSON array out of a model's text reply, even when it wraps it in prose or code fences.
pub fn parse_array_reply(text: &str) -> Result<Vec<String>, String> {
    let (a, b) = (text.find('['), text.rfind(']'));
    match (a, b) {
        (Some(a), Some(b)) if b > a => {
            let arr = Json::parse(&text[a..=b]).map_err(|_| "The translation reply was not a JSON array.".to_string())?;
            Ok(arr.items().iter().map(|x| x.str()).collect())
        }
        _ => Err("The translation reply was not a JSON array.".to_string()),
    }
}

fn short(r: &crate::http::Response) -> String {
    r.text().chars().take(160).collect()
}

pub struct Gemini<'a> {
    pub http: &'a dyn Http,
    pub key: String,
    pub model: String,
}

impl Translator for Gemini<'_> {
    fn name(&self) -> String {
        "Gemini".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        self.translate_hinted(texts, source, target, "")
    }

    fn translate_hinted(&self, texts: &[String], source: &str, target: &str, hint: &str) -> Result<Vec<String>, String> {
        let body = Json::obj(vec![
            ("systemInstruction", Json::obj(vec![("parts", Json::Arr(vec![Json::obj(vec![("text", system_prompt_for(source, target, hint).into())])]))])),
            ("contents", Json::Arr(vec![Json::obj(vec![
                ("role", "user".into()),
                ("parts", Json::Arr(vec![Json::obj(vec![("text", Json::from(texts.to_vec()).dump().into())])])),
            ])])),
            ("generationConfig", Json::obj(vec![("temperature", 0.2f64.into()), ("responseMimeType", "application/json".into())])),
        ]);
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", self.model.trim());
        let r = self.http.send(&Request::post(&url).header("x-goog-api-key", self.key.trim()).json_body(body.dump()).timeout(120))?;
        if r.status == 400 && r.text().contains("API key") || r.status == 401 || r.status == 403 {
            return Err("The Gemini API key was rejected. Check it in Settings > Subtitles.".to_string());
        }
        if !r.ok() {
            return Err(format!("Gemini answered {}: {}", r.status, short(&r)));
        }
        let j = r.json()?;
        let text: String = j.path("candidates.0.content.parts").items().iter().map(|p| p.get("text").str()).collect();
        parse_array_reply(&text)
    }
}

/// OpenAI (GPT) and anything that speaks the same chat API.
pub struct OpenAi<'a> {
    pub http: &'a dyn Http,
    pub key: String,
    pub model: String,
    pub base: String,
}

impl Translator for OpenAi<'_> {
    fn name(&self) -> String {
        "OpenAI".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        self.translate_hinted(texts, source, target, "")
    }

    fn translate_hinted(&self, texts: &[String], source: &str, target: &str, hint: &str) -> Result<Vec<String>, String> {
        let body = Json::obj(vec![
            ("model", self.model.trim().into()),
            ("temperature", 0.2f64.into()),
            ("messages", Json::Arr(vec![
                Json::obj(vec![("role", "system".into()), ("content", system_prompt_for(source, target, hint).into())]),
                Json::obj(vec![("role", "user".into()), ("content", Json::from(texts.to_vec()).dump().into())]),
            ])),
        ]);
        let base = if self.base.trim().is_empty() { "https://api.openai.com/v1" } else { self.base.trim() };
        let url = format!("{}/chat/completions", base.trim_end_matches('/'));
        let r = self.http.send(&Request::post(&url).header("Authorization", &format!("Bearer {}", self.key.trim())).json_body(body.dump()).timeout(120))?;
        if r.status == 401 {
            return Err("The OpenAI API key was rejected. Check it in Settings > Subtitles.".to_string());
        }
        if !r.ok() {
            return Err(format!("OpenAI answered {}: {}", r.status, short(&r)));
        }
        let j = r.json()?;
        parse_array_reply(&j.path("choices.0.message.content").str())
    }
}

/// Builds the translator chosen in settings, or None when translation is off or incomplete.
pub fn from_settings<'a>(s: &Subtitles, http: &'a dyn Http) -> Option<Box<dyn Translator + 'a>> {
    if !s.enabled {
        return None;
    }
    match s.provider.as_str() {
        // Without a server address the translation works by itself (see `Automatic`).
        "libretranslate" => Some(Box::new(Automatic::new(http, &s.libre_url, &s.libre_key))),
        "claude" if !s.claude_key.trim().is_empty() => {
            Some(Box::new(Claude { http, key: s.claude_key.trim().to_string(), model: s.claude_model.clone() }))
        }
        "gemini" if !s.gemini_key.trim().is_empty() => Some(Box::new(Gemini { http, key: s.gemini_key.clone(), model: s.gemini_model.clone() })),
        "openai" if !s.openai_key.trim().is_empty() => {
            Some(Box::new(OpenAi { http, key: s.openai_key.clone(), model: s.openai_model.clone(), base: s.openai_base.clone() }))
        }
        // the local AI with its usual model (`routing::for_use` passes the one chosen in Conversation); a cloud
        // translator on Android and iOS
        "ollama" => local_ai(s, http, ""),
        _ => None,
    }
}

/// The translator of the page readers (Images and Text): the one chosen for the subtitles, whether or not subtitle
/// translation is on, and LibreTranslate (no Google) when it is the free one.
pub fn for_readers<'a>(s: &Subtitles, http: &'a dyn Http) -> Box<dyn Translator + 'a> {
    let mut on = s.clone();
    on.enabled = true;
    match from_settings(&on, http) {
        Some(t) if s.provider != "libretranslate" => t,
        _ => Box::new(Automatic::new(http, &s.libre_url, &s.libre_key).without_google()),
    }
}

fn translate_range(tr: &dyn Translator, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
    let got = tr.translate(texts, source, target)?;
    if got.len() == texts.len() {
        return Ok(got);
    }
    // The answer did not line up with the input. Split the batch until it does.
    if texts.len() == 1 {
        return Ok(texts.to_vec());
    }
    let mid = texts.len() / 2;
    let mut a = translate_range(tr, &texts[..mid], source, target)?;
    a.extend(translate_range(tr, &texts[mid..], source, target)?);
    Ok(a)
}

fn translate_range_h(tr: &dyn Translator, texts: &[String], source: &str, target: &str, hint: &str) -> Result<Vec<String>, String> {
    let got = tr.translate_hinted(texts, source, target, hint)?;
    if got.len() == texts.len() {
        return Ok(got);
    }
    if texts.len() == 1 {
        return Ok(texts.to_vec());
    }
    let mid = texts.len() / 2;
    let mut a = translate_range_h(tr, &texts[..mid], source, target, hint)?;
    a.extend(translate_range_h(tr, &texts[mid..], source, target, hint)?);
    Ok(a)
}

/// A 64-bit FNV-1a hash (the same on every run and every version of Rust, unlike `DefaultHasher`).
pub fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Translates `texts` for the readers: what was translated before is read from the disk (so a chapter opened again is not
/// translated again), the rest goes in pieces on up to `threads` requests at the same time (three for a long chapter), and every
/// answer is written to the disk as soon as the translation ends, even when a piece failed.
/// `key` names the translator (a cache of its own), `hint` is "manga" or "novel" (see `system_prompt_for`).
#[allow(clippy::too_many_arguments)]
pub fn translate_cached(
    tr: &dyn Translator,
    texts: &[String],
    source: &str,
    target: &str,
    hint: &str,
    key: &str,
    dir: &std::path::Path,
    threads: usize,
    max_batch: usize,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> Result<Vec<String>, String> {
    let file = dir.join("translations").join(format!("{}-{}.json", key, target));
    let mut known: std::collections::HashMap<u64, String> = std::collections::HashMap::new();
    if let Ok(t) = std::fs::read_to_string(&file) {
        if let Ok(j) = Json::parse(&t) {
            if let Json::Obj(items) = &j {
                for (k, v) in items.iter() {
                    if let (Ok(h), Some(x)) = (u64::from_str_radix(k, 16), v.as_str()) {
                        known.insert(h, x.to_string());
                    }
                }
            }
        }
    }
    let hash = |t: &str| fnv(&format!("{}|{}|{}", source, hint, t));
    // What is still to translate, each text once.
    let mut todo: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for t in texts {
        if t.trim().is_empty() {
            continue;
        }
        let h = hash(t);
        if !known.contains_key(&h) && seen.insert(h) {
            todo.push(t.clone());
        }
    }
    let mut first_error: Option<String> = None;
    let total = todo.len();
    progress(0, total);
    if !todo.is_empty() {
        let threads = threads.max(1);
        let batch = ((todo.len() + threads - 1) / threads).clamp(4, max_batch.max(4));
        let ranges: Vec<(usize, usize)> = (0..todo.len()).step_by(batch).map(|a| (a, (a + batch).min(todo.len()))).collect();
        let shared = Mutex::new(std::mem::take(&mut known));
        let next = AtomicUsize::new(0);
        let failed = AtomicBool::new(false);
        let error: Mutex<Option<String>> = Mutex::new(None);
        let done = AtomicUsize::new(0);
        std::thread::scope(|sc| {
            for _ in 0..threads.min(ranges.len()) {
                sc.spawn(|| loop {
                    if failed.load(Ordering::SeqCst) {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= ranges.len() {
                        return;
                    }
                    let (a, b) = ranges[i];
                    match translate_range_h(tr, &todo[a..b], source, target, hint) {
                        Ok(v) => {
                            let mut m = shared.lock().unwrap();
                            for (src, out) in todo[a..b].iter().zip(v) {
                                m.insert(hash(src), out);
                            }
                            drop(m);
                            let d = done.fetch_add(b - a, Ordering::SeqCst) + (b - a);
                            progress(d, total);
                        }
                        Err(e) => {
                            failed.store(true, Ordering::SeqCst);
                            *error.lock().unwrap() = Some(e);
                        }
                    }
                });
            }
        });
        known = shared.into_inner().unwrap();
        first_error = error.into_inner().unwrap();
    }
    // The answer is built before the cache is trimmed below, or a cache that just grew past the limit would be
    // emptied first and the texts would not be found in it.
    let mut out = Vec::with_capacity(texts.len());
    let mut missing = false;
    for t in texts {
        if t.trim().is_empty() {
            out.push(t.clone());
        } else {
            match known.get(&hash(t)) {
                Some(x) => out.push(x.clone()),
                None => {
                    missing = true;
                    break;
                }
            }
        }
    }
    if !todo.is_empty() {
        // Kept on disk (a cache that grew too much starts again).
        if known.len() > 60_000 {
            known.clear();
        }
        let obj = Json::Obj(known.iter().map(|(k, v)| (format!("{:016x}", k), Json::Str(v.clone()))).collect());
        if let Some(p) = file.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, obj.dump()).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
    }
    if missing {
        return Err(first_error.unwrap_or_else(|| "The translation did not line up with the text.".to_string()));
    }
    Ok(out)
}

/// Requests the readers send at the same time when a long chapter is translated (kept low so the free servers do not refuse).
pub const READER_THREADS: usize = 3;

/// Translates `texts` in batches on a few threads. Calls `progress` with a value from 0 to 1.
pub fn translate_all(
    tr: &dyn Translator,
    texts: &[String],
    source: &str,
    target: &str,
    batch: usize,
    threads: usize,
    progress: &(dyn Fn(f32) + Sync),
) -> Result<Vec<String>, String> {
    translate_all_with(tr, texts, source, target, batch, threads, progress, &AtomicBool::new(false))
}

/// Like `translate_all`, and stops early with an error once `cancel` is set.
#[allow(clippy::too_many_arguments)]
pub fn translate_all_with(
    tr: &dyn Translator,
    texts: &[String],
    source: &str,
    target: &str,
    batch: usize,
    threads: usize,
    progress: &(dyn Fn(f32) + Sync),
    cancel: &AtomicBool,
) -> Result<Vec<String>, String> {
    let batch = batch.max(1);
    let ranges: Vec<(usize, usize)> = (0..texts.len()).step_by(batch).map(|s| (s, (s + batch).min(texts.len()))).collect();
    let results: Mutex<Vec<Option<Vec<String>>>> = Mutex::new(vec![None; ranges.len()]);
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let error: Mutex<Option<String>> = Mutex::new(None);
    std::thread::scope(|sc| {
        for _ in 0..threads.max(1).min(ranges.len().max(1)) {
            sc.spawn(|| loop {
                if failed.load(Ordering::SeqCst) {
                    return;
                }
                if cancel.load(Ordering::SeqCst) {
                    failed.store(true, Ordering::SeqCst);
                    *error.lock().unwrap() = Some("Cancelled.".to_string());
                    return;
                }
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= ranges.len() {
                    return;
                }
                let (a, b) = ranges[i];
                match translate_range(tr, &texts[a..b], source, target) {
                    Ok(v) => {
                        results.lock().unwrap()[i] = Some(v);
                        let d = done.fetch_add(b - a, Ordering::SeqCst) + (b - a);
                        progress(d as f32 / texts.len() as f32);
                    }
                    Err(e) => {
                        failed.store(true, Ordering::SeqCst);
                        *error.lock().unwrap() = Some(e);
                    }
                }
            });
        }
    });
    if let Some(e) = error.into_inner().unwrap() {
        return Err(e);
    }
    Ok(results.into_inner().unwrap().into_iter().flat_map(|r| r.unwrap_or_default()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    struct Upper {
        break_big: bool,
    }
    impl Translator for Upper {
        fn name(&self) -> String {
            "up".into()
        }
        fn translate(&self, t: &[String], _s: &str, _t: &str) -> Result<Vec<String>, String> {
            if self.break_big && t.len() > 2 {
                return Ok(t.iter().skip(1).map(|x| x.to_uppercase()).collect());
            }
            Ok(t.iter().map(|x| x.to_uppercase()).collect())
        }
    }

    #[test]
    fn keeps_order_across_batches_and_threads() {
        let texts: Vec<String> = (0..103).map(|i| format!("line {}", i)).collect();
        let out = translate_all(&Upper { break_big: false }, &texts, "en", "pt", 10, 4, &|_| {}).unwrap();
        assert_eq!(out, texts.iter().map(|t| t.to_uppercase()).collect::<Vec<_>>());
    }

    #[test]
    fn splits_batches_with_the_wrong_length() {
        let texts: Vec<String> = (0..9).map(|i| format!("l{}", i)).collect();
        let out = translate_all(&Upper { break_big: true }, &texts, "en", "pt", 9, 2, &|_| {}).unwrap();
        assert_eq!(out, texts.iter().map(|t| t.to_uppercase()).collect::<Vec<_>>());
    }

    struct Fails;
    impl Translator for Fails {
        fn name(&self) -> String {
            "f".into()
        }
        fn translate(&self, _: &[String], _: &str, _: &str) -> Result<Vec<String>, String> {
            Err("no key".into())
        }
    }

    #[test]
    fn stops_on_a_hard_error() {
        let texts: Vec<String> = (0..50).map(|i| i.to_string()).collect();
        assert_eq!(translate_all(&Fails, &texts, "en", "pt", 5, 3, &|_| {}).unwrap_err(), "no key");
    }

    #[test]
    fn libretranslate_request_and_answer() {
        let http = Fake::new().route("POST http://lt:5000/translate", 200, r#"{"translatedText":["a","b"]}"#);
        let lt = LibreTranslate { http: &http, url: "http://lt:5000/".into(), key: "k".into() };
        assert_eq!(lt.translate(&["x".into(), "y".into()], "en", "pt").unwrap(), vec!["a", "b"]);
        let body = Json::parse(&String::from_utf8_lossy(&http.requests()[0].body.as_ref().unwrap().1)).unwrap();
        assert_eq!((body.get("target").str(), body.get("api_key").str(), body.get("source").str()), ("pt".into(), "k".into(), "en".into()));
    }

    #[test]
    fn automatic_uses_a_working_server_then_falls_back_to_the_web() {
        let http = Fake::new().route("POST https://translate.terraprint.co/translate", 403, "key needed").route("POST https://lt.vern.cc/translate", 200, r#"{"translatedText":["a","b"]}"#);
        let a = Automatic::new(&http, "", "");
        assert_eq!(a.translate(&["x".into(), "y".into()], "en", "pt").unwrap(), vec!["a", "b"]);
        assert!(a.name().contains("lt.vern.cc"));
        // Every server fails: the web translation answers, line by line.
        let http = Fake::new().route("translate_a/single", 200, r#"[[["Olá ","Hello ",null,null,1],["mundo","world",null,null,1]],null,"en"]"#);
        let a = Automatic::new(&http, "http://localhost:5000", "");
        assert_eq!(a.translate(&["Hello world".into()], "en", "pt").unwrap(), vec!["Olá mundo"]);
        assert!(a.name().contains("web"));
        // Nothing answers at all.
        let none = Fake::new();
        assert!(Automatic::new(&none, "", "").translate(&["Hi".into()], "en", "pt").is_err());
    }

    #[test]
    fn libretranslate_alone_never_falls_back_to_the_web() {
        // the web translation would answer, but the "LibreTranslate" choice must not use it
        let http = Fake::new().route("translate_a/single", 200, r#"[[["Olá","Hello",null,null,1]],null,"en"]"#);
        let a = Automatic::new(&http, "http://my.server", "").libre_only();
        assert!(a.translate(&["Hello".into()], "en", "pt").unwrap_err().contains("LibreTranslate"));
        assert!(http.requests().iter().all(|r| r.url.contains("/translate") && !r.url.contains("translate_a") && !r.url.contains("microsoft")));
        assert_eq!(a.name(), "LibreTranslate");
    }

    #[test]
    fn claude_request_and_answer() {
        let http = Fake::new().route("POST https://api.anthropic.com/v1/messages", 200, r#"{"content":[{"type":"text","text":"Sure: [\"um\",\"dois\"]"}]}"#);
        let c = Claude { http: &http, key: "sk".into(), model: "m".into() };
        assert_eq!(c.translate(&["one".into(), "two".into()], "en", "pt").unwrap(), vec!["um", "dois"]);
        let req = &http.requests()[0];
        assert!(req.headers.iter().any(|(k, v)| k == "x-api-key" && v == "sk"));
        let denied = Fake::new().route("messages", 401, "{}");
        assert!(Claude { http: &denied, key: "x".into(), model: "m".into() }.translate(&["a".into()], "en", "pt").unwrap_err().contains("rejected"));
    }

    #[test]
    fn picks_a_translator_from_settings() {
        let http = Fake::new();
        let mut s = crate::settings::Settings::default().subtitles;
        assert!(from_settings(&s, &http).unwrap().name().starts_with("LibreTranslate"));
        s.provider = "claude".into();
        assert!(from_settings(&s, &http).is_none());
        s.claude_key = "k".into();
        assert_eq!(from_settings(&s, &http).unwrap().name(), "Claude");
        s.enabled = false;
        assert!(from_settings(&s, &http).is_none());
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn gemini_reads_the_reply_and_sends_the_key_as_a_header() {
        let f = Fake::new().route(
            "generativelanguage.googleapis.com",
            200,
            r#"{"candidates":[{"content":{"parts":[{"text":"```json\n[\"Ola\",\"Tchau\"]\n```"}]}}]}"#,
        );
        let g = Gemini { http: &f, key: "K".into(), model: "gemini-2.5-flash".into() };
        assert_eq!(g.translate(&["Hi".into(), "Bye".into()], "en", "pt").unwrap(), vec!["Ola", "Tchau"]);
        let r = &f.requests()[0];
        assert!(r.url.contains("models/gemini-2.5-flash:generateContent"));
        assert!(r.headers.iter().any(|(k, v)| k == "x-goog-api-key" && v == "K"));
        assert!(!r.url.contains("key="));
    }

    #[test]
    fn gemini_bad_key() {
        let f = Fake::new().route("googleapis", 403, "{}");
        let g = Gemini { http: &f, key: "K".into(), model: "m".into() };
        assert!(g.translate(&["a".into()], "", "pt").unwrap_err().contains("API key"));
    }

    #[test]
    fn openai_uses_the_configured_base_url() {
        let f = Fake::new().route("my.server/v1/chat/completions", 200, r#"{"choices":[{"message":{"content":"[\"Hola\"]"}}]}"#);
        let o = OpenAi { http: &f, key: "sk".into(), model: "gpt-4o-mini".into(), base: "https://my.server/v1/".into() };
        assert_eq!(o.translate(&["Hi".into()], "en", "es").unwrap(), vec!["Hola"]);
        assert!(f.requests()[0].headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer sk"));
    }

    #[test]
    fn openai_unauthorised_and_default_base() {
        let f = Fake::new().route("api.openai.com/v1/chat/completions", 401, "{}");
        let o = OpenAi { http: &f, key: "x".into(), model: "m".into(), base: String::new() };
        assert!(o.translate(&["a".into()], "", "pt").unwrap_err().contains("rejected"));
    }

    #[test]
    fn from_settings_picks_each_provider() {
        let f = Fake::new();
        let mut s = crate::settings::Settings::default().subtitles;
        s.provider = "gemini".into();
        assert!(from_settings(&s, &f).is_none());
        s.gemini_key = "k".into();
        assert_eq!(from_settings(&s, &f).unwrap().name(), "Gemini");
        s.provider = "openai".into();
        s.openai_key = "k".into();
        assert_eq!(from_settings(&s, &f).unwrap().name(), "OpenAI");
    }

    #[test]
    fn array_reply_parsing() {
        assert_eq!(parse_array_reply("Sure! [\"a\", \"b\"] done").unwrap(), vec!["a", "b"]);
        assert!(parse_array_reply("no array").is_err());
    }
}
