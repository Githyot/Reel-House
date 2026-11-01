//! Real-Debrid client: turns a magnet link into a direct HTTPS link that mpv can play.
use crate::http::{Http, Request};
use crate::json::Json;
use crate::torrent::pick_file;
use std::time::{Duration, Instant};

const API: &str = "https://api.real-debrid.com/rest/1.0";

pub struct RealDebrid<'a> {
    pub http: &'a dyn Http,
    pub token: String,
    pub poll_ms: u64,
    pub max_wait_secs: u64,
}

fn short(s: &str) -> String {
    let t: String = s.chars().take(160).collect();
    t.replace('\n', " ")
}

impl<'a> RealDebrid<'a> {
    pub fn new(http: &'a dyn Http, token: &str) -> RealDebrid<'a> {
        RealDebrid { http, token: token.trim().to_string(), poll_ms: 1500, max_wait_secs: 90 }
    }

    fn call(&self, r: Request) -> Result<crate::http::Response, String> {
        let resp = self.http.send(&r.header("Authorization", &format!("Bearer {}", self.token)))?;
        match resp.status {
            401 | 403 => Err("Real-Debrid rejected your API key. Check it in Settings > Real-Debrid.".to_string()),
            s if s >= 400 => Err(format!("Real-Debrid error {}: {}", s, short(&resp.text()))),
            _ => Ok(resp),
        }
    }

    /// Account summary for the Settings "Test" button.
    pub fn user(&self) -> Result<String, String> {
        if self.token.is_empty() {
            return Err("Paste your Real-Debrid API token first.".to_string());
        }
        let j = self.call(Request::get(&format!("{}/user", API)))?.json()?;
        let name = j.get("username").str();
        let kind = j.get("type").str();
        let exp = j.get("expiration").str();
        let mut out = format!("{} ({})", if name.is_empty() { "account" } else { &name }, if kind.is_empty() { "unknown plan" } else { &kind });
        if let Some(day) = exp.split('T').next().filter(|d| !d.is_empty()) {
            out.push_str(&format!(", premium until {}", day));
        }
        Ok(out)
    }

    pub fn unrestrict(&self, link: &str) -> Result<String, String> {
        let j = self.call(Request::post(&format!("{}/unrestrict/link", API)).form(&[("link", link)]))?.json()?;
        let url = j.get("download").str();
        if url.is_empty() {
            return Err("Real-Debrid did not return a download link.".to_string());
        }
        Ok(url)
    }

    /// Adds the magnet, selects the wanted file, waits until Real-Debrid has it and returns the direct URL.
    /// Cached torrents are ready in seconds. Uncached ones are downloaded by Real-Debrid first.
    pub fn resolve(
        &self,
        magnet: &str,
        file_idx: Option<usize>,
        progress: &mut dyn FnMut(String),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String, String> {
        if self.token.is_empty() {
            return Err("Real-Debrid is turned on but no API token is set.".to_string());
        }
        progress("Sending the magnet to Real-Debrid".to_string());
        let added = self.call(Request::post(&format!("{}/torrents/addMagnet", API)).form(&[("magnet", magnet)]))?.json()?;
        let id = added.get("id").str();
        if id.is_empty() {
            return Err("Real-Debrid did not accept the magnet link.".to_string());
        }
        let started = Instant::now();
        let mut selected = false;
        loop {
            if cancelled() {
                return Err("Cancelled".to_string());
            }
            let info = self.call(Request::get(&format!("{}/torrents/info/{}", API, id)))?.json()?;
            let status = info.get("status").str();
            let pct = info.get("progress").as_f64().unwrap_or(0.0);
            match status.as_str() {
                "waiting_files_selection" if !selected => {
                    let files: Vec<(String, u64)> = info
                        .get("files")
                        .items()
                        .iter()
                        .map(|f| (f.get("path").str(), f.get("bytes").as_u64().unwrap_or(0)))
                        .collect();
                    let pos = pick_file(&files, file_idx).ok_or_else(|| "The torrent has no files.".to_string())?;
                    let file_id = info.get("files").at(pos).get("id").str();
                    let file_id = if file_id.is_empty() { (pos + 1).to_string() } else { file_id };
                    self.call(Request::post(&format!("{}/torrents/selectFiles/{}", API, id)).form(&[("files", &file_id)]))?;
                    selected = true;
                    progress("Selecting the video file".to_string());
                }
                "downloaded" => {
                    let link = info.get("links").at(0).str();
                    if link.is_empty() {
                        return Err("Real-Debrid finished but returned no link.".to_string());
                    }
                    progress("Getting the direct link".to_string());
                    return self.unrestrict(&link);
                }
                "magnet_error" | "error" | "virus" | "dead" => {
                    return Err(format!("Real-Debrid could not use this torrent (status: {}). Try another one.", status));
                }
                "downloading" | "compressing" | "uploading" => {
                    progress(format!("Real-Debrid is fetching this torrent: {:.0}%. It was not cached.", pct));
                }
                _ => progress("Waiting for Real-Debrid".to_string()),
            }
            if started.elapsed().as_secs() >= self.max_wait_secs {
                return Err(format!(
                    "This torrent is not cached on Real-Debrid yet (status: {}, {:.0}%). Pick another torrent, or try again later.",
                    if status.is_empty() { "unknown" } else { &status },
                    pct
                ));
            }
            std::thread::sleep(Duration::from_millis(self.poll_ms));
        }
    }
}

#[allow(dead_code)]
fn _assert_json_used(_: &Json) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    fn rd(http: &Fake) -> RealDebrid<'_> {
        RealDebrid { http, token: "TOK".into(), poll_ms: 0, max_wait_secs: 2 }
    }

    #[test]
    fn resolves_a_cached_torrent_end_to_end() {
        let http = Fake::new()
            .route("POST https://api.real-debrid.com/rest/1.0/torrents/addMagnet", 201, r#"{"id":"ABC","uri":"u"}"#)
            .route("GET https://api.real-debrid.com/rest/1.0/torrents/info/ABC", 200,
                r#"{"status":"waiting_files_selection","files":[{"id":1,"path":"/a.txt","bytes":10},{"id":2,"path":"/movie.mkv","bytes":9000},{"id":3,"path":"/s.srt","bytes":5}]}"#)
            .route("GET https://api.real-debrid.com/rest/1.0/torrents/info/ABC", 200, r#"{"status":"downloaded","progress":100,"links":["https://real-debrid.com/d/XYZ"]}"#)
            .route("POST https://api.real-debrid.com/rest/1.0/torrents/selectFiles/ABC", 204, "")
            .route("POST https://api.real-debrid.com/rest/1.0/unrestrict/link", 200, r#"{"download":"https://cdn.example/movie.mkv"}"#);
        let mut msgs = Vec::new();
        let url = rd(&http).resolve("magnet:?xt=urn:btih:abc", None, &mut |m| msgs.push(m), &|| false).unwrap();
        assert_eq!(url, "https://cdn.example/movie.mkv");
        let reqs = http.requests();
        assert!(reqs.iter().all(|r| r.headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer TOK")));
        let select = reqs.iter().find(|r| r.url.contains("selectFiles")).unwrap();
        assert_eq!(String::from_utf8_lossy(&select.body.as_ref().unwrap().1), "files=2");
        let add = reqs.iter().find(|r| r.url.contains("addMagnet")).unwrap();
        assert_eq!(String::from_utf8_lossy(&add.body.as_ref().unwrap().1), "magnet=magnet%3A%3Fxt%3Durn%3Abtih%3Aabc");
        assert!(msgs.iter().any(|m| m.contains("direct link")));
    }

    #[test]
    fn honours_the_requested_file_index() {
        let http = Fake::new()
            .route("addMagnet", 201, r#"{"id":"Q"}"#)
            .route("torrents/info/Q", 200, r#"{"status":"waiting_files_selection","files":[{"id":1,"path":"/e1.mkv","bytes":5},{"id":2,"path":"/e2.mkv","bytes":50}]}"#)
            .route("torrents/info/Q", 200, r#"{"status":"downloaded","links":["L"]}"#)
            .route("selectFiles", 204, "")
            .route("unrestrict", 200, r#"{"download":"D"}"#);
        rd(&http).resolve("m", Some(0), &mut |_| {}, &|| false).unwrap();
        let reqs = http.requests();
        let select = reqs.iter().find(|r| r.url.contains("selectFiles")).unwrap();
        assert_eq!(String::from_utf8_lossy(&select.body.as_ref().unwrap().1), "files=1");
    }

    #[test]
    fn gives_up_on_uncached_torrents_with_a_clear_message() {
        let http = Fake::new()
            .route("addMagnet", 201, r#"{"id":"Q"}"#)
            .route("torrents/info/Q", 200, r#"{"status":"downloading","progress":12}"#);
        let mut r = rd(&http);
        r.max_wait_secs = 0;
        let err = r.resolve("m", None, &mut |_| {}, &|| false).unwrap_err();
        assert!(err.contains("not cached") && err.contains("12%"), "{}", err);
    }

    #[test]
    fn explains_bad_tokens_and_dead_torrents() {
        let http = Fake::new().route("addMagnet", 403, r#"{"error":"bad_token"}"#);
        assert!(rd(&http).resolve("m", None, &mut |_| {}, &|| false).unwrap_err().contains("API key"));
        let http = Fake::new().route("addMagnet", 201, r#"{"id":"Q"}"#).route("info/Q", 200, r#"{"status":"dead"}"#);
        assert!(rd(&http).resolve("m", None, &mut |_| {}, &|| false).unwrap_err().contains("dead"));
        assert!(RealDebrid::new(&http, "").user().is_err());
    }

    #[test]
    fn can_be_cancelled() {
        let http = Fake::new().route("addMagnet", 201, r#"{"id":"Q"}"#);
        assert_eq!(rd(&http).resolve("m", None, &mut |_| {}, &|| true).unwrap_err(), "Cancelled");
    }

    #[test]
    fn summarises_the_account() {
        let http = Fake::new().route("/user", 200, r#"{"username":"neo","type":"premium","expiration":"2027-01-02T10:00:00.000Z"}"#);
        assert_eq!(rd(&http).user().unwrap(), "neo (premium), premium until 2027-01-02");
    }
}
