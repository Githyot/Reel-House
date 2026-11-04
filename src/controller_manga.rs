//! The Images mode: browsing sources, the manga library, the series page and the reader.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::manga::{self, Browse, Chapter, HistoryItem, Series};
use crate::readcache;
use crate::suwayomi;

/// The list on the Explore page.
#[derive(Clone, Debug)]
pub struct MangaList {
    pub what: Browse,
    pub items: Vec<Series>,
    pub offset: usize,
    pub more: bool,
    pub loading: bool,
    pub error: String,
    pub(super) gen: u64,
}

/// The series page.
#[derive(Clone, Debug)]
pub struct MangaView {
    pub series: Series,
    pub chapters: Vec<Chapter>,
    pub loading: bool,
    /// Shown from the cache while a newer copy is being fetched.
    pub refreshing: bool,
    pub error: String,
    pub(super) gen: u64,
}

/// The chapter being read.
#[derive(Clone, Debug)]
pub struct Reader {
    pub series: Series,
    pub chapter: Chapter,
    pub pages: Vec<String>,
    pub loading: bool,
    pub error: String,
    gen: u64,
}

/// When the library last looked for new chapters (see `manga_refresh_library_due`).
static LAST_LIBRARY_REFRESH: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

impl MangaList {
    pub fn new() -> MangaList {
        MangaList { what: Browse::Popular, items: vec![], offset: 0, more: false, loading: false, error: String::new(), gen: 0 }
    }
}

impl Core {
    // ----- Explore -----

    /// Starts a list from the beginning: popular, latest, or a search.
    pub fn manga_browse(&mut self, what: Browse) {
        self.manga_gen += 1;
        self.manga_list = MangaList { what, items: vec![], offset: 0, more: false, loading: false, error: String::new(), gen: self.manga_gen };
        self.manga_more();
    }

    /// Loads the next page of the list on screen. A page read less than `LIST_TTL` ago comes from the cache at once;
    /// an older one is asked for again, and is still shown when the source cannot be reached.
    pub fn manga_more(&mut self) {
        if self.manga_list.loading || (self.manga_list.offset > 0 && !self.manga_list.more) {
            return;
        }
        let (what, offset, gen) = (self.manga_list.what.clone(), self.manga_list.offset, self.manga_list.gen);
        let lang = self.settings.general.manga_lang.clone();
        let source = self.settings.general.manga_source.clone();
        let store = self.images_cache();
        let key = list_key(&source, &lang, &what, offset);
        let cached = store.get(&key);
        if let Some(l) = cached.as_ref().filter(|h| readcache::fresh(h.age, readcache::LIST_TTL)).and_then(|h| manga::list_from_json(&h.data)) {
            self.on_manga_list(gen, offset, Ok(l));
            return;
        }
        let stale = cached.and_then(|h| manga::list_from_json(&h.data));
        let on_server = !(source == "mangadex" || source.is_empty());
        if !crate::connectivity::online() {
            match stale {
                Some(l) => self.on_manga_list(gen, offset, Ok(l)),
                None => {
                    self.manga_list.error = "No internet connection. Your library and the chapters read before still open.".to_string();
                    self.events.push(Event::MangaList);
                }
            }
            return;
        }
        if on_server && !self.suwayomi_ready() && self.suwayomi_booting {
            // asked again by `on_suwayomi_up` when the server answers, instead of a "could not reach" error
            match stale {
                Some(l) => self.on_manga_list(gen, offset, Ok(l)),
                None => {
                    self.manga_list.error = "The Suwayomi server is starting…".to_string();
                    self.events.push(Event::MangaList);
                }
            }
            return;
        }
        self.manga_list.loading = true;
        self.manga_list.error.clear();
        self.events.push(Event::MangaList);
        let srv = self.suwayomi_server();
        self.spawn(move |tx, http| {
            let r = if source == "mangadex" || source.is_empty() {
                manga::browse(&*http, &what, offset, &lang)
            } else {
                let (kind, q) = match &what {
                    Browse::Popular => (suwayomi::Kind::Popular, String::new()),
                    Browse::Latest => (suwayomi::Kind::Latest, String::new()),
                    Browse::Search(q) => (suwayomi::Kind::Search, q.clone()),
                };
                suwayomi::browse(&*http, &srv, &source, kind, offset / manga::PAGE + 1, &q)
            };
            let r = match r {
                Ok((items, more)) => {
                    if !items.is_empty() {
                        store.put(&key, manga::list_to_json(&items, more));
                    }
                    Ok((items, more))
                }
                // what was read before is better than an error (offline, the server still starting)
                Err(e) => stale.ok_or(e),
            };
            send(tx, Msg::MangaList(gen, offset, r));
        });
    }

    pub(super) fn on_manga_list(&mut self, gen: u64, offset: usize, r: Result<(Vec<Series>, bool), String>) {
        if gen != self.manga_list.gen || offset != self.manga_list.offset {
            return;
        }
        self.manga_list.loading = false;
        self.manga_list.error.clear();
        match r {
            Ok((items, more)) => {
                self.manga_list.offset += manga::PAGE;
                self.manga_list.more = more;
                for s in items {
                    if !self.manga_list.items.iter().any(|x| x.id == s.id) {
                        self.manga_list.items.push(s);
                    }
                }
            }
            Err(e) => self.manga_list.error = e,
        }
        self.events.push(Event::MangaList);
    }

    /// The cache of what the Images mode read from its sources (lists, series, chapter pictures).
    pub(super) fn images_cache(&self) -> readcache::Store {
        readcache::prune_once(&self.data_dir);
        readcache::Store::of(&self.data_dir, "images")
    }

    // ----- Suwayomi -----

    pub fn suwayomi_server(&self) -> suwayomi::Server {
        let g = &self.settings.general;
        if let Some(port) = self.ext_host_port.filter(|_| g.suwayomi_managed) {
            return suwayomi::Server::local_host(port, self.suwayomi_repos(), self.ext_host_dir.clone());
        }
        if g.suwayomi_managed {
            return suwayomi::Server::new(&format!("http://127.0.0.1:{}", self.suwayomi_port), "", "");
        }
        suwayomi::Server::new(&g.suwayomi_url, &g.suwayomi_user, &g.suwayomi_pass)
    }

    /// True when the server (ours or the user's) can be asked things.
    pub fn suwayomi_ready(&self) -> bool {
        !self.settings.general.suwayomi_managed || self.suwayomi_host.is_some() || self.ext_host_port.is_some()
    }

    /// Android runs the extensions itself (no Suwayomi there): the host started by the app is used at once.
    fn uses_ext_host(&self) -> bool {
        cfg!(target_os = "android") && self.settings.general.suwayomi_managed
    }

    /// Called once the Android app's extension host listens (bridge command "ext_host").
    pub fn ext_host_up(&mut self, port: u16, dir: &str) {
        crate::debug::log("extensions", &format!("extension host on port {} ({})", port, dir));
        self.ext_host_dir = std::path::PathBuf::from(dir);
        self.ext_host_port = if port > 0 { Some(port) } else { None };
        if port == 0 {
            self.suwayomi_status = "The extension host of the app did not start.".to_string();
            self.events.push(Event::MangaSources);
            return;
        }
        if self.suwayomi_boot_wanted || self.mode() == "images" {
            self.suwayomi_connect();
        }
    }

    /// Starts the server of the app when the Images mode wants it: downloaded once, then started hidden.
    pub fn suwayomi_boot(&mut self) {
        if !self.settings.general.suwayomi_managed || self.suwayomi_host.is_some() || self.suwayomi_booting {
            return;
        }
        if self.uses_ext_host() {
            // nothing to download or start: the app's own host answers once it listens (`ext_host_up`)
            if self.ext_host_port.is_none() {
                self.suwayomi_status = "Starting the extensions…".to_string();
                self.events.push(Event::MangaSources);
            }
            return;
        }
        self.suwayomi_booting = true;
        self.suwayomi_idle = None;
        self.suwayomi_port = free_port().unwrap_or(suwayomi::DEFAULT_PORT);
        self.suwayomi_status = "Starting the Suwayomi server…".to_string();
        self.events.push(Event::MangaSources);
        let (dir, port, repos) = (self.data_dir.join("suwayomi"), self.suwayomi_port, self.suwayomi_repos());
        self.spawn(move |tx, http| {
            let status = |t: String| send(tx, Msg::SuwayomiStatus(t));
            let r = (|| {
                let bundle = crate::suwayomi_host::install(&*http, &dir, &status)?;
                status("Starting the Suwayomi server… (the first start takes a minute)".to_string());
                crate::suwayomi_host::launch(&*http, &dir, &bundle, port, &repos, &status)
            })();
            send(tx, Msg::SuwayomiUp(r));
        });
    }

    pub(super) fn on_suwayomi_up(&mut self, r: Result<crate::suwayomi_host::Host, String>) {
        crate::debug::log("suwayomi", &match &r {
            Ok(h) => format!("server up on port {}", h.port),
            Err(e) => format!("server failed to start: {}", e),
        });
        self.suwayomi_booting = false;
        match r {
            Ok(h) => {
                self.suwayomi_port = h.port;
                self.suwayomi_host = Some(h);
                self.suwayomi_status = "Server ready. Adding the repositories…".to_string();
                let srv = self.suwayomi_server();
                let repos = self.suwayomi_repos();
                self.spawn(move |tx, http| {
                    send(tx, Msg::SuwayomiSources(suwayomi::sources(&*http, &srv)));
                    read_extensions(tx, &*http, &srv, &repos);
                });
                // a list that waited for the server is read now
                let src = self.settings.general.manga_source.clone();
                if src != "mangadex" && !src.is_empty() && self.manga_list.items.is_empty() && !self.manga_list.loading {
                    self.manga_browse(self.manga_list.what.clone());
                }
            }
            Err(e) => {
                self.suwayomi_boot_wanted = false;
                self.suwayomi_status = e;
            }
        }
        self.events.push(Event::MangaSources);
    }

    /// Called every tick: the server runs only while the Images mode is in use (a few minutes of grace when leaving it).
    pub fn suwayomi_tick(&mut self) {
        if !self.settings.general.suwayomi_managed || self.uses_ext_host() {
            return;
        }
        if self.mode() == "images" {
            self.suwayomi_idle = None;
            if self.suwayomi_host.is_none() && !self.suwayomi_booting && self.suwayomi_boot_wanted {
                self.suwayomi_boot();
            }
        } else if self.suwayomi_host.is_some() {
            let since = *self.suwayomi_idle.get_or_insert_with(std::time::Instant::now);
            if since.elapsed().as_secs() >= 180 {
                self.suwayomi_host = None;
                self.suwayomi_idle = None;
                self.manga_sources.clear();
                self.suwayomi_exts.clear();
                self.suwayomi_status = "Stopped (the Images mode is not in use).".to_string();
                self.events.push(Event::MangaSources);
            }
        }
    }

    /// Reads the sources the server has (starting the server of the app first when needed).
    pub fn suwayomi_connect(&mut self) {
        self.suwayomi_boot_wanted = true;
        if self.settings.general.suwayomi_managed && self.suwayomi_host.is_none() && self.ext_host_port.is_none() {
            self.suwayomi_boot();
            return;
        }
        self.suwayomi_status = "Connecting…".to_string();
        self.events.push(Event::MangaSources);
        let srv = self.suwayomi_server();
        self.spawn(move |tx, http| {
            send(tx, Msg::SuwayomiSources(suwayomi::sources(&*http, &srv)));
            send(tx, Msg::SuwayomiExts(suwayomi::extensions(&*http, &srv)));
        });
    }

    /// Stops the server of the app and starts it again.
    pub fn suwayomi_restart(&mut self) {
        self.suwayomi_host = None;
        self.manga_sources.clear();
        self.suwayomi_connect();
    }

    pub(super) fn on_suwayomi_sources(&mut self, r: Result<Vec<suwayomi::Source>, String>) {
        match r {
            Ok(list) => {
                self.suwayomi_status = format!("Connected. {} sources installed.", list.len());
                self.manga_sources = list;
            }
            Err(e) => {
                self.suwayomi_status = e;
                self.manga_sources.clear();
            }
        }
        // A source that is gone goes back to MangaDex.
        let cur = self.settings.general.manga_source.clone();
        if cur != "mangadex" && !self.manga_sources.iter().any(|s| s.id == cur) && !self.manga_sources.is_empty() {
            self.settings.general.manga_source = "mangadex".to_string();
            self.save_settings();
        }
        self.events.push(Event::MangaSources);
    }

    /// Chooses what Explore lists: "mangadex" or the id of a source of the server.
    pub fn manga_set_source(&mut self, id: &str) {
        if self.settings.general.manga_source != id {
            self.settings.general.manga_source = id.to_string();
            self.save_settings();
        }
        self.manga_browse(Browse::Popular);
    }

    /// The repositories in use (the user's list).
    pub fn suwayomi_repos(&self) -> Vec<String> {
        self.settings.general.suwayomi_repos.clone()
    }

    /// Gives the server the repositories of the list and reads what they offer.
    pub fn suwayomi_add_repos(&mut self) {
        self.suwayomi_status = "Adding the repositories…".to_string();
        self.events.push(Event::MangaSources);
        let (srv, repos) = (self.suwayomi_server(), self.suwayomi_repos());
        crate::debug::log("suwayomi", &format!("repositories: {}", repos.join(" ")));
        self.spawn(move |tx, http| read_extensions(tx, &*http, &srv, &repos));
    }

    /// Adds a repository typed by the user.
    pub fn suwayomi_add_repo(&mut self, text: &str) -> Result<(), String> {
        let url = suwayomi::normalize_repo(text)?;
        if self.settings.general.suwayomi_repos.iter().any(|r| r == &url) {
            return Err("That repository is already in the list.".to_string());
        }
        self.settings.general.suwayomi_repos.push(url);
        self.save_settings();
        self.suwayomi_add_repos();
        Ok(())
    }

    pub fn suwayomi_remove_repo(&mut self, url: &str) {
        self.settings.general.suwayomi_repos.retain(|r| r != url);
        self.save_settings();
        self.suwayomi_add_repos();
    }

    pub fn suwayomi_list_exts(&mut self) {
        self.suwayomi_status = "Reading the extensions…".to_string();
        self.events.push(Event::MangaSources);
        let srv = self.suwayomi_server();
        self.spawn(move |tx, http| send(tx, Msg::SuwayomiExts(suwayomi::extensions(&*http, &srv))));
    }

    pub(super) fn on_suwayomi_exts(&mut self, r: Result<Vec<suwayomi::Extension>, String>) {
        crate::debug::log("suwayomi", &match &r {
            Ok(l) => format!("{} extensions offered", l.len()),
            Err(e) => format!("extensions failed: {}", e),
        });
        match r {
            Ok(mut l) => {
                l.sort_by(|a, b| b.installed.cmp(&a.installed).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
                self.suwayomi_status = format!("{} extensions offered by the repositories.", l.len());
                self.suwayomi_exts = l;
            }
            Err(e) => self.suwayomi_status = e,
        }
        self.events.push(Event::MangaSources);
    }

    pub fn suwayomi_install(&mut self, pkg: &str) {
        self.toast("Installing…");
        let (srv, pkg) = (self.suwayomi_server(), pkg.to_string());
        self.spawn(move |tx, http| {
            let r = suwayomi::install(&*http, &srv, &pkg);
            send(tx, Msg::SuwayomiDone(match r {
                Ok(()) => String::new(),
                Err(e) => e,
            }));
            if let Ok(list) = suwayomi::sources(&*http, &srv) {
                send(tx, Msg::SuwayomiSources(Ok(list)));
            }
            send(tx, Msg::SuwayomiExts(suwayomi::extensions(&*http, &srv)));
        });
    }

    /// Installs several extensions one after the other (all of a language, say), telling how far it is.
    pub fn suwayomi_install_many(&mut self, pkgs: Vec<String>) {
        if pkgs.is_empty() {
            return;
        }
        self.toast(&format!("Installing {} extensions… this takes a while.", pkgs.len()));
        let srv = self.suwayomi_server();
        self.spawn(move |tx, http| {
            let total = pkgs.len();
            let mut failed = 0;
            for (i, pkg) in pkgs.iter().enumerate() {
                send(tx, Msg::SuwayomiStatus(format!("Installing extensions: {} of {}", i + 1, total)));
                if suwayomi::install(&*http, &srv, pkg).is_err() {
                    failed += 1;
                }
                // The sources appear as they come.
                if i % 10 == 9 {
                    if let Ok(list) = suwayomi::sources(&*http, &srv) {
                        send(tx, Msg::SuwayomiSources(Ok(list)));
                    }
                }
            }
            send(tx, Msg::SuwayomiDone(if failed == 0 { String::new() } else { format!("{} of {} did not install", failed, total) }));
            if let Ok(list) = suwayomi::sources(&*http, &srv) {
                send(tx, Msg::SuwayomiSources(Ok(list)));
            }
            send(tx, Msg::SuwayomiExts(suwayomi::extensions(&*http, &srv)));
        });
    }

    pub fn suwayomi_uninstall(&mut self, pkg: &str) {
        let (srv, pkg) = (self.suwayomi_server(), pkg.to_string());
        self.spawn(move |tx, http| {
            let _ = suwayomi::uninstall(&*http, &srv, &pkg);
            if let Ok(list) = suwayomi::sources(&*http, &srv) {
                send(tx, Msg::SuwayomiSources(Ok(list)));
            }
            send(tx, Msg::SuwayomiExts(suwayomi::extensions(&*http, &srv)));
        });
    }

    pub(super) fn on_suwayomi_done(&mut self, err: String) {
        if err.is_empty() {
            self.toast("Installed.");
        } else {
            self.toast(&format!("Could not install: {}", err));
        }
    }

    // ----- library -----

    pub fn manga_in_library(&self, id: &str) -> bool {
        self.manga.in_library(id)
    }

    pub fn manga_toggle_library(&mut self, s: &Series) {
        if let Some(i) = self.manga.library.iter().position(|x| x.id == s.id) {
            self.manga.library.remove(i);
        } else {
            let mut item = s.clone();
            if let Some(v) = self.manga_view.as_ref().filter(|v| v.series.id == s.id) {
                item.chapter_count = v.chapters.len() as u32;
            }
            self.manga.library.push(item);
        }
        self.save_manga();
        self.events.push(Event::MangaLibrary);
        self.events.push(Event::MangaView);
    }

    pub fn manga_set_categories(&mut self, id: &str, cats: Vec<String>) {
        if let Some(s) = self.manga.library.iter_mut().find(|s| s.id == id) {
            s.categories = cats;
            self.save_manga();
            self.events.push(Event::MangaLibrary);
        }
    }

    pub fn manga_add_category(&mut self, name: &str) {
        let name = name.trim();
        if !name.is_empty() && !self.manga.categories.iter().any(|c| c == name) {
            self.manga.categories.push(name.to_string());
            self.save_manga();
            self.events.push(Event::MangaLibrary);
        }
    }

    pub fn manga_remove_category(&mut self, name: &str) {
        self.manga.categories.retain(|c| c != name);
        for s in self.manga.library.iter_mut() {
            s.categories.retain(|c| c != name);
        }
        self.save_manga();
        self.events.push(Event::MangaLibrary);
    }

    /// A cover chosen by the person (a picture file): kept in the data folder, "covers", so it stays when the source changes.
    /// An empty `file` goes back to the cover of the source (its address was kept beside the copy).
    /// Works for both shelves (`novel`); the library entry, the history and the open series page change.
    pub fn set_series_cover(&mut self, novel: bool, id: &str, file: &str) -> Result<(), String> {
        let dir = self.data_dir.join("covers");
        let stem: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        let orig = dir.join(format!("{}.orig", stem));
        let current = {
            let (lib, view) = if novel { (&self.novels.library, self.novel_view.as_ref()) } else { (&self.manga.library, self.manga_view.as_ref()) };
            lib.iter().find(|s| s.id == id).map(|s| s.cover.clone()).or_else(|| view.filter(|v| v.series.id == id).map(|v| v.series.cover.clone())).unwrap_or_default()
        };
        let new_cover = if file.is_empty() {
            let back = std::fs::read_to_string(&orig).unwrap_or_default();
            let _ = std::fs::remove_file(&orig);
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n.starts_with(&format!("{}.", stem)) && !n.ends_with(".orig") {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
            back.trim().to_string()
        } else {
            let src = std::path::PathBuf::from(crate::util::file_url_path(file));
            let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| e.len() <= 5).unwrap_or_else(|| "jpg".to_string());
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let dest = dir.join(format!("{}.{}", stem, ext));
            std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
            if !orig.exists() && !current.is_empty() && !current.starts_with(&*dir.to_string_lossy()) {
                let _ = std::fs::write(&orig, &current);
            }
            // A new name each time, so the picture shown is not the old one kept in memory.
            let shown = dir.join(format!("{}-{}.{}", stem, util::now_unix(), ext));
            std::fs::rename(&dest, &shown).map_err(|e| e.to_string())?;
            shown.to_string_lossy().replace('\\', "/")
        };
        let state = if novel { &mut self.novels } else { &mut self.manga };
        if let Some(sr) = state.library.iter_mut().find(|s| s.id == id) {
            sr.cover = new_cover.clone();
        }
        for h in state.history.iter_mut().filter(|h| h.series_id == id) {
            h.cover = new_cover.clone();
        }
        let view = if novel { self.novel_view.as_mut() } else { self.manga_view.as_mut() };
        if let Some(v) = view.filter(|v| v.series.id == id) {
            v.series.cover = new_cover;
        }
        if novel {
            self.save_novels();
            self.events.push(Event::NovelLibrary);
            self.events.push(Event::NovelView);
        } else {
            self.save_manga();
            self.events.push(Event::MangaLibrary);
            self.events.push(Event::MangaView);
        }
        Ok(())
    }

    pub(super) fn save_manga(&self) {
        self.manga.save(&self.profile_dir);
    }

    /// Looks for new chapters of the series in the library (the unread numbers follow). The chapter lists read are
    /// kept in the cache too, so the series pages of the library open at once with what is new.
    pub fn manga_refresh_library(&mut self) {
        // offline the server would ask each source and wait: the counts on disk stay until the connection is back
        if self.manga_refreshing || self.manga.library.is_empty() || !crate::connectivity::online() {
            return;
        }
        self.manga_refreshing = true;
        *LAST_LIBRARY_REFRESH.lock().unwrap_or_else(|e| e.into_inner()) = Some(std::time::Instant::now());
        let list: Vec<Series> = self.manga.library.iter().take(60).cloned().collect();
        let lang = self.settings.general.manga_lang.clone();
        let srv = self.suwayomi_server();
        let store = self.images_cache();
        self.spawn(move |tx, http| {
            let out = std::sync::Mutex::new(Vec::<(String, u32)>::new());
            let queue = std::sync::Mutex::new(list);
            std::thread::scope(|sc| {
                for _ in 0..4 {
                    sc.spawn(|| loop {
                        let Some(s) = queue.lock().ok().and_then(|mut q| q.pop()) else { break };
                        let id = s.id.clone();
                        let got = if suwayomi::is_suwayomi(&id) { suwayomi::chapters(&*http, &srv, &id) } else { manga::chapters(&*http, &id, &lang) };
                        if let Ok(list) = got {
                            let key = format!("series|{}", id);
                            // the details of the cached copy are richer than the library's
                            let mut series = store.get(&key).and_then(|h| manga::view_from_json(&h.data)).map(|(c, _)| c).unwrap_or_else(|| s.clone());
                            fill_details(&mut series, &s);
                            series.categories.clear();
                            series.chapter_count = list.len() as u32;
                            store.put(&key, manga::view_to_json(&series, &list));
                            if let Ok(mut o) = out.lock() {
                                o.push((id, list.len() as u32));
                            }
                        }
                    });
                }
            });
            send(tx, Msg::MangaCounts(out.into_inner().unwrap_or_default()));
        });
    }

    /// The same, at most every half hour: entering the Images mode does not ask every source again each time.
    pub fn manga_refresh_library_due(&mut self) {
        let last = *LAST_LIBRARY_REFRESH.lock().unwrap_or_else(|e| e.into_inner());
        if last.map(|t| t.elapsed().as_secs() >= readcache::LIST_TTL).unwrap_or(true) {
            self.manga_refresh_library();
        }
    }

    pub(super) fn on_manga_counts(&mut self, counts: Vec<(String, u32)>) {
        self.manga_refreshing = false;
        let mut changed = false;
        let mut fresh: Vec<(String, String, String, u32)> = Vec::new();
        for (id, n) in counts {
            if let Some(s) = self.manga.library.iter_mut().find(|s| s.id == id) {
                if s.chapter_count != n {
                    // A series that was known before and got more chapters goes to the bell.
                    if s.chapter_count > 0 && n > s.chapter_count {
                        fresh.push((s.id.clone(), s.title.clone(), s.cover.clone(), n - s.chapter_count));
                    }
                    s.chapter_count = n;
                    changed = true;
                }
            }
        }
        for (sid, title, cover, added) in fresh {
            self.note_new_chapters("manga", &sid, &title, &cover, added);
        }
        if changed {
            self.save_manga();
            self.events.push(Event::MangaLibrary);
        }
    }

    // ----- the series page -----

    /// Opens the page of a series. A series read before shows at once from the cache; when that copy is older than
    /// `SERIES_TTL` a new one is fetched in the background and replaces it when it comes.
    pub fn manga_open(&mut self, series: Series) {
        self.clear_notice(&format!("manga:{}", series.id));
        self.manga_gen += 1;
        let gen = self.manga_gen;
        let id = series.id.clone();
        // The library keeps the richer copy.
        let mut shown = self.manga.library.iter().find(|s| s.id == id).cloned().unwrap_or(series);
        let store = self.images_cache();
        let key = format!("series|{}", id);
        let cached = store.get(&key).and_then(|h| manga::view_from_json(&h.data).map(|v| (v, h.age)));
        if let Some(((cs, chapters), age)) = cached {
            fill_details(&mut shown, &cs);
            if !chapters.is_empty() {
                shown.chapter_count = chapters.len() as u32;
            }
            // offline the copy on disk is the answer: asking the server would only make it wait for the source
            let stale = (!readcache::fresh(age, readcache::SERIES_TTL) || chapters.is_empty()) && crate::connectivity::online();
            self.manga_view = Some(MangaView { series: shown, chapters, loading: false, refreshing: stale, error: String::new(), gen });
            self.events.push(Event::MangaView);
            if !stale {
                return;
            }
        } else if !crate::connectivity::online() {
            let error = "No internet connection. This series was never opened with internet, so its chapters are not on this computer.".to_string();
            self.manga_view = Some(MangaView { series: shown, chapters: vec![], loading: false, refreshing: false, error, gen });
            self.events.push(Event::MangaView);
            return;
        } else {
            self.manga_view = Some(MangaView { series: shown.clone(), chapters: vec![], loading: true, refreshing: false, error: String::new(), gen });
            self.events.push(Event::MangaView);
        }
        let need_details = self.manga_view.as_ref().map(|v| v.series.description.is_empty() || v.series.authors.is_empty()).unwrap_or(true);
        let lang = self.settings.general.manga_lang.clone();
        let srv = self.suwayomi_server();
        self.spawn(move |tx, http| {
            let sw = suwayomi::is_suwayomi(&id);
            let details = if !need_details {
                None
            } else if sw {
                suwayomi::series(&*http, &srv, &id).ok()
            } else {
                manga::series(&*http, &id).ok()
            };
            let list = if sw { suwayomi::chapters(&*http, &srv, &id) } else { manga::chapters(&*http, &id, &lang) };
            send(tx, Msg::MangaView(gen, details, list));
        });
    }

    pub fn manga_close(&mut self) {
        self.manga_view = None;
        self.manga_gen += 1;
    }

    pub(super) fn on_manga_view(&mut self, gen: u64, details: Option<Series>, r: Result<Vec<Chapter>, String>) {
        let store = self.images_cache();
        let Some(v) = self.manga_view.as_mut().filter(|v| v.gen == gen) else { return };
        v.loading = false;
        v.refreshing = false;
        if let Some(d) = details {
            let keep = (v.series.categories.clone(), v.series.chapter_count, v.series.cover.clone());
            v.series = d;
            v.series.categories = keep.0;
            v.series.chapter_count = keep.1;
            // a cover the person chose stays
            if keep.2.contains("/covers/") && std::path::Path::new(&keep.2).exists() {
                v.series.cover = keep.2;
            }
        }
        match r {
            Ok(list) => {
                v.chapters = list;
                v.series.chapter_count = v.chapters.len() as u32;
                v.error.clear();
                let copy = v.series.clone();
                // kept for the next opening (the categories belong to the library, not to the source)
                let mut plain = copy.clone();
                plain.categories.clear();
                store.put(&format!("series|{}", copy.id), manga::view_to_json(&plain, &v.chapters));
                if let Some(s) = self.manga.library.iter_mut().find(|s| s.id == copy.id) {
                    s.chapter_count = copy.chapter_count;
                    if s.description.is_empty() {
                        s.description = copy.description.clone();
                    }
                    if s.authors.is_empty() {
                        s.authors = copy.authors.clone();
                    }
                    if s.mal_id.is_none() {
                        s.mal_id = copy.mal_id;
                    }
                    self.save_manga();
                    self.events.push(Event::MangaLibrary);
                }
                self.link_mal(copy);
            }
            // the copy from the cache stays on screen when the new one could not be read
            Err(e) => {
                if v.chapters.is_empty() {
                    v.error = e;
                }
            }
        }
        self.events.push(Event::MangaView);
    }

    pub fn manga_is_read(&self, series: &str, chapter: &str) -> bool {
        self.manga.is_read(series, chapter)
    }

    /// Marks one chapter as read or not read.
    pub fn manga_mark(&mut self, series: &str, chapter: &str, read: bool) {
        let p = self.manga.progress.entry(series.to_string()).or_default();
        p.read.retain(|c| c != chapter);
        if read {
            p.read.push(chapter.to_string());
        }
        self.save_manga();
        self.events.push(Event::MangaView);
        self.events.push(Event::MangaLibrary);
    }

    /// Marks every chapter of the series on screen as read or not read.
    pub fn manga_mark_all(&mut self, read: bool) {
        let Some(v) = self.manga_view.as_ref() else { return };
        let (id, ids): (String, Vec<String>) = (v.series.id.clone(), v.chapters.iter().map(|c| c.id.clone()).collect());
        let p = self.manga.progress.entry(id).or_default();
        p.read = if read { ids } else { vec![] };
        if !read {
            p.chapter.clear();
            p.page = 0;
        }
        self.save_manga();
        self.events.push(Event::MangaView);
        self.events.push(Event::MangaLibrary);
    }

    /// Marks every chapter before this one as read.
    pub fn manga_mark_before(&mut self, chapter: &str) {
        let Some(v) = self.manga_view.as_ref() else { return };
        let Some(at) = v.chapters.iter().position(|c| c.id == chapter) else { return };
        let (id, ids): (String, Vec<String>) = (v.series.id.clone(), v.chapters[..at].iter().map(|c| c.id.clone()).collect());
        let p = self.manga.progress.entry(id).or_default();
        for i in ids {
            if !p.read.contains(&i) {
                p.read.push(i);
            }
        }
        self.save_manga();
        self.events.push(Event::MangaView);
        self.events.push(Event::MangaLibrary);
    }

    // ----- the reader -----

    /// Opens a chapter of the series on screen.
    pub fn manga_read(&mut self, chapter: Chapter) {
        let Some(series) = self.manga_view.as_ref().map(|v| v.series.clone()) else { return };
        self.manga_gen += 1;
        let gen = self.manga_gen;
        // Pages of the chapter that was left no longer wait for the upscaling, the text reading or the translation.
        crate::mangaup::new_generation();
        self.reader_changed();
        let id = chapter.id.clone();
        let resume = self.manga.progress.get(&series.id).filter(|p| p.chapter == id).map(|p| p.page).unwrap_or(0);
        self.manga_resume = resume;
        self.manga.note_history(HistoryItem { series_id: series.id.clone(), title: series.title.clone(), cover: series.cover.clone(), chapter_id: id.clone(), label: chapter.label(), at: util::now_unix() });
        self.manga.progress.entry(series.id.clone()).or_default().chapter = id.clone();
        self.save_manga();
        self.manga_reader = Some(Reader { series, chapter, pages: vec![], loading: true, error: String::new(), gen });
        self.pagedb_open();
        self.events.push(Event::MangaPages);
        let saver = self.settings.general.manga_saver;
        let srv = self.suwayomi_server();
        let (store, key) = (self.images_cache(), pages_key(&id, saver));
        let cached = store.get(&key).map(|h| (pages_from(&srv, &h.data), h.age));
        let ttl = pages_ttl(&id);
        let offline = !crate::connectivity::online();
        // offline, any copy is used: the pictures read before are kept on disk under their addresses (pictures/)
        if let Some((p, _)) = cached.as_ref().filter(|(p, age)| !p.is_empty() && (offline || readcache::fresh(*age, ttl))) {
            self.on_manga_pages(gen, Ok(p.clone()));
            return;
        }
        if offline {
            let error = "No internet connection. This chapter was never opened with internet, so its pages are not on this computer.".to_string();
            self.on_manga_pages(gen, Err(error));
            return;
        }
        // the old answer still works when the source cannot be asked: the manga server's addresses stay valid, and
        // the MangaDex pictures read before are on disk under their (expired) addresses
        let stale = cached.map(|(p, _)| p).filter(|p| !p.is_empty());
        self.spawn(move |tx, http| {
            let r = fetch_pages(&*http, &srv, &id, saver, &store, &key);
            send(tx, Msg::MangaPages(gen, r.or_else(|e| stale.ok_or(e))));
        });
    }

    pub fn manga_resume_page(&self) -> u32 {
        self.manga_resume
    }

    pub(super) fn on_manga_pages(&mut self, gen: u64, r: Result<Vec<String>, String>) {
        let Some(rd) = self.manga_reader.as_mut().filter(|r| r.gen == gen) else { return };
        rd.loading = false;
        let ok = r.is_ok();
        match r {
            Ok(p) => rd.pages = p,
            Err(e) => rd.error = e,
        }
        self.events.push(Event::MangaPages);
        if ok {
            self.prefetch_next_pages();
        }
    }

    /// The list of pictures of the next chapter is read ahead, so going on is instant.
    fn prefetch_next_pages(&mut self) {
        let Some(next) = self.manga_neighbour(1) else { return };
        let saver = self.settings.general.manga_saver;
        let (store, key) = (self.images_cache(), pages_key(&next.id, saver));
        if !crate::connectivity::online() || store.get(&key).map(|h| readcache::fresh(h.age, pages_ttl(&next.id))).unwrap_or(false) {
            return;
        }
        let srv = self.suwayomi_server();
        self.spawn(move |_tx, http| {
            let _ = fetch_pages(&*http, &srv, &next.id, saver, &store, &key);
        });
    }

    pub fn manga_close_reader(&mut self) {
        // the page reached is only written every few pages: closing must not lose it
        if self.manga_reader.take().is_some() {
            self.save_manga();
        }
        self.manga_gen += 1;
        self.reader_changed();
    }

    /// The reader is on page `page` (0 based) of the chapter.
    pub fn manga_page(&mut self, page: u32) {
        let Some(rd) = self.manga_reader.as_ref() else { return };
        let (sid, cid) = (rd.series.id.clone(), rd.chapter.id.clone());
        let p = self.manga.progress.entry(sid).or_default();
        p.chapter = cid;
        p.page = page;
        // Written now and then, not on every page.
        self.manga_page_saves += 1;
        if self.manga_page_saves % 8 == 0 {
            self.save_manga();
        }
    }

    /// The last page was reached: the chapter counts as read, and MyAnimeList is told.
    pub fn manga_finished(&mut self) {
        let Some(rd) = self.manga_reader.as_ref() else { return };
        let (series, chapter) = (rd.series.clone(), rd.chapter.clone());
        if self.manga.is_read(&series.id, &chapter.id) {
            return;
        }
        let p = self.manga.progress.entry(series.id.clone()).or_default();
        p.read.push(chapter.id.clone());
        p.page = 0;
        self.save_manga();
        self.events.push(Event::MangaView);
        self.events.push(Event::MangaLibrary);
        let count = self.manga.progress.get(&series.id).map(|p| p.read.len() as u32).unwrap_or(1);
        self.track_mal(series, chapter, count);
    }

    /// The chapter after (+1) or before (-1) the one being read.
    pub fn manga_neighbour(&self, delta: i32) -> Option<Chapter> {
        let rd = self.manga_reader.as_ref()?;
        let v = self.manga_view.as_ref()?;
        let at = v.chapters.iter().position(|c| c.id == rd.chapter.id)? as i32 + delta;
        if at < 0 {
            return None;
        }
        v.chapters.get(at as usize).cloned()
    }
}

/// Gives the server the repositories and reads what they offer. The server downloads them from the internet, so the
/// first answers may be empty: it asks again a few times. Why it ended the way it did goes into the status.
fn read_extensions(tx: &Sender<Msg>, http: &dyn Http, srv: &suwayomi::Server, repos: &[String]) {
    let set = suwayomi::set_repos(http, srv, repos);
    send(tx, Msg::SuwayomiStatus("Reading the repositories (the server downloads them)…".to_string()));
    let mut last = Ok(vec![]);
    for attempt in 0..4 {
        last = suwayomi::extensions(http, srv);
        if matches!(&last, Ok(v) if !v.is_empty()) {
            break;
        }
        if attempt < 3 {
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    }
    if let (Err(e), Ok(v)) = (&set, &last) {
        if v.is_empty() {
            last = Err(format!("The server did not accept the repositories ({})", e));
        }
    }
    if repos.is_empty() {
        last = Err("No repository in the list. Add one below.".to_string());
    }
    send(tx, Msg::SuwayomiExts(last));
}

/// The cache key of a page of a list.
fn list_key(source: &str, lang: &str, what: &Browse, offset: usize) -> String {
    let w = match what {
        Browse::Popular => "popular".to_string(),
        Browse::Latest => "latest".to_string(),
        Browse::Search(q) => format!("search:{}", q.trim().to_lowercase()),
    };
    // the language only changes what MangaDex lists; a source of the server has its own
    let lang = if source == "mangadex" || source.is_empty() { lang } else { "" };
    format!("list|{}|{}|{}|{}", if source.is_empty() { "mangadex" } else { source }, lang, w, offset)
}

fn pages_key(chapter: &str, saver: bool) -> String {
    format!("pages|{}|{}", chapter, if saver { "saver" } else { "full" })
}

fn pages_ttl(chapter: &str) -> u64 {
    if suwayomi::is_suwayomi(chapter) {
        readcache::PAGES_TTL
    } else {
        readcache::MANGADEX_PAGES_TTL
    }
}

/// The addresses kept in the cache, pointed at the server running now.
fn pages_from(srv: &suwayomi::Server, j: &Json) -> Vec<String> {
    j.strings().iter().map(|u| suwayomi::rebase(srv, u)).collect()
}

/// Asks the source for the pictures of a chapter and keeps the answer.
fn fetch_pages(http: &dyn Http, srv: &suwayomi::Server, id: &str, saver: bool, store: &readcache::Store, key: &str) -> Result<Vec<String>, String> {
    let r = if suwayomi::is_suwayomi(id) { suwayomi::pages(http, srv, id) } else { manga::pages(http, id, saver) };
    if let Ok(p) = &r {
        if !p.is_empty() {
            store.put(key, Json::Arr(p.iter().map(|u| u.as_str().into()).collect()));
        }
    }
    r
}

/// What the cached copy of a series knows and the one on screen does not (the library copy is kept short).
fn fill_details(shown: &mut Series, cached: &Series) {
    if shown.description.is_empty() {
        shown.description = cached.description.clone();
    }
    if shown.authors.is_empty() {
        shown.authors = cached.authors.clone();
    }
    if shown.tags.is_empty() {
        shown.tags = cached.tags.clone();
    }
    if shown.status.is_empty() {
        shown.status = cached.status.clone();
    }
    if shown.year.is_none() {
        shown.year = cached.year;
    }
    if shown.mal_id.is_none() {
        shown.mal_id = cached.mal_id;
    }
    if shown.cover.is_empty() {
        shown.cover = cached.cover.clone();
    }
    if shown.title.is_empty() {
        shown.title = cached.title.clone();
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn list_keys_tell_sources_and_pages_apart() {
        let a = list_key("mangadex", "en", &Browse::Popular, 0);
        assert_ne!(a, list_key("mangadex", "pt-br", &Browse::Popular, 0));
        assert_ne!(a, list_key("mangadex", "en", &Browse::Popular, 24));
        assert_ne!(a, list_key("mangadex", "en", &Browse::Latest, 0));
        assert_eq!(a, list_key("", "en", &Browse::Popular, 0));
        // the language does not split the lists of a server source
        assert_eq!(list_key("suwayomi:123", "en", &Browse::Popular, 0), list_key("suwayomi:123", "ja", &Browse::Popular, 0));
        assert_eq!(list_key("mangadex", "en", &Browse::Search(" Berserk ".into()), 0), list_key("mangadex", "en", &Browse::Search("berserk".into()), 0));
    }

    #[test]
    fn mangadex_pictures_expire_sooner() {
        assert_eq!(pages_ttl("mangadex-chapter-uuid"), readcache::MANGADEX_PAGES_TTL);
        assert_eq!(pages_ttl(&suwayomi::series_id("55")), readcache::PAGES_TTL);
        assert_ne!(pages_key("x", true), pages_key("x", false));
    }

    #[test]
    fn cached_pictures_follow_the_new_port() {
        let srv = suwayomi::Server::new("http://127.0.0.1:6000", "", "");
        let j = Json::Arr(vec!["http://127.0.0.1:58447/api/v1/manga/1/chapter/2/page/0".into(), "https://cdn.example/x.jpg".into()]);
        assert_eq!(pages_from(&srv, &j), vec!["http://127.0.0.1:6000/api/v1/manga/1/chapter/2/page/0".to_string(), "https://cdn.example/x.jpg".to_string()]);
    }

    #[test]
    fn the_cached_copy_fills_what_the_library_lacks() {
        let mut shown = Series { id: "a".into(), title: "A".into(), categories: vec!["Fav".into()], ..Default::default() };
        let cached = Series { id: "a".into(), title: "Other".into(), description: "D".into(), authors: vec!["X".into()], ..Default::default() };
        fill_details(&mut shown, &cached);
        assert_eq!((shown.title.as_str(), shown.description.as_str(), shown.categories.len()), ("A", "D", 1));
    }
}
