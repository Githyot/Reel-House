//! Client for the Stremio addon protocol (catalog, meta, stream, subtitles).
use crate::http::{get_json, Http};
use crate::json::Json;
use crate::model::{MetaItem, Row};
use crate::settings::AddonEntry;
use crate::streams::Stream;
use crate::util::url_encode;

/// Items plus per-addon errors, so the UI can say which addon failed.
#[derive(Debug, Default)]
pub struct Fetched<T> {
    pub items: Vec<T>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SubCand {
    pub lang: String,
    pub url: String,
    pub addon: String,
}

#[derive(Clone, Debug)]
pub struct CatalogRef {
    pub addon_url: String,
    pub addon_name: String,
    pub kind: String,
    pub id: String,
    pub name: String,
    pub searchable: bool,
    pub needs_input: bool,
}

pub fn normalize_manifest_url(input: &str) -> Result<String, String> {
    let mut u = input.trim().to_string();
    if let Some(rest) = u.strip_prefix("stremio://") {
        u = format!("https://{}", rest);
    }
    if !(u.starts_with("https://") || u.starts_with("http://")) {
        return Err("Enter the addon's manifest link. It starts with https:// or stremio://.".to_string());
    }
    if let Some(i) = u.find('#') {
        u.truncate(i);
    }
    let (path, query) = match u.find('?') {
        Some(i) => (u[..i].to_string(), u[i..].to_string()),
        None => (u.clone(), String::new()),
    };
    let mut p = path.trim_end_matches('/').to_string();
    // the addon's settings page was pasted (".../configure"): its manifest sits one level up
    if let Some(up) = p.strip_suffix("/configure") {
        p = up.to_string();
    }
    if !p.ends_with("/manifest.json") {
        p.push_str("/manifest.json");
    }
    Ok(format!("{}{}", p, query))
}

pub fn base_of(url: &str) -> String {
    let u = url.split('?').next().unwrap_or(url);
    u.strip_suffix("/manifest.json").unwrap_or(u).trim_end_matches('/').to_string()
}

pub fn resource_url(addon_url: &str, resource: &str, kind: &str, id: &str, extra: &[(&str, &str)]) -> String {
    let mut s = format!("{}/{}/{}/{}", base_of(addon_url), resource, url_encode(kind), url_encode(id));
    if !extra.is_empty() {
        s.push('/');
        s.push_str(&extra.iter().map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v))).collect::<Vec<_>>().join("&"));
    }
    s.push_str(".json");
    // a manifest link with settings in its query (`manifest.json?token=…`): every request carries them too, as
    // Stremio does (it swaps only the `/manifest.json` part of the link)
    if let Some(i) = addon_url.find('?') {
        let q = addon_url[i..].split('#').next().unwrap_or("");
        if q.len() > 1 {
            s.push_str(q);
        }
    }
    s
}

pub fn supports(m: &Json, resource: &str, kind: &str, id: &str) -> bool {
    let top_types = m.get("types").strings();
    let top_prefixes = m.get("idPrefixes").strings();
    m.get("resources").items().iter().any(|r| {
        let (name, types, prefixes) = match r {
            Json::Str(s) => (s.clone(), top_types.clone(), top_prefixes.clone()),
            obj => {
                let t = obj.get("types").strings();
                let p = obj.get("idPrefixes").strings();
                (obj.get("name").str(), if t.is_empty() { top_types.clone() } else { t }, if p.is_empty() { top_prefixes.clone() } else { p })
            }
        };
        name == resource
            && (types.is_empty() || types.iter().any(|t| t == kind))
            && (prefixes.is_empty() || id.is_empty() || prefixes.iter().any(|p| id.starts_with(p.as_str())))
    })
}

pub fn validate_manifest(m: &Json) -> Result<(), String> {
    if !matches!(m, Json::Obj(_)) {
        return Err("That link did not return an addon manifest.".to_string());
    }
    for key in ["id", "name", "version"] {
        if m.get(key).str().is_empty() {
            return Err(format!("The manifest is missing \"{}\".", key));
        }
    }
    if m.get("resources").items().is_empty() {
        return Err("The manifest does not list any resources.".to_string());
    }
    Ok(())
}

/// True when the manifest asks to be configured first (`behaviorHints.configurationRequired`) and the
/// link has no settings in its path (`https://host/manifest.json`). Such a link answers the manifest
/// but every catalog, stream or subtitle request fails until the user makes a personal link on the
/// addon's own page. A configured link carries the settings in its path, so it is never caught here.
pub fn needs_setup(url: &str, m: &Json) -> bool {
    if m.get("behaviorHints").get("configurationRequired").as_bool() != Some(true) {
        return false;
    }
    let base = base_of(url);
    let after_scheme = base.split("://").nth(1).unwrap_or(&base);
    // settings may also ride in the query (`manifest.json?config=…`)
    let query = url.split_once('?').map(|(_, q)| q.split('#').next().unwrap_or("")).unwrap_or("");
    !after_scheme.contains('/') && query.is_empty()
}

pub const NEEDS_SETUP: &str = "This addon has to be set up on its own page first. Open its link in a browser with /configure in place of /manifest.json, pick the options (some ask for a free API key) and paste here the manifest link that page gives you.";

pub fn fetch_manifest(http: &dyn Http, url: &str) -> Result<Json, String> {
    let m = get_json(http, url, 20)?;
    validate_manifest(&m)?;
    Ok(m)
}

/// Adds an addon, or replaces one with the same manifest id.
pub fn install(http: &dyn Http, entries: &mut Vec<AddonEntry>, input: &str) -> Result<String, String> {
    let mut url = normalize_manifest_url(input)?;
    let manifest = match fetch_manifest(http, &url) {
        Ok(m) => m,
        Err(e) => {
            // a link to a page of the site, not to the manifest: the manifest at the site's root, when there is one
            let root = url.split('/').take(3).collect::<Vec<_>>().join("/") + "/manifest.json";
            let tried = url.split('?').next().unwrap_or(&url).to_string();
            if root == tried {
                return Err(format!("{} ({})", e, tried));
            }
            match fetch_manifest(http, &root) {
                Ok(m) => {
                    url = root;
                    m
                }
                Err(_) => return Err(format!("{} ({}). Copy the addon's install link (it ends in manifest.json) from its page.", e, tried)),
            }
        }
    };
    if needs_setup(&url, &manifest) {
        return Err(NEEDS_SETUP.to_string());
    }
    let id = manifest.get("id").str();
    let name = manifest.get("name").str();
    entries.retain(|e| e.url != url && e.manifest.get("id").str() != id);
    entries.push(AddonEntry { url, enabled: true, manifest });
    Ok(name)
}

/// Downloads manifests for entries that do not have one yet. Returns the errors per addon.
pub fn refresh_missing(http: &dyn Http, entries: &mut [AddonEntry]) -> Vec<String> {
    let mut errors = Vec::new();
    for e in entries.iter_mut().filter(|e| !e.has_manifest()) {
        match fetch_manifest(http, &e.url) {
            Ok(m) => e.manifest = m,
            Err(err) => errors.push(format!("{}: {}", e.url.split('/').nth(2).unwrap_or(&e.url), err)),
        }
    }
    errors
}

pub fn list_catalogs(entries: &[AddonEntry]) -> Vec<CatalogRef> {
    let mut out = Vec::new();
    for e in entries.iter().filter(|e| e.enabled && e.has_manifest() && !e.needs_setup()) {
        for c in e.manifest.get("catalogs").items() {
            let kind = c.get("type").str();
            if !supports(&e.manifest, "catalog", &kind, "") {
                continue;
            }
            let extra = c.get("extra").items();
            let searchable = extra.iter().any(|x| x.get("name").str() == "search")
                || c.get("extraSupported").strings().iter().any(|x| x == "search");
            let needs_input = extra.iter().any(|x| x.get("isRequired").as_bool() == Some(true) && x.get("name").str() != "skip")
                || c.get("extraRequired").strings().iter().any(|x| x != "skip");
            let id = c.get("id").str();
            let mut name = c.get("name").str();
            if name.is_empty() {
                name = id.clone();
            }
            out.push(CatalogRef { addon_url: e.url.clone(), addon_name: e.name(), kind, id, name, searchable, needs_input });
        }
    }
    out
}

pub fn board_catalogs(entries: &[AddonEntry]) -> Vec<CatalogRef> {
    list_catalogs(entries).into_iter().filter(|c| !c.needs_input).collect()
}

pub fn search_catalogs(entries: &[AddonEntry]) -> Vec<CatalogRef> {
    list_catalogs(entries).into_iter().filter(|c| c.searchable).collect()
}

pub fn fetch_catalog(http: &dyn Http, cat: &CatalogRef, extra: &[(&str, &str)]) -> Result<Row, String> {
    let url = resource_url(&cat.addon_url, "catalog", &cat.kind, &cat.id, extra);
    let j = get_json(http, &url, 25)?;
    let items: Vec<MetaItem> = j
        .get("metas")
        .items()
        .iter()
        .map(MetaItem::from_json)
        .filter(|m| !m.id.is_empty())
        .map(|mut m| {
            if m.kind.is_empty() {
                m.kind = cat.kind.clone();
            }
            m
        })
        .take(60)
        .collect();
    Ok(Row { title: cat.name.clone(), addon: cat.addon_name.clone(), kind: cat.kind.clone(), items })
}

pub fn fetch_meta(http: &dyn Http, entries: &[AddonEntry], kind: &str, id: &str) -> Option<MetaItem> {
    for e in entries.iter().filter(|e| e.enabled && e.has_manifest() && !e.needs_setup() && supports(&e.manifest, "meta", kind, id)) {
        let url = resource_url(&e.url, "meta", kind, id, &[]);
        if let Ok(j) = get_json(http, &url, 20) {
            let m = j.get("meta");
            if !m.is_null() {
                let mut item = MetaItem::from_json(m);
                if item.name.is_empty() {
                    continue;
                }
                if item.kind.is_empty() {
                    item.kind = kind.to_string();
                }
                if item.id.is_empty() {
                    item.id = id.to_string();
                }
                return Some(item);
            }
        }
    }
    None
}

fn fan_out<T: Send>(
    http: &dyn Http,
    entries: &[AddonEntry],
    resource: &str,
    kind: &str,
    id: &str,
    timeout: u64,
    extra: &[(&str, &str)],
    parse: &(dyn Fn(&Json, &str) -> Vec<T> + Sync),
) -> Fetched<T> {
    let targets: Vec<&AddonEntry> =
        entries.iter().filter(|e| e.enabled && e.has_manifest() && !e.needs_setup() && supports(&e.manifest, resource, kind, id)).collect();
    let mut out = Fetched { items: Vec::new(), errors: Vec::new() };
    std::thread::scope(|sc| {
        let handles: Vec<_> = targets
            .iter()
            .map(|e| {
                sc.spawn(move || {
                    let url = resource_url(&e.url, resource, kind, id, extra);
                    get_json(http, &url, timeout).map(|j| parse(&j, &e.name())).map_err(|err| format!("{}: {}", e.name(), err))
                })
            })
            .collect();
        for h in handles {
            match h.join() {
                Ok(Ok(v)) => out.items.extend(v),
                Ok(Err(e)) => out.errors.push(e),
                Err(_) => out.errors.push("an addon request crashed".to_string()),
            }
        }
    });
    out
}

pub fn fetch_streams(http: &dyn Http, entries: &[AddonEntry], kind: &str, id: &str) -> Fetched<Stream> {
    fan_out(http, entries, "stream", kind, id, 35, &[], &|j, addon| {
        j.get("streams").items().iter().map(|s| Stream::from_json(s, addon)).filter(|s| s.is_playable()).collect()
    })
}

/// Some addons answer with placeholders ("about:blank", a language like "MAL=FAILED"). Those are not subtitles.
fn real_sub(s: &SubCand) -> bool {
    let u = s.url.trim();
    (u.starts_with("http://") || u.starts_with("https://") || u.starts_with("//")) && !u.contains("about:blank") && lang_ok(&s.lang)
}

fn lang_ok(l: &str) -> bool {
    !l.is_empty() && l.len() <= 12 && l.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '_') && !l.eq_ignore_ascii_case("failed")
}

fn parse_subs(j: &Json, addon: &str) -> Vec<SubCand> {
    j.get("subtitles")
        .items()
        .iter()
        .map(|s| SubCand { lang: s.get("lang").str(), url: s.get("url").str(), addon: addon.to_string() })
        .filter(real_sub)
        .collect()
}

pub const OPENSUBTITLES: &str = "https://opensubtitles-v3.strem.io";

pub fn fetch_subtitles(http: &dyn Http, entries: &[AddonEntry], kind: &str, id: &str) -> Fetched<SubCand> {
    let mut found = fan_out(http, entries, "subtitles", kind, id, 25, &[], &parse_subs);
    // Stremio ships with the OpenSubtitles addon. Ask it too when it is not installed, so there are
    // as many languages and versions to choose from as in Stremio.
    let has_os = entries.iter().any(|e| e.enabled && e.url.contains("opensubtitles"));
    if !has_os && id.starts_with("tt") {
        let url = format!("{}/subtitles/{}/{}.json", OPENSUBTITLES, kind, url_encode(id));
        match get_json(http, &url, 25) {
            Ok(j) => found.items.extend(parse_subs(&j, "OpenSubtitles")),
            Err(e) => found.errors.push(format!("OpenSubtitles: {}", e)),
        }
    }
    let mut seen: Vec<String> = Vec::new();
    found.items.retain(|s| {
        let new = !seen.contains(&s.url);
        seen.push(s.url.clone());
        new
    });
    found
}

/// Subtitles that match given release names (file names such as "[Group] Show - 05 (1080p).mkv"): subtitle
/// addons rank their answers by the `filename` they are given. Answers for the first names come first.
pub fn fetch_subtitles_for(http: &dyn Http, entries: &[AddonEntry], kind: &str, id: &str, filenames: &[String]) -> Fetched<SubCand> {
    let mut all = Fetched { items: Vec::new(), errors: Vec::new() };
    for f in filenames.iter().filter(|f| !f.trim().is_empty()).take(4) {
        let mut part = fan_out(http, entries, "subtitles", kind, id, 25, &[("filename", f.as_str())], &parse_subs);
        let has_os = entries.iter().any(|e| e.enabled && e.url.contains("opensubtitles"));
        if !has_os && id.starts_with("tt") {
            let url = resource_url(OPENSUBTITLES, "subtitles", kind, id, &[("filename", f.as_str())]);
            if let Ok(j) = get_json(http, &url, 25) {
                part.items.extend(parse_subs(&j, "OpenSubtitles"));
            }
        }
        all.items.extend(part.items);
        all.errors.extend(part.errors);
    }
    let plain = fetch_subtitles(http, entries, kind, id);
    all.items.extend(plain.items);
    all.errors.extend(plain.errors);
    let mut seen: Vec<String> = Vec::new();
    all.items.retain(|s| {
        let new = !seen.contains(&s.url);
        seen.push(s.url.clone());
        new
    });
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    fn manifest(extra: &str) -> String {
        format!(
            r#"{{"id":"x.a","name":"X","version":"1.0.0","types":["movie","series"],"idPrefixes":["tt"],
            "resources":["catalog","stream",{{"name":"meta","types":["series"],"idPrefixes":["mal:"]}},"subtitles"],
            "catalogs":[{{"type":"movie","id":"top","name":"Popular","extra":[{{"name":"search"}},{{"name":"skip"}}]}},
            {{"type":"movie","id":"find","name":"Find","extra":[{{"name":"search","isRequired":true}}]}},
            {{"type":"series","id":"year","extra":[{{"name":"genre","isRequired":true}}]}}{}]}}"#,
            extra
        )
    }

    fn entry(url: &str, m: &str) -> AddonEntry {
        AddonEntry { url: url.to_string(), enabled: true, manifest: Json::parse(m).unwrap() }
    }

    #[test]
    fn configure_page_link_points_at_the_manifest() {
        assert_eq!(normalize_manifest_url("https://a.b/x1/configure").unwrap(), "https://a.b/x1/manifest.json");
        assert_eq!(normalize_manifest_url("https://a.b/configure/").unwrap(), "https://a.b/manifest.json");
    }

    #[test]
    fn normalises_manifest_links() {
        assert_eq!(normalize_manifest_url("stremio://h.com/a/manifest.json").unwrap(), "https://h.com/a/manifest.json");
        assert_eq!(normalize_manifest_url("https://h.com/cfg/").unwrap(), "https://h.com/cfg/manifest.json");
        assert_eq!(normalize_manifest_url("https://h.com").unwrap(), "https://h.com/manifest.json");
        assert_eq!(normalize_manifest_url(" https://h.com/m.json?x=1#frag ").unwrap(), "https://h.com/m.json/manifest.json?x=1");
        assert!(normalize_manifest_url("nope").is_err());
    }

    #[test]
    fn builds_resource_urls() {
        let a = "https://h.com/cfg/manifest.json";
        assert_eq!(resource_url(a, "stream", "series", "tt1:1:2", &[]), "https://h.com/cfg/stream/series/tt1:1:2.json");
        assert_eq!(resource_url(a, "catalog", "movie", "top", &[("search", "a b&c")]), "https://h.com/cfg/catalog/movie/top/search=a%20b%26c.json");
        assert_eq!(resource_url("https://h.com/manifest.json?key=1", "stream", "movie", "tt1", &[]), "https://h.com/stream/movie/tt1.json?key=1");
    }

    #[test]
    fn checks_resource_support() {
        let m = Json::parse(&manifest("")).unwrap();
        assert!(supports(&m, "stream", "movie", "tt1"));
        assert!(!supports(&m, "stream", "movie", "mal:1"));
        assert!(supports(&m, "meta", "series", "mal:5"));
        assert!(!supports(&m, "meta", "movie", "mal:5"));
        assert!(!supports(&m, "addon_catalog", "movie", ""));
    }

    #[test]
    fn splits_board_and_search_catalogs() {
        let e = [entry("https://h.com/manifest.json", &manifest(""))];
        let board: Vec<_> = board_catalogs(&e).iter().map(|c| c.id.clone()).collect();
        let search: Vec<_> = search_catalogs(&e).iter().map(|c| c.id.clone()).collect();
        assert_eq!(board, vec!["top"]);
        assert_eq!(search, vec!["top", "find"]);
    }

    #[test]
    fn installs_validates_and_replaces() {
        let http = Fake::new().route("GET https://h.com/manifest.json", 200, &manifest("")).route("GET https://bad.com/", 200, "{}");
        let mut entries = vec![AddonEntry::new("https://old.com/manifest.json")];
        assert_eq!(install(&http, &mut entries, "stremio://h.com/manifest.json").unwrap(), "X");
        assert_eq!(install(&http, &mut entries, "https://h.com").unwrap(), "X");
        assert_eq!(entries.len(), 2);
        assert!(install(&http, &mut entries, "https://bad.com/manifest.json").unwrap_err().contains("missing"));
        let errs = refresh_missing(&http, &mut entries);
        assert_eq!(errs.len(), 1);
        assert!(entries[0].has_manifest() == false && entries[1].has_manifest());
    }

    #[test]
    fn bare_link_of_an_addon_that_needs_setup_is_refused() {
        let m = Json::parse(r#"{"id":"x","name":"X","version":"1","resources":["subtitles"],"behaviorHints":{"configurable":true,"configurationRequired":true}}"#).unwrap();
        assert!(needs_setup("https://subs.example/manifest.json", &m));
        assert!(!needs_setup("https://subs.example/eyJrZXkiOiJhYmMifQ/manifest.json", &m));
        let free = Json::parse(r#"{"id":"x","name":"X","version":"1","resources":["subtitles"],"behaviorHints":{"configurable":true}}"#).unwrap();
        assert!(!needs_setup("https://subs.example/manifest.json", &free));
        let http = Fake::new().route("/manifest.json", 200, r#"{"id":"x","name":"X","version":"1","resources":["subtitles"],"behaviorHints":{"configurationRequired":true}}"#);
        let mut e = Vec::new();
        assert_eq!(install(&http, &mut e, "https://subs.example/manifest.json"), Err(NEEDS_SETUP.to_string()));
        assert!(e.is_empty());
        let old = vec![AddonEntry { url: "https://subs.example/manifest.json".into(), enabled: true, manifest: m }];
        let _ = fetch_subtitles(&http, &old, "movie", "tt1");
        assert!(http.requests().iter().all(|r| !r.url.contains("subs.example/subtitles")));
    }

    #[test]
    fn fetches_catalog_meta_streams_and_subtitles() {
        let e = [entry("https://h.com/manifest.json", &manifest(""))];
        let http = Fake::new()
            .route("/catalog/movie/top.json", 200, r#"{"metas":[{"id":"tt1","name":"One"},{"name":"no id"}]}"#)
            .route("/meta/series/mal:5.json", 200, r#"{"meta":{"id":"mal:5","name":"Anime","videos":[{"id":"mal:5:1","episode":1}]}}"#)
            .route("/stream/movie/tt1.json", 200, r#"{"streams":[{"name":"1080p","infoHash":"abc"},{"name":"junk"}]}"#)
            .route("/subtitles/movie/tt1.json", 200, r#"{"subtitles":[{"lang":"eng","url":"http://s/1.srt"},{"lang":"por"}]}"#);
        let row = fetch_catalog(&http, &board_catalogs(&e)[0], &[]).unwrap();
        assert_eq!((row.items.len(), row.items[0].kind.as_str()), (1, "movie"));
        assert_eq!(fetch_meta(&http, &e, "series", "mal:5").unwrap().name, "Anime");
        assert!(fetch_meta(&http, &e, "movie", "tt1").is_none());
        let s = fetch_streams(&http, &e, "movie", "tt1");
        assert_eq!((s.items.len(), s.errors.len()), (1, 0));
        assert_eq!(s.items[0].addon, "X");
        let sub = fetch_subtitles(&http, &e, "movie", "tt1");
        assert_eq!(sub.items.len(), 1);
    }

    #[test]
    fn reports_which_addon_failed() {
        let e = [entry("https://h.com/manifest.json", &manifest(""))];
        let http = Fake::new().route("/stream/", 500, "boom");
        let s = fetch_streams(&http, &e, "movie", "tt1");
        assert!(s.items.is_empty());
        assert!(s.errors[0].starts_with("X: h.com answered 500"));
    }
}
