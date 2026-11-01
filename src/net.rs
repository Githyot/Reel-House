//! HTTP over `ureq` (TLS through rustls, no system libraries needed).
use crate::http::{Http, Request, Response};
use std::io::Read;
use std::time::Duration;

pub struct UreqHttp {
    agent: ureq::Agent,
    /// Same as `agent`, never through a proxy: for the engines on this computer.
    local: ureq::Agent,
}

impl UreqHttp {
    fn pick(&self, url: &str) -> &ureq::Agent {
        let rest = url.split("://").nth(1).unwrap_or("");
        if rest.starts_with("127.0.0.1") || rest.starts_with("localhost") || rest.starts_with("[::1]") {
            &self.local
        } else {
            &self.agent
        }
    }
}

/// The addresses of a host with IPv4 first. ureq tries them one after the other within one connect timeout, so a
/// network whose IPv6 route is broken (common at home: the router announces it, nothing answers) spent all of it on
/// the IPv6 address and failed with "connection timed out" where a browser (which races both) worked. Seen with
/// kara.moe on the owner's Windows.
fn v4_first(netloc: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
    use std::net::ToSocketAddrs;
    let mut v: Vec<std::net::SocketAddr> = netloc.to_socket_addrs()?.collect();
    v.sort_by_key(|a| a.is_ipv6());
    Ok(v)
}

/// Offline, a request to another computer is refused at once (no 12 s connect timeout per request); the callers
/// that keep a copy on disk show it instead (src/connectivity.rs).
fn gate(url: &str) -> Result<(), String> {
    if crate::connectivity::online() || crate::connectivity::is_local(url) {
        Ok(())
    } else {
        Err(crate::connectivity::offline_error(url))
    }
}

/// A request that could not connect asks the watcher to check the connection now.
fn failed(url: &str, e: &ureq::Error) {
    if matches!(e, ureq::Error::Transport(_)) && !crate::connectivity::is_local(url) {
        crate::connectivity::note_failure();
    }
}

impl UreqHttp {
    pub fn new() -> UreqHttp {
        crate::connectivity::start();
        #[allow(unused_mut)]
        let mut b = ureq::AgentBuilder::new().resolver(v4_first).timeout_connect(Duration::from_secs(12)).user_agent("Reelhouse/0.2").redirects(5);
        #[cfg(feature = "devproxy")]
        if let Some(p) = std::env::var("HTTPS_PROXY").ok().and_then(|u| ureq::Proxy::new(u).ok()) {
            b = b.proxy(p);
        }
        let local = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(12)).user_agent("Reelhouse/0.2").redirects(5).build();
        let agent = b.build();
        UreqHttp { agent, local }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        UreqHttp::new()
    }
}

fn read_response(resp: ureq::Response) -> Result<Response, String> {
    let status = resp.status();
    let location = resp.header("Location").unwrap_or("").to_string();
    let mut body = Vec::new();
    resp.into_reader().take(256 * 1024 * 1024).read_to_end(&mut body).map_err(|e| format!("could not read the answer ({})", e))?;
    Ok(Response { status, body, location })
}

impl Http for UreqHttp {
    fn download(&self, url: &str, path: &std::path::Path, progress: &dyn Fn(u64, Option<u64>)) -> Result<(), String> {
        let host = url.split('/').nth(2).unwrap_or(url).to_string();
        gate(url)?;
        let resp = match self.pick(url).get(url).timeout(Duration::from_secs(7200)).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => return Err(format!("{} answered {}", host, code)),
            Err(e) => {
                failed(url, &e);
                return Err(format!("could not reach {} ({})", host, e));
            }
        };
        let total = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok());
        let mut reader = resp.into_reader();
        let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 256 * 1024];
        let (mut done, mut last) = (0u64, 0u64);
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("the download was interrupted ({})", e))?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            if done - last >= 2 * 1024 * 1024 {
                last = done;
                progress(done, total);
            }
        }
        progress(done, total);
        if let Some(t) = total {
            if done < t {
                return Err("the download ended early".to_string());
            }
        }
        Ok(())
    }

    fn send_lines(&self, req: &Request, on_line: &mut dyn FnMut(&str)) -> Result<u16, String> {
        let host = req.url.split('/').nth(2).unwrap_or(&req.url).to_string();
        gate(&req.url)?;
        let mut r = self.pick(&req.url).request(req.method, &req.url).timeout(Duration::from_secs(req.timeout_secs.max(1)));
        for (k, v) in &req.headers {
            r = r.set(k, v);
        }
        let result = match &req.body {
            Some((content_type, body)) => r.set("Content-Type", content_type).send_bytes(body),
            None => r.call(),
        };
        let resp = match result {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_, resp)) => resp,
            Err(e) => {
                failed(&req.url, &e);
                return Err(format!("could not reach {} ({})", host, e));
            }
        };
        let status = resp.status();
        let mut reader = std::io::BufReader::new(resp.into_reader());
        let mut line = Vec::new();
        loop {
            line.clear();
            let n = std::io::BufRead::read_until(&mut reader, b'\n', &mut line).map_err(|e| format!("the answer of {} was interrupted ({})", host, e))?;
            if n == 0 {
                break;
            }
            on_line(String::from_utf8_lossy(&line).trim_end());
        }
        Ok(status)
    }

    fn send(&self, req: &Request) -> Result<Response, String> {
        if !crate::debug::enabled() {
            return self.send_inner(req);
        }
        let t = std::time::Instant::now();
        let r = self.send_inner(req);
        let ms = t.elapsed().as_millis();
        let url = crate::debug::redact(&req.url);
        match &r {
            Ok(x) if x.status < 400 && ["jpg", "jpeg", "png", "webp", "gif", "avif"].iter().any(|e| url.split('?').next().unwrap_or("").to_lowercase().ends_with(e)) => {
                // Pictures are only counted, not listed.
                crate::debug::log("img", &format!("{} ms, {} bytes", ms, x.body.len()));
            }
            Ok(x) => {
                let mut line = format!("{} {} -> {} in {} ms, {} bytes", req.method, url, x.status, ms, x.body.len());
                // What the karaoke and the repositories answer is what is needed to find faults there.
                if x.status >= 400 || url.contains("karaokes.moe") || url.contains("kara.moe") || url.contains("/api/graphql") {
                    let snip: String = String::from_utf8_lossy(&x.body[..x.body.len().min(300)]).chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
                    line.push_str(&format!(" | {}", snip));
                }
                crate::debug::log("http", &line);
            }
            Err(e) => crate::debug::log("http", &format!("{} {} -> FAILED after {} ms: {}", req.method, url, ms, e)),
        }
        r
    }
}

impl UreqHttp {
    fn send_inner(&self, req: &Request) -> Result<Response, String> {
        gate(&req.url)?;
        let mut r = self.pick(&req.url).request(req.method, &req.url).timeout(Duration::from_secs(req.timeout_secs.max(1)));
        for (k, v) in &req.headers {
            r = r.set(k, v);
        }
        let result = match &req.body {
            Some((content_type, body)) => r.set("Content-Type", content_type).send_bytes(body),
            None => r.call(),
        };
        match result {
            Ok(resp) => read_response(resp),
            // HTTP error codes are data for the caller, not transport failures.
            Err(ureq::Error::Status(_, resp)) => read_response(resp),
            Err(e) => {
                failed(&req.url, &e);
                let host = req.url.split('/').nth(2).unwrap_or(&req.url);
                Err(format!("could not reach {} ({})", host, e))
            }
        }
    }
}

#[cfg(test)]
mod v4_tests {
    #[test]
    fn ipv4_comes_before_ipv6() {
        let v = super::v4_first("localhost:80").unwrap();
        if let Some(i) = v.iter().position(|a| a.is_ipv6()) {
            assert!(v[i..].iter().all(|a| a.is_ipv6()));
        }
    }
}
