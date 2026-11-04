//! One box for every addon (Preferences > Addons & accounts): the user pastes a link and the app finds out which
//! mode it belongs to. A video addon (a Stremio-style manifest.json), a repository of manga / comic extensions
//! (Tachiyomi / Mihon style index.min.json, served to Suwayomi) or a list of novel plugins (the JSON list that
//! manga and light novel readers use). No repository or addon is named or added here: the user brings them.
use crate::json::Json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Video,
    Images,
    Text,
}

impl Kind {
    pub fn from_id(id: &str) -> Option<Kind> {
        match id {
            "video" => Some(Kind::Video),
            "images" | "manga" => Some(Kind::Images),
            "text" | "novel" => Some(Kind::Text),
            _ => None,
        }
    }
}

/// The kind told by the address alone, when it is clear (no request needed).
pub fn guess(text: &str) -> Option<Kind> {
    let t = text.trim().to_lowercase();
    if t.starts_with("stremio://") {
        return Some(Kind::Video);
    }
    let path = t.split(['?', '#']).next().unwrap_or("").trim_end_matches('/');
    let file = path.rsplit('/').next().unwrap_or("");
    if file == "manifest.json" {
        return Some(Kind::Video);
    }
    if file == "index.min.json" || file == "index.pb" || (file == "index.json" && path.contains("repo")) {
        return Some(Kind::Images);
    }
    if file.ends_with(".json") && file.contains("plugin") {
        return Some(Kind::Text);
    }
    // a GitHub page of a repository: the Mihon repositories are given this way (suwayomi::normalize_repo)
    let host = path.trim_start_matches("https://").trim_start_matches("http://");
    if host.starts_with("github.com/") && !file.ends_with(".json") {
        return Some(Kind::Images);
    }
    None
}

/// The kind told by what the address answered.
pub fn classify(j: &Json) -> Option<Kind> {
    if !j.get("resources").is_null() || (!j.get("id").is_null() && (!j.get("catalogs").is_null() || !j.get("types").is_null())) {
        return Some(Kind::Video);
    }
    let first = j.items().first()?;
    if !first.get("pkg").is_null() || !first.get("apk").is_null() {
        return Some(Kind::Images);
    }
    if !first.get("id").is_null() && !first.get("url").is_null() && (!first.get("site").is_null() || !first.get("lang").is_null()) {
        return Some(Kind::Text);
    }
    None
}

pub const UNKNOWN: &str = "Could not tell which mode this link is for. Pick the kind above the box and add it again.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_from_the_address() {
        assert_eq!(guess("stremio://example.org/abc/manifest.json"), Some(Kind::Video));
        assert_eq!(guess("https://example.org/x/manifest.json?a=1"), Some(Kind::Video));
        assert_eq!(guess("https://raw.example.org/u/r/repo/index.min.json"), Some(Kind::Images));
        assert_eq!(guess("https://github.com/user/repo"), Some(Kind::Images));
        assert_eq!(guess("https://raw.example.org/u/plugins/v3/.dist/plugins.min.json"), Some(Kind::Text));
        assert_eq!(guess("https://example.org/list.json"), None);
    }

    #[test]
    fn classifies_the_answer() {
        let man = Json::parse(r#"{"id":"a.b","version":"1.0.0","resources":["stream"],"types":["movie"]}"#).unwrap();
        assert_eq!(classify(&man), Some(Kind::Video));
        let repo = Json::parse(r#"[{"name":"X","pkg":"eu.x","apk":"x.apk","lang":"en"}]"#).unwrap();
        assert_eq!(classify(&repo), Some(Kind::Images));
        let nov = Json::parse(r#"[{"id":"x","name":"X","site":"https://x","lang":"English","url":"https://x/x.js"}]"#).unwrap();
        assert_eq!(classify(&nov), Some(Kind::Text));
        assert_eq!(classify(&Json::parse("[]").unwrap()), None);
        assert_eq!(classify(&Json::parse(r#"{"a":1}"#).unwrap()), None);
    }
}
