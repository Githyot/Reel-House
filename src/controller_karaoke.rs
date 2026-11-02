//! Karaoke over the openings and endings of anime (Karaoke Mugen data). The songs of the series are looked up
//! once per episode; while the player is inside an opening or an ending (the times come from AniSkip, IntroDB,
//! Anime Skip or the chapters) the lyrics of the matching song are shown and the normal subtitle comes back
//! afterwards. The timing can be fine tuned with the usual "subtitles earlier / later" keys.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::animeskip::Kind;
use crate::anilist;
use crate::karaoke::{self, Song};

#[derive(Default)]
pub struct KaraState {
    pub started: bool,
    pub songs: Vec<Song>,
    /// What the Statistics panel says about it.
    pub status: String,
    /// The karaoke track now shown: its track id and the subtitle track to bring back.
    pub active: Option<(i64, Option<i64>)>,
    /// The style override of the viewer's subtitles was relaxed while the karaoke is on screen (see karaoke_step).
    pub relaxed: bool,
    /// The lyrics on screen, to be written again when their look or delay is changed: file, original text, where the part starts, settings used.
    pub live: Option<(PathBuf, String, f64, String)>,
    /// Segment starts that have no matching song, so they are not tried again.
    pub missed: Vec<i64>,
    /// What the player's Subtitles menu shows about the search (the series it found, how many songs…).
    pub detail: String,
    /// Name of the song on screen now.
    pub shown: Option<String>,
    /// The variant (index in `songs`) the viewer chose in the menu; `None` lets the best match be picked.
    pub forced: Option<usize>,
    /// Key of the song shown last (its own delay is kept under it).
    pub last_key: Option<String>,
    /// Makes the file names of the lyrics different each time.
    pub serial: u64,
}

impl Core {
    /// Looks the songs of this episode's series up (anime only, once per episode).
    pub(super) fn start_karaoke(&mut self) {
        if !self.settings.general.karaoke {
            return;
        }
        let Some(s) = self.session.as_mut() else { return };
        if s.kara.started || s.kind != "series" || s.ep.show.is_empty() {
            return;
        }
        let meta_id = s.meta.as_ref().map(|m| m.id.clone()).unwrap_or_default();
        let anime = meta_id.starts_with("mal:")
            || meta_id.starts_with("kitsu:")
            || meta_id.starts_with("anilist:")
            || s.meta.as_ref().map(|m| m.genres.iter().any(|g| g.to_lowercase().contains("anim"))).unwrap_or(false);
        s.kara.started = true;
        crate::debug::log("karaoke", &format!("episode of '{}' (meta id '{}'): anime={}", s.ep.show, meta_id, anime));
        if !anime {
            s.kara.status = "not an anime".to_string();
            return;
        }
        s.kara.status = "searching...".to_string();
        let token = s.id;
        let show = s.ep.show.clone();
        let season = s.ep.season;
        let mal_direct = meta_id.strip_prefix("mal:").and_then(|x| x.parse::<u64>().ok());
        self.spawn(move |tx, http| {
            // The series is identified through AniList (fast and reachable) unless the title already carries its id.
            let mut ids: Vec<u64> = mal_direct.into_iter().collect();
            let mut names: Vec<String> = Vec::new();
            let mut year: Option<i64> = None;
            let note: String;
            if ids.is_empty() {
                let q = match season {
                    Some(n) if n > 1 => format!("{} season {}", show, n),
                    _ => show.clone(),
                };
                match anilist::search(&*http, &q) {
                    // an episode of a series: a film or music video of the same name is never its songs (AniList answers
                    // "Pokemon season 6" with a Pokémon film)
                    Ok(list) => match list.iter().find(|f| karaoke::series_kind(&f.kind)) {
                        Some(f) => {
                            ids.push(f.id);
                            year = f.year;
                            // The romaji name first (the site names series by it), then the name of the file, then the rest.
                            names.extend(f.names.first().filter(|n| !n.is_empty()).cloned());
                            names.push(show.clone());
                            names.extend(f.names.iter().skip(1).filter(|n| !n.is_empty()).cloned());
                            note = format!("{} (MyAnimeList {})", f.names.first().cloned().unwrap_or_default(), f.id);
                        }
                        None => note = "AniList does not know this title".to_string(),
                    },
                    Err(e) => note = format!("AniList: {}", e),
                }
            } else {
                note = format!("MyAnimeList {}", ids[0]);
            }
            if !names.contains(&show) {
                names.push(show.clone());
            }
            // The site names series by their romaji title (the first name of AniList) and keeps every season under the
            // name of the series, often with the id of the first season: find() also tries the names without the season.
            let r = karaoke::songs_for(&*http, &ids, &names, year);
            crate::debug::log("karaoke", &format!("ids {:?}, year {:?}, names {:?}; result: {}", ids, year, names, match &r {
                Ok(v) => format!("{} song(s): {}", v.len(), v.iter().map(|x| format!("{} ({:.0}s)", x.label, x.duration)).collect::<Vec<_>>().join("; ")),
                Err(e) => format!("error: {}", e),
            }));
            send(tx, Msg::Karaoke(token, r, note));
        });
    }

    /// Searches again (from the Subtitles menu).
    pub fn karaoke_retry(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.kara = KaraState::default();
        }
        self.start_karaoke();
        self.events.push(Event::Session);
    }

    pub(super) fn on_karaoke(&mut self, token: u64, r: Result<Vec<Song>, String>, note: String) {
        let Some(s) = self.session.as_mut().filter(|s| s.id == token) else { return };
        s.kara.detail = note;
        match r {
            Ok(v) if v.is_empty() => s.kara.status = "no karaoke for this series on kara.moe".to_string(),
            Ok(v) => {
                let op = v.iter().filter(|x| x.kind == Kind::Intro).count();
                let ed = v.len() - op;
                s.kara.status = format!("found {} opening(s) and {} ending(s)", op, ed);
                s.kara.songs = v;
            }
            Err(e) => s.kara.status = format!("could not search: {}", e),
        }
        let toast = format!("Karaoke: {}", s.kara.status);
        self.events.push(Event::Toast(toast));
        self.events.push(Event::Session);
    }

    /// Runs on every tick: puts the lyrics on when an opening or ending starts and takes them off when it ends.
    pub(super) fn karaoke_step(&mut self) {
        let on = self.settings.general.karaoke;
        // borrowed, not cloned: this runs on every tick and General has well over a hundred fields
        let g = &self.settings.general;
        let forced = self.settings.subtitles.style.force_style;
        let Some(mpv) = self.mpv.clone() else { return };
        let Some(s) = self.session.as_mut() else { return };
        if s.stage != Stage::Playing || (s.kara.songs.is_empty() && s.kara.active.is_none()) {
            return;
        }
        let pos = mpv.get_f64("time-pos").unwrap_or(0.0);
        let dur = mpv.get_f64("duration").unwrap_or(0.0);
        let seg = if on { animeskip::segment_at(&s.segments, pos, dur).filter(|x| matches!(x.kind, Kind::Intro | Kind::Credits)).cloned() } else { None };
        match (seg, s.kara.active) {
            (Some(seg), None) => {
                let key = seg.start as i64;
                if s.kara.missed.contains(&key) {
                    return;
                }
                let end = seg.end.unwrap_or(dur);
                let len = if end > seg.start { end - seg.start } else { 0.0 };
                let chosen = s.kara.forced.and_then(|i| s.kara.songs.get(i)).filter(|x| x.kind == seg.kind);
                let Some(song) = chosen.or_else(|| karaoke::pick(&s.kara.songs, seg.kind, len)) else {
                    crate::debug::log("karaoke", &format!("{:?} at {:.0}s lasts {:.0}s: no song of that length among {}", seg.kind, seg.start, len, s.kara.songs.len()));
                    s.kara.missed.push(key);
                    return;
                };
                let dir = std::env::temp_dir().join("reelhouse-karaoke");
                let _ = std::fs::create_dir_all(&dir);
                let tag = if seg.kind == Kind::Intro { "op" } else { "ed" };
                s.kara.serial += 1;
                let path = dir.join(format!("{}-{}-{}-{}.ass", s.id, key, tag, s.kara.serial));
                let skey = song_key(song);
                let sig = karaoke_sig(&g, &skey);
                if std::fs::write(&path, karaoke_text(&song.ass, seg.start, &g, &skey)).is_err() {
                    s.kara.missed.push(key);
                    return;
                }
                // The karaoke goes in the second subtitle slot, so the viewer's own subtitle keeps playing and
                // can be chosen on its own.
                let prev = mpv.get_i64("sid");
                let label = song.label.clone();
                if mpv.cmd(&["sub-add", &path.to_string_lossy(), "auto", &label, "und"]).is_err() {
                    s.kara.missed.push(key);
                    return;
                }
                let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                let Some(track) = mpv.tracks("sub").into_iter().find(|t| t.file.ends_with(&file)) else {
                    crate::debug::log("karaoke", "the player did not list the karaoke track");
                    s.kara.missed.push(key);
                    return;
                };
                // If the player picked the new track as the main one (nothing was chosen before), the choice is given back.
                if mpv.get_i64("sid") == Some(track.id) && prev != Some(track.id) {
                    let _ = mpv.set("sid", &prev.map(|p| p.to_string()).unwrap_or_else(|| "no".to_string()));
                }
                let _ = mpv.set("secondary-sid", &track.id.to_string());
                // The viewer's style for subtitles must not touch the karaoke: it has its own look.
                if mpv.set("secondary-sub-ass-override", "no").is_err() && forced {
                    let _ = mpv.set("sub-ass-override", "scale");
                    s.kara.relaxed = true;
                }
                crate::debug::log("karaoke", &format!("showing '{}' from {:.0}s as the second subtitle (track {}; main subtitle {:?})", label, seg.start, track.id, prev));
                s.kara.active = Some((track.id, prev));
                s.kara.live = Some((path.clone(), song.ass.clone(), seg.start, sig));
                s.kara.last_key = Some(skey);
                s.kara.shown = Some(label.clone());
                self.events.push(Event::Toast(label));
            }
            (None, Some((kara_sid, _))) => {
                if mpv.get_i64("secondary-sid") == Some(kara_sid) {
                    let _ = mpv.set("secondary-sid", "no");
                }
                if s.kara.relaxed {
                    let _ = mpv.set("sub-ass-override", "force");
                    s.kara.relaxed = false;
                }
                s.kara.active = None;
                s.kara.shown = None;
                s.kara.live = None;
            }
            (Some(_), Some((id, _))) => {
                // The look or the delay was changed in Preferences while the lyrics are on screen.
                if let Some((path, ass, start, used)) = s.kara.live.clone() {
                    let skey = s.kara.last_key.clone().unwrap_or_default();
                    let now = karaoke_sig(&g, &skey);
                    if used != now {
                        // The new file is put in first, then the old one leaves: the lyrics never go away in between, and the
                        // viewer's own subtitle (main slot) is left alone. Reloading the track in place made the player give it
                        // the look of the viewer's subtitle.
                        let prev_main = mpv.get_i64("sid");
                        s.kara.serial += 1;
                        let dir = path.parent().map(|d| d.to_path_buf()).unwrap_or_else(std::env::temp_dir);
                        let newpath = dir.join(format!("{}-{}-hot{}.ass", s.id, start as i64, s.kara.serial));
                        let label = s.kara.shown.clone().unwrap_or_default();
                        if std::fs::write(&newpath, karaoke_text(&ass, start, &g, &skey)).is_ok()
                            && mpv.cmd(&["sub-add", &newpath.to_string_lossy(), "auto", &label, "und"]).is_ok()
                        {
                            let file = newpath.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                            if let Some(track) = mpv.tracks("sub").into_iter().find(|t| t.file.ends_with(&file)) {
                                if mpv.get_i64("sid") == Some(track.id) && prev_main != Some(track.id) {
                                    let _ = mpv.set("sid", &prev_main.map(|p| p.to_string()).unwrap_or_else(|| "no".to_string()));
                                }
                                let _ = mpv.set("secondary-sid", &track.id.to_string());
                                let _ = mpv.set("secondary-sub-ass-override", "no");
                                // The log of a run showed the lyrics becoming the MAIN subtitle (so they took its look) after a reload:
                                // if the main slot holds a karaoke track, the viewer's own choice is given back.
                                let mine = s.kara.active.and_then(|a| a.1);
                                let cur = mpv.get_i64("sid");
                                if cur == Some(id) || cur == Some(track.id) {
                                    let _ = mpv.set("sid", &mine.filter(|m| *m != id).map(|m| m.to_string()).unwrap_or_else(|| "no".to_string()));
                                }
                                let _ = mpv.cmd(&["sub-remove", &id.to_string()]);
                                let _ = std::fs::remove_file(&path);
                                s.kara.active = Some((track.id, s.kara.active.and_then(|a| a.1)));
                                s.kara.live = Some((newpath, ass, start, now.clone()));
                            }
                        }
                        // not tried again every tick when it did not work
                        if let Some(l) = s.kara.live.as_mut() {
                            l.3 = now;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// One line for the Statistics panel and the Subtitles menu.
    pub fn karaoke_status(&self) -> String {
        let Some(s) = self.session.as_ref() else { return String::new() };
        if !self.settings.general.karaoke {
            return "off (Preferences > Karaoke)".to_string();
        }
        if let Some(x) = &s.kara.shown {
            return format!("showing {}", x);
        }
        if s.kara.status.is_empty() {
            return "waiting for the episode to start".to_string();
        }
        let mut t = s.kara.status.clone();
        if !s.kara.songs.is_empty() && !s.segments.iter().any(|x| matches!(x.kind, Kind::Intro | Kind::Credits)) {
            t.push_str("; waiting for the opening/ending times");
        }
        t
    }

    /// The variants of the songs for the menu ("OP1 · 89 s"), and the one the viewer chose.
    pub fn karaoke_variants(&self) -> (Vec<String>, Option<usize>) {
        match self.session.as_ref() {
            Some(s) => (s.kara.songs.iter().map(|x| format!("{} · {:.0} s", x.label.trim_start_matches("Karaoke "), x.duration)).collect(), s.kara.forced),
            None => (vec![], None),
        }
    }

    /// Chooses a variant (`None`: the best match). What is on screen is taken off so the choice shows at once.
    pub fn karaoke_choose(&mut self, i: Option<usize>) {
        let mpv = self.mpv.clone();
        let Some(s) = self.session.as_mut() else { return };
        s.kara.forced = i;
        s.kara.missed.clear();
        if let (Some((id, _)), Some(m)) = (s.kara.active, mpv) {
            if m.get_i64("secondary-sid") == Some(id) {
                let _ = m.set("secondary-sid", "no");
            }
            s.kara.active = None;
            s.kara.shown = None;
            s.kara.live = None;
        }
        self.events.push(Event::Session);
    }

    /// The song whose delay the menu adjusts (the one on screen, or the chosen variant) and its delay.
    pub fn karaoke_song_delay(&self) -> Option<(String, f64)> {
        let s = self.session.as_ref()?;
        let key = s.kara.last_key.clone().or_else(|| s.kara.forced.and_then(|i| s.kara.songs.get(i)).map(song_key))?;
        let d = self.settings.general.kara_delay_for(&key);
        Some((key, d))
    }

    /// Sets the delay of one karaoke (the lyrics on screen are written again at the next tick).
    pub fn karaoke_set_song_delay(&mut self, key: &str, v: f64) {
        self.settings.general.set_kara_delay(key, (v * 10.0).round() / 10.0);
        self.save_settings();
    }

    /// For the Subtitles menu: the series that was identified and the songs that were found ("OP · title · 89 s").
    pub fn karaoke_info(&self) -> (String, Vec<String>) {
        match self.session.as_ref() {
            Some(s) => (s.kara.detail.clone(), s.kara.songs.iter().map(|x| format!("{} · {:.0} s", x.label.trim_start_matches("Karaoke "), x.duration)).collect()),
            None => (String::new(), vec![]),
        }
    }
}

/// What names a song for its own delay.
fn song_key(s: &Song) -> String {
    format!("{}|{:.0}", s.label, s.duration)
}

/// What decides how the lyrics look and when they start (a change means the file is written again).
fn karaoke_sig(g: &settings::General, key: &str) -> String {
    format!("{}|{}|{}|{}|{}|{}|{:?}", g.karaoke_top, g.karaoke_delay, g.karaoke_size, g.karaoke_margin, g.kara_delay_for(key), karaoke_rgb(g).map(|c| format!("{:?}", c)).unwrap_or_default(), karaoke_look(g))
}

fn onoff(v: &str) -> Option<bool> {
    match v {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// The look chosen in Preferences (font, bold, outline, shadow, the colour still to sing) as the file is written.
pub fn karaoke_look(g: &settings::General) -> karaoke::Look {
    karaoke::Look {
        font: Some(g.karaoke_font.trim().to_string()).filter(|f| !f.is_empty()),
        bold: match g.karaoke_bold.as_str() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        },
        outline: Some(g.karaoke_outline as f64).filter(|v| *v >= 0.0),
        shadow: Some(g.karaoke_shadow as f64).filter(|v| *v >= 0.0),
        unfilled: crate::theme::parse_hex(&g.karaoke_unfilled),
        italic: onoff(&g.karaoke_italic),
        underline: onoff(&g.karaoke_underline),
        outline_color: crate::theme::parse_hex(&g.karaoke_outline_color),
        back_color: crate::theme::parse_hex(&g.karaoke_back_color),
        scale_x: Some(g.karaoke_scale_x).filter(|v| *v > 0),
        scale_y: Some(g.karaoke_scale_y).filter(|v| *v > 0),
        spacing: Some(g.karaoke_spacing as f64).filter(|v| *v > -100.0),
        align: Some(g.karaoke_align as u8).filter(|v| (1..=9).contains(v)),
        margin_l: Some(g.karaoke_margin_l).filter(|v| *v > 0),
        margin_r: Some(g.karaoke_margin_r).filter(|v| *v > 0),
        boxed: match g.karaoke_border.as_str() {
            "box" => Some(true),
            "outline" => Some(false),
            _ => None,
        },
    }
}

/// The lyrics file: moved to where the opening/ending starts (plus the delays) and dressed as asked.
fn karaoke_text(ass: &str, start: f64, g: &settings::General, key: &str) -> String {
    let t = karaoke::restyle(&karaoke::shift_ass(ass, start + g.karaoke_delay + g.kara_delay_for(key)), g.karaoke_top, g.karaoke_size, g.karaoke_margin);
    let t = match karaoke_rgb(g) {
        Some(c) => karaoke::recolor(&t, c),
        None => t,
    };
    karaoke::set_styles(&t, &karaoke_look(g))
}

/// The colour the lyrics are painted in: the accent of the app, a colour of one's own, or none (the file's own).
fn karaoke_rgb(g: &settings::General) -> Option<(u8, u8, u8)> {
    match g.karaoke_color.as_str() {
        "" => None,
        "accent" => Some(crate::theme::parse_hex(&g.accent).unwrap_or((0x35, 0x84, 0xe4))),
        c => crate::theme::parse_hex(c),
    }
}
