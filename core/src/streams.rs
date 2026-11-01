//! Stream objects from addons: parsing the spec lines, ranking, magnets and the quality list.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyHeaders {
    #[serde(default)]
    pub request: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct BehaviorHints {
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub video_size: Option<u64>,
    #[serde(default)]
    pub proxy_headers: Option<ProxyHeaders>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub yt_id: Option<String>,
    #[serde(default)]
    pub info_hash: Option<String>,
    #[serde(default)]
    pub file_idx: Option<u32>,
    #[serde(default)]
    pub external_url: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub behavior_hints: Option<BehaviorHints>,
    #[serde(default)]
    pub addon_name: String,
}

/// What the UI shows for one stream. The full stream travels along so the UI can send it back to play.
#[derive(Serialize, Clone, Debug)]
pub struct StreamView {
    pub kind: &'static str,
    pub quality: String,
    pub resolution: u32,
    pub heading: String,
    pub lines: Vec<String>,
    pub seeders: u32,
    pub size: Option<String>,
    pub addon: String,
    pub stream: Stream,
}

static RES_4K: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)2160|\b4k\b|\buhd\b").unwrap());
static SEEDERS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(?:\u{1F464}|seeders?)[:\s]*(\d+)").unwrap());
static SIZE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*(GB|MB|GiB|MiB)").unwrap());

pub fn parse_resolution(text: &str) -> u32 {
    let t = text.to_lowercase();
    if RES_4K.is_match(&t) {
        2160
    } else if t.contains("1440") {
        1440
    } else if t.contains("1080") {
        1080
    } else if t.contains("720") {
        720
    } else if t.contains("480") {
        480
    } else {
        0
    }
}

pub fn quality_label(res: u32) -> &'static str {
    match res {
        2160.. => "4K",
        1440.. => "1440p",
        1080.. => "1080p",
        720.. => "720p",
        480.. => "480p",
        _ => "SD",
    }
}

pub fn parse_seeders(text: &str) -> u32 {
    SEEDERS.captures(text).and_then(|c| c[1].parse().ok()).unwrap_or(0)
}

fn format_size(bytes: u64) -> String {
    let gb = bytes as f64 / 1_073_741_824.0;
    if gb >= 1.0 { format!("{gb:.1} GB") } else { format!("{:.0} MB", bytes as f64 / 1_048_576.0) }
}

impl Stream {
    pub fn kind(&self) -> &'static str {
        if self.url.is_some() {
            "direct"
        } else if self.info_hash.is_some() {
            "torrent"
        } else if self.yt_id.is_some() {
            "youtube"
        } else if self.external_url.is_some() {
            "external"
        } else {
            "unknown"
        }
    }

    fn text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.name.as_deref().unwrap_or(""),
            self.title.as_deref().unwrap_or(""),
            self.description.as_deref().unwrap_or(""),
            self.behavior_hints.as_ref().and_then(|b| b.filename.as_deref()).unwrap_or("")
        )
    }

    pub fn resolution(&self) -> u32 {
        parse_resolution(&self.text())
    }

    pub fn seeders(&self) -> u32 {
        parse_seeders(&self.text())
    }

    pub fn view(&self) -> StreamView {
        let res = self.resolution();
        let name = self.name.clone().unwrap_or_default();
        let heading = name.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" · ");
        let body = self.description.as_deref().or(self.title.as_deref()).unwrap_or("");
        let lines: Vec<String> = body.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
        let size = self
            .behavior_hints
            .as_ref()
            .and_then(|b| b.video_size)
            .map(format_size)
            .or_else(|| SIZE.captures(&self.text()).map(|c| format!("{} {}", &c[1], c[2].to_uppercase())));
        StreamView {
            kind: self.kind(),
            quality: quality_label(res).to_string(),
            resolution: res,
            heading: if heading.is_empty() { self.addon_name.clone() } else { heading },
            lines,
            seeders: self.seeders(),
            size,
            addon: self.addon_name.clone(),
            stream: self.clone(),
        }
    }
}

/// Direct links first, then resolution, then seeders.
pub fn rank(streams: &mut [Stream]) {
    let score = |s: &Stream| (if s.url.is_some() { 1_000_000u64 } else { 0 }) + s.resolution() as u64 * 100 + s.seeders().min(99) as u64;
    streams.sort_by_key(|s| std::cmp::Reverse(score(s)));
}

pub fn build_magnet(s: &Stream) -> Option<String> {
    let hash = s.info_hash.as_ref()?;
    let mut m = format!("magnet:?xt=urn:btih:{hash}");
    if let Some(dn) = s.behavior_hints.as_ref().and_then(|b| b.filename.as_ref()) {
        m.push_str(&format!("&dn={}", urlencoding::encode(dn)));
    }
    for src in &s.sources {
        if let Some(t) = src.strip_prefix("tracker:") {
            m.push_str(&format!("&tr={}", urlencoding::encode(t)));
        }
    }
    Some(m)
}

#[derive(Serialize, Clone, Debug)]
pub struct QualityOption {
    pub index: usize,
    pub label: String,
    pub detail: String,
    pub current: bool,
}

/// Entries for the in-player quality menu: the other streams for the same title, best first.
pub fn quality_options(streams: &[Stream], current: usize, limit: usize) -> Vec<QualityOption> {
    let mut order: Vec<usize> = (0..streams.len()).filter(|&i| matches!(streams[i].kind(), "direct" | "torrent")).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(streams[i].resolution() as u64 * 1000 + streams[i].seeders().min(999) as u64));
    order.truncate(limit);
    if !order.contains(&current) && current < streams.len() {
        order.push(current);
    }
    order
        .into_iter()
        .map(|i| {
            let v = streams[i].view();
            let mut detail: Vec<String> = Vec::new();
            if let Some(s) = &v.size { detail.push(s.clone()); }
            if v.seeders > 0 { detail.push(format!("{} seeds", v.seeders)); }
            detail.push(v.addon.clone());
            QualityOption { index: i, label: format!("{} {}", v.quality, v.heading.replace(&v.quality, "").trim()).trim().to_string(), detail: detail.join(" · "), current: i == current }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(json: &str) -> Stream {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn resolution_and_labels() {
        assert_eq!(parse_resolution("4K · Bluray"), 2160);
        assert_eq!(parse_resolution("Movie.2024.1080p.WEB"), 1080);
        assert_eq!(parse_resolution("nothing"), 0);
        assert_eq!(quality_label(2160), "4K");
        assert_eq!(quality_label(720), "720p");
        assert_eq!(quality_label(0), "SD");
    }

    #[test]
    fn view_keeps_every_spec_on_its_own_line() {
        let v = s(r#"{"infoHash":"abc","name":"4K · Bluray","description":"🟢 89/100 · 👤 17\nx265 · DV\natmos · 5.1\n💾 8.0 GB","addonName":"A"}"#).view();
        assert_eq!(v.kind, "torrent");
        assert_eq!(v.quality, "4K");
        assert_eq!(v.lines.len(), 4);
        assert_eq!(v.seeders, 17);
        assert_eq!(v.size.as_deref(), Some("8.0 GB"));
    }

    #[test]
    fn rank_puts_direct_links_first_then_resolution() {
        let mut list = vec![
            s(r#"{"infoHash":"a","name":"1080p 👤 50"}"#),
            s(r#"{"url":"http://x/720.mp4","name":"720p"}"#),
            s(r#"{"url":"http://x/2160.mp4","name":"4K"}"#),
        ];
        rank(&mut list);
        let keys: Vec<_> = list.iter().map(|x| x.url.clone().or(x.info_hash.clone()).unwrap()).collect();
        assert_eq!(keys, ["http://x/2160.mp4", "http://x/720.mp4", "a"]);
    }

    #[test]
    fn magnet_has_trackers_and_filename() {
        let m = build_magnet(&s(r#"{"infoHash":"abc","sources":["tracker:udp://t.example:80","dht:abc"],"behaviorHints":{"filename":"a b.mkv"}}"#)).unwrap();
        assert_eq!(m, "magnet:?xt=urn:btih:abc&dn=a%20b.mkv&tr=udp%3A%2F%2Ft.example%3A80");
        assert!(build_magnet(&s(r#"{"url":"http://x"}"#)).is_none());
    }

    #[test]
    fn quality_options_mark_the_current_stream_and_sort_by_resolution() {
        let list = vec![
            s(r#"{"infoHash":"a","name":"720p","addonName":"A"}"#),
            s(r#"{"infoHash":"b","name":"4K","addonName":"A"}"#),
            s(r#"{"infoHash":"c","name":"1080p","addonName":"B"}"#),
        ];
        let q = quality_options(&list, 0, 10);
        assert_eq!(q.iter().map(|o| o.index).collect::<Vec<_>>(), [1, 2, 0]);
        assert!(q.iter().find(|o| o.index == 0).unwrap().current);
        assert_eq!(q[0].label, "4K");
    }
}
