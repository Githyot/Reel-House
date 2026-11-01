//! AniList (GraphQL): the same MyAnimeList-style data when Jikan cannot be reached, plus category pages and
//! "people who liked this also liked" rows. AniList knows the MyAnimeList id of most titles (`idMal`).
use crate::http::{Http, Request};
use crate::json::Json;
use crate::mal::{Card, Character, Info, Named, Related, Relation, Review, Staff};

const URL: &str = "https://graphql.anilist.co";

/// Genres AniList has (other categories, like Isekai, are "tags").
pub const GENRES: &[&str] = &[
    "Action", "Adventure", "Comedy", "Drama", "Ecchi", "Fantasy", "Horror", "Mahou Shoujo", "Mecha", "Music", "Mystery", "Psychological", "Romance", "Sci-Fi", "Slice of Life", "Sports", "Supernatural", "Thriller",
];

/// Popular tags worth a button.
pub const TAGS: &[&str] = &[
    "Isekai", "Time Loop", "Shounen", "Seinen", "Shoujo", "Josei", "School", "Magic", "Military", "Samurai", "Vampire", "Post-Apocalyptic", "Space", "Super Power", "Survival", "Gore", "Reincarnation", "Iyashikei", "Idol", "Cyberpunk",
    "Martial Arts", "Detective", "Video Games", "Historical", "Parody", "Coming of Age",
];

pub fn is_genre(name: &str) -> bool {
    GENRES.iter().any(|g| g.eq_ignore_ascii_case(name))
}

/// Cinemeta's genre names are not always AniList's.
pub fn genre_name(name: &str) -> Option<String> {
    let n = name.trim();
    match n.to_lowercase().as_str() {
        "animation" | "anime" | "" => None,
        "science fiction" | "sci-fi & fantasy" => Some("Sci-Fi".to_string()),
        "sport" => Some("Sports".to_string()),
        "musical" | "music" => Some("Music".to_string()),
        "mystery" => Some("Mystery".to_string()),
        _ => GENRES.iter().chain(TAGS.iter()).find(|g| g.eq_ignore_ascii_case(n)).map(|g| g.to_string()),
    }
}

pub fn query(http: &dyn Http, q: &str, vars: Vec<(&str, Json)>) -> Result<Json, String> {
    let body = Json::obj(vec![("query", Json::Str(q.to_string())), ("variables", Json::obj(vars))]).dump();
    let r = http.send(&Request::post(URL).header("Accept", "application/json").json_body(body).timeout(20))?;
    let j = r.json().map_err(|e| format!("AniList did not answer with JSON ({})", e))?;
    if !r.ok() {
        let m = j.path("errors.0.message").str();
        return Err(format!("AniList answered {}{}", r.status, if m.is_empty() { String::new() } else { format!(": {}", m) }));
    }
    Ok(j)
}

const CARD: &str = "idMal title{romaji english} coverImage{extraLarge large} averageScore seasonYear startDate{year} format description(asHtml:false) genres";

fn title_of(t: &Json) -> String {
    let e = t.get("english").str();
    if e.is_empty() { t.get("romaji").str() } else { e }
}

fn card_of(m: &Json, votes: u64) -> Option<Card> {
    let id = m.get("idMal").as_u64()?;
    let img = {
        let x = m.path("coverImage.extraLarge").str();
        if x.is_empty() { m.path("coverImage.large").str() } else { x }
    };
    Some(Card {
        id,
        title: title_of(m.get("title")),
        image: img,
        score: m.get("averageScore").as_f64().unwrap_or(0.0) / 10.0,
        kind: m.get("format").str(),
        year: m.get("seasonYear").as_i64().or_else(|| m.path("startDate.year").as_i64()).unwrap_or(0),
        votes,
        key: String::new(),
        synopsis: strip_html(&m.get("description").str()).replace("(Source: ", "\n(Source: ").trim().to_string(),
        genres: m.get("genres").strings(),
    })
}

fn cards_of(list: &Json) -> Vec<Card> {
    list.items().iter().filter_map(|m| card_of(m, 0)).collect()
}

/// One page of a category, most popular first.
pub fn by_genre(http: &dyn Http, name: &str, page: u32) -> Result<(Vec<Card>, bool), String> {
    let (arg, var) = if is_genre(name) { ("genre:$g", "g") } else { ("tag:$g", "g") };
    let q = format!("query($page:Int,$g:String){{Page(page:$page,perPage:24){{pageInfo{{hasNextPage}} media(type:ANIME,{} ,sort:POPULARITY_DESC,isAdult:false){{{}}}}}}}", arg, CARD);
    let canonical = GENRES.iter().chain(TAGS.iter()).find(|g| g.eq_ignore_ascii_case(name)).map(|g| g.to_string()).unwrap_or_else(|| name.to_string());
    let j = query(http, &q, vec![("page", Json::Num(page.max(1) as f64)), (var, Json::Str(canonical))])?;
    let p = j.path("data.Page");
    Ok((cards_of(p.get("media")), p.path("pageInfo.hasNextPage").as_bool().unwrap_or(false)))
}

/// A title found by name.
#[derive(Clone, Debug, Default)]
pub struct Found {
    pub id: u64,
    pub names: Vec<String>,
    pub kind: String,
    pub year: Option<i64>,
}

fn found_of(m: &Json) -> Option<Found> {
    let id = m.get("idMal").as_u64()?;
    let mut names = vec![m.path("title.romaji").str(), m.path("title.english").str()];
    names.extend(m.get("synonyms").strings());
    Some(Found { id, names, kind: m.get("format").str(), year: m.get("seasonYear").as_i64().or_else(|| m.path("startDate.year").as_i64()) })
}

pub fn search(http: &dyn Http, text: &str) -> Result<Vec<Found>, String> {
    let q = "query($q:String){Page(perPage:10){media(search:$q,type:ANIME,sort:SEARCH_MATCH){idMal title{romaji english} synonyms format seasonYear startDate{year}}}}";
    let j = query(http, q, vec![("q", Json::Str(text.to_string()))])?;
    Ok(j.path("data.Page.media").items().iter().filter_map(found_of).collect())
}

/// Titles that people who liked `name` also liked: (the title found, its recommendations).
pub fn recommendations_for(http: &dyn Http, name: &str, year: Option<i64>, season: i64) -> Result<(Option<Found>, Vec<Card>), String> {
    let q = format!(
        "query($q:String){{Page(perPage:6){{media(search:$q,type:ANIME,sort:SEARCH_MATCH){{idMal title{{romaji english}} synonyms format seasonYear startDate{{year}} recommendations(perPage:18,sort:RATING_DESC){{nodes{{rating mediaRecommendation{{{}}}}}}}}}}}}}",
        CARD
    );
    let j = query(http, &q, vec![("q", Json::Str(name.to_string()))])?;
    let media = j.path("data.Page.media");
    let found: Vec<Found> = media.items().iter().filter_map(found_of).collect();
    let Some(id) = crate::mal::pick_from(&found, name, year, season) else { return Ok((None, vec![])) };
    let m = media.items().iter().find(|m| m.get("idMal").as_u64() == Some(id));
    let cards = m.map(|m| recs_of(m.path("recommendations.nodes"))).unwrap_or_default();
    Ok((found.into_iter().find(|f| f.id == id), cards))
}

fn recs_of(nodes: &Json) -> Vec<Card> {
    let mut v: Vec<Card> = nodes.items().iter().filter_map(|n| card_of(n.get("mediaRecommendation"), n.get("rating").as_u64().unwrap_or(0))).collect();
    v.dedup_by_key(|c| c.id);
    v
}

// ---------------------------------------------------------------- everything about one title

#[derive(Clone, Debug, Default)]
pub struct Full {
    pub info: Option<Info>,
    pub characters: Vec<Character>,
    pub staff: Vec<Staff>,
    pub recs: Vec<Card>,
    pub reviews: Vec<Review>,
}

pub fn full(http: &dyn Http, mal_id: u64) -> Result<Full, String> {
    let q = format!(
        "query($id:Int){{Media(idMal:$id,type:ANIME){{idMal siteUrl title{{romaji english native}} synonyms coverImage{{extraLarge large}} trailer{{id site}} format source episodes status duration season seasonYear averageScore popularity favourites description(asHtml:false) startDate{{year month day}} endDate{{year month day}} genres tags{{name rank isMediaSpoiler}} studios{{edges{{isMain node{{name isAnimationStudio}}}}}} rankings{{rank type allTime}} externalLinks{{site url type}} relations{{edges{{relationType node{{idMal title{{romaji english}} type}}}}}} characters(sort:[ROLE,RELEVANCE],perPage:16){{edges{{role node{{id name{{full}} image{{large}}}} voiceActors(language:JAPANESE){{name{{full}} image{{large}}}}}}}} staff(perPage:14){{edges{{role node{{name{{full}} image{{large}}}}}}}} recommendations(perPage:16,sort:RATING_DESC){{nodes{{rating mediaRecommendation{{{}}}}}}} reviews(perPage:8,sort:RATING_DESC){{nodes{{summary body(asHtml:false) score ratingAmount rating user{{name avatar{{large}}}} createdAt siteUrl}}}}}}}}",
        CARD
    );
    let j = query(http, &q, vec![("id", Json::Num(mal_id as f64))])?;
    let m = j.path("data.Media");
    if m.is_null() {
        return Err("AniList has no page for this title.".to_string());
    }
    Ok(parse_full(m, mal_id))
}

pub fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                if tag.to_lowercase().starts_with("br") || tag.to_lowercase().starts_with("/p") {
                    out.push('\n');
                }
            }
            _ if in_tag => tag.push(c),
            _ => out.push(c),
        }
    }
    let out = out.replace("&quot;", "\"").replace("&#039;", "'").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ");
    // Collapse more than one blank line.
    let mut res = String::new();
    let mut blank = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        res.push_str(line.trim_end());
        res.push('\n');
    }
    res.trim().to_string()
}

fn date(d: &Json) -> String {
    let (y, m, day) = (d.get("year").as_u64(), d.get("month").as_u64(), d.get("day").as_u64());
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    match (y, m, day) {
        (Some(y), Some(m), Some(d)) if (1..=12).contains(&m) => format!("{} {}, {}", M[m as usize - 1], d, y),
        (Some(y), Some(m), None) if (1..=12).contains(&m) => format!("{} {}", M[m as usize - 1], y),
        (Some(y), _, _) => y.to_string(),
        _ => String::new(),
    }
}

fn pretty(s: &str) -> String {
    let t = s.replace('_', " ").to_lowercase();
    let mut c = t.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn parse_full(m: &Json, mal_id: u64) -> Full {
    let rank_of = |kind: &str| m.get("rankings").items().iter().find(|r| r.get("type").str() == kind && r.get("allTime").as_bool().unwrap_or(false)).and_then(|r| r.get("rank").as_u64()).unwrap_or(0);
    let mut studios = vec![];
    let mut producers = vec![];
    for e in m.path("studios.edges").items() {
        let n = Named { id: 0, name: e.path("node.name").str() };
        if e.path("node.isAnimationStudio").as_bool().unwrap_or(false) {
            studios.push(n);
        } else {
            producers.push(n);
        }
    }
    let mut streaming = vec![];
    let mut external = vec![];
    for l in m.get("externalLinks").items() {
        let pair = (l.get("site").str(), l.get("url").str());
        if pair.0.is_empty() || pair.1.is_empty() {
            continue;
        }
        if l.get("type").str() == "STREAMING" { streaming.push(pair) } else { external.push(pair) }
    }
    let aired = {
        let (a, b) = (date(m.get("startDate")), date(m.get("endDate")));
        match (a.is_empty(), b.is_empty()) {
            (true, _) => String::new(),
            (false, true) => format!("{} to ?", a),
            (false, false) if a == b => a,
            _ => format!("{} to {}", a, b),
        }
    };
    let mut rel: Vec<Relation> = vec![];
    for e in m.path("relations.edges").items() {
        let Some(id) = e.path("node.idMal").as_u64() else { continue };
        let kind = pretty(&e.get("relationType").str());
        let r = Related { id, kind: e.path("node.type").str().to_lowercase(), name: title_of(e.path("node.title")) };
        match rel.iter_mut().find(|x| x.kind == kind) {
            Some(x) => x.entries.push(r),
            None => rel.push(Relation { kind, entries: vec![r] }),
        }
    }
    let trailer = if m.path("trailer.site").str() == "youtube" && !m.path("trailer.id").str().is_empty() { format!("https://www.youtube.com/watch?v={}", m.path("trailer.id").str()) } else { String::new() };
    let format_name = match m.get("format").str().as_str() {
        "TV_SHORT" => "TV".to_string(),
        "MOVIE" => "Movie".to_string(),
        "TV" | "OVA" | "ONA" => m.get("format").str(),
        other => pretty(other),
    };
    let status = match m.get("status").str().as_str() {
        "FINISHED" => "Finished Airing",
        "RELEASING" => "Currently Airing",
        "NOT_YET_RELEASED" => "Not yet aired",
        "CANCELLED" => "Cancelled",
        "HIATUS" => "On hiatus",
        _ => "",
    };
    let season = m.get("season").str();
    let year = m.get("seasonYear").as_u64();
    let info = Info {
        provider: String::new(),
        id: mal_id,
        url: format!("https://myanimelist.net/anime/{}", mal_id),
        title: m.path("title.romaji").str(),
        title_en: m.path("title.english").str(),
        title_ja: m.path("title.native").str(),
        synonyms: m.get("synonyms").strings(),
        image: { let x = m.path("coverImage.extraLarge").str(); if x.is_empty() { m.path("coverImage.large").str() } else { x } },
        trailer,
        kind: format_name,
        source: pretty(&m.get("source").str()),
        episodes: m.get("episodes").as_u64().map(|n| n as u32),
        status: status.to_string(),
        aired,
        premiered: if season.is_empty() { String::new() } else { format!("{}{}", pretty(&season), year.map(|y| format!(" {}", y)).unwrap_or_default()) },
        broadcast: String::new(),
        duration: m.get("duration").as_u64().map(|d| format!("{} min per ep", d)).unwrap_or_default(),
        rating: String::new(),
        score: m.get("averageScore").as_f64().unwrap_or(0.0) / 10.0,
        scored_by: 0,
        rank: rank_of("RATED"),
        popularity: rank_of("POPULAR"),
        members: m.get("popularity").as_u64().unwrap_or(0),
        favorites: m.get("favourites").as_u64().unwrap_or(0),
        synopsis: strip_html(&m.get("description").str()),
        background: String::new(),
        producers,
        licensors: vec![],
        studios,
        genres: m.get("genres").strings().into_iter().map(|name| Named { id: 0, name }).collect(),
        themes: m
            .get("tags")
            .items()
            .iter()
            .filter(|t| t.get("rank").as_u64().unwrap_or(0) >= 50 && !t.get("isMediaSpoiler").as_bool().unwrap_or(false))
            .take(8)
            .map(|t| Named { id: 0, name: t.get("name").str() })
            .collect(),
        demographics: vec![],
        openings: vec![],
        endings: vec![],
        streaming,
        external,
        facts: vec![],
        relations: rel,
    };
    let characters = m
        .path("characters.edges")
        .items()
        .iter()
        .map(|e| {
            let va = e.path("voiceActors.0");
            Character {
                id: e.path("node.id").as_u64().unwrap_or(0),
                name: e.path("node.name.full").str(),
                role: pretty(&e.get("role").str()),
                image: e.path("node.image.large").str(),
                actor: va.path("name.full").str(),
                actor_lang: if va.is_null() { String::new() } else { "Japanese".to_string() },
                actor_image: va.path("image.large").str(),
            }
        })
        .filter(|c| !c.name.is_empty())
        .collect();
    let staff = m
        .path("staff.edges")
        .items()
        .iter()
        .map(|e| Staff { name: e.path("node.name.full").str(), image: e.path("node.image.large").str(), positions: e.get("role").str() })
        .filter(|s| !s.name.is_empty())
        .collect();
    let reviews = m
        .path("reviews.nodes")
        .items()
        .iter()
        .map(|r| {
            let body = r.get("body").str();
            let score = (r.get("score").as_u64().unwrap_or(0) as f64 / 10.0).round() as u32;
            let tag = if score >= 7 { "Recommended" } else if score >= 5 { "Mixed Feelings" } else { "Not Recommended" };
            let spoiler = body.contains("~!");
            // The markup (pictures, headings, links) stays: the page turns it into text and pictures.
            let text = body.replace("~!", "").replace("!~", "");
            let summary = r.get("summary").str();
            Review {
                user: r.path("user.name").str(),
                avatar: r.path("user.avatar.large").str(),
                date: crate::util::civil(r.get("createdAt").as_u64().unwrap_or(0)).pipe_date(),
                tag: if r.get("score").as_u64().unwrap_or(0) == 0 { String::new() } else { tag.to_string() },
                score,
                text: if text.is_empty() { summary } else { text },
                spoiler,
                url: r.get("siteUrl").str(),
                reactions: r.get("rating").as_u64().unwrap_or(0) as u32,
            }
        })
        .filter(|r| !r.text.is_empty())
        .collect();
    Full { info: Some(info), characters, staff, recs: recs_of(m.path("recommendations.nodes")), reviews }
}

trait PipeDate {
    fn pipe_date(self) -> String;
}
impl PipeDate for (i64, u32, u32, u32, u32, u32) {
    fn pipe_date(self) -> String {
        if self.0 <= 1970 { String::new() } else { format!("{:04}-{:02}-{:02}", self.0, self.1, self.2) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_is_stripped() {
        assert_eq!(strip_html("A<br>B<br><br><br>C &amp; D <i>x</i>"), "A\nB\n\nC & D x");
    }

    #[test]
    fn genre_names() {
        assert_eq!(genre_name("Science Fiction").as_deref(), Some("Sci-Fi"));
        assert_eq!(genre_name("Animation"), None);
        assert_eq!(genre_name("isekai").as_deref(), Some("Isekai"));
        assert!(is_genre("sci-fi"));
        assert!(!is_genre("Isekai"));
    }

    #[test]
    fn full_page_maps_to_mal_shapes() {
        let j = Json::parse(
            r#"{"idMal":31240,"title":{"romaji":"Re:Zero kara Hajimeru Isekai Seikatsu","english":"Re:ZERO -Starting Life in Another World-","native":"Re:ゼロ"},
            "synonyms":["Re Zero"],"coverImage":{"extraLarge":"http://x/c.jpg"},"trailer":{"id":"abc","site":"youtube"},"format":"TV","source":"LIGHT_NOVEL","episodes":25,
            "status":"FINISHED","duration":25,"season":"SPRING","seasonYear":2016,"averageScore":83,"popularity":1000,"favourites":5,"description":"Hi<br>there",
            "startDate":{"year":2016,"month":4,"day":4},"endDate":{"year":2016,"month":9,"day":18},"genres":["Drama","Fantasy"],
            "tags":[{"name":"Isekai","rank":90,"isMediaSpoiler":false},{"name":"Spoil","rank":99,"isMediaSpoiler":true},{"name":"Low","rank":10,"isMediaSpoiler":false}],
            "studios":{"edges":[{"isMain":true,"node":{"name":"White Fox","isAnimationStudio":true}},{"node":{"name":"Kadokawa","isAnimationStudio":false}}]},
            "rankings":[{"rank":42,"type":"RATED","allTime":true},{"rank":7,"type":"POPULAR","allTime":true},{"rank":1,"type":"RATED","allTime":false}],
            "externalLinks":[{"site":"Crunchyroll","url":"http://cr","type":"STREAMING"},{"site":"Official Site","url":"http://o","type":"INFO"}],
            "relations":{"edges":[{"relationType":"SEQUEL","node":{"idMal":42,"title":{"romaji":"Part 2"},"type":"ANIME"}}]},
            "characters":{"edges":[{"role":"MAIN","node":{"id":1,"name":{"full":"Subaru"},"image":{"large":"http://x/s.jpg"}},"voiceActors":[{"name":{"full":"Yusuke"},"image":{"large":"http://x/y.jpg"}}]}]},
            "staff":{"edges":[{"role":"Director","node":{"name":{"full":"Masaharu"},"image":{"large":"http://x/m.jpg"}}}]},
            "recommendations":{"nodes":[{"rating":50,"mediaRecommendation":{"idMal":9,"title":{"romaji":"Other"},"coverImage":{"large":"http://x/o.jpg"},"averageScore":75,"seasonYear":2019,"format":"TV"}},{"rating":1,"mediaRecommendation":{"title":{"romaji":"No mal id"}}}]},
            "reviews":{"nodes":[{"summary":"s","body":"Great ~!spoiler!~ show","score":90,"rating":3,"user":{"name":"bob","avatar":{"large":"http://x/b.jpg"}},"createdAt":1600000000,"siteUrl":"http://r"}]}}"#,
        )
        .unwrap();
        let f = parse_full(&j, 31240);
        let i = f.info.unwrap();
        assert_eq!(i.title, "Re:Zero kara Hajimeru Isekai Seikatsu");
        assert_eq!(i.kind, "TV");
        assert_eq!(i.source, "Light novel");
        assert_eq!(i.status, "Finished Airing");
        assert_eq!(i.aired, "Apr 4, 2016 to Sep 18, 2016");
        assert_eq!(i.premiered, "Spring 2016");
        assert!((i.score - 8.3).abs() < 1e-9);
        assert_eq!((i.rank, i.popularity, i.members), (42, 7, 1000));
        assert_eq!(i.synopsis, "Hi\nthere");
        assert_eq!(i.themes.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["Isekai"]);
        assert_eq!(i.studios[0].name, "White Fox");
        assert_eq!(i.producers[0].name, "Kadokawa");
        assert_eq!(i.streaming.len(), 1);
        assert_eq!(i.external.len(), 1);
        assert_eq!(i.relations[0].kind, "Sequel");
        assert_eq!(i.relations[0].entries[0].id, 42);
        assert_eq!(i.trailer, "https://www.youtube.com/watch?v=abc");
        assert_eq!(f.characters[0].actor, "Yusuke");
        assert_eq!(f.characters[0].role, "Main");
        assert_eq!(f.staff[0].positions, "Director");
        assert_eq!(f.recs.len(), 1);
        assert_eq!(f.recs[0].votes, 50);
        assert_eq!(f.reviews[0].tag, "Recommended");
        assert!(f.reviews[0].spoiler);
        assert_eq!(f.reviews[0].date, "2020-09-13");
        assert_eq!(f.reviews[0].text, "Great spoiler show");
    }
}
