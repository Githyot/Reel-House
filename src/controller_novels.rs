//! The Text mode: plugins, the novel library, the series page and the reader.
//! The shelf (library, progress, history) is the same as the Images mode's, kept in `novels.json`.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::json::Json;
use crate::manga::{self, Browse, Chapter, HistoryItem, Series};
use crate::epub;
use crate::novels::{self, Block, Plugin};
use crate::readcache;
use crate::readstudy;

/// Shown where a mode needs a source repository and none was added: the app ships with none.
pub const NO_REPO: &str = "No repository added yet. Paste the address of one in Preferences.";

pub const SHELF: &str = "novels.json";

/// The chapter being read.
#[derive(Clone, Debug)]
pub struct NovelReader {
    pub series: Series,
    pub chapter: Chapter,
    pub blocks: Vec<Block>,
    pub loading: bool,
    pub error: String,
    /// The text shown is one the person edited (a chapter of a site; the original can be brought back).
    pub edited: bool,
    /// Study in the reader (see `readstudy`): {lang, study, mix: [pieces per paragraph], quiz: [{para, text}]}.
    pub study: Json,
    gen: u64,
}

fn plugin_json(p: &Plugin) -> Json {
    Json::obj(vec![
        ("id", p.id.as_str().into()),
        ("name", p.name.as_str().into()),
        ("site", p.site.as_str().into()),
        ("lang", p.lang.as_str().into()),
        ("version", p.version.as_str().into()),
        ("url", p.url.as_str().into()),
        ("iconUrl", p.icon.as_str().into()),
    ])
}

impl Core {
    fn plugins_dir(&self) -> PathBuf {
        self.data_dir.join("plugins")
    }

    /// The installed plugins (shared by every profile, in the data folder) and the shelf of the profile in use.
    pub(super) fn load_novels(dir: &Path, profile: &Path) -> (manga::State, Vec<Plugin>) {
        let installed = std::fs::read_to_string(dir.join("plugins").join("installed.json")).ok().and_then(|t| Json::parse(&t).ok()).map(|j| novels::parse_index(&j)).unwrap_or_default();
        (manga::State::load_file(profile, SHELF), installed)
    }

    pub fn save_novels(&self) {
        self.novels.save_file(&self.profile_dir, SHELF);
    }

    fn save_installed(&self) {
        let dir = self.plugins_dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("installed.json"), Json::Arr(self.novel_installed.iter().map(plugin_json).collect()).pretty());
    }

    // ----- plugins -----

    /// The cache of what the Text mode read from its plugins (lists, novels, chapters, the plugin list).
    fn text_cache(&self) -> readcache::Store {
        readcache::prune_once(&self.data_dir);
        readcache::Store::of(&self.data_dir, "text")
    }

    /// Reads the list of plugins of the repository (from the cache when it was read in the last hours: entering the
    /// Text mode does not download it each time).
    pub fn novel_load_catalog(&mut self) {
        if self.novel_catalog_loading {
            return;
        }
        let repo = self.settings.general.text_repo.clone();
        if repo.trim().is_empty() {
            self.novel_catalog_status = NO_REPO.to_string();
            self.events.push(Event::NovelPlugins);
            return;
        }
        let (store, key) = (self.text_cache(), format!("catalog|{}", repo.trim()));
        let cached = store.get(&key);
        if let Some(h) = cached.as_ref().filter(|h| readcache::fresh(h.age, readcache::CATALOG_TTL)) {
            let l = novels::parse_index(&h.data);
            if !l.is_empty() {
                // already on screen: nothing to tell
                if self.novel_catalog != l {
                    self.on_novel_catalog(Ok(l));
                }
                return;
            }
        }
        let stale = cached.map(|h| novels::parse_index(&h.data)).filter(|l| !l.is_empty());
        self.novel_catalog_loading = true;
        self.novel_catalog_status = "Loading…".to_string();
        self.events.push(Event::NovelPlugins);
        self.spawn(move |tx, http| {
            let r = novels::fetch_index(&*http, &repo);
            if let Ok(l) = &r {
                store.put(&key, Json::Arr(l.iter().map(plugin_json).collect()));
            }
            send(tx, Msg::NovelCatalog(r.or_else(|e| stale.ok_or(e))));
        });
    }

    pub(super) fn on_novel_catalog(&mut self, r: Result<Vec<Plugin>, String>) {
        self.novel_catalog_loading = false;
        match r {
            Ok(l) => {
                self.novel_catalog_status = format!("{} plugins offered.", l.len());
                self.novel_catalog = l;
            }
            Err(e) => self.novel_catalog_status = e,
        }
        self.events.push(Event::NovelPlugins);
    }

    pub fn novel_is_installed(&self, id: &str) -> Option<&Plugin> {
        self.novel_installed.iter().find(|p| p.id == id)
    }

    /// Downloads a plugin (or its new version) and keeps it.
    pub fn novel_install(&mut self, p: &Plugin) {
        self.toast("Installing…");
        let (p, dir) = (p.clone(), self.plugins_dir());
        self.spawn(move |tx, http| {
            let r = novels::fetch_code(&*http, &p).and_then(|code| {
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                std::fs::write(dir.join(format!("{}.js", p.id)), code).map_err(|e| e.to_string())?;
                Ok(p.clone())
            });
            send(tx, Msg::NovelInstalled(r));
        });
    }

    pub(super) fn on_novel_installed(&mut self, r: Result<Plugin, String>) {
        match r {
            Ok(p) => {
                self.novel_installed.retain(|x| x.id != p.id);
                self.novel_installed.push(p);
                self.novel_installed.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.save_installed();
                if self.settings.general.novel_source.is_empty() {
                    self.settings.general.novel_source = self.novel_installed[0].id.clone();
                    self.save_settings();
                }
                self.toast("Installed.");
            }
            Err(e) => self.toast(&format!("Could not install: {}", e)),
        }
        self.events.push(Event::NovelPlugins);
    }

    pub fn novel_remove_plugin(&mut self, id: &str) {
        self.novel_installed.retain(|p| p.id != id);
        let _ = std::fs::remove_file(self.plugins_dir().join(format!("{}.js", id)));
        self.save_installed();
        if self.settings.general.novel_source == id {
            self.settings.general.novel_source = self.novel_installed.first().map(|p| p.id.clone()).unwrap_or_default();
            self.save_settings();
        }
        self.events.push(Event::NovelPlugins);
    }

    /// Chooses the plugin Explore lists.
    pub fn novel_set_source(&mut self, id: &str) {
        if self.settings.general.novel_source != id {
            self.settings.general.novel_source = id.to_string();
            self.save_settings();
        }
        self.novel_browse(Browse::Popular);
    }

    // ----- explore -----

    pub fn novel_browse(&mut self, what: Browse) {
        self.novel_gen += 1;
        self.novel_list = MangaList { what, items: vec![], offset: 0, more: false, loading: false, error: String::new(), gen: self.novel_gen };
        self.novel_more();
    }

    pub fn novel_more(&mut self) {
        if self.novel_list.loading || (self.novel_list.offset > 0 && !self.novel_list.more) {
            return;
        }
        let plugin = self.settings.general.novel_source.clone();
        if plugin.is_empty() || self.novel_is_installed(&plugin).is_none() {
            self.novel_list.error = "Install a plugin in Preferences → Text.".to_string();
            self.events.push(Event::NovelList);
            return;
        }
        let (what, offset, gen, dir) = (self.novel_list.what.clone(), self.novel_list.offset, self.novel_list.gen, self.plugins_dir());
        let store = self.text_cache();
        let key = format!("list|{}|{}|{}", plugin, browse_name(&what), offset);
        let cached = store.get(&key);
        if let Some(l) = cached.as_ref().filter(|h| readcache::fresh(h.age, readcache::LIST_TTL)).and_then(|h| manga::list_from_json(&h.data)) {
            self.on_novel_list(gen, offset, Ok(l));
            return;
        }
        let stale = cached.and_then(|h| manga::list_from_json(&h.data));
        self.novel_list.loading = true;
        self.novel_list.error.clear();
        self.events.push(Event::NovelList);
        self.spawn(move |tx, http| {
            let page = (offset / manga::PAGE + 1) as f64;
            let r = match &what {
                Browse::Search(q) => novel_call(&http, &dir, &plugin, "searchNovels", vec![q.as_str().into(), page.into()]),
                Browse::Latest => novel_call(&http, &dir, &plugin, "popularNovels", vec![page.into(), Json::obj(vec![("showLatestNovels", true.into()), ("filters", "__default".into())])]),
                Browse::Popular => novel_call(&http, &dir, &plugin, "popularNovels", vec![page.into(), Json::obj(vec![("showLatestNovels", false.into()), ("filters", "__default".into())])]),
            };
            let r = r.map(|j| {
                let l = novels::parse_list(&plugin, &j);
                let more = !l.is_empty();
                if more {
                    store.put(&key, manga::list_to_json(&l, more));
                }
                (l, more)
            });
            send(tx, Msg::NovelList(gen, offset, r.or_else(|e| stale.ok_or(e))));
        });
    }

    pub(super) fn on_novel_list(&mut self, gen: u64, offset: usize, r: Result<(Vec<Series>, bool), String>) {
        if gen != self.novel_list.gen || offset != self.novel_list.offset {
            return;
        }
        self.novel_list.loading = false;
        self.novel_list.error.clear();
        match r {
            Ok((items, more)) => {
                self.novel_list.offset += manga::PAGE;
                let before = self.novel_list.items.len();
                for s in items {
                    if !self.novel_list.items.iter().any(|x| x.id == s.id) {
                        self.novel_list.items.push(s);
                    }
                }
                // A site without pages answers the same list again: that is the end.
                self.novel_list.more = more && self.novel_list.items.len() > before;
            }
            Err(e) => self.novel_list.error = e,
        }
        self.events.push(Event::NovelList);
    }

    // ----- library -----

    pub fn novel_toggle_library(&mut self, s: &Series) {
        if let Some(i) = self.novels.library.iter().position(|x| x.id == s.id) {
            self.novels.library.remove(i);
        } else {
            let mut item = s.clone();
            if let Some(v) = self.novel_view.as_ref().filter(|v| v.series.id == s.id) {
                item.chapter_count = v.chapters.len() as u32;
            }
            self.novels.library.push(item);
        }
        self.save_novels();
        self.events.push(Event::NovelLibrary);
        self.events.push(Event::NovelView);
    }

    pub fn novel_set_categories(&mut self, id: &str, cats: Vec<String>) {
        if let Some(s) = self.novels.library.iter_mut().find(|s| s.id == id) {
            s.categories = cats;
            self.save_novels();
            self.events.push(Event::NovelLibrary);
        }
    }

    pub fn novel_add_category(&mut self, name: &str) {
        let name = name.trim();
        if !name.is_empty() && !self.novels.categories.iter().any(|c| c == name) {
            self.novels.categories.push(name.to_string());
            self.save_novels();
            self.events.push(Event::NovelLibrary);
        }
    }

    pub fn novel_remove_category(&mut self, name: &str) {
        self.novels.categories.retain(|c| c != name);
        for s in self.novels.library.iter_mut() {
            s.categories.retain(|c| c != name);
        }
        self.save_novels();
        self.events.push(Event::NovelLibrary);
    }

    /// Looks for new chapters of the novels in the library.
    pub fn novel_refresh_library(&mut self) {
        if self.novel_refreshing || self.novels.library.is_empty() || !crate::connectivity::online() {
            return;
        }
        self.novel_refreshing = true;
        let ids: Vec<String> = self.novels.library.iter().map(|s| s.id.clone()).take(40).collect();
        let (dir, books) = (self.plugins_dir(), epub::books_dir(&self.profile_dir));
        self.spawn(move |tx, http| {
            let out = std::sync::Mutex::new(Vec::<(String, u32)>::new());
            let queue = std::sync::Mutex::new(ids);
            std::thread::scope(|sc| {
                for _ in 0..3 {
                    sc.spawn(|| loop {
                        let Some(id) = queue.lock().ok().and_then(|mut q| q.pop()) else { break };
                        if let Ok((_, list)) = novel_details(&http, &dir, &books, &id) {
                            if let Ok(mut o) = out.lock() {
                                o.push((id, list.len() as u32));
                            }
                        }
                    });
                }
            });
            send(tx, Msg::NovelCounts(out.into_inner().unwrap_or_default()));
        });
    }

    pub(super) fn on_novel_counts(&mut self, counts: Vec<(String, u32)>) {
        self.novel_refreshing = false;
        let mut changed = false;
        let mut fresh: Vec<(String, String, String, u32)> = Vec::new();
        for (id, n) in counts {
            if let Some(s) = self.novels.library.iter_mut().find(|s| s.id == id) {
                if s.chapter_count != n {
                    if s.chapter_count > 0 && n > s.chapter_count {
                        fresh.push((s.id.clone(), s.title.clone(), s.cover.clone(), n - s.chapter_count));
                    }
                    s.chapter_count = n;
                    changed = true;
                }
            }
        }
        for (sid, title, cover, added) in fresh {
            self.note_new_chapters("novel", &sid, &title, &cover, added);
        }
        if changed {
            self.save_novels();
            self.events.push(Event::NovelLibrary);
        }
    }

    // ----- books from files -----

    /// Adds a book from a file (.epub, .txt or .html) to the shelf. The answer comes as `Event::NovelLibrary`.
    pub fn novel_import(&mut self, path: PathBuf) {
        let books = epub::books_dir(&self.profile_dir);
        self.toast("Adding the book…");
        self.spawn(move |tx, _| {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let r = std::fs::read(&path).map_err(|e| format!("Could not read the file: {}", e)).and_then(|bytes| {
                std::fs::create_dir_all(&books).map_err(|e| e.to_string())?;
                epub::import(&name, &bytes, &books)
            });
            let r = r.and_then(|b| epub::load(&books, &b.id).map(|(s, _)| s));
            send(tx, Msg::NovelImported(r));
        });
    }

    pub(super) fn on_novel_imported(&mut self, r: Result<Series, String>) {
        match r {
            Ok(s) => {
                let title = s.title.clone();
                if !self.novels.library.iter().any(|x| x.id == s.id) {
                    self.novels.library.push(s);
                    self.save_novels();
                }
                self.toast(&format!("Added: {}", title));
                self.events.push(Event::NovelLibrary);
            }
            Err(e) => self.toast(&e),
        }
    }

    // ----- the series page -----

    /// Opens the page of a novel: from the cache at once when it was read before (asked for again in the background
    /// when that copy is older than `SERIES_TTL`). Books made from files are read from the disk anyway.
    pub fn novel_open(&mut self, series: Series) {
        self.clear_notice(&format!("novel:{}", series.id));
        self.novel_gen += 1;
        let gen = self.novel_gen;
        let id = series.id.clone();
        let shown = self.novels.library.iter().find(|s| s.id == id).cloned().unwrap_or(series);
        let own = novels::split_id(&id).map(|(p, _)| p == epub::PLUGIN).unwrap_or(false);
        let cached = if own { None } else { self.text_cache().get(&format!("series|{}", id)).and_then(|h| manga::view_from_json(&h.data).map(|v| (v, h.age))) };
        match cached {
            Some(((cs, chapters), age)) if !chapters.is_empty() => {
                // offline the copy on disk is the answer (the plugin could only fail)
                let stale = !readcache::fresh(age, readcache::SERIES_TTL) && crate::connectivity::online();
                self.novel_view = Some(MangaView { series: shown, chapters, loading: false, refreshing: stale, error: String::new(), gen });
                // the copy of the source fills in what the library copy lacks (same rules as a fresh answer)
                self.on_novel_view_keep(gen, cs);
                self.events.push(Event::NovelView);
                if !stale {
                    return;
                }
            }
            _ if !own && !crate::connectivity::online() => {
                let error = "No internet connection. This novel was never opened with internet, so its chapters are not on this computer.".to_string();
                self.novel_view = Some(MangaView { series: shown, chapters: vec![], loading: false, refreshing: false, error, gen });
                self.events.push(Event::NovelView);
                return;
            }
            _ => {
                self.novel_view = Some(MangaView { series: shown, chapters: vec![], loading: true, refreshing: false, error: String::new(), gen });
                self.events.push(Event::NovelView);
            }
        }
        let (dir, books) = (self.plugins_dir(), epub::books_dir(&self.profile_dir));
        self.spawn(move |tx, http| send(tx, Msg::NovelView(gen, novel_details(&http, &dir, &books, &id))));
    }

    /// The details of the source copy, put into the series on screen without touching its chapters.
    fn on_novel_view_keep(&mut self, gen: u64, s: Series) {
        let Some(v) = self.novel_view.as_mut().filter(|v| v.gen == gen) else { return };
        let keep = (v.series.categories.clone(), v.series.mal_id, v.series.cover.clone(), v.chapters.len() as u32);
        v.series = s;
        v.series.categories = keep.0;
        v.series.mal_id = v.series.mal_id.or(keep.1);
        if v.series.cover.is_empty() || v.series.cover.contains("coverNotAvailable") || (keep.2.contains("/covers/") && std::path::Path::new(&keep.2).exists()) {
            v.series.cover = keep.2;
        }
        v.series.chapter_count = keep.3;
    }

    pub fn novel_close(&mut self) {
        self.novel_view = None;
        self.novel_gen += 1;
    }

    pub(super) fn on_novel_view(&mut self, gen: u64, r: Result<(Series, Vec<Chapter>), String>) {
        let store = self.text_cache();
        let Some(v) = self.novel_view.as_mut().filter(|v| v.gen == gen) else { return };
        v.loading = false;
        v.refreshing = false;
        match r {
            Ok((s, list)) => {
                let own = novels::split_id(&s.id).map(|(p, _)| p == epub::PLUGIN).unwrap_or(false);
                if !own && !list.is_empty() {
                    let mut plain = s.clone();
                    plain.categories.clear();
                    store.put(&format!("series|{}", s.id), manga::view_to_json(&plain, &list));
                }
                let keep = (v.series.categories.clone(), v.series.mal_id, v.series.cover.clone());
                v.series = s;
                v.series.categories = keep.0;
                v.series.mal_id = keep.1;
                if v.series.cover.is_empty() || v.series.cover.contains("coverNotAvailable") || (keep.2.contains("/covers/") && std::path::Path::new(&keep.2).exists()) {
                    v.series.cover = keep.2;
                }
                v.chapters = list;
                v.error.clear();
                v.series.chapter_count = v.chapters.len() as u32;
                let copy = v.series.clone();
                if let Some(s) = self.novels.library.iter_mut().find(|s| s.id == copy.id) {
                    s.chapter_count = copy.chapter_count;
                    if s.description.is_empty() {
                        s.description = copy.description.clone();
                    }
                    if s.cover.is_empty() {
                        s.cover = copy.cover.clone();
                    }
                    self.save_novels();
                    self.events.push(Event::NovelLibrary);
                }
                self.link_mal(copy);
            }
            // the cached copy on screen stays when the plugin could not be asked
            Err(e) => {
                if v.chapters.is_empty() {
                    v.error = e;
                }
            }
        }
        self.events.push(Event::NovelView);
    }

    pub fn novel_is_read(&self, series: &str, chapter: &str) -> bool {
        self.novels.is_read(series, chapter)
    }

    pub fn novel_mark(&mut self, series: &str, chapter: &str, read: bool) {
        let p = self.novels.progress.entry(series.to_string()).or_default();
        p.read.retain(|c| c != chapter);
        if read {
            p.read.push(chapter.to_string());
        }
        self.save_novels();
        self.events.push(Event::NovelView);
        self.events.push(Event::NovelLibrary);
    }

    pub fn novel_mark_all(&mut self, read: bool) {
        let Some(v) = self.novel_view.as_ref() else { return };
        let (id, ids): (String, Vec<String>) = (v.series.id.clone(), v.chapters.iter().map(|c| c.id.clone()).collect());
        let p = self.novels.progress.entry(id).or_default();
        p.read = if read { ids } else { vec![] };
        if !read {
            p.chapter.clear();
            p.page = 0;
        }
        self.save_novels();
        self.events.push(Event::NovelView);
        self.events.push(Event::NovelLibrary);
    }

    pub fn novel_mark_before(&mut self, chapter: &str) {
        let Some(v) = self.novel_view.as_ref() else { return };
        let Some(at) = v.chapters.iter().position(|c| c.id == chapter) else { return };
        let (id, ids): (String, Vec<String>) = (v.series.id.clone(), v.chapters[..at].iter().map(|c| c.id.clone()).collect());
        let p = self.novels.progress.entry(id).or_default();
        for i in ids {
            if !p.read.contains(&i) {
                p.read.push(i);
            }
        }
        self.save_novels();
        self.events.push(Event::NovelView);
        self.events.push(Event::NovelLibrary);
    }

    // ----- the reader -----

    pub fn novel_read(&mut self, chapter: Chapter) {
        let Some(series) = self.novel_view.as_ref().map(|v| v.series.clone()) else { return };
        self.novel_gen += 1;
        let gen = self.novel_gen;
        // translations asked for by the chapter that was left are dropped
        self.reader_changed();
        let id = chapter.id.clone();
        self.novel_resume = self.novels.progress.get(&series.id).filter(|p| p.chapter == id).map(|p| p.page).unwrap_or(0);
        self.novels.note_history(HistoryItem { series_id: series.id.clone(), title: series.title.clone(), cover: series.cover.clone(), chapter_id: id.clone(), label: chapter.label(), at: util::now_unix() });
        self.novels.progress.entry(series.id.clone()).or_default().chapter = id.clone();
        self.save_novels();
        self.novel_reader = Some(NovelReader { series, chapter, blocks: vec![], loading: true, error: String::new(), edited: false, study: Json::Null, gen });
        self.events.push(Event::NovelText);
        let (dir, books, edit) = (self.plugins_dir(), epub::books_dir(&self.profile_dir), self.edit_file(&id));
        let store = self.text_cache();
        self.spawn(move |tx, http| {
            // What the person wrote over a chapter of a site is shown instead of the site's text.
            if let Ok(html) = std::fs::read_to_string(&edit) {
                send(tx, Msg::NovelText(gen, Ok(html)));
                return;
            }
            let r = match novels::split_id(&id) {
                Some((plugin, path)) if plugin == epub::PLUGIN => match epub::split_chapter(path) {
                    (book, Some(n)) => epub::chapter_html(&books, book, n),
                    _ => Err("Not a chapter of a book.".to_string()),
                },
                Some((plugin, path)) => {
                    // a chapter read before comes from the cache (its text rarely changes)
                    let key = format!("chapter|{}", id);
                    let cached = store.get(&key);
                    // offline any copy is used, however old
                    let offline = !crate::connectivity::online();
                    match cached.as_ref().filter(|h| (offline || readcache::fresh(h.age, readcache::TEXT_TTL)) && !h.data.str().trim().is_empty()) {
                        Some(h) => Ok(h.data.str()),
                        None if offline => Err("No internet connection. This chapter was never opened with internet, so its text is not on this computer.".to_string()),
                        None => {
                            let r = novel_call(&http, &dir, plugin, "parseChapter", vec![path.into()]).map(|j| j.str());
                            match r {
                                Ok(html) => {
                                    if !html.trim().is_empty() {
                                        store.put(&key, html.as_str().into());
                                    }
                                    Ok(html)
                                }
                                Err(e) => cached.map(|h| h.data.str()).filter(|t| !t.trim().is_empty()).ok_or(e),
                            }
                        }
                    }
                }
                None => Err("Not a novel chapter.".to_string()),
            };
            send(tx, Msg::NovelText(gen, r));
        });
    }

    /// Where the reader starts, in thousandths of the chapter.
    pub fn novel_resume_at(&self) -> u32 {
        self.novel_resume
    }

    pub(super) fn on_novel_text(&mut self, gen: u64, r: Result<String, String>) {
        let Some(rd) = self.novel_reader.as_mut().filter(|r| r.gen == gen) else { return };
        rd.loading = false;
        match r {
            Ok(html) => {
                rd.blocks = novels::blocks(&html);
                // A document of the person's own starts empty: that is not an error.
                let own = novels::split_id(&rd.series.id).map(|(p, _)| p == epub::PLUGIN).unwrap_or(false);
                if rd.blocks.is_empty() && !own {
                    rd.error = "This chapter has no text. The site may need a login or block this app.".to_string();
                }
            }
            Err(e) => rd.error = e,
        }
        let study = self.novel_study();
        if let Some(rd) = self.novel_reader.as_mut() {
            rd.study = study;
        }
        self.events.push(Event::NovelText);
    }

    /// The language a novel is written in (3-letter code): its plugin's, else told from its text; "" when unknown.
    fn novel_lang3(&self, series: &str, sample: &str) -> String {
        let by_plugin = novels::split_id(series).and_then(|(p, _)| self.novel_installed.iter().find(|x| x.id == p)).map(|p| readstudy::lang3_of(&p.lang)).unwrap_or_default();
        if !by_plugin.is_empty() {
            return by_plugin;
        }
        readstudy::guess_lang3(sample)
    }

    /// The study language (3-letter): the one of the preferences, else the one of the knowledge file.
    pub(super) fn study_lang3(&self) -> String {
        let g = &self.settings.general.study_lang;
        if !g.trim().is_empty() {
            return crate::learn::lang3(g);
        }
        self.knowledge.study_lang()
    }

    /// Study in the Text reader, worked out once per chapter (docs/study-in-readers.md): the learned words mixed in
    /// when the novel is in a language the user reads (the player's word mixing: `wordmix_swap`, on
    /// `study_wordmix_rate` % of the sentences), and the fully known sentences for quiz cards when it is in the study
    /// language. The reader shows them according to the study switches of the preferences.
    fn novel_study(&mut self) -> Json {
        let Some(rd) = self.novel_reader.as_ref() else { return Json::Null };
        let study = self.study_lang3();
        if study.is_empty() || rd.blocks.is_empty() {
            return Json::Null;
        }
        let texts: Vec<String> = rd.blocks.iter().map(|b| if b.image.is_some() || b.heading { String::new() } else { b.text.clone() }).collect();
        let sample: String = texts.iter().take(12).map(|t| t.as_str()).collect::<Vec<_>>().join(" ");
        let lang = self.novel_lang3(&rd.series.id, &sample);
        if lang.is_empty() {
            return Json::Null;
        }
        let g = &self.settings.general;
        let (rate, quiz_rate) = (g.study_wordmix_rate, g.study_quiz_rate);
        let mut mix = Json::Arr(vec![]);
        let mut quiz = Json::Arr(vec![]);
        if lang == study {
            quiz = Json::Arr(
                readstudy::quiz_picks(&self.knowledge, &lang, &texts, readstudy::quiz_every(quiz_rate))
                    .into_iter()
                    .map(|(para, text)| Json::obj(vec![("para", (para as f64).into()), ("text", text.as_str().into())]))
                    .collect(),
            );
        } else {
            // only a novel in a language the user reads (the known languages, or the app's)
            let known: Vec<String> = g.known_langs.iter().map(|l| crate::learn::lang3(l)).collect();
            if known.contains(&lang) || crate::learn::lang3(&g.ui_lang(&self.system_lang)) == lang {
                let mut rows = Vec::with_capacity(texts.len());
                let mut any = false;
                for t in &texts {
                    let mut pieces = vec![];
                    let mut swapped = false;
                    for part in readstudy::sentence_parts(t) {
                        // the same sentences are mixed every time the chapter is opened
                        if crate::controller::wordmix::picked(part.trim(), rate) {
                            let (new, spans) = self.wordmix_swap(&part, &lang);
                            if !spans.is_empty() {
                                swapped = true;
                                readstudy::from_wordmix(crate::controller::wordmix::pieces(&new, &spans), &mut pieces);
                                continue;
                            }
                        }
                        readstudy::from_wordmix(vec![(part, None)], &mut pieces);
                    }
                    any |= swapped;
                    rows.push(if swapped { readstudy::pieces_json(&pieces) } else { Json::Arr(vec![]) });
                }
                if any {
                    mix = Json::Arr(rows);
                }
            }
        }
        Json::obj(vec![("lang", lang.as_str().into()), ("study", study.as_str().into()), ("mix", mix), ("quiz", quiz)])
    }

    /// Where the text a person wrote over a chapter of a site is kept.
    fn edit_file(&self, chapter: &str) -> PathBuf {
        self.profile_dir.join("edits").join(format!("{}.html", util::fnv_hex(chapter)))
    }

    /// The text of the chapter as the editor shows it.
    pub fn novel_markup(&self) -> String {
        self.novel_reader.as_ref().map(|r| novels::to_markup(&r.blocks)).unwrap_or_default()
    }

    /// Keeps what was written in the editor: in the file of the book, or over the chapter of a site.
    pub fn novel_save_edit(&mut self, text: &str) {
        let Some(rd) = self.novel_reader.as_ref() else { return };
        let blocks = novels::parse_markup(text);
        let html = novels::to_html(&blocks);
        let id = rd.chapter.id.clone();
        let r = match novels::split_id(&id) {
            Some((p, path)) if p == epub::PLUGIN => match epub::split_chapter(path) {
                (book, Some(n)) => epub::save_chapter(&epub::books_dir(&self.profile_dir), book, n, &html),
                _ => Err("Not a chapter of a book.".to_string()),
            },
            _ => {
                let f = self.edit_file(&id);
                std::fs::create_dir_all(f.parent().unwrap_or(&self.profile_dir)).map_err(|e| e.to_string()).and_then(|_| std::fs::write(&f, &html).map_err(|e| format!("Could not save the text: {}", e)))
            }
        };
        match r {
            Ok(()) => {
                if let Some(rd) = self.novel_reader.as_mut() {
                    rd.blocks = blocks;
                    rd.edited = novels::split_id(&id).map(|(p, _)| p != epub::PLUGIN).unwrap_or(true);
                    rd.error.clear();
                }
                self.toast("Saved.");
            }
            Err(e) => self.toast(&e),
        }
        self.events.push(Event::NovelText);
    }

    /// Brings back the text of the site for the chapter being read (the edited one is dropped).
    pub fn novel_reset_edit(&mut self) {
        let Some(rd) = self.novel_reader.as_ref() else { return };
        let (chapter, id) = (rd.chapter.clone(), rd.chapter.id.clone());
        let _ = std::fs::remove_file(self.edit_file(&id));
        self.novel_read(chapter);
    }

    /// A new document of the person's own, with one empty chapter. It goes to the library; the series is returned.
    pub fn novel_new_book(&mut self, title: &str) -> Option<Series> {
        let books = epub::books_dir(&self.profile_dir);
        let r = std::fs::create_dir_all(&books).map_err(|e| e.to_string()).and_then(|_| epub::create(&books, title)).and_then(|b| epub::load(&books, &b.id));
        match r {
            Ok((s, _)) => {
                self.novels.library.push(s.clone());
                self.save_novels();
                self.events.push(Event::NovelLibrary);
                Some(s)
            }
            Err(e) => {
                self.toast(&e);
                None
            }
        }
    }

    /// Adds a chapter to the document (or book from a file) whose page is open.
    pub fn novel_add_chapter(&mut self, title: &str) {
        let Some(v) = self.novel_view.as_ref() else { return };
        let series = v.series.clone();
        let Some((p, book)) = novels::split_id(&series.id).filter(|(p, _)| *p == epub::PLUGIN) else { return };
        let _ = p;
        match epub::add_chapter(&epub::books_dir(&self.profile_dir), book, title) {
            Ok(_) => self.novel_open(series),
            Err(e) => self.toast(&e),
        }
    }

    pub fn novel_close_reader(&mut self) {
        self.novel_reader = None;
        self.novel_gen += 1;
        self.reader_changed();
    }

    /// The reader is at `permille` (0..=1000) of the chapter.
    pub fn novel_progress(&mut self, permille: u32) {
        let Some(rd) = self.novel_reader.as_ref() else { return };
        let (sid, cid) = (rd.series.id.clone(), rd.chapter.id.clone());
        let p = self.novels.progress.entry(sid).or_default();
        p.chapter = cid;
        p.page = permille;
        self.novel_saves += 1;
        if self.novel_saves % 6 == 0 {
            self.save_novels();
        }
    }

    pub fn novel_finished(&mut self) {
        let Some(rd) = self.novel_reader.as_ref() else { return };
        let (series, chapter) = (rd.series.clone(), rd.chapter.clone());
        if self.novels.is_read(&series.id, &chapter.id) {
            return;
        }
        let p = self.novels.progress.entry(series.id.clone()).or_default();
        p.read.push(chapter.id.clone());
        p.page = 0;
        let count = p.read.len() as u32;
        self.save_novels();
        self.events.push(Event::NovelView);
        self.events.push(Event::NovelLibrary);
        self.track_mal(series, chapter, count);
    }

    pub fn novel_neighbour(&self, delta: i32) -> Option<Chapter> {
        let rd = self.novel_reader.as_ref()?;
        let v = self.novel_view.as_ref()?;
        let at = v.chapters.iter().position(|c| c.id == rd.chapter.id)? as i32 + delta;
        if at < 0 {
            return None;
        }
        v.chapters.get(at as usize).cloned()
    }

    // ----- MyAnimeList -----

    /// Finds the MyAnimeList entry of a title that does not say which one it is, and remembers it.
    pub(super) fn link_mal(&mut self, s: Series) {
        if s.mal_id.is_some() || s.title.is_empty() || self.mal_linking.contains(&s.id) {
            return;
        }
        self.mal_linking.insert(s.id.clone());
        let (id, title) = (s.id.clone(), s.title.clone());
        self.spawn(move |tx, http| send(tx, Msg::MalLinked(id, crate::malmatch::resolve(&*http, &title))));
    }

    pub(super) fn on_mal_linked(&mut self, id: String, mal: Option<u64>) {
        self.mal_linking.remove(&id);
        let Some(mal) = mal else { return };
        for v in [self.manga_view.as_mut(), self.novel_view.as_mut()].into_iter().flatten() {
            if v.series.id == id {
                v.series.mal_id = Some(mal);
            }
        }
        let mut changed = (false, false);
        if let Some(s) = self.manga.library.iter_mut().find(|s| s.id == id) {
            s.mal_id = Some(mal);
            changed.0 = true;
        }
        if let Some(s) = self.novels.library.iter_mut().find(|s| s.id == id) {
            s.mal_id = Some(mal);
            changed.1 = true;
        }
        if changed.0 {
            self.save_manga();
        }
        if changed.1 {
            self.save_novels();
        }
    }

    /// Tells MyAnimeList how far the person has read. The MAL entry is looked up first when the source does not give it.
    pub(super) fn track_mal(&mut self, series: Series, chapter: Chapter, count: u32) {
        let acc = self.settings.mal.clone();
        if acc.access_token.is_empty() && acc.refresh_token.is_empty() {
            return;
        }
        let n = chapter.number.trim().parse::<f64>().map(|x| x.floor() as u32).unwrap_or(0).max(count);
        self.spawn(move |tx, http| {
            let mal_id = match series.mal_id {
                Some(m) => Some(m),
                None => {
                    let found = crate::malmatch::resolve(&*http, &series.title);
                    send(tx, Msg::MalLinked(series.id.clone(), found));
                    found
                }
            };
            let Some(mal_id) = mal_id else { return };
            let r = controller_mal::fresh_token(&*http, &acc).and_then(|(tok, newt)| {
                if let Some(t) = newt {
                    send(tx, Msg::MalTokens(t));
                }
                mal::Api { http: &*http, token: tok }.track_manga(mal_id, n)
            });
            if let Err(e) = r {
                send(tx, Msg::Info(MsgKind::Mal, e));
            }
        });
    }
}

/// Runs a method of an installed plugin.
fn novel_call(http: &HttpRef, dir: &Path, plugin: &str, method: &str, args: Vec<Json>) -> Result<Json, String> {
    let code = std::fs::read_to_string(dir.join(format!("{}.js", plugin))).map_err(|_| "That plugin is not installed.".to_string())?;
    jsrt::run(http.clone(), plugin, &code, method, &Json::Arr(args))
}

/// The series and all its chapters, from the plugin that owns it.
/// (`dir` is the plugins folder, shared by every profile; `books` the books folder of the profile in use.)
fn novel_details(http: &HttpRef, dir: &Path, books: &Path, id: &str) -> Result<(Series, Vec<Chapter>), String> {
    let (plugin, path) = novels::split_id(id).ok_or_else(|| "Not a novel.".to_string())?;
    // A book made from a file: everything is on the disk.
    if plugin == epub::PLUGIN {
        return epub::load(books, path);
    }
    let j = novel_call(http, dir, plugin, "parseNovel", vec![path.into()])?;
    let (mut s, mut chapters, pages) = novels::parse_novel(plugin, &j).ok_or_else(|| "The plugin did not give this novel.".to_string())?;
    // The id stays what the list gave, whatever form of the path the plugin answers with.
    s.id = id.to_string();
    if pages > 1 {
        for page in 2..=pages.min(80) {
            let Ok(p) = novel_call(http, dir, plugin, "parsePage", vec![path.into(), (page as f64).into()]) else { break };
            let more = novels::parse_chapters(plugin, p.get("chapters"), chapters.len());
            if more.is_empty() {
                break;
            }
            for c in more {
                if !chapters.iter().any(|x| x.id == c.id) {
                    chapters.push(c);
                }
            }
        }
    }
    Ok((s, chapters))
}

fn browse_name(what: &Browse) -> String {
    match what {
        Browse::Popular => "popular".to_string(),
        Browse::Latest => "latest".to_string(),
        Browse::Search(q) => format!("search:{}", q.trim().to_lowercase()),
    }
}
