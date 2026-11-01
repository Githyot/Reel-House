//! Work running in the background, listed for the progress panel (bottom left of the window), each with
//! a way to cancel it. This file is part of `controller` (it needs the private parts of `Core`).
use super::*;

/// One line of the progress panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    /// Stable id: "dl:<hash>", "translate" or "find".
    pub key: String,
    /// What it is about: the title of the show.
    pub title: String,
    /// What is being done, as a fixed sentence ("Downloading", "Translating subtitle"): the interface translates it.
    pub what: &'static str,
    /// Numbers that go after it ("3 of 12 files · 41%").
    pub detail: String,
    /// 0 to 1; `None` when the end cannot be told.
    pub fraction: Option<f64>,
}

/// A subtitle translation in progress.
pub struct TransJob {
    pub lang: String,
    pub pct: u8,
    pub cancel: Arc<AtomicBool>,
}

/// The search for the other episodes of a season.
pub struct Finding {
    pub title: String,
    pub total: usize,
    pub done: usize,
    pub cancel: Arc<AtomicBool>,
}

impl Core {
    /// Everything running right now.
    pub fn tasks(&self) -> Vec<Task> {
        let mut v = Vec::new();
        if let Some(f) = &self.finding {
            v.push(Task {
                key: "find".into(),
                title: f.title.clone(),
                what: "Finding episodes",
                detail: format!("{} of {}", f.done.min(f.total), f.total),
                fraction: Some(if f.total == 0 { 0.0 } else { f.done as f64 / f.total as f64 }),
            });
        }
        for d in self.downloads.iter().filter(|d| d.saved && !d.complete && self.dl_running.contains(&d.hash)) {
            let total = d.files.len();
            let got = d.files.iter().filter(|f| f.done).count();
            v.push(Task {
                key: format!("dl:{}", d.hash),
                title: d.title.clone(),
                what: "Downloading",
                detail: if total > 1 { format!("{} of {} files · {:.0}%", got, total, d.percent) } else { format!("{:.0}%", d.percent) },
                fraction: if d.percent > 0.0 { Some((d.percent / 100.0).clamp(0.0, 1.0)) } else { None },
            });
        }
        if let Some(t) = &self.translating {
            v.push(Task {
                key: "translate".into(),
                title: util::lang_name(&t.lang),
                what: "Translating subtitle",
                detail: format!("{}%", t.pct),
                fraction: Some(t.pct as f64 / 100.0),
            });
        }
        v
    }

    /// Stops a task from the panel.
    pub fn cancel_task(&mut self, key: &str) {
        match key {
            "find" => {
                if let Some(f) = self.finding.take() {
                    f.cancel.store(true, Ordering::SeqCst);
                }
            }
            "translate" => self.cancel_translation(),
            k => {
                if let Some(hash) = k.strip_prefix("dl:") {
                    // Stops the download and frees what was already downloaded.
                    self.remove_torrent(hash);
                }
            }
        }
    }
}
