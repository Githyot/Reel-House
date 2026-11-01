//! Research mode's ad blocker ("Bloqueador de anúncios (uBlock)"): uBlock Origin's own filter lists (uBlock filters,
//! privacy, unbreak) with EasyList, EasyPrivacy and Peter Lowe's list, matched by Brave's adblock-rust.
//!
//! uBlock Origin itself cannot be loaded: the browser is CEF 154, and CEF dropped its (Alloy) Chrome extension support
//! in M128. So the same lists are applied the way uBlock applies them: every network request of a page is checked
//! (Chromium's network thread asks `should_block`), and the element hiding rules of a site are given as CSS
//! (`page_css`), which the browser adds to each page.
//!
//! The lists are downloaded into `<profile>/adblock/` the first time and refreshed once a week, in the background;
//! until the first download finishes nothing is blocked. The engine is global (one per process) because Chromium's
//! threads read it without going through the controller.
use std::sync::RwLock;

/// (file name, address) of each list.
pub const LISTS: &[(&str, &str)] = &[
    ("ublock-filters.txt", "https://ublockorigin.github.io/uAssets/filters/filters.txt"),
    ("ublock-privacy.txt", "https://ublockorigin.github.io/uAssets/filters/privacy.txt"),
    ("ublock-unbreak.txt", "https://ublockorigin.github.io/uAssets/filters/unbreak.txt"),
    ("ublock-badware.txt", "https://ublockorigin.github.io/uAssets/filters/badware.txt"),
    ("easylist.txt", "https://easylist.to/easylist/easylist.txt"),
    ("easyprivacy.txt", "https://easylist.to/easylist/easyprivacy.txt"),
    ("peter-lowe.txt", "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=adblockplus&showintro=0&mimetype=plaintext"),
];

/// How old the lists may get before they are downloaded again.
pub const REFRESH_SECS: u64 = 7 * 24 * 3600;

/// The host of an address, lower case, without "www.".
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = if host.starts_with('[') { host.split(']').next().map(|h| format!("{}]", h)).unwrap_or_default() } else { host.split(':').next().unwrap_or("").to_string() };
    let host = host.to_ascii_lowercase();
    host.strip_prefix("www.").map(str::to_string).unwrap_or(host)
}

/// Whether `host` is `site` or one of its subdomains.
fn under(host: &str, site: &str) -> bool {
    !site.is_empty() && (host == site || host.ends_with(&format!(".{}", site)))
}

/// The CSS that hides `selectors` (in chunks, so one selector the page's CSS engine rejects drops only its chunk).
pub fn hide_css(selectors: &[String]) -> String {
    let mut out = String::new();
    for chunk in selectors.chunks(64) {
        out.push_str(&chunk.join(",\n"));
        out.push_str("{display:none!important}\n");
    }
    out
}

/// The switches the user set: on or off, and the sites allowed to show their ads.
#[derive(Default)]
struct Config {
    enabled: bool,
    allow: Vec<String>,
}

static CONFIG: RwLock<Config> = RwLock::new(Config { enabled: true, allow: Vec::new() });

/// Turns blocking on or off and sets the sites where nothing is blocked.
pub fn configure(enabled: bool, allow: &[String]) {
    if let Ok(mut c) = CONFIG.write() {
        c.enabled = enabled;
        c.allow = allow.iter().map(|s| host_of(s)).filter(|s| !s.is_empty()).collect();
    }
}

/// Whether blocking applies on a page at `page_url`.
pub fn active_on(page_url: &str) -> bool {
    let Ok(c) = CONFIG.read() else { return false };
    if !c.enabled {
        return false;
    }
    let h = host_of(page_url);
    !c.allow.iter().any(|s| under(&h, s))
}

#[cfg(all(feature = "app", feature = "desktop"))]
mod imp {
    use super::*;
    use crate::http::{Http, Request};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::path::{Path, PathBuf};

    static ENGINE: RwLock<Option<Arc<adblock::Engine>>> = RwLock::new(None);
    static STATUS: Mutex<(usize, u64, String)> = Mutex::new((0, 0, String::new())); // (rules, lists' time, problem)
    static BUSY: AtomicBool = AtomicBool::new(false);

    fn age(path: &Path) -> Option<u64> {
        let m = std::fs::metadata(path).ok()?.modified().ok()?;
        Some(std::time::SystemTime::now().duration_since(m).map(|d| d.as_secs()).unwrap_or(0))
    }

    fn mtime(path: &Path) -> u64 {
        std::fs::metadata(path).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
    }

    /// Builds the engine from the lists on disk. Returns the number of rules.
    fn build(dir: &Path) -> usize {
        let mut set = adblock::lists::FilterSet::new(false);
        let mut n = 0;
        let mut newest = 0;
        for (file, _) in LISTS {
            let p = dir.join(file);
            if let Ok(text) = std::fs::read_to_string(&p) {
                n += text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('!') && !l.starts_with('[')).count();
                let _ = set.add_filter_list(text, adblock::lists::ParseOptions::default());
                newest = newest.max(mtime(&p));
            }
        }
        if n > 0 {
            let engine = adblock::Engine::new_with_filter_set(set);
            if let Ok(mut e) = ENGINE.write() {
                *e = Some(Arc::new(engine));
            }
        }
        if let Ok(mut s) = STATUS.lock() {
            s.0 = n;
            s.1 = newest;
        }
        n
    }

    /// Downloads the lists that are missing or older than a week (all of them with `force`). Returns whether one changed.
    fn refresh(dir: &Path, force: bool) -> bool {
        let _ = std::fs::create_dir_all(dir);
        let http = crate::net::UreqHttp::new();
        let mut changed = false;
        let mut problem = String::new();
        for (file, url) in LISTS {
            let p = dir.join(file);
            if !force && age(&p).is_some_and(|a| a < REFRESH_SECS) {
                continue;
            }
            match http.send(&Request::get(url).timeout(60)) {
                Ok(r) if r.ok() && r.body.len() > 64 => {
                    let tmp = p.with_extension("txt.tmp");
                    if std::fs::write(&tmp, &r.body).is_ok() && std::fs::rename(&tmp, &p).is_ok() {
                        changed = true;
                    }
                }
                Ok(r) => problem = format!("{} answered {}", host_of(url), r.status),
                Err(e) => problem = e,
            }
        }
        if let Ok(mut s) = STATUS.lock() {
            s.2 = problem;
        }
        changed
    }

    /// Loads the lists in `dir` (on a thread of its own) and downloads the ones that are missing or a week old.
    pub fn start(dir: PathBuf, force: bool) {
        if BUSY.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = std::thread::Builder::new().name("adblock".into()).spawn(move || {
            let have = ENGINE.read().map(|e| e.is_some()).unwrap_or(false);
            if !have {
                build(&dir);
            }
            if refresh(&dir, force) {
                let n = build(&dir);
                crate::debug::log("adblock", &format!("lists updated: {} rules", n));
            }
            BUSY.store(false, Ordering::SeqCst);
        });
    }

    fn engine() -> Option<Arc<adblock::Engine>> {
        ENGINE.read().ok().and_then(|e| e.clone())
    }

    pub fn should_block(url: &str, source: &str, kind: &str) -> bool {
        if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ws")) {
            return false;
        }
        if !active_on(source) {
            return false;
        }
        let Some(e) = engine() else { return false };
        let Ok(req) = adblock::request::Request::new(url, source, kind, "GET") else { return false };
        e.check_network_request(&req).should_block()
    }

    pub fn page_css(url: &str) -> String {
        if !active_on(url) {
            return String::new();
        }
        let Some(e) = engine() else { return String::new() };
        let r = e.url_cosmetic_resources(url);
        let mut sel: Vec<String> = r.hide_selectors.into_iter().collect();
        sel.sort();
        hide_css(&sel)
    }

    pub fn status() -> (usize, u64, String, bool) {
        let s = STATUS.lock().map(|s| s.clone()).unwrap_or_default();
        (s.0, s.1, s.2, BUSY.load(Ordering::SeqCst))
    }
}

#[cfg(not(all(feature = "app", feature = "desktop")))]
mod imp {
    pub fn start(_dir: std::path::PathBuf, _force: bool) {}
    pub fn should_block(_url: &str, _source: &str, _kind: &str) -> bool {
        false
    }
    pub fn page_css(_url: &str) -> String {
        String::new()
    }
    pub fn status() -> (usize, u64, String, bool) {
        (0, 0, String::new(), false)
    }
}

/// Loads the lists from `dir` and refreshes them when a week old (`force`: download them all now).
pub fn start(dir: std::path::PathBuf, force: bool) {
    imp::start(dir, force)
}

/// Whether a request for `url` made by the page at `source` should be cancelled. `kind` is the request type
/// ("script", "image", "stylesheet", "xmlhttprequest", "subdocument", "media", "font", "ping", "other"…).
/// Any thread.
pub fn should_block(url: &str, source: &str, kind: &str) -> bool {
    imp::should_block(url, source, kind)
}

/// The element hiding CSS for the page at `url` ("" when nothing applies).
pub fn page_css(url: &str) -> String {
    imp::page_css(url)
}

/// (rules loaded, time of the newest list, last download problem, downloading now).
pub fn status() -> (usize, u64, String, bool) {
    imp::status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(host_of("https://www.Example.com:8080/a?b"), "example.com");
        assert_eq!(host_of("http://user@sub.site.org/"), "sub.site.org");
        assert!(under("a.b.com", "b.com"));
        assert!(!under("ab.com", "b.com"));
    }

    #[test]
    fn css_chunks() {
        let s: Vec<String> = (0..70).map(|i| format!(".ad{}", i)).collect();
        assert_eq!(hide_css(&s).matches("display:none").count(), 2);
    }

    #[cfg(all(feature = "app", feature = "desktop"))]
    #[test]
    fn matching() {
        let set = {
            let mut s = adblock::lists::FilterSet::new(false);
            let _ = s.add_filter_list("||ads.example.com^\nexample.org##.banner\n".to_string(), adblock::lists::ParseOptions::default());
            s
        };
        let e = adblock::Engine::new_with_filter_set(set);
        let r = adblock::request::Request::new("https://ads.example.com/x.js", "https://news.site/", "script", "GET").unwrap();
        assert!(e.check_network_request(&r).should_block());
        assert!(e.url_cosmetic_resources("https://example.org/").hide_selectors.contains(".banner"));
    }
}
