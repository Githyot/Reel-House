//! Where each episode (or movie) stopped, and the audio / subtitle chosen in it: `resume.json` in the profile.
//!
//! "Continue watching" (`progress.json`) keeps one entry per title, so opening an episode from its list (or any
//! episode other than the last one) used to start at 0:00. This store is per episode (the session's `sid`, the
//! same for every torrent of that episode), so any way of opening the episode starts where it stopped.
//!
//! Tracks are remembered by language and title (track numbers change between torrents). When the file loads, the
//! saved audio is picked again (also when a dub from another torrent shows up later), and the saved subtitle (or
//! "off") during the first two minutes, while the downloaded subtitles are still being added. A track the user
//! picks by hand wins at once. An episode never opened before takes the tracks of the last episode of the title.

use super::*;
use crate::json::Json;
use crate::mpv::{Mpv, Track};

/// The subtitle chosen last time is put back during this time after the file loads (downloaded ones come late).
const SUB_WINDOW: Duration = Duration::from_secs(120);
/// Episodes kept in resume.json.
const KEEP: usize = 600;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackPref {
    /// Subtitles turned off.
    pub off: bool,
    pub lang: String,
    pub title: String,
    /// A file added to the video (a downloaded subtitle, a dub).
    pub external: bool,
}

impl TrackPref {
    fn of(t: &Track) -> TrackPref {
        TrackPref { off: false, lang: t.lang.clone(), title: t.title.clone(), external: t.external }
    }

    fn to_json(&self) -> Json {
        Json::obj(vec![("off", Json::Bool(self.off)), ("lang", self.lang.as_str().into()), ("title", self.title.as_str().into()), ("external", Json::Bool(self.external))])
    }

    fn from_json(j: &Json) -> Option<TrackPref> {
        if j.get("off").as_bool().is_none() && j.get("lang").as_str().is_none() {
            return None;
        }
        Some(TrackPref { off: j.get("off").as_bool().unwrap_or(false), lang: j.get("lang").str(), title: j.get("title").str(), external: j.get("external").as_bool().unwrap_or(false) })
    }
}

/// The track of `tracks` that best fits `pref`: same title and language, then same language (a file added or not
/// as before), then same title. None when nothing fits (a language this torrent does not have).
pub fn pick_track(tracks: &[Track], pref: &TrackPref) -> Option<i64> {
    if pref.off {
        return None;
    }
    let lang = util::lang_base(&pref.lang);
    let same_lang = |t: &Track| !lang.is_empty() && util::lang_base(&t.lang) == lang;
    let same_title = |t: &Track| !pref.title.is_empty() && t.title.trim().eq_ignore_ascii_case(pref.title.trim());
    let score = |t: &Track| -> u8 {
        match (same_lang(t), same_title(t), t.external == pref.external) {
            (true, true, true) => 6,
            (true, true, false) => 5,
            (true, false, true) => 4,
            (true, false, false) => 3,
            (false, true, _) if lang.is_empty() || t.lang.is_empty() => 2,
            _ => 0,
        }
    };
    let best = tracks.iter().map(|t| (score(t), t)).filter(|(s, _)| *s > 0).max_by_key(|(s, t)| (*s, t.selected))?;
    Some(best.1.id)
}

#[derive(Clone, Debug, Default)]
pub struct EpResume {
    pub meta_id: String,
    pub video_id: String,
    pub position: f64,
    pub duration: f64,
    pub audio: Option<TrackPref>,
    pub sub: Option<TrackPref>,
    pub updated: u64,
}

pub fn load_resume(dir: &Path) -> Vec<EpResume> {
    let Some(j) = std::fs::read_to_string(dir.join("resume.json")).ok().and_then(|t| Json::parse(&t).ok()) else { return vec![] };
    let mut v: Vec<EpResume> = j
        .items()
        .iter()
        .map(|e| EpResume {
            meta_id: e.get("meta_id").str(),
            video_id: e.get("video_id").str(),
            position: e.get("position").as_f64().unwrap_or(0.0),
            duration: e.get("duration").as_f64().unwrap_or(0.0),
            audio: TrackPref::from_json(e.get("audio")),
            sub: TrackPref::from_json(e.get("sub")),
            updated: e.get("updated").as_u64().unwrap_or(0),
        })
        .filter(|p| !p.video_id.is_empty())
        .collect();
    v.sort_by_key(|p| std::cmp::Reverse(p.updated));
    v
}

/// What the player is putting back in the video playing.
#[derive(Debug)]
pub struct TrackRestore {
    /// Session id and file load it belongs to (switching torrent loads a new file in the same session).
    session: u64,
    loaded: Option<Instant>,
    audio: Option<TrackPref>,
    sub: Option<TrackPref>,
    audio_hand: bool,
    sub_hand: bool,
    checked: Instant,
}

impl Core {
    pub(super) fn save_resume_file(&self) {
        let items: Vec<Json> = self
            .ep_resume
            .iter()
            .map(|p| {
                Json::obj(vec![
                    ("meta_id", p.meta_id.as_str().into()),
                    ("video_id", p.video_id.as_str().into()),
                    ("position", p.position.into()),
                    ("duration", p.duration.into()),
                    ("audio", p.audio.as_ref().map(|t| t.to_json()).unwrap_or(Json::Null)),
                    ("sub", p.sub.as_ref().map(|t| t.to_json()).unwrap_or(Json::Null)),
                    ("updated", (p.updated as f64).into()),
                ])
            })
            .collect();
        let _ = std::fs::write(self.profile_dir.join("resume.json"), Json::Arr(items).pretty());
    }

    /// Where episode `video_id` stopped (None when it was not started, or was watched to the end).
    pub(super) fn resume_point(&self, video_id: &str) -> Option<f64> {
        if video_id.is_empty() {
            return None;
        }
        let p = self.ep_resume.iter().find(|p| p.video_id == video_id)?;
        (p.position >= 20.0 && (p.duration <= 0.0 || p.position / p.duration < 0.92)).then_some(p.position)
    }

    /// Keeps where the episode playing is and its tracks. `finished`: watched to the end (next time from 0:00, the
    /// tracks are kept for the next episodes).
    pub(super) fn remember_episode(&mut self, meta_id: &str, video_id: &str, pos: f64, dur: f64, finished: bool) {
        if video_id.is_empty() {
            return;
        }
        let (audio, sub) = self.tracks_to_keep(video_id);
        let i = self.ep_resume.iter().position(|p| p.video_id == video_id);
        let mut e = i.map(|i| self.ep_resume.remove(i)).unwrap_or_default();
        e.meta_id = meta_id.to_string();
        e.video_id = video_id.to_string();
        e.position = if finished { 0.0 } else { pos };
        e.duration = dur;
        if audio.is_some() {
            e.audio = audio;
        }
        if sub.is_some() {
            e.sub = sub;
        }
        e.updated = now_secs();
        self.ep_resume.insert(0, e);
        self.ep_resume.truncate(KEEP);
        self.save_resume_file();
    }

    /// Forgets where the episode stopped (marked as watched / removed from Continue watching); keeps its tracks.
    pub(super) fn forget_position(&mut self, pred: impl Fn(&EpResume) -> bool) {
        let mut changed = false;
        for p in self.ep_resume.iter_mut().filter(|p| pred(p) && p.position > 0.0) {
            p.position = 0.0;
            changed = true;
        }
        if changed {
            self.save_resume_file();
        }
    }

    /// The tracks to save for the episode playing: what is selected now, except a saved choice that is still being
    /// put back (its subtitle not downloaded yet, the dub not found yet).
    fn tracks_to_keep(&self, video_id: &str) -> (Option<TrackPref>, Option<TrackPref>) {
        let Some(m) = self.mpv.as_ref() else { return (None, None) };
        let Some(s) = self.session.as_ref().filter(|s| s.sid == video_id && s.file_loaded) else { return (None, None) };
        let r = self.track_restore.as_ref().filter(|r| r.session == s.id);
        let audio_tracks = m.tracks("audio");
        let sub_tracks = m.tracks("sub");
        let now_audio = audio_tracks.iter().find(|t| t.selected).map(TrackPref::of);
        let now_sub = Some(sub_tracks.iter().find(|t| t.selected).map(TrackPref::of).unwrap_or(TrackPref { off: true, ..Default::default() }));
        let audio = match r {
            Some(r) if !r.audio_hand => match &r.audio {
                Some(p) if pick_track(&audio_tracks, p).map(|id| !audio_tracks.iter().any(|t| t.id == id && t.selected)).unwrap_or(true) => Some(p.clone()),
                _ => now_audio,
            },
            _ => now_audio,
        };
        let sub = match r {
            Some(r) if !r.sub_hand && r.loaded.map(|t| t.elapsed() < SUB_WINDOW).unwrap_or(true) => match &r.sub {
                Some(p) if !p.off && pick_track(&sub_tracks, p).map(|id| !sub_tracks.iter().any(|t| t.id == id && t.selected)).unwrap_or(true) => Some(p.clone()),
                _ => now_sub,
            },
            _ => now_sub,
        };
        (audio, sub)
    }

    /// A command from the player's menus that picks a track: from now on the user's choice is not overridden.
    pub fn tracks_by_hand(&mut self, cmd: &str) {
        let Some(r) = self.track_restore.as_mut() else { return };
        match cmd {
            "set_audio" | "audio_add" | "audio_remove" | "dub_try" => r.audio_hand = true,
            "set_sub" | "load_sub_cand" => r.sub_hand = true,
            _ => {}
        }
    }

    /// Runs on each tick while a video plays: puts back the audio and subtitle chosen last time.
    pub(super) fn restore_tracks_step(&mut self, mpv: &Mpv) {
        let Some(s) = self.session.as_ref() else { return };
        let (id, sid, loaded) = (s.id, s.sid.clone(), s.loaded_at);
        let Some(loaded_at) = loaded else { return };
        // the video's own tracks are known a moment after it loads
        if loaded_at.elapsed() < Duration::from_millis(1500) {
            return;
        }
        let fresh = match &self.track_restore {
            Some(r) if r.session == id && r.loaded == loaded => false,
            Some(r) if r.session == id => {
                // another torrent of the same episode: what was chosen in the last one is put back in this one
                let (a, b) = (r.audio.clone(), r.sub.clone());
                self.track_restore = Some(TrackRestore { session: id, loaded, audio: a, sub: b, audio_hand: false, sub_hand: false, checked: Instant::now() });
                true
            }
            _ => {
                let meta_id = s.meta.as_ref().map(|m| m.id.clone()).unwrap_or_default();
                // this episode's tracks, otherwise those of the last episode of the title
                let saved = self
                    .ep_resume
                    .iter()
                    .find(|p| p.video_id == sid && !sid.is_empty())
                    .or_else(|| self.ep_resume.iter().find(|p| !meta_id.is_empty() && p.meta_id == meta_id && (p.audio.is_some() || p.sub.is_some())));
                let (audio, sub) = saved.map(|p| (p.audio.clone(), p.sub.clone())).unwrap_or((None, None));
                crate::debug::log("player", &format!("resume: tracks to put back: audio {:?}, subtitle {:?}", audio, sub));
                self.track_restore = Some(TrackRestore { session: id, loaded, audio, sub, audio_hand: false, sub_hand: false, checked: Instant::now() });
                true
            }
        };
        let Some(r) = self.track_restore.as_mut() else { return };
        if !fresh && r.checked.elapsed() < Duration::from_secs(1) {
            return;
        }
        r.checked = Instant::now();
        let audio_tracks = mpv.tracks("audio");
        let sub_tracks = mpv.tracks("sub");
        // a choice made by hand is the one to put back if another torrent of the episode is opened
        if r.audio_hand {
            if let Some(t) = audio_tracks.iter().find(|t| t.selected) {
                r.audio = Some(TrackPref::of(t));
            }
        }
        if r.sub_hand {
            r.sub = Some(sub_tracks.iter().find(|t| t.selected).map(TrackPref::of).unwrap_or(TrackPref { off: true, ..Default::default() }));
        }
        let mut pick_audio = None;
        if let (false, Some(p)) = (r.audio_hand, r.audio.as_ref()) {
            if let Some(want) = pick_track(&audio_tracks, p) {
                if !audio_tracks.iter().any(|t| t.id == want && t.selected) {
                    pick_audio = Some(want);
                }
            }
        }
        if let (false, Some(p), true) = (r.sub_hand, r.sub.as_ref(), loaded_at.elapsed() < SUB_WINDOW) {
            if p.off {
                if sub_tracks.iter().any(|t| t.selected) {
                    crate::debug::log("player", "resume: subtitles were off last time");
                    let _ = mpv.set("sid", "no");
                }
            } else if let Some(want) = pick_track(&sub_tracks, p) {
                if !sub_tracks.iter().any(|t| t.id == want && t.selected) {
                    crate::debug::log("player", &format!("resume: subtitle {} ({} {}) as last time", want, p.lang, p.title));
                    let _ = mpv.set("sid", &want.to_string());
                    self.events.push(Event::Session);
                }
            }
        }
        if let Some(want) = pick_audio {
            crate::debug::log("player", &format!("resume: audio {} as last time", want));
            // through audio_select: a loaded dub gets its saved delay
            self.audio_select(want);
            self.events.push(Event::Session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: i64, lang: &str, title: &str, external: bool) -> Track {
        Track { id, lang: lang.into(), title: title.into(), selected: false, external, file: String::new() }
    }

    #[test]
    fn picks_the_same_language_in_another_torrent() {
        let tracks = vec![t(1, "jpn", "Japanese", false), t(2, "por", "Português (Brasil)", false), t(3, "eng", "English", false)];
        let pref = TrackPref { off: false, lang: "pt-BR".into(), title: "PT-BR".into(), external: false };
        assert_eq!(pick_track(&tracks, &pref), Some(2));
    }

    #[test]
    fn title_breaks_a_tie_and_off_picks_nothing() {
        let tracks = vec![t(1, "eng", "Signs", false), t(2, "eng", "Full", false), t(3, "eng", "Full", true)];
        assert_eq!(pick_track(&tracks, &TrackPref { off: false, lang: "en".into(), title: "full".into(), external: true }), Some(3));
        assert_eq!(pick_track(&tracks, &TrackPref { off: false, lang: "en".into(), title: "Full".into(), external: false }), Some(2));
        assert_eq!(pick_track(&tracks, &TrackPref { off: true, ..Default::default() }), None);
        assert_eq!(pick_track(&tracks, &TrackPref { off: false, lang: "ja".into(), title: String::new(), external: false }), None);
    }

    #[test]
    fn prefs_survive_json() {
        let p = TrackPref { off: false, lang: "ja".into(), title: "x".into(), external: true };
        assert_eq!(TrackPref::from_json(&p.to_json()), Some(p));
        assert_eq!(TrackPref::from_json(&Json::Null), None);
    }
}
