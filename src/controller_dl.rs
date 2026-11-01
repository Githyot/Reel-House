//! Torrents kept on disk: choosing one, downloading it in the background, marking the episodes,
//! and the list on the Torrents page. This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::downloads::{self, Download, EpState};
use crate::model::Video;

/// The "Keep downloaded" window: the torrents found for the first episode of a season.
pub struct DlPick {
    pub gen: u64,
    pub season: i64,
    pub seasons: Vec<i64>,
    pub loading: bool,
    pub list: Vec<Stream>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TorrentRow {
    /// Hash of the torrent, or "folder:NAME" for a folder the app has no record of.
    pub key: String,
    pub title: String,
    pub detail: String,
    pub poster: Option<String>,
    pub bytes: u64,
    pub saved: bool,
    pub percent: f64,
    pub downloading: bool,
}

/// Torrents of the same content, shown together with its cover.
#[derive(Clone, Debug, Default)]
pub struct TorrentGroup {
    pub key: String,
    pub title: String,
    pub poster: Option<String>,
    pub bytes: u64,
    pub saved_bytes: u64,
    pub rows: Vec<TorrentRow>,
}

#[derive(Clone, Debug, Default)]
pub struct Overview {
    pub total: u64,
    pub saved: u64,
    pub cache: u64,
    pub groups: Vec<TorrentGroup>,
}

pub fn load_covers(dir: &std::path::Path) -> HashMap<String, (String, String)> {
    let Ok(text) = std::fs::read_to_string(dir.join("covers.json")) else { return HashMap::new() };
    let Ok(j) = crate::json::Json::parse(&text) else { return HashMap::new() };
    j.entries().iter().map(|(k, v)| (k.clone(), (v.get("name").str(), v.get("poster").str()))).collect()
}

fn save_covers(dir: &std::path::Path, covers: &HashMap<String, (String, String)>) {
    let pairs: Vec<(String, crate::json::Json)> = covers
        .iter()
        .map(|(k, (n, p))| (k.clone(), crate::json::Json::obj(vec![("name", crate::json::Json::Str(n.clone())), ("poster", crate::json::Json::Str(p.clone()))])))
        .collect();
    let _ = std::fs::write(dir.join("covers.json"), crate::json::Json::Obj(pairs).dump());
}

/// Looks a title up in the catalog to get its cover: series first, then movies.
fn find_cover(http: &dyn Http, title: &str) -> Option<(String, String)> {
    let words: Vec<&str> = title.split_whitespace().collect();
    let mut tries = vec![title.to_string()];
    if words.len() > 2 {
        tries.push(words[..2].join(" "));
    }
    for q in tries {
        for kind in ["series", "movie"] {
            let url = format!("https://v3-cinemeta.strem.io/catalog/{}/top/search={}.json", kind, crate::util::url_encode(&q));
            if let Ok(j) = crate::http::get_json(http, &url, 15) {
                if let Some(m) = j.get("metas").items().iter().find(|m| !m.get("poster").str().is_empty()) {
                    return Some((m.get("name").str(), m.get("poster").str()));
                }
            }
        }
    }
    None
}

/// Follows a download in the engine until it finishes, reporting progress.
fn monitor(tx: &Sender<Msg>, http: &dyn Http, base: &str, id: u64, hash: &str, files: &[(usize, u64)]) {
    let rq = Rqbit { http, base: base.to_string() };
    let mut misses = 0;
    loop {
        match rq.stats(id) {
            Some(st) => {
                misses = 0;
                let (mut got, mut total) = (0u64, 0u64);
                let mut done = Vec::new();
                for (idx, len) in files {
                    let p = st.file_progress.get(*idx).copied().unwrap_or(0).min(*len);
                    got += p;
                    total += *len;
                    if *len > 0 && p >= *len {
                        done.push(*idx);
                    }
                }
                let complete = st.finished || (!files.is_empty() && done.len() == files.len());
                let percent = if total > 0 { got as f64 * 100.0 / total as f64 } else { st.percent() };
                if tx.send(Msg::DlProgress(hash.to_string(), percent, done, complete)).is_err() || complete {
                    return;
                }
            }
            None => {
                misses += 1;
                if misses > 20 {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

impl Core {
    // ----- what the episode rows show -----

    pub(super) fn season_episode_of(&self, video_id: &str) -> Option<(i64, i64)> {
        let d = self.detail.as_ref()?;
        let (s, e) = match d.meta.videos.iter().find(|v| v.id == video_id) {
            Some(v) if v.season.is_some() || v.episode.is_some() => (v.season, v.episode),
            _ => parse_episode_id(video_id),
        };
        Some((s.unwrap_or(1), e?))
    }

    /// Saved on disk, being downloaded, or only online.
    pub fn episode_state(&self, v: &Video) -> EpState {
        let Some(d) = self.detail.as_ref() else { return EpState::Online };
        let (Some(e), s) = (v.episode, v.season.unwrap_or(1)) else { return EpState::Online };
        downloads::episode_state(&self.downloads, &self.data_dir.join("torrents"), &d.meta.id, s, e)
    }

    /// The copy on disk of an episode, as a stream that goes first in the list.
    pub(super) fn local_stream_for(&self, video_id: &str) -> Option<Stream> {
        let d = self.detail.as_ref()?;
        let (s, e) = self.season_episode_of(video_id)?;
        let (dl, f, path) = downloads::saved_for(&self.downloads, &self.data_dir.join("torrents"), &d.meta.id, s, e)?;
        Some(Stream {
            addon: "Saved on disk".to_string(),
            name: if dl.headline.is_empty() { streams::res_label(dl.res) } else { dl.headline.clone() },
            desc: dl.lines.join("\n"),
            local: Some(path.to_string_lossy().into_owned()),
            info_hash: Some(dl.hash.clone()),
            filename: f.path.rsplit('/').next().map(|n| n.to_string()),
            res: dl.res,
            seeders: dl.seeders,
            size_mb: dl.size_mb,
            ..Default::default()
        })
    }

    // ----- choosing what to keep -----

    /// Opens the "Keep downloaded" window for the season on screen.
    pub fn dl_open(&mut self) {
        let Some(d) = self.detail.as_ref() else { return };
        let seasons: Vec<i64> = d.meta.seasons().into_iter().filter(|s| *s >= 1).collect();
        let season = if seasons.contains(&d.season) { d.season } else { seasons.first().copied().unwrap_or(1) };
        self.dl_gen += 1;
        self.dl_pick = Some(DlPick { gen: self.dl_gen, season, seasons, loading: false, list: vec![], errors: vec![] });
        self.dl_load_streams(season);
    }

    pub fn dl_close(&mut self) {
        self.dl_pick = None;
        self.dl_gen += 1;
    }

    /// Looks for torrents of the first episode of a season (season packs are listed there too).
    pub fn dl_load_streams(&mut self, season: i64) {
        let Some(d) = self.detail.as_ref() else { return };
        let Some(first) = d.meta.episodes(season).first().map(|v| v.id.clone()) else { return };
        let kind = d.meta.kind.clone();
        self.dl_gen += 1;
        let gen = self.dl_gen;
        if let Some(p) = self.dl_pick.as_mut() {
            p.gen = gen;
            p.season = season;
            p.loading = true;
            p.list.clear();
            p.errors.clear();
        }
        self.events.push(Event::DlPick);
        let (entries, preferred) = (self.settings.addons.clone(), self.settings.general.preferred_height);
        self.spawn(move |tx, http| {
            let found = addons::fetch_streams(&*http, &entries, &kind, &first);
            let mut seen: Vec<String> = Vec::new();
            let mut list: Vec<Stream> = found
                .items
                .into_iter()
                .filter(|s| s.is_torrent())
                .filter(|s| {
                    let key = s.info_hash.clone().unwrap_or_default();
                    !seen.contains(&key) && {
                        seen.push(key);
                        true
                    }
                })
                .collect();
            streams::rank(&mut list, preferred);
            send(tx, Msg::DlStreams(gen, list, found.errors));
        });
    }

    pub(super) fn on_dl_streams(&mut self, gen: u64, list: Vec<Stream>, errors: Vec<String>) {
        if let Some(p) = self.dl_pick.as_mut().filter(|p| p.gen == gen) {
            p.loading = false;
            p.list = list;
            p.errors = errors;
            self.events.push(Event::DlPick);
        }
    }

    /// Starts keeping the chosen torrent: every episode of the ticked seasons that it holds. Episodes it does
    /// not hold are looked for in other torrents of the same release family (same group, quality, source).
    pub fn dl_start(&mut self, idx: usize, seasons: Vec<i64>) {
        let Some(p) = self.dl_pick.as_ref() else { return };
        let Some(stream) = p.list.get(idx).cloned() else { return };
        let Some(d) = self.detail.as_ref() else { return };
        let reference = d.meta.episodes(p.season).first().and_then(|v| v.episode).map(|e| (p.season, e));
        let meta = d.meta.clone();
        self.dl_pick = None;
        self.events.push(Event::DlPick);
        self.toast("Looking for the files in this torrent…");
        self.dl_begin(stream, seasons, reference, meta, None, true);
    }

    fn dl_begin(&mut self, stream: Stream, seasons: Vec<i64>, reference: Option<(i64, i64)>, meta: MetaItem, only: Option<(i64, i64)>, find_rest: bool) {
        let Some(hash) = stream.info_hash.clone() else { return };
        let Some(magnet) = streams::magnet(&stream) else { return };
        // the page of the title is kept with its episodes, so they open and play offline (controller.rs open_detail)
        if !meta.videos.is_empty() {
            crate::settings::save_detail(&self.data_dir, &meta);
        }
        let mut dl = Download {
            hash: hash.clone(),
            magnet: magnet.clone(),
            saved: true,
            meta_id: meta.id.clone(),
            kind: meta.kind.clone(),
            title: meta.name.clone(),
            poster: self.poster_for(&meta),
            added: util::now_unix(),
            headline: stream.headline(),
            lines: stream.detail_lines().into_iter().take(4).collect(),
            res: stream.res,
            seeders: stream.seeders,
            size_mb: stream.size_mb,
            ..Default::default()
        };
        if self.dl_running.contains(&hash) {
            if find_rest {
                self.toast("This torrent is already downloading.");
            }
            return;
        }
        self.dl_running.insert(hash);
        let (engine, exe, rt_dir, dl_dir) = (self.engine.clone(), self.rt.rqbit.clone(), self.rt.dir.clone(), self.data_dir.join("torrents"));
        // For torrents that number episodes over all seasons.
        let abs = seasons.first().and_then(|s| {
            let before: usize = meta.seasons().into_iter().filter(|x| *x >= 1 && x < s).map(|x| meta.episodes(x).len()).sum();
            let len = meta.episodes(*s).len();
            (before > 0 && len > 0).then_some((len, before as i64))
        });
        self.spawn(move |tx, http| {
            let prepared = (|| -> Result<(String, u64, Vec<(usize, u64)>, Download), String> {
                let base = ensure_engine(&*http, &engine, exe.as_deref(), &rt_dir, &dl_dir, &|_| {})?;
                let rq = Rqbit { http: &*http, base: base.clone() };
                let (name, files) = rq.list_files(&magnet)?;
                let plan = downloads::plan_for(&files, &seasons, reference, only, abs);
                if plan.is_empty() {
                    return Err("No episodes of the chosen seasons were found in this torrent. Try another one.".to_string());
                }
                let only_idx: Vec<usize> = plan.iter().map(|f| f.idx).collect();
                let (id, name2, _) = rq.add_only(&magnet, &only_idx)?;
                dl.name = if name.is_empty() { name2 } else { name };
                dl.files = plan;
                let included = dl.files.iter().map(|f| (f.idx, f.len)).collect();
                Ok((base, id, included, dl))
            })();
            match prepared {
                Err(e) => send(tx, Msg::DlAdded(if find_rest { Err(e) } else { Err(String::new()) }, stream.info_hash.clone().unwrap_or_default())),
                Ok((base, id, included, dl)) => {
                    let hash = dl.hash.clone();
                    if find_rest {
                        // Episodes of the wanted seasons that this torrent does not hold.
                        let mut missing: Vec<(i64, i64, String)> = Vec::new();
                        for s in &seasons {
                            for v in meta.episodes(*s) {
                                let e = v.episode.unwrap_or(0);
                                if !dl.files.iter().any(|f| f.season.unwrap_or(1) == *s && f.episode == Some(e)) {
                                    missing.push((*s, e, v.id.clone()));
                                }
                            }
                        }
                        if !missing.is_empty() {
                            send(tx, Msg::DlMissing(Box::new(meta.clone()), Box::new(stream.clone()), missing));
                        }
                    }
                    send(tx, Msg::DlAdded(Ok(dl), hash.clone()));
                    monitor(tx, &*http, &base, id, &hash, &included);
                }
            }
        });
    }

    /// The rest of a season: for each episode the first torrent did not hold, find the torrent of the same family.
    pub(super) fn on_dl_missing(&mut self, meta: MetaItem, reference: Stream, missing: Vec<(i64, i64, String)>) {
        let jobs: Vec<(i64, i64, String)> = missing
            .into_iter()
            .filter(|(s, e, _)| matches!(self.episode_state_of(&meta.id, *s, *e), EpState::Online))
            .collect();
        if jobs.is_empty() {
            return;
        }
        let (entries, kind) = (self.settings.addons.clone(), meta.kind.clone());
        let total = jobs.len();
        let cancel = Arc::new(AtomicBool::new(false));
        self.finding = Some(Finding { title: meta.name.clone(), total, done: 0, cancel: cancel.clone() });
        self.spawn(move |tx, http| {
            let found = std::sync::atomic::AtomicUsize::new(0);
            let per = jobs.len().div_ceil(3).max(1);
            std::thread::scope(|sc| {
                for chunk in jobs.chunks(per) {
                    let (http, tx, entries, kind, meta, reference, found, cancel) = (&http, tx, &entries, &kind, &meta, &reference, &found, &cancel);
                    sc.spawn(move || {
                        for (s, e, vid) in chunk {
                            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                                return;
                            }
                            let got = addons::fetch_streams(&**http, entries, kind, vid);
                            let pick = downloads::pick_family(reference, &got.items).map(|i| got.items[i].clone());
                            if pick.is_some() {
                                found.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            }
                            send(tx, Msg::DlSibling(Box::new(meta.clone()), *s, *e, pick.map(Box::new)));
                        }
                    });
                }
            });
            send(tx, Msg::DlSiblingsDone(found.load(std::sync::atomic::Ordering::SeqCst), total));
        });
    }

    fn episode_state_of(&self, meta_id: &str, season: i64, episode: i64) -> EpState {
        downloads::episode_state(&self.downloads, &self.data_dir.join("torrents"), meta_id, season, episode)
    }

    pub(super) fn on_dl_sibling(&mut self, meta: MetaItem, season: i64, episode: i64, stream: Option<Stream>) {
        // Cancelled from the panel: late answers are ignored.
        let Some(f) = self.finding.as_mut() else { return };
        f.done += 1;
        let Some(stream) = stream else { return };
        if !matches!(self.episode_state_of(&meta.id, season, episode), EpState::Online) {
            return;
        }
        self.dl_begin(stream, vec![season], Some((season, episode)), meta, Some((season, episode)), false);
    }

    pub(super) fn on_dl_siblings_done(&mut self, found: usize, total: usize) {
        // Cancelled from the panel: the search was already dropped.
        if self.finding.take().is_none() {
            return;
        }
        let text = if found == total {
            format!("Found all {} episodes in the same release family. They download in the background.", total)
        } else if found == 0 {
            "No other episodes from the same release were found. Try keeping another torrent.".to_string()
        } else {
            format!("Found {} of {} more episodes in the same release family.", found, total)
        };
        self.toast(&text);
    }

    pub(super) fn on_dl_added(&mut self, r: Result<Download, String>, hash: String) {
        match r {
            Err(e) => {
                self.dl_running.remove(&hash);
                // An empty message is a resume that has to wait for the next start.
                if !e.is_empty() {
                    self.toast(&e);
                }
            }
            Ok(d) => {
                let title = d.title.clone();
                match self.downloads.iter_mut().find(|x| x.hash == d.hash) {
                    Some(old) => {
                        let keep: Vec<(usize, bool)> = old.files.iter().map(|f| (f.idx, f.done)).collect();
                        let mut new = d;
                        for f in new.files.iter_mut() {
                            if keep.iter().any(|(i, done)| *i == f.idx && *done) {
                                f.done = true;
                            }
                        }
                        *old = new;
                    }
                    None => self.downloads.push(d),
                }
                downloads::save(&self.data_dir, &self.downloads);
                self.toast(&format!("Downloading {} to disk.", title));
            }
        }
        self.events.push(Event::Downloads);
        self.events.push(Event::Detail);
    }

    pub(super) fn on_dl_progress(&mut self, hash: String, percent: f64, done: Vec<usize>, complete: bool) {
        let Some(d) = self.downloads.iter_mut().find(|d| d.hash == hash) else { return };
        let before = (d.percent as u32, d.files.iter().filter(|f| f.done).count(), d.complete);
        d.percent = percent;
        for f in d.files.iter_mut() {
            if complete || done.contains(&f.idx) {
                f.done = true;
            }
        }
        d.complete = complete || d.files.iter().all(|f| f.done);
        let after = (d.percent as u32, d.files.iter().filter(|f| f.done).count(), d.complete);
        let (finished, title) = (d.complete, d.title.clone());
        if after != before {
            if after.1 != before.1 || after.2 != before.2 {
                downloads::save(&self.data_dir, &self.downloads);
                self.events.push(Event::Detail);
            }
            self.events.push(Event::Downloads);
        }
        if finished && !before.2 {
            self.dl_running.remove(&hash);
            self.toast(&format!("{} is saved on disk.", title));
        }
    }

    /// Continues the downloads that were not finished when the app closed.
    pub fn dl_resume_all(&mut self) {
        let todo: Vec<Download> = self.downloads.iter().filter(|d| d.saved && !d.complete && !d.magnet.is_empty() && !self.dl_running.contains(&d.hash)).cloned().collect();
        for d in todo {
            self.dl_running.insert(d.hash.clone());
            let (engine, exe, rt_dir, dl_dir) = (self.engine.clone(), self.rt.rqbit.clone(), self.rt.dir.clone(), self.data_dir.join("torrents"));
            self.spawn(move |tx, http| {
                let only: Vec<usize> = d.files.iter().filter(|f| !f.done).map(|f| f.idx).collect();
                let started = ensure_engine(&*http, &engine, exe.as_deref(), &rt_dir, &dl_dir, &|_| {}).and_then(|base| {
                    let rq = Rqbit { http: &*http, base: base.clone() };
                    rq.add_only(&d.magnet, &only).map(|(id, _, _)| (base, id))
                });
                match started {
                    Ok((base, id)) => {
                        let included: Vec<(usize, u64)> = d.files.iter().map(|f| (f.idx, f.len)).collect();
                        monitor(tx, &*http, &base, id, &d.hash, &included);
                    }
                    Err(_) => {
                        // Not now (no engine, no internet). It is tried again the next time the app starts.
                        send(tx, Msg::DlAdded(Err(String::new()), d.hash.clone()));
                    }
                }
            });
        }
    }

    // ----- the Torrents page -----

    pub fn torrent_overview(&mut self) -> Overview {
        let dl_dir = self.data_dir.join("torrents");
        let on_disk = downloads::folder_entries(&dl_dir);
        let total: u64 = on_disk.iter().map(|(_, s)| *s).sum();
        let mut rows: Vec<(String, TorrentRow)> = Vec::new();
        let mut known: Vec<String> = Vec::new();
        for d in &self.downloads {
            known.push(d.root());
            known.push(d.name.clone());
            let bytes = downloads::disk_bytes(&dl_dir, d);
            if !d.saved && bytes == 0 {
                continue;
            }
            let eps = d.files.iter().filter(|f| f.episode.is_some()).count();
            let done = d.files.iter().filter(|f| f.done).count();
            let detail = if d.saved {
                if d.complete { format!("Saved on disk · {} episodes", eps) } else { format!("Downloading · {} of {} episodes · {:.0}%", done, eps, d.percent) }
            } else {
                "Streaming cache".to_string()
            };
            let title = if d.title.is_empty() { d.name.clone() } else { d.title.clone() };
            rows.push((
                downloads::series_key(&title),
                TorrentRow { key: d.hash.clone(), title: d.name.clone(), detail, poster: d.poster.clone(), bytes, saved: d.saved, percent: d.percent, downloading: d.saved && !d.complete },
            ));
        }
        for (name, bytes) in on_disk {
            if !known.contains(&name) {
                rows.push((downloads::series_key(&name), TorrentRow { key: format!("folder:{}", name), title: name, detail: "Streaming cache".to_string(), poster: None, bytes, saved: false, percent: 0.0, downloading: false }));
            }
        }
        // Torrents of the same content go together.
        let mut groups: Vec<TorrentGroup> = Vec::new();
        for (key, row) in rows {
            let clean = downloads::clean_title(&row.title);
            let shown = self.downloads.iter().find(|d| d.hash == row.key && !d.title.is_empty()).map(|d| d.title.clone()).unwrap_or(clean);
            match groups.iter_mut().find(|g| g.key == key) {
                Some(g) => {
                    // A real title (from a record) beats a cleaned file name.
                    if self.downloads.iter().any(|d| d.hash == row.key && !d.title.is_empty()) {
                        g.title = shown;
                    }
                    if g.poster.is_none() {
                        g.poster = row.poster.clone();
                    }
                    g.bytes += row.bytes;
                    if row.saved {
                        g.saved_bytes += row.bytes;
                    }
                    g.rows.push(row);
                }
                None => groups.push(TorrentGroup {
                    key,
                    title: shown,
                    poster: row.poster.clone(),
                    bytes: row.bytes,
                    saved_bytes: if row.saved { row.bytes } else { 0 },
                    rows: vec![row],
                }),
            }
        }
        for g in groups.iter_mut() {
            if let Some((name, poster)) = self.covers.get(&g.key) {
                if g.poster.is_none() && !poster.is_empty() {
                    g.poster = Some(poster.clone());
                }
                if !name.is_empty() && !self.downloads.iter().any(|d| !d.title.is_empty() && downloads::series_key(&d.title) == g.key) {
                    g.title = name.clone();
                }
            } else if g.poster.is_none() && self.cover_asked.insert(g.key.clone()) {
                let (key, title) = (g.key.clone(), g.title.clone());
                self.spawn(move |tx, http| send(tx, Msg::Cover(key, find_cover(&*http, &title))));
            }
            g.rows.sort_by(|a, b| b.saved.cmp(&a.saved).then(b.bytes.cmp(&a.bytes)));
        }
        groups.sort_by(|a, b| (b.saved_bytes > 0).cmp(&(a.saved_bytes > 0)).then(b.bytes.cmp(&a.bytes)));
        let saved: u64 = groups.iter().map(|g| g.saved_bytes).sum();
        Overview { total, saved, cache: total.saturating_sub(saved), groups }
    }

    pub(super) fn on_cover(&mut self, key: String, found: Option<(String, String)>) {
        if let Some(f) = found {
            self.covers.insert(key, f);
            save_covers(&self.data_dir, &self.covers);
            self.events.push(Event::Downloads);
        }
    }

    /// Deletes one torrent from the disk.
    pub fn remove_torrent(&mut self, key: &str) {
        let dl_dir = self.data_dir.join("torrents");
        let playing = self.session.as_ref().and_then(|s| s.torrent.as_ref()).map(|t| t.2.clone());
        let engine = self.engine.clone();
        if let Some(name) = key.strip_prefix("folder:") {
            if name.contains('/') || name.contains('\\') || name == ".." {
                return;
            }
            if self.session.is_some() {
                self.toast("Stop the video first.");
                return;
            }
            let path = dl_dir.join(name);
            std::thread::spawn(move || {
                // The engine may still hold the files open.
                if let Ok(mut g) = engine.lock() {
                    g.take();
                }
                let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
            });
            self.dl_running_reset();
            self.events.push(Event::Downloads);
            return;
        }
        if playing.as_deref() == Some(key) {
            self.toast("Stop the video first.");
            return;
        }
        let Some(i) = self.downloads.iter().position(|d| d.hash == key) else { return };
        let d = self.downloads.remove(i);
        self.dl_running.remove(&d.hash);
        downloads::save(&self.data_dir, &self.downloads);
        self.events.push(Event::Downloads);
        self.events.push(Event::Detail);
        self.spawn(move |tx, http| {
            let base = engine.lock().ok().and_then(|g| g.as_ref().map(|p| p.base()));
            if let Some(b) = base {
                Rqbit { http: &*http, base: b }.delete(&d.hash);
                std::thread::sleep(Duration::from_millis(700));
            }
            let mut freed = downloads::remove_files(&dl_dir, &d);
            let left = d.files.iter().any(|f| downloads::locate(&dl_dir, &d.name, &f.path).is_some());
            let mut restart = false;
            if left {
                // Windows keeps files locked while the engine has them open: stop it and try again.
                if let Ok(mut g) = engine.lock() {
                    g.take();
                }
                std::thread::sleep(Duration::from_millis(500));
                freed += downloads::remove_files(&dl_dir, &d);
                restart = true;
            }
            send(tx, Msg::DlRemoved(format!("Freed {}.", torrent::human_bytes(freed)), restart));
        });
    }

    /// Forget which downloads were running, after the engine was stopped.
    fn dl_running_reset(&mut self) {
        self.dl_running.clear();
    }

    /// Keeps a record of a torrent the engine streams, so the Torrents page can show it with its cover.
    pub(super) fn note_cache_entry(&mut self, res: &Resolved) {
        let Some((_, _, hash)) = res.torrent.as_ref() else { return };
        if hash.is_empty() || res.torrent_files.is_empty() || self.downloads.iter().any(|d| &d.hash == hash) {
            return;
        }
        let Some(meta) = self.session.as_ref().and_then(|s| s.meta.clone()) else { return };
        let files = res
            .torrent_files
            .iter()
            .enumerate()
            .map(|(idx, (path, len))| {
                let (s, e) = downloads::parse_episode(path);
                downloads::DlFile { idx, path: path.clone(), len: *len, season: s, episode: e, done: false }
            })
            .collect();
        let poster = self.poster_for(&meta);
        self.downloads.push(Download { hash: hash.clone(), saved: false, meta_id: meta.id.clone(), kind: meta.kind.clone(), title: meta.name.clone(), poster, name: res.torrent_name.clone(), files, added: util::now_unix(), ..Default::default() });
        downloads::save(&self.data_dir, &self.downloads);
    }
}
