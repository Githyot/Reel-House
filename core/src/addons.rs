//! Client for the Stremio addon protocol (https://github.com/Stremio/stremio-addon-sdk).
//! An addon is an HTTP server with /manifest.json and /{resource}/{type}/{id}[/{extra}].json endpoints.

use crate::error::{Error, Result};
use crate::settings::SettingsStore;
use crate::streams::Stream;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// Installed on first run. Cinemeta is the metadata source (posters, logos, backgrounds, ratings).
pub const DEFAULT_ADDONS: &[&str] = &[
    "https://v3-cinemeta.strem.io/manifest.json",
    "https://mal-stremio.vercel.app/7032624/manifest.json",
];

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub resources: Vec<ResourceDecl>,
    #[serde(default)]
    pub catalogs: Vec<CatalogDecl>,
    #[serde(default)]
    pub id_prefixes: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ResourceDecl {
    Name(String),
    Full {
        name: String,
        #[serde(default)]
        types: Vec<String>,
        #[serde(default, rename = "idPrefixes")]
        id_prefixes: Option<Vec<String>>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDecl {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub extra: Vec<ExtraDecl>,
    #[serde(default)]
    pub extra_required: Vec<String>,
    #[serde(default)]
    pub extra_supported: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExtraDecl {
    pub name: String,
    #[serde(default)]
    pub is_required: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstalledAddon {
    pub url: String,
    pub enabled: bool,
    pub manifest: Manifest,
}

#[derive(Serialize, Clone, Debug)]
pub struct CatalogRow {
    pub key: String,
    pub title: String,
    pub addon: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub items: Vec<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Subtitle {
    #[serde(default)]
    pub id: Option<String>,
    pub url: String,
    #[serde(default)]
    pub lang: String,
    #[serde(default, rename = "addonName")]
    pub addon_name: String,
}

pub fn normalize_manifest_url(input: &str) -> Result<String> {
    let mut s = input.trim().to_string();
    if s.len() >= 10 && s[..10].eq_ignore_ascii_case("stremio://") {
        s = format!("https://{}", &s[10..]);
    }
    if !(s.starts_with("http://") || s.starts_with("https://")) {
        return Err(Error::msg("Enter the addon's manifest link. It starts with https:// or stremio://."));
    }
    let mut url = url::Url::parse(&s).map_err(|_| Error::msg("That does not look like a valid link."))?;
    url.set_fragment(None);
    if !url.path().ends_with("/manifest.json") {
        let p = format!("{}/manifest.json", url.path().trim_end_matches('/'));
        url.set_path(&p);
    }
    Ok(url.to_string())
}

pub fn base_of(manifest_url: &str) -> String {
    let no_query = manifest_url.split('?').next().unwrap_or(manifest_url);
    no_query.trim_end_matches("/manifest.json").to_string()
}

fn enc(s: &str) -> String {
    urlencoding::encode(s).replace("%3A", ":")
}

pub fn resource_url(addon_url: &str, resource: &str, kind: &str, id: &str, extra: &[(&str, &str)]) -> String {
    let extra_str = if extra.is_empty() {
        String::new()
    } else {
        format!("/{}", extra.iter().map(|(k, v)| format!("{}={}", enc(k), enc(v))).collect::<Vec<_>>().join("&"))
    };
    format!("{}/{}/{}/{}{}.json", base_of(addon_url), resource, enc(kind), enc(id), extra_str)
}

fn entries(m: &Manifest) -> Vec<(String, Vec<String>, Option<Vec<String>>)> {
    m.resources
        .iter()
        .map(|r| match r {
            ResourceDecl::Name(n) => (n.clone(), m.types.clone(), m.id_prefixes.clone()),
            ResourceDecl::Full { name, types, id_prefixes } => (
                name.clone(),
                if types.is_empty() { m.types.clone() } else { types.clone() },
                id_prefixes.clone().or_else(|| m.id_prefixes.clone()),
            ),
        })
        .collect()
}

pub fn supports(m: &Manifest, resource: &str, kind: &str, id: &str) -> bool {
    entries(m).iter().any(|(name, types, prefixes)| {
        name == resource
            && (types.is_empty() || types.iter().any(|t| t == kind))
            && prefixes.as_ref().map_or(true, |p| id.is_empty() || p.iter().any(|x| id.starts_with(x.as_str())))
    })
}

fn has_resource(m: &Manifest, resource: &str) -> bool {
    entries(m).iter().any(|(n, _, _)| n == resource)
}

async fn get_json(http: &reqwest::Client, url: &str, timeout: Duration) -> Result<Value> {
    let res = http.get(url).timeout(timeout).header("Accept", "application/json").send().await?;
    if !res.status().is_success() {
        let host = url::Url::parse(url).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_default();
        return Err(Error::msg(format!("{host} answered {}", res.status().as_u16())));
    }
    Ok(res.json().await?)
}

pub struct AddonManager {
    settings: Arc<SettingsStore>,
    http: reqwest::Client,
}

impl AddonManager {
    pub fn new(settings: Arc<SettingsStore>, http: reqwest::Client) -> Self {
        Self { settings, http }
    }

    pub fn list(&self) -> Vec<InstalledAddon> {
        self.settings.get().addons
    }

    pub fn active(&self) -> Vec<InstalledAddon> {
        self.list().into_iter().filter(|a| a.enabled).collect()
    }

    async fn fetch_manifest(&self, url: &str) -> Result<Manifest> {
        let v = get_json(&self.http, url, Duration::from_secs(15)).await?;
        let m: Manifest = serde_json::from_value(v)
            .map_err(|_| Error::msg("That link did not return a valid addon manifest."))?;
        if m.resources.is_empty() {
            return Err(Error::msg("The manifest does not list any resources."));
        }
        Ok(m)
    }

    pub async fn install(&self, input: &str) -> Result<Vec<InstalledAddon>> {
        let url = normalize_manifest_url(input)?;
        let manifest = self.fetch_manifest(&url).await?;
        self.settings.update(|s| {
            s.addons.retain(|a| a.url != url && a.manifest.id != manifest.id);
            s.addons.push(InstalledAddon { url: url.clone(), enabled: true, manifest: manifest.clone() });
        })?;
        Ok(self.list())
    }

    pub fn remove(&self, url: &str) -> Result<Vec<InstalledAddon>> {
        self.settings.update(|s| s.addons.retain(|a| a.url != url))?;
        Ok(self.list())
    }

    pub fn toggle(&self, url: &str, enabled: bool) -> Result<Vec<InstalledAddon>> {
        self.settings.update(|s| {
            for a in s.addons.iter_mut().filter(|a| a.url == url) {
                a.enabled = enabled;
            }
        })?;
        Ok(self.list())
    }

    /// Installs the default addons on first run. Safe to call on every start: it does nothing once
    /// the defaults have been installed, and retries next time if the network was down.
    pub async fn ensure_defaults(&self, defaults: &[&str]) {
        if self.settings.get().first_run_done {
            return;
        }
        let mut installed = 0;
        for url in defaults {
            if self.list().iter().any(|a| a.url == *url) || self.install(url).await.is_ok() {
                installed += 1;
            }
        }
        if installed == defaults.len() {
            let _ = self.settings.update(|s| s.first_run_done = true);
        }
    }

    fn catalog_needs_input(c: &CatalogDecl) -> bool {
        c.extra.iter().any(|e| e.is_required && e.name != "skip") || c.extra_required.iter().any(|n| n != "skip")
    }

    fn catalog_searchable(c: &CatalogDecl) -> bool {
        c.extra.iter().any(|e| e.name == "search") || c.extra_supported.iter().any(|n| n == "search")
    }

    async fn load_catalog(&self, addon: &InstalledAddon, c: &CatalogDecl, extra: &[(&str, &str)]) -> Option<CatalogRow> {
        let url = resource_url(&addon.url, "catalog", &c.kind, &c.id, extra);
        let v = get_json(&self.http, &url, Duration::from_secs(15)).await.ok()?;
        let items: Vec<Value> = v.get("metas")?.as_array()?.iter().take(40).cloned().collect();
        if items.is_empty() {
            return None;
        }
        Some(CatalogRow {
            key: format!("{}/{}/{}", addon.manifest.id, c.kind, c.id),
            title: c.name.clone().unwrap_or_else(|| c.id.clone()),
            addon: addon.manifest.name.clone(),
            kind: c.kind.clone(),
            items,
        })
    }

    pub async fn board(&self, limit: usize) -> Vec<CatalogRow> {
        let active = self.active();
        let mut jobs = Vec::new();
        for a in &active {
            if !has_resource(&a.manifest, "catalog") {
                continue;
            }
            for c in a.manifest.catalogs.iter().filter(|c| !Self::catalog_needs_input(c)) {
                jobs.push((a, c));
            }
        }
        join_all(jobs.into_iter().take(limit).map(|(a, c)| self.load_catalog(a, c, &[]))).await.into_iter().flatten().collect()
    }

    pub async fn search(&self, query: &str) -> Vec<CatalogRow> {
        let active = self.active();
        let mut jobs = Vec::new();
        for a in &active {
            for c in a.manifest.catalogs.iter().filter(|c| Self::catalog_searchable(c)) {
                jobs.push((a, c));
            }
        }
        join_all(jobs.into_iter().take(12).map(|(a, c)| self.load_catalog(a, c, &[("search", query)]))).await.into_iter().flatten().collect()
    }

    pub async fn meta(&self, kind: &str, id: &str) -> Option<Value> {
        for a in self.active() {
            if !supports(&a.manifest, "meta", kind, id) {
                continue;
            }
            let url = resource_url(&a.url, "meta", kind, id, &[]);
            if let Ok(v) = get_json(&self.http, &url, Duration::from_secs(15)).await {
                if let Some(m) = v.get("meta") {
                    if m.is_object() {
                        return Some(m.clone());
                    }
                }
            }
        }
        None
    }

    pub async fn streams(&self, kind: &str, id: &str) -> Vec<Stream> {
        let active = self.active();
        let jobs = active.iter().filter(|a| supports(&a.manifest, "stream", kind, id)).map(|a| async move {
            let url = resource_url(&a.url, "stream", kind, id, &[]);
            let v = get_json(&self.http, &url, Duration::from_secs(25)).await?;
            let list: Vec<Stream> = serde_json::from_value(v.get("streams").cloned().unwrap_or(Value::Array(vec![]))).unwrap_or_default();
            Ok::<_, Error>(list.into_iter().map(|mut s| { s.addon_name = a.manifest.name.clone(); s }).collect::<Vec<_>>())
        });
        join_all(jobs).await.into_iter().filter_map(|r| r.ok()).flatten().collect()
    }

    pub async fn subtitles(&self, kind: &str, id: &str) -> Vec<Subtitle> {
        let active = self.active();
        let jobs = active.iter().filter(|a| supports(&a.manifest, "subtitles", kind, id)).map(|a| async move {
            let url = resource_url(&a.url, "subtitles", kind, id, &[]);
            let v = get_json(&self.http, &url, Duration::from_secs(20)).await?;
            let list: Vec<Subtitle> = serde_json::from_value(v.get("subtitles").cloned().unwrap_or(Value::Array(vec![]))).unwrap_or_default();
            Ok::<_, Error>(list.into_iter().map(|mut s| { s.addon_name = a.manifest.name.clone(); s }).collect::<Vec<_>>())
        });
        join_all(jobs).await.into_iter().filter_map(|r| r.ok()).flatten().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[test]
    fn manifest_urls_are_normalised() {
        assert_eq!(normalize_manifest_url("stremio://example.com/abc/manifest.json").unwrap(), "https://example.com/abc/manifest.json");
        assert_eq!(normalize_manifest_url("https://example.com/cfg/").unwrap(), "https://example.com/cfg/manifest.json");
        assert!(normalize_manifest_url("not a url").is_err());
    }

    #[test]
    fn resource_urls_follow_the_protocol() {
        let u = "https://example.com/cfg/manifest.json";
        assert_eq!(resource_url(u, "stream", "series", "tt1:1:2", &[]), "https://example.com/cfg/stream/series/tt1:1:2.json");
        assert_eq!(resource_url(u, "catalog", "movie", "top", &[("search", "a b")]), "https://example.com/cfg/catalog/movie/top/search=a%20b.json");
        assert_eq!(base_of("https://x.io/a/manifest.json?k=1"), "https://x.io/a");
    }

    fn manifest(json: &str) -> Manifest {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn supports_honours_types_and_prefixes() {
        let m = manifest(r#"{"id":"a","name":"A","version":"1","types":["movie"],"idPrefixes":["tt"],
            "resources":["stream",{"name":"subtitles","types":["series"],"idPrefixes":["kitsu:"]}]}"#);
        assert!(supports(&m, "stream", "movie", "tt1"));
        assert!(!supports(&m, "stream", "movie", "kitsu:1"));
        assert!(supports(&m, "subtitles", "series", "kitsu:1"));
        assert!(!supports(&m, "meta", "movie", "tt1"));
    }

    #[tokio::test]
    async fn install_query_and_remove_against_a_mock_addon() {
        let server = MockServer::start();
        let manifest_json = r#"{"id":"x.addon","name":"X","version":"1.0.0","types":["movie"],
            "resources":["catalog","meta","stream","subtitles"],
            "catalogs":[{"type":"movie","id":"top","name":"Top"},
                        {"type":"movie","id":"s","extra":[{"name":"search","isRequired":true}]}]}"#;
        server.mock(|w, t| { w.method(GET).path("/manifest.json"); t.status(200).body(manifest_json); });
        server.mock(|w, t| { w.method(GET).path("/catalog/movie/top.json"); t.status(200).json_body(serde_json::json!({"metas":[{"id":"tt1","type":"movie","name":"One"}]})); });
        server.mock(|w, t| { w.method(GET).path("/catalog/movie/s/search=one.json"); t.status(200).json_body(serde_json::json!({"metas":[{"id":"tt1","type":"movie","name":"One"}]})); });
        server.mock(|w, t| { w.method(GET).path("/meta/movie/tt1.json"); t.status(200).json_body(serde_json::json!({"meta":{"id":"tt1","name":"One","background":"http://bg"}})); });
        server.mock(|w, t| { w.method(GET).path("/stream/movie/tt1.json"); t.status(200).json_body(serde_json::json!({"streams":[{"url":"http://v/1.mp4","name":"1080p"},{"infoHash":"abc","title":"x"}]})); });
        server.mock(|w, t| { w.method(GET).path("/subtitles/movie/tt1.json"); t.status(200).json_body(serde_json::json!({"subtitles":[{"id":"1","lang":"eng","url":"http://s/1.srt"}]})); });

        let dir = tempfile::tempdir().unwrap();
        let mgr = AddonManager::new(SettingsStore::load(dir.path()), reqwest::Client::new());
        mgr.install(&format!("{}/manifest.json", server.base_url())).await.unwrap();
        assert_eq!(mgr.list().len(), 1);
        let board = mgr.board(12).await;
        assert_eq!(board.len(), 1, "the search-only catalog stays off the board");
        assert_eq!(board[0].items[0]["name"], "One");
        assert_eq!(mgr.search("one").await.len(), 1);
        assert_eq!(mgr.meta("movie", "tt1").await.unwrap()["background"], "http://bg");
        let streams = mgr.streams("movie", "tt1").await;
        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].addon_name, "X");
        assert_eq!(mgr.subtitles("movie", "tt1").await[0].lang, "eng");
        let url = mgr.list()[0].url.clone();
        mgr.toggle(&url, false).unwrap();
        assert!(mgr.streams("movie", "tt1").await.is_empty());
        mgr.remove(&url).unwrap();
        assert!(mgr.list().is_empty());
    }

    #[tokio::test]
    async fn ensure_defaults_installs_once_and_marks_first_run_done() {
        let server = MockServer::start();
        server.mock(|w, t| { w.method(GET).path("/manifest.json"); t.status(200).body(r#"{"id":"c","name":"Cinemeta","version":"3","resources":["catalog"]}"#); });
        let dir = tempfile::tempdir().unwrap();
        let settings = SettingsStore::load(dir.path());
        let mgr = AddonManager::new(settings.clone(), reqwest::Client::new());
        let url = format!("{}/manifest.json", server.base_url());
        mgr.ensure_defaults(&[url.as_str()]).await;
        assert_eq!(mgr.list().len(), 1);
        assert!(settings.get().first_run_done);
        mgr.remove(&url).unwrap();
        mgr.ensure_defaults(&[url.as_str()]).await; // a removed default must stay removed
        assert!(mgr.list().is_empty());
    }

    #[test]
    fn default_addons_include_cinemeta_first() {
        assert_eq!(DEFAULT_ADDONS[0], "https://v3-cinemeta.strem.io/manifest.json");
        assert!(DEFAULT_ADDONS.iter().any(|u| u.contains("mal-stremio")));
    }
}
