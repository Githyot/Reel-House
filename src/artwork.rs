//! Optional artwork and rating overrides from TMDB. Cinemeta is the default source of metadata.
use crate::http::{Http, Request};
use crate::json::Json;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Art {
    pub poster: Option<String>,
    pub background: Option<String>,
    pub rating: Option<String>,
}

/// Looks an IMDb id up on TMDB. A v3 key goes in the query. A long v4 token is sent as a bearer header.
pub fn tmdb_art(http: &dyn Http, key: &str, imdb_id: &str) -> Option<Art> {
    let key = key.trim();
    if key.is_empty() || !imdb_id.starts_with("tt") {
        return None;
    }
    let base = format!("https://api.themoviedb.org/3/find/{}?external_source=imdb_id", imdb_id);
    let req = if key.len() > 40 {
        Request::get(&base).header("Authorization", &format!("Bearer {}", key))
    } else {
        Request::get(&format!("{}&api_key={}", base, key))
    };
    let r = http.send(&req.timeout(15)).ok()?;
    if !r.ok() {
        return None;
    }
    let j = r.json().ok()?;
    let hit: &Json = [j.get("movie_results").at(0), j.get("tv_results").at(0)].into_iter().find(|x| !x.is_null())?;
    let img = |size: &str, field: &str| {
        let p = hit.get(field).str();
        if p.is_empty() { None } else { Some(format!("https://image.tmdb.org/t/p/{}{}", size, p)) }
    };
    let rating = hit.get("vote_average").as_f64().filter(|v| *v > 0.0).map(|v| format!("{:.1}", v));
    Some(Art { poster: img("w500", "poster_path"), background: img("w1280", "backdrop_path"), rating })
}

/// The IMDb id inside an image address, when there is one.
pub fn imdb_in(url: &str) -> Option<String> {
    let b = url.as_bytes();
    let mut i = 0;
    while i + 3 < b.len() {
        if b[i] == b't' && b[i + 1] == b't' && b[i + 2].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j - i >= 8 {
                return Some(url[i..j].to_string());
            }
        }
        i += 1;
    }
    None
}

/// "poster", "background" or "logo", judged from the address.
pub fn image_role(url: &str) -> &'static str {
    let u = url.to_lowercase();
    if u.contains("background") || u.contains("backdrop") || u.contains("fanart") {
        "background"
    } else if u.contains("logo") {
        "logo"
    } else {
        "poster"
    }
}

/// Other places for the same picture, tried in order when the first address does not give a usable image:
/// other sizes on Metahub, the picture Cinemeta lists, TMDB (with a key), OMDb (with a key).
pub fn alt_image(http: &dyn Http, url: &str, tmdb_key: &str, omdb_key: &str) -> Option<Vec<u8>> {
    let fetch = |u: &str| -> Option<Vec<u8>> {
        let r = http.send(&Request::get(u).timeout(25)).ok()?;
        if r.ok() && !r.body.is_empty() { Some(r.body) } else { None }
    };
    let imdb = imdb_in(url)?;
    let role = image_role(url);
    if url.contains("metahub.space") {
        for size in ["medium", "large", "small"] {
            let u = format!("https://images.metahub.space/{}/{}/{}/img", role, size, imdb);
            if u != url {
                if let Some(b) = fetch(&u) {
                    return Some(b);
                }
            }
        }
    }
    for kind in ["movie", "series"] {
        let m = format!("https://v3-cinemeta.strem.io/meta/{}/{}.json", kind, imdb);
        if let Ok(j) = crate::http::get_json(http, &m, 15) {
            let meta = j.get("meta");
            let field = if role == "background" { "background" } else if role == "logo" { "logo" } else { "poster" };
            let u = meta.get(field).str();
            if !u.is_empty() && u != url {
                if let Some(b) = fetch(&u) {
                    return Some(b);
                }
            }
        }
    }
    if let Some(a) = tmdb_art(http, tmdb_key, &imdb) {
        let u = if role == "background" { a.background } else { a.poster };
        if let Some(b) = u.and_then(|u| fetch(&u)) {
            return Some(b);
        }
    }
    if role == "poster" && !omdb_key.trim().is_empty() {
        return fetch(&format!("https://img.omdbapi.com/?i={}&apikey={}&h=600", imdb, omdb_key.trim()));
    }
    None
}

/// Basic title information from OMDb, for when no addon knows the title.
pub fn omdb_meta(http: &dyn Http, key: &str, imdb_id: &str, kind: &str) -> Option<crate::model::MetaItem> {
    if key.trim().is_empty() || !imdb_id.starts_with("tt") {
        return None;
    }
    let j = crate::http::get_json(http, &format!("https://www.omdbapi.com/?i={}&plot=full&apikey={}", imdb_id, key.trim()), 15).ok()?;
    if j.get("Response").str() == "False" {
        return None;
    }
    let na = |k: &str| {
        let v = j.get(k).str();
        if v == "N/A" { String::new() } else { v }
    };
    let mut m = crate::model::MetaItem::default();
    m.id = imdb_id.to_string();
    m.kind = kind.to_string();
    m.name = na("Title");
    if m.name.is_empty() {
        return None;
    }
    let poster = na("Poster");
    m.poster = if poster.is_empty() { None } else { Some(poster) };
    m.description = na("Plot");
    m.release_info = na("Year");
    m.runtime = na("Runtime");
    m.rating = na("imdbRating");
    m.genres = na("Genre").split(',').map(|g| g.trim().to_string()).filter(|g| !g.is_empty()).collect();
    m.cast = na("Actors").split(',').map(|g| g.trim().to_string()).filter(|g| !g.is_empty()).collect();
    Some(m)
}

// ----- covers found by title -----

/// The cover address in an AniList search answer.
pub fn anilist_cover(j: &Json) -> Option<String> {
    j.path("data.Page.media").items().iter().map(|m| {
        let big = m.path("coverImage.extraLarge").str();
        if big.is_empty() { m.path("coverImage.large").str() } else { big }
    }).find(|u| !u.is_empty())
}

/// The cover address in a Kitsu search answer.
pub fn kitsu_cover(j: &Json) -> Option<String> {
    j.get("data").items().iter().map(|d| {
        let p = d.path("attributes.posterImage");
        let m = p.get("medium").str();
        if m.is_empty() { p.get("large").str() } else { m }
    }).find(|u| !u.is_empty())
}

/// The cover address in a Jikan search answer.
pub fn jikan_cover(j: &Json) -> Option<String> {
    j.get("data").items().iter().map(|d| {
        let big = d.path("images.jpg.large_image_url").str();
        if big.is_empty() { d.path("images.jpg.image_url").str() } else { big }
    }).find(|u| !u.is_empty())
}

/// Looks a cover up by the name of the title when its own address does not give a picture, or when it has none:
/// AniList, then Kitsu, then Jikan, then the Cinemeta catalog. The first source that gives a picture wins.
pub fn title_cover(http: &dyn Http, title: &str) -> Option<Vec<u8>> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
    let fetch = |u: &str| -> Option<Vec<u8>> {
        let r = http.send(&Request::get(&crate::util::direct_image_url(u)).header("User-Agent", UA).header("Accept", "image/*").timeout(25)).ok()?;
        if r.ok() && !r.body.is_empty() { Some(r.body) } else { None }
    };
    let enc = crate::util::url_encode(title);
    let mut sources: Vec<Box<dyn Fn() -> Option<String> + '_>> = Vec::new();
    sources.push(Box::new(|| {
        let q = "query($q:String){Page(perPage:3){media(search:$q,type:ANIME,sort:SEARCH_MATCH){coverImage{extraLarge large}}}}";
        crate::anilist::query(http, q, vec![("q", Json::Str(title.to_string()))]).ok().and_then(|j| anilist_cover(&j))
    }));
    sources.push(Box::new(|| crate::http::get_json(http, &format!("https://kitsu.io/api/edge/anime?filter%5Btext%5D={}&page%5Blimit%5D=3", enc), 15).ok().and_then(|j| kitsu_cover(&j))));
    sources.push(Box::new(|| crate::http::get_json(http, &format!("https://api.jikan.moe/v4/anime?q={}&limit=3&sfw=false", enc), 12).ok().and_then(|j| jikan_cover(&j))));
    for kind in ["series", "movie"] {
        let enc = enc.clone();
        sources.push(Box::new(move || {
            let url = format!("https://v3-cinemeta.strem.io/catalog/{}/top/search={}.json", kind, enc);
            let j = crate::http::get_json(http, &url, 15).ok()?;
            j.get("metas").items().iter().map(|m| m.get("poster").str()).find(|p| !p.is_empty())
        }));
    }
    for src in sources {
        if let Some(u) = src() {
            if let Some(b) = fetch(&u) {
                return Some(b);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn finds_ids_in_addresses() {
        assert_eq!(imdb_in("https://images.metahub.space/poster/small/tt1234567/img").as_deref(), Some("tt1234567"));
        assert_eq!(imdb_in("https://x.org/attempt.jpg"), None);
        assert_eq!(image_role("https://images.metahub.space/background/medium/tt1234567/img"), "background");
    }

    #[test]
    fn reads_movie_and_tv_results() {
        let http = Fake::new().route("/find/tt1?", 200, r#"{"movie_results":[],"tv_results":[{"poster_path":"/p.jpg","backdrop_path":"/b.jpg","vote_average":8.14}]}"#);
        let a = tmdb_art(&http, "abc", "tt1").unwrap();
        assert_eq!(a.poster.as_deref(), Some("https://image.tmdb.org/t/p/w500/p.jpg"));
        assert_eq!(a.background.as_deref(), Some("https://image.tmdb.org/t/p/w1280/b.jpg"));
        assert_eq!(a.rating.as_deref(), Some("8.1"));
        assert!(http.requests()[0].url.contains("api_key=abc"));
    }

    #[test]
    fn uses_bearer_for_long_tokens_and_skips_bad_input() {
        let long = "x".repeat(60);
        let http = Fake::new().route("/find/", 200, r#"{"movie_results":[{"poster_path":"/p.jpg"}]}"#);
        assert!(tmdb_art(&http, &long, "tt9").is_some());
        assert!(http.requests()[0].headers.iter().any(|(k, _)| k == "Authorization"));
        assert!(tmdb_art(&http, "", "tt9").is_none());
        assert!(tmdb_art(&http, "k", "mal:5").is_none());
        let none = Fake::new().route("/find/", 200, r#"{"movie_results":[],"tv_results":[]}"#);
        assert!(tmdb_art(&none, "k", "tt1").is_none());
    }

    #[test]
    fn covers_from_each_source() {
        let a = Json::parse(r#"{"data":{"Page":{"media":[{"coverImage":{"extraLarge":"","large":"https://a/l.jpg"}}]}}}"#).unwrap();
        assert_eq!(anilist_cover(&a).as_deref(), Some("https://a/l.jpg"));
        let k = Json::parse(r#"{"data":[{"attributes":{"posterImage":{"medium":"https://k/m.jpg"}}}]}"#).unwrap();
        assert_eq!(kitsu_cover(&k).as_deref(), Some("https://k/m.jpg"));
        let j = Json::parse(r#"{"data":[{"images":{"jpg":{"large_image_url":"https://j/l.jpg"}}}]}"#).unwrap();
        assert_eq!(jikan_cover(&j).as_deref(), Some("https://j/l.jpg"));
        assert_eq!(kitsu_cover(&Json::parse("{}").unwrap()), None);
    }

    #[test]
    fn falls_through_the_sources() {
        let http = Fake::new()
            .route("graphql", 200, r#"{"data":{"Page":{"media":[]}}}"#)
            .route("kitsu.io", 500, "{}")
            .route("api.jikan.moe", 200, r#"{"data":[{"images":{"jpg":{"large_image_url":"https://img/p.jpg"}}}]}"#)
            .route("img/p.jpg", 200, "PIC");
        assert_eq!(title_cover(&http, "Steins;Gate").unwrap(), b"PIC".to_vec());
        assert!(title_cover(&http, " ").is_none());
    }
}
