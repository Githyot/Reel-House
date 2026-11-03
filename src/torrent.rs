//! Torrent playback without Real-Debrid, through the bundled rqbit engine (a small HTTP-controlled
//! torrent client). Reelhouse starts it in the background and asks it for a stream URL for mpv.
use crate::http::{Http, Request};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const VIDEO_EXT: &[&str] = &["mkv", "mp4", "avi", "mov", "webm", "m4v", "ts", "wmv"];

pub fn is_video(name: &str) -> bool {
    let lower = name.to_lowercase();
    VIDEO_EXT.iter().any(|e| lower.ends_with(&format!(".{}", e)))
}

/// Index of the file to play: the one the addon named, else the biggest video, else the biggest file.
pub fn pick_file(files: &[(String, u64)], idx: Option<usize>) -> Option<usize> {
    if files.is_empty() {
        return None;
    }
    if let Some(i) = idx {
        if i < files.len() {
            return Some(i);
        }
    }
    files
        .iter()
        .enumerate()
        .filter(|(_, (n, _))| is_video(n))
        .max_by_key(|(_, (_, len))| *len)
        .map(|(i, _)| i)
        .or_else(|| files.iter().enumerate().max_by_key(|(_, (_, len))| *len).map(|(i, _)| i))
}

/// Like `pick_file`, for a season pack: when the addon did not name the file, the one whose name carries the episode that
/// was asked for (season and episode from the video id) is chosen, not just the biggest file of the pack.
pub fn pick_episode(files: &[(String, u64)], idx: Option<usize>, season: Option<i64>, episode: Option<i64>) -> Option<usize> {
    if let Some(i) = idx {
        if i < files.len() {
            return Some(i);
        }
    }
    if let Some(ep) = episode {
        // 0: the season is named and right; 1: no season in the name; 2: any season. A season pack's "S08X01 …
        // Chronicles E03" must not beat "S06E03" (checked on a real Pokémon S06 torrent, 2026-11-01).
        let matches = |level: u8| {
            files
                .iter()
                .enumerate()
                .filter(|(_, (n, _))| is_video(n))
                .filter(|(_, (n, _))| {
                    let (s, e) = crate::downloads::parse_episode(n);
                    e == Some(ep)
                        && match level {
                            0 => season.is_none() || s == season,
                            1 => s.is_none(),
                            _ => true,
                        }
                })
                .max_by_key(|(_, (_, len))| *len)
                .map(|(i, _)| i)
        };
        if let Some(i) = matches(0).or_else(|| matches(1)).or_else(|| matches(2)) {
            return Some(i);
        }
    }
    pick_file(files, None)
}

pub struct Rqbit<'a> {
    pub http: &'a dyn Http,
    pub base: String,
}

impl<'a> Rqbit<'a> {
    /// Adds the magnet and waits for its metadata. Returns the torrent id and its files.
    pub fn add(&self, magnet: &str) -> Result<(u64, Vec<(String, u64)>), String> {
        let (id, _, files) = self.add_raw(magnet, "overwrite=true")?;
        Ok((id.ok_or_else(|| "The torrent engine did not return a torrent id.".to_string())?, files))
    }

    /// Reads the file list of a magnet without downloading anything. Returns the torrent name and files.
    pub fn list_files(&self, magnet: &str) -> Result<(String, Vec<(String, u64)>), String> {
        self.list_files_within(magnet, 150)
    }

    /// `list_files`, giving up after `secs` (the file list needs the torrent's metadata from a peer).
    pub fn list_files_within(&self, magnet: &str, secs: u64) -> Result<(String, Vec<(String, u64)>), String> {
        let (_, name, files) = self.add_raw_within(magnet, "list_only=true&overwrite=true", secs)?;
        Ok((name, files))
    }

    /// Adds the magnet and downloads only the given files (all of them when `only` is empty).
    pub fn add_only(&self, magnet: &str, only: &[usize]) -> Result<(u64, String, Vec<(String, u64)>), String> {
        let q = if only.is_empty() { "overwrite=true".to_string() } else { format!("overwrite=true&only_files={}", only.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")) };
        let (id, name, files) = self.add_raw(magnet, &q)?;
        let id = id.ok_or_else(|| "The torrent engine did not return a torrent id.".to_string())?;
        // A torrent the engine already had (played whole by an older version, or saved from an earlier session)
        // keeps its old choice of files when it is added again: set it to these files only, so a season pack does
        // not go on downloading every episode (HP).
        if !only.is_empty() {
            let body = format!("{{\"only_files\":[{}]}}", only.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","));
            let _ = self.http.send(&Request::post(&format!("{}/torrents/{}/update_only_files", self.base, id)).json_body(body).timeout(20));
        }
        Ok((id, name, files))
    }

    fn add_raw(&self, magnet: &str, query: &str) -> Result<(Option<u64>, String, Vec<(String, u64)>), String> {
        self.add_raw_within(magnet, query, 150)
    }

    fn add_raw_within(&self, magnet: &str, query: &str, secs: u64) -> Result<(Option<u64>, String, Vec<(String, u64)>), String> {
        let url = format!("{}/torrents?{}", self.base, query);
        let r = self.http.send(&Request::post(&url).text_body(magnet).timeout(secs))?;
        if !r.ok() {
            let t: String = r.text().chars().take(160).collect();
            return Err(format!("The torrent engine refused the magnet ({}): {}", r.status, t));
        }
        let j = r.json()?;
        let files = j
            .path("details.files")
            .items()
            .iter()
            .map(|f| (f.get("name").str(), f.get("length").as_u64().unwrap_or(0)))
            .collect();
        Ok((j.get("id").as_u64(), j.path("details.name").str(), files))
    }

    /// Stops a torrent and deletes its files (the engine knows where they are).
    pub fn delete(&self, id_or_hash: &str) -> bool {
        self.http.send(&Request::post(&format!("{}/torrents/{}/delete", self.base, id_or_hash)).timeout(20)).map(|r| r.ok()).unwrap_or(false)
    }

    pub fn stream_url(&self, id: u64, file: usize) -> String {
        format!("{}/torrents/{}/stream/{}", self.base, id, file)
    }

    /// Connection numbers for the Statistics panel and the loading text.
    pub fn stats(&self, id: u64) -> Option<TorrentStats> {
        let r = self.http.send(&Request::get(&format!("{}/torrents/{}/stats/v1", self.base, id)).timeout(4)).ok()?;
        if !r.ok() {
            return None;
        }
        Some(TorrentStats::from_json(&r.json().ok()?))
    }

    /// Waits until the engine really serves bytes of the file (it may still be starting up or
    /// looking for peers). Returns Ok when the first bytes could be read.
    pub fn wait_stream(&self, id: u64, url: &str, secs: u64, status: &dyn Fn(String), cancelled: &dyn Fn() -> bool) -> Result<(), String> {
        let started = Instant::now();
        let mut last;
        loop {
            if cancelled() {
                return Err("Cancelled.".to_string());
            }
            match self.http.send(&Request::get(url).header("Range", "bytes=0-4095").timeout(8)) {
                Ok(r) if r.status == 200 || r.status == 206 => {
                    if !r.body.is_empty() {
                        return Ok(());
                    }
                    last = "empty answer".to_string();
                }
                Ok(r) => last = format!("status {}: {}", r.status, r.text().chars().take(100).collect::<String>()),
                Err(e) => last = e,
            }
            let elapsed = started.elapsed().as_secs();
            if elapsed >= secs {
                return Err(format!("The torrent did not start in {} seconds (last answer: {}). It may have no active peers; try another torrent.", secs, last));
            }
            match self.stats(id) {
                Some(st) => status(format!("Connecting to peers: {} found · {}/s · {} downloaded", st.peers, human_bytes(st.speed as u64), human_bytes(st.progress))),
                None => status("Waiting for the torrent engine".to_string()),
            }
            std::thread::sleep(Duration::from_millis(1200));
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TorrentStats {
    pub peers: u64,
    /// Bytes per second.
    pub speed: f64,
    pub progress: u64,
    pub total: u64,
    pub state: String,
    /// Bytes downloaded of each file, in the order of the torrent.
    pub file_progress: Vec<u64>,
    pub finished: bool,
}

impl TorrentStats {
    pub fn from_json(j: &crate::json::Json) -> TorrentStats {
        let num = |paths: &[&str]| paths.iter().find_map(|p| j.path(p).as_f64());
        TorrentStats {
            peers: num(&["live.snapshot.peer_stats.live", "live.peer_stats.live", "peer_stats.live"]).unwrap_or(0.0) as u64,
            // rqbit reports "mbps", which is mebibytes per second.
            speed: num(&["live.download_speed.mbps"]).unwrap_or(0.0) * 1_048_576.0,
            progress: num(&["progress_bytes"]).unwrap_or(0.0) as u64,
            total: num(&["total_bytes"]).unwrap_or(0.0) as u64,
            state: j.get("state").str(),
            file_progress: j.get("file_progress").items().iter().map(|x| x.as_u64().unwrap_or(0)).collect(),
            finished: j.get("finished").as_bool().unwrap_or(false),
        }
    }

    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.progress as f64 * 100.0 / self.total as f64
        }
    }
}

pub fn human_bytes(n: u64) -> String {
    let n = n as f64;
    if n >= 1_073_741_824.0 {
        format!("{:.2} GB", n / 1_073_741_824.0)
    } else if n >= 1_048_576.0 {
        format!("{:.1} MB", n / 1_048_576.0)
    } else {
        format!("{:.0} KB", n / 1024.0)
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The background engine process. It is stopped when this value is dropped. Without a process (`child` None) it is the
/// engine inside the app (`torrent_embed`, Android), which runs as long as the app.
pub struct RqbitProcess {
    child: Option<Child>,
    pub port: u16,
}

impl RqbitProcess {
    pub fn log_path(download_dir: &Path) -> std::path::PathBuf {
        download_dir.parent().unwrap_or(download_dir).join("torrent-engine.log")
    }

    /// The last lines the engine wrote, for error messages.
    pub fn log_tail(download_dir: &Path) -> String {
        // Some builds of the engine write UTF-16 or NUL bytes when the output goes to a file: drop them so the text stays readable.
        let raw = std::fs::read(Self::log_path(download_dir)).unwrap_or_default();
        let raw: Vec<u8> = raw.into_iter().filter(|b| *b != 0).collect();
        let text: String = String::from_utf8_lossy(&raw).chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\u{1b}').collect();
        let lines: Vec<String> = text.lines().map(strip_ansi).collect();
        Self::summarize_log(&lines)
    }

    /// The error the engine gave and its causes when there is one (the backtrace after it is left out); otherwise the
    /// last few lines.
    fn summarize_log(lines: &[String]) -> String {
        if let Some(i) = lines.iter().position(|l| l.contains(" ERROR ") || l.starts_with("Error:") || l.starts_with("error:")) {
            let mut out: Vec<String> = vec![lines[i].split("ERROR").last().unwrap_or(&lines[i]).trim().to_string()];
            for l in &lines[i + 1..] {
                let t = l.trim();
                if t.to_lowercase().starts_with("stack backtrace") {
                    break;
                }
                if !t.is_empty() && !t.starts_with("at ") {
                    out.push(t.to_string());
                }
                if out.len() >= 8 {
                    break;
                }
            }
            return out.join(" | ");
        }
        let clean: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("at ") && !l.to_lowercase().contains("stack backtrace")).collect();
        let tail: Vec<String> = clean.iter().rev().take(4).rev().map(|l| l.to_string()).collect();
        tail.join(" | ")
    }

    pub fn start(exe: &Path, download_dir: &Path, port: u16, extra: &[String]) -> Result<RqbitProcess, String> {
        crate::platform::needs_program("This build has no torrent engine. Use Real-Debrid for streams (Preferences).")?;
        std::fs::create_dir_all(download_dir).map_err(|e| e.to_string())?;
        Self::kill_stale(exe, download_dir);
        let log = std::fs::File::create(Self::log_path(download_dir)).ok();
        let (out, err) = match log.and_then(|f| f.try_clone().ok().map(|c| (f, c))) {
            Some((a, b)) => (Stdio::from(a), Stdio::from(b)),
            None => (Stdio::null(), Stdio::null()),
        };
        let mut cmd = Command::new(exe);
        cmd.arg("--http-api-listen-addr").arg(format!("127.0.0.1:{}", port));
        cmd.args(extra);
        cmd.arg("server").arg("start").arg("--disable-persistence").arg(download_dir).stdin(Stdio::null()).stdout(out).stderr(err);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = cmd.spawn().map_err(|e| format!("Could not start the torrent engine ({}): {}", exe.display(), e))?;
        let _ = std::fs::write(Self::pid_path(download_dir), child.id().to_string());
        #[cfg(windows)]
        win_job::adopt(&child);
        Ok(RqbitProcess { child: Some(child), port })
    }

    /// The engine inside the app, serving on `port`.
    #[cfg(feature = "torrent-embed")]
    pub fn embedded(download_dir: &Path, port: u16) -> Result<RqbitProcess, String> {
        crate::torrent_embed::start(download_dir, port)?;
        Ok(RqbitProcess { child: None, port })
    }

    fn pid_path(download_dir: &Path) -> std::path::PathBuf {
        Self::log_path(download_dir).with_extension("pid")
    }

    /// Kills an engine left over from a previous run (crash or forced close) that still holds the ports and the log:
    /// the one named in the pid file, and on Windows any other copy started from the same program file.
    fn kill_stale(exe: &Path, download_dir: &Path) {
        let pp = Self::pid_path(download_dir);
        let mut killed = false;
        if let Some(pid) = std::fs::read_to_string(&pp).ok().and_then(|t| t.trim().parse::<u32>().ok()) {
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F", "/T"]).creation_flags(0x0800_0000).stdout(Stdio::null()).stderr(Stdio::null()).status();
            }
            #[cfg(not(windows))]
            {
                let _ = Command::new("kill").arg("-9").arg(pid.to_string()).stdout(Stdio::null()).stderr(Stdio::null()).status();
            }
            killed = true;
        }
        let _ = std::fs::remove_file(pp);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let path = exe.to_string_lossy().replace('\'', "''");
            let script = format!("Get-Process -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -and ($_.Path -ieq '{}') }} | Stop-Process -Force -ErrorAction SilentlyContinue", path);
            if let Ok(st) = Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).creation_flags(0x0800_0000).stdout(Stdio::null()).stderr(Stdio::null()).status() {
                killed |= st.success();
            }
        }
        #[cfg(not(windows))]
        let _ = exe;
        if killed {
            std::thread::sleep(Duration::from_millis(400));
        }
    }

    pub fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn wait_ready(&mut self, http: &dyn Http, secs: u64) -> Result<(), String> {
        let started = Instant::now();
        loop {
            if let Some(Ok(Some(status))) = self.child.as_mut().map(|c| c.try_wait()) {
                return Err(format!("The torrent engine stopped right after starting ({}).", status));
            }
            if let Ok(r) = http.send(&Request::get(&format!("{}/", self.base())).timeout(2)) {
                if r.status < 500 {
                    return Ok(());
                }
            }
            if started.elapsed().as_secs() >= secs {
                return Err("The torrent engine did not start in time.".to_string());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

/// Windows does not end child programs with their parent. Every engine joins a job that Windows closes, and so ends,
/// when Reelhouse exits for any reason (closed, crashed or killed), so no engine is left holding the ports.
#[cfg(windows)]
pub(crate) mod win_job {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    #[derive(Default)]
    struct Basic {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        v: [u64; 6],
    }
    #[repr(C)]
    #[derive(Default)]
    struct Extended {
        basic: Basic,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }
    extern "system" {
        fn CreateJobObjectW(attrs: *const c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(job: *mut c_void, class: i32, info: *const c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    }
    struct Job(*mut c_void);
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}
    static JOB: std::sync::OnceLock<Option<Job>> = std::sync::OnceLock::new();

    pub fn adopt(child: &std::process::Child) {
        let job = JOB.get_or_init(|| unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return None;
            }
            let mut info = Extended::default();
            info.basic.limit_flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            let ok = SetInformationJobObject(h, 9, &info as *const Extended as *const c_void, std::mem::size_of::<Extended>() as u32);
            if ok == 0 {
                return None;
            }
            Some(Job(h)) // never closed: Windows closes it when Reelhouse ends
        });
        if let Some(j) = job {
            unsafe {
                AssignProcessToJobObject(j.0, child.as_raw_handle() as *mut c_void);
            }
        }
    }
}

impl Drop for RqbitProcess {
    fn drop(&mut self) {
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        #[cfg(feature = "torrent-embed")]
        if self.child.is_none() {
            crate::torrent_embed::stop(self.port);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn engine_log_shows_the_error_not_the_backtrace() {
        let log: Vec<String> = ["2026 INFO rqbit: increased open file limit", "2026 ERROR rqbit: error running rqbit: error initializing rqbit session", "", "Caused by:", "    0: error creating UDP tracker client", "    1: error creating socket: Address family not supported", "", "Stack backtrace:", "   0: main", "             at ./x.rs:1"].iter().map(|s| s.to_string()).collect();
        let s = RqbitProcess::summarize_log(&log);
        assert!(s.starts_with("rqbit: error running rqbit"), "{}", s);
        assert!(s.contains("Address family not supported"));
        assert!(!s.contains("main"));
        let plain: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(RqbitProcess::summarize_log(&plain), "a | b");
    }

    #[test]
    fn season_named_beats_season_unknown() {
        let files = vec![
            ("Pokemon S08X01 Chronicles/Pokemon Chronicles E03 The Legend.mp4".to_string(), 900u64),
            ("Season 6/Pokemon S06E03 There's No Place Like Hoenn.mkv".to_string(), 300u64),
        ];
        assert_eq!(pick_episode(&files, None, Some(6), Some(3)), Some(1));
    }

    #[test]
    fn picks_the_right_file() {
        let files = vec![("a.nfo".to_string(), 10), ("movie.MKV".to_string(), 5000), ("sample.mkv".to_string(), 50), ("b.srt".to_string(), 9000)];
        assert_eq!(pick_file(&files, None), Some(1));
        assert_eq!(pick_file(&files, Some(2)), Some(2));
        assert_eq!(pick_file(&files, Some(99)), Some(1));
        assert_eq!(pick_file(&[("x.bin".to_string(), 1), ("y.bin".to_string(), 2)], None), Some(1));
        assert_eq!(pick_file(&[], None), None);
    }

    #[test]
    fn adds_a_magnet_and_builds_the_stream_url() {
        let http = Fake::new().route(
            "POST http://127.0.0.1:3030/torrents?overwrite=true",
            200,
            r#"{"id":4,"details":{"files":[{"name":"d/a.mkv","length":123},{"name":"d/b.srt","length":4}]}}"#,
        );
        let rq = Rqbit { http: &http, base: "http://127.0.0.1:3030".into() };
        let (id, files) = rq.add("magnet:?xt=urn:btih:abc").unwrap();
        assert_eq!((id, files.len()), (4, 2));
        assert_eq!(rq.stream_url(id, 0), "http://127.0.0.1:3030/torrents/4/stream/0");
        let sent = &http.requests()[0];
        assert_eq!(String::from_utf8_lossy(&sent.body.as_ref().unwrap().1), "magnet:?xt=urn:btih:abc");
    }

    #[test]
    fn waits_for_the_stream_and_reads_stats() {
        let http = Fake::new()
            .route("GET http://127.0.0.1:3030/torrents/4/stream/0", 503, "starting")
            .route("GET http://127.0.0.1:3030/torrents/4/stream/0", 206, "data")
            .route("stats/v1", 200, r#"{"state":"live","progress_bytes":2097152,"total_bytes":4194304,"live":{"download_speed":{"mbps":2.0},"snapshot":{"peer_stats":{"live":7}}}}"#);
        let rq = Rqbit { http: &http, base: "http://127.0.0.1:3030".into() };
        let seen = std::cell::RefCell::new(Vec::new());
        rq.wait_stream(4, "http://127.0.0.1:3030/torrents/4/stream/0", 30, &|m| seen.borrow_mut().push(m), &|| false).unwrap();
        assert!(seen.borrow()[0].contains("7 found"));
        let st = rq.stats(4).unwrap();
        assert_eq!((st.peers, st.percent() as u32, st.speed as u64), (7, 50, 2_097_152));
    }

    #[test]
    fn lists_files_and_downloads_only_some() {
        let http = Fake::new()
            .route("POST http://e/torrents?list_only=true", 200, r#"{"details":{"name":"Pack","files":[{"name":"Pack/a.mkv","length":10},{"name":"Pack/b.mkv","length":20}]}}"#)
            .route("POST http://e/torrents?overwrite=true&only_files=1,2", 200, r#"{"id":4,"details":{"name":"Pack","files":[{"name":"Pack/a.mkv","length":10}]}}"#)
            .route("/torrents/abc/delete", 200, "{}")
            .route("stats/v1", 200, r#"{"state":"live","file_progress":[10,5],"progress_bytes":15,"total_bytes":30,"finished":false}"#);
        let rq = Rqbit { http: &http, base: "http://e".into() };
        let (name, files) = rq.list_files("magnet:?xt=urn:btih:abc").unwrap();
        assert_eq!((name.as_str(), files.len()), ("Pack", 2));
        let (id, name, _) = rq.add_only("magnet:?xt=urn:btih:abc", &[1, 2]).unwrap();
        assert_eq!((id, name.as_str()), (4, "Pack"));
        assert!(rq.delete("abc"));
        let st = rq.stats(4).unwrap();
        assert_eq!((st.file_progress.clone(), st.finished), (vec![10, 5], false));
    }

    #[test]
    fn reports_engine_errors() {
        let http = Fake::new().route("/torrents", 400, "bad magnet");
        let rq = Rqbit { http: &http, base: "http://127.0.0.1:3030".into() };
        assert!(rq.add("x").unwrap_err().contains("refused"));
    }
}

// ---------- getting the engine ----------

pub const ENGINE_RELEASE_API: &str = "https://api.github.com/repos/ikatson/rqbit/releases/latest";

/// Picks the release file for this system out of a GitHub "latest release" answer.
/// Returns (file name, download URL). The desktop app and checksum files are skipped.
pub fn pick_engine_asset(release: &crate::json::Json, os: &str, arch: &str) -> Option<(String, String)> {
    let os_words: &[&str] = match os {
        "windows" => &["windows", "win"],
        "macos" => &["macos", "darwin", "osx", "apple"],
        _ => &["linux"],
    };
    let arch_words: &[&str] = match arch {
        "aarch64" => &["aarch64", "arm64"],
        _ => &["x86_64", "amd64", "x64"],
    };
    let bad_ext = [".sha256", ".sha256sum", ".sig", ".asc", ".txt", ".md", ".msi", ".dmg", ".deb", ".rpm", ".apk", ".appimage", ".json", ".pkg"];
    let mut best: Option<(i32, String, String)> = None;
    for a in release.get("assets").items() {
        let name = a.get("name").str();
        let url = a.get("browser_download_url").str();
        let l = name.to_lowercase();
        if url.is_empty() || bad_ext.iter().any(|e| l.ends_with(e)) || l.contains("desktop") || l.contains("webui") {
            continue;
        }
        // The Windows build is published as a bare "rqbit.exe" with no system name in it.
        let windows_exe = os == "windows" && l.ends_with(".exe");
        if !windows_exe && !os_words.iter().any(|w| l.contains(w)) {
            continue;
        }
        let mut score = 10;
        if arch_words.iter().any(|w| l.contains(w)) {
            score += 20;
        } else if l.contains("arm") || l.contains("i686") || l.contains("x86.") {
            continue;
        }
        if l.contains("static") || l.contains("musl") {
            score += 3;
        }
        if l.contains("rqbit") {
            score += 5;
        }
        if l.ends_with(".exe") || !l.contains('.') {
            score += 2;
        }
        if best.as_ref().map(|(s, _, _)| score > *s).unwrap_or(true) {
            best = Some((score, name, url));
        }
    }
    best.map(|(_, n, u)| (n, u))
}

pub(crate) fn find_file(dir: &Path, wanted: &[&str]) -> Option<std::path::PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, wanted) {
                return Some(f);
            }
        } else if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
            if wanted.iter().any(|w| n.eq_ignore_ascii_case(w)) {
                return Some(p);
            }
        }
    }
    None
}

/// Downloads the torrent engine into `dir` (once) and returns the path of the program.
pub fn install_engine(http: &dyn Http, dir: &Path, status: &dyn Fn(String)) -> Result<std::path::PathBuf, String> {
    crate::platform::needs_program("This build has no torrent engine. Use Real-Debrid for streams (Preferences).")?;
    let exe_name = if cfg!(windows) { "rqbit.exe" } else { "rqbit" };
    let target = dir.join(exe_name);
    if target.is_file() {
        return Ok(target);
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    status("Downloading the torrent engine (first time only)".to_string());
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
    let rel = crate::http::get_json(http, ENGINE_RELEASE_API, 30)
        .map_err(|e| format!("Could not look up the torrent engine: {}. Turn on Real-Debrid, or check your internet connection.", e))?;
    let (name, url) = pick_engine_asset(&rel, os, arch).ok_or_else(|| format!("No torrent engine build was found for {} {}.", os, arch))?;
    let r = http.send(&Request::get(&url).timeout(900))?;
    if !r.ok() {
        return Err(format!("Downloading the torrent engine failed ({}).", r.status));
    }
    let lower = name.to_lowercase();
    let archive = lower.ends_with(".zip") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".tar.xz");
    if !archive {
        std::fs::write(&target, &r.body).map_err(|e| e.to_string())?;
    } else {
        let tmp = dir.join("download");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let file = tmp.join(&name);
        std::fs::write(&file, &r.body).map_err(|e| e.to_string())?;
        let out = tmp.join("x");
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let mut ok = Command::new("tar").arg("-xf").arg(&file).arg("-C").arg(&out).status().map(|s| s.success()).unwrap_or(false);
        if !ok && lower.ends_with(".zip") {
            ok = Command::new("unzip").arg("-o").arg(&file).arg("-d").arg(&out).status().map(|s| s.success()).unwrap_or(false);
        }
        if !ok {
            return Err("Could not unpack the torrent engine download.".to_string());
        }
        let found = find_file(&out, &["rqbit.exe", "rqbit"]).ok_or_else(|| "The torrent engine download did not contain a program.".to_string())?;
        std::fs::copy(&found, &target).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
    }
    Ok(target)
}

#[cfg(test)]
mod engine_tests {
    use super::*;
    use crate::json::Json;

    fn release() -> Json {
        Json::parse(
            r#"{"assets":[
            {"name":"rqbit-desktop-windows-x86_64.msi","browser_download_url":"https://x/a.msi"},
            {"name":"rqbit-windows-x86_64.exe","browser_download_url":"https://x/win.exe"},
            {"name":"rqbit-windows-x86_64.exe.sha256","browser_download_url":"https://x/win.sha"},
            {"name":"rqbit-linux-static-x86_64","browser_download_url":"https://x/lin"},
            {"name":"rqbit-linux-static-aarch64","browser_download_url":"https://x/linarm"},
            {"name":"rqbit-osx-universal","browser_download_url":"https://x/mac"}
        ]}"#,
        )
        .unwrap()
    }

    #[test]
    fn picks_by_system() {
        assert_eq!(pick_engine_asset(&release(), "windows", "x86_64").unwrap().1, "https://x/win.exe");
        assert_eq!(pick_engine_asset(&release(), "linux", "x86_64").unwrap().1, "https://x/lin");
        assert_eq!(pick_engine_asset(&release(), "linux", "aarch64").unwrap().1, "https://x/linarm");
    }

    #[test]
    fn real_release_names() {
        let j = Json::parse(
            r#"{"assets":[
            {"name":"rqbit-desktop-linux-amd64","browser_download_url":"d"},
            {"name":"rqbit-desktop_9.0.1_x64_en-US.msi","browser_download_url":"m"},
            {"name":"rqbit-linux-amd64","browser_download_url":"lin"},
            {"name":"rqbit-linux-arm64","browser_download_url":"arm"},
            {"name":"rqbit-osx-universal","browser_download_url":"mac"},
            {"name":"rqbit.exe","browser_download_url":"win"}]}"#,
        )
        .unwrap();
        assert_eq!(pick_engine_asset(&j, "windows", "x86_64").unwrap().1, "win");
        assert_eq!(pick_engine_asset(&j, "linux", "x86_64").unwrap().1, "lin");
        assert_eq!(pick_engine_asset(&j, "linux", "aarch64").unwrap().1, "arm");
        assert_eq!(pick_engine_asset(&j, "macos", "aarch64").unwrap().1, "mac");
    }

    #[test]
    fn nothing_for_unknown_system() {
        let j = Json::parse(r#"{"assets":[{"name":"notes.txt","browser_download_url":"u"}]}"#).unwrap();
        assert!(pick_engine_asset(&j, "windows", "x86_64").is_none());
    }
}
