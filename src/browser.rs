//! Research mode, the built-in browser: the history of visited pages, the download list and the browser's own
//! preferences (search engine, pinned pages, the tabs left open), kept as JSON in `<data dir>/browser/`.
//! The pages themselves are drawn by Chromium (CEF, off screen) in the Qt front end (qt/src/webpane.cpp, cef_host.cpp);
//! the core only keeps these lists, so they are in the backup and the same for the Research page and the player's
//! quick search.
use crate::json::Json;
use std::path::{Path, PathBuf};

/// The search engines the address field can use: id, name, search address (the query is appended), start page.
pub const ENGINES: [(&str, &str, &str, &str); 3] = [
    ("google", "Google", "https://www.google.com/search?q=", "https://www.google.com/"),
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q=", "https://duckduckgo.com/"),
    ("bing", "Bing", "https://www.bing.com/search?q=", "https://www.bing.com/"),
];

/// More than this and the oldest visits are dropped (about 3 MB of JSON).
pub const MAX_HISTORY: usize = 20_000;
/// The download list keeps the newest entries only.
pub const MAX_DOWNLOADS: usize = 300;

fn engine(id: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    ENGINES.iter().copied().find(|e| e.0 == id).unwrap_or(ENGINES[0])
}

pub fn search_url(engine_id: &str, q: &str) -> String {
    format!("{}{}", engine(engine_id).2, crate::util::form_encode(q.trim()).replace("%20", "+"))
}

pub fn home_url(engine_id: &str) -> String {
    engine(engine_id).3.to_string()
}

/// What the address field typed means: an address (with or without `https://`) or words to search for.
pub fn address_for(input: &str, engine_id: &str) -> String {
    let t = input.trim();
    if t.is_empty() {
        return home_url(engine_id);
    }
    let lower = t.to_ascii_lowercase();
    // Anything with a scheme the browser opens itself.
    for scheme in ["http://", "https://", "file:///", "about:", "edge://", "data:", "view-source:"] {
        if lower.starts_with(scheme) {
            return t.to_string();
        }
    }
    if t.contains(char::is_whitespace) {
        return search_url(engine_id, t);
    }
    // A Windows path typed by hand (C:\…) opens as a file.
    let b = t.as_bytes();
    if b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/') {
        return format!("file:///{}", t.replace('\\', "/"));
    }
    // host[:port][/path]: a dot in the host with a real ending ("example.com", "192.168.0.1"), or localhost.
    let host = t.split(['/', '?', '#']).next().unwrap_or("");
    let name = host.rsplit_once('@').map(|x| x.1).unwrap_or(host);
    let name = match name.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.bytes().all(|c| c.is_ascii_digit()) => h,
        _ => name,
    };
    let ipv4 = name.split('.').count() == 4 && name.split('.').all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|c| c.is_ascii_digit()));
    let looks_like_host = name.eq_ignore_ascii_case("localhost")
        || ipv4
        || (name.contains('.')
            && !name.starts_with('.')
            && !name.ends_with('.')
            && name.rsplit('.').next().map(|tld| tld.len() >= 2 && tld.bytes().all(|c| c.is_ascii_alphanumeric())).unwrap_or(false)
            && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-'));
    if looks_like_host {
        let local = name.eq_ignore_ascii_case("localhost") || name.starts_with("127.") || name.starts_with("192.168.") || name.starts_with("10.");
        return format!("{}{}", if local { "http://" } else { "https://" }, t);
    }
    search_url(engine_id, t)
}

/// Addresses that are not worth keeping in the history.
fn skip_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.is_empty() || u.starts_with("about:") || u.starts_with("data:") || u.starts_with("edge:") || u.starts_with("chrome") || u.starts_with("blob:")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Visit {
    /// Unix seconds.
    pub time: u64,
    pub title: String,
    pub url: String,
    /// "page" (Research mode), "quick" (the quick search over the video) or "lens" (a picture search).
    pub kind: String,
}

impl Visit {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![("time", Json::Num(self.time as f64)), ("title", Json::Str(self.title.clone())), ("url", Json::Str(self.url.clone())), ("kind", Json::Str(self.kind.clone()))])
    }
    fn from_json(j: &Json) -> Option<Visit> {
        let url = j.get("url").str();
        if url.is_empty() {
            return None;
        }
        Some(Visit { time: j.get("time").as_u64().unwrap_or(0), title: j.get("title").str(), url, kind: j.get("kind").as_str().unwrap_or("page").to_string() })
    }
}

/// Visited pages, oldest first.
#[derive(Default, Debug)]
pub struct History {
    pub items: Vec<Visit>,
}

impl History {
    /// Adds a visit. The same address as the newest entry within half an hour (a reload, a page that changed its own
    /// address) only refreshes that entry. Returns whether anything changed.
    pub fn add(&mut self, v: Visit) -> bool {
        if skip_url(&v.url) {
            return false;
        }
        if let Some(last) = self.items.last_mut() {
            if last.url == v.url && v.time.saturating_sub(last.time) < 1800 {
                last.time = v.time.max(last.time);
                if !v.title.is_empty() {
                    last.title = v.title;
                }
                // a picture search stays one even if the page is opened again from the history
                if last.kind != "lens" {
                    last.kind = v.kind;
                }
                return true;
            }
        }
        self.items.push(v);
        if self.items.len() > MAX_HISTORY {
            let extra = self.items.len() - MAX_HISTORY;
            self.items.drain(..extra);
        }
        true
    }

    /// The page's title arrived after the visit was written: put it on the newest entry with that address.
    pub fn set_title(&mut self, url: &str, title: &str) -> bool {
        if title.trim().is_empty() {
            return false;
        }
        let n = self.items.len();
        for v in self.items.iter_mut().rev().take(50.min(n)) {
            if v.url == url {
                if v.title == title {
                    return false;
                }
                v.title = title.to_string();
                return true;
            }
        }
        false
    }

    /// Newest first; every word of `q` must be in the title or the address (any case).
    pub fn search(&self, q: &str, limit: usize) -> Vec<&Visit> {
        let words: Vec<String> = q.split_whitespace().map(|w| w.to_lowercase()).collect();
        self.items
            .iter()
            .rev()
            .filter(|v| {
                if words.is_empty() {
                    return true;
                }
                let hay = format!("{} {}", v.title.to_lowercase(), v.url.to_lowercase());
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .take(limit)
            .collect()
    }

    pub fn remove(&mut self, time: u64, url: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|v| !(v.time == time && v.url == url));
        before != self.items.len()
    }

    /// Clears what was visited since `since` (Unix seconds); 0 clears everything.
    pub fn clear_since(&mut self, since: u64) {
        self.items.retain(|v| since > 0 && v.time < since);
    }

    pub fn to_json(&self) -> Json {
        Json::Arr(self.items.iter().map(Visit::to_json).collect())
    }

    pub fn from_json(j: &Json) -> History {
        let mut items: Vec<Visit> = j.items().iter().filter_map(Visit::from_json).collect();
        items.sort_by_key(|v| v.time);
        History { items }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct DlItem {
    /// Given by the front end: Chromium's download id with the start time of the run (its ids start again each run).
    pub id: String,
    pub url: String,
    /// Where the file is written.
    pub path: String,
    pub received: u64,
    /// 0 when the server did not say.
    pub total: u64,
    /// "progress", "paused", "done", "failed", "cancelled" or "interrupted" (the app closed in the middle).
    pub state: String,
    /// When it started (Unix seconds).
    pub time: u64,
}

impl DlItem {
    pub fn name(&self) -> String {
        let p = self.path.replace('\\', "/");
        let n = p.rsplit('/').next().unwrap_or("").to_string();
        if !n.is_empty() {
            return n;
        }
        self.url.split(['?', '#']).next().unwrap_or("").rsplit('/').find(|s| !s.is_empty()).unwrap_or("download").to_string()
    }
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("id", Json::Str(self.id.clone())),
            ("url", Json::Str(self.url.clone())),
            ("path", Json::Str(self.path.clone())),
            ("name", Json::Str(self.name())),
            ("received", Json::Num(self.received as f64)),
            ("total", Json::Num(self.total as f64)),
            ("state", Json::Str(self.state.clone())),
            ("time", Json::Num(self.time as f64)),
        ])
    }
    fn from_json(j: &Json) -> Option<DlItem> {
        let id = j.get("id").str();
        if id.is_empty() {
            return None;
        }
        Some(DlItem {
            id,
            url: j.get("url").str(),
            path: j.get("path").str(),
            received: j.get("received").as_u64().unwrap_or(0),
            total: j.get("total").as_u64().unwrap_or(0),
            state: j.get("state").str(),
            time: j.get("time").as_u64().unwrap_or(0),
        })
    }
}

/// Downloads, oldest first.
#[derive(Default, Debug)]
pub struct Downloads {
    pub items: Vec<DlItem>,
}

impl Downloads {
    /// Adds or updates a download. Returns true when it is new or its state changed: worth writing to disk (the
    /// progress alone changes several times a second and is not saved).
    pub fn update(&mut self, d: DlItem) -> bool {
        if let Some(cur) = self.items.iter_mut().find(|x| x.id == d.id) {
            let changed = cur.state != d.state || cur.path != d.path;
            cur.received = d.received;
            if d.total > 0 {
                cur.total = d.total;
            }
            if !d.path.is_empty() {
                cur.path = d.path;
            }
            if !d.url.is_empty() {
                cur.url = d.url;
            }
            if !d.state.is_empty() {
                cur.state = d.state;
            }
            return changed;
        }
        self.items.push(d);
        if self.items.len() > MAX_DOWNLOADS {
            let extra = self.items.len() - MAX_DOWNLOADS;
            self.items.drain(..extra);
        }
        true
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|x| x.id != id);
        before != self.items.len()
    }

    /// Takes the finished, failed and cancelled ones off the list (the files stay).
    pub fn clear_finished(&mut self) {
        self.items.retain(|x| x.state == "progress" || x.state == "paused");
    }

    pub fn to_json(&self) -> Json {
        Json::Arr(self.items.iter().map(DlItem::to_json).collect())
    }

    /// A download that was running when the app closed cannot go on: it is shown as interrupted.
    pub fn from_json(j: &Json) -> Downloads {
        let mut items: Vec<DlItem> = j.items().iter().filter_map(DlItem::from_json).collect();
        for d in items.iter_mut() {
            if d.state == "progress" || d.state == "paused" || d.state.is_empty() {
                d.state = "interrupted".into();
            }
        }
        Downloads { items }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub title: String,
    pub url: String,
}

impl Page {
    fn to_json(&self) -> Json {
        Json::obj(vec![("title", Json::Str(self.title.clone())), ("url", Json::Str(self.url.clone()))])
    }
    fn list(j: &Json) -> Vec<Page> {
        j.items().iter().filter(|x| !x.get("url").str().is_empty()).map(|x| Page { title: x.get("title").str(), url: x.get("url").str() }).collect()
    }
}

/// The browser's own preferences (shown in the Research page's settings sheet).
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    /// "google", "duckduckgo" or "bing".
    pub engine: String,
    /// Pinned pages ("Essentials" in Zen): always at the top of the sidebar.
    pub pins: Vec<Page>,
    /// The tabs open when Research mode was left, opened again next time.
    pub tabs: Vec<Page>,
    pub restore_tabs: bool,
    /// The quick search over the video pauses it while open.
    pub pause_quick: bool,
    /// Size of the quick search window (logical pixels).
    pub quick_w: u32,
    pub quick_h: u32,
    /// The ad blocker (uBlock's lists, see `crate::adblock`) is on.
    pub adblock: bool,
    /// Sites where the ad blocker is off (hosts, "example.com" covers its subdomains).
    pub adblock_allow: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs { engine: "google".into(), pins: vec![], tabs: vec![], restore_tabs: true, pause_quick: true, quick_w: 520, quick_h: 420, adblock: true, adblock_allow: vec![] }
    }
}

impl Prefs {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("engine", Json::Str(self.engine.clone())),
            ("pins", Json::Arr(self.pins.iter().map(Page::to_json).collect())),
            ("tabs", Json::Arr(self.tabs.iter().map(Page::to_json).collect())),
            ("restore_tabs", Json::Bool(self.restore_tabs)),
            ("pause_quick", Json::Bool(self.pause_quick)),
            ("quick_w", Json::Num(self.quick_w as f64)),
            ("quick_h", Json::Num(self.quick_h as f64)),
            ("adblock", Json::Bool(self.adblock)),
            ("adblock_allow", Json::Arr(self.adblock_allow.iter().map(|h| Json::Str(h.clone())).collect())),
        ])
    }
    pub fn from_json(j: &Json) -> Prefs {
        let d = Prefs::default();
        let engine = j.get("engine").str();
        Prefs {
            engine: if ENGINES.iter().any(|e| e.0 == engine) { engine } else { d.engine },
            pins: Page::list(j.get("pins")),
            tabs: Page::list(j.get("tabs")),
            restore_tabs: j.get("restore_tabs").as_bool().unwrap_or(d.restore_tabs),
            pause_quick: j.get("pause_quick").as_bool().unwrap_or(d.pause_quick),
            quick_w: j.get("quick_w").as_u64().map(|x| x.clamp(280, 4000) as u32).unwrap_or(d.quick_w),
            quick_h: j.get("quick_h").as_u64().map(|x| x.clamp(200, 4000) as u32).unwrap_or(d.quick_h),
            adblock: j.get("adblock").as_bool().unwrap_or(d.adblock),
            adblock_allow: j.get("adblock_allow").items().iter().map(|h| h.str()).filter(|h| !h.is_empty()).collect(),
        }
    }

    /// Pins or unpins a page. Returns whether it is pinned now.
    pub fn pin(&mut self, url: &str, title: &str, on: bool) -> bool {
        self.pins.retain(|p| p.url != url);
        if on && !skip_url(url) {
            self.pins.push(Page { title: title.to_string(), url: url.to_string() });
            return true;
        }
        false
    }
}

/// Everything of Research mode the core keeps. Read from disk on first use (the history can be a few MB).
pub struct Browser {
    /// The folder of Chromium's own profile (cookies, sign-ins), shared by every profile of the app: the engine reads
    /// it once when it starts.
    pub dir: PathBuf,
    /// The folder of the lists (history, downloads, pins and tabs), the one of the app's profile in use.
    pub lists: PathBuf,
    loaded: bool,
    pub history: History,
    pub downloads: Downloads,
    pub prefs: Prefs,
    /// The words the history panel is filtered with.
    pub query: String,
    /// The answer to the last `browser_address` command.
    pub address: String,
    /// The video frame saved for a picture search, or why it failed.
    pub frame: Result<String, String>,
    /// The number of the newest `browser_grab_frame_async` request (older answers are dropped).
    pub frame_token: u64,
}

fn read_json(path: &Path) -> Json {
    std::fs::read_to_string(path).ok().and_then(|t| Json::parse(&t).ok()).unwrap_or(Json::Null)
}

/// Writes through a temporary file, so a crash in the middle never leaves half a file.
fn write_json(path: &Path, j: &Json) {
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, j.dump()).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

impl Browser {
    pub fn new(dir: PathBuf) -> Browser {
        Browser::with_lists(dir.clone(), dir)
    }

    /// The engine's folder and the lists' folder apart (a profile of the app other than the main one).
    pub fn with_lists(dir: PathBuf, lists: PathBuf) -> Browser {
        Browser {
            dir,
            lists,
            loaded: false,
            history: History::default(),
            downloads: Downloads::default(),
            prefs: Prefs::default(),
            query: String::new(),
            address: String::new(),
            frame: Err(String::new()),
            frame_token: 0,
        }
    }

    pub fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.history = History::from_json(&read_json(&self.lists.join("history.json")));
        self.downloads = Downloads::from_json(&read_json(&self.lists.join("downloads.json")));
        self.prefs = Prefs::from_json(&read_json(&self.lists.join("settings.json")));
        self.apply_adblock(false);
    }

    /// Hands the ad blocker's switches to the blocker and loads its lists (kept with Chromium's profile, shared by the
    /// app's profiles); `force` downloads the lists again now.
    pub fn apply_adblock(&self, force: bool) {
        crate::adblock::configure(self.prefs.adblock, &self.prefs.adblock_allow);
        if self.prefs.adblock || force {
            crate::adblock::start(self.dir.join("adblock"), force);
        }
    }

    /// Allows (`on` = false: blocking off) or blocks again the site of `url`. Returns whether it changed.
    pub fn adblock_site(&mut self, url: &str, blocked: bool) -> bool {
        let h = crate::adblock::host_of(url);
        if h.is_empty() {
            return false;
        }
        let had = self.prefs.adblock_allow.iter().any(|x| *x == h);
        if blocked == !had {
            return false;
        }
        if blocked {
            self.prefs.adblock_allow.retain(|x| *x != h);
        } else {
            self.prefs.adblock_allow.push(h);
        }
        true
    }

    /// Another profile of the app is in use: its lists are read from its own folder when next needed.
    pub fn set_lists(&mut self, lists: PathBuf) {
        if lists != self.lists {
            self.lists = lists;
            self.loaded = false;
            self.history = History::default();
            self.downloads = Downloads::default();
            self.prefs = Prefs::default();
            self.query.clear();
        }
    }

    pub fn save_history(&self) {
        write_json(&self.lists.join("history.json"), &self.history.to_json());
    }
    pub fn save_downloads(&self) {
        write_json(&self.lists.join("downloads.json"), &self.downloads.to_json());
    }
    pub fn save_prefs(&self) {
        write_json(&self.lists.join("settings.json"), &self.prefs.to_json());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rh_browser_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn visit(time: u64, url: &str, title: &str) -> Visit {
        Visit { time, title: title.into(), url: url.into(), kind: "page".into() }
    }

    #[test]
    fn typed_text_becomes_an_address_or_a_search() {
        assert_eq!(address_for("example.com", "google"), "https://example.com");
        assert_eq!(address_for("https://a.b/c d", "google"), "https://a.b/c d");
        assert_eq!(address_for("localhost:8080/x", "google"), "http://localhost:8080/x");
        assert_eq!(address_for("192.168.0.1", "google"), "http://192.168.0.1");
        assert_eq!(address_for("one piece ep 1", "google"), "https://www.google.com/search?q=one+piece+ep+1");
        assert_eq!(address_for("naruto", "duckduckgo"), "https://duckduckgo.com/?q=naruto");
        assert_eq!(address_for("c++ & rust", "bing"), "https://www.bing.com/search?q=c%2B%2B+%26+rust");
        assert_eq!(address_for("", "bing"), "https://www.bing.com/");
        assert_eq!(address_for("file.", "google"), "https://www.google.com/search?q=file.");
        assert_eq!(address_for("v1.2", "google"), "https://www.google.com/search?q=v1.2");
        assert_eq!(address_for("sub.example.co.uk/path?q=1", "google"), "https://sub.example.co.uk/path?q=1");
        assert_eq!(address_for("C:\\Users\\me\\a.pdf", "google"), "file:///C:/Users/me/a.pdf");
        assert_eq!(address_for("unknown-engine-test", "nope"), "https://www.google.com/search?q=unknown-engine-test");
    }

    #[test]
    fn history_merges_reloads_and_searches_words() {
        let mut h = History::default();
        assert!(h.add(visit(100, "https://a.com/", "")));
        assert!(h.add(visit(110, "https://a.com/", "Site A")));
        assert_eq!(h.items.len(), 1);
        assert_eq!(h.items[0].title, "Site A");
        assert!(!h.add(visit(120, "about:blank", "")));
        h.add(visit(4000, "https://a.com/", "Site A"));
        assert_eq!(h.items.len(), 2, "after half an hour the same page is a new visit");
        h.add(visit(4100, "https://www.google.com/search?q=frieren", ""));
        assert!(h.set_title("https://www.google.com/search?q=frieren", "frieren - Google Search"));
        assert!(!h.set_title("https://www.google.com/search?q=frieren", "frieren - Google Search"));
        let found = h.search("FRIEREN google", 10);
        assert_eq!(found.len(), 1);
        assert_eq!(h.search("", 10)[0].time, 4100, "newest first");
        assert!(h.remove(4000, "https://a.com/"));
        assert_eq!(h.items.len(), 2);
        h.clear_since(4050);
        assert_eq!(h.items.len(), 1);
        h.clear_since(0);
        assert!(h.items.is_empty());
    }

    #[test]
    fn history_is_capped() {
        let mut h = History::default();
        for i in 0..(MAX_HISTORY + 5) {
            h.add(visit(i as u64 * 2000, &format!("https://x/{}", i), ""));
        }
        assert_eq!(h.items.len(), MAX_HISTORY);
        assert_eq!(h.items[0].url, "https://x/5");
    }

    #[test]
    fn downloads_save_only_on_state_changes() {
        let mut d = Downloads::default();
        let mut a = DlItem { id: "1".into(), url: "https://h/f.zip".into(), path: "C:\\Down\\f.zip".into(), received: 0, total: 100, state: "progress".into(), time: 5 };
        assert!(d.update(a.clone()));
        a.received = 50;
        assert!(!d.update(a.clone()), "progress alone is not saved");
        assert_eq!(d.items[0].received, 50);
        a.state = "done".into();
        a.received = 100;
        assert!(d.update(a.clone()));
        assert_eq!(d.items[0].name(), "f.zip");
        d.update(DlItem { id: "2".into(), url: "https://h/dir/g.pdf?x=1".into(), state: "progress".into(), ..Default::default() });
        assert_eq!(d.items[1].name(), "g.pdf");
        d.clear_finished();
        assert_eq!(d.items.len(), 1);
        // a running download read back after a restart is interrupted
        let back = Downloads::from_json(&d.to_json());
        assert_eq!(back.items[0].state, "interrupted");
        assert!(d.remove("2"));
        assert!(d.items.is_empty());
    }

    #[test]
    fn stores_round_trip_on_disk() {
        let dir = tmp("disk");
        let mut b = Browser::new(dir.clone());
        b.ensure_loaded();
        assert_eq!(b.prefs.engine, "google");
        b.history.add(visit(1, "https://a/", "A \"quoted\" title"));
        b.downloads.update(DlItem { id: "x".into(), state: "done".into(), path: "/tmp/a.bin".into(), ..Default::default() });
        b.prefs.engine = "bing".into();
        assert!(b.prefs.pin("https://a/", "A", true));
        assert!(!b.prefs.pin("about:blank", "", true));
        b.save_history();
        b.save_downloads();
        b.save_prefs();
        let mut c = Browser::new(dir.clone());
        c.ensure_loaded();
        assert_eq!(c.history.items, b.history.items);
        assert_eq!(c.downloads.items, b.downloads.items);
        assert_eq!(c.prefs, b.prefs);
        assert!(!c.prefs.pin("https://a/", "A", false));
        assert!(c.prefs.pins.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
