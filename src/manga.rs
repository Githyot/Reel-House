//! Manga and comics: the MangaDex source (its public API), and the library, reading progress and history kept on disk.
use crate::http::Http;
use crate::json::Json;
use std::collections::HashMap;
use std::path::Path;

const API: &str = "https://api.mangadex.org";
pub const SOURCE: &str = "mangadex";

/// A manga, comic or webtoon.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Series {
    /// "mangadex:<uuid>"
    pub id: String,
    pub title: String,
    pub cover: String,
    pub description: String,
    pub status: String,
    pub year: Option<i64>,
    pub tags: Vec<String>,
    pub authors: Vec<String>,
    /// The MyAnimeList id, when the source knows it: it is what the reading progress is sent to.
    pub mal_id: Option<u64>,
    /// Categories of the library this series is in (empty: only "All").
    pub categories: Vec<String>,
    /// Number of chapters seen at the last look, for the unread count.
    pub chapter_count: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chapter {
    pub id: String,
    /// "12", "12.5", or empty for one-shots
    pub number: String,
    pub volume: String,
    pub title: String,
    pub lang: String,
    pub pages: u32,
    pub published: String,
    pub group: String,
}

impl Chapter {
    pub fn sort_key(&self) -> f64 {
        self.number.trim().parse().unwrap_or(f64::MAX)
    }

    /// "Ch. 12 · Title", "Ch. 12", "Oneshot"
    pub fn label(&self) -> String {
        let head = if self.number.is_empty() { "Oneshot".to_string() } else { format!("Ch. {}", self.number) };
        if self.title.is_empty() { head } else { format!("{} · {}", head, self.title) }
    }
}

pub fn raw_id(series_id: &str) -> &str {
    series_id.split_once(':').map(|(_, r)| r).unwrap_or(series_id)
}

fn pick_text(map: &Json) -> String {
    let en = map.get("en").str();
    if !en.is_empty() {
        return en;
    }
    map.entries().first().map(|(_, v)| v.str()).unwrap_or_default()
}

/// A series from the API answer.
pub fn parse_series(d: &Json) -> Option<Series> {
    let id = d.get("id").str();
    if id.is_empty() {
        return None;
    }
    let a = d.get("attributes");
    let mut title = pick_text(a.get("title"));
    if title.is_empty() {
        title = a.get("altTitles").items().iter().map(pick_text).find(|t| !t.is_empty()).unwrap_or_default();
    }
    if title.is_empty() {
        return None;
    }
    let rels = d.get("relationships");
    let cover = rels
        .items()
        .iter()
        .find(|r| r.get("type").str() == "cover_art")
        .map(|r| r.path("attributes.fileName").str())
        .filter(|f| !f.is_empty())
        .map(|f| format!("https://uploads.mangadex.org/covers/{}/{}.512.jpg", id, f))
        .unwrap_or_default();
    let authors = rels
        .items()
        .iter()
        .filter(|r| matches!(r.get("type").str().as_str(), "author" | "artist"))
        .map(|r| r.path("attributes.name").str())
        .filter(|n| !n.is_empty())
        .fold(Vec::<String>::new(), |mut v, n| {
            if !v.contains(&n) {
                v.push(n);
            }
            v
        });
    Some(Series {
        id: format!("{}:{}", SOURCE, id),
        title,
        cover,
        description: pick_text(a.get("description")),
        status: a.get("status").str(),
        year: a.get("year").as_i64(),
        tags: a.get("tags").items().iter().map(|t| pick_text(t.path("attributes.name"))).filter(|t| !t.is_empty()).collect(),
        authors,
        mal_id: a.path("links.mal").str().parse().ok(),
        categories: vec![],
        chapter_count: 0,
    })
}

/// What a list of series asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Browse {
    Popular,
    Latest,
    Search(String),
}

pub const PAGE: usize = 24;

/// The languages the chapters can be read in: the code the sources use, and the name shown.
pub const LANGS: [(&str, &str); 30] = [
    ("en", "English"),
    ("pt-br", "Português (Brasil)"),
    ("pt", "Português (Portugal)"),
    ("es", "Español (España)"),
    ("es-la", "Español (Latinoamérica)"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("it", "Italiano"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("zh", "中文 (简体)"),
    ("zh-hk", "中文 (繁體)"),
    ("ru", "Русский"),
    ("id", "Bahasa Indonesia"),
    ("vi", "Tiếng Việt"),
    ("th", "ไทย"),
    ("tr", "Türkçe"),
    ("ar", "العربية"),
    ("pl", "Polski"),
    ("uk", "Українська"),
    ("nl", "Nederlands"),
    ("hi", "हिन्दी"),
    ("ro", "Română"),
    ("hu", "Magyar"),
    ("cs", "Čeština"),
    ("bg", "Български"),
    ("fil", "Filipino"),
    ("ms", "Bahasa Melayu"),
    ("he", "עברית"),
    ("all", "All languages"),
];

/// The code a source reports ("pt-BR", "pt_br") in the form of `LANGS`.
pub fn lang_code(l: &str) -> String {
    l.trim().to_lowercase().replace('_', "-")
}

fn rating_params() -> &'static str {
    "&contentRating%5B%5D=safe&contentRating%5B%5D=suggestive"
}

pub fn list_url(what: &Browse, offset: usize, lang: &str) -> String {
    let mut u = format!("{}/manga?limit={}&offset={}&includes%5B%5D=cover_art&includes%5B%5D=author&includes%5B%5D=artist{}", API, PAGE, offset, rating_params());
    if !lang.is_empty() && lang != "all" {
        u.push_str(&format!("&availableTranslatedLanguage%5B%5D={}", crate::util::url_encode(&lang_code(lang))));
    }
    match what {
        Browse::Popular => u.push_str("&order%5BfollowedCount%5D=desc"),
        Browse::Latest => u.push_str("&order%5BlatestUploadedChapter%5D=desc&hasAvailableChapters=true"),
        Browse::Search(q) => u.push_str(&format!("&title={}&order%5Brelevance%5D=desc", crate::util::url_encode(q.trim()))),
    }
    u
}

/// A page of series, and whether more follow.
pub fn browse(http: &dyn Http, what: &Browse, offset: usize, lang: &str) -> Result<(Vec<Series>, bool), String> {
    let j = crate::http::get_json(http, &list_url(what, offset, lang), 25)?;
    if j.get("result").str() == "error" {
        return Err(j.path("errors.0.detail").str());
    }
    let items: Vec<Series> = j.get("data").items().iter().filter_map(parse_series).collect();
    let total = j.get("total").as_u64().unwrap_or(0) as usize;
    Ok((items, offset + PAGE < total))
}

/// One series with its details.
pub fn series(http: &dyn Http, id: &str) -> Result<Series, String> {
    let url = format!("{}/manga/{}?includes%5B%5D=cover_art&includes%5B%5D=author&includes%5B%5D=artist", API, raw_id(id));
    let j = crate::http::get_json(http, &url, 25)?;
    parse_series(j.get("data")).ok_or_else(|| "The source did not know this title.".to_string())
}

pub fn parse_chapter(d: &Json) -> Option<Chapter> {
    let id = d.get("id").str();
    let a = d.get("attributes");
    // Chapters that live on another site cannot be read here.
    if id.is_empty() || !a.get("externalUrl").str().is_empty() {
        return None;
    }
    let group = d.get("relationships").items().iter().find(|r| r.get("type").str() == "scanlation_group").map(|r| r.path("attributes.name").str()).unwrap_or_default();
    Some(Chapter {
        id,
        number: a.get("chapter").str(),
        volume: a.get("volume").str(),
        title: a.get("title").str(),
        lang: a.get("translatedLanguage").str(),
        pages: a.get("pages").as_u64().unwrap_or(0) as u32,
        published: a.get("publishAt").str().chars().take(10).collect(),
        group,
    })
}

/// One chapter per number, in reading order: where several groups translated the same chapter, the longest one is kept.
pub fn dedupe(mut list: Vec<Chapter>) -> Vec<Chapter> {
    let mut best: HashMap<String, Chapter> = HashMap::new();
    let mut loose = Vec::new();
    for c in list.drain(..) {
        if c.number.is_empty() {
            loose.push(c);
            continue;
        }
        let keep = best.get(&c.number).map(|o| c.pages > o.pages).unwrap_or(true);
        if keep {
            best.insert(c.number.clone(), c);
        }
    }
    let mut out: Vec<Chapter> = best.into_values().collect();
    out.sort_by(|a, b| a.sort_key().partial_cmp(&b.sort_key()).unwrap_or(std::cmp::Ordering::Equal).then(a.number.cmp(&b.number)));
    out.extend(loose);
    out
}

/// All the chapters of a series in a language, oldest first.
pub fn chapters(http: &dyn Http, id: &str, lang: &str) -> Result<Vec<Chapter>, String> {
    let mut all = Vec::new();
    let mut offset = 0usize;
    loop {
        let lq = match lang_code(lang).as_str() {
            "all" => String::new(),
            "" => "&translatedLanguage%5B%5D=en".to_string(),
            l => format!("&translatedLanguage%5B%5D={}", crate::util::url_encode(l)),
        };
        let base = format!("{}/manga/{}/feed?offset={}{}&order%5Bchapter%5D=asc", API, raw_id(id), offset, lq);
        // The richest request first; if the server dislikes a parameter, ask again with fewer.
        let tries = [
            format!("{}&limit=500&includes%5B%5D=scanlation_group&includeFuturePublishAt=0{}", base, rating_params()),
            format!("{}&limit=100&includes%5B%5D=scanlation_group{}", base, rating_params()),
            // The last resort: no ordering, no extras (the list is sorted here anyway).
            format!("{}/manga/{}/feed?offset={}&limit=100{}", API, raw_id(id), offset, lq),
        ];
        let mut res = Err(String::new());
        for t in &tries {
            res = crate::http::get_json(http, t, 30);
            if res.is_ok() {
                break;
            }
        }
        let j = res?;
        let page: Vec<Chapter> = j.get("data").items().iter().filter_map(parse_chapter).collect();
        let got = j.get("data").items().len();
        all.extend(page);
        let total = j.get("total").as_u64().unwrap_or(0) as usize;
        offset += got;
        if got == 0 || offset >= total || offset >= 5000 {
            break;
        }
    }
    Ok(dedupe(all))
}

/// The addresses of the pictures of a chapter, in order.
pub fn pages(http: &dyn Http, chapter_id: &str, data_saver: bool) -> Result<Vec<String>, String> {
    let j = crate::http::get_json(http, &format!("{}/at-home/server/{}", API, chapter_id), 25)?;
    let base = j.get("baseUrl").str();
    let hash = j.path("chapter.hash").str();
    if base.is_empty() || hash.is_empty() {
        return Err("The source did not give the pictures of this chapter.".to_string());
    }
    let (folder, key) = if data_saver { ("data-saver", "dataSaver") } else { ("data", "data") };
    let mut files = j.path(&format!("chapter.{}", key)).strings();
    if files.is_empty() {
        files = j.path("chapter.data").strings();
    }
    let folder = if files.is_empty() { "data" } else { folder };
    if files.is_empty() {
        return Err("This chapter has no pictures.".to_string());
    }
    Ok(files.iter().map(|f| format!("{}/{}/{}/{}", base, folder, hash, f)).collect())
}

// ----- what is kept on disk -----

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    /// Chapters read (ids).
    pub read: Vec<String>,
    /// The chapter being read and the page (0 based).
    pub chapter: String,
    pub page: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryItem {
    pub series_id: String,
    pub title: String,
    pub cover: String,
    pub chapter_id: String,
    pub label: String,
    pub at: u64,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub library: Vec<Series>,
    /// Names of the library categories, in order.
    pub categories: Vec<String>,
    pub progress: HashMap<String, Progress>,
    pub history: Vec<HistoryItem>,
}

pub fn series_json(s: &Series) -> Json {
    Json::obj(vec![
        ("id", s.id.as_str().into()),
        ("title", s.title.as_str().into()),
        ("cover", s.cover.as_str().into()),
        ("description", s.description.as_str().into()),
        ("status", s.status.as_str().into()),
        ("year", s.year.map(|y| y as f64).unwrap_or(0.0).into()),
        ("tags", Json::Arr(s.tags.iter().map(|t| t.as_str().into()).collect())),
        ("authors", Json::Arr(s.authors.iter().map(|t| t.as_str().into()).collect())),
        ("mal_id", (s.mal_id.unwrap_or(0) as f64).into()),
        ("categories", Json::Arr(s.categories.iter().map(|t| t.as_str().into()).collect())),
        ("chapter_count", (s.chapter_count as f64).into()),
    ])
}

pub fn series_from(j: &Json) -> Option<Series> {
    let id = j.get("id").str();
    if id.is_empty() {
        return None;
    }
    Some(Series {
        id,
        title: j.get("title").str(),
        cover: j.get("cover").str(),
        description: j.get("description").str(),
        status: j.get("status").str(),
        year: j.get("year").as_i64().filter(|y| *y > 0),
        tags: j.get("tags").strings(),
        authors: j.get("authors").strings(),
        mal_id: j.get("mal_id").as_u64().filter(|m| *m > 0),
        categories: j.get("categories").strings(),
        chapter_count: j.get("chapter_count").as_u64().unwrap_or(0) as u32,
    })
}

/// A chapter as kept in the cache of the readers (`readcache`).
pub fn chapter_to_json(c: &Chapter) -> Json {
    Json::obj(vec![
        ("id", c.id.as_str().into()),
        ("number", c.number.as_str().into()),
        ("volume", c.volume.as_str().into()),
        ("title", c.title.as_str().into()),
        ("lang", c.lang.as_str().into()),
        ("pages", (c.pages as f64).into()),
        ("published", c.published.as_str().into()),
        ("group", c.group.as_str().into()),
    ])
}

pub fn chapter_from_json(j: &Json) -> Option<Chapter> {
    let id = j.get("id").str();
    if id.is_empty() {
        return None;
    }
    Some(Chapter {
        id,
        number: j.get("number").str(),
        volume: j.get("volume").str(),
        title: j.get("title").str(),
        lang: j.get("lang").str(),
        pages: j.get("pages").as_u64().unwrap_or(0) as u32,
        published: j.get("published").str(),
        group: j.get("group").str(),
    })
}

/// A page of a list of series, with "there is more", for the cache.
pub fn list_to_json(items: &[Series], more: bool) -> Json {
    Json::obj(vec![("items", Json::Arr(items.iter().map(series_json).collect())), ("more", more.into())])
}

pub fn list_from_json(j: &Json) -> Option<(Vec<Series>, bool)> {
    let items: Vec<Series> = j.get("items").items().iter().filter_map(series_from).collect();
    if items.is_empty() {
        return None;
    }
    Some((items, j.get("more").as_bool().unwrap_or(false)))
}

/// A series with its chapters, for the cache.
pub fn view_to_json(s: &Series, chapters: &[Chapter]) -> Json {
    Json::obj(vec![("series", series_json(s)), ("chapters", Json::Arr(chapters.iter().map(chapter_to_json).collect()))])
}

pub fn view_from_json(j: &Json) -> Option<(Series, Vec<Chapter>)> {
    let s = series_from(j.get("series"))?;
    Some((s, j.get("chapters").items().iter().filter_map(chapter_from_json).collect()))
}

impl State {
    pub fn to_json(&self) -> Json {
        let progress: Vec<(&str, Json)> = self
            .progress
            .iter()
            .map(|(k, p)| (k.as_str(), Json::obj(vec![("read", Json::Arr(p.read.iter().map(|r| r.as_str().into()).collect())), ("chapter", p.chapter.as_str().into()), ("page", (p.page as f64).into())])))
            .collect();
        Json::obj(vec![
            ("library", Json::Arr(self.library.iter().map(series_json).collect())),
            ("categories", Json::Arr(self.categories.iter().map(|c| c.as_str().into()).collect())),
            ("progress", Json::obj(progress)),
            (
                "history",
                Json::Arr(
                    self.history
                        .iter()
                        .map(|h| {
                            Json::obj(vec![
                                ("series_id", h.series_id.as_str().into()),
                                ("title", h.title.as_str().into()),
                                ("cover", h.cover.as_str().into()),
                                ("chapter_id", h.chapter_id.as_str().into()),
                                ("label", h.label.as_str().into()),
                                ("at", (h.at as f64).into()),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    pub fn from_json(j: &Json) -> State {
        State {
            library: j.get("library").items().iter().filter_map(series_from).collect(),
            categories: j.get("categories").strings(),
            progress: j
                .get("progress")
                .entries()
                .iter()
                .map(|(k, p)| (k.clone(), Progress { read: p.get("read").strings(), chapter: p.get("chapter").str(), page: p.get("page").as_u64().unwrap_or(0) as u32 }))
                .collect(),
            history: j
                .get("history")
                .items()
                .iter()
                .map(|h| HistoryItem {
                    series_id: h.get("series_id").str(),
                    title: h.get("title").str(),
                    cover: h.get("cover").str(),
                    chapter_id: h.get("chapter_id").str(),
                    label: h.get("label").str(),
                    at: h.get("at").as_u64().unwrap_or(0),
                })
                .filter(|h| !h.series_id.is_empty())
                .collect(),
        }
    }

    pub fn load(dir: &Path) -> State {
        State::load_file(dir, "manga.json")
    }

    pub fn save(&self, dir: &Path) {
        self.save_file(dir, "manga.json");
    }

    /// The same shelf kept in another file (the Text mode uses "novels.json").
    pub fn load_file(dir: &Path, name: &str) -> State {
        std::fs::read_to_string(dir.join(name)).ok().and_then(|t| Json::parse(&t).ok()).map(|j| State::from_json(&j)).unwrap_or_default()
    }

    pub fn save_file(&self, dir: &Path, name: &str) {
        let _ = std::fs::write(dir.join(name), self.to_json().pretty());
    }

    pub fn in_library(&self, id: &str) -> bool {
        self.library.iter().any(|s| s.id == id)
    }

    pub fn is_read(&self, series: &str, chapter: &str) -> bool {
        self.progress.get(series).map(|p| p.read.iter().any(|c| c == chapter)).unwrap_or(false)
    }

    /// Chapters of the series that were not read (by the count at the last look).
    pub fn unread(&self, s: &Series) -> u32 {
        let read = self.progress.get(&s.id).map(|p| p.read.len() as u32).unwrap_or(0);
        s.chapter_count.saturating_sub(read)
    }

    /// Notes a chapter in the history: the newest first, one line per series.
    pub fn note_history(&mut self, item: HistoryItem) {
        self.history.retain(|h| h.series_id != item.series_id);
        self.history.insert(0, item);
        self.history.truncate(200);
    }
}

/// The chapter to open for "Continue": the one being read, else the first one not read, else the first.
pub fn next_to_read<'a>(chapters: &'a [Chapter], progress: Option<&Progress>) -> Option<&'a Chapter> {
    if let Some(p) = progress {
        if !p.chapter.is_empty() {
            if let Some(c) = chapters.iter().find(|c| c.id == p.chapter) {
                return Some(c);
            }
        }
        if let Some(c) = chapters.iter().find(|c| !p.read.contains(&c.id)) {
            return Some(c);
        }
    }
    chapters.first()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    const ONE: &str = r#"{"id":"abc","type":"manga","attributes":{"title":{"ja-ro":"Berserk"},"altTitles":[{"en":"Berserk EN"}],"description":{"en":"Guts."},"status":"ongoing","year":1989,"tags":[{"attributes":{"name":{"en":"Action"}}}],"links":{"mal":"2"}},"relationships":[{"type":"cover_art","attributes":{"fileName":"c.jpg"}},{"type":"author","attributes":{"name":"Miura"}},{"type":"artist","attributes":{"name":"Miura"}}]}"#;

    #[test]
    fn reads_a_series() {
        let s = parse_series(&Json::parse(ONE).unwrap()).unwrap();
        assert_eq!(s.id, "mangadex:abc");
        assert_eq!(s.title, "Berserk");
        assert_eq!(s.cover, "https://uploads.mangadex.org/covers/abc/c.jpg.512.jpg");
        assert_eq!(s.authors, vec!["Miura"]);
        assert_eq!(s.mal_id, Some(2));
        assert_eq!(s.tags, vec!["Action"]);
        assert_eq!(raw_id(&s.id), "abc");
    }

    #[test]
    fn urls() {
        let u = list_url(&Browse::Search("one piece".into()), 24, "en");
        assert!(u.contains("title=one%20piece") || u.contains("title=one+piece"));
        assert!(u.contains("offset=24"));
        assert!(list_url(&Browse::Popular, 0, "").contains("followedCount"));
    }

    #[test]
    fn dedupes_chapters() {
        let mk = |n: &str, pages: u32, id: &str| Chapter { id: id.into(), number: n.into(), pages, ..Default::default() };
        let v = dedupe(vec![mk("10", 5, "a"), mk("2", 20, "b"), mk("2", 22, "c"), mk("", 3, "d")]);
        assert_eq!(v.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec!["c", "a", "d"]);
        assert_eq!(v[0].label(), "Ch. 2");
        assert_eq!(v[2].label(), "Oneshot");
    }

    #[test]
    fn chapters_and_pages_from_the_api() {
        let http = Fake::new()
            .route("/manga/abc/feed", 200, r#"{"result":"ok","total":2,"data":[{"id":"c1","attributes":{"chapter":"1","pages":10,"translatedLanguage":"en","publishAt":"2020-01-02T00:00:00+00:00"},"relationships":[{"type":"scanlation_group","attributes":{"name":"G"}}]},{"id":"c2","attributes":{"chapter":"2","externalUrl":"https://x"}}]}"#)
            .route("/at-home/server/c1", 200, r#"{"baseUrl":"https://cdn","chapter":{"hash":"h","data":["1.png","2.png"],"dataSaver":["1.jpg"]}}"#);
        let c = chapters(&http, "mangadex:abc", "en").unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].group, "G");
        assert_eq!(c[0].published, "2020-01-02");
        assert_eq!(pages(&http, "c1", false).unwrap(), vec!["https://cdn/data/h/1.png", "https://cdn/data/h/2.png"]);
        assert_eq!(pages(&http, "c1", true).unwrap(), vec!["https://cdn/data-saver/h/1.jpg"]);
    }

    #[test]
    fn a_refused_request_is_asked_again_with_fewer_parameters() {
        let http = Fake::new()
            .route("/manga/abc/feed", 400, r#"{"result":"error","errors":[{"title":"Bad","detail":"Some parameter is not allowed"}]}"#)
            .route("/manga/abc/feed", 400, r#"{"result":"error","errors":[{"detail":"again"}]}"#)
            .route("/manga/abc/feed", 200, r#"{"result":"ok","total":1,"data":[{"id":"c1","attributes":{"chapter":"1","pages":3,"translatedLanguage":"pt-br"}}]}"#);
        let c = chapters(&http, "mangadex:abc", "pt_BR").unwrap();
        assert_eq!(c.len(), 1);
        let reqs = http.requests();
        assert_eq!(reqs.len(), 3);
        assert!(reqs[0].url.contains("translatedLanguage%5B%5D=pt-br"));
        assert!(!reqs[2].url.contains("contentRating") && !reqs[2].url.contains("order"));
        // The reason the server gave is part of the message when nothing works.
        let bad = Fake::new().route("/manga/abc/feed", 400, r#"{"errors":[{"detail":"nope"}]}"#);
        assert!(chapters(&bad, "mangadex:abc", "en").unwrap_err().contains("nope"));
    }

    #[test]
    fn every_language_means_no_language_filter() {
        let http = Fake::new().route("/manga/abc/feed", 200, r#"{"result":"ok","total":0,"data":[]}"#);
        chapters(&http, "mangadex:abc", "all").unwrap();
        assert!(!http.requests()[0].url.contains("translatedLanguage"));
        assert!(!list_url(&Browse::Popular, 0, "all").contains("availableTranslatedLanguage"));
        assert_eq!(lang_code("pt_BR"), "pt-br");
    }

    #[test]
    fn state_round_trip_and_unread() {
        let mut st = State::default();
        let mut s = parse_series(&Json::parse(ONE).unwrap()).unwrap();
        s.chapter_count = 5;
        s.categories = vec!["Reading".into()];
        st.library.push(s.clone());
        st.categories.push("Reading".into());
        st.progress.insert(s.id.clone(), Progress { read: vec!["c1".into(), "c2".into()], chapter: "c3".into(), page: 4 });
        st.note_history(HistoryItem { series_id: s.id.clone(), title: "Berserk".into(), chapter_id: "c3".into(), label: "Ch. 3".into(), at: 9, ..Default::default() });
        st.note_history(HistoryItem { series_id: s.id.clone(), title: "Berserk".into(), chapter_id: "c4".into(), label: "Ch. 4".into(), at: 10, ..Default::default() });
        let back = State::from_json(&Json::parse(&st.to_json().pretty()).unwrap());
        assert_eq!(back.library, st.library);
        assert_eq!(back.history.len(), 1);
        assert_eq!(back.progress[&s.id].page, 4);
        assert_eq!(back.unread(&s), 3);
        assert!(back.is_read(&s.id, "c1"));
    }

    #[test]
    fn picks_where_to_continue() {
        let mk = |id: &str| Chapter { id: id.into(), ..Default::default() };
        let list = vec![mk("a"), mk("b"), mk("c")];
        assert_eq!(next_to_read(&list, None).unwrap().id, "a");
        let p = Progress { read: vec!["a".into()], chapter: String::new(), page: 0 };
        assert_eq!(next_to_read(&list, Some(&p)).unwrap().id, "b");
        let p = Progress { read: vec!["a".into(), "b".into()], chapter: "b".into(), page: 3 };
        assert_eq!(next_to_read(&list, Some(&p)).unwrap().id, "b");
    }

    #[test]
    fn views_and_lists_survive_the_cache() {
        let s = Series { id: "mangadex:x".into(), title: "T".into(), tags: vec!["A".into()], chapter_count: 2, ..Default::default() };
        let ch = vec![Chapter { id: "c1".into(), number: "1".into(), title: "One".into(), pages: 20, published: "2024-01-02".into(), group: "G".into(), ..Default::default() }];
        let (s2, ch2) = view_from_json(&Json::parse(&view_to_json(&s, &ch).dump()).unwrap()).unwrap();
        assert_eq!((s2, ch2), (s.clone(), ch));
        let (items, more) = list_from_json(&Json::parse(&list_to_json(&[s.clone()], true).dump()).unwrap()).unwrap();
        assert_eq!((items, more), (vec![s], true));
        assert!(list_from_json(&list_to_json(&[], false)).is_none());
    }
}
