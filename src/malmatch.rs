//! Finds the MyAnimeList id of a manga or novel by its title, so reading can be tracked
//! even when the source does not say which MAL entry a title is. AniList knows the MAL id of most titles;
//! Jikan (an unofficial MAL mirror) is the second try.
use crate::http::{Http, Request};
use crate::json::Json;

pub fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// The first candidate whose names include the title (the same letters, ignoring case and punctuation);
/// else one whose name contains it or is contained by it while being at least 80% as long.
pub fn best(title: &str, candidates: &[(u64, Vec<String>)]) -> Option<u64> {
    let want = normalize(title);
    if want.len() < 2 {
        return None;
    }
    for (id, names) in candidates {
        if *id > 0 && names.iter().any(|n| normalize(n) == want) {
            return Some(*id);
        }
    }
    for (id, names) in candidates {
        if *id == 0 {
            continue;
        }
        for n in names {
            let got = normalize(n);
            if got.len() < 4 {
                continue;
            }
            let (short, long) = if got.len() < want.len() { (&got, &want) } else { (&want, &got) };
            if long.contains(short.as_str()) && short.len() * 10 >= long.len() * 8 {
                return Some(*id);
            }
        }
    }
    None
}

fn anilist(http: &dyn Http, title: &str, kind: &str) -> Option<u64> {
    let query = "query($s:String,$t:MediaType){Page(perPage:8){media(search:$s,type:$t){idMal title{romaji english native} synonyms}}}";
    let body = Json::obj(vec![("query", query.into()), ("variables", Json::obj(vec![("s", title.into()), ("t", kind.into())]))]).dump();
    let r = http.send(&Request::post("https://graphql.anilist.co").header("Accept", "application/json").json_body(body).timeout(15)).ok()?;
    if !r.ok() {
        return None;
    }
    let j = r.json().ok()?;
    let cands: Vec<(u64, Vec<String>)> = j
        .path("data.Page.media")
        .items()
        .iter()
        .map(|m| {
            let mut names: Vec<String> = ["romaji", "english", "native"].iter().map(|k| m.path(&format!("title.{}", k)).str()).filter(|n| !n.is_empty()).collect();
            names.extend(m.get("synonyms").strings());
            (m.get("idMal").as_u64().unwrap_or(0), names)
        })
        .collect();
    best(title, &cands)
}

fn jikan(http: &dyn Http, title: &str, kind: &str) -> Option<u64> {
    let url = format!("https://api.jikan.moe/v4/{}?q={}&limit=8", kind, crate::util::url_encode(title));
    let j = crate::http::get_json(http, &url, 15).ok()?;
    let cands: Vec<(u64, Vec<String>)> = j
        .get("data")
        .items()
        .iter()
        .map(|m| {
            let mut names: Vec<String> = vec![m.get("title").str(), m.get("title_english").str()];
            names.extend(m.get("titles").items().iter().map(|t| t.get("title").str()));
            names.retain(|n| !n.is_empty());
            (m.get("mal_id").as_u64().unwrap_or(0), names)
        })
        .collect();
    best(title, &cands)
}

/// `novel` is true for light and web novels (MAL keeps them in the manga list too).
pub fn resolve(http: &dyn Http, title: &str) -> Option<u64> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    anilist(http, title, "MANGA").or_else(|| jikan(http, title, "manga"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn matching() {
        let c = vec![(1, vec!["Berserk: The Prototype".to_string()]), (2, vec!["Berserk".to_string(), "ベルセルク".to_string()])];
        assert_eq!(best("berserk!", &c), Some(2));
        assert_eq!(best("Solo Leveling", &c), None);
        assert_eq!(best("x", &c), None);
        // A longer name that merely starts with the title is not enough.
        assert_eq!(best("Berserk", &[(1, vec!["Berserk: The Prototype".to_string()])]), None);
        // A near match (one extra word of a long title) is.
        assert_eq!(best("Tensei shitara Slime Datta Ken", &[(9, vec!["Tensei shitara Slime Datta Ken.".to_string()])]), Some(9));
        // No MAL id on the candidate: skipped.
        assert_eq!(best("Berserk", &[(0, vec!["Berserk".to_string()])]), None);
    }

    #[test]
    fn anilist_then_jikan() {
        let h = Fake::new().route("graphql.anilist.co", 200, r#"{"data":{"Page":{"media":[{"idMal":null,"title":{"romaji":"Other","english":null,"native":null},"synonyms":[]},{"idMal":2,"title":{"romaji":"Berserk","english":"Berserk","native":"ベルセルク"},"synonyms":[]}]}}}"#);
        assert_eq!(resolve(&h, "Berserk"), Some(2));
        let h = Fake::new().route("graphql.anilist.co", 500, "no").route("api.jikan.moe", 200, r#"{"data":[{"mal_id":44,"title":"Solo Leveling","title_english":null,"titles":[{"title":"Na Honjaman Level-Up"}]}]}"#);
        assert_eq!(resolve(&h, "Solo Leveling"), Some(44));
        assert_eq!(resolve(&Fake::new(), "Nothing at all"), None);
    }
}
