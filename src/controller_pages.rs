//! What the reader does with the pages of a chapter: upscaling them, recognizing the text written on them and
//! translating it. The work runs on other threads; the answers come back as events.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::mangaup;
use crate::ocr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// One failure of the engine is told to the user; the pages that fail after it stay quiet.
static WARNED: AtomicBool = AtomicBool::new(false);

/// Changes when a reader opens or closes a chapter. Each page read (OCR) or translated takes it when it is asked for;
/// work still waiting for its turn when the chapter is left is dropped, so a closed chapter does not keep the free
/// services (and the machine) busy for minutes.
static READER_GEN: AtomicU64 = AtomicU64::new(0);

fn reader_gen() -> u64 {
    READER_GEN.load(Ordering::SeqCst)
}

/// Pages being read now, with the reader generation that asked (key: provider|language|address), so one is not
/// asked for twice. Each is removed when its work ends, also when the work was dropped, so the page can be asked
/// for again (another chapter opened, the text forgotten).
static OCR_ASKED: std::sync::Mutex<Option<HashMap<String, u64>>> = std::sync::Mutex::new(None);

fn ocr_done(key: &str, gen: u64) {
    if let Some(m) = OCR_ASKED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        if m.get(key) == Some(&gen) {
            m.remove(key);
        }
    }
}

impl Core {
    // ----- upscaling -----

    /// The program of the upscaling engine, when there is one.
    pub fn upscale_engine(&self) -> Option<PathBuf> {
        mangaup::find_exe(&self.data_dir, &self.settings.general.up_engine)
    }

    /// The models of the engine and the one in use.
    pub fn upscale_models(&self) -> (Vec<String>, String) {
        let list = self.upscale_engine().map(|e| mangaup::models(&e)).unwrap_or_default();
        let g = &self.settings.general;
        let cur = if g.up_model.is_empty() { mangaup::DEFAULT_MODEL.to_string() } else { g.up_model.clone() };
        (list, cur)
    }

    /// Downloads the engine. `Event::UpscaleEngine` tells when it is done.
    pub fn upscale_install(&mut self) {
        self.toast("Downloading the upscaling engine (about 40 MB, once)…");
        let data = self.data_dir.clone();
        self.spawn(move |tx, http| {
            let r = mangaup::install(&*http, &data, &|_| {});
            send(tx, Msg::UpscaleInstalled(r));
        });
    }

    pub(super) fn on_upscale_installed(&mut self, r: Result<PathBuf, String>) {
        match r {
            Ok(_) => self.toast("The upscaling engine is ready."),
            Err(e) => self.toast(&e),
        }
        self.events.push(Event::UpscaleEngine);
    }

    /// Starts upscaling a page (once); `Event::Upscaled` brings the file with the result.
    pub fn upscale_page(&self, url: &str) {
        let g = &self.settings.general;
        let Some(exe) = self.upscale_engine() else { return };
        let model = if g.up_model.is_empty() { mangaup::DEFAULT_MODEL.to_string() } else { g.up_model.clone() };
        let scale = mangaup::scale_for(&model, g.up_scale);
        let key = format!("{}|{}|{}", url, model, scale);
        if !mangaup::ask(&key) {
            return;
        }
        let (data, tile, limit, gen, url) = (self.data_dir.clone(), g.up_tile, g.up_cache_mb as u64, mangaup::generation(), url.to_string());
        self.spawn(move |tx, http| {
            let r = (|| {
                let out = mangaup::cached(&data, &url, &model, scale);
                if out.is_file() {
                    return Ok(out);
                }
                let bytes = page_bytes(&*http, &data, &url)?;
                mangaup::run(&data, &exe, &model, scale, tile, &url, &bytes, gen)
            })();
            // Asked again when the page comes back on screen: the file on the disk answers at once.
            mangaup::forget(&key);
            send(tx, Msg::Upscaled(url, r));
            mangaup::trim_cache(&data, limit);
        });
    }

    /// The reader's switch was turned on: errors are told again.
    pub fn upscale_reset_warning(&self) {
        WARNED.store(false, Ordering::SeqCst);
    }

    pub(super) fn on_upscaled(&mut self, url: String, r: Result<PathBuf, String>) {
        if let Err(e) = &r {
            crate::debug::log("upscale", &format!("{}: {}", url, e));
            if e != "skipped" && !WARNED.swap(true, Ordering::SeqCst) {
                self.toast(e);
            }
        }
        self.reader_upscaled.insert(url.clone(), r.clone());
        self.events.push(Event::Upscaled(url, r));
    }

    // ----- text of the pages -----

    /// The file the page was saved in when it was downloaded.
    pub fn page_file(&self, url: &str) -> PathBuf {
        self.data_dir.join(crate::backup::image_file(url))
    }

    /// Reads the text of a page. `prepare` makes the picture the service takes (a small enough JPEG and its size); it runs on the worker thread.
    pub fn ocr_page(&self, url: &str, prepare: impl FnOnce() -> Result<Vec<ocr::Tile>, String> + Send + 'static) {
        let g = &self.settings.general;
        // Google Cloud Vision without a key cannot work: the free service is used instead.
        let provider = if g.ocr_provider == "vision" && g.ocr_key.trim().is_empty() { "ocrspace".to_string() } else { g.ocr_provider.clone() };
        let cfg = ocr::Cfg { provider, key: g.ocr_key.clone(), lang: g.ocr_lang.clone() };
        let url = url.to_string();
        let data = self.data_dir.clone();
        // A page already being read is not asked for twice. Another provider or language is another reading, and a
        // page asked for by an earlier opening of the reader (its work is dropped) is asked for again.
        let key = format!("{}|{}|{}", cfg.provider, cfg.lang, url);
        let gen = reader_gen();
        {
            let mut a = OCR_ASKED.lock().unwrap_or_else(|e| e.into_inner());
            let map = a.get_or_insert_with(Default::default);
            if map.get(&key) == Some(&gen) {
                return;
            }
            map.insert(key.clone(), gen);
        }
        self.spawn(move |tx, http| {
            // What was read before is kept on the disk: the free service allows only a few pages an hour.
            let file = ocr_cache_file(&data, &url, &cfg);
            let r = match read_ocr_cache(&file) {
                Some(v) => Ok(v),
                None => {
                    // At most two pages are read at a time (a whole chapter asked at once would stall the machine).
                    let _slot = Slot::take();
                    // the chapter was left while this page waited: nothing to do
                    if reader_gen() != gen {
                        ocr_done(&key, gen);
                        return;
                    }
                    let got = page_bytes(&*http, &data, &url);
                    let r = got.and_then(|_| prepare()).and_then(|tiles| ocr::recognize_tiles(&*http, &cfg, &tiles));
                    if let Ok(v) = &r {
                        write_ocr_cache(&file, v);
                    }
                    r
                }
            };
            crate::debug::log("ocr", &match &r {
                Ok(v) => format!("{}: {} block(s)", url, v.len()),
                Err(e) => format!("{}: {}", url, e),
            });
            // The answer is kept by the reader (`reader_text`); from now on the page can be asked for again (a page
            // that failed, or whose text was forgotten, is read again; a good one comes from the disk at once).
            ocr_done(&key, gen);
            send(tx, Msg::PageText(url, r));
        });
    }

    pub fn on_page_text(&mut self, url: String, r: Result<Vec<ocr::Block>, String>) {
        if let Err(e) = &r {
            // Pages are read as they appear; one message a minute is enough.
            static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
            let mut last = LAST.lock().unwrap_or_else(|p| p.into_inner());
            if last.map(|t| t.elapsed().as_secs() >= 60).unwrap_or(true) {
                *last = Some(std::time::Instant::now());
                self.toast(e);
            }
        }
        if let Ok(blocks) = &r {
            let known = self.known_blocks(blocks);
            PAGE_KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(url.clone(), known);
        }
        self.reader_text.insert(url.clone(), r.clone());
        self.events.push(Event::PageText(url, r));
    }

    // ----- study in the readers (see `readstudy`) -----

    /// For each bubble of a page: written in the study language with every word known. The language of the bubbles
    /// is the one the text is read in (OCR language), told from the kana when that is automatic.
    fn known_blocks(&self, blocks: &[ocr::Block]) -> Vec<bool> {
        let study = self.study_lang3();
        if study.is_empty() || blocks.is_empty() {
            return vec![false; blocks.len()];
        }
        let ocr = self.settings.general.ocr_lang.trim().to_lowercase();
        let lang = match ocr.as_str() {
            "jpn" | "eng" | "por" | "spa" | "kor" | "fra" | "deu" | "ita" => ocr.clone(),
            _ => crate::readstudy::guess_lang3(&blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" ")),
        };
        if lang != study {
            return vec![false; blocks.len()];
        }
        crate::readstudy::known_texts(&self.knowledge, &lang, &blocks.iter().map(|b| b.text.clone()).collect::<Vec<_>>())
    }

    /// Which bubbles of a page could be a quiz card (same order as the page's text blocks).
    pub fn page_known(&self, url: &str) -> Vec<bool> {
        PAGE_KNOWN.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(url).cloned()).unwrap_or_default()
    }

    /// The answer to a quiz card of a reader ("I understood it" or not): every word of the line is recorded in the
    /// knowledge file, like the quizzes of the player. `lang` is a 2- or 3-letter code ("" = the study language).
    pub fn reader_study_answer(&mut self, text: &str, lang: &str, correct: bool) {
        let lang = if lang.trim().is_empty() { self.study_lang3() } else { crate::learn::lang3(&crate::util::lang_base(lang)) };
        if lang.is_empty() || text.trim().is_empty() {
            return;
        }
        self.time_input();
        self.knowledge.record_line(&lang, text, correct);
        self.knowledge.save();
    }

    /// Translates pieces of text for the reader (the answer has the same `id`). The language and the translator are those of the subtitles.
    pub fn translate_texts(&self, id: u64, texts: Vec<String>) {
        let sub = &self.settings.subtitles;
        let target = if sub.target_lang.trim().is_empty() { self.ui_lang() } else { sub.target_lang.clone() };
        let source = ocr::source_code(&self.settings.general.ocr_lang).to_string();
        self.translate_texts_to(id, texts, &source, &target);
    }

    /// The same with the languages given ("auto" as the source lets the translator find out).
    pub fn translate_texts_to(&self, id: u64, texts: Vec<String>, source: &str, target: &str) {
        self.translate_texts_kind(id, texts, source, target, "");
    }

    /// Translation for the readers. `kind` is "manga" (the bubbles of one page, translated together for context) or "novel"
    /// (paragraphs, up to ten requests at the same time). What was translated before is kept in the data folder
    /// ("translations"), so opening the same chapter again does not translate it again.
    pub fn translate_texts_kind(&self, id: u64, texts: Vec<String>, source: &str, target: &str, kind: &str) {
        // the place of Preferences > Translators the text comes from, when the interface does not say it
        let use_id = match kind {
            "novel" => "readers_text",
            "manga" => "readers_images",
            _ => "other",
        };
        self.translate_texts_for(id, texts, source, target, kind, use_id);
    }

    /// The same, with the translator of `use_id` ("readers_images", "bubbles", "study_cards"… see `translate::USES`).
    pub fn translate_texts_for(&self, id: u64, texts: Vec<String>, source: &str, target: &str, kind: &str, use_id: &str) {
        let st = self.settings.clone();
        let use_id = use_id.to_string();
        let (source, target, kind) = (source.to_string(), util::lang_base(target).to_string(), kind.to_string());
        let data = self.data_dir.clone();
        let gen = reader_gen();
        let gate = self.local_ai_gate(translate::uses_local_ai(&use_id, &st));
        self.spawn(move |tx, http| {
            // One reader translation at a time in the whole app: the pages of a chapter queue up instead of all asking
            // the same (maybe unreachable) services at once.
            let _turn = READER_TURN.lock().unwrap_or_else(|e| e.into_inner());
            // the chapter was left while this waited for its turn: no answer is wanted any more
            if reader_gen() != gen {
                return;
            }
            let tx2 = tx.clone();
            // A page of bubbles is one request (the context makes the dialogue natural); a chapter goes in pieces, side by side.
            let (threads, max_batch) = if gate.is_some() {
                // the local AI: one request at a time, small pieces (a 3B model on a CPU is slow)
                (1, translate::LOCAL_MAX_ITEMS)
            } else if kind == "novel" {
                (translate::READER_THREADS, 30)
            } else {
                (1, 60)
            };
            // each translator keeps its own saved translations ("auto" keeps the folder of before)
            let key = translate::cache_key(&use_id, &st);
            let r = translate::for_use(&use_id, &st, &*http).map(|t| controller_translators::gated(t, &gate, None)).and_then(|t| {
                translate::translate_cached(t.as_ref(), &texts, &source, &target, &kind, &key, &data, threads, max_batch, &|d, n| send(&tx2, Msg::TranslateProgress(gen, id, d, n)))
            });
            send(tx, Msg::Translated(gen, id, r));
        });
    }

    /// A reader opened or closed a chapter: the work asked for before is dropped, and the answers kept for the
    /// interface are emptied (the readers number their requests from the same start each time, and the old answers
    /// would only grow).
    pub(super) fn reader_changed(&mut self) {
        READER_GEN.fetch_add(1, Ordering::SeqCst);
        self.reader_translations.clear();
        self.reader_tr_progress.clear();
        // The text read on the pages and the upscaled files of the chapter that was left: both are kept on the disk
        // (asked again, they come back at once), and kept here they made the reader state grow with every chapter.
        self.reader_text.clear();
        self.reader_upscaled.clear();
        if let Some(m) = PAGE_KNOWN.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            m.clear();
        }
    }

    pub(super) fn on_translate_progress(&mut self, gen: u64, id: u64, done: usize, total: usize) {
        // an answer for a chapter that was left
        if gen != reader_gen() {
            return;
        }
        self.reader_tr_progress.insert(id, (done, total));
        self.events.push(Event::TranslateProgress(id));
    }

    pub(super) fn on_translated(&mut self, gen: u64, id: u64, r: Result<Vec<String>, String>) {
        if gen != reader_gen() {
            return;
        }
        self.reader_translations.insert(id, r.clone());
        self.events.push(Event::Translated(id, r));
    }
}

static READER_TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// For every page read: which of its bubbles are all known in the study language (see `known_blocks`).
static PAGE_KNOWN: std::sync::Mutex<Option<HashMap<String, Vec<bool>>>> = std::sync::Mutex::new(None);

/// The file where the text read from a page is kept. The local manga server has another port at each start, so only
/// the path counts for its pages.
fn ocr_cache_file(data: &std::path::Path, url: &str, cfg: &ocr::Cfg) -> PathBuf {
    let mut key = url.to_string();
    for pre in ["http://127.0.0.1:", "http://localhost:"] {
        if let Some(rest) = key.strip_prefix(pre) {
            key = format!("local{}", rest.find('/').map(|i| &rest[i..]).unwrap_or(""));
        }
    }
    let h = crate::translate::fnv(&format!("{}|{}|{}", key, cfg.provider, cfg.lang));
    data.join("ocr").join(format!("{:016x}.json", h))
}

fn read_ocr_cache(file: &std::path::Path) -> Option<Vec<ocr::Block>> {
    let j = crate::json::Json::parse(&std::fs::read_to_string(file).ok()?).ok()?;
    Some(j.items().iter().map(|b| ocr::Block { text: b.get("t").str(), x: b.get("x").as_f64().unwrap_or(0.0), y: b.get("y").as_f64().unwrap_or(0.0), w: b.get("w").as_f64().unwrap_or(0.0), h: b.get("h").as_f64().unwrap_or(0.0) }).collect())
}

fn write_ocr_cache(file: &std::path::Path, v: &[ocr::Block]) {
    use crate::json::Json;
    let j = Json::Arr(v.iter().map(|b| Json::obj(vec![("t", Json::Str(b.text.clone())), ("x", Json::Num(b.x)), ("y", Json::Num(b.y)), ("w", Json::Num(b.w)), ("h", Json::Num(b.h))])).collect());
    if let Some(p) = file.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let _ = std::fs::write(file, j.dump());
}

/// A place among the pages being read (see `ocr_page`).
struct Slot;

static BUSY: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
static FREE: std::sync::Condvar = std::sync::Condvar::new();

impl Slot {
    fn take() -> Slot {
        let mut n = BUSY.lock().unwrap_or_else(|e| e.into_inner());
        while *n >= 2 {
            n = FREE.wait(n).unwrap_or_else(|e| e.into_inner());
        }
        *n += 1;
        Slot
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut n = BUSY.lock().unwrap_or_else(|e| e.into_inner());
        *n = n.saturating_sub(1);
        FREE.notify_one();
    }
}

/// The page as it was downloaded: from the disk cache, or from the web when it is not there yet.
fn page_bytes(http: &dyn crate::http::Http, data: &std::path::Path, url: &str) -> Result<Vec<u8>, String> {
    let file = data.join(crate::backup::image_file(url));
    if let Ok(b) = std::fs::read(&file) {
        if !b.is_empty() {
            return Ok(b);
        }
    }
    let req = crate::http::Request::get(&util::direct_image_url(url))
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .timeout(40);
    let r = http.send(&req)?;
    if !r.ok() || r.body.is_empty() {
        return Err(format!("The page could not be downloaded ({}).", r.status));
    }
    super::keep_picture(&file, &r.body);
    Ok(r.body)
}

/// Escapes text for the HTML of a card and keeps its line breaks.
fn card_html(s: &str) -> String {
    s.trim().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace("\r\n", "\n").replace('\n', "<br>")
}

/// The two fields of a picture card (note type "Basic"): the front has the cut-out of the page and the text read in
/// it, the back the translation and where the picture came from.
fn picture_fields(media_name: &str, text: &str, translation: &str, source: &str) -> Vec<String> {
    let mut front = format!("<img src=\"{}\">", media_name);
    if !text.trim().is_empty() {
        front.push_str(&format!("<br><big>{}</big>", card_html(text)));
    }
    let mut back = card_html(translation);
    if !source.trim().is_empty() {
        if !back.is_empty() {
            back.push_str("<br><br>");
        }
        back.push_str(&format!("<small>{}</small>", card_html(source)));
    }
    vec![front, back]
}

impl Core {
    /// Saves a cut-out of a page of the Images reader as a Study card. `path` is the picture the interface cut (a
    /// temporary file): it is kept in the Study media folder as WebP and the temporary file is removed. `deck` 0 uses
    /// the app's deck "Seu Reels" (made when missing).
    pub fn study_add_picture(&mut self, path: &str, text: &str, translation: &str, deck: i64, source: &str) {
        let bytes = match std::fs::read(path) {
            Ok(b) if !b.is_empty() => b,
            _ => {
                self.toast("The picture could not be cut from the page.");
                return;
            }
        };
        let _ = std::fs::remove_file(path);
        // the media folder keeps WebP (the original bytes when the picture cannot be converted)
        let data = crate::webpstore::for_disk(&bytes).into_owned();
        let ext = if crate::webpstore::is_webp(&data) { "webp" } else { "png" };
        let media = self.study_media_dir();
        let _ = std::fs::create_dir_all(&media);
        let now = Self::now_secs();
        let mut k = 0;
        let mut name = format!("page_{}.{}", now, ext);
        while media.join(&name).exists() {
            k += 1;
            name = format!("page_{}_{}.{}", now, k, ext);
        }
        if !util::safe_media_name(&name) || std::fs::write(media.join(&name), &data).is_err() {
            self.toast("The picture could not be saved in the Study folder.");
            return;
        }
        let did = if deck != 0 && self.study.decks.iter().any(|d| d.id == deck) {
            deck
        } else {
            // the app's own deck, where every card the app makes goes
            match self.study.app_deck() {
                Some(id) => id,
                None => {
                    self.toast("Choose a deck.");
                    return;
                }
            }
        };
        match self.study.add_note(did, crate::study::BASIC, picture_fields(&name, text, translation, source), now) {
            Ok(_) => {
                self.save_study();
                let deck_name = self.study.deck_name(did);
                self.toast(&format!("Saved to the deck {}.", deck_name));
            }
            Err(e) => {
                let _ = std::fs::remove_file(media.join(&name));
                self.toast(&e);
            }
        }
        self.events.push(Event::Study);
    }
}

#[cfg(test)]
mod picture_card_tests {
    use super::picture_fields;

    #[test]
    fn fields_escape_text_and_keep_lines() {
        let f = picture_fields("page_1.webp", "a <b>\nc", "x & y", "Title · ch. 3 p. 4");
        assert_eq!(f[0], "<img src=\"page_1.webp\"><br><big>a &lt;b&gt;<br>c</big>");
        assert_eq!(f[1], "x &amp; y<br><br><small>Title · ch. 3 p. 4</small>");
    }

    #[test]
    fn empty_text_gives_picture_only() {
        let f = picture_fields("p.webp", "  ", "", "");
        assert_eq!(f, vec!["<img src=\"p.webp\">".to_string(), String::new()]);
    }
}
