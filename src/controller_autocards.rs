//! Autocards from the player: the subtitle on screen becomes (or completes) an Anki card. Part of `controller`.
use super::*;
use crate::autocards::{self, Cfg, Mined};
use crate::subtitles::{decode, parse_cues};

fn cue_secs(t: &str) -> f64 {
    let t = t.replace(',', ".");
    let p: Vec<f64> = t.split(':').filter_map(|x| x.trim().parse::<f64>().ok()).collect();
    match p.len() {
        3 => p[0] * 3600.0 + p[1] * 60.0 + p[2],
        2 => p[0] * 60.0 + p[1],
        _ => p.first().copied().unwrap_or(0.0),
    }
}

impl Core {
    pub fn autocards_cfg(&self) -> Cfg {
        let g = &self.settings.general;
        Cfg {
            url: if g.autocards_url.trim().is_empty() { "http://127.0.0.1:8765".into() } else { g.autocards_url.trim().to_string() },
            deck: g.autocards_deck.clone(),
            model: g.autocards_model.clone(),
            f_sentence: g.autocards_f_sentence.clone(),
            f_expression: g.autocards_f_expression.clone(),
            f_picture: g.autocards_f_picture.clone(),
            f_audio: g.autocards_f_audio.clone(),
        }
    }

    /// The line on screen: its sound (with padding), the frame now on screen, and the lines around it, sent to Anki.
    pub fn autocard_mine(&mut self) {
        if self.autocard_busy {
            self.toast("A card is already being made.");
            return;
        }
        let Some(mpv) = self.mpv.clone() else { return };
        let Some(sess) = self.session.as_ref() else { return };
        let line = mpv.get_str("sub-text").unwrap_or_default().trim().to_string();
        let (Some(a), Some(b)) = (mpv.get_f64("sub-start"), mpv.get_f64("sub-end")) else {
            self.toast("No subtitle on screen to make a card from.");
            return;
        };
        if line.is_empty() {
            self.toast("No subtitle on screen to make a card from.");
            return;
        }
        let url = mpv.get_str("path").unwrap_or_default();
        let at = mpv.get_f64("time-pos").unwrap_or(a);
        let delay = mpv.get_f64("sub-delay").unwrap_or(0.0);
        let headers = sess.streams.get(sess.cur).map(|s| s.headers.clone()).unwrap_or_default();
        let sub_file = mpv.tracks("sub").into_iter().find(|t| t.selected && t.external && !t.file.is_empty()).map(|t| t.file);
        let g = &self.settings.general;
        let (np, nn, pad) = (g.autocards_prev.min(5) as usize, g.autocards_next.min(5) as usize, g.autocards_pad_ms.min(2000) as f64 / 1000.0);
        let cfg = self.autocards_cfg();
        let libs = self.rt.libmpv_candidates();
        let tmp = self.data_dir.join("autocards");
        let _ = std::fs::create_dir_all(&tmp);
        let stamp = Self::now_secs();
        let media = self.study_media_dir();
        self.autocard_busy = true;
        self.toast("Making the Anki card…");
        self.spawn(move |tx, http| {
            let r = (|| -> Result<String, String> {
                // the lines around the one on screen, from the subtitle file when it can be read
                let (mut prev, mut next) = (Vec::new(), Vec::new());
                if np + nn > 0 {
                    if let Some(f) = sub_file {
                        let path = crate::util::file_url_path(&f);
                        if let Ok(bytes) = std::fs::read(&path) {
                            let cues = parse_cues(&decode(&bytes));
                            let here = a - delay;
                            if let Some(i) = cues.iter().position(|c| (cue_secs(&c.start) - here).abs() < 0.35) {
                                prev = cues[i.saturating_sub(np)..i].iter().map(|c| c.text.replace('\n', " ")).collect();
                                next = cues[(i + 1).min(cues.len())..(i + 1 + nn).min(cues.len())].iter().map(|c| c.text.replace('\n', " ")).collect();
                            }
                        }
                    }
                }
                let base = tmp.join(format!("reelhouse_{}", stamp)).display().to_string();
                let audio = Mpv::encode_audio(&libs, &url, &headers, (a - pad).max(0.0), b + pad, &base).ok().and_then(|p| {
                    let bytes = std::fs::read(&p).ok()?;
                    let _ = std::fs::remove_file(&p);
                    let name = std::path::Path::new(&p).file_name()?.to_string_lossy().into_owned();
                    Some((name, bytes))
                });
                let jpg = format!("{}.jpg", base);
                let picture = Mpv::encode(&libs, &url, &headers, at, at + 0.2, &jpg, true, 45).ok().and_then(|_| {
                    let bytes = std::fs::read(&jpg).ok()?;
                    let _ = std::fs::remove_file(&jpg);
                    #[cfg(feature = "webp")]
                    if let Some(w) = crate::webpstore::to_webp(&bytes) {
                        return Some((format!("reelhouse_{}.webp", stamp), w));
                    }
                    Some((format!("reelhouse_{}.jpg", stamp), bytes))
                });
                if audio.is_none() && picture.is_none() {
                    return Err("The sound and the picture could not be cut from the video.".into());
                }
                let m = Mined { sentence_line: line, prev, next, picture, audio };
                match autocards::export(&*http, &cfg, &m, (stamp as i64) * 1000) {
                    Err(e) if e == autocards::NOT_ANSWERING => Err(local_card(&media, &m).map(|f| format!("\u{1}{}", f.join("\u{1f}")))?),
                    r => r,
                }
            })();
            match r {
                // Anki is closed: the card goes to the app's own deck (the error carries its fields)
                Err(e) if e.starts_with('\u{1}') => send(tx, Msg::AutocardLocal(e[1..].split('\u{1f}').map(str::to_string).collect())),
                r => send(tx, Msg::AutocardDone(r)),
            }
        });
    }

    /// Anki was closed: the mined line is saved as a card of "Seu Reels" (the app's own study deck).
    pub(super) fn on_autocard_local(&mut self, fields: Vec<String>) {
        self.autocard_busy = false;
        self.study.ensure_builtin();
        let r = self.study.app_deck().ok_or_else(|| "could not make the deck".to_string()).and_then(|did| self.study.add_note(did, crate::study::BASIC, fields, Self::now_secs()));
        match r {
            Ok(_) => {
                if let Some(n) = self.study.notes.last_mut() {
                    n.tags = "seu_reels autocard".into();
                }
                self.save_study();
                self.events.push(Event::Study);
                self.toast("Anki is closed: the card was saved in Seu Reels, in the Study mode.");
            }
            Err(e) => self.toast(&format!("{} ({})", autocards::NOT_ANSWERING, e)),
        }
    }

    pub(super) fn on_autocard_done(&mut self, r: Result<String, String>) {
        self.autocard_busy = false;
        match r {
            Ok(t) => {
                self.fire_hook("onClipSaved", Json::obj(vec![("text", Json::Str(t.clone())), ("autocard", Json::Bool(true))]));
                self.toast(&t)
            }
            Err(e) => self.toast(&e),
        }
    }

    /// Asks AnkiConnect for its version: the test button of the settings.
    pub fn autocards_test(&mut self) {
        let cfg = self.autocards_cfg();
        self.spawn(move |tx, http| {
            let r = autocards::invoke(&*http, &cfg.url, "deckNames", Json::obj(vec![])).map(|d| {
                let names: Vec<String> = d.items().iter().map(|x| x.str()).collect();
                if names.iter().any(|n| n == &cfg.deck) {
                    format!("Anki answered. The deck \"{}\" is there.", cfg.deck)
                } else {
                    format!("Anki answered, but there is no deck \"{}\" (decks: {}).", cfg.deck, names.join(", "))
                }
            });
            send(tx, Msg::AutocardTested(r));
        });
    }
}

/// The card for the app's own deck: the picture and the sound saved in its media folder, the line (and the lines
/// around it) on the back. Front, Back of the Basic note type.
fn local_card(media: &std::path::Path, m: &Mined) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(media).map_err(|e| e.to_string())?;
    let mut front = String::new();
    if let Some((name, bytes)) = &m.picture {
        std::fs::write(media.join(name), bytes).map_err(|e| e.to_string())?;
        front.push_str(&format!("<img src=\"{}\">", name));
    }
    if let Some((name, bytes)) = &m.audio {
        std::fs::write(media.join(name), bytes).map_err(|e| e.to_string())?;
        front.push_str(&format!("<br>[sound:{}]", name));
    }
    let esc = |t: &str| t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\n', "<br>");
    let mut back = String::new();
    for l in &m.prev {
        back.push_str(&format!("<small>{}</small><br>", esc(l)));
    }
    back.push_str(&format!("<big>{}</big>", esc(&m.sentence_line)));
    for l in &m.next {
        back.push_str(&format!("<br><small>{}</small>", esc(l)));
    }
    Ok(vec![front, back])
}
