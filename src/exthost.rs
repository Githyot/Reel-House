//! The extension host of the Android app (docs/android-extensions.md): Mihon extensions loaded by the app itself,
//! reached over HTTP on 127.0.0.1. `suwayomi.rs` hands its calls here when its `Server` is this host, so the rest
//! of the Images mode does not care which one runs the extensions.
use crate::http::{Http, Request};
use crate::json::Json;
use crate::manga::{Chapter, Series};
use crate::suwayomi::{self, Extension, Kind, Server, Source};
use std::path::{Path, PathBuf};

/// What the host needs besides its address: the user's repositories and a folder for downloaded APKs.
#[derive(Clone, Debug, Default)]
pub struct Host {
    pub repos: Vec<String>,
    pub dir: PathBuf,
}

fn api(http: &dyn Http, srv: &Server, body: Json) -> Result<Json, String> {
    let r = http.send(&Request::post(&srv.url("/api")).json_body(body.dump()).timeout(90))?;
    if !r.ok() {
        return Err(format!("The extension host answered {}.", r.status));
    }
    let j = r.json()?;
    match j.get("error").str() {
        e if !e.is_empty() => Err(e),
        _ => Ok(j),
    }
}

fn op(name: &str, mut fields: Vec<(&str, Json)>) -> Json {
    fields.insert(0, ("op", name.into()));
    Json::obj(fields)
}

// ----- ids: "suwayomi:x<base64url of source \n url>", so the library keeps working with the ids it already uses -----

fn b64url(data: &[u8]) -> String {
    suwayomi::base64(data).replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()
}

fn unb64url(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u32> = s.bytes().map(val).collect::<Option<_>>()?;
    let mut out = Vec::new();
    for c in bytes.chunks(4) {
        let n = c.iter().enumerate().fold(0u32, |n, (i, v)| n | v << (18 - 6 * i));
        out.push((n >> 16) as u8);
        if c.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if c.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

pub fn make_id(source: &str, url: &str) -> String {
    suwayomi::series_id(&format!("x{}", b64url(format!("{}\n{}", source, url).as_bytes())))
}

/// (source, url) of an id made by `make_id`.
pub fn split_id(id: &str) -> Option<(String, String)> {
    let raw = suwayomi::raw(id).strip_prefix('x')?;
    let text = String::from_utf8(unb64url(raw)?).ok()?;
    let (s, u) = text.split_once('\n')?;
    Some((s.to_string(), u.to_string()))
}

// ----- repositories -----

/// The folder of a repository that holds `index.min.json`, `apk/` and `icon/`.
pub fn repo_base(repo: &str) -> String {
    let r = repo.trim().trim_end_matches('/');
    if let Some(dir) = r.strip_suffix("index.min.json").or_else(|| r.strip_suffix("index.json")).or_else(|| r.strip_suffix("index.pb")) {
        return dir.to_string();
    }
    // github.com/user/repo/tree/branch (the form the repository list keeps)
    if let Some(rest) = r.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 4 && parts[2] == "tree" {
            return format!("https://raw.githubusercontent.com/{}/{}/{}/", parts[0], parts[1], parts[3..].join("/"));
        }
        if parts.len() == 2 {
            return format!("https://raw.githubusercontent.com/{}/{}/repo/", parts[0], parts[1]);
        }
    }
    format!("{}/", r)
}

struct Offered {
    base: String,
    entry: Json,
}

fn offered(http: &dyn Http, repos: &[String]) -> (Vec<Offered>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for repo in repos {
        let base = repo_base(repo);
        match crate::http::get_json(http, &format!("{}index.min.json", base), 30) {
            Ok(j) => out.extend(j.items().iter().map(|e| Offered { base: base.clone(), entry: e.clone() })),
            Err(e) => errors.push(format!("{}: {}", suwayomi::repo_label(repo), e)),
        }
    }
    (out, errors)
}

// ----- what suwayomi.rs hands over -----

fn source_of(j: &Json) -> Source {
    Source { id: j.get("id").str(), name: j.get("name").str(), lang: j.get("lang").str(), icon: String::new() }
}

pub fn sources(http: &dyn Http, srv: &Server) -> Result<Vec<Source>, String> {
    let j = api(http, srv, op("sources", vec![]))?;
    let mut out: Vec<Source> = j.get("sources").items().iter().map(source_of).filter(|s| !s.id.is_empty()).collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.lang.cmp(&b.lang)));
    Ok(out)
}

pub fn extensions(http: &dyn Http, srv: &Server, host: &Host) -> Result<Vec<Extension>, String> {
    let installed = api(http, srv, op("installed", vec![]))?;
    let have: Vec<(String, i64)> = installed.get("extensions").items().iter().map(|e| (e.get("pkg").str(), e.get("code").as_i64().unwrap_or(0))).collect();
    let (list, errors) = offered(http, &host.repos);
    if list.is_empty() && !errors.is_empty() {
        return Err(errors.join(" · "));
    }
    let mut out: Vec<Extension> = Vec::new();
    for o in list {
        let e = &o.entry;
        let pkg = e.get("pkg").str();
        if pkg.is_empty() || out.iter().any(|x| x.pkg == pkg) {
            continue;
        }
        let name = e.get("name").str().trim_start_matches("Tachiyomi: ").to_string();
        let mine = have.iter().find(|(p, _)| *p == pkg);
        out.push(Extension {
            lang: e.get("lang").str(),
            installed: mine.is_some(),
            update: mine.map(|(_, c)| e.get("code").as_i64().unwrap_or(0) > *c).unwrap_or(false),
            nsfw: e.get("nsfw").as_i64().unwrap_or(0) == 1 || e.get("nsfw").as_bool().unwrap_or(false) || suwayomi::looks_adult(&name),
            icon: format!("{}icon/{}.png", o.base, pkg),
            pkg,
            name,
        });
    }
    // installed ones whose repository is gone still show, so they can be removed
    for e in installed.get("extensions").items() {
        let pkg = e.get("pkg").str();
        if !pkg.is_empty() && !out.iter().any(|x| x.pkg == pkg) {
            out.push(Extension { name: e.get("name").str(), lang: e.get("lang").str(), installed: true, nsfw: e.get("nsfw").as_bool().unwrap_or(false), pkg, ..Default::default() });
        }
    }
    Ok(out)
}

pub fn install(http: &dyn Http, srv: &Server, host: &Host, pkg: &str) -> Result<(), String> {
    let (list, errors) = offered(http, &host.repos);
    let o = list.iter().find(|o| o.entry.get("pkg").str() == pkg).ok_or_else(|| if errors.is_empty() { format!("No repository offers {}.", pkg) } else { errors.join(" · ") })?;
    let apk = o.entry.get("apk").str();
    if apk.is_empty() || apk.contains('/') || apk.contains("..") {
        return Err("The repository gave no valid file for this extension.".to_string());
    }
    let r = http.send(&Request::get(&format!("{}apk/{}", o.base, apk)).timeout(180))?;
    if !r.ok() || r.body.is_empty() {
        return Err(format!("Could not download {} ({}).", apk, r.status));
    }
    let path = save(&host.dir.join("downloads"), &apk, &r.body)?;
    let res = api(http, srv, op("install", vec![("apk", path.to_string_lossy().to_string().into())]));
    let _ = std::fs::remove_file(&path);
    res.map(|_| ())
}

fn save(dir: &Path, name: &str, data: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let p = dir.join(name);
    std::fs::write(&p, data).map_err(|e| format!("Could not save {}: {}", name, e))?;
    Ok(p)
}

pub fn uninstall(http: &dyn Http, srv: &Server, pkg: &str) -> Result<(), String> {
    api(http, srv, op("uninstall", vec![("pkg", pkg.into())])).map(|_| ())
}

fn status_text(s: i64) -> String {
    // SManga: 1 ongoing, 2 completed, 3 licensed, 4 publishing finished, 5 cancelled, 6 on hiatus
    match s {
        1 => "Ongoing",
        2 | 4 => "Completed",
        3 => "Licensed",
        5 => "Cancelled",
        6 => "On hiatus",
        _ => "",
    }
    .to_string()
}

fn split_list(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

pub fn parse_manga(srv: &Server, source: &str, m: &Json) -> Option<Series> {
    let (url, title) = (m.get("url").str(), m.get("title").str());
    if url.is_empty() || title.is_empty() {
        return None;
    }
    let mut authors: Vec<String> = Vec::new();
    for a in split_list(&m.get("author").str()).into_iter().chain(split_list(&m.get("artist").str())) {
        if !authors.contains(&a) {
            authors.push(a);
        }
    }
    Some(Series {
        id: make_id(source, &url),
        title,
        cover: srv.url(&m.get("thumbnail").str()),
        description: m.get("description").str(),
        status: status_text(m.get("status").as_i64().unwrap_or(0)),
        year: None,
        tags: split_list(&m.get("genre").str()),
        authors,
        mal_id: None,
        categories: vec![],
        chapter_count: 0,
    })
}

pub fn browse(http: &dyn Http, srv: &Server, source: &str, kind: Kind, page: usize, query: &str) -> Result<(Vec<Series>, bool), String> {
    let name = match kind {
        Kind::Popular => "popular",
        Kind::Latest => "latest",
        Kind::Search => "search",
    };
    let j = api(http, srv, op(name, vec![("source", source.into()), ("page", (page as i64).into()), ("query", query.into())]))?;
    Ok((j.get("mangas").items().iter().filter_map(|m| parse_manga(srv, source, m)).collect(), j.get("hasNext").as_bool().unwrap_or(false)))
}

pub fn series(http: &dyn Http, srv: &Server, id: &str) -> Result<Series, String> {
    let (source, url) = split_id(id).ok_or_else(|| "This title does not belong to an extension.".to_string())?;
    let j = api(http, srv, op("details", vec![("source", source.as_str().into()), ("url", url.as_str().into())]))?;
    let mut fields = j.clone();
    if fields.get("url").str().is_empty() {
        fields = Json::obj(vec![("url", url.as_str().into()), ("title", j.get("title").clone()), ("thumbnail", j.get("thumbnail").clone()), ("author", j.get("author").clone()), ("artist", j.get("artist").clone()), ("description", j.get("description").clone()), ("genre", j.get("genre").clone()), ("status", j.get("status").clone())]);
    }
    parse_manga(srv, &source, &fields).ok_or_else(|| "The extension did not know this title.".to_string())
}

pub fn chapters(http: &dyn Http, srv: &Server, id: &str) -> Result<Vec<Chapter>, String> {
    let (source, url) = split_id(id).ok_or_else(|| "This title does not belong to an extension.".to_string())?;
    let j = api(http, srv, op("chapters", vec![("source", source.as_str().into()), ("url", url.as_str().into())]))?;
    // extensions list the newest first
    let mut list: Vec<Chapter> = j
        .get("chapters")
        .items()
        .iter()
        .filter_map(|c| {
            let curl = c.get("url").str();
            if curl.is_empty() {
                return None;
            }
            let n = c.get("number").as_f64().filter(|n| *n >= 0.0);
            let name = c.get("name").str();
            let low = name.to_lowercase();
            let plain = n.is_some() && (low.starts_with("chapter") || low.starts_with("ch.") || low.starts_with("cap")) && name.chars().filter(|c| c.is_alphabetic()).count() < 12;
            Some(Chapter {
                id: make_id(&source, &curl),
                number: n.map(crate::json::fmt_num).unwrap_or_default(),
                volume: String::new(),
                title: if plain { String::new() } else { name },
                lang: String::new(),
                pages: 0,
                published: suwayomi::date_of(&c.get("date").as_i64().unwrap_or(0).to_string()),
                group: c.get("scanlator").str(),
            })
        })
        .collect();
    list.reverse();
    Ok(list)
}

pub fn pages(http: &dyn Http, srv: &Server, chapter: &str) -> Result<Vec<String>, String> {
    let (source, url) = split_id(chapter).ok_or_else(|| "This chapter does not belong to an extension.".to_string())?;
    let j = api(http, srv, op("pages", vec![("source", source.as_str().into()), ("url", url.as_str().into())]))?;
    let p: Vec<String> = j.get("pages").strings().iter().map(|p| srv.url(p)).collect();
    if p.is_empty() {
        Err("This chapter has no pictures.".to_string())
    } else {
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        let id = make_id("2499283573021220255", "/manga/one-piece?x=1&y=ç");
        assert!(suwayomi::is_suwayomi(&id));
        assert!(!suwayomi::raw(&id).contains(':'));
        assert_eq!(split_id(&id), Some(("2499283573021220255".to_string(), "/manga/one-piece?x=1&y=ç".to_string())));
        assert_eq!(split_id("suwayomi:12"), None);
    }

    #[test]
    fn repository_folders() {
        assert_eq!(repo_base("https://github.com/user/extensions/tree/repo"), "https://raw.githubusercontent.com/user/extensions/repo/");
        assert_eq!(repo_base("https://raw.githubusercontent.com/user/extensions/repo/index.min.json"), "https://raw.githubusercontent.com/user/extensions/repo/");
        assert_eq!(repo_base("https://example.org/repo/"), "https://example.org/repo/");
    }
}
