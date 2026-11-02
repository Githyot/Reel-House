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

/// The streams worth trying for a dub in `lang`, best first: the surest names, then ones with peers, then the
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

    /// The streams of the episode playing that say they carry a dub in the app's language, for the Audio menu
    /// (index in the session's streams, label). Listed also when the automatic search is off or did not run.
    pub fn dub_choices(&self) -> Vec<(usize, String)> {
        let Some(s) = self.session.as_ref() else { return vec![] };
        let lang = self.dub_lang();
        if lang.is_empty() {
            return vec![];
        }
        dub_candidates(&s.streams, s.cur, &lang).into_iter().take(8).map(|i| (i, dub_label(&s.streams[i]))).collect()
    }

    /// The user picked a dub of another torrent in the Audio menu: open it now, whatever the search thought.
    pub fn dub_try(&mut self, index: usize) {
        let Some(s) = self.session.as_ref() else { return };
        let Some(st) = s.streams.get(index).cloned() else { return };
        if s.stage != Stage::Playing || index == s.cur || self.audio_sync.dub_busy {
            return;
        }
        let token = s.token;
        // a search running for this episode is overtaken (its answer is dropped as too late)
        self.audio_sync.dub_token = token;
        self.audio_sync.dub_manual = true;
        crate::debug::log("player", &format!("dub: picked by hand: {}", dub_label(&st)));
        self.dub_start(token, vec![st]);
    }

    /// Runs on each tick: once per episode, after it started playing, looks for a dub when it has none.
    pub(super) fn dub_step(&mut self) {
        if self.ext_audio.no_auto_dub || self.audio_sync.dub_busy {
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
        let Some(m) = self.mpv.clone() else { return };
        let token = s.token;
        self.audio_sync.dub_token = token;
        self.audio_sync.dub_note.clear();
        self.audio_sync.dub_manual = false;
        let lang = self.dub_lang();
        let tracks = m.tracks("audio");
        let cur = s.streams.get(s.cur);
        let doubtful = cur.map(|x| tag_doubtful(x, &lang)).unwrap_or(false);
        let tagged = tracks.iter().any(|t| !t.external && !t.lang.is_empty() && util::lang_base(&t.lang) == lang);
        let has_it = (tagged && !doubtful) || tracks.iter().any(|t| t.external);
        let cur_is_dub = cur.map(|x| dub_rank(x, &lang) == 2).unwrap_or(false);
        let cands = dub_candidates(&s.streams, s.cur, &lang);
        if lang.is_empty() || has_it || cur_is_dub {
            crate::debug::log(
                "player",
                &format!("dub: not looked for (language '{}', tagged: {}, doubtful tag: {}, name says dub: {}, {} listed in the menu)", lang, tagged, doubtful, cur_is_dub, cands.len()),
            );
            if tagged && !cands.is_empty() {
                self.audio_sync.dub_note = "This video's audio says it is already in your language. Other torrents with a dub:".into();
            }
            return;
        }
        if cands.is_empty() {
            crate::debug::log("player", &format!("dub: no other torrent of this episode says it has '{}' audio ({} streams)", lang, s.streams.len()));
            self.audio_sync.dub_note = "No dub found in the other torrents of this episode.".into();
            return;
        }
        if tagged {
            crate::debug::log("player", &format!("dub: the video's audio is tagged '{}' but its name says otherwise; looking anyway", lang));
        }
        let tries: Vec<Stream> = cands.iter().take(2).map(|&i| s.streams[i].clone()).collect();
        self.dub_start(token, tries);
    }

    /// Opens the first of `tries` that works in the torrent engine (or by its address) and adds its sound.
    fn dub_start(&mut self, token: u64, tries: Vec<Stream>) {
        let Some(s) = self.session.as_ref() else { return };
        let sid = s.sid.clone();
        let cancel = s.cancel.clone();
        let st = self.settings.clone();
        let engine = self.engine.clone();
        let exe = self.rt.rqbit.clone();
        let rt_dir = self.rt.dir.clone();
        let dl = self.data_dir.join("torrents");
        self.audio_sync.dub_busy = true;
        self.audio_sync.dub_note.clear();
        self.events.push(Event::Session);
        crate::debug::log("player", &format!("dub: trying {:?}", tries.iter().map(dub_label).collect::<Vec<_>>()));
        self.spawn(move |tx, http| {
            let cancelled = || cancel.load(Ordering::SeqCst);
            let status = |m: String| crate::debug::log("player", &format!("dub: {}", m));
            let mut last = String::from("no stream");
            for stream in &tries {
                if cancelled() {
                    return;
                }
                match resolve_stream(&*http, &st, stream, &engine, exe.as_deref(), &rt_dir, &dl, &sid, &status, &cancelled) {
                    Ok(res) => {
                        let label = stream.name.lines().next().unwrap_or("").to_string();
                        send(tx, Msg::DubFound(token, label, Ok(res)));
                        return;
                    }
                    Err(e) => {
                        crate::debug::log("player", &format!("dub: {} failed: {}", stream.name.replace('\n', " "), e));
                        last = e;
                    }
                }
            }
            send(tx, Msg::DubFound(token, String::new(), Err(last)));
        });
    }

    pub(super) fn on_dub_found(&mut self, token: u64, label: String, r: Result<Resolved, String>) {
        self.audio_sync.dub_busy = false;
        self.events.push(Event::Session);
        let current = self.session.as_ref().map(|s| s.token == token).unwrap_or(false);
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
        if let Some((base, id, _)) = &res.torrent {
            // kept to be deleted when the episode ends (also when this answer came too late)
            self.dub_drop();
            self.audio_sync.dub_torrent = Some((base.clone(), *id));
        }
        let (Some(m), Some(s), true) = (self.mpv.clone(), self.session.as_ref(), current) else {
            self.dub_drop();
            return;
        };
        let sid = s.sid.clone();
        if let Err(e) = m.cmd(&["audio-add", &res.url, "auto"]) {
            crate::debug::log("player", &format!("dub: audio-add failed: {}", e));
            self.audio_sync.dub_note = "A dub was listed, but its torrent could not be opened.".into();
            return;
        }
        let lang = self.dub_lang();
        let tracks = m.tracks("audio");
        let own = tracks.iter().find(|t| !t.external && t.selected).or_else(|| tracks.iter().find(|t| !t.external)).map(|t| t.lang.clone()).unwrap_or_default();
        let of_file: Vec<(String, String)> = tracks.iter().filter(|t| t.external && controller_audio::same_file(&t.file, &res.url)).map(|t| (t.lang.clone(), t.title.clone())).collect();
        let nth = pick_dub_track(&of_file, &lang, &own);
        crate::debug::log("player", &format!("dub: added from '{}' ({} audio tracks in it, using #{})", label, of_file.len(), nth));
        let rec = ExtAudio { active: false, file_aid: nth, remote: true, ..ExtAudio::new(&sid, &res.url) };
        self.ext_audio.tracks.push(rec.clone());
        self.audio_sync.dub_note = "Dub found in another torrent:".into();
        let Some(id) = file_track_id(&m, &res.url, nth) else { return };
        if self.audio_sync.dub_manual {
            // picked by hand: heard at once, the sync only fixes its delay
            self.audio_select(id);
            self.toast("Dub added. Syncing it with the video…");
        } else {
            self.audio_sync.select_after = Some(res.url.clone());
            self.toast("Dub found in another torrent. Syncing it with the video…");
        }
        self.audio_sync_rec(rec, true);
    }

    /// Deletes the torrent a dub was streamed from (the episode ended or another one came).
    pub(super) fn dub_drop(&mut self) {
        self.ext_audio.tracks.retain(|t| !t.remote);
        if let Some((base, id)) = self.audio_sync.dub_torrent.take() {
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
    fn picks_the_dub_track_of_a_dual_audio_file() {
        let t = |l: &str, n: &str| (l.to_string(), n.to_string());
        assert_eq!(pick_dub_track(&[t("jpn", ""), t("por", "")], "pt", "jpn"), 2);
        assert_eq!(pick_dub_track(&[t("", "Japonês"), t("", "Português BR")], "pt", ""), 2);
        assert_eq!(pick_dub_track(&[t("eng", ""), t("und", "")], "pt", "eng"), 2);
        assert_eq!(pick_dub_track(&[t("", "")], "pt", "jpn"), 1);
    }
}
