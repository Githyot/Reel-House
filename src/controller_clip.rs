//! The player's Editor: cuts a part of the video, takes the subtitle of that moment, finds the kanji, the hiragana /
//! katakana readings and a description on the internet, and saves it all as a card of the Study mode.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::subtitles::{decode, parse_cues};

/// What the background job brings back.
#[derive(Clone, Debug)]
pub struct ClipResult {
    pub fields: Vec<String>,
    pub note: String,
    /// The video was cut: the card asks with the video and answers with the subtitle (`CLIP_VIDEO`); otherwise the
    /// older text card (`study::CLIP`) is made.
    pub video: bool,
}

/// The note type of the clip cards made since 2026-10: the front plays the video clip, the back shows the subtitle
/// (and its translation). The fields are those of `study::CLIP` in the same order, plus "Translation" at the end, so
/// the code that reads clip notes by field name reads both. Older clip cards keep their own note type.
pub const CLIP_VIDEO: i64 = 1_000_000_005;

/// Adds the video note type to the collection when it is missing.
fn ensure_clip_video(col: &mut crate::study::Collection) {
    if col.models.iter().any(|m| m.id == CLIP_VIDEO) {
        return;
    }
    let fields = ["Sentence", "Hiragana", "Katakana", "Kanji", "Meaning", "Description", "Clip", "Image", "Source", "Translation"];
    col.models.push(crate::study::Model {
        id: CLIP_VIDEO,
        name: "Reelhouse clip (video)".into(),
        cloze: false,
        fields: fields.iter().map(|f| f.to_string()).collect(),
        tmpls: vec![crate::study::Tmpl {
            name: "Clip".into(),
            q: "{{Image}}<br>{{Clip}}".into(),
            a: "{{FrontSide}}<hr id=answer><big>{{Sentence}}</big><br>{{#Translation}}<i>{{Translation}}</i><br>{{/Translation}}<br>{{#Hiragana}}<b>ひらがな</b><br>{{Hiragana}}<br><br>{{/Hiragana}}{{#Katakana}}<b>カタカナ</b><br>{{Katakana}}<br><br>{{/Katakana}}{{#Meaning}}{{Meaning}}<br><br>{{/Meaning}}{{#Kanji}}{{Kanji}}<br><br>{{/Kanji}}{{#Description}}<small>{{Description}}</small><br>{{/Description}}<small>{{Source}}</small>".into(),
        }],
    });
}

/// Adds a video clip card to the app's deck "Seu Reels" (made when missing).
pub fn add_video_clip(col: &mut crate::study::Collection, fields: Vec<String>, now: i64) -> Result<i64, String> {
    col.ensure_builtin();
    ensure_clip_video(col);
    let did = col.app_deck().ok_or("could not make the deck")?;
    col.add_note(did, CLIP_VIDEO, fields, now)?;
    Ok(did)
}

/// The lines of a subtitle file that overlap `start..end` (seconds), each on one line.
fn lines_in(file: &str, start: f64, end: f64) -> Vec<String> {
    let path = crate::util::file_url_path(file);
    let Ok(bytes) = std::fs::read(&path) else { return vec![] };
    parse_cues(&decode(&bytes))
        .into_iter()
        .filter(|c| secs(&c.end) > start && secs(&c.start) < end)
        .map(|c| c.text.replace('\n', " "))
        .collect()
}

fn secs(t: &str) -> f64 {
    // "HH:MM:SS,mmm"
    let t = t.replace(',', ".");
    let p: Vec<f64> = t.split(':').map(|x| x.parse::<f64>().unwrap_or(0.0)).collect();
    match p.len() {
        3 => p[0] * 3600.0 + p[1] * 60.0 + p[2],
        2 => p[0] * 60.0 + p[1],
        _ => p.first().copied().unwrap_or(0.0),
    }
}

fn stamp(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

impl Core {
    /// Saves the part `start..end` (seconds) of what is playing as a clip card. `fallback` is the subtitle on screen,
    /// used when the subtitle is inside the video file and cannot be read as a whole.
    pub fn clip_save(&mut self, start: f64, end: f64, fallback: String) {
        if self.clip_busy {
            self.toast("A clip is already being saved.");
            return;
        }
        let Some(mpv) = self.mpv.clone() else { return };
        let Some(sess) = self.session.as_ref() else { return };
        if end <= start + 0.2 {
            self.toast("Choose a start and an end first.");
            return;
        }
        let url = mpv.get_str("path").unwrap_or_default();
        if url.is_empty() {
            return;
        }
        let headers = sess.streams.get(sess.cur).map(|s| s.headers.clone()).unwrap_or_default();
        let title = sess.title.clone();
        // the subtitle file the player shows, when there is one (mpv marks the second subtitle "selected" too, so the
        // main one is found by its id)
        let subs = mpv.tracks("sub");
        let sid = mpv.get_i64("sid");
        let main = subs.iter().find(|t| Some(t.id) == sid).or_else(|| subs.iter().find(|t| t.selected));
        let sub_file = main.filter(|t| t.external && !t.file.is_empty()).map(|t| t.file.clone());
        // its translation for the back of the card: the second subtitle shown, else a translation made for this episode
        let sid2 = mpv.get_i64("secondary-sid");
        let tr_file = subs
            .iter()
            .find(|t| Some(t.id) == sid2 && t.external && !t.file.is_empty())
            .map(|t| t.file.clone())
            .or_else(|| self.forced.iter().find(|f| f.sid == sess.sid && Some(&f.path) != sub_file.as_ref()).map(|f| f.path.clone()))
            .filter(|f| Some(f) != sub_file.as_ref());
        let libs = self.rt.libmpv_candidates();
        let media = self.study_media_dir();
        let _ = std::fs::create_dir_all(&media);
        let now = Self::now_secs();
        self.clip_busy = true;
        self.toast("Saving the clip… (cutting the video and looking the words up)");
        self.events.push(Event::Study);
        self.spawn(move |tx, http| {
            let r = (|| -> Result<ClipResult, String> {
                // the lines of the subtitle in the time of the clip
                let mut lines: Vec<String> = sub_file.map(|f| lines_in(&f, start, end)).unwrap_or_default();
                let translation = tr_file.map(|f| lines_in(&f, start, end)).unwrap_or_default().join("<br>");
                if lines.is_empty() && !fallback.trim().is_empty() {
                    lines.push(fallback.replace('\n', " "));
                }
                let sentence = lines.join("<br>");
                let plain = lines.join(" ");
                let video = format!("clip_{}.mp4", now);
                let pic = format!("clip_{}.jpg", now);
                let vpath = media.join(&video).display().to_string();
                let ppath = media.join(&pic).display().to_string();
                let cut = Mpv::encode(&libs, &url, &headers, start, end, &vpath, false, 180);
                let thumb = Mpv::encode(&libs, &url, &headers, start + 0.3, start + 0.6, &ppath, true, 60);
                let cut_err = cut.err();
                let kt = crate::kanji::build(&*http, &plain);
                let mut note = String::new();
                if let Some(e) = &cut_err {
                    note = format!("The video could not be cut ({}); the card has the text only.", e);
                }
                let clip_field = if cut_err.is_none() { format!("<a href=\"clip:{}\">▶ Play the clip</a>", video) } else { String::new() };
                let img_field = if thumb.is_ok() { format!("<img src=\"{}\">", pic) } else { String::new() };
                let source = format!("{} · {}–{}", title, stamp(start), stamp(end));
                let video = cut_err.is_none();
                let mut fields = vec![sentence, kt.hiragana, kt.katakana, kt.kanji, kt.meaning, kt.description, clip_field, img_field, source];
                if video {
                    fields.push(translation);
                }
                Ok(ClipResult { fields, note, video })
            })();
            send(tx, Msg::ClipDone(r));
        });
    }

    pub(super) fn on_clip_done(&mut self, r: Result<ClipResult, String>) {
        self.clip_busy = false;
        match r {
            Ok(c) => match {
                self.fire_hook("onClipSaved", Json::obj(vec![("text", c.fields.first().cloned().unwrap_or_default().into())]));
                if c.video {
                    add_video_clip(&mut self.study, c.fields, Self::now_secs())
                } else {
                    self.study.add_clip(c.fields, Self::now_secs())
                }
            } {
                Ok(_) => {
                    self.save_study();
                    if c.note.is_empty() {
                        self.toast(&format!("Saved to the deck {}.", crate::study::APP_DECK));
                    } else {
                        self.toast(&c.note);
                    }
                }
                Err(e) => self.toast(&e),
            },
            Err(e) => self.toast(&e),
        }
        self.events.push(Event::Study);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_clip_card_asks_with_the_video() {
        let mut col = crate::study::Collection::default();
        let f: Vec<String> = ["学校に行く", "学校 【がっこう】", "", "学", "school", "", "<a href=\"clip:a.mp4\">▶</a>", "<img src=\"a.jpg\">", "src", "I go to school"].iter().map(|x| x.to_string()).collect();
        let did = add_video_clip(&mut col, f, 1_000_000).unwrap();
        assert_eq!(col.deck_name(did), crate::study::APP_DECK);
        let (q, a) = col.render(&col.cards[0], std::path::Path::new("/m"));
        assert!(q.contains("a.mp4") && !q.contains("学校に行く"));
        assert!(a.contains("学校に行く") && a.contains("I go to school"));
        // the older clip cards still render with their own type
        let old: Vec<String> = ["x", "", "", "", "", "", "", "", ""].iter().map(|x| x.to_string()).collect();
        col.add_clip(old, 1_000_001).unwrap();
        assert!(col.render(&col.cards[1], std::path::Path::new("/m")).0.contains('x'));
    }
}
