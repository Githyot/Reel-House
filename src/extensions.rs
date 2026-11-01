//! The catalogs of reading sources: the extension repositories of the Images mode (Tachiyomi / Mihon format) and of the
//! Text mode (IReader). Only the lists are read here.
use crate::http::Http;
use crate::json::Json;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ext {
    pub name: String,
    pub pkg: String,
    pub lang: String,
    pub version: String,
    pub nsfw: bool,
    /// File name of the package in the repository.
    pub apk: String,
    /// Sites this extension reads (Mihon extensions list them).
    pub sources: Vec<String>,
}

/// The address of the JSON copy of a repository index. A repository publishes `index.pb` (the one Mihon is given) and,
/// in the same folder, `index.min.json` with the same list.
pub fn json_index_url(repo: &str) -> String {
    let r = repo.trim();
    match r.strip_suffix("index.pb") {
        Some(base) => format!("{}index.min.json", base),
        None => {
            if r.ends_with(".json") {
                r.to_string()
            } else {
                format!("{}/index.min.json", r.trim_end_matches('/'))
            }
        }
    }
}

/// The address a package is downloaded from: next to the index, in the `apk` folder.
pub fn package_url(repo: &str, apk: &str) -> String {
    let base = match repo.trim().rfind('/') {
        Some(i) => &repo.trim()[..i],
        None => repo.trim(),
    };
    format!("{}/apk/{}", base, apk)
}

pub fn parse_index(j: &Json) -> Vec<Ext> {
    let flag = |v: &Json| v.as_bool().unwrap_or_else(|| v.as_u64().unwrap_or(0) > 0);
    let mut out: Vec<Ext> = j
        .items()
        .iter()
        .filter_map(|e| {
            let name = e.get("name").str();
            if name.is_empty() {
                return None;
            }
            // Mihon names are "Tachiyomi: Site"; the prefix says nothing to a reader.
            let name = name.strip_prefix("Tachiyomi: ").unwrap_or(&name).to_string();
            Some(Ext {
                name,
                pkg: e.get("pkg").str(),
                lang: e.get("lang").str(),
                version: e.get("version").str(),
                nsfw: flag(e.get("nsfw")),
                apk: e.get("apk").str(),
                sources: e.get("sources").items().iter().map(|s| s.get("name").str()).filter(|n| !n.is_empty()).collect(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn fetch_index(http: &dyn Http, repo: &str) -> Result<Vec<Ext>, String> {
    let url = json_index_url(repo);
    let j = crate::http::get_json(http, &url, 30).map_err(|e| format!("Could not read the repository ({})", e))?;
    let list = parse_index(&j);
    if list.is_empty() {
        return Err("The repository answered, but its list is empty or in a format this app does not know.".to_string());
    }
    Ok(list)
}

/// Languages present in a list, most common first.
pub fn languages(list: &[Ext]) -> Vec<(String, usize)> {
    let mut m: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for e in list {
        *m.entry(e.lang.as_str()).or_insert(0) += 1;
    }
    let mut v: Vec<(String, usize)> = m.into_iter().map(|(k, n)| (k.to_string(), n)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn addresses() {
        assert_eq!(json_index_url("https://github.com/example-user/extensions/raw/repo/index.pb"), "https://github.com/example-user/extensions/raw/repo/index.min.json");
        assert_eq!(json_index_url("https://x.org/r/index.min.json"), "https://x.org/r/index.min.json");
        assert_eq!(json_index_url("https://x.org/r/"), "https://x.org/r/index.min.json");
        assert_eq!(package_url("https://x.org/repo/index.min.json", "a.apk"), "https://x.org/repo/apk/a.apk");
    }

    #[test]
    fn reads_both_formats() {
        let mihon = Json::parse(r#"[{"name":"Tachiyomi: Zeta","pkg":"eu.z","apk":"z.apk","lang":"en","version":"1.4.2","nsfw":1,"sources":[{"name":"Zeta Site","lang":"en","id":"1","baseUrl":"https://z"}]},{"name":"Alpha","pkg":"eu.a","lang":"pt","version":"1.4.1","nsfw":0}]"#).unwrap();
        let v = parse_index(&mihon);
        assert_eq!(v[0].name, "Alpha");
        assert!(!v[0].nsfw);
        assert_eq!(v[1].name, "Zeta");
        assert!(v[1].nsfw);
        assert_eq!(v[1].sources, vec!["Zeta Site"]);
        let ireader = Json::parse(r#"[{"pkg":"ireader.a.en","apk":"a.apk","name":"AllNovelFull","lang":"en","version":"1.2","nsfw":false}]"#).unwrap();
        assert_eq!(parse_index(&ireader)[0].name, "AllNovelFull");
        assert_eq!(languages(&parse_index(&mihon)).len(), 2);
    }

    #[test]
    fn fetches_and_complains() {
        let http = Fake::new().route("good/index.min.json", 200, r#"[{"name":"A","lang":"en"}]"#).route("empty/index.min.json", 200, "[]");
        assert_eq!(fetch_index(&http, "https://good/index.pb").unwrap().len(), 1);
        assert!(fetch_index(&http, "https://empty/index.pb").is_err());
    }
}
