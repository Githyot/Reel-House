//! The opening or ending playing now, on YouTube: the song comes from MyAnimeList (through Jikan, the anime's
//! "theme songs" for this episode), the video is found with yt-dlp ("ytsearch"), and played inside the X-Ray panel
//! (a second mpv, see `controller_theme`).
//! yt-dlp is downloaded the first time (one file, from its GitHub releases) into `<data>/tools`. mpv decodes and joins
//! the streams with its own FFmpeg libraries, so no separate ffmpeg program is needed to play.
use crate::http::Http;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Theme {
    /// "op" or "ed"
    pub kind: String,
    /// 1 for "1: ..." (0 when MyAnimeList gives no number)
    pub number: u32,
    pub song: String,
    pub artist: String,
    /// episodes it is used for, inclusive (0, 0 when not said)
    pub eps: (u32, u32),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Video {
    pub theme: Theme,
    pub video_id: String,
    pub title: String,
    pub channel: String,
    pub duration: f64,
}

impl Video {
    pub fn url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.video_id)
    }
    pub fn thumbnail(&self) -> String {
        format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", self.video_id)
    }
}

/// `1: "Song" by Artist (eps 1-12)`, `"Song (Romaji)" by Artist`, `2: "Song" by Artist (eps 13, 15-24)`.
pub fn parse_theme(kind: &str, line: &str) -> Theme {
    let mut t = Theme { kind: kind.to_string(), ..Theme::default() };
    let mut rest = line.trim();
    if let Some((n, r)) = rest.split_once(':') {
        if let Ok(v) = n.trim().trim_start_matches('#').parse::<u32>() {
            t.number = v;
            rest = r.trim();
        }
    }
    if let Some(p) = rest.rfind("(ep") {
        let eps = &rest[p..];
        let nums: Vec<u32> = eps.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect();
        if let (Some(a), Some(b)) = (nums.first(), nums.last()) {
            t.eps = (*a, *b);
        }
        rest = rest[..p].trim();
    }
    let (song, artist) = match rest.rsplit_once(" by ") {
        Some((s, a)) => (s, a),
        None => (rest, ""),
    };
    t.song = song.trim().trim_matches('"').trim().to_string();
    t.artist = artist.trim().to_string();
    t
}

/// The openings and endings of a MyAnimeList entry.
pub fn themes(http: &dyn Http, mal_id: u64) -> Result<Vec<Theme>, String> {
    // through `mal::Jikan`: spaced to Jikan's 3 requests a second and retried after a 429
    let j = crate::mal::Jikan { http }.get(&format!("/anime/{}/themes", mal_id))?;
    let d = j.get("data");
    let mut out: Vec<Theme> = d.get("openings").items().iter().map(|x| parse_theme("op", &x.str())).collect();
    out.extend(d.get("endings").items().iter().map(|x| parse_theme("ed", &x.str())));
    Ok(out.into_iter().filter(|t| !t.song.is_empty()).collect())
}

/// What was found for a season is kept (`<data>/themes.json`): the opening and the ending are the same for the whole
/// season, so the next episode (and the next day) needs no search at all. The key says the MyAnimeList entry and which
/// theme ("123:op:1").
pub fn cache_key(mal: u64, t: &Theme) -> String {
    format!("{}:{}:{}", mal, t.kind, t.number)
}

pub fn cache_get(data: &Path, key: &str) -> Option<Video> {
    let j = crate::json::Json::parse(&std::fs::read_to_string(data.join("themes.json")).ok()?).ok()?;
    let v = j.get(key);
    let id = v.get("id").str();
    if id.is_empty() {
        return None;
    }
    Some(Video { theme: Theme::default(), video_id: id, title: v.get("title").str(), channel: v.get("channel").str(), duration: v.get("duration").as_f64().unwrap_or(0.0) })
}

pub fn cache_put(data: &Path, key: &str, v: &Video) {
    let path = data.join("themes.json");
    let mut j = std::fs::read_to_string(&path).ok().and_then(|t| crate::json::Json::parse(&t).ok()).unwrap_or_else(|| crate::json::Json::obj(vec![]));
    j.set(key, crate::json::Json::obj(vec![("id", v.video_id.clone().into()), ("title", v.title.clone().into()), ("channel", v.channel.clone().into()), ("duration", v.duration.into())]));
    let _ = std::fs::write(&path, j.dump());
}

/// The song of `kind` used in episode `ep` (the first one when the episodes are not said).
pub fn pick<'a>(list: &'a [Theme], kind: &str, ep: Option<i64>) -> Option<&'a Theme> {
    let of: Vec<&Theme> = list.iter().filter(|t| t.kind == kind).collect();
    if let Some(e) = ep.filter(|e| *e > 0).map(|e| e as u32) {
        if let Some(t) = of.iter().find(|t| t.eps.0 > 0 && t.eps.0 <= e && e <= t.eps.1.max(t.eps.0)) {
            return Some(t);
        }
    }
    of.first().copied()
}

pub fn ytdlp_path(data: &Path) -> PathBuf {
    data.join("tools").join(if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" })
}

/// One download of yt-dlp at a time: the opening and the ending (and a click on the card) can all ask at once.
static YTDLP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// A refresh of an old yt-dlp failed in this run: the old one is used without trying again until the next start
/// (offline, each lookup would wait for the download to fail).
static REFRESH_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// YouTube changes often and an old yt-dlp stops finding videos: after two weeks a new one is downloaded.
fn too_old(p: &Path) -> bool {
    let age = std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
    age.map(|a| a.as_secs() > 14 * 24 * 3600).unwrap_or(false) && !REFRESH_FAILED.load(std::sync::atomic::Ordering::SeqCst)
}

/// yt-dlp on this computer: the one in the tools folder (downloaded when missing or old), else one on the PATH.
pub fn ensure_ytdlp(http: &dyn Http, data: &Path) -> Result<PathBuf, String> {
    crate::platform::needs_program("Theme songs need yt-dlp, a program that cannot run on phones and tablets.")?;
    let p = ytdlp_path(data);
    if p.is_file() && !too_old(&p) {
        return Ok(p);
    }
    let _guard = YTDLP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // another thread may have downloaded it while this one waited
    if p.is_file() && !too_old(&p) {
        return Ok(p);
    }
    let have_old = p.is_file();
    let _ = std::fs::create_dir_all(p.parent().unwrap_or(data));
    // pieces of downloads cut short by an earlier run (an hour old, so none being written now)
    if let Ok(rd) = std::fs::read_dir(p.parent().unwrap_or(data)) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|a| a.as_secs() > 3600).unwrap_or(false);
            if n.starts_with("yt-dlp.") && n.ends_with(".part") && old {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    match download_ytdlp(http, &p) {
        Ok(()) => Ok(p),
        Err(e) => {
            if have_old {
                crate::debug::log("theme", &format!("yt-dlp could not be updated, the old one is used: {}", e));
                REFRESH_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
                return Ok(p);
            }
            // a yt-dlp installed by the user
            if Command::new("yt-dlp").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false) {
                Ok(PathBuf::from("yt-dlp"))
            } else {
                Err(format!("yt-dlp could not be downloaded ({})", e))
            }
        }
    }
}

/// Downloads the newest yt-dlp over `p` (through a file of its own, renamed at the end).
fn download_ytdlp(http: &dyn Http, p: &Path) -> Result<(), String> {
    let asset = if cfg!(windows) { "yt-dlp.exe" } else if cfg!(target_os = "macos") { "yt-dlp_macos" } else { "yt-dlp" };
    let url = format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}", asset);
    // a name of its own, so a download left half done by an earlier run (or another program) is never mixed in
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let part = p.with_file_name(format!("yt-dlp.{}-{}.part", std::process::id(), stamp));
    let got = http.download(&url, &part, &|_, _| {}).and_then(|()| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755));
        }
        // on Windows this fails while the old yt-dlp.exe is running; the old one is kept then
        std::fs::rename(&part, p).map_err(|e| e.to_string())
    });
    if got.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    got
}

/// Runs yt-dlp and gives what it printed. A yt-dlp that hangs (a network that never answers) is stopped after
/// 90 seconds, so the lookup ends with an error instead of waiting for ever.
fn run(ytdlp: &Path, args: &[&str]) -> Result<String, String> {
    crate::platform::needs_program("Theme songs need yt-dlp, a program that cannot run on phones and tablets.")?;
    use std::io::Read;
    let mut cmd = Command::new(ytdlp);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::suwayomi_host::hide(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("yt-dlp could not start: {}", e))?;
    // the output is read on threads while waiting: a full pipe would make yt-dlp wait for us and never end
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut b);
        }
        b
    });
    let err_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut b);
        }
        b
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if started.elapsed().as_secs() >= 90 => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => {
                let _ = child.kill();
                return Err(format!("yt-dlp: {}", e));
            }
        }
    };
    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    let Some(status) = status else {
        return Err("yt-dlp took too long (no answer in 90 seconds).".into());
    };
    if !status.success() {
        let err = String::from_utf8_lossy(&stderr);
        return Err(format!("yt-dlp: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim()));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

/// What is searched on YouTube for the song, from the most precise to the widest. A plain "<song> <anime>" search
/// brought trailers and PVs first: every query says "opening" / "ending" and the fansub words for the clean video
/// (NCOP / NCED, creditless).
pub fn queries(anime: &str, t: &Theme) -> Vec<String> {
    let op = t.kind == "op";
    let (word, short, nc) = if op { ("opening", "OP", "NCOP") } else { ("ending", "ED", "NCED") };
    let n = if t.number > 0 { format!(" {}", t.number) } else { String::new() };
    let nn = if t.number > 0 { t.number.to_string() } else { String::new() };
    // "opening 2", not "<anime> 2 opening", which YouTube reads as the second season
    let list = [
        format!("{} {}{} {} {}", anime, word, n, t.song, t.artist),
        format!("{} {}{} creditless {}{}", anime, short, nn, nc, nn),
        format!("{} {} {} {}", t.song, t.artist, anime, word),
    ];
    let mut out: Vec<String> = Vec::new();
    for q in list {
        let q = q.split_whitespace().collect::<Vec<_>>().join(" ");
        if !q.is_empty() && !out.contains(&q) {
            out.push(q);
        }
    }
    out
}

/// The first query (kept for callers that want one line, like the debug report).
pub fn query(anime: &str, t: &Theme) -> String {
    queries(anime, t).into_iter().next().unwrap_or_default()
}

/// One result of a YouTube search.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub id: String,
    pub title: String,
    pub channel: String,
    /// seconds; 0 when YouTube did not say
    pub duration: f64,
}

/// What yt-dlp prints with `--print "%(id)s\t%(duration)s\t%(channel)s\t%(title)s"`: one result per line.
pub fn parse_results(out: &str) -> Vec<Found> {
    out.lines()
        .filter_map(|l| {
            let mut p = l.trim_end_matches('\r').splitn(4, '\t');
            let id = p.next()?.trim().to_string();
            // a YouTube id is 11 letters; warnings and empty lines are not results
            if id.len() != 11 || id.contains(' ') {
                return None;
            }
            let duration = p.next().and_then(|d| d.trim().parse::<f64>().ok()).unwrap_or(0.0);
            let channel = p.next().unwrap_or("").trim();
            let title = p.next().unwrap_or("").trim().to_string();
            Some(Found { id, title, channel: if channel == "NA" { String::new() } else { channel.to_string() }, duration })
        })
        .collect()
}

/// Lowercase words of a text (letters and digits of any script); everything else separates them.
fn words(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_string()).collect()
}

/// The words joined by single spaces, with a space at both ends, so " op " finds the whole word only.
fn spaced(s: &str) -> String {
    format!(" {} ", words(s).join(" "))
}

/// The song's names to look for: "Gurenge (紅蓮華)" is "gurenge" or "紅蓮華"; "(feat. X)" and "(TV size)" are dropped.
fn song_names(song: &str) -> Vec<String> {
    let mut out = Vec::new();
    let main = song.split(['(', '（', '[']).next().unwrap_or(song);
    out.push(spaced(main));
    for part in song.split(['(', '（']).skip(1) {
        let inner = part.split([')', '）']).next().unwrap_or("");
        let l = inner.to_lowercase();
        if !(l.starts_with("feat") || l.starts_with("ft.") || l.contains("size") || l.contains("ver")) {
            out.push(spaced(inner));
        }
    }
    out.retain(|s| s.trim().chars().count() >= 2);
    out
}

/// The number written next to OP / opening / NCOP (or the ED words) in a title: "op2", "opening 2", "ncop 02".
fn number_near(ws: &[String], kind_words: &[&str], short: &[&str]) -> Option<u32> {
    for (i, w) in ws.iter().enumerate() {
        for s in short {
            if let Some(rest) = w.strip_prefix(s) {
                if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                    return rest.parse().ok();
                }
            }
        }
        if kind_words.contains(&w.as_str()) || short.contains(&w.as_str()) {
            if let Some(n) = ws.get(i + 1).and_then(|x| x.parse::<u32>().ok()).filter(|n| *n > 0 && *n < 60) {
                return Some(n);
            }
        }
    }
    None
}

/// Words that mark a video as something other than the song: (word or phrase, penalty). Phrases are matched on the
/// spaced form of the title, so "pv" never matches inside another word.
const BAD: &[(&str, i32)] = &[
    (" trailer ", 90), (" official trailer ", 30), (" pv ", 70), (" teaser ", 70), (" cm ", 60), (" preview ", 60), (" promo ", 50),
    (" promotional ", 50), (" reaction ", 80), (" reacts ", 70), (" react ", 60), (" amv ", 70), (" cover ", 60), (" covered ", 50),
    (" piano ", 60), (" guitar ", 50), (" violin ", 50), (" drum ", 45), (" drums ", 45), (" instrumental ", 35), (" nightcore ", 60),
    (" slowed ", 50), (" sped up ", 50), (" remix ", 45), (" 8d ", 40), (" 1 hour ", 60), (" 10 hours ", 60), (" loop ", 35),
    (" tutorial ", 60), (" mmd ", 50), (" fanmade ", 40), (" fan made ", 40), (" ranking ", 50), (" top 10 ", 60), (" top 20 ", 60),
    (" lyrics ", 15), (" lyric ", 15), (" full version ", 20), (" full ver ", 20), (" concert ", 30), (" live ", 15), (" episode ", 15),
    (" review ", 50), (" tiktok ", 40), (" shorts ", 30), (" vs ", 25), (" kpop ", 20),
];
/// The same for Japanese titles (searched as plain text: Japanese has no spaces between words).
const BAD_JA: &[(&str, i32)] = &[("予告", 90), ("特報", 70), ("番宣", 60), ("歌ってみた", 60), ("弾いてみた", 60), ("ピアノ", 50), ("反応", 50)];

/// How well a result fits the song: positive for an opening / ending video of this song, very negative for trailers,
/// PVs, reactions and covers. `pick_best` keeps the best at or above `MIN_SCORE`.
pub fn score(f: &Found, anime: &str, t: &Theme) -> i32 {
    let op = t.kind == "op";
    let title = spaced(&f.title);
    let raw = f.title.to_lowercase();
    let ws = words(&f.title);
    let has = |w: &str| ws.iter().any(|x| x == w);
    let mut s = 0;

    // ---- opening / ending words ----
    // (whole words, prefixes followed by a number "op2" / "ncop1", the creditless prefix) of the asked kind and the other
    const OP: (&[&str], &[&str], &str) = (&["opening", "openings", "op"], &["op", "ncop"], "ncop");
    const ED: (&[&str], &[&str], &str) = (&["ending", "endings", "ed"], &["ed", "nced"], "nced");
    let ((kind_words, short, nc), (other_words, other_short, other_nc)) = if op { (OP, ED) } else { (ED, OP) };
    let is_kind = kind_words.iter().any(|w| has(w)) || number_near(&ws, kind_words, short).is_some() || raw.contains(if op { "オープニング" } else { "エンディング" });
    let is_other = other_words.iter().any(|w| has(w)) || number_near(&ws, other_words, other_short).is_some() || raw.contains(if op { "エンディング" } else { "オープニング" });
    if is_kind {
        s += 30;
    }
    if ws.iter().any(|w| w.starts_with(nc)) {
        s += 20;
    }
    if has("creditless") || has("textless") || title.contains(" non credit ") || has("noncredit") || title.contains(" no credits ") || raw.contains("ノンクレジット") {
        s += 15;
    }
    // asked for the opening, the title only speaks of the ending (an "OP & ED" compilation keeps both)
    if is_other && !is_kind {
        s -= 30;
    }
    if ws.iter().any(|w| w.starts_with(other_nc)) && !is_kind {
        s -= 20;
    }
    if t.number > 0 {
        match number_near(&ws, kind_words, short) {
            Some(n) if n == t.number => s += 12,
            Some(_) => s -= 20,
            None => {}
        }
    }

    // ---- the song, the artist and the anime ----
    let names = song_names(&t.song);
    if names.iter().any(|n| title.contains(n.as_str())) {
        s += 30;
    } else if let Some(main) = names.first() {
        let sw: Vec<&str> = main.split_whitespace().collect();
        let got = sw.iter().filter(|w| has(w)).count();
        if !sw.is_empty() {
            s += (got * 18 / sw.len()) as i32;
        }
    }
    let artist = t.artist.split(['(', '（']).next().unwrap_or("");
    let artist_sp = spaced(artist);
    let chan = spaced(&f.channel);
    let artist_hit = artist_sp.trim().chars().count() >= 2 && (title.contains(artist_sp.as_str()) || chan.contains(artist_sp.as_str()));
    if artist_hit {
        s += 15;
    }
    let aw = words(anime);
    if !aw.is_empty() {
        let got = aw.iter().filter(|w| has(w)).count();
        s += (got * 15 / aw.len()) as i32;
        // The name of the show must be in the title: an opening of another anime has the opening words and a good length
        // too, and used to win ("Slime" for Re:Zero). Only the song's own name (4 letters or more) can stand in
        // for a title that spells the show differently (English / Japanese).
        let song_hit = names.iter().any(|n| n.chars().count() >= 4 && title.contains(n.as_str()));
        // (the first two words are the name people write: "Re:Zero" for "Re:Zero - Starting Life in Another World")
        let lead = aw.iter().take(2).all(|w| has(w));
        if got * 2 < aw.len() && !lead && !song_hit {
            s -= 60;
        }
    }

    // ---- length: a TV-size opening is about 90 s; a full song about 4 min ----
    let d = f.duration;
    if d > 0.0 {
        if (60.0..=200.0).contains(&d) {
            s += 20;
        } else if (200.0..=360.0).contains(&d) {
            s += 4;
        } else if d < 45.0 {
            s -= 25; // a teaser or a Short
        } else if d > 360.0 {
            s -= 45; // a full album, an hour-long loop, a compilation
        }
    }

    // ---- what it is not ----
    for (w, p) in BAD {
        if title.contains(w) {
            s -= p;
        }
    }
    for (w, p) in BAD_JA {
        if raw.contains(w) {
            s -= p;
        }
    }
    s
}

/// The lowest score taken as the song: an opening word, or the song's own name, and a believable length.
pub const MIN_SCORE: i32 = 45;

/// The best result at or above `MIN_SCORE`, with its score.
pub fn pick_best(list: &[Found], anime: &str, t: &Theme) -> Option<(Found, i32)> {
    let mut best: Option<(Found, i32)> = None;
    for f in list {
        let sc = score(f, anime, t);
        if sc >= MIN_SCORE && best.as_ref().map(|(_, b)| sc > *b).unwrap_or(true) {
            best = Some((f.clone(), sc));
        }
    }
    best
}

/// Runs one search (`n` results).
fn search_list(ytdlp: &Path, q: &str, n: u32) -> Result<Vec<Found>, String> {
    let out = run(ytdlp, &["--flat-playlist", "--no-warnings", "--skip-download", "--print", "%(id)s\t%(duration)s\t%(channel)s\t%(title)s", &format!("ytsearch{}:{}", n, q)])?;
    Ok(parse_results(&out))
}

/// A score this high is clearly the song (an opening word, its name, a TV-size length): no need for more searches.
const SURE_SCORE: i32 = 90;

/// Finds the opening / ending video of `t`: a few searches (the precise one first), every result scored, the best
/// kept. "Not found" when nothing looks like the song: a trailer is never played in its place.
pub fn find_video(ytdlp: &Path, anime: &str, t: &Theme) -> Result<Video, String> {
    let mut seen: Vec<Found> = Vec::new();
    let mut last_err = None;
    for (i, q) in queries(anime, t).iter().enumerate() {
        match search_list(ytdlp, q, if i == 0 { 8 } else { 5 }) {
            Ok(list) => {
                for f in list {
                    if !seen.iter().any(|x| x.id == f.id) {
                        seen.push(f);
                    }
                }
            }
            Err(e) => last_err = Some(e),
        }
        if pick_best(&seen, anime, t).map(|(_, s)| s >= SURE_SCORE).unwrap_or(false) {
            break;
        }
    }
    if seen.is_empty() {
        if let Some(e) = last_err {
            return Err(e);
        }
    }
    match pick_best(&seen, anime, t) {
        Some((f, sc)) => {
            crate::debug::log("theme", &format!("picked \"{}\" ({}, score {}) of {} results", f.title, f.id, sc, seen.len()));
            Ok(Video { theme: t.clone(), video_id: f.id, title: f.title, channel: f.channel, duration: f.duration })
        }
        None => {
            let what = if t.kind == "op" { "opening" } else { "ending" };
            crate::debug::log("theme", &format!("no {} among: {}", what, seen.iter().map(|f| format!("\"{}\" {}", f.title, score(f, anime, t))).collect::<Vec<_>>().join(" | ")));
            Err(format!("The {} video was not found on YouTube (only trailers or other videos came up).", what))
        }
    }
}

/// Kept for callers that search with one query: the best result of it, scored like `find_video`.
pub fn search(ytdlp: &Path, q: &str) -> Result<Video, String> {
    let list = search_list(ytdlp, q, 5)?;
    let f = list.into_iter().find(|f| f.duration == 0.0 || (50.0..=480.0).contains(&f.duration)).ok_or_else(|| "Nothing found on YouTube.".to_string())?;
    Ok(Video { theme: Theme::default(), video_id: f.id, title: f.title, channel: f.channel, duration: f.duration })
}

/// What mpv plays for a YouTube video: one address with picture and sound, or (when YouTube only offers them apart
/// today) the picture and a separate sound track that mpv adds with `audio-files`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stream {
    pub video: String,
    pub audio: Option<String>,
}

/// The formats asked for, from best to simplest. YouTube now hides most formats from clients without a "PO token";
/// a combined stream up to 720p is tried first, then picture + sound apart, then anything at all.
const FORMATS: &[&str] = &["b[height<=720][vcodec!=none][acodec!=none]/bv*[height<=720]+ba/b/bv*+ba", "bv*+ba/b"];
/// Other YouTube "clients" yt-dlp can pretend to be: some still give formats when the default ones do not.
const CLIENTS: &str = "youtube:player_client=default,web_safari,android_vr,tv";

/// yt-dlp's error says it could not read YouTube's formats: an old yt-dlp is the usual reason.
pub fn outdated_error(e: &str) -> bool {
    let l = e.to_lowercase();
    l.contains("requested format is not available") || l.contains("signature") || l.contains("nsig") || l.contains("no video formats found")
}

/// The addresses yt-dlp printed with `-g`: the first is the picture (or both), a second one is the sound.
pub fn parse_stream(out: &str) -> Option<Stream> {
    let urls: Vec<&str> = out.lines().map(|l| l.trim()).filter(|l| l.starts_with("http")).collect();
    let video = urls.first()?.to_string();
    Some(Stream { video, audio: urls.get(1).map(|s| s.to_string()) })
}

/// Asks yt-dlp for the addresses, trying the formats and then the other clients.
fn try_streams(ytdlp: &Path, video_url: &str) -> Result<Stream, String> {
    let mut last = String::new();
    for clients in [None, Some(CLIENTS)] {
        for f in FORMATS {
            let mut args = vec!["-g", "--no-warnings", "--no-playlist", "-f", f];
            if let Some(c) = clients {
                args.extend(["--extractor-args", c]);
            }
            args.push(video_url);
            match run(ytdlp, &args) {
                Ok(out) => match parse_stream(&out) {
                    Some(s) => return Ok(s),
                    None => last = "yt-dlp gave no address to play.".into(),
                },
                // another format or client only helps when the formats were the problem
                Err(e) if outdated_error(&e) => last = e,
                Err(e) => return Err(e),
            }
        }
    }
    Err(last)
}

/// A direct address for mpv to play at once (720p at most), with the sound apart when YouTube gives it apart.
/// When yt-dlp cannot read the formats (it is often just old: YouTube changed), a new yt-dlp is downloaded and
/// asked again.
pub fn stream_url(http: &dyn Http, data: &Path, ytdlp: &Path, video_url: &str) -> Result<Stream, String> {
    match try_streams(ytdlp, video_url) {
        Ok(s) => Ok(s),
        Err(e) if outdated_error(&e) => match refresh_ytdlp(http, data) {
            Some(fresh) => {
                crate::debug::log("theme", &format!("yt-dlp refreshed after: {}", e));
                try_streams(&fresh, video_url)
            }
            None => Err(e),
        },
        Err(e) => Err(e),
    }
}

/// Downloads yt-dlp again now, whatever its age (once per run: a second failure is not a stale yt-dlp).
/// None when it was already refreshed in this run, or when it could not be.
pub fn refresh_ytdlp(http: &dyn Http, data: &Path) -> Option<PathBuf> {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) || REFRESH_FAILED.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    let p = ytdlp_path(data);
    if !p.is_file() {
        return None; // a yt-dlp of the user (on the PATH): not ours to replace
    }
    let _guard = YTDLP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match download_ytdlp(http, &p) {
        Ok(()) => Some(p),
        Err(e) => {
            crate::debug::log("theme", &format!("yt-dlp could not be refreshed: {}", e));
            REFRESH_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_are_read() {
        let t = parse_theme("op", "2: \"Kaikai Kitan\" by Eve (eps 1-24)");
        assert_eq!((t.number, t.song.as_str(), t.artist.as_str(), t.eps), (2, "Kaikai Kitan", "Eve", (1, 24)));
        let u = parse_theme("ed", "\"Lost in Paradise (feat. AKLO)\" by ALI");
        assert_eq!((u.number, u.song.as_str(), u.artist.as_str()), (0, "Lost in Paradise (feat. AKLO)", "ALI"));
        let list = vec![parse_theme("op", "1: \"A\" by X (eps 1-12)"), parse_theme("op", "2: \"B\" by Y (eps 13-24)")];
        assert_eq!(pick(&list, "op", Some(14)).map(|t| t.song.as_str()), Some("B"));
        assert!(query("Jujutsu Kaisen", &t).contains("opening 2"));
        let q = queries("Jujutsu Kaisen", &t);
        assert!(q[1].contains("OP2") && q[1].contains("NCOP2") && q[1].contains("creditless"));
        let e = queries("Frieren", &parse_theme("ed", "1: \"Anytime Anywhere\" by milet"));
        assert!(e[0].contains("ending 1") && e[1].contains("NCED1"));
    }

    fn f(id: &str, title: &str, dur: f64) -> Found {
        Found { id: format!("{:_<11}", id), title: title.into(), channel: String::new(), duration: dur }
    }

    #[test]
    fn search_results_are_read() {
        let out = "abcdefghijk\t89\tCrunchyroll\tJujutsu Kaisen - Opening | Kaikai Kitan\nWARNING: something\nzyxwvutsrqp\tNA\tNA\tTitle\twith tab\n";
        let r = parse_results(out);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].duration, r[0].channel.as_str()), (89.0, "Crunchyroll"));
        assert_eq!((r[1].duration, r[1].channel.as_str(), r[1].title.as_str()), (0.0, "", "Title\twith tab"));
    }

    #[test]
    fn the_opening_wins_over_trailers() {
        let t = parse_theme("op", "1: \"Kaikai Kitan\" by Eve (eps 1-13)");
        let list = vec![
            f("trailer", "Jujutsu Kaisen - Official Trailer", 120.0),
            f("pv", "TVアニメ「呪術廻戦」PV第2弾 Jujutsu Kaisen PV", 95.0),
            f("piano", "Kaikai Kitan - Eve (Jujutsu Kaisen OP) Piano Cover", 210.0),
            f("react", "JUJUTSU KAISEN Opening REACTION", 600.0),
            f("ncop", "Jujutsu Kaisen - Opening 1 | 4K | 60FPS | Creditless | Kaikai Kitan", 90.0),
            f("mv", "Eve MV - Kaikai Kitan", 225.0),
        ];
        let (best, sc) = pick_best(&list, "Jujutsu Kaisen", &t).unwrap();
        assert!(best.id.starts_with("ncop"), "{:?}", best);
        assert!(sc >= SURE_SCORE);
        // every bad one scores below the line
        for bad in &list[..4] {
            assert!(score(bad, "Jujutsu Kaisen", &t) < MIN_SCORE, "{} scored {}", bad.title, score(bad, "Jujutsu Kaisen", &t));
        }
        // with no opening video, the music video of the song is still the song
        assert!(pick_best(&list[5..], "Jujutsu Kaisen", &t).is_some());
    }

    #[test]
    fn only_trailers_means_not_found() {
        let t = parse_theme("op", "\"Gurenge (紅蓮華)\" by LiSA");
        let list = vec![
            f("t1", "Demon Slayer: Kimetsu no Yaiba - Official Trailer", 140.0),
            f("t2", "Demon Slayer Season 2 Teaser PV", 60.0),
            f("t3", "Kimetsu no Yaiba CM 30s", 30.0),
        ];
        assert!(pick_best(&list, "Demon Slayer", &t).is_none());
        // the Japanese name counts as the song
        let ja = vec![f("ok", "鬼滅の刃 ノンクレジットオープニング 紅蓮華", 90.0)];
        assert!(pick_best(&ja, "Demon Slayer", &t).is_some());
    }

    #[test]
    fn endings_and_numbers() {
        let ed = parse_theme("ed", "2: \"Lost in Paradise\" by ALI (eps 1-13)");
        let list = vec![
            f("op", "Jujutsu Kaisen Opening 2 Creditless", 90.0),
            f("ed1", "Jujutsu Kaisen Ending 1 NCED", 90.0),
            f("ed2", "Jujutsu Kaisen ED2 NCED Lost in Paradise", 90.0),
            f("full", "ALI - Lost in Paradise feat. AKLO (Full Version) 1 Hour Loop", 3600.0),
        ];
        let (best, _) = pick_best(&list, "Jujutsu Kaisen", &ed).unwrap();
        assert!(best.id.starts_with("ed2"), "{:?}", best);
        assert!(score(&list[0], "Jujutsu Kaisen", &ed) < score(&list[1], "Jujutsu Kaisen", &ed));
        assert!(score(&list[3], "Jujutsu Kaisen", &ed) < MIN_SCORE);
    }

    #[test]
    fn streams_are_read() {
        let one = parse_stream("https://v.example/a\n").unwrap();
        assert_eq!((one.video.as_str(), one.audio), ("https://v.example/a", None));
        let two = parse_stream("https://v.example/v\nhttps://v.example/s\n").unwrap();
        assert_eq!(two.audio.as_deref(), Some("https://v.example/s"));
        assert!(parse_stream("WARNING: x\n").is_none());
        assert!(outdated_error("yt-dlp: ERROR: [youtube] P4l9d-riD4o: Requested format is not available. Use --list-formats"));
        assert!(!outdated_error("yt-dlp: ERROR: Video unavailable"));
    }

    #[test]
    fn a_video_of_another_anime_is_not_taken() {
        let t = Theme { kind: "op".into(), number: 1, song: "Redo".into(), artist: "Konomi Suzuki".into(), eps: (0, 0) };
        let other = Found { id: "a".into(), title: "That Time I Got Reincarnated as a Slime - Opening 1".into(), channel: "Anicrad".into(), duration: 90.0 };
        let right = Found { id: "b".into(), title: "Re:Zero - Opening 1 | creditless".into(), channel: "Pavia".into(), duration: 90.0 };
        let anime = "Re:Zero - Starting Life in Another World";
        assert!(score(&other, anime, &t) < MIN_SCORE, "scored {}", score(&other, anime, &t));
        assert!(score(&right, anime, &t) >= MIN_SCORE, "scored {}", score(&right, anime, &t));
        assert_eq!(pick_best(&[other, right.clone()], anime, &t).map(|x| x.0.id), Some("b".to_string()));
    }
}
