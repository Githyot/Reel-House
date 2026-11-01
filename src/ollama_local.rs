//! The local AI of Conversation: Ollama run by Reelhouse itself, so nothing has to be installed or typed.
//!
//! `Local::run` (in a background thread) goes through: a server already answering at `ROOT` (the Ollama app, or one
//! left from before) → an installed `ollama` program (PATH, %LOCALAPPDATA%\Programs\Ollama) → the portable copy in
//! `<data>/ollama/runtime` → on Windows, the official portable build downloaded and unpacked there. A program found is
//! started as `ollama serve` (hidden, stopped when Reelhouse closes; only the one Reelhouse started). Then the model is
//! pulled through `/api/pull`, with its progress. Each step is in `Status` (`phase`, bytes done / total, message).
//! The calls to the tutor wait for `ready` with `wait_ready` instead of failing while this runs.
use crate::http::{Http, Request};
use crate::json::Json;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Ollama's own API (the chat goes through its OpenAI-compatible `/v1`, see `llm::OLLAMA_BASE`).
pub const ROOT: &str = "http://127.0.0.1:11434";
/// The official portable build for Windows (the program and its libraries, no installer).
pub const WINDOWS_ZIP: &str = "https://github.com/ollama/ollama/releases/latest/download/ollama-windows-amd64.zip";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// "idle", "checking", "downloading_runtime", "starting", "pulling_model", "ready" or "error"
    pub phase: String,
    /// a sentence for the interface (English; the page translates it), or the error
    pub message: String,
    pub done: u64,
    pub total: u64,
    /// the model being prepared (or ready)
    pub model: String,
    /// a setup is running in the background
    pub busy: bool,
    /// the server was started by Reelhouse (and is stopped with it)
    pub ours: bool,
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
            // only Windows gets the program downloaded; elsewhere Ollama must be installed
            ("canDownload", Json::Bool(cfg!(windows))),
        ])
    }
}

#[derive(Default)]
struct Inner {
    st: Mutex<Status>,
    cv: Condvar,
    child: Mutex<Option<Child>>,
}

/// The handle kept by the core; clones share the same state.
#[derive(Clone, Default)]
pub struct Local {
    inner: Arc<Inner>,
}

/// The progress of `/api/pull`: one entry for each layer (digest) with its bytes done and total.
#[derive(Default, Debug)]
pub struct Pull {
    layers: Vec<(String, u64, u64)>,
    pub status: String,
    pub success: bool,
}

impl Pull {
    /// Reads one line of the stream. An `{"error":…}` line is returned as Err.
    pub fn feed(&mut self, line: &str) -> Result<(), String> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(());
        }
        let j = Json::parse(line).map_err(|_| format!("Ollama answered: {}", line.chars().take(160).collect::<String>()))?;
        let err = j.get("error").str();
        if !err.is_empty() {
            return Err(err);
        }
        let status = j.get("status").str();
        let digest = j.get("digest").str();
        let total = j.get("total").as_f64().unwrap_or(0.0) as u64;
        if !digest.is_empty() && total > 0 {
            let done = j.get("completed").as_f64().unwrap_or(0.0) as u64;
            match self.layers.iter_mut().find(|l| l.0 == digest) {
                Some(l) => {
                    l.1 = done.max(l.1).min(total);
                    l.2 = total;
                }
                None => self.layers.push((digest, done.min(total), total)),
            }
        }
        if status == "success" {
            self.success = true;
            for l in &mut self.layers {
                l.1 = l.2;
            }
        }
        if !status.is_empty() {
            self.status = status;
        }
        Ok(())
    }

    pub fn done(&self) -> u64 {
        self.layers.iter().map(|l| l.1).sum()
    }

    pub fn total(&self) -> u64 {
        self.layers.iter().map(|l| l.2).sum()
    }
}

/// Where an `ollama` program may be, in order: each folder of PATH, the per-user install of the Ollama app on
/// Windows, then the portable copy of Reelhouse (`runtime`, searched by `find_runtime`).
pub fn candidates(path_var: &str, local_app_data: &str, windows: bool) -> Vec<PathBuf> {
    let exe = if windows { "ollama.exe" } else { "ollama" };
    let sep = if windows { ';' } else { ':' };
    let mut out: Vec<PathBuf> = path_var.split(sep).map(|d| d.trim().trim_matches('"')).filter(|d| !d.is_empty()).map(|d| Path::new(d).join(exe)).collect();
    if windows && !local_app_data.trim().is_empty() {
        out.push(Path::new(local_app_data.trim()).join("Programs").join("Ollama").join(exe));
    }
    if !windows {
        out.push(PathBuf::from("/usr/local/bin/ollama"));
        out.push(PathBuf::from("/usr/bin/ollama"));
    }
    out
}

/// The first program that exists: installed first (`candidates`), then the portable copy under `runtime`.
pub fn find_program(cands: &[PathBuf], runtime: &Path, exists: &dyn Fn(&Path) -> bool) -> Option<(PathBuf, bool)> {
    if let Some(p) = cands.iter().find(|p| exists(p)) {
        return Some((p.clone(), false));
    }
    let exe = if cfg!(windows) { "ollama.exe" } else { "ollama" };
    let direct = runtime.join(exe);
    if exists(&direct) {
        return Some((direct, true));
    }
    crate::torrent::find_file(runtime, &[exe]).map(|p| (p, true))
}

/// Whether an Ollama server answers at `ROOT`.
pub fn alive(http: &dyn Http) -> bool {
    http.send(&Request::get(&format!("{}/api/version", ROOT)).timeout(3)).map(|r| r.ok()).unwrap_or(false)
}

/// Whether the server already has the model.
fn has_model(http: &dyn Http, model: &str) -> bool {
    let body = Json::obj(vec![("model", model.into())]).dump();
    http.send(&Request::post(&format!("{}/api/show", ROOT)).json_body(body).timeout(20)).map(|r| r.ok()).unwrap_or(false)
}

pub(crate) fn hidden(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Unpacks a zip (or a .tar.gz: tar finds the compression itself) with Windows' own tar (bsdtar reads zip), then
/// PowerShell; elsewhere tar, then unzip. Also used by `whisper_local`.
pub(crate) fn unpack(zip: &Path, out: &Path) -> Result<(), String> {
    let mut tars: Vec<PathBuf> = vec![];
    if cfg!(windows) {
        // not a tar of MSYS2 on PATH: GNU tar does not read zip files
        if let Ok(root) = std::env::var("SystemRoot") {
            tars.push(Path::new(&root).join("System32").join("tar.exe"));
        }
    }
    tars.push(PathBuf::from("tar"));
    for t in &tars {
        let mut c = Command::new(t);
        c.arg("-xf").arg(zip).arg("-C").arg(out).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        hidden(&mut c);
        if c.status().map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
    }
    let mut c = if cfg!(windows) {
        let q = |p: &Path| p.to_string_lossy().replace('\'', "''");
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", &format!("Expand-Archive -Force -LiteralPath '{}' -DestinationPath '{}'", q(zip), q(out))]);
        c
    } else {
        let mut c = Command::new("unzip");
        c.arg("-o").arg(zip).arg("-d").arg(out);
        c
    };
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    hidden(&mut c);
    if c.status().map(|s| s.success()).unwrap_or(false) {
        Ok(())
    } else {
        Err("Could not unpack the local AI download.".into())
    }
}

/// The last lines of the server's log, for an error.
pub(crate) fn log_tail(path: &Path) -> String {
    let t = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = t.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(3)..].join(" | ").chars().take(400).collect()
}

impl Local {
    pub fn status(&self) -> Status {
        self.inner.st.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Claims the setup for `model`: true when the caller must run `run` (in a thread). False when one already
    /// runs, or the model is ready and the server still answers… which is checked by `run` itself, so a ready
    /// state is trusted here only for the same model.
    pub fn begin(&self, model: &str) -> bool {
        let Ok(mut s) = self.inner.st.lock() else { return false };
        if s.busy || (s.phase == "ready" && s.model == model) {
            return false;
        }
        s.busy = true;
        s.phase = "checking".into();
        s.message = "Checking the local AI…".into();
        s.model = model.into();
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

    /// Blocks until the model is ready (Ok), the setup failed (Err with its message) or `timeout` passes.
    pub fn wait_ready(&self, model: &str, timeout: Duration) -> Result<(), String> {
        let end = Instant::now() + timeout;
        let Ok(mut s) = self.inner.st.lock() else { return Err("The local AI is not available.".into()) };
        loop {
            if s.phase == "ready" && s.model == model {
                return Ok(());
            }
            if !s.busy {
                return Err(if s.phase == "error" { s.message.clone() } else { "The local AI is not ready.".into() });
            }
            let now = Instant::now();
            if now >= end {
                return Err("The local AI took too long to get ready.".into());
            }
            s = match self.inner.cv.wait_timeout(s, end - now) {
                Ok((g, _)) => g,
                Err(_) => return Err("The local AI is not available.".into()),
            };
        }
    }

    /// The whole setup (call after `begin` returned true). `data_dir/ollama` holds the portable program, the models
    /// of a server started here and its log. `notify` is called when the status changes (now and then while bytes
    /// arrive).
    pub fn run(&self, http: &dyn Http, data_dir: &Path, notify: &dyn Fn()) {
        let model = self.status().model;
        let r = self.run_inner(http, data_dir, &model, notify);
        self.set(|s| {
            s.busy = false;
            match &r {
                Ok(()) => {
                    s.phase = "ready".into();
                    s.message = "The local AI is ready.".into();
                    s.done = s.total;
                }
                Err(e) => {
                    s.phase = "error".into();
                    s.message = e.clone();
                }
            }
        });
        if let Err(e) = &r {
            crate::debug::log("ollama", e);
        }
        notify();
    }

    fn run_inner(&self, http: &dyn Http, data_dir: &Path, model: &str, notify: &dyn Fn()) -> Result<(), String> {
        let home = data_dir.join("ollama");
        if !alive(http) {
            self.start_server(http, &home, notify)?;
        }
        if has_model(http, model) {
            return Ok(());
        }
        self.set(|s| {
            s.phase = "pulling_model".into();
            s.message = "Downloading the model…".into();
            s.done = 0;
            s.total = 0;
        });
        notify();
        let mut pull = Pull::default();
        let mut failed: Option<String> = None;
        let mut last = Instant::now();
        let body = Json::obj(vec![("model", model.into()), ("stream", Json::Bool(true))]).dump();
        let status = http.send_lines(&Request::post(&format!("{}/api/pull", ROOT)).json_body(body).timeout(6 * 3600), &mut |line| {
            if failed.is_some() {
                return;
            }
            if let Err(e) = pull.feed(line) {
                failed = Some(e);
                return;
            }
            if last.elapsed() >= Duration::from_millis(400) || pull.success {
                last = Instant::now();
                let (d, t) = (pull.done(), pull.total());
                self.set(|s| {
                    s.done = d;
                    s.total = t;
                });
                notify();
            }
        })?;
        if let Some(e) = failed {
            return Err(format!("Ollama could not download the model {}: {}", model, e));
        }
        if !(200..300).contains(&status) {
            return Err(format!("Ollama could not download the model {} (answer {}).", model, status));
        }
        if !pull.success && !has_model(http, model) {
            return Err(format!("The download of the model {} did not finish. Try again.", model));
        }
        Ok(())
    }

    /// Finds (or downloads) the program and starts `ollama serve`, then waits until it answers.
    fn start_server(&self, http: &dyn Http, home: &Path, notify: &dyn Fn()) -> Result<(), String> {
        crate::platform::needs_program("The local AI (Ollama) cannot run on phones and tablets. Use a cloud AI (Preferences).")?;
        let runtime = home.join("runtime");
        let cands = candidates(&std::env::var("PATH").unwrap_or_default(), &std::env::var("LOCALAPPDATA").unwrap_or_default(), cfg!(windows));
        let found = find_program(&cands, &runtime, &|p: &Path| p.is_file());
        let (exe, portable) = match found {
            Some(f) => f,
            None if cfg!(windows) => (self.download_runtime(http, home, &runtime, notify)?, true),
            None => return Err("Ollama is not installed on this computer. Install it from ollama.com (Linux: curl -fsSL https://ollama.com/install.sh | sh), then try again.".into()),
        };
        self.set(|s| {
            s.phase = "starting".into();
            s.message = "Starting the local AI…".into();
            s.done = 0;
            s.total = 0;
        });
        notify();
        std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
        let log_path = home.join("serve.log");
        let (out, err) = match std::fs::File::create(&log_path).ok().and_then(|f| f.try_clone().ok().map(|c| (f, c))) {
            Some((a, b)) => (Stdio::from(a), Stdio::from(b)),
            None => (Stdio::null(), Stdio::null()),
        };
        let mut cmd = Command::new(&exe);
        cmd.arg("serve").stdin(Stdio::null()).stdout(out).stderr(err);
        cmd.env("OLLAMA_HOST", "127.0.0.1:11434");
        // the portable copy keeps its models beside it; an installed Ollama keeps its own (models already pulled stay)
        if portable {
            let models = home.join("models");
            let _ = std::fs::create_dir_all(&models);
            cmd.env("OLLAMA_MODELS", &models);
        }
        hidden(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("Could not start the local AI ({}): {}", exe.display(), e))?;
        #[cfg(windows)]
        crate::torrent::win_job::adopt(&child);
        let started = Instant::now();
        loop {
            if alive(http) {
                break;
            }
            if let Ok(Some(st)) = child.try_wait() {
                return Err(format!("The local AI stopped at start ({}): {}", st, log_tail(&log_path)));
            }
            if started.elapsed() > Duration::from_secs(90) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("The local AI did not answer after starting: {}", log_tail(&log_path)));
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        if let Ok(mut c) = self.inner.child.lock() {
            if let Some(mut old) = c.replace(child) {
                kill_tree(&mut old);
            }
        }
        self.set(|s| s.ours = true);
        Ok(())
    }

    /// Windows only: the portable build downloaded and unpacked into `runtime`.
    fn download_runtime(&self, http: &dyn Http, home: &Path, runtime: &Path, notify: &dyn Fn()) -> Result<PathBuf, String> {
        std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
        self.set(|s| {
            s.phase = "downloading_runtime".into();
            s.message = "Downloading the local AI program…".into();
            s.done = 0;
            s.total = 0;
        });
        notify();
        let zip = home.join("ollama-windows-amd64.zip.part");
        let last = Mutex::new(Instant::now());
        let r = http.download(WINDOWS_ZIP, &zip, &|done, total| {
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
            let _ = std::fs::remove_file(&zip);
            return Err(format!("Could not download the local AI ({}). Check the internet connection and try again.", e));
        }
        self.set(|s| {
            s.message = "Unpacking the local AI program…".into();
        });
        notify();
        let tmp = home.join("runtime.new");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let r = unpack(&zip, &tmp);
        let _ = std::fs::remove_file(&zip);
        if let Err(e) = r {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
        let _ = std::fs::remove_dir_all(runtime);
        std::fs::rename(&tmp, runtime).map_err(|e| format!("Could not put the local AI in place: {}", e))?;
        crate::torrent::find_file(runtime, &["ollama.exe"]).ok_or_else(|| "The local AI download did not contain ollama.exe.".to_string())
    }

    /// Stops the server Reelhouse started (an Ollama started by someone else keeps running).
    pub fn stop(&self) {
        let child = self.inner.child.lock().ok().and_then(|mut c| c.take());
        if let Some(mut c) = child {
            kill_tree(&mut c);
            self.set(|s| {
                s.ours = false;
                if !s.busy {
                    s.phase = "idle".into();
                }
            });
        }
    }
}

/// `ollama serve` starts runners of its own: on Windows the whole tree goes.
pub(crate) fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut k = Command::new("taskkill");
        k.args(["/PID", &child.id().to_string(), "/F", "/T"]).stdout(Stdio::null()).stderr(Stdio::null());
        hidden(&mut k);
        let _ = k.status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn pull_progress_adds_the_layers() {
        let mut p = Pull::default();
        p.feed(r#"{"status":"pulling manifest"}"#).unwrap();
        assert_eq!((p.done(), p.total()), (0, 0));
        p.feed(r#"{"status":"pulling a1","digest":"sha256:a1","total":1000,"completed":250}"#).unwrap();
        p.feed(r#"{"status":"pulling b2","digest":"sha256:b2","total":100}"#).unwrap();
        assert_eq!((p.done(), p.total()), (250, 1100));
        p.feed(r#"{"status":"pulling a1","digest":"sha256:a1","total":1000,"completed":900}"#).unwrap();
        p.feed("").unwrap();
        assert_eq!(p.done(), 900);
        assert!(!p.success);
        p.feed(r#"{"status":"verifying sha256 digest"}"#).unwrap();
        p.feed(r#"{"status":"success"}"#).unwrap();
        assert!(p.success);
        assert_eq!(p.done(), p.total());
        assert_eq!(p.feed(r#"{"error":"pull model manifest: file does not exist"}"#).unwrap_err(), "pull model manifest: file does not exist");
        assert!(p.feed("<html>").is_err());
    }

    #[test]
    fn programs_are_looked_for_in_order() {
        let c = candidates(r#"C:\Windows;"C:\Tools";;"#, r"C:\Users\hp\AppData\Local", true);
        assert_eq!(c.len(), 3);
        assert!(c[0].to_string_lossy().ends_with("ollama.exe"));
        assert!(c[2].to_string_lossy().contains("Programs") && c[2].to_string_lossy().contains("Ollama"));
        let l = candidates("/usr/bin:/opt/x", "", false);
        assert_eq!(l[0], PathBuf::from("/usr/bin/ollama"));
        assert!(l.contains(&PathBuf::from("/usr/local/bin/ollama")));
        let runtime = Path::new("/data/ollama/runtime");
        let exe = if cfg!(windows) { "ollama.exe" } else { "ollama" };
        // installed wins over the portable copy
        let both = |p: &Path| p == Path::new("/opt/x/ollama") || p == runtime.join(exe);
        assert_eq!(find_program(&l, runtime, &both), Some((PathBuf::from("/opt/x/ollama"), false)));
        let portable = |p: &Path| p == runtime.join(exe);
        assert_eq!(find_program(&l, runtime, &portable), Some((runtime.join(exe), true)));
        assert_eq!(find_program(&l, runtime, &|_: &Path| false), None);
    }

    #[test]
    fn a_running_server_only_pulls_the_model() {
        let http = Fake::new()
            .route("GET http://127.0.0.1:11434/api/version", 200, r#"{"version":"0.12.0"}"#)
            .route("POST http://127.0.0.1:11434/api/show", 404, r#"{"error":"not found"}"#)
            .route("POST http://127.0.0.1:11434/api/show", 200, r#"{}"#)
            .route("POST http://127.0.0.1:11434/api/pull", 200, "{\"status\":\"pulling x\",\"digest\":\"d\",\"total\":10,\"completed\":10}\n{\"status\":\"success\"}\n");
        let l = Local::default();
        assert!(l.begin("qwen2.5:3b-instruct"));
        assert!(!l.begin("qwen2.5:3b-instruct"), "one setup at a time");
        l.run(&http, Path::new("/nonexistent"), &|| {});
        let s = l.status();
        assert_eq!(s.phase, "ready", "{:?}", s);
        assert!(!s.busy && !s.ours);
        assert_eq!(s.percent(), 100.0);
        assert!(l.wait_ready("qwen2.5:3b-instruct", Duration::from_secs(1)).is_ok());
        assert!(!l.begin("qwen2.5:3b-instruct"), "already ready");
        assert!(l.begin("llama3.2"), "another model is set up again");
    }

    #[test]
    fn a_failed_pull_is_an_error() {
        let http = Fake::new()
            .route("GET http://127.0.0.1:11434/api/version", 200, "{}")
            .route("POST http://127.0.0.1:11434/api/show", 404, "{}")
            .route("POST http://127.0.0.1:11434/api/pull", 200, "{\"error\":\"no space left on device\"}\n");
        let l = Local::default();
        assert!(l.begin("m"));
        l.run(&http, Path::new("/nonexistent"), &|| {});
        let s = l.status();
        assert_eq!(s.phase, "error");
        assert!(s.message.contains("no space left"));
        assert!(l.wait_ready("m", Duration::from_millis(10)).unwrap_err().contains("no space left"));
        assert!(l.begin("m"), "an error can be retried");
    }
}
