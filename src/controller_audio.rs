//! Audio tracks loaded from outside the video (a dub in another language, e.g. Brazilian Portuguese), their delay, and
//! lining them up with the video's own audio on their own (`audiosync`).
//!
//! - "Load an audio track…" in the player's Audio menu adds a file with mpv's `audio-add` and selects it.
//! - Each loaded file is kept for the episode in `<data>/external_audio.json` with its delay, and comes back (selected
//!   when it was the one playing) whenever the episode is played again.
//! - mpv's `audio-delay` is one value for the whole player: it is set to the delay of the track picked and back to 0
//!   for the video's own tracks, so switching between the dub and the original stays in time.
//! - The folder of the file is remembered for the series: the next episode looks there for a file with its number
//!   (S06E04, E04, - 04 …) and loads it by itself.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::audiosync;

/// One audio file loaded for one episode.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtAudio {
    pub sid: String,
    pub path: String,
    /// mpv `audio-delay` for this track, in seconds.
    pub delay: f64,
    /// It was the track playing when the episode was last watched.
    pub active: bool,
    /// The delay came from the automatic sync (not typed by hand).
    pub synced: bool,
    /// Which audio track of the file (1 = first): a dual-audio video has several.
    pub file_aid: i64,
    /// Streamed from another torrent of the episode (`controller_dub`): only for this session, never saved.
    pub remote: bool,
}

impl ExtAudio {
    pub fn new(sid: &str, path: &str) -> ExtAudio {
        ExtAudio { sid: sid.to_string(), path: path.to_string(), delay: 0.0, active: true, synced: false, file_aid: 1, remote: false }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExtAudioStore {
    pub tracks: Vec<ExtAudio>,
    /// series id -> folder its dubs came from
    pub folders: Vec<(String, String)>,
    /// The user turned off looking for a dub in the episode's other torrents (on by default).
    pub no_auto_dub: bool,
}

/// The automatic sync running or last finished, for the Audio menu.
#[derive(Clone, Debug, Default)]
pub struct AudioSyncState {
    pub busy: bool,
    /// What the last sync found, in English (translated by the interface).
    pub note: String,
    /// A track was loaded by itself at the start of the episode: sync it on the next tick.
    pub pending: bool,
    /// Select this track (file) when its sync succeeds (a dub found in another torrent).
    pub select_after: Option<String>,
    /// The dub search in other torrents (controller_dub): session token it ran for, running now, its torrent in the
    /// engine (base, id) to delete when the episode ends, and what it found for the menu.
    pub dub_token: u64,
    pub dub_busy: bool,
    pub dub_torrents: Vec<(String, u64)>,
    pub dub_note: String,
    /// Each opening of another stream gets a number; answers of an older one are dropped. The one picked by hand.
    pub dub_gen: u64,
    pub dub_manual_gen: u64,
    /// What is being opened (for the menu), and the tracks being added that mpv has not shown yet.
    pub dub_opening: String,
    /// where opening / adding the other stream is ("Connecting to peers: 3 found · …", "12 s"), shown beside the note
    pub dub_progress: String,
    pub dub_pending: Option<super::controller_dub::DubPending>,
    /// Links and stream indexes already added to this video (not offered again).
    pub extra_urls: Vec<String>,
    pub extra_opened: Vec<usize>,
    /// Syncing the subtitle with the speech (crate::subsync): running now, and what the last one found.
    pub sub_busy: bool,
    pub sub_note: String,
    /// Session token whose downloaded subtitle was already synced by itself (once per video).
    pub sub_auto_token: u64,
}

const FILE: &str = "external_audio.json";

/// Extensions taken as audio sources (a whole video works too: its sound is used).
const AUDIO_EXT: &[&str] = &["mka", "mp3", "aac", "m4a", "ac3", "eac3", "dts", "flac", "ogg", "oga", "opus", "wav", "wma", "mp2", "mkv", "mp4", "avi", "webm"];

pub fn load_ext_audio(dir: &Path) -> ExtAudioStore {
    let Ok(text) = std::fs::read_to_string(dir.join(FILE)) else { return ExtAudioStore::default() };
    let Ok(j) = Json::parse(&text) else { return ExtAudioStore::default() };
    let tracks = j
        .get("tracks")
        .items()
        .iter()
        .map(|x| ExtAudio {
            sid: x.get("sid").str(),
            path: x.get("path").str(),
            delay: x.get("delay").as_f64().unwrap_or(0.0),
            active: x.get("active").as_bool().unwrap_or(false),
            synced: x.get("synced").as_bool().unwrap_or(false),
            file_aid: x.get("file_aid").as_i64().unwrap_or(1).max(1),
            remote: false,
        })
        .filter(|t| !t.sid.is_empty() && Path::new(&t.path).exists())
        .collect();
    let folders = j.get("folders").items().iter().map(|x| (x.get("show").str(), x.get("dir").str())).filter(|(s, d)| !s.is_empty() && !d.is_empty()).collect();
    ExtAudioStore { tracks, folders, no_auto_dub: j.get("no_auto_dub").as_bool().unwrap_or(false) }
}

pub(super) fn save_ext_audio(dir: &Path, st: &ExtAudioStore) {
    let tracks = Json::Arr(
        st.tracks
            .iter()
            .filter(|t| !t.remote)
            .map(|t| {
                Json::obj(vec![
                    ("sid", Json::Str(t.sid.clone())),
                    ("path", Json::Str(t.path.clone())),
                    ("delay", Json::Num(t.delay)),
                    ("active", Json::Bool(t.active)),
                    ("synced", Json::Bool(t.synced)),
                    ("file_aid", Json::Num(t.file_aid as f64)),
                ])
            })
            .collect(),
    );
    let folders = Json::Arr(st.folders.iter().map(|(s, d)| Json::obj(vec![("show", Json::Str(s.clone())), ("dir", Json::Str(d.clone()))])).collect());
    let _ = std::fs::write(dir.join(FILE), Json::obj(vec![("tracks", tracks), ("folders", folders), ("no_auto_dub", Json::Bool(st.no_auto_dub))]).dump());
}

/// The language a file name suggests, as mpv's 3-letter code ("" when nothing is said).
pub fn lang_from_name(name: &str) -> &'static str {
    let l = name.to_lowercase();
    let words: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |w: &str| words.contains(&w);
    if l.contains("pt-br") || l.contains("pt_br") || l.contains("ptbr") || has("dublado") || has("por") || has("pt") || has("portuguese") || has("portugues") {
        "por"
    } else if has("spa") || has("es") || has("latino") || has("spanish") || has("castellano") || has("doblado") {
        "spa"
    } else if has("eng") || has("en") || has("english") {
        "eng"
    } else if has("jpn") || has("ja") || has("jp") || has("japanese") {
        "jpn"
    } else {
        ""
    }
}

/// Numbers standing alone in a file name (not part of a longer run of digits), without the usual noise
/// (resolutions, codecs, years, audio channels).
fn numbers(name: &str) -> Vec<i64> {
    let b = name.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let s = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let prev = if s > 0 { b[s - 1].to_ascii_lowercase() } else { b' ' };
            let next = if i < b.len() { b[i].to_ascii_lowercase() } else { b' ' };
            // x264, h265, 1080p, 5.1, 2.0, 10bit
            if prev == b'x' || prev == b'h' || prev == b'.' || next == b'p' || next == b'.' || next == b'b' || next == b'k' {
                continue;
            }
            if let Ok(v) = name[s..i].parse::<i64>() {
                if !(1900..=2100).contains(&v) && v < 1000 {
                    out.push(v);
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// How well file name `name` fits episode `ep` of season `season`: 2 = "S06E04" style, 1 = the number alone, 0 = no.
pub fn episode_fit(name: &str, season: Option<i64>, ep: i64) -> u8 {
    let l = name.to_lowercase();
    let b = l.as_bytes();
    // "s06e04", "s6e4", "6x04"
    let mut i = 0;
    let mut other_ep = false;
    while i < b.len() {
        let (sn, en) = if b[i] == b's' && i + 1 < b.len() && b[i + 1].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphabetic()) {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j + 1 < b.len() && b[j] == b'e' && b[j + 1].is_ascii_digit() {
                let mut k = j + 1;
                while k < b.len() && b[k].is_ascii_digit() {
                    k += 1;
                }
                (l[i + 1..j].parse::<i64>().ok(), l[j + 1..k].parse::<i64>().ok())
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        if let Some(e) = en {
            if e == ep && (season.is_none() || sn == season) {
                return 2;
            }
            other_ep = true;
        }
        i += 1;
    }
    if other_ep {
        return 0;
    }
    if numbers(&l).contains(&ep) {
        1
    } else {
        0
    }
}

/// The audio file in `dir` for episode `ep` (season `season`): the only one fitting best, never `skip` (the video
/// itself). None when nothing fits or two fit equally.
pub fn find_episode_audio(dir: &Path, season: Option<i64>, ep: i64, skip: &str) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut best: Vec<(u8, PathBuf)> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !AUDIO_EXT.contains(&ext.as_str()) || !p.is_file() || p.to_string_lossy() == skip {
            continue;
        }
        let name = p.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
        let fit = episode_fit(&name, season, ep);
        if fit > 0 {
            best.push((fit, p));
        }
    }
    let top = best.iter().map(|b| b.0).max()?;
    let mut tops: Vec<PathBuf> = best.into_iter().filter(|b| b.0 == top).map(|b| b.1).collect();
    if tops.len() == 1 {
        tops.pop()
    } else {
        None
    }
}

fn title_of(path: &str) -> String {
    Path::new(path).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "Audio".to_string())
}

/// Adds the saved tracks of episode `sid` to the player (selected and with its delay when it was the one playing);
/// with none saved, looks in the series' folder for this episode's file. True when a file was found that way (the
/// caller then syncs it).
pub fn restore_ext_audio(mpv: &Mpv, st: &mut ExtAudioStore, data_dir: &Path, sid: &str, ep: &EpInfo) -> bool {
    st.tracks.retain(|t| !t.remote);
    let mine: Vec<ExtAudio> = st.tracks.iter().filter(|t| t.sid == sid).cloned().collect();
    if !mine.is_empty() {
        for t in &mine {
            let title = title_of(&t.path);
            let lang = lang_from_name(&title);
            let flag = if t.active { "select" } else { "auto" };
            let _ = mpv.cmd(&["audio-add", &t.path, flag, &title, lang]);
            if t.active && t.file_aid > 1 {
                if let Some(id) = file_track_id(mpv, &t.path, t.file_aid) {
                    let _ = mpv.set("aid", &id.to_string());
                }
            }
            if t.active {
                let _ = mpv.set("audio-delay", &format!("{:.3}", t.delay));
            }
        }
        crate::debug::log("player", &format!("audio: {} saved track(s) added back for this episode", mine.len()));
        return false;
    }
    let (Some(e), false) = (ep.episode, ep.show.is_empty()) else { return false };
    let Some(dir) = st.folders.iter().find(|(s, _)| *s == ep.show).map(|(_, d)| d.clone()) else { return false };
    let me = mpv.get_str("path").unwrap_or_default();
    let Some(p) = find_episode_audio(Path::new(&dir), ep.season, e, &me) else {
        crate::debug::log("player", &format!("audio: no file for episode {} in {}", e, dir));
        return false;
    };
    let path = p.to_string_lossy().to_string();
    let title = title_of(&path);
    if mpv.cmd(&["audio-add", &path, "select", &title, lang_from_name(&title)]).is_err() {
        return false;
    }
    let _ = mpv.set("audio-delay", "0");
    st.tracks.push(ExtAudio::new(sid, &path));
    save_ext_audio(data_dir, st);
    crate::debug::log("player", &format!("audio: {} found for episode {} in the series folder", path, e));
    true
}

impl Core {
    fn t_ui(&self, en: &'static str) -> String {
        crate::i18n::translate(&self.ui_lang(), en).unwrap_or(en).to_string()
    }

    /// The external track selected now: its mpv id and the saved record.
    fn selected_ext(&self) -> Option<(i64, ExtAudio)> {
        let m = self.mpv.as_ref()?;
        let s = self.session.as_ref()?;
        let aid = m.get_i64("aid")?;
        let t = m.tracks("audio").into_iter().find(|t| t.id == aid && t.external)?;
        let rec = self.ext_audio.tracks.iter().find(|r| r.sid == s.sid && same_file(&r.path, &t.file))?.clone();
        Some((aid, rec))
    }

    /// "Load an audio track…": adds `path` to the episode, selects it and lines it up with the video's audio.
    pub fn audio_add(&mut self, path: &str) {
        let path = path.trim().to_string();
        let (Some(m), Some(s)) = (self.mpv.clone(), self.session.as_ref()) else {
            self.toast("Open a video first.");
            return;
        };
        if path.is_empty() || !Path::new(&path).is_file() {
            self.toast("The audio file was not found.");
            return;
        }
        let (sid, show) = (s.sid.clone(), s.ep.show.clone());
        let title = title_of(&path);
        if let Err(e) = m.cmd(&["audio-add", &path, "select", &title, lang_from_name(&title)]) {
            crate::debug::log("player", &format!("audio: audio-add {} failed: {}", path, e));
            self.toast("The player could not open this audio file.");
            return;
        }
        let _ = m.set("audio-delay", "0");
        for t in self.ext_audio.tracks.iter_mut().filter(|t| t.sid == sid) {
            t.active = false;
        }
        self.ext_audio.tracks.retain(|t| !(t.sid == sid && same_file(&t.path, &path)));
        self.ext_audio.tracks.push(ExtAudio::new(&sid, &path));
        if let (false, Some(dir)) = (show.is_empty(), Path::new(&path).parent()) {
            self.ext_audio.folders.retain(|(s, _)| *s != show);
            self.ext_audio.folders.push((show, dir.to_string_lossy().to_string()));
        }
        save_ext_audio(&self.data_dir, &self.ext_audio);
        crate::debug::log("player", &format!("audio: loaded {}", path));
        self.events.push(Event::Session);
        self.audio_sync_start(true);
    }

    /// Picks audio track `id`: an external one gets its saved delay, the video's own tracks none.
    pub fn audio_select(&mut self, id: i64) {
        let Some(m) = self.mpv.clone() else { return };
        let _ = m.set("aid", &id.to_string());
        let Some(sid) = self.session.as_ref().map(|s| s.sid.clone()) else { return };
        let tracks = m.tracks("audio");
        let file = tracks.iter().find(|t| t.id == id && t.external).map(|t| t.file.clone());
        // its place among the tracks of its file (a dual-audio video brings several)
        let nth = file.as_deref().map(|f| tracks.iter().filter(|t| t.external && same_file(&t.file, f)).position(|t| t.id == id).unwrap_or(0) as i64 + 1).unwrap_or(1);
        let mut delay = 0.0;
        for t in self.ext_audio.tracks.iter_mut().filter(|t| t.sid == sid) {
            t.active = file.as_deref().map(|f| same_file(&t.path, f)).unwrap_or(false);
            if t.active {
                delay = t.delay;
                t.file_aid = nth;
            }
        }
        let _ = m.set("audio-delay", &format!("{:.3}", delay));
        save_ext_audio(&self.data_dir, &self.ext_audio);
    }

    pub fn audio_delay(&self) -> f64 {
        self.mpv.as_ref().and_then(|m| m.get_f64("audio-delay")).unwrap_or(0.0)
    }

    /// The audio delay by hand (+ = the sound comes later). Kept for the external track playing.
    pub fn set_audio_delay(&mut self, secs: f64) {
        let v = (secs.clamp(-600.0, 600.0) * 1000.0).round() / 1000.0;
        if let Some(m) = &self.mpv {
            let _ = m.set("audio-delay", &format!("{:.3}", v));
        }
        if let Some((_, rec)) = self.selected_ext() {
            if let Some(t) = self.ext_audio.tracks.iter_mut().find(|t| **t == rec) {
                t.delay = v;
                t.synced = false;
            }
            save_ext_audio(&self.data_dir, &self.ext_audio);
        }
    }

    /// Takes the selected external track out of the episode (and out of the player).
    pub fn audio_remove(&mut self) {
        let Some((aid, rec)) = self.selected_ext() else { return };
        if let Some(m) = &self.mpv {
            let _ = m.cmd(&["audio-remove", &aid.to_string()]);
            let _ = m.set("audio-delay", "0");
        }
        self.ext_audio.tracks.retain(|t| *t != rec);
        save_ext_audio(&self.data_dir, &self.ext_audio);
        self.audio_sync.note.clear();
        self.events.push(Event::Session);
    }

    /// Runs on each tick of a session: the sync asked for when a track was found by itself.
    pub(super) fn audio_step(&mut self) {
        let playing = self.session.as_ref().map(|s| s.stage == Stage::Playing).unwrap_or(false);
        if self.audio_sync.pending && playing && !self.audio_sync.busy {
            self.audio_sync.pending = false;
            self.audio_sync_start(true);
        }
        if !self.audio_sync.busy {
            self.dub_step();
        }
        // a downloaded subtitle: synced with the speech once per video, about 10 s after it starts (when chosen)
        if playing && self.settings.subtitles.auto_sync && !self.audio_sync.sub_busy && !self.audio_sync.busy {
            let ready = self.session.as_ref().and_then(|s| {
                let done = matches!(s.sub_job, SubJob::Done(Some(_)));
                let settled = s.loaded_at.map(|t| t.elapsed().as_secs() >= 10).unwrap_or(false);
                (done && settled && s.token != self.audio_sync.sub_auto_token).then_some(s.token)
            });
            if let Some(token) = ready {
                self.audio_sync.sub_auto_token = token;
                self.sub_sync_start(true);
            }
        }
    }

    /// Lines the selected external track up with the video's own audio, around the place playing now. `auto` = asked
    /// by loading a track (quiet when there is nothing to compare with).
    pub fn audio_sync_start(&mut self, auto: bool) {
        if self.audio_sync.busy {
            return;
        }
        let Some((_, rec)) = self.selected_ext() else {
            if !auto {
                self.toast("Pick an audio track loaded from a file first.");
            }
            return;
        };
        self.audio_sync_rec(rec, auto);
    }

    /// Lines the track `rec` up with the video's own audio (see `audio_sync_start`).
    pub(super) fn audio_sync_rec(&mut self, rec: ExtAudio, auto: bool) {
        if self.audio_sync.busy {
            return;
        }
        let (Some(m), Some(s)) = (self.mpv.clone(), self.session.as_ref()) else { return };
        let tracks = m.tracks("audio");
        let Some(own) = tracks.iter().find(|t| !t.external) else {
            self.audio_sync.note = "This video has no audio of its own to compare with.".into();
            if !auto {
                self.toast("This video has no audio of its own to compare with.");
            }
            return;
        };
        let url = m.get_str("path").unwrap_or_default();
        if url.is_empty() {
            return;
        }
        let headers = s.streams.get(s.cur).map(|x| x.headers.clone()).unwrap_or_default();
        let token = s.token;
        let pos = m.get_f64("time-pos").unwrap_or(0.0).max(0.0);
        let dur = m.get_f64("duration").unwrap_or(0.0);
        // 45 s of the video from here (not the very end), compared with the dub from 60 s before to 60 s after
        const LEN: f64 = 45.0;
        const SEARCH: f64 = 60.0;
        let vid_start = if dur > LEN + 5.0 { pos.min(dur - LEN - 2.0).max(0.0) } else { 0.0 };
        let vid_end = if dur > 0.0 { (vid_start + LEN).min(dur) } else { vid_start + LEN };
        let dub_start = (vid_start - SEARCH).max(0.0);
        let dub_end = vid_end + SEARCH;
        let own_id = own.id.to_string();
        let libs = self.rt.libmpv_candidates();
        let work = self.data_dir.join("cache").join("audiosync");
        let _ = std::fs::create_dir_all(&work);
        let dub_path = rec.path.clone();
        let dub_aid = rec.file_aid.max(1).to_string();
        self.audio_sync.busy = true;
        self.audio_sync.note.clear();
        self.toast("Syncing the audio with the original…");
        self.events.push(Event::Session);
        crate::debug::log("player", &format!("audio: sync of {} (video {:.1}-{:.1} s, dub {:.1}-{:.1} s, own track {})", dub_path, vid_start, vid_end, dub_start, dub_end, own_id));
        self.spawn(move |tx, _| {
            let t0 = Instant::now();
            let r = (|| -> Result<audiosync::Match, String> {
                let vf = work.join("video.pcm").to_string_lossy().to_string();
                let df = work.join("dub.pcm").to_string_lossy().to_string();
                let vk = Mpv::encode_pcm(&libs, &url, &headers, &own_id, vid_start, vid_end, &vf, 180).map_err(|e| format!("the video's audio could not be read ({})", e))?;
                let dk = Mpv::encode_pcm(&libs, &dub_path, &[], &dub_aid, dub_start, dub_end, &df, 180).map_err(|e| format!("the audio file could not be read ({})", e))?;
                let vid = audiosync::read_pcm(&std::fs::read(&vf).map_err(|e| e.to_string())?, vk);
                let dub = audiosync::read_pcm(&std::fs::read(&df).map_err(|e| e.to_string())?, dk);
                let _ = std::fs::remove_file(&vf);
                let _ = std::fs::remove_file(&df);
                // the dub may have started later than asked when it is shorter than the window before
                audiosync::find(&vid, vid_start, &dub, dub_start).ok_or_else(|| "too little sound to compare".to_string())
            })();
            crate::debug::log("player", &format!("audio: sync took {} ms: {:?}", t0.elapsed().as_millis(), r));
            send(tx, Msg::AudioSync(token, dub_path, r));
        });
    }

    pub(super) fn on_audio_sync(&mut self, token: u64, path: String, r: Result<audiosync::Match, String>) {
        self.audio_sync.busy = false;
        self.events.push(Event::Session);
        let Some(sid) = self.session.as_ref().filter(|s| s.token == token).map(|s| s.sid.clone()) else { return };
        // a dub found in another torrent is selected only once it is in time
        let select = self.audio_sync.select_after.take().filter(|p| same_file(p, &path));
        if select.is_some() && !matches!(&r, Ok(m) if m.quality() != "unsure") {
            self.audio_sync.note = "A dub was found in another torrent but could not be synced. Pick it in this menu and adjust the delay.".into();
            self.toast("A dub was found in another torrent but could not be synced. Pick it in this menu and adjust the delay.");
            return;
        }
        let m = match r {
            Ok(m) => m,
            Err(e) => {
                self.audio_sync.note = "The audio could not be synced on its own. Adjust the delay by hand.".into();
                let msg = format!("{} ({})", self.t_ui("The audio could not be synced on its own. Adjust the delay by hand."), e);
                self.toast(&msg);
                return;
            }
        };
        if m.quality() == "unsure" {
            self.audio_sync.note = "The audio could not be synced on its own. Adjust the delay by hand.".into();
            self.toast("The audio could not be synced on its own. Adjust the delay by hand.");
            return;
        }
        let delay = (audiosync::delay_for(m.offset) * 1000.0).round() / 1000.0;
        let mut active = false;
        for t in self.ext_audio.tracks.iter_mut().filter(|t| t.sid == sid && same_file(&t.path, &path)) {
            t.delay = delay;
            t.synced = true;
            active = t.active;
        }
        save_ext_audio(&self.data_dir, &self.ext_audio);
        if select.is_some() {
            let nth = self.ext_audio.tracks.iter().find(|t| t.sid == sid && same_file(&t.path, &path)).map(|t| t.file_aid).unwrap_or(1);
            if let Some(id) = self.mpv.as_ref().and_then(|mp| file_track_id(mp, &path, nth)) {
                self.audio_select(id);
            }
        } else if active {
            if let Some(mp) = &self.mpv {
                let _ = mp.set("audio-delay", &format!("{:.3}", delay));
            }
        }
        let head = if m.quality() == "good" { "Audio synced:" } else { "Audio probably synced (check it):" };
        self.audio_sync.note = head.to_string();
        let msg = format!("{} {}{:.2} s", self.t_ui(head), if delay >= 0.0 { "+" } else { "" }, delay);
        self.toast(&msg);
    }
}

impl Core {
    /// Lines the subtitle on screen (a file: downloaded or loaded by hand) up with the speech of the video, around the
    /// place playing now, and sets mpv's `sub-delay`. `auto` = asked by itself once the video plays (quiet on failure).
    pub fn sub_sync_start(&mut self, auto: bool) {
        if self.audio_sync.sub_busy {
            return;
        }
        let (Some(m), Some(s)) = (self.mpv.clone(), self.session.as_ref()) else { return };
        let say = |c: &mut Core, msg: &str| {
            c.audio_sync.sub_note = msg.to_string();
            if !auto {
                c.toast(msg);
            }
            c.events.push(Event::Session);
        };
        let Some(sub) = m.tracks("sub").into_iter().find(|t| t.selected) else {
            say(self, "Turn a subtitle on first.");
            return;
        };
        if !sub.external || sub.file.is_empty() {
            say(self, "Only a subtitle from a file (downloaded or loaded) can be synced with the speech.");
            return;
        }
        let text = std::fs::read(&sub.file).map(|b| crate::subtitles::decode(&b)).unwrap_or_default();
        let cues: Vec<(f64, f64)> =
            crate::subtitles::parse_cues(&text).iter().map(|c| (super::studyplay::secs(&c.start), super::studyplay::secs(&c.end))).collect();
        if cues.len() < 6 {
            say(self, "This subtitle has too few lines to sync.");
            return;
        }
        let audio = m.tracks("audio");
        let Some(own) = audio.iter().find(|t| t.selected && !t.external).or_else(|| audio.iter().find(|t| !t.external)) else {
            say(self, "This video has no audio of its own to compare with.");
            return;
        };
        let url = m.get_str("path").unwrap_or_default();
        if url.is_empty() {
            return;
        }
        let headers = s.streams.get(s.cur).map(|x| x.headers.clone()).unwrap_or_default();
        let token = s.token;
        let pos = m.get_f64("time-pos").unwrap_or(0.0).max(0.0);
        let dur = m.get_f64("duration").unwrap_or(0.0);
        // 150 s of sound from a little before here; the lines of its middle are slid up to 30 s each way
        const LEN: f64 = 150.0;
        const SHIFT: f64 = 30.0;
        let start = if dur > LEN + 5.0 { (pos - 10.0).max(0.0).min(dur - LEN - 2.0) } else { 0.0 };
        let end = if dur > 0.0 { (start + LEN).min(dur) } else { start + LEN };
        let own_id = own.id.to_string();
        let libs = self.rt.libmpv_candidates();
        let work = self.data_dir.join("cache").join("subsync");
        let _ = std::fs::create_dir_all(&work);
        self.audio_sync.sub_busy = true;
        self.audio_sync.sub_note.clear();
        if !auto {
            self.toast("Syncing the subtitle with the speech…");
        }
        self.events.push(Event::Session);
        crate::debug::log("player", &format!("subsync: {} lines, sound {:.1}-{:.1} s, track {}", cues.len(), start, end, own_id));
        self.spawn(move |tx, _| {
            let t0 = Instant::now();
            let r = (|| -> Result<crate::subsync::Match, String> {
                let f = work.join("speech.pcm").to_string_lossy().to_string();
                let kind = Mpv::encode_pcm(&libs, &url, &headers, &own_id, start, end, &f, 240).map_err(|e| format!("the video's audio could not be read ({})", e))?;
                let pcm = audiosync::read_pcm(&std::fs::read(&f).map_err(|e| e.to_string())?, kind);
                let _ = std::fs::remove_file(&f);
                let voice = crate::subsync::voice_frames(&pcm);
                crate::subsync::find(&voice, start, &cues, SHIFT).ok_or_else(|| "too little speech or too few lines here".to_string())
            })();
            crate::debug::log("player", &format!("subsync: took {} ms: {:?}", t0.elapsed().as_millis(), r));
            send(tx, Msg::SubSync(token, auto, r));
        });
    }

    pub(super) fn on_sub_sync(&mut self, token: u64, auto: bool, r: Result<crate::subsync::Match, String>) {
        self.audio_sync.sub_busy = false;
        self.events.push(Event::Session);
        if !self.session.as_ref().map(|s| s.token == token).unwrap_or(false) {
            return;
        }
        let m = match r {
            Ok(m) if m.quality() != "unsure" => m,
            other => {
                self.audio_sync.sub_note = "The subtitle could not be synced on its own. Adjust the delay by hand.".into();
                if !auto {
                    let why = match other {
                        Err(e) => format!(" ({})", e),
                        _ => String::new(),
                    };
                    let msg = format!("{}{}", self.t_ui("The subtitle could not be synced on its own. Adjust the delay by hand."), why);
                    self.toast(&msg);
                }
                return;
            }
        };
        let delay = (m.delay * 100.0).round() / 100.0;
        // an automatic run leaves a delay the user already set alone, unless it is clearly off
        if auto && (self.sub_delay() - delay).abs() < 0.15 {
            return;
        }
        self.set_sub_delay(delay);
        let head = if m.quality() == "good" { "Subtitle synced:" } else { "Subtitle probably synced (check it):" };
        self.audio_sync.sub_note = head.to_string();
        let msg = format!("{} {}{:.2} s", self.t_ui(head), if delay >= 0.0 { "+" } else { "" }, delay);
        self.toast(&msg);
    }
}

/// mpv's id of the `nth` (1 = first) audio track that came from file `path`.
pub(super) fn file_track_id(mpv: &Mpv, path: &str, nth: i64) -> Option<i64> {
    mpv.tracks("audio").into_iter().filter(|t| t.external && same_file(&t.file, path)).nth((nth.max(1) - 1) as usize).map(|t| t.id)
}

/// Two names of the same file (mpv may report it with other slashes).
pub(super) fn same_file(a: &str, b: &str) -> bool {
    let n = |s: &str| s.replace('\\', "/").trim_start_matches("file://").to_string();
    n(a) == n(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_numbers_in_names() {
        assert_eq!(episode_fit("Pokemon S06E04 PT-BR", Some(6), 4), 2);
        assert_eq!(episode_fit("pokemon.s6e4.dublado", Some(6), 4), 2);
        assert_eq!(episode_fit("Pokemon S06E05 PT-BR", Some(6), 4), 0);
        assert_eq!(episode_fit("[Grupo] Pokemon - 04 [1080p][x264][AAC 2.0]", Some(6), 4), 1);
        assert_eq!(episode_fit("[Grupo] Pokemon - 05 [1080p]", Some(6), 4), 0);
        assert_eq!(episode_fit("Pokemon 1080p", None, 1080), 0);
    }

    #[test]
    fn picks_one_file_per_episode() {
        let dir = std::env::temp_dir().join(format!("rh-extaudio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["Show S01E01 PTBR.mka", "Show S01E02 PTBR.mka", "Show S01E02.mkv", "notes.txt", "Show - 03.mp3", "Show - 03 (2).mp3"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let video = dir.join("Show S01E02.mkv").to_string_lossy().to_string();
        assert_eq!(find_episode_audio(&dir, Some(1), 2, &video).unwrap().file_name().unwrap(), "Show S01E02 PTBR.mka");
        assert_eq!(find_episode_audio(&dir, Some(1), 1, "").unwrap().file_name().unwrap(), "Show S01E01 PTBR.mka");
        // two files fit equally: none is guessed
        assert!(find_episode_audio(&dir, Some(1), 3, "").is_none());
        assert!(find_episode_audio(&dir, Some(1), 9, "").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn languages_in_names() {
        assert_eq!(lang_from_name("Pokemon S06E03 PT-BR"), "por");
        assert_eq!(lang_from_name("ep03 dublado"), "por");
        assert_eq!(lang_from_name("ep03 latino"), "spa");
        assert_eq!(lang_from_name("ep03"), "");
        assert_eq!(lang_from_name("report"), "");
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("rh-extaudio-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.mka");
        std::fs::write(&file, b"x").unwrap();
        let st = ExtAudioStore {
            tracks: vec![
                ExtAudio { delay: -1.25, synced: true, file_aid: 2, ..ExtAudio::new("tt1:6:3", &file.to_string_lossy()) },
                ExtAudio { remote: true, ..ExtAudio::new("tt1:6:3", "http://127.0.0.1:3030/torrents/4/stream/0") },
            ],
            folders: vec![("tt1".into(), dir.to_string_lossy().to_string())],
            no_auto_dub: true,
        };
        save_ext_audio(&dir, &st);
        let back = load_ext_audio(&dir);
        // the track streamed from another torrent is not kept
        assert_eq!(back.tracks, st.tracks[..1].to_vec());
        assert!(back.no_auto_dub);
        assert_eq!(back.folders, st.folders);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
