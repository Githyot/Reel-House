//! Translating a subtitle on demand ("Translation" tab of the player's Subtitles menu). The result is kept for
//! the episode and shows up again, as "Forced translation - PT", whenever it is played.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::subtitles::{decode, parse_cues, Cue};

/// Where the text to translate comes from.
#[derive(Clone, Debug)]
pub enum SubSource {
    /// One of the subtitles the addons offered (index into the session's list).
    Candidate(usize),
    /// A subtitle file already loaded in the player: its path and language.
    File(String, String),
    /// A subtitle track built into the video (mpv's track id): read out of the MKV and translated.
    Embedded(i64),
}

/// Where a built-in subtitle track is: the video's address and how to find the track inside it.
struct EmbeddedAt {
    url: String,
    want: crate::mkvsubs::Want,
    codec: String,
}


#[derive(Clone, Debug)]
pub struct Forced {
    pub sid: String,
    pub lang: String,
    pub path: String,
    pub title: String,
}

pub fn load_forced(dir: &Path) -> Vec<Forced> {
    let Ok(text) = std::fs::read_to_string(dir.join("forced_subs.json")) else { return vec![] };
    let Ok(j) = crate::json::Json::parse(&text) else { return vec![] };
    j.items()
        .iter()
        .map(|x| Forced { sid: x.get("sid").str(), lang: x.get("lang").str(), path: x.get("path").str(), title: x.get("title").str() })
        .filter(|f| !f.sid.is_empty() && Path::new(&f.path).exists())
        .collect()
}

// ---- Choosing subtitles from the languages the user knows (Preferences > General > Languages I know) ----
//
// With a list of known languages (best first), a subtitle in any of them is used as it is, in that order of
// preference; a track built into the video wins over a download in the same language (its timing fits). Only
// when nothing in a known language exists is the best subtitle translated into the main (first) language.
// An empty list keeps the old behaviour: the subtitle target language only.

/// The known languages as 2-letter codes, without blanks or repeats, in the user's order.
pub fn known_list(known: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in known {
        let b = util::lang_base(l);
        if !b.is_empty() && !out.contains(&b) {
            out.push(b);
        }
    }
    out
}

/// Place of a language in the known list (0 = main), None when the user does not know it.
pub fn known_rank(known: &[String], lang: &str) -> Option<usize> {
    let b = util::lang_base(lang);
    if b.is_empty() {
        return None;
    }
    known.iter().position(|k| *k == b)
}

/// One attempt of the subtitle download job: the candidates to try, the language to end in (a candidate in
/// another language is translated into it) and the source languages to prefer among those that need translating.
#[derive(Clone, Debug)]
pub struct SubStep {
    pub target: String,
    pub cands: Vec<SubCand>,
    pub preferred: Vec<String>,
}

/// The attempts, in order, for the addon subtitles. `fallback_target` and `source_langs` are the old settings
/// (subtitle target language and preferred sources), used as before when `known` is empty.
pub fn sub_plan(known: &[String], cands: &[SubCand], fallback_target: &str, source_langs: &[String]) -> Vec<SubStep> {
    let known = known_list(known);
    if known.is_empty() {
        return vec![SubStep { target: util::lang_base(fallback_target), cands: cands.to_vec(), preferred: source_langs.to_vec() }];
    }
    let mut plan = Vec::new();
    // Every known language that has a subtitle, in preference order: used as it is.
    for k in &known {
        let same: Vec<SubCand> = cands.iter().filter(|c| !c.url.is_empty() && util::lang_base(&c.lang) == *k).cloned().collect();
        if !same.is_empty() {
            plan.push(SubStep { target: k.clone(), cands: same, preferred: vec![] });
        }
    }
    // Nothing usable in a known language (or every one failed to download): the best one, translated into the main language.
    plan.push(SubStep { target: known[0].clone(), cands: cands.to_vec(), preferred: source_langs.to_vec() });
    plan
}

/// The addon subtitle to load: the steps of `sub_plan` tried in order, until one gives a file.
#[allow(clippy::too_many_arguments)]
pub fn prepare_known(
    http: &dyn crate::http::Http,
    known: &[String],
    cands: &[SubCand],
    fallback_target: &str,
    source_langs: &[String],
    translator: Option<&dyn translate::Translator>,
    cache: &Path,
    status: &(dyn Fn(String) + Sync),
) -> Result<Option<subtitles::SubFile>, String> {
    let mut last: Result<Option<subtitles::SubFile>, String> = Ok(None);
    for step in sub_plan(known, cands, fallback_target, source_langs) {
        let wanted = Wanted { candidates: &step.cands, target: &step.target, preferred: &step.preferred };
        match subtitles::prepare(http, &wanted, translator, cache, status) {
            Ok(Some(f)) => return Ok(Some(f)),
            // a known language whose files are all broken: the next language (or the translation) is tried
            other => last = other,
        }
    }
    last
}

/// What to do with the subtitle tracks built into the video.
#[derive(Clone, Debug, PartialEq)]
pub enum EmbeddedPick {
    /// Select this track (mpv track id) and drop the download.
    Use(i64),
    /// Keep the downloaded subtitle (or none): no built-in track is better.
    Skip,
    /// A built-in track in a secondary language exists, but the addons have not answered yet: ask again later.
    Wait,
}

/// Picks among the built-in tracks `(id, language)`. `cands` are the addon subtitles, None while still searching.
pub fn pick_embedded(known: &[String], tracks: &[(i64, String)], cands: Option<&[SubCand]>) -> EmbeddedPick {
    let known = known_list(known);
    let best = tracks.iter().filter_map(|(id, l)| known_rank(&known, l).map(|r| (r, *id))).min();
    let Some((rank, id)) = best else { return EmbeddedPick::Skip };
    if rank == 0 {
        return EmbeddedPick::Use(id);
    }
    let Some(cands) = cands else { return EmbeddedPick::Wait };
    let addon = cands.iter().filter(|c| !c.url.is_empty()).filter_map(|c| known_rank(&known, &c.lang)).min();
    match addon {
        Some(a) if a < rank => EmbeddedPick::Skip,
        _ => EmbeddedPick::Use(id),
    }
}

/// Reads a built-in subtitle track out of the video into an SRT file in `cache` (kept: translating the same
/// track into another language does not read the video again). `progress(0..1)` follows the bytes read.
fn read_embedded(e: &EmbeddedAt, cache: &Path, cancel: &AtomicBool, progress: &dyn Fn(f32)) -> Result<PathBuf, String> {
    if crate::mkvsubs::is_picture_codec(&e.codec) {
        return Err("This subtitle track is made of pictures (PGS/VobSub), not text, so it cannot be translated. Pick a text subtitle or one from the addons.".to_string());
    }
    let key = format!("{}|{:?}|{}", e.url, e.want.src_id, e.want.nth);
    let out = cache.join(format!("embedded-{}.srt", util::fnv_hex(&key)));
    if out.exists() {
        return Ok(out);
    }
    let (mut reader, total): (Box<dyn std::io::Read>, Option<u64>) = if e.url.starts_with("http://") || e.url.starts_with("https://") {
        let resp = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(15))
            .timeout_read(std::time::Duration::from_secs(120))
            .build()
            .get(&e.url)
            .call()
            .map_err(|x| format!("Could not open the video to read its subtitles ({}).", x))?;
        let total = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok());
        (Box::new(std::io::BufReader::with_capacity(1 << 20, resp.into_reader())), total)
    } else {
        let p = e.url.strip_prefix("file://").unwrap_or(&e.url);
        let f = std::fs::File::open(p).map_err(|x| format!("Could not open the video to read its subtitles ({}).", x))?;
        let total = f.metadata().ok().map(|m| m.len());
        (Box::new(std::io::BufReader::with_capacity(1 << 20, f)), total)
    };
    let t0 = std::time::Instant::now();
    let mut report = |bytes: u64| {
        if let Some(t) = total.filter(|t| *t > 0) {
            progress((bytes as f32 / t as f32).min(1.0));
        }
    };
    let (track, cues) = crate::mkvsubs::extract(&mut *reader, &e.want, &mut report, cancel)?;
    crate::debug::log("subs", &format!("built-in track {} ({}, {}) read: {} line(s) in {} ms", track.number, track.codec, track.lang, cues.len(), t0.elapsed().as_millis()));
    std::fs::create_dir_all(cache).map_err(|x| x.to_string())?;
    std::fs::write(&out, subtitles::cues_to_srt(&cues)).map_err(|x| x.to_string())?;
    Ok(out)
}

fn save_forced(dir: &Path, list: &[Forced]) {
    use crate::json::Json;
    let arr = Json::Arr(
        list.iter()
            .map(|f| Json::obj(vec![("sid", Json::Str(f.sid.clone())), ("lang", Json::Str(f.lang.clone())), ("path", Json::Str(f.path.clone())), ("title", Json::Str(f.title.clone()))]))
            .collect(),
    );
    let _ = std::fs::write(dir.join("forced_subs.json"), arr.dump());
}

impl Core {
    /// "Forced translation - PT", in the language of the interface.
    pub fn forced_title(&self, lang: &str) -> String {
        let ui = match self.ui_lang().as_str() {
            "system" => util::lang_base(&self.system_lang),
            other => util::lang_base(other),
        };
        let code = util::lang_base(lang).to_uppercase();
        match ui.as_str() {
            "pt" => format!("Tradução forçada - {}", code),
            "es" => format!("Traducción forzada - {}", code),
            _ => format!("Forced translation - {}", code),
        }
    }

    /// Translates a subtitle into `target` and loads it. Keeps it for this episode.
    pub fn translate_subtitle(&mut self, source: SubSource, target: &str) {
        let Some(s) = self.session.as_ref() else { return };
        let token = s.token;
        let sid = s.sid.clone();
        let target = util::lang_base(target);
        if target.is_empty() {
            return;
        }
        let (url, src_lang) = match &source {
            SubSource::Candidate(i) => match s.sub_cands.get(*i) {
                Some(c) => (Some(c.url.clone()), c.lang.clone()),
                None => return,
            },
            SubSource::File(_, lang) => (None, lang.clone()),
            SubSource::Embedded(id) => (None, self.mpv.as_ref().and_then(|m| m.tracks("sub").into_iter().find(|t| t.id == *id)).map(|t| t.lang).unwrap_or_default()),
        };
        let embedded = match &source {
            SubSource::Embedded(id) => match self.embedded_at(*id) {
                Some(e) => Some(e),
                None => {
                    self.translate_note = Some((false, "The subtitle track was not found in the player.".to_string()));
                    self.events.push(Event::Session);
                    return;
                }
            },
            _ => None,
        };
        let cache = self.data_dir.join("subtitles");
        let out_dir = self.data_dir.join("forced");
        // the translator of "Forced translation" in Preferences > Translators (works even with automatic subtitles off)
        let st = self.settings.clone();
        let gate = self.local_ai_gate(translate::uses_local_ai("forced", &st));
        let title = self.forced_title(&target);
        if self.translating.is_some() {
            self.toast("A translation is already running.");
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.translating = Some(TransJob { lang: target.clone(), pct: 0, cancel: cancel.clone() });
        self.translate_note = None;
        self.spawn(move |tx, http| {
            let r = (|| -> Result<Forced, String> {
                // reading a built-in track takes the first part of the progress bar
                let base = if embedded.is_some() { 30.0 } else { 0.0 };
                let path = match (&source, &url) {
                    (SubSource::Candidate(_), Some(u)) => subtitles::fetch_srt(&*http, u, &cache)?,
                    (SubSource::File(p, _), _) => PathBuf::from(p),
                    (SubSource::Embedded(_), _) => match &embedded {
                        Some(e) => read_embedded(e, &cache, &cancel, &|p| send(tx, Msg::ForcedProgress(((p * base) as u8).min(29))))?,
                        None => return Err("Nothing to translate.".to_string()),
                    },
                    _ => return Err("Nothing to translate.".to_string()),
                };
                let bytes = std::fs::read(&path).map_err(|e| format!("Could not read the subtitle: {}", e))?;
                let cues = parse_cues(&decode(&bytes));
                if cues.is_empty() {
                    return Err("This subtitle has no lines Reelhouse can read (only SRT-style files can be translated).".to_string());
                }
                let translator = controller_translators::gated(translate::for_use("forced", &st, &*http)?, &gate, None);
                let texts: Vec<String> = cues.iter().map(|c| c.text.clone()).collect();
                let last = std::sync::atomic::AtomicU32::new(0);
                let progress = |p: f32| {
                    let pct = ((base + p * (100.0 - base)) as u32).min(99);
                    if pct > last.swap(pct, std::sync::atomic::Ordering::SeqCst) {
                        send(tx, Msg::ForcedProgress(pct as u8));
                    }
                };
                let from = if util::lang_base(&src_lang).is_empty() { "auto".to_string() } else { util::lang_base(&src_lang) };
                // the local AI answers one request at a time anyway: smaller batches show the progress sooner
                let (batch, threads) = if gate.is_some() { (translate::LOCAL_MAX_ITEMS, 1) } else { (40, 3) };
                let done = translate::translate_all_with(translator.as_ref(), &texts, &from, &target, batch, threads, &progress, &cancel)?;
                let out: Vec<Cue> = cues.into_iter().zip(done).map(|(c, t)| Cue { text: t, ..c }).collect();
                std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
                let file = out_dir.join(format!("{}.{}.srt", util::fnv_hex(&format!("{}|{}|{}", sid, target, path.display())), target));
                std::fs::write(&file, subtitles::cues_to_srt(&out)).map_err(|e| e.to_string())?;
                Ok(Forced { sid: sid.clone(), lang: target.clone(), path: file.to_string_lossy().into_owned(), title: title.clone() })
            })();
            send(tx, Msg::Forced(token, r));
        });
    }

    /// The address of the video playing and how to find the built-in subtitle track `id` in it.
    fn embedded_at(&self, id: i64) -> Option<EmbeddedAt> {
        let m = self.mpv.as_ref()?;
        let count = m.get_i64("track-list/count").unwrap_or(0);
        let mut nth = 0usize;
        for i in 0..count {
            if m.get_str(&format!("track-list/{}/type", i)).as_deref() != Some("sub") {
                continue;
            }
            if m.get_flag(&format!("track-list/{}/external", i)).unwrap_or(false) {
                continue;
            }
            if m.get_i64(&format!("track-list/{}/id", i)) == Some(id) {
                let src_id = m.get_i64(&format!("track-list/{}/src-id", i)).filter(|x| *x > 0).map(|x| x as u64);
                let codec = m.get_str(&format!("track-list/{}/codec", i)).unwrap_or_default();
                let url = m.get_str("stream-open-filename").filter(|x| !x.is_empty()).or_else(|| m.get_str("path"))?;
                return Some(EmbeddedAt { url, want: crate::mkvsubs::Want { src_id, nth }, codec });
            }
            nth += 1;
        }
        None
    }

    /// Stops the translation that is running (the Cancel button).
    pub fn cancel_translation(&mut self) {
        if let Some(t) = self.translating.take() {
            t.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub(super) fn on_forced(&mut self, token: u64, r: Result<Forced, String>) {
        // A cancelled job already cleared its state; its late answer is dropped.
        if matches!(&r, Err(e) if e == "Cancelled.") || self.translating.take().is_none() {
            return;
        }
        match r {
            Err(e) => {
                self.translate_note = Some((false, e.clone()));
                self.toast(&e);
            }
            Ok(f) => {
                // One forced translation per episode and language.
                self.forced.retain(|x| !(x.sid == f.sid && x.lang == f.lang));
                self.forced.push(f.clone());
                save_forced(&self.data_dir, &self.forced);
                let current = self.session.as_ref().map(|s| s.token == token).unwrap_or(false);
                if current {
                    if let Some(m) = &self.mpv {
                        let _ = m.cmd(&["sub-add", &f.path, "select", &f.title, &f.lang]);
                    }
                }
                self.translate_note = Some((true, f.title.clone()));
                self.toast(&format!("{} is ready.", f.title));
            }
        }
    }

    /// Whether a forced translation exists for the episode on screen (for the menu).
    pub fn forced_for_session(&self) -> Vec<Forced> {
        let Some(s) = self.session.as_ref() else { return vec![] };
        self.forced.iter().filter(|f| f.sid == s.sid).cloned().collect()
    }

    pub(super) fn reload_forced(&mut self) {
        self.forced = load_forced(&self.data_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(lang: &str, url: &str) -> SubCand {
        SubCand { lang: lang.into(), url: url.into(), addon: "test".into() }
    }
    fn v(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_known_keeps_the_old_plan() {
        let c = vec![cand("eng", "a"), cand("por", "b")];
        let p = sub_plan(&[], &c, "pt-BR", &v(&["en"]));
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].target, "pt");
        assert_eq!(p[0].cands.len(), 2);
        assert_eq!(p[0].preferred, v(&["en"]));
    }

    #[test]
    fn known_languages_in_preference_order_without_translation() {
        let c = vec![cand("jpn", "j"), cand("spa", "s"), cand("eng", "e1"), cand("en", "e2")];
        let p = sub_plan(&v(&["pt", "en", "es"]), &c, "pt", &v(&["ja"]));
        // no Portuguese: English first (both files), then Spanish, then the translation into Portuguese
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].target, "en");
        assert_eq!(p[0].cands.iter().map(|c| c.url.as_str()).collect::<Vec<_>>(), vec!["e1", "e2"]);
        assert!(p[0].preferred.is_empty());
        assert_eq!(p[1].target, "es");
        assert_eq!(p[2].target, "pt");
        assert_eq!(p[2].cands.len(), 4);
        assert_eq!(p[2].preferred, v(&["ja"]));
    }

    #[test]
    fn nothing_known_translates_into_the_main_language() {
        let c = vec![cand("jpn", "j"), cand("fre", "")];
        let p = sub_plan(&v(&["pt-BR", "pt", "en"]), &c, "es", &[]);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].target, "pt");
        assert_eq!(known_list(&v(&["pt-BR", "pt", "", "EN"])), v(&["pt", "en"]));
    }

    #[test]
    fn known_languages_setting_round_trip() {
        let mut s = crate::settings::Settings::default();
        assert!(s.general.known_langs.is_empty());
        s.general.known_langs = v(&["pt-BR", "en"]);
        s.general.study_lang = "jpn".into();
        let back = crate::settings::Settings::from_json(&s.to_json());
        assert_eq!(back.general.known_langs, v(&["pt", "en"]));
        assert_eq!(back.general.study_lang, "ja");
    }

    #[test]
    fn built_in_track_choice() {
        let known = v(&["pt", "en"]);
        let tracks = vec![(1, "jpn".to_string()), (2, "eng".to_string()), (3, "por".to_string())];
        // the main language built in: always taken
        assert_eq!(pick_embedded(&known, &tracks, None), EmbeddedPick::Use(3));
        let tracks = vec![(1, "jpn".to_string()), (2, "eng".to_string())];
        // English built in, addons still searching: wait, they may have Portuguese
        assert_eq!(pick_embedded(&known, &tracks, None), EmbeddedPick::Wait);
        // the addons have Portuguese: the download wins
        assert_eq!(pick_embedded(&known, &tracks, Some(&[cand("pt", "x")])), EmbeddedPick::Skip);
        // the addons have English too (same language): the built-in track wins
        assert_eq!(pick_embedded(&known, &tracks, Some(&[cand("en", "x"), cand("ja", "y")])), EmbeddedPick::Use(2));
        // nothing known built in: keep whatever the download gives
        assert_eq!(pick_embedded(&known, &[(1, "jpn".to_string())], None), EmbeddedPick::Skip);
    }
}
