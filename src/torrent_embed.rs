//! The torrent engine inside the app (feature `torrent-embed`), for Android, which may not start the rqbit program.
//! librqbit is the library rqbit is built on: it serves the same HTTP API on 127.0.0.1, so `torrent::Rqbit` talks to
//! it exactly as to the program (add a magnet, pick files, stream, stats). It lives as long as the app.

use std::path::Path;
use std::sync::mpsc;

/// Starts the engine on its own thread, serving the API on 127.0.0.1:`port` and downloading into `download_dir`.
/// Returns once the API listens, or the reason it could not start.
pub fn start(download_dir: &Path, port: u16) -> Result<(), String> {
    std::fs::create_dir_all(download_dir).map_err(|e| e.to_string())?;
    let dir = download_dir.to_path_buf();
    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    std::thread::Builder::new()
        .name("torrent-engine".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(format!("The torrent engine could not start: {}", e)));
                    return;
                }
            };
            rt.block_on(async move {
                // like `rqbit server start --disable-persistence`; IPv4 only on the second try, for phones and
                // emulators that cannot open IPv6 sockets
                let mut session = None;
                let mut last = String::new();
                for ipv4_only in [false, true] {
                    let opts = librqbit::SessionOptions {
                        persistence: None,
                        dht: Some(librqbit::DhtSessionConfig { persistence: None, ..Default::default() }),
                        ipv4_only,
                        ..Default::default()
                    };
                    match librqbit::Session::new_with_opts(dir.clone(), opts).await {
                        Ok(s) => {
                            session = Some(s);
                            break;
                        }
                        Err(e) => last = format!("{:#}", e),
                    }
                }
                let Some(session) = session else {
                    let _ = tx.send(Err(format!("The torrent engine could not start: {}", last)));
                    return;
                };
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                let bind = librqbit_dualstack_sockets::BindOpts { request_dualstack: false, ..Default::default() };
                let listener = match librqbit_dualstack_sockets::TcpListener::bind_tcp(addr, bind) {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx.send(Err(format!("The torrent engine could not listen on port {}: {}", port, e)));
                        return;
                    }
                };
                let api = librqbit::api::Api::new(session, None, None);
                let http = librqbit::http_api::HttpApi::new(api, Some(librqbit::http_api::HttpApiOptions { read_only: false, ..Default::default() }));
                let _ = tx.send(Ok(()));
                if let Err(e) = http.make_http_api_and_run(listener, None).await {
                    crate::debug::log("torrent", &format!("embedded engine stopped: {:#}", e));
                }
            });
        })
        .map_err(|e| e.to_string())?;
    rx.recv().unwrap_or_else(|_| Err("The torrent engine stopped while starting.".to_string()))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    #[test]
    fn serves_the_rqbit_api() {
        // librqbit's UDP tracker client always opens an IPv6 socket; a sandbox without IPv6 cannot run the engine
        if std::net::UdpSocket::bind("[::]:0").is_err() {
            eprintln!("no IPv6 sockets here: skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("rh-embed-{}", std::process::id()));
        let port = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
        super::start(&dir, port).unwrap();
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"GET /torrents HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        assert!(out.starts_with("HTTP/1.0 200") || out.starts_with("HTTP/1.1 200"), "{}", &out[..out.len().min(200)]);
        assert!(out.contains("\"torrents\""), "{}", out);
        let _ = std::fs::remove_dir_all(dir);
    }
}
