//! Cookies a site gave the in-app browser after its bot check (Cloudflare "Just a moment…", a 403 to plain requests).
//! The text sources (LNReader plugins) send them, with the browser's own User-Agent, on every request to that site:
//! the check's cookie (cf_clearance) is only good with the User-Agent that passed it (HP: AllNovel answered 403).
use crate::json::Json;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pass {
    pub cookie: String,
    pub ua: String,
}

static PASSES: Mutex<Option<HashMap<String, Pass>>> = Mutex::new(None);

fn file() -> std::path::PathBuf {
    crate::util::data_dir().join("site-passes.json")
}

/// "https://www.allnovel.org/x" -> "allnovel.org" (a pass is kept per site, with or without "www.").
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("").to_ascii_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_string()
}

fn with<R>(f: impl FnOnce(&mut HashMap<String, Pass>) -> R) -> R {
    let mut g = PASSES.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        let mut m = HashMap::new();
        if let Some(j) = std::fs::read_to_string(file()).ok().and_then(|t| Json::parse(&t).ok()) {
            for (k, v) in j.entries() {
                m.insert(k.clone(), Pass { cookie: v.get("cookie").str(), ua: v.get("ua").str() });
            }
        }
        *g = Some(m);
    }
    f(g.as_mut().unwrap())
}

/// The pass for the site of `url`, when the browser got one.
pub fn get(url: &str) -> Option<Pass> {
    let h = host_of(url);
    if h.is_empty() {
        return None;
    }
    with(|m| {
        // the site itself, or a site it belongs to ("api.site.com" uses the pass of "site.com")
        let mut cur = h.as_str();
        loop {
            if let Some(p) = m.get(cur) {
                return Some(p.clone());
            }
            match cur.split_once('.') {
                Some((_, rest)) if rest.contains('.') => cur = rest,
                _ => return None,
            }
        }
    })
}

/// Keeps what the browser has for the site of `url` (an empty cookie forgets it) and saves it.
pub fn set(url: &str, cookie: &str, ua: &str) {
    if let Some(json) = remember(url, cookie, ua) {
        let _ = std::fs::write(file(), json.dump());
    }
}

/// `set` without saving: the passes as they are now, to save.
fn remember(url: &str, cookie: &str, ua: &str) -> Option<Json> {
    let h = host_of(url);
    if h.is_empty() {
        return None;
    }
    Some(with(|m| {
        if cookie.trim().is_empty() {
            m.remove(&h);
        } else {
            m.insert(h, Pass { cookie: cookie.trim().to_string(), ua: ua.trim().to_string() });
        }
        Json::obj(m.iter().map(|(k, p)| (k.as_str(), Json::obj(vec![("cookie", p.cookie.as_str().into()), ("ua", p.ua.as_str().into())]))).collect())
    }))
}

/// The request headers with the pass of the site added: its cookies joined to the plugin's own, its User-Agent.
pub fn apply(url: &str, headers: &mut Vec<(String, String)>) {
    let Some(p) = get(url) else { return };
    if !p.ua.is_empty() {
        headers.retain(|(k, _)| !k.eq_ignore_ascii_case("user-agent"));
        headers.push(("User-Agent".into(), p.ua));
    }
    if let Some(c) = headers.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case("cookie")) {
        c.1 = format!("{}; {}", c.1.trim_end_matches(';'), p.cookie);
    } else {
        headers.push(("Cookie".into(), p.cookie));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_headers() {
        assert_eq!(host_of("https://www.AllNovel.org/novel/x?y=1"), "allnovel.org");
        assert_eq!(host_of("http://user@site.com:8080/a"), "site.com");
        remember("https://www.allnovel.org/", "cf_clearance=abc", "Mozilla/5.0 (Linux; Android 13) Chrome/120");
        let mut h = vec![("User-Agent".to_string(), "Mozilla/5.0 (Windows)".to_string()), ("Cookie".to_string(), "a=1".to_string())];
        apply("https://cdn.allnovel.org/c/1", &mut h);
        assert!(h.contains(&("User-Agent".to_string(), "Mozilla/5.0 (Linux; Android 13) Chrome/120".to_string())));
        assert!(h.contains(&("Cookie".to_string(), "a=1; cf_clearance=abc".to_string())));
        let mut other = vec![];
        apply("https://novelfull.com/", &mut other);
        assert!(other.is_empty());
        remember("https://allnovel.org/", "", "");
        assert!(get("https://allnovel.org/").is_none());
    }
}
