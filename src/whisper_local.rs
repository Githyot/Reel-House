//! Whisper on this computer: whisper.cpp's `whisper-server`, run by Reelhouse itself so the voice of Conversation
//! works offline and stays private (nothing is sent anywhere). Managed like the local AI (`ollama_local`).
//!
//! `Local::run` (in a background thread): a `whisper-server` already installed (PATH; Homebrew and distribution
//! packages; it may be a GPU build) → the portable copy in `<data>/whisper/runtime` → on Windows x64 and Linux x64
//! the official build of the whisper.cpp release downloaded and unpacked there. Then the ggml model of the language
//! (`voice::whisper_model`) and, when the program knows `--vad-model`, Silero VAD are downloaded to
//! `<data>/whisper/models`, and the server is started (hidden, on a free port of 127.0.0.1, stopped when Reelhouse
//! closes) with the model loaded once; each sentence is then one POST to /inference (`voice::whisper_transcribe`).
//! A server of another model is restarted with the new one. Each step is in `Status`, like `ollama_local`.
use crate::http::{Http, Request};
use crate::json::Json;
use crate::ollama_local::{hidden, kill_tree, log_tail, unpack};
use crate::voice::{self, WhisperModel};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// The release whose binaries were checked (2026-10-15: v1.9.5 carries only the sources, its build b5454 has
/// whisper-bin-x64.zip (8.5 MB, CPU) and whisper-bin-ubuntu-x64.tar.gz (9.9 MB)). When it is gone, the newest
/// release with the same file is found through GitHub's API.
pub const RELEASE_TAG: &str = "b5454";
pub const RELEASES_API: &str = "https://api.github.com/repos/ggml-org/whisper.cpp/releases?per_page=20";

pub fn release_url(tag: &str, asset: &str) -> String {
    format!("https://github.com/ggml-org/whisper.cpp/releases/download/{}/{}", tag, asset)
}

/// The prebuilt program for this system (`os`, `arch` as in std::env::consts), or None: install whisper.cpp.
pub fn platform_asset(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("whisper-bin-x64.zip"),
        ("linux", "x86_64") => Some("whisper-bin-ubuntu-x64.tar.gz"),
        _ => None,
    }
}

/// The download address of `asset` in the newest release of GitHub's release list that has it.
pub fn pick_release_asset(releases: &Json, asset: &str) -> Option<String> {
    releases.items().iter().filter(|r| r.get("draft").as_bool() != Some(true)).find_map(|r| {
        r.get("assets").items().iter().find(|a| a.get("name").str() == asset).map(|a| a.get("browser_download_url").str()).filter(|u| u.starts_with("https://"))
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// "idle", "checking", "downloading_runtime", "downloading_model", "starting", "ready" or "error"
    pub phase: String,
    pub message: String,
    pub done: u64,
    pub total: u64,
    /// the model id wanted (or loaded, when ready)
    pub model: String,
    pub busy: bool,
    /// the server was started by Reelhouse
    pub ours: bool,
    /// where it answers ("http://127.0.0.1:<port>")
    pub root: String,
    /// it cuts silence with Silero VAD
    pub vad: bool,
    /// the program used
    pub program: String,
}

impl Status {
    pub fn percent(&self) -> f64 {
        if self.phase == "ready" {
            100.0
        } else if self.total > 0 {
            (self.done as f64 * 100.0 / self.total as f64).clamp(0.0, 100.0)
        } else {
            0.0
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("phase", Json::Str(if self.phase.is_empty() { "idle".into() } else { self.phase.clone() })),
            ("message", Json::Str(self.message.clone())),
            ("done", Json::Num(self.done as f64)),
            ("total", Json::Num(self.total as f64)),
            ("percent", Json::Num((self.percent() * 10.0).round() / 10.0)),
            ("model", Json::Str(self.model.clone())),
            ("busy", Json::Bool(self.busy)),
            ("ready", Json::Bool(self.phase == "ready")),
            ("ours", Json::Bool(self.ours)),
            ("vad", Json::Bool(self.vad)),
            ("program", Json::Str(self.program.clone())),
            ("canDownload", Json::Bool(platform_asset(std::env::consts::OS, std::env::consts::ARCH).is_some())),
            ("available", Json::Bool(true)),
        ])
    }
}

#[derive(Default)]
struct Inner {
    st: Mutex<Status>,
    cv: Condvar,
    /// the server started here and the model it loaded
    child: Mutex<Option<(Child, String)>>,
}

/// The handle kept by the core; clones share the same state.
#[derive(Clone, Default)]
pub struct Local {
    inner: Arc<Inner>,
}

/// `<data>/whisper`: the portable program (runtime), the models, the server's log.
pub fn home(data_dir: &Path) -> PathBuf {
    data_dir.join("whisper")
}

pub fn model_path(data_dir: &Path, m: &WhisperModel) -> PathBuf {
    home(data_dir).join("models").join(m.file)
}

/// Where a `whisper-server` may be: each folder of PATH, then the usual install places outside Windows.
pub fn candidates(path_var: &str, windows: bool) -> Vec<PathBuf> {
    let names: &[&str] = if windows { &["whisper-server.exe"] } else { &["whisper-server", "whisper-cpp-server"] };
    let sep = if windows { ';' } else { ':' };
    let mut dirs: Vec<PathBuf> = path_var.split(sep).map(|d| d.trim().trim_matches('"')).filter(|d| !d.is_empty()).map(PathBuf::from).collect();
    if !windows {
        for d in ["/usr/local/bin", "/usr/bin", "/opt/homebrew/bin"] {
            if !dirs.iter().any(|x| x == Path::new(d)) {
                dirs.push(PathBuf::from(d));
            }
        }
    }
    dirs.iter().flat_map(|d| names.iter().map(move |n| d.join(n))).collect()
}

/// The first program that exists: an installed one first (it may be a GPU build), then the portable copy.
pub fn find_program(cands: &[PathBuf], runtime: &Path, exists: &dyn Fn(&Path) -> bool) -> Option<(PathBuf, bool)> {
    if let Some(p) = cands.iter().find(|p| exists(p)) {
        return Some((p.clone(), false));
    }
    let exe = if cfg!(windows) { "whisper-server.exe" } else { "whisper-server" };
    let direct = runtime.join(exe);
    if exists(&direct) {
        return Some((direct, true));
    }
    if !runtime.is_dir() {
        return None;
    }
    crate::torrent::find_file(runtime, &[exe]).map(|p| (p, true))
}

/// Whether the program and the model are on this computer already (a fallback may then start it without a download).
pub fn installed(data_dir: &Path, m: &WhisperModel) -> bool {
    if !model_path(data_dir, m).is_file() {
        return false;
    }
    let cands = candidates(&std::env::var("PATH").unwrap_or_default(), cfg!(windows));
    find_program(&cands, &home(data_dir).join("runtime"), &|p: &Path| p.is_file()).is_some()
}

/// Whether the program's help names the VAD options (whisper.cpp 1.7.6 and later).
pub fn supports_vad(help: &str) -> bool {
    help.contains("--vad-model") || help.contains("--vad ")
}

/// The command line of the server.
pub fn server_args(model: &Path, port: u16, threads: usize, vad: Option<&Path>) -> Vec<String> {
    let mut a: Vec<String> = vec!["-m".into(), model.to_string_lossy().into_owned(), "--host".into(), "127.0.0.1".into(), "--port".into(), port.to_string(), "-t".into(), threads.to_string()];
    if let Some(v) = vad {
        a.extend(["--vad".to_string(), "--vad-model".to_string(), v.to_string_lossy().into_owned()]);
    }
    a
}

/// Threads for whisper.cpp: about the physical cores (half the logical ones), 2 to 8.
pub fn threads() -> usize {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    (n / 2).clamp(2, 8)
}

/// What the server says: Some(true) ready, Some(false) loading its model, None not answering.
pub fn health(http: &dyn Http, root: &str) -> Option<bool> {
    let r = http.send(&Request::get(&format!("{}/health", root)).timeout(3)).ok()?;
    match r.status {
        200 => Some(true),
        503 => Some(false),
        // an older server without /health: its page answers once it listens
        _ => http.send(&Request::get(&format!("{}/", root)).timeout(3)).ok().map(|r| r.ok()),
    }
}

fn free_port() -> Result<u16, String> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    l.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
}

/// The portable Linux build keeps its libraries beside the program (or in ../lib).
fn library_env(cmd: &mut Command, exe: &Path, portable: bool) {
    if portable && cfg!(target_os = "linux") {
        if let Some(dir) = exe.parent() {
            let mut paths = vec![dir.to_path_buf(), dir.join("..").join("lib")];
            if let Ok(old) = std::env::var("LD_LIBRARY_PATH") {
                paths.extend(old.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
            }
            if let Ok(v) = std::env::join_paths(paths) {
                cmd.env("LD_LIBRARY_PATH", v);
            }
        }
    }
}

impl Local {
    pub fn status(&self) -> Status {
        self.inner.st.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Claims the setup for the model `id`: true when the caller must run `run` (in a thread).
    pub fn begin(&self, id: &str) -> bool {
        let Ok(mut s) = self.inner.st.lock() else { return false };
        if s.busy || (s.phase == "ready" && s.model == id) {
            return false;
        }
        s.busy = true;
        s.phase = "checking".into();
        s.message = "Checking Whisper on this computer…".into();
        s.model = id.into();
        s.done = 0;
        s.total = 0;
        true
    }

    /// The ready state is forgotten (the server stopped answering): the next `begin` checks again.
    pub fn invalidate(&self) {
        if let Ok(mut s) = self.inner.st.lock() {
            if !s.busy && s.phase == "ready" {
                s.phase = "idle".into();
                s.message.clear();
            }
        }
    }

    fn set(&self, f: impl FnOnce(&mut Status)) {
        if let Ok(mut s) = self.inner.st.lock() {
            f(&mut s);
        }
        self.inner.cv.notify_all();
    }

    /// Blocks until the model `id` is ready (its server's address), the setup failed (its message) or `timeout`.
    pub fn wait_ready(&self, id: &str, timeout: Duration) -> Result<(String, bool), String> {
        let end = Instant::now() + timeout;
        let Ok(mut s) = self.inner.st.lock() else { return Err("Whisper on this computer is not available.".into()) };
        loop {
            if s.phase == "ready" && s.model == id {
                return Ok((s.root.clone(), s.vad));
            }
            if !s.busy {
                return Err(if s.phase == "error" { s.message.clone() } else { "Whisper on this computer is not ready.".into() });
            }
            let now = Instant::now();
            if now >= end {
                return Err("Whisper on this computer took too long to get ready.".into());
            }
            s = match self.inner.cv.wait_timeout(s, end - now) {
                Ok((g, _)) => g,
                Err(_) => return Err("Whisper on this computer is not available.".into()),
            };
        }
    }

    /// The whole setup (call after `begin` returned true). `notify` is called when the status changes.
    pub fn run(&self, http: &dyn Http, data_dir: &Path, notify: &dyn Fn()) {
        let id = self.status().model;
        let r = self.run_inner(http, data_dir, &id, notify);
        self.set(|s| {
            s.busy = false;
            match &r {
                Ok(()) => {
                    s.phase = "ready".into();
                    s.message = "Whisper is ready on this computer.".into();
                    s.done = s.total;
                }
                Err(e) => {
                    s.phase = "error".into();
                    s.message = e.clone();
                }
            }
        });
        match &r {
            Ok(()) => {
                let st = self.status();
                crate::debug::log("voice", &format!("local whisper ready: model {} at {} (vad {}, {})", st.model, st.root, st.vad, st.program));
            }
            Err(e) => crate::debug::log("voice", &format!("local whisper failed: {}", e)),
        }
        notify();
    }

    fn run_inner(&self, http: &dyn Http, data_dir: &Path, id: &str, notify: &dyn Fn()) -> Result<(), String> {
        crate::platform::needs_program("Local Whisper cannot run on phones and tablets. Use the cloud speech recognition.")?;
        let Some(m) = voice::WHISPER_MODELS.iter().find(|m| m.id == id) else { return Err(format!("Unknown Whisper model {}.", id)) };
        let home = home(data_dir);
        // our server with this model still answers: nothing to do
        let running = self.inner.child.lock().ok().and_then(|c| c.as_ref().map(|(_, loaded)| loaded.clone()));
        if running.as_deref() == Some(id) {
            let root = self.status().root;
            if health(http, &root) == Some(true) {
                return Ok(());
            }
        }
        self.stop_child();
        let runtime = home.join("runtime");
        let cands = candidates(&std::env::var("PATH").unwrap_or_default(), cfg!(windows));
        let (exe, portable) = match find_program(&cands, &runtime, &|p: &Path| p.is_file()) {
            Some(f) => f,
            None => match platform_asset(std::env::consts::OS, std::env::consts::ARCH) {
                Some(asset) => (self.download_runtime(http, &home, &runtime, asset, notify)?, true),
                None => {
                    return Err("whisper.cpp is not installed on this computer. Install it (macOS: brew install whisper-cpp; Linux: the whisper-cpp package) so that whisper-server is on the PATH, then try again.".into())
                }
            },
        };
        let model = model_path(data_dir, m);
        if !model.is_file() {
            self.download(http, m.url, &model, "downloading_model", "Downloading the voice model…", notify)?;
        }
        // Silero VAD, when this build knows it (a failed download only means no VAD)
        let vad_file = home.join("models").join(voice::WHISPER_VAD_FILE);
        let mut vad = supports_vad(&self.help(&exe, portable));
        if vad && !vad_file.is_file() {
            if let Err(e) = self.download(http, voice::WHISPER_VAD_URL, &vad_file, "downloading_model", "Downloading the voice detector…", notify) {
                crate::debug::log("voice", &format!("no Silero VAD: {}", e));
                vad = false;
            }
        }
        match self.start_server(http, &home, &exe, portable, &model, if vad { Some(&vad_file) } else { None }, id, notify) {
            Err(e) if vad => {
                // a VAD model this build cannot read: once more without it
                crate::debug::log("voice", &format!("whisper-server with VAD failed ({}), starting it without", e));
                self.start_server(http, &home, &exe, portable, &model, None, id, notify)
            }
            r => r,
        }
    }

    /// The program's help text (the options it knows).
    fn help(&self, exe: &Path, portable: bool) -> String {
        let mut c = Command::new(exe);
        c.arg("--help").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        library_env(&mut c, exe, portable);
        hidden(&mut c);
        match c.output() {
            Ok(o) => format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
            Err(_) => String::new(),
        }
    }

    /// A file downloaded to `to` (through a .part file), with its progress in the status.
    fn download(&self, http: &dyn Http, url: &str, to: &Path, phase: &str, message: &str, notify: &dyn Fn()) -> Result<(), String> {
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        self.set(|s| {
            s.phase = phase.into();
            s.message = message.into();
            s.done = 0;
            s.total = 0;
        });
        notify();
        let part = to.with_extension("part");
        let last = Mutex::new(Instant::now());
        let r = http.download(url, &part, &|done, total| {
            self.set(|s| {
                s.done = done;
                s.total = total.unwrap_or(0);
            });
            if let Ok(mut l) = last.lock() {
                if l.elapsed() >= Duration::from_millis(400) {
                    *l = Instant::now();
                    notify();
                }
            }
        });
        if let Err(e) = r {
            let _ = std::fs::remove_file(&part);
            return Err(format!("Could not download {} ({}). Check the internet connection and try again.", url.rsplit('/').next().unwrap_or(url), e));
        }
        let size = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if size < 1024 {
            let _ = std::fs::remove_file(&part);
            return Err(format!("The download of {} was empty. Try again.", url.rsplit('/').next().unwrap_or(url)));
        }
        std::fs::rename(&part, to).map_err(|e| format!("Could not put {} in place: {}", to.display(), e))
    }

    /// The release build downloaded and unpacked into `runtime`.
    fn download_runtime(&self, http: &dyn Http, home: &Path, runtime: &Path, asset: &str, notify: &dyn Fn()) -> Result<PathBuf, String> {
        std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
        let archive = home.join(asset);
        let msg = "Downloading the Whisper program (about 10 MB)…";
        if let Err(first) = self.download(http, &release_url(RELEASE_TAG, asset), &archive, "downloading_runtime", msg, notify) {
            // the pinned build is gone: the newest release that has the same file
            let url = http
                .send(&Request::get(RELEASES_API).header("Accept", "application/vnd.github+json").timeout(30))
                .ok()
                .filter(|r| r.ok())
                .and_then(|r| r.json().ok())
                .and_then(|j| pick_release_asset(&j, asset))
                .ok_or(first)?;
            self.download(http, &url, &archive, "downloading_runtime", msg, notify)?;
        }
        self.set(|s| s.message = "Unpacking the Whisper program…".into());
        notify();
        let tmp = home.join("runtime.new");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let r = unpack(&archive, &tmp);
        let _ = std::fs::remove_file(&archive);
        if r.is_err() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err("Could not unpack the Whisper program.".into());
        }
        let _ = std::fs::remove_dir_all(runtime);
        std::fs::rename(&tmp, runtime).map_err(|e| format!("Could not put the Whisper program in place: {}", e))?;
        let exe = if cfg!(windows) { "whisper-server.exe" } else { "whisper-server" };
        crate::torrent::find_file(runtime, &[exe]).ok_or_else(|| format!("The Whisper download did not contain {}.", exe))
    }

    #[allow(clippy::too_many_arguments)]
    fn start_server(&self, http: &dyn Http, home: &Path, exe: &Path, portable: bool, model: &Path, vad: Option<&Path>, id: &str, notify: &dyn Fn()) -> Result<(), String> {
        self.set(|s| {
            s.phase = "starting".into();
            s.message = "Starting Whisper on this computer…".into();
            s.done = 0;
            s.total = 0;
        });
        notify();
        let port = free_port()?;
        let root = format!("http://127.0.0.1:{}", port);
        let log_path = home.join("server.log");
        let (out, err) = match std::fs::File::create(&log_path).ok().and_then(|f| f.try_clone().ok().map(|c| (f, c))) {
            Some((a, b)) => (Stdio::from(a), Stdio::from(b)),
            None => (Stdio::null(), Stdio::null()),
        };
        let mut cmd = Command::new(exe);
        cmd.args(server_args(model, port, threads(), vad)).stdin(Stdio::null()).stdout(out).stderr(err);
        // the folder of the program: its DLLs, and the server's default public folder
        if let Some(dir) = exe.parent() {
            cmd.current_dir(dir);
        }
        library_env(&mut cmd, exe, portable);
        hidden(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("Could not start Whisper ({}): {}", exe.display(), e))?;
        #[cfg(windows)]
        crate::torrent::win_job::adopt(&child);
        let started = Instant::now();
        loop {
            if health(http, &root) == Some(true) {
                break;
            }
            if let Ok(Some(st)) = child.try_wait() {
                return Err(format!("Whisper stopped at start ({}): {}", st, log_tail(&log_path)));
            }
            if started.elapsed() > Duration::from_secs(180) {
                kill_tree(&mut child);
                return Err(format!("Whisper did not answer after starting: {}", log_tail(&log_path)));
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        if let Ok(mut c) = self.inner.child.lock() {
            if let Some((mut old, _)) = c.replace((child, id.to_string())) {
                kill_tree(&mut old);
            }
        }
        let program = exe.to_string_lossy().into_owned();
        self.set(|s| {
            s.ours = true;
            s.root = root;
            s.vad = vad.is_some();
            s.program = program;
        });
        Ok(())
    }

    fn stop_child(&self) {
        let child = self.inner.child.lock().ok().and_then(|mut c| c.take());
        if let Some((mut c, _)) = child {
            kill_tree(&mut c);
        }
    }

    /// Stops the server Reelhouse started.
    pub fn stop(&self) {
        let had = self.inner.child.lock().map(|c| c.is_some()).unwrap_or(false);
        self.stop_child();
        if had {
            self.set(|s| {
                s.ours = false;
                if !s.busy {
                    s.phase = "idle".into();
                    s.message.clear();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn the_program_is_looked_for_in_order() {
        let w = candidates(r#"C:\Windows;"C:\Tools";;"#, true);
        assert_eq!(w, vec![PathBuf::from(r"C:\Windows").join("whisper-server.exe"), PathBuf::from(r"C:\Tools").join("whisper-server.exe")]);
        let l = candidates("/usr/bin:/opt/x", false);
        assert_eq!(l[0], PathBuf::from("/usr/bin/whisper-server"));
        assert_eq!(l[1], PathBuf::from("/usr/bin/whisper-cpp-server"));
        assert!(l.contains(&PathBuf::from("/opt/homebrew/bin/whisper-server")));
        assert_eq!(l.iter().filter(|p| p.starts_with("/usr/bin")).count(), 2, "a folder is not listed twice");
        let runtime = Path::new("/data/whisper/runtime");
        let exe = if cfg!(windows) { "whisper-server.exe" } else { "whisper-server" };
        let both = |p: &Path| p == Path::new("/opt/x/whisper-server") || p == runtime.join(exe);
        assert_eq!(find_program(&l, runtime, &both), Some((PathBuf::from("/opt/x/whisper-server"), false)));
        let portable = |p: &Path| p == runtime.join(exe);
        assert_eq!(find_program(&l, runtime, &portable), Some((runtime.join(exe), true)));
        assert_eq!(find_program(&l, runtime, &|_: &Path| false), None);
    }

    #[test]
    fn the_release_build_is_found() {
        assert_eq!(platform_asset("windows", "x86_64"), Some("whisper-bin-x64.zip"));
        assert_eq!(platform_asset("linux", "x86_64"), Some("whisper-bin-ubuntu-x64.tar.gz"));
        assert_eq!(platform_asset("macos", "aarch64"), None);
        assert_eq!(release_url("b5454", "whisper-bin-x64.zip"), "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip");
        // v1.9.5 has only the sources; the build before it has the binaries
        let j = Json::parse(
            r#"[{"tag_name":"v1.9.5","draft":false,"assets":[]},
                {"tag_name":"b9999","draft":true,"assets":[{"name":"whisper-bin-x64.zip","browser_download_url":"https://x/draft.zip"}]},
                {"tag_name":"b5454","prerelease":true,"assets":[{"name":"whisper-bin-Win32.zip","browser_download_url":"https://x/32.zip"},{"name":"whisper-bin-x64.zip","browser_download_url":"https://x/64.zip"}]}]"#,
        )
        .unwrap();
        assert_eq!(pick_release_asset(&j, "whisper-bin-x64.zip").as_deref(), Some("https://x/64.zip"));
        assert_eq!(pick_release_asset(&j, "nothing.zip"), None);
    }

    #[test]
    fn the_server_command_line() {
        let a = server_args(Path::new("/m/ggml-small-q5_1.bin"), 8178, 4, Some(Path::new("/m/vad.bin")));
        assert_eq!(a[..8], ["-m", "/m/ggml-small-q5_1.bin", "--host", "127.0.0.1", "--port", "8178", "-t", "4"]);
        assert_eq!(a[8..], ["--vad", "--vad-model", "/m/vad.bin"]);
        assert_eq!(server_args(Path::new("m"), 1, 2, None).len(), 8);
        assert!(supports_vad("  --vad                  [false  ] enable Voice Activity Detection (VAD)\n  -vm FNAME, --vad-model FNAME"));
        assert!(!supports_vad("usage: whisper-server [options]\n  -m FNAME, --model FNAME"));
        assert!((2..=8).contains(&threads()));
    }

    #[test]
    fn health_reads_loading_and_ready() {
        let http = Fake::new()
            .route("GET http://127.0.0.1:1/health", 503, "{}")
            .route("GET http://127.0.0.1:2/health", 200, "{\"status\":\"ok\"}")
            .route("GET http://127.0.0.1:3/health", 404, "not found")
            .route("GET http://127.0.0.1:3/", 200, "<html>");
        assert_eq!(health(&http, "http://127.0.0.1:1"), Some(false));
        assert_eq!(health(&http, "http://127.0.0.1:2"), Some(true));
        // no /health (404 from the fake): the page
        assert_eq!(health(&http, "http://127.0.0.1:3"), Some(true));
        assert_eq!(health(&http, "http://127.0.0.1:4"), Some(false));
    }

    #[test]
    fn setup_is_claimed_once_and_waits() {
        let l = Local::default();
        assert!(l.begin("turbo"));
        assert!(!l.begin("turbo"), "one setup at a time");
        assert!(l.wait_ready("turbo", Duration::from_millis(20)).unwrap_err().contains("too long"));
        l.set(|s| {
            s.busy = false;
            s.phase = "ready".into();
            s.root = "http://127.0.0.1:5".into();
            s.vad = true;
        });
        assert_eq!(l.wait_ready("turbo", Duration::from_millis(5)).unwrap(), ("http://127.0.0.1:5".to_string(), true));
        assert!(l.wait_ready("small", Duration::from_millis(5)).is_err(), "another model is not ready");
        assert!(!l.begin("turbo"), "already ready");
        assert!(l.begin("small"), "another model is set up again");
        let s = l.status().to_json();
        assert_eq!(s.get("phase").str(), "checking");
        assert_eq!(s.get("busy").as_bool(), Some(true));
    }

    #[test]
    fn an_unknown_model_is_an_error() {
        let l = Local::default();
        assert!(l.begin("whatever"));
        l.run(&Fake::new(), Path::new("/nonexistent"), &|| {});
        let s = l.status();
        assert_eq!(s.phase, "error");
        assert!(!s.busy);
        assert!(l.begin("whatever"), "an error can be retried");
    }
}
