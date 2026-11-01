//! Opening / ending / recap times from places other than the video file:
//! AniSkip (crowdsourced, by MyAnimeList id) and IntroDB (crowdsourced, by IMDb id).
use crate::animeskip::{Kind, Segment};
use crate::http::{get_json, Http};
use crate::json::Json;
use crate::util::url_encode;

fn seg(start: f64, end: f64, kind: Kind, name: &str) -> Segment {
    Segment { start, end: Some(end), kind, name: name.to_string() }
}

/// AniSkip answers with the times of one release; `episode_length` makes it return only matching releases.
pub fn aniskip(http: &dyn Http, mal_id: u64, episode: i64, episode_length: f64) -> Result<Vec<Segment>, String> {
    let url = format!(
        "https://api.aniskip.com/v2/skip-times/{}/{}?types=op&types=ed&types=mixed-op&types=mixed-ed&types=recap&episodeLength={}",
        mal_id,
        episode,
        episode_length.round() as i64
    );
    let j = get_json(http, &url, 15)?;
    Ok(parse_aniskip(&j))
}

pub fn parse_aniskip(j: &Json) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    for r in j.get("results").items() {
        let (Some(a), Some(b)) = (r.path("interval.startTime").as_f64(), r.path("interval.endTime").as_f64()) else { continue };
        let (kind, name) = match r.get("skipType").str().as_str() {
            "op" | "mixed-op" => (Kind::Intro, "Opening"),
            "ed" | "mixed-ed" => (Kind::Credits, "Ending"),
            "recap" => (Kind::Recap, "Recap"),
            _ => continue,
        };
        if b > a {
            out.push(seg(a, b, kind, name));
        }
    }
    out.sort_by(|x, y| x.start.partial_cmp(&y.start).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// IntroDB (introdb.app): intro, recap and outro by IMDb id, season and episode.
pub fn introdb(http: &dyn Http, imdb: &str, season: i64, episode: i64) -> Result<Vec<Segment>, String> {
    let url = format!("https://api.introdb.app/segments?imdb_id={}&season={}&episode={}", url_encode(imdb), season, episode);
    let j = get_json(http, &url, 15)?;
    Ok(parse_introdb(&j))
}

pub fn parse_introdb(j: &Json) -> Vec<Segment> {
    let mut out = Vec::new();
    for (key, kind, name) in [("recap", Kind::Recap, "Recap"), ("intro", Kind::Intro, "Opening"), ("outro", Kind::Credits, "Ending")] {
        let s = j.get(key);
        if let (Some(a), Some(b)) = (s.get("start_sec").as_f64(), s.get("end_sec").as_f64()) {
            if b > a {
                out.push(seg(a, b, kind, name));
            }
        }
    }
    out.sort_by(|x, y| x.start.partial_cmp(&y.start).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Finds the MyAnimeList id of a show by its name (Jikan search).
pub fn find_mal_id(http: &dyn Http, name: &str, season: Option<i64>) -> Result<Option<u64>, String> {
    let q = match season {
        Some(s) if s > 1 => format!("{} season {}", name, s),
        _ => name.to_string(),
    };
    // AniList first: it answers fast everywhere (Jikan is blocked or slow on some networks).
    if let Ok(list) = crate::anilist::search(http, &q) {
        let want = name.trim().to_lowercase();
        let hit = list.iter().find(|f| f.names.iter().any(|n| !n.is_empty() && n.to_lowercase().contains(&want))).or_else(|| list.first());
        if let Some(f) = hit {
            return Ok(Some(f.id));
        }
    }
    let url = format!("https://api.jikan.moe/v4/anime?q={}&limit=5&order_by=popularity", url_encode(&q));
    let j = get_json(http, &url, 15)?;
    let want = name.trim().to_lowercase();
    let list = j.get("data").items();
    let hit = list
        .iter()
        .find(|a| a.get("title").str().to_lowercase().contains(&want) || a.get("title_english").str().to_lowercase().contains(&want))
        .or_else(|| list.first());
    Ok(hit.and_then(|a| a.get("mal_id").as_f64()).map(|f| f as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_aniskip() {
        let j = Json::parse(r#"{"found":true,"results":[{"interval":{"startTime":47.3,"endTime":137.3},"skipType":"op"},{"interval":{"startTime":1300,"endTime":1390},"skipType":"ed"}]}"#).unwrap();
        let s = parse_aniskip(&j);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].kind, Kind::Intro);
        assert_eq!(s[1].end, Some(1390.0));
    }

    #[test]
    fn reads_introdb() {
        let j = Json::parse(r#"{"intro":null,"recap":null,"outro":{"start_sec":3431,"end_sec":3500}}"#).unwrap();
        let s = parse_introdb(&j);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kind, Kind::Credits);
    }
}
