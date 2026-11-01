//! What the Images and Text modes read from their sources, kept so a list, a series or a chapter opened again shows
//! at once instead of being asked for again: in memory (the last few hundred answers) and on the disk under
//! `<data dir>/cache/images` and `<data dir>/cache/text` (the `cache` folder stays out of backups).
//!
//! Each answer carries the time it was saved; the callers decide how old is too old with `fresh` and the TTL
//! constants below. An answer past its time is still useful: it is shown while the new one is fetched, and it is
//! what is left when the source cannot be reached.
use crate::json::Json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Lists of a source (popular, latest, a search): they change during the day.
pub const LIST_TTL: u64 = 30 * 60;
/// A series and its chapters: shown at once, asked for again in the background after this.
pub const SERIES_TTL: u64 = 6 * 3600;
/// The addresses of the pictures of a chapter of the manga server (they only change if the source re-uploads them).
pub const PAGES_TTL: u64 = 7 * 24 * 3600;
/// MangaDex hands out picture addresses of its delivery network that expire after about 15 minutes.
pub const MANGADEX_PAGES_TTL: u64 = 10 * 60;
/// The text of a chapter of a novel.
pub const TEXT_TTL: u64 = 30 * 24 * 3600;
/// The list of plugins of the Text mode repository.
pub const CATALOG_TTL: u64 = 6 * 3600;
/// Files not used (written or read) for this long are removed by `prune`.
pub const KEEP_ON_DISK: u64 = 60 * 24 * 3600;
/// Answers kept in memory before the oldest go.
const MEM_MAX: usize = 400;

/// An answer found in the cache, and how old it is (seconds).
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub data: Json,
    pub age: u64,
}

/// Is an answer of this age still good for `ttl` seconds?
pub fn fresh(age: u64, ttl: u64) -> bool {
    age < ttl
}

/// The answers in memory, for every store (key: the file of the answer), with the time they were saved.
static MEM: Mutex<Option<HashMap<PathBuf, (u64, Json)>>> = Mutex::new(None);

/// One folder of answers ("images" or "text").
#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Store {
        Store { dir: dir.into() }
    }

    /// The store of a mode under the data folder.
    pub fn of(data_dir: &Path, mode: &str) -> Store {
        Store::new(data_dir.join("cache").join(mode))
    }

    fn file(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}.json", crate::util::fnv_hex(key)))
    }

    pub fn get(&self, key: &str) -> Option<Hit> {
        self.get_at(key, crate::util::now_unix())
    }

    pub fn get_at(&self, key: &str, now: u64) -> Option<Hit> {
        let file = self.file(key);
        if let Some((at, data)) = MEM.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&file).cloned()) {
            return Some(Hit { data, age: now.saturating_sub(at) });
        }
        let j = Json::parse(&std::fs::read_to_string(&file).ok()?).ok()?;
        // two keys with the same hash: not this answer
        if j.get("key").str() != key {
            return None;
        }
        let at = j.get("at").as_u64().unwrap_or(0);
        let data = j.get("data").clone();
        // read again = still in use: `prune` goes by the file's time, so a series or chapter opened now and then
        // (offline above all) stays on disk instead of going 60 days after it was written
        touch(&file);
        remember(file, at, data.clone());
        Some(Hit { data, age: now.saturating_sub(at) })
    }

    pub fn put(&self, key: &str, data: Json) {
        self.put_at(key, data, crate::util::now_unix());
    }

    /// Keeps an answer (memory at once, the disk right after).
    pub fn put_at(&self, key: &str, data: Json, now: u64) {
        let file = self.file(key);
        remember(file.clone(), now, data.clone());
        let text = Json::obj(vec![("key", key.into()), ("at", (now as f64).into()), ("data", data)]).dump();
        let _ = std::fs::create_dir_all(&self.dir);
        // written beside and renamed, so a reader never finds half a file
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &file).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    /// Forgets an answer (a source that changed, a chapter whose pictures failed).
    pub fn forget(&self, key: &str) {
        let file = self.file(key);
        if let Some(m) = MEM.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            m.remove(&file);
        }
        let _ = std::fs::remove_file(file);
    }

    /// Removes the files older than `max_age` seconds (by their modification time). Returns how many went.
    pub fn prune(&self, max_age: u64) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return 0 };
        let mut n = 0;
        for e in rd.flatten() {
            let old = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).map(|d| d.as_secs() >= max_age).unwrap_or(false);
            if old && std::fs::remove_file(e.path()).is_ok() {
                n += 1;
            }
        }
        n
    }
}

fn touch(file: &Path) {
    if let Ok(f) = std::fs::File::options().write(true).open(file) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
}

fn remember(file: PathBuf, at: u64, data: Json) {
    let mut g = MEM.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if m.len() >= MEM_MAX && !m.contains_key(&file) {
        // the oldest quarter goes
        let mut ats: Vec<(u64, PathBuf)> = m.iter().map(|(k, (a, _))| (*a, k.clone())).collect();
        ats.sort();
        for (_, k) in ats.into_iter().take(MEM_MAX / 4) {
            m.remove(&k);
        }
    }
    m.insert(file, (at, data));
}

/// Old files of both stores go, once per run of the app, on a thread of their own.
pub fn prune_once(data_dir: &Path) {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let dir = data_dir.to_path_buf();
    std::thread::spawn(move || {
        for mode in ["images", "text"] {
            Store::of(&dir, mode).prune(KEEP_ON_DISK);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rh-readcache-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn ttl_decides_what_is_fresh() {
        assert!(fresh(0, LIST_TTL));
        assert!(fresh(LIST_TTL - 1, LIST_TTL));
        assert!(!fresh(LIST_TTL, LIST_TTL));
        assert!(!fresh(SERIES_TTL + 5, SERIES_TTL));
        assert!(MANGADEX_PAGES_TTL < PAGES_TTL);
    }

    #[test]
    fn answers_come_back_with_their_age() {
        let d = tmp("age");
        let st = Store::new(&d);
        st.put_at("list|a", Json::Arr(vec!["x".into()]), 1_000);
        let h = st.get_at("list|a", 1_000 + 120).unwrap();
        assert_eq!(h.age, 120);
        assert_eq!(h.data.items()[0].str(), "x");
        assert!(fresh(h.age, LIST_TTL));
        assert!(!fresh(st.get_at("list|a", 1_000 + LIST_TTL).unwrap().age, LIST_TTL));
        assert!(st.get_at("list|b", 1_000).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_disk_answers_when_the_memory_is_empty() {
        let d = tmp("disk");
        let st = Store::new(&d);
        st.put_at("series|mangadex:1", Json::obj(vec![("title", "T".into())]), 50);
        // as after a restart of the app
        MEM.lock().unwrap().as_mut().unwrap().clear();
        let h = st.get_at("series|mangadex:1", 60).unwrap();
        assert_eq!(h.data.get("title").str(), "T");
        assert_eq!(h.age, 10);
        st.forget("series|mangadex:1");
        assert!(st.get_at("series|mangadex:1", 60).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_newer_answer_replaces_the_old_one() {
        let d = tmp("replace");
        let st = Store::new(&d);
        st.put_at("k", "old".into(), 10);
        st.put_at("k", "new".into(), 20);
        let h = st.get_at("k", 25).unwrap();
        assert_eq!((h.data.str().as_str(), h.age), ("new", 5));
        let _ = std::fs::remove_dir_all(&d);
    }
}
