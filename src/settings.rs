//! Settings and library files.
use crate::json::Json;
use crate::model::MetaItem;
use std::fs;
use std::path::{Path, PathBuf};

pub const CINEMETA: &str = "https://v3-cinemeta.strem.io/manifest.json";
/// AniSync addresses are personal (each one reads one person's MyAnimeList lists), so the app never
/// ships one: the user adds their own in Addons. Builds up to settings version 4 shipped the owner's
/// address as a default; version 5 takes that one back out, recognised by this hash of its token.
pub const ANISYNC_HOST: &str = "anisync.qzz.io";
const SHIPPED_ANISYNC_TOKEN_HASH: u64 = 0x5fdad424a8a9c78e;
pub const NETFLIX: &str = "https://7a82163c306e-stremio-netflix-catalog-addon.baby-beamup.club/manifest.json";
/// Bumped when new default addons are added, so existing settings files get them once.
pub const SETTINGS_VERSION: u32 = 5;

#[derive(Clone, Debug)]
pub struct AddonEntry {
    pub url: String,
    pub enabled: bool,
    pub manifest: Json,
}

impl AddonEntry {
    pub fn new(url: &str) -> AddonEntry {
        AddonEntry { url: url.to_string(), enabled: true, manifest: Json::Null }
    }
    pub fn name(&self) -> String {
        let n = self.manifest.get("name").str();
        if n.is_empty() { self.url.clone() } else { n }
    }
    pub fn has_manifest(&self) -> bool {
        !self.manifest.is_null()
    }
    /// The manifest says it must be configured first and the link is the bare one from the site's root
    /// (no personal settings in its path), so every request to it fails.
    pub fn needs_setup(&self) -> bool {
        crate::addons::needs_setup(&self.url, &self.manifest)
    }
}


impl General {
    /// The delay set for one song (0 when none).
    pub fn kara_delay_for(&self, key: &str) -> f64 {
        self.karaoke_delays.iter().find_map(|e| e.split_once('\t').filter(|(k, _)| *k == key).and_then(|(_, v)| v.parse::<f64>().ok())).unwrap_or(0.0)
    }

    /// Sets the delay of one song; 0 forgets it.
    pub fn set_kara_delay(&mut self, key: &str, v: f64) {
        self.karaoke_delays.retain(|e| e.split_once('\t').map(|(k, _)| k != key).unwrap_or(true));
        if v.abs() > 0.001 {
            self.karaoke_delays.push(format!("{}\t{:.1}", key, v));
        }
    }

    /// The language of the interface as a 2-letter code. "system" (the default) follows the main language of
    /// "Languages I know" when the interface is translated into it, else the language of the system (`system`,
    /// such as "pt-BR"). A language picked under "UI language" always wins.
    pub fn ui_lang(&self, system: &str) -> String {
        match self.language.trim() {
            "" | "system" => {
                // the first of "Languages I know" the interface is translated into (HP lists Japanese, which it is
                // not, before Portuguese; a phone or Waydroid set to English must still show Portuguese)
                if let Some(l) = self.known_langs.iter().map(|l| crate::util::lang_base(l)).find(|l| crate::i18n::has_language(l)) {
                    return l;
                }
                let sys = crate::util::lang_base(system);
                if sys.is_empty() { "en".into() } else { sys }
            }
            other => crate::util::lang_base(other),
        }
    }
}

#[derive(Clone, Debug)]
pub struct General {
    pub hwdec: String,
    pub buffer_seconds: u32,
    /// "ask" shows the list of streams first, "best" plays the top-ranked one.
    pub pick_mode: String,
    pub preferred_height: u32,
    pub volume: f64,
    /// Preview pictures while hovering the seek bar.
    pub thumbnails: bool,
    /// Seconds the Skip button jumps forward.
    pub skip_seconds: u32,
    /// Size of text and posters in percent (100 = normal).
    pub ui_scale: u32,
    /// Blur the pictures of episodes that have not been watched yet.
    pub blur_unwatched: bool,
    /// "Offline mode" chosen by the user: no internet use even with a connection (src/connectivity.rs).
    pub offline_mode: bool,
    /// The floating feedback button over the app (writes to Feedback.log).
    pub feedback_button: bool,
    /// Controller mode: a soft blip when the focus moves (default on).
    pub pad_sound: bool,
    /// Controller mode: the orbs of time behind the pages (default on).
    pub pad_orbs: bool,
    /// Controller mode: background music on (the file in `sound_music`).
    pub pad_music: bool,
    /// Controller mode sounds picked by the user, relative to the data folder (`sounds/nav.wav`); "" = the made-up
    /// blips (nav, select) or silence (music).
    pub sound_nav: String,
    pub sound_select: String,
    pub sound_music: String,
    /// Volume of the controller mode sounds, 0-100.
    pub sound_volume: u32,
    /// PlayStation controllers (DualShock 4, DualSense): the light bar takes the theme's accent colour (default) or
    /// `pad_light_color` ("#rrggbb"), at `pad_light_brightness` percent (0 = off).
    pub pad_light_accent: bool,
    pub pad_light_color: String,
    pub pad_light_brightness: u32,
    /// The touchpad of a PlayStation controller moves the pointer (click = left click, two fingers = scroll), at
    /// `pad_touch_speed` percent.
    pub pad_touch_mouse: bool,
    pub pad_touch_speed: u32,
    /// Left stick deadzone in percent (a direction counts past twice this).
    pub pad_deadzone: u32,
    /// Milliseconds between repeats of a held direction.
    pub pad_repeat: u32,
    /// A short vibration when the focus moves (PlayStation controllers).
    pub pad_rumble: bool,
    /// Look for a newer release on GitHub once when the app opens (updater.rs).
    pub update_check: bool,
    /// Download a newer release by itself and install it when the app closes (Android: the system installer asks).
    pub update_auto: bool,
    /// Delete an episode's file from the torrents folder once it has been watched to the end (default on).
    pub delete_watched: bool,
    /// Most space the torrents folder may take, in GB (0 = no limit). The oldest is deleted first.
    pub cache_limit_gb: u32,
    /// Colours from the cover that slowly move behind the title of a page.
    pub animated_bg: bool,
    /// The colours behind a title keep drifting (costs CPU); otherwise they only change with the cover.
    pub aurora_motion: bool,
    /// What the app is for right now: "video", "images" (manga and comics) or "text" (light and web novels) or "study" (flashcards).
    pub mode: String,
    /// Extension repository of the Images mode (Tachiyomi/Mihon format); empty until the user adds one.
    pub images_repo: String,
    /// Extension repository of the Text mode (IReader format).
    pub text_repo: String,
    /// Language of the chapters to read ("en", "pt-br"…).
    pub manga_lang: String,
    /// How the reader turns pages: "rtl", "ltr", "vertical" or "webtoon".
    pub manga_mode: String,
    /// Smaller pictures, for slow connections.
    pub manga_saver: bool,
    /// The source shown in Explore: "mangadex" or the id of a Suwayomi source.
    pub manga_source: String,
    /// The Text mode: the installed plugin shown in Explore, and the size of the reader text.
    pub novel_source: String,
    pub novel_size: u32,
    /// Suwayomi-Server, the engine of the Mihon / Tachiyomi extensions.
    pub suwayomi_url: String,
    pub suwayomi_user: String,
    pub suwayomi_pass: String,
    /// The app runs Suwayomi-Server itself (downloaded once) instead of connecting to one the user started.
    pub suwayomi_managed: bool,
    /// Show the extensions for adults (18+) in the list of sources.
    pub show_nsfw: bool,
    /// Extension repositories given to Suwayomi (the three usual ones until the user changes the list).
    pub suwayomi_repos: Vec<String>,
    /// Sources pinned at the top of the list in Explore (ids).
    pub manga_pins: Vec<String>,
    /// Delay of one karaoke only ("song key\tseconds"), for the songs that really come late or early.
    pub karaoke_delays: Vec<String>,
    /// Karaoke lyrics over the openings and endings of anime.
    pub karaoke: bool,
    /// A small flash card of a word in the subtitle that is in a Study deck.
    pub study_hints: bool,
    /// Pop-ups with the macaw at the top of the window (study reminders, Pomodoro…).
    pub banners: bool,
    /// Preferences: the "Advanced" part of every tab is open.
    pub prefs_advanced: bool,
    /// Quiet time of the study reminders: from this hour to that one (local, 0-23; equal = never quiet).
    pub banner_quiet_from: u32,
    pub banner_quiet_to: u32,
    /// Languages the user can read, best first (2-letter codes; the first is the main one). Subtitles in one of
    /// them are used as they are; otherwise the best one is translated into the first. Empty = the old behaviour
    /// (the subtitle target language only).
    pub known_langs: Vec<String>,
    /// The language being learned (2-letter, may be empty).
    pub study_lang: String,
    /// Study while watching: when the subtitle is in the study language, the player now and then pauses for a short
    /// question about a line the user should know (see studyplay.rs).
    pub study_in_player: bool,
    /// How often those questions come, 0..100 (100: about one every 2 minutes).
    pub study_quiz_rate: u32,
    /// The kinds of study while watching, each one the user's choice (all under `study_in_player`): question cards
    /// after a line, parts played without subtitles, learned words mixed into subtitles in a known language (on
    /// `study_wordmix_rate` % of the lines, the Japanese reading shown small after the word when
    /// `study_wordmix_reading`), cards mined from single words between silences (at most `study_mining_max` per
    /// episode), and the macaw's cues over the video. See studyplay.rs, wordmix.rs, mining.rs.
    pub study_quiz_cards: bool,
    pub study_quiz_listen: bool,
    pub study_wordmix: bool,
    pub study_wordmix_rate: u32,
    pub study_wordmix_reading: bool,
    /// Word mixing swaps only words said in the subtitle of the audio's language, as said there; without that
    /// subtitle (audio in the study language), no word is swapped.
    pub study_wordmix_confirm: bool,
    pub study_mining: bool,
    pub study_mining_max: u32,
    pub study_mascot_cues: bool,
    /// Learn lessons never take hearts (for those who do not want to be stopped by mistakes).
    pub learn_unlimited_hearts: bool,
    /// Pomodoro timer: minutes of focus, of the short and the long break, focus sessions before the long break,
    /// the next phase starts by itself, a soft ticking during focus, a chime when a phase ends.
    /// Volume (0-100 %) of the sounds and clips of the Study cards, set in the Study sidebar.
    pub study_volume: u32,
    /// Conversation (Study): the model of the tutor ("auto", "claude", "gemini", "openai", "ollama"), the Ollama model,
    /// the level (N5…N1 for Japanese, A1…C2 otherwise), the learner's name and interests, what shows under the tutor's
    /// messages (reading in kana, romaji, translation), the tutor's voice read aloud, and the check of each message.
    pub kaiwa_ai: String,
    pub kaiwa_ollama: String,
    pub kaiwa_level: String,
    pub kaiwa_name: String,
    pub kaiwa_interests: String,
    pub kaiwa_reading: bool,
    pub kaiwa_romaji: bool,
    pub kaiwa_translation: bool,
    pub kaiwa_voice: bool,
    pub kaiwa_check: bool,
    /// Conversation: what the microphone heard is sent at once (off: put in the box to read first), and the
    /// microphone opens again by itself after the tutor's answer (hands-free). Both on by default.
    pub kaiwa_autosend: bool,
    pub kaiwa_handsfree: bool,
    /// The voice of the Study mode (voice.rs): who hears the learner ("auto", "gemini", "openai", "local" = Whisper on
    /// this computer), the local Whisper model ("auto" or an id of voice::WHISPER_MODELS), who reads aloud ("auto",
    /// "google", "local" = a voice of the system), the system voice chosen ("" = the first of the language) and the
    /// pause that ends a turn of a voice call (ms, 0 = automatic: longer for Japanese).
    pub voice_input: String,
    pub voice_model: String,
    pub voice_output: String,
    pub voice_local: String,
    pub voice_pause: u32,
    /// How far over the room's noise (dB) the voice must be to count as speech: the "cut sensitivity" of
    /// Settings > Study > Microphone (lower = more sensitive). 10 by default.
    pub voice_margin: u32,
    /// The microphone of Conversation and of Gemini Live: the id of a Qt audio input ("" = the system's default).
    pub voice_mic: String,
    /// Conversation by voice with Gemini Live (desktop, needs the Gemini key of Preferences > Accounts): scenes,
    /// lessons and free talk start spoken (`kaiwa_speak`; the voice call always is), the Live model ("" = the default
    /// of `live`), the voice ("" = chosen by the language), push-to-talk ("ptt") or open microphone ("open"),
    /// headphones (the open microphone stays on while the tutor speaks, so the learner can interrupt it) and the pause
    /// in milliseconds that ends a turn of the learner.
    pub kaiwa_speak: bool,
    pub kaiwa_live_model: String,
    pub kaiwa_live_voice: String,
    pub kaiwa_live_mic: String,
    pub kaiwa_live_headset: bool,
    pub kaiwa_live_pause: u32,
    pub pomo_focus: u32,
    pub pomo_short: u32,
    pub pomo_long: u32,
    pub pomo_rounds: u32,
    pub pomo_auto: bool,
    pub pomo_tick: bool,
    pub pomo_sound: bool,
    /// AnkiWeb account of the Study mode (the password is never kept, only the sync key AnkiWeb gives back).
    pub ankiweb_user: String,
    pub ankiweb_hkey: String,
    /// Extensions (plugins folder) the user turned off, by id.
    pub disabled_plugins: Vec<String>,
    /// Autocards (learnjapanese.moe/autocards): AnkiConnect address, mining deck, note type, the four fields, lines of
    /// context before and after, and the padding of the sentence audio.
    pub autocards_url: String,
    pub autocards_deck: String,
    pub autocards_model: String,
    pub autocards_f_sentence: String,
    pub autocards_f_expression: String,
    pub autocards_f_picture: String,
    pub autocards_f_audio: String,
    pub autocards_prev: u32,
    pub autocards_next: u32,
    pub autocards_pad_ms: u32,
    /// The karaoke is drawn at the top of the screen, away from the subtitle.
    pub karaoke_top: bool,
    /// Seconds added to the time the lyrics start (negative = earlier). Lyrics usually come about 2 s late.
    pub karaoke_delay: f64,
    /// Size of the lyrics, percent.
    pub karaoke_size: u32,
    /// Distance of the lyrics from the edge of the picture; 0 keeps the one of the file.
    pub karaoke_margin: u32,
    /// Colour of the lyrics: empty keeps the file's own, "accent" follows the accent of the app, or a `#rrggbb`.
    pub karaoke_color: String,
    /// Font of the lyrics: empty keeps the file's own.
    pub karaoke_font: String,
    /// "on" or "off" makes the lyrics bold or not; empty keeps the file's own.
    pub karaoke_bold: String,
    /// Outline (pixels of the script); -1 keeps the file's own.
    pub karaoke_outline: i64,
    /// Shadow (pixels of the script); -1 keeps the file's own.
    pub karaoke_shadow: i64,
    /// Colour of the part still to be sung: empty keeps the one the colour of the lyrics makes; else `#rrggbb`.
    pub karaoke_unfilled: String,
    /// "on" or "off"; empty keeps the file's own.
    pub karaoke_italic: String,
    /// "on" or "off"; empty keeps the file's own.
    pub karaoke_underline: String,
    /// Colour of the border around the letters (`#rrggbb`); empty keeps the file's own.
    pub karaoke_outline_color: String,
    /// Colour of the shadow, or of the box behind the letters (`#rrggbb`); empty keeps the file's own.
    pub karaoke_back_color: String,
    /// Width of the letters, percent; 0 keeps the file's own.
    pub karaoke_scale_x: u32,
    /// Height of the letters, percent; 0 keeps the file's own.
    pub karaoke_scale_y: u32,
    /// Space between the letters (pixels of the script); -100 keeps the file's own.
    pub karaoke_spacing: i64,
    /// Place of the lyrics 1-9 as on the numeric keypad; 0 follows the top/bottom choice and the file.
    pub karaoke_align: u32,
    /// Left distance from the edge; 0 keeps the file's own.
    pub karaoke_margin_l: u32,
    /// Right distance from the edge; 0 keeps the file's own.
    pub karaoke_margin_r: u32,
    /// "outline" (border and shadow) or "box" (opaque box); empty keeps the file's own.
    pub karaoke_border: String,
    /// Writes the debug report (see crate::debug).
    pub debug: bool,
    /// The reader of the Images mode: "width", "screen", "height" or "original"; zoom in percent; background; buttons on the screen; tap zones; automatic scrolling (percent of a screen per second, 0 = off).
    pub reader_fit: String,
    pub reader_zoom: u32,
    /// The look of the whole app: the id of a file in qt/themes ("gnome", "apple", "hyperos"...); "" = the theme of the
    /// device's own system (ThemeManager::systemId: HyperOS on Xiaomi phones, Android, iOS, GNOME on desktop).
    pub theme: String,
    /// Size of the translated text over the speech bubbles, in percent.
    pub reader_font: u32,
    /// Pages of the reader go through the upscaling engine (chosen again in each reader).
    pub reader_upscale: bool,
    /// The pages of the reader are read for text all the time (to select it with the mouse).
    pub reader_ocr: bool,
    /// Language the text of every page of the chapter is translated into, written over the page; empty is off.
    pub reader_translate: String,
    /// Source languages shown in the Images and Text lists; empty shows them all.
    pub manga_langs: Vec<String>,
    pub novel_langs: Vec<String>,
    /// Look of the text reader.
    pub text_font: String,
    pub text_line: u32,
    pub text_indent: u32,
    pub text_word: u32,
    pub text_align: String,
    pub text_bg: String,
    pub text_bg_color: String,
    pub text_fg: String,
    pub text_width: u32,
    /// Titles inside the text: size (percent of the text), colour (empty: as the text), centred, and the space between paragraphs (pixels).
    pub text_head_size: u32,
    pub text_head_color: String,
    pub text_head_center: bool,
    pub text_para_gap: u32,
    /// Blue light filter of the text reader, in percent (0 is off): only while a text is being read.
    pub red_light: u32,
    /// Blue light filter over the whole app (night), in percent (0 is off).
    pub night_filter: u32,
    /// Language the pages of the Text mode are translated into (empty: off, "app": the language of the app).
    pub novel_translate: String,
    /// Upscaling model (empty: the default one), size (2 to 4), program of the engine (empty: the downloaded one), tile size (0: automatic) and cache limit.
    pub up_model: String,
    pub up_scale: u32,
    pub up_engine: String,
    pub up_tile: u32,
    pub up_cache_mb: u32,
    /// Text of the pages: "ocrspace" or "vision", the key of the service and the language written on the pages.
    pub ocr_provider: String,
    pub ocr_key: String,
    pub ocr_lang: String,
    pub reader_bg: String,
    pub reader_buttons: bool,
    pub reader_taps: bool,
    /// The text boxes over the speech bubbles of the Images reader answer a tap or a click (off: they are only drawn,
    /// so a swipe on a phone scrolls or turns the page instead of opening a box).
    pub reader_bubble_taps: bool,
    pub reader_scroll: u32,
    /// How the Images reader turns a page: "slide", "curl" or "none"; the space between the pages of the strip (pixels);
    /// two pages side by side (a spread) in the page-by-page modes.
    pub reader_turn: String,
    pub reader_gap: u32,
    pub reader_spread: bool,
    /// Theme of the Text reader (Apple Books style): "original", "quiet", "paper", "bold", "calm", "focus"; empty uses
    /// the colours chosen by hand (text_bg, text_fg). `text_theme_night`: the night variant while the app is dark.
    pub text_theme: String,
    pub text_theme_night: bool,
    /// Heavier letters, the space between letters (tenths of a pixel), pages instead of scrolling, and how a page turns.
    pub text_bold: bool,
    pub text_letter: u32,
    pub text_pages: bool,
    pub text_turn: String,
    pub autoplay_next: bool,
    /// Seconds before the end when the "next episode" card appears.
    pub next_popup_secs: u32,
    /// A pattern (regex, case ignored) the stream that plays by itself must match, e.g. "1080p.*web". Empty: none.
    pub autoplay_rule: String,
    /// The user read and accepted, once, that playing a torrent shares its pieces with other people (P2P).
    pub p2p_ok: bool,
    /// "system" (follow Languages I know, then the system) or a language code such as "pt". Use `ui_lang` to read it.
    pub language: String,
    /// "fit", "crop" or "stretch"
    pub video_scale: String,
    /// Where the X-Ray takes the cast from: "auto", "tmdb", "mal" or "imdb".
    pub xray_source: String,
    /// The X-Ray asks Google Lens who is on screen by itself when it opens (otherwise only with its button): the
    /// frame is sent to Google, so it is off unless the viewer turns it on.
    pub xray_lens_auto: bool,
    /// Graphics renderer of the window: auto, ngl, vulkan or cairo (read when the app starts).
    pub renderer: String,
    /// "system", "light" or "dark"
    pub color_scheme: String,
    /// Colour of buttons and highlights as "#rrggbb"; empty keeps the system colour.
    pub accent: String,
    /// Key bindings that differ from the defaults: (action, accelerator).
    pub shortcuts: Vec<(String, String)>,
}

/// Automatic and manual backups.
#[derive(Clone, Debug)]
pub struct Backup {
    pub auto: bool,
    /// Hours between automatic backups.
    pub every_hours: u32,
    /// How many automatic backups are kept.
    pub keep: u32,
    /// Folder for automatic backups; empty means the `backups` folder of the app.
    pub folder: String,
    /// Unix time of the last backup (automatic or manual).
    pub last: u64,
    /// The study cards' pictures, sounds and clips go in the backup too (they can be big).
    pub media: bool,
    /// A copy of each backup online (Google Drive, OneDrive, iCloud Drive), see `cloud.rs`.
    pub cloud: crate::cloud::CloudSettings,
}

/// MyAnimeList account (official API, OAuth with PKCE).
#[derive(Clone, Debug, Default)]
pub struct MalAccount {
    /// From myanimelist.net/apiconfig.
    pub client_id: String,
    /// Only apps of type "web" have one; "other" apps leave it empty.
    pub client_secret: String,
    /// The redirect address registered at MyAnimeList when it is not the local one (empty = local).
    pub redirect: String,
    pub access_token: String,
    pub refresh_token: String,
    /// Unix time when the access token stops working.
    pub expires_at: u64,
    pub user: String,
}

#[derive(Clone, Debug)]
pub struct Debrid {
    pub enabled: bool,
    pub api_key: String,
}

#[derive(Clone, Debug)]
pub struct Metadata {
    pub tmdb_key: String,
    pub omdb_key: String,
    pub tmdb_art: bool,
}

#[derive(Clone, Debug)]
pub struct Subtitles {
    pub enabled: bool,
    pub target_lang: String,
    pub source_langs: Vec<String>,
    /// "libretranslate", "claude", "gemini", "openai", "ollama" (the local AI, see translate::LocalAi) or "none"
    pub provider: String,
    pub libre_url: String,
    pub libre_key: String,
    pub claude_key: String,
    pub claude_model: String,
    pub gemini_key: String,
    pub gemini_model: String,
    pub openai_key: String,
    pub openai_model: String,
    /// Any OpenAI-compatible server works (OpenRouter, a local server, Azure style proxies).
    pub openai_base: String,
    pub max_wait_secs: u32,
    /// Remove the hearing-impaired notes ([music], (laughs), "JOHN:") from downloaded subtitles.
    pub strip_sdh: bool,
    /// Line a downloaded subtitle up with the speech of the video by itself, once per video (crate::subsync).
    pub auto_sync: bool,
    pub style: SubStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubStyle {
    pub font: String,
    pub size: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub text_color: String,
    /// 0 to 100
    pub text_opacity: u32,
    pub border_color: String,
    pub border_size: f64,
    pub shadow_color: String,
    pub shadow_offset: f64,
    pub bg_enabled: bool,
    pub bg_color: String,
    pub bg_opacity: u32,
    /// 0 top, 100 bottom
    pub position: u32,
    /// Apply this style even to subtitles that bring their own (ASS) styling.
    pub force_style: bool,
    /// "left", "center" or "right"
    pub align_x: String,
    /// Distance from the side edge in pixels, used with left/right alignment.
    pub margin_x: u32,
}

impl Default for SubStyle {
    fn default() -> SubStyle {
        SubStyle {
            font: "Sans".into(),
            size: 55,
            bold: false,
            italic: false,
            underline: false,
            text_color: "#FFFFFF".into(),
            text_opacity: 100,
            border_color: "#000000".into(),
            border_size: 2.5,
            shadow_color: "#000000".into(),
            shadow_offset: 1.0,
            bg_enabled: false,
            bg_color: "#000000".into(),
            bg_opacity: 60,
            position: 95,
            force_style: false,
            align_x: "center".into(),
            margin_x: 25,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AnimeSkip {
    pub enabled: bool,
    /// Created on anime-skip.com in the account settings.
    pub client_id: String,
    pub email: String,
    pub token: String,
    pub refresh_token: String,
    pub auto_skip: bool,
}

#[derive(Clone, Debug)]
pub struct Upscaling {
    pub enabled: bool,
    pub mode: String,
    pub quality: String,
    pub clamp_highlights: bool,
    pub max_source_height: u32,
    pub gpu_hq: bool,
    pub scaler: String,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub version: u32,
    pub general: General,
    pub anime_skip: AnimeSkip,
    pub debrid: Debrid,
    pub metadata: Metadata,
    pub subtitles: Subtitles,
    pub upscaling: Upscaling,
    pub backup: Backup,
    pub mal: MalAccount,
    pub addons: Vec<AddonEntry>,
    /// Preferences > Translators: which translator serves each place, and the Microsoft Translator key.
    pub translate: TranslateCfg,
}

/// The "translate" section of the settings file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TranslateCfg {
    /// Which translator serves each place that translates ("subtitles", "readers_images"… see `translate::USES`):
    /// a translator id (`translate::CATALOGUE`). A place that is missing, or "auto", keeps the automatic choice of before.
    pub per_use: Vec<(String, String)>,
    /// Microsoft Translator (Azure AI Translator): the resource key, its region and, optionally, its own address.
    pub azure_key: String,
    pub azure_region: String,
    pub azure_endpoint: String,
}

impl Subtitles {
    /// The cloud translator that stands in for the local AI where it does not exist (Android, iOS): the first of
    /// Claude, Gemini and OpenAI with a key, else LibreTranslate (free).
    pub fn cloud_fallback(&self) -> &'static str {
        if !self.claude_key.trim().is_empty() {
            "claude"
        } else if !self.gemini_key.trim().is_empty() {
            "gemini"
        } else if !self.openai_key.trim().is_empty() {
            "openai"
        } else {
            "libretranslate"
        }
    }
}

impl TranslateCfg {
    /// The translator chosen for a place, "auto" when none.
    pub fn choice(&self, use_id: &str) -> String {
        self.per_use.iter().find(|(u, _)| u == use_id).map(|(_, t)| t.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| "auto".to_string())
    }

    pub fn from_json(j: &Json) -> TranslateCfg {
        TranslateCfg {
            // the local AI (Ollama) is desktop only: on Android and iOS a place set to it takes the automatic choice
            per_use: j.get("per_use").entries().iter().map(|(k, v)| (k.clone(), v.str())).filter(|(_, v)| !v.is_empty() && (crate::platform::DESKTOP_AI || v != "ollama")).collect(),
            azure_key: s(j, "azure_key", ""),
            azure_region: s(j, "azure_region", ""),
            azure_endpoint: s(j, "azure_endpoint", ""),
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("per_use", Json::obj(self.per_use.iter().map(|(k, v)| (k.as_str(), v.clone().into())).collect())),
            ("azure_key", self.azure_key.clone().into()),
            ("azure_region", self.azure_region.clone().into()),
            ("azure_endpoint", self.azure_endpoint.clone().into()),
        ])
    }
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            version: SETTINGS_VERSION,
            anime_skip: AnimeSkip { enabled: true, ..AnimeSkip::default() },
            general: General { autocards_url: "http://127.0.0.1:8765".into(), autocards_deck: "Mining Deck".into(), autocards_model: "Lapis".into(), autocards_f_sentence: "Sentence".into(), autocards_f_expression: "Expression".into(), autocards_f_picture: "Picture".into(), autocards_f_audio: "SentenceAudio".into(), autocards_prev: 0, autocards_next: 0, autocards_pad_ms: 250, ankiweb_user: String::new(), ankiweb_hkey: String::new(), disabled_plugins: Vec::new(), hwdec: "auto-copy".into(), buffer_seconds: 12, pick_mode: "ask".into(), preferred_height: 1080, volume: 100.0, thumbnails: true, skip_seconds: 90, ui_scale: 120, blur_unwatched: true, offline_mode: false, feedback_button: true, pad_sound: true, pad_orbs: true, pad_music: true, sound_nav: String::new(), sound_select: String::new(), sound_music: String::new(), sound_volume: 60, pad_light_accent: true, pad_light_color: String::new(), pad_light_brightness: 100, pad_touch_mouse: true, pad_touch_speed: 100, pad_deadzone: 30, pad_repeat: 90, pad_rumble: false, update_check: true, update_auto: true, delete_watched: true, cache_limit_gb: 0, animated_bg: true, aurora_motion: false, mode: "video".into(), images_repo: String::new(), text_repo: String::new(), manga_lang: "en".into(), manga_mode: "rtl".into(), manga_saver: false, manga_source: "mangadex".into(), novel_source: String::new(), novel_size: 19, suwayomi_url: crate::suwayomi::DEFAULT_URL.into(), suwayomi_user: String::new(), suwayomi_pass: String::new(), suwayomi_managed: true, show_nsfw: false, suwayomi_repos: Vec::new(), manga_pins: vec![], karaoke_delays: vec![], karaoke: true, study_hints: true, banners: true, prefs_advanced: false, banner_quiet_from: 22, banner_quiet_to: 8, known_langs: vec![], study_lang: String::new(), study_in_player: false, study_quiz_rate: 50, study_quiz_cards: true, study_quiz_listen: true, study_wordmix: true, study_wordmix_rate: 50, study_wordmix_reading: true, study_wordmix_confirm: true, study_mining: true, study_mining_max: 10, study_mascot_cues: true, learn_unlimited_hearts: false, study_volume: 100, kaiwa_ai: "auto".into(), kaiwa_ollama: String::new(), kaiwa_level: String::new(), kaiwa_name: String::new(), kaiwa_interests: String::new(), kaiwa_reading: false, kaiwa_romaji: false, kaiwa_translation: false, kaiwa_voice: false, kaiwa_check: true, kaiwa_autosend: true, kaiwa_handsfree: true, voice_input: "auto".into(), voice_model: "auto".into(), voice_output: "auto".into(), voice_local: String::new(), voice_pause: 0, voice_margin: 10, voice_mic: String::new(), pomo_focus: 25, pomo_short: 5, pomo_long: 15, pomo_rounds: 4, pomo_auto: false, pomo_tick: false, pomo_sound: true, karaoke_top: true, karaoke_delay: 0.0, karaoke_size: 100, karaoke_margin: 0, karaoke_color: String::new(), karaoke_font: String::new(), karaoke_bold: String::new(), karaoke_outline: -1, karaoke_shadow: -1, karaoke_unfilled: String::new(), karaoke_italic: String::new(), karaoke_underline: String::new(), karaoke_outline_color: String::new(), karaoke_back_color: String::new(), karaoke_scale_x: 0, karaoke_scale_y: 0, karaoke_spacing: -100, karaoke_align: 0, karaoke_margin_l: 0, karaoke_margin_r: 0, karaoke_border: String::new(), debug: false, reader_fit: "screen".into(), reader_zoom: 100, theme: String::new(), reader_font: 100, reader_upscale: false, reader_ocr: true, reader_translate: String::new(), manga_langs: vec!["en".into()], novel_langs: vec![], text_font: String::new(), text_line: 170, text_indent: 0, text_word: 0, text_align: "start".into(), text_bg: "app".into(), text_bg_color: "#fbf0d9".into(), text_fg: String::new(), text_width: 760, text_head_size: 135, text_head_color: String::new(), text_head_center: false, text_para_gap: 14, red_light: 0, night_filter: 0, novel_translate: String::new(), up_model: String::new(), up_scale: 2, up_engine: String::new(), up_tile: 0, up_cache_mb: 1024, ocr_provider: "ocrspace".into(), ocr_key: String::new(), ocr_lang: "jpn".into(), reader_bg: "dark".into(), reader_buttons: true, reader_taps: true, reader_bubble_taps: true, reader_scroll: 0, reader_turn: "slide".into(), reader_gap: 0, reader_spread: false, text_theme: String::new(), text_theme_night: true, text_bold: false, text_letter: 0, text_pages: false, text_turn: "slide".into(), autoplay_next: true, next_popup_secs: 35, autoplay_rule: String::new(), p2p_ok: false, language: "system".into(), video_scale: "fit".into(), xray_source: "auto".into(), xray_lens_auto: false, renderer: "auto".into(), color_scheme: "system".into(), accent: String::new(), kaiwa_speak: false, kaiwa_live_model: String::new(), kaiwa_live_voice: String::new(), kaiwa_live_mic: "open".into(), kaiwa_live_headset: false, kaiwa_live_pause: 1200, shortcuts: vec![] },
            debrid: Debrid { enabled: false, api_key: String::new() },
            metadata: Metadata { tmdb_key: String::new(), omdb_key: String::new(), tmdb_art: false },
            subtitles: Subtitles {
                enabled: true,
                target_lang: String::new(),
                source_langs: vec!["en".into()],
                provider: "libretranslate".into(),
                libre_url: String::new(),
                libre_key: String::new(),
                claude_key: String::new(),
                claude_model: "claude-haiku-5-5".into(),
                gemini_key: String::new(),
                gemini_model: "gemini-2.5-flash".into(),
                openai_key: String::new(),
                openai_model: "gpt-4o-mini".into(),
                openai_base: "https://api.openai.com/v1".into(),
                // playback starts this long after buffering at most; subtitles that come later are added then
                max_wait_secs: 8,
                strip_sdh: false,
                auto_sync: false,
                style: SubStyle::default(),
            },
            upscaling: Upscaling {
                enabled: false,
                mode: "A".into(),
                quality: "HQ".into(),
                clamp_highlights: true,
                max_source_height: 1080,
                gpu_hq: true,
                scaler: "ewa_lanczossharp".into(),
            },
            backup: Backup { auto: false, every_hours: 24, keep: 7, folder: String::new(), last: 0, media: true, cloud: crate::cloud::CloudSettings::default() },
            mal: MalAccount::default(),
            translate: TranslateCfg::default(),
            addons: vec![AddonEntry::new(CINEMETA), AddonEntry::new(NETFLIX)],
        }
    }
}

/// FNV-1a, enough to recognise one known address without keeping it in the source.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

/// True for the AniSync address that builds up to settings version 4 shipped by default.
fn is_shipped_anisync(url: &str) -> bool {
    let Some(rest) = url.split_once(ANISYNC_HOST).map(|(_, r)| r) else { return false };
    let token = rest.trim_start_matches('/').split('/').next().unwrap_or("");
    !token.is_empty() && fnv1a(token) == SHIPPED_ANISYNC_TOKEN_HASH
}

fn s(j: &Json, k: &str, d: &str) -> String {
    match j.get(k) {
        Json::Str(v) => v.clone(),
        _ => d.to_string(),
    }
}
fn b(j: &Json, k: &str, d: bool) -> bool {
    j.get(k).as_bool().unwrap_or(d)
}
fn n(j: &Json, k: &str, d: f64) -> f64 {
    j.get(k).as_f64().unwrap_or(d)
}

impl Settings {
    pub fn from_json(j: &Json) -> Settings {
        let mut out = Settings::from_json_raw(j);
        // the local AI (Ollama) is desktop only: on Android and iOS its translations go to a cloud translator
        if !crate::platform::DESKTOP_AI && out.subtitles.provider == "ollama" {
            out.subtitles.provider = out.subtitles.cloud_fallback().to_string();
        }
        out
    }

    fn from_json_raw(j: &Json) -> Settings {
        let d = Settings::default();
        let g = j.get("general");
        let r = j.get("debrid");
        let m = j.get("metadata");
        let sb = j.get("subtitles");
        let u = j.get("upscaling");
        let sk = j.get("anime_skip");
        let st = sb.get("style");
        let ds = SubStyle::default();
        let version = n(j, "version", 1.0) as u32;
        let mut addons: Vec<AddonEntry> = match j.get("addons") {
            Json::Arr(items) => items
                .iter()
                .filter(|a| !a.get("url").str().is_empty())
                .map(|a| AddonEntry { url: a.get("url").str(), enabled: b(a, "enabled", true), manifest: a.get("manifest").clone() })
                .collect(),
            _ => d.addons.clone(),
        };
        if version < 3 && !addons.iter().any(|a| a.url == NETFLIX) {
            addons.insert(1.min(addons.len()), AddonEntry::new(NETFLIX));
        }
        // Version 4 dropped the old MyAnimeList addon (its address was personal too).
        if version < 4 {
            addons.retain(|a| !a.url.contains("mal-stremio"));
        }
        // Version 5 removes the personal AniSync address that earlier builds added by default.
        if version < 5 {
            addons.retain(|a| !is_shipped_anisync(&a.url));
        }
        let bk = j.get("backup");
        let ma = j.get("mal");
        // The old default pointed at a local server that nobody had; empty now means "automatic".
        let mut libre_url = s(sb, "libre_url", &d.subtitles.libre_url);
        if libre_url.trim_end_matches('/') == "http://localhost:5000" && version < 4 {
            libre_url.clear();
        }
        let langs = sb.get("source_langs").strings();
        Settings {
            version: SETTINGS_VERSION,
            anime_skip: AnimeSkip {
                enabled: b(sk, "enabled", true),
                client_id: s(sk, "client_id", ""),
                email: s(sk, "email", ""),
                token: s(sk, "token", ""),
                refresh_token: s(sk, "refresh_token", ""),
                auto_skip: b(sk, "auto_skip", false),
            },
            general: General {
                hwdec: s(g, "hwdec", &d.general.hwdec),
                buffer_seconds: n(g, "buffer_seconds", d.general.buffer_seconds as f64) as u32,
                pick_mode: s(g, "pick_mode", &d.general.pick_mode),
                preferred_height: n(g, "preferred_height", d.general.preferred_height as f64) as u32,
                volume: n(g, "volume", d.general.volume),
                thumbnails: b(g, "thumbnails", true),
                skip_seconds: n(g, "skip_seconds", 90.0) as u32,
                ui_scale: (n(g, "ui_scale", 120.0) as u32).clamp(50, 200),
                blur_unwatched: b(g, "blur_unwatched", true),
                offline_mode: b(g, "offline_mode", false),
                feedback_button: b(g, "feedback_button", true),
                pad_sound: b(g, "pad_sound", true),
                pad_orbs: b(g, "pad_orbs", true),
                pad_music: b(g, "pad_music", true),
                sound_nav: s(g, "sound_nav", ""),
                sound_select: s(g, "sound_select", ""),
                sound_music: s(g, "sound_music", ""),
                sound_volume: (n(g, "sound_volume", 60.0) as u32).min(100),
                pad_light_accent: b(g, "pad_light_accent", true),
                pad_light_color: s(g, "pad_light_color", ""),
                pad_light_brightness: (n(g, "pad_light_brightness", 100.0) as u32).min(100),
                pad_touch_mouse: b(g, "pad_touch_mouse", true),
                pad_touch_speed: (n(g, "pad_touch_speed", 100.0) as u32).clamp(20, 300),
                pad_deadzone: (n(g, "pad_deadzone", 30.0) as u32).clamp(5, 60),
                pad_repeat: (n(g, "pad_repeat", 90.0) as u32).clamp(40, 300),
                pad_rumble: b(g, "pad_rumble", false),
                update_check: b(g, "update_check", true),
                update_auto: b(g, "update_auto", true),
                delete_watched: b(g, "delete_watched", true),
                cache_limit_gb: (n(g, "cache_limit_gb", 0.0).max(0.0) as u32).min(100_000),
                animated_bg: b(g, "animated_bg", true),
                aurora_motion: b(g, "aurora_motion", false),
                mode: s(g, "mode", "video"),
                images_repo: s(g, "images_repo", ""),
                text_repo: s(g, "text_repo", ""),
                manga_lang: s(g, "manga_lang", "en"),
                manga_mode: s(g, "manga_mode", "rtl"),
                manga_saver: b(g, "manga_saver", false),
                manga_source: s(g, "manga_source", "mangadex"),
                novel_source: s(g, "novel_source", ""),
                novel_size: (n(g, "novel_size", 19.0) as u32).clamp(10, 48),
                suwayomi_url: s(g, "suwayomi_url", crate::suwayomi::DEFAULT_URL),
                suwayomi_user: s(g, "suwayomi_user", ""),
                suwayomi_pass: s(g, "suwayomi_pass", ""),
                suwayomi_managed: b(g, "suwayomi_managed", true),
                show_nsfw: b(g, "show_nsfw", false),
                manga_pins: g.get("manga_pins").strings(),
                karaoke_delays: g.get("karaoke_delays").strings(),
                suwayomi_repos: g.get("suwayomi_repos").strings(),
                karaoke: b(g, "karaoke", true),
                study_hints: b(g, "study_hints", true),
                banners: b(g, "banners", true),
                prefs_advanced: b(g, "prefs_advanced", false),
                banner_quiet_from: n(g, "banner_quiet_from", 22.0).clamp(0.0, 23.0) as u32,
                banner_quiet_to: n(g, "banner_quiet_to", 8.0).clamp(0.0, 23.0) as u32,
                known_langs: g.get("known_langs").strings().iter().map(|l| crate::util::lang_base(l)).filter(|l| !l.is_empty()).collect(),
                study_lang: crate::util::lang_base(&s(g, "study_lang", "")),
                study_in_player: b(g, "study_in_player", false),
                study_quiz_rate: n(g, "study_quiz_rate", 50.0).clamp(0.0, 100.0) as u32,
                study_quiz_cards: b(g, "study_quiz_cards", true),
                study_quiz_listen: b(g, "study_quiz_listen", true),
                study_wordmix: b(g, "study_wordmix", true),
                study_wordmix_rate: n(g, "study_wordmix_rate", 50.0).clamp(0.0, 100.0) as u32,
                study_wordmix_reading: b(g, "study_wordmix_reading", true),
                study_wordmix_confirm: b(g, "study_wordmix_confirm", true),
                study_mining: b(g, "study_mining", true),
                study_mining_max: n(g, "study_mining_max", 10.0).clamp(1.0, 50.0) as u32,
                study_mascot_cues: b(g, "study_mascot_cues", true),
                learn_unlimited_hearts: b(g, "learn_unlimited_hearts", false),
                study_volume: n(g, "study_volume", 100.0).clamp(0.0, 100.0) as u32,
                kaiwa_ai: s(g, "kaiwa_ai", "auto"),
                kaiwa_ollama: s(g, "kaiwa_ollama", ""),
                kaiwa_level: s(g, "kaiwa_level", ""),
                kaiwa_name: s(g, "kaiwa_name", ""),
                kaiwa_interests: s(g, "kaiwa_interests", ""),
                kaiwa_reading: b(g, "kaiwa_reading", false),
                kaiwa_romaji: b(g, "kaiwa_romaji", false),
                kaiwa_translation: b(g, "kaiwa_translation", false),
                kaiwa_voice: b(g, "kaiwa_voice", false),
                kaiwa_check: b(g, "kaiwa_check", true),
                kaiwa_autosend: b(g, "kaiwa_autosend", true),
                kaiwa_handsfree: b(g, "kaiwa_handsfree", true),
                voice_input: s(g, "voice_input", "auto"),
                voice_model: s(g, "voice_model", "auto"),
                voice_output: s(g, "voice_output", "auto"),
                voice_local: s(g, "voice_local", ""),
                voice_pause: n(g, "voice_pause", 0.0).clamp(0.0, 6000.0) as u32,
                voice_margin: n(g, "voice_margin", 10.0).clamp(3.0, 30.0) as u32,
                voice_mic: s(g, "voice_mic", ""),
                kaiwa_speak: b(g, "kaiwa_speak", false),
                kaiwa_live_model: s(g, "kaiwa_live_model", "").trim().to_string(),
                kaiwa_live_voice: s(g, "kaiwa_live_voice", "").trim().to_string(),
                kaiwa_live_mic: if s(g, "kaiwa_live_mic", "open") == "ptt" { "ptt".into() } else { "open".into() },
                kaiwa_live_headset: b(g, "kaiwa_live_headset", false),
                kaiwa_live_pause: n(g, "kaiwa_live_pause", 1200.0).clamp(500.0, 3000.0) as u32,
                pomo_focus: n(g, "pomo_focus", 25.0).clamp(1.0, 180.0) as u32,
                pomo_short: n(g, "pomo_short", 5.0).clamp(1.0, 60.0) as u32,
                pomo_long: n(g, "pomo_long", 15.0).clamp(1.0, 120.0) as u32,
                pomo_rounds: n(g, "pomo_rounds", 4.0).clamp(1.0, 12.0) as u32,
                pomo_auto: b(g, "pomo_auto", false),
                pomo_tick: b(g, "pomo_tick", false),
                pomo_sound: b(g, "pomo_sound", true),
                ankiweb_user: s(g, "ankiweb_user", ""),
                ankiweb_hkey: s(g, "ankiweb_hkey", ""),
                disabled_plugins: g.get("disabled_plugins").strings(),
                autocards_url: s(g, "autocards_url", "http://127.0.0.1:8765"),
                autocards_deck: s(g, "autocards_deck", "Mining Deck"),
                autocards_model: s(g, "autocards_model", "Lapis"),
                autocards_f_sentence: s(g, "autocards_f_sentence", "Sentence"),
                autocards_f_expression: s(g, "autocards_f_expression", "Expression"),
                autocards_f_picture: s(g, "autocards_f_picture", "Picture"),
                autocards_f_audio: s(g, "autocards_f_audio", "SentenceAudio"),
                autocards_prev: g.get("autocards_prev").as_u64().unwrap_or(0) as u32,
                autocards_next: g.get("autocards_next").as_u64().unwrap_or(0) as u32,
                autocards_pad_ms: g.get("autocards_pad_ms").as_u64().unwrap_or(250) as u32,
                karaoke_top: b(g, "karaoke_top", true),
                karaoke_delay: { let d = g.get("karaoke_delay").as_f64().unwrap_or(0.0); if d == -2.0 { 0.0 } else { d } },
                karaoke_size: g.get("karaoke_size").as_u64().unwrap_or(100) as u32,
                karaoke_margin: g.get("karaoke_margin").as_u64().unwrap_or(0) as u32,
                karaoke_color: s(g, "karaoke_color", ""),
                karaoke_font: s(g, "karaoke_font", ""),
                karaoke_bold: s(g, "karaoke_bold", ""),
                karaoke_outline: g.get("karaoke_outline").as_i64().unwrap_or(-1),
                karaoke_shadow: g.get("karaoke_shadow").as_i64().unwrap_or(-1),
                karaoke_unfilled: s(g, "karaoke_unfilled", ""),
                karaoke_italic: s(g, "karaoke_italic", ""),
                karaoke_underline: s(g, "karaoke_underline", ""),
                karaoke_outline_color: s(g, "karaoke_outline_color", ""),
                karaoke_back_color: s(g, "karaoke_back_color", ""),
                karaoke_scale_x: g.get("karaoke_scale_x").as_u64().unwrap_or(0) as u32,
                karaoke_scale_y: g.get("karaoke_scale_y").as_u64().unwrap_or(0) as u32,
                karaoke_spacing: g.get("karaoke_spacing").as_i64().unwrap_or(-100),
                karaoke_align: g.get("karaoke_align").as_u64().unwrap_or(0).min(9) as u32,
                karaoke_margin_l: g.get("karaoke_margin_l").as_u64().unwrap_or(0) as u32,
                karaoke_margin_r: g.get("karaoke_margin_r").as_u64().unwrap_or(0) as u32,
                karaoke_border: s(g, "karaoke_border", ""),
                debug: b(g, "debug", false),
                reader_fit: s(g, "reader_fit", "screen"),
                reader_zoom: (n(g, "reader_zoom", 100.0) as u32).clamp(10, 400),
                // "ios" was folded into "apple" (its phone side), so a saved "ios" opens the Apple theme
                theme: match s(g, "theme", "") { t if t == "ios" => "apple".into(), t => t },
                reader_font: (n(g, "reader_font", 100.0) as u32).clamp(40, 300),
                reader_upscale: b(g, "reader_upscale", false),
                reader_ocr: b(g, "reader_ocr", true),
                reader_translate: s(g, "reader_translate", ""),
                manga_langs: if matches!(g.get("manga_langs"), Json::Arr(_)) { g.get("manga_langs").strings() } else { vec![s(g, "manga_lang", "en")].into_iter().filter(|l| !l.is_empty() && l != "all").collect() },
                novel_langs: g.get("novel_langs").strings(),
                text_font: s(g, "text_font", ""),
                text_line: (n(g, "text_line", 170.0) as u32).clamp(100, 300),
                text_indent: (n(g, "text_indent", 0.0) as u32).min(120),
                text_word: (n(g, "text_word", 0.0) as u32).min(30),
                text_align: s(g, "text_align", "start"),
                text_bg: s(g, "text_bg", "app"),
                text_bg_color: s(g, "text_bg_color", "#fbf0d9"),
                text_fg: s(g, "text_fg", ""),
                text_width: (n(g, "text_width", 760.0) as u32).clamp(360, 1600),
                text_head_size: (n(g, "text_head_size", 135.0) as u32).clamp(80, 300),
                text_head_color: s(g, "text_head_color", ""),
                text_head_center: b(g, "text_head_center", false),
                text_para_gap: (n(g, "text_para_gap", 14.0) as u32).min(60),
                night_filter: (n(g, "night_filter", 0.0) as u32).min(100),
                novel_translate: s(g, "novel_translate", ""),
                red_light: (n(g, "red_light", 0.0) as u32).min(100),
                up_model: s(g, "up_model", ""),
                up_scale: (n(g, "up_scale", 2.0) as u32).clamp(2, 4),
                up_engine: s(g, "up_engine", ""),
                up_tile: n(g, "up_tile", 0.0) as u32,
                up_cache_mb: (n(g, "up_cache_mb", 1024.0) as u32).clamp(100, 20000),
                ocr_provider: s(g, "ocr_provider", "ocrspace"),
                ocr_key: s(g, "ocr_key", ""),
                ocr_lang: s(g, "ocr_lang", "jpn"),
                reader_bg: s(g, "reader_bg", "dark"),
                reader_buttons: b(g, "reader_buttons", true),
                reader_taps: b(g, "reader_taps", true),
                reader_bubble_taps: b(g, "reader_bubble_taps", true),
                reader_scroll: (n(g, "reader_scroll", 0.0) as u32).min(100),
                reader_turn: s(g, "reader_turn", "slide"),
                reader_gap: (n(g, "reader_gap", 0.0) as u32).min(120),
                reader_spread: b(g, "reader_spread", false),
                text_theme: s(g, "text_theme", ""),
                text_theme_night: b(g, "text_theme_night", true),
                text_bold: b(g, "text_bold", false),
                text_letter: (n(g, "text_letter", 0.0) as u32).min(50),
                text_pages: b(g, "text_pages", false),
                text_turn: s(g, "text_turn", "slide"),
                autoplay_next: b(g, "autoplay_next", true),
                next_popup_secs: n(g, "next_popup_secs", 35.0) as u32,
                autoplay_rule: s(g, "autoplay_rule", ""),
                p2p_ok: b(g, "p2p_ok", false),
                // "ui_language" replaced "language" (2026-10-31): the old default "en" was saved for everyone, so an old
                // "en" now means "system" (follow the main language of Languages I know, then the system).
                language: if g.get("ui_language").is_null() { match s(g, "language", "system").as_str() { "en" | "" => "system".into(), o => o.to_string() } } else { s(g, "ui_language", "system") },
                video_scale: s(g, "video_scale", "fit"),
                xray_source: s(g, "xray_source", "auto"),
                xray_lens_auto: b(g, "xray_lens_auto", false),
                renderer: s(g, "renderer", "auto"),
                color_scheme: s(g, "color_scheme", "system"),
                accent: s(g, "accent", ""),
                shortcuts: g.get("shortcuts").entries().iter().map(|(k, v)| (k.clone(), v.str())).collect(),
            },
            debrid: Debrid { enabled: b(r, "enabled", false), api_key: s(r, "api_key", "") },
            metadata: Metadata { tmdb_key: s(m, "tmdb_key", ""), omdb_key: s(m, "omdb_key", ""), tmdb_art: b(m, "tmdb_art", false) },
            subtitles: Subtitles {
                enabled: b(sb, "enabled", d.subtitles.enabled),
                target_lang: s(sb, "target_lang", ""),
                source_langs: if langs.is_empty() { d.subtitles.source_langs.clone() } else { langs },
                provider: s(sb, "provider", &d.subtitles.provider),
                libre_url,
                libre_key: s(sb, "libre_key", ""),
                claude_key: s(sb, "claude_key", ""),
                claude_model: s(sb, "claude_model", &d.subtitles.claude_model),
                gemini_key: s(sb, "gemini_key", ""),
                gemini_model: s(sb, "gemini_model", &d.subtitles.gemini_model),
                openai_key: s(sb, "openai_key", ""),
                openai_model: s(sb, "openai_model", &d.subtitles.openai_model),
                openai_base: s(sb, "openai_base", &d.subtitles.openai_base),
                // 90 was the old default (the video waited up to 90 s for subtitles): now they are added when they come
                max_wait_secs: match n(sb, "max_wait_secs", d.subtitles.max_wait_secs as f64) as u32 { 90 => d.subtitles.max_wait_secs, v => v },
                strip_sdh: b(sb, "strip_sdh", d.subtitles.strip_sdh),
                auto_sync: b(sb, "auto_sync", d.subtitles.auto_sync),
                style: SubStyle {
                    font: s(st, "font", &ds.font),
                    size: n(st, "size", ds.size as f64) as u32,
                    bold: b(st, "bold", ds.bold),
                    italic: b(st, "italic", ds.italic),
                    underline: b(st, "underline", ds.underline),
                    text_color: s(st, "text_color", &ds.text_color),
                    text_opacity: n(st, "text_opacity", ds.text_opacity as f64) as u32,
                    border_color: s(st, "border_color", &ds.border_color),
                    border_size: n(st, "border_size", ds.border_size),
                    shadow_color: s(st, "shadow_color", &ds.shadow_color),
                    shadow_offset: n(st, "shadow_offset", ds.shadow_offset),
                    bg_enabled: b(st, "bg_enabled", ds.bg_enabled),
                    bg_color: s(st, "bg_color", &ds.bg_color),
                    bg_opacity: n(st, "bg_opacity", ds.bg_opacity as f64) as u32,
                    position: n(st, "position", ds.position as f64) as u32,
                    force_style: b(st, "force_style", ds.force_style),
                    align_x: s(st, "align_x", &ds.align_x),
                    margin_x: n(st, "margin_x", ds.margin_x as f64) as u32,
                },
            },
            upscaling: Upscaling {
                enabled: b(u, "enabled", false),
                mode: s(u, "mode", &d.upscaling.mode),
                quality: s(u, "quality", &d.upscaling.quality),
                clamp_highlights: b(u, "clamp_highlights", true),
                max_source_height: n(u, "max_source_height", d.upscaling.max_source_height as f64) as u32,
                gpu_hq: b(u, "gpu_hq", true),
                scaler: s(u, "scaler", &d.upscaling.scaler),
            },
            backup: Backup {
                auto: b(bk, "auto", false),
                every_hours: (n(bk, "every_hours", 24.0) as u32).clamp(1, 24 * 90),
                keep: (n(bk, "keep", 7.0) as u32).clamp(1, 100),
                folder: s(bk, "folder", ""),
                last: n(bk, "last", 0.0) as u64,
                media: b(bk, "media", true),
                cloud: crate::cloud::CloudSettings::from_json(bk.get("cloud")),
            },
            mal: MalAccount {
                client_id: s(ma, "client_id", ""),
                client_secret: s(ma, "client_secret", ""),
                redirect: s(ma, "redirect", ""),
                access_token: s(ma, "access_token", ""),
                refresh_token: s(ma, "refresh_token", ""),
                expires_at: n(ma, "expires_at", 0.0) as u64,
                user: s(ma, "user", ""),
            },
            translate: TranslateCfg::from_json(j.get("translate")),
            addons,
        }
    }

    pub fn to_json(&self) -> Json {
        let st = &self.subtitles.style;
        Json::obj(vec![
            ("version", self.version.into()),
            ("anime_skip", Json::obj(vec![
                ("enabled", self.anime_skip.enabled.into()),
                ("client_id", self.anime_skip.client_id.clone().into()),
                ("email", self.anime_skip.email.clone().into()),
                ("token", self.anime_skip.token.clone().into()),
                ("refresh_token", self.anime_skip.refresh_token.clone().into()),
                ("auto_skip", self.anime_skip.auto_skip.into()),
            ])),
            ("general", Json::obj(vec![
                ("hwdec", self.general.hwdec.clone().into()),
                ("buffer_seconds", self.general.buffer_seconds.into()),
                ("pick_mode", self.general.pick_mode.clone().into()),
                ("preferred_height", self.general.preferred_height.into()),
                ("volume", self.general.volume.into()),
                ("thumbnails", self.general.thumbnails.into()),
                ("skip_seconds", self.general.skip_seconds.into()),
                ("ui_scale", self.general.ui_scale.into()),
                ("blur_unwatched", self.general.blur_unwatched.into()),
                ("offline_mode", self.general.offline_mode.into()),
                ("feedback_button", self.general.feedback_button.into()),
                ("pad_sound", self.general.pad_sound.into()),
                ("pad_orbs", self.general.pad_orbs.into()),
                ("pad_music", self.general.pad_music.into()),
                ("sound_nav", self.general.sound_nav.clone().into()),
                ("sound_select", self.general.sound_select.clone().into()),
                ("sound_music", self.general.sound_music.clone().into()),
                ("sound_volume", self.general.sound_volume.into()),
                ("pad_light_accent", self.general.pad_light_accent.into()),
                ("pad_light_color", self.general.pad_light_color.clone().into()),
                ("pad_light_brightness", self.general.pad_light_brightness.into()),
                ("pad_touch_mouse", self.general.pad_touch_mouse.into()),
                ("pad_touch_speed", self.general.pad_touch_speed.into()),
                ("pad_deadzone", self.general.pad_deadzone.into()),
                ("pad_repeat", self.general.pad_repeat.into()),
                ("pad_rumble", self.general.pad_rumble.into()),
                ("update_check", self.general.update_check.into()),
                ("update_auto", self.general.update_auto.into()),
                ("delete_watched", self.general.delete_watched.into()),
                ("cache_limit_gb", self.general.cache_limit_gb.into()),
                ("animated_bg", self.general.animated_bg.into()),
                ("aurora_motion", self.general.aurora_motion.into()),
                ("mode", self.general.mode.clone().into()),
                ("images_repo", self.general.images_repo.clone().into()),
                ("text_repo", self.general.text_repo.clone().into()),
                ("manga_lang", self.general.manga_lang.clone().into()),
                ("manga_mode", self.general.manga_mode.clone().into()),
                ("manga_saver", self.general.manga_saver.into()),
                ("manga_source", self.general.manga_source.clone().into()),
                ("novel_source", self.general.novel_source.clone().into()),
                ("novel_size", (self.general.novel_size as f64).into()),
                ("suwayomi_url", self.general.suwayomi_url.clone().into()),
                ("suwayomi_user", self.general.suwayomi_user.clone().into()),
                ("suwayomi_pass", self.general.suwayomi_pass.clone().into()),
                ("suwayomi_managed", self.general.suwayomi_managed.into()),
                ("show_nsfw", self.general.show_nsfw.into()),
                ("manga_pins", Json::Arr(self.general.manga_pins.iter().map(|r| r.clone().into()).collect())),
                ("karaoke_delays", Json::Arr(self.general.karaoke_delays.iter().map(|r| r.clone().into()).collect())),
                ("suwayomi_repos", Json::Arr(self.general.suwayomi_repos.iter().map(|r| r.clone().into()).collect())),
                ("karaoke", self.general.karaoke.into()),
                ("study_hints", self.general.study_hints.into()),
                ("banners", self.general.banners.into()),
                ("prefs_advanced", self.general.prefs_advanced.into()),
                ("banner_quiet_from", self.general.banner_quiet_from.into()),
                ("banner_quiet_to", self.general.banner_quiet_to.into()),
                ("known_langs", Json::Arr(self.general.known_langs.iter().map(|r| r.clone().into()).collect())),
                ("study_lang", self.general.study_lang.clone().into()),
                ("study_in_player", self.general.study_in_player.into()),
                ("study_quiz_rate", self.general.study_quiz_rate.into()),
                ("study_quiz_cards", self.general.study_quiz_cards.into()),
                ("study_quiz_listen", self.general.study_quiz_listen.into()),
                ("study_wordmix", self.general.study_wordmix.into()),
                ("study_wordmix_rate", self.general.study_wordmix_rate.into()),
                ("study_wordmix_reading", self.general.study_wordmix_reading.into()),
                ("study_wordmix_confirm", self.general.study_wordmix_confirm.into()),
                ("study_mining", self.general.study_mining.into()),
                ("study_mining_max", self.general.study_mining_max.into()),
                ("study_mascot_cues", self.general.study_mascot_cues.into()),
                ("learn_unlimited_hearts", self.general.learn_unlimited_hearts.into()),
                ("study_volume", self.general.study_volume.into()),
                ("kaiwa_ai", self.general.kaiwa_ai.clone().into()),
                ("kaiwa_ollama", self.general.kaiwa_ollama.clone().into()),
                ("kaiwa_level", self.general.kaiwa_level.clone().into()),
                ("kaiwa_name", self.general.kaiwa_name.clone().into()),
                ("kaiwa_interests", self.general.kaiwa_interests.clone().into()),
                ("kaiwa_reading", self.general.kaiwa_reading.into()),
                ("kaiwa_romaji", self.general.kaiwa_romaji.into()),
                ("kaiwa_translation", self.general.kaiwa_translation.into()),
                ("kaiwa_voice", self.general.kaiwa_voice.into()),
                ("kaiwa_check", self.general.kaiwa_check.into()),
                ("kaiwa_autosend", self.general.kaiwa_autosend.into()),
                ("kaiwa_handsfree", self.general.kaiwa_handsfree.into()),
                ("voice_input", self.general.voice_input.clone().into()),
                ("voice_model", self.general.voice_model.clone().into()),
                ("voice_output", self.general.voice_output.clone().into()),
                ("voice_local", self.general.voice_local.clone().into()),
                ("voice_pause", self.general.voice_pause.into()),
                ("voice_margin", self.general.voice_margin.into()),
                ("voice_mic", self.general.voice_mic.clone().into()),
                ("kaiwa_speak", self.general.kaiwa_speak.into()),
                ("kaiwa_live_model", self.general.kaiwa_live_model.clone().into()),
                ("kaiwa_live_voice", self.general.kaiwa_live_voice.clone().into()),
                ("kaiwa_live_mic", self.general.kaiwa_live_mic.clone().into()),
                ("kaiwa_live_headset", self.general.kaiwa_live_headset.into()),
                ("kaiwa_live_pause", self.general.kaiwa_live_pause.into()),
                ("pomo_focus", self.general.pomo_focus.into()),
                ("pomo_short", self.general.pomo_short.into()),
                ("pomo_long", self.general.pomo_long.into()),
                ("pomo_rounds", self.general.pomo_rounds.into()),
                ("pomo_auto", self.general.pomo_auto.into()),
                ("pomo_tick", self.general.pomo_tick.into()),
                ("pomo_sound", self.general.pomo_sound.into()),
                ("ankiweb_user", self.general.ankiweb_user.clone().into()),
                ("ankiweb_hkey", self.general.ankiweb_hkey.clone().into()),
                ("disabled_plugins", Json::Arr(self.general.disabled_plugins.iter().map(|r| r.clone().into()).collect())),
                ("autocards_url", self.general.autocards_url.clone().into()),
                ("autocards_deck", self.general.autocards_deck.clone().into()),
                ("autocards_model", self.general.autocards_model.clone().into()),
                ("autocards_f_sentence", self.general.autocards_f_sentence.clone().into()),
                ("autocards_f_expression", self.general.autocards_f_expression.clone().into()),
                ("autocards_f_picture", self.general.autocards_f_picture.clone().into()),
                ("autocards_f_audio", self.general.autocards_f_audio.clone().into()),
                ("autocards_prev", (self.general.autocards_prev as f64).into()),
                ("autocards_next", (self.general.autocards_next as f64).into()),
                ("autocards_pad_ms", (self.general.autocards_pad_ms as f64).into()),
                ("karaoke_top", self.general.karaoke_top.into()),
                ("karaoke_delay", self.general.karaoke_delay.into()),
                ("karaoke_size", (self.general.karaoke_size as f64).into()),
                ("karaoke_margin", (self.general.karaoke_margin as f64).into()),
                ("karaoke_color", self.general.karaoke_color.clone().into()),
                ("karaoke_font", self.general.karaoke_font.clone().into()),
                ("karaoke_bold", self.general.karaoke_bold.clone().into()),
                ("karaoke_outline", (self.general.karaoke_outline as f64).into()),
                ("karaoke_shadow", (self.general.karaoke_shadow as f64).into()),
                ("karaoke_unfilled", self.general.karaoke_unfilled.clone().into()),
                ("karaoke_italic", self.general.karaoke_italic.clone().into()),
                ("karaoke_underline", self.general.karaoke_underline.clone().into()),
                ("karaoke_outline_color", self.general.karaoke_outline_color.clone().into()),
                ("karaoke_back_color", self.general.karaoke_back_color.clone().into()),
                ("karaoke_scale_x", (self.general.karaoke_scale_x as f64).into()),
                ("karaoke_scale_y", (self.general.karaoke_scale_y as f64).into()),
                ("karaoke_spacing", (self.general.karaoke_spacing as f64).into()),
                ("karaoke_align", (self.general.karaoke_align as f64).into()),
                ("karaoke_margin_l", (self.general.karaoke_margin_l as f64).into()),
                ("karaoke_margin_r", (self.general.karaoke_margin_r as f64).into()),
                ("karaoke_border", self.general.karaoke_border.clone().into()),
                ("debug", self.general.debug.into()),
                ("reader_fit", self.general.reader_fit.clone().into()),
                ("reader_zoom", self.general.reader_zoom.into()),
                ("reader_font", self.general.reader_font.into()),
                ("theme", self.general.theme.as_str().into()),
                ("reader_upscale", self.general.reader_upscale.into()),
                ("reader_ocr", self.general.reader_ocr.into()),
                ("reader_translate", self.general.reader_translate.clone().into()),
                ("manga_langs", Json::Arr(self.general.manga_langs.iter().map(|r| r.clone().into()).collect())),
                ("novel_langs", Json::Arr(self.general.novel_langs.iter().map(|r| r.clone().into()).collect())),
                ("text_font", self.general.text_font.clone().into()),
                ("text_line", self.general.text_line.into()),
                ("text_indent", self.general.text_indent.into()),
                ("text_word", self.general.text_word.into()),
                ("text_align", self.general.text_align.clone().into()),
                ("text_bg", self.general.text_bg.clone().into()),
                ("text_bg_color", self.general.text_bg_color.clone().into()),
                ("text_fg", self.general.text_fg.clone().into()),
                ("text_width", self.general.text_width.into()),
                ("text_head_size", self.general.text_head_size.into()),
                ("text_head_color", self.general.text_head_color.clone().into()),
                ("text_head_center", self.general.text_head_center.into()),
                ("text_para_gap", self.general.text_para_gap.into()),
                ("night_filter", self.general.night_filter.into()),
                ("novel_translate", self.general.novel_translate.clone().into()),
                ("red_light", self.general.red_light.into()),
                ("up_model", self.general.up_model.clone().into()),
                ("up_scale", self.general.up_scale.into()),
                ("up_engine", self.general.up_engine.clone().into()),
                ("up_tile", self.general.up_tile.into()),
                ("up_cache_mb", self.general.up_cache_mb.into()),
                ("ocr_provider", self.general.ocr_provider.clone().into()),
                ("ocr_key", self.general.ocr_key.clone().into()),
                ("ocr_lang", self.general.ocr_lang.clone().into()),
                ("reader_bg", self.general.reader_bg.clone().into()),
                ("reader_buttons", self.general.reader_buttons.into()),
                ("reader_taps", self.general.reader_taps.into()),
                ("reader_bubble_taps", self.general.reader_bubble_taps.into()),
                ("reader_scroll", self.general.reader_scroll.into()),
                ("reader_turn", self.general.reader_turn.clone().into()),
                ("reader_gap", self.general.reader_gap.into()),
                ("reader_spread", self.general.reader_spread.into()),
                ("text_theme", self.general.text_theme.clone().into()),
                ("text_theme_night", self.general.text_theme_night.into()),
                ("text_bold", self.general.text_bold.into()),
                ("text_letter", self.general.text_letter.into()),
                ("text_pages", self.general.text_pages.into()),
                ("text_turn", self.general.text_turn.clone().into()),
                ("autoplay_next", self.general.autoplay_next.into()),
                ("next_popup_secs", self.general.next_popup_secs.into()),
                ("autoplay_rule", self.general.autoplay_rule.clone().into()),
                ("p2p_ok", self.general.p2p_ok.into()),
                ("ui_language", self.general.language.clone().into()),
                ("language", self.general.language.clone().into()),
                ("video_scale", self.general.video_scale.clone().into()),
                ("xray_source", self.general.xray_source.clone().into()),
                ("xray_lens_auto", self.general.xray_lens_auto.into()),
                ("renderer", self.general.renderer.clone().into()),
                ("color_scheme", self.general.color_scheme.clone().into()),
                ("accent", self.general.accent.clone().into()),
                ("shortcuts", Json::obj(self.general.shortcuts.iter().map(|(k, v)| (k.as_str(), v.clone().into())).collect())),
            ])),
            ("debrid", Json::obj(vec![("enabled", self.debrid.enabled.into()), ("api_key", self.debrid.api_key.clone().into())])),
            ("metadata", Json::obj(vec![("tmdb_key", self.metadata.tmdb_key.clone().into()), ("omdb_key", self.metadata.omdb_key.clone().into()), ("tmdb_art", self.metadata.tmdb_art.into())])),
            ("subtitles", Json::obj(vec![
                ("enabled", self.subtitles.enabled.into()),
                ("target_lang", self.subtitles.target_lang.clone().into()),
                ("source_langs", self.subtitles.source_langs.clone().into()),
                ("provider", self.subtitles.provider.clone().into()),
                ("libre_url", self.subtitles.libre_url.clone().into()),
                ("libre_key", self.subtitles.libre_key.clone().into()),
                ("claude_key", self.subtitles.claude_key.clone().into()),
                ("claude_model", self.subtitles.claude_model.clone().into()),
                ("gemini_key", self.subtitles.gemini_key.clone().into()),
                ("gemini_model", self.subtitles.gemini_model.clone().into()),
                ("openai_key", self.subtitles.openai_key.clone().into()),
                ("openai_model", self.subtitles.openai_model.clone().into()),
                ("openai_base", self.subtitles.openai_base.clone().into()),
                ("max_wait_secs", self.subtitles.max_wait_secs.into()),
                ("strip_sdh", self.subtitles.strip_sdh.into()),
                ("auto_sync", self.subtitles.auto_sync.into()),
                ("style", Json::obj(vec![
                    ("font", st.font.clone().into()),
                    ("size", st.size.into()),
                    ("bold", st.bold.into()),
                    ("italic", st.italic.into()),
                    ("underline", st.underline.into()),
                    ("text_color", st.text_color.clone().into()),
                    ("text_opacity", st.text_opacity.into()),
                    ("border_color", st.border_color.clone().into()),
                    ("border_size", st.border_size.into()),
                    ("shadow_color", st.shadow_color.clone().into()),
                    ("shadow_offset", st.shadow_offset.into()),
                    ("bg_enabled", st.bg_enabled.into()),
                    ("bg_color", st.bg_color.clone().into()),
                    ("bg_opacity", st.bg_opacity.into()),
                    ("position", st.position.into()),
                    ("force_style", st.force_style.into()),
                    ("align_x", st.align_x.clone().into()),
                    ("margin_x", st.margin_x.into()),
                ])),
            ])),
            ("upscaling", Json::obj(vec![
                ("enabled", self.upscaling.enabled.into()),
                ("mode", self.upscaling.mode.clone().into()),
                ("quality", self.upscaling.quality.clone().into()),
                ("clamp_highlights", self.upscaling.clamp_highlights.into()),
                ("max_source_height", self.upscaling.max_source_height.into()),
                ("gpu_hq", self.upscaling.gpu_hq.into()),
                ("scaler", self.upscaling.scaler.clone().into()),
            ])),
            ("backup", Json::obj(vec![
                ("auto", self.backup.auto.into()),
                ("every_hours", self.backup.every_hours.into()),
                ("keep", self.backup.keep.into()),
                ("folder", self.backup.folder.clone().into()),
                ("last", (self.backup.last as f64).into()),
                ("media", self.backup.media.into()),
                ("cloud", self.backup.cloud.to_json()),
            ])),
            ("mal", Json::obj(vec![
                ("client_id", self.mal.client_id.clone().into()),
                ("client_secret", self.mal.client_secret.clone().into()),
                ("redirect", self.mal.redirect.clone().into()),
                ("access_token", self.mal.access_token.clone().into()),
                ("refresh_token", self.mal.refresh_token.clone().into()),
                ("expires_at", (self.mal.expires_at as f64).into()),
                ("user", self.mal.user.clone().into()),
            ])),
            ("translate", self.translate.to_json()),
            ("addons", Json::Arr(self.addons.iter().map(|a| Json::obj(vec![
                ("url", a.url.clone().into()),
                ("enabled", a.enabled.into()),
                ("manifest", a.manifest.clone()),
            ])).collect())),
        ])
    }

    pub fn path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    pub fn load(dir: &Path) -> Settings {
        let path = Settings::path(dir);
        let Ok(text) = fs::read_to_string(&path) else { return Settings::default() };
        match Json::parse(&text) {
            Ok(j) => Settings::from_json(&j),
            Err(_) => {
                // A file that cannot be read is kept aside: the next save would overwrite it, keys and all.
                let _ = fs::copy(&path, path.with_extension("json.bad"));
                Settings::default()
            }
        }
    }

    /// The file can hold API keys, so on Unix it is private to the user.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = Settings::path(dir);
        let tmp = dir.join("settings.json.tmp");
        fs::write(&tmp, self.to_json().pretty()).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
        fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    /// Target subtitle language: the setting, or the system language when empty.
    pub fn target_lang(&self, system: &str) -> String {
        let t = crate::util::lang_base(&self.subtitles.target_lang);
        if t.is_empty() { crate::util::lang_base(system) } else { t }
    }
}

pub fn load_library(dir: &Path) -> Vec<MetaItem> {
    load_list(dir, "library.json")
}

pub fn save_library(dir: &Path, items: &[MetaItem]) -> Result<(), String> {
    save_list(dir, "library.json", items)
}

/// A list of titles kept in a json file (library, favorites).
pub fn load_list(dir: &Path, file: &str) -> Vec<MetaItem> {
    fs::read_to_string(dir.join(file))
        .ok()
        .and_then(|t| Json::parse(&t).ok())
        .map(|j| j.items().iter().map(MetaItem::from_json).filter(|m| !m.id.is_empty()).collect())
        .unwrap_or_default()
}

/// Where the full page of a title in the library or the favourites is kept (one file per title).
fn detail_file(dir: &Path, id: &str) -> PathBuf {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(60).collect();
    dir.join("detail-cache").join(format!("{}-{}.json", safe, crate::util::fnv_hex(id)))
}

/// The saved page of a title, if one was kept.
pub fn load_detail(dir: &Path, id: &str) -> Option<MetaItem> {
    let t = fs::read_to_string(detail_file(dir, id)).ok()?;
    let m = MetaItem::from_json(&Json::parse(&t).ok()?);
    (m.id == id).then_some(m)
}

/// Keeps the full page of a title (the library and favourites call this when a page is loaded).
pub fn save_detail(dir: &Path, m: &MetaItem) {
    let f = detail_file(dir, &m.id);
    if let Some(parent) = f.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(f, m.to_full_json().pretty());
}

pub fn save_list(dir: &Path, file: &str, items: &[MetaItem]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    fs::write(dir.join(file), Json::Arr(items.iter().map(|m| m.to_json()).collect()).pretty()).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("rh-set-{}-{}", std::process::id(), crate::util::fnv_hex(&format!("{:?}", std::time::Instant::now()))));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn defaults_include_the_default_addons() {
        let d = Settings::default();
        assert_eq!(d.addons.iter().map(|a| a.url.as_str()).collect::<Vec<_>>(), vec![CINEMETA, NETFLIX]);
        assert_eq!(d.general.pick_mode, "ask");
    }

    #[test]
    fn the_ios_theme_becomes_apple() {
        let s = Settings::from_json(&Json::parse(r#"{"general":{"theme":"ios"}}"#).unwrap());
        assert_eq!(s.general.theme, "apple");
        let s = Settings::from_json(&Json::parse(r#"{"general":{"theme":"gnome"}}"#).unwrap());
        assert_eq!(s.general.theme, "gnome");
    }

    #[test]
    fn saves_and_loads() {
        let dir = tmp();
        let mut s = Settings::default();
        s.debrid.api_key = "KEY".into();
        s.debrid.enabled = true;
        s.upscaling.mode = "C+A".into();
        s.subtitles.source_langs = vec!["ja".into(), "en".into()];
        s.addons[0].manifest = Json::parse(r#"{"name":"Cinemeta"}"#).unwrap();
        s.save(&dir).unwrap();
        let l = Settings::load(&dir);
        assert_eq!(l.debrid.api_key, "KEY");
        assert!(l.debrid.enabled);
        assert_eq!(l.upscaling.mode, "C+A");
        assert_eq!(l.subtitles.source_langs, vec!["ja", "en"]);
        assert_eq!(l.addons[0].name(), "Cinemeta");
        assert_eq!(l.addons.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_broken_file_gives_defaults() {
        let dir = tmp();
        assert_eq!(Settings::load(&dir).addons.len(), 2);
        fs::create_dir_all(&dir).unwrap();
        fs::write(Settings::path(&dir), "{not json").unwrap();
        assert_eq!(Settings::load(&dir).general.buffer_seconds, 12);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_settings_get_the_new_addon_once() {
        let old = Json::parse(r#"{"addons":[{"url":"https://v3-cinemeta.strem.io/manifest.json","enabled":true},{"url":"https://addon.example/manifest.json","enabled":true}]}"#).unwrap();
        let s = Settings::from_json(&old);
        assert_eq!(s.addons.len(), 3);
        assert_eq!(s.addons[1].url, NETFLIX);
        assert!(!s.addons.iter().any(|a| a.url.contains(ANISYNC_HOST)));
        // Saved again, it is version 3 and the user's choice to remove it sticks.
        let mut s2 = s.clone();
        s2.addons.retain(|a| a.url != NETFLIX);
        let back = Settings::from_json(&s2.to_json());
        assert!(!back.addons.iter().any(|a| a.url == NETFLIX));
    }

    #[test]
    fn the_old_mal_addon_goes_away_and_new_fields_round_trip() {
        let old_mal = "https://mal-stremio.vercel.app/1/manifest.json";
        let old = Json::parse(&format!(r#"{{"version":3,"addons":[{{"url":"{}","enabled":true}},{{"url":"{}","enabled":true}}],"subtitles":{{"libre_url":"http://localhost:5000"}}}}"#, CINEMETA, old_mal)).unwrap();
        let s = Settings::from_json(&old);
        assert_eq!(s.addons.iter().map(|a| a.url.as_str()).collect::<Vec<_>>(), vec![CINEMETA]);
        assert_eq!(s.subtitles.libre_url, "");
        let mut s = s;
        s.general.accent = "#3584e4".into();
        s.general.color_scheme = "dark".into();
        s.backup = Backup { auto: true, every_hours: 6, keep: 3, folder: "D:/b".into(), last: 1_700_000_000, media: false, cloud: crate::cloud::CloudSettings { enabled: true, provider: "onedrive".into(), ..Default::default() } };
        s.mal.client_id = "abc".into();
        s.mal.expires_at = 1_800_000_000;
        let back = Settings::from_json(&s.to_json());
        assert_eq!((back.general.accent.as_str(), back.general.color_scheme.as_str()), ("#3584e4", "dark"));
        assert_eq!((back.backup.auto, back.backup.every_hours, back.backup.keep, back.backup.last), (true, 6, 3, 1_700_000_000));
        assert_eq!((back.mal.client_id.as_str(), back.mal.expires_at), ("abc", 1_800_000_000));
    }

    #[test]
    fn only_the_shipped_anisync_address_is_removed() {
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert!(!is_shipped_anisync("https://anisync.qzz.io/someone-else/manifest.json"));
        assert!(!is_shipped_anisync(CINEMETA));
        // A user's own AniSync survives the version 5 clean-up and later saves.
        let own = "https://anisync.qzz.io/my-own-token/manifest.json";
        let old = Json::parse(&format!(r#"{{"version":4,"addons":[{{"url":"{}","enabled":true}},{{"url":"{}","enabled":true}}]}}"#, CINEMETA, own)).unwrap();
        let s = Settings::from_json(&old);
        assert_eq!(s.addons.iter().map(|a| a.url.as_str()).collect::<Vec<_>>(), vec![CINEMETA, own]);
        assert_eq!(Settings::from_json(&s.to_json()).addons.len(), 2);
    }

    /// The voice settings start automatic and survive a save.
    #[test]
    fn voice_settings_round_trip() {
        let d = Settings::default();
        assert_eq!((d.general.voice_input.as_str(), d.general.voice_model.as_str(), d.general.voice_output.as_str(), d.general.voice_pause), ("auto", "auto", "auto", 0));
        assert_eq!((d.general.voice_margin, d.general.kaiwa_autosend, d.general.kaiwa_handsfree), (10, true, true));
        let old = Settings::from_json(&Json::parse(r#"{"version":3,"general":{"kaiwa_ai":"ollama"}}"#).unwrap());
        assert_eq!((old.general.voice_input.as_str(), old.general.voice_output.as_str()), ("auto", "auto"));
        let mut s = d.clone();
        s.general.voice_input = "local".into();
        s.general.voice_model = "kotoba".into();
        s.general.voice_output = "local".into();
        s.general.voice_local = "Microsoft Haruka Desktop".into();
        s.general.voice_pause = 2500;
        s.general.voice_margin = 16;
        s.general.kaiwa_handsfree = false;
        let back = Settings::from_json(&s.to_json());
        assert_eq!((back.general.voice_input.as_str(), back.general.voice_model.as_str(), back.general.voice_output.as_str()), ("local", "kotoba", "local"));
        assert_eq!((back.general.voice_local.as_str(), back.general.voice_pause), ("Microsoft Haruka Desktop", 2500));
        assert_eq!((back.general.voice_margin, back.general.kaiwa_handsfree, back.general.kaiwa_autosend), (16, false, true));
    }

    /// The local AI is kept on desktop; on Android and iOS it falls back to a cloud translator (or the automatic choice).
    #[test]
    fn the_local_ai_is_desktop_only() {
        let saved = Json::parse(r#"{"version":3,"subtitles":{"provider":"ollama","gemini_key":"g"},"translate":{"per_use":{"bubbles":"ollama","forced":"azure"}},"general":{"kaiwa_ai":"ollama","kaiwa_level":"N4"}}"#).unwrap();
        let s = Settings::from_json(&saved);
        assert_eq!(s.translate.choice("forced"), "azure");
        assert_eq!(s.general.kaiwa_level, "N4");
        if crate::platform::DESKTOP_AI {
            assert_eq!(s.subtitles.provider, "ollama");
            assert_eq!(s.translate.choice("bubbles"), "ollama");
        } else {
            assert_eq!(s.subtitles.provider, "gemini");
            assert_eq!(s.translate.choice("bubbles"), "auto");
        }
        let mut sb = Settings::default().subtitles;
        assert_eq!(sb.cloud_fallback(), "libretranslate");
        sb.openai_key = "o".into();
        assert_eq!(sb.cloud_fallback(), "openai");
        sb.claude_key = "c".into();
        assert_eq!(sb.cloud_fallback(), "claude");
    }

    #[test]
    fn style_and_accounts_round_trip() {
        let mut s = Settings::default();
        s.subtitles.style.font = "Cantarell".into();
        s.subtitles.style.underline = true;
        s.subtitles.style.bg_enabled = true;
        s.subtitles.style.bg_opacity = 35;
        s.subtitles.provider = "gemini".into();
        s.subtitles.gemini_key = "G".into();
        s.anime_skip.client_id = "C".into();
        s.anime_skip.token = "T".into();
        s.general.skip_seconds = 85;
        let b = Settings::from_json(&s.to_json());
        assert_eq!(b.subtitles.style, s.subtitles.style);
        assert_eq!(b.subtitles.gemini_key, "G");
        assert_eq!(b.anime_skip.client_id, "C");
        assert_eq!(b.general.skip_seconds, 85);
    }

    #[test]
    fn target_language_falls_back_to_system() {
        let mut s = Settings::default();
        assert_eq!(s.target_lang("pt-BR"), "pt");
        s.subtitles.target_lang = "es".into();
        assert_eq!(s.target_lang("pt-BR"), "es");
    }

    #[test]
    fn library_round_trip() {
        let dir = tmp();
        let m = MetaItem { id: "tt1".into(), kind: "movie".into(), name: "A".into(), ..Default::default() };
        save_library(&dir, &[m]).unwrap();
        assert_eq!(load_library(&dir)[0].name, "A");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod ui_lang_tests {
    use super::*;

    #[test]
    fn ui_lang_follows_main_known_language() {
        let mut s = Settings::default();
        assert_eq!(s.general.language, "system");
        assert_eq!(s.general.ui_lang("en-US"), "en");
        s.general.known_langs = vec!["pt".into(), "en".into()];
        assert_eq!(s.general.ui_lang("en-US"), "pt");
        s.general.known_langs = vec!["ja".into(), "pt-BR".into()];
        assert_eq!(s.general.ui_lang("en-US"), "pt");
        s.general.known_langs = vec!["ja".into()];
        assert_eq!(s.general.ui_lang("es_ES"), "es");
        s.general.language = "en".into();
        s.general.known_langs = vec!["pt".into()];
        assert_eq!(s.general.ui_lang("pt-BR"), "en");
    }

    #[test]
    fn old_default_english_becomes_automatic() {
        let mut s = Settings::default();
        s.general.language = "pt".into();
        let mut j = s.to_json();
        // a settings file from before "ui_language": only "language", with the old default
        let mut g = j.get("general").clone();
        g.set("language", "en".into());
        if let Json::Obj(v) = &mut g {
            v.retain(|(k, _)| k != "ui_language");
        }
        j.set("general", g);
        let back = Settings::from_json(&j);
        assert_eq!(back.general.language, "system");
        // a language picked on purpose stays, and a saved file keeps it
        let again = Settings::from_json(&s.to_json());
        assert_eq!(again.general.language, "pt");
        // the preferences write "ui_language" over a file that already has it: the new value wins
        let mut j3 = again.to_json();
        let mut g3 = j3.get("general").clone();
        g3.set("ui_language", Json::Str("es".into()));
        j3.set("general", g3);
        assert_eq!(Settings::from_json(&j3).general.language, "es");
    }
}
