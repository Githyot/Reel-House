//! Real-Debrid client (https://api.real-debrid.com). The user's own API token turns a magnet link
//! into a direct HTTPS link that mpv can play, so no peer-to-peer download happens on this computer.

use crate::error::{Error, Result};
use serde::Deserialize;
use std::time::{Duration, Instant};

pub const API_BASE: &str = "https://api.real-debrid.com/rest/1.0";

#[derive(Deserialize, Debug, Clone, serde::Serialize)]
pub struct RdUser {
    pub username: String,
    #[serde(default)]
    pub premium: u64,
    #[serde(default, rename = "type")]
    pub account_type: String,
    #[serde(default)]
    pub expiration: Option<String>,
}

#[derive(Deserialize, Debug)]
struct AddResponse {
    id: String,
}

#[derive(Deserialize, Debug)]
struct RdFile {
    id: u32,
    path: String,
    bytes: u64,
}

#[derive(Deserialize, Debug)]
struct RdInfo {
    status: String,
    #[serde(default)]
    progress: f32,
    #[serde(default)]
    files: Vec<RdFile>,
    #[serde(default)]
    links: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct Unrestricted {
    download: String,
}

#[derive(Debug, Clone)]
pub struct RdProgress {
    pub message: String,
    pub fraction: Option<f32>,
}

pub struct RdClient {
    http: reqwest::Client,
    base: String,
    token: String,
}

const VIDEO_EXT: &[&str] = &["mkv", "mp4", "avi", "mov", "webm", "m4v", "ts", "wmv"];

fn is_video(path: &str) -> bool {
    path.rsplit('.').next().map_or(false, |e| VIDEO_EXT.contains(&e.to_lowercase().as_str()))
}

/// Chooses which file inside the torrent to download. Stremio's `fileIdx` is zero-based and
/// Real-Debrid numbers files from one; without a hint the largest video file wins.
fn pick_file(files: &[RdFile], file_idx: Option<u32>) -> Option<u32> {
    if let Some(i) = file_idx {
        if let Some(f) = files.iter().find(|f| f.id == i + 1 && is_video(&f.path)) {
            return Some(f.id);
        }
    }
    files.iter().filter(|f| is_video(&f.path)).max_by_key(|f| f.bytes).map(|f| f.id)
}

impl RdClient {
    pub fn new(http: reqwest::Client, token: &str) -> Self {
        Self { http, base: API_BASE.to_string(), token: token.trim().to_string() }
    }

    pub fn with_base(mut self, base: &str) -> Self {
        self.base = base.trim_end_matches('/').to_string();
        self
    }

    async fn check(res: reqwest::Response) -> Result<reqwest::Response> {
        let status = res.status();
        if status.is_success() {
            return Ok(res);
        }
        let body = res.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str().map(String::from)))
            .unwrap_or_default();
        Err(Error::msg(match status.as_u16() {
            401 | 403 => "Real-Debrid rejected the API token. Check it in Settings > Accounts.".to_string(),
            429 => "Real-Debrid is limiting requests. Wait a minute and try again.".to_string(),
            code => format!("Real-Debrid answered {code}{}", if detail.is_empty() { String::new() } else { format!(": {detail}") }),
        }))
    }

    pub async fn user(&self) -> Result<RdUser> {
        let res = self.http.get(format!("{}/user", self.base)).bearer_auth(&self.token).send().await?;
        Ok(Self::check(res).await?.json().await?)
    }

    async fn info(&self, id: &str) -> Result<RdInfo> {
        let res = self.http.get(format!("{}/torrents/info/{id}", self.base)).bearer_auth(&self.token).send().await?;
        Ok(Self::check(res).await?.json().await?)
    }

    /// Adds the magnet, selects one video file, waits until Real-Debrid has it and returns a direct link.
    /// Torrents that Real-Debrid already has are ready almost at once; others download on their servers.
    pub async fn resolve(
        &self,
        magnet: &str,
        file_idx: Option<u32>,
        wait: Duration,
        on_progress: &(dyn Fn(RdProgress) + Send + Sync),
    ) -> Result<String> {
        if self.token.is_empty() {
            return Err(Error::msg("Add your Real-Debrid API token in Settings > Accounts first."));
        }
        on_progress(RdProgress { message: "Sending the torrent to Real-Debrid".into(), fraction: None });
        let res = self
            .http
            .post(format!("{}/torrents/addMagnet", self.base))
            .bearer_auth(&self.token)
            .form(&[("magnet", magnet)])
            .send()
            .await?;
        let added: AddResponse = Self::check(res).await?.json().await?;

        let started = Instant::now();
        let mut selected = false;
        loop {
            let info = self.info(&added.id).await?;
            match info.status.as_str() {
                "waiting_files_selection" if !selected => {
                    let choice = pick_file(&info.files, file_idx)
                        .ok_or_else(|| Error::msg("The torrent has no video file."))?;
                    let res = self
                        .http
                        .post(format!("{}/torrents/selectFiles/{}", self.base, added.id))
                        .bearer_auth(&self.token)
                        .form(&[("files", choice.to_string())])
                        .send()
                        .await?;
                    Self::check(res).await?;
                    selected = true;
                }
                "downloaded" => {
                    let link = info.links.first().ok_or_else(|| Error::msg("Real-Debrid finished but returned no link."))?;
                    let res = self
                        .http
                        .post(format!("{}/unrestrict/link", self.base))
                        .bearer_auth(&self.token)
                        .form(&[("link", link.as_str())])
                        .send()
                        .await?;
                    let u: Unrestricted = Self::check(res).await?.json().await?;
                    return Ok(u.download);
                }
                "magnet_error" | "error" | "virus" | "dead" => {
                    return Err(Error::msg(format!("Real-Debrid could not use this torrent ({}). Try another stream.", info.status)));
                }
                other => {
                    let nice = match other {
                        "downloading" => "Real-Debrid is downloading the file",
                        "queued" => "Waiting in the Real-Debrid queue",
                        "magnet_conversion" => "Real-Debrid is reading the magnet",
                        "compressing" | "uploading" => "Real-Debrid is preparing the file",
                        _ => "Waiting for Real-Debrid",
                    };
                    on_progress(RdProgress { message: nice.into(), fraction: Some(info.progress / 100.0) });
                }
            }
            if started.elapsed() > wait {
                return Err(Error::msg("Real-Debrid has not finished this torrent yet. It keeps working on it, so try again in a few minutes, or pick a stream marked as cached."));
            }
            tokio::time::sleep(Duration::from_millis(if cfg!(test) { 20 } else { 1500 })).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use std::sync::Mutex;

    fn client(server: &MockServer) -> RdClient {
        RdClient::new(reqwest::Client::new(), "tok").with_base(&server.base_url())
    }

    #[test]
    fn file_choice_prefers_the_hint_then_the_largest_video() {
        let files = vec![
            RdFile { id: 1, path: "/a/readme.txt".into(), bytes: 10 },
            RdFile { id: 2, path: "/a/ep1.mkv".into(), bytes: 100 },
            RdFile { id: 3, path: "/a/ep2.mkv".into(), bytes: 300 },
        ];
        assert_eq!(pick_file(&files, None), Some(3));
        assert_eq!(pick_file(&files, Some(1)), Some(2));
        assert_eq!(pick_file(&files, Some(0)), Some(3)); // id 1 is not a video
        assert_eq!(pick_file(&files[..1], None), None);
    }

    #[tokio::test]
    async fn user_check_reports_bad_tokens_clearly() {
        let server = MockServer::start();
        server.mock(|w, t| { w.method(GET).path("/user"); t.status(401).json_body(serde_json::json!({"error":"bad_token"})); });
        let err = client(&server).user().await.unwrap_err().to_string();
        assert!(err.contains("rejected the API token"), "{err}");
    }

    #[tokio::test]
    async fn resolves_a_magnet_through_select_download_and_unrestrict() {
        let server = MockServer::start();
        server.mock(|w, t| { w.method(POST).path("/torrents/addMagnet").header("authorization", "Bearer tok").body_contains("magnet=magnet"); t.status(201).json_body(serde_json::json!({"id":"T1","uri":"x"})); });
        let select = server.mock(|w, t| { w.method(POST).path("/torrents/selectFiles/T1").body("files=3"); t.status(204); });
        // First poll asks for a file selection, the second says it is ready.
        let polls = Mutex::new(0);
        let info_waiting = server.mock(|w, t| {
            w.method(GET).path("/torrents/info/T1");
            t.status(200).json_body(serde_json::json!({"status":"waiting_files_selection","files":[
                {"id":1,"path":"/x/info.txt","bytes":5},{"id":3,"path":"/x/movie.mkv","bytes":900}]}));
        });
        server.mock(|w, t| { w.method(POST).path("/unrestrict/link").body_contains("link=https%3A%2F%2Frd.example%2Fabc"); t.status(200).json_body(serde_json::json!({"download":"https://dl.example/movie.mkv"})); });

        let c = client(&server);
        let handle = tokio::spawn({
            let base = server.base_url();
            async move {
                let c = RdClient::new(reqwest::Client::new(), "tok").with_base(&base);
                c.resolve("magnet:?xt=urn:btih:abc", None, Duration::from_secs(5), &|_| {}).await
            }
        });
        // Wait for the selection call, then switch the info mock to "downloaded".
        for _ in 0..100 {
            if select.hits() > 0 { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        info_waiting.delete();
        *polls.lock().unwrap() += 1;
        server.mock(|w, t| { w.method(GET).path("/torrents/info/T1"); t.status(200).json_body(serde_json::json!({"status":"downloaded","links":["https://rd.example/abc"]})); });
        let url = handle.await.unwrap().unwrap();
        assert_eq!(url, "https://dl.example/movie.mkv");
        select.assert();
        drop(c);
    }

    #[tokio::test]
    async fn dead_torrents_fail_with_a_helpful_message() {
        let server = MockServer::start();
        server.mock(|w, t| { w.method(POST).path("/torrents/addMagnet"); t.status(201).json_body(serde_json::json!({"id":"T2"})); });
        server.mock(|w, t| { w.method(GET).path("/torrents/info/T2"); t.status(200).json_body(serde_json::json!({"status":"dead"})); });
        let err = client(&server).resolve("magnet:?xt=urn:btih:x", None, Duration::from_secs(2), &|_| {}).await.unwrap_err().to_string();
        assert!(err.contains("Try another stream"), "{err}");
    }

    #[tokio::test]
    async fn an_empty_token_is_explained() {
        let c = RdClient::new(reqwest::Client::new(), " ");
        let err = c.resolve("magnet:?xt=urn:btih:x", None, Duration::from_secs(1), &|_| {}).await.unwrap_err().to_string();
        assert!(err.contains("Settings > Accounts"));
    }
}
