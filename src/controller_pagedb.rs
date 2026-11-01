//! The translation database of the chapter being read (see `pagedb`). Part of `controller`.
use super::*;
use crate::pagedb::{self, Bubble};

impl Core {
    /// The file and contents for the chapter in the reader, loaded from disk the first time.
    fn pagedb_current(&mut self) -> Option<&mut pagedb::Chapter> {
        let rd = self.manga_reader.as_ref()?;
        let (sid, stitle, cid, clabel) = (rd.series.id.clone(), rd.series.title.clone(), rd.chapter.id.clone(), rd.chapter.label());
        let same = matches!(&self.page_db, Some((_, c)) if c.series_id == sid && c.chapter_id == cid);
        if !same {
            let file = pagedb::file_for(&self.data_dir, &sid, &stitle, &cid, &clabel);
            let mut c = std::fs::read_to_string(&file).ok().and_then(|t| pagedb::Chapter::from_markdown(&t).ok()).unwrap_or_default();
            c.series_id = sid;
            c.series_title = stitle;
            c.chapter_id = cid;
            c.chapter_label = clabel;
            if c.language.is_empty() {
                c.language = self.settings.general.reader_translate.clone();
            }
            self.page_db = Some((file, c));
        }
        self.page_db.as_mut().map(|(_, c)| c)
    }

    /// The database as the reader shows it (only when it belongs to the chapter on screen).
    pub fn pagedb_json(&self) -> Json {
        match (&self.page_db, self.manga_reader.as_ref()) {
            (Some((file, c)), Some(rd)) if c.series_id == rd.series.id && c.chapter_id == rd.chapter.id => {
                Json::obj(vec![("file", Json::Str(file.display().to_string())), ("pages", c.to_json())])
            }
            _ => Json::obj(vec![("file", Json::Str(String::new())), ("pages", Json::Arr(vec![]))]),
        }
    }

    pub fn pagedb_open(&mut self) {
        let _ = self.pagedb_current();
        self.events.push(Event::MangaPages);
    }

    fn pagedb_save(&mut self) {
        if let Some((file, c)) = &self.page_db {
            if c.is_empty() && !file.exists() {
                return;
            }
            if let Some(p) = file.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            if let Err(e) = std::fs::write(file, c.to_markdown()) {
                self.events.push(Event::Toast(format!("The translation could not be saved: {}", e)));
            }
        }
        self.events.push(Event::MangaPages);
    }

    /// Creates or changes bubble `index` of page `page` (1 based). `index` = -1 adds one.
    #[allow(clippy::too_many_arguments)]
    pub fn pagedb_set_bubble(&mut self, page: usize, index: i64, b: Bubble) {
        let Some(c) = self.pagedb_current() else { return };
        let p = c.page_mut(page.max(1));
        let mut b = b;
        b.html = pagedb::clean_html(&b.html);
        b.clean_look();
        // f64::clamp panics on NaN bounds or when min > max, and the numbers come from QML,
        // so a broken value becomes a sensible default and the corner keeps room for the box
        fn finite_or(v: f64, d: f64) -> f64 {
            if v.is_finite() {
                v
            } else {
                d
            }
        }
        b.x = finite_or(b.x, 0.0).clamp(0.0, 0.995);
        b.y = finite_or(b.y, 0.0).clamp(0.0, 0.995);
        b.w = finite_or(b.w, 0.1).clamp(0.005, 1.0 - b.x);
        b.h = finite_or(b.h, 0.1).clamp(0.005, 1.0 - b.y);
        if index >= 0 && (index as usize) < p.bubbles.len() {
            p.bubbles[index as usize] = b;
        } else if b.ocr >= 0 {
            // an OCR block corrected for the first time
            match p.bubbles.iter().position(|x| x.ocr == b.ocr) {
                Some(i) => p.bubbles[i] = b,
                None => p.bubbles.push(b),
            }
        } else {
            p.bubbles.push(b);
        }
        self.pagedb_save();
    }

    pub fn pagedb_remove_bubble(&mut self, page: usize, index: usize) {
        let Some(c) = self.pagedb_current() else { return };
        let p = c.page_mut(page.max(1));
        if index < p.bubbles.len() {
            // an OCR block stays marked as deleted, so the reader does not show it again; a drawn box goes away
            if p.bubbles[index].ocr >= 0 {
                p.bubbles[index].deleted = true;
            } else {
                p.bubbles.remove(index);
            }
        }
        self.pagedb_save();
    }

    /// The edit operations of the picture of page `page` (a JSON array as text).
    pub fn pagedb_set_ops(&mut self, page: usize, ops: &str) {
        let ok = Json::parse(ops).map(|j| matches!(j, Json::Arr(_))).unwrap_or(false);
        if !ok && !ops.trim().is_empty() {
            return;
        }
        let Some(c) = self.pagedb_current() else { return };
        c.page_mut(page.max(1)).ops = ops.trim().to_string();
        self.pagedb_save();
    }

    /// Copies the chapter's file to `path` (to send it to someone).
    pub fn pagedb_export(&mut self, path: &Path) {
        let Some(c) = self.pagedb_current() else { return };
        let md = c.to_markdown();
        match std::fs::write(path, md) {
            Ok(()) => self.toast("Translation saved."),
            Err(e) => self.toast(&format!("Could not save the file: {}", e)),
        }
    }

    /// Reads a translation file made on another computer and keeps it where its chapter looks for it. When it is the
    /// chapter on screen, its bubbles and picture edits show at once.
    pub fn pagedb_import(&mut self, path: &Path) {
        let r = std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| pagedb::Chapter::from_markdown(&t));
        match r {
            Ok(c) if !c.series_id.is_empty() && !c.chapter_id.is_empty() => {
                let file = pagedb::file_for(&self.data_dir, &c.series_id, &c.series_title, &c.chapter_id, &c.chapter_label);
                if let Some(p) = file.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                if let Err(e) = std::fs::write(&file, c.to_markdown()) {
                    self.toast(&format!("Could not keep the file: {}", e));
                    return;
                }
                self.page_db = None;
                let _ = self.pagedb_current();
                self.toast(&format!("Translation of {} — {} added.", c.series_title, c.chapter_label));
                self.events.push(Event::MangaPages);
            }
            Ok(_) => self.toast("The file does not say which series and chapter it belongs to."),
            Err(e) => self.toast(&e),
        }
    }
}
