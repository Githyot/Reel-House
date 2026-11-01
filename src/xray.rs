//! X-Ray (like Prime Video): who is in the episode, the songs, the scenes and trivia, each with the time range it
//! belongs to, built in the background when a video starts.
//!
//! Sources and what they really give:
//! - TMDB (needs the key of Preferences > Accounts and keys): the cast of the episode or film, with photos and
//!   characters, guest stars of the episode, crew facts. TMDB has no per-scene presence, so cast items span the whole
//!   video (`start_time` 0, `end_time` = duration) — except guest stars, which are marked as such.
//! - IMDb: there is no free API. Cinemeta (Stremio's metadata, which comes from IMDb) gives the main cast, directors,
//!   writers, awards, rating and trailers; the photos of the cast come from IMDb's public search suggestions.
//!   The trailers are kept apart (kind "trailer"): they are never listed as the opening or ending.
//! - MyAnimeList (through Jikan): characters with their Japanese and other voice actors, the opening and ending songs
//!   (timed with the Anime Skip / AniSkip segments of the episode, or chapters named like them), the background text
//!   and the episode notes (filler, recap) as trivia. AniList adds the native and other names of the characters, so
//!   Google Lens answers in any language can be matched (`lens_match`).
//! - An anime known only by its IMDb id still gets its songs: its MyAnimeList entry is searched by title and year,
//!   and when MyAnimeList lists no songs, the opening / ending videos are searched on YouTube (scored, see `themesong`).
//! - The video itself: its chapters (mpv) become scenes with their own start and end.
use crate::http::{get_json, Http};
use crate::json::Json;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    /// "cast", "music", "scene", "trivia"
    pub kind: String,
    pub start: f64,
    pub end: f64,
    pub name: String,
    /// character (cast), artist (music), source (trivia)
    pub role: String,
    /// voice actor(s) for anime characters
    pub voice: String,
    pub image: String,
    pub text: String,
    /// main, supporting, guest; "lens" for someone Google Lens recognised in the frame
    pub tag: String,
    /// Other names of a character (native script, nicknames from AniList), only to recognise them in a Google Lens
    /// answer; not sent to the interface.
    pub aliases: Vec<String>,
    /// Where more about this person is found (`person_info`): "mal-char:<id>", "tmdb-person:<id>" or
    /// "name:<name>" (Wikipedia); empty for the rows that are not people.
    pub link: String,
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    pub title: String,
    pub kind: String,
    pub imdb: Option<String>,
    pub tmdb_key: String,
    pub mal: Option<u64>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub duration: f64,
    /// (start, end, "op"/"ed") from Anime Skip / AniSkip
    pub songs_at: Vec<(f64, f64, String)>,
    /// chapters of the file: (start, title)
    pub chapters: Vec<(f64, String)>,
    /// the language for the second voice actor ("Portuguese (BR)", "English"...)
    pub dub_lang: String,
    /// where the cast comes from: "auto" (MyAnimeList for anime, TMDB for the rest), "tmdb", "mal" or "imdb"
    pub source: String,
    /// The title is an anime (a mal: / kitsu: id, AniSkip segments, an "Animation" genre): its openings and endings
    /// are looked up whatever the source of the cast.
    pub anime: bool,
    /// The year the title started (the catalogue's release info), to pick the right MyAnimeList entry by name.
    pub year: Option<i64>,
    /// The data folder (where yt-dlp lives). With it, an anime whose songs MyAnimeList does not list gets its opening
    /// and ending found on YouTube; None skips that search (the tests).
    pub data_dir: Option<std::path::PathBuf>,
}

pub struct XRay {
    pub items: Vec<Item>,
    pub sources: Vec<String>,
    pub notes: Vec<String>,
}

fn img_tmdb(path: &str, size: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("https://image.tmdb.org/t/p/{}{}", size, path)
    }
}

/// TMDB v3 with an API key or a v4 read token (the same field in the settings).
fn tmdb(http: &dyn Http, key: &str, path: &str) -> Result<Json, String> {
    let sep = if path.contains('?') { '&' } else { '?' };
    if key.len() > 60 {
        let req = crate::http::Request::get(&format!("https://api.themoviedb.org/3{}", path)).header("Authorization", &format!("Bearer {}", key)).timeout(20);
        return http.send(&req)?.json();
    }
    get_json(http, &format!("https://api.themoviedb.org/3{}{}api_key={}", path, sep, key), 20)
}

fn tmdb_cast(http: &dyn Http, inp: &Input, out: &mut XRay) -> Result<(), String> {
    let imdb = inp.imdb.as_deref().ok_or("no IMDb id")?;
    let found = tmdb(http, &inp.tmdb_key, &format!("/find/{}?external_source=imdb_id", imdb))?;
    let end = if inp.duration > 0.0 { inp.duration } else { f64::MAX };
    let push_cast = |out: &mut XRay, list: &[Json], tag: &str| {
        for c in list.iter().take(24) {
            out.items.push(Item {
                kind: "cast".into(),
                start: 0.0,
                end,
                name: c.get("name").str(),
                role: c.get("character").str(),
                image: img_tmdb(&c.get("profile_path").str(), "w185"),
                tag: tag.into(),
                link: format!("tmdb-person:{}", c.get("id").as_i64().unwrap_or(0)),
                ..Item::default()
            });
        }
    };
    if let Some(tv) = found.get("tv_results").items().first() {
        let id = tv.get("id").as_i64().unwrap_or(0);
        if let (Some(s), Some(e)) = (inp.season, inp.episode) {
            let ep = tmdb(http, &inp.tmdb_key, &format!("/tv/{}/season/{}/episode/{}?append_to_response=credits", id, s, e))?;
            let credits = ep.get("credits");
            push_cast(out, credits.get("cast").items(), "main");
            push_cast(out, credits.get("guest_stars").items(), "guest");
            for c in ep.get("crew").items().iter().filter(|c| matches!(c.get("job").str().as_str(), "Director" | "Writer" | "Screenplay")).take(4) {
                out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: format!("{}: {}", c.get("job").str(), c.get("name").str()), role: "TMDB".into(), ..Item::default() });
            }
            let ov = ep.get("overview").str();
            if !ov.is_empty() {
                out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: ep.get("name").str(), text: ov, role: "TMDB".into(), ..Item::default() });
            }
        } else {
            let cr = tmdb(http, &inp.tmdb_key, &format!("/tv/{}/aggregate_credits", id))?;
            for c in cr.get("cast").items().iter().take(24) {
                let role = c.get("roles").items().first().map(|r| r.get("character").str()).unwrap_or_default();
                out.items.push(Item { kind: "cast".into(), start: 0.0, end, name: c.get("name").str(), role, image: img_tmdb(&c.get("profile_path").str(), "w185"), tag: "main".into(), link: format!("tmdb-person:{}", c.get("id").as_i64().unwrap_or(0)), ..Item::default() });
            }
        }
        out.sources.push("TMDB".into());
        return Ok(());
    }
    if let Some(mv) = found.get("movie_results").items().first() {
        let id = mv.get("id").as_i64().unwrap_or(0);
        let m = tmdb(http, &inp.tmdb_key, &format!("/movie/{}?append_to_response=credits", id))?;
        push_cast(out, m.get("credits").get("cast").items(), "main");
        let money = |v: f64| if v >= 1e6 { format!("US$ {:.0} million", v / 1e6) } else { format!("US$ {:.0}", v) };
        let (b, r) = (m.get("budget").as_f64().unwrap_or(0.0), m.get("revenue").as_f64().unwrap_or(0.0));
        if b > 0.0 {
            out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: format!("Budget: {}", money(b)), role: "TMDB".into(), ..Item::default() });
        }
        if r > 0.0 {
            out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: format!("Box office: {}", money(r)), role: "TMDB".into(), ..Item::default() });
        }
        let tag = m.get("tagline").str();
        if !tag.is_empty() {
            out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: tag, role: "TMDB".into(), tag: "tagline".into(), ..Item::default() });
        }
        for c in m.get("credits").get("crew").items().iter().filter(|c| matches!(c.get("job").str().as_str(), "Director" | "Original Music Composer" | "Screenplay")).take(5) {
            out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: format!("{}: {}", c.get("job").str(), c.get("name").str()), role: "TMDB".into(), ..Item::default() });
        }
        out.sources.push("TMDB".into());
        return Ok(());
    }
    Err("TMDB does not know this IMDb id".into())
}

/// One Jikan (MyAnimeList) request through `mal::Jikan`: spaced out to Jikan's limit (3 a second), retried after a
/// 429 "too many requests", and skipped at once while Jikan is known to be down. The X-Ray asks Jikan four or five
/// times in a row while the player asks it for the skip times and the songs: without the spacing the burst got 429
/// answers and the MyAnimeList source came back empty.
fn jikan(http: &dyn Http, path: &str) -> Result<Json, String> {
    if crate::mal::jikan_down() {
        return Err("MyAnimeList (Jikan) is not answering right now.".into());
    }
    crate::mal::Jikan { http }.get(path)
}

/// The AniList name of a dub language ("Portuguese" -> "PORTUGUESE"), for the second voice actor.
fn anilist_lang(dub: &str) -> &'static str {
    match dub.to_lowercase().as_str() {
        l if l.starts_with("portuguese") => "PORTUGUESE",
        l if l.starts_with("spanish") => "SPANISH",
        l if l.starts_with("french") => "FRENCH",
        l if l.starts_with("german") => "GERMAN",
        l if l.starts_with("italian") => "ITALIAN",
        _ => "ENGLISH",
    }
}

/// The characters of a MyAnimeList entry from AniList (looked up by the MyAnimeList id), for when Jikan does not
/// answer (busy, down, or blocked on the viewer's network). Same rows as `mal_data` makes from Jikan.
fn anilist_cast(http: &dyn Http, inp: &Input, mal: u64, end: f64) -> Result<Vec<Item>, String> {
    let q = format!(
        "query($id:Int){{Media(idMal:$id,type:ANIME){{characters(sort:[ROLE,RELEVANCE,ID],perPage:30){{edges{{role node{{name{{full}} image{{large}}}} ja:voiceActors(language:JAPANESE){{name{{full}}}} dub:voiceActors(language:{}){{name{{full}}}}}}}}}}}}",
        anilist_lang(&inp.dub_lang)
    );
    let j = crate::anilist::query(http, &q, vec![("id", Json::Num(mal as f64))])?;
    let edges = j.path("data.Media.characters.edges");
    if edges.items().is_empty() {
        return Err("AniList has no characters for this entry.".into());
    }
    Ok(edges
        .items()
        .iter()
        .map(|e| {
            let role = match e.get("role").str().as_str() {
                "MAIN" => "Main",
                "SUPPORTING" => "Supporting",
                _ => "Background",
            };
            let ja = e.path("ja.0.name.full").str();
            let dub = e.path("dub.0.name.full").str();
            let dub = if dub.is_empty() || inp.dub_lang.is_empty() { String::new() } else { format!("{} ({})", dub, inp.dub_lang) };
            let name = e.path("node.name.full").str();
            Item {
                kind: "cast".into(),
                start: 0.0,
                end,
                voice: [ja, dub].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "),
                image: e.path("node.image.large").str(),
                tag: if role == "Main" { "main".into() } else { "supporting".into() },
                link: format!("name:{}", name),
                name,
                role: role.into(),
                ..Item::default()
            }
        })
        .collect())
}

fn mal_data(http: &dyn Http, inp: &Input, mal: u64, out: &mut XRay) -> Result<(), String> {
    let end = if inp.duration > 0.0 { inp.duration } else { f64::MAX };
    let ch = match jikan(http, &format!("/anime/{}/characters", mal)) {
        Ok(ch) => ch,
        Err(e) => {
            // Jikan failed: the characters from AniList, and the songs when Jikan answers them later
            let cast = anilist_cast(http, inp, mal, end).map_err(|e2| format!("{} (AniList: {})", e, e2))?;
            crate::debug::log("xray", &format!("MyAnimeList characters from AniList ({} rows): Jikan said {}", cast.len(), e));
            out.items.extend(cast);
            add_aliases(http, mal, &mut out.items);
            if let Ok(themes) = crate::themesong::themes(http, mal) {
                push_themes(inp, &themes, out);
            }
            out.sources.push("MyAnimeList (AniList)".into());
            return Ok(());
        }
    };
    let mut list: Vec<&Json> = ch.get("data").items().iter().collect();
    // main characters first, then the most liked
    list.sort_by_key(|c| (c.get("role").str() != "Main", -(c.get("favorites").as_i64().unwrap_or(0))));
    for c in list.into_iter().take(30) {
        let vas = c.get("voice_actors").items();
        let ja = vas.iter().find(|v| v.get("language").str() == "Japanese").map(|v| v.get("person").get("name").str()).unwrap_or_default();
        let dub = vas
            .iter()
            .find(|v| !inp.dub_lang.is_empty() && v.get("language").str().to_lowercase().starts_with(&inp.dub_lang.to_lowercase()))
            .map(|v| format!("{} ({})", v.get("person").get("name").str(), v.get("language").str()))
            .unwrap_or_default();
        let voice = [ja, dub].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
        out.items.push(Item {
            kind: "cast".into(),
            start: 0.0,
            end,
            name: c.get("character").get("name").str(),
            role: c.get("role").str(),
            voice,
            image: c.get("character").get("images").get("jpg").get("image_url").str(),
            tag: if c.get("role").str() == "Main" { "main".into() } else { "supporting".into() },
            link: format!("mal-char:{}", c.get("character").get("mal_id").as_i64().unwrap_or(0)),
            ..Item::default()
        });
    }
    // the other names of the characters (Japanese script, nicknames), for Google Lens answers
    add_aliases(http, mal, &mut out.items);
    // the songs: timed with the opening / ending segments; without them, untimed
    if let Ok(themes) = crate::themesong::themes(http, mal) {
        push_themes(inp, &themes, out);
    }
    // trivia: the background of the entry, sentence by sentence, and the notes of the episode
    if let Ok(full) = jikan(http, &format!("/anime/{}", mal)) {
        let bg = full.get("data").get("background").str();
        for s in bg.split(". ").map(|x| x.trim()).filter(|x| x.len() > 30).take(6) {
            let s = if s.ends_with('.') { s.to_string() } else { format!("{}.", s) };
            out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: s, role: "MyAnimeList".into(), ..Item::default() });
        }
        let d = full.get("data");
        let studios: Vec<String> = d.get("studios").items().iter().map(|x| x.get("name").str()).collect();
        if !studios.is_empty() {
            out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: format!("Studio: {}", studios.join(", ")), role: "MyAnimeList".into(), ..Item::default() });
        }
    }
    if let Some(e) = inp.episode.filter(|e| *e > 0) {
        if let Ok(ep) = jikan(http, &format!("/anime/{}/episodes/{}", mal, e)) {
            let d = ep.get("data");
            if d.get("filler").as_bool() == Some(true) {
                out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: "This episode is filler (not from the original story).".into(), role: "MyAnimeList".into(), ..Item::default() });
            }
            if d.get("recap").as_bool() == Some(true) {
                out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: "This episode is a recap.".into(), role: "MyAnimeList".into(), ..Item::default() });
            }
            let t = d.get("title_japanese").str();
            if !t.is_empty() {
                out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name: format!("Japanese title: {}", t), role: "MyAnimeList".into(), ..Item::default() });
            }
        }
    }
    out.sources.push("MyAnimeList".into());
    Ok(())
}

/// An IMDb picture address at a small width (the original files are several thousand pixels wide).
fn imdb_small(url: &str) -> String {
    match url.rfind("._V1_") {
        Some(p) if url.ends_with(".jpg") => format!("{}._V1_UX190_.jpg", &url[..p]),
        _ => url.to_string(),
    }
}

/// The photo of a person, from IMDb's search suggestions (the box under its search field): the first person found
/// with that exact name.
fn imdb_photo(http: &dyn Http, name: &str) -> String {
    let url = format!("https://v3.sg.media-imdb.com/suggestion/x/{}.json", crate::util::url_encode(name));
    let Ok(j) = get_json(http, &url, 15) else { return String::new() };
    let found = j.get("d").items().iter().find(|d| d.get("id").str().starts_with("nm") && d.get("l").str().eq_ignore_ascii_case(name)).cloned();
    found.map(|d| imdb_small(&d.get("i").get("imageUrl").str())).unwrap_or_default()
}

/// IMDb through Cinemeta: the main cast (with photos), directors, writers, awards, rating, the trailers and the
/// summary of the episode.
fn imdb_data(http: &dyn Http, inp: &Input, out: &mut XRay) -> Result<(), String> {
    let imdb = inp.imdb.as_deref().ok_or("this video has no IMDb id")?;
    let end = if inp.duration > 0.0 { inp.duration } else { f64::MAX };
    // the kind can be wrong (an anime catalogue says "series" for a film): the other one is tried too
    let kinds: [&str; 2] = if inp.kind == "movie" { ["movie", "series"] } else { ["series", "movie"] };
    let mut meta = Json::Null;
    for k in kinds {
        if let Ok(j) = get_json(http, &format!("https://v3-cinemeta.strem.io/meta/{}/{}.json", k, imdb), 20) {
            if !j.get("meta").get("name").str().is_empty() {
                meta = j.get("meta").clone();
                break;
            }
        }
    }
    if meta.get("name").str().is_empty() {
        return Err("IMDb (Cinemeta) does not know this title".into());
    }
    // the photos are looked up all at once: one request per person, a few seconds in total instead of one by one
    let cast: Vec<String> = meta.get("cast").strings().into_iter().take(12).collect();
    let photos: Vec<String> = std::thread::scope(|sc| {
        let hs: Vec<_> = cast.iter().map(|n| sc.spawn(move || imdb_photo(http, n))).collect();
        hs.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    for (name, image) in cast.iter().zip(photos) {
        out.items.push(Item { kind: "cast".into(), start: 0.0, end, name: name.clone(), image, tag: "main".into(), link: format!("name:{}", name), ..Item::default() });
    }
    let fact = |out: &mut XRay, name: String, text: String| {
        out.items.push(Item { kind: "trivia".into(), start: 0.0, end, name, text, role: "IMDb".into(), ..Item::default() });
    };
    // the episode playing: its title and summary
    if let (Some(s), Some(e)) = (inp.season, inp.episode) {
        if let Some(v) = meta.get("videos").items().iter().find(|v| v.get("season").as_i64() == Some(s) && (v.get("episode").as_i64() == Some(e) || v.get("number").as_i64() == Some(e))) {
            let title = [v.get("name").str(), v.get("title").str()].into_iter().find(|x| !x.is_empty()).unwrap_or_default();
            let ov = [v.get("overview").str(), v.get("description").str()].into_iter().find(|x| !x.is_empty()).unwrap_or_default();
            if !title.is_empty() || !ov.is_empty() {
                fact(out, if title.is_empty() { format!("S{}E{}", s, e) } else { title }, ov);
            }
        }
    }
    let list = |k: &str| meta.get(k).strings().join(", ");
    for (label, k) in [("Director", "director"), ("Writer", "writer")] {
        let v = list(k);
        if !v.is_empty() {
            fact(out, format!("{}: {}", label, v), String::new());
        }
    }
    for (label, k) in [("IMDb rating", "imdbRating"), ("Awards", "awards"), ("Country", "country"), ("Released", "releaseInfo")] {
        let v = meta.get(k).str();
        if !v.trim().is_empty() {
            fact(out, format!("{}: {}", label, v.trim()), String::new());
        }
    }
    let genres = meta.get("genres").strings();
    if !genres.is_empty() {
        fact(out, format!("Genres: {}", genres.join(", ")), String::new());
    }
    // trailers on YouTube: a list of their own (never the opening or ending); they play in the X-Ray like the songs
    // (the address goes in `text`)
    for (i, t) in meta.get("trailers").items().iter().filter(|t| !t.get("source").str().is_empty()).take(3).enumerate() {
        out.items.push(Item {
            kind: "trailer".into(),
            start: 0.0,
            end: f64::INFINITY,
            name: format!("{} — {} {}", meta.get("name").str(), t.get("type").as_str().filter(|x| !x.is_empty()).unwrap_or("Trailer"), i + 1),
            role: "YouTube".into(),
            text: format!("https://www.youtube.com/watch?v={}", t.get("source").str()),
            tag: "Trailer".into(),
            ..Item::default()
        });
    }
    out.sources.push("IMDb".into());
    Ok(())
}

/// Builds the X-Ray of a video. Each source that fails is noted and the others still count.
pub fn build(http: &dyn Http, inp: &Input) -> XRay {
    let mut out = XRay { items: vec![], sources: vec![], notes: vec![] };
    // scenes: the chapters of the file
    for (i, (start, title)) in inp.chapters.iter().enumerate() {
        let end = inp.chapters.get(i + 1).map(|c| c.0).unwrap_or(if inp.duration > 0.0 { inp.duration } else { f64::MAX });
        let name = if title.trim().is_empty() { format!("Chapter {}", i + 1) } else { title.trim().to_string() };
        out.items.push(Item { kind: "scene".into(), start: *start, end, name, ..Item::default() });
    }
    gather(http, inp, &mut out);
    // an anime gets its opening and ending whatever source gave the cast (IMDb and TMDB know no songs)
    if (inp.anime || inp.mal.is_some()) && !out.items.iter().any(|i| i.kind == "music") {
        anime_songs(http, inp, &mut out);
    }
    out
}

/// The cast, trivia and (MyAnimeList) songs from the source chosen, or from the best ones for the title.
fn gather(http: &dyn Http, inp: &Input, out: &mut XRay) {
    // a source chosen in the X-Ray: only that one
    match inp.source.as_str() {
        "imdb" => {
            if let Err(e) = imdb_data(http, inp, out) {
                out.notes.push(format!("IMDb: {}", e));
            }
            return;
        }
        "mal" => {
            match inp.mal {
                Some(mal) => {
                    if let Err(e) = mal_data(http, inp, mal, out) {
                        out.notes.push(format!("MyAnimeList: {}", e));
                    }
                }
                // not found before the build (AniList, then Jikan by popularity): Jikan's search by title and year
                None => match find_mal(http, &inp.title, inp.year, inp.season) {
                    Some(mal) => {
                        if let Err(e) = mal_data(http, inp, mal, out) {
                            out.notes.push(format!("MyAnimeList: {}", e));
                        }
                    }
                    None => out.notes.push("MyAnimeList: this title was not found on MyAnimeList.".into()),
                },
            }
            return;
        }
        "tmdb" => {
            if inp.tmdb_key.trim().is_empty() {
                out.notes.push("Add a TMDB key (Preferences > Accounts and keys) for the cast of films and series.".into());
            } else if let Err(e) = tmdb_cast(http, inp, out) {
                out.notes.push(format!("TMDB: {}", e));
            }
            return;
        }
        _ => {}
    }
    if let Some(mal) = inp.mal {
        if let Err(e) = mal_data(http, inp, mal, out) {
            out.notes.push(format!("MyAnimeList: {}", e));
        }
    }
    let has_cast = out.items.iter().any(|i| i.kind == "cast");
    if !inp.tmdb_key.trim().is_empty() && inp.imdb.is_some() {
        // for anime the MyAnimeList characters come first; TMDB adds the actors of live action
        if !has_cast || inp.mal.is_none() {
            if let Err(e) = tmdb_cast(http, inp, out) {
                out.notes.push(format!("TMDB: {}", e));
            }
        }
    }
    // no cast yet (no TMDB key, or TMDB failed): IMDb through Cinemeta, which needs no key
    if !out.items.iter().any(|i| i.kind == "cast") && inp.imdb.is_some() {
        if let Err(e) = imdb_data(http, inp, out) {
            out.notes.push(format!("IMDb: {}", e));
        }
    }
    if !out.items.iter().any(|i| i.kind == "cast") && inp.tmdb_key.trim().is_empty() && inp.mal.is_none() {
        out.notes.push("Add a TMDB key (Preferences > Accounts and keys) for the cast of films and series.".into());
    }
}

/// "op" / "ed" for a chapter named like an opening or ending ("Opening", "OP2", "NCED", "Ending Credits",
/// "オープニング"); None for the others. Whole words only: "Operation" or "Edward" are not songs.
pub fn chapter_kind(title: &str) -> Option<&'static str> {
    let l = title.to_lowercase();
    if l.contains("オープニング") {
        return Some("op");
    }
    if l.contains("エンディング") {
        return Some("ed");
    }
    let ws: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    // "op", "op2", "ncop", "ncop1" (the same for ed)
    let short = |w: &str, base: &str| {
        let w = w.strip_prefix("nc").unwrap_or(w);
        w.strip_prefix(base).map(|rest| rest.chars().all(|c| c.is_ascii_digit())).unwrap_or(false)
    };
    if ws.iter().any(|w| matches!(*w, "opening" | "abertura" | "apertura") || short(w, "op")) {
        return Some("op");
    }
    if ws.iter().any(|w| matches!(*w, "ending" | "credits" | "encerramento" | "cierre") || short(w, "ed")) {
        return Some("ed");
    }
    None
}

/// The (start, end) of the opening or ending (`kind`) in this episode: the Anime Skip / AniSkip segments first, else
/// the chapters named like it. Empty when nothing says where the song is.
fn song_ranges(inp: &Input, kind: &str) -> Vec<(f64, f64)> {
    let at: Vec<(f64, f64)> = inp.songs_at.iter().filter(|x| x.2 == kind).map(|x| (x.0, x.1)).collect();
    if !at.is_empty() {
        return at;
    }
    let end_of = |i: usize| inp.chapters.get(i + 1).map(|c| c.0).unwrap_or(if inp.duration > 0.0 { inp.duration } else { f64::INFINITY });
    inp.chapters.iter().enumerate().filter(|(_, c)| chapter_kind(&c.1) == Some(kind)).map(|(i, c)| (c.0, end_of(i))).collect()
}

/// The opening and ending of this episode from MyAnimeList's list of songs, with their number ("Opening 2"), timed
/// with `song_ranges`, or untimed (`end` infinite: listed, never "playing now") when nothing says where they are.
/// Gives how many were added.
fn push_themes(inp: &Input, themes: &[crate::themesong::Theme], out: &mut XRay) -> usize {
    let mut n = 0;
    for (kind, label) in [("op", "Opening"), ("ed", "Ending")] {
        let Some(t) = crate::themesong::pick(themes, kind, inp.episode) else { continue };
        let tag = format!("{}{}", label, if t.number > 0 { format!(" {}", t.number) } else { String::new() });
        let mut ranges = song_ranges(inp, kind);
        if ranges.is_empty() {
            ranges.push((0.0, f64::INFINITY));
        }
        for (a, b) in ranges {
            out.items.push(Item { kind: "music".into(), start: a, end: b, name: t.song.clone(), role: t.artist.clone(), tag: tag.clone(), ..Item::default() });
            n += 1;
        }
    }
    n
}

/// The first year written in a release info ("2016–", "2016-2020", "2016").
pub fn year_of(release: &str) -> Option<i64> {
    let digits: String = release.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok().filter(|y| (1900..2200).contains(y))
}

/// The MyAnimeList id of an anime known only by its title (an IMDb-only catalogue): Jikan's search, the entry
/// named like the title that started in `year` (for a later season, one named like it that started after).
pub fn find_mal(http: &dyn Http, title: &str, year: Option<i64>, season: Option<i64>) -> Option<u64> {
    let later = season.filter(|s| *s > 1);
    let q = match later {
        Some(s) => format!("{} season {}", title, s),
        None => title.to_string(),
    };
    let j = jikan(http, &format!("/anime?q={}&limit=10", crate::util::url_encode(&q))).ok()?;
    pick_mal(j.get("data").items(), title, year, later)
}

/// The choice of `find_mal` among Jikan's answers.
fn pick_mal(list: &[Json], title: &str, year: Option<i64>, later: Option<i64>) -> Option<u64> {
    let want = crate::malmatch::normalize(title);
    if want.len() < 2 {
        return None;
    }
    let named = |a: &Json| {
        let mut names = vec![a.get("title").str(), a.get("title_english").str()];
        names.extend(a.get("titles").items().iter().map(|t| t.get("title").str()));
        names.iter().map(|n| crate::malmatch::normalize(n)).any(|n| !n.is_empty() && (n.contains(&want) || (n.len() >= 4 && want.contains(&n))))
    };
    let started = |a: &Json| a.get("year").as_i64().or_else(|| a.path("aired.prop.from.year").as_i64());
    let id = |a: &Json| a.get("mal_id").as_u64();
    let fits_year = |a: &Json| match (year, started(a)) {
        (Some(y), Some(s)) => if later.is_some() { s > y } else { s == y },
        _ => false,
    };
    list.iter().find(|a| named(a) && fits_year(a)).or_else(|| list.iter().find(|a| named(a))).and_then(id)
}

/// The songs of an anime whose source gave none (IMDb, TMDB, or MyAnimeList without a song list): MyAnimeList's
/// list when its entry can be found, else the opening / ending videos found on YouTube (the search is scored so a
/// trailer is never taken for the song). Nothing is added when neither works.
fn anime_songs(http: &dyn Http, inp: &Input, out: &mut XRay) {
    let mal = inp.mal.or_else(|| find_mal(http, &inp.title, inp.year, inp.season));
    if let Some(mal) = mal {
        if let Ok(themes) = crate::themesong::themes(http, mal) {
            if push_themes(inp, &themes, out) > 0 {
                if !out.sources.iter().any(|s| s == "MyAnimeList") {
                    out.sources.push("MyAnimeList".into());
                }
                return;
            }
        }
    }
    let Some(data) = inp.data_dir.as_deref() else { return };
    if inp.title.trim().is_empty() {
        return;
    }
    let ytdlp = match crate::themesong::ensure_ytdlp(http, data) {
        Ok(p) => p,
        Err(e) => {
            out.notes.push(format!("YouTube: {}", e));
            return;
        }
    };
    // "Re:Zero season 4 opening": the later seasons have their own songs
    let show = match inp.season.filter(|s| *s > 1) {
        Some(s) => format!("{} season {}", inp.title, s),
        None => inp.title.clone(),
    };
    // both searches at once (each one is a few yt-dlp runs)
    let found: Vec<(&str, Result<crate::themesong::Video, String>)> = std::thread::scope(|sc| {
        let hs: Vec<_> = ["op", "ed"]
            .into_iter()
            .map(|kind| {
                let (ytdlp, show) = (&ytdlp, &show);
                sc.spawn(move || {
                    let t = crate::themesong::Theme { kind: kind.to_string(), ..Default::default() };
                    (kind, crate::themesong::find_video(ytdlp, show, &t))
                })
            })
            .collect();
        hs.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    for (kind, r) in found {
        let Ok(v) = r else { continue };
        let mut ranges = song_ranges(inp, kind);
        if ranges.is_empty() {
            ranges.push((0.0, f64::INFINITY));
        }
        for (a, b) in ranges {
            out.items.push(Item {
                kind: "music".into(),
                start: a,
                end: b,
                name: v.title.clone(),
                role: v.channel.clone(),
                text: v.url(),
                tag: if kind == "op" { "Opening".into() } else { "Ending".into() },
                ..Item::default()
            });
        }
    }
    if out.items.iter().any(|i| i.kind == "music") && !out.sources.iter().any(|s| s == "YouTube") {
        out.sources.push("YouTube".into());
    }
}

/// AniList's names of the characters (full, native, alternative), added to the MyAnimeList characters with the
/// same name (MyAnimeList writes "Natsuki, Subaru", AniList "Subaru Natsuki": compared as sets of words).
fn add_aliases(http: &dyn Http, mal: u64, items: &mut [Item]) {
    const Q: &str = "query($id:Int){Media(idMal:$id,type:ANIME){characters(perPage:50,sort:[ROLE,RELEVANCE,ID]){nodes{name{full native alternative}}}}}";
    let Ok(j) = crate::anilist::query(http, Q, vec![("id", Json::Num(mal as f64))]) else { return };
    let key = |s: &str| {
        let mut w = name_words(s);
        w.sort();
        w
    };
    for n in j.path("data.Media.characters.nodes").items() {
        let name = n.get("name");
        let full = name.get("full").str();
        if full.is_empty() {
            continue;
        }
        let k = key(&full);
        if let Some(it) = items.iter_mut().find(|i| i.kind == "cast" && key(&i.name) == k) {
            let mut more = vec![full.clone(), name.get("native").str()];
            more.extend(name.get("alternative").strings());
            for a in more.into_iter().filter(|a| !a.trim().is_empty()) {
                if !it.aliases.contains(&a) {
                    it.aliases.push(a);
                }
            }
        }
    }
}

// ---------------------------------------------------------------- Google Lens: who is in this frame

/// Lowercase, with the accents of Latin letters taken off ("Émilia" is "emilia").
fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'ē' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'ō' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'ū' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// The words of a name or text, folded (letters and digits of any script; the rest separates them).
fn name_words(s: &str) -> Vec<String> {
    fold(s).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_string()).collect()
}

/// A name written in Japanese / Chinese / Korean script (no spaces between words): matched as a piece of the text.
fn is_cjk(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff))
}

/// Without spaces and the dots between Japanese names ("ナツキ・スバル" is "ナツキスバル").
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && !matches!(c, '・' | '·' | '･' | ',' | '、' | '=' | '＝')).collect()
}

/// Words that are names of characters but also plain words of a description: alone, they say nothing.
const LENS_COMMON: &[&str] = &[
    "the", "and", "from", "with", "anime", "manga", "season", "episode", "series", "movie", "film", "character", "characters",
    "girl", "boy", "man", "woman", "king", "queen", "prince", "princess", "knight", "master", "lord", "lady", "sister", "brother",
    "mother", "father", "doctor", "captain", "teacher", "witch", "demon", "dragon", "cat", "the", "voice", "actor", "actress",
    "personagem", "personagens", "temporada", "episodio", "serie", "filme", "rei", "rainha", "mae", "pai", "irmao", "irma",
];

/// The people of the X-Ray named in the text Google Lens gave for the frame on screen (its overview of the picture
/// and the titles of the pages it found), in the order the text names them. A character counts when its whole name
/// is there ("Subaru Natsuki" for MyAnimeList's "Natsuki, Subaru"), a one-word name when it is written with a capital
/// ("Emilia"), and one distinctive word of a longer name when no one else has it ("Julius" for "Julius Juukulius",
/// which Lens may spell otherwise). Actors (TMDB / IMDb cast) and voice actors count the same way; the person shown is
/// always the X-Ray's row, so the photo, the character and the actor come with it. `text` of each row says what
/// was matched; its `tag` is "lens".
pub fn lens_match(items: &[Item], text: &str) -> Vec<Item> {
    let words = name_words(text);
    // the words written with a capital (names), folded; Japanese has no capitals: every word counts
    let capital: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().next().map(|c| c.is_uppercase() || !c.is_lowercase()).unwrap_or(false))
        .map(fold)
        .collect();
    let text_compact = compact(text);
    // the names of each cast row: the character / person, its other names, the character played, the voice actors
    let cast: Vec<(usize, Vec<String>)> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.kind == "cast")
        .map(|(n, i)| {
            let mut names = vec![i.name.clone()];
            names.extend(i.aliases.iter().cloned());
            // TMDB's "Subaru Natsuki (voice)"; MyAnimeList's role is "Main" / "Supporting", not a name
            let role = i.role.split(['(', '/']).next().unwrap_or("").trim().to_string();
            if !role.is_empty() && !matches!(role.as_str(), "Main" | "Supporting") {
                names.push(role);
            }
            names.extend(i.voice.split(" · ").map(|v| v.split('(').next().unwrap_or("").trim().to_string()).filter(|v| !v.is_empty()));
            (n, names)
        })
        .collect();
    // how many rows have each word: a word of one row only can stand for its name
    let mut owners: std::collections::HashMap<String, std::collections::HashSet<usize>> = Default::default();
    for (n, names) in &cast {
        for nm in names {
            for w in name_words(nm) {
                owners.entry(w).or_default().insert(*n);
            }
        }
    }
    let pos_of = |w: &str| words.iter().position(|x| x == w);
    // (row, strength 3 whole name / 2 one-word name / 1 one word, first mention in the text, what matched)
    let mut hits: Vec<(usize, u8, usize, String)> = Vec::new();
    for (n, names) in &cast {
        let mut best: Option<(u8, usize, String)> = None;
        // the strongest match says what was matched; the first mention in the text gives the order
        let mut take = |s: u8, at: usize, what: &str| match best.as_mut() {
            None => best = Some((s, at, what.to_string())),
            Some(b) => {
                if s > b.0 {
                    b.0 = s;
                    b.2 = what.to_string();
                }
                b.1 = b.1.min(at);
            }
        };
        for nm in names {
            if is_cjk(nm) {
                let c = compact(nm);
                if c.chars().count() >= 2 {
                    if let Some(p) = text_compact.find(&c) {
                        take(3, text_compact[..p].chars().count(), nm);
                    }
                }
                continue;
            }
            let ws: Vec<String> = name_words(nm).into_iter().filter(|w| w.chars().count() >= 2).collect();
            if ws.is_empty() {
                continue;
            }
            if ws.len() >= 2 && ws.iter().all(|w| words.contains(w)) {
                let at = ws.iter().filter_map(|w| pos_of(w)).min().unwrap_or(usize::MAX);
                take(3, at, nm);
            } else if ws.len() == 1 {
                let w = &ws[0];
                if w.chars().count() >= 3 && !LENS_COMMON.contains(&w.as_str()) && capital.contains(w) {
                    take(2, pos_of(w).unwrap_or(usize::MAX), nm);
                }
            } else {
                for w in &ws {
                    let unique = owners.get(w).map(|o| o.len() == 1).unwrap_or(false);
                    if w.chars().count() >= 4 && unique && !LENS_COMMON.contains(&w.as_str()) && capital.contains(w) {
                        take(1, pos_of(w).unwrap_or(usize::MAX), nm);
                    }
                }
            }
        }
        if let Some((s, at, what)) = best {
            hits.push((*n, s, at, what));
        }
    }
    hits.sort_by_key(|h| h.2);
    let mut out: Vec<Item> = Vec::new();
    for (n, _, _, what) in hits {
        let it = &items[n];
        // the same person twice (a character of MyAnimeList and the actor of TMDB with the same name): once
        if out.iter().any(|o| o.name == it.name) {
            continue;
        }
        out.push(Item { tag: "lens".into(), text: what, aliases: vec![], ..it.clone() });
        // the same person in the other sources: the actor of a character (TMDB "Subaru Natsuki (voice)") or the
        // character of an actor, so the row says both
        let i = out.len() - 1;
        let key = name_words(&it.name);
        for o in items.iter().filter(|o| o.kind == "cast" && o.name != it.name) {
            let role_words = name_words(o.role.split('(').next().unwrap_or(""));
            let named = |ws: &[String], k: &[String]| !k.is_empty() && k.iter().all(|w| ws.contains(w));
            if named(&role_words, &key) && out[i].voice.is_empty() {
                out[i].voice = o.name.clone();
            } else if !o.voice.is_empty() && named(&name_words(&o.voice), &key) && out[i].role.is_empty() {
                out[i].role = o.name.clone();
            }
        }
        if out.len() >= 10 {
            break;
        }
    }
    out
}

// ---------------------------------------------------------------- more about one person of the X-Ray

/// The first `n` characters of a text, cut at a sentence when one ends near there, without MyAnimeList's
/// "(Source: …)" notes and blank lines.
fn clip_text(t: &str, n: usize) -> String {
    let t = t.split("(Source:").next().unwrap_or(t).replace("\r", "");
    let t: String = t.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
    if t.chars().count() <= n {
        return t;
    }
    let cut: String = t.chars().take(n).collect();
    match cut.rfind(". ") {
        Some(p) if p > n / 2 => cut[..=p].to_string(),
        _ => format!("{}…", cut.trim_end()),
    }
}

/// A Wikipedia summary of a page, in the language of the interface first, then in English.
fn wiki_summary(http: &dyn Http, title: &str, lang: &str) -> Option<(String, String)> {
    let langs: Vec<&str> = if lang.is_empty() || lang == "en" { vec!["en"] } else { vec![lang, "en"] };
    for l in langs {
        let url = format!("https://{}.wikipedia.org/api/rest_v1/page/summary/{}", l, crate::util::url_encode(&title.replace(' ', "_")));
        let Ok(j) = get_json(http, &url, 15) else { continue };
        if j.get("type").str() != "standard" {
            continue;
        }
        let t = j.get("extract").str();
        if !t.is_empty() {
            return Some((t, j.get("content_urls").get("desktop").get("page").str()));
        }
    }
    None
}

/// What the sources know of one person of the X-Ray (`Item::link`): a short text about them, a few facts
/// (nicknames, birthday, where they appear, who voices them) and the address of their page. `lang` is the
/// language of the interface ("pt", "es", "en") for TMDB's biography and Wikipedia.
pub fn person_info(http: &dyn Http, link: &str, tmdb_key: &str, lang: &str) -> Result<Json, String> {
    let mut about = String::new();
    let mut facts: Vec<String> = vec![];
    let mut url = String::new();
    let mut source = "";
    if let Some(id) = link.strip_prefix("mal-char:") {
        let j = jikan(http, &format!("/characters/{}/full", id))?;
        let d = j.get("data");
        about = clip_text(&d.get("about").str(), 700);
        url = d.get("url").str();
        source = "MyAnimeList";
        let kanji = d.get("name_kanji").str();
        if !kanji.is_empty() {
            facts.push(format!("名前: {}", kanji));
        }
        let nick = d.get("nicknames").strings();
        if !nick.is_empty() {
            facts.push(format!("Nicknames: {}", nick.into_iter().take(5).collect::<Vec<_>>().join(", ")));
        }
        let voices: Vec<String> = d.get("voices").items().iter().take(4).map(|v| format!("{} ({})", v.get("person").get("name").str(), v.get("language").str())).collect();
        if !voices.is_empty() {
            facts.push(format!("Voices: {}", voices.join(", ")));
        }
        let shows: Vec<String> = d.get("anime").items().iter().take(5).map(|a| a.get("anime").get("title").str()).filter(|t| !t.is_empty()).collect();
        if !shows.is_empty() {
            facts.push(format!("Appears in: {}", shows.join(", ")));
        }
    } else if let Some(id) = link.strip_prefix("tmdb-person:") {
        if tmdb_key.trim().is_empty() {
            return Err("Add a TMDB key (Preferences > Accounts and keys).".into());
        }
        let tl = match lang {
            "pt" => "pt-BR",
            "es" => "es-ES",
            _ => "en-US",
        };
        let p = tmdb(http, tmdb_key, &format!("/person/{}?language={}&append_to_response=combined_credits", id, tl))?;
        about = p.get("biography").str();
        if about.is_empty() && tl != "en-US" {
            if let Ok(en) = tmdb(http, tmdb_key, &format!("/person/{}", id)) {
                about = en.get("biography").str();
            }
        }
        about = clip_text(&about, 700);
        source = "TMDB";
        url = match p.get("imdb_id").str() {
            i if !i.is_empty() => format!("https://www.imdb.com/name/{}/", i),
            _ => format!("https://www.themoviedb.org/person/{}", id),
        };
        let born = p.get("birthday").str();
        if !born.is_empty() {
            let place = p.get("place_of_birth").str();
            facts.push(if place.is_empty() { format!("Born: {}", born) } else { format!("Born: {} · {}", born, place) });
        }
        let mut credits: Vec<&Json> = p.get("combined_credits").get("cast").items().iter().collect();
        credits.sort_by(|a, b| b.get("popularity").as_f64().unwrap_or(0.0).partial_cmp(&a.get("popularity").as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
        let mut known: Vec<String> = vec![];
        for c in credits {
            let t = [c.get("title").str(), c.get("name").str()].into_iter().find(|x| !x.is_empty()).unwrap_or_default();
            if !t.is_empty() && !known.contains(&t) {
                known.push(t);
            }
            if known.len() >= 5 {
                break;
            }
        }
        if !known.is_empty() {
            facts.push(format!("Known for: {}", known.join(", ")));
        }
    }
    // nothing yet (an IMDb name, or a source without a text): Wikipedia
    if about.is_empty() {
        let name = link.strip_prefix("name:").map(|x| x.to_string());
        if let Some(n) = name {
            if let Some((t, u)) = wiki_summary(http, &n, lang) {
                about = clip_text(&t, 700);
                if url.is_empty() {
                    url = u;
                }
                source = "Wikipedia";
            }
        }
    }
    if about.is_empty() && facts.is_empty() {
        return Err("Nothing more was found about this person.".into());
    }
    Ok(Json::obj(vec![
        ("about", Json::Str(about)),
        ("facts", Json::Arr(facts.into_iter().map(Json::Str).collect())),
        ("url", Json::Str(url)),
        ("source", Json::Str(source.into())),
    ]))
}

impl XRay {
    /// The payload for the interface: every item with `start_time` / `end_time` in seconds.
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            (
                "items",
                Json::Arr(
                    self.items
                        .iter()
                        .map(|i| {
                            Json::obj(vec![
                                ("type", Json::Str(i.kind.clone())),
                                ("start_time", Json::Num(i.start)),
                                ("end_time", Json::Num(if i.end.is_finite() && i.end < 1e9 { i.end } else { 1e9 })),
                                ("name", Json::Str(i.name.clone())),
                                ("role", Json::Str(i.role.clone())),
                                ("voice", Json::Str(i.voice.clone())),
                                ("image", Json::Str(i.image.clone())),
                                ("text", Json::Str(i.text.clone())),
                                ("tag", Json::Str(i.tag.clone())),
                                ("ref", Json::Str(i.link.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("sources", Json::Arr(self.sources.iter().map(|s| Json::Str(s.clone())).collect())),
            ("notes", Json::Arr(self.notes.iter().map(|s| Json::Str(s.clone())).collect())),
        ])
    }
}

/// The items of `kind` that cover `pos`.
pub fn at<'a>(items: &'a [Item], pos: f64) -> impl Iterator<Item = &'a Item> {
    items.iter().filter(move |i| pos >= i.start && pos < i.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenes_from_chapters_and_time_filter() {
        let inp = Input { duration: 100.0, chapters: vec![(0.0, "Intro".into()), (30.0, String::new())], ..Input::default() };
        let x = build(&crate::http::fake::Fake::new(), &inp);
        assert_eq!(x.items.len(), 2);
        assert_eq!(x.items[1].name, "Chapter 2");
        assert_eq!(at(&x.items, 40.0).map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["Chapter 2"]);
    }

    #[test]
    fn imdb_source_reads_cinemeta_and_photos() {
        let meta = r#"{"meta":{"name":"The Matrix","cast":["Keanu Reeves","Carrie-Anne Moss"],"director":["Lana Wachowski"],"writer":[],"imdbRating":"8.7","awards":"Won 4 Oscars.","genres":["Action"],"trailers":[{"source":"abc123","type":"Trailer"}]}}"#;
        let sugg = r#"{"d":[{"id":"tt1","l":"Keanu Reeves"},{"id":"nm0000206","l":"Keanu Reeves","i":{"imageUrl":"https://m.media-amazon.com/images/M/X._V1_.jpg"}}]}"#;
        let http = crate::http::fake::Fake::new()
            .route("cinemeta.strem.io/meta/movie/tt0133093", 200, meta)
            .route("suggestion/x/Keanu%20Reeves", 200, sugg);
        let inp = Input { kind: "movie".into(), imdb: Some("tt0133093".into()), source: "imdb".into(), duration: 100.0, ..Input::default() };
        let x = build(&http, &inp);
        assert_eq!(x.sources, vec!["IMDb".to_string()]);
        let cast: Vec<&Item> = x.items.iter().filter(|i| i.kind == "cast").collect();
        assert_eq!(cast.len(), 2);
        assert_eq!(cast[0].image, "https://m.media-amazon.com/images/M/X._V1_UX190_.jpg");
        assert_eq!(cast[1].image, "");
        assert!(x.items.iter().any(|i| i.kind == "trivia" && i.name == "Director: Lana Wachowski"));
        // the trailer is listed apart, never as a song
        assert!(x.items.iter().any(|i| i.kind == "trailer" && i.tag == "Trailer" && i.text.ends_with("abc123")));
        assert!(!x.items.iter().any(|i| i.kind == "music"));
    }

    #[test]
    fn person_info_reads_mal_characters_and_wikipedia() {
        let ch = r#"{"data":{"url":"https://myanimelist.net/character/118735/Subaru_Natsuki","name_kanji":"ナツキ・スバル","nicknames":["Barusu"],"about":"Subaru is a young man who was summoned to another world.\n\n(Source: Wiki)","anime":[{"role":"Main","anime":{"title":"Re:Zero"}}],"voices":[{"language":"Japanese","person":{"name":"Kobayashi, Yuusuke"}}]}}"#;
        let wiki = r#"{"type":"standard","extract":"Keanu Reeves is a Canadian actor.","content_urls":{"desktop":{"page":"https://en.wikipedia.org/wiki/Keanu_Reeves"}}}"#;
        let http = crate::http::fake::Fake::new().route("characters/118735/full", 200, ch).route("en.wikipedia.org/api/rest_v1/page/summary/Keanu_Reeves", 200, wiki);
        let j = person_info(&http, "mal-char:118735", "", "en").unwrap();
        assert_eq!(j.get("about").str(), "Subaru is a young man who was summoned to another world.");
        assert_eq!(j.get("source").str(), "MyAnimeList");
        let facts = j.get("facts").strings();
        assert!(facts.iter().any(|f| f.contains("Barusu")) && facts.iter().any(|f| f.contains("Kobayashi")) && facts.iter().any(|f| f.contains("Re:Zero")));
        let k = person_info(&http, "name:Keanu Reeves", "", "pt").unwrap();
        assert_eq!(k.get("source").str(), "Wikipedia");
        assert!(k.get("url").str().ends_with("Keanu_Reeves"));
        // TMDB without a key says so instead of asking
        assert!(person_info(&http, "tmdb-person:6384", "", "en").is_err());
    }

    const REZERO_META: &str = r#"{"meta":{"name":"Re:ZERO -Starting Life in Another World-","cast":["Yusuke Kobayashi"],"genres":["Animation"],"trailers":[{"source":"trl00000001","type":"Trailer"},{"source":"trl00000002","type":"Trailer"}]}}"#;
    const REZERO_THEMES: &str = r#"{"data":{"openings":["1: \"Redo\" by Konomi Suzuki (eps 1-14)","2: \"Paradisus-Paradoxum\" by MYTH & ROID (eps 15-25)"],"endings":["1: \"STYX HELIX\" by MYTH & ROID (eps 1-13)"]}}"#;

    #[test]
    fn anime_known_by_imdb_gets_its_songs_not_trailers() {
        // IMDb only (the source chosen), no MyAnimeList id: the entry is searched by title and year
        let search = r#"{"data":[
            {"mal_id":1,"title":"Re:Zero kara Hajimeru Break Time","title_english":"Re:ZERO -Starting Break Time From Zero-","year":2016},
            {"mal_id":31240,"title":"Re:Zero kara Hajimeru Isekai Seikatsu","title_english":"Re:ZERO -Starting Life in Another World-","year":2016},
            {"mal_id":39587,"title":"Re:Zero kara Hajimeru Isekai Seikatsu 2nd Season","title_english":"Re:ZERO -Starting Life in Another World- Season 2","year":2020}]}"#;
        let http = crate::http::fake::Fake::new()
            .route("cinemeta.strem.io/meta/series/tt5607616", 200, REZERO_META)
            .route("api.jikan.moe/v4/anime?q=", 200, search)
            .route("api.jikan.moe/v4/anime/31240/themes", 200, REZERO_THEMES);
        let inp = Input {
            title: "Re:ZERO -Starting Life in Another World-".into(),
            kind: "series".into(),
            imdb: Some("tt5607616".into()),
            source: "imdb".into(),
            anime: true,
            year: Some(2016),
            season: Some(1),
            episode: Some(16),
            duration: 1421.0,
            // the opening is timed by AniSkip; the ending by a chapter of the file
            songs_at: vec![(60.0, 150.0, "op".into())],
            chapters: vec![(0.0, "Prologue".into()), (60.0, "Opening".into()), (150.0, "Part A".into()), (1290.0, "Ending".into()), (1380.0, "Preview".into())],
            ..Input::default()
        };
        let x = build(&http, &inp);
        let music: Vec<&Item> = x.items.iter().filter(|i| i.kind == "music").collect();
        assert_eq!(music.len(), 2, "{:?}", music);
        assert_eq!((music[0].name.as_str(), music[0].role.as_str(), music[0].tag.as_str()), ("Paradisus-Paradoxum", "MYTH & ROID", "Opening 2"));
        assert_eq!((music[0].start, music[0].end), (60.0, 150.0));
        assert_eq!((music[1].name.as_str(), music[1].tag.as_str()), ("STYX HELIX", "Ending 1"));
        assert_eq!((music[1].start, music[1].end), (1290.0, 1380.0));
        assert!(music.iter().all(|m| m.tag != "Trailer" && !m.text.contains("trl000")));
        assert_eq!(x.items.iter().filter(|i| i.kind == "trailer").count(), 2);
        assert!(x.sources.contains(&"MyAnimeList".to_string()));
    }

    #[test]
    fn untimed_songs_and_no_youtube_without_data_dir() {
        let http = crate::http::fake::Fake::new().route("api.jikan.moe/v4/anime/31240/themes", 200, REZERO_THEMES);
        let inp = Input { mal: Some(31240), source: "imdb".into(), episode: Some(3), duration: 1400.0, ..Input::default() };
        let x = build(&http, &inp);
        let music: Vec<&Item> = x.items.iter().filter(|i| i.kind == "music").collect();
        assert_eq!(music.len(), 2);
        // nothing says where the song is: no time range, never "playing now"
        assert!(music.iter().all(|m| m.end.is_infinite()));
        assert_eq!(x.to_json().get("items").items()[0].get("end_time").as_f64(), Some(1e9));
        // MyAnimeList lists no songs and there is no yt-dlp folder: nothing is invented
        let none = crate::http::fake::Fake::new().route("api.jikan.moe/v4/anime/5/themes", 200, r#"{"data":{"openings":[],"endings":[]}}"#);
        let x = build(&none, &Input { mal: Some(5), source: "imdb".into(), ..Input::default() });
        assert!(!x.items.iter().any(|i| i.kind == "music"));
    }

    #[test]
    fn mal_entry_by_title_and_year() {
        let list = Json::parse(r#"[{"mal_id":10,"title":"Fruits Basket","year":2001},{"mal_id":38680,"title":"Fruits Basket (2019)","title_english":"Fruits Basket","year":2019},{"mal_id":40417,"title":"Fruits Basket 2nd Season","year":2020}]"#).unwrap();
        assert_eq!(pick_mal(list.items(), "Fruits Basket", Some(2019), None), Some(38680));
        assert_eq!(pick_mal(list.items(), "Fruits Basket", Some(2001), None), Some(10));
        // a later season: one that started after the series
        assert_eq!(pick_mal(list.items(), "Fruits Basket", Some(2019), Some(2)), Some(40417));
        assert_eq!(pick_mal(list.items(), "Naruto", Some(2002), None), None);
        assert_eq!(year_of("2016–"), Some(2016));
        assert_eq!(year_of(""), None);
    }

    #[test]
    fn chapters_named_like_songs() {
        assert_eq!(chapter_kind("Opening"), Some("op"));
        assert_eq!(chapter_kind("OP2"), Some("op"));
        assert_eq!(chapter_kind("NCOP"), Some("op"));
        assert_eq!(chapter_kind("Ending Credits"), Some("ed"));
        assert_eq!(chapter_kind("エンディング"), Some("ed"));
        assert_eq!(chapter_kind("Operation Start"), None);
        assert_eq!(chapter_kind("Edward's room"), None);
        assert_eq!(chapter_kind("Part A"), None);
    }

    fn rezero_cast() -> Vec<Item> {
        let c = |name: &str, role: &str, voice: &str, aliases: &[&str]| Item {
            kind: "cast".into(),
            end: 1400.0,
            name: name.into(),
            role: role.into(),
            voice: voice.into(),
            image: format!("https://img/{}.jpg", name.len()),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
            ..Item::default()
        };
        vec![
            c("Natsuki, Subaru", "Main", "Kobayashi, Yuusuke", &["Subaru Natsuki", "ナツキ・スバル"]),
            c("Emilia", "Main", "Takahashi, Rie", &["エミリア"]),
            c("Juukulius, Julius", "Supporting", "Ezawa, Masaki", &["Julius Juukulius", "ユリウス・ユークリウス"]),
            c("Rem", "Main", "Minase, Inori", &["レム"]),
            c("Reinhard van Astrea", "Supporting", "Nakamura, Yuuichi", &[]),
            Item { kind: "music".into(), name: "Redo".into(), ..Item::default() },
        ]
    }

    #[test]
    fn lens_finds_the_characters_of_the_owners_example() {
        let items = rezero_cast();
        let text = "Julius Euclius e Subaru Natsuki, do anime Re:Zero − Começando a Vida em Outro Mundo. A cena mostra os dois após o duelo.";
        let hits = lens_match(&items, text);
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        // in the order the text names them; Julius by his given name (Lens spells the family name otherwise)
        assert_eq!(names, vec!["Juukulius, Julius", "Natsuki, Subaru"]);
        assert!(hits.iter().all(|h| h.tag == "lens" && !h.image.is_empty()));
        assert_eq!(hits[1].voice, "Kobayashi, Yuusuke");
        // "Re:Zero", "Mundo", "cena" name no one; music is never a person
        assert!(!names.contains(&"Rem") && !names.contains(&"Emilia"));
    }

    #[test]
    fn lens_reads_other_scripts_actors_and_skips_plain_words() {
        let items = rezero_cast();
        // Japanese names, written without spaces
        let ja = lens_match(&items, "リゼロ ナツキ・スバルとエミリアの名シーン");
        assert_eq!(ja.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), vec!["Natsuki, Subaru", "Emilia"]);
        // the voice actor named: the character's row comes
        let va = lens_match(&items, "Inori Minase at the Re:Zero event");
        assert_eq!(va.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), vec!["Rem"]);
        // a one-word name written as a plain word (no capital) is not a match; accents are ignored
        assert!(lens_match(&items, "a rem sleep scene").is_empty());
        assert_eq!(lens_match(&items, "ÉMILIA smiles").len(), 1);
        // live action: TMDB rows are actors with the character in `role`
        let cast = vec![
            Item { kind: "cast".into(), name: "Keanu Reeves".into(), role: "Neo (Thomas Anderson)".into(), ..Item::default() },
            Item { kind: "cast".into(), name: "Carrie-Anne Moss".into(), role: "Trinity".into(), ..Item::default() },
        ];
        let h = lens_match(&cast, "Neo e Trinity no filme Matrix, com Keanu Reeves");
        assert_eq!(h.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), vec!["Keanu Reeves", "Carrie-Anne Moss"]);
        assert!(lens_match(&cast, "").is_empty());
    }

    #[test]
    fn chosen_source_without_data_says_why() {
        let inp = Input { source: "mal".into(), ..Input::default() };
        let x = build(&crate::http::fake::Fake::new(), &inp);
        assert!(x.items.is_empty());
        assert!(x.notes[0].starts_with("MyAnimeList"));
    }

    // Jikan v4 answers, cut to a few rows, in the shape of https://api.jikan.moe/v4 (docs.api.jikan.moe)
    const JIKAN_CHARACTERS: &str = r#"{"data":[
        {"character":{"mal_id":118737,"url":"https://myanimelist.net/character/118737/Natsuki_Subaru","images":{"jpg":{"image_url":"https://cdn.myanimelist.net/images/characters/16/302893.jpg"},"webp":{"image_url":"https://cdn.myanimelist.net/images/characters/16/302893.webp","small_image_url":"https://cdn.myanimelist.net/r/42x62/images/characters/16/302893.webp"}},"name":"Natsuki, Subaru"},
         "role":"Main","favorites":23000,
         "voice_actors":[{"person":{"mal_id":11817,"url":"https://myanimelist.net/people/11817/Yuusuke_Kobayashi","images":{"jpg":{"image_url":"https://cdn.myanimelist.net/images/voiceactors/3/62133.jpg"}},"name":"Kobayashi, Yuusuke"},"language":"Japanese"},
                         {"person":{"mal_id":44447,"url":"https://myanimelist.net/people/44447/Sean_Chiplock","images":{"jpg":{"image_url":"https://cdn.myanimelist.net/images/voiceactors/2/48519.jpg"}},"name":"Chiplock, Sean"},"language":"English"},
                         {"person":{"mal_id":59339,"url":"https://myanimelist.net/people/59339/Fabio_Lucindo","images":{"jpg":{"image_url":"https://cdn.myanimelist.net/images/voiceactors/1/1.jpg"}},"name":"Lucindo, Fabio"},"language":"Portuguese (BR)"}]},
        {"character":{"mal_id":118763,"url":"https://myanimelist.net/character/118763/Puck","images":{"jpg":{"image_url":"https://cdn.myanimelist.net/images/characters/9/307541.jpg"}},"name":"Puck"},
         "role":"Supporting","favorites":2400,
         "voice_actors":[{"person":{"mal_id":185,"name":"Uchiyama, Kouki","images":{"jpg":{"image_url":""}}},"language":"Japanese"}]},
        {"character":{"mal_id":118735,"url":"https://myanimelist.net/character/118735/Emilia","images":{"jpg":{"image_url":"https://cdn.myanimelist.net/images/characters/3/302895.jpg"}},"name":"Emilia"},
         "role":"Main","favorites":21000,
         "voice_actors":[{"person":{"mal_id":34785,"name":"Takahashi, Rie","images":{"jpg":{"image_url":""}}},"language":"Japanese"}]}]}"#;
    const JIKAN_EPISODE: &str = r#"{"data":{"mal_id":3,"url":"https://myanimelist.net/anime/31240/Re_Zero_kara_Hajimeru_Isekai_Seikatsu/episode/3","title":"Starting Over from Zero","title_japanese":"ゼロから始まる異世界生活","title_romanji":"Zero kara Hajimaru Isekai Seikatsu","duration":1500,"aired":"2016-04-18T00:00:00+00:00","filler":false,"recap":false,"synopsis":"..."}}"#;
    const JIKAN_ANIME: &str = r#"{"data":{"mal_id":31240,"title":"Re:Zero kara Hajimeru Isekai Seikatsu","background":"Re:Zero won the Best Adaptation award at the Newtype Anime Awards 2016. Its first episode ran for an hour, twice the usual length of a TV episode.","studios":[{"mal_id":314,"type":"anime","name":"White Fox","url":"https://myanimelist.net/anime/producer/314/White_Fox"}],"year":2016}}"#;

    fn jikan_rezero(characters: &[(u16, &str)]) -> crate::http::fake::Fake {
        let mut h = crate::http::fake::Fake::new();
        for (st, body) in characters {
            h = h.route("api.jikan.moe/v4/anime/31240/characters", *st, body);
        }
        h.route("api.jikan.moe/v4/anime/31240/episodes/3", 200, JIKAN_EPISODE)
            .route("api.jikan.moe/v4/anime/31240/themes", 200, REZERO_THEMES)
            .route("api.jikan.moe/v4/anime/31240", 200, JIKAN_ANIME)
    }

    #[test]
    fn mal_source_reads_jikan_characters_episode_and_background() {
        let http = jikan_rezero(&[(200, JIKAN_CHARACTERS)]);
        let inp = Input { mal: Some(31240), source: "mal".into(), episode: Some(3), duration: 1400.0, dub_lang: "Portuguese".into(), ..Input::default() };
        let x = build(&http, &inp);
        assert!(x.notes.is_empty(), "{:?}", x.notes);
        let cast: Vec<&Item> = x.items.iter().filter(|i| i.kind == "cast").collect();
        // the main characters first, the most liked first among them
        assert_eq!(cast.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Natsuki, Subaru", "Emilia", "Puck"]);
        assert_eq!(cast[0].voice, "Kobayashi, Yuusuke · Lucindo, Fabio (Portuguese (BR))");
        assert_eq!(cast[0].link, "mal-char:118737");
        assert!(cast[0].image.ends_with("302893.jpg"));
        let trivia: Vec<&str> = x.items.iter().filter(|i| i.kind == "trivia").map(|i| i.name.as_str()).collect();
        assert!(trivia.contains(&"Studio: White Fox"));
        assert!(trivia.contains(&"Japanese title: ゼロから始まる異世界生活"));
        assert!(trivia.iter().any(|t| t.starts_with("Re:Zero won the Best Adaptation")));
        assert!(x.items.iter().any(|i| i.kind == "music"));
        assert_eq!(x.sources, vec!["MyAnimeList".to_string()]);
    }

    #[test]
    fn jikan_busy_is_waited_for() {
        // the burst of requests got "429 Too Many Requests": asked again after a pause, not given up
        let http = jikan_rezero(&[(429, r#"{"status":429,"type":"RateLimitException","message":"You are being rate limited by Jikan or MyAnimeList is rate-limiting our servers."}"#), (200, JIKAN_CHARACTERS)]);
        let x = build(&http, &Input { mal: Some(31240), source: "mal".into(), ..Input::default() });
        assert_eq!(x.items.iter().filter(|i| i.kind == "cast").count(), 3, "{:?}", x.notes);
    }

    #[test]
    fn mal_characters_from_anilist_when_jikan_fails() {
        let anilist = r#"{"data":{"Media":{"characters":{"edges":[
            {"role":"MAIN","node":{"name":{"full":"Subaru Natsuki"},"image":{"large":"https://s4.anilist.co/s.png"}},"ja":[{"name":{"full":"Yuusuke Kobayashi"}}],"dub":[{"name":{"full":"Fabio Lucindo"}}]},
            {"role":"SUPPORTING","node":{"name":{"full":"Puck"},"image":{"large":""}},"ja":[{"name":{"full":"Kouki Uchiyama"}}],"dub":[]}]}}}}"#;
        let http = crate::http::fake::Fake::new()
            .route("api.jikan.moe/v4/anime/31240/characters", 404, r#"{"status":404,"message":"Resource does not exist"}"#)
            .route("graphql.anilist.co", 200, anilist);
        let x = build(&http, &Input { mal: Some(31240), source: "mal".into(), dub_lang: "Portuguese".into(), ..Input::default() });
        let cast: Vec<&Item> = x.items.iter().filter(|i| i.kind == "cast").collect();
        assert_eq!(cast.len(), 2, "{:?}", x.notes);
        assert_eq!(cast[0].voice, "Yuusuke Kobayashi · Fabio Lucindo (Portuguese)");
        assert_eq!(cast[0].tag, "main");
        assert_eq!(cast[1].link, "name:Puck");
        assert_eq!(x.sources, vec!["MyAnimeList (AniList)".to_string()]);
        let sent = http.requests();
        let q = sent.iter().find(|r| r.url.contains("anilist")).unwrap();
        assert!(String::from_utf8_lossy(&q.body.as_ref().unwrap().1).contains("language:PORTUGUESE"));
    }

    #[test]
    fn mal_picked_without_an_id_searches_by_title() {
        let search = r#"{"data":[{"mal_id":31240,"title":"Re:Zero kara Hajimeru Isekai Seikatsu","title_english":"Re:ZERO -Starting Life in Another World-","year":2016}]}"#;
        let http = jikan_rezero(&[(200, JIKAN_CHARACTERS)]).route("api.jikan.moe/v4/anime?q=", 200, search);
        let x = build(&http, &Input { title: "Re:Zero kara Hajimeru Isekai Seikatsu".into(), source: "mal".into(), year: Some(2016), ..Input::default() });
        assert!(x.items.iter().any(|i| i.kind == "cast" && i.name == "Emilia"), "{:?}", x.notes);
    }
}
