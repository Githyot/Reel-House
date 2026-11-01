//! Updates of the app (the logic is in `updater.rs`). This file is part of `controller`.
use super::*;
use crate::updater::{self, Found, Platform, Release};
use std::sync::atomic::AtomicU64;

/// The update while the app runs.
#[derive(Default)]
pub struct UpdateLive {
    /// "", "checking", "available", "current", "error", "downloading", "installing", "opened", "page".
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
}

pub const NEW_VERSION: &str = "A new version of Reelhouse is available: Preferences > General > Updates.";

pub enum UpdateDone {
    /// The result, and whether it was the check at startup.
    Checked(Result<Found, String>, bool),
    /// Windows: the helper is ready at this path. Linux/macOS: the package was saved there.
    Ready(Result<PathBuf, String>),
}

impl Core {
    /// The check when the app opens (when it is on), a little after the start so it does not slow it down.
    pub fn update_startup(&mut self) {
        if !self.settings.general.update_check {
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
        crate::debug::log("update", &format!("{} {} {}", self.update.state, self.update.latest, self.update.error));
        self.events.push(Event::Info(MsgKind::Update, self.update.state.clone()));
    }

    fn update_check(&mut self) {
        if matches!(self.update.state.as_str(), "checking" | "downloading" | "installing") {
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
                if matches!(self.update.state.as_str(), "downloading" | "installing") {
                    return;
                }
                match r {
                    Ok(f) => {
                        self.update.latest = f.latest.as_ref().map(|r| r.tag.clone()).unwrap_or_default();
                        self.update.found = f.update(&updater::current_version()).cloned();
                        self.update.state = if self.update.found.is_some() { "available".into() } else { "current".into() };
                        if auto && self.update.found.is_some() {
                            // a fixed text, so the window can translate it
                            self.events.push(Event::Toast(NEW_VERSION.into()));
                        }
                    }
                    Err(e) => {
                        // the check at startup fails quietly (no token yet, offline…): only a press shows it
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
                self.update.state = if self.update.cancel.load(Ordering::SeqCst) { "available".into() } else { "error".into() };
                self.update.error = e;
                self.update_info();
            }
            UpdateDone::Ready(Ok(path)) => match Platform::this() {
                Platform::Windows => {
                    self.save_settings();
                    match updater::run_windows_script(&path) {
                        Ok(()) => {
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
                p => {
                    let opened = updater::open_with_system(&path);
                    self.update.state = "opened".into();
                    self.update.note = match p {
                        Platform::Linux(rpm) => format!("{}\n{}", path.display(), updater::linux_command(&path, rpm)),
                        _ => path.display().to_string(),
                    };
                    if let Err(e) = opened {
                        self.update.error = e;
                    }
                    self.update_info();
                }
            },
        }
    }

    fn update_install(&mut self) {
        let Some(rel) = self.update.found.clone() else {
            self.update_check();
            return;
        };
        if matches!(self.update.state.as_str(), "downloading" | "installing") {
            return;
        }
        let p = Platform::this();
        let asset = rel.asset_for(p).cloned();
        if !p.downloads() || asset.is_none() {
            // phones (and a release without the file of this platform): the release page
            let page = if rel.page.is_empty() { updater::RELEASES_PAGE.to_string() } else { rel.page.clone() };
            self.events.push(Event::OpenUrl(page));
            self.update.state = "page".into();
            self.update_info();
            return;
        }
        let asset = asset.unwrap();
        let token = self.settings.backup.cloud.github_token.clone();
        self.update.state = "downloading".into();
        self.update.error.clear();
        self.update.note.clear();
        self.update.cancel = Arc::new(AtomicBool::new(false));
        self.update.done.store(0, Ordering::SeqCst);
        self.update.total.store(asset.size, Ordering::SeqCst);
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
            let r = match p {
                Platform::Windows => prepare_windows(&*http, &token, &asset, &data, &progress, &go_on),
                _ => {
                    let dest = updater::package_dir().join(&asset.name);
                    updater::download(&*http, &token, &asset, &dest, &progress, &go_on).map(|_| dest)
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
            ("tokenSet", (!self.settings.backup.cloud.github_token.trim().is_empty()).into()),
        ])
    }
}

/// Windows: downloads and unpacks the zip, then writes the helper that replaces the program folder. Returns the
/// helper's path. The data folder is never inside what gets replaced (it is left out when it is).
fn prepare_windows(http: &dyn Http, token: &str, asset: &updater::Asset, data: &Path, progress: &dyn Fn(u64, u64), go_on: &dyn Fn() -> bool) -> Result<PathBuf, String> {
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
    let zip = work.join(&asset.name);
    updater::download(http, token, asset, &zip, progress, go_on)?;
    let unpacked = work.join("new");
    updater::unzip(&zip, &unpacked)?;
    let _ = std::fs::remove_file(&zip);
    let new_dir = updater::find_app_dir(&unpacked, &exe_name).ok_or_else(|| format!("The update has no {}.", exe_name))?;
    let keep = updater::kept_inside(&install, data, &work);
    let log = crate::debug::dir().unwrap_or_else(|| data.to_path_buf()).join("update.log");
    let script = updater::windows_script(std::process::id(), &new_dir, &install, &work.join("old"), &exe, &keep, &work, &log);
    let path = work.join("update.ps1");
    // PowerShell 5 reads a file without a byte order mark as the system code page: keep paths with accents right
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(script.replace('\n', "\r\n").as_bytes());
    std::fs::write(&path, bytes).map_err(|e| format!("Could not write {}: {}", path.display(), e))?;
    Ok(path)
}
