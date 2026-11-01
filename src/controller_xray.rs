//! X-Ray of the video playing (see `xray`), who Google Lens recognised in the frame on screen (`xray_lens`), the
//! playback clock observed from mpv (time-pos, pause) and the names of the audio / subtitle tracks (`track_label`).
//! Part of `controller`.
use super::*;
use crate::mpv::PropValue;
use crate::xray::{self, Input, XRay};

/// Per episode.
#[derive(Default)]
pub struct XRayState {
    pub asked: bool,
    pub result: Option<Result<XRay, String>>,
    /// Counts the lookups of this episode: a "Fetch" with another source starts a new one, and the answer of an
    /// older one (still on its way) is dropped.
    pub gen: u64,
    /// What Google Lens said of the frames looked at, one per stretch of `LENS_BUCKET` seconds of the episode.
    pub lens: Vec<LensFrame>,
    /// The trivia of this episode asked to Google / the language model (`xray_trivia`): None until asked, then
    /// loading (Some(None)), then the answer.
    pub trivia: Option<Option<Result<crate::xray_trivia::Trivia, String>>>,
}

/// Google Lens's answer for one stretch of the episode, and who of the X-Ray it names.
pub struct LensFrame {
    pub bucket: i64,
    pub text: String,
    pub hits: Vec<xray::Item>,
}

/// The frames of 15 seconds share one Lens answer: looking again a few seconds later is not sent to Google again.
pub const LENS_BUCKET: f64 = 15.0;

fn lens_bucket(pos: f64) -> i64 {
    (pos.max(0.0) / LENS_BUCKET).floor() as i64
}

/// The sources the X-Ray can be asked to use ("auto" = MyAnimeList for anime, TMDB for the rest, IMDb as fallback).
pub const XRAY_SOURCES: [&str; 4] = ["auto", "tmdb", "mal", "imdb"];

/// One number for the session and the lookup, so the answer can be matched with both.
fn xray_token(session: u64, gen: u64) -> u64 {
    session.wrapping_mul(1 << 20).wrapping_add(gen)
}

/// What mpv reported last, and what the interface was last told.
pub struct Clock {
    pub pos: f64,
    pub paused: bool,
    sent_pos: f64,
    sent_paused: bool,
    sent_at: Instant,
}

impl Default for Clock {
    fn default() -> Clock {
        Clock { pos: 0.0, paused: false, sent_pos: -1.0, sent_paused: false, sent_at: Instant::now() }
    }
}

impl Core {
    /// Starts watching time-pos and pause of the player (called once, when the player is made).
    pub(super) fn observe_playback(&self) {
        if let Some(m) = &self.mpv {
            let _ = m.observe("time-pos", true);
            let _ = m.observe("pause", false);
        }
    }

    /// A property change from mpv. An event goes to the interface when pause changes, and for the position at most
    /// four times a second (or at once after a jump).
    pub(super) fn on_playback_prop(&mut self, name: &str, v: Option<PropValue>) {
        match (name, v) {
            ("time-pos", Some(PropValue::Double(p))) => self.clock.pos = p,
            ("pause", Some(PropValue::Flag(b))) => {
                let resumed = self.clock.paused && !b;
                self.clock.paused = b;
                // the episode was started again (space, the play button) while an opening video plays: that one stops
                if resumed {
                    self.theme_on_main_resumed();
                }
            }
            _ => return,
        }
        let c = &mut self.clock;
        let jump = (c.pos - c.sent_pos).abs() > 2.0;
        if c.paused != c.sent_paused || jump || (c.sent_at.elapsed().as_millis() >= 250 && (c.pos - c.sent_pos).abs() >= 0.2) {
            c.sent_pos = c.pos;
            c.sent_paused = c.paused;
            c.sent_at = Instant::now();
            self.events.push(Event::Playback(c.pos, c.paused));
        }
    }

    /// Builds the X-Ray once the video plays and its opening / ending times are known (or after 25 s without them).
    pub(super) fn xray_step(&mut self) {
        let dur = self.mpv.as_ref().and_then(|m| m.get_f64("duration")).unwrap_or(0.0);
        let Some(s) = self.session.as_mut() else { return };
        // (a "Fetch" after the first lookup, `gen` > 0, does not wait for the opening times again)
        if s.xray.asked || s.stage != Stage::Playing || !(s.segments_fetched || s.started.elapsed().as_secs() > 25 || s.xray.gen > 0) {
            return;
        }
        s.xray.asked = true;
        let tmdb_key = self.settings.metadata.tmdb_key.clone();
        let source = self.settings.general.xray_source.clone();
        let dub_lang = match self.settings.general.ui_lang(&self.system_lang).as_str() {
            l if l.starts_with("pt") => "Portuguese",
            l if l.starts_with("es") => "Spanish",
            l if l.starts_with("fr") => "French",
            l if l.starts_with("de") => "German",
            l if l.starts_with("it") => "Italian",
            _ => "English",
        }
        .to_string();
        let meta_id = s.meta.as_ref().map(|m| m.id.clone()).unwrap_or_default();
        let imdb = [meta_id.as_str(), s.sid.as_str()].iter().find(|x| x.starts_with("tt")).map(|x| x.split(':').next().unwrap_or("").to_string());
        let anime = meta_id.starts_with("mal:") || meta_id.starts_with("kitsu:") || meta_id.starts_with("anilist:") || s.seg_source == "AniSkip" || s.meta.as_ref().map(|m| m.genres.iter().any(|g| g.to_lowercase().contains("anim"))).unwrap_or(false);
        let mal_direct = meta_id.strip_prefix("mal:").and_then(|x| x.parse::<u64>().ok());
        let songs_at: Vec<(f64, f64, String)> = s
            .segments
            .iter()
            .filter_map(|x| match x.kind {
                crate::animeskip::Kind::Intro => Some((x.start, x.end.unwrap_or(x.start + 90.0), "op".to_string())),
                crate::animeskip::Kind::Credits => Some((x.start, x.end.unwrap_or(if dur > 0.0 { dur } else { x.start + 90.0 }), "ed".to_string())),
                _ => None,
            })
            .collect();
        let show = if s.ep.show.is_empty() { s.title.clone() } else { s.ep.show.clone() };
        let year = s.meta.as_ref().and_then(|m| xray::year_of(&m.release_info));
        let data_dir = self.data_dir.clone();
        let inp = Input {
            title: show.clone(),
            kind: s.kind.clone(),
            imdb,
            tmdb_key,
            mal: mal_direct,
            season: s.ep.season,
            episode: s.ep.episode,
            duration: dur,
            songs_at,
            chapters: s.chapters.clone(),
            dub_lang,
            source,
            anime,
            year,
            data_dir: Some(data_dir),
        };
        // the session id, not the token: the token changes when the quality is switched or the video retried,
        // and `asked` stays set then, so a result tied to the token would be dropped and stay "loading"
        s.xray.gen += 1;
        let token = xray_token(s.id, s.xray.gen);
        self.events.push(Event::XRay);
        self.spawn(move |tx, http| {
            let mut inp = inp;
            // the MyAnimeList entry by name: for anime, and also whenever MAL was picked in the X-Ray header (a title
            // not seen as anime, e.g. a torrent or a file without catalogue genres, used to get "not found" here)
            if inp.mal.is_none() && (anime || inp.source == "mal") {
                inp.mal = crate::skipdb::find_mal_id(&*http, &show, inp.season).ok().flatten();
            }
            let x = xray::build(&*http, &inp);
            crate::debug::log("xray", &format!("{} items from {:?}; {}", x.items.len(), x.sources, x.notes.join(" | ")));
            let r = if x.items.is_empty() { Err(if x.notes.is_empty() { "Nothing was found for this video.".to_string() } else { x.notes.join(" ") }) } else { Ok(x) };
            send(tx, Msg::XRayDone(token, r));
        });
    }

    pub(super) fn on_xray_done(&mut self, token: u64, r: Result<XRay, String>) {
        if let Some(s) = self.session.as_mut().filter(|s| xray_token(s.id, s.xray.gen) == token) {
            // a Lens answer that came before the cast (or with another source): matched again with the new cast
            if let Ok(x) = &r {
                for f in s.xray.lens.iter_mut() {
                    f.hits = xray::lens_match(&x.items, &f.text);
                }
            }
            s.xray.result = Some(r);
            self.events.push(Event::XRay);
        }
    }

    /// The text of Google Lens for the frame at `pos` (`text`, its overview, and `titles`, the pages it found): the
    /// people of the X-Ray it names are kept for that stretch of the episode and shown at the top of "In this scene".
    /// Only sent by the interface when the viewer asks (the button) or turned on `xray_lens_auto`.
    pub fn xray_lens(&mut self, j: &Json) {
        let pos = j.get("pos").as_f64().unwrap_or(self.clock.pos);
        let mut text = j.get("text").str();
        for t in j.get("titles").strings() {
            text.push('\n');
            text.push_str(&t);
        }
        let Some(s) = self.session.as_mut() else { return };
        let hits = match &s.xray.result {
            Some(Ok(x)) => xray::lens_match(&x.items, &text),
            _ => vec![],
        };
        crate::debug::log("xray", &format!("lens at {:.0}s: {} recognised", pos, hits.len()));
        let bucket = lens_bucket(pos);
        s.xray.lens.retain(|f| f.bucket != bucket);
        s.xray.lens.push(LensFrame { bucket, text, hits });
        if s.xray.lens.len() > 60 {
            s.xray.lens.remove(0);
        }
        self.events.push(Event::XRay);
    }

    /// Lens's answers of this episode, one per stretch of `LENS_BUCKET` seconds (the interface shows the one of the
    /// stretch playing): who it names and the start of its text (shown as "What Google Lens says").
    fn lens_json(&self) -> Json {
        let frames = self.session.as_ref().map(|s| s.xray.lens.as_slice()).unwrap_or(&[]);
        let frame = |f: &LensFrame| {
            let t = f.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let summary = if t.chars().count() > 320 { format!("{}…", t.chars().take(320).collect::<String>()) } else { t };
            let hits = xray::XRay { items: f.hits.clone(), sources: vec![], notes: vec![] }.to_json().get("items").clone();
            Json::obj(vec![("bucket", Json::Num(f.bucket as f64)), ("summary", Json::Str(summary)), ("hits", hits)])
        };
        Json::obj(vec![("bucketSecs", Json::Num(LENS_BUCKET)), ("frames", Json::Arr(frames.iter().map(frame).collect()))])
    }

    /// More about one person of the X-Ray (a click on the row): looked up once in the background, then kept.
    pub fn xray_person(&mut self, link: &str) {
        if link.is_empty() || self.xray_people.get(link).map(|v| v.get("error").str().is_empty()).unwrap_or(false) {
            return;
        }
        self.xray_people.insert(link.to_string(), Json::obj(vec![("loading", Json::Bool(true))]));
        self.events.push(Event::XRay);
        let key = self.settings.metadata.tmdb_key.clone();
        let lang = self.ui_lang().chars().take(2).collect::<String>();
        let link = link.to_string();
        self.spawn(move |tx, http| {
            let r = xray::person_info(&*http, &link, &key, &lang);
            send(tx, Msg::XRayPerson(link, r));
        });
    }

    /// The trivia of the episode playing, for the "Trivia" tab: the question "Tell me trivia about <title> that
    /// happened in episode <n>" asked to Google (its AI Overview) or else to the language model of Preferences >
    /// Conversation (Gemini searches Google), in the language of the interface. Asked once per episode (when the
    /// tab is opened) and kept on disk; `again` asks once more after an error.
    pub fn xray_trivia(&mut self, again: bool) {
        let lang = {
            let l = self.ui_lang();
            let l = if l.is_empty() || l == "system" { crate::util::system_lang() } else { l };
            l.chars().take(2).collect::<String>().to_lowercase()
        };
        let tutor = crate::llm::pick(&self.settings.general.kaiwa_ai, &self.settings.subtitles, &self.settings.general.kaiwa_ollama);
        let cfgs = crate::xray_trivia::models(&tutor, &self.settings.subtitles);
        let cache = self.data_dir.join("xray-trivia");
        let Some(s) = self.session.as_mut() else { return };
        match &s.xray.trivia {
            Some(None) => return,
            Some(Some(Ok(_))) => return,
            Some(Some(Err(_))) if !again => return,
            _ => {}
        }
        let title = if s.ep.show.is_empty() { s.title.clone() } else { s.ep.show.clone() };
        let ask = crate::xray_trivia::Ask { title, season: s.ep.season, episode: s.ep.episode, lang };
        s.xray.trivia = Some(None);
        let session = s.id;
        self.events.push(Event::XRay);
        self.spawn(move |tx, http| {
            let r = crate::xray_trivia::fetch(&*http, &cfgs, &ask, Some(&cache));
            crate::debug::log("xray", &format!("trivia for {:?} s{:?} e{:?}: {}", ask.title, ask.season, ask.episode, match &r { Ok(t) => format!("{} items from {}", t.items.len(), t.source), Err(e) => e.clone() }));
            send(tx, Msg::XRayTrivia(session, r));
        });
    }

    pub(super) fn on_xray_trivia(&mut self, session: u64, r: Result<crate::xray_trivia::Trivia, String>) {
        if let Some(s) = self.session.as_mut().filter(|s| s.id == session) {
            s.xray.trivia = Some(Some(r));
            self.events.push(Event::XRay);
        }
    }

    fn trivia_json(&self) -> Json {
        let t = self.session.as_ref().and_then(|s| s.xray.trivia.as_ref());
        let (status, body) = match t {
            None => ("none", Json::Null),
            Some(None) => ("loading", Json::Null),
            Some(Some(Ok(t))) => ("ready", t.to_json()),
            Some(Some(Err(e))) => ("error", Json::Str(e.clone())),
        };
        Json::obj(vec![("status", Json::Str(status.into())), ("data", body)])
    }

    /// "Fetch" in the X-Ray: keeps the source chosen (TMDB, MyAnimeList, IMDb or automatic) and builds the X-Ray
    /// again with it. The lookup starts on the next tick (`xray_step`).
    pub fn xray_fetch(&mut self, source: &str) {
        let source = if XRAY_SOURCES.contains(&source) { source } else { "auto" };
        if self.settings.general.xray_source != source {
            self.settings.general.xray_source = source.to_string();
            self.save_settings();
        }
        if let Some(s) = self.session.as_mut() {
            s.xray.asked = false;
            s.xray.result = None;
        }
        self.events.push(Event::XRay);
    }

    /// The X-Ray payload for the interface: status, the items with their time ranges, and the clock.
    pub fn xray_json(&self) -> Json {
        let (status, body) = match self.session.as_ref() {
            None => ("none", Json::Null),
            Some(s) => match &s.xray.result {
                None if s.xray.asked => ("loading", Json::Null),
                None => ("waiting", Json::Null),
                Some(Ok(x)) => ("ready", x.to_json()),
                Some(Err(e)) => ("error", Json::Str(e.clone())),
            },
        };
        Json::obj(vec![
            ("status", Json::Str(status.into())),
            ("data", body),
            ("pos", Json::Num(self.clock.pos)),
            ("paused", Json::Bool(self.clock.paused)),
            // the source chosen in the X-Ray header ("auto", "tmdb", "mal", "imdb")
            ("source", Json::Str(self.settings.general.xray_source.clone())),
            // Google Lens: who it recognised in the frame of this moment, and whether it is asked by itself on open
            ("lens", self.lens_json()),
            ("lensAuto", Json::Bool(self.settings.general.xray_lens_auto)),
            // more about the people asked for (a click on a row), by their link
            ("people", Json::obj(self.xray_people.iter().map(|(k, v)| (k.as_str(), v.clone())).collect())),
            // the trivia of the episode from Google / the language model ("none", "loading", "ready", "error")
            ("episodeTrivia", self.trivia_json()),
        ])
    }
}

/// The language of an audio / subtitle track's name written the way the viewer reads it: "ja" is "日本語", "en" is
/// "EN-US" and "pt" is "PT-BR" ("ja - [Erai-raws]_AAC_CR" becomes "日本語 - [Erai-raws]_AAC_CR"). Only a whole word in
/// any case ("Ja", "JA", "pT") that stands as a language: the first word, or one set apart by brackets, dashes or
/// other signs ("Audio [ja]", "Track 2 - en"). Pieces of words ("Japan", "Pedro") and a word inside a sentence
/// ("comentario en español") are left alone. Used for the labels and titles of both menus (`tracks_json`).
pub fn track_label(label: &str) -> String {
    let chars: Vec<char> = label.chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(label.len() + 8);
    let mut i = 0;
    let mut first = true;
    while i < chars.len() {
        if !is_word(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && is_word(chars[i]) {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        // the signs around it, spaces skipped: a letter there means the word is part of a sentence
        let before = chars[..start].iter().rev().find(|c| !c.is_whitespace()).copied();
        let after = chars[i..].iter().find(|c| !c.is_whitespace()).copied();
        let apart = first || (before.map(|c| !is_word(c)).unwrap_or(true) && after.map(|c| !is_word(c)).unwrap_or(true));
        let nice = match word.to_ascii_lowercase().as_str() {
            "ja" | "jp" | "jpn" => Some("日本語"),
            "en" | "eng" => Some("EN-US"),
            "pt" | "por" => Some("PT-BR"),
            _ => None,
        };
        out.push_str(nice.filter(|_| apart).unwrap_or(&word));
        first = false;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_labels_name_the_language() {
        assert_eq!(track_label("ja - [Erai-raws]_AAC_CR"), "日本語 - [Erai-raws]_AAC_CR");
        assert_eq!(track_label("JA"), "日本語");
        assert_eq!(track_label("Ja"), "日本語");
        assert_eq!(track_label("en - English"), "EN-US - English");
        assert_eq!(track_label("En"), "EN-US");
        assert_eq!(track_label("pT - Portuguese (Brazil)"), "PT-BR - Portuguese (Brazil)");
        assert_eq!(track_label("Pt"), "PT-BR");
        assert_eq!(track_label("Track 2 - Audio [ja]"), "Track 2 - Audio [日本語]");
        // whole words only
        assert_eq!(track_label("Japan Dub"), "Japan Dub");
        assert_eq!(track_label("Pedro's commentary"), "Pedro's commentary");
        assert_eq!(track_label("English"), "English");
        assert_eq!(track_label("en_US stereo"), "en_US stereo");
        assert_eq!(track_label("Comentario en español"), "Comentario en español");
        assert_eq!(track_label("Track 2 - en"), "Track 2 - EN-US");
        assert_eq!(track_label("eng (SDH)"), "EN-US (SDH)");
        assert_eq!(track_label(""), "");
    }
}
