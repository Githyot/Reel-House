//! Streaming check on a real network (torrent-probe workflow, owner's Linux runner): plays an episode's torrent
//! the way the Android app does (embedded librqbit + libmpv with the app's options) and prints, every second, how
//! much mpv has buffered and what the engine downloaded, plus every byte range mpv asks for (through a small
//! logging proxy). `cargo run --release --example torrent_probe --features torrent-embed -- tt0168366 6 3`
use reelhouse::streams::{self, Stream};

use reelhouse::torrent::{self, Rqbit};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn proxy(listen: TcpListener, upstream: String, t0: Instant, served: Arc<AtomicU64>) {
    for c in listen.incoming().flatten() {
        let (up, served) = (upstream.clone(), served.clone());
        std::thread::spawn(move || {
            let mut c = c;
            let mut head = Vec::new();
            let mut b = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if c.read(&mut b).unwrap_or(0) == 0 {
                    return;
                }
                head.push(b[0]);
            }
            let h = String::from_utf8_lossy(&head).to_string();
            let first = h.lines().next().unwrap_or("").to_string();
            let range = h.lines().find(|l| l.to_lowercase().starts_with("range:")).unwrap_or("range: none").to_string();
            println!("[{:6.1}s] mpv asks: {} | {}", t0.elapsed().as_secs_f64(), first, range);
            let Ok(mut u) = TcpStream::connect(&up) else { return };
            let _ = u.write_all(&head);
            let mut uc = u.try_clone().unwrap();
            let mut cc = c.try_clone().unwrap();
            std::thread::spawn(move || {
                let _ = std::io::copy(&mut cc, &mut uc);
            });
            let mut buf = vec![0u8; 65536];
            let mut n_total = 0u64;
            let start = Instant::now();
            loop {
                match u.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        n_total += n as u64;
                        served.fetch_add(n as u64, Ordering::SeqCst);
                        if c.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                }
            }
            println!("[{:6.1}s] reply closed after {:.1}s, {:.1} MB sent ({})", t0.elapsed().as_secs_f64(), start.elapsed().as_secs_f64(), n_total as f64 / 1e6, range);
        });
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (id, se, ep) = (a.get(1).cloned().unwrap_or("tt0168366".into()), a.get(2).cloned().unwrap_or("6".into()), a.get(3).cloned().unwrap_or("3".into()));
    let http = reelhouse::net::UreqHttp::new();
    let vid = format!("{}:{}:{}", id, se, ep);
    let _ = &vid;
    // the torrent comes from the PROBE_HASH secret (an info hash), so no source is named in the repository
    let hash = std::env::var("PROBE_HASH").unwrap_or_default();
    let mut one = Stream::default();
    one.info_hash = Some(hash);
    let list = vec![one];
    let dir = std::env::temp_dir().join(format!("rh-probe-{}", std::process::id()));
    let port = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
    if let Err(e) = reelhouse::torrent_embed::start(&dir, port) {
        return println!("engine: {}", e);
    }
    let base = format!("http://127.0.0.1:{}", port);
    let rq = Rqbit { http: &http, base: base.clone() };
    let (season, episode) = (se.parse::<i64>().ok(), ep.parse::<i64>().ok());
    let mut chosen = None;
    for s in list.iter().filter(|s| s.info_hash.is_some()).take(4) {
        let magnet = streams::magnet(s).unwrap();
        match rq.list_files_within(&magnet, 45) {
            Ok((_, f)) => {
                chosen = Some((s.clone(), magnet, f));
                break;
            }
            Err(e) => println!("no file list: {} ({})", s.info_hash.clone().unwrap_or_default(), e),
        }
    }
    let Some((first, magnet, listed)) = chosen else { return };
    for (i, f) in listed.iter().enumerate().filter(|(_, f)| f.0.contains("Season 06")).take(5) {
        println!("  file {}: {} ({:.0} MB)", i, f.0, f.1 as f64 / 1e6);
    }
    let idx = listed.iter().position(|f| f.0.contains("Season 06") && (f.0.contains(" 003") || f.0.contains("E03"))).or_else(|| torrent::pick_episode(&listed, first.file_idx, season, episode)).unwrap_or(0);
    println!("== play {} [{}] file {} ({:.2} GB)", first.info_hash.clone().unwrap_or_default(), first.desc.lines().next().unwrap_or(""), listed[idx].0, listed[idx].1 as f64 / 1e9);
    let (tid, _, _) = match rq.add_only(&magnet, &[idx]) {
        Ok(x) => x,
        Err(e) => return println!("add_only: {}", e),
    };
    let t0 = Instant::now();
    let surl = rq.stream_url(tid, idx);
    if let Err(e) = rq.wait_stream(tid, &surl, 150, &|_| {}, &|| false) {
        return println!("wait_stream: {}", e);
    }
    println!("[{:6.1}s] first bytes of the stream answered", t0.elapsed().as_secs_f64());
    let pl = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let pport = pl.local_addr().unwrap().port();
    let served = Arc::new(AtomicU64::new(0));
    {
        let (up, served) = (format!("127.0.0.1:{}", port), served.clone());
        std::thread::spawn(move || proxy(pl, up, t0, served));
    }
    let purl = surl.replace(&format!(":{}/", port), &format!(":{}/", pport));
    // the app's player, without a window
    let mpv = match reelhouse::mpv::Mpv::new_thumbnailer(&[]) {
        Ok(m) => m,
        Err(e) => return println!("libmpv: {}", e),
    };
    for (k, v) in [("aid", "auto"), ("hr-seek", "default"), ("demuxer-max-bytes", "512MiB"), ("demuxer-readahead-secs", "36"), ("network-timeout", "120"), ("pause", "yes")] {
        let _ = mpv.set(k, v);
    }
    let _ = mpv.cmd(&["loadfile", &purl]);
    let mut loaded_at = None;
    for _ in 0..90 {
        std::thread::sleep(Duration::from_secs(1));
        for e in mpv.events() {
            if matches!(e, reelhouse::mpv::Evt::FileLoaded) {
                loaded_at = Some(t0.elapsed().as_secs_f64());
                println!("[{:6.1}s] FILE LOADED (format {:?}, duration {:?})", t0.elapsed().as_secs_f64(), mpv.get_str("file-format"), mpv.get_f64("duration"));
            }
        }
        let st = rq.stats(tid).unwrap_or_default();
        println!(
            "[{:6.1}s] cache={:.1}s cache-state={:?} paused-for-cache={:?} | engine: peers={} {:.1} MB/s file={:.1} MB | proxy sent {:.1} MB",
            t0.elapsed().as_secs_f64(),
            mpv.get_f64("demuxer-cache-duration").unwrap_or(-1.0),
            mpv.get_str("demuxer-cache-state").map(|s| s.chars().take(160).collect::<String>()),
            mpv.get_flag("paused-for-cache"),
            st.peers,
            st.speed / 1e6,
            st.file_progress.get(idx).copied().unwrap_or(0) as f64 / 1e6,
            served.load(Ordering::SeqCst) as f64 / 1e6
        );
        if loaded_at.is_some() && mpv.get_f64("demuxer-cache-duration").unwrap_or(0.0) >= 12.0 {
            println!("[{:6.1}s] 12 s buffered: the app would start playing here", t0.elapsed().as_secs_f64());
            break;
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}
