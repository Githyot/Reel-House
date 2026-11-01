//! MyAnimeList inside the app: titles, characters, staff, recommendations and reviews (through the
//! public Jikan API), the user's list (AniSync catalogs and the official MyAnimeList API with OAuth),
//! and the rules that turn the list into "watched" marks on episodes.
use crate::http::{get_json, Http, Request};
use crate::json::Json;
use crate::model::MetaItem;
use crate::util::{form_encode, url_encode};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- data

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Named {
    pub id: u64,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Related {
    pub id: u64,
    pub kind: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Relation {
    pub kind: String,
    pub entries: Vec<Related>,
}

#[derive(Clone, Debug, Default)]
pub struct Info {
    /// Where the page came from: empty for MyAnimeList (or AniList), "tmdb" for movies and series that MyAnimeList does not know.
    pub provider: String,
    pub id: u64,
    pub url: String,
    pub title: String,
    pub title_en: String,
    pub title_ja: String,
    pub synonyms: Vec<String>,
    pub image: String,
    pub trailer: String,
    pub kind: String,
    pub source: String,
    pub episodes: Option<u32>,
    pub status: String,
    pub aired: String,
    pub premiered: String,
    pub broadcast: String,
    pub duration: String,
    pub rating: String,
    pub score: f64,
    pub scored_by: u64,
    pub rank: u64,
    pub popularity: u64,
    pub members: u64,
    pub favorites: u64,
    pub synopsis: String,
    pub background: String,
    pub producers: Vec<Named>,
    pub licensors: Vec<Named>,
    pub studios: Vec<Named>,
    pub genres: Vec<Named>,
    pub themes: Vec<Named>,
    pub demographics: Vec<Named>,
    pub openings: Vec<String>,
    pub endings: Vec<String>,
    pub streaming: Vec<(String, String)>,
    pub external: Vec<(String, String)>,
    /// Extra facts of the source, label and value (TMDB: budget, revenue, languages, countries, seasons…).
    pub facts: Vec<(String, String)>,
    pub relations: Vec<Relation>,
}

/// A title in a list: genre pages and recommendations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub id: u64,
    pub title: String,
    pub image: String,
    pub score: f64,
    pub kind: String,
    pub year: i64,
    /// Number of people who recommended it (recommendations only).
    pub votes: u64,
    /// The id of the title in the catalogs when it is not a MyAnimeList one (movies and series by category).
    pub key: String,
    pub synopsis: String,
    pub genres: Vec<String>,
}

impl Card {
    /// The card as a catalog item, so it opens like any other title.
    pub fn to_meta(&self) -> MetaItem {
        let own = !self.key.is_empty();
        MetaItem {
            id: if own { self.key.clone() } else { format!("mal:{}", self.id) },
            kind: if own && self.kind == "movie" { "movie".to_string() } else { "series".to_string() },
            name: self.title.clone(),
            poster: if self.image.is_empty() { None } else { Some(self.image.clone()) },
            rating: if self.score > 0.0 { format!("{:.1}", self.score) } else { String::new() },
            release_info: if self.year > 0 { self.year.to_string() } else { String::new() },
            description: self.synopsis.clone(),
            genres: self.genres.clone(),
            mal_id: if own { None } else { Some(self.id) },
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Character {
    pub id: u64,
    pub name: String,
    pub role: String,
    pub image: String,
    pub actor: String,
    pub actor_lang: String,
    pub actor_image: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Staff {
    pub name: String,
    pub image: String,
    pub positions: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Review {
    pub user: String,
    pub avatar: String,
    pub date: String,
    /// "Recommended", "Mixed Feelings" or "Not Recommended".
    pub tag: String,
    pub score: u32,
    pub text: String,
    pub spoiler: bool,
    pub url: String,
    pub reactions: u32,
}

/// Everything loaded for one title page.
#[derive(Clone, Debug, Default)]
pub struct MalData {
    pub id: Option<u64>,
    /// "tmdb" when the page was filled from TMDB (a movie or series MyAnimeList does not have); empty otherwise.
    pub source: String,
    /// The lookup is running or about to run.
    pub loading: bool,
    /// No MyAnimeList entry matches this title.
    pub not_found: bool,
    pub error: String,
    pub info: Option<Info>,
    pub characters: Vec<Character>,
    pub staff: Vec<Staff>,
    pub recs: Vec<Card>,
    pub reviews: Vec<Review>,
    pub my: Option<MyStatus>,
    /// When the lookup began (to offer a retry when it takes too long).
    pub started: Option<std::time::Instant>,
}

pub const STATUSES: &[(&str, &str)] = &[("watching", "Watching"), ("completed", "Completed"), ("on_hold", "On hold"), ("dropped", "Dropped"), ("plan_to_watch", "Plan to watch")];

// ---------------------------------------------------------------- parsing (Jikan)

fn named(j: &Json) -> Vec<Named> {
    j.items().iter().map(|x| Named { id: x.get("mal_id").as_u64().unwrap_or(0), name: x.get("name").str() }).filter(|n| !n.name.is_empty()).collect()
}

fn pairs(j: &Json) -> Vec<(String, String)> {
    j.items().iter().map(|x| (x.get("name").str(), x.get("url").str())).filter(|(n, u)| !n.is_empty() && !u.is_empty()).collect()
}

fn image_of(j: &Json) -> String {
    for p in ["images.jpg.large_image_url", "images.jpg.image_url", "images.webp.large_image_url", "images.webp.image_url"] {
        let s = j.path(p).str();
        if !s.is_empty() {
            return s;
        }
    }
    String::new()
}

pub fn parse_info(d: &Json) -> Info {
    let mut genres = named(d.get("genres"));
    genres.extend(named(d.get("explicit_genres")));
    Info {
        provider: String::new(),
        id: d.get("mal_id").as_u64().unwrap_or(0),
        url: d.get("url").str(),
        title: d.get("title").str(),
        title_en: d.get("title_english").str(),
        title_ja: d.get("title_japanese").str(),
        synonyms: d.get("title_synonyms").strings(),
        image: image_of(d),
        trailer: d.path("trailer.url").str(),
        kind: d.get("type").str(),
        source: d.get("source").str(),
        episodes: d.get("episodes").as_u64().map(|n| n as u32),
        status: d.get("status").str(),
        aired: d.path("aired.string").str(),
        premiered: {
            let (s, y) = (d.get("season").str(), d.get("year").str());
            if s.is_empty() { String::new() } else { format!("{}{}{}", s[..1].to_uppercase(), &s[1..], if y.is_empty() { String::new() } else { format!(" {}", y) }) }
        },
        broadcast: d.path("broadcast.string").str(),
        duration: d.get("duration").str(),
        rating: d.get("rating").str(),
        score: d.get("score").as_f64().unwrap_or(0.0),
        scored_by: d.get("scored_by").as_u64().unwrap_or(0),
        rank: d.get("rank").as_u64().unwrap_or(0),
        popularity: d.get("popularity").as_u64().unwrap_or(0),
        members: d.get("members").as_u64().unwrap_or(0),
        favorites: d.get("favorites").as_u64().unwrap_or(0),
        synopsis: d.get("synopsis").str(),
        background: d.get("background").str(),
        producers: named(d.get("producers")),
        licensors: named(d.get("licensors")),
        studios: named(d.get("studios")),
        genres,
        themes: named(d.get("themes")),
        demographics: named(d.get("demographics")),
        openings: d.path("theme.openings").strings(),
        endings: d.path("theme.endings").strings(),
        streaming: pairs(d.get("streaming")),
        external: pairs(d.get("external")),
        facts: vec![],
        relations: d
            .get("relations")
            .items()
            .iter()
            .map(|r| Relation {
                kind: r.get("relation").str(),
                entries: r.get("entry").items().iter().map(|e| Related { id: e.get("mal_id").as_u64().unwrap_or(0), kind: e.get("type").str(), name: e.get("name").str() }).collect(),
            })
            .filter(|r| !r.entries.is_empty())
            .collect(),
    }
}

/// Anime lists (genre pages): each item is the anime itself.
pub fn parse_cards(data: &Json) -> Vec<Card> {
    data.items()
        .iter()
        .map(|a| Card {
            id: a.get("mal_id").as_u64().unwrap_or(0),
            title: {
                let t = a.get("title").str();
                if t.is_empty() { a.get("title_english").str() } else { t }
            },
            image: image_of(a),
            score: a.get("score").as_f64().unwrap_or(0.0),
            kind: a.get("type").str(),
            year: a.get("year").as_i64().or_else(|| a.path("aired.prop.from.year").as_i64()).unwrap_or(0),
            votes: 0,
            key: String::new(),
            synopsis: a.get("synopsis").str().replace("[Written by MAL Rewrite]", "").trim().to_string(),
            genres: a.get("genres").items().iter().map(|g| g.get("name").str()).filter(|g| !g.is_empty()).collect(),
        })
        .filter(|c| c.id != 0 && !c.title.is_empty())
        .collect()
}

pub fn parse_recommendations(data: &Json) -> Vec<Card> {
    data.items()
        .iter()
        .filter_map(|r| {
            let e = r.get("entry");
            let id = e.get("mal_id").as_u64()?;
            let title = e.get("title").str();
            if title.is_empty() {
                return None;
            }
            Some(Card { id, title, image: image_of(e), score: 0.0, kind: String::new(), year: 0, votes: r.get("votes").as_u64().unwrap_or(0), key: String::new(), ..Default::default() })
        })
        .collect()
}

pub fn parse_characters(data: &Json) -> Vec<Character> {
    data.items()
        .iter()
        .map(|c| {
            let ch = c.get("character");
            let vas = c.get("voice_actors").items();
            let va = vas.iter().find(|v| v.get("language").str() == "Japanese").or_else(|| vas.first());
            Character {
                id: ch.get("mal_id").as_u64().unwrap_or(0),
                name: ch.get("name").str(),
                role: c.get("role").str(),
                image: image_of(ch),
                actor: va.map(|v| v.path("person.name").str()).unwrap_or_default(),
                actor_lang: va.map(|v| v.get("language").str()).unwrap_or_default(),
                actor_image: va.map(|v| image_of(v.get("person"))).unwrap_or_default(),
            }
        })
        .filter(|c| !c.name.is_empty())
        .collect()
}

pub fn parse_staff(data: &Json) -> Vec<Staff> {
    data.items()
        .iter()
        .map(|s| Staff { name: s.path("person.name").str(), image: image_of(s.get("person")), positions: s.get("positions").strings().join(", ") })
        .filter(|s| !s.name.is_empty())
        .collect()
}

pub fn parse_reviews(data: &Json) -> Vec<Review> {
    data.items()
        .iter()
        .map(|r| {
            let tags = r.get("tags").strings();
            let tag = tags.iter().find(|t| matches!(t.as_str(), "Recommended" | "Mixed Feelings" | "Not Recommended")).cloned().unwrap_or_default();
            Review {
                user: r.path("user.username").str(),
                avatar: image_of(r.get("user")),
                date: r.get("date").str().chars().take(10).collect(),
                tag,
                score: r.get("score").as_u64().unwrap_or(0) as u32,
                text: r.get("review").str().trim().to_string(),
                spoiler: r.get("is_spoiler").as_bool().unwrap_or(false),
                url: r.get("url").str(),
                reactions: r.path("reactions.overall").as_u64().unwrap_or(0) as u32,
            }
        })
        .filter(|r| !r.text.is_empty())
        .collect()
}

// ---------------------------------------------------------------- Jikan client

static LAST_CALL: Mutex<Option<Instant>> = Mutex::new(None);

/// Jikan allows about three requests per second; calls are spaced out so it does not answer 429.
fn throttle() {
    let wait = {
        let mut g = LAST_CALL.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let at = match *g {
            Some(prev) if prev + Duration::from_millis(380) > now => prev + Duration::from_millis(380),
            _ => now,
        };
        *g = Some(at);
        at.saturating_duration_since(now)
    };
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
}

/// Unix time until which Jikan counts as unreachable (after a timeout or a server error), so the
/// next requests go straight to AniList instead of each waiting for a timeout.
static JIKAN_DOWN_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn jikan_down() -> bool {
    JIKAN_DOWN_UNTIL.load(std::sync::atomic::Ordering::Relaxed) > crate::util::now_unix()
}

fn mark_jikan_down() {
    JIKAN_DOWN_UNTIL.store(crate::util::now_unix() + 600, std::sync::atomic::Ordering::Relaxed);
}

pub struct Jikan<'a> {
    pub http: &'a dyn Http,
}

impl Jikan<'_> {
    pub fn get(&self, path: &str) -> Result<Json, String> {
        let url = format!("https://api.jikan.moe/v4{}", path);
        for attempt in 0..3u64 {
            throttle();
            let r = match self.http.send(&Request::get(&url).header("Accept", "application/json").timeout(10)) {
                Ok(r) => r,
                Err(e) => {
                    mark_jikan_down();
                    return Err(e);
                }
            };
            match r.status {
                s if s >= 500 => {
                    mark_jikan_down();
                    return Err(format!("MyAnimeList (Jikan) answered {}", s));
                }
                429 => std::thread::sleep(Duration::from_millis(1400 * (attempt + 1))),
                404 => return Err("MyAnimeList has no page for this title.".to_string()),
                s if (200..300).contains(&s) => return r.json(),
                s => return Err(format!("MyAnimeList (Jikan) answered {}", s)),
            }
        }
        Err("MyAnimeList (Jikan) is busy. Try again in a moment.".to_string())
    }

    pub fn info(&self, id: u64) -> Result<Info, String> {
        let j = self.get(&format!("/anime/{}/full", id))?;
        let info = parse_info(j.get("data"));
        if info.id == 0 {
            return Err("MyAnimeList returned an empty page.".to_string());
        }
        Ok(info)
    }
    pub fn characters(&self, id: u64) -> Result<Vec<Character>, String> {
        Ok(parse_characters(self.get(&format!("/anime/{}/characters", id))?.get("data")))
    }
    pub fn staff(&self, id: u64) -> Result<Vec<Staff>, String> {
        Ok(parse_staff(self.get(&format!("/anime/{}/staff", id))?.get("data")))
    }
    pub fn recommendations(&self, id: u64) -> Result<Vec<Card>, String> {
        Ok(parse_recommendations(self.get(&format!("/anime/{}/recommendations", id))?.get("data")))
    }
    pub fn reviews(&self, id: u64) -> Result<Vec<Review>, String> {
        Ok(parse_reviews(self.get(&format!("/anime/{}/reviews?preserve_newlines=true", id))?.get("data")))
    }
    /// One page of a genre, most popular first. Returns the cards and whether there is a next page.
    pub fn by_genre(&self, genre: u64, page: u32) -> Result<(Vec<Card>, bool), String> {
        let j = self.get(&format!("/anime?genres={}&order_by=members&sort=desc&limit=24&page={}&sfw=true", genre, page.max(1)))?;
        Ok((parse_cards(j.get("data")), j.path("pagination.has_next_page").as_bool().unwrap_or(false)))
    }
}

/// Cinemeta genre pages: movies or series of one genre, most popular first. `skip` is how many are already shown.
pub fn cinemeta_genre(http: &dyn Http, kind: &str, genre: &str, skip: usize) -> Result<(Vec<Card>, bool), String> {
    let url = format!("https://v3-cinemeta.strem.io/catalog/{}/top/genre={}&skip={}.json", kind, url_encode(genre), skip);
    let j = get_json(http, &url, 20)?;
    let cards: Vec<Card> = parse_cinemeta(&j, kind);
    let more = cards.len() >= 20;
    Ok((cards, more))
}

pub fn parse_cinemeta(j: &Json, kind: &str) -> Vec<Card> {
    j.get("metas")
        .items()
        .iter()
        .filter(|m| !m.get("id").str().is_empty() && !m.get("name").str().is_empty())
        .map(|m| Card {
            id: 0,
            title: m.get("name").str(),
            image: m.get("poster").str(),
            score: m.get("imdbRating").str().parse().unwrap_or(0.0),
            kind: kind.to_string(),
            year: first_year(&m.get("releaseInfo").str()).unwrap_or(0),
            votes: 0,
            key: m.get("id").str(),
            synopsis: m.get("description").str(),
            genres: m.get("genres").strings(),
        })
        .collect()
}

// ---------------------------------------------------------------- finding the MyAnimeList entry of a title

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

fn first_year(text: &str) -> Option<i64> {
    let digits: String = text.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() == 4 { digits.parse().ok() } else { None }
}

fn season_marker(title: &str, season: i64) -> bool {
    let t = title.to_lowercase();
    let ord = match season {
        2 => "2nd",
        3 => "3rd",
        n if n > 3 => return t.contains(&format!("{}th season", n)) || t.contains(&format!("season {}", n)),
        _ => "",
    };
    t.contains(&format!("season {}", season)) || (!ord.is_empty() && t.contains(&format!("{} season", ord))) || t.ends_with(&format!(" {}", season))
}

/// A search result, from Jikan or AniList.
#[derive(Clone, Debug, Default)]
pub struct Cand {
    pub id: u64,
    pub names: Vec<String>,
    pub kind: String,
    pub year: Option<i64>,
}

impl From<&crate::anilist::Found> for Cand {
    fn from(f: &crate::anilist::Found) -> Cand {
        Cand { id: f.id, names: f.names.clone(), kind: f.kind.clone(), year: f.year }
    }
}

/// Picks the search result that is most likely the title. None when nothing is close.
pub fn pick_best(results: &Json, name: &str, year: Option<i64>, season: i64) -> Option<u64> {
    let cands: Vec<Cand> = results
        .get("data")
        .items()
        .iter()
        .filter_map(|a| {
            let id = a.get("mal_id").as_u64()?;
            let mut names = vec![a.get("title").str(), a.get("title_english").str()];
            names.extend(a.get("title_synonyms").strings());
            Some(Cand { id, names, kind: a.get("type").str(), year: a.get("year").as_i64().or_else(|| a.path("aired.prop.from.year").as_i64()) })
        })
        .collect();
    pick_cands(&cands, name, year, season)
}

pub fn pick_from(found: &[crate::anilist::Found], name: &str, year: Option<i64>, season: i64) -> Option<u64> {
    let cands: Vec<Cand> = found.iter().map(Cand::from).collect();
    pick_cands(&cands, name, year, season)
}

pub fn pick_cands(cands: &[Cand], name: &str, year: Option<i64>, season: i64) -> Option<u64> {
    let want = norm(name);
    if want.is_empty() {
        return None;
    }
    let mut best: Option<(i32, u64)> = None;
    for a in cands {
        let names = &a.names;
        let mut score = 0;
        for n in names.iter().map(|n| norm(n)).filter(|n| !n.is_empty()) {
            if n == want {
                score = score.max(100);
            } else if n.starts_with(&want) {
                score = score.max(60);
            } else if n.contains(&want) {
                score = score.max(40);
            }
        }
        if score == 0 {
            continue;
        }
        let kind = a.kind.as_str();
        if matches!(kind, "TV" | "Movie" | "ONA" | "MOVIE" | "TV_SHORT") {
            score += 5;
        }
        if season > 1 {
            if names.iter().any(|n| season_marker(n, season)) {
                score += 40;
            } else {
                score -= 20;
            }
        } else if names.iter().any(|n| season_marker(n, 2) || season_marker(n, 3)) {
            score -= 15;
        }
        if let (Some(y), Some(ay)) = (year, a.year) {
            if (y - ay).abs() <= 1 {
                score += 15;
            } else if season <= 1 {
                score -= 15;
            }
        }
        if best.map(|(s, _)| score > s).unwrap_or(true) {
            best = Some((score, a.id));
        }
    }
    best.filter(|(s, _)| *s >= 40).map(|(_, id)| id)
}

/// The MyAnimeList id of a title page: the addon's own id, an id such as "mal:123", or a search by name.
pub fn resolve_id(http: &dyn Http, meta: &MetaItem, season: i64) -> Result<Option<u64>, String> {
    if let Some(id) = meta.mal_id {
        return Ok(Some(id));
    }
    if let Some(n) = meta.id.strip_prefix("mal:").and_then(|r| r.split(':').next()).and_then(|r| r.parse::<u64>().ok()) {
        return Ok(Some(n));
    }
    let genres_ok = meta.genres.iter().any(|g| g.eq_ignore_ascii_case("animation") || g.eq_ignore_ascii_case("anime")) || meta.id.starts_with("kitsu:") || meta.kind == "anime";
    if !genres_ok || meta.name.trim().is_empty() {
        return Ok(None);
    }
    let q = if season > 1 { format!("{} season {}", meta.name, season) } else { meta.name.clone() };
    let year = first_year(&meta.release_info);
    if !jikan_down() {
        match (Jikan { http }).get(&format!("/anime?q={}&limit=10&sfw=true", url_encode(&q))) {
            Ok(j) => return Ok(pick_best(&j, &meta.name, year, season)),
            // Jikan cannot be reached: AniList knows the same ids.
            Err(_) if jikan_down() => {}
            Err(e) => return Err(e),
        }
    }
    let found = crate::anilist::search(http, &q)?;
    Ok(pick_from(&found, &meta.name, year, season))
}

// ---------------------------------------------------------------- the user's list

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListEntry {
    pub mal_id: u64,
    pub imdb: String,
    pub kitsu: String,
    pub title: String,
    /// watching, completed, on_hold, dropped or plan_to_watch
    pub status: String,
    pub progress: u32,
    pub episodes: u32,
    pub score: u32,
    pub year: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MyStatus {
    pub status: String,
    pub score: u32,
    pub watched: u32,
}

pub const ANISYNC_LISTS: &[(&str, &str)] =
    &[("mal_watching", "watching"), ("mal_plan_to_watch", "plan_to_watch"), ("mal_completed", "completed"), ("mal_on_hold", "on_hold"), ("mal_dropped", "dropped")];

/// "MAL Watchlist, Completed, progress 12/12" -> (12, 12)
fn progress_of(desc: &str) -> (u32, u32) {
    let Some(i) = desc.find("progress") else { return (0, 0) };
    let rest = &desc[i + 8..];
    let mut nums = rest.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty());
    let a = nums.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let b = nums.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (a, b)
}

pub fn parse_anisync_list(j: &Json, status: &str) -> Vec<ListEntry> {
    j.get("metas")
        .items()
        .iter()
        .map(|m| {
            let (progress, total) = progress_of(&m.get("description").str());
            let episodes = m.get("episodes").as_u64().map(|n| n as u32).filter(|n| *n > 0).unwrap_or(total);
            ListEntry {
                mal_id: m.get("mal_id").as_u64().unwrap_or(0),
                imdb: m.get("imdb_id").str(),
                kitsu: {
                    let k = m.get("kitsu_id").str();
                    if k.is_empty() { m.get("id").str().strip_prefix("kitsu:").unwrap_or("").to_string() } else { k }
                },
                title: m.get("name").str(),
                status: status.to_string(),
                progress,
                episodes,
                score: 0,
                year: m.get("year").as_i64().or_else(|| first_year(&m.get("releaseInfo").str())).unwrap_or(0),
            }
        })
        .filter(|e| e.mal_id != 0 || !e.kitsu.is_empty() || !e.imdb.is_empty())
        .collect()
}

/// The five lists of the user from AniSync. Returns what could be read and the errors.
pub fn fetch_anisync(http: &dyn Http, addon_url: &str) -> (Vec<ListEntry>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for (cat, status) in ANISYNC_LISTS {
        let mut skip = 0usize;
        for _ in 0..12 {
            let sk = skip.to_string();
            let extra: Vec<(&str, &str)> = if skip == 0 { vec![] } else { vec![("skip", sk.as_str())] };
            let url = crate::addons::resource_url(addon_url, "catalog", "anime", cat, &extra);
            match get_json(http, &url, 25) {
                Ok(j) => {
                    let page = parse_anisync_list(&j, status);
                    let raw = j.get("metas").items().len();
                    out.extend(page);
                    if raw == 0 {
                        break;
                    }
                    skip += raw;
                }
                Err(e) => {
                    errors.push(format!("AniSync {}: {}", status, e));
                    break;
                }
            }
        }
    }
    (out, errors)
}

/// How many episodes of an entry count as watched.
pub fn watched_count(e: &ListEntry) -> u32 {
    match e.status.as_str() {
        "completed" => e.episodes.max(e.progress),
        "plan_to_watch" => 0,
        _ => e.progress,
    }
}

/// Episode ids of `meta` that the list says were watched.
pub fn watched_ids(meta: &MetaItem, list: &[ListEntry]) -> HashSet<String> {
    let mut out = HashSet::new();
    let imdb = if meta.id.starts_with("tt") { meta.id.split(':').next().unwrap_or("").to_string() } else { meta.imdb_id.clone() };
    let kitsu = meta.id.strip_prefix("kitsu:").map(|r| r.split(':').next().unwrap_or("").to_string());
    let malid = meta.mal_id.or_else(|| meta.id.strip_prefix("mal:").and_then(|r| r.split(':').next()).and_then(|r| r.parse().ok()));
    let mut hits: Vec<&ListEntry> = list
        .iter()
        .filter(|e| {
            (!imdb.is_empty() && e.imdb == imdb) || (kitsu.as_deref().map(|k| !k.is_empty() && e.kitsu == k).unwrap_or(false)) || (malid.is_some() && Some(e.mal_id) == malid)
        })
        .collect();
    if hits.is_empty() {
        return out;
    }
    // Entries that are the title itself (kitsu / mal ids): the episodes are numbered inside the entry.
    if kitsu.is_some() || (malid.is_some() && imdb.is_empty()) {
        let w = hits.iter().map(|e| watched_count(e)).max().unwrap_or(0) as i64;
        for v in meta.videos.iter().filter(|v| v.season.unwrap_or(1) != 0) {
            if let Some(ep) = v.episode {
                if ep <= w {
                    out.insert(v.id.clone());
                }
            }
        }
        return out;
    }
    // One IMDb show, one entry per season (or per cour): lay the entries over the episodes in order.
    hits.sort_by_key(|e| (e.year, e.mal_id));
    hits.dedup_by_key(|e| e.mal_id);
    let seasons: Vec<i64> = meta.seasons().into_iter().filter(|s| *s >= 1).collect();
    let mut free: Vec<Vec<&crate::model::Video>> = seasons.iter().map(|s| meta.episodes(*s)).collect();
    for e in hits {
        let w = watched_count(e) as usize;
        let n = e.episodes as usize;
        let mut taken: Vec<&crate::model::Video> = Vec::new();
        if n > 0 {
            if let Some(i) = free.iter().position(|eps| eps.len() == n) {
                taken = std::mem::take(&mut free[i]);
            }
        }
        if taken.is_empty() {
            let want = if n > 0 { n } else { w.max(1) };
            for eps in free.iter_mut() {
                while taken.len() < want && !eps.is_empty() {
                    taken.push(eps.remove(0));
                }
            }
        }
        for v in taken.into_iter().take(w) {
            out.insert(v.id.clone());
        }
    }
    out
}

// ---------------------------------------------------------------- official API (OAuth)

pub const API: &str = "https://api.myanimelist.net/v2";
pub const REDIRECT_PORT: u16 = 51789;

pub fn redirect_uri() -> String {
    format!("http://localhost:{}/callback", REDIRECT_PORT)
}

/// The address sent to MyAnimeList: the one the person registered, or the local one.
/// It has to be one of the addresses of the app on myanimelist.net/apiconfig, letter for letter.
pub fn effective_redirect(custom: &str) -> String {
    let c = custom.trim();
    if c.is_empty() { redirect_uri() } else { c.to_string() }
}

/// Whether the browser can come back to this program by itself.
pub fn is_local_redirect(redirect: &str) -> bool {
    redirect == redirect_uri()
}

/// 64 random characters allowed in a PKCE verifier. Randomness comes from the keys std gives every hasher.
/// A MyAnimeList Client ID: 32 hexadecimal characters.
pub fn valid_client_id(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn new_verifier() -> String {
    use std::hash::{BuildHasher, Hasher};
    const SET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let state = std::collections::hash_map::RandomState::new();
    (0..64)
        .map(|i| {
            let mut h = state.build_hasher();
            h.write_u64(i);
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
            SET[(h.finish() % SET.len() as u64) as usize] as char
        })
        .collect()
}

/// MyAnimeList only supports the "plain" method: the challenge is the verifier itself.
pub fn auth_url(client_id: &str, verifier: &str, state: &str, redirect: &str) -> String {
    format!(
        "https://myanimelist.net/v1/oauth2/authorize?response_type=code&client_id={}&state={}&redirect_uri={}&code_challenge={}&code_challenge_method=plain",
        form_encode(client_id),
        form_encode(state),
        form_encode(redirect),
        form_encode(verifier)
    )
}

/// What the browser brings back: `?code=...&state=...` (or `?error=...`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Callback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Reads a callback from a whole address, a request line ("GET /callback?code=.. HTTP/1.1") or just the query.
pub fn parse_callback(text: &str) -> Callback {
    let text = text.trim();
    let first = text.lines().next().unwrap_or("");
    // "GET /path?query HTTP/1.1" or a plain URL: take the piece that holds the query.
    let piece = first.split_whitespace().find(|p| p.contains('?')).unwrap_or(first);
    let query = piece.split_once('?').map(|x| x.1).unwrap_or(piece);
    let query = query.split('#').next().unwrap_or("");
    let get = |k: &str| query.split('&').find_map(|kv| kv.split_once('=').filter(|(a, _)| *a == k).map(|(_, v)| percent_decode(v)));
    Callback { code: get("code").filter(|c| !c.is_empty()), state: get("state"), error: get("error") }
}

/// Waits for the browser to come back to http://localhost with `?code=...`. Returns the code.
/// Listens on IPv4 and IPv6: Windows browsers often resolve "localhost" to ::1 first.
pub fn wait_for_code(state: &str, secs: u64, cancel: &std::sync::atomic::AtomicBool) -> Result<String, String> {
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering;
    let mut listeners: Vec<std::net::TcpListener> = Vec::new();
    for _ in 0..8 {
        if let Ok(l) = std::net::TcpListener::bind(("127.0.0.1", REDIRECT_PORT)) {
            listeners.push(l);
            break;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    if let Ok(l) = std::net::TcpListener::bind(("::1", REDIRECT_PORT)) {
        listeners.push(l);
    }
    if listeners.is_empty() {
        return Err(format!("Port {} is in use, so the browser cannot come back to the app. Close the other program, or paste the address of the page below.", REDIRECT_PORT));
    }
    for l in &listeners {
        l.set_nonblocking(true).map_err(|e| e.to_string())?;
    }
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("Cancelled.".to_string());
        }
        if started.elapsed().as_secs() > secs {
            return Err("The login took too long. Try again.".to_string());
        }
        let mut idle = true;
        for l in &listeners {
            let Ok((mut s, _)) = l.accept() else { continue };
            idle = false;
            let _ = s.set_nonblocking(false);
            // Browsers open spare connections that send nothing: do not wait long on them.
            let _ = s.set_read_timeout(Some(Duration::from_millis(1500)));
            let mut buf = [0u8; 8192];
            let n = s.read(&mut buf).unwrap_or(0);
            if n == 0 {
                continue;
            }
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            let line = req.lines().next().unwrap_or("");
            // The page icon and other side requests are not the answer.
            if !line.contains("/callback") {
                let _ = write!(s, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                continue;
            }
            let cb = parse_callback(line);
            let (title, ok) = match (&cb.code, &cb.state, &cb.error) {
                (Some(_), Some(x), _) if x == state => ("Logged in. You can close this tab and go back to Reelhouse.", true),
                (_, _, Some(_)) => ("MyAnimeList refused the login. You can close this tab.", false),
                _ => ("This is not the login Reelhouse is waiting for. You can close this tab.", false),
            };
            let body = format!("<html><head><meta charset=\"utf-8\"></head><body style=\"font-family:sans-serif;text-align:center;margin-top:20vh\"><h2>{}</h2></body></html>", title);
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            let _ = s.flush();
            if ok {
                return Ok(cb.code.unwrap_or_default());
            }
            if let Some(e) = cb.error {
                return Err(format!("MyAnimeList refused the login ({}).", e));
            }
        }
        if idle {
            std::thread::sleep(Duration::from_millis(150));
        }
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub expires_in: u64,
}

fn tokens_from(r: &crate::http::Response) -> Result<Tokens, String> {
    let j = r.json().unwrap_or(Json::Null);
    if !r.ok() {
        let msg = j.get("message").str();
        let hint = j.get("hint").str();
        let err = j.get("error").str();
        return Err(format!("MyAnimeList refused the login ({}): {}", r.status, [err, msg, hint].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")));
    }
    let access = j.get("access_token").str();
    if access.is_empty() {
        return Err("MyAnimeList did not return a token.".to_string());
    }
    Ok(Tokens { access, refresh: j.get("refresh_token").str(), expires_in: j.get("expires_in").as_u64().unwrap_or(2_678_400) })
}

fn client_pairs<'a>(client_id: &'a str, secret: &'a str, mut rest: Vec<(&'a str, &'a str)>) -> Vec<(&'a str, &'a str)> {
    let mut v = vec![("client_id", client_id)];
    // Apps of type "web" have a secret, and MyAnimeList then requires it.
    if !secret.is_empty() {
        v.push(("client_secret", secret));
    }
    v.append(&mut rest);
    v
}

pub fn exchange(http: &dyn Http, client_id: &str, secret: &str, redirect: &str, code: &str, verifier: &str) -> Result<Tokens, String> {
    let pairs = client_pairs(client_id, secret, vec![("grant_type", "authorization_code"), ("code", code), ("redirect_uri", redirect), ("code_verifier", verifier)]);
    let r = http.send(&Request::post("https://myanimelist.net/v1/oauth2/token").form(&pairs).timeout(25))?;
    tokens_from(&r)
}

pub fn refresh(http: &dyn Http, client_id: &str, secret: &str, refresh_token: &str) -> Result<Tokens, String> {
    let pairs = client_pairs(client_id, secret, vec![("grant_type", "refresh_token"), ("refresh_token", refresh_token)]);
    let r = http.send(&Request::post("https://myanimelist.net/v1/oauth2/token").form(&pairs).timeout(25))?;
    tokens_from(&r)
}

pub struct Api<'a> {
    pub http: &'a dyn Http,
    pub token: String,
}

fn parse_my_status(s: &Json) -> Option<MyStatus> {
    let status = s.get("status").str();
    if status.is_empty() {
        return None;
    }
    Some(MyStatus { status, score: s.get("score").as_u64().unwrap_or(0) as u32, watched: s.get("num_episodes_watched").as_u64().unwrap_or(0) as u32 })
}

impl Api<'_> {
    fn call(&self, method: &'static str, path: &str, body: Option<&[(&str, &str)]>) -> Result<Json, String> {
        let url = if path.starts_with("http") { path.to_string() } else { format!("{}{}", API, path) };
        let mut req = Request::post(&url).header("Authorization", &format!("Bearer {}", self.token)).timeout(25);
        req.method = method;
        if let Some(b) = body {
            req = req.form(b);
        }
        let r = self.http.send(&req)?;
        if r.status == 401 {
            return Err("MyAnimeList says you are not logged in. Log in again in Accounts.".to_string());
        }
        if !r.ok() {
            let j = r.json().unwrap_or(Json::Null);
            let m = j.get("message").str();
            return Err(format!("MyAnimeList answered {}{}", r.status, if m.is_empty() { String::new() } else { format!(": {}", m) }));
        }
        Ok(r.json().unwrap_or(Json::Null))
    }

    pub fn me(&self) -> Result<String, String> {
        Ok(self.call("GET", "/users/@me", None)?.get("name").str())
    }

    pub fn my_status(&self, id: u64) -> Result<Option<MyStatus>, String> {
        let j = self.call("GET", &format!("/anime/{}?fields=my_list_status", id), None)?;
        Ok(parse_my_status(j.get("my_list_status")))
    }

    pub fn set_status(&self, id: u64, s: &MyStatus) -> Result<MyStatus, String> {
        let (score, watched) = (s.score.to_string(), s.watched.to_string());
        let j = self.call("PUT", &format!("/anime/{}/my_list_status", id), Some(&[("status", s.status.as_str()), ("score", score.as_str()), ("num_watched_episodes", watched.as_str())]))?;
        Ok(parse_my_status(&j).unwrap_or_else(|| s.clone()))
    }

    /// Tells MyAnimeList how far a manga was read. The count only ever goes up. Returns whether it was changed.
    pub fn track_manga(&self, id: u64, chapters_read: u32) -> Result<bool, String> {
        let j = self.call("GET", &format!("/manga/{}?fields=my_list_status", id), None)?;
        let cur = j.path("my_list_status.num_chapters_read").as_u64().unwrap_or(0) as u32;
        let status = j.path("my_list_status.status").str();
        if chapters_read <= cur || status == "completed" {
            return Ok(false);
        }
        let n = chapters_read.to_string();
        self.call("PUT", &format!("/manga/{}/my_list_status", id), Some(&[("status", "reading"), ("num_chapters_read", n.as_str())]))?;
        Ok(true)
    }

    pub fn remove(&self, id: u64) -> Result<(), String> {
        let url = format!("{}/anime/{}/my_list_status", API, id);
        let mut req = Request::get(&url).header("Authorization", &format!("Bearer {}", self.token)).timeout(25);
        req.method = "DELETE";
        let r = self.http.send(&req)?;
        // 404 means it was not on the list, which is the goal anyway.
        if r.ok() || r.status == 404 { Ok(()) } else { Err(format!("MyAnimeList answered {}", r.status)) }
    }

    /// The whole list of the user.
    pub fn list(&self) -> Result<Vec<ListEntry>, String> {
        let mut out = Vec::new();
        let mut url = "/users/@me/animelist?fields=list_status,num_episodes&limit=1000&nsfw=true".to_string();
        for _ in 0..20 {
            let j = self.call("GET", &url, None)?;
            for it in j.get("data").items() {
                let n = it.get("node");
                let ls = it.get("list_status");
                out.push(ListEntry {
                    mal_id: n.get("id").as_u64().unwrap_or(0),
                    imdb: String::new(),
                    kitsu: String::new(),
                    title: n.get("title").str(),
                    status: ls.get("status").str(),
                    progress: ls.get("num_episodes_watched").as_u64().unwrap_or(0) as u32,
                    episodes: n.get("num_episodes").as_u64().unwrap_or(0) as u32,
                    score: ls.get("score").as_u64().unwrap_or(0) as u32,
                    year: 0,
                });
            }
            match j.path("paging.next").as_str() {
                Some(next) if !next.is_empty() => url = next.to_string(),
                _ => break,
            }
        }
        Ok(out)
    }
}

/// Merges the lists: the official one wins on status and progress, AniSync adds the IMDb and Kitsu ids.
pub fn merge_lists(anisync: Vec<ListEntry>, official: Vec<ListEntry>) -> Vec<ListEntry> {
    let mut out = anisync;
    for o in official {
        match out.iter_mut().find(|e| e.mal_id == o.mal_id && o.mal_id != 0) {
            Some(e) => {
                e.status = o.status;
                e.progress = o.progress;
                e.score = o.score;
                if o.episodes > 0 {
                    e.episodes = o.episodes;
                }
            }
            None => out.push(o),
        }
    }
    out
}

pub fn form_pair(k: &str, v: &str) -> String {
    format!("{}={}", form_encode(k), form_encode(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;
    use crate::model::Video;

    fn j(s: &str) -> Json {
        Json::parse(s).unwrap()
    }

    #[test]
    fn reads_a_full_anime_page() {
        let d = j(r#"{"mal_id":51105,"url":"https://myanimelist.net/anime/51105","title":"NieR:Automata Ver1.1a","title_english":"NieR:Automata Ver1.1a","title_japanese":"ニーア","title_synonyms":["NieR"],
          "images":{"jpg":{"large_image_url":"https://cdn/p.jpg"}},"trailer":{"url":"https://yt/x"},"type":"TV","source":"Game","episodes":12,"status":"Finished Airing",
          "aired":{"string":"Jan 8, 2023 to Mar 26, 2023"},"duration":"24 min per ep","rating":"R - 17+","score":7.9,"scored_by":100,"rank":900,"popularity":300,"members":5000,"favorites":10,
          "synopsis":"S","background":"B","season":"winter","year":2023,"broadcast":{"string":"Sundays at 23:00 (JST)"},
          "producers":[{"mal_id":1,"name":"Aniplex"}],"studios":[{"mal_id":2,"name":"A-1 Pictures"}],"genres":[{"mal_id":1,"name":"Action"},{"mal_id":8,"name":"Drama"}],"explicit_genres":[],
          "themes":[{"mal_id":50,"name":"Adult Cast"}],"demographics":[{"mal_id":42,"name":"Seinen"}],
          "relations":[{"relation":"Sequel","entry":[{"mal_id":56063,"type":"anime","name":"Cour 2"}]},{"relation":"Other","entry":[]}],
          "theme":{"openings":["1: \"Ex-Human\""],"endings":["1: \"Escape\""]},"streaming":[{"name":"Crunchyroll","url":"https://cr"}],"external":[{"name":"Official Site","url":"https://nier"}]}"#);
        let i = parse_info(&d);
        assert_eq!((i.id, i.kind.as_str(), i.episodes, i.premiered.as_str()), (51105, "TV", Some(12), "Winter 2023"));
        assert_eq!(i.genres.iter().map(|g| g.id).collect::<Vec<_>>(), vec![1, 8]);
        assert_eq!(i.relations.len(), 1);
        assert_eq!((i.openings.len(), i.streaming[0].0.as_str(), i.broadcast.as_str()), (1, "Crunchyroll", "Sundays at 23:00 (JST)"));
        assert_eq!(i.image, "https://cdn/p.jpg");
    }

    #[test]
    fn reads_characters_staff_recommendations_and_reviews() {
        let c = parse_characters(&j(r#"[{"character":{"mal_id":1,"name":"2B","images":{"jpg":{"image_url":"c.jpg"}}},"role":"Main","voice_actors":[{"person":{"name":"Hanae, Natsuki","images":{"jpg":{"image_url":"v.jpg"}}},"language":"English"},{"person":{"name":"Ishikawa, Yui","images":{"jpg":{"image_url":"j.jpg"}}},"language":"Japanese"}]}]"#));
        assert_eq!((c[0].actor.as_str(), c[0].actor_lang.as_str(), c[0].actor_image.as_str()), ("Ishikawa, Yui", "Japanese", "j.jpg"));
        let s = parse_staff(&j(r#"[{"person":{"name":"Matsumoto, Miho","images":{"jpg":{"image_url":"x"}}},"positions":["Producer","Planning"]}]"#));
        assert_eq!(s[0].positions, "Producer, Planning");
        let r = parse_recommendations(&j(r#"[{"entry":{"mal_id":5,"title":"Other","images":{"jpg":{"large_image_url":"o.jpg"}}},"votes":12},{"entry":{"mal_id":6,"title":""},"votes":1}]"#));
        assert_eq!((r.len(), r[0].votes), (1, 12));
        let v = parse_reviews(&j(r#"[{"url":"u","date":"2026-10-11T10:00:00+00:00","review":" Great ","score":9,"tags":["Recommended"],"is_spoiler":false,"reactions":{"overall":3},"user":{"username":"sussytuna","images":{"jpg":{"image_url":"a.jpg"}}}},{"review":""}]"#));
        assert_eq!((v.len(), v[0].tag.as_str(), v[0].date.as_str(), v[0].reactions, v[0].text.as_str()), (1, "Recommended", "2026-10-11", 3, "Great"));
    }

    #[test]
    fn picks_the_right_search_result() {
        let res = j(r#"{"data":[
          {"mal_id":1,"title":"Re:Zero kara Hajimeru Isekai Seikatsu","title_english":"Re:ZERO -Starting Life in Another World-","type":"TV","year":2016},
          {"mal_id":2,"title":"Re:Zero kara Hajimeru Isekai Seikatsu 2nd Season","title_english":"Re:ZERO -Starting Life in Another World- Season 2","type":"TV","year":2020},
          {"mal_id":3,"title":"Something else entirely","type":"TV","year":2020}]}"#);
        assert_eq!(pick_best(&res, "Re:ZERO -Starting Life in Another World-", Some(2016), 1), Some(1));
        assert_eq!(pick_best(&res, "Re:ZERO -Starting Life in Another World-", Some(2016), 2), Some(2));
        assert_eq!(pick_best(&res, "Totally unrelated name", None, 1), None);
        assert_eq!(first_year("2024–"), Some(2024));
    }

    #[test]
    fn finds_the_id_from_the_item_or_a_search() {
        let http = Fake::new().route("/anime?q=Dandadan", 200, r#"{"data":[{"mal_id":57334,"title":"Dandadan","type":"TV","year":2024}]}"#);
        let mut m = MetaItem { id: "tt1".into(), name: "Dandadan".into(), release_info: "2024–".into(), genres: vec!["Animation".into()], ..Default::default() };
        assert_eq!(resolve_id(&http, &m, 1).unwrap(), Some(57334));
        m.genres = vec!["Drama".into()];
        assert_eq!(resolve_id(&http, &m, 1).unwrap(), None);
        m.id = "mal:777:1".into();
        assert_eq!(resolve_id(&http, &m, 1).unwrap(), Some(777));
        m.mal_id = Some(5);
        assert_eq!(resolve_id(&http, &m, 1).unwrap(), Some(5));
    }

    #[test]
    fn reads_anisync_lists() {
        let page = j(r#"{"metas":[{"id":"kitsu:47784","name":"NieR:Automata Ver1.1a (Cour 2)","type":"series","year":2024,"episodes":12,"description":"MAL Watchlist, Completed, progress 12/12","mal_id":56063,"kitsu_id":47784,"imdb_id":"tt18311412"},
          {"id":"kitsu:1","name":"X","description":"MAL Watchlist, Watching, progress 3/24","mal_id":9}]}"#);
        let l = parse_anisync_list(&page, "completed");
        assert_eq!((l[0].mal_id, l[0].progress, l[0].episodes, l[0].kitsu.as_str(), l[0].imdb.as_str()), (56063, 12, 12, "47784", "tt18311412"));
        assert_eq!((l[1].progress, l[1].episodes), (3, 24));
        let http = Fake::new().route("catalog/anime/mal_completed.json", 200, r#"{"metas":[{"id":"kitsu:2","name":"Done","mal_id":2,"description":"progress 1/1"}]}"#).route("mal_", 200, r#"{"metas":[]}"#);
        let (all, errs) = fetch_anisync(&http, "https://a.example/tok/manifest.json");
        assert!(errs.is_empty());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, "completed");
    }

    fn show(seasons: &[(i64, i64)]) -> MetaItem {
        let mut m = MetaItem { id: "tt18311412".into(), kind: "series".into(), ..Default::default() };
        for (s, n) in seasons {
            for e in 1..=*n {
                m.videos.push(Video { id: format!("tt18311412:{}:{}", s, e), season: Some(*s), episode: Some(e), ..Default::default() });
            }
        }
        m
    }

    fn entry(mal: u64, year: i64, status: &str, progress: u32, episodes: u32) -> ListEntry {
        ListEntry { mal_id: mal, imdb: "tt18311412".into(), status: status.into(), progress, episodes, year, ..Default::default() }
    }

    #[test]
    fn marks_watched_episodes_from_the_list() {
        // Two cours of 12 episodes on one Cinemeta season of 24: the list fills it in order.
        let m = show(&[(1, 24)]);
        let w = watched_ids(&m, &[entry(51105, 2023, "completed", 12, 12), entry(56063, 2024, "watching", 5, 12)]);
        assert_eq!(w.len(), 17);
        assert!(w.contains("tt18311412:1:12") && w.contains("tt18311412:1:17") && !w.contains("tt18311412:1:18"));
        // Separate seasons of different lengths: an entry lands on the season with the same length.
        let m = show(&[(1, 12), (2, 24)]);
        let w = watched_ids(&m, &[entry(1, 2020, "completed", 12, 12)]);
        assert_eq!(w.len(), 12);
        assert!(w.contains("tt18311412:1:1") && !w.contains("tt18311412:2:1"));
        // Only season 2 of 24 on the list: season 1 stays unwatched.
        let w = watched_ids(&m, &[entry(2, 2021, "watching", 4, 24)]);
        assert_eq!(w.len(), 4);
        assert!(w.contains("tt18311412:2:4") && !w.contains("tt18311412:1:1"));
        // Plan to watch is not watched; unknown shows are not either.
        assert!(watched_ids(&m, &[entry(2, 2021, "plan_to_watch", 0, 24)]).is_empty());
        assert!(watched_ids(&MetaItem { id: "tt999".into(), ..m.clone() }, &[entry(2, 2021, "completed", 24, 24)]).is_empty());
    }

    #[test]
    fn kitsu_titles_use_their_own_numbering() {
        let mut m = MetaItem { id: "kitsu:47784".into(), ..Default::default() };
        for e in 1..=12 {
            m.videos.push(Video { id: format!("kitsu:47784:{}", e), season: Some(1), episode: Some(e), ..Default::default() });
        }
        let e = ListEntry { mal_id: 56063, kitsu: "47784".into(), status: "watching".into(), progress: 3, episodes: 12, ..Default::default() };
        let w = watched_ids(&m, &[e.clone()]);
        assert_eq!(w.len(), 3);
        let done = ListEntry { status: "completed".into(), progress: 12, ..e };
        assert_eq!(watched_ids(&m, &[done]).len(), 12);
    }

    #[test]
    fn merges_the_lists() {
        let a = vec![entry(1, 2020, "watching", 2, 12)];
        let o = vec![ListEntry { mal_id: 1, status: "completed".into(), progress: 12, episodes: 12, score: 9, ..Default::default() }, ListEntry { mal_id: 2, status: "dropped".into(), ..Default::default() }];
        let m = merge_lists(a, o);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].status.as_str(), m[0].imdb.as_str(), m[0].score), ("completed", "tt18311412", 9));
    }

    #[test]
    fn client_ids() {
        assert!(valid_client_id("0123456789abcdef0123456789ABCDEF"));
        assert!(!valid_client_id("hpjogo"));
        assert!(!valid_client_id(""));
    }

    #[test]
    fn oauth_pieces() {
        let v = new_verifier();
        assert_eq!(v.len(), 64);
        assert_ne!(v, new_verifier());
        let u = auth_url("cid", &v, "st", &redirect_uri());
        assert!(u.contains("client_id=cid") && u.contains("code_challenge_method=plain") && u.contains(&format!("code_challenge={}", v)));
        assert!(u.contains("redirect_uri=http%3A%2F%2Flocalhost%3A51789%2Fcallback"));
        assert_eq!(percent_decode("a%20b%2Bc+d%"), "a b+c d%");
        assert_eq!(percent_decode("x%41"), "xA");
        let http = Fake::new().route("POST https://myanimelist.net/v1/oauth2/token", 200, r#"{"access_token":"A","refresh_token":"R","expires_in":3600}"#);
        let t = exchange(&http, "cid", "sec", &redirect_uri(), "code", &v).unwrap();
        assert_eq!((t.access.as_str(), t.refresh.as_str(), t.expires_in), ("A", "R", 3600));
        let body = String::from_utf8_lossy(&http.requests()[0].body.as_ref().unwrap().1).into_owned();
        assert!(body.contains("grant_type=authorization_code") && body.contains("code_verifier=") && body.contains("client_secret=sec"));
        assert_eq!(effective_redirect("  "), redirect_uri());
        assert_eq!(effective_redirect(" https://x.y/configure "), "https://x.y/configure");
        assert!(auth_url("c", &v, "s", "https://x.y/configure").contains("redirect_uri=https%3A%2F%2Fx.y%2Fconfigure"));
        let cb = parse_callback("GET /callback?code=AB%2Fc&state=xyz HTTP/1.1");
        assert_eq!((cb.code.as_deref(), cb.state.as_deref()), (Some("AB/c"), Some("xyz")));
        let cb = parse_callback("http://localhost:51789/callback?state=s&code=Q#frag");
        assert_eq!((cb.code.as_deref(), cb.state.as_deref()), (Some("Q"), Some("s")));
        assert_eq!(parse_callback("http://localhost:51789/callback?error=access_denied").error.as_deref(), Some("access_denied"));
        assert!(parse_callback("nothing here").code.is_none());
        let bad = Fake::new().route("oauth2/token", 400, r#"{"error":"invalid_grant","hint":"expired"}"#);
        assert!(refresh(&bad, "c", "", "r").unwrap_err().contains("expired"));
    }

    #[test]
    fn list_status_calls() {
        let http = Fake::new()
            .route("GET https://api.myanimelist.net/v2/anime/5?fields=my_list_status", 200, r#"{"id":5,"my_list_status":{"status":"watching","score":8,"num_episodes_watched":3}}"#)
            .route("PUT https://api.myanimelist.net/v2/anime/5/my_list_status", 200, r#"{"status":"completed","score":9,"num_episodes_watched":12}"#)
            .route("GET https://api.myanimelist.net/v2/users/@me/animelist", 200, r#"{"data":[{"node":{"id":5,"title":"T","num_episodes":12},"list_status":{"status":"watching","score":0,"num_episodes_watched":3}}],"paging":{}}"#);
        let api = Api { http: &http, token: "tok".into() };
        let s = api.my_status(5).unwrap().unwrap();
        assert_eq!((s.status.as_str(), s.score, s.watched), ("watching", 8, 3));
        let s = api.set_status(5, &MyStatus { status: "completed".into(), score: 9, watched: 12 }).unwrap();
        assert_eq!(s.status, "completed");
        let put = http.requests().into_iter().find(|r| r.method == "PUT").unwrap();
        assert!(String::from_utf8_lossy(&put.body.as_ref().unwrap().1).contains("num_watched_episodes=12"));
        assert!(put.headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer tok"));
        let l = api.list().unwrap();
        assert_eq!((l[0].mal_id, l[0].episodes, l[0].progress), (5, 12, 3));
        let gone = Fake::new().route("my_list_status", 401, "{}");
        assert!(Api { http: &gone, token: "x".into() }.my_status(5).unwrap_err().contains("not logged in"));
    }

    #[test]
    fn genre_pages() {
        let http = Fake::new().route("/anime?genres=8", 200, r#"{"data":[{"mal_id":9,"title":"Drama One","images":{"jpg":{"large_image_url":"d.jpg"}},"score":8.5,"year":2019,"type":"TV"}],"pagination":{"has_next_page":true}}"#);
        let (cards, more) = Jikan { http: &http }.by_genre(8, 1).unwrap();
        assert!(more);
        assert_eq!((cards[0].id, cards[0].year), (9, 2019));
        let m = cards[0].to_meta();
        assert_eq!((m.id.as_str(), m.rating.as_str(), m.mal_id), ("mal:9", "8.5", Some(9)));
    }
}
