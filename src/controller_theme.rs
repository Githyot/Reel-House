//! The opening / ending of the player: while the opening or ending of an anime plays (Anime Skip / AniSkip
//! segments), its song is looked up (MyAnimeList through Jikan) and found on YouTube (yt-dlp), and a card offers it.
//! The video plays inside the X-Ray panel (not in a window of its own): a second mpv draws small pictures into memory
//! (`ThemePlayer`), the interface shows them (`rh_theme_frame`) and its sound plays while the episode is paused.
//! The songs of the X-Ray's Music list (and the trailers, listed apart under "All") play the same way. Part of
//! `controller`.
use super::*;
use crate::animeskip::Kind;
use crate::themesong::{self, Theme, Video};

/// Per episode: what was asked, what was found, what the viewer closed.
#[derive(Default)]
pub struct ThemeState {
    pub asked: Vec<String>,
    pub found: Vec<(String, Result<Video, String>)>,
    pub dismissed: Vec<String>,
}

/// The video player of the X-Ray (one for the whole run, reused for every song).
#[derive(Default)]
pub struct ThemePlayer {
    pub mpv: Option<Arc<Mpv>>,
    /// mpv has a new picture the interface has not taken yet (set from mpv's thread).
    pub fresh: Arc<AtomicBool>,
    /// "" (nothing), "loading", "playing" or "error"
    pub status: String,
    pub error: String,
    /// The song (or trailer) and its YouTube page.
    pub video: Option<Video>,
    /// "Opening 2", "Ending", "Trailer"...
    pub label: String,
    /// Counts the requests: the answer of an older one (the viewer clicked another song meanwhile) is dropped.
    pub req: u64,
    /// The episode was playing when the video started: it plays again when the video is closed.
    pub main_was_playing: bool,
    /// Volume and mute of this player (the episode keeps its own).
    pub volume: Option<f64>,
    pub muted: bool,
}

/// The YouTube video id of a watch / short address ("" when it is not one).
pub fn youtube_id(url: &str) -> String {
    let after = |key: &str| url.split_once(key).map(|(_, r)| r.split(['&', '?', '#', '/']).next().unwrap_or("").to_string());
    after("watch?v=").or_else(|| after("youtu.be/")).or_else(|| after("/shorts/")).unwrap_or_default()
}

/// "Opening 2" / "Ending" for a song.
fn label_of(kind: &str, number: u32) -> String {
    let base = match kind {
        "op" => "Opening",
        "ed" => "Ending",
        _ => "Video",
    };
    if number > 0 { format!("{} {}", base, number) } else { base.to_string() }
}

impl Core {
    /// "op" / "ed" while the opening / ending plays.
    pub fn theme_now(&self) -> Option<&'static str> {
        let s = self.session.as_ref()?;
        if s.stage != Stage::Playing || s.segments.is_empty() {
            return None;
        }
        let m = self.mpv.as_ref()?;
        let pos = m.get_f64("time-pos")?;
        let dur = m.get_f64("duration").unwrap_or(0.0);
        // only while it plays
        let seg = s.segments.iter().find(|x| matches!(x.kind, Kind::Intro | Kind::Credits) && pos >= x.start - 0.25 && pos < x.end.unwrap_or(if dur > 0.0 { dur } else { f64::MAX }))?;
        Some(if matches!(seg.kind, Kind::Intro) { "op" } else { "ed" })
    }

    /// Called with the player's tick: starts the lookup the first time the opening / ending plays, and reads what
    /// the video player of the X-Ray reported.
    pub(super) fn theme_step(&mut self) {
        self.theme_video_step();
        let Some(kind) = self.theme_now() else { return };
        let Some(s) = self.session.as_mut() else { return };
        if s.theme.asked.iter().any(|k| k == kind) {
            return;
        }
        s.theme.asked.push(kind.to_string());
        // the session id (stays the same when the quality is switched), as `asked` is not reset then
        let token = s.id;
        let show = if s.ep.show.is_empty() { s.title.clone() } else { s.ep.show.clone() };
        let (season, ep) = (s.ep.season, s.ep.episode);
        let mal_direct = s.meta.as_ref().and_then(|m| m.id.strip_prefix("mal:").and_then(|x| x.parse::<u64>().ok()));
        let data = self.data_dir.clone();
        let kind = kind.to_string();
        self.events.push(Event::Session);
        self.spawn(move |tx, http| {
            let r = (|| -> Result<Video, String> {
                let mal = match mal_direct {
                    Some(m) => m,
                    None => crate::skipdb::find_mal_id(&*http, &show, season)?.ok_or_else(|| "The anime was not found on MyAnimeList.".to_string())?,
                };
                let list = themesong::themes(&*http, mal)?;
                let theme = themesong::pick(&list, &kind, ep).cloned().ok_or_else(|| format!("MyAnimeList lists no {} for this anime.", if kind == "op" { "opening" } else { "ending" }))?;
                // the same song for the whole season: kept, so a later episode needs no search
                let key = themesong::cache_key(mal, &theme);
                if let Some(mut v) = themesong::cache_get(&data, &key) {
                    v.theme = theme.clone();
                    return Ok(v);
                }
                let ytdlp = themesong::ensure_ytdlp(&*http, &data)?;
                let v = themesong::find_video(&ytdlp, &show, &theme)?;
                themesong::cache_put(&data, &key, &v);
                Ok(v)
            })();
            if let Err(e) = &r {
                crate::debug::log("theme", &format!("{} of {}: {}", kind, show, e));
            }
            send(tx, Msg::ThemeFound(token, kind, r));
        });
    }

    pub(super) fn on_theme_found(&mut self, token: u64, kind: String, r: Result<Video, String>) {
        if let Some(s) = self.session.as_mut().filter(|s| s.id == token) {
            s.theme.found.retain(|(k, _)| k != &kind);
            s.theme.found.push((kind, r));
            self.events.push(Event::Session);
        }
    }

    /// The card to show now: (kind, the video or why there is none; None while it is being looked up).
    pub fn theme_card(&self) -> Option<(&'static str, Option<&Result<Video, String>>)> {
        let kind = self.theme_now()?;
        let s = self.session.as_ref()?;
        if s.theme.dismissed.iter().any(|k| k == kind) {
            return None;
        }
        Some((kind, s.theme.found.iter().find(|(k, _)| k == kind).map(|(_, r)| r)))
    }

    pub fn theme_dismiss(&mut self) {
        if let Some(kind) = self.theme_now() {
            if let Some(s) = self.session.as_mut() {
                s.theme.dismissed.push(kind.to_string());
            }
        }
        self.events.push(Event::Session);
    }

    /// Pauses the episode and plays a video in the X-Ray. `j` may give:
    /// - `url`: a YouTube (or any yt-dlp) page, e.g. a trailer or the video of the card;
    /// - `kind` ("op"/"ed") with `song`, `artist`, `number`: a song of the Music list, searched on YouTube;
    /// - nothing: the opening / ending playing now (the card).
    pub fn theme_play(&mut self, j: &Json) {
        let mut kind = j.get("kind").str();
        if kind.is_empty() {
            kind = self.theme_now().unwrap_or("").to_string();
        }
        let url = j.get("url").str();
        let song = j.get("song").str();
        let number = j.get("number").as_f64().unwrap_or(0.0).max(0.0) as u32;
        // the video already found for the card, when it is that song
        let found = self
            .session
            .as_ref()
            .and_then(|s| s.theme.found.iter().find(|(k, _)| *k == kind))
            .and_then(|(_, r)| r.as_ref().ok())
            .filter(|v| (song.is_empty() || v.theme.song == song) && (url.is_empty() || v.url() == url))
            .cloned();
        let theme = match &found {
            Some(v) => v.theme.clone(),
            None => Theme { kind: kind.clone(), number, song: song.clone(), artist: j.get("artist").str(), eps: (0, 0) },
        };
        let known: Option<Video> = found.or_else(|| {
            (!url.is_empty()).then(|| Video { theme: theme.clone(), video_id: youtube_id(&url), title: j.get("title").str(), channel: String::new(), duration: 0.0 })
        });
        if known.is_none() && theme.song.is_empty() {
            self.toast("Nothing to play.");
            return;
        }
        let page = if url.is_empty() { known.as_ref().map(|v| v.url()).unwrap_or_default() } else { url };
        let show = self.session.as_ref().map(|s| if s.ep.show.is_empty() { s.title.clone() } else { s.ep.show.clone() }).unwrap_or_default();
        let label = { let l = j.get("label").str(); if l.is_empty() { label_of(&theme.kind, theme.number) } else { l } };

        // the episode waits; it plays again on close if it was playing now
        let main_paused = self.mpv.as_ref().and_then(|m| m.get_flag("pause")).unwrap_or(true);
        let tp = &mut self.theme_player;
        if tp.status.is_empty() {
            tp.main_was_playing = !main_paused;
        } else if !main_paused {
            tp.main_was_playing = true;
        }
        if let Some(m) = &self.mpv {
            let _ = m.set("pause", "yes");
        }
        // the old song stops at once, so two never play together
        if let Some(m) = &tp.mpv {
            let _ = m.cmd(&["stop"]);
        }
        tp.req += 1;
        tp.status = "loading".into();
        tp.error.clear();
        tp.label = label;
        tp.video = Some(known.clone().unwrap_or(Video { theme: theme.clone(), ..Video::default() }));
        let (req, data) = (tp.req, self.data_dir.clone());
        self.events.push(Event::XRay);
        self.spawn(move |tx, http| {
            let r = (|| -> Result<(Video, themesong::Stream), String> {
                let ytdlp = themesong::ensure_ytdlp(&*http, &data)?;
                let (v, page) = match known {
                    Some(v) => (v, page),
                    None => {
                        let v = themesong::find_video(&ytdlp, &show, &theme)?;
                        let p = v.url();
                        (v, p)
                    }
                };
                let stream = themesong::stream_url(&*http, &data, &ytdlp, &page)?;
                Ok((v, stream))
            })();
            if let Err(e) = &r {
                crate::debug::log("theme", &format!("video: {}", e));
            }
            send(tx, Msg::ThemeStream(req, r));
        });
    }

    pub(super) fn on_theme_stream(&mut self, req: u64, r: Result<(Video, themesong::Stream), String>) {
        if req != self.theme_player.req || self.theme_player.status != "loading" {
            return; // another song was asked meanwhile, or the video was closed
        }
        self.events.push(Event::XRay);
        let (v, stream) = match r {
            Ok(x) => x,
            Err(e) => {
                self.theme_player.status = "error".into();
                self.theme_player.error = e;
                return;
            }
        };
        self.theme_player.video = Some(v);
        if self.theme_player.mpv.is_none() {
            let made = Mpv::new_embedded(&self.rt.libmpv_candidates()).and_then(|mut m| {
                let fresh = self.theme_player.fresh.clone();
                m.init_render_sw(Box::new(move || fresh.store(true, Ordering::SeqCst)))?;
                Ok(m)
            });
            match made {
                Ok(m) => self.theme_player.mpv = Some(Arc::new(m)),
                Err(e) => {
                    crate::debug::log("theme", &format!("player: {}", e));
                    self.theme_player.status = "error".into();
                    self.theme_player.error = format!("The video could not be played: {}", e);
                    return;
                }
            }
        }
        let vol = *self.theme_player.volume.get_or_insert(self.settings.general.volume.clamp(0.0, 100.0));
        let muted = self.theme_player.muted;
        let Some(m) = self.theme_player.mpv.clone() else { return };
        let _ = m.events(); // what is left of the song before
        let _ = m.set("volume", &format!("{:.0}", vol));
        let _ = m.set("mute", if muted { "yes" } else { "no" });
        // YouTube may give the sound apart: mpv plays it along with the picture. `change-list ... append` adds the
        // address as one item (a plain list would split it at the ':' of "https:").
        let _ = m.cmd(&["change-list", "audio-files", "clr", ""]);
        if let Some(a) = &stream.audio {
            let _ = m.cmd(&["change-list", "audio-files", "append", a]);
        }
        match m.cmd(&["loadfile", &stream.video, "replace"]).and_then(|_| m.set("pause", "no")) {
            Ok(()) => self.theme_player.status = "playing".into(),
            Err(e) => {
                self.theme_player.status = "error".into();
                self.theme_player.error = e;
            }
        }
    }

    /// What the video player of the X-Ray reported since the last tick: a failure to open the video ends it with
    /// the reason.
    fn theme_video_step(&mut self) {
        let Some(m) = self.theme_player.mpv.clone() else { return };
        for e in m.events() {
            if let Evt::EndFile { reason: 4, error } = e {
                if self.theme_player.status == "playing" {
                    self.theme_player.status = "error".into();
                    self.theme_player.error = if error.is_empty() { "The video could not be played.".into() } else { error };
                    self.events.push(Event::XRay);
                }
            }
        }
    }

    /// The episode started again while a video plays in the X-Ray: the video pauses (both would play at once).
    pub(super) fn theme_on_main_resumed(&mut self) {
        // still looking for it: the viewer chose the episode, the lookup is dropped
        if self.theme_player.status == "loading" {
            self.theme_stop_quiet();
            return;
        }
        if self.theme_player.status == "playing" {
            if let Some(m) = &self.theme_player.mpv {
                if m.get_flag("pause") == Some(false) {
                    let _ = m.set("pause", "yes");
                }
            }
            self.theme_player.main_was_playing = false;
        }
    }

    /// Play / pause of the video (toggles when `pause` is not given). Playing it pauses the episode again.
    pub fn theme_pause(&mut self, pause: Option<bool>) {
        let Some(m) = self.theme_player.mpv.clone() else { return };
        let now = m.get_flag("pause").unwrap_or(false);
        let want = pause.unwrap_or(!now);
        // at the end (keep-open): play starts the video again from the beginning
        if !want && m.get_flag("eof-reached") == Some(true) {
            let _ = m.cmd(&["seek", "0", "absolute"]);
        }
        let _ = m.set("pause", if want { "yes" } else { "no" });
        if !want {
            if let Some(main) = &self.mpv {
                if main.get_flag("pause") == Some(false) {
                    let _ = main.set("pause", "yes");
                    self.theme_player.main_was_playing = true;
                }
            }
        }
    }

    pub fn theme_seek(&mut self, pos: f64) {
        if let Some(m) = &self.theme_player.mpv {
            let _ = m.cmd(&["seek", &format!("{:.2}", pos.max(0.0)), "absolute"]);
        }
    }

    /// Volume (0-100) and / or mute of the video in the X-Ray.
    pub fn theme_volume(&mut self, volume: Option<f64>, mute: Option<bool>) {
        if let Some(v) = volume {
            self.theme_player.volume = Some(v.clamp(0.0, 100.0));
        }
        if let Some(b) = mute {
            self.theme_player.muted = b;
        }
        if let Some(m) = &self.theme_player.mpv {
            if let Some(v) = self.theme_player.volume {
                let _ = m.set("volume", &format!("{:.0}", v));
            }
            let _ = m.set("mute", if self.theme_player.muted { "yes" } else { "no" });
        }
    }

    /// Closes the video of the X-Ray. `resume`: Some(true) plays the episode again, Some(false) leaves it paused,
    /// None plays it again only if it was playing when the video started.
    pub fn theme_stop(&mut self, resume: Option<bool>) {
        let was = self.theme_player.main_was_playing;
        self.theme_stop_quiet();
        if resume.unwrap_or(was) {
            if let Some(m) = &self.mpv {
                let _ = m.set("pause", "no");
            }
        }
    }

    /// Stops the video of the X-Ray without touching the episode (the player is closed, or another video starts).
    pub(super) fn theme_stop_quiet(&mut self) {
        let tp = &mut self.theme_player;
        if let Some(m) = &tp.mpv {
            let _ = m.cmd(&["stop"]);
        }
        tp.req += 1; // a lookup still running is dropped when it ends
        tp.status.clear();
        tp.error.clear();
        tp.video = None;
        tp.main_was_playing = false;
        self.events.push(Event::XRay);
    }

    /// The player and its "new picture" flag, for the interface to draw the video (see `rh_theme_frame`).
    pub fn theme_frame_source(&self) -> Option<(Arc<Mpv>, Arc<AtomicBool>)> {
        if self.theme_player.status != "playing" {
            return None;
        }
        Some((self.theme_player.mpv.clone()?, self.theme_player.fresh.clone()))
    }

    /// State of the video in the X-Ray: status, the song, the clock of the video and its sound.
    pub fn theme_video_json(&self) -> Json {
        let tp = &self.theme_player;
        let m = tp.mpv.as_ref().filter(|_| tp.status == "playing");
        let f = |name: &str| m.and_then(|m| m.get_f64(name)).unwrap_or(0.0);
        let v = tp.video.clone().unwrap_or_default();
        let has_video = m.and_then(|m| m.get_i64("dwidth")).unwrap_or(0) > 0;
        Json::obj(vec![
            ("status", Json::Str(tp.status.clone())),
            ("error", Json::Str(tp.error.clone())),
            ("label", Json::Str(tp.label.clone())),
            ("kind", Json::Str(v.theme.kind.clone())),
            ("song", Json::Str(v.theme.song.clone())),
            ("artist", Json::Str(v.theme.artist.clone())),
            ("title", Json::Str(v.title.clone())),
            ("channel", Json::Str(v.channel.clone())),
            ("thumbnail", Json::Str(if v.video_id.is_empty() { String::new() } else { v.thumbnail() })),
            ("pos", Json::Num(f("time-pos"))),
            ("dur", Json::Num(if f("duration") > 0.0 { f("duration") } else { v.duration })),
            ("paused", Json::Bool(m.and_then(|m| m.get_flag("pause")).unwrap_or(false))),
            ("ended", Json::Bool(m.and_then(|m| m.get_flag("eof-reached")).unwrap_or(false))),
            // still opening (no picture yet) though the status already says playing
            ("buffering", Json::Bool(m.is_some() && !has_video)),
            ("volume", Json::Num(tp.volume.unwrap_or(self.settings.general.volume.clamp(0.0, 100.0)))),
            ("muted", Json::Bool(tp.muted)),
            ("mainWasPlaying", Json::Bool(tp.main_was_playing)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_ids_and_labels() {
        assert_eq!(youtube_id("https://www.youtube.com/watch?v=abc123&t=4"), "abc123");
        assert_eq!(youtube_id("https://youtu.be/xyz?si=1"), "xyz");
        assert_eq!(youtube_id("https://example.com/v"), "");
        assert_eq!(label_of("op", 2), "Opening 2");
        assert_eq!(label_of("ed", 0), "Ending");
    }
}
