//! Updates of the app from the GitHub Releases of its repository.
//!
//! `check` reads the releases (drafts and prereleases left out) and returns the newest one when it is newer than
//! the running version. `download` saves the file of this platform in pieces (Range), so the answer size limit of
//! the HTTP layer never cuts the ~335 MB Windows zip. How it is installed depends on the platform:
//! - Windows (the portable folder): the zip is unpacked, a PowerShell helper (`windows_script`) waits for the app
//!   to close, keeps a copy of the old files, copies the new ones over the program folder (never over the data
//!   folder), puts the old files back if the copy fails, and opens the app again.
//! - Linux (.rpm / .deb): the changed files of the release's .zip are copied over /opt/reelhouse (pkexec asks for
//!   the password once); without that zip, or without pkexec, the whole package is installed with `pkexec dnf` /
//!   `pkexec apt-get`, and as a last resort it is opened with the system installer.
//! - Android: the APK is put together (update_delta.rs) or downloaded and handed to the system installer.
//! - macOS and iOS: the release page opens.
//!
//! Only what changed is downloaded when the release has a zip of the program (update_delta.rs); otherwise the
//! whole file. With "Update by itself" (Preferences > General) the startup check also downloads the update and
//! installs it when the app closes (Windows, Linux) or hands it to the installer right away (Android).
//!
//! The repository is public: requests are anonymous. The GitHub token of the online backup is tried only when an
//! anonymous request is refused (GitHub's hourly limit for anonymous requests).
//! User data never lives in the program folder (`util::data_dir`: %APPDATA%\Reelhouse, ~/.config/reelhouse, the
//! app container on phones), so replacing the program keeps everything.
use crate::http::{Http, Request, Response};
use crate::json::Json;
use std::path::{Path, PathBuf};

pub const REPO: &str = "Githyot/Reel-House";
pub const API: &str = "https://api.github.com";
/// The page of the releases, for platforms that cannot install by themselves.
pub const RELEASES_PAGE: &str = "https://github.com/Githyot/Reel-House/releases";

/// The version of this build: the release tag it was built for (REELHOUSE_RELEASE, set by the workflows), else the
/// version of Cargo.toml.
pub fn current_version() -> String {
    let tag = option_env!("REELHOUSE_RELEASE").unwrap_or("").trim();
    if tag.starts_with('v') && parse(tag).is_some() {
        return tag.trim_start_matches('v').to_string();
    }
    env!("CARGO_PKG_VERSION").to_string()
}

/// (major, minor, patch, prerelease) of "v1.2.3", "1.2", "1.2.3-beta.1+build"; None when it is not a version.
pub fn parse(v: &str) -> Option<(u64, u64, u64, String)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let v = v.split('+').next().unwrap_or("");
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, p.to_string()),
        None => (v, String::new()),
    };
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().map(|x| x.parse().ok()).unwrap_or(Some(0))?;
    let patch = it.next().map(|x| x.parse().ok()).unwrap_or(Some(0))?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch, pre))
}

/// Semver order of two prerelease parts ("" is a final release, which comes after any prerelease).
fn pre_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Equal,
        (true, false) => return Greater,
        (false, true) => return Less,
        _ => {}
    }
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Equal,
            (None, Some(_)) => return Less,
            (Some(_), None) => return Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Less,
                    (Err(_), Ok(_)) => Greater,
                    _ => p.cmp(q),
                };
                if o != Equal {
                    return o;
                }
            }
        }
    }
}

/// Compares two versions (a leading "v" ignored); None when one of them is not a version.
pub fn compare(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let (x, y) = (parse(a)?, parse(b)?);
    Some((x.0, x.1, x.2).cmp(&(y.0, y.1, y.2)).then_with(|| pre_cmp(&x.3, &y.3)))
}

/// True when `candidate` is a newer version than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    compare(candidate, current) == Some(std::cmp::Ordering::Greater)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub size: u64,
    /// "sha256:<hex>" when GitHub published one.
    pub digest: String,
    /// The public download address (github.com/…/releases/download/…), which does not count against GitHub's
    /// hourly limit of API requests.
    pub url: String,
}

impl Asset {
    /// The SHA-256 GitHub published for the file, lowercase hex ("" when there is none).
    pub fn sha256(&self) -> String {
        self.digest.strip_prefix("sha256:").unwrap_or("").to_lowercase()
    }
}

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub name: String,
    pub notes: String,
    pub page: String,
    pub assets: Vec<Asset>,
}

/// The platforms Reelhouse is released for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Platform {
    Windows,
    MacOs,
    /// `true`: an rpm system (Fedora, openSUSE…), else a deb one.
    Linux(bool),
    Android,
    Ios,
    Other,
}

impl Platform {
    pub fn this() -> Platform {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "android") {
            Platform::Android
        } else if cfg!(target_os = "ios") {
            Platform::Ios
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "linux") {
            let rpm = !Path::new("/usr/bin/dpkg").exists() && (Path::new("/usr/bin/rpm").exists() || Path::new("/bin/rpm").exists());
            Platform::Linux(rpm)
        } else {
            Platform::Other
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::MacOs => "macos",
            Platform::Linux(_) => "linux",
            Platform::Android => "android",
            Platform::Ios => "ios",
            Platform::Other => "other",
        }
    }
    /// The file of the release for this platform (the names the workflows give them).
    pub fn asset_name(self) -> Option<&'static str> {
        match self {
            Platform::Windows => Some("ReelHouse-windows.zip"),
            Platform::MacOs => Some("ReelHouse-macos.dmg"),
            Platform::Linux(false) => Some("ReelHouse-linux-x86_64.deb"),
            Platform::Linux(true) => Some("ReelHouse-linux-x86_64.rpm"),
            Platform::Android if cfg!(target_arch = "x86_64") => Some("ReelHouse-android-x86_64.apk"),
            Platform::Android => Some("ReelHouse-android-arm64-v8a.apk"),
            Platform::Ios => Some("ReelHouse-ios-unsigned.ipa"),
            Platform::Other => None,
        }
    }
    /// Whether the app downloads the file itself (else the release page opens).
    pub fn downloads(self) -> bool {
        matches!(self, Platform::Windows | Platform::MacOs | Platform::Linux(_) | Platform::Android)
    }
    /// Whether the update can install without the user doing anything but agreeing (the startup update).
    pub fn installs_alone(self) -> bool {
        matches!(self, Platform::Windows | Platform::Linux(_) | Platform::Android)
    }
    /// The zip of the program folder, from which only the changed files are taken (Windows: the release file itself).
    pub fn folder_zip(self) -> Option<&'static str> {
        match self {
            Platform::Windows => Some("ReelHouse-windows.zip"),
            Platform::Linux(_) => Some("ReelHouse-linux-x86_64.zip"),
            _ => None,
        }
    }
    /// The program inside its folder, as a zip path.
    pub fn exe_rel(self) -> &'static str {
        match self {
            Platform::Windows => "reelhouse-qt.exe",
            _ => "bin/reelhouse-qt",
        }
    }
}

impl Release {
    pub fn asset_for(&self, p: Platform) -> Option<&Asset> {
        let name = p.asset_name()?;
        self.asset_named(name)
    }
    pub fn asset_named(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name.eq_ignore_ascii_case(name))
    }
}

fn release_of(j: &Json) -> Option<Release> {
    if j.get("draft").as_bool() == Some(true) || j.get("prerelease").as_bool() == Some(true) {
        return None;
    }
    let tag = j.get("tag_name").str();
    let (a, b, c, pre) = parse(&tag)?;
    if !pre.is_empty() {
        return None;
    }
    let assets = j
        .get("assets")
        .items()
        .iter()
        .map(|a| Asset { id: a.get("id").as_u64().unwrap_or(0), name: a.get("name").str(), size: a.get("size").as_u64().unwrap_or(0), digest: a.get("digest").str(), url: a.get("browser_download_url").str() })
        .filter(|a| a.id > 0)
        .collect();
    Some(Release {
        version: format!("{}.{}.{}", a, b, c),
        name: j.get("name").str(),
        notes: j.get("body").str(),
        page: j.get("html_url").str(),
        tag,
        assets,
    })
}

/// The newest final release of a `/releases` answer.
pub fn newest(list: &Json) -> Option<Release> {
    let mut best: Option<Release> = None;
    for r in list.items().iter().filter_map(release_of) {
        if best.as_ref().map(|b| is_newer(&r.version, &b.version)).unwrap_or(true) {
            best = Some(r);
        }
    }
    best
}

fn api_req(url: &str, token: &str) -> Request {
    let mut q = Request::get(url).header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28").header("User-Agent", "Reelhouse").timeout(30);
    if !token.trim().is_empty() {
        q = q.header("Authorization", &format!("Bearer {}", token.trim()));
    }
    q
}

/// What a check found.
#[derive(Clone, Debug)]
pub struct Found {
    /// The newest release (None when the repository has none yet).
    pub latest: Option<Release>,
    /// True when the token was used (anonymous requests failed or no token is set → false).
    pub with_token: bool,
}

impl Found {
    /// The newest release when it is newer than `current`.
    pub fn update(&self, current: &str) -> Option<&Release> {
        self.latest.as_ref().filter(|r| is_newer(&r.version, current))
    }
}

pub const NOT_FOUND: &str = "GitHub did not find the releases of Githyot/Reel-House. Try again later.";

fn failure(r: &Response) -> String {
    let msg = Json::parse(&r.text()).map(|j| j.get("message").str()).unwrap_or_default();
    match r.status {
        403 | 429 if msg.to_lowercase().contains("rate limit") => "GitHub limits how often it can be asked without a token. Try again in an hour, or connect a GitHub token.".into(),
        _ if msg.is_empty() => format!("GitHub answered {}.", r.status),
        _ => format!("GitHub answered {}: {}", r.status, msg),
    }
}

/// Reads the releases of the repository: anonymously (it is public), then with the token of the online backup
/// when GitHub refused the anonymous request (its hourly limit).
pub fn check(http: &dyn Http, token: &str) -> Result<Found, String> {
    let url = format!("{}/repos/{}/releases?per_page=30", API, REPO);
    let r = http.send(&api_req(&url, ""))?;
    if r.ok() {
        let j = r.json().map_err(|e| format!("GitHub did not return valid JSON ({})", e))?;
        return Ok(Found { latest: newest(&j), with_token: false });
    }
    let token = token.trim();
    if !token.is_empty() && matches!(r.status, 403 | 429) {
        let t = http.send(&api_req(&url, token))?;
        if t.ok() {
            let j = t.json().map_err(|e| format!("GitHub did not return valid JSON ({})", e))?;
            return Ok(Found { latest: newest(&j), with_token: true });
        }
    }
    Err(if r.status == 404 { NOT_FOUND.into() } else { failure(&r) })
}

/// A request for a release file: its public address when known (anonymous), else the API with octet-stream.
fn file_req(asset: &Asset, token: &str) -> Request {
    if !asset.url.is_empty() && asset.url.starts_with("https://github.com/") {
        return Request::get(&asset.url).header("User-Agent", "Reelhouse").header("Accept", "application/octet-stream");
    }
    let url = format!("{}/repos/{}/releases/assets/{}", API, REPO, asset.id);
    let mut q = api_req(&url, token);
    q.headers.retain(|(k, _)| k != "Accept");
    q.header("Accept", "application/octet-stream")
}

/// Reads `len` bytes of a release file from `from` (one Range request).
pub fn fetch_range(http: &dyn Http, token: &str, asset: &Asset, from: u64, len: u64) -> Result<Vec<u8>, String> {
    if len == 0 {
        return Ok(vec![]);
    }
    let q = file_req(asset, token).header("Range", &format!("bytes={}-{}", from, from + len - 1)).timeout(600);
    let r = http.send(&q)?;
    if !r.ok() {
        return Err(failure(&r));
    }
    if r.status == 200 {
        // the server ignored the Range: take the part asked for
        let a = (from as usize).min(r.body.len());
        let b = ((from + len) as usize).min(r.body.len());
        return Ok(r.body[a..b].to_vec());
    }
    Ok(r.body)
}

/// Checks a downloaded file against the SHA-256 GitHub published (nothing to check when there is none).
pub fn verify(asset: &Asset, file: &Path) -> Result<(), String> {
    let want = asset.sha256();
    if want.is_empty() {
        return Ok(());
    }
    let got = crate::update_delta::sha256_file(file)?;
    if got != want {
        let _ = std::fs::remove_file(file);
        return Err(format!("The downloaded {} is damaged (SHA-256 does not match). Try again.", asset.name));
    }
    Ok(())
}

/// Downloads a file of a release into `dest`, in pieces so no answer passes the size limit of the HTTP layer.
/// `progress(done, total)` is called after each piece; returning false from `go_on` stops it.
pub fn download(http: &dyn Http, token: &str, asset: &Asset, dest: &Path, progress: &dyn Fn(u64, u64), go_on: &dyn Fn() -> bool) -> Result<(), String> {
    download_in(http, token, asset, dest, progress, go_on, 16 * 1024 * 1024)
}

fn download_in(http: &dyn Http, token: &str, asset: &Asset, dest: &Path, progress: &dyn Fn(u64, u64), go_on: &dyn Fn() -> bool, piece: u64) -> Result<(), String> {
    use std::io::Write;
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("Could not make {}: {}", d.display(), e))?;
    }
    let mut out = std::fs::File::create(dest).map_err(|e| format!("Could not write {}: {}", dest.display(), e))?;
    let mut from = 0u64;
    loop {
        if !go_on() {
            return Err("The download was cancelled.".into());
        }
        let q = file_req(asset, token).header("Range", &format!("bytes={}-{}", from, from + piece - 1)).timeout(600);
        let r = http.send(&q)?;
        if !r.ok() {
            if r.status == 416 && from > 0 {
                break;
            }
            return Err(failure(&r));
        }
        out.write_all(&r.body).map_err(|e| e.to_string())?;
        from += r.body.len() as u64;
        progress(from, asset.size);
        if r.status == 200 || r.body.is_empty() || (r.body.len() as u64) < piece || (asset.size > 0 && from >= asset.size) {
            break;
        }
    }
    if asset.size > 0 && from != asset.size {
        return Err(format!("The download ended with {} of {} bytes. Try again.", from, asset.size));
    }
    Ok(())
}

/// The folder for the files of an update (download, unpacked copy, the old files, the helper).
pub fn work_dir() -> PathBuf {
    std::env::temp_dir().join("ReelHouse-update")
}

/// The folder that holds `exe_name` inside an unpacked zip: the root, or one level down ("ReelHouse/").
pub fn find_app_dir(unpacked: &Path, exe_name: &str) -> Option<PathBuf> {
    if unpacked.join(exe_name).is_file() {
        return Some(unpacked.to_path_buf());
    }
    std::fs::read_dir(unpacked).ok()?.flatten().map(|e| e.path()).find(|p| p.is_dir() && p.join(exe_name).is_file())
}

fn ps_quote(p: &Path) -> String {
    format!("'{}'", p.display().to_string().replace('\'', "''"))
}

/// The PowerShell helper of a Windows update. It waits for the app (`pid`) to close, copies the program folder to
/// `backup` (leaving out `keep`, folders that must not be touched such as a data folder inside the program
/// folder), copies `new_dir` over `install` (again leaving out `keep`), and on a failed copy puts the old files back.
/// Then it opens `exe` again and removes the work folder. What it does goes to `log`.
#[allow(clippy::too_many_arguments)]
pub fn windows_script(pid: u32, new_dir: &Path, install: &Path, backup: &Path, exe: &Path, keep: &[PathBuf], work: &Path, log: &Path, relaunch: bool) -> String {
    let start = if relaunch { format!("Start-Process -FilePath {} -WorkingDirectory {}", ps_quote(exe), ps_quote(install)) } else { "# the app closed by itself: it is not opened again".to_string() };
    let xd = if keep.is_empty() { String::new() } else { format!(" /XD {}", keep.iter().map(|k| ps_quote(k)).collect::<Vec<_>>().join(" ")) };
    let rc = "/E /R:5 /W:2 /NFL /NDL /NJH /NJS /NP";
    format!(
        r#"# Reelhouse update helper (src/updater.rs). Made by the app; removes itself.
$ErrorActionPreference = 'Continue'
$log = {log}
function Log($m) {{ try {{ Add-Content -LiteralPath $log -Value ('[' + (Get-Date -Format s) + '] ' + $m) }} catch {{}} }}
Log 'update: waiting for the app (pid {pid}) to close'
try {{ Wait-Process -Id {pid} -Timeout 180 -ErrorAction SilentlyContinue }} catch {{}}
Start-Sleep -Seconds 1
if (Test-Path -LiteralPath {backup}) {{ Remove-Item -LiteralPath {backup} -Recurse -Force -ErrorAction SilentlyContinue }}
robocopy {install} {backup} {rc}{xd} | Out-Null
if ($LASTEXITCODE -ge 8) {{
  Log "update: could not keep a copy of the old files (robocopy $LASTEXITCODE); nothing was changed"
  {start}
  exit 1
}}
robocopy {new} {install} {rc}{xd} | Out-Null
if ($LASTEXITCODE -ge 8) {{
  Log "update: copying the new files failed (robocopy $LASTEXITCODE); putting the old ones back"
  robocopy {backup} {install} {rc}{xd} | Out-Null
  Log "update: old files restored (robocopy $LASTEXITCODE). The copy stays in {backup_plain}"
  {start}
  exit 1
}}
Log 'update: installed'
{start}
Start-Sleep -Seconds 2
Remove-Item -LiteralPath {work} -Recurse -Force -ErrorAction SilentlyContinue
"#,
        log = ps_quote(log),
        pid = pid,
        backup = ps_quote(backup),
        backup_plain = backup.display().to_string().replace('"', "").replace('$', "`$"),
        install = ps_quote(install),
        new = ps_quote(new_dir),
        start = start,
        work = ps_quote(work),
        rc = rc,
        xd = xd,
    )
}

/// Folders inside the program folder the update must leave alone: the data folder (when REELHOUSE_DATA points
/// inside it) and the work folder.
pub fn kept_inside(install: &Path, data: &Path, work: &Path) -> Vec<PathBuf> {
    [data, work].iter().filter(|p| p.starts_with(install) && *p != &install).map(|p| p.to_path_buf()).collect()
}

/// Unpacks a zip with the tools Windows has (tar.exe since Windows 10, else PowerShell's Expand-Archive).
pub fn unzip(zip: &Path, dest: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(dest);
    std::fs::create_dir_all(dest).map_err(|e| format!("Could not make {}: {}", dest.display(), e))?;
    let tar = hidden(std::process::Command::new("tar").arg("-xf").arg(zip).arg("-C").arg(dest)).status();
    if matches!(tar, Ok(s) if s.success()) {
        return Ok(());
    }
    let cmd = format!("Expand-Archive -LiteralPath {} -DestinationPath {} -Force", ps_quote(zip), ps_quote(dest));
    let ps = hidden(std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &cmd])).status();
    match ps {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("The update could not be unpacked (Expand-Archive ended with {}).", s)),
        Err(e) => Err(format!("The update could not be unpacked: {}", e)),
    }
}

#[cfg(windows)]
fn hidden(c: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW
    c.creation_flags(0x0800_0000)
}

#[cfg(not(windows))]
fn hidden(c: &mut std::process::Command) -> &mut std::process::Command {
    c
}

/// Starts the helper of a Windows update, detached from the app (it must outlive it).
pub fn run_windows_script(script: &Path) -> Result<(), String> {
    let mut c = std::process::Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"]).arg(script);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB
        c.creation_flags(0x0800_0000 | 0x0000_0200 | 0x0100_0000);
    }
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    match c.spawn() {
        Ok(_) => Ok(()),
        Err(_) => {
            // a job that refuses the breakaway: start it without that flag
            let mut c = std::process::Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"]).arg(script);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x0800_0000 | 0x0000_0200);
            }
            c.spawn().map(|_| ()).map_err(|e| format!("Could not start the update helper: {}", e))
        }
    }
}

/// The first program of `names` found in the usual folders.
fn find_prog(names: &[&str]) -> Option<PathBuf> {
    for n in names {
        for d in ["/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
            let p = Path::new(d).join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// The shell command (run as root through pkexec) that installs an update on Linux: the changed files of a staging
/// folder copied over the program folder (each file replaced, not rewritten, so the running app keeps its old
/// copy), or a whole package.
pub fn linux_root_command(what: &LinuxInstall) -> Option<String> {
    let q = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', "'\\''"));
    match what {
        LinuxInstall::Files { stage, install } => Some(format!("cp -r --remove-destination {}/. {}/ && rm -rf {}", q(stage), q(install), q(stage))),
        LinuxInstall::Package { file, rpm: true } => {
            if find_prog(&["dnf5", "dnf"]).is_some() {
                Some(format!("dnf install -y {}", q(file)))
            } else if find_prog(&["zypper"]).is_some() {
                Some(format!("zypper --non-interactive install --allow-unsigned-rpm {}", q(file)))
            } else {
                Some(format!("rpm -U --replacepkgs {}", q(file)))
            }
        }
        LinuxInstall::Package { file, rpm: false } => Some(format!("apt-get install -y {}", q(file))),
    }
}

/// What a Linux update installs.
#[derive(Clone, Debug, PartialEq)]
pub enum LinuxInstall {
    /// The changed files in `stage`, copied over `install` (/opt/reelhouse).
    Files { stage: PathBuf, install: PathBuf },
    /// A whole .rpm / .deb.
    Package { file: PathBuf, rpm: bool },
}

/// Runs a Linux update as root through pkexec (the system asks for the password). `wait`: wait for the end (the
/// button); otherwise it is started in its own session so it outlives the app (when the app closes).
pub fn linux_install(what: &LinuxInstall, wait: bool) -> Result<(), String> {
    let pkexec = find_prog(&["pkexec"]).ok_or("pkexec is not installed: install the update by hand.")?;
    let cmd = linux_root_command(what).ok_or("Nothing to install.")?;
    if wait {
        let st = std::process::Command::new(&pkexec).args(["sh", "-c", &cmd]).status().map_err(|e| format!("Could not run pkexec: {}", e))?;
        return match st.code() {
            Some(0) => Ok(()),
            Some(126) | Some(127) => Err("The password was not given: nothing was installed.".into()),
            _ => Err(format!("The install ended with {}.", st)),
        };
    }
    // `sh` stays as pkexec's parent (pkexec asks about its parent process) after the app is gone
    let outer = format!("{} sh -c '{}'", pkexec.display(), cmd.replace('\'', "'\\''"));
    let mut c = std::process::Command::new(find_prog(&["setsid"]).unwrap_or_else(|| PathBuf::from("sh")));
    if c.get_program().to_string_lossy().ends_with("setsid") {
        c.args(["sh", "-c", &outer]);
    } else {
        c.args(["-c", &outer]);
    }
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    c.spawn().map(|_| ()).map_err(|e| format!("Could not start pkexec: {}", e))
}

/// The program folder of the installed Linux package (/opt/reelhouse), when the app runs from it.
pub fn linux_install_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let bin = exe.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let root = bin.parent()?.to_path_buf();
    root.starts_with("/opt").then_some(root)
}

/// Opens a downloaded package with the system (xdg-open on Linux, open on macOS).
pub fn open_with_system(file: &Path) -> Result<(), String> {
    let prog = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(prog).arg(file).spawn().map(|_| ()).map_err(|e| format!("Could not run {}: {}", prog, e))
}

/// Where a package for Linux/macOS is saved: the Downloads folder when there is one.
pub fn package_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        let d = PathBuf::from(h).join("Downloads");
        if d.is_dir() {
            return d;
        }
    }
    work_dir()
}

/// The command that installs a downloaded Linux package by hand.
pub fn linux_command(file: &Path, rpm: bool) -> String {
    if rpm {
        format!("sudo dnf install '{}'", file.display())
    } else {
        format!("sudo apt install '{}'", file.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;
    use std::cmp::Ordering::*;

    const LIST: &str = r#"[
        {"tag_name":"v0.9.0-beta.1","prerelease":true,"draft":false,"assets":[]},
        {"tag_name":"v1.0.0","prerelease":false,"draft":true,"assets":[]},
        {"tag_name":"v0.5.0","name":"Reelhouse 0.5.0","body":"New things","html_url":"https://github.com/Githyot/Reel-House/releases/tag/v0.5.0","prerelease":false,"draft":false,
         "assets":[{"id":11,"name":"ReelHouse-windows.zip","size":5},{"id":12,"name":"ReelHouse-linux-x86_64.deb","size":3},{"id":13,"name":"ReelHouse-linux-x86_64.rpm","size":3},
                   {"id":14,"name":"ReelHouse-macos.dmg","size":3},{"id":15,"name":"ReelHouse-android-arm64-v8a.apk","size":3},{"id":16,"name":"ReelHouse-ios-unsigned.ipa","size":3}]},
        {"tag_name":"v0.4.2","prerelease":false,"draft":false,"assets":[]}
    ]"#;

    #[test]
    fn versions() {
        assert_eq!(parse("v1.2.3"), Some((1, 2, 3, String::new())));
        assert_eq!(parse("0.4"), Some((0, 4, 0, String::new())));
        assert_eq!(parse("1.0.0-beta.2+abc"), Some((1, 0, 0, "beta.2".into())));
        assert_eq!(parse("nightly"), None);
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("0.3.1", "v0.3.0"));
        assert!(!is_newer("v0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        // a development build of 0.3.0 is older than the release 0.3.0
        assert!(is_newer("0.3.0", "0.3.0-dev.7"));
        assert_eq!(compare("1.0.0-alpha", "1.0.0-alpha.1"), Some(Less));
        assert_eq!(compare("1.0.0-beta.11", "1.0.0-beta.2"), Some(Greater));
        assert_eq!(compare("1.0.0-rc.1", "1.0.0-beta"), Some(Greater));
        assert_eq!(compare("x", "1.0"), None);
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(parse(&current_version()).is_some());
    }

    #[test]
    fn newest_skips_drafts_and_prereleases_and_picks_the_asset() {
        let r = newest(&Json::parse(LIST).unwrap()).unwrap();
        assert_eq!(r.tag, "v0.5.0");
        assert_eq!(r.version, "0.5.0");
        assert_eq!(r.notes, "New things");
        assert_eq!(r.asset_for(Platform::Windows).unwrap().id, 11);
        assert_eq!(r.asset_for(Platform::Linux(false)).unwrap().id, 12);
        assert_eq!(r.asset_for(Platform::Linux(true)).unwrap().id, 13);
        assert_eq!(r.asset_for(Platform::MacOs).unwrap().id, 14);
        assert_eq!(r.asset_named("ReelHouse-android-arm64-v8a.apk").unwrap().id, 15);
        assert_eq!(r.asset_for(Platform::Ios).unwrap().id, 16);
        assert!(r.asset_for(Platform::Other).is_none());
        assert!(Platform::Windows.downloads() && Platform::Android.downloads() && !Platform::Ios.downloads());
        assert_eq!(Platform::Linux(true).folder_zip(), Some("ReelHouse-linux-x86_64.zip"));
        assert!(Platform::Android.folder_zip().is_none());
        assert!(newest(&Json::parse("[]").unwrap()).is_none());
    }

    #[test]
    fn check_is_anonymous() {
        let f = Fake::new().route("GET https://api.github.com/repos/Githyot/Reel-House/releases", 200, LIST);
        let found = check(&f, " tok ").unwrap();
        assert!(!found.with_token);
        assert_eq!(found.update("0.4.2").unwrap().tag, "v0.5.0");
        assert!(found.update("0.5.0").is_none());
        let reqs = f.requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].headers.iter().all(|(k, _)| k != "Authorization"));
        let missing = Fake::new().route("GET https://api.github.com/repos/", 404, r#"{"message":"Not Found"}"#);
        assert_eq!(check(&missing, "tok").unwrap_err(), NOT_FOUND);
        assert_eq!(missing.requests().len(), 1);
    }

    #[test]
    fn the_token_helps_only_past_the_limit() {
        let f = Fake::new().route("GET https://api.github.com/repos/", 403, r#"{"message":"API rate limit exceeded for 1.2.3.4"}"#).route("GET https://api.github.com/repos/", 200, LIST);
        let found = check(&f, "tok").unwrap();
        assert!(found.with_token);
        assert!(f.requests()[1].headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer tok"));
        let limited = Fake::new().route("GET https://api.github.com/repos/", 403, r#"{"message":"API rate limit exceeded for 1.2.3.4"}"#);
        assert!(check(&limited, "").unwrap_err().contains("limits"));
    }

    #[test]
    fn linux_commands() {
        let files = LinuxInstall::Files { stage: PathBuf::from("/tmp/it's"), install: PathBuf::from("/opt/reelhouse") };
        let c = linux_root_command(&files).unwrap();
        assert!(c.starts_with("cp -r --remove-destination '/tmp/it'\\''s'/. '/opt/reelhouse'/"), "{}", c);
        let deb = linux_root_command(&LinuxInstall::Package { file: PathBuf::from("/tmp/a.deb"), rpm: false }).unwrap();
        assert_eq!(deb, "apt-get install -y '/tmp/a.deb'");
    }

    #[test]
    fn download_in_pieces() {
        let dir = std::env::temp_dir().join(format!("rh-upd-{}", std::process::id()));
        let dest = dir.join("a.zip");
        let f = Fake::new()
            .route("GET https://api.github.com/repos/Githyot/Reel-House/releases/assets/11", 206, "abc")
            .route("GET https://api.github.com/repos/Githyot/Reel-House/releases/assets/11", 206, "de");
        let asset = Asset { id: 11, name: "ReelHouse-windows.zip".into(), size: 5, digest: String::new(), url: String::new() };
        let seen = std::sync::Mutex::new(vec![]);
        download_in(&f, "tok", &asset, &dest, &|d, t| seen.lock().unwrap().push((d, t)), &|| true, 3).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"abcde");
        assert_eq!(*seen.lock().unwrap(), vec![(3, 5), (5, 5)]);
        let reqs = f.requests();
        assert!(reqs[0].headers.iter().any(|(k, v)| k == "Range" && v == "bytes=0-2"));
        assert!(reqs[1].headers.iter().any(|(k, v)| k == "Range" && v == "bytes=3-5"));
        assert!(reqs[0].headers.iter().any(|(k, v)| k == "Accept" && v == "application/octet-stream"));
        // a short file is an error, and so is a cancel
        let short = Fake::new().route("GET https://api.github.com/", 200, "ab");
        assert!(download_in(&short, "", &asset, &dest, &|_, _| {}, &|| true, 3).is_err());
        assert!(download_in(&f, "", &asset, &dest, &|_, _| {}, &|| false, 3).unwrap_err().contains("cancelled"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_helper_keeps_the_data_folder() {
        let install = Path::new("C:\\Apps\\ReelHouse");
        let data = install.join("data");
        let work = Path::new("C:\\Temp\\ReelHouse-update");
        let keep = kept_inside(install, &data, work);
        assert_eq!(keep, vec![data.clone()]);
        assert!(kept_inside(install, Path::new("C:\\Users\\x\\AppData\\Roaming\\Reelhouse"), work).is_empty());
        let s = windows_script(42, &work.join("new"), install, &work.join("old"), &install.join("reelhouse-qt.exe"), &keep, work, Path::new("C:\\it's\\update.log"), true);
        assert!(s.contains("Start-Process -FilePath 'C:\\Apps\\ReelHouse"));
        let quiet = windows_script(42, &work.join("new"), install, &work.join("old"), &install.join("reelhouse-qt.exe"), &keep, work, Path::new("C:\\l.log"), false);
        assert!(!quiet.contains("Start-Process"));
        assert!(s.contains("Wait-Process -Id 42"));
        assert!(s.contains(&format!("/XD '{}'", data.display())));
        assert!(s.contains("'C:\\it''s\\update.log'"));
        // the old files are copied before the new ones, and put back on failure
        let (old, new) = (s.find("robocopy 'C:\\Apps\\ReelHouse'").unwrap(), s.find(&format!("robocopy '{}'", work.join("new").display())).unwrap());
        assert!(old < new);
        assert!(s.contains(&format!("robocopy '{}' 'C:\\Apps\\ReelHouse'", work.join("old").display())));
    }

    #[test]
    fn finds_the_program_in_the_zip() {
        let dir = std::env::temp_dir().join(format!("rh-upd-zip-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("ReelHouse")).unwrap();
        std::fs::write(dir.join("ReelHouse").join("reelhouse-qt.exe"), b"x").unwrap();
        assert_eq!(find_app_dir(&dir, "reelhouse-qt.exe").unwrap(), dir.join("ReelHouse"));
        assert!(find_app_dir(&dir, "other.exe").is_none());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(linux_command(Path::new("/tmp/a.rpm"), true).starts_with("sudo dnf"));
    }
}
