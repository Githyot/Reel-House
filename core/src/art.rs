//! Artwork and ratings. Cinemeta (the default metadata addon) already supplies poster, logo, background
//! and IMDb rating. When the user adds a TMDB API key, anything Cinemeta left out is filled from TMDB.

use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

pub const TMDB_API: &str = "https://api.themoviedb.org/3";
pub const TMDB_IMG: &str = "https://image.tmdb.org/t/p/original";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Art {
    pub poster: Option<String>,
    pub background: Option<String>,
    pub logo: Option<String>,
    pub rating: Option<String>,
}

#[derive(Deserialize)]
struct FindResult {
    #[serde(default)]
    movie_results: Vec<TmdbItem>,
    #[serde(default)]
    tv_results: Vec<TmdbItem>,
}

#[derive(Deserialize, Clone)]
struct TmdbItem {
    id: u64,
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    backdrop_path: Option<String>,
    #[serde(default)]
    vote_average: Option<f64>,
}

#[derive(Deserialize)]
struct Images {
    #[serde(default)]
    logos: Vec<Logo>,
}

#[derive(Deserialize)]
struct Logo {
    file_path: String,
    #[serde(default)]
    iso_639_1: Option<String>,
}

fn is_empty(v: Option<&Value>) -> bool {
    v.and_then(|x| x.as_str()).map_or(true, |s| s.trim().is_empty())
}

/// Fields of `meta` that artwork is still missing.
pub fn missing(meta: &Value) -> bool {
    ["poster", "background", "logo", "imdbRating"].iter().any(|k| is_empty(meta.get(*k)))
}

/// Copies artwork into `meta` without overwriting what the metadata addon already provided.
pub fn merge(meta: &mut Value, art: &Art) {
    let Some(obj) = meta.as_object_mut() else { return };
    let mut put = |key: &str, val: &Option<String>| {
        if let Some(v) = val {
            if is_empty(obj.get(key)) {
                obj.insert(key.to_string(), Value::String(v.clone()));
            }
        }
    };
    put("poster", &art.poster);
    put("background", &art.background);
    put("logo", &art.logo);
    put("imdbRating", &art.rating);
}

/// Looks the title up on TMDB by its IMDb id. `kind` is Stremio's "movie" or "series".
pub async fn tmdb_art(http: &reqwest::Client, api: &str, key: &str, imdb_id: &str, kind: &str) -> Option<Art> {
    if key.trim().is_empty() || !imdb_id.starts_with("tt") {
        return None;
    }
    let auth = |req: reqwest::RequestBuilder| {
        // A v4 "read access token" is a long JWT and goes in a header. A v3 key goes in the query string.
        if key.len() > 40 { req.bearer_auth(key) } else { req.query(&[("api_key", key)]) }
    };
    let find: FindResult = auth(http.get(format!("{api}/find/{imdb_id}")).query(&[("external_source", "imdb_id")]))
        .timeout(Duration::from_secs(10)).send().await.ok()?.json().await.ok()?;
    let (item, media) = if kind == "series" {
        (find.tv_results.first().cloned()?, "tv")
    } else {
        (find.movie_results.first().cloned()?, "movie")
    };
    let logo = match auth(http.get(format!("{api}/{media}/{}/images", item.id)).query(&[("include_image_language", "en,null")]))
        .timeout(Duration::from_secs(10)).send().await
    {
        Ok(r) => r.json::<Images>().await.ok().and_then(|i| {
            i.logos.iter().find(|l| l.iso_639_1.as_deref() == Some("en")).or(i.logos.first()).map(|l| format!("{TMDB_IMG}{}", l.file_path))
        }),
        Err(_) => None,
    };
    Some(Art {
        poster: item.poster_path.map(|p| format!("{TMDB_IMG}{p}")),
        background: item.backdrop_path.map(|p| format!("{TMDB_IMG}{p}")),
        logo,
        rating: item.vote_average.filter(|v| *v > 0.0).map(|v| format!("{v:.1}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    #[test]
    fn merge_fills_gaps_but_never_overwrites_cinemeta() {
        let mut meta = json!({"poster":"https://cinemeta/p.jpg","background":"","name":"X"});
        merge(&mut meta, &Art { poster: Some("https://tmdb/p.jpg".into()), background: Some("https://tmdb/b.jpg".into()), logo: Some("https://tmdb/l.png".into()), rating: Some("8.1".into()) });
        assert_eq!(meta["poster"], "https://cinemeta/p.jpg");
        assert_eq!(meta["background"], "https://tmdb/b.jpg");
        assert_eq!(meta["logo"], "https://tmdb/l.png");
        assert_eq!(meta["imdbRating"], "8.1");
        assert!(!missing(&meta));
        assert!(missing(&json!({"poster":"x"})));
    }

    #[tokio::test]
    async fn tmdb_lookup_returns_poster_backdrop_logo_and_rating() {
        let server = MockServer::start();
        server.mock(|w, t| { w.method(GET).path("/find/tt1160419").query_param("api_key", "k123").query_param("external_source", "imdb_id");
            t.status(200).json_body(json!({"movie_results":[{"id":693134,"poster_path":"/p.jpg","backdrop_path":"/b.jpg","vote_average":8.2}],"tv_results":[]})); });
        server.mock(|w, t| { w.method(GET).path("/movie/693134/images");
            t.status(200).json_body(json!({"logos":[{"file_path":"/fr.png","iso_639_1":"fr"},{"file_path":"/en.png","iso_639_1":"en"}]})); });
        let art = tmdb_art(&reqwest::Client::new(), &server.base_url(), "k123", "tt1160419", "movie").await.unwrap();
        assert_eq!(art.background.as_deref(), Some("https://image.tmdb.org/t/p/original/b.jpg"));
        assert_eq!(art.logo.as_deref(), Some("https://image.tmdb.org/t/p/original/en.png"));
        assert_eq!(art.rating.as_deref(), Some("8.2"));
    }

    #[tokio::test]
    async fn no_key_or_non_imdb_id_means_no_request() {
        let c = reqwest::Client::new();
        assert!(tmdb_art(&c, "http://127.0.0.1:1", "", "tt1", "movie").await.is_none());
        assert!(tmdb_art(&c, "http://127.0.0.1:1", "k", "kitsu:1", "series").await.is_none());
    }
}
