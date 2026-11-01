//! A tiny HTTP abstraction. The app implements it with `ureq`; tests use a fake.
use crate::json::Json;
use crate::util::form_body;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<(String, Vec<u8>)>,
    pub timeout_secs: u64,
}

impl Request {
    pub fn get(url: &str) -> Request {
        Request { method: "GET", url: url.to_string(), headers: vec![], body: None, timeout_secs: 30 }
    }
    pub fn post(url: &str) -> Request {
        Request { method: "POST", url: url.to_string(), headers: vec![], body: None, timeout_secs: 30 }
    }
    pub fn header(mut self, k: &str, v: &str) -> Request {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }
    pub fn form(mut self, pairs: &[(&str, &str)]) -> Request {
        self.body = Some(("application/x-www-form-urlencoded".to_string(), form_body(pairs).into_bytes()));
        self
    }
    pub fn json_body(mut self, body: String) -> Request {
        self.body = Some(("application/json".to_string(), body.into_bytes()));
        self
    }
    pub fn text_body(mut self, body: &str) -> Request {
        self.body = Some(("text/plain".to_string(), body.as_bytes().to_vec()));
        self
    }
    pub fn timeout(mut self, secs: u64) -> Request {
        self.timeout_secs = secs;
        self
    }
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    /// The Location header of a redirect that was not followed (POST redirects), empty otherwise.
    pub location: String,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
    pub fn json(&self) -> Result<Json, String> {
        Json::parse(&self.text())
    }
}

/// Non-2xx answers are returned as `Ok` with their status. `Err` means the request itself failed.
pub trait Http: Send + Sync {
    fn send(&self, req: &Request) -> Result<Response, String>;

    /// Saves a (large) answer straight to a file; `progress(done, total)` is called now and then.
    fn download(&self, url: &str, path: &std::path::Path, progress: &dyn Fn(u64, Option<u64>)) -> Result<(), String> {
        let r = self.send(&Request::get(url).timeout(900))?;
        if !r.ok() {
            return Err(format!("{} answered {}", url.split('/').nth(2).unwrap_or(url), r.status));
        }
        progress(r.body.len() as u64, Some(r.body.len() as u64));
        std::fs::write(path, &r.body).map_err(|e| e.to_string())
    }

    /// Sends `req` and hands each line of the answer to `on_line` as it arrives (for servers that stream JSON lines,
    /// such as Ollama's /api/pull). Returns the status; the lines of an error answer are handed over too.
    fn send_lines(&self, req: &Request, on_line: &mut dyn FnMut(&str)) -> Result<u16, String> {
        let r = self.send(req)?;
        for l in r.text().lines() {
            on_line(l);
        }
        Ok(r.status)
    }
}

pub type HttpRef = Arc<dyn Http>;

pub fn get_json(http: &dyn Http, url: &str, timeout: u64) -> Result<Json, String> {
    let r = http.send(&Request::get(url).header("Accept", "application/json").timeout(timeout))?;
    if !r.ok() {
        let host = url.split('/').nth(2).unwrap_or(url);
        let why = r.json().ok().map(|j| { let d = j.path("errors.0.detail").str(); if d.is_empty() { j.path("errors.0.title").str() } else { d } }).unwrap_or_default();
        return Err(if why.is_empty() { format!("{} answered {}", host, r.status) } else { format!("{} answered {}: {}", host, r.status, why) });
    }
    r.json().map_err(|e| format!("{} did not return valid JSON ({})", url.split('/').nth(2).unwrap_or(url), e))
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Routes look like "GET /torrents/info". Several answers for one route are served in order,
    /// and the last one repeats.
    pub struct Fake {
        routes: Mutex<Vec<(String, VecDeque<(u16, String)>)>>,
        pub log: Mutex<Vec<Request>>,
    }

    impl Fake {
        pub fn new() -> Fake {
            Fake { routes: Mutex::new(vec![]), log: Mutex::new(vec![]) }
        }
        pub fn route(self, pattern: &str, status: u16, body: &str) -> Fake {
            {
                let mut r = self.routes.lock().unwrap();
                match r.iter_mut().find(|(p, _)| p == pattern) {
                    Some((_, q)) => q.push_back((status, body.to_string())),
                    None => r.push((pattern.to_string(), VecDeque::from(vec![(status, body.to_string())]))),
                }
            }
            self
        }
        pub fn requests(&self) -> Vec<Request> {
            self.log.lock().unwrap().clone()
        }
    }

    impl Http for Fake {
        fn send(&self, req: &Request) -> Result<Response, String> {
            self.log.lock().unwrap().push(req.clone());
            let key = format!("{} {}", req.method, req.url);
            let mut routes = self.routes.lock().unwrap();
            for (pat, q) in routes.iter_mut() {
                if key.contains(pat.as_str()) {
                    let (st, b) = if q.len() > 1 { q.pop_front().unwrap() } else { q.front().cloned().unwrap() };
                    return Ok(Response { status: st, body: b.into_bytes(), location: String::new() });
                }
            }
            Ok(Response { status: 404, body: b"not found".to_vec(), location: String::new() })
        }
    }
}
