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

/// Many links pasted at once into the same box: a "source pack" so a whole list can be shared as one copy-and-paste
/// code. Accepted shapes:
/// - a JSON object with the lists by mode: `{"video":[...],"images":[...],"text":[...]}` (also "manga"/"novel";
///   a value may be one link or a list; other keys, like a name or a version, are ignored);
/// - a JSON list of links, or of `{"url":..., "kind":...}`;
/// - plain links separated by spaces, new lines, commas or semicolons.
///
/// Gives `None` when the text is a single link (the usual path of the box).
pub fn split_pack(text: &str) -> Option<Vec<(String, Option<Kind>)>> {
    let t = text.trim();
    let mut out: Vec<(String, Option<Kind>)> = Vec::new();
    let push = |out: &mut Vec<(String, Option<Kind>)>, v: &Json, k: Option<Kind>| {
        let mut one = |s: &str, k: Option<Kind>| {
            let s = s.trim();
            if !s.is_empty() && !out.iter().any(|(u, _)| u == s) {
                out.push((s.to_string(), k));
            }
        };
        match v {
            Json::Arr(_) => {
                for it in v.items() {
                    match it.as_str() {
                        Some(s) => one(s, k),
                        None => one(&it.get("url").str(), Kind::from_id(&it.get("kind").str()).or(k)),
                    }
                }
            }
            _ => match v.as_str() {
                Some(s) => one(s, k),
                None => one(&v.get("url").str(), Kind::from_id(&v.get("kind").str()).or(k)),
            },
        }
    };
    if t.starts_with('{') || t.starts_with('[') {
        let j = Json::parse(t).ok()?;
        if t.starts_with('[') {
            push(&mut out, &j, None);
        } else {
            for (key, v) in j.entries() {
                if let Some(k) = Kind::from_id(&key.to_lowercase()) {
                    push(&mut out, v, Some(k));
                }
            }
        }
        return if out.is_empty() { None } else { Some(out) };
    }
    for s in t.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let s = s.trim();
        if !s.is_empty() && !out.iter().any(|(u, _)| u == s) {
            out.push((s.to_string(), None));
        }
    }
    if out.len() > 1 { Some(out) } else { None }
}

/// What a source pack asks to install once its links are in: every manga / comic extension and every novel plugin
/// in these languages (`"install": {"langs": ["pt-BR", "en"], "adult": false}`). Sources for all languages ("all",
/// "multi") come too, as in the lists of Preferences.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackInstall {
    pub langs: Vec<String>,
    pub adult: bool,
}

pub fn pack_install(text: &str) -> Option<PackInstall> {
    let t = text.trim();
    if !t.starts_with('{') {
        return None;
    }
    let j = Json::parse(t).ok()?;
    let i = j.get("install");
    let langs: Vec<String> = match i.get("langs") {
        Json::Arr(_) => i.get("langs").strings(),
        l => l.as_str().map(|s| vec![s.to_string()]).unwrap_or_default(),
    };
    let langs: Vec<String> = langs.iter().map(|l| lang_key(l)).filter(|l| !l.is_empty()).collect();
    if langs.is_empty() {
        return None;
    }
    Some(PackInstall { langs, adult: i.get("adult").as_bool().unwrap_or(false) })
}

/// A language as an extension ("pt-BR", "pt_br") or a novel plugin ("Português", "English", "日本語") gives it, as a code.
pub fn lang_key(l: &str) -> String {
    let c = crate::manga::lang_code(l);
    let named = match c.as_str() {
        "english" => "en",
        "português" | "portugues" | "portuguese" | "português (brasil)" | "portuguese (brazil)" => "pt",
        "日本語" | "japanese" => "ja",
        "español" | "espanol" | "spanish" => "es",
        "français" | "french" => "fr",
        "deutsch" | "german" => "de",
        "italiano" | "italian" => "it",
        "русский" | "russian" => "ru",
        "中文" | "chinese" => "zh",
        "한국어" | "korean" => "ko",
        "multi" | "multilingual" | "all" => "all",
        _ => "",
    };
    if named.is_empty() { c } else { named.to_string() }
}

impl PackInstall {
    /// Whether a source in the language `lang` is wanted ("pt" takes "pt-br" and the other way round).
    pub fn wants(&self, lang: &str) -> bool {
        let l = lang_key(lang);
        if l == "all" {
            return true;
        }
        let base = |s: &str| s.split('-').next().unwrap_or("").to_string();
        self.langs.iter().any(|w| *w == l || base(w) == l || base(&l) == *w || (base(w) == base(&l) && !w.contains('-')))
    }
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
    fn splits_a_pack() {
        assert_eq!(split_pack("https://example.org/a/manifest.json"), None);
        let p = split_pack(r#"{"name":"x","video":["https://a.example/manifest.json","https://b.example/manifest.json"],"images":"https://github.com/u/r","novel":["https://c.example/plugins.min.json"]}"#).unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(p[0], ("https://a.example/manifest.json".to_string(), Some(Kind::Video)));
        assert_eq!(p[2].1, Some(Kind::Images));
        assert_eq!(p[3].1, Some(Kind::Text));
        let l = split_pack(r#"["https://a.example/manifest.json", {"url":"https://github.com/u/r","kind":"images"}]"#).unwrap();
        assert_eq!(l[1], ("https://github.com/u/r".to_string(), Some(Kind::Images)));
        let plain = split_pack("https://a.example/manifest.json\nhttps://a.example/manifest.json https://b.example/x.json").unwrap();
        assert_eq!(plain.len(), 2);
        assert_eq!(split_pack("{not json"), None);
    }

    #[test]
    fn reads_what_to_install() {
        assert_eq!(pack_install(r#"{"video":["https://a.example/manifest.json"]}"#), None);
        let i = pack_install(r#"{"install":{"langs":["pt-BR","en"],"adult":false}}"#).unwrap();
        assert_eq!(i.langs, vec!["pt-br".to_string(), "en".to_string()]);
        assert!(i.wants("pt-BR") && i.wants("en") && i.wants("all") && i.wants("Multi"));
        assert!(i.wants("English") && i.wants("Português"));
        assert!(!i.wants("ja") && !i.wants("es-419") && !i.wants("pt-PT"));
        let j = pack_install(r#"{"install":{"langs":"ja"}}"#).unwrap();
        assert!(j.wants("日本語") && j.wants("ja") && !j.wants("en"));
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
