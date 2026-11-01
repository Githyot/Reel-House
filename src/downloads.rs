//! Torrents kept on disk ("Keep downloaded"), the episodes in them, and the streaming cache.
//! The list lives in `downloads.json`; the files themselves are in the `torrents` folder.
use crate::json::Json;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DlFile {
    /// Index in the torrent.
    pub idx: usize,
    /// Path inside the torrent, with `/`.
    pub path: String,
    pub len: u64,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub done: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Download {
    pub hash: String,
    pub magnet: String,
    /// Kept on purpose with the Keep downloaded button. Other entries are only the streaming cache.
    pub saved: bool,
    pub meta_id: String,
    pub kind: String,
    pub title: String,
    pub poster: Option<String>,
    /// Name of the torrent (its folder).
    pub name: String,
    pub files: Vec<DlFile>,
    pub added: u64,
    /// What the user saw when choosing it: shown again in the green box.
    pub headline: String,
    pub lines: Vec<String>,
    pub res: u32,
    pub seeders: u32,
    pub size_mb: u32,
    pub complete: bool,
    /// Progress while downloading, 0 to 100. Not saved.
    pub percent: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EpState {
    /// On disk: path of the file.
    Saved(PathBuf),
    /// Being downloaded, percent.
    Downloading(f64),
    Online,
}

impl Download {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("hash", self.hash.clone().into()),
            ("magnet", self.magnet.clone().into()),
            ("saved", self.saved.into()),
            ("meta_id", self.meta_id.clone().into()),
            ("kind", self.kind.clone().into()),
            ("title", self.title.clone().into()),
            ("poster", self.poster.clone().unwrap_or_default().into()),
            ("name", self.name.clone().into()),
            ("added", (self.added as f64).into()),
            ("headline", self.headline.clone().into()),
            ("lines", self.lines.clone().into()),
            ("res", self.res.into()),
            ("seeders", self.seeders.into()),
            ("size_mb", self.size_mb.into()),
            ("complete", self.complete.into()),
            (
                "files",
                Json::Arr(
                    self.files
                        .iter()
                        .map(|f| {
                            Json::obj(vec![
                                ("idx", (f.idx as f64).into()),
                                ("path", f.path.clone().into()),
                                ("len", (f.len as f64).into()),
                                ("season", f.season.map(|s| Json::Num(s as f64)).unwrap_or(Json::Null)),
                                ("episode", f.episode.map(|s| Json::Num(s as f64)).unwrap_or(Json::Null)),
                                ("done", f.done.into()),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    pub fn from_json(j: &Json) -> Download {
        let poster = j.get("poster").str();
        Download {
            hash: j.get("hash").str(),
            magnet: j.get("magnet").str(),
            saved: j.get("saved").as_bool().unwrap_or(false),
            meta_id: j.get("meta_id").str(),
            kind: j.get("kind").str(),
            title: j.get("title").str(),
            poster: if poster.is_empty() { None } else { Some(poster) },
            name: j.get("name").str(),
            added: j.get("added").as_u64().unwrap_or(0),
            headline: j.get("headline").str(),
            lines: j.get("lines").strings(),
            res: j.get("res").as_u64().unwrap_or(0) as u32,
            seeders: j.get("seeders").as_u64().unwrap_or(0) as u32,
            size_mb: j.get("size_mb").as_u64().unwrap_or(0) as u32,
            complete: j.get("complete").as_bool().unwrap_or(false),
            percent: 0.0,
            files: j
                .get("files")
                .items()
                .iter()
                .map(|f| DlFile {
                    idx: f.get("idx").as_u64().unwrap_or(0) as usize,
                    path: f.get("path").str(),
                    len: f.get("len").as_u64().unwrap_or(0),
                    season: f.get("season").as_i64(),
                    episode: f.get("episode").as_i64(),
                    done: f.get("done").as_bool().unwrap_or(false),
                })
                .collect(),
        }
    }

    /// First folder of the files (how the torrent shows up in the `torrents` folder).
    pub fn root(&self) -> String {
        let first = self.files.first().map(|f| f.path.as_str()).unwrap_or("");
        match first.split_once('/') {
            Some((r, _)) => r.to_string(),
            None => self.name.clone(),
        }
    }
}

pub fn load(dir: &Path) -> Vec<Download> {
    fs::read_to_string(dir.join("downloads.json"))
        .ok()
        .and_then(|t| Json::parse(&t).ok())
        .map(|j| j.items().iter().map(Download::from_json).filter(|d| !d.hash.is_empty()).collect())
        .unwrap_or_default()
}

pub fn save(dir: &Path, items: &[Download]) {
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(dir.join("downloads.json"), Json::Arr(items.iter().map(|d| d.to_json()).collect()).pretty());
}

/// Where a file of a torrent is on disk, if it is there.
pub fn locate(dl_dir: &Path, torrent_name: &str, path: &str) -> Option<PathBuf> {
    let direct = dl_dir.join(path);
    if direct.is_file() {
        return Some(direct);
    }
    let inner = dl_dir.join(torrent_name).join(path);
    if inner.is_file() {
        return Some(inner);
    }
    None
}

/// Bytes the files of a download take on disk.
pub fn disk_bytes(dl_dir: &Path, d: &Download) -> u64 {
    d.files.iter().filter_map(|f| locate(dl_dir, &d.name, &f.path)).filter_map(|p| fs::metadata(p).ok()).map(|m| m.len()).sum()
}

/// What the Torrents page shows for something stored in the folder: the entry of the registry, or a bare folder.
pub fn folder_entries(dl_dir: &Path) -> Vec<(String, u64)> {
    let Ok(rd) = fs::read_dir(dl_dir) else { return vec![] };
    let mut v: Vec<(String, u64)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let m = e.metadata().ok()?;
            let size = if m.is_dir() { dir_bytes(&e.path()) } else { m.len() };
            Some((name, size))
        })
        .collect();
    v.sort();
    v
}

pub fn dir_bytes(p: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_bytes(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Removes the files of a download and the folders left empty.
pub fn remove_files(dl_dir: &Path, d: &Download) -> u64 {
    let mut freed = 0;
    let root = d.root();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for f in &d.files {
        if let Some(p) = locate(dl_dir, &d.name, &f.path) {
            freed += fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            let _ = fs::remove_file(&p);
            if let Some(par) = p.parent() {
                dirs.push(par.to_path_buf());
            }
        }
    }
    dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    dirs.dedup();
    for d in dirs {
        // Only empty folders go; `remove_dir` refuses a folder with something in it.
        let mut cur = Some(d);
        while let Some(c) = cur {
            if c == dl_dir || !c.starts_with(dl_dir) || fs::remove_dir(&c).is_err() {
                break;
            }
            cur = c.parent().map(|p| p.to_path_buf());
        }
    }
    let leftover = dl_dir.join(&root);
    if !root.is_empty() && leftover.is_dir() && dir_bytes(&leftover) == 0 {
        let _ = fs::remove_dir_all(&leftover);
    }
    freed
}

// ----- which episode is a file -----

fn digits_at(b: &[u8], mut i: usize) -> (usize, usize) {
    let st = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    (st, i)
}

fn num(b: &[u8], st: usize, en: usize) -> i64 {
    std::str::from_utf8(&b[st..en]).ok().and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// Season and episode from a file name: "S01E02", "1x02", "E02" / "EP02", "- 02", "[02]".
pub fn parse_episode(name: &str) -> (Option<i64>, Option<i64>) {
    let base = name.rsplit('/').next().unwrap_or(name);
    let lower = base.to_lowercase();
    let b = lower.as_bytes();
    let is_alnum = |i: usize| i < b.len() && b[i].is_ascii_alphanumeric();
    // S01E02, S01.E02, S01 E02
    for i in 0..b.len() {
        if b[i] == b's' && (i == 0 || !is_alnum(i - 1)) {
            let (a, z) = digits_at(b, i + 1);
            if z > a && z - a <= 2 {
                let mut k = z;
                while k < b.len() && matches!(b[k], b'.' | b'_' | b' ' | b'-') {
                    k += 1;
                }
                if k < b.len() && b[k] == b'e' {
                    let (c, d) = digits_at(b, k + 1);
                    if d > c && d - c <= 4 {
                        return (Some(num(b, a, z)), Some(num(b, c, d)));
                    }
                }
            }
        }
    }
    // 1x02
    for i in 1..b.len() {
        if b[i] == b'x' && b[i - 1].is_ascii_digit() && (i < 2 || !b[i - 2].is_ascii_digit() || (i >= 3 && !b[i - 3].is_ascii_digit())) {
            let mut st = i - 1;
            while st > 0 && b[st - 1].is_ascii_digit() {
                st -= 1;
            }
            let (c, d) = digits_at(b, i + 1);
            if i - st <= 2 && (st == 0 || !is_alnum(st - 1)) && d > c && d - c >= 2 && d - c <= 3 && !(is_alnum(d)) {
                return (Some(num(b, st, i)), Some(num(b, c, d)));
            }
        }
    }
    // E02, EP02, Episode 02
    for i in 0..b.len() {
        if b[i] == b'e' && (i == 0 || !b[i - 1].is_ascii_alphabetic()) {
            let mut k = i + 1;
            if lower[i..].starts_with("episode") {
                k = i + 7;
            } else if lower[i..].starts_with("ep") {
                k = i + 2;
            }
            while k < b.len() && matches!(b[k], b' ' | b'.' | b'_') {
                k += 1;
            }
            let (c, d) = digits_at(b, k);
            if d > c && d - c <= 3 && !is_alnum(d) {
                return (None, Some(num(b, c, d)));
            }
        }
    }
    // "Show - 05 (1080p)" as fansub groups write it
    if let Some(p) = lower.find(" - ") {
        let mut from = p;
        loop {
            let (c, d) = digits_at(b, from + 3);
            if d > c && d - c <= 3 && (d >= b.len() || !b[d].is_ascii_alphabetic() || b[d] == b'v') {
                let n = num(b, c, d);
                if n > 0 && ![480, 720, 1080, 2160].contains(&n) {
                    return (None, Some(n));
                }
            }
            match lower[from + 3..].find(" - ") {
                Some(q) => from = from + 3 + q,
                None => break,
            }
        }
    }
    // "[05]"
    for i in 0..b.len() {
        if b[i] == b'[' {
            let (c, d) = digits_at(b, i + 1);
            if d > c && d - c >= 2 && d - c <= 3 && d < b.len() && b[d] == b']' {
                let n = num(b, c, d);
                if n > 0 && ![480, 720, 1080, 2160, 264, 265].contains(&n) {
                    return (None, Some(n));
                }
            }
        }
    }
    (None, None)
}

/// "Season 2", "S02" in a folder name.
fn season_in(component: &str) -> Option<i64> {
    let l = component.to_lowercase();
    let b = l.as_bytes();
    if let Some(p) = l.find("season") {
        let mut k = p + 6;
        while k < b.len() && matches!(b[k], b' ' | b'.' | b'_' | b'-') {
            k += 1;
        }
        let (c, d) = digits_at(b, k);
        if d > c {
            return Some(num(b, c, d));
        }
    }
    // "S02" as a word of its own
    for i in 0..b.len() {
        if b[i] == b's' && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let (c, d) = digits_at(b, i + 1);
            if d > c && d - c <= 2 && (d >= b.len() || !b[d].is_ascii_alphanumeric()) {
                return Some(num(b, c, d));
            }
        }
    }
    None
}

/// Season of a file from its folders when the name has none ("Show/Season 2/05.mkv").
pub fn season_from_path(path: &str) -> Option<i64> {
    let comps: Vec<&str> = path.split('/').collect();
    comps.iter().take(comps.len().saturating_sub(1)).rev().find_map(|c| season_in(c))
}

/// Which files of a torrent to keep for the chosen seasons, with the episode of each.
/// `files` are (path, length); `seasons` the seasons the user ticked; `reference` the episode the stream was picked for.
pub fn plan(files: &[(String, u64)], seasons: &[i64], reference: Option<(i64, i64)>) -> Vec<DlFile> {
    plan_for(files, seasons, reference, None, None)
}

/// Like `plan`, with two more rules. `only` keeps just that (season, episode). `abs` is (episodes in the season,
/// episodes in the seasons before it): torrents that number the episodes over all seasons ("- 53") are mapped
/// back to the season.
pub fn plan_for(files: &[(String, u64)], seasons: &[i64], reference: Option<(i64, i64)>, only: Option<(i64, i64)>, abs: Option<(usize, i64)>) -> Vec<DlFile> {
    let default_season = seasons.first().copied().unwrap_or(1);
    let videos: Vec<usize> = files.iter().enumerate().filter(|(_, (p, _))| crate::torrent::is_video(p)).map(|(i, _)| i).collect();
    // A torrent of one episode: the episode is the one it was found for, whatever its name says.
    if videos.len() == 1 {
        if let Some((s, e)) = reference {
            let (path, len) = &files[videos[0]];
            if only.map(|o| o == (s, e)).unwrap_or(true) {
                return vec![DlFile { idx: videos[0], path: path.clone(), len: *len, season: Some(s), episode: Some(e), done: false }];
            }
            return vec![];
        }
    }
    let mut out: Vec<DlFile> = Vec::new();
    for (idx, (path, len)) in files.iter().enumerate() {
        if !crate::torrent::is_video(path) {
            continue;
        }
        let (s, e) = parse_episode(path);
        let Some(mut ep) = e else { continue };
        let named_season = s.or_else(|| season_from_path(path));
        let season = named_season.unwrap_or(default_season);
        if s.is_none() {
            if let Some((len_s, offset)) = abs {
                if ep > len_s as i64 && ep - offset >= 1 && ep - offset <= len_s as i64 {
                    ep -= offset;
                }
            }
        }
        if !seasons.is_empty() && !seasons.contains(&season) {
            continue;
        }
        if only.map(|o| o != (season, ep)).unwrap_or(false) {
            continue;
        }
        out.push(DlFile { idx, path: path.clone(), len: *len, season: Some(season), episode: Some(ep), done: false });
    }
    out
}

// ---------------------------------------------------------------- the same release family

fn lower_text(s: &crate::streams::Stream) -> String {
    format!("{}\n{}\n{}", s.name, s.desc, s.filename.clone().unwrap_or_default()).to_lowercase()
}

/// "[SubsPlease]" of a release name, when there is one.
pub fn release_group(s: &crate::streams::Stream) -> Option<String> {
    for line in format!("{}\n{}\n{}", s.name, s.desc, s.filename.clone().unwrap_or_default()).lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix('[') {
            if let Some(end) = rest.find(']') {
                let g = rest[..end].trim().to_lowercase();
                if !g.is_empty() && g.len() <= 24 {
                    return Some(g);
                }
            }
        }
    }
    None
}

fn source_of(t: &str) -> Option<&'static str> {
    for (needle, name) in [("bluray", "bd"), ("blu-ray", "bd"), ("bdrip", "bd"), ("brrip", "bd"), ("web-dl", "webdl"), ("webdl", "webdl"), ("webrip", "webrip"), ("hdtv", "hdtv"), ("web", "webdl")] {
        if t.contains(needle) {
            return Some(name);
        }
    }
    None
}

fn codec_of(t: &str) -> Option<&'static str> {
    if t.contains("x265") || t.contains("hevc") || t.contains("h265") || t.contains("h.265") {
        Some("hevc")
    } else if t.contains("x264") || t.contains("h264") || t.contains("avc") || t.contains("h.264") {
        Some("avc")
    } else if t.contains("av1") {
        Some("av1")
    } else {
        None
    }
}

fn language_flags(t: &str) -> (bool, bool) {
    (t.contains("dub") || t.contains("dual"), t.contains("multi"))
}

/// How well `cand` fits the release family of `reference` (same group, quality, source, codec, language). Higher is better.
pub fn family_score(reference: &crate::streams::Stream, cand: &crate::streams::Stream) -> i32 {
    let (rt, ct) = (lower_text(reference), lower_text(cand));
    let mut score = 0;
    match (release_group(reference), release_group(cand)) {
        (Some(a), Some(b)) if a == b => score += 60,
        (Some(_), Some(_)) => score -= 40,
        _ => {}
    }
    if reference.res != 0 && cand.res != 0 {
        score += if reference.res == cand.res { 40 } else { -60 };
    }
    if let (Some(a), Some(b)) = (source_of(&rt), source_of(&ct)) {
        score += if a == b { 20 } else { -15 };
    }
    if let (Some(a), Some(b)) = (codec_of(&rt), codec_of(&ct)) {
        score += if a == b { 15 } else { -10 };
    }
    let (rd, cd) = (language_flags(&rt), language_flags(&ct));
    if rd != cd {
        score -= 25;
    }
    if reference.addon == cand.addon {
        score += 5;
    }
    score + (cand.seeders.min(50) / 10) as i32
}

/// The best torrent of the family of `reference` among `list`, if one is close enough.
pub fn pick_family(reference: &crate::streams::Stream, list: &[crate::streams::Stream]) -> Option<usize> {
    list.iter()
        .enumerate()
        .filter(|(_, c)| c.is_torrent() && c.info_hash != reference.info_hash)
        .map(|(i, c)| (i, family_score(reference, c)))
        .filter(|(_, sc)| *sc >= 40)
        .max_by_key(|(_, sc)| *sc)
        .map(|(i, _)| i)
}

/// What the episode row shows.
pub fn episode_state(items: &[Download], dl_dir: &Path, meta_id: &str, season: i64, episode: i64) -> EpState {
    let mut downloading: Option<f64> = None;
    for d in items.iter().filter(|d| d.saved && d.meta_id == meta_id) {
        for f in d.files.iter().filter(|f| f.season.unwrap_or(1) == season && f.episode == Some(episode)) {
            if f.done {
                if let Some(p) = locate(dl_dir, &d.name, &f.path) {
                    return EpState::Saved(p);
                }
            } else if downloading.is_none() {
                downloading = Some(d.percent);
            }
        }
    }
    match downloading {
        Some(p) => EpState::Downloading(p),
        None => EpState::Online,
    }
}

/// The saved torrent that holds an episode, with its file.
pub fn saved_for<'a>(items: &'a [Download], dl_dir: &Path, meta_id: &str, season: i64, episode: i64) -> Option<(&'a Download, &'a DlFile, PathBuf)> {
    for d in items.iter().filter(|d| d.saved && d.meta_id == meta_id) {
        for f in d.files.iter().filter(|f| f.done && f.season.unwrap_or(1) == season && f.episode == Some(episode)) {
            if let Some(p) = locate(dl_dir, &d.name, &f.path) {
                return Some((d, f, p));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_episode_numbers_from_names() {
        let t = |n: &str| parse_episode(n);
        assert_eq!(t("Show.Name.S01E02.1080p.WEB-DL.x265.mkv"), (Some(1), Some(2)));
        assert_eq!(t("Show Name s02 e11 [1080p].mkv"), (Some(2), Some(11)));
        assert_eq!(t("Folder/Show 1x03 720p.mkv"), (Some(1), Some(3)));
        assert_eq!(t("[SubsPlease] Dandadan - 05 (1080p) [ABCD1234].mkv"), (None, Some(5)));
        assert_eq!(t("[Erai-raws] Show - 12v2 [1080p].mkv"), (None, Some(12)));
        assert_eq!(t("Show EP07 1080p.mkv"), (None, Some(7)));
        assert_eq!(t("Show Episode 3.mkv"), (None, Some(3)));
        assert_eq!(t("NieR Automata Ver1.1a - 12 [1080p].mkv"), (None, Some(12)));
        assert_eq!(t("[Group] Show [08][1080p].mkv"), (None, Some(8)));
        assert_eq!(t("Movie.2019.1080p.BluRay.x264.mkv"), (None, None));
        assert_eq!(t("Show - 1080p.mkv"), (None, None));
        assert_eq!(t("Show.S03E105.mkv"), (Some(3), Some(105)));
    }

    #[test]
    fn season_from_folders() {
        assert_eq!(season_from_path("Show/Season 2/05.mkv"), Some(2));
        assert_eq!(season_from_path("Show S03 Complete/Show - 04.mkv"), Some(3));
        assert_eq!(season_from_path("Show/Show - 04.mkv"), None);
    }

    fn files() -> Vec<(String, u64)> {
        vec![
            ("Show/Show S01E01.mkv".into(), 100),
            ("Show/Show S01E02.mkv".into(), 100),
            ("Show/Show S02E01.mkv".into(), 100),
            ("Show/readme.txt".into(), 1),
            ("Show/Show S01E02.srt".into(), 1),
        ]
    }

    #[test]
    fn plans_the_files_of_the_chosen_seasons() {
        let p = plan(&files(), &[1], None);
        assert_eq!(p.iter().map(|f| (f.idx, f.episode.unwrap())).collect::<Vec<_>>(), vec![(0, 1), (1, 2)]);
        let both = plan(&files(), &[1, 2], None);
        assert_eq!(both.len(), 3);
        // Names without a season use the chosen one.
        let anime = vec![("[G] Show - 01.mkv".to_string(), 5), ("[G] Show - 02.mkv".to_string(), 5)];
        let p = plan(&anime, &[3], None);
        assert_eq!(p.iter().map(|f| f.season.unwrap()).collect::<Vec<_>>(), vec![3, 3]);
        // A single video without a number belongs to the episode it was picked for.
        let single = vec![("Show.Special.mkv".to_string(), 9)];
        let p = plan(&single, &[1], Some((1, 4)));
        assert_eq!((p[0].season, p[0].episode), (Some(1), Some(4)));
        assert!(plan(&single, &[1], None).is_empty());
    }

    #[test]
    fn registry_round_trip_and_episode_states() {
        let dir = std::env::temp_dir().join(format!("reelhouse-dl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Show")).unwrap();
        fs::write(dir.join("Show/Show S01E01.mkv"), vec![1u8; 100]).unwrap();
        let mut d = Download { hash: "abc".into(), saved: true, meta_id: "tt1".into(), name: "Show".into(), title: "Show".into(), headline: "1080p".into(), lines: vec!["a".into(), "b".into()], res: 1080, ..Default::default() };
        d.files = plan(&files(), &[1], None);
        d.files[0].done = true;
        d.percent = 40.0;
        save(&dir, &[d.clone()]);
        let back = load(&dir);
        assert_eq!(back.len(), 1);
        assert_eq!((back[0].files.len(), back[0].files[0].done, back[0].lines.len(), back[0].root().as_str()), (2, true, 2, "Show"));
        match episode_state(&back, &dir, "tt1", 1, 1) {
            EpState::Saved(p) => assert!(p.ends_with("Show S01E01.mkv")),
            other => panic!("{:?}", other),
        }
        let mut live = back.clone();
        live[0].percent = 40.0;
        assert_eq!(episode_state(&live, &dir, "tt1", 1, 2), EpState::Downloading(40.0));
        assert_eq!(episode_state(&live, &dir, "tt1", 2, 1), EpState::Online);
        assert_eq!(episode_state(&live, &dir, "tt2", 1, 1), EpState::Online);
        assert!(saved_for(&live, &dir, "tt1", 1, 1).is_some());
        assert!(saved_for(&live, &dir, "tt1", 1, 2).is_none());
        assert_eq!(disk_bytes(&dir, &live[0]), 100);
        assert_eq!(remove_files(&dir, &live[0]), 100);
        assert!(!dir.join("Show").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------- grouping torrents by title

fn is_digits(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

fn stops_title(t: &str, prev: Option<&str>) -> bool {
    let l = t.to_lowercase();
    let b = l.as_bytes();
    let sxe = b.first() == Some(&b's') && l.contains('e') && l[1..].chars().all(|c| c.is_ascii_digit() || c == 'e') && b.len() >= 4;
    let s_only = b.first() == Some(&b's') && b.len() >= 2 && b.len() <= 3 && is_digits(&l[1..]);
    let ep = (l.starts_with("ep") && b.len() > 2 && is_digits(&l[2..])) || (b.first() == Some(&b'e') && b.len() > 1 && is_digits(&l[1..]));
    let res = l.ends_with('p') && b.len() >= 4 && is_digits(&l[..b.len() - 1]);
    let tech = matches!(l.as_str(), "1080" | "720" | "2160" | "480" | "web" | "webrip" | "web-dl" | "webdl" | "bluray" | "blu-ray" | "bdrip" | "brrip" | "hdtv" | "x264" | "x265" | "h264" | "h265" | "hevc" | "avc" | "season" | "complete" | "batch" | "multi" | "dual" | "uhd" | "remux" | "amzn" | "nf" | "cr");
    let after_dash = is_digits(&l) && prev == Some("-");
    sxe || s_only || ep || res || tech || after_dash
}

/// The title of the content in a torrent or file name: "[LostYears] Re ZERO, Starting Life - S03E04 (WEB-DL)" gives
/// "Re ZERO, Starting Life".
pub fn clean_title(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    // Without the extension of a video file.
    let base = match base.rsplit_once('.') {
        Some((a, ext)) if matches!(ext.to_lowercase().as_str(), "mkv" | "mp4" | "avi" | "webm" | "m4v" | "ts") => a,
        _ => base,
    };
    // Drop [groups] and (details).
    let mut plain = String::new();
    let mut depth = 0;
    for c in base.chars() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    // Scene names use dots or underscores for spaces.
    let plain = if plain.contains(' ') { plain.replace('_', " ") } else { plain.replace(['.', '_'], " ") };
    let mut out: Vec<&str> = Vec::new();
    for t in plain.split_whitespace() {
        if !out.is_empty() && stops_title(t, out.last().copied()) {
            break;
        }
        out.push(t);
    }
    while out.last().map(|t| *t == "-" || *t == "_").unwrap_or(false) {
        out.pop();
    }
    out.join(" ").trim_matches(|c: char| c == '-' || c == ' ').to_string()
}

/// Names that belong to the same content share this key: its first two meaningful words.
pub fn series_key(name: &str) -> String {
    const STOP: &[&str] = &["the", "a", "an", "of", "on", "in", "to", "no", "wa", "ga", "and"];
    let t = clean_title(name).to_lowercase();
    let words: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty() && !STOP.contains(w)).collect();
    match words.len() {
        0 => t,
        1 => words[0].to_string(),
        _ => format!("{} {}", words[0], words[1]),
    }
}

#[cfg(test)]
mod family_tests {
    use super::*;
    use crate::streams::Stream;

    fn st(addon: &str, name: &str, desc: &str, res: u32, seeders: u32, hash: &str) -> Stream {
        Stream { addon: addon.into(), name: name.into(), desc: desc.into(), res, seeders, info_hash: Some(hash.into()), ..Default::default() }
    }

    #[test]
    fn picks_the_same_release_family() {
        let reference = st("Addon", "1080p · WEBRIP", "[SubsPlease] Show - 01 (1080p) [AAAA].mkv", 1080, 100, "a");
        let list = vec![
            st("Addon", "1080p", "[Erai-raws] Show - 02 [1080p][HEVC].mkv", 1080, 300, "b"),
            st("Addon", "720p", "[SubsPlease] Show - 02 (720p) [BBBB].mkv", 720, 200, "c"),
            st("Addon", "1080p", "[SubsPlease] Show - 02 (1080p) [CCCC].mkv", 1080, 80, "d"),
        ];
        assert_eq!(pick_family(&reference, &list), Some(2));
        // Without a release group in the name, quality, source and codec decide.
        let tc = st("Addon B", "1080p · Webrip", "x265\naac", 1080, 10, "x");
        let others = vec![st("Addon B", "1080p · Bluray", "avc", 1080, 90, "y"), st("Addon B", "1080p · Webrip", "x265", 1080, 20, "z"), st("Addon B", "480p · Webrip", "x265", 480, 90, "w")];
        assert_eq!(pick_family(&tc, &others), Some(1));
        // Nothing close: no pick.
        let far = vec![st("Addon B", "480p · Bluray", "avc", 480, 5, "q")];
        assert_eq!(pick_family(&tc, &far), None);
    }

    #[test]
    fn single_episode_torrents_and_absolute_numbers() {
        let one = vec![("[SubsPlease] Show - 53 (1080p).mkv".to_string(), 100)];
        let p = plan_for(&one, &[2], Some((2, 22)), Some((2, 22)), None);
        assert_eq!((p[0].season, p[0].episode), (Some(2), Some(22)));
        assert!(plan_for(&one, &[2], Some((2, 22)), Some((2, 23)), None).is_empty());
        let pack: Vec<(String, u64)> = (26..=50).map(|n| (format!("Show/[X] Show - {} (1080p).mkv", n), 10)).collect();
        let p = plan_for(&pack, &[2], Some((2, 1)), Some((2, 5)), Some((25, 25)));
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].episode, Some(5));
        assert!(p[0].path.contains(" - 30 "));
    }
}

#[cfg(test)]
mod group_tests {
    use super::*;

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(clean_title("[LostYears] Re ZERO, Starting Life in Another World - S03E04 (WEB-DL 1080p x264 E-AC-3) [8D7E52F0].mkv"), "Re ZERO, Starting Life in Another World");
        assert_eq!(clean_title("[SubsPlease] Re Zero kara Hajimeru Isekai Seikatsu - 53 (1080p) [BA3C02EA].mkv"), "Re Zero kara Hajimeru Isekai Seikatsu");
        assert_eq!(clean_title("NieR.Automata.Ver1.1a.S01E03.1080p.AMZN.WEB-DL.DDP2.0.H.264-VARYG.mkv"), "NieR Automata Ver1 1a");
        assert_eq!(clean_title("[Yameii] NieR Automata Ver1.1a - 04 [English Dub] [WEB-DL 1080p] [692E2393].mkv"), "NieR Automata Ver1.1a");
        assert_eq!(clean_title("[Erai-raws] NieR-Automata Ver1_1a Part 2 - 04 [1080p][HEVC][Multiple Subtitle][12B6557F].mkv"), "NieR-Automata Ver1 1a Part 2");
        assert_eq!(clean_title("Mob Psycho 100 S02 Complete 1080p"), "Mob Psycho 100");
    }

    #[test]
    fn same_content_same_key() {
        let a = series_key("[LostYears] Re ZERO, Starting Life in Another World - S03E04 (WEB-DL).mkv");
        let b = series_key("[SubsPlease] Re Zero kara Hajimeru Isekai Seikatsu - 53 (1080p).mkv");
        let c = series_key("Re:Zero - Starting Life in Another World");
        assert_eq!(a, "re zero");
        assert_eq!(a, b);
        assert_eq!(a, c);
        let n1 = series_key("NieR.Automata.Ver1.1a.S01E03.1080p.AMZN.WEB-DL.DDP2.0.H.264-VARYG.mkv");
        let n2 = series_key("[Erai-raws] NieR-Automata Ver1_1a Part 2 - 04 [1080p].mkv");
        assert_eq!(n1, "nier automata");
        assert_eq!(n1, n2);
        assert_ne!(a, n1);
    }
}
