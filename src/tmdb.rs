//! TMDB on the title page, for movies and series. Anime always comes from MyAnimeList (see `mal`); TMDB is only asked
//! when MyAnimeList has no entry for the title. Everything is mapped onto the same shapes the MyAnimeList page uses
//! (`mal::Info`, characters, staff, recommendations, reviews), so one page shows either source.
use crate::http::{Http, Request};
use crate::json::Json;
use crate::mal::{Card, Character, Info, Named, Related, Relation, Review, Staff};

/// Everything TMDB knows about one title.
#[derive(Clone, Debug, Default)]
pub struct Bundle {
    pub info: Info,
    pub recs: Vec<Card>,
    pub reviews: Vec<Review>,
    pub characters: Vec<Character>,
    pub staff: Vec<Staff>,
}

const API: &str = "https://api.themoviedb.org/3";

/// A v3 key goes in the query, a long v4 token is sent as a bearer header (the same rule as the artwork lookup).
fn get(http: &dyn Http, key: &str, path: &str) -> Result<Json, String> {
    let key = key.trim();
    let sep = if path.contains('?') { '&' } else { '?' };
    let req = if key.len() > 40 {
        Request::get(&format!("{}{}", API, path)).header("Authorization", &format!("Bearer {}", key))
    } else {
        Request::get(&format!("{}{}{}api_key={}", API, path, sep, key))
    };
    let r = http.send(&req.header("Accept", "application/json").timeout(20))?;
    if !r.ok() {
        let why = r.json().ok().map(|j| j.get("status_message").str()).unwrap_or_default();
        return Err(if r.status == 401 { "TMDB refused the key (check it in Preferences > Metadata)".to_string() } else if why.is_empty() { format!("TMDB answered {}", r.status) } else { format!("TMDB answered {}: {}", r.status, why) });
    }
    r.json().map_err(|e| format!("TMDB did not return valid JSON ({})", e))
}

fn img(size: &str, path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    // Gravatar avatars come as "/https://..." in review authors.
    let p = path.trim_start_matches('/');
    if p.starts_with("http") {
        return p.to_string();
    }
    format!("https://image.tmdb.org/t/p/{}/{}", size, p)
}

fn year(date: &str) -> i64 {
    date.get(..4).and_then(|y| y.parse().ok()).unwrap_or(0)
}

/// The TMDB id and kind ("movie" or "tv") of an IMDb title. `prefer_movie` decides when both exist.
fn find(http: &dyn Http, key: &str, imdb: &str, prefer_movie: bool) -> Result<Option<(u64, &'static str)>, String> {
    let j = get(http, key, &format!("/find/{}?external_source=imdb_id", imdb))?;
    let movie = j.get("movie_results").at(0).get("id").as_u64().map(|i| (i, "movie"));
    let tv = j.get("tv_results").at(0).get("id").as_u64().map(|i| (i, "tv"));
    Ok(if prefer_movie { movie.or(tv) } else { tv.or(movie) })
}

fn imdb_of(http: &dyn Http, key: &str, media: &str, id: u64) -> Option<String> {
    let j = get(http, key, &format!("/{}/{}/external_ids", media, id)).ok()?;
    let s = j.get("imdb_id").str();
    if s.starts_with("tt") { Some(s) } else { None }
}

/// Looks the title up and loads its page. `Ok(None)`: TMDB does not know the IMDb id.
pub fn full(http: &dyn Http, key: &str, imdb: &str, kind: &str, lang: &str) -> Result<Option<Bundle>, String> {
    if key.trim().is_empty() || !imdb.starts_with("tt") {
        return Ok(None);
    }
    let Some((id, media)) = find(http, key, imdb, kind == "movie")? else { return Ok(None) };
    let extra = if media == "movie" { "release_dates" } else { "content_ratings" };
    let tag = locale(lang);
    // The text comes in the language of the app when TMDB has it; videos and reviews are asked without a language filter.
    let d = get(http, key, &format!("/{}/{}?language={}&include_video_language=en,null,{}&append_to_response=credits,reviews,recommendations,similar,videos,external_ids,keywords,watch/providers,{}", media, id, tag, util_lang(&tag), extra))?;
    let mut b = parse_region(&d, media, imdb, region_of(&tag));
    // A title not translated yet has no synopsis in that language: the English one fills the gap.
    if b.info.synopsis.is_empty() && tag != "en-US" {
        if let Ok(en) = get(http, key, &format!("/{}/{}?language=en-US", media, id)) {
            b.info.synopsis = en.get("overview").str();
            if b.info.background.is_empty() {
                b.info.background = en.get("tagline").str();
            }
            if b.info.title.is_empty() {
                b.info.title = if media == "movie" { en.get("title").str() } else { en.get("name").str() };
            }
        }
    }
    // Recommended and similar titles open in the app by their IMDb id, which TMDB gives one request at a time.
    let mut items: Vec<Card> = Vec::new();
    for list in [d.path("recommendations.results"), d.path("similar.results")] {
        for r in list.items() {
            let rid = r.get("id").as_u64().unwrap_or(0);
            if rid == 0 || rid == id || items.iter().any(|c| c.id == rid) {
                continue;
            }
            let title = if media == "movie" { r.get("title").str() } else { r.get("name").str() };
            let date = if media == "movie" { r.get("release_date").str() } else { r.get("first_air_date").str() };
            items.push(Card {
                id: rid,
                title,
                image: img("w342", &r.get("poster_path").str()),
                score: r.get("vote_average").as_f64().unwrap_or(0.0),
                kind: if media == "movie" { "movie".into() } else { "series".into() },
                year: year(&date),
                synopsis: r.get("overview").str(),
                ..Default::default()
            });
        }
    }
    items.truncate(18);
    let keys: Vec<Option<String>> = std::thread::scope(|s| {
        let handles: Vec<_> = items.iter().map(|c| s.spawn(move || imdb_of(http, key, media, c.id))).collect();
        handles.into_iter().map(|h| h.join().ok().flatten()).collect()
    });
    for (mut c, k) in items.into_iter().zip(keys) {
        if let Some(k) = k {
            c.key = k;
            b.recs.push(c);
        }
    }
    Ok(Some(b))
}

/// TMDB's language tag for the app language ("pt" is Brazilian here).
fn locale(lang: &str) -> String {
    match lang.split(['-', '_']).next().unwrap_or("en") {
        "pt" => "pt-BR".into(),
        "es" => "es-ES".into(),
        "fr" => "fr-FR".into(),
        "de" => "de-DE".into(),
        "it" => "it-IT".into(),
        "ja" => "ja-JP".into(),
        "ru" => "ru-RU".into(),
        _ => "en-US".into(),
    }
}

fn util_lang(tag: &str) -> String {
    tag.split('-').next().unwrap_or("en").to_string()
}

/// The country whose streaming services are listed.
fn region_of(tag: &str) -> &'static str {
    match tag {
        "pt-BR" => "BR",
        "es-ES" => "ES",
        "fr-FR" => "FR",
        "de-DE" => "DE",
        "it-IT" => "IT",
        "ja-JP" => "JP",
        "ru-RU" => "RU",
        _ => "US",
    }
}

fn language_name(code: &str) -> String {
    let n = match code {
        "en" => "English", "ja" => "Japanese", "ko" => "Korean", "zh" | "cn" => "Chinese", "es" => "Spanish", "fr" => "French", "de" => "German",
        "it" => "Italian", "pt" => "Portuguese", "ru" => "Russian", "hi" => "Hindi", "tr" => "Turkish", "ar" => "Arabic", "th" => "Thai",
        "sv" => "Swedish", "da" => "Danish", "no" | "nb" => "Norwegian", "fi" => "Finnish", "nl" => "Dutch", "pl" => "Polish", "id" => "Indonesian",
        _ => return code.to_uppercase(),
    };
    n.to_string()
}

fn money(v: i64) -> String {
    let t = v.to_string();
    let mut out = String::new();
    for (i, c) in t.chars().enumerate() {
        if i > 0 && (t.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("US$ {}", out)
}

fn names(j: &Json) -> Vec<Named> {
    j.items().iter().map(|x| Named { id: 0, name: x.get("name").str() }).filter(|n| !n.name.is_empty()).collect()
}

/// Turns TMDB's answer for a title (with the extra parts appended) into the page's data, without the recommendations.
pub fn parse(d: &Json, media: &str, imdb: &str) -> Bundle {
    parse_region(d, media, imdb, "US")
}

pub fn parse_region(d: &Json, media: &str, imdb: &str, region: &str) -> Bundle {
    let movie = media == "movie";
    let id = d.get("id").as_u64().unwrap_or(0);
    let title = if movie { d.get("title").str() } else { d.get("name").str() };
    let original = if movie { d.get("original_title").str() } else { d.get("original_name").str() };
    let first = if movie { d.get("release_date").str() } else { d.get("first_air_date").str() };
    let last = d.get("last_air_date").str();

    // Age rating: the American one when there is one.
    let mut rating = String::new();
    if movie {
        for c in d.path("release_dates.results").items() {
            if c.get("iso_3166_1").str() == "US" {
                rating = c.get("release_dates").items().iter().map(|r| r.get("certification").str()).find(|s| !s.is_empty()).unwrap_or_default();
            }
        }
    } else {
        for c in d.path("content_ratings.results").items() {
            if c.get("iso_3166_1").str() == "US" {
                rating = c.get("rating").str();
            }
        }
    }

    let trailer = {
        let vids = d.path("videos.results").items();
        let pick = vids.iter().find(|v| v.get("site").str() == "YouTube" && v.get("type").str() == "Trailer" && v.get("official").as_bool().unwrap_or(false)).or_else(|| vids.iter().find(|v| v.get("site").str() == "YouTube" && v.get("type").str() == "Trailer"));
        pick.map(|v| format!("https://www.youtube.com/watch?v={}", v.get("key").str())).unwrap_or_default()
    };

    let minutes = if movie { d.get("runtime").as_u64() } else { d.path("episode_run_time.0").as_u64().or_else(|| d.path("last_episode_to_air.runtime").as_u64()) };
    let duration = minutes.filter(|m| *m > 0).map(|m| format!("{} min", m)).unwrap_or_default();

    let mut external: Vec<(String, String)> = Vec::new();
    let home = d.get("homepage").str();
    if !home.is_empty() {
        external.push(("Official site".into(), home));
    }
    external.push(("TMDB".into(), format!("https://www.themoviedb.org/{}/{}", media, id)));
    // More videos besides the trailer: teasers, clips, featurettes (YouTube).
    let mut extra = 0;
    for v in d.path("videos.results").items() {
        let kind = v.get("type").str();
        if v.get("site").str() != "YouTube" || !["Teaser", "Clip", "Featurette", "Behind the Scenes", "Trailer"].contains(&kind.as_str()) {
            continue;
        }
        let url = format!("https://www.youtube.com/watch?v={}", v.get("key").str());
        if external.iter().any(|(_, u)| *u == url) || url == trailer {
            continue;
        }
        let name = v.get("name").str();
        external.push((format!("{}: {}", kind, if name.chars().count() > 38 { name.chars().take(38).collect::<String>() + "…" } else { name }), url));
        extra += 1;
        if extra >= 5 {
            break;
        }
    }
    let imdb_id = if imdb.is_empty() { d.path("external_ids.imdb_id").str() } else { imdb.to_string() };
    if imdb_id.starts_with("tt") {
        external.push(("IMDb".into(), format!("https://www.imdb.com/title/{}/", imdb_id)));
    }
    for (field, name, base) in [("facebook_id", "Facebook", "https://www.facebook.com/"), ("instagram_id", "Instagram", "https://www.instagram.com/"), ("twitter_id", "X", "https://x.com/")] {
        let v = d.path(&format!("external_ids.{}", field)).str();
        if !v.is_empty() {
            external.push((name.into(), format!("{}{}", base, v)));
        }
    }

    let crew = d.path("credits.crew").items();
    let producers: Vec<Named> = {
        let mut v: Vec<Named> = Vec::new();
        for c in crew.iter().filter(|c| c.get("job").str() == "Producer") {
            let n = c.get("name").str();
            if !n.is_empty() && !v.iter().any(|x| x.name == n) {
                v.push(Named { id: 0, name: n });
            }
            if v.len() >= 6 {
                break;
            }
        }
        v
    };

    let keywords = if movie { d.path("keywords.keywords") } else { d.path("keywords.results") };
    let collection = d.path("belongs_to_collection.name").str();
    let relations = if collection.is_empty() { vec![] } else { vec![Relation { kind: "Collection".into(), entries: vec![Related { id: 0, kind: "movie".into(), name: collection }] }] };
    let tagline = d.get("tagline").str();

    // Everything else TMDB knows, as label and value.
    let mut facts: Vec<(String, String)> = Vec::new();
    let mut fact = |k: &str, v: String| {
        if !v.trim().is_empty() {
            facts.push((k.to_string(), v));
        }
    };
    let ol = d.get("original_language").str();
    fact("Original language", if ol.is_empty() { String::new() } else { language_name(&ol) });
    let spoken: Vec<String> = d.get("spoken_languages").items().iter().map(|l| l.get("english_name").str()).filter(|n| !n.is_empty()).collect();
    fact("Spoken languages", spoken.join(", "));
    let countries: Vec<String> = {
        let pc: Vec<String> = d.get("production_countries").items().iter().map(|c| c.get("name").str()).filter(|n| !n.is_empty()).collect();
        if pc.is_empty() { d.get("origin_country").items().iter().map(|c| c.str()).collect() } else { pc }
    };
    fact("Countries", countries.join(", "));
    if movie {
        fact("Budget", d.get("budget").as_i64().filter(|v| *v > 0).map(money).unwrap_or_default());
        fact("Revenue", d.get("revenue").as_i64().filter(|v| *v > 0).map(money).unwrap_or_default());
        fact("Release date", first.clone());
        // theatrical release of the country shown first, when TMDB has it
        for c in d.path("release_dates.results").items() {
            if c.get("iso_3166_1").str() == region && region != "US" {
                let dt = c.get("release_dates").items().iter().map(|r| r.get("release_date").str()).find(|x| !x.is_empty()).unwrap_or_default();
                fact(&format!("Release in {}", region), dt.chars().take(10).collect());
            }
        }
    } else {
        fact("Type", d.get("type").str());
        fact("Seasons", d.get("number_of_seasons").as_u64().map(|n| n.to_string()).unwrap_or_default());
        fact("In production", if d.get("in_production").as_bool().unwrap_or(false) { "Yes".into() } else { String::new() });
        let creators: Vec<String> = d.get("created_by").items().iter().map(|c| c.get("name").str()).filter(|n| !n.is_empty()).collect();
        fact("Created by", creators.join(", "));
        let ep = |k: &str| {
            let e = d.get(k);
            if e.get("episode_number").as_u64().is_none() {
                return String::new();
            }
            let name = e.get("name").str();
            format!("S{:02}E{:02}{} · {}", e.get("season_number").as_u64().unwrap_or(0), e.get("episode_number").as_u64().unwrap_or(0), if name.is_empty() { String::new() } else { format!(" {}", name) }, e.get("air_date").str())
        };
        fact("Last episode", ep("last_episode_to_air"));
        fact("Next episode", ep("next_episode_to_air"));
        for sn in d.get("seasons").items() {
            let n = sn.get("season_number").as_u64().unwrap_or(0);
            if n == 0 {
                continue;
            }
            let eps = sn.get("episode_count").as_u64().unwrap_or(0);
            let date = sn.get("air_date").str();
            fact(&format!("Season {}", n), format!("{} episode{}{}", eps, if eps == 1 { "" } else { "s" }, if date.is_empty() { String::new() } else { format!(" · {}", date) }));
        }
    }
    fact("Original title", { let o = if movie { d.get("original_title").str() } else { d.get("original_name").str() }; let t = if movie { d.get("title").str() } else { d.get("name").str() }; if o == t { String::new() } else { o } });
    let facts = facts;

    // Where to watch (JustWatch data from TMDB), the country of the app language, else the American one.
    let mut streaming: Vec<(String, String)> = Vec::new();
    let prov = d.path("watch/providers.results");
    let country = [region, "US"].iter().map(|r| prov.get(r)).find(|c| matches!(c, Json::Obj(_)));
    if let Some(c) = country {
        let link = c.get("link").str();
        for kind in ["flatrate", "free", "ads", "rent", "buy"] {
            for p in c.get(kind).items() {
                let n = p.get("provider_name").str();
                let label = if kind == "flatrate" { n.clone() } else { format!("{} ({})", n, kind) };
                if !n.is_empty() && !streaming.iter().any(|(x, _)| *x == label) {
                    streaming.push((label, link.clone()));
                }
            }
        }
        streaming.truncate(14);
    }

    let info = Info {
        provider: "tmdb".into(),
        id,
        url: format!("https://www.themoviedb.org/{}/{}", media, id),
        title: title.clone(),
        title_en: title,
        title_ja: if original.is_empty() { String::new() } else { original },
        synonyms: vec![],
        image: img("w500", &d.get("poster_path").str()),
        trailer,
        kind: if movie { "Movie".into() } else { "TV series".into() },
        source: String::new(),
        episodes: if movie { None } else { d.get("number_of_episodes").as_u64().map(|n| n as u32) },
        status: d.get("status").str(),
        aired: if movie || last.is_empty() || last == first { first.clone() } else { format!("{} to {}", first, last) },
        premiered: if first.len() >= 4 { first[..4].to_string() } else { String::new() },
        broadcast: String::new(),
        duration,
        rating,
        score: d.get("vote_average").as_f64().unwrap_or(0.0),
        scored_by: d.get("vote_count").as_u64().unwrap_or(0),
        rank: 0,
        popularity: d.get("popularity").as_f64().map(|p| p.round() as u64).unwrap_or(0),
        members: 0,
        favorites: 0,
        synopsis: d.get("overview").str(),
        background: tagline,
        producers,
        licensors: names(d.get("networks")),
        studios: names(d.get("production_companies")),
        genres: names(d.get("genres")),
        themes: names(keywords).into_iter().take(12).collect(),
        demographics: vec![],
        openings: vec![],
        endings: vec![],
        streaming,
        external,
        facts,
        relations,
    };

    let characters: Vec<Character> = d
        .path("credits.cast")
        .items()
        .iter()
        .take(30)
        .enumerate()
        .map(|(i, c)| {
            let photo = img("w185", &c.get("profile_path").str());
            Character {
                id: c.get("id").as_u64().unwrap_or(0),
                name: { let n = c.get("character").str(); if n.is_empty() { c.get("name").str() } else { n } },
                role: if i < 8 { "Main".into() } else { "Supporting".into() },
                image: photo.clone(),
                actor: c.get("name").str(),
                actor_lang: String::new(),
                actor_image: photo,
            }
        })
        .collect();

    // The people behind the camera, the important jobs first.
    let jobs = ["Director", "Writer", "Screenplay", "Creator", "Novel", "Story", "Producer", "Executive Producer", "Original Music Composer", "Composer", "Director of Photography", "Editor"];
    let mut staff: Vec<(u64, Staff)> = Vec::new();
    for job in jobs {
        for c in crew.iter().filter(|c| c.get("job").str() == job) {
            let (pid, name) = (c.get("id").as_u64().unwrap_or(0), c.get("name").str());
            if name.is_empty() {
                continue;
            }
            match staff.iter_mut().find(|(i, s)| *i == pid && s.name == name) {
                Some((_, s)) => {
                    if !s.positions.split(", ").any(|p| p == job) {
                        s.positions = format!("{}, {}", s.positions, job);
                    }
                }
                None => staff.push((pid, Staff { name, image: img("w185", &c.get("profile_path").str()), positions: job.to_string() })),
            }
        }
    }
    let staff: Vec<Staff> = staff.into_iter().map(|(_, s)| s).take(24).collect();

    let reviews: Vec<Review> = d
        .path("reviews.results")
        .items()
        .iter()
        .map(|r| {
            let score = r.path("author_details.rating").as_f64().map(|v| v.round() as u32).unwrap_or(0);
            let user = r.get("author").str();
            Review {
                user,
                avatar: img("w92", &r.path("author_details.avatar_path").str()),
                date: r.get("created_at").str().chars().take(10).collect(),
                tag: if score == 0 { String::new() } else if score >= 7 { "Recommended".into() } else if score >= 5 { "Mixed Feelings".into() } else { "Not Recommended".into() },
                score,
                text: r.get("content").str(),
                spoiler: false,
                url: r.get("url").str(),
                reactions: 0,
            }
        })
        .collect();

    Bundle { info, recs: vec![], reviews, characters, staff }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    const MOVIE: &str = r#"{"id":27205,"title":"Inception","original_title":"Inception","release_date":"2010-07-15","runtime":148,"vote_average":8.36,"vote_count":35000,"popularity":52.4,"overview":"A thief.","tagline":"Your mind is the scene of the crime.","status":"Released","poster_path":"/p.jpg","genres":[{"id":28,"name":"Action"}],"production_companies":[{"id":1,"name":"Legendary"}],"belongs_to_collection":null,"homepage":"https://inception.example","external_ids":{"imdb_id":"tt1375666","instagram_id":"inception"},"keywords":{"keywords":[{"id":1,"name":"dream"}]},"release_dates":{"results":[{"iso_3166_1":"US","release_dates":[{"certification":""},{"certification":"PG-13"}]}]},"videos":{"results":[{"site":"YouTube","type":"Trailer","official":true,"key":"abc"}]},"credits":{"cast":[{"id":6193,"name":"Leonardo DiCaprio","character":"Cobb","profile_path":"/l.jpg"}],"crew":[{"id":525,"name":"Christopher Nolan","job":"Director","profile_path":"/n.jpg"},{"id":525,"name":"Christopher Nolan","job":"Writer"},{"id":9,"name":"Hans Zimmer","job":"Original Music Composer"}]},"reviews":{"results":[{"author":"ann","author_details":{"rating":9.0,"avatar_path":"/https://www.gravatar.com/avatar/x"},"content":"Great.","created_at":"2020-01-02T10:00:00.000Z","url":"https://www.themoviedb.org/review/1"}]}}"#;

    #[test]
    fn maps_a_movie() {
        let b = parse(&Json::parse(MOVIE).unwrap(), "movie", "tt1375666");
        assert_eq!(b.info.provider, "tmdb");
        assert_eq!(b.info.title, "Inception");
        assert_eq!(b.info.duration, "148 min");
        assert_eq!(b.info.rating, "PG-13");
        assert_eq!(b.info.premiered, "2010");
        assert_eq!(b.info.trailer, "https://www.youtube.com/watch?v=abc");
        assert_eq!(b.info.image, "https://image.tmdb.org/t/p/w500/p.jpg");
        assert!(b.info.external.iter().any(|(n, u)| n == "IMDb" && u.ends_with("tt1375666/")));
        assert_eq!(b.characters[0].name, "Cobb");
        assert_eq!(b.characters[0].actor, "Leonardo DiCaprio");
        assert_eq!(b.staff[0].name, "Christopher Nolan");
        assert_eq!(b.staff[0].positions, "Director, Writer");
        assert_eq!(b.reviews[0].tag, "Recommended");
        assert_eq!(b.reviews[0].avatar, "https://www.gravatar.com/avatar/x");
        assert_eq!(b.reviews[0].date, "2020-01-02");
    }

    #[test]
    fn unknown_id_is_not_found_and_no_key_is_skipped() {
        let http = Fake::new().route("/find/tt1?", 200, r#"{"movie_results":[],"tv_results":[]}"#);
        assert!(full(&http, "k", "tt1", "movie", "en").unwrap().is_none());
        assert!(full(&http, "", "tt1", "movie", "en").unwrap().is_none());
        assert!(full(&http, "k", "mal:5", "movie", "en").unwrap().is_none());
    }
}
