//! Suwayomi-Server: the engine that runs extensions in the Mihon / Tachiyomi format, from repositories the user adds.
//! The app talks to it over GraphQL; the server does the scraping, the app only draws.
use crate::http::{Http, Request};
use crate::json::Json;
use crate::manga::{Chapter, Series};

pub const PREFIX: &str = "suwayomi";
pub const DEFAULT_PORT: u16 = 4567;
pub const DEFAULT_URL: &str = "http://127.0.0.1:4567";

#[derive(Clone, Debug, Default)]
pub struct Server {
    pub base: String,
    pub user: String,
    pub pass: String,
}

impl Server {
    pub fn new(base: &str, user: &str, pass: &str) -> Server {
        let b = base.trim().trim_end_matches('/');
        let b = if b.is_empty() { DEFAULT_URL.to_string() } else if b.contains("://") { b.to_string() } else { format!("http://{}", b) };
        Server { base: b, user: user.to_string(), pass: pass.to_string() }
    }

    /// A path of the server ("/api/v1/manga/3/thumbnail") as a full address.
    pub fn url(&self, path: &str) -> String {
        if path.starts_with("http") {
            path.to_string()
        } else if path.is_empty() {
            String::new()
        } else {
            format!("{}{}", self.base, if path.starts_with('/') { path.to_string() } else { format!("/{}", path) })
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub lang: String,
    /// Address of the icon (served by the server); empty when there is none.
    pub icon: String,
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// One GraphQL call. The answer's `data` is returned.
pub fn gql(http: &dyn Http, srv: &Server, query: &str) -> Result<Json, String> {
    let body = Json::obj(vec![("query", query.into())]).dump();
    let mut req = Request::post(&format!("{}/api/graphql", srv.base)).header("Accept", "application/json").json_body(body).timeout(60);
    if !srv.user.is_empty() {
        req = req.header("Authorization", &format!("Basic {}", base64(format!("{}:{}", srv.user, srv.pass).as_bytes())));
    }
    let r = http.send(&req).map_err(|e| format!("Could not reach Suwayomi at {} ({})", srv.base, e))?;
    if r.status == 401 {
        return Err("Suwayomi asked for a user name and password.".to_string());
    }
    let j = r.json().map_err(|_| format!("{} did not answer like Suwayomi (status {})", srv.base, r.status))?;
    let errs = j.get("errors").items();
    if let Some(e) = errs.first() {
        return Err(e.get("message").str());
    }
    Ok(j.get("data").clone())
}

fn quote(s: &str) -> String {
    Json::Str(s.to_string()).dump()
}

/// A query with a number that the server may want as text or as a number: tries both.
fn gql_id(http: &dyn Http, srv: &Server, make: impl Fn(&str) -> String, id: &str) -> Result<Json, String> {
    match gql(http, srv, &make(&quote(id))) {
        Ok(j) => Ok(j),
        Err(e) if e.contains("LongString") || e.contains("Long") || e.contains("coerc") || e.contains("literal") || e.contains("Validation") => gql(http, srv, &make(id)),
        Err(e) => Err(e),
    }
}

pub fn sources(http: &dyn Http, srv: &Server) -> Result<Vec<Source>, String> {
    let j = gql(http, srv, "query { sources { nodes { id name lang iconUrl } } }").or_else(|_| gql(http, srv, "query { sources { nodes { id name lang } } }"))?;
    let mut out: Vec<Source> = j
        .path("sources.nodes")
        .items()
        .iter()
        .map(|s| {
            let icon = s.get("iconUrl").str();
            Source { id: s.get("id").str(), name: s.get("name").str(), lang: s.get("lang").str(), icon: if icon.is_empty() { String::new() } else if icon.starts_with("http") { icon } else { srv.url(&icon) } }
        })
        .filter(|s| !s.id.is_empty() && s.id != "0")
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.lang.cmp(&b.lang)));
    Ok(out)
}

/// Turns what the user typed into an address the server accepts: a link to the `index.min.json` of a repository
/// (raw.githubusercontent.com/…), or a GitHub page of a repository (`github.com/user/repo`, with or without
/// `/tree/branch`; without a branch, the branch `repo` that the Mihon repositories use is assumed).
pub fn normalize_repo(text: &str) -> Result<String, String> {
    let t = text.trim().trim_end_matches('/').to_string();
    if t.is_empty() {
        return Err("Type the address of the repository.".to_string());
    }
    let t = if t.starts_with("http://") || t.starts_with("https://") { t } else { format!("https://{}", t) };
    let Some(rest) = t.strip_prefix("https://github.com/").or_else(|| t.strip_prefix("http://github.com/")) else {
        if t.split('/').nth(2).map(|h| h.contains('.')).unwrap_or(false) {
            return Ok(t);
        }
        return Err("That does not look like an address.".to_string());
    };
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    match parts.len() {
        0 | 1 => Err("A GitHub repository looks like github.com/user/repository.".to_string()),
        2 => Ok(format!("https://github.com/{}/{}/tree/repo", parts[0], parts[1].trim_end_matches(".git"))),
        _ => Ok(t),
    }
}

/// A short name for the list: "user/repo" for GitHub addresses, else the host.
pub fn repo_label(url: &str) -> String {
    let rest = url.trim_start_matches("https://").trim_start_matches("http://");
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.first() {
        Some(&"github.com") | Some(&"raw.githubusercontent.com") if parts.len() >= 3 => format!("{}/{}", parts[1], parts[2]),
        Some(h) => h.to_string(),
        None => url.to_string(),
    }
}

/// Gives the server the repositories (replacing its list).
pub fn set_repos(http: &dyn Http, srv: &Server, repos: &[String]) -> Result<(), String> {
    let list = repos.iter().map(|r| quote(r)).collect::<Vec<_>>().join(", ");
    gql(http, srv, &format!("mutation {{ setSettings(input: {{ settings: {{ extensionRepos: [{}] }} }}) {{ settings {{ extensionRepos }} }} }}", list)).map(|_| ())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extension {
    pub pkg: String,
    pub name: String,
    pub lang: String,
    pub installed: bool,
    pub update: bool,
    /// For adults (18+): what the server says, or a name that gives it away.
    pub nsfw: bool,
    /// Address of the icon (served by the server); empty when there is none.
    pub icon: String,
}

/// What the server's repositories offer (the server reads them now).
pub fn extensions(http: &dyn Http, srv: &Server) -> Result<Vec<Extension>, String> {
    // With the icon and the 18+ mark; a server that does not know those fields gets the short question.
    let j = gql(http, srv, "mutation { fetchExtensions(input: {}) { extensions { pkgName name lang isInstalled hasUpdate isNsfw iconUrl } } }").or_else(|_| gql(http, srv, "mutation { fetchExtensions(input: {}) { extensions { pkgName name lang isInstalled hasUpdate } } }"))?;
    Ok(j.path("fetchExtensions.extensions")
        .items()
        .iter()
        .map(|e| {
            let name = e.get("name").str();
            let icon = e.get("iconUrl").str();
            Extension {
                pkg: e.get("pkgName").str(),
                lang: e.get("lang").str(),
                installed: e.get("isInstalled").as_bool().unwrap_or(false),
                update: e.get("hasUpdate").as_bool().unwrap_or(false),
                nsfw: e.get("isNsfw").as_bool().unwrap_or(false) || looks_adult(&name),
                icon: if icon.is_empty() { String::new() } else if icon.starts_with("http") { icon } else { srv.url(&icon) },
                name,
            }
        })
        .filter(|e| !e.pkg.is_empty())
        .collect())
}

/// Names that give an adult source away, for servers that do not mark them.
pub fn looks_adult(name: &str) -> bool {
    let n = name.to_lowercase();
    ["hentai", "porn", "xxx", "nsfw", "18+", "r34", "doujin", "lewd", "ecchi"].iter().any(|w| n.contains(w))
}

pub fn install(http: &dyn Http, srv: &Server, pkg: &str) -> Result<(), String> {
    gql(http, srv, &format!("mutation {{ updateExtension(input: {{ id: {}, patch: {{ install: true }} }}) {{ extension {{ pkgName isInstalled }} }} }}", quote(pkg))).map(|_| ())
}

pub fn uninstall(http: &dyn Http, srv: &Server, pkg: &str) -> Result<(), String> {
    gql(http, srv, &format!("mutation {{ updateExtension(input: {{ id: {}, patch: {{ uninstall: true }} }}) {{ extension {{ pkgName isInstalled }} }} }}", quote(pkg))).map(|_| ())
}

// ----- series -----

const MANGA_FIELDS: &str = "id title thumbnailUrl description author artist genre status";

pub fn series_id(id: &str) -> String {
    format!("{}:{}", PREFIX, id)
}

/// The number inside "suwayomi:12".
pub fn raw(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}

pub fn is_suwayomi(id: &str) -> bool {
    id.starts_with("suwayomi:")
}

fn status_text(s: &str) -> String {
    match s {
        "ONGOING" => "Ongoing",
        "COMPLETED" | "PUBLISHING_FINISHED" => "Completed",
        "ON_HIATUS" => "On hiatus",
        "CANCELLED" => "Cancelled",
        "LICENSED" => "Licensed",
        _ => "",
    }
    .to_string()
}

pub fn parse_manga(srv: &Server, m: &Json) -> Option<Series> {
    let id = m.get("id").str();
    let title = m.get("title").str();
    if id.is_empty() || title.is_empty() {
        return None;
    }
    let mut authors = Vec::new();
    for k in ["author", "artist"] {
        let a = m.get(k).str();
        if !a.is_empty() && !authors.contains(&a) {
            authors.push(a);
        }
    }
    Some(Series {
        id: series_id(&id),
        title,
        cover: srv.url(&m.get("thumbnailUrl").str()),
        description: m.get("description").str(),
        status: status_text(&m.get("status").str()),
        year: None,
        tags: m.get("genre").strings(),
        authors,
        mal_id: None,
        categories: vec![],
        chapter_count: 0,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Popular,
    Latest,
    Search,
}

/// A page (1 based) of what a source lists.
pub fn browse(http: &dyn Http, srv: &Server, source: &str, kind: Kind, page: usize, query: &str) -> Result<(Vec<Series>, bool), String> {
    let t = match kind {
        Kind::Popular => "POPULAR",
        Kind::Latest => "LATEST",
        Kind::Search => "SEARCH",
    };
    let q = if kind == Kind::Search { format!(", query: {}", quote(query)) } else { String::new() };
    let j = gql_id(
        http,
        srv,
        |s| format!("mutation {{ fetchSourceManga(input: {{ source: {}, type: {}, page: {}{} }}) {{ hasNextPage mangas {{ {} }} }} }}", s, t, page, q, MANGA_FIELDS),
        source,
    )?;
    let p = j.get("fetchSourceManga");
    Ok((p.get("mangas").items().iter().filter_map(|m| parse_manga(srv, m)).collect(), p.get("hasNextPage").as_bool().unwrap_or(false)))
}

/// Details of a series, read from its source.
pub fn series(http: &dyn Http, srv: &Server, id: &str) -> Result<Series, String> {
    let j = gql(http, srv, &format!("mutation {{ fetchManga(input: {{ id: {} }}) {{ manga {{ {} }} }} }}", raw(id), MANGA_FIELDS))?;
    parse_manga(srv, j.path("fetchManga.manga")).ok_or_else(|| "The server did not know this title.".to_string())
}

/// "2024-05-17" from a time in milliseconds.
pub fn date_of(ms: &str) -> String {
    let Ok(ms) = ms.trim().parse::<i64>() else { return String::new() };
    if ms <= 0 {
        return String::new();
    }
    let days = ms / 86_400_000;
    // Civil date from days since 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn parse_chapter(c: &Json) -> Option<Chapter> {
    let id = c.get("id").str();
    if id.is_empty() {
        return None;
    }
    let n = c.get("chapterNumber").as_f64().filter(|n| *n >= 0.0);
    Some(Chapter {
        id: series_id(&id),
        number: n.map(crate::json::fmt_num).unwrap_or_default(),
        volume: String::new(),
        title: {
            // Names are often "Chapter 12": the number is shown by the label already.
            let name = c.get("name").str();
            let low = name.to_lowercase();
            if n.is_some() && (low.starts_with("chapter") || low.starts_with("ch.") || low.starts_with("cap")) && name.chars().filter(|c| c.is_alphabetic()).count() < 12 { String::new() } else { name }
        },
        lang: String::new(),
        pages: c.get("pageCount").as_u64().unwrap_or(0) as u32,
        published: date_of(&c.get("uploadDate").str()),
        group: c.get("scanlator").str(),
    })
}

/// The chapters of a series, oldest first.
pub fn chapters(http: &dyn Http, srv: &Server, manga: &str) -> Result<Vec<Chapter>, String> {
    let j = gql(http, srv, &format!("mutation {{ fetchChapters(input: {{ mangaId: {} }}) {{ chapters {{ id name chapterNumber uploadDate scanlator sourceOrder pageCount }} }} }}", raw(manga)))?;
    let mut list: Vec<(i64, Chapter)> = j.path("fetchChapters.chapters").items().iter().filter_map(|c| parse_chapter(c).map(|ch| (c.get("sourceOrder").as_i64().unwrap_or(0), ch))).collect();
    // The server numbers them from the newest (0) to the oldest.
    list.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(list.into_iter().map(|(_, c)| c).collect())
}

/// The server of the app listens on a new port at every start: an address of its pictures kept from an earlier run
/// (in the cache of the readers) is pointed at the current server. Other addresses stay as they are.
pub fn rebase(srv: &Server, url: &str) -> String {
    for pre in ["http://127.0.0.1:", "http://localhost:"] {
        if let Some(rest) = url.strip_prefix(pre) {
            if let Some(i) = rest.find('/') {
                if rest[i..].starts_with("/api/") {
                    return srv.url(&rest[i..]);
                }
            }
        }
    }
    url.to_string()
}

/// The addresses of the pictures of a chapter.
pub fn pages(http: &dyn Http, srv: &Server, chapter: &str) -> Result<Vec<String>, String> {
    let j = gql(http, srv, &format!("mutation {{ fetchChapterPages(input: {{ chapterId: {} }}) {{ pages }} }}", raw(chapter)))?;
    let p: Vec<String> = j.path("fetchChapterPages.pages").strings().iter().map(|p| srv.url(p)).collect();
    if p.is_empty() {
        Err("This chapter has no pictures.".to_string())
    } else {
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn repository_addresses() {
        assert_eq!(normalize_repo("github.com/example-user/extensions").unwrap(), "https://github.com/example-user/extensions/tree/repo");
        assert_eq!(normalize_repo(" https://github.com/a/b/tree/main/ ").unwrap(), "https://github.com/a/b/tree/main");
        assert_eq!(normalize_repo("https://raw.githubusercontent.com/example-user/extensions/repo/index.min.json").unwrap(), "https://raw.githubusercontent.com/example-user/extensions/repo/index.min.json");
        assert!(normalize_repo("").is_err());
        assert!(normalize_repo("https://github.com/onlyuser").is_err());
        assert!(normalize_repo("nonsense").is_err());
        assert_eq!(repo_label("https://github.com/example-user/extensions/tree/repo"), "example-user/extensions");
        assert_eq!(repo_label("https://example.org/x/index.json"), "example.org");
    }

    fn srv() -> Server {
        Server::new("localhost:4567/", "", "")
    }

    #[test]
    fn address_and_auth() {
        assert_eq!(srv().base, "http://localhost:4567");
        assert_eq!(srv().url("/api/v1/manga/3/thumbnail"), "http://localhost:4567/api/v1/manga/3/thumbnail");
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn dates() {
        assert_eq!(date_of("1715904000000"), "2024-05-17");
        assert_eq!(date_of("0"), "");
        assert_eq!(date_of("x"), "");
    }

    #[test]
    fn lists_sources() {
        let h = Fake::new().route("POST http://localhost:4567/api/graphql", 200, r#"{"data":{"sources":{"nodes":[{"id":"22","name":"Zed","lang":"en"},{"id":"0","name":"Local source","lang":"localsourcelang"},{"id":"7","name":"Alpha","lang":"pt"}]}}}"#);
        let s = sources(&h, &srv()).unwrap();
        assert_eq!(s.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), vec!["Alpha", "Zed"]);
    }

    #[test]
    fn browse_and_chapters() {
        let h = Fake::new().route(
            "graphql",
            200,
            r#"{"data":{"fetchSourceManga":{"hasNextPage":true,"mangas":[{"id":5,"title":"Berserk","thumbnailUrl":"/api/v1/manga/5/thumbnail","description":"d","author":"Miura","artist":"Miura","genre":["Dark"],"status":"ONGOING"}]}}}"#,
        );
        let (list, more) = browse(&h, &srv(), "123", Kind::Popular, 1, "").unwrap();
        assert!(more);
        assert_eq!(list[0].id, "suwayomi:5");
        assert_eq!(list[0].cover, "http://localhost:4567/api/v1/manga/5/thumbnail");
        assert_eq!(list[0].authors, vec!["Miura"]);
        assert_eq!(list[0].status, "Ongoing");
        let sent = String::from_utf8(h.requests()[0].body.clone().unwrap().1).unwrap();
        assert!(sent.contains("POPULAR"));

        let h = Fake::new().route(
            "graphql",
            200,
            r#"{"data":{"fetchChapters":{"chapters":[{"id":12,"name":"Chapter 2","chapterNumber":2,"uploadDate":"1715904000000","scanlator":"G","sourceOrder":0,"pageCount":0},{"id":11,"name":"Chapter 1: Start of it all","chapterNumber":1,"uploadDate":"0","scanlator":"","sourceOrder":1,"pageCount":0}]}}}"#,
        );
        let c = chapters(&h, &srv(), "suwayomi:5").unwrap();
        assert_eq!(c[0].id, "suwayomi:11");
        assert_eq!(c[0].number, "1");
        assert_eq!(c[0].title, "Chapter 1: Start of it all");
        assert_eq!(c[1].title, "");
        assert_eq!(c[1].published, "2024-05-17");
    }

    #[test]
    fn errors_come_through() {
        let h = Fake::new().route("graphql", 200, r#"{"errors":[{"message":"boom"}],"data":null}"#);
        assert_eq!(sources(&h, &srv()).unwrap_err(), "boom");
        let h = Fake::new().route("graphql", 401, "no");
        assert!(sources(&h, &srv()).unwrap_err().contains("password"));
    }

    #[test]
    fn pages_get_the_server_address() {
        let h = Fake::new().route("graphql", 200, r#"{"data":{"fetchChapterPages":{"pages":["/api/v1/manga/5/chapter/1/page/0","/api/v1/manga/5/chapter/1/page/1"]}}}"#);
        let p = pages(&h, &srv(), "suwayomi:11").unwrap();
        assert_eq!(p[1], "http://localhost:4567/api/v1/manga/5/chapter/1/page/1");
    }
}
