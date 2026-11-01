//! Backup and restore of everything the app keeps about the user, and the copy online. This file is part of
//! `controller`. The file format is in `backup.rs`, the clouds in `cloud.rs`.
use super::*;
use crate::backup;
use crate::cloud::{self, Done, Provider};

/// Copies kept of the backups made just before a restore.
const KEEP_BEFORE_RESTORE: usize = 3;

impl Core {
    /// Addresses of the pictures in use: the library, favorites, what is being watched and the chosen art.
    fn backup_urls(&self) -> Vec<String> {
        let mut urls: Vec<String> = Vec::new();
        let mut add = |u: &Option<String>| {
            if let Some(u) = u.as_ref().filter(|u| u.starts_with("http")) {
                if !urls.contains(u) {
                    urls.push(u.clone());
                }
            }
        };
        for m in self.library.iter().chain(self.favorites.iter()).chain(self.progress.iter().map(|p| &p.meta)) {
            add(&m.poster);
            add(&m.background);
            add(&m.logo);
        }
        for (p, b) in self.art.values() {
            add(p);
            add(b);
        }
        for d in &self.downloads {
            add(&d.poster);
        }
        for (_, poster) in self.covers.values() {
            add(&Some(poster.clone()));
        }
        urls
    }

    fn backup_options(&self) -> backup::Options {
        backup::Options { media: self.settings.backup.media }
    }

    /// Everything kept in memory is written first, so the backup holds what is on screen now.
    fn flush_for_backup(&mut self) {
        let _ = self.settings.save(&self.data_dir);
        if self.study_dirty > 0 {
            self.save_study();
        }
    }

    /// Writes a backup. `dest` None means one in the backup folder (automatic, "Back up now" and "Upload now"; the
    /// oldest ones there are removed).
    pub fn backup_now(&mut self, dest: Option<PathBuf>) {
        if self.backup_busy {
            return;
        }
        self.backup_busy = true;
        let auto = dest.is_none();
        // a name typed without the extension gets it
        let dest = dest.map(|p| if p.to_string_lossy().to_lowercase().ends_with(backup::EXT) { p } else { PathBuf::from(format!("{}{}", p.display(), backup::EXT)) });
        let dir = self.data_dir.clone();
        let urls = self.backup_urls();
        let opts = self.backup_options();
        let folder = backup::backup_dir(&dir, &self.settings.backup.folder);
        let keep = self.settings.backup.keep as usize;
        self.flush_for_backup();
        self.events.push(Event::Info(MsgKind::Backup, "Saving the backup…".to_string()));
        self.spawn(move |tx, _| {
            let now = util::now_unix();
            let target = dest.unwrap_or_else(|| backup::free_name(&folder, now, backup::PREFIX));
            let r = backup::write_backup(&dir, &target, &urls, now, opts).map(|s| (target.to_string_lossy().into_owned(), s.files, s.size));
            if auto && r.is_ok() {
                // the old automatic backups ("reelhouse-auto-…proto.gz") are pruned with the new ones
                backup::prune(&folder, &[backup::PREFIX, "reelhouse-auto-"], keep);
            }
            send(tx, Msg::BackupDone(r, auto, now));
        });
    }

    /// Called about once a minute: makes the automatic backup when it is due.
    pub fn auto_backup_tick(&mut self) {
        let b = &self.settings.backup;
        if !b.auto || self.backup_busy {
            return;
        }
        let due = b.last == 0 || util::now_unix().saturating_sub(b.last) >= b.every_hours as u64 * 3600;
        if due {
            self.backup_now(None);
        }
    }

    pub(super) fn on_backup_done(&mut self, r: Result<(String, usize, u64), String>, auto: bool, now: u64) {
        self.backup_busy = false;
        let upload = std::mem::take(&mut self.cloud.upload_next);
        match r {
            Ok((path, n, bytes)) => {
                self.settings.backup.last = now;
                self.save_settings();
                self.events.push(Event::Info(MsgKind::Backup, format!("Backup saved: {} ({} files, {}).", path, n, torrent::human_bytes(bytes))));
                if !auto {
                    self.toast("Backup saved.");
                }
                if upload || self.settings.backup.cloud.enabled {
                    self.cloud_upload_file(PathBuf::from(path), upload);
                }
            }
            Err(e) => {
                self.events.push(Event::Info(MsgKind::Backup, e.clone()));
                self.toast(&format!("Backup failed: {}", e));
            }
        }
    }

    /// Puts a backup file back. A copy of the current state is saved first (in the `backups` folder of the data
    /// folder, "reelhouse-before-restore-…"), so a wrong restore can be undone.
    pub fn restore_backup(&mut self, src: PathBuf) {
        if self.backup_busy {
            self.toast("A backup is being saved; try again in a moment.");
            return;
        }
        self.backup_busy = true;
        let dir = self.data_dir.clone();
        let urls = self.backup_urls();
        let opts = self.backup_options();
        self.flush_for_backup();
        self.events.push(Event::Info(MsgKind::Backup, "Restoring…".to_string()));
        self.spawn(move |tx, _| {
            let now = util::now_unix();
            let safety = dir.join("backups");
            let r = backup::write_backup(&dir, &backup::free_name(&safety, now, backup::BEFORE_RESTORE), &urls, now, opts)
                .map_err(|e| format!("The backup was not restored: the copy of what you have now could not be saved first ({}).", e))
                .and_then(|_| {
                    backup::prune(&safety, &[backup::BEFORE_RESTORE], KEEP_BEFORE_RESTORE);
                    backup::restore(&dir, &src)
                });
            send(tx, Msg::Restored(r));
        });
    }

    pub(super) fn on_restored(&mut self, r: Result<usize, String>) {
        self.backup_busy = false;
        match r {
            Err(e) => {
                self.events.push(Event::Info(MsgKind::Backup, e.clone()));
                self.toast(&e);
            }
            Ok(n) => {
                let dir = self.data_dir.clone();
                // the backup options and the cloud sign-in belong to this computer: a restored backup (maybe made
                // elsewhere, maybe old) does not replace them
                let this_computer = self.settings.backup.clone();
                self.settings = Settings::load(&dir);
                self.settings.backup = this_computer;
                // what everyone shares
                self.art = load_art(&dir);
                self.downloads = downloads::load(&dir);
                self.novel_installed = Core::load_novels(&dir, &dir).1;
                self.events.push(Event::NovelPlugins);
                // the profiles of the backup, and every list of the one that was in use when it was made (library,
                // favourites, history, Images and Text shelves, the Study collection, knowledge.md, Learn…)
                self.profiles = crate::profiles::Store::load(&dir);
                self.profile_dir = self.profiles.active_dir(&dir);
                let _ = std::fs::create_dir_all(&self.profile_dir);
                self.learn = controller_learn::LearnState::default();
                self.browser = crate::browser::Browser::with_lists(dir.join("browser"), self.profile_dir.join("browser"));
                self.profile_reload();
                self.save_settings();
                self.bootstrap();
                self.refresh_mal_list();
                self.reload_forced();
                self.events.push(Event::Info(MsgKind::Backup, format!("Backup restored ({} files). Restart Reelhouse so every part reads it again.", n)));
                self.toast("Backup restored. Restart Reelhouse so every part reads it again.");
                self.events.push(Event::Restored);
                self.events.push(Event::Progress);
                self.events.push(Event::Home);
                self.events.push(Event::Detail);
            }
        }
    }

    // ----- the copy online -----

    /// A status line for the window (and the debug report).
    fn cloud_status(&mut self, text: &str) {
        self.cloud.status = text.to_string();
        crate::debug::log("cloud", text);
        self.events.push(Event::Info(MsgKind::Backup, text.to_string()));
    }

    /// The sync folder used: the one chosen by hand, or the first one found.
    pub fn cloud_folder(&self) -> Option<PathBuf> {
        let c = &self.settings.backup.cloud;
        if !c.folder.trim().is_empty() {
            return Some(PathBuf::from(c.folder.trim()));
        }
        self.cloud.detected(c.provider()).into_iter().next()
    }

    /// Commands of the Backup section: "cloud_sign_in", "cloud_cancel", "cloud_sign_out", "cloud_list",
    /// "cloud_upload", "cloud_restore" (`id`, `name`), "cloud_detect", "cloud_github_connect" (`token`, `repo`).
    pub fn cloud_cmd(&mut self, cmd: &str, j: &Json) {
        match cmd {
            "cloud_sign_in" => self.cloud_sign_in(),
            "cloud_github_connect" => self.cloud_github_connect(&j.get("token").str(), &j.get("repo").str()),
            "cloud_cancel" => {
                self.cloud.cancel.store(true, Ordering::SeqCst);
                self.cloud.pending = None;
                self.cloud_status("Sign-in cancelled.");
            }
            "cloud_sign_out" => {
                let p = self.settings.backup.cloud.provider();
                self.settings.backup.cloud.set_login(p, "", "");
                self.cloud.access = None;
                self.cloud.remote.clear();
                self.cloud.listed = false;
                self.save_settings();
                self.cloud_status(&format!("Signed out of {}.", p.name()));
            }
            "cloud_list" => self.cloud_list(),
            "cloud_upload" => {
                if self.backup_busy {
                    self.toast("A backup is being saved; try again in a moment.");
                } else {
                    // a fresh backup, uploaded when it is written (on_backup_done)
                    self.cloud.upload_next = true;
                    self.backup_now(None);
                }
            }
            "cloud_restore" => self.cloud_restore(&j.get("id").str(), &j.get("name").str()),
            "cloud_detect" => {
                self.cloud.redetect();
                let p = self.settings.backup.cloud.provider();
                let found = self.cloud.detected(p);
                self.cloud_status(&match found.first() {
                    Some(f) => format!("{} sync folder: {}", p.name(), f.display()),
                    None => format!("No {} sync folder was found on this computer.", p.name()),
                });
            }
            _ => {}
        }
    }

    /// Starts the device-code sign-in: a code is shown, the user types it on the provider's page.
    fn cloud_sign_in(&mut self) {
        let c = &self.settings.backup.cloud;
        let p = c.provider();
        if !p.has_account() {
            self.cloud_status("iCloud Drive has no sign-in here: Apple offers no iCloud Drive API for Windows programs. Use its sync folder (iCloud for Windows).");
            return;
        }
        if p == Provider::GitHub {
            self.cloud_status("GitHub is reached with a token: paste it and press Connect.");
            return;
        }
        let app = c.app(p);
        // a sign-in already waiting is replaced
        self.cloud.cancel.store(true, Ordering::SeqCst);
        self.cloud.cancel = Arc::new(AtomicBool::new(false));
        self.cloud.pending = None;
        self.cloud_status(&format!("Asking {} for a sign-in code…", p.name()));
        self.spawn(move |tx, http| {
            send(tx, Msg::Cloud(Done::Code(p, cloud::device_start(&*http, &app))));
        });
    }

    /// GitHub: checks the token, finds or makes the private repository, then keeps both (and turns the online copy
    /// on: connecting is asking for it).
    fn cloud_github_connect(&mut self, token: &str, repo: &str) {
        let token = token.trim().to_string();
        // an empty field keeps the token already connected (a new repository name only)
        let token = if token.is_empty() { self.settings.backup.cloud.github_token.clone() } else { token };
        if token.is_empty() {
            self.cloud_status("Paste a GitHub token first.");
            return;
        }
        if self.cloud.busy == "signin" {
            return;
        }
        let repo = repo.trim().to_string();
        self.cloud.busy = "signin".into();
        self.cloud_status("Checking the token on GitHub…");
        self.spawn(move |tx, http| {
            let r = cloud::github::connect(&*http, &token, &repo).map(|(login, full)| (token, login, full));
            send(tx, Msg::Cloud(Done::GitHub(r)));
        });
    }

    /// Waits (in the background) for the code to be typed, checking every few seconds as the provider asks.
    fn cloud_wait_sign_in(&mut self, p: Provider, code: cloud::DeviceCode) {
        let app = self.settings.backup.cloud.app(p);
        let cancel = self.cloud.cancel.clone();
        self.spawn(move |tx, http| {
            let started = std::time::Instant::now();
            let mut every = code.interval.max(1);
            let r = loop {
                for _ in 0..every * 4 {
                    std::thread::sleep(Duration::from_millis(250));
                    if cancel.load(Ordering::SeqCst) {
                        return;
                    }
                }
                if started.elapsed().as_secs() > code.expires_in {
                    break Err("The code expired before it was typed. Start the sign-in again.".to_string());
                }
                match cloud::device_poll(&*http, &app, &code.device_code) {
                    cloud::Poll::Pending => {}
                    cloud::Poll::SlowDown => every += 5,
                    cloud::Poll::Done(t) => {
                        let name = cloud::account_name(&*http, p, &t.access);
                        break Ok((t, name));
                    }
                    cloud::Poll::Failed(e) => break Err(e),
                }
            };
            if !cancel.load(Ordering::SeqCst) {
                send(tx, Msg::Cloud(Done::SignedIn(p, r)));
            }
        });
    }

    /// Keeps tokens refreshed on the way: the access token in memory, a new refresh token in the settings
    /// (Microsoft gives a new one each time).
    fn cloud_keep_tokens(&mut self, t: Option<(Provider, cloud::Tokens)>) {
        let Some((p, t)) = t else { return };
        self.cloud.access = Some((p.id().to_string(), t.access.clone(), util::now_unix() + t.expires_in));
        if !t.refresh.is_empty() && t.refresh != self.settings.backup.cloud.refresh_token(p) {
            let account = self.settings.backup.cloud.account(p).to_string();
            self.settings.backup.cloud.set_login(p, &t.refresh, &account);
            self.save_settings();
        }
    }

    /// What an account job needs: the app, the refresh token and the access token in memory. None (with the reason
    /// shown) when not signed in.
    fn cloud_account(&mut self) -> Option<(Provider, cloud::OAuthApp, String, Option<String>)> {
        let c = &self.settings.backup.cloud;
        let p = c.provider();
        let refresh = c.refresh_token(p).to_string();
        if refresh.is_empty() {
            self.cloud_status(&format!("Sign in to {} first.", p.name()));
            return None;
        }
        Some((p, c.app(p), refresh, self.cloud.token(p)))
    }

    /// Sends a backup file to the cloud chosen (copy to the sync folder, or upload with the account).
    fn cloud_upload_file(&mut self, file: PathBuf, by_hand: bool) {
        if self.cloud.busy == "upload" {
            return;
        }
        let c = self.settings.backup.cloud.clone();
        let p = c.provider();
        let keep = c.keep as usize;
        if c.by_account() {
            let Some((p, app, refresh, cached)) = self.cloud_account() else { return };
            let repo = c.github_repo.clone();
            self.cloud.busy = "upload".into();
            self.cloud_status(&format!("Uploading the backup to {}…", p.name()));
            self.spawn(move |tx, http| {
                let mut tokens = None;
                let r = cloud::access(&*http, &app, cached, &refresh).and_then(|(token, t)| {
                    tokens = t;
                    let gh = p == Provider::GitHub;
                    let up = if gh { cloud::github::upload(&*http, &token, &repo, &file)? } else { cloud::upload(&*http, p, &token, &file)? };
                    // the oldest copies online go; a failure there does not undo the upload
                    let pruned = if gh { cloud::github::prune(&*http, &token, &repo, keep) } else { cloud::prune(&*http, p, &token, keep) };
                    if let Err(e) = pruned {
                        crate::debug::log("cloud", &format!("old backups not removed: {}", e));
                    }
                    Ok(up)
                });
                send(tx, Msg::Cloud(Done::Uploaded(r, tokens)));
            });
        } else {
            let Some(folder) = self.cloud_folder() else {
                let e = format!("No {} sync folder was found. Install the {} desktop program, or choose the folder by hand.", p.name(), p.name());
                self.settings.backup.cloud.last_error = e.clone();
                self.save_settings();
                self.cloud_status(&e);
                if by_hand {
                    self.toast(&e);
                }
                return;
            };
            self.cloud.busy = "upload".into();
            self.cloud_status(&format!("Copying the backup to {}…", folder.display()));
            self.spawn(move |tx, _| {
                send(tx, Msg::Cloud(Done::Uploaded(cloud::folder_upload(&file, &folder, keep), None)));
            });
        }
    }

    /// Reads the list of backups online.
    fn cloud_list(&mut self) {
        let c = self.settings.backup.cloud.clone();
        if c.by_account() {
            let Some((p, app, refresh, cached)) = self.cloud_account() else { return };
            let repo = c.github_repo.clone();
            self.cloud.busy = "list".into();
            self.cloud_status(&format!("Reading the backups on {}…", p.name()));
            self.spawn(move |tx, http| {
                let mut tokens = None;
                let r = cloud::access(&*http, &app, cached, &refresh).and_then(|(token, t)| {
                    tokens = t;
                    if p == Provider::GitHub {
                        cloud::github::list(&*http, &token, &repo)
                    } else {
                        cloud::list(&*http, p, &token)
                    }
                });
                send(tx, Msg::Cloud(Done::Listed(r, tokens)));
            });
        } else {
            let r = match self.cloud_folder() {
                Some(f) => Ok(cloud::folder_list(&f)),
                None => Err(format!("No {} sync folder was found on this computer.", c.provider().name())),
            };
            self.on_cloud(Done::Listed(r, None));
        }
    }

    /// Restores a backup kept online (downloaded first into the backups folder; a sync folder file is used as it is).
    fn cloud_restore(&mut self, id: &str, name: &str) {
        let c = self.settings.backup.cloud.clone();
        if !c.by_account() {
            let path = PathBuf::from(id);
            // only a file of the list (inside the sync folder) is restored
            if self.cloud.remote.iter().any(|r| r.id == id) && path.is_file() {
                self.restore_backup(path);
            } else {
                self.cloud_status("That backup is no longer in the sync folder.");
            }
            return;
        }
        let Some(remote) = self.cloud.remote.iter().find(|r| r.id == id).cloned() else {
            self.cloud_status("That backup is no longer in the list. Read the list again.");
            return;
        };
        let Some((p, app, refresh, cached)) = self.cloud_account() else { return };
        // the name comes from the cloud: only its last part, and only a backup name, is used for the file
        let file = std::path::Path::new(if name.is_empty() { remote.name.as_str() } else { name }).file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| backup::is_backup_name(n)).unwrap_or_else(|| format!("{}download{}", backup::PREFIX, backup::EXT));
        let dest = self.data_dir.join("backups").join("downloaded").join(file);
        let repo = c.github_repo.clone();
        self.cloud.busy = "fetch".into();
        self.cloud_status(&format!("Downloading {} from {}…", remote.name, p.name()));
        self.spawn(move |tx, http| {
            let mut tokens = None;
            let r = cloud::access(&*http, &app, cached, &refresh).and_then(|(token, t)| {
                tokens = t;
                if let Some(d) = dest.parent() {
                    std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
                }
                if p == Provider::GitHub {
                    cloud::github::download(&*http, &token, &repo, &remote, &dest)?;
                } else {
                    cloud::download(&*http, p, &token, &remote, &dest)?;
                }
                // a quick look before anything is replaced: is it a backup at all?
                backup::read_manifest(&dest)?;
                Ok(dest)
            });
            send(tx, Msg::Cloud(Done::Fetched(r, tokens)));
        });
    }

    pub(super) fn on_cloud(&mut self, d: Done) {
        match d {
            Done::Code(p, Ok(code)) => {
                self.cloud.pending = Some(cloud::Pending { provider: p.id().into(), code: code.clone(), started: util::now_unix() });
                self.cloud_status(&format!("Open {} and type the code {}.", code.verify_url, code.user_code));
                self.events.push(Event::OpenUrl(code.verify_url.clone()));
                self.cloud_wait_sign_in(p, code);
            }
            Done::Code(_, Err(e)) => self.cloud_status(&e),
            Done::SignedIn(p, r) => {
                self.cloud.pending = None;
                match r {
                    Ok((t, account)) => {
                        self.settings.backup.cloud.set_login(p, &t.refresh, &account);
                        self.cloud.access = Some((p.id().to_string(), t.access.clone(), util::now_unix() + t.expires_in));
                        self.save_settings();
                        let who = if account.is_empty() { String::new() } else { format!(" as {}", account) };
                        self.cloud_status(&format!("Signed in to {}{}.", p.name(), who));
                        if t.refresh.is_empty() {
                            self.cloud_status("Signed in, but no refresh token came back: the sign-in will last only an hour.");
                        }
                        self.cloud_list();
                    }
                    Err(e) => self.cloud_status(&e),
                }
            }
            Done::GitHub(r) => {
                self.cloud.busy.clear();
                match r {
                    Ok((token, login, repo)) => {
                        let c = &mut self.settings.backup.cloud;
                        c.provider = Provider::GitHub.id().into();
                        c.method = "account".into();
                        c.github_token = token;
                        c.github_account = login.clone();
                        c.github_repo = repo.clone();
                        c.enabled = true;
                        self.cloud.remote.clear();
                        self.cloud.listed = false;
                        self.save_settings();
                        self.cloud_status("Connected to GitHub. The backups go to its private repository.");
                        self.cloud_list();
                    }
                    Err(e) => self.cloud_status(&e),
                }
            }
            Done::Uploaded(r, tokens) => {
                self.cloud.busy.clear();
                self.cloud_keep_tokens(tokens);
                let name = self.settings.backup.cloud.provider().name();
                match r {
                    Ok(remote) => {
                        let c = &mut self.settings.backup.cloud;
                        c.last_upload = util::now_unix();
                        c.last_name = remote.name.clone();
                        c.last_size = remote.size;
                        c.last_error.clear();
                        self.save_settings();
                        self.cloud_status(&format!("Backup sent to {}: {} ({}).", name, remote.name, torrent::human_bytes(remote.size)));
                        if self.cloud.listed {
                            self.cloud_list();
                        }
                    }
                    Err(e) => {
                        self.settings.backup.cloud.last_error = e.clone();
                        self.save_settings();
                        self.cloud_status(&format!("The backup was not sent to {}: {}", name, e));
                        self.toast(&format!("Online backup failed: {}", e));
                    }
                }
            }
            Done::Listed(r, tokens) => {
                self.cloud.busy.clear();
                self.cloud_keep_tokens(tokens);
                match r {
                    Ok(list) => {
                        let n = list.len();
                        self.cloud.remote = list;
                        self.cloud.listed = true;
                        self.cloud.listed_for = self.cloud_list_key();
                        self.cloud_status(&format!("{} backups online.", n));
                    }
                    Err(e) => self.cloud_status(&e),
                }
            }
            Done::Fetched(r, tokens) => {
                self.cloud.busy.clear();
                self.cloud_keep_tokens(tokens);
                match r {
                    Ok(path) => self.restore_backup(path),
                    Err(e) => {
                        self.cloud_status(&e);
                        self.toast(&e);
                    }
                }
            }
        }
    }

    /// What a list of backups online belongs to: the provider and the method.
    fn cloud_list_key(&self) -> String {
        let c = &self.settings.backup.cloud;
        format!("{}/{}/{}/{}", c.provider().id(), c.by_account(), c.folder, c.github_repo)
    }

    /// The online copy for the Preferences window (`extra.cloud` of the "prefs" state).
    pub fn cloud_state(&self) -> Json {
        let c = &self.settings.backup.cloud;
        let p = c.provider();
        let current = self.cloud.listed_for == self.cloud_list_key();
        let det = |q: Provider| Json::Arr(self.cloud.detected(q).iter().map(|f| Json::Str(f.display().to_string())).collect());
        let pending = match &self.cloud.pending {
            Some(x) => Json::obj(vec![
                ("code", x.code.user_code.clone().into()),
                ("url", x.code.verify_url.clone().into()),
                ("until", ((x.started + x.code.expires_in) as f64).into()),
            ]),
            None => Json::Null,
        };
        Json::obj(vec![
            ("provider", p.id().into()),
            ("providerName", p.name().into()),
            ("byAccount", c.by_account().into()),
            ("folder", self.cloud_folder().map(|f| f.join(cloud::FOLDER).display().to_string()).unwrap_or_default().into()),
            ("detected", Json::obj(vec![("gdrive", det(Provider::Google)), ("onedrive", det(Provider::OneDrive)), ("icloud", det(Provider::ICloud))])),
            ("signedIn", (!c.refresh_token(p).is_empty()).into()),
            ("account", c.account(p).to_string().into()),
            ("registerUrl", cloud::register_url(p).into()),
            ("github", Json::obj(vec![("repo", c.github_repo.clone().into()), ("account", c.github_account.clone().into()), ("connected", (!c.github_token.is_empty()).into()), ("defaultRepo", cloud::github::DEFAULT_REPO.into())])),
            ("busy", self.cloud.busy.clone().into()),
            ("status", self.cloud.status.clone().into()),
            ("pending", pending),
            ("listed", (self.cloud.listed && current).into()),
            ("remote", Json::Arr(if current { self.cloud.remote.iter().map(|r| r.to_json()).collect() } else { vec![] })),
        ])
    }
}
