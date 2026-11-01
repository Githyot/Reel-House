//! The engine ties everything together. The app crate only forwards UI calls here.
//!
//! Playback: mpv starts paused, the video buffers, Anime4K is applied, a subtitle is found and
//! translated, and only then does playback start. A Lua script in mpv adds a Quality button that
//! sends `rh-quality-pick` back here, which switches to another stream at the same position.

use crate::addons::{AddonManager, DEFAULT_ADDONS};
use crate::anime4k::{self, ShaderManager, ShaderStatus};
use crate::art;
use crate::error::{Error, Result};
use crate::mpv::{Mpv, MpvEvent};
use crate::realdebrid::{RdClient, RdUser};
use crate::settings::{Settings, SettingsStore};
use crate::streams::{self, Stream, StreamView};
use crate::subtitles::{self, lang_base, PreparedSub};
use crate::translators;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;

/// Sends an event to the UI: (channel, payload).
pub type Notify = Arc<dyn Fn(&str, Value) + Send + Sync>;

pub struct EngineConfig {
    pub data_dir: PathBuf,
    /// The mpv that ships inside the app, if present.
    pub bundled_mpv: Option<PathBuf>,
    /// The system language, for example "pt-BR".
    pub locale: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PlayRequest {
    pub stream: Stream,
    pub kind: String,
    pub id: String,
    pub title: String,
}

struct Resolved {
    url: String,
    headers: Option<std::collections::HashMap<String, String>>,
}

enum SubOutcome {
    None,
    Ready(PreparedSub),
    Failed(String),
}

pub struct Engine {
    pub settings: Arc<SettingsStore>,
    pub addons: AddonManager,
    pub shaders: ShaderManager,
    http: reqwest::Client,
    cfg: EngineConfig,
    locale: Mutex<String>,
    notify: Notify,
    generation: AtomicU64,
    session: AsyncMutex<Option<Arc<Mpv>>>,
    last_streams: Mutex<(String, Vec<Stream>)>,
}

const LUA: &str = include_str!("../assets/reelhouse.lua");

impl Engine {
    pub fn new(cfg: EngineConfig, notify: Notify) -> Arc<Engine> {
        let settings = SettingsStore::load(&cfg.data_dir);
        let http = reqwest::Client::builder().user_agent("Reelhouse/0.2").build().expect("http client");
        Arc::new(Engine {
            addons: AddonManager::new(settings.clone(), http.clone()),
            shaders: ShaderManager::new(&cfg.data_dir),
            settings,
            http,
            locale: Mutex::new(cfg.locale.clone()),
            cfg,
            notify,
            generation: AtomicU64::new(0),
            session: AsyncMutex::new(None),
            last_streams: Mutex::new((String::new(), Vec::new())),
        })
    }

    /// Call once at start: installs Cinemeta and the MAL addon the first time the app runs.
    pub async fn startup(&self) {
        self.addons.ensure_defaults(DEFAULT_ADDONS).await;
    }

    /// The UI reports the system language (navigator.language), which is reliable on every platform.
    pub fn set_locale(&self, locale: &str) {
        if !locale.trim().is_empty() {
            *self.locale.lock().unwrap() = locale.trim().to_string();
        }
    }

    fn emit(&self, channel: &str, payload: Value) {
        (self.notify)(channel, payload);
    }

    fn status(&self, stage: &str, message: impl Into<String>, progress: Option<f32>) {
        self.emit("play-status", json!({"stage": stage, "message": message.into(), "progress": progress}));
    }

    fn alive(&self, token: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == token
    }

    /* ---------- browsing ---------- */

    /// Metadata from Cinemeta (or another meta addon). Artwork that is missing is filled from TMDB when a key is set.
    pub async fn meta(&self, kind: &str, id: &str) -> Option<Value> {
        let mut m = self.addons.meta(kind, id).await?;
        let key = self.settings.get().accounts.tmdb_api_key;
        if !key.trim().is_empty() && art::missing(&m) {
            if let Some(a) = art::tmdb_art(&self.http, art::TMDB_API, &key, id.split(':').next().unwrap_or(id), kind).await {
                art::merge(&mut m, &a);
            }
        }
        Some(m)
    }

    pub async fn streams(&self, kind: &str, id: &str) -> Vec<StreamView> {
        let mut list = self.addons.streams(kind, id).await;
        streams::rank(&mut list);
        *self.last_streams.lock().unwrap() = (format!("{kind}/{id}"), list.clone());
        list.iter().map(Stream::view).collect()
    }

    pub async fn verify_real_debrid(&self) -> Result<RdUser> {
        let token = self.settings.get().accounts.real_debrid_token;
        if token.trim().is_empty() {
            return Err(Error::msg("Paste your Real-Debrid API token first."));
        }
        RdClient::new(self.http.clone(), &token).user().await
    }

    pub async fn test_translator(&self) -> Result<Value> {
        let s = self.settings.get();
        let t = translators::from_settings(&s.subtitles, &self.http)
            .ok_or_else(|| Error::msg("No translator is set up. Choose a provider and fill in its details first."))?;
        let target = self.target_lang(&s);
        let out = t.translate(&["Where are you going?".to_string()], "en", &target).await?;
        Ok(json!({"provider": t.name(), "target": target, "sample": out.first().cloned().unwrap_or_default()}))
    }

    pub async fn install_shaders(&self) -> Result<ShaderStatus> {
        let notify = self.notify.clone();
        self.shaders.install(&self.http, &move |m| notify("shaders-progress", json!({"message": m}))).await
    }

    fn target_lang(&self, s: &Settings) -> String {
        let locale = self.locale.lock().unwrap().clone();
        let t = if s.subtitles.target_lang.trim().is_empty() { locale.as_str() } else { s.subtitles.target_lang.as_str() };
        let b = lang_base(t);
        if b.is_empty() { "en".into() } else { b }
    }

    /* ---------- playback ---------- */

    fn mpv_binary(&self, s: &Settings) -> String {
        if !s.general.mpv_path.trim().is_empty() {
            return s.general.mpv_path.trim().to_string();
        }
        match &self.cfg.bundled_mpv {
            Some(p) if p.exists() => p.to_string_lossy().into_owned(),
            _ => "mpv".to_string(),
        }
    }

    async fn resolve(&self, s: &Stream) -> Result<Resolved> {
        if let Some(url) = &s.url {
            let headers = s.behavior_hints.as_ref().and_then(|b| b.proxy_headers.as_ref()).and_then(|p| p.request.clone());
            return Ok(Resolved { url: url.clone(), headers });
        }
        if s.info_hash.is_some() {
            let token = self.settings.get().accounts.real_debrid_token;
            if token.trim().is_empty() {
                return Err(Error::msg("This stream is a torrent. Add your Real-Debrid API token in Settings > Accounts to play it, or pick a direct stream."));
            }
            let magnet = streams::build_magnet(s).ok_or_else(|| Error::msg("The stream has no info hash."))?;
            let rd = RdClient::new(self.http.clone(), &token);
            let url = rd
                .resolve(&magnet, s.file_idx, Duration::from_secs(120), &|p| self.status("resolving", p.message, p.fraction))
                .await?;
            return Ok(Resolved { url, headers: None });
        }
        if s.yt_id.is_some() {
            return Err(Error::msg("YouTube streams are not supported yet."));
        }
        if s.external_url.is_some() {
            return Err(Error::msg("This stream opens in a web browser and cannot be played in mpv."));
        }
        Err(Error::msg("This stream has no playable source."))
    }

    /// Starts playback in the background. Progress and errors arrive as `play-status` events.
    pub async fn play(self: &Arc<Self>, req: PlayRequest) {
        let token = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.quit_session().await;
        let me = self.clone();
        tokio::spawn(async move {
            if let Err(e) = me.run(req, token).await {
                if me.alive(token) {
                    me.status("error", e.to_string(), None);
                }
                me.quit_session().await;
            }
        });
    }

    pub async fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.quit_session().await;
        self.status("closed", "Stopped", None);
    }

    async fn quit_session(&self) {
        let m = self.session.lock().await.take();
        if let Some(m) = m {
            m.quit().await;
        }
    }

    async fn run(self: &Arc<Self>, req: PlayRequest, token: u64) -> Result<()> {
        let cfg = self.settings.get();
        self.status("resolving", "Finding the video source", None);
        let resolved = self.resolve(&req.stream).await?;
        if !self.alive(token) {
            return Ok(());
        }

        // mpv starts paused so the cache fills while subtitles and upscaling are prepared.
        let script = self.cfg.data_dir.join("reelhouse.lua");
        std::fs::create_dir_all(&self.cfg.data_dir)?;
        std::fs::write(&script, LUA)?;
        let mut args: Vec<String> = vec![
            "--force-window=immediate".into(),
            "--pause=yes".into(),
            "--keep-open=yes".into(),
            "--cache=yes".into(),
            format!("--demuxer-readahead-secs={}", (cfg.general.buffer_seconds * 2).max(30)),
            "--demuxer-max-bytes=400MiB".into(),
            "--sid=no".into(),
            format!("--hwdec={}", cfg.general.hwdec),
            format!("--force-media-title={}", req.title),
            format!("--script={}", script.to_string_lossy()),
        ];
        let shaders_ready = cfg.upscaling.enabled && self.shaders.missing_for(&cfg.upscaling).is_empty();
        if cfg.upscaling.enabled {
            args.extend(anime4k::mpv_args(&cfg.upscaling));
        }
        if shaders_ready {
            args.push(format!("--input-conf={}", self.shaders.write_input_conf(&cfg.upscaling)?.to_string_lossy()));
        }
        if let Some(h) = &resolved.headers {
            args.push(format!("--http-header-fields={}", h.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(",")));
        }
        args.push(resolved.url.clone());

        self.status("buffering", "Opening the player", None);
        let mpv = Mpv::start(&self.mpv_binary(&cfg), &args).await?;
        *self.session.lock().await = Some(mpv.clone());
        if !self.alive(token) {
            mpv.quit().await;
            return Ok(());
        }
        {
            let me = self.clone();
            let mut rx = mpv.subscribe();
            tokio::spawn(async move {
                while let Ok(ev) = rx.recv().await {
                    if matches!(ev, MpvEvent::Exited) {
                        if me.alive(token) {
                            me.status("closed", "Playback ended", None);
                        }
                        break;
                    }
                }
            });
        }
        mpv.observe(&["track-list", "height", "cache-buffering-state", "demuxer-cache-duration", "time-pos"]).await?;
        mpv.wait_for("the video to load", Duration::from_secs(60), |p| {
            p.get("track-list").and_then(|t| t.as_array()).map_or(false, |t| t.iter().any(|x| x["type"] == "video"))
        })
        .await?;

        self.apply_upscaling(&mpv, &cfg).await;

        // Subtitles are prepared while the video buffers.
        let sub_job = {
            let me = self.clone();
            let mpv = mpv.clone();
            let (kind, id) = (req.kind.clone(), req.id.clone());
            tokio::spawn(async move { me.prepare_subtitles(&mpv, &kind, &id, token).await })
        };

        self.status("buffering", "Buffering", None);
        let wanted = cfg.general.buffer_seconds as f64;
        let _ = mpv
            .wait_for("the buffer", Duration::from_secs(180), |p| {
                p.get("cache-buffering-state").and_then(|v| v.as_f64()) == Some(100.0)
                    || p.get("demuxer-cache-duration").and_then(|v| v.as_f64()).unwrap_or(0.0) >= wanted
            })
            .await;
        if !self.alive(token) {
            return Ok(());
        }

        // Subtitles must be ready before playback starts, within the wait limit.
        let outcome = match tokio::time::timeout(Duration::from_secs(cfg.subtitles.max_wait_seconds), sub_job).await {
            Ok(Ok(o)) => o,
            Ok(Err(_)) => SubOutcome::Failed("the subtitle task stopped unexpectedly".into()),
            Err(_) => SubOutcome::Failed("Subtitles took too long to translate, so playback started without them.".into()),
        };
        if !self.alive(token) {
            return Ok(());
        }
        let mut note = String::new();
        let mut sub_file: Option<(PathBuf, String, bool)> = None;
        match outcome {
            SubOutcome::Ready(sub) => {
                let title = if sub.translated { format!("{} (translated)", sub.lang) } else { sub.lang.clone() };
                mpv.command(vec![json!("sub-add"), json!(sub.file.to_string_lossy()), json!("select"), json!(title), json!(sub.lang)]).await?;
                if sub.untranslated {
                    note = "Subtitles are in their original language because no translator is set up.".into();
                }
                sub_file = Some((sub.file, title, sub.untranslated));
            }
            SubOutcome::Failed(m) => note = if m.starts_with("Subtitles took") { m } else { format!("Subtitles unavailable: {m}") },
            SubOutcome::None => {}
        }

        mpv.set_property("pause", json!(false)).await?;
        self.status("ready", if note.is_empty() { "Playing in mpv".to_string() } else { note.clone() }, None);

        self.start_quality_menu(&mpv, &req, token, sub_file).await;
        Ok(())
    }

    async fn apply_upscaling(&self, mpv: &Mpv, cfg: &Settings) {
        let u = &cfg.upscaling;
        if !u.enabled {
            return;
        }
        let missing = self.shaders.missing_for(u);
        if !missing.is_empty() {
            self.status("buffering", "Anime4K shaders are not installed yet. Playing without upscaling.", None);
            return;
        }
        let height = mpv.prop("height").and_then(|v| v.as_u64()).map(|h| h as u32);
        if !anime4k::should_apply(u, height) {
            self.status("buffering", format!("Source is {}p, above your {}p limit. Upscaling skipped.", height.unwrap_or(0), u.max_source_height), None);
            return;
        }
        if let Ok(cmds) = anime4k::mpv_commands(&self.shaders.dir, u) {
            for c in cmds {
                if mpv.command(c.into_iter().map(Value::String).collect()).await.is_err() {
                    return;
                }
            }
            self.status("buffering", format!("Anime4K Mode {} ({}) active", u.mode, u.quality), None);
        }
    }

    async fn prepare_subtitles(&self, mpv: &Mpv, kind: &str, id: &str, token: u64) -> SubOutcome {
        let cfg = self.settings.get();
        let s = &cfg.subtitles;
        if !s.enabled {
            return SubOutcome::None;
        }
        let target = self.target_lang(&cfg);
        // The file may already carry subtitles in the user's language. Then mpv's own track is used.
        if let Some(tracks) = mpv.prop("track-list").and_then(|t| t.as_array().cloned()) {
            let own = tracks.iter().find(|t| t["type"] == "sub" && lang_base(t["lang"].as_str().unwrap_or("")) == target);
            if let Some(t) = own {
                let _ = mpv.set_property("sid", t["id"].clone()).await;
                return SubOutcome::None;
            }
        }
        self.status("subtitles", "Looking for subtitles", None);
        let candidates = self.addons.subtitles(kind, id).await;
        if candidates.is_empty() {
            return SubOutcome::None;
        }
        let translator = translators::from_settings(s, &self.http);
        let on_status = |m: String| {
            if self.alive(token) {
                self.status("subtitles", m, None);
            }
        };
        match subtitles::prepare_subtitle(
            &self.http,
            &candidates,
            &target,
            &s.preferred_source_langs,
            translator.as_deref(),
            &self.cfg.data_dir.join("subtitles"),
            &on_status,
        )
        .await
        {
            Ok(Some(p)) => SubOutcome::Ready(p),
            Ok(None) => SubOutcome::None,
            Err(e) => SubOutcome::Failed(e.to_string()),
        }
    }

    /* ---------- in-player quality menu ---------- */

    async fn start_quality_menu(self: &Arc<Self>, mpv: &Arc<Mpv>, req: &PlayRequest, token: u64, sub: Option<(PathBuf, String, bool)>) {
        let alternatives: Vec<Stream> = {
            let guard = self.last_streams.lock().unwrap();
            if guard.0 == format!("{}/{}", req.kind, req.id) { guard.1.clone() } else { Vec::new() }
        };
        if alternatives.len() < 2 {
            return;
        }
        let same = |a: &Stream, b: &Stream| (a.url.is_some() && a.url == b.url) || (a.info_hash.is_some() && a.info_hash == b.info_hash && a.file_idx == b.file_idx);
        let mut current = alternatives.iter().position(|s| same(s, &req.stream)).unwrap_or(usize::MAX);
        let options = streams::quality_options(&alternatives, current, 20);
        let payload = serde_json::to_string(&options).unwrap_or_else(|_| "[]".into());
        let _ = mpv.command(vec![json!("script-message"), json!("rh-set-qualities"), json!(payload)]).await;

        let me = self.clone();
        let mpv = mpv.clone();
        let mut rx = mpv.subscribe();
        tokio::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                match ev {
                    MpvEvent::Exited => break,
                    MpvEvent::Client(args) if args.first().map(String::as_str) == Some("rh-quality-pick") => {
                        let Some(idx) = args.get(1).and_then(|s| s.parse::<usize>().ok()) else { continue };
                        let Some(stream) = alternatives.get(idx).cloned() else { continue };
                        if !me.alive(token) {
                            break;
                        }
                        match me.switch_stream(&mpv, &stream, &sub).await {
                            Ok(()) => {
                                current = idx;
                                let opts = streams::quality_options(&alternatives, current, 20);
                                let payload = serde_json::to_string(&opts).unwrap_or_else(|_| "[]".into());
                                let _ = mpv.command(vec![json!("script-message"), json!("rh-set-qualities"), json!(payload)]).await;
                            }
                            Err(e) => mpv.show_text(&format!("Could not switch: {e}"), 6000).await,
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    /// Loads another stream at the current position and puts the subtitle back.
    async fn switch_stream(&self, mpv: &Mpv, stream: &Stream, sub: &Option<(PathBuf, String, bool)>) -> Result<()> {
        mpv.show_text("Preparing the new stream...", 60000).await;
        let resolved = self.resolve(stream).await?;
        let pos = mpv.get_property("time-pos").await.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
        let mut loaded = mpv.subscribe();
        mpv.command(vec![json!("loadfile"), json!(resolved.url), json!("replace"), json!(-1), json!(format!("start={pos:.1}"))]).await?;
        let wait = async {
            while let Ok(ev) = loaded.recv().await {
                if matches!(&ev, MpvEvent::Named(n) if n == "file-loaded") {
                    return true;
                }
            }
            false
        };
        if tokio::time::timeout(Duration::from_secs(60), wait).await != Ok(true) {
            return Err(Error::msg("the new stream did not start"));
        }
        if let Some((file, title, _)) = sub {
            let _ = mpv.command(vec![json!("sub-add"), json!(file.to_string_lossy()), json!("select"), json!(title)]).await;
        }
        mpv.show_text("", 1).await;
        Ok(())
    }
}
