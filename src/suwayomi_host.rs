//! Runs Suwayomi-Server for the user: the bundle (it carries its own Java) is downloaded once into the data folder,
//! started hidden on a free local port when the Images mode needs it, and stopped when the app or the mode is done with it.
use crate::http::Http;
use crate::json::Json;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const RELEASE_API: &str = "https://api.github.com/repos/Suwayomi/Suwayomi-Server/releases/latest";

/// The file of the release that suits this computer.
pub fn pick_asset(release: &Json, os: &str, arch: &str) -> Option<(String, String)> {
    let want: &[&str] = match (os, arch) {
        ("windows", _) => &["windows-x64.zip"],
        ("macos", "aarch64") => &["macos-arm64.tar.gz"],
        ("macos", _) => &["macos-x64.tar.gz"],
        _ => &["linux-x64.tar.gz"],
    };
    release.get("assets").items().iter().find_map(|a| {
        let n = a.get("name").str();
        let l = n.to_lowercase();
        if want.iter().any(|w| l.ends_with(w)) {
            Some((n, a.get("browser_download_url").str()))
        } else {
            None
        }
    })
}

fn find_in(dir: &Path, f: &dyn Fn(&str) -> bool, depth: usize) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                if let Some(x) = find_in(&p, f, depth - 1) {
                    return Some(x);
                }
            }
        } else if p.file_name().and_then(|n| n.to_str()).map(f).unwrap_or(false) {
            return Some(p);
        }
    }
    None
}

/// The server's own jar ("Suwayomi-Server.jar", "Suwayomi-Server-v2.x.jar", ...), never the launcher's.
fn is_server_jar(name: &str) -> bool {
    let l = name.to_lowercase();
    l.starts_with("suwayomi") && l.ends_with(".jar") && !l.contains("launcher")
}

/// The jar of the server and the Java that runs it (the bundled one, else the one of the system).
pub fn locate(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    // The release also ships "Suwayomi-Launcher.jar", a Swing window that only starts the server. Folder order
    // is random on Linux, so a plain "first suwayomi*.jar" could pick it and that window opened over the app.
    let jar = find_in(dir, &|n| is_server_jar(n), 8)?;
    let java_names: &[&str] = if cfg!(windows) { &["java.exe"] } else { &["java"] };
    let bundled = find_in(dir, &|n| java_names.iter().any(|j| n.eq_ignore_ascii_case(j)), 8);
    Some((jar, bundled.unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "java.exe" } else { "java" }))))
}

/// Downloads and unpacks the server (once). Returns the folder that holds it.
pub fn install(http: &dyn Http, dir: &Path, status: &dyn Fn(String)) -> Result<PathBuf, String> {
    crate::platform::needs_program("The Suwayomi server needs Java, which cannot run on phones and tablets. Use a remote Suwayomi server (Preferences).")?;
    let bundle = dir.join("bundle");
    if locate(&bundle).is_some() {
        return Ok(bundle);
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    status("Looking up the Suwayomi server…".to_string());
    let rel = crate::http::get_json(http, RELEASE_API, 30).map_err(|e| format!("Could not look up the Suwayomi server: {}", e))?;
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
    let (name, url) = pick_asset(&rel, os, arch).ok_or_else(|| format!("No Suwayomi build was found for {} {}.", os, arch))?;
    let file = dir.join(&name);
    // A finished download is kept, so a failed unpacking does not mean downloading 330 MB again.
    if !file.is_file() {
        let part = dir.join(format!("{}.part", name));
        http.download(&url, &part, &|done, total| {
            let mb = done / 1_048_576;
            status(match total {
                Some(t) => format!("Downloading the Suwayomi server (first time only): {} of {} MB", mb, t / 1_048_576),
                None => format!("Downloading the Suwayomi server (first time only): {} MB", mb),
            });
        })
        .map_err(|e| format!("Downloading the Suwayomi server failed: {}", e))?;
        std::fs::rename(&part, &file).map_err(|e| e.to_string())?;
    }
    status("Unpacking the Suwayomi server… (a minute or two)".to_string());
    let tmp = dir.join("unpack");
    let zip = name.to_lowercase().ends_with(".zip");
    let mut tries: Vec<(String, Vec<String>)> = Vec::new();
    let (f, t) = (file.to_string_lossy().into_owned(), tmp.to_string_lossy().into_owned());
    #[cfg(windows)]
    {
        // The tar of Windows reads zip files; the GNU tar of MSYS2 takes "C:" for a host name and fails.
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        tries.push((format!("{}\\System32\\tar.exe", root), vec!["-xf".into(), f.clone(), "-C".into(), t.clone()]));
        if zip {
            tries.push((
                "powershell".to_string(),
                vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), format!("Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force", f.replace('\'', "''"), t.replace('\'', "''"))],
            ));
        }
    }
    tries.push(("tar".to_string(), vec!["-xf".into(), f.clone(), "-C".into(), t.clone()]));
    if zip {
        tries.push(("unzip".to_string(), vec!["-o".into(), f.clone(), "-d".into(), t.clone()]));
    }
    let mut last: Vec<String> = Vec::new();
    let mut done = false;
    for (prog, args) in tries {
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&prog);
        cmd.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        hide(&mut cmd);
        match cmd.output() {
            Ok(o) if o.status.success() && locate(&tmp).is_some() => {
                done = true;
                break;
            }
            Ok(o) if o.status.success() => {
                let top: Vec<String> = std::fs::read_dir(&tmp).map(|d| d.flatten().take(6).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
                last.push(format!("{}: unpacked, but no jar was found (inside: {})", prog, top.join(", ")));
            }
            Ok(o) => last.push(format!("{} {}: {}", prog, o.status, String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("no message"))),
            Err(e) => last.push(format!("{}: {}", prog, e)),
        }
    }
    if !done {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("Could not unpack the Suwayomi download ({}). The file is kept in {}.", last.join(" | "), dir.display()));
    }
    let _ = std::fs::remove_dir_all(&bundle);
    std::fs::rename(&tmp, &bundle).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&file);
    Ok(bundle)
}

pub(crate) fn hide(_cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
}

/// The server process. It is stopped when this is dropped.
pub struct Host {
    child: Child,
    pub port: u16,
    pid_file: PathBuf,
    /// Set when the server stopped by itself: its exit code (None when it has none, killed by the system).
    pub exited: Option<Option<i32>>,
}

pub fn log_path(dir: &Path) -> PathBuf {
    dir.join("suwayomi.log")
}

/// The last lines of the log, for error messages.
pub fn log_tail(dir: &Path) -> String {
    let text = std::fs::read_to_string(log_path(dir)).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines.iter().rev().take(3).rev().map(|l| l.chars().take(160).collect::<String>()).collect::<Vec<_>>().join(" | ")
}

/// The lines of the log that tell why the server stopped: errors and their causes (the last few), else the tail.
pub fn log_digest(text: &str) -> String {
    let wanted = |l: &str| l.contains("ERROR") || l.contains("Exception") || l.contains("Caused by") || l.contains("Failed") || l.contains("Shutting Down");
    let lines: Vec<String> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty() && wanted(l)).map(|l| l.chars().take(220).collect::<String>()).collect();
    let pick: Vec<String> = if lines.is_empty() {
        text.lines().filter(|l| !l.trim().is_empty()).map(|l| l.trim().chars().take(220).collect::<String>()).collect()
    } else {
        lines
    };
    let mut seen: Vec<String> = vec![];
    for l in pick.into_iter().rev() {
        if !seen.contains(&l) {
            seen.push(l);
        }
        if seen.len() >= 5 {
            break;
        }
    }
    seen.reverse();
    seen.join(" | ")
}

/// What an exit code of the server means (Suwayomi-Server's AppExit.kt), in words.
pub fn exit_meaning(code: Option<i32>) -> &'static str {
    match code {
        Some(1) | Some(2) => "Another Suwayomi server is already running with the same data folder.",
        Some(4) => "Its settings file could not be updated.",
        Some(5) | Some(9) => "Its database could not be updated.",
        Some(6) => "Its settings file could not be created.",
        Some(7) | Some(8) => "Its data folder could not be set up (the database or the local source folder could not be opened).",
        _ => "",
    }
}

/// The folders the server is given. All of them live under the data folder of the app (never in a folder of the
/// user that OneDrive or a permission may block), and they exist before the server starts: Suwayomi ignores a
/// local source folder that does not exist, and stops with "LocalSourceSetupFailure" when it cannot set it up.
#[derive(Clone, Debug, PartialEq)]
pub struct Dirs {
    pub root: PathBuf,
    pub local: PathBuf,
    pub downloads: PathBuf,
    pub backups: PathBuf,
    pub tmp: PathBuf,
}

impl Dirs {
    pub fn under(root: &Path) -> Dirs {
        Dirs { root: root.to_path_buf(), local: root.join("local"), downloads: root.join("downloads"), backups: root.join("backups"), tmp: root.join("tmp") }
    }

    /// Creates the folders and checks a file can be written in them.
    pub fn prepare(&self) -> Result<(), String> {
        for d in [&self.root, &self.local, &self.downloads, &self.backups, &self.tmp, &self.root.join("extensions").join("icon")] {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {}", d.display(), e))?;
        }
        for d in [&self.root, &self.local] {
            let probe = d.join(".reelhouse-write-test");
            std::fs::write(&probe, b"ok").map_err(|e| format!("{} cannot be written: {}", d.display(), e))?;
            let _ = std::fs::remove_file(&probe);
        }
        Ok(())
    }
}

/// Other places for the data when the usual one cannot be written: the local (not roaming) application data of
/// Windows, then the temporary folder.
fn fallback_roots() -> Vec<PathBuf> {
    let mut v = vec![];
    if cfg!(windows) {
        if let Ok(p) = std::env::var("LOCALAPPDATA") {
            v.push(PathBuf::from(p).join("Reelhouse").join("suwayomi-data"));
        }
    }
    v.push(std::env::temp_dir().join("reelhouse-suwayomi-data"));
    v
}

/// The data folder of the server: `<dir>/data`, or a fallback when that one cannot be set up.
pub fn choose_dirs(dir: &Path) -> Result<Dirs, String> {
    let first = Dirs::under(&dir.join("data"));
    let err = match first.prepare() {
        Ok(()) => return Ok(first),
        Err(e) => e,
    };
    crate::debug::log("suwayomi", &format!("data folder unusable ({}), trying another place", err));
    for root in fallback_roots() {
        let d = Dirs::under(&root);
        if d.prepare().is_ok() {
            return Ok(d);
        }
    }
    Err(format!("The Suwayomi server has no folder it can write to ({}).", err))
}

/// `target` written from `base` ("../bundle/x.jar"), when both are on the same drive.
pub fn relative_to(base: &Path, target: &Path) -> Option<PathBuf> {
    let b: Vec<_> = base.components().collect();
    let t: Vec<_> = target.components().collect();
    let common = b.iter().zip(t.iter()).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return None;
    }
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &t[common..] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    Some(out)
}

/// A mark put on the command line of our server, so a server left running by a crash of the app can be found and
/// stopped even when its pid file is gone (it would keep the database locked: exit code 8 at the next start).
pub fn owner_mark(root: &Path) -> String {
    format!("-Dreelhouse.owner={}", crate::util::fnv_hex(&root.to_string_lossy()))
}

/// Paths with letters outside ASCII are given relative to the data folder (see `java_args`).
pub fn needs_relative(jar: &Path, dirs: &Dirs) -> bool {
    !dirs.root.to_string_lossy().is_ascii() || !jar.to_string_lossy().is_ascii()
}

/// The options handed to Java (the settings travel as properties, so no file has to be written). The server runs
/// in its data folder; a path with letters outside ASCII is given relative to it, because the Java launcher of
/// Windows reads its arguments in the old code page and would garble it (the folder of a user named "João").
pub fn java_args(jar: &Path, dirs: &Dirs, port: u16, repos: &[String]) -> Vec<String> {
    let rel = needs_relative(jar, dirs);
    let show = |p: &Path| -> String {
        if rel {
            relative_to(&dirs.root, p).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|| p.to_string_lossy().into_owned())
        } else {
            p.to_string_lossy().into_owned()
        }
    };
    let p = |k: &str, v: &str| format!("-Dsuwayomi.tachidesk.config.server.{}={}", k, v);
    // IP protection (src/privacy.rs): the extensions' requests go through the local gateway (SOCKS5)
    let gw = crate::privacy::proxy_port();
    let socks: Vec<String> = if gw != 0 {
        vec![p("socksProxyEnabled", "true"), p("socksProxyVersion", "5"), p("socksProxyHost", "127.0.0.1"), p("socksProxyPort", &gw.to_string())]
    } else {
        vec![p("socksProxyEnabled", "false")]
    };
    let mut v = vec![
        "-Xmx768m".to_string(),
        // no window of its own, ever (tray icon, launcher, dialogs): the app is its only face
        "-Djava.awt.headless=true".to_string(),
        owner_mark(&dirs.root),
        format!("-Djava.io.tmpdir={}", show(&dirs.tmp)),
        p("rootDir", &show(&dirs.root)),
        p("localSourcePath", &show(&dirs.local)),
        p("downloadsPath", &show(&dirs.downloads)),
        p("backupPath", &show(&dirs.backups)),
        p("ip", "127.0.0.1"),
        p("port", &port.to_string()),
        p("initialOpenInBrowserEnabled", "false"),
        p("systemTrayEnabled", "false"),
        p("webUIEnabled", "false"),
        p("debugLogsEnabled", "false"),
        // The repositories are also given at start, in case the server does not take them later through its API.
        p("extensionRepos", &format!("[{}]", repos.iter().map(|r| format!("\"{}\"", r)).collect::<Vec<_>>().join(","))),
    ];
    v.extend(socks);
    v.push("-jar".to_string());
    v.push(show(jar));
    v
}

fn pid_path(dir: &Path) -> PathBuf {
    dir.join("suwayomi.pid")
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let mut kill = Command::new("taskkill");
    kill.arg("/PID").arg(pid.to_string()).args(["/F", "/T"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    hide(&mut kill);
    let _ = kill.status();
}

/// A server left behind by a crash would keep the database locked: it is stopped before a new one starts.
fn stop_stale(dir: &Path) {
    let Some(pid) = std::fs::read_to_string(pid_path(dir)).ok().and_then(|t| t.trim().parse::<u32>().ok()) else { return };
    let _ = std::fs::remove_file(pid_path(dir));
    #[cfg(windows)]
    {
        let mut list = Command::new("tasklist");
        list.arg("/FI").arg(format!("PID eq {}", pid)).args(["/FO", "CSV", "/NH"]).stdin(Stdio::null());
        hide(&mut list);
        let is_java = list.output().map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("java")).unwrap_or(false);
        if is_java {
            kill_pid(pid);
        }
    }
    #[cfg(unix)]
    {
        let is_java = std::fs::read_to_string(format!("/proc/{}/comm", pid)).map(|c| c.to_lowercase().contains("java")).unwrap_or(false);
        if is_java {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
    }
}

/// Stops every Java process that carries our mark (see `owner_mark`), or, for a server started by an older version
/// of the app, our data folder as its rootDir. Returns how many were found.
fn stop_marked(root: &Path) -> usize {
    let marks = [owner_mark(root), format!("-Dsuwayomi.tachidesk.config.server.rootDir={}", root.to_string_lossy())];
    let mut found = 0;
    #[cfg(windows)]
    {
        let test = marks.iter().map(|m| format!("$_.CommandLine.Contains('{}')", m.replace('\'', "''"))).collect::<Vec<_>>().join(" -or ");
        // no double quotes in the script: they would have to survive the quoting of the command line
        let script = format!("Get-CimInstance Win32_Process | Where-Object {{ ($_.Name -eq 'java.exe' -or $_.Name -eq 'javaw.exe') -and $_.CommandLine -and ({}) }} | ForEach-Object {{ $_.ProcessId }}", test);
        let mut ps = Command::new("powershell");
        ps.args(["-NoProfile", "-NonInteractive", "-Command", &script]).stdin(Stdio::null()).stderr(Stdio::null());
        hide(&mut ps);
        if let Ok(o) = ps.output() {
            for pid in String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| l.trim().parse::<u32>().ok()) {
                kill_pid(pid);
                found += 1;
            }
        }
    }
    #[cfg(unix)]
    {
        if let Ok(rd) = std::fs::read_dir("/proc") {
            for e in rd.flatten() {
                let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
                let cmd = std::fs::read(e.path().join("cmdline")).unwrap_or_default();
                if pid != std::process::id() && String::from_utf8_lossy(&cmd).split('\0').any(|a| marks.iter().any(|m| m == a)) {
                    let _ = Command::new("kill").arg(pid.to_string()).status();
                    found += 1;
                }
            }
        }
    }
    if found > 0 {
        // the files are let go when the process is really gone
        std::thread::sleep(Duration::from_millis(1500));
    }
    found
}

/// The log says the database is held by another process (a server still running).
pub fn log_says_locked(log: &str) -> bool {
    let l = log.to_lowercase();
    l.contains("already in use") || l.contains("locked by another") || l.contains("being used by another process")
}

/// The log says the database file itself is broken (not merely locked or unreachable).
pub fn log_says_corrupt(log: &str) -> bool {
    let l = log.to_lowercase();
    l.contains("file corrupted") || l.contains("corrupt") || l.contains("unsupported database file version") || l.contains("mvstoreexception") || l.contains("checksum mismatch")
}

/// Before the second try: the settings file of the server is put aside (it is made again from its defaults, and
/// everything the app needs is given on the command line). The database is put aside only when the log shows the
/// file itself is broken: the ids of the library's series ("suwayomi:123") are the rows of that database, so a new
/// one would point them at nothing. Nothing is deleted: the files get a ".bad-<time>" name.
/// Returns the names of what was put aside.
pub fn set_aside(root: &Path, code: Option<i32>, log: &str, stamp: u64) -> Vec<String> {
    let mut names = vec!["server.conf".to_string()];
    if matches!(code, Some(5) | Some(7) | Some(8) | Some(9)) && !log_says_locked(log) && log_says_corrupt(log) {
        names.extend(["database.mv.db", "database.trace.db", "database.lock.db"].iter().map(|s| s.to_string()));
    }
    names
        .into_iter()
        .filter(|n| {
            let f = root.join(n);
            f.exists() && std::fs::rename(&f, root.join(format!("{}.bad-{}", n, stamp))).is_ok()
        })
        .collect()
}

fn free_port() -> Option<u16> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).ok()?;
    l.local_addr().ok().map(|a| a.port())
}

/// Starts the server and waits for it. When it stops right after starting (a server left running by a crash, a
/// broken settings file or database), what holds it is cleared and it is started once more; if that fails too the
/// error tells why, with the lines of its log that matter.
pub fn launch(http: &dyn Http, dir: &Path, bundle: &Path, port: u16, repos: &[String], status: &dyn Fn(String)) -> Result<Host, String> {
    let dirs = choose_dirs(dir)?;
    stop_stale(dir);
    stop_marked(&dirs.root);
    let mut h = Host::start(dir, bundle, &dirs, port, repos)?;
    let first = match h.wait_ready(http, dir, 240) {
        Ok(()) => return Ok(h),
        Err(e) => e,
    };
    let Some(code) = h.exited else { return Err(first) };
    drop(h);
    let log = std::fs::read_to_string(log_path(dir)).unwrap_or_default();
    crate::debug::log("suwayomi", &format!("stopped at start (exit {:?}): {}", code, log_digest(&log)));
    status("The Suwayomi server stopped at start; trying again with a fresh setup…".to_string());
    stop_marked(&dirs.root);
    let moved = set_aside(&dirs.root, code, &log, crate::util::now_unix());
    if !moved.is_empty() {
        crate::debug::log("suwayomi", &format!("put aside: {}", moved.join(", ")));
    }
    let port = free_port().unwrap_or(port);
    let mut h = Host::start(dir, bundle, &dirs, port, repos)?;
    match h.wait_ready(http, dir, 240) {
        Ok(()) => Ok(h),
        Err(_) if h.exited.is_some() => {
            let code = h.exited.flatten();
            let log = std::fs::read_to_string(log_path(dir)).unwrap_or_default();
            let code_text = code.map(|c| c.to_string()).unwrap_or_else(|| "none".into());
            Err(format!("The Suwayomi server stopped right after starting, twice (exit code {}). {} Details: {} — Log: {}", code_text, exit_meaning(code), log_digest(&log), log_path(dir).display()).replace("  ", " "))
        }
        Err(e) => Err(e),
    }
}

impl Host {
    pub fn start(dir: &Path, bundle: &Path, dirs: &Dirs, port: u16, repos: &[String]) -> Result<Host, String> {
        crate::platform::needs_program("The Suwayomi server needs Java, which cannot run on phones and tablets. Use a remote Suwayomi server (Preferences).")?;
        let (jar, java) = locate(bundle).ok_or_else(|| "The Suwayomi files are missing.".to_string())?;
        dirs.prepare()?;
        let log = std::fs::File::create(log_path(dir)).ok();
        let (out, err) = match log.and_then(|f| f.try_clone().ok().map(|c| (f, c))) {
            Some((a, b)) => (Stdio::from(a), Stdio::from(b)),
            None => (Stdio::null(), Stdio::null()),
        };
        let mut cmd = Command::new(&java);
        // with paths relative to the data folder it runs there; otherwise beside its jar, as its own launcher does
        let cwd = if needs_relative(&jar, dirs) { dirs.root.clone() } else { jar.parent().unwrap_or(bundle).to_path_buf() };
        cmd.args(java_args(&jar, dirs, port, repos)).current_dir(&cwd).stdin(Stdio::null()).stdout(out).stderr(err);
        // Java without a UTF-8 locale garbles file names outside ASCII (tested: "João" became "Jo??o")
        #[cfg(unix)]
        {
            let utf8 = ["LC_ALL", "LC_CTYPE", "LANG"].iter().any(|v| std::env::var(v).map(|x| x.to_uppercase().contains("UTF-8") || x.to_uppercase().contains("UTF8")).unwrap_or(false));
            if !utf8 {
                cmd.env("LC_ALL", "C.UTF-8");
            }
        }
        hide(&mut cmd);
        let child = cmd.spawn().map_err(|e| format!("Could not start the Suwayomi server ({}): {}. Java is needed.", java.display(), e))?;
        let _ = std::fs::write(pid_path(dir), child.id().to_string());
        Ok(Host { child, port, pid_file: pid_path(dir), exited: None })
    }

    pub fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Waits until the server answers (the first start can take a while: it sets itself up).
    pub fn wait_ready(&mut self, http: &dyn Http, dir: &Path, secs: u64) -> Result<(), String> {
        let started = Instant::now();
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                self.exited = Some(status.code());
                return Err(format!("The Suwayomi server stopped right after starting ({}). {}", status, log_tail(dir)));
            }
            if let Ok(r) = http.send(&crate::http::Request::get(&format!("{}/api/v1/settings/about", self.base())).timeout(2)) {
                if r.status < 500 {
                    return Ok(());
                }
            }
            if started.elapsed().as_secs() >= secs {
                return Err(format!("The Suwayomi server did not start in time. {}", log_tail(dir)));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.pid_file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_bundle_for_the_system() {
        let rel = Json::parse(
            r#"{"assets":[
            {"name":"Suwayomi-Server-v2.4.1-windows-x64.msi","browser_download_url":"https://x/a.msi"},
            {"name":"Suwayomi-Server-v2.4.1-windows-x64.zip","browser_download_url":"https://x/a.zip"},
            {"name":"Suwayomi-Server-v2.4.1-linux-x64.tar.gz","browser_download_url":"https://x/l.tgz"},
            {"name":"Suwayomi-Server-v2.4.1-macOS-arm64.tar.gz","browser_download_url":"https://x/m.tgz"},
            {"name":"Suwayomi-Server-v2.4.1.jar","browser_download_url":"https://x/j.jar"}
        ]}"#,
        )
        .unwrap();
        assert_eq!(pick_asset(&rel, "windows", "x86_64").unwrap().1, "https://x/a.zip");
        assert_eq!(pick_asset(&rel, "linux", "x86_64").unwrap().1, "https://x/l.tgz");
        assert_eq!(pick_asset(&rel, "macos", "aarch64").unwrap().1, "https://x/m.tgz");
        assert!(pick_asset(&rel, "macos", "x86_64").is_none());
    }

    #[test]
    fn the_launcher_jar_is_never_the_server() {
        assert!(is_server_jar("Suwayomi-Server.jar"));
        assert!(is_server_jar("Suwayomi-Server-v2.1.1867.jar"));
        assert!(!is_server_jar("Suwayomi-Launcher.jar"));
        assert!(!is_server_jar("suwayomi-launcher.JAR"));
    }

    #[test]
    fn finds_the_jar_and_the_bundled_java() {
        let d = std::env::temp_dir().join(format!("rh-sw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("Suwayomi/bin")).unwrap();
        std::fs::create_dir_all(d.join("Suwayomi/jre/bin")).unwrap();
        std::fs::write(d.join("Suwayomi/bin/Suwayomi-Server.jar"), b"x").unwrap();
        // the launcher sits beside bin/ in the release and must never be the one started
        std::fs::write(d.join("Suwayomi/Suwayomi-Launcher.jar"), b"x").unwrap();
        let java = if cfg!(windows) { "java.exe" } else { "java" };
        std::fs::write(d.join("Suwayomi/jre/bin").join(java), b"x").unwrap();
        let (jar, j) = locate(&d).unwrap();
        assert!(jar.ends_with("Suwayomi-Server.jar"));
        assert!(j.ends_with(java) && j.to_string_lossy().contains("jre"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_settings_travel_as_properties() {
        let dirs = Dirs::under(Path::new("/d"));
        // folders are joined with the platform's separator (backslash on Windows)
        let d = |n: &str| Path::new("/d").join(n).to_string_lossy().into_owned();
        let a = java_args(Path::new("/s/Suwayomi-Server.jar"), &dirs, 4999, &["https://github.com/a/b/tree/repo".to_string()]);
        assert!(a.contains(&"-Dsuwayomi.tachidesk.config.server.port=4999".to_string()));
        assert!(a.contains(&"-Dsuwayomi.tachidesk.config.server.rootDir=/d".to_string()));
        assert!(a.contains(&"-Dsuwayomi.tachidesk.config.server.webUIEnabled=false".to_string()));
        assert!(a.contains(&"-Djava.awt.headless=true".to_string()));
        // the local source, downloads and backups are named, inside the data folder
        assert!(a.contains(&format!("-Dsuwayomi.tachidesk.config.server.localSourcePath={}", d("local"))));
        assert!(a.contains(&format!("-Dsuwayomi.tachidesk.config.server.downloadsPath={}", d("downloads"))));
        assert!(a.contains(&format!("-Dsuwayomi.tachidesk.config.server.backupPath={}", d("backups"))));
        assert!(a.contains(&format!("-Djava.io.tmpdir={}", d("tmp"))));
        assert!(a.contains(&owner_mark(Path::new("/d"))));
        assert!(a.iter().any(|x| x.ends_with("extensionRepos=[\"https://github.com/a/b/tree/repo\"]")));
        assert_eq!(a[a.len() - 2], "-jar");
        assert_eq!(a[a.len() - 1], "/s/Suwayomi-Server.jar");
        // every -D comes before -jar (Java reads what follows the jar as the program's own arguments)
        let jar_at = a.iter().position(|x| x == "-jar").unwrap();
        assert!(a.iter().skip(jar_at).all(|x| !x.starts_with("-D")));
    }

    #[test]
    fn paths_outside_ascii_are_given_relative_to_the_data_folder() {
        let dirs = Dirs::under(Path::new("/home/João/rh/suwayomi/data"));
        let a = java_args(Path::new("/home/João/rh/suwayomi/bundle/bin/Suwayomi-Server.jar"), &dirs, 4000, &[]);
        assert!(a.contains(&"-Dsuwayomi.tachidesk.config.server.rootDir=.".to_string()));
        assert!(a.contains(&"-Dsuwayomi.tachidesk.config.server.localSourcePath=local".to_string()));
        assert!(a.contains(&"-Djava.io.tmpdir=tmp".to_string()));
        assert_eq!(a[a.len() - 1], "../bundle/bin/Suwayomi-Server.jar");
        assert!(a.iter().all(|x| x.is_ascii()));
        assert_eq!(relative_to(Path::new("/a/b"), Path::new("/a/b")).unwrap(), PathBuf::from("."));
        assert_eq!(relative_to(Path::new("/a/b"), Path::new("/a/c/d")).unwrap(), PathBuf::from("../c/d"));
    }

    #[test]
    fn the_folders_exist_before_the_server_starts() {
        let d = std::env::temp_dir().join(format!("rh-sw-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let dirs = choose_dirs(&d).unwrap();
        assert_eq!(dirs, Dirs::under(&d.join("data")));
        for p in [&dirs.root, &dirs.local, &dirs.downloads, &dirs.backups, &dirs.tmp] {
            assert!(p.is_dir(), "{}", p.display());
        }
        assert!(dirs.root.join("extensions/icon").is_dir());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failed_start_puts_the_config_and_database_aside() {
        let d = std::env::temp_dir().join(format!("rh-sw-aside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for f in ["server.conf", "database.mv.db"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        // only locked: the database stays, the settings file goes
        assert_eq!(set_aside(&d, Some(8), "Database may be already in use", 7), vec!["server.conf".to_string()]);
        assert!(d.join("database.mv.db").exists() && d.join("server.conf.bad-7").exists());
        // a plain failure: only the settings file goes (the library's ids live in the database)
        std::fs::write(d.join("server.conf"), b"x").unwrap();
        assert_eq!(set_aside(&d, Some(8), "Failed to setup LocalSource", 8), vec!["server.conf".to_string()]);
        assert!(d.join("database.mv.db").exists());
        // a broken database file: it is put aside too
        assert_eq!(set_aside(&d, Some(8), "Failed to setup LocalSource\nMVStoreException: File corrupted in chunk 12", 10), vec!["database.mv.db".to_string()]);
        assert!(!d.join("database.mv.db").exists() && d.join("database.mv.db.bad-10").exists());
        // another reason: the database is left alone
        std::fs::write(d.join("database.mv.db"), b"x").unwrap();
        assert!(set_aside(&d, Some(3), "", 9).is_empty());
        assert!(d.join("database.mv.db").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_log_says_why() {
        let log = "12:00 INFO Running Suwayomi-Server v2\n12:01 ERROR Failed to setup LocalSource\norg.h2.jdbc.JdbcSQLNonTransientConnectionException: Database may be already in use\nCaused by: java.io.IOException: locked\n12:02 INFO Shutting Down Suwayomi-Server. Goodbye! (reason= 8 (LocalSourceSetupFailure))\n";
        let d = log_digest(log);
        assert!(d.starts_with("12:01 ERROR Failed to setup LocalSource"));
        assert!(d.contains("Caused by") && d.contains("LocalSourceSetupFailure"));
        assert!(!d.contains("Running"));
        assert!(log_says_locked(log));
        assert_eq!(log_digest("a\nb"), "a | b");
        assert!(exit_meaning(Some(8)).contains("data folder"));
        assert_eq!(exit_meaning(Some(0)), "");
    }
}
