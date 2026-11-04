//! Updates of the app (the logic is in `updater.rs` and `update_delta.rs`). This file is part of `controller`.
//!
//! The check at startup (Preferences > General > "Check for updates") finds a newer release. With "Update by itself"
//! on, it also downloads it in the background (only the changed files when it can) and:
//! - Windows and Linux: installs it when the app closes (state "ready"); "Install now" installs it at once.
//! - Android: hands the APK to the system installer as soon as it is ready (Android always asks the user).
use super::*;
use crate::update_delta as delta;
use crate::updater::{self, Found, LinuxInstall, Platform, Release};
use std::sync::atomic::AtomicU64;

/// An update downloaded and waiting to be installed.
#[derive(Clone, Debug)]
pub enum Ready {
    /// The helper that opens the app again (Install now) and the one that does not (when the app closes).
    Windows { relaunch: PathBuf, quiet: PathBuf },
    Linux(LinuxInstall),
    Android(PathBuf),
    /// macOS: a package opened with the system.
    Package(PathBuf),
}

/// The update while the app runs.
#[derive(Default)]
pub struct UpdateLive {
    /// "", "checking", "available", "current", "error", "downloading", "ready", "installing", "installed",
    /// "opened", "page".
    pub state: String,
    pub found: Option<Release>,
    /// The newest release seen (even when it is not newer).
    pub latest: String,
    pub error: String,
    /// A line for the user after the install step (the command of a Linux package, the file saved…).
    pub note: String,
    pub done: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
    pub ready: Option<Ready>,
    /// The download running now was started by the check at startup (install when the app closes).
    pub auto: bool,
    /// Bytes the last download took, and the size of the whole file (to show what the delta saved).
    pub downloaded: u64,
    pub full: u64,
}

pub const NEW_VERSION: &str = "A new version of Reelhouse is available: Preferences > General > Updates.";
pub const READY_ON_CLOSE: &str = "A new version of Reelhouse was downloaded: it is installed when you close the app.";
pub const INSTALLED_RESTART: &str = "The new version is installed. Close and open Reelhouse to use it.";

/// What a download prepared.
pub struct Prepared {
    pub ready: Ready,
    pub downloaded: u64,
    pub full: u64,
}

pub enum UpdateDone {
    /// The result, and whether it was the check at startup.
    Checked(Result<Found, String>, bool),
    Ready(Result<Prepared, String>),
    /// A Linux install that ran (the button).
    Installed(Result<(), String>),
}

impl Core {
    /// The check when the app opens (when it is on), a little after the start so it does not slow it down.
    pub fn update_startup(&mut self) {
        if !self.settings.general.update_check && !self.settings.general.update_auto {
            return;
        }
        let token = self.settings.backup.cloud.github_token.clone();
        self.spawn(move |tx, http| {
            std::thread::sleep(Duration::from_secs(8));
            let r = updater::check(&*http, &token);
            send(tx, Msg::Update(UpdateDone::Checked(r, true)));
        });
    }

    pub fn update_cmd(&mut self, cmd: &str) {
        match cmd {
            "update_check" => self.update_check(),
            "update_install" => self.update_install(),
            "update_cancel" => {
                self.update.cancel.store(true, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    fn update_info(&mut self) {
        crate::debug::log("update", &format!("{} {} {} {}", self.update.state, self.update.latest, self.update.error, self.update.note));
        self.events.push(Event::Info(MsgKind::Update, self.update.state.clone()));
    }

    fn update_busy(&self) -> bool {
        matches!(self.update.state.as_str(), "checking" | "downloading" | "installing")
    }

    fn update_check(&mut self) {
        if self.update_busy() || self.update.state == "ready" {
            return;
        }
        self.update.state = "checking".into();
        self.update.error.clear();
        self.update_info();
        let token = self.settings.backup.cloud.github_token.clone();
        self.spawn(move |tx, http| {
            let r = updater::check(&*http, &token);
            send(tx, Msg::Update(UpdateDone::Checked(r, false)));
        });
    }

    pub(super) fn on_update(&mut self, d: UpdateDone) {
        match d {
            UpdateDone::Checked(r, auto) => {
                if matches!(self.update.state.as_str(), "downloading" | "installing" | "ready") {
                    return;
                }
                match r {
                    Ok(f) => {
                        self.update.latest = f.latest.as_ref().map(|r| r.tag.clone()).unwrap_or_default();
                        self.update.found = f.update(&updater::current_version()).cloned();
                        self.update.state = if self.update.found.is_some() { "available".into() } else { "current".into() };
                        if auto && self.update.found.is_some() {
                            if self.settings.general.update_auto && Platform::this().installs_alone() {
                                self.update_info();
                                self.update_download(true);
                                return;
                            }
                            // a fixed text, so the window can translate it
                            self.events.push(Event::Toast(NEW_VERSION.into()));
                        }
                    }
                    Err(e) => {
                        // the check at startup fails quietly (offline, GitHub's limit…): only a press shows it
                        if auto {
                            crate::debug::log("update", &e);
                            return;
                        }
                        self.update.state = "error".into();
                        self.update.error = e;
                    }
                }
                self.update_info();
            }
            UpdateDone::Ready(Err(e)) => {
                let cancelled = self.update.cancel.load(Ordering::SeqCst);
                self.update.state = if cancelled { "available".into() } else { "error".into() };
                // a failed startup download is only logged; the button shows it
                if self.update.auto && !cancelled {
                    crate::debug::log("update", &format!("automatic download failed: {}", e));
                }
                self.update.error = e;
                self.update_info();
            }
            UpdateDone::Ready(Ok(p)) => {
                self.update.downloaded = p.downloaded;
                self.update.full = p.full;
                self.update.ready = Some(p.ready.clone());
                self.update.state = "ready".into();
                self.update.note.clear();
                if self.update.auto && !matches!(p.ready, Ready::Android(_)) {
                    self.events.push(Event::Toast(READY_ON_CLOSE.into()));
                    self.update_info();
                } else {
                    // the button (or Android, which always asks first): install now
                    self.update_install_now();
                }
            }
            UpdateDone::Installed(r) => {
                match r {
                    Ok(()) => {
                        self.update.state = "installed".into();
                        self.update.ready = None;
                        self.update.note = INSTALLED_RESTART.into();
                        self.events.push(Event::Toast(INSTALLED_RESTART.into()));
                    }
                    Err(e) => {
                        // pkexec missing or refused: the package goes to the system installer, the command is shown
                        self.update.state = "ready".into();
                        self.update.error = e;
                        if let Some(Ready::Linux(LinuxInstall::Package { file, rpm })) = &self.update.ready {
                            let _ = updater::open_with_system(file);
                            self.update.note = format!("{}\n{}", file.display(), updater::linux_command(file, *rpm));
                        }
                    }
                }
                self.update_info();
            }
        }
    }

    /// The button: installs a downloaded update, else downloads it (and installs it when it is ready).
    fn update_install(&mut self) {
        if self.update.state == "ready" && self.update.ready.is_some() {
            self.update_install_now();
            return;
        }
        if self.update.found.is_none() {
            self.update_check();
            return;
        }
        self.update_download(false);
    }

    fn update_install_now(&mut self) {
        let Some(ready) = self.update.ready.clone() else { return };
        self.update.error.clear();
        match ready {
            Ready::Windows { relaunch, .. } => {
                self.save_settings();
                match updater::run_windows_script(&relaunch) {
                    Ok(()) => {
                        self.update.ready = None;
                        self.update.state = "installing".into();
                        self.update_info();
                        // the helper waits for the app to close, then copies the new files and opens it again
                        self.events.push(Event::Quit);
                    }
                    Err(e) => {
                        self.update.state = "error".into();
                        self.update.error = e;
                        self.update_info();
                    }
                }
            }
            Ready::Linux(what) => {
                self.update.state = "installing".into();
                self.update_info();
                self.spawn(move |tx, _| {
                    let r = updater::linux_install(&what, true);
                    send(tx, Msg::Update(UpdateDone::Installed(r)));
                });
            }
            Ready::Android(apk) => {
                // the system installer asks; the app is replaced (and closed) when the user agrees
                self.events.push(Event::InstallApk(apk.display().to_string()));
                self.update_info();
            }
            Ready::Package(file) => {
                let opened = updater::open_with_system(&file);
                self.update.state = "opened".into();
                self.update.ready = None;
                self.update.note = file.display().to_string();
                if let Err(e) = opened {
                    self.update.error = e;
                }
                self.update_info();
            }
        }
    }

    /// Called when the app closes: a downloaded update is installed now (Windows, Linux).
    pub fn update_on_close(&mut self) {
        if self.update.state != "ready" {
            return;
        }
        let r = match self.update.ready.take() {
            Some(Ready::Windows { quiet, .. }) => updater::run_windows_script(&quiet),
            Some(Ready::Linux(what)) => updater::linux_install(&what, false),
            _ => Ok(()),
        };
        if let Err(e) = r {
            crate::debug::log("update", &format!("install on close failed: {}", e));
        }
    }

    fn update_download(&mut self, auto: bool) {
        let Some(rel) = self.update.found.clone() else { return };
        if self.update_busy() {
            return;
        }
        let p = Platform::this();
        let asset = rel.asset_for(p).cloned().or_else(|| p.folder_zip().and_then(|z| rel.asset_named(z)).cloned());
        if !p.downloads() || asset.is_none() {
            // iOS (and a release without the file of this platform): the release page
            if auto {
                self.events.push(Event::Toast(NEW_VERSION.into()));
                return;
            }
            let page = if rel.page.is_empty() { updater::RELEASES_PAGE.to_string() } else { rel.page.clone() };
            self.events.push(Event::OpenUrl(page));
            self.update.state = "page".into();
            self.update_info();
            return;
        }
        let token = self.settings.backup.cloud.github_token.clone();
        self.update.auto = auto;
        self.update.state = "downloading".into();
        self.update.error.clear();
        self.update.note.clear();
        self.update.cancel = Arc::new(AtomicBool::new(false));
        self.update.done.store(0, Ordering::SeqCst);
        self.update.total.store(asset.as_ref().map(|a| a.size).unwrap_or(0), Ordering::SeqCst);
        self.update_info();
        let (done, total, cancel) = (self.update.done.clone(), self.update.total.clone(), self.update.cancel.clone());
        let data = self.data_dir.clone();
        let tx2 = self.tx.clone();
        self.spawn(move |tx, http| {
            let progress = |d: u64, t: u64| {
                done.store(d, Ordering::SeqCst);
                total.store(t, Ordering::SeqCst);
                // the window redraws the bar on the Info event
                send(&tx2, Msg::Info(MsgKind::Update, "downloading".into()));
            };
            let go_on = || !cancel.load(Ordering::SeqCst);
            // files come from their public address; the token is only for the API fallback (no address known)
            let tok = token;
            let job = Job { http: &*http, token: &tok, rel: &rel, progress: &progress, go_on: &go_on };
            let r = match p {
                Platform::Windows => prepare_windows(&job, &data),
                Platform::Linux(rpm) => prepare_linux(&job, rpm),
                Platform::Android => prepare_android(&job, &data),
                _ => {
                    let a = rel.asset_for(p).cloned().ok_or("This release has no file for this system.".to_string());
                    a.and_then(|a| {
                        let dest = updater::package_dir().join(&a.name);
                        updater::download(&*http, &tok, &a, &dest, &progress, &go_on)?;
                        updater::verify(&a, &dest)?;
                        Ok(Prepared { ready: Ready::Package(dest), downloaded: a.size, full: a.size })
                    })
                }
            };
            send(tx, Msg::Update(UpdateDone::Ready(r)));
        });
    }

    pub fn update_state(&self) -> Json {
        let u = &self.update;
        let f = u.found.as_ref();
        Json::obj(vec![
            ("current", updater::current_version().into()),
            ("platform", Platform::this().id().into()),
            ("canInstall", Platform::this().downloads().into()),
            ("state", u.state.clone().into()),
            ("latest", u.latest.clone().into()),
            ("error", u.error.clone().into()),
            ("note", u.note.clone().into()),
            ("tag", f.map(|r| r.tag.clone()).unwrap_or_default().into()),
            ("name", f.map(|r| r.name.clone()).unwrap_or_default().into()),
            ("notes", f.map(|r| r.notes.clone()).unwrap_or_default().into()),
            ("page", f.map(|r| r.page.clone()).unwrap_or_else(|| updater::RELEASES_PAGE.to_string()).into()),
            ("size", (f.and_then(|r| r.asset_for(Platform::this())).map(|a| a.size).unwrap_or(0) as f64).into()),
            ("done", (u.done.load(Ordering::SeqCst) as f64).into()),
            ("total", (u.total.load(Ordering::SeqCst) as f64).into()),
            ("downloaded", (u.downloaded as f64).into()),
            ("full", (u.full as f64).into()),
            // installed when the app closes (Windows, Linux) or by the system installer (Android)
            ("onClose", matches!(u.ready, Some(Ready::Windows { .. }) | Some(Ready::Linux(_))).into()),
        ])
    }
}

/// What every download needs.
struct Job<'a> {
    http: &'a dyn Http,
    token: &'a str,
    rel: &'a Release,
    progress: &'a dyn Fn(u64, u64),
    go_on: &'a dyn Fn() -> bool,
}

impl Job<'_> {
    fn fetcher<'b>(&'b self, a: &'b updater::Asset) -> impl Fn(u64, u64) -> Result<Vec<u8>, String> + 'b {
        move |from, len| updater::fetch_range(self.http, self.token, a, from, len)
    }
}

/// A delta is worth it when it saves at least a fifth of the file.
fn worth(delta: u64, full: u64) -> bool {
    full > 0 && delta * 5 < full * 4
}

/// Only the changed files of a program-folder zip, unpacked into `stage`. None when a delta is not worth it.
fn folder_delta(job: &Job, zip: &updater::Asset, install: &Path, exe_rel: &str, stage: &Path) -> Result<Option<u64>, String> {
    let fetch = job.fetcher(zip);
    let dir = delta::read_directory(&fetch, zip.size)?;
    let plan = delta::plan_folder(&dir, install, exe_rel)?;
    crate::debug::log("update", &format!("delta: {} changed files, {} of {} bytes", plan.changed.len(), plan.bytes, zip.size));
    if !worth(plan.bytes, zip.size) {
        return Ok(None);
    }
    delta::fetch_folder(&fetch, &dir, &plan, stage, Some(install), job.progress, job.go_on)?;
    Ok(Some(plan.bytes))
}

/// Windows: the changed files (or the whole zip, unpacked), then the helpers that copy them over the program folder.
/// The data folder is never inside what gets replaced (it is left out when it is).
fn prepare_windows(job: &Job, data: &Path) -> Result<Prepared, String> {
    let asset = job.rel.asset_for(Platform::Windows).cloned().ok_or("This release has no Windows file.")?;
    let exe = std::env::current_exe().map_err(|e| format!("Could not find the program: {}", e))?;
    let install = exe.parent().ok_or("Could not find the program folder.")?.to_path_buf();
    let exe_name = exe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "reelhouse-qt.exe".into());
    // a folder Windows protects (Program Files) cannot be written without asking for administrator rights
    let probe = install.join(".reelhouse-update-probe");
    std::fs::write(&probe, b"x").map_err(|_| format!("Reelhouse cannot write to its folder ({}). Move the folder somewhere of yours (like Documents) or update by hand.", install.display()))?;
    let _ = std::fs::remove_file(&probe);
    if data == install {
        return Err("The data folder is the program folder: update by hand so nothing of yours is replaced.".into());
    }
    let work = updater::work_dir();
    let _ = std::fs::remove_dir_all(&work);
    let stage = work.join("new");
    let (new_dir, downloaded) = match folder_delta(job, &asset, &install, &exe_name, &stage) {
        Ok(Some(n)) => (stage, n),
        r => {
            if let Err(e) = r {
                if !(job.go_on)() {
                    return Err(e);
                }
                crate::debug::log("update", &format!("delta failed, downloading the whole zip: {}", e));
            }
            let zip = work.join(&asset.name);
            updater::download(job.http, job.token, &asset, &zip, job.progress, job.go_on)?;
            updater::verify(&asset, &zip)?;
            let unpacked = work.join("unpacked");
            updater::unzip(&zip, &unpacked)?;
            let _ = std::fs::remove_file(&zip);
            (updater::find_app_dir(&unpacked, &exe_name).ok_or_else(|| format!("The update has no {}.", exe_name))?, asset.size)
        }
    };
    let keep = updater::kept_inside(&install, data, &work);
    let log = crate::debug::dir().unwrap_or_else(|| data.to_path_buf()).join("update.log");
    let write = |name: &str, relaunch: bool| -> Result<PathBuf, String> {
        let script = updater::windows_script(std::process::id(), &new_dir, &install, &work.join("old"), &exe, &keep, &work, &log, relaunch);
        let path = work.join(name);
        // PowerShell 5 reads a file without a byte order mark as the system code page: keep paths with accents right
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(script.replace('\n', "\r\n").as_bytes());
        std::fs::write(&path, bytes).map_err(|e| format!("Could not write {}: {}", path.display(), e))?;
        Ok(path)
    };
    let ready = Ready::Windows { relaunch: write("update.ps1", true)?, quiet: write("update-on-close.ps1", false)? };
    Ok(Prepared { ready, downloaded, full: asset.size })
}

/// Linux: the changed files of the release's zip when the app runs from /opt/reelhouse, else the whole package.
fn prepare_linux(job: &Job, rpm: bool) -> Result<Prepared, String> {
    let pkg = job.rel.asset_for(Platform::Linux(rpm)).cloned();
    let zip = Platform::Linux(rpm).folder_zip().and_then(|z| job.rel.asset_named(z)).cloned();
    if let (Some(install), Some(zip)) = (updater::linux_install_dir(), zip.as_ref()) {
        let stage = updater::work_dir().join("stage");
        match folder_delta(job, zip, &install, Platform::Linux(rpm).exe_rel(), &stage) {
            Ok(Some(n)) => return Ok(Prepared { ready: Ready::Linux(LinuxInstall::Files { stage, install }), downloaded: n, full: pkg.as_ref().map(|p| p.size).unwrap_or(zip.size) }),
            Ok(None) => {}
            Err(e) => {
                if !(job.go_on)() {
                    return Err(e);
                }
                crate::debug::log("update", &format!("delta failed, downloading the whole package: {}", e));
            }
        }
    }
    let pkg = pkg.ok_or("This release has no package for this system.")?;
    let file = updater::package_dir().join(&pkg.name);
    updater::download(job.http, job.token, &pkg, &file, job.progress, job.go_on)?;
    updater::verify(&pkg, &file)?;
    Ok(Prepared { ready: Ready::Linux(LinuxInstall::Package { file, rpm }), downloaded: pkg.size, full: pkg.size })
}

/// Android: the new APK put together from the installed one plus the changed parts (checked against GitHub's
/// SHA-256), else downloaded whole.
fn prepare_android(job: &Job, data: &Path) -> Result<Prepared, String> {
    let asset = job.rel.asset_for(Platform::Android).cloned().ok_or("This release has no APK for this phone.")?;
    let dir = data.join("update");
    let _ = std::fs::remove_dir_all(&dir);
    let dest = dir.join(&asset.name);
    if !asset.sha256().is_empty() {
        if let Some(old) = delta::installed_apk() {
            let r = (|| -> Result<Option<u64>, String> {
                let fetch = job.fetcher(&asset);
                let new = delta::read_directory(&fetch, asset.size)?;
                let od = delta::read_local_directory(&old)?;
                let plan = delta::plan_rebuild(&new, &od);
                crate::debug::log("update", &format!("apk delta: {} of {} bytes, {} files reused", plan.bytes, asset.size, plan.reuse.len()));
                if !worth(plan.bytes, asset.size) {
                    return Ok(None);
                }
                delta::rebuild_zip(&fetch, &new, &old, &plan, &dest, job.progress, job.go_on)?;
                updater::verify(&asset, &dest)?;
                Ok(Some(plan.bytes))
            })();
            match r {
                Ok(Some(n)) => return Ok(Prepared { ready: Ready::Android(dest), downloaded: n, full: asset.size }),
                Ok(None) => {}
                Err(e) => {
                    if !(job.go_on)() {
                        return Err(e);
                    }
                    crate::debug::log("update", &format!("apk delta failed, downloading the whole APK: {}", e));
                }
            }
        }
    }
    updater::download(job.http, job.token, &asset, &dest, job.progress, job.go_on)?;
    updater::verify(&asset, &dest)?;
    Ok(Prepared { ready: Ready::Android(dest), downloaded: asset.size, full: asset.size })
}
