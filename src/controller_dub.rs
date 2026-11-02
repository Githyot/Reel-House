//! Finding a dub on its own: while an episode plays without audio in the language of the app, the other torrents the
//! addons offered for the same episode are read for one whose name says it carries that language ("Dublado", "Dual
//! Áudio", "PT-BR"…). The best is started in the torrent engine and only its sound is added to the player
//! (`audio-add` of the engine's stream address), lined up with the video's own audio (`audiosync`) and selected
//! when the match is sure. Nothing is saved: the torrent is deleted when the episode ends.
//!
//! No source is built in: only the addons the user added are read. Off with "Look for a dub in other torrents" in
//! the Audio menu (`ExtAudioStore::no_auto_dub`).
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::controller_audio::{file_track_id, save_ext_audio, ExtAudio};
use super::*;

/// Words in a torrent's name that say it carries audio in `lang`: (sure, maybe). "Dual"/"multi" only say there is a
/// second language, likely the one of the people who share it.
fn markers(lang: &str) -> (&'static [&'static str], &'static [&'static str]) {
    match lang {
        "pt" => (
            &["dublado", "dublada", "dublagem", "pt-br", "pt_br", "ptbr", "pt br", "portugues", "português", "portuguese", "nacional", "🇧🇷", "brazilian", "por]"],
            &["dual audio", "dual áudio", "dual-audio", "dual.audio", "dual.áudio", "multi-audio", "multi audio", "multiaudio", "dual"],
        ),
        "es" => (&["latino", "castellano", "español", "espanol", "spanish", "doblado", "🇪🇸", "🇲🇽", "spa]"], &["dual audio", "dual-audio", "multi-audio", "multi audio", "dual"]),
        "en" => (&["english dub", "eng dub", "english", "🇬🇧", "🇺🇸", "eng]"], &["dual audio", "dual-audio", "multi-audio", "multi audio", "dual"]),
        "ja" => (&["japanese", "jpn", "🇯🇵", "日本語"], &["dual audio", "dual-audio", "multi-audio", "multi audio", "dual"]),
        _ => (&[], &["dual audio", "dual-audio", "multi-audio", "multi audio"]),
    }
}

/// 2 = the name says the stream has audio in `lang`, 1 = it has several languages, 0 = nothing said.
pub fn dub_rank(st: &Stream, lang: &str) -> u8 {
    let text = format!("{} {} {}", st.name, st.desc, st.filename.as_deref().unwrap_or("")).to_lowercase();
    let (sure, maybe) = markers(lang);
    // "Legendado" alone = subtitles in that language, not a dub
    if sure.iter().any(|m| text.contains(m)) && !(lang == "pt" && text.contains("legendado") && !text.contains("dublado") && !text.contains("dual")) {
        2
    } else if maybe.iter().any(|m| text.contains(m)) {
        1
    } else {
        0
    }
}

/// The streams worth trying for a dub in `lang`, best first: the surest names, then direct links, then ones with peers, then the
/// smallest file (only its sound is used, the rest is downloaded for nothing). Never `cur`, never one without a
/// torrent or address.
pub fn dub_candidates(streams: &[Stream], cur: usize, lang: &str) -> Vec<usize> {
    let mut v: Vec<(u8, usize)> = streams
        .iter()
        .enumerate()
        .filter(|(i, s)| *i != cur && (s.info_hash.is_some() || s.url.is_some()) && s.local.is_none())
        .map(|(i, s)| (dub_rank(s, lang), i))
        .filter(|(r, _)| *r > 0)
        .collect();
    v.sort_by(|a, b| {
        let (sa, sb) = (&streams[a.1], &streams[b.1]);
        b.0.cmp(&a.0)
            // a direct link opens at once; a torrent may sit minutes looking for peers
            .then(sb.url.is_some().cmp(&sa.url.is_some()))
            .then((sb.seeders > 0).cmp(&(sa.seeders > 0)))
            .then(sa.pack.cmp(&sb.pack))
            .then(sa.size_mb.max(1).cmp(&sb.size_mb.max(1)))
            .then(sb.seeders.cmp(&sa.seeders))
    });
    v.into_iter().map(|(_, i)| i).collect()
}

/// Whether the video's own audio tag in `lang` can be believed. Not when its stream's name says it is subtitled
/// ("Legendado") or names only audio in other languages: many releases tag every track with the language of the
/// group that shares them, so a Japanese track may come tagged Portuguese.
pub fn tag_doubtful(st: &Stream, lang: &str) -> bool {
    let text = format!("{} {} {}", st.name, st.desc, st.filename.as_deref().unwrap_or(""));
    let low = text.to_lowercase();
    if dub_rank(st, lang) == 2 {
        return false;
    }
    if lang == "pt" && (low.contains("legendado") || low.contains("legendada")) {
        return true;
    }
    let a = crate::streamlang::audio_langs(&text);
    !a.multi && !a.langs.is_empty() && !a.langs.iter().any(|l| util::lang_base(&l.lang) == lang)
}

/// A short name for a dub candidate in the Audio menu: its addon and the first words of its name.
pub fn dub_label(st: &Stream) -> String {
    let flat = |x: &str| x.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = flat(&st.name);
    let desc = flat(st.desc.lines().next().unwrap_or(""));
    let mut t = if desc.is_empty() || name.contains(&desc) { name } else { format!("{} {}", name, desc) };
    if t.chars().count() > 70 {
        t = t.chars().take(69).collect::<String>() + "…";
    }
    if st.addon.is_empty() { t } else { format!("{} · {}", st.addon, t) }
}

/// Which audio track of a file is the dub (1 = first): one tagged `lang`, else one titled with its words, else the
/// first one in another language than `own` (the video's own audio), else the first.
pub fn pick_dub_track(tracks: &[(String, String)], lang: &str, own: &str) -> i64 {
    let (sure, _) = markers(lang);
    let is_lang = |l: &str| !l.is_empty() && util::lang_base(l) == lang;
    if let Some(i) = tracks.iter().position(|(l, _)| is_lang(l)) {
        return i as i64 + 1;
    }
    if let Some(i) = tracks.iter().position(|(_, t)| {
        let t = t.to_lowercase();
        sure.iter().any(|m| t.contains(m))
    }) {
        return i as i64 + 1;
    }
    let own = util::lang_base(own);
    if !own.is_empty() {
        if let Some(i) = tracks.iter().position(|(l, _)| !l.is_empty() && util::lang_base(l) != own) {
            return i as i64 + 1;
        }
    }
    1
}

/// Another stream of the episode whose tracks can be added to the one playing: its audio languages (from its name),
/// whether it says it has several, and whether it says it has subtitles.
#[derive(Clone, Debug)]
pub struct Source {
    pub index: usize,
    pub label: String,
    pub langs: Vec<String>,
    pub multi: bool,
    pub subs: bool,
}

/// The other streams of the episode, quickest to open first: direct links (no torrent to start), then the ones that
/// name more languages, then ones with peers, then the smallest.
pub fn other_sources(streams: &[Stream], cur: usize) -> Vec<Source> {
    let mut v: Vec<Source> = streams
        .iter()
        .enumerate()
        .filter(|(i, s)| *i != cur && (s.info_hash.is_some() || s.url.is_some()) && s.local.is_none())
        .map(|(i, s)| {
            let text = format!("{} {} {}", s.name, s.desc, s.filename.as_deref().unwrap_or(""));
            let a = crate::streamlang::audio_langs(&text);
            let mut langs: Vec<String> = Vec::new();
            for l in a.langs {
                let b = util::lang_base(&l.lang);
                if !b.is_empty() && !langs.contains(&b) {
                    langs.push(b);
                }
            }
            Source { index: i, label: dub_label(s), langs, multi: a.multi, subs: crate::streamlang::says_subs(&text) }
        })
        .collect();
    v.sort_by(|a, b| {
        let (sa, sb) = (&streams[a.index], &streams[b.index]);
        sb.url.is_some()
            .cmp(&sa.url.is_some())
            .then((b.langs.len() + b.multi as usize).cmp(&(a.langs.len() + a.multi as usize)))
            .then((sb.seeders > 0).cmp(&(sa.seeders > 0)))
            .then(sa.pack.cmp(&sb.pack))
            .then(sa.size_mb.max(1).cmp(&sb.size_mb.max(1)))
    });
    v
}

/// The Audio menu's "Audio from other streams": the best stream for each language named, then the ones that only
/// say they have several (index, language or "", text).
pub fn audio_choices(sources: &[Source]) -> Vec<(usize, String, String)> {
    let mut out: Vec<(usize, String, String)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for s in sources {
        for l in &s.langs {
            if !seen.contains(l) {
                seen.push(l.clone());
                out.push((s.index, l.clone(), format!("{} · {}", util::lang_name(l), s.label)));
            }
        }
    }
    for s in sources.iter().filter(|s| s.multi && s.langs.len() < 2).take(3) {
        out.push((s.index, String::new(), s.label.clone()));
    }
    out.truncate(16);
    out
}

/// The Subtitles menu's "From other streams": the ones that say they carry subtitles first, then the others with
/// several languages (index, text).
pub fn sub_choices(sources: &[Source]) -> Vec<(usize, String)> {
    let mut v: Vec<&Source> = sources.iter().filter(|s| s.subs).collect();
    v.extend(sources.iter().filter(|s| !s.subs && (s.multi || s.langs.len() > 1)));
    v.into_iter().take(6).map(|s| (s.index, s.label.clone())).collect()
}

/// Tracks being added from another stream (mpv opens it in the background): what to do once they show up.
#[derive(Clone, Debug)]
pub struct DubPending {
    pub url: String,
    /// the audio language wanted ("" = any: just add what it has)
    pub lang: String,
    pub manual: bool,
    pub label: String,
    pub since: std::time::Instant,
    /// `sub-add` was sent too (the sound came without subtitles, or no sound came)
    pub subs_asked: bool,
}

impl Core {
    /// The language a dub is looked for in: the app's.
    fn dub_lang(&self) -> String {
        util::lang_base(&self.ui_lang())
    }

    /// "Look for a dub in other torrents" in the Audio menu.
    pub fn set_auto_dub(&mut self, on: bool) {
        self.ext_audio.no_auto_dub = !on;
        save_ext_audio(&self.data_dir, &self.ext_audio);
        if on {
            // look now for the episode playing
            self.audio_sync.dub_token = 0;
        }
        self.events.push(Event::Session);
    }

    pub fn auto_dub(&self) -> bool {
        !self.ext_audio.no_auto_dub
    }

    fn sources_now(&self) -> Vec<Source> {
        let Some(s) = self.session.as_ref() else { return vec![] };
        let opened = &self.audio_sync.extra_urls;
        other_sources(&s.streams, s.cur).into_iter().filter(|x| !s.streams[x.index].url.as_ref().map(|u| opened.contains(u)).unwrap_or(false) && !self.audio_sync.extra_opened.contains(&x.index)).collect()
    }

    /// For the Audio menu: the audio languages other streams of the episode say they have (index, language, text).
    pub fn audio_sources(&self) -> Vec<(usize, String, String)> {
        audio_choices(&self.sources_now())
    }

    /// For the Subtitles menu: other streams that may bring subtitles in more languages (index, text).
    pub fn sub_sources(&self) -> Vec<(usize, String)> {
        sub_choices(&self.sources_now())
    }

    /// The user picked another stream in the Audio or Subtitles menu: its audio and subtitle tracks are added to the
    /// video playing (`lang` = the audio language to switch to, "" = none).
    pub fn dub_try(&mut self, index: usize, lang: &str) {
        let Some(s) = self.session.as_ref() else { return };
        let Some(st) = s.streams.get(index).cloned() else { return };
        if s.stage != Stage::Playing || index == s.cur {
            return;
        }
        let token = s.token;
        // the automatic search does not run after a pick by hand
        self.audio_sync.dub_token = token;
        crate::debug::log("player", &format!("dub: picked by hand: {} (audio '{}')", dub_label(&st), lang));
        self.extra_open(token, index, vec![st], lang.to_string(), true);
    }

    /// Runs on each tick: once per episode, after it started playing, looks for a dub when it has none, and for
    /// subtitles in the app's language when neither the file nor the subtitle addons have them.
    pub(super) fn dub_step(&mut self) {
        self.dub_poll();
        if self.audio_sync.dub_busy || self.audio_sync.dub_pending.is_some() {
            return;
        }
        let Some(s) = self.session.as_ref() else { return };
        if s.stage != Stage::Playing || self.audio_sync.dub_token == s.token {
            return;
        }
        // a few seconds in: the video's own tracks are known and the restored files are added
        if s.loaded_at.map(|t| t.elapsed().as_secs() < 4).unwrap_or(true) {
            return;
        }
        // the subtitle addons answered (or gave up)
        if matches!(s.sub_job, SubJob::Running) && s.loaded_at.map(|t| t.elapsed().as_secs() < 30).unwrap_or(true) {
            return;
        }
        let Some(m) = self.mpv.clone() else { return };
        let token = s.token;
        self.audio_sync.dub_token = token;
        self.audio_sync.dub_note.clear();
        let lang = self.dub_lang();
        if lang.is_empty() {
            return;
        }
        let tracks = m.tracks("audio");
        let cur = s.streams.get(s.cur);
        let doubtful = cur.map(|x| tag_doubtful(x, &lang)).unwrap_or(false);
        let tagged = tracks.iter().any(|t| !t.external && !t.lang.is_empty() && util::lang_base(&t.lang) == lang);
        let has_it = (tagged && !doubtful) || tracks.iter().any(|t| t.external);
        let cur_is_dub = cur.map(|x| dub_rank(x, &lang) == 2).unwrap_or(false);
        let cands = dub_candidates(&s.streams, s.cur, &lang);
        if !self.ext_audio.no_auto_dub && !has_it && !cur_is_dub && !cands.is_empty() {
            if tagged {
                crate::debug::log("player", &format!("dub: the video's audio is tagged '{}' but its name says otherwise; looking anyway", lang));
            }
            let tries: Vec<Stream> = cands.iter().take(2).map(|&i| s.streams[i].clone()).collect();
            let first = cands[0];
            self.extra_open(token, first, tries, lang, false);
            return;
        }
        crate::debug::log(
            "player",
            &format!("dub: not looked for (auto {}, language '{}', tagged: {}, doubtful: {}, name says dub: {}, {} candidates)", !self.ext_audio.no_auto_dub, lang, tagged, doubtful, cur_is_dub, cands.len()),
        );
        if cands.is_empty() && !has_it && !cur_is_dub && !self.ext_audio.no_auto_dub {
            self.audio_sync.dub_note = "No dub found in the other torrents of this episode.".into();
        }
        // subtitles in the app's language: the file's own, the addons', else a stream that says it has subtitles
        let subs_here = m.tracks("sub").iter().any(|t| util::lang_base(&t.lang) == lang) || s.sub_cands.iter().any(|c| util::lang_base(&c.lang) == lang);
        if !subs_here && !self.ext_audio.no_auto_dub {
            let src = other_sources(&s.streams, s.cur).into_iter().find(|x| x.subs);
            if let Some(x) = src {
                let st = s.streams[x.index].clone();
                crate::debug::log("player", &format!("dub: no '{}' subtitles here; adding the tracks of {}", lang, x.label));
                self.extra_open(token, x.index, vec![st], String::new(), false);
            }
        }
    }

    /// Opens the first of `tries` that works (a direct link at once, a torrent in the engine) and adds its tracks.
    fn extra_open(&mut self, token: u64, index: usize, tries: Vec<Stream>, lang: String, manual: bool) {
        let Some(s) = self.session.as_ref() else { return };
        let sid = s.sid.clone();
        let cancel = s.cancel.clone();
        let st = self.settings.clone();
        let engine = self.engine.clone();
        let exe = self.rt.rqbit.clone();
        let rt_dir = self.rt.dir.clone();
        let dl = self.data_dir.join("torrents");
        self.audio_sync.dub_gen += 1;
        let gen = self.audio_sync.dub_gen;
        self.audio_sync.dub_busy = true;
        self.audio_sync.dub_pending = None;
        self.audio_sync.dub_note.clear();
        self.audio_sync.dub_progress.clear();
        self.audio_sync.dub_opening = tries.first().map(dub_label).unwrap_or_default();
        self.audio_sync.extra_opened.push(index);
        if manual {
            self.audio_sync.dub_manual_gen = gen;
        }
        self.events.push(Event::Session);
        crate::debug::log("player", &format!("dub: opening {:?}", tries.iter().map(dub_label).collect::<Vec<_>>()));
        self.spawn(move |tx, http| {
            let cancelled = || cancel.load(Ordering::SeqCst);
            // logged, and shown in the Audio menu so a slow torrent reads as working (HP: tapped again and again)
            let status = |m: String| {
                crate::debug::log("player", &format!("dub: {}", m));
                send(tx, Msg::DubStatus(token, gen, m));
            };
            let mut last = String::from("no stream");
            for stream in &tries {
                if cancelled() {
                    return;
                }
                let r = resolve_stream(&*http, &st, stream, &engine, exe.as_deref(), &rt_dir, &dl, &sid, &status, &cancelled).and_then(|res| {
                    // a direct link is asked for one byte first: one that does not answer fails here, in this thread,
                    // not inside the player
                    if res.torrent.is_none() && res.url.starts_with("http") {
                        let mut rq = crate::http::Request::get(&res.url).header("Range", "bytes=0-0").timeout(20);
                        for (k, v) in &res.headers {
                            rq = rq.header(k, v);
                        }
                        let resp = http.send(&rq).map_err(|e| format!("the link does not answer ({})", e))?;
                        if resp.status >= 400 {
                            return Err(format!("the link answers HTTP {}", resp.status));
                        }
                    }
                    Ok(res)
                });
                match r {
                    Ok(res) => {
                        send(tx, Msg::DubFound(token, gen, dub_label(stream), lang.clone(), Ok(res)));
                        return;
                    }
                    Err(e) => {
                        crate::debug::log("player", &format!("dub: {} failed: {}", dub_label(stream), e));
                        last = e;
                    }
                }
            }
            send(tx, Msg::DubFound(token, gen, String::new(), lang, Err(last)));
        });
    }

    pub(super) fn on_dub_found(&mut self, token: u64, gen: u64, label: String, lang: String, r: Result<Resolved, String>) {
        let current = gen == self.audio_sync.dub_gen && self.session.as_ref().map(|s| s.token == token).unwrap_or(false);
        if gen == self.audio_sync.dub_gen {
            self.audio_sync.dub_busy = false;
            self.audio_sync.dub_opening.clear();
            self.audio_sync.dub_progress.clear();
        }
        self.events.push(Event::Session);
        let res = match r {
            Ok(res) => res,
            Err(e) => {
                if current {
                    self.audio_sync.dub_note = "A dub was listed, but its torrent could not be opened.".into();
                    crate::debug::log("player", &format!("dub: none could be opened ({})", e));
                }
                return;
            }
        };
        if let Some((base, id, _)) = res.torrent.clone() {
            if current {
                // deleted when the episode ends
                self.audio_sync.dub_torrents.push((base, id));
            } else {
                // came too late: not needed any more
                self.spawn(move |_, http| {
                    let _ = Rqbit { http: &*http, base }.delete(&id.to_string());
                });
                return;
            }
        }
        let (Some(m), true) = (self.mpv.clone(), current) else { return };
        if !res.headers.is_empty() {
            crate::debug::log("player", "dub: the link asks for headers the player cannot send for an added track; trying without");
        }
        // in the background: mpv opens the file without holding the player (a slow link froze the menu before)
        // (recent mpv adds every track of the file with it, subtitles included; older ones only its sound, so the
        // subtitles are asked for apart when none came)
        let a = m.cmd(&["async", "audio-add", &res.url, "auto"]);
        crate::debug::log("player", &format!("dub: adding the tracks of '{}' ({:?})", label, a.err()));
        self.audio_sync.extra_urls.push(res.url.clone());
        let manual = self.audio_sync.dub_manual_gen == gen;
        self.audio_sync.dub_pending = Some(DubPending { url: res.url, lang, manual, label, since: std::time::Instant::now(), subs_asked: false });
        self.audio_sync.dub_note = "Adding the tracks of another stream…".into();
    }

    /// Waits for the tracks of the stream being added, then picks the dub among them and syncs it.
    fn dub_poll(&mut self) {
        let Some(p) = self.audio_sync.dub_pending.clone() else { return };
        let Some(m) = self.mpv.clone() else { return };
        let Some(sid) = self.session.as_ref().map(|s| s.sid.clone()) else { return };
        let audio = m.tracks("audio");
        let of_file: Vec<&crate::mpv::Track> = audio.iter().filter(|t| t.external && controller_audio::same_file(&t.file, &p.url)).collect();
        let subs = m.tracks("sub").into_iter().filter(|t| t.external && controller_audio::same_file(&t.file, &p.url)).count();
        let waited = p.since.elapsed().as_secs();
        // the seconds since mpv began reading the other stream, beside "Adding the tracks…"
        let shown = if waited >= 2 { format!("{} s", waited) } else { String::new() };
        if shown != self.audio_sync.dub_progress {
            self.audio_sync.dub_progress = shown;
            self.events.push(Event::Session);
        }
        // no subtitles came with the sound (or no sound in 20 s): ask for them apart, once
        if subs == 0 && !p.subs_asked && (!of_file.is_empty() || waited >= 20) {
            let r = m.cmd(&["async", "sub-add", &p.url, "auto"]);
            crate::debug::log("player", &format!("dub: asking for the subtitles of '{}' apart ({:?})", p.label, r.err()));
            if let Some(q) = self.audio_sync.dub_pending.as_mut() {
                q.subs_asked = true;
                q.since = std::time::Instant::now();
            }
            if of_file.is_empty() {
                return;
            }
        }
        if of_file.is_empty() && subs == 0 && waited < 120 {
            return;
        }
        self.audio_sync.dub_pending = None;
        self.audio_sync.dub_progress.clear();
        self.events.push(Event::Session);
        if of_file.is_empty() && subs == 0 {
            crate::debug::log("player", &format!("dub: no tracks came from '{}' in {} s", p.label, waited));
            self.audio_sync.dub_note = "A dub was listed, but its torrent could not be opened.".into();
            return;
        }
        let langs: Vec<String> = of_file.iter().map(|t| t.lang.clone()).collect();
        crate::debug::log("player", &format!("dub: '{}' brought {} audio tracks {:?} and {} subtitle tracks", p.label, of_file.len(), langs, subs));
        self.audio_sync.dub_note = "Tracks added from another stream.".into();
        if of_file.is_empty() {
            return;
        }
        if p.lang.is_empty() {
            // opened for its subtitles: its sound tracks would only crowd the Audio menu
            for t in &of_file {
                let _ = m.cmd(&["audio-remove", &t.id.to_string()]);
            }
            return;
        }
        let own = audio.iter().find(|t| !t.external && t.selected).or_else(|| audio.iter().find(|t| !t.external)).map(|t| t.lang.clone()).unwrap_or_default();
        let pairs: Vec<(String, String)> = of_file.iter().map(|t| (t.lang.clone(), t.title.clone())).collect();
        let nth = pick_dub_track(&pairs, &p.lang, &own);
        let rec = ExtAudio { active: false, file_aid: nth, remote: true, ..ExtAudio::new(&sid, &p.url) };
        self.ext_audio.tracks.push(rec.clone());
        let Some(id) = file_track_id(&m, &p.url, nth) else { return };
        // only the language asked for stays: mpv adds every sound track of the file (HP: picked Portuguese, got 12)
        for t in of_file.iter().filter(|t| t.id != id) {
            let r = m.cmd(&["audio-remove", &t.id.to_string()]);
            if let Err(e) = r {
                crate::debug::log("player", &format!("dub: could not drop track {} ({})", t.id, e));
            }
        }
        if p.manual {
            // picked by hand: heard at once, the sync only fixes its delay
            self.audio_select(id);
            self.toast("Dub added. Syncing it with the video…");
        } else {
            self.audio_sync.select_after = Some(p.url.clone());
            self.toast("Dub found in another torrent. Syncing it with the video…");
        }
        self.audio_sync_rec(rec, true);
    }

    /// Deletes the torrents tracks were streamed from (the episode ended or another one came).
    pub(super) fn dub_drop(&mut self) {
        self.ext_audio.tracks.retain(|t| !t.remote);
        self.audio_sync.dub_pending = None;
        self.audio_sync.dub_busy = false;
        self.audio_sync.dub_opening.clear();
        self.audio_sync.extra_urls.clear();
        self.audio_sync.extra_opened.clear();
        // answers still on the way are dropped
        self.audio_sync.dub_gen += 1;
        for (base, id) in std::mem::take(&mut self.audio_sync.dub_torrents) {
            self.spawn(move |_, http| {
                let ok = Rqbit { http: &*http, base }.delete(&id.to_string());
                crate::debug::log("player", &format!("dub: torrent {} deleted: {}", id, ok));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(name: &str, desc: &str, seeders: u32, size_mb: u32) -> Stream {
        Stream {
            addon: "a".into(),
            name: name.into(),
            desc: desc.into(),
            url: None,
            info_hash: Some("abc".into()),
            file_idx: None,
            trackers: vec![],
            filename: None,
            headers: vec![],
            yt_id: None,
            res: 1080,
            seeders,
            size_mb,
            pack: false,
            pack_mb: 0,
            size_exact: false,
            size_lookup: 0,
            score: 0,
            local: None,
            binge_group: None,
        }
    }

    #[test]
    fn reads_dub_words_in_names() {
        assert_eq!(dub_rank(&st("Pokemon S06E03 1080p DUBLADO", "", 1, 1), "pt"), 2);
        assert_eq!(dub_rank(&st("Show", "Show.S01E02.DUAL.AUDIO.1080p", 1, 1), "pt"), 1);
        assert_eq!(dub_rank(&st("Show", "Show S01E02 PT-BR 720p", 1, 1), "pt"), 2);
        assert_eq!(dub_rank(&st("Show LEGENDADO 1080p", "", 1, 1), "pt"), 0);
        assert_eq!(dub_rank(&st("Show [Erai-raws] 1080p", "", 1, 1), "pt"), 0);
        assert_eq!(dub_rank(&st("Show LATINO", "", 1, 1), "es"), 2);
    }

    #[test]
    fn best_dub_first() {
        let list = vec![
            st("Show 1080p DUBLADO", "", 50, 1400), // playing now
            st("Show 1080p DUAL", "", 90, 900),
            st("Show 720p DUBLADO", "", 0, 300),
            st("Show 1080p DUBLADO", "", 12, 1600),
            st("Show 480p DUBLADO", "", 3, 350),
            st("Show 1080p", "", 500, 800),
        ];
        assert_eq!(dub_candidates(&list, 0, "pt"), vec![4, 3, 2, 1]);
        assert!(dub_candidates(&list[5..], 99, "pt").is_empty());
    }

    #[test]
    fn audio_tags_contradicted_by_the_name() {
        // a subtitled release whose Japanese track came tagged Portuguese
        assert!(tag_doubtful(&st("Dandadan S02E04 1080p LEGENDADO", "", 1, 1), "pt"));
        assert!(tag_doubtful(&st("Dandadan S02E04 [JPN]", "", 1, 1), "pt"));
        assert!(!tag_doubtful(&st("Dandadan S02E04 1080p", "", 1, 1), "pt"));
        assert!(!tag_doubtful(&st("Dandadan S02E04 DUAL ÁUDIO", "", 1, 1), "pt"));
        assert!(!tag_doubtful(&st("Dandadan S02E04 DUBLADO", "", 1, 1), "pt"));
    }

    #[test]
    fn dub_labels_are_short() {
        let mut s = st("Addon\n1080p", "Show S01E02 DUBLADO\n1.2 GB", 1, 1);
        s.addon = "Mine".into();
        assert_eq!(dub_label(&s), "Mine · Addon 1080p Show S01E02 DUBLADO");
    }

    #[test]
    fn every_language_of_the_other_streams() {
        let mut link = st("Addon 1080p", "DAN DA DAN T02 EP05\nDublado", 0, 0);
        link.url = Some("https://x/ep.mp4".into());
        link.info_hash = None;
        let list = vec![
            st("[Erai-raws] Show - 05 [1080p][MultiSub]", "", 6, 900), // playing now
            st("Show S02E05 1080p", "Show.S02E05.WEB-DL.DUAL.1080p 🇧🇷 🇯🇵", 5, 900),
            link,
            st("Show S02E05 LATINO", "", 9, 700),
            st("[Erai-raws] Show - 05 [1080p][MultiSub]", "", 3, 900),
            st("Show 1080p", "", 50, 800),
        ];
        let src = other_sources(&list, 0);
        // the direct link first
        assert_eq!(src[0].index, 2);
        let a = audio_choices(&src);
        let langs: Vec<&str> = a.iter().map(|x| x.1.as_str()).collect();
        assert!(langs.contains(&"pt") && langs.contains(&"ja") && langs.contains(&"es"), "{:?}", langs);
        // Portuguese comes from the direct link
        assert_eq!(a.iter().find(|x| x.1 == "pt").unwrap().0, 2);
        // subtitles: the MultiSub release first
        assert_eq!(sub_choices(&src)[0].0, 4);
        assert!(!sub_choices(&src).iter().any(|x| x.0 == 5));
    }

    #[test]
    fn picks_the_dub_track_of_a_dual_audio_file() {
        let t = |l: &str, n: &str| (l.to_string(), n.to_string());
        assert_eq!(pick_dub_track(&[t("jpn", ""), t("por", "")], "pt", "jpn"), 2);
        assert_eq!(pick_dub_track(&[t("", "Japonês"), t("", "Português BR")], "pt", ""), 2);
        assert_eq!(pick_dub_track(&[t("eng", ""), t("und", "")], "pt", "eng"), 2);
        assert_eq!(pick_dub_track(&[t("", "")], "pt", "jpn"), 1);
    }
}
