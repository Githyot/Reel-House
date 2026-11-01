//! The shared decks of AnkiWeb inside the Study mode (the "Implement" button): search them by category or words,
//! read a deck's page, and add it to the user's decks without leaving the app (download, then the usual import).
//! The site's services are in `ankishared.rs`; this file is part of `controller` (it needs the private parts of `Core`).
use super::ankishared::{self, DeckInfo, DeckRow};
use super::*;

/// Results per page of the browser.
const PER_PAGE: usize = 24;
/// The site's answers may be cached for 10 minutes (its Cache-Control); searches done again within that time are
/// answered from memory.
const CACHE_SECS: u64 = 600;
const SORTS: &[&str] = &["rating", "modified", "notes", "audio", "images", "title"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DlStage {
    Downloading,
    Importing,
}

/// A deck being added: downloaded, then imported through `study_import`.
#[derive(Clone, Debug)]
pub struct SharedDl {
    pub id: u32,
    pub title: String,
    pub stage: DlStage,
    pub done: u64,
    pub total: Option<u64>,
}

/// What the shared-deck browser shows.
#[derive(Default)]
pub struct SharedState {
    pub open: bool,
    pub query: String,
    pub cat: String,
    pub sort: String,
    pub page: usize,
    pub loading: bool,
    pub error: String,
    /// The words of the results below, and the results in the chosen order.
    words: String,
    rows: Arc<Vec<DeckRow>>,
    gen: u64,
    /// Recent searches: (words, when, rows in the server's order).
    cache: Vec<(String, Option<Instant>, Arc<Vec<DeckRow>>)>,
    pub info: Option<DeckInfo>,
    pub info_id: u32,
    pub info_loading: bool,
    pub info_error: String,
    info_gen: u64,
    pub dl: Option<SharedDl>,
}

fn n(v: f64) -> Json {
    Json::Num(v)
}

impl Core {
    /// A text of this feature in the window's language (toasts and banners are made here, not in QML).
    fn shared_tr(&self, english: &str) -> String {
        crate::i18n::translate(&self.ui_lang(), english).unwrap_or(english).to_string()
    }

    pub fn shared_open(&mut self) {
        self.shared.open = true;
        if self.shared.sort.is_empty() {
            self.shared.sort = "rating".into();
        }
        self.events.push(Event::Study);
    }

    pub fn shared_close(&mut self) {
        self.shared.open = false;
        self.shared.info = None;
        self.shared.info_id = 0;
        self.events.push(Event::Study);
    }

    /// Searches (category and / or words), or only changes the order or the page of the results already there.
    pub fn shared_search(&mut self, query: &str, cat: &str, page: usize, sort: &str) {
        self.events.push(Event::Study);
        if let Some((gen, words)) = self.shared.plan_search(query, cat, page, sort) {
            self.spawn(move |tx, http| {
                let r = ankishared::list(&*http, &words);
                send(tx, Msg::SharedList(gen, words, r));
            });
        }
    }

    pub(super) fn on_shared_list(&mut self, gen: u64, words: String, r: Result<Vec<DeckRow>, String>) {
        if self.shared.take_list(gen, words, r) {
            self.events.push(Event::Study);
        }
    }

    /// Opens the page of a deck (0 closes it).
    pub fn shared_info(&mut self, id: u32) {
        self.events.push(Event::Study);
        if id == 0 {
            self.shared.info_gen += 1;
            self.shared.info = None;
            self.shared.info_id = 0;
            self.shared.info_loading = false;
            return;
        }
        if self.shared.info_id == id && (self.shared.info.is_some() || self.shared.info_loading) {
            return;
        }
        self.shared.info_gen += 1;
        let gen = self.shared.info_gen;
        self.shared.info_id = id;
        self.shared.info = None;
        self.shared.info_loading = true;
        self.shared.info_error.clear();
        self.spawn(move |tx, http| {
            let r = ankishared::deck_info(&*http, id);
            send(tx, Msg::SharedInfo(gen, r));
        });
    }

    pub(super) fn on_shared_info(&mut self, gen: u64, r: Result<DeckInfo, String>) {
        if self.shared.take_info(gen, r) {
            self.events.push(Event::Study);
        }
    }

    /// Adds a shared deck to the user's decks: a fresh page of the deck (its download key), the package into a
    /// temporary folder, then the import of `.apkg` files.
    pub fn shared_download(&mut self, id: u32) {
        if id == 0 {
            return;
        }
        if self.shared.dl.is_some() || self.study_importing {
            let t = self.shared_tr("A deck is already being added. Wait for it to finish.");
            self.toast(&t);
            return;
        }
        let title = self
            .shared
            .info
            .as_ref()
            .filter(|d| d.id == id)
            .map(|d| d.title.clone())
            .or_else(|| self.shared.rows.iter().find(|r| r.id == id).map(|r| r.title.clone()))
            .unwrap_or_default();
        self.shared.dl = Some(SharedDl { id, title, stage: DlStage::Downloading, done: 0, total: None });
        self.events.push(Event::Study);
        self.spawn(move |tx, http| {
            let dir = ankishared::download_dir();
            // packages of earlier adds were already imported (or failed): they are not kept
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    if e.path().extension().map(|x| x == "apkg").unwrap_or(false) {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
            // the page is read again just before: its download key is only valid for a while
            let r = ankishared::deck_info(&*http, id).and_then(|info| {
                let tx2 = tx.clone();
                ankishared::download(&*http, &info, &dir, &move |done, total| send(&tx2, Msg::SharedProgress(id, done, total)))
            });
            send(tx, Msg::SharedDownloaded(id, r));
        });
    }

    pub(super) fn on_shared_progress(&mut self, id: u32, done: u64, total: Option<u64>) {
        if let Some(d) = self.shared.dl.as_mut().filter(|d| d.id == id) {
            d.done = done;
            d.total = total;
        }
    }

    pub(super) fn on_shared_downloaded(&mut self, id: u32, r: Result<PathBuf, String>) {
        if self.shared.dl.as_ref().map(|d| d.id != id).unwrap_or(true) {
            return;
        }
        match r {
            Ok(path) if !self.study_importing => {
                if let Some(d) = self.shared.dl.as_mut() {
                    d.stage = DlStage::Importing;
                }
                self.study_import(path);
            }
            Ok(path) => {
                let _ = std::fs::remove_file(path);
                self.shared.dl = None;
                let t = self.shared_tr("Another import is running. Try again when it ends.");
                self.toast(&t);
            }
            Err(e) => {
                self.shared.dl = None;
                let t = self.shared_tr(&e);
                self.toast(&t);
            }
        }
        self.events.push(Event::Study);
    }

    /// Called after every import of a package: ends the add in progress, with a banner when it worked.
    pub(super) fn shared_after_import(&mut self, ok: bool) {
        let Some(d) = self.shared.dl.take_if(|d| d.stage == DlStage::Importing) else { return };
        let _ = std::fs::remove_file(ankishared::download_dir().join(format!("{}.apkg", d.id)));
        if ok {
            let title = self.shared_tr("Deck added");
            self.banner("info", &title, &d.title, "happy", None);
        }
        self.events.push(Event::Study);
    }

    /// Plays a sound of a sample note (only the site's preview files).
    pub fn shared_play(&mut self, url: &str) {
        if !url.starts_with(&format!("{}/shared/mpreview/", ankishared::SITE)) {
            return;
        }
        if self.study_audio.is_none() {
            match Mpv::new_audio(&self.rt.libmpv_candidates()) {
                Ok(m) => self.study_audio = Some(m),
                Err(e) => {
                    self.toast(&format!("The sound could not be played: {}", e));
                    return;
                }
            }
        }
        if let Some(m) = &self.study_audio {
            let _ = m.set("speed", "1");
            let _ = m.cmd(&["loadfile", url, "replace"]);
        }
    }

    /// The state "shared" of the bridge.
    pub fn shared_json(&self) -> Json {
        self.shared.json(self.study_importing)
    }
}

impl SharedState {
    fn pages(&self) -> usize {
        self.rows.len().div_ceil(PER_PAGE).max(1)
    }

    fn sorted(rows: &[DeckRow], sort: &str) -> Arc<Vec<DeckRow>> {
        let mut v = rows.to_vec();
        ankishared::sort_rows(&mut v, sort);
        Arc::new(v)
    }

    /// Takes a search of the browser. Gives (generation, words) when the server must be asked; the order, the page,
    /// a cached search or a too short one are handled here.
    fn plan_search(&mut self, query: &str, cat: &str, page: usize, sort: &str) -> Option<(u64, String)> {
        let sort = if SORTS.contains(&sort) { sort.to_string() } else { "rating".to_string() };
        let cat = if ankishared::category_label(cat).is_some() { cat.to_string() } else { String::new() };
        let words = ankishared::search_words(query, &cat);
        let sort_changed = sort != self.sort;
        self.query = query.trim().to_string();
        self.cat = cat;
        self.sort = sort.clone();
        self.page = page;
        if words.is_empty() || ankishared::too_short(&words) {
            // back to the front page (or a hint to type more)
            self.gen += 1;
            self.loading = false;
            self.words.clear();
            self.rows = Arc::new(vec![]);
            self.error = if words.is_empty() { String::new() } else { "Type at least 3 letters to search.".into() };
            return None;
        }
        if words == self.words && !self.loading && self.error.is_empty() {
            if sort_changed {
                self.rows = Self::sorted(&self.rows.clone(), &sort);
            }
            self.page = page.min(self.pages() - 1);
            return None;
        }
        let fresh = self.cache.iter().find(|(w, at, _)| *w == words && at.map(|t| t.elapsed().as_secs() < CACHE_SECS).unwrap_or(false)).map(|(_, _, r)| r.clone());
        self.gen += 1;
        self.error.clear();
        if let Some(rows) = fresh {
            self.loading = false;
            self.words = words;
            self.rows = Self::sorted(&rows, &sort);
            self.page = page.min(self.pages() - 1);
            return None;
        }
        self.loading = true;
        Some((self.gen, words))
    }

    /// The answer of a search. False when it is an old one (another search came after it).
    fn take_list(&mut self, gen: u64, words: String, r: Result<Vec<DeckRow>, String>) -> bool {
        if gen != self.gen {
            return false;
        }
        self.loading = false;
        match r {
            Ok(rows) => {
                let rows = Arc::new(rows);
                self.cache.retain(|(w, _, _)| *w != words);
                self.cache.push((words.clone(), Some(Instant::now()), rows.clone()));
                if self.cache.len() > 6 {
                    self.cache.remove(0);
                }
                self.rows = Self::sorted(&rows, &self.sort);
                self.words = words;
                self.page = self.page.min(self.pages() - 1);
            }
            Err(e) => {
                self.words.clear();
                self.rows = Arc::new(vec![]);
                self.error = e;
            }
        }
        true
    }

    fn take_info(&mut self, gen: u64, r: Result<DeckInfo, String>) -> bool {
        if gen != self.info_gen {
            return false;
        }
        self.info_loading = false;
        match r {
            Ok(d) => self.info = Some(d),
            Err(e) => self.info_error = e,
        }
        true
    }

    fn row_json(r: &DeckRow) -> Json {
        let total = r.up + r.down;
        Json::obj(vec![
            ("id", n(r.id as f64)),
            ("title", r.title.as_str().into()),
            ("up", n(r.up as f64)),
            ("down", n(r.down as f64)),
            // share of thumbs up, 0..100 (-1 without votes)
            ("score", n(if total == 0 { -1.0 } else { (r.up as f64 * 100.0 / total as f64).round() })),
            ("notes", n(r.notes as f64)),
            ("audio", n(r.audio as f64)),
            ("images", n(r.images as f64)),
            ("updated", n(r.mtime as f64)),
            ("date", ankishared::date_text(r.mtime).into()),
        ])
    }

    fn info_json(&self) -> Json {
        let Some(d) = self.info.as_ref() else {
            return Json::obj(vec![("id", n(self.info_id as f64)), ("loading", Json::Bool(self.info_loading)), ("error", self.info_error.as_str().into())]);
        };
        let samples: Vec<Json> = d
            .samples
            .iter()
            .map(|sample| {
                let (mut images, mut sounds, mut fields) = (vec![], vec![], vec![]);
                for (name, value) in sample {
                    for (kind, file) in ankishared::media_refs(value) {
                        let url = Json::Str(ankishared::preview_url(d.id, &file, d.updated));
                        if kind == "image" {
                            images.push(url)
                        } else {
                            sounds.push(url)
                        }
                    }
                    let text = ankishared::plain(value);
                    if !text.is_empty() {
                        fields.push(Json::obj(vec![("name", name.as_str().into()), ("text", text.into())]));
                    }
                }
                Json::obj(vec![("fields", Json::Arr(fields)), ("images", Json::Arr(images)), ("sounds", Json::Arr(sounds))])
            })
            .collect();
        let reviews: Vec<Json> = d
            .reviews
            .iter()
            .filter(|r| !r.body.trim().is_empty())
            .take(20)
            .map(|r| {
                Json::obj(vec![
                    ("up", Json::Bool(r.up)),
                    ("body", r.body.trim().into()),
                    ("reply", r.reply.trim().into()),
                    ("date", ankishared::date_text(r.time).into()),
                ])
            })
            .collect();
        let total = d.up + d.down;
        Json::obj(vec![
            ("id", n(d.id as f64)),
            ("loading", Json::Bool(false)),
            ("error", "".into()),
            ("title", d.title.as_str().into()),
            ("tags", d.tags.clone().into()),
            ("size", ankishared::size_text(d.size).into()),
            ("updated", n(d.updated as f64)),
            ("date", ankishared::date_text(d.updated).into()),
            ("html", ankishared::description_html(d).into()),
            ("up", n(d.up as f64)),
            ("down", n(d.down as f64)),
            ("score", n(if total == 0 { -1.0 } else { (d.up as f64 * 100.0 / total as f64).round() })),
            ("tooNew", Json::Bool(d.too_new_for_rating)),
            ("notes", n(d.notes as f64)),
            ("audio", n(d.audio as f64)),
            ("images", n(d.images as f64)),
            ("samples", Json::Arr(samples)),
            ("reviews", Json::Arr(reviews)),
            ("reviewCount", n(d.reviews.len() as f64)),
            ("byAuthor", n(d.by_author as f64)),
            ("original", d.original_name.as_str().into()),
            ("url", format!("{}/shared/info/{}", ankishared::SITE, d.id).into()),
        ])
    }

    fn json(&self, importing: bool) -> Json {
        let cats: Vec<Json> = ankishared::categories()
            .iter()
            .map(|g| {
                let items: Vec<Json> = g.items.iter().map(|c| Json::obj(vec![("key", c.key.into()), ("label", c.label.into())])).collect();
                Json::obj(vec![("label", g.label.into()), ("icon", g.icon.into()), ("items", Json::Arr(items))])
            })
            .collect();
        let pages = self.pages();
        let page = self.page.min(pages - 1);
        let results: Vec<Json> = self.rows.iter().skip(page * PER_PAGE).take(PER_PAGE).map(Self::row_json).collect();
        let dl = match &self.dl {
            Some(d) => Json::obj(vec![
                ("id", n(d.id as f64)),
                ("title", d.title.as_str().into()),
                ("stage", (if d.stage == DlStage::Importing { "import" } else { "download" }).into()),
                ("done", n(d.done as f64)),
                ("total", n(d.total.unwrap_or(0) as f64)),
                // -1 while the size is unknown
                ("pct", n(match d.total {
                    Some(t) if t > 0 => (d.done as f64 * 100.0 / t as f64).min(100.0).floor(),
                    _ => -1.0,
                })),
            ]),
            None => Json::Null,
        };
        Json::obj(vec![
            ("open", Json::Bool(self.open)),
            ("query", self.query.as_str().into()),
            ("cat", self.cat.as_str().into()),
            ("catLabel", ankishared::category_label(&self.cat).unwrap_or("").into()),
            ("sort", (if self.sort.is_empty() { "rating" } else { self.sort.as_str() }).into()),
            ("page", n(page as f64)),
            ("pages", n(pages as f64)),
            ("total", n(self.rows.len() as f64)),
            // the site warns when a search matched too much (it stops at about 2500)
            ("tooMany", Json::Bool(self.rows.len() >= 2500)),
            ("searched", Json::Bool(!self.words.is_empty())),
            ("loading", Json::Bool(self.loading)),
            ("error", self.error.as_str().into()),
            ("categories", Json::Arr(cats)),
            ("results", Json::Arr(results)),
            ("info", self.info_json()),
            ("dl", dl),
            ("importing", Json::Bool(importing)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u32, up: u32) -> DeckRow {
        DeckRow { id, title: format!("deck {}", id), up, ..Default::default() }
    }

    #[test]
    fn pages_and_sorts_cached_results() {
        let mut s = SharedState::default();
        let (gen, words) = s.plan_search(" ", "japanese", 0, "rating").unwrap();
        assert_eq!(words, "japanese");
        assert!(s.loading);
        let rows: Vec<DeckRow> = (1..=50).map(|i| row(i, i)).collect();
        assert!(s.take_list(gen, words, Ok(rows)));
        assert_eq!(s.pages(), 3);
        let j = s.json(false);
        assert_eq!(j.get("total").as_f64(), Some(50.0));
        assert_eq!(j.get("results").items().len(), PER_PAGE);
        assert_eq!(j.get("results").at(0).get("id").as_f64(), Some(50.0));
        // the same words: no request, only the page and the order change
        assert!(s.plan_search("", "japanese", 2, "title").is_none());
        let j = s.json(false);
        assert_eq!(j.get("page").as_f64(), Some(2.0));
        assert_eq!(j.get("results").items().len(), 2);
        // a stale answer is ignored
        assert!(!s.take_list(gen - 1, "other".into(), Ok(vec![row(1, 1)])));
        assert_eq!(s.rows.len(), 50);
        // too short: a hint, no results
        assert!(s.plan_search("n5", "", 0, "rating").is_none());
        assert!(s.error.contains("3 letters"));
        assert_eq!(s.json(false).get("results").items().len(), 0);
        // back to the category: answered from the cache
        assert!(s.plan_search("", "japanese", 0, "rating").is_none());
        assert!(!s.loading);
        assert_eq!(s.rows.len(), 50);
        // words plus a category ask the server; an unknown category is dropped
        assert_eq!(s.plan_search("core", "japanese", 0, "bogus").unwrap().1, "japanese core");
        assert_eq!(s.sort, "rating");
        assert_eq!(s.plan_search("core", "nope", 0, "notes").unwrap().1, "core");
        // a failed search shows its error
        let g = s.gen;
        assert!(s.take_list(g, "core".into(), Err("AnkiWeb is busy. Try again in a moment.".into())));
        assert_eq!(s.json(false).get("error").str(), "AnkiWeb is busy. Try again in a moment.");
    }

    #[test]
    fn info_json_lists_samples_and_media() {
        let mut s = SharedState { info_gen: 2, info_id: 9, ..Default::default() };
        let d = DeckInfo {
            id: 9,
            title: "T".into(),
            updated: 1_800_000_000,
            description: "<p>Hi <script>x</script></p>".into(),
            samples: vec![vec![("Front".into(), "犬[sound:0.mp3]".into()), ("Pic".into(), "[image:1.jpg]".into())]],
            ..Default::default()
        };
        assert!(!s.take_info(1, Ok(d.clone())));
        assert!(s.take_info(2, Ok(d)));
        let j = s.json(false);
        let info = j.get("info");
        assert_eq!(info.get("html").str(), "<p>Hi </p>");
        let smp = info.get("samples").at(0);
        assert_eq!(smp.get("fields").items().len(), 1);
        assert_eq!(smp.get("fields").at(0).get("text").str(), "犬");
        assert_eq!(smp.get("images").at(0).str(), "https://ankiweb.net/shared/mpreview/9/1.jpg?1800000000");
        assert_eq!(smp.get("sounds").items().len(), 1);
        s.dl = Some(SharedDl { id: 9, title: "T".into(), stage: DlStage::Downloading, done: 50, total: Some(200) });
        assert_eq!(s.json(true).get("dl").get("pct").as_f64(), Some(25.0));
    }
}
