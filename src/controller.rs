//! Application logic without any window code: catalogs, title pages, streams, the playback
//! pipeline, Anime Skip and preview pictures. The Qt interface (through `qtbridge`) drives it and reacts to its events.
//!
//! Playing a stream runs these steps. The screen shows a "preparing" panel until all are done,
//! so the video starts with its subtitles already working:
//!   1. Resolve: direct link, Real-Debrid, or the torrent engine gives a URL for mpv.
//!   2. Load: mpv opens the URL paused, so it can fill its cache.
//!   3. Subtitles are found and translated on a worker thread while the buffer fills.
//!   4. Anime4K shaders are applied if the source is small enough to benefit.
//!   5. When the buffer is ready and the subtitles are ready (or the wait limit passed), mpv plays.
use crate::addons::{self, SubCand};
use crate::json::Json;
use crate::animeskip::{self, Segment};
use crate::skipdb;
use crate::artwork::{self, Art};
use crate::debrid::RealDebrid;
use crate::http::{Http, HttpRef};
use crate::model::{MetaItem, Row};
use crate::mpv::{Evt, Mpv};
use crate::net::UreqHttp;
use crate::runtime::{self, Runtime};
use crate::settings::{self, AddonEntry, Settings};
use crate::streams::{self, Stream};
use crate::subtitles::{self, SubFile, Wanted};
use crate::torrent::{self, Rqbit, RqbitProcess};
use crate::downloads::{self, Download};
use crate::mal::{self, ListEntry, MalData};
pub use controller_sub::{Forced, SubSource};
pub use controller_manga::{MangaList, MangaView, Reader};
pub use controller_marks::MarkScope;
pub use controller_xray::track_label;
pub use controller_novels::NovelReader;
pub use controller_study::StudySession;
use crate::jsrt;
pub use controller_modes::MODES;
pub use controller_notify::Notice;
pub use controller_tasks::{Finding, Task, TransJob};
pub use controller_dl::{DlPick, Overview, TorrentGroup, TorrentRow};
pub use controller_mal::{GenrePage, MalBundle, MalPart};

#[path = "controller_backup.rs"]
mod controller_backup;
#[path = "controller_update.rs"]
mod controller_update;
pub use controller_update::{UpdateDone, UpdateLive};
#[path = "controller_dl.rs"]
mod controller_dl;
#[path = "controller_mal.rs"]
mod controller_mal;
#[path = "controller_karaoke.rs"]
mod controller_karaoke;
#[path = "controller_pages.rs"]
mod controller_pages;
#[path = "controller_manga.rs"]
mod controller_manga;
#[path = "controller_novels.rs"]
mod controller_novels;
#[path = "controller_study.rs"]
mod controller_study;
#[path = "controller_clip.rs"]
mod controller_clip;
#[path = "controller_jlab.rs"]
mod controller_jlab;
#[path = "controller_ext.rs"]
mod controller_ext;
#[path = "controller_autocards.rs"]
mod controller_autocards;
#[path = "controller_pagedb.rs"]
mod controller_pagedb;
#[path = "controller_theme.rs"]
mod controller_theme;
#[path = "controller_xray.rs"]
mod controller_xray;
// Conversation (Study) and the local AI: desktop only; Android and iOS get commands that only say so.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[path = "controller_kaiwa.rs"]
mod controller_kaiwa;
#[cfg(any(target_os = "android", target_os = "ios"))]
#[path = "controller_kaiwa_mobile.rs"]
mod controller_kaiwa;
// The voice of the Study mode (hearing and speaking, Whisper on this computer): every platform; the local parts
// answer "not available" on Android and iOS (whisper_local_mobile.rs).
#[path = "controller_voice.rs"]
mod controller_voice;
#[path = "controller_marks.rs"]
mod controller_marks;
#[path = "controller_resume.rs"]
mod controller_resume;
#[path = "controller_modes.rs"]
mod controller_modes;
#[path = "controller_notify.rs"]
mod controller_notify;
#[path = "notify_study.rs"]
pub mod notify_study;
#[path = "controller_sub.rs"]
mod controller_sub;
#[path = "controller_audio.rs"]
pub mod controller_audio;
#[path = "controller_dub.rs"]
pub mod controller_dub;
#[path = "controller_translators.rs"]
mod controller_translators;
// local profiles: each person's library, favourites, history and study (src/profiles.rs)
#[path = "controller_profiles.rs"]
mod controller_profiles;
#[path = "controller_tasks.rs"]
mod controller_tasks;
#[path = "controller_learn.rs"]
pub mod controller_learn;
// AnkiWeb's shared decks (the site's services) and the browser of them in the Study mode
#[path = "ankishared.rs"]
pub mod ankishared;
#[path = "controller_ankishared.rs"]
pub mod controller_ankishared;
#[path = "controller_browser.rs"]
mod controller_browser;
#[path = "studyplay.rs"]
pub mod studyplay;
// learned words mixed into subtitles / texts in a known language (shared with the readers), and cards mined from
// single words said between silences
#[path = "wordmix.rs"]
pub mod wordmix;
#[path = "mining.rs"]
pub mod mining;
#[path = "timetrack.rs"]
pub mod timetrack;
#[path = "controller_time.rs"]
pub mod controller_time;
#[path = "controller_net.rs"]
pub mod controller_net;
#[path = "controller_privacy.rs"]
pub mod controller_privacy;
use crate::{anime4k, substyle, translate, util};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub struct Resolved {
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// Set when the engine streams a torrent: (engine address, torrent id, info hash).
    pub torrent: Option<(String, u64, String)>,
    /// Name and files of the torrent, so the Torrents page can list it.
    pub torrent_name: String,
    pub torrent_files: Vec<(String, u64)>,
}

impl Resolved {
    fn plain(url: String, headers: Vec<(String, String)>) -> Resolved {
        Resolved { url, headers, torrent: None, torrent_name: String::new(), torrent_files: vec![] }
    }
}

/// Which status line of the settings window a message belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgKind {
    RealDebrid,
    Translator,
    Shaders,
    Addon,
    AnimeSkip,
    Mal,
    Backup,
    AnkiWeb,
    Autocards,
    Update,
}

/// Things the window should redraw after `Core::poll`.
pub enum Event {
    /// The player's position / pause changed (observed from mpv): (seconds, paused).
    Playback(f64, bool),
    /// The X-Ray of the video changed.
    XRay,
    Home,
    Search,
    Detail,
    Streams,
    Session,
    Toast(String),
    /// A phone-style pop-up with the macaw (study reminders, Pomodoro…): see docs/mascot-contract.md.
    Banner { kind: String, title: String, body: String, mood: String, action: Option<Json> },
    Image(String, Vec<u8>),
    /// A page was upscaled (the file with the result), or it failed.
    Upscaled(String, Result<PathBuf, String>),
    /// The engine of the upscaling was downloaded (or failed).
    UpscaleEngine,
    /// The text of a page was read.
    PageText(String, Result<Vec<crate::ocr::Block>, String>),
    /// A translation asked by the reader arrived.
    Translated(u64, Result<Vec<String>, String>),
    TranslateProgress(u64),
    Thumb(f64, PathBuf),
    Info(MsgKind, String),
    Stats,
    Calendar,
    Progress,
    /// MyAnimeList data of the open title changed.
    Mal,
    Genre,
    Downloads,
    DlPick,
    /// A backup was restored: settings, theme and lists changed.
    Restored,
    /// The list of profiles changed (a name, a picture, the one in use).
    Profiles,
    /// The window should open this address in the browser.
    OpenUrl(String),
    /// A line for the log of the window (reelhouse-qt.log), such as the `[live]` lines of Gemini Live.
    Log(String),
    /// Bring the window to the front.
    Raise,
    /// Close the app (an update replaces it).
    Quit,
    /// The library cards got their covers and texts.
    Library,
    /// The frame asked by `browser_grab_frame_async`: (request number, the JPEG or why not, milliseconds).
    LensFrame(u64, Result<String, String>, u64),
    /// Open this title in the app.
    OpenItem(MetaItem),
    /// The list of new episodes changed.
    Notices,
    /// The mode (Video, Images, Text) changed.
    Mode,
    /// A source catalog was read.
    Extensions,
    /// Images mode: the Explore list, the library, the series page and the reader changed.
    MangaList,
    MangaLibrary,
    MangaView,
    MangaPages,
    /// Suwayomi: the sources, the extensions or the status changed.
    MangaSources,
    /// Text mode: the Explore list, library, series page, reader text and plugins changed.
    Study,
    NovelList,
    NovelLibrary,
    NovelView,
    NovelText,
    NovelPlugins,
}

pub enum Msg {
    /// IP protection: a Cloudflare WARP tunnel made (its wg-quick file) or why not (controller_privacy.rs).
    PrivacyWarp(Result<String, String>),
    Row(u64, usize, Row),
    HomeError(u64, String),
    HomeDone(u64),
    SearchRow(u64, usize, Row),
    SearchDone(u64),
    Manifests(Vec<AddonEntry>, Vec<String>),
    AddonInstalled(Result<(Vec<AddonEntry>, String), String>),
    /// The one addon box of Preferences: the link and the mode its answer belongs to.
    AddonKind(String, Option<crate::addon_kind::Kind>),
    Meta(u64, Option<MetaItem>),
    Art(u64, Art),
    Streams(u64, Vec<Stream>, Vec<String>),
    /// The episode file's own size inside a pack (gen, info hash, file bytes, whole torrent bytes).
    FileSize(u64, String, u64, u64),
    Info(MsgKind, String),
    ShaderDone(Result<usize, String>),
    Resolved(u64, Result<Resolved, String>),
    ResolveStatus(u64, String),
    SubStatus(u64, String),
    SubDone(u64, Result<Option<SubFile>, String>),
    /// token, priority (higher wins), where the times came from, the times
    Segments(u64, u8, String, Result<Vec<Segment>, String>),
    Karaoke(u64, Result<Vec<crate::karaoke::Song>, String>, String),
    Upscaled(String, Result<PathBuf, String>),
    UpscaleInstalled(Result<PathBuf, String>),
    PageText(String, Result<Vec<crate::ocr::Block>, String>),
    /// reader generation (see `controller_pages`), request id, the texts
    Translated(u64, u64, Result<Vec<String>, String>),
    TranslateProgress(u64, u64, usize, usize),
    AsLogin(Result<animeskip::Login, String>),
    Image(String, Vec<u8>),
    Thumb(f64, PathBuf),
    Stats(Option<torrent::TorrentStats>),
    Calendar(u64, Vec<CalEntry>, String),
    SubCands(u64, Vec<SubCand>),
    SubVariant(u64, Result<(PathBuf, String, String), String>),
    Prefetched(String, Vec<Stream>, usize, Result<Resolved, String>),
    Mal(u64, MalPart),
    MalList(Vec<ListEntry>, Vec<String>),
    MalLogin(Result<(mal::Tokens, String), String>),
    MalTokens(mal::Tokens),
    MalMy(u64, Result<Option<mal::MyStatus>, String>),
    Genre(u64, u32, Result<(Vec<mal::Card>, bool), String>),
    LibraryFill(Vec<MetaItem>),
    /// A frame for the quick search's Google Lens: (request number, the JPEG or why not, milliseconds it took).
    LensFrame(u64, Result<String, String>, u64),
    NewEps(Vec<controller_notify::Found>),
    MangaList(u64, usize, Result<(Vec<crate::manga::Series>, bool), String>),
    MangaView(u64, Option<crate::manga::Series>, Result<Vec<crate::manga::Chapter>, String>),
    MangaPages(u64, Result<Vec<String>, String>),
    MangaCounts(Vec<(String, u32)>),
    SuwayomiSources(Result<Vec<crate::suwayomi::Source>, String>),
    SuwayomiExts(Result<Vec<crate::suwayomi::Extension>, String>),
    SuwayomiDone(String),
    SuwayomiStatus(String),
    SuwayomiUp(Result<crate::suwayomi_host::Host, String>),
    StudyImported(Result<crate::study::Collection, String>),
    AnkiWebLogin(Result<(String, String), String>),
    ExtReply(String, String, Result<Json, String>),
    AutocardDone(Result<String, String>),
    /// Anki was not answering: the fields of a Basic card for the app's own deck (Seu Reels), media already saved
    AutocardLocal(Vec<String>),
    ThemeFound(u64, String, Result<crate::themesong::Video, String>),
    /// The video of an opening / ending for the X-Ray: request number, the song and the address mpv plays.
    ThemeStream(u64, Result<(crate::themesong::Video, crate::themesong::Stream), String>),
    XRayDone(u64, Result<crate::xray::XRay, String>),
    XRayPerson(String, Result<Json, String>),
    /// the session it was asked for, and the trivia of its episode
    XRayTrivia(u64, Result<crate::xray_trivia::Trivia, String>),
    Kaiwa(controller_kaiwa::KaiwaMsg),
    Voice(controller_voice::VoiceMsg),
    AutocardTested(Result<String, String>),
    /// the collection already read on the worker thread, and AnkiWeb's message
    AnkiWebPulled(Result<(crate::study::AnkiDb, String), String>),
    ClipDone(Result<controller_clip::ClipResult, String>),
    /// A word of a Jlab card looked up (note id, word, what the dictionary gave)
    JlabLooked(i64, String, Option<crate::jlab::Look>),
    /// A moment of the episode was cut for a mined card (mining.rs): the player session, the clip.
    Mined(u64, Result<mining::Mined, String>),
    /// A Learn course pack read on a worker thread: its language and the pack.
    LearnPack(String, Result<crate::learn::Pack, String>),
    /// AnkiWeb shared decks: a search (generation, words), a deck's page, the download of a deck (id, done, total)
    /// and its end (the package file).
    SharedList(u64, String, Result<Vec<ankishared::DeckRow>, String>),
    SharedInfo(u64, Result<ankishared::DeckInfo, String>),
    SharedProgress(u32, u64, Option<u64>),
    SharedDownloaded(u32, Result<PathBuf, String>),
    NovelCatalog(Result<Vec<crate::novels::Plugin>, String>),
    NovelInstalled(Result<crate::novels::Plugin, String>),
    NovelList(u64, usize, Result<(Vec<crate::manga::Series>, bool), String>),
    NovelView(u64, Result<(crate::manga::Series, Vec<crate::manga::Chapter>), String>),
    NovelText(u64, Result<String, String>),
    NovelCounts(Vec<(String, u32)>),
    NovelImported(Result<crate::manga::Series, String>),
    MalLinked(String, Option<u64>),
    Extensions(String, Result<Vec<crate::extensions::Ext>, String>),
    CardOpen(u64, Box<MetaItem>),
    Taste(u64, Vec<Row>),
    Cover(String, Option<(String, String)>),
    DlMissing(Box<MetaItem>, Box<Stream>, Vec<(i64, i64, String)>),
    DlSibling(Box<MetaItem>, i64, i64, Option<Box<Stream>>),
    DlSiblingsDone(usize, usize),
    Forced(u64, Result<Forced, String>),
    /// The subtitle in the language of the audio for word mixing: (episode key, where from, the file).
    OrigSubs(String, String, Result<PathBuf, String>),
    /// The automatic sync of an external audio track: session token, file, result.
    AudioSync(u64, String, Result<crate::audiosync::Match, String>),
    /// The sync of the subtitle with the speech (crate::subsync): session token, asked by itself, result.
    SubSync(u64, bool, Result<crate::subsync::Match, String>),
    /// A dub found in another torrent of the episode (controller_dub): session token, its name, the stream opened.
    DubFound(u64, u64, String, String, Result<Resolved, String>),
    /// how opening the other stream goes (token, gen, the engine's status line)
    DubStatus(u64, u64, String),
    ForcedProgress(u8),
    BackupDone(Result<(String, usize, u64), String>, bool, u64),
    Restored(Result<usize, String>),
    Cloud(crate::cloud::Done),
    Update(UpdateDone),
    DlStreams(u64, Vec<Stream>, Vec<String>),
    /// The result and the hash of the torrent.
    DlAdded(Result<Download, String>, String),
    DlProgress(String, f64, Vec<usize>, bool),
    DlRemoved(String, bool),
    /// A watched episode's file was deleted (info hash of its torrent).
    DlPurged(String),
    /// The cache limit freed this many bytes.
    DlEvicted(u64),
}

pub struct StreamsState {
    pub kind: String,
    pub key: String,
    pub label: String,
    pub loading: bool,
    pub list: Vec<Stream>,
    pub errors: Vec<String>,
}

/// Which episode of which show is playing (for Anime Skip).
#[derive(Clone, Debug, Default)]
pub struct EpInfo {
    pub show: String,
    pub season: Option<i64>,
    pub episode: Option<i64>,
}

pub struct Detail {
    pub gen: u64,
    pub meta: MetaItem,
    pub loading: bool,
    pub season: i64,
    pub streams: Option<StreamsState>,
    pub rating: Option<String>,
    pub autoplay: bool,
    pub ep: EpInfo,
    /// MyAnimeList data: information, recommendations, reviews, the user's status.
    pub mal: MalData,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Stage {
    Resolving,
    Loading,
    Buffering,
    Playing,
    /// Waiting for the user to accept, once, that a torrent shares pieces with other people (P2P).
    Consent,
    Failed(String),
}

pub enum SubJob {
    Skipped,
    Running,
    Done(Option<SubFile>),
    Failed(String),
    TimedOut,
}

pub struct Session {
    /// Stays the same when the quality is switched. `token` changes with every restart.
    pub id: u64,
    pub token: u64,
    pub title: String,
    pub kind: String,
    pub sid: String,
    pub ep: EpInfo,
    pub streams: Vec<Stream>,
    pub cur: usize,
    /// Sources skipped on their own because they found no peers (the next one is tried, up to 3 times).
    pub auto_skips: u8,
    pub stage: Stage,
    pub resolve_msg: String,
    pub sub_msg: String,
    pub sub_job: SubJob,
    pub resolved: Option<Resolved>,
    pub file_loaded: bool,
    pub loaded_at: Option<Instant>,
    pub buffer_ready: bool,
    pub buffer_done_at: Option<Instant>,
    pub upscale_done: bool,
    pub embedded_checked: bool,
    pub start_pos: f64,
    pub cancel: Arc<AtomicBool>,
    pub notes: Vec<String>,
    pub segments: Vec<Segment>,
    /// Segment starts the user already skipped or dismissed, so the button does not come back.
    pub skipped: Vec<i64>,
    pub segments_fetched: bool,
    pub seg_prio: u8,
    pub seg_source: String,
    pub kara: controller_karaoke::KaraState,
    pub chapters: Vec<(f64, String)>,
    /// Engine address, torrent id and info hash when the engine streams this video.
    pub torrent: Option<(String, u64, String)>,
    /// Already counted as watched.
    pub marked: bool,
    /// The file of this episode in the torrents folder, noted when it was watched to the end:
    /// deleted once the player lets go of it (controller_dl).
    pub finished: Option<controller_dl::Finished>,
    pub next_dismissed: bool,
    pub next_started: bool,
    pub meta: Option<MetaItem>,
    pub sub_cands: Vec<SubCand>,
    pub started: Instant,
    pub prefetch_started: bool,
    /// The opening / ending card (see controller_theme).
    pub theme: controller_theme::ThemeState,
    /// X-Ray of the episode (see controller_xray).
    pub xray: controller_xray::XRayState,
}

enum ThumbCmd {
    Load(String, Vec<(String, String)>),
    Seek(f64),
    Quit,
}

pub struct Core {
    pub http: HttpRef,
    /// The data folder: what every profile shares (preferences, addons, keys, engines, caches).
    pub data_dir: PathBuf,
    /// The folder of the active profile's own data (library, favourites, history, study, added books): the data
    /// folder itself for the main profile, `<data>/profiles/<id>` for the others (see `profiles.rs`).
    pub profile_dir: PathBuf,
    /// The people who use the app on this computer, and who is using it now.
    pub profiles: crate::profiles::Store,
    pub rt: Runtime,
    pub settings: Settings,
    pub library: Vec<MetaItem>,
    pub system_lang: String,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    engine: Arc<Mutex<Option<RqbitProcess>>>,

    pub rows: Vec<(usize, Arc<Row>)>,
    home_gen: u64,
    pub home_loading: bool,
    pub home_errors: Vec<String>,
    pub search_query: Option<String>,
    pub search_rows: Vec<(usize, Arc<Row>)>,
    search_gen: u64,
    pub search_loading: bool,
    pub detail: Option<Detail>,
    detail_gen: u64,
    stream_gen: u64,

    pub mpv: Option<Arc<Mpv>>,
    pub mpv_error: Option<String>,
    pub session: Option<Session>,
    token_counter: u64,
    thumb: Option<Sender<ThumbCmd>>,
    pub shader_busy: bool,
    events: Vec<Event>,
    images: Arc<ImageQueue>,
    /// Episodes and movies that were watched (ids), kept in watched.json.
    pub watched: HashSet<String>,
    /// Pictures the user chose for a title: id -> (poster, background). "none" hides the picture.
    pub art: HashMap<String, (Option<String>, Option<String>)>,
    pub torrent_stats: Option<torrent::TorrentStats>,
    stats_busy: bool,
    pub progress: Vec<Progress>,
    last_progress_save: Instant,
    resume_pos: Option<f64>,
    /// Where each episode stopped and its tracks (resume.json, controller_resume).
    ep_resume: Vec<controller_resume::EpResume>,
    track_restore: Option<controller_resume::TrackRestore>,
    pub calendar: Vec<CalEntry>,
    pub calendar_loading: bool,
    pub calendar_info: String,
    pub calendar_at: Option<Instant>,
    calendar_gen: u64,
    prefetch: Option<Prefetch>,
    preresolved: Option<Resolved>,
    pub favorites: Vec<MetaItem>,
    pub mal_list: Vec<ListEntry>,
    pub mal_list_info: String,
    external_watched: HashSet<String>,
    ext_off: HashSet<String>,
    library_filling: bool,
    pub manga: crate::manga::State,
    pub manga_list: MangaList,
    pub manga_view: Option<MangaView>,
    pub manga_reader: Option<Reader>,
    manga_gen: u64,
    pub novels: crate::manga::State,
    pub novel_installed: Vec<crate::novels::Plugin>,
    pub novel_catalog: Vec<crate::novels::Plugin>,
    pub novel_catalog_status: String,
    novel_catalog_loading: bool,
    pub novel_list: MangaList,
    pub novel_view: Option<MangaView>,
    pub novel_reader: Option<NovelReader>,
    pub study: crate::study::Collection,
    pub study_session: Option<controller_study::StudySession>,
    /// Jlab mode of the Study (jlab.rs): decks in it, settings, per-note phase and lookups
    pub jlab: crate::jlab::Jlab,
    /// the word being looked up for a Jlab card ("" = none)
    pub jlab_busy: String,
    /// What the user knows of the study language (knowledge.md), read by the player quizzes and Study.
    pub knowledge: crate::knowledge::Knowledge,
    /// The Learn area (Duolingo-style lessons): its course, progress and the lesson in progress.
    pub learn: controller_learn::LearnState,
    /// The browser of AnkiWeb's shared decks (controller_ankishared).
    pub shared: controller_ankishared::SharedState,
    /// Study reminders shown as banners with the macaw (notify_study.rs).
    pub nudges: notify_study::Nudges,
    /// Study time per day and the Pomodoro timer (controller_time.rs, timetrack.rs).
    pub time: controller_time::TimeState,
    study_dirty: u32,
    pub study_importing: bool,
    /// Players of the Study mode: card sounds, and the clip window.
    pub study_audio: Option<Mpv>,
    /// An Autocards export is running.
    pub autocard_busy: bool,
    /// Player of the opening / ending videos shown inside the X-Ray.
    pub theme_player: controller_theme::ThemePlayer,
    /// Study while watching: the questions planned over the episode and the one on screen (studyplay.rs).
    pub study_play: studyplay::StudyPlay,
    /// Cards mined from the episode playing (mining.rs).
    pub mining: mining::MiningState,
    /// The learned words the readers mix into their texts (`Core::wordmix_swap`, built when first asked).
    pub wordmix_readers: Option<wordmix::Lexicon>,
    /// Playback clock observed from mpv (time-pos, pause).
    pub clock: controller_xray::Clock,
    /// Translation database of the chapter in the reader: its file and contents.
    pub page_db: Option<(PathBuf, crate::pagedb::Chapter)>,
    /// Extensions of the app (extensions folder), with their hooks.
    pub app_exts: Vec<crate::appext::AppExt>,
    /// The clip of a clip card, played inside the card: a second mpv whose pictures are drawn into memory (like the
    /// X-Ray videos) and shown by the interface (`rh_clip_frame`).
    pub study_clip: Option<std::sync::Arc<Mpv>>,
    /// mpv has a new picture of the clip the interface has not taken yet.
    pub study_clip_fresh: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// More about the people of the X-Ray, by their link (`xray::person_info`): the answer, `{"loading":true}` or
    /// `{"error":…}`. Kept for the whole run: the same character comes back episode after episode.
    pub xray_people: std::collections::HashMap<String, Json>,
    /// Conversation in the Study mode (controller_kaiwa.rs)
    pub kaiwa: controller_kaiwa::KaiwaState,
    /// The local AI of Conversation: Ollama found, downloaded or started by Reelhouse (ollama_local.rs)
    pub ollama: crate::ollama_local::Local,
    /// The voice of the Study mode (controller_voice.rs) and Whisper on this computer (whisper_local.rs)
    pub voice: controller_voice::VoiceState,
    pub whisper: crate::whisper_local::Local,
    /// AnkiWeb: busy with a login or a pull, the last status line, and when the last pull ended (unix seconds).
    pub ankiweb_busy: bool,
    pub ankiweb_status: String,
    pub ankiweb_last: i64,
    pub clip_busy: bool,
    pub hint_cache: Option<Vec<crate::study::Hint>>,
    pub hint_text: String,
    pub hint_cool: std::collections::HashMap<i64, Instant>,
    pub flash: Option<(crate::study::Hint, Instant)>,
    /// the card browser (deck, search text) and the statistics page (deck, 0 = all) when open
    pub study_browse: Option<(i64, String)>,
    pub study_stats: Option<i64>,
    novel_gen: u64,
    novel_refreshing: bool,
    novel_resume: u32,
    /// Results of the reader's text (OCR), translations and upscaling, kept for the Qt bridge (per page address, per request id).
    pub reader_text: HashMap<String, Result<Vec<crate::ocr::Block>, String>>,
    pub reader_translations: HashMap<u64, Result<Vec<String>, String>>,
    pub reader_tr_progress: HashMap<u64, (usize, usize)>,
    pub reader_upscaled: HashMap<String, Result<PathBuf, String>>,
    novel_saves: u32,
    mal_linking: HashSet<String>,
    pub manga_sources: Vec<crate::suwayomi::Source>,
    pub suwayomi_exts: Vec<crate::suwayomi::Extension>,
    pub suwayomi_status: String,
    suwayomi_host: Option<crate::suwayomi_host::Host>,
    suwayomi_booting: bool,
    suwayomi_boot_wanted: bool,
    suwayomi_port: u16,
    /// Android: port of the app's own extension host (docs/android-extensions.md), which replaces Suwayomi there.
    pub ext_host_port: Option<u16>,
    /// The host's folder: it installs only APKs saved inside it.
    pub ext_host_dir: std::path::PathBuf,
    suwayomi_idle: Option<std::time::Instant>,
    manga_refreshing: bool,
    manga_resume: u32,
    manga_page_saves: u32,
    pub ext_lists: HashMap<String, Vec<crate::extensions::Ext>>,
    pub ext_status: HashMap<String, String>,
    ext_loading: HashSet<String>,
    seen_latest: HashMap<String, String>,
    pub notices: Vec<Notice>,
    new_checked: Option<Instant>,
    checking_new: bool,
    card_gen: u64,
    mal_cache: Arc<Mutex<HashMap<u64, MalBundle>>>,
    mal_cancel: Arc<AtomicBool>,
    /// The login in progress: PKCE verifier and state.
    mal_pending: Option<(String, String)>,
    pub translating: Option<TransJob>,
    /// How the last translation ended (true = done), shown in the Translation tab.
    pub translate_note: Option<(bool, String)>,
    finding: Option<Finding>,
    pub genre: Option<GenrePage>,
    genre_gen: u64,
    /// "Because you watched…" rows from AniList.
    pub taste: Vec<Arc<Row>>,
    taste_gen: u64,
    covers: HashMap<String, (String, String)>,
    forced: Vec<Forced>,
    /// Audio files loaded for episodes (dubs) and their delays (see controller_audio).
    pub ext_audio: controller_audio::ExtAudioStore,
    pub audio_sync: controller_audio::AudioSyncState,
    cover_asked: HashSet<String>,
    pub downloads: Vec<Download>,
    dl_running: HashSet<String>,
    pub dl_pick: Option<DlPick>,
    dl_gen: u64,
    backup_busy: bool,
    /// The online copy of the backup while the app runs (sign-in, uploads, the list of backups online).
    pub cloud: crate::cloud::Live,
    /// The update of the app (controller_update.rs).
    pub update: UpdateLive,
    /// Research mode: history, downloads and preferences of the built-in browser (read on first use).
    pub browser: crate::browser::Browser,
}

struct ImageQueue {
    pending: Mutex<VecDeque<String>>,
    wake: Condvar,
    requested: Mutex<HashSet<String>>,
    /// (TMDB key, OMDb key) for backup pictures.
    keys: Mutex<(String, String)>,
    /// Folder of the pictures kept on disk.
    cache: PathBuf,
    /// The title each picture address belongs to, for finding the cover by name when the address fails.
    titles: Mutex<HashMap<String, String>>,
    /// How many times a picture could not be found (after a few, it is left alone).
    fails: Mutex<HashMap<String, u32>>,
}

pub fn send(tx: &Sender<Msg>, m: Msg) {
    let _ = tx.send(m);
}

fn free_port() -> Result<u16, String> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    l.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
}

/// The start of the error of a torrent that found no peers in time: the player moves on to the next source.
const NO_PEERS: &str = "No peers answered for this source.";

/// Starts the torrent engine when it is not running. Returns its address.
fn ensure_engine(
    http: &dyn Http,
    engine: &Mutex<Option<RqbitProcess>>,
    rqbit_exe: Option<&Path>,
    runtime_dir: &Path,
    dl_dir: &Path,
    status: &dyn Fn(String),
) -> Result<String, String> {
    // Android: the engine runs inside the app (no program to download or start).
    // IP protection: Tor refuses torrents unless allowed; a tunnel that does not work stops them (kill switch)
    crate::privacy::torrent_allowed()?;
    #[cfg(feature = "torrent-embed")]
    {
        let _ = (rqbit_exe, runtime_dir);
        status("Starting the torrent engine".to_string());
        let mut g = engine.lock().map_err(|_| "The torrent engine is busy.".to_string())?;
        if g.is_none() {
            let mut p = RqbitProcess::embedded(dl_dir, free_port()?)?;
            p.wait_ready(http, 20)?;
            *g = Some(p);
        }
        return Ok(g.as_ref().map(|p| p.base()).unwrap_or_default());
    }
    // The engine is downloaded once if it is not built in.
    #[allow(unreachable_code)]
    let exe = match rqbit_exe {
        Some(p) => p.to_path_buf(),
        None => torrent::install_engine(http, runtime_dir, status)?,
    };
    status("Starting the torrent engine".to_string());
    let mut g = engine.lock().map_err(|_| "The torrent engine is busy.".to_string())?;
    if g.is_none() {
        // The second try moves the peer port off 4240 (another torrent program may hold it); the third is for old engine
        // builds on systems that cannot open IPv6 sockets. An option an engine build does not know only fails that try.
        let mut last = String::new();
        for attempt in 0..3 {
            let port = free_port()?;
            let mut extra: Vec<String> = match attempt {
                0 => vec![],
                // with IP protection nothing listens, so no other peer port is needed
                1 if crate::privacy::active() => vec![],
                1 => vec!["--listen-port".to_string(), free_port()?.to_string()],
                _ => vec!["--ipv4-only".to_string()],
            };
            // IP protection: peers only through the tunnel (SOCKS5), no DHT, UPnP, local discovery or listening
            extra.extend(crate::privacy::rqbit_args());
            let started = RqbitProcess::start(&exe, dl_dir, port, &extra).and_then(|mut p| p.wait_ready(http, 20).map(|_| p));
            match started {
                Ok(p) => {
                    *g = Some(p);
                    break;
                }
                Err(e) => {
                    let tail = RqbitProcess::log_tail(dl_dir);
                    crate::debug::log("torrent", &format!("engine try {} ({:?}) failed: {} {}", attempt + 1, extra, e, tail));
                    // The first failure explains the most; later tries only add "unknown option".
                    if last.is_empty() {
                        last = format!("{} {}", e, tail);
                    }
                }
            }
        }
        if g.is_none() {
            return Err(last);
        }
    }
    Ok(g.as_ref().map(|p| p.base()).unwrap_or_default())
}

/// Turns a stream into a URL that mpv can open.
#[allow(clippy::too_many_arguments)]
fn resolve_stream(
    http: &dyn Http,
    st: &Settings,
    stream: &Stream,
    engine: &Mutex<Option<RqbitProcess>>,
    rqbit_exe: Option<&Path>,
    runtime_dir: &Path,
    dl_dir: &Path,
    sid: &str,
    status: &dyn Fn(String),
    cancelled: &dyn Fn() -> bool,
) -> Result<Resolved, String> {
    if let Some(path) = &stream.local {
        if !Path::new(path).is_file() {
            return Err("The saved file is not on the disk any more. Pick another source.".to_string());
        }
        return Ok(Resolved::plain(path.clone(), vec![]));
    }
    if let Some(url) = &stream.url {
        return Ok(Resolved::plain(url.clone(), stream.headers.clone()));
    }
    if let Some(y) = &stream.yt_id {
        return Ok(Resolved::plain(format!("https://www.youtube.com/watch?v={}", y), vec![]));
    }
    let magnet = streams::magnet(stream).ok_or_else(|| "This stream has no playable source.".to_string())?;
    if st.debrid.enabled && !st.debrid.api_key.trim().is_empty() {
        let rd = RealDebrid::new(http, &st.debrid.api_key);
        let url = rd.resolve(&magnet, stream.file_idx, &mut |m| status(m), cancelled)?;
        return Ok(Resolved::plain(url, vec![]));
    }
    // No Real-Debrid: stream the torrent directly.
    let base = ensure_engine(http, engine, rqbit_exe, runtime_dir, dl_dir, status)?;
    status("Looking for peers. This can take a minute.".to_string());
    let rq = Rqbit { http, base: base.clone() };
    // The file list first (nothing is downloaded), then only the file that was asked for: a season pack must not be
    // downloaded whole when one episode is played.
    let drop_engine = |e: String| {
        // Drop the engine so the next attempt starts a fresh one.
        if let Ok(mut g) = engine.lock() {
            *g = None;
        }
        format!("{} {}", e, RqbitProcess::log_tail(dl_dir)).trim().to_string()
    };
    // A torrent whose file list does not come in 45 s has no live peers (checked on a real network: an addon's
    // source with "71 seeders" sat 150 s): give up on it so the next source is tried (HP: stuck on "Looking for peers").
    let (_, listed) = rq.list_files_within(&magnet, 45).map_err(|e| {
        if e.contains("timed out") {
            format!("{} {}", NO_PEERS, e)
        } else {
            drop_engine(e)
        }
    })?;
    let (season, episode) = parse_episode_id(sid);
    let idx = torrent::pick_episode(&listed, stream.file_idx, season, episode).ok_or_else(|| "The torrent has no files.".to_string())?;
    let (fname, flen) = listed.get(idx).cloned().unwrap_or_default();
    status(format!("Downloading only this file: {} ({:.2} GB)", fname.rsplit('/').next().unwrap_or(&fname), flen as f64 / 1_073_741_824.0));
    let (id, name, files) = rq.add_only(&magnet, &[idx]).map_err(&drop_engine)?;
    let url = rq.stream_url(id, idx);
    if let Err(e) = rq.wait_stream(id, &url, 150, status, cancelled) {
        return Err(format!("{} {}", e, RqbitProcess::log_tail(dl_dir)).trim().to_string());
    }
    Ok(Resolved { url, headers: vec![], torrent: Some((base, id, stream.info_hash.clone().unwrap_or_default())), torrent_name: name, torrent_files: files })
}


/// The "next episode" card shown near the end of an episode.
#[derive(Clone, Debug)]
pub struct NextCard {
    pub title: String,
    /// "S02E08"
    pub label: String,
    /// the episode's own name ("" when it has none)
    pub name: String,
    pub seconds_left: f64,
    /// how long the card shows in all (the countdown bar runs from this to 0)
    pub total: f64,
    pub thumbnail: Option<String>,
    /// the series' backdrop or poster, shown when the episode has no picture or its address fails
    pub fallback: Option<String>,
}

/// The episode after the one playing (see `next_video`).
struct NextVideo {
    id: String,
    label: String,
    title: String,
    name: String,
    thumbnail: Option<String>,
    fallback: Option<String>,
}

/// One entry of "Continue watching".
#[derive(Clone, Debug)]
pub struct Progress {
    pub meta: MetaItem,
    pub video_id: String,
    pub label: String,
    pub position: f64,
    pub duration: f64,
    pub updated: u64,
}

impl Progress {
    pub fn fraction(&self) -> f64 {
        if self.duration > 0.0 {
            (self.position / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// An episode that comes out on a given day, for the calendar.
#[derive(Clone, Debug)]
pub struct CalEntry {
    pub date: String,
    pub show_id: String,
    pub show: String,
    pub poster: Option<String>,
    pub label: String,
    pub title: String,
    pub video_id: String,
}

/// The next episode, loaded in the background while the current one is still playing.
struct Prefetch {
    video_id: String,
    ready: Option<(Vec<Stream>, usize, Result<Resolved, String>)>,
}

fn load_watched(dir: &Path) -> HashSet<String> {
    std::fs::read_to_string(dir.join("watched.json"))
        .ok()
        .and_then(|t| crate::json::Json::parse(&t).ok())
        .map(|j| j.items().iter().map(|x| x.str()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default()
}

fn load_art(dir: &Path) -> HashMap<String, (Option<String>, Option<String>)> {
    let mut out = HashMap::new();
    if let Some(j) = std::fs::read_to_string(dir.join("artwork.json")).ok().and_then(|t| crate::json::Json::parse(&t).ok()) {
        for (id, v) in j.entries() {
            let p = Some(v.get("poster").str()).filter(|x| !x.is_empty());
            let b = Some(v.get("background").str()).filter(|x| !x.is_empty());
            if p.is_some() || b.is_some() {
                out.insert(id.clone(), (p, b));
            }
        }
    }
    out
}

impl Core {
    // ----- watched episodes -----

    pub fn mark_watched(&mut self, id: &str) {
        if self.watched.insert(id.to_string()) {
            self.save_watched();
            self.events.push(Event::Detail);
        }
    }

    fn save_watched(&self) {
        let mut ids: Vec<&String> = self.watched.iter().collect();
        ids.sort();
        let json = crate::json::Json::Arr(ids.into_iter().map(|i| i.as_str().into()).collect());
        let _ = std::fs::write(self.profile_dir.join("watched.json"), json.pretty());
    }

    // ----- pictures chosen by the user -----

    pub fn poster_for(&self, m: &MetaItem) -> Option<String> {
        self.art.get(&m.id).and_then(|a| a.0.clone()).or_else(|| m.poster.clone())
    }

    pub fn background_for(&self, m: &MetaItem) -> Option<String> {
        self.art.get(&m.id).and_then(|a| a.1.clone()).or_else(|| m.background.clone())
    }

    pub fn has_art_override(&self, id: &str) -> bool {
        self.art.contains_key(id)
    }

    /// `which` is "poster" or "background"; `value` None goes back to the original.
    pub fn set_art(&mut self, id: &str, which: &str, value: Option<String>) {
        let e = self.art.entry(id.to_string()).or_insert((None, None));
        if which == "poster" {
            e.0 = value;
        } else {
            e.1 = value;
        }
        if e.0.is_none() && e.1.is_none() {
            self.art.remove(id);
        }
        self.save_art();
        self.events.push(Event::Home);
        self.events.push(Event::Detail);
    }

    pub fn clear_art(&mut self, id: &str) {
        self.art.remove(id);
        self.save_art();
        self.events.push(Event::Home);
        self.events.push(Event::Detail);
    }

    fn save_art(&self) {
        let mut keys: Vec<&String> = self.art.keys().collect();
        keys.sort();
        let pairs: Vec<(&str, crate::json::Json)> = keys
            .iter()
            .map(|k| {
                let (p, b) = &self.art[*k];
                (k.as_str(), crate::json::Json::obj(vec![("poster", p.clone().unwrap_or_default().into()), ("background", b.clone().unwrap_or_default().into())]))
            })
            .collect();
        let _ = std::fs::write(self.data_dir.join("artwork.json"), crate::json::Json::obj(pairs).pretty());
    }

    // ----- playback extras -----

    pub fn speed(&self) -> f64 {
        self.mpv.as_ref().and_then(|m| m.get_f64("speed")).unwrap_or(1.0)
    }

    pub fn set_speed(&self, v: f64) {
        if let Some(m) = &self.mpv {
            let _ = m.set("speed", &format!("{:.2}", v.clamp(0.25, 4.0)));
        }
    }

    pub fn sub_delay(&self) -> f64 {
        self.mpv.as_ref().and_then(|m| m.get_f64("sub-delay")).unwrap_or(0.0)
    }

    pub fn set_sub_delay(&self, secs: f64) {
        if let Some(m) = &self.mpv {
            let _ = m.set("sub-delay", &format!("{:.2}", secs.clamp(-300.0, 300.0)));
        }
    }

    /// Where the subtitles sit: vertical position (0 top, 100 bottom), side alignment and side margin.
    pub fn set_sub_layout(&mut self, position: u32, align_x: &str, margin_x: u32) {
        let st = &mut self.settings.subtitles.style;
        st.position = position.min(100);
        st.align_x = align_x.to_string();
        st.margin_x = margin_x;
        self.apply_sub_style();
        self.save_settings();
    }

    /// Asks the engine for fresh connection numbers (they arrive as `Event::Stats`).
    pub fn refresh_stats(&mut self) {
        if self.stats_busy {
            return;
        }
        let Some((base, id, _)) = self.session.as_ref().and_then(|s| s.torrent.clone()) else {
            self.torrent_stats = None;
            return;
        };
        self.stats_busy = true;
        self.spawn(move |tx, http| {
            let rq = Rqbit { http: &*http, base };
            send(tx, Msg::Stats(rq.stats(id)));
        });
    }

    pub fn info_hash(&self) -> Option<String> {
        self.session.as_ref().and_then(|s| s.torrent.as_ref()).map(|t| t.2.clone()).filter(|h| !h.is_empty())
    }

    pub fn is_torrent_session(&self) -> bool {
        self.session.as_ref().map(|s| s.torrent.is_some()).unwrap_or(false)
    }

    /// Numbers straight from the player, as (label, value).
    pub fn player_stats(&self) -> Vec<(String, String)> {
        let Some(m) = &self.mpv else { return vec![] };
        let mut v = Vec::new();
        let (nch, src) = self.skip_info();
        v.push(("Chapters in file".to_string(), nch.to_string()));
        v.push(("Skip times".to_string(), src));
        v.push(("Karaoke".to_string(), self.karaoke_status()));
        if let Some(x) = m.get_f64("demuxer-cache-duration") {
            v.push(("Buffered".to_string(), format!("{:.0} s", x)));
        }
        if let Some(x) = m.get_f64("cache-speed") {
            v.push(("Read speed".to_string(), format!("{}/s", torrent::human_bytes(x as u64))));
        }
        if let Some(x) = m.get_f64("video-bitrate") {
            if x > 0.0 {
                v.push(("Video bitrate".to_string(), format!("{:.1} Mbps", x / 1_000_000.0)));
            }
        }
        if let Some(x) = m.get_f64("estimated-vf-fps") {
            v.push(("Frames per second".to_string(), format!("{:.1}", x)));
        }
        if let Some(x) = m.get_f64("frame-drop-count") {
            v.push(("Dropped frames".to_string(), format!("{:.0}", x)));
        }
        v
    }

    // ----- next episode -----

    /// The video after the one playing, in season and episode order.
    fn next_video(&self) -> Option<NextVideo> {
        let s = self.session.as_ref()?;
        let d = self.detail.as_ref()?;
        if !d.meta.is_series() {
            return None;
        }
        let mut vids: Vec<&crate::model::Video> = d.meta.videos.iter().filter(|v| v.season.unwrap_or(0) > 0 || v.episode.is_some()).collect();
        vids.sort_by_key(|v| (v.season.unwrap_or(0), v.episode.unwrap_or(0)));
        let i = vids.iter().position(|v| v.id == s.sid)?;
        let n = vids.get(i + 1)?;
        let label = format!("S{:02}E{:02}", n.season.unwrap_or(0), n.episode.unwrap_or(0));
        let title = if n.title.is_empty() { format!("{} {}", d.meta.name, label) } else { format!("{} · {}", label, n.title) };
        let fallback = d.meta.background.clone().or_else(|| d.meta.poster.clone()).filter(|u| !u.is_empty());
        Some(NextVideo { id: n.id.clone(), label, title, name: n.title.clone(), thumbnail: n.thumbnail.clone().filter(|u| !u.is_empty()), fallback })
    }

    /// Runs on every tick: counts watched episodes, and returns the "next episode" card when it is time.
    /// Also starts the next episode by itself when the video ends and auto-play is on.
    pub fn poll_playback(&mut self) -> Option<NextCard> {
        let mpv = self.mpv.clone()?;
        let (playing, sid, dismissed, started, marked, prefetch_started) = {
            let s = self.session.as_ref()?;
            (s.stage == Stage::Playing, s.sid.clone(), s.next_dismissed, s.next_started, s.marked, s.prefetch_started)
        };
        if !playing {
            return None;
        }
        let pos = mpv.get_f64("time-pos").unwrap_or(0.0);
        let dur = mpv.get_f64("duration").unwrap_or(0.0);
        let eof = mpv.get_flag("eof-reached").unwrap_or(false);
        self.sub_hint(&mpv);
        self.restore_tracks_step(&mpv);
        self.track_progress(pos, dur, eof);
        if dur > 1.0 && !self.session.as_ref().map(|s| s.segments_fetched).unwrap_or(true) {
            self.probe_segments(dur);
        }
        if dur > 60.0 && (eof || pos / dur >= 0.9) && !marked {
            if let Some(s) = self.session.as_mut() {
                s.marked = true;
            }
            self.mark_watched(&sid);
            self.dl_note_finished();
        }
        // (two fields, not a clone of the whole General: this runs on every tick of the player)
        let (autoplay_next, next_popup_secs) = (self.settings.general.autoplay_next, self.settings.general.next_popup_secs);
        if !autoplay_next || dur < 60.0 {
            return None;
        }
        let NextVideo { id, label, title, name, thumbnail, fallback } = self.next_video()?;
        let left = (dur - pos).max(0.0);
        // Five minutes before the end the next episode starts loading quietly.
        if left <= 300.0 && !prefetch_started {
            if let Some(s) = self.session.as_mut() {
                s.prefetch_started = true;
            }
            self.start_prefetch(&id);
        }
        if (eof || left < 1.0) && !started && !self.study_quiz_open() {
            if let Some(s) = self.session.as_mut() {
                s.next_started = true;
            }
            self.toast(&format!("Playing {}", title));
            self.choose_episode_with(&id, &label, true);
            return None;
        }
        if left <= next_popup_secs as f64 && !dismissed && !started {
            return Some(NextCard { title, label, name, seconds_left: left, total: (next_popup_secs as f64).max(1.0), thumbnail, fallback });
        }
        None
    }

    pub fn dismiss_next(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.next_dismissed = true;
        }
    }

    /// "Play now" on the next-episode card, or the next button of the player.
    pub fn play_next_now(&mut self) {
        if let Some(NextVideo { id, label, title, .. }) = self.next_video() {
            if let Some(s) = self.session.as_mut() {
                s.next_started = true;
            }
            self.toast(&format!("Playing {}", title));
            self.choose_episode_with(&id, &label, true);
        }
    }

    /// The episode before the one playing (controller L1 in the player).
    pub fn play_prev_now(&mut self) {
        let prev = (|| {
            let s = self.session.as_ref()?;
            let d = self.detail.as_ref()?;
            if !d.meta.is_series() {
                return None;
            }
            let mut vids: Vec<&crate::model::Video> = d.meta.videos.iter().filter(|v| v.season.unwrap_or(0) > 0 || v.episode.is_some()).collect();
            vids.sort_by_key(|v| (v.season.unwrap_or(0), v.episode.unwrap_or(0)));
            let i = vids.iter().position(|v| v.id == s.sid)?;
            let n = vids.get(i.checked_sub(1)?)?;
            Some((n.id.clone(), format!("S{:02}E{:02}", n.season.unwrap_or(0), n.episode.unwrap_or(0))))
        })();
        if let Some((id, label)) = prev {
            self.toast(&format!("Playing {}", label));
            self.choose_episode_with(&id, &label, true);
        }
    }

    pub fn has_next(&self) -> bool {
        self.next_video().is_some()
    }
}

fn load_progress(dir: &Path) -> Vec<Progress> {
    let Some(j) = std::fs::read_to_string(dir.join("progress.json")).ok().and_then(|t| crate::json::Json::parse(&t).ok()) else { return vec![] };
    let mut v: Vec<Progress> = j
        .items()
        .iter()
        .map(|e| Progress {
            meta: MetaItem::from_json(e.get("meta")),
            video_id: e.get("video_id").str(),
            label: e.get("label").str(),
            position: e.get("position").as_f64().unwrap_or(0.0),
            duration: e.get("duration").as_f64().unwrap_or(0.0),
            updated: e.get("updated").as_u64().unwrap_or(0),
        })
        .filter(|p| !p.meta.id.is_empty())
        .collect();
    v.sort_by_key(|p| std::cmp::Reverse(p.updated));
    v
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Core {
    // ----- continue watching -----

    fn save_progress_file(&self) {
        let items: Vec<crate::json::Json> = self
            .progress
            .iter()
            .map(|p| {
                crate::json::Json::obj(vec![
                    ("meta", p.meta.to_json()),
                    ("video_id", p.video_id.as_str().into()),
                    ("label", p.label.as_str().into()),
                    ("position", p.position.into()),
                    ("duration", p.duration.into()),
                    ("updated", (p.updated as f64).into()),
                ])
            })
            .collect();
        let _ = std::fs::write(self.profile_dir.join("progress.json"), crate::json::Json::Arr(items).pretty());
    }

    /// Remembers where the video is, a few seconds apart.
    fn track_progress(&mut self, pos: f64, dur: f64, eof: bool) {
        if dur < 60.0 {
            return;
        }
        let Some(s) = self.session.as_ref() else { return };
        let Some(meta) = s.meta.clone() else { return };
        let (video_id, label) = (s.sid.clone(), self.detail.as_ref().and_then(|d| d.streams.as_ref().map(|x| x.label.clone())).unwrap_or_default());
        let finished = eof || pos / dur >= 0.92;
        if finished {
            if self.resume_point(&video_id).is_some() {
                self.remember_episode(&meta.id, &video_id, pos, dur, true);
            }
            if let Some(i) = self.progress.iter().position(|p| p.meta.id == meta.id && p.video_id == video_id) {
                self.progress.remove(i);
                self.save_progress_file();
                self.events.push(Event::Progress);
            }
            return;
        }
        // Every 30 s (it was 5): each save writes progress.json and makes the UI rebuild the whole home page.
        // The player's close saves the last position anyway (save_progress_now).
        if pos < 20.0 || self.last_progress_save.elapsed() < Duration::from_secs(30) {
            return;
        }
        self.last_progress_save = Instant::now();
        self.remember_episode(&meta.id, &video_id, pos, dur, false);
        self.progress.retain(|p| p.meta.id != meta.id);
        self.progress.insert(0, Progress { meta, video_id, label, position: pos, duration: dur, updated: now_secs() });
        self.progress.truncate(30);
        self.save_progress_file();
        self.events.push(Event::Progress);
    }

    /// One last save when the player closes.
    fn save_progress_now(&mut self) {
        let Some(m) = self.mpv.clone() else { return };
        let pos = m.get_f64("time-pos").unwrap_or(0.0);
        let dur = m.get_f64("duration").unwrap_or(0.0);
        let eof = m.get_flag("eof-reached").unwrap_or(false);
        self.last_progress_save = Instant::now() - Duration::from_secs(60);
        self.track_progress(pos, dur, eof);
    }

    pub fn remove_progress(&mut self, meta_id: &str) {
        self.progress.retain(|p| p.meta.id != meta_id);
        self.forget_position(|p| p.meta_id == meta_id);
        self.save_progress_file();
        self.events.push(Event::Progress);
    }

    /// Opens the title and plays the episode (or movie) from where it stopped.
    pub fn resume(&mut self, p: Progress) {
        self.open_detail(p.meta.clone());
        self.resume_pos = Some(p.position);
        let (kind, vid, label) = (p.meta.kind.clone(), p.video_id.clone(), p.label.clone());
        let (se, ep) = parse_episode_id(&vid);
        if let Some(d) = self.detail.as_mut() {
            d.ep = EpInfo { show: p.meta.name.clone(), season: se, episode: ep };
            if let Some(se) = se {
                d.season = se;
            }
        }
        if kind == "movie" {
            if let Some(d) = self.detail.as_mut() {
                d.autoplay = true;
            }
        } else {
            self.request_streams(&kind, &vid, &label, true);
        }
    }

    // ----- watched marks -----

    pub fn toggle_watched(&mut self, id: &str) {
        // A mark that comes from the MyAnimeList list is switched off for this session.
        if self.toggle_watched_external(id) {
            self.events.push(Event::Detail);
            return;
        }
        if !self.watched.remove(id) {
            self.watched.insert(id.to_string());
            self.forget_position(|p| p.video_id == id);
            if let Some(i) = self.progress.iter().position(|p| p.video_id == id) {
                self.progress.remove(i);
                self.save_progress_file();
                self.events.push(Event::Progress);
            }
        }
        self.save_watched();
        self.events.push(Event::Detail);
    }

    // ----- calendar -----

    /// Loads the release dates of the series in the library (and those being watched).
    pub fn load_calendar(&mut self) {
        if self.calendar_loading {
            return;
        }
        let mut shows: Vec<(String, String)> = Vec::new();
        for m in self.library.iter().chain(self.progress.iter().map(|p| &p.meta)) {
            if (m.kind == "series" || m.kind == "anime") && !shows.iter().any(|(id, _)| *id == m.id) {
                shows.push((m.id.clone(), m.kind.clone()));
            }
        }
        shows.truncate(60);
        self.calendar_gen += 1;
        let gen = self.calendar_gen;
        self.calendar_loading = true;
        self.events.push(Event::Calendar);
        let entries = self.settings.addons.clone();
        self.spawn(move |tx, http| {
            let total = shows.len();
            let out = std::sync::Mutex::new(Vec::<CalEntry>::new());
            let failed = std::sync::Mutex::new(Vec::<String>::new());
            let queue = std::sync::Mutex::new(shows);
            std::thread::scope(|sc| {
                for _ in 0..6 {
                    sc.spawn(|| loop {
                        let Some((id, kind)) = queue.lock().ok().and_then(|mut q| q.pop()) else { break };
                        let mut meta = addons::fetch_meta(&*http, &entries, &kind, &id);
                        // Cinemeta knows every IMDb series, even when the addon that listed it does not answer.
                        if meta.as_ref().map(|m| m.videos.is_empty()).unwrap_or(true) && id.starts_with("tt") {
                            let url = format!("https://v3-cinemeta.strem.io/meta/series/{}.json", id);
                            if let Ok(j) = crate::http::get_json(&*http, &url, 20) {
                                let m = crate::model::MetaItem::from_json(j.get("meta"));
                                if !m.name.is_empty() {
                                    meta = Some(m);
                                }
                            }
                        }
                        let Some(meta) = meta else {
                            if let Ok(mut f) = failed.lock() {
                                f.push(id.clone());
                            }
                            continue;
                        };
                        let mut mine = Vec::new();
                        for v in meta.videos.iter() {
                            // `get`, not `[..10]`: the date comes from the addon and may hold any bytes
                            let Some(date) = v.released.get(..10) else { continue };
                            mine.push(CalEntry {
                                date: date.to_string(),
                                show_id: id.clone(),
                                show: meta.name.clone(),
                                poster: meta.poster.clone(),
                                label: format!("S{:02}E{:02}", v.season.unwrap_or(0), v.episode.unwrap_or(0)),
                                title: v.title.clone(),
                                video_id: v.id.clone(),
                            });
                        }
                        if let Ok(mut o) = out.lock() {
                            o.extend(mine);
                        }
                    });
                }
            });
            let mut out = out.into_inner().unwrap_or_default();
            out.sort_by(|a, b| a.date.cmp(&b.date).then(a.show.cmp(&b.show)).then(a.label.cmp(&b.label)));
            let failed = failed.into_inner().map(|f| f.len()).unwrap_or(0);
            send(tx, Msg::Calendar(gen, out, format!("Checked {} series, {} did not answer.", total, failed)));
        });
    }

    // ----- loading the next episode early -----

    fn start_prefetch(&mut self, video_id: &str) {
        if self.prefetch.as_ref().map(|p| p.video_id == video_id).unwrap_or(false) {
            return;
        }
        let Some(d) = self.detail.as_ref() else { return };
        let kind = d.meta.kind.clone();
        let cur_stream = self.session.as_ref().and_then(|s| s.streams.get(s.cur)).cloned();
        let rule_text = self.settings.general.autoplay_rule.clone();
        let p2p_ok = self.settings.general.p2p_ok || (self.settings.debrid.enabled && !self.settings.debrid.api_key.trim().is_empty());
        self.prefetch = Some(Prefetch { video_id: video_id.to_string(), ready: None });
        let (entries, preferred, st) = (self.settings.addons.clone(), self.settings.general.preferred_height, self.settings.clone());
        let (engine, exe, rt_dir, dl) = (self.engine.clone(), self.rt.rqbit.clone(), self.rt.dir.clone(), self.data_dir.join("torrents"));
        let vid = video_id.to_string();
        let local = self.local_stream_for(&vid);
        self.spawn(move |tx, http| {
            let found = addons::fetch_streams(&*http, &entries, &kind, &vid);
            let mut seen: Vec<String> = Vec::new();
            let mut list: Vec<Stream> = found
                .items
                .into_iter()
                .filter(|s| {
                    let key = s.info_hash.clone().or_else(|| s.url.clone()).unwrap_or_default();
                    !key.is_empty() && {
                        let new = !seen.contains(&key);
                        seen.push(key);
                        new
                    }
                })
                .collect();
            // an episode ("<show>:<season>:<episode>"): a whole-season torrent is marked a pack and goes below
            if vid.contains(':') {
                streams::mark_big_packs(&mut list);
            }
            streams::rank(&mut list, preferred);
            let has_local = local.is_some();
            if let Some(l) = local {
                list.insert(0, l);
            }
            if list.is_empty() {
                send(tx, Msg::Prefetched(vid, list, 0, Err("No streams for the next episode.".to_string())));
                return;
            }
            // The copy on disk first; otherwise the user's rule and the same release as now (the addon's binge group,
            // else the closest release family), else the same quality as now, when the next episode has it.
            let idx = if has_local {
                0
            } else {
                let rule = streams::autoplay_rule(&rule_text);
                let picked = streams::autoplay_pick(&list, cur_stream.as_ref(), None, rule.as_ref());
                let cur_res = cur_stream.as_ref().map(|x| x.res).unwrap_or(0);
                if rule.is_none() && cur_res != 0 && list[picked].res != cur_res {
                    list.iter().position(|s| s.res == cur_res && s.is_playable()).unwrap_or(picked)
                } else {
                    picked
                }
            };
            // no torrent starts in the background before the user accepted P2P
            if !p2p_ok && list[idx].is_torrent() {
                send(tx, Msg::Prefetched(vid, list, idx, Err("P2P not accepted yet.".to_string())));
                return;
            }
            let res = resolve_stream(&*http, &st, &list[idx], &engine, exe.as_deref(), &rt_dir, &dl, &vid, &|_| {}, &|| false);
            if let Ok(r) = &res {
                if r.torrent.is_some() {
                    // Pull the first part of the file so the torrent is already moving when it is played.
                    let _ = http.send(&crate::http::Request::get(&r.url).header("Range", "bytes=0-16777215").timeout(120));
                }
            }
            send(tx, Msg::Prefetched(vid, list, idx, res));
        });
    }

    /// Starts the prepared episode right away, when there is one.
    fn play_prefetched(&mut self, video_id: &str, label: &str) -> bool {
        let ok = matches!(self.prefetch.as_ref(), Some(p) if p.video_id == video_id && matches!(p.ready, Some((_, _, Ok(_)))));
        if !ok {
            return false;
        }
        let Some(Prefetch { ready: Some((list, idx, Ok(res))), .. }) = self.prefetch.take() else { return false };
        let Some(d) = self.detail.as_mut() else { return false };
        let v = d.meta.videos.iter().find(|v| v.id == video_id);
        let (se, ep) = match v {
            Some(v) if v.season.is_some() || v.episode.is_some() => (v.season, v.episode),
            _ => parse_episode_id(video_id),
        };
        let epinfo = EpInfo { show: d.meta.name.clone(), season: se, episode: ep };
        d.ep = epinfo.clone();
        let (kind, title) = (d.meta.kind.clone(), format!("{} {}", d.meta.name, label));
        d.streams = Some(StreamsState { kind: kind.clone(), key: video_id.to_string(), label: label.to_string(), loading: false, list: list.clone(), errors: vec![] });
        self.events.push(Event::Streams);
        self.preresolved = Some(res);
        self.start_play(list, idx, title, kind, video_id.to_string(), epinfo);
        true
    }

    // ----- subtitle variants -----

    /// Loads one of the subtitles the addons offered for this video.
    pub fn load_sub_candidate(&mut self, idx: usize) {
        let Some(s) = self.session.as_ref() else { return };
        let Some(c) = s.sub_cands.get(idx).cloned() else { return };
        let token = s.token;
        let cache = self.data_dir.join("subtitles");
        let title = format!("{} ({})", util::lang_name(&c.lang), c.addon);
        self.spawn(move |tx, http| {
            let r = subtitles::fetch_srt(&*http, &c.url, &cache).map(|p| (p, c.lang.clone(), title));
            send(tx, Msg::SubVariant(token, r));
        });
    }

    // ----- video scale -----

    /// "fit" keeps the whole picture, "crop" fills the window, "stretch" fills it without keeping the shape.
    pub fn set_video_scale(&mut self, mode: &str) {
        self.settings.general.video_scale = mode.to_string();
        self.apply_video_scale();
        self.save_settings();
    }

    pub fn apply_video_scale(&self) {
        let Some(m) = &self.mpv else { return };
        let (pan, keep) = match self.settings.general.video_scale.as_str() {
            "crop" => ("1.0", "yes"),
            "stretch" => ("0", "no"),
            _ => ("0", "yes"),
        };
        let _ = m.set("panscan", pan);
        let _ = m.set("keepaspect", keep);
    }
}

/// The release (file) name of a stream when it has one that looks like it: "[Group] Show - 05 (1080p).mkv".
fn release_name(s: &Stream) -> Option<String> {
    let looks = |t: &str| t.len() > 12 && (t.contains('[') || t.matches('.').count() >= 2 || t.contains(" - "));
    s.filename.clone().filter(|f| looks(f)).or_else(|| s.desc.lines().map(|l| l.trim()).find(|l| looks(l) && !l.contains('·')).map(|l| l.to_string()))
}

fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Season and episode from a Stremio video id such as "tt123:1:2" or "kitsu:42:7".
pub fn parse_episode_id(id: &str) -> (Option<i64>, Option<i64>) {
    let parts: Vec<&str> = id.split(':').collect();
    if id.starts_with("tt") && parts.len() >= 3 {
        return (parts[parts.len() - 2].parse().ok(), parts[parts.len() - 1].parse().ok());
    }
    if parts.len() >= 3 {
        return (None, parts[parts.len() - 1].parse().ok());
    }
    (None, None)
}

impl Core {
    pub fn new() -> Core {
        let mut c = Core::new_in(util::data_dir());
        c.update_startup();
        c
    }

    /// The core over a given data folder (the app uses `util::data_dir()`; tests use a folder of their own).
    pub fn new_in(data_dir: PathBuf) -> Core {
        let data_dir2 = data_dir.clone();
        // whose data to read: the profile in use last time (the main one, over the data folder itself, by default)
        let profiles = crate::profiles::Store::load(&data_dir);
        let pdir = profiles.active_dir(&data_dir);
        let _ = std::fs::create_dir_all(&pdir);
        let novels_loaded = Core::load_novels(&data_dir2, &pdir);
        let mut settings = Settings::load(&data_dir);
        crate::debug::init(&data_dir);
        // the interface sounds: empty slots take <data>/sounds/nav.*, select.*, music.* (crate::uisounds)
        let adopted = crate::uisounds::adopt(&mut settings.general, &data_dir);
        for (slot, rel) in &adopted {
            crate::debug::log("sound", &format!("{} = {} (found in the sounds folder)", slot, rel));
        }
        if !adopted.is_empty() {
            let _ = settings.save(&data_dir);
        }
        let library = settings::load_library(&pdir);
        let rt = runtime::prepare(&data_dir);
        let (tx, rx) = channel();
        crate::connectivity::set_forced(settings.general.offline_mode);
        let http: HttpRef = Arc::new(UreqHttp::new());
        // read once: both fields below come from the same file
        let (seen_latest, notices) = controller_notify::load_notices(&pdir);
        let mut c = Core {
            http,
            data_dir,
            profile_dir: pdir.clone(),
            profiles,
            rt,
            settings,
            library,
            system_lang: util::system_lang(),
            tx,
            rx,
            engine: Arc::new(Mutex::new(None)),
            rows: vec![],
            home_gen: 0,
            home_loading: false,
            home_errors: vec![],
            search_query: None,
            search_rows: vec![],
            search_gen: 0,
            search_loading: false,
            detail: None,
            detail_gen: 0,
            stream_gen: 0,
            mpv: None,
            mpv_error: None,
            session: None,
            token_counter: 0,
            thumb: None,
            shader_busy: false,
            events: vec![],
            images: Arc::new(ImageQueue { pending: Mutex::new(VecDeque::new()), wake: Condvar::new(), requested: Mutex::new(HashSet::new()), keys: Mutex::new((String::new(), String::new())), cache: data_dir2.clone(), titles: Mutex::new(HashMap::new()), fails: Mutex::new(HashMap::new()) }),
            watched: load_watched(&pdir),
            art: load_art(&data_dir2),
            torrent_stats: None,
            stats_busy: false,
            progress: load_progress(&pdir),
            last_progress_save: Instant::now(),
            resume_pos: None,
            ep_resume: controller_resume::load_resume(&pdir),
            track_restore: None,
            calendar: vec![],
            calendar_loading: false,
            calendar_info: String::new(),
            calendar_at: None,
            calendar_gen: 0,
            prefetch: None,
            preresolved: None,
            favorites: settings::load_list(&pdir, "favorites.json"),
            mal_list: vec![],
            mal_list_info: String::new(),
            external_watched: HashSet::new(),
            ext_off: HashSet::new(),
            library_filling: false,
            manga: crate::manga::State::load(&pdir),
            manga_list: MangaList::new(),
            manga_view: None,
            manga_reader: None,
            manga_gen: 0,
            novels: novels_loaded.0,
            novel_installed: novels_loaded.1,
            novel_catalog: vec![],
            novel_catalog_status: String::new(),
            novel_catalog_loading: false,
            novel_list: MangaList::new(),
            study: Core::load_study(&pdir),
            study_session: None,
            jlab: crate::jlab::Jlab::load(&pdir),
            jlab_busy: String::new(),
            knowledge: crate::knowledge::Knowledge::load_from(&crate::study::dir(&pdir).join("knowledge.md")),
            learn: controller_learn::LearnState::default(),
            shared: Default::default(),
            nudges: Default::default(),
            time: Default::default(),
            study_dirty: 0,
            study_importing: false,
            study_audio: None,
            autocard_busy: false,
            theme_player: Default::default(),
            study_play: Default::default(),
            mining: Default::default(),
            wordmix_readers: None,
            clock: Default::default(),
            page_db: None,
            app_exts: crate::appext::scan(&data_dir2),
            study_clip: None,
            study_clip_fresh: Default::default(),
            xray_people: Default::default(),
            kaiwa: Default::default(),
            ollama: Default::default(),
            voice: Default::default(),
            whisper: Default::default(),
            ankiweb_busy: false,
            ankiweb_status: String::new(),
            ankiweb_last: 0,
            clip_busy: false,
            hint_cache: None,
            hint_text: String::new(),
            hint_cool: Default::default(),
            flash: None,
            study_browse: None,
            study_stats: None,
            novel_view: None,
            novel_reader: None,
            novel_gen: 0,
            novel_refreshing: false,
            novel_resume: 0,
            reader_text: HashMap::new(),
            reader_translations: HashMap::new(),
            reader_tr_progress: HashMap::new(),
            reader_upscaled: HashMap::new(),
            novel_saves: 0,
            mal_linking: HashSet::new(),
            manga_sources: vec![],
            suwayomi_exts: vec![],
            suwayomi_status: String::new(),
            suwayomi_host: None,
            suwayomi_booting: false,
            suwayomi_boot_wanted: false,
            suwayomi_port: crate::suwayomi::DEFAULT_PORT,
            ext_host_port: None,
            ext_host_dir: std::path::PathBuf::new(),
            suwayomi_idle: None,
            manga_refreshing: false,
            manga_resume: 0,
            manga_page_saves: 0,
            ext_lists: HashMap::new(),
            ext_status: HashMap::new(),
            ext_loading: HashSet::new(),
            seen_latest,
            notices,
            new_checked: None,
            checking_new: false,
            card_gen: 0,
            mal_cache: Arc::new(Mutex::new(HashMap::new())),
            mal_cancel: Arc::new(AtomicBool::new(false)),
            mal_pending: None,
            translating: None,
            translate_note: None,
            finding: None,
            genre: None,
            genre_gen: 0,
            taste: vec![],
            taste_gen: 0,
            covers: controller_dl::load_covers(&data_dir2),
            forced: controller_sub::load_forced(&data_dir2),
            ext_audio: controller_audio::load_ext_audio(&data_dir2),
            audio_sync: Default::default(),
            cover_asked: HashSet::new(),
            downloads: downloads::load(&data_dir2),
            dl_running: HashSet::new(),
            dl_pick: None,
            dl_gen: 0,
            backup_busy: false,
            cloud: crate::cloud::Live::default(),
            update: UpdateLive::default(),
            browser: crate::browser::Browser::with_lists(data_dir2.join("browser"), pdir.join("browser")),
        };
        c.start_image_workers();
        c.bootstrap();
        c.refresh_mal_list();
        c.load_taste();
        c.dl_resume_all();
        c.dl_startup_cleanup();
        // the course of the study language, read in the background: its words cut subtitle lines into words
        c.learn_ensure();
        c
    }

    fn bootstrap(&mut self) {
        if self.settings.addons.iter().any(|a| a.enabled && !a.has_manifest()) {
            let mut entries = self.settings.addons.clone();
            self.spawn(move |tx, http| {
                let errors = addons::refresh_missing(&*http, &mut entries);
                send(tx, Msg::Manifests(entries, errors));
            });
        } else {
            self.load_home();
        }
    }

    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce(&Sender<Msg>, HttpRef) + Send + 'static,
    {
        let tx = self.tx.clone();
        let http = self.http.clone();
        std::thread::spawn(move || f(&tx, http));
    }

    /// A note written with the floating feedback button: appended to `<data>/Feedback.log` with the time, the version
    /// and where the person was in the app, so the author can read every note later.
    pub fn feedback_add(&mut self, text: &str, place: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let path = self.data_dir.join("Feedback.log");
        let when = crate::util::now_unix();
        let (y, mo, d, h, mi, se) = crate::util::civil(when);
        let line = format!(
            "==== {} (unix {}) | Reelhouse {} | mode {} | {}\n{}\n\n",
            format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", y, mo, d, h, mi, se),
            when,
            option_env!("CARGO_PKG_VERSION").unwrap_or("dev"),
            self.settings.general.mode,
            place,
            text
        );
        use std::io::Write;
        let ok = std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|mut f| f.write_all(line.as_bytes())).is_ok();
        self.toast(if ok { "Thanks! Your feedback was saved in Feedback.log." } else { "The feedback could not be saved." });
    }

    /// Opens Feedback.log in the system's text editor.
    pub fn feedback_open(&mut self) {
        let p = self.data_dir.join("Feedback.log");
        if !p.exists() {
            self.toast("No feedback written yet.");
            return;
        }
        let p = p.to_string_lossy().replace('\\', "/");
        self.events.push(Event::OpenUrl(format!("file:///{}", p.trim_start_matches('/'))));
    }

    pub fn toast(&mut self, text: &str) {
        self.events.push(Event::Toast(text.to_string()));
    }

    /// A pop-up at the top of the window with the macaw. `action` is the command sent when it is clicked.
    pub fn banner(&mut self, kind: &str, title: &str, body: &str, mood: &str, action: Option<Json>) {
        self.events.push(Event::Banner {
            kind: kind.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            mood: mood.to_string(),
            action,
        });
    }

    /// First lines of the debug report: what is needed to read the rest.
    pub fn debug_header(&self) -> String {
        let g = &self.settings.general;
        format!(
            "Reelhouse {} on {} {}; language {} (system {}); mode {}; data dir {}\n  settings: karaoke={} animated_bg={} aurora_motion={} hwdec={} ui_scale={} suwayomi_managed={} repos={} blur_unwatched={}\n  suwayomi: {} (port {})",
            option_env!("CARGO_PKG_VERSION").unwrap_or("dev"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            format!("{} -> {}", g.language, g.ui_lang(&self.system_lang)),
            self.system_lang,
            g.mode,
            self.data_dir.display(),
            g.karaoke,
            g.animated_bg,
            g.aurora_motion,
            g.hwdec,
            g.ui_scale,
            g.suwayomi_managed,
            self.suwayomi_repos().len(),
            g.blur_unwatched,
            self.suwayomi_status,
            self.suwayomi_port
        )
    }

    /// One short line for the 5 second summary of the report.
    pub fn debug_state(&self) -> String {
        format!(
            "mode {}, suwayomi {}, session {}",
            self.mode(),
            if self.suwayomi_host.is_some() { "running" } else { "off" },
            self.session.as_ref().map(|s| format!("{:?}", s.stage)).unwrap_or_else(|| "none".into())
        )
    }

    pub fn save_settings(&mut self) {
        if let Ok(mut k) = self.images.keys.lock() {
            *k = (self.settings.metadata.tmdb_key.clone(), self.settings.metadata.omdb_key.clone());
        }
        if let Err(e) = self.settings.save(&self.data_dir) {
            self.toast(&format!("Could not save settings: {}", e));
        }
    }

    // ----- pictures -----

    /// Starts downloading a picture once. It arrives as `Event::Image`.
    pub fn want_image(&self, url: &str) {
        if !self.images.requested.lock().unwrap_or_else(|p| p.into_inner()).insert(url.to_string()) {
            return;
        }
        self.images.pending.lock().unwrap_or_else(|p| p.into_inner()).push_back(url.to_string());
        self.images.wake.notify_one();
    }

    /// Lets a picture be asked for again (the reader let go of it and the page came back on screen).
    pub fn forget_image(&self, url: &str) {
        if let Ok(mut r) = self.images.requested.lock() {
            r.remove(url);
        }
    }

    /// Forgets which pictures were asked for or failed, so every picture on screen is asked for again.
    pub fn reset_images(&self) {
        if let Ok(mut r) = self.images.requested.lock() {
            r.clear();
        }
        if let Ok(mut f) = self.images.fails.lock() {
            f.clear();
        }
    }

    /// Tells which title a picture address belongs to, so a broken address can be replaced by a cover found by name.
    pub fn hint_image(&self, url: &str, title: &str) {
        if !url.is_empty() && !title.is_empty() {
            if let Ok(mut t) = self.images.titles.lock() {
                t.entry(url.to_string()).or_insert_with(|| title.to_string());
            }
        }
    }

    /// The picture arrived but could not be read (an unsupported format): ask for another source, once.
    pub fn image_unreadable(&self, url: &str) {
        // A broken picture must not stay in the disk cache.
        let _ = std::fs::remove_file(self.data_dir.join(crate::backup::image_file(url)));
        let key = format!("alt:{}", url);
        if !self.images.requested.lock().unwrap_or_else(|p| p.into_inner()).insert(key) {
            return;
        }
        let (tk, ok) = self.images.keys.lock().map(|k| k.clone()).unwrap_or_default();
        let (tx, http, url) = (self.tx.clone(), self.http.clone(), url.to_string());
        std::thread::spawn(move || {
            if let Some(bytes) = crate::artwork::alt_image(&*http, &url, &tk, &ok) {
                send(&tx, Msg::Image(url, bytes));
            }
        });
    }

    /// Removes cached pictures that were not touched for ten years (in practice never: covers and backgrounds stay on disk, so the app works offline).
    fn prune_image_cache(&self) {
        let dir = self.data_dir.join("imgcache");
        std::thread::spawn(move || {
            let limit = Duration::from_secs(3650 * 86_400);
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|a| a > limit).unwrap_or(false);
                    if old {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        });
    }

    fn start_image_workers(&self) {
        self.prune_image_cache();
        for _ in 0..8 {
            let q = self.images.clone();
            let tx = self.tx.clone();
            let http = self.http.clone();
            std::thread::spawn(move || loop {
                let url = {
                    // A worker that panicked must not poison the queue for the other seven and the UI thread.
                    let mut p = q.pending.lock().unwrap_or_else(|e| e.into_inner());
                    while p.is_empty() {
                        p = q.wake.wait(p).unwrap_or_else(|e| e.into_inner());
                    }
                    p.pop_front().unwrap()
                };
                // A picture seen before is kept on disk: it shows at once, and is part of the backup.
                let file = q.cache.join(crate::backup::image_file(&url));
                if let Ok(bytes) = std::fs::read(&file) {
                    if !bytes.is_empty() {
                        send(&tx, Msg::Image(url.clone(), bytes));
                        continue;
                    }
                }
                // Poster servers sometimes drop a request; try a few times, then let a later redraw ask again.
                let mut got = false;
                // "title:Name" asks for a cover found by the name alone (the title has no picture address).
                let by_title = url.starts_with("title:");
                for attempt in 0..if by_title { 0 } else { 3u64 } {
                    // Some picture hosts (imgur among them) refuse requests that do not look like a browser.
                    let req = crate::http::Request::get(&crate::util::direct_image_url(&url))
                        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
                        .header("Accept", "image/png,image/jpeg,image/gif,image/*;q=0.8")
                        .timeout(25);
                    if let Ok(r) = http.send(&req) {
                        if r.ok() && !r.body.is_empty() {
                            keep_picture(&file, &r.body);
                            send(&tx, Msg::Image(url.clone(), r.body));
                            got = true;
                            break;
                        }
                        if r.status == 404 {
                            break;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(700 * (attempt + 1)));
                }
                if !got {
                    // Another source for the same picture (see artwork::alt_image).
                    let (tk, ok) = q.keys.lock().map(|k| k.clone()).unwrap_or_default();
                    let hint = url.strip_prefix("title:").map(|t| t.to_string()).or_else(|| q.titles.lock().ok().and_then(|t| t.get(&url).cloned()));
                    let found = crate::artwork::alt_image(&*http, &url, &tk, &ok).or_else(|| hint.as_deref().and_then(|t| crate::artwork::title_cover(&*http, t)));
                    if let Some(bytes) = found {
                        keep_picture(&file, &bytes);
                        send(&tx, Msg::Image(url.clone(), bytes));
                    } else {
                        // Asked for again later, a few times; then it is left alone.
                        let n = q.fails.lock().map(|mut f| {
                            let e = f.entry(url.clone()).or_insert(0);
                            *e += 1;
                            *e
                        }).unwrap_or(9);
                        if n < 4 {
                            q.requested.lock().unwrap_or_else(|e| e.into_inner()).remove(&url);
                        }
                    }
                }
            });
        }
    }

    // ----- catalogs -----

    pub fn load_home(&mut self) {
        self.home_gen += 1;
        let gen = self.home_gen;
        self.rows.clear();
        self.home_errors.clear();
        let cats: Vec<_> = addons::board_catalogs(&self.settings.addons).into_iter().take(24).collect();
        self.home_loading = !cats.is_empty();
        self.events.push(Event::Home);
        if cats.is_empty() {
            return;
        }
        // each row is kept on disk: offline (or when an addon is down) the last one read shows instead of an error
        let store = crate::readcache::Store::of(&self.data_dir, "home");
        self.spawn(move |tx, http| {
            std::thread::scope(|sc| {
                for (i, c) in cats.iter().enumerate() {
                    let (tx, http, store) = (tx.clone(), http.clone(), store.clone());
                    let key = format!("row|{}|{}|{}", c.addon_url, c.kind, c.id);
                    sc.spawn(move || match addons::fetch_catalog(&*http, c, &[]) {
                        Ok(row) => {
                            if !row.items.is_empty() {
                                store.put(&key, row_to_json(&row));
                            }
                            send(&tx, Msg::Row(gen, i, row))
                        }
                        Err(e) => match store.get(&key).and_then(|h| row_from_json(&h.data)) {
                            Some(row) => send(&tx, Msg::Row(gen, i, row)),
                            None => send(&tx, Msg::HomeError(gen, format!("{}: {}", c.addon_name, e))),
                        },
                    });
                }
            });
            send(tx, Msg::HomeDone(gen));
        });
    }

    pub fn run_search(&mut self, text: &str) {
        // offline the addons are not asked (each would only fail); the page says why it is empty
        if !crate::connectivity::online() && !text.trim().is_empty() {
            self.search_gen += 1;
            self.search_rows.clear();
            self.search_query = Some(text.trim().to_string());
            self.search_loading = false;
            self.events.push(Event::Search);
            return;
        }
        let q = text.trim().to_string();
        self.search_rows.clear();
        if q.is_empty() {
            self.search_query = None;
            self.search_loading = false;
            self.events.push(Event::Search);
            return;
        }
        self.search_gen += 1;
        let gen = self.search_gen;
        self.search_query = Some(q.clone());
        let cats: Vec<_> = addons::search_catalogs(&self.settings.addons).into_iter().take(14).collect();
        self.search_loading = !cats.is_empty();
        self.events.push(Event::Search);
        if cats.is_empty() {
            return;
        }
        self.spawn(move |tx, http| {
            std::thread::scope(|sc| {
                for (i, c) in cats.iter().enumerate() {
                    let (tx, http, q) = (tx.clone(), http.clone(), q.clone());
                    sc.spawn(move || {
                        if let Ok(row) = addons::fetch_catalog(&*http, c, &[("search", q.as_str())]) {
                            send(&tx, Msg::SearchRow(gen, i, row));
                        }
                    });
                }
            });
            send(tx, Msg::SearchDone(gen));
        });
    }

    // ----- title pages -----

    pub fn open_detail(&mut self, item: MetaItem) {
        self.fire_hook("onTitleOpened", Json::obj(vec![("id", item.id.clone().into()), ("name", item.name.clone().into()), ("kind", item.kind.clone().into())]));
        self.detail_gen += 1;
        let gen = self.detail_gen;
        let (kind, id) = (item.kind.clone(), item.id.clone());
        // Opening the title page is what takes the NEW mark away.
        self.clear_notice(&id);
        let is_movie = item.kind == "movie";
        let entries = self.settings.addons.clone();
        let tmdb = if self.settings.metadata.tmdb_art { self.settings.metadata.tmdb_key.clone() } else { String::new() };
        let omdb = self.settings.metadata.omdb_key.clone();
        self.external_watched.clear();
        self.ext_off.clear();
        // A title of the library or the favourites opens from its saved page at once; the addons are asked again behind it
        // (not offline: they could only fail).
        // A title with episodes kept on disk too, so they can be played offline.
        let keeps = self.is_saved(&id) || self.downloads.iter().any(|x| x.meta_id == id);
        let saved = keeps.then(|| settings::load_detail(&self.data_dir, &id)).flatten();
        let (meta, loading, season) = match saved {
            Some(mut cached) => {
                cached.fill_from(&item);
                let seasons = cached.seasons();
                let season = seasons.iter().copied().find(|s| *s >= 1).or_else(|| seasons.first().copied()).unwrap_or(1);
                (cached, false, season)
            }
            None => (item, true, 1),
        };
        self.detail = Some(Detail { gen, meta, loading, season, streams: None, rating: None, autoplay: false, ep: EpInfo::default(), mal: MalData::default() });
        if !crate::connectivity::online() {
            // the saved page is all there is; a title downloaded before its page was kept gets its episodes from
            // the files on disk, so they still play
            let kept = self.offline_videos(&id);
            if let Some(d) = self.detail.as_mut() {
                d.loading = false;
                if d.meta.videos.is_empty() && !kept.is_empty() {
                    d.meta.videos = kept;
                    let seasons = d.meta.seasons();
                    d.season = seasons.iter().copied().find(|s| *s >= 1).or_else(|| seasons.first().copied()).unwrap_or(1);
                }
            }
        } else {
            let (kind, id) = (kind.clone(), id.clone());
            self.spawn(move |tx, http| {
                let mut meta = addons::fetch_meta(&*http, &entries, &kind, &id);
                if meta.is_none() {
                    meta = artwork::omdb_meta(&*http, &omdb, &id, &kind);
                }
                send(tx, Msg::Meta(gen, meta));
                if !tmdb.is_empty() {
                    if let Some(art) = artwork::tmdb_art(&*http, &tmdb, &id) {
                        send(tx, Msg::Art(gen, art));
                    }
                }
            });
        }
        if is_movie {
            self.request_streams(&kind, &id, "", false);
        }
        self.events.push(Event::Detail);
    }

    pub fn close_detail(&mut self) {
        self.detail = None;
        self.dl_pick = None;
        self.external_watched.clear();
        self.stream_gen += 1;
    }

    pub fn set_season(&mut self, s: i64) {
        if let Some(d) = self.detail.as_mut() {
            d.season = s;
        }
        self.events.push(Event::Detail);
        self.start_mal_lookup();
    }

    /// Picks an episode: loads its streams and, when "start the best stream" is on, plays right away.
    pub fn choose_episode(&mut self, video_id: &str, label: &str) {
        let auto = self.settings.general.pick_mode == "best";
        self.choose_episode_with(video_id, label, auto);
    }

    pub fn choose_episode_with(&mut self, video_id: &str, label: &str, auto: bool) {
        if auto && self.play_prefetched(video_id, label) {
            return;
        }
        let (kind, ep) = match self.detail.as_ref() {
            Some(d) => {
                let v = d.meta.videos.iter().find(|v| v.id == video_id);
                let (s, e) = match v {
                    Some(v) if v.season.is_some() || v.episode.is_some() => (v.season, v.episode),
                    _ => parse_episode_id(video_id),
                };
                (d.meta.kind.clone(), EpInfo { show: d.meta.name.clone(), season: s, episode: e })
            }
            None => return,
        };
        if let Some(d) = self.detail.as_mut() {
            d.ep = ep;
        }
        self.request_streams(&kind, video_id, label, auto);
    }

    pub fn request_streams(&mut self, kind: &str, id: &str, label: &str, autoplay: bool) {
        self.stream_gen += 1;
        let gen = self.stream_gen;
        if let Some(d) = self.detail.as_mut() {
            d.autoplay = autoplay;
            d.streams = Some(StreamsState { kind: kind.to_string(), key: id.to_string(), label: label.to_string(), loading: true, list: vec![], errors: vec![] });
        }
        self.events.push(Event::Streams);
        let entries = self.settings.addons.clone();
        let preferred = self.settings.general.preferred_height;
        let (kind, id) = (kind.to_string(), id.to_string());
        // An episode kept on disk is listed first, right away.
        let local = self.local_stream_for(&id);
        if let (Some(l), Some(ss)) = (local.clone(), self.detail.as_mut().and_then(|d| d.streams.as_mut())) {
            ss.list = vec![l];
        }
        if !crate::connectivity::online() {
            // offline only the copy on disk can play: answered at once (and played at once when asked)
            let errors = if local.is_some() { vec![] } else { vec!["No internet connection: only episodes kept on disk play offline.".to_string()] };
            self.on_streams(gen, local.into_iter().collect(), errors);
            return;
        }
        self.spawn(move |tx, http| {
            let found = addons::fetch_streams(&*http, &entries, &kind, &id);
            let mut seen: Vec<String> = Vec::new();
            let mut list: Vec<Stream> = found
                .items
                .into_iter()
                .filter(|s| {
                    let key = s.info_hash.clone().or_else(|| s.url.clone()).unwrap_or_default();
                    if key.is_empty() || seen.contains(&key) {
                        false
                    } else {
                        seen.push(key);
                        true
                    }
                })
                .collect();
            // an episode ("<show>:<season>:<episode>"): a whole-season torrent is marked a pack and goes below
            if id.contains(':') {
                streams::mark_big_packs(&mut list);
            }
            streams::rank(&mut list, preferred);
            if let Some(l) = local {
                list.insert(0, l);
            }
            send(tx, Msg::Streams(gen, list, found.errors));
        });
    }

    /// The episodes of a title found in its downloads (season and episode read from the file names), for its page
    /// offline when no copy of the page was kept.
    pub(super) fn offline_videos(&self, id: &str) -> Vec<crate::model::Video> {
        let mut out: Vec<crate::model::Video> = Vec::new();
        for d in self.downloads.iter().filter(|d| d.meta_id == id) {
            for f in &d.files {
                let Some(e) = f.episode else { continue };
                let s = f.season.unwrap_or(1);
                if out.iter().any(|v| v.season == Some(s) && v.episode == Some(e)) {
                    continue;
                }
                let title = f.path.rsplit('/').next().unwrap_or(&f.path).to_string();
                out.push(crate::model::Video { id: format!("{}:{}:{}", id, s, e), season: Some(s), episode: Some(e), title, ..Default::default() });
            }
        }
        out.sort_by_key(|v| (v.season, v.episode));
        out
    }

    /// True for a title of the library or of the favourites (its page is kept on disk).
    pub fn is_saved(&self, id: &str) -> bool {
        self.library.iter().any(|x| x.id == id) || self.favorites.iter().any(|x| x.id == id)
    }

    pub fn toggle_library(&mut self, m: &MetaItem) {
        if let Some(i) = self.library.iter().position(|x| x.id == m.id) {
            self.library.remove(i);
        } else {
            self.library.push(m.clone());
        }
        let _ = settings::save_library(&self.profile_dir, &self.library);
        self.keep_open_info();
        if let Some(d) = self.detail.as_ref().filter(|d| d.meta.id == m.id && !d.loading && !d.meta.videos.is_empty()) {
            if self.library.iter().any(|x| x.id == m.id) {
                settings::save_detail(&self.data_dir, &d.meta);
            }
        }
    }

    pub fn in_library(&self, id: &str) -> bool {
        self.library.iter().any(|m| m.id == id)
    }

    /// Plays the stream at `idx` of the streams listed on the title page.
    pub fn play_from_detail(&mut self, idx: usize) {
        let Some(d) = self.detail.as_ref() else { return };
        let Some(ss) = d.streams.as_ref() else { return };
        if idx >= ss.list.len() {
            return;
        }
        let title = if ss.label.is_empty() { d.meta.name.clone() } else { format!("{} {}", d.meta.name, ss.label) };
        let (list, kind, key, ep) = (ss.list.clone(), ss.kind.clone(), ss.key.clone(), d.ep.clone());
        self.start_play(list, idx, title, kind, key, ep);
    }

    // ----- messages -----

    /// Applies finished background work and returns what changed.
    pub fn poll(&mut self) -> Vec<Event> {
        while let Ok(m) = self.rx.try_recv() {
            self.apply(m);
        }
        // study reminders (looks once a minute)
        self.nudge_tick();
        // seconds of study, and the Pomodoro timer
        self.time_tick();
        // Conversation by voice: the end of a scene, the nudge after a long silence, the sound devices' errors
        self.kaiwa_live_tick();
        std::mem::take(&mut self.events)
    }

    fn apply(&mut self, m: Msg) {
        match m {
            Msg::Row(g, i, row) => {
                if g == self.home_gen && !row.items.is_empty() {
                    self.rows.push((i, Arc::new(row)));
                    self.rows.sort_by_key(|(i, _)| *i);
                    self.events.push(Event::Home);
                }
            }
            Msg::HomeError(g, e) => {
                if g == self.home_gen {
                    self.home_errors.push(e);
                }
            }
            Msg::HomeDone(g) => {
                if g == self.home_gen {
                    self.home_loading = false;
                    self.events.push(Event::Home);
                }
            }
            Msg::SearchRow(g, i, row) => {
                if g == self.search_gen && !row.items.is_empty() {
                    self.search_rows.push((i, Arc::new(row)));
                    self.search_rows.sort_by_key(|(i, _)| *i);
                    self.events.push(Event::Search);
                }
            }
            Msg::SearchDone(g) => {
                if g == self.search_gen {
                    self.search_loading = false;
                    self.events.push(Event::Search);
                }
            }
            Msg::Manifests(entries, errors) => {
                for e in entries {
                    if let Some(x) = self.settings.addons.iter_mut().find(|a| a.url == e.url) {
                        if !x.has_manifest() {
                            x.manifest = e.manifest;
                        }
                    }
                }
                self.save_settings();
                self.load_home();
                self.home_errors.extend(errors);
            }
            Msg::AddonKind(text, kind) => match kind {
                Some(k) => self.add_addon_as(&text, k),
                None => self.events.push(Event::Info(MsgKind::Addon, crate::addon_kind::UNKNOWN.to_string())),
            },
            Msg::AddonInstalled(r) => match r {
                Ok((entries, name)) => {
                    self.settings.addons = entries;
                    self.save_settings();
                    self.events.push(Event::Info(MsgKind::Addon, format!("Installed {}.", name)));
                    self.load_home();
                }
                Err(e) => self.events.push(Event::Info(MsgKind::Addon, e)),
            },
            Msg::Meta(g, meta) => self.on_meta(g, meta),
            Msg::Art(g, art) => {
                if let Some(d) = self.detail.as_mut() {
                    if d.gen == g {
                        if art.poster.is_some() {
                            d.meta.poster = art.poster;
                        }
                        if art.background.is_some() {
                            d.meta.background = art.background;
                        }
                        d.rating = art.rating;
                        self.events.push(Event::Detail);
                    }
                }
            }
            Msg::Streams(g, list, errors) => self.on_streams(g, list, errors),
            Msg::FileSize(g, hash, file, total) => {
                if g == self.stream_gen {
                    if let Some(ss) = self.detail.as_mut().and_then(|d| d.streams.as_mut()) {
                        for s in ss.list.iter_mut().filter(|s| s.info_hash.as_deref() == Some(hash.as_str())) {
                            if file > 0 {
                                s.set_file_size(file, total);
                            } else {
                                s.size_lookup = 2;
                            }
                        }
                        self.events.push(Event::Streams);
                    }
                }
            }
            Msg::Info(k, m) => self.events.push(Event::Info(k, m)),
            Msg::ShaderDone(r) => {
                self.shader_busy = false;
                let text = match r {
                    Ok(n) => format!("Installed {} shader files.", n),
                    Err(e) => e,
                };
                self.events.push(Event::Info(MsgKind::Shaders, text));
            }
            Msg::Resolved(token, r) => self.on_resolved(token, r),
            Msg::ResolveStatus(token, m) => {
                if let Some(s) = self.session.as_mut().filter(|s| s.token == token) {
                    s.resolve_msg = m;
                    self.events.push(Event::Session);
                }
            }
            Msg::SubStatus(token, m) => {
                if let Some(s) = self.session.as_mut().filter(|s| s.token == token) {
                    s.sub_msg = m;
                    self.events.push(Event::Session);
                }
            }
            Msg::SubDone(token, r) => {
                if let Some(s) = self.session.as_mut().filter(|s| s.token == token) {
                    if matches!(s.sub_job, SubJob::Running | SubJob::TimedOut) {
                        let late = s.stage == Stage::Playing;
                        // playback already started without them (it does not wait long, like Stremio): add them now
                        if let (true, Ok(Some(f)), Some(m)) = (late, &r, &self.mpv) {
                            let lang = if f.lang.is_empty() { "und".to_string() } else { f.lang.clone() };
                            let title = if f.translated { format!("{} (translated)", lang) } else { lang.clone() };
                            let _ = m.cmd(&["sub-add", &f.path.to_string_lossy(), "select", &title, &lang]);
                            self.events.push(Event::Toast(format!("Subtitles: {}", title)));
                        }
                        s.sub_job = match r {
                            Ok(f) => SubJob::Done(f),
                            Err(e) => SubJob::Failed(e),
                        };
                    }
                }
            }
            Msg::Segments(token, prio, source, r) => {
                crate::debug::log("skip", &match &r {
                    Ok(v) => format!("{}: {} segment(s){}", source, v.len(), v.iter().map(|x| format!(" [{:?} {:.0}-{}]", x.kind, x.start, x.end.map(|e| format!("{:.0}", e)).unwrap_or_else(|| "end".into()))).collect::<String>()),
                    Err(e) => format!("{}: {}", source, e),
                });
                if let Some(s) = self.session.as_mut().filter(|s| s.id == token) {
                    match r {
                        Ok(v) if !v.is_empty() => {
                            if prio > s.seg_prio {
                                s.segments = v;
                                s.seg_prio = prio;
                                s.seg_source = source;
                                self.events.push(Event::Session);
                            }
                        }
                        Ok(_) => {}
                        Err(e) => s.notes.push(format!("{}: {}", source, e)),
                    }
                }
            }
            Msg::Karaoke(token, r, note) => self.on_karaoke(token, r, note),
            Msg::Upscaled(url, r) => self.on_upscaled(url, r),
            Msg::UpscaleInstalled(r) => self.on_upscale_installed(r),
            Msg::PageText(url, r) => self.on_page_text(url, r),
            Msg::Translated(gen, id, r) => self.on_translated(gen, id, r),
            Msg::TranslateProgress(gen, id, d, n) => self.on_translate_progress(gen, id, d, n),
            Msg::AsLogin(r) => match r {
                Ok(l) => {
                    self.settings.anime_skip.token = l.token;
                    self.settings.anime_skip.refresh_token = l.refresh;
                    self.save_settings();
                    let who = self.settings.anime_skip.email.clone();
                    self.events.push(Event::Info(MsgKind::AnimeSkip, format!("Logged in as {}.", who)));
                }
                Err(e) => self.events.push(Event::Info(MsgKind::AnimeSkip, e)),
            },
            Msg::Image(url, bytes) => self.events.push(Event::Image(url, bytes)),
            Msg::Thumb(t, p) => self.events.push(Event::Thumb(t, p)),
            Msg::Calendar(g, list, info) => {
                if g == self.calendar_gen {
                    self.calendar = list;
                    self.calendar_info = info;
                    self.calendar_loading = false;
                    self.calendar_at = Some(Instant::now());
                    self.events.push(Event::Calendar);
                }
            }
            Msg::SubCands(token, list) => {
                if let Some(s) = self.session.as_mut().filter(|s| s.id == token || s.token == token) {
                    s.sub_cands = list;
                    self.events.push(Event::Session);
                }
            }
            Msg::SubVariant(token, r) => {
                let current = self.session.as_ref().map(|s| s.token == token).unwrap_or(false);
                if current {
                    match r {
                        Ok((path, lang, title)) => {
                            let lang = if lang.is_empty() { "und".to_string() } else { util::lang_base(&lang) };
                            if let Some(m) = &self.mpv {
                                let _ = m.cmd(&["sub-add", &path.to_string_lossy(), "select", &title, &lang]);
                            }
                            self.events.push(Event::Toast(format!("Subtitles: {}", title)));
                        }
                        Err(e) => self.events.push(Event::Toast(e)),
                    }
                }
            }
            Msg::Prefetched(video_id, list, idx, res) => {
                if let Some(pf) = self.prefetch.as_mut().filter(|p| p.video_id == video_id) {
                    pf.ready = Some((list, idx, res));
                }
            }
            Msg::Stats(st) => {
                self.stats_busy = false;
                self.torrent_stats = st;
                self.events.push(Event::Stats);
            }
            Msg::Mal(g, part) => self.on_mal(g, part),
            Msg::MalList(l, e) => self.on_mal_list(l, e),
            Msg::MalLogin(r) => self.on_mal_login(r),
            Msg::MalTokens(t) => self.on_mal_tokens(t),
            Msg::MalMy(id, r) => self.on_mal_my(id, r),
            Msg::Genre(g, p, r) => self.on_genre(g, p, r),
            Msg::Taste(g, rows) => self.on_taste(g, rows),
            Msg::LibraryFill(v) => self.on_library_fill(v),
            Msg::LensFrame(token, r, ms) => self.on_lens_frame(token, r, ms),
            Msg::NewEps(v) => self.on_new_eps(v),
            Msg::MangaList(g, o, r) => self.on_manga_list(g, o, r),
            Msg::MangaView(g, d, r) => self.on_manga_view(g, d, r),
            Msg::MangaPages(g, r) => self.on_manga_pages(g, r),
            Msg::MangaCounts(c) => self.on_manga_counts(c),
            Msg::SuwayomiSources(r) => self.on_suwayomi_sources(r),
            Msg::SuwayomiExts(r) => self.on_suwayomi_exts(r),
            Msg::PrivacyWarp(r) => self.on_privacy_warp(r),
            Msg::SuwayomiDone(t) => self.on_suwayomi_done(t),
            Msg::SuwayomiStatus(t) => {
                crate::debug::log("suwayomi", &t);
                self.suwayomi_status = t;
                self.events.push(Event::MangaSources);
            }
            Msg::SuwayomiUp(r) => self.on_suwayomi_up(r),
            Msg::StudyImported(r) => {
                // a deck added from AnkiWeb's shared decks ends with this import
                let ok = r.is_ok();
                self.on_study_imported(r);
                self.shared_after_import(ok);
            }
            Msg::AnkiWebLogin(r) => self.on_ankiweb_login(r),
            Msg::ExtReply(id, hook, r) => self.on_ext_reply(id, hook, r),
            Msg::AutocardDone(r) => self.on_autocard_done(r),
            Msg::AutocardLocal(f) => self.on_autocard_local(f),
            Msg::ThemeFound(token, kind, r) => self.on_theme_found(token, kind, r),
            Msg::ThemeStream(req, r) => self.on_theme_stream(req, r),
            Msg::XRayDone(token, r) => self.on_xray_done(token, r),
            Msg::XRayTrivia(session, r) => self.on_xray_trivia(session, r),
            Msg::Kaiwa(m) => self.on_kaiwa(m),
            Msg::Voice(m) => self.on_voice(m),
            Msg::XRayPerson(link, r) => {
                self.xray_people.insert(link, r.unwrap_or_else(|e| Json::obj(vec![("error", Json::Str(e))])));
                self.events.push(Event::XRay);
            }
            Msg::AutocardTested(r) => self.events.push(Event::Info(MsgKind::Autocards, r.unwrap_or_else(|e| e))),
            Msg::AnkiWebPulled(r) => self.on_ankiweb_pulled(r),
            Msg::ClipDone(r) => self.on_clip_done(r),
            Msg::JlabLooked(nid, w, r) => self.on_jlab_looked(nid, w, r),
            Msg::Mined(session, r) => self.on_mined(session, r),
            Msg::LearnPack(lang, r) => self.on_learn_pack(lang, r),
            Msg::SharedList(g, w, r) => self.on_shared_list(g, w, r),
            Msg::SharedInfo(g, r) => self.on_shared_info(g, r),
            Msg::SharedProgress(id, d, t) => self.on_shared_progress(id, d, t),
            Msg::SharedDownloaded(id, r) => self.on_shared_downloaded(id, r),
            Msg::NovelCatalog(r) => self.on_novel_catalog(r),
            Msg::NovelInstalled(r) => self.on_novel_installed(r),
            Msg::NovelList(g, o, r) => self.on_novel_list(g, o, r),
            Msg::NovelView(g, r) => self.on_novel_view(g, r),
            Msg::NovelText(g, r) => self.on_novel_text(g, r),
            Msg::NovelCounts(c) => self.on_novel_counts(c),
            Msg::NovelImported(r) => self.on_novel_imported(r),
            Msg::MalLinked(i, m) => self.on_mal_linked(i, m),
            Msg::Extensions(m, r) => self.on_extensions(m, r),
            Msg::CardOpen(g, m) => self.on_card_open(g, *m),
            Msg::Cover(k, f) => self.on_cover(k, f),
            Msg::DlMissing(m, r, v) => self.on_dl_missing(*m, *r, v),
            Msg::DlSibling(m, s, e, st) => self.on_dl_sibling(*m, s, e, st.map(|b| *b)),
            Msg::DlSiblingsDone(f, t) => self.on_dl_siblings_done(f, t),
            Msg::Forced(t, r) => self.on_forced(t, r),
            Msg::OrigSubs(k, f, r) => self.on_orig_subs(k, f, r),
            Msg::AudioSync(t, p, r) => self.on_audio_sync(t, p, r),
            Msg::SubSync(t, a, r) => self.on_sub_sync(t, a, r),
            Msg::DubFound(t, g, l, lang, r) => self.on_dub_found(t, g, l, lang, r),
            Msg::DubStatus(t, g, line) => {
                if g == self.audio_sync.dub_gen && self.audio_sync.dub_busy && self.session.as_ref().map(|s| s.token == t).unwrap_or(false) {
                    self.audio_sync.dub_progress = line;
                    self.events.push(Event::Session);
                }
            }
            Msg::ForcedProgress(p) => {
                if let Some(t) = self.translating.as_mut() {
                    t.pct = t.pct.max(p);
                }
            }
            Msg::BackupDone(r, auto, now) => self.on_backup_done(r, auto, now),
            Msg::Restored(r) => self.on_restored(r),
            Msg::Cloud(d) => self.on_cloud(d),
            Msg::Update(d) => self.on_update(d),
            Msg::DlStreams(g, l, e) => self.on_dl_streams(g, l, e),
            Msg::DlAdded(r, h) => self.on_dl_added(r, h),
            Msg::DlProgress(h, p, d, c) => self.on_dl_progress(h, p, d, c),
            Msg::DlPurged(hash) => self.on_dl_purged(hash),
            Msg::DlEvicted(freed) => {
                if freed > 0 {
                    self.toast(&format!("Cache limit: freed {}.", torrent::human_bytes(freed)));
                }
                self.events.push(Event::Downloads);
                self.events.push(Event::Detail);
            }
            Msg::DlRemoved(text, restart) => {
                self.toast(&text);
                self.events.push(Event::Downloads);
                if restart {
                    self.dl_running.clear();
                    self.dl_resume_all();
                }
            }
        }
    }

    fn on_meta(&mut self, gen: u64, meta: Option<MetaItem>) {
        let mut request: Option<(String, String)> = None;
        if let Some(d) = self.detail.as_mut() {
            if d.gen != gen {
                return;
            }
            d.loading = false;
            if let Some(mut m) = meta {
                m.fill_from(&d.meta);
                let seasons = m.seasons();
                d.season = seasons.iter().copied().find(|s| *s >= 1).or_else(|| seasons.first().copied()).unwrap_or(1);
                d.meta = m;
            }
            // a title with episodes kept on disk too: offline its page (and so its saved episodes) still opens
            let saved_now = !d.meta.videos.is_empty()
                && (self.library.iter().any(|x| x.id == d.meta.id) || self.favorites.iter().any(|x| x.id == d.meta.id) || self.downloads.iter().any(|x| x.meta_id == d.meta.id));
            if saved_now {
                settings::save_detail(&self.data_dir, &d.meta);
            }
            if !d.meta.is_series() && d.streams.is_none() {
                request = Some((d.meta.kind.clone(), d.meta.id.clone()));
            }
        }
        self.events.push(Event::Detail);
        self.refresh_external_watched();
        self.start_mal_lookup();
        if let Some((kind, id)) = request {
            self.request_streams(&kind, &id, "", false);
        }
    }

    /// The stream of the list on screen that plays by itself: the one that played this video before, else the
    /// user's rule and the release of the last episode (see `streams::autoplay_pick`). A copy on disk stays first.
    fn autoplay_index(&self) -> usize {
        let Some(d) = self.detail.as_ref() else { return 0 };
        let Some(ss) = d.streams.as_ref() else { return 0 };
        if ss.list.first().map(|s| s.local.is_some()).unwrap_or(false) {
            return 0;
        }
        let (same, prev) = crate::streammemory::hints(&self.data_dir, &d.meta.id, &ss.key);
        let rule = streams::autoplay_rule(&self.settings.general.autoplay_rule);
        streams::autoplay_pick(&ss.list, prev.as_ref(), same.as_deref(), rule.as_ref())
    }

    fn on_streams(&mut self, gen: u64, list: Vec<Stream>, errors: Vec<String>) {
        if gen != self.stream_gen {
            return;
        }
        let mut auto_play = false;
        if let Some(d) = self.detail.as_mut() {
            let auto = d.autoplay;
            if let Some(ss) = d.streams.as_mut() {
                ss.loading = false;
                ss.errors = errors;
                auto_play = auto && !list.is_empty();
                ss.list = list;
            }
            d.autoplay = false;
        }
        self.events.push(Event::Streams);
        if auto_play {
            let idx = self.autoplay_index();
            self.play_from_detail(idx);
        } else {
            self.look_up_file_sizes(gen);
        }
    }

    /// A season pack listed for one episode shows the whole torrent's size; the file list (read by the torrent
    /// engine, nothing downloaded) gives the episode file's own size, which replaces it (HP, 2026-10-31).
    fn look_up_file_sizes(&mut self, gen: u64) {
        if self.settings.debrid.enabled && !self.settings.debrid.api_key.trim().is_empty() {
            return;
        }
        let Some(ss) = self.detail.as_mut().and_then(|d| d.streams.as_mut()) else { return };
        let (season, episode) = parse_episode_id(&ss.key);
        if episode.is_none() {
            return;
        }
        // every torrent whose size is not the episode's own (the list is ranked, so the first ones), not only those
        // that look like packs: an addon's size text is often the whole torrent's
        let mut packs: Vec<Stream> = Vec::new();
        for s in ss.list.iter_mut().filter(|s| s.needs_file_size()).take(12) {
            s.size_lookup = 1;
            packs.push(s.clone());
        }
        if packs.is_empty() {
            return;
        }
        self.events.push(Event::Streams);
        crate::debug::log("streams", &format!("reading the file list of {} torrents for the episode's size", packs.len()));
        for stream in packs {
            let engine = self.engine.clone();
            let exe = self.rt.rqbit.clone();
            let rt_dir = self.rt.dir.clone();
            let dl = self.data_dir.join("torrents");
            self.spawn(move |tx, http| {
                let hash = stream.info_hash.clone().unwrap_or_default();
                let found = (|| -> Result<(u64, u64), String> {
                    let magnet = streams::magnet(&stream).ok_or("no magnet")?;
                    let base = ensure_engine(&*http, &engine, exe.as_deref(), &rt_dir, &dl, &|_| {})?;
                    let rq = Rqbit { http: &*http, base };
                    let (_, files) = rq.list_files_within(&magnet, 60)?;
                    let i = torrent::pick_episode(&files, stream.file_idx, season, episode).ok_or("no episode file")?;
                    Ok((files[i].1, files.iter().map(|f| f.1).sum()))
                })();
                match found {
                    Ok((file, total)) => send(tx, Msg::FileSize(gen, hash, file, total)),
                    Err(e) => {
                        crate::debug::log("streams", &format!("episode size of {}: {}", hash, e));
                        send(tx, Msg::FileSize(gen, hash, 0, 0));
                    }
                }
            });
        }
    }

    // ----- player -----

    /// Creates the player. Call it with the window's OpenGL context current.
    /// `mode`: 0 = drawn into the window's OpenGL area, 1 = software into memory, 2 = hidden OpenGL window (shaders work).
    pub fn init_player(&mut self, on_update: Box<dyn Fn() + Send + Sync>, mode: u8) {
        if self.mpv.is_some() {
            return;
        }
        match Mpv::new(&self.rt.libmpv_candidates()) {
            Ok(mut m) => match match mode { 1 => m.init_render_sw(on_update), 2 => m.init_render_hidden(on_update), _ => m.init_render(on_update) } {
                Ok(()) => {
                    let _ = m.set("volume", &format!("{}", self.settings.general.volume));
                    self.mpv = Some(Arc::new(m));
                    self.observe_playback();
                    self.apply_sub_style();
                    self.apply_video_scale();
                }
                Err(e) => {
                    crate::debug::log("player", &format!("mpv could not draw into the window: {}", e));
                    self.mpv_error = Some(e)
                }
            },
            Err(e) => {
                crate::debug::log("player", &format!("mpv could not start: {}", e));
                self.mpv_error = Some(e)
            }
        }
    }

    pub fn apply_sub_style(&self) {
        if let Some(m) = &self.mpv {
            for (k, v) in substyle::mpv_options(&self.settings.subtitles.style) {
                let _ = m.set(&k, &v);
            }
        }
    }

    pub fn start_play(&mut self, list: Vec<Stream>, idx: usize, title: String, kind: String, sid: String, ep: EpInfo) {
        self.fire_hook(
            "onPlay",
            Json::obj(vec![
                ("title", title.clone().into()),
                ("kind", kind.clone().into()),
                ("id", sid.clone().into()),
                ("season", ep.season.map(|x| Json::Num(x as f64)).unwrap_or(Json::Null)),
                ("episode", ep.episode.map(|x| Json::Num(x as f64)).unwrap_or(Json::Null)),
            ]),
        );
        if let Some(m) = &self.mpv {
            let _ = m.cmd(&["stop"]);
            // events of the previous video (an end-of-file error above all) must not be read as this one's
            let _ = m.events();
        }
        self.stop_thumbs();
        // a video starts: Conversation's local AI and Whisper let go of the memory the player needs
        self.kaiwa_release();
        // a dub streamed for the last episode stops downloading
        self.dub_drop();
        // the episode that was playing, if it was watched to the end, leaves the disk (not when the same one starts again)
        let finished = self.session.as_mut().filter(|s| s.sid != sid).and_then(|s| s.finished.take());
        self.token_counter += 1;
        self.session = Some(Session {
            id: self.token_counter,
            token: self.token_counter,
            title,
            kind,
            sid,
            ep,
            streams: list,
            cur: idx,
            auto_skips: 0,
            stage: Stage::Resolving,
            resolve_msg: String::new(),
            sub_msg: String::new(),
            sub_job: SubJob::Skipped,
            resolved: None,
            file_loaded: false,
            loaded_at: None,
            buffer_ready: false,
            buffer_done_at: None,
            upscale_done: false,
            embedded_checked: false,
            start_pos: 0.0,
            cancel: Arc::new(AtomicBool::new(false)),
            notes: vec![],
            segments: vec![],
            skipped: vec![],
            segments_fetched: false,
            seg_prio: 0,
            seg_source: String::new(),
            kara: Default::default(),
            chapters: vec![],
            torrent: None,
            marked: false,
            finished: None,
            next_dismissed: false,
            next_started: false,
            meta: None,
            sub_cands: vec![],
            started: Instant::now(),
            prefetch_started: false,
            theme: Default::default(),
            xray: Default::default(),
        });
        if let Some(f) = finished {
            self.dl_purge(f);
        }
        if let Some(s) = self.session.as_mut() {
            s.meta = self.detail.as_ref().map(|d| {
                let mut m = d.meta.clone();
                m.videos.clear();
                m
            });
        }
        // from "Continue watching", otherwise where this episode stopped (whichever torrent it is opened from)
        let here = self.session.as_ref().map(|s| s.sid.clone()).unwrap_or_default();
        let start = self.resume_pos.take().or_else(|| self.resume_point(&here)).unwrap_or(0.0);
        if start > 0.0 {
            crate::debug::log("player", &format!("resume: {} starts at {:.0} s", here, start));
        }
        self.prefetch = None;
        self.begin_stream(start, None);
        self.events.push(Event::Session);
    }

    /// True while the user has not accepted P2P and this stream would be played as a torrent (not through debrid).
    pub fn needs_p2p_consent(&self, s: &Stream) -> bool {
        let debrid = self.settings.debrid.enabled && !self.settings.debrid.api_key.trim().is_empty();
        !self.settings.general.p2p_ok && s.is_torrent() && !debrid
    }

    /// The user accepted P2P on the player's notice: remember it and start the waiting stream.
    pub fn p2p_accept(&mut self) {
        self.settings.general.p2p_ok = true;
        self.save_settings();
        let start = match self.session.as_ref() {
            Some(s) if s.stage == Stage::Consent => s.start_pos,
            _ => return,
        };
        self.begin_stream(start, None);
    }

    /// Looks for opening / ending times once the video is playing and its length is known:
    /// first the chapters inside the file, then AniSkip, IntroDB and Anime Skip.
    fn probe_segments(&mut self, dur: f64) {
        self.start_karaoke();
        let chapters = self.mpv.as_ref().map(|m| m.chapters()).unwrap_or_default();
        let a = self.settings.anime_skip.clone();
        let Some(s) = self.session.as_mut() else { return };
        s.segments_fetched = true;
        let from_chapters = animeskip::segments_from(&chapters);
        s.chapters = chapters;
        if from_chapters.iter().any(|x| x.skippable()) {
            s.segments = from_chapters;
            s.seg_prio = 4;
            s.seg_source = "Chapters inside the file".to_string();
            self.events.push(Event::Session);
            return;
        }
        if s.kind != "series" || s.ep.show.is_empty() {
            return;
        }
        let Some(ep) = s.ep.episode else { return };
        let season = s.ep.season;
        let token = s.id;
        let show = s.ep.show.clone();
        let meta = s.meta.clone();
        let video_id = s.sid.clone();
        let meta_id = meta.as_ref().map(|m| m.id.clone()).unwrap_or_default();
        let anime = meta_id.starts_with("mal:")
            || meta_id.starts_with("kitsu:")
            || meta_id.starts_with("anilist:")
            || meta.as_ref().map(|m| m.genres.iter().any(|g| g.to_lowercase().contains("anim"))).unwrap_or(false);
        let imdb = [meta_id.as_str(), video_id.as_str()].iter().find(|x| x.starts_with("tt")).map(|x| x.split(':').next().unwrap_or("").to_string());
        let mal_direct = meta_id.strip_prefix("mal:").and_then(|x| x.parse::<u64>().ok());
        self.spawn(move |tx, http| {
            let send_segs = |prio: u8, name: &str, r: Result<Vec<Segment>, String>| {
                let found = matches!(&r, Ok(v) if !v.is_empty());
                send(tx, Msg::Segments(token, prio, name.to_string(), r));
                found
            };
            if anime {
                let mal = match mal_direct {
                    Some(m) => Ok(Some(m)),
                    None => skipdb::find_mal_id(&*http, &show, season),
                };
                match mal {
                    Ok(Some(id)) => {
                        // For a MyAnimeList entry the episode number counts from 1 inside that entry.
                        if send_segs(3, "AniSkip", skipdb::aniskip(&*http, id, ep, dur)) {
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        send_segs(3, "AniSkip", Err(e));
                    }
                }
            }
            if let Some(imdb) = imdb {
                if send_segs(2, "IntroDB", skipdb::introdb(&*http, &imdb, season.unwrap_or(1), ep)) {
                    return;
                }
            }
            if a.enabled && !a.client_id.trim().is_empty() {
                let c = animeskip::Client { http: &*http, client_id: a.client_id.clone(), token: a.token.clone() };
                send_segs(1, "Anime Skip", c.segments(&show, season, ep, dur));
            }
        });
    }

    /// Where the skip data came from, for the Statistics panel.
    pub fn skip_info(&self) -> (usize, String) {
        match self.session.as_ref() {
            Some(s) => (s.chapters.len(), if s.segments.iter().any(|x| x.skippable()) { s.seg_source.clone() } else { "none found".to_string() }),
            None => (0, String::new()),
        }
    }

    /// Marks for the seek bar: where openings, endings and recaps start and end (or the chapters when there is no such data).
    pub fn seek_marks(&self) -> Vec<f64> {
        let Some(s) = self.session.as_ref() else { return vec![] };
        let mut v: Vec<f64> = Vec::new();
        let skippable: Vec<&Segment> = s.segments.iter().filter(|x| x.skippable()).collect();
        if skippable.is_empty() {
            v.extend(s.chapters.iter().map(|c| c.0).filter(|t| *t > 1.0));
        } else {
            for x in skippable {
                v.push(x.start);
                if let Some(e) = x.end {
                    v.push(e);
                }
            }
        }
        v
    }

    /// Starts (or restarts) the pipeline for the current stream. `reuse` keeps an already-prepared
    /// subtitle when only the quality changes.
    pub fn begin_stream(&mut self, start_pos: f64, reuse: Option<Option<SubFile>>) {
        // the first torrent: say what P2P means before anything is downloaded or shared (idea from NuvioMobile)
        let ask = self.session.as_ref().and_then(|s| s.streams.get(s.cur)).map(|x| self.needs_p2p_consent(x)).unwrap_or(false);
        if ask {
            if let Some(s) = self.session.as_mut() {
                s.stage = Stage::Consent;
                s.start_pos = start_pos;
            }
            self.events.push(Event::Session);
            return;
        }
        let subs_on = self.settings.subtitles.enabled;
        let (token, cancel, stream, kind, sid) = {
            let Some(s) = self.session.as_mut() else { return };
            s.cancel.store(true, Ordering::SeqCst);
            s.cancel = Arc::new(AtomicBool::new(false));
            self.token_counter += 1;
            s.token = self.token_counter;
            s.stage = Stage::Resolving;
            s.resolve_msg = "Finding the video source".to_string();
            s.sub_msg.clear();
            s.resolved = None;
            s.file_loaded = false;
            s.loaded_at = None;
            s.buffer_ready = false;
            s.buffer_done_at = None;
            s.upscale_done = false;
            s.embedded_checked = false;
            s.segments_fetched = false;
            s.notes.clear();
            s.start_pos = start_pos;
            s.sub_job = match &reuse {
                Some(prev) => SubJob::Done(prev.clone()),
                None if subs_on => SubJob::Running,
                None => SubJob::Skipped,
            };
            (s.token, s.cancel.clone(), s.streams[s.cur].clone(), s.kind.clone(), s.sid.clone())
        };
        // Anime catalogs (Kitsu, MAL, AniList) give ids most subtitle addons, OpenSubtitles first of all, do not
        // know: ask them with the IMDb id too when the metadata has one.
        let imdb_sid = self.session.as_ref().and_then(|s| imdb_episode_id(&s.sid, s.meta.as_ref().map(|m| m.imdb_id.as_str()).unwrap_or(""), &s.kind, s.ep.season, s.ep.episode));

        let st = self.settings.clone();
        let engine = self.engine.clone();
        let exe = self.rt.rqbit.clone();
        let rt_dir = self.rt.dir.clone();
        let dl = self.data_dir.join("torrents");
        let cancel2 = cancel.clone();
        let stream_for_subs = stream.clone();
        let sid_for_resolve = sid.clone();
        if let Some(res) = self.preresolved.take() {
            // The next episode was prepared while the last one played.
            self.on_resolved(token, Ok(res));
        } else {
            self.spawn(move |tx, http| {
                let status = |m: String| send(tx, Msg::ResolveStatus(token, m));
                let cancelled = || cancel2.load(Ordering::SeqCst);
                let r = resolve_stream(&*http, &st, &stream, &engine, exe.as_deref(), &rt_dir, &dl, &sid_for_resolve, &status, &cancelled);
                send(tx, Msg::Resolved(token, r));
            });
        }

        // The subtitles the addons offer are always asked for: the Subtitles menu and Forced translation list them
        // even when automatic subtitles are off; only choosing and preparing one needs the setting.
        if reuse.is_none() {
            let st = self.settings.clone();
            let cache = self.data_dir.join("subtitles");
            let target = st.target_lang(&self.system_lang);
            let own_name = release_name(&stream_for_subs);
            let gate = self.local_ai_gate(translate::uses_local_ai("subtitles", &st));
            self.spawn(move |tx, http| {
                let status = |m: String| send(tx, Msg::SubStatus(token, m));
                let result = (|| -> Result<Option<SubFile>, String> {
                    // The release name of the file being played: subtitles that match it are asked for first,
                    // because their timing fits.
                    let names: Vec<String> = own_name.into_iter().collect();
                    let ask = |id: &str| if names.is_empty() { addons::fetch_subtitles(&*http, &st.addons, &kind, id) } else { addons::fetch_subtitles_for(&*http, &st.addons, &kind, id, &names) };
                    let mut found = ask(&sid);
                    if let Some(alt) = &imdb_sid {
                        let more = ask(alt);
                        for c in more.items {
                            if !found.items.iter().any(|x| x.url == c.url) {
                                found.items.push(c);
                            }
                        }
                    }
                    crate::debug::log("subs", &format!("{} subtitle(s) offered for {}{}{}", found.items.len(), sid, imdb_sid.as_deref().map(|a| format!(" (also asked as {})", a)).unwrap_or_default(), if found.errors.is_empty() { String::new() } else { format!(" | {}", found.errors.join("; ")) }));
                    send(tx, Msg::SubCands(token, found.items.clone()));
                    if !subs_on || found.items.is_empty() {
                        return Ok(None);
                    }
                    // the translator of "Automatic subtitles" in Preferences > Translators (none while subtitle translation is off)
                    let note: Box<dyn Fn(&str) + Send + Sync> = Box::new(move |m: &str| send(tx, Msg::SubStatus(token, m.to_string())));
                    let translator = translate::for_use("subtitles", &st, &*http).ok().map(|t| controller_translators::gated(t, &gate, Some(note)));
                    // A subtitle in a language the user knows first; else the best one translated (controller_sub.rs).
                    controller_sub::prepare_known(&*http, &st.general.known_langs, &found.items, &target, &st.subtitles.source_langs, translator.as_deref(), &cache, st.subtitles.strip_sdh, &status)
                })();
                send(tx, Msg::SubDone(token, result));
            });
        }
        self.events.push(Event::Session);
    }

    fn on_resolved(&mut self, token: u64, r: Result<Resolved, String>) {
        if self.session.as_ref().map(|s| s.token == token).unwrap_or(false) {
            if let Ok(res) = &r {
                self.note_cache_entry(res);
            }
        }
        let Some(s) = self.session.as_mut().filter(|s| s.token == token) else { return };
        if let Err(e) = &r {
            if e.starts_with(NO_PEERS) && s.auto_skips < 3 && s.cur + 1 < s.streams.len() {
                s.auto_skips += 1;
                let next = s.cur + 1;
                crate::debug::log("player", &format!("source {} found no peers, trying {}", s.cur, next));
                self.toast("No peers for this source. Trying the next one.");
                self.switch_stream(next);
                return;
            }
        }
        let Some(s) = self.session.as_mut().filter(|s| s.token == token) else { return };
        match r {
            Err(e) => s.stage = Stage::Failed(e),
            Ok(res) => s.resolved = Some(res),
        }
        self.events.push(Event::Session);
        self.load_resolved();
    }

    /// Hands the resolved URL to mpv, once both exist.
    fn load_resolved(&mut self) {
        let Some(mpv) = self.mpv.clone() else { return };
        let st = &self.settings;
        let Some(s) = self.session.as_mut() else { return };
        if s.stage != Stage::Resolving {
            return;
        }
        let Some(res) = s.resolved.take() else { return };
        s.torrent = res.torrent.clone();
        let g = &st.general;
        let _ = mpv.set("hwdec", &g.hwdec);
        if st.upscaling.enabled {
            for (k, v) in anime4k::mpv_options(&st.upscaling) {
                let _ = mpv.set(&k, &v);
            }
        }
        let _ = mpv.set("pause", "yes");
        let _ = mpv.set("demuxer-readahead-secs", &format!("{}", (g.buffer_seconds * 3).max(30)));
        // What mpv reads ahead is kept in a file on the disk, not in memory (it was up to 512 MB + 50 MB behind in
        // RAM). A phone keeps a smaller buffer in memory (its libmpv and storage vary too much for the disk cache).
        if cfg!(target_os = "android") {
            let _ = mpv.set("demuxer-max-bytes", "96MiB");
            let _ = mpv.set("demuxer-max-back-bytes", "24MiB");
        } else {
            let dir = self.data_dir.join("cache").join("mpv");
            if std::fs::create_dir_all(&dir).is_ok() {
                let _ = mpv.set("cache-dir", &dir.to_string_lossy());
                let _ = mpv.set("cache-on-disk", "yes");
                let _ = mpv.set("demuxer-max-bytes", "512MiB");
                let _ = mpv.set("demuxer-max-back-bytes", "128MiB");
            } else {
                let _ = mpv.set("demuxer-max-bytes", "192MiB");
                let _ = mpv.set("demuxer-max-back-bytes", "32MiB");
            }
        }
        let _ = mpv.set("start", &if s.start_pos > 1.0 { format!("{:.1}", s.start_pos) } else { "none".to_string() });
        let _ = mpv.set("http-header-fields", &crate::mpv::header_list(&res.headers));
        let _ = mpv.set("sub-visibility", "yes");
        // Android: mpv's own log (why a stream fails to open, a crash inside it) beside ours, since its messages are
        // otherwise lost there; the folder is read with `waydroid shell` / `adb`
        if cfg!(target_os = "android") {
            if let Some(d) = crate::debug::dir() {
                let _ = mpv.set("log-file", &d.join("mpv-player.log").to_string_lossy());
            }
        }
        // what is still queued belongs to the previous video or attempt (its end-of-file error would fail this one)
        let _ = mpv.events();
        match mpv.cmd(&["loadfile", &res.url]) {
            Ok(()) => s.stage = Stage::Loading,
            Err(e) => s.stage = Stage::Failed(e),
        }
        let (url, hdrs) = (res.url.clone(), res.headers.clone());
        self.events.push(Event::Session);
        self.start_thumbs(url, hdrs);
    }

    /// Runs on every tick while a session exists.
    pub fn drive_session(&mut self) {
        self.load_resolved();
        self.karaoke_step();
        self.theme_step();
        self.xray_step();
        self.studyplay_step();
        self.audio_step();
        let Some(mpv) = self.mpv.clone() else {
            // No player (it failed to start): say so instead of "Finding the video source" for ever.
            if let (Some(e), Some(s)) = (self.mpv_error.clone(), self.session.as_mut()) {
                if !matches!(s.stage, Stage::Failed(_)) {
                    s.stage = Stage::Failed(format!("The player could not start: {}", e));
                    self.events.push(Event::Session);
                }
            }
            return;
        };
        let quiz_open = self.study_quiz_open();
        let st = &self.settings;
        let target = st.target_lang(&self.system_lang);
        let shaders_dir = self.rt.shaders_dir.clone();
        let Some(s) = self.session.as_mut() else { return };
        let before = s.stage.clone();

        let mut props: Vec<(String, Option<crate::mpv::PropValue>)> = Vec::new();
        for ev in mpv.events() {
            match ev {
                Evt::Prop(name, v) => props.push((name, v)),
                Evt::FileLoaded => {
                    s.file_loaded = true;
                    s.loaded_at = Some(Instant::now());
                    // this stream works: the next episode keeps its release, resuming this video plays it again
                    if let (Some(d), Some(cur)) = (self.detail.as_ref(), s.streams.get(s.cur)) {
                        let mid = &d.meta.id;
                        if s.sid == *mid || s.sid.starts_with(&format!("{}:", mid)) {
                            crate::streammemory::remember(&self.data_dir, mid, &s.sid, cur);
                        }
                    }
                    if s.stage == Stage::Loading {
                        s.stage = Stage::Buffering;
                    }
                }
                Evt::EndFile { reason: 4, error } => {
                    s.stage = Stage::Failed(format!("mpv could not play this stream ({}). Try another one.", if error.is_empty() { "unknown error".to_string() } else { error }));
                }
                _ => {}
            }
        }
        if s.stage == Stage::Buffering {
            let loaded_for = s.loaded_at.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);

            // A subtitle track in the user's language inside the file beats downloading one.
            if !s.embedded_checked {
                s.embedded_checked = true;
                if st.subtitles.enabled {
                    let tracks = mpv.tracks("sub");
                    let known = &st.general.known_langs;
                    let chosen = if known.is_empty() {
                        tracks.iter().find(|t| !t.external && util::lang_base(&t.lang) == target)
                    } else {
                        // Languages the user knows: the best built-in one, unless a download in a better language comes.
                        let inside: Vec<(i64, String)> = tracks.iter().filter(|t| !t.external).map(|t| (t.id, t.lang.clone())).collect();
                        let cands = if matches!(s.sub_job, SubJob::Running) && s.sub_cands.is_empty() { None } else { Some(&s.sub_cands[..]) };
                        match controller_sub::pick_embedded(known, &inside, cands) {
                            controller_sub::EmbeddedPick::Use(id) => tracks.iter().find(|t| t.id == id),
                            controller_sub::EmbeddedPick::Wait => {
                                s.embedded_checked = false;
                                None
                            }
                            controller_sub::EmbeddedPick::Skip => None,
                        }
                    };
                    if let Some(t) = chosen {
                        let _ = mpv.set("sid", &t.id.to_string());
                        s.sub_job = SubJob::Skipped;
                        s.notes.push(format!("Using the subtitles built into the video ({}).", t.label()));
                    }
                }
            }

            if !s.upscale_done {
                let h = mpv.get_i64("video-params/h");
                if h.is_some() || loaded_for > 4.0 {
                    s.upscale_done = true;
                    let u = &st.upscaling;
                    if u.enabled {
                        let missing = anime4k::missing_for(&shaders_dir, u);
                        if !missing.is_empty() {
                            s.notes.push("Anime4K shaders are not installed. Open Preferences > Upscalling to get them.".to_string());
                        } else if anime4k::should_apply(u, h) {
                            match anime4k::mpv_commands(&shaders_dir, u) {
                                Ok(cmds) => {
                                    for c in cmds {
                                        let _ = mpv.cmd_owned(&c);
                                    }
                                }
                                Err(e) => s.notes.push(e),
                            }
                        } else {
                            let _ = mpv.cmd_owned(&anime4k::clear_command());
                            s.notes.push(format!("The video is {}p, above your {}p upscaling limit, so Anime4K was skipped.", h.unwrap_or(0), u.max_source_height));
                        }
                    } else {
                        let _ = mpv.cmd_owned(&anime4k::clear_command());
                    }
                }
            }

            if !s.buffer_ready {
                let cache = mpv.get_f64("demuxer-cache-duration").unwrap_or(0.0);
                let dur = mpv.get_f64("duration").unwrap_or(0.0);
                let eof = mpv.get_flag("eof-reached").unwrap_or(false);
                let whole_file = dur > 0.0 && cache >= dur - 1.0;
                if cache >= st.general.buffer_seconds as f64 || eof || whole_file || loaded_for > 120.0 {
                    s.buffer_ready = true;
                    s.buffer_done_at = Some(Instant::now());
                }
            }

            if s.buffer_ready && s.upscale_done {
                let waited = s.buffer_done_at.map(|t| t.elapsed().as_secs()).unwrap_or(0);
                if matches!(s.sub_job, SubJob::Running) && waited >= st.subtitles.max_wait_secs as u64 {
                    s.sub_job = SubJob::TimedOut;
                }
                if !matches!(s.sub_job, SubJob::Running) {
                    match &s.sub_job {
                        SubJob::Done(Some(f)) => {
                            let lang = if f.lang.is_empty() { "und".to_string() } else { f.lang.clone() };
                            let title = if f.translated { format!("{} (translated)", lang) } else { lang.clone() };
                            let path = f.path.to_string_lossy().to_string();
                            let _ = mpv.cmd(&["sub-add", &path, "select", &title, &lang]);
                            if f.untranslated {
                                s.notes.push("Subtitles are in their original language because no translator is set up.".to_string());
                            }
                        }
                        SubJob::Done(None) => {
                            if st.subtitles.enabled && !s.notes.iter().any(|n| n.starts_with("Using the subtitles")) {
                                s.notes.push("No subtitles were found for this title.".to_string());
                            }
                        }
                        SubJob::Failed(e) => s.notes.push(format!("Subtitles unavailable: {}", e)),
                        SubJob::TimedOut => s.notes.push("Playback started; the subtitles are added as soon as they are ready.".to_string()),
                        _ => {}
                    }
                    // Translations the user made for this episode come back.
                    for f in self.forced.iter().filter(|f| f.sid == s.sid) {
                        let _ = mpv.cmd(&["sub-add", &f.path, "auto", &f.title, &f.lang]);
                    }
                    // Audio files loaded for this episode (a dub) come back with their delay.
                    if controller_audio::restore_ext_audio(&mpv, &mut self.ext_audio, &self.data_dir, &s.sid, &s.ep) {
                        self.audio_sync.pending = true;
                    }
                    let _ = mpv.set("pause", "no");
                    s.stage = Stage::Playing;
                    if let Some(first) = s.notes.first().cloned() {
                        self.events.push(Event::Toast(first));
                    }
                }
            }
        }

        // Automatic skipping of openings and endings.
        // (not while a question of the macaw holds the episode: it skips once the question is closed)
        if s.stage == Stage::Playing && st.anime_skip.auto_skip && !s.segments.is_empty() && !quiz_open {
            let pos = mpv.get_f64("time-pos").unwrap_or(0.0);
            let dur = mpv.get_f64("duration").unwrap_or(0.0);
            if let Some(seg) = animeskip::segment_at(&s.segments, pos, dur) {
                let key = seg.start as i64;
                if !s.skipped.contains(&key) {
                    s.skipped.push(key);
                    let to = animeskip::skip_target(seg, dur);
                    let label = seg.label().replace("Skip ", "");
                    let _ = mpv.cmd(&["seek", &format!("{:.2}", to), "absolute"]);
                    self.events.push(Event::Toast(format!("Skipped the {}", label.to_lowercase())));
                }
            }
        }
        if s.stage != before {
            crate::debug::log("player", &format!("stage {:?} -> {:?}", before, s.stage));
            self.events.push(Event::Session);
        }
        for (name, v) in props {
            self.on_playback_prop(&name, v);
        }
    }

    /// The Anime Skip button to show right now: its label and the time it jumps to.
    pub fn current_skip(&self) -> Option<(String, f64)> {
        let s = self.session.as_ref()?;
        if s.stage != Stage::Playing || s.segments.is_empty() {
            return None;
        }
        let m = self.mpv.as_ref()?;
        let pos = m.get_f64("time-pos")?;
        let dur = m.get_f64("duration").unwrap_or(0.0);
        let seg = animeskip::segment_at(&s.segments, pos, dur)?;
        if s.skipped.contains(&(seg.start as i64)) {
            return None;
        }
        Some((seg.label().to_string(), animeskip::skip_target(seg, dur)))
    }

    pub fn do_skip(&mut self) {
        let Some((_, to)) = self.current_skip() else { return };
        if let Some(m) = &self.mpv {
            let _ = m.cmd(&["seek", &format!("{:.2}", to), "absolute"]);
        }
        if let (Some(s), Some(m)) = (self.session.as_mut(), self.mpv.as_ref()) {
            let pos = m.get_f64("time-pos").unwrap_or(0.0);
            let dur = m.get_f64("duration").unwrap_or(0.0);
            if let Some(seg) = animeskip::segment_at(&s.segments, pos, dur) {
                s.skipped.push(seg.start as i64);
            }
        }
    }

    /// The Skip button in the control bar: jumps forward by the configured number of seconds.
    pub fn skip_forward(&self) {
        if let Some(m) = &self.mpv {
            let _ = m.cmd(&["seek", &self.settings.general.skip_seconds.max(1).to_string(), "relative"]);
        }
    }

    /// Switches to another stream of the same title and keeps the current position.
    pub fn switch_stream(&mut self, idx: usize) {
        let pos = self.mpv.as_ref().and_then(|m| m.get_f64("time-pos")).unwrap_or(0.0);
        let reuse = match self.session.as_ref().map(|s| &s.sub_job) {
            Some(SubJob::Done(f)) => Some(f.clone()),
            _ => Some(None),
        };
        if let Some(s) = self.session.as_mut() {
            if idx >= s.streams.len() {
                return;
            }
            s.cur = idx;
        }
        self.stop_thumbs();
        self.begin_stream(pos, reuse);
    }

    pub fn retry_stream(&mut self) {
        let pos = self.session.as_ref().map(|s| s.start_pos).unwrap_or(0.0);
        self.begin_stream(pos, None);
    }

    pub fn close_player(&mut self) {
        if let Some(s) = &self.session {
            let (pos, dur) = self.mpv.as_ref().map(|m| (m.get_f64("time-pos").unwrap_or(0.0), m.get_f64("duration").unwrap_or(0.0))).unwrap_or((0.0, 0.0));
            self.fire_hook("onPlayEnd", Json::obj(vec![("title", s.title.clone().into()), ("position", Json::Num(pos)), ("duration", Json::Num(dur))]));
        }
        self.save_progress_now();
        self.prefetch = None;
        self.preresolved = None;
        let finished = self.session.take().and_then(|s| {
            s.cancel.store(true, Ordering::SeqCst);
            s.finished
        });
        self.dub_drop();
        self.stop_thumbs();
        if let Some(m) = &self.mpv {
            let _ = m.cmd(&["stop"]);
        }
        // the episode watched to the end leaves the disk, and the cache goes back under its limit
        if let Some(f) = finished {
            self.dl_purge(f);
        }
        self.dl_enforce_limit();
        // the opening / ending video of the X-Ray would go on playing
        self.theme_stop_quiet();
        // what belonged to this video: the clock, and the study hint shown over its subtitles
        self.clock = Default::default();
        self.flash = None;
        self.hint_text.clear();
        self.events.push(Event::Session);
    }

    /// Re-applies the Anime4K chain to the video that is playing now.
    pub fn apply_upscaling_now(&mut self) {
        let Some(mpv) = self.mpv.clone() else { return };
        let u = &self.settings.upscaling;
        if !u.enabled {
            let _ = mpv.cmd_owned(&anime4k::clear_command());
            return;
        }
        if !anime4k::missing_for(&self.rt.shaders_dir, u).is_empty() {
            self.toast("Anime4K shaders are not installed. Open Preferences > Upscalling to get them.");
            return;
        }
        for (k, v) in anime4k::mpv_options(u) {
            let _ = mpv.set(&k, &v);
        }
        if let Ok(cmds) = anime4k::mpv_commands(&self.rt.shaders_dir, u) {
            for c in cmds {
                let _ = mpv.cmd_owned(&c);
            }
        }
    }

    // ----- preview pictures on the seek bar -----

    fn start_thumbs(&mut self, url: String, headers: Vec<(String, String)>) {
        if !self.settings.general.thumbnails {
            return;
        }
        if self.thumb.is_none() {
            let libs = self.rt.libmpv_candidates();
            let out = self.data_dir.join("thumbs");
            let _ = std::fs::create_dir_all(&out);
            let msg = self.tx.clone();
            let (ctx, rx) = channel::<ThumbCmd>();
            std::thread::spawn(move || thumb_worker(libs, out, msg, rx));
            self.thumb = Some(ctx);
        }
        if let Some(t) = &self.thumb {
            let _ = t.send(ThumbCmd::Load(url, headers));
        }
    }

    fn stop_thumbs(&mut self) {
        if let Some(t) = self.thumb.take() {
            let _ = t.send(ThumbCmd::Quit);
        }
    }

    /// Asks for the picture at `secs`. It arrives as `Event::Thumb`.
    pub fn request_thumb(&self, secs: f64) {
        if let Some(t) = &self.thumb {
            let _ = t.send(ThumbCmd::Seek(secs));
        }
    }

    // ----- account and tool actions -----

    pub fn install_addon(&mut self, input: &str) {
        let input = input.to_string();
        let mut entries = self.settings.addons.clone();
        self.events.push(Event::Info(MsgKind::Addon, "Contacting the addon...".to_string()));
        self.spawn(move |tx, http| {
            let r = addons::install(&*http, &mut entries, &input).map(|name| (entries, name));
            send(tx, Msg::AddonInstalled(r));
        });
    }

    /// The one addon box of Preferences > Addons & accounts: a video addon, a repository of manga / comic
    /// extensions or a list of novel plugins. `kind` is "auto" (found out from the link, or from its answer) or a mode.
    pub fn add_any_addon(&mut self, text: &str, kind: &str) {
        use crate::addon_kind::{self, Kind};
        let text = text.trim().to_string();
        if text.is_empty() {
            self.events.push(Event::Info(MsgKind::Addon, "Paste the link of the addon.".to_string()));
            return;
        }
        if let Some(k) = Kind::from_id(kind).or_else(|| addon_kind::guess(&text)) {
            self.add_addon_as(&text, k);
            return;
        }
        self.events.push(Event::Info(MsgKind::Addon, "Reading the link…".to_string()));
        self.spawn(move |tx, http| {
            let url = if text.starts_with("http://") || text.starts_with("https://") { text.clone() } else { format!("https://{}", text) };
            let kind = crate::http::get_json(&*http, &url, 30).ok().and_then(|j| addon_kind::classify(&j));
            send(tx, Msg::AddonKind(text, kind));
        });
    }

    pub fn add_addon_as(&mut self, text: &str, kind: crate::addon_kind::Kind) {
        use crate::addon_kind::Kind;
        crate::debug::log("addons", &format!("add {:?}: {}", kind, text));
        match kind {
            Kind::Video => self.install_addon(text),
            Kind::Images => {
                let msg = match self.suwayomi_add_repo(text) {
                    Ok(()) => "Added to Images: its sources are in Images > Sources.".to_string(),
                    Err(e) => e,
                };
                self.events.push(Event::Info(MsgKind::Addon, msg));
            }
            Kind::Text => {
                self.settings.general.text_repo = text.to_string();
                self.save_settings();
                self.novel_load_catalog();
                self.events.push(Event::Info(MsgKind::Addon, "Added to Text: its sources are in Text > Sources.".to_string()));
            }
        }
    }

    pub fn remove_addon(&mut self, url: &str) {
        self.settings.addons.retain(|a| a.url != url);
        self.save_settings();
        self.load_home();
    }

    pub fn test_real_debrid(&mut self) {
        let key = self.settings.debrid.api_key.clone();
        self.events.push(Event::Info(MsgKind::RealDebrid, "Checking...".to_string()));
        self.spawn(move |tx, http| {
            let r = RealDebrid::new(&*http, &key).user();
            send(tx, Msg::Info(MsgKind::RealDebrid, r.unwrap_or_else(|e| e)));
        });
    }

    pub fn test_translator(&mut self) {
        let sub = self.settings.subtitles.clone();
        let target = self.settings.target_lang(&self.system_lang);
        self.events.push(Event::Info(MsgKind::Translator, "Testing...".to_string()));
        let gate = self.local_ai_gate(sub.provider == "ollama");
        let model = translate::local_model(&self.settings);
        self.spawn(move |tx, http| {
            // the local AI on desktop; on Android and iOS `local_ai` gives the cloud translator that stands in for it
            let tr = if sub.provider == "ollama" { translate::local_ai(&sub, &*http, &model) } else { translate::from_settings(&sub, &*http) };
            let r = match tr.map(|t| controller_translators::gated(t, &gate, None)) {
                None => Err("No translator is set up. Choose a provider and fill in its details first.".to_string()),
                Some(t) => t
                    .translate(&["Where are you going?".to_string()], "en", &target)
                    .map(|v| format!("{}: \"Where are you going?\" becomes \"{}\" ({})", t.name(), v.first().cloned().unwrap_or_default(), target)),
            };
            send(tx, Msg::Info(MsgKind::Translator, r.unwrap_or_else(|e| e)));
        });
    }

    pub fn download_shaders(&mut self) {
        let dir = self.rt.shaders_dir.clone();
        self.shader_busy = true;
        self.events.push(Event::Info(MsgKind::Shaders, "Starting...".to_string()));
        self.spawn(move |tx, http| {
            let status = |m: String| send(tx, Msg::Info(MsgKind::Shaders, m));
            let r = anime4k::install_from_github(&*http, &dir, &status);
            send(tx, Msg::ShaderDone(r));
        });
    }

    /// Stops everything that runs in the background. Called when the window closes.
    pub fn shutdown(&mut self) {
        self.stop_thumbs();
        if let Some(m) = &self.mpv {
            let _ = m.cmd(&["stop"]);
        }
        if let Ok(mut g) = self.engine.lock() {
            g.take();
        }
        self.suwayomi_host = None;
        // the local AI and the local Whisper, when Reelhouse started them
        self.ollama.stop();
        self.whisper.stop();
        // a call of Gemini Live hangs up
        self.kaiwa_live_shutdown();
        // the last seconds of study time
        self.time_flush();
        // where the readers are (written only every few pages while reading)
        if self.manga_reader.is_some() {
            self.save_manga();
        }
        if self.novel_reader.is_some() {
            self.save_novels();
        }
        let _ = self.settings.save(&self.data_dir);
    }

    /// Deletes the files the torrent engine downloaded while streaming. Torrents kept with
    /// "Keep downloaded" stay.
    pub fn clear_torrent_cache(&mut self) -> String {
        if self.session.is_some() {
            return "Stop the video first.".to_string();
        }
        if let Ok(mut g) = self.engine.lock() {
            g.take();
        }
        self.dl_running.clear();
        let dir = self.data_dir.join("torrents");
        let mut keep: Vec<String> = Vec::new();
        for d in self.downloads.iter().filter(|d| d.saved) {
            keep.push(d.root());
            keep.push(d.name.clone());
        }
        let mut freed = 0u64;
        let mut failed = false;
        for (name, size) in downloads::folder_entries(&dir) {
            if keep.contains(&name) {
                continue;
            }
            let path = dir.join(&name);
            let ok = if path.is_dir() { std::fs::remove_dir_all(&path).is_ok() } else { std::fs::remove_file(&path).is_ok() };
            if ok {
                freed += size;
            } else {
                failed = true;
            }
        }
        self.downloads.retain(|d| d.saved);
        downloads::save(&self.data_dir, &self.downloads);
        self.dl_resume_all();
        self.events.push(Event::Downloads);
        if failed {
            "Some files are in use and were kept.".to_string()
        } else if freed == 0 {
            "Nothing to clear.".to_string()
        } else {
            format!("Freed {}.", torrent::human_bytes(freed))
        }
    }

    pub fn torrent_cache_bytes(&self) -> u64 {
        dir_size(&self.data_dir.join("torrents"))
    }

    pub fn anime_skip_login(&mut self, email: &str, password: &str) {
        let a = self.settings.anime_skip.clone();
        let (email, password) = (email.trim().to_string(), password.to_string());
        self.settings.anime_skip.email = email.clone();
        self.events.push(Event::Info(MsgKind::AnimeSkip, "Logging in...".to_string()));
        self.spawn(move |tx, http| {
            let c = animeskip::Client { http: &*http, client_id: a.client_id.clone(), token: String::new() };
            send(tx, Msg::AsLogin(c.login(&email, &password)));
        });
    }

    pub fn anime_skip_logout(&mut self) {
        self.settings.anime_skip.token.clear();
        self.settings.anime_skip.refresh_token.clear();
        self.save_settings();
        self.events.push(Event::Info(MsgKind::AnimeSkip, "Logged out.".to_string()));
    }
}

impl Default for Core {
    fn default() -> Self {
        Core::new()
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        // Stop the torrent engine so it does not keep running after the window closes.
        if let Ok(mut g) = self.engine.lock() {
            g.take();
        }
        self.ollama.stop();
        self.whisper.stop();
        self.kaiwa_live_shutdown();
        self.stop_thumbs();
    }
}

/// Owns a silent second mpv that opens the same video and saves a small picture for each requested time,
/// the same idea as the thumbfast script (github.com/po5/thumbfast) but built into the program.
///
/// Times are cut into slots (`slot_len` seconds, from the duration). A slot is made once and kept; the answer carries the
/// slot's time, so the UI can keep its own cache and show the nearest picture at once. After the first hover, idle time
/// fills a coarse-to-fine grid of slots over the whole video, so sliding fast finds pictures already made.
fn thumb_worker(libs: Vec<PathBuf>, out_dir: PathBuf, msg: Sender<Msg>, rx: Receiver<ThumbCmd>) {
    use std::collections::{BTreeMap, VecDeque};
    let m = match Mpv::new_thumbnailer(&libs) {
        Ok(m) => m,
        Err(e) => {
            crate::debug::log("thumbs", &format!("the preview player could not start: {}", e));
            return;
        }
    };
    let _ = m.set("screenshot-format", "jpeg");
    let _ = m.set("screenshot-jpeg-quality", "70");
    // Small pictures: the screenshot is scaled by the filter, so a 4K video costs no more than a 320 px one.
    let _ = m.set("vf", "scale=w=320:h=-2");
    let _ = m.set("demuxer-readahead-secs", "2");
    // Old pictures of an earlier run (the names change with each video, so nothing stale can be shown).
    if let Ok(rd) = std::fs::read_dir(&out_dir) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    // The generation in the file names comes from one counter for the whole program: a new worker is started for
    // each video, and if it counted from 0 again the names would repeat and Qt would show the cached pictures of
    // the previous video.
    static THUMB_GEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let mut gen: u32 = THUMB_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed).wrapping_add(1);
    let mut loaded = false;
    let mut dur = 0.0f64;
    let mut slot_len = 5.0f64;
    let mut hovered = false;
    let mut grid_filled = false;
    let mut want: Option<f64> = None;
    let mut grid: VecDeque<f64> = VecDeque::new();
    let mut made: BTreeMap<i64, PathBuf> = BTreeMap::new();
    let slot_of = |t: f64, len: f64| (t.max(0.0) / len).round() as i64;
    // commands that arrived while a seek was being waited for, handled at the top of the next turn
    let mut pending: Vec<ThumbCmd> = Vec::new();
    // grid points whose seek gave no frame yet, and how often each was tried
    let mut tries: BTreeMap<i64, u8> = BTreeMap::new();

    loop {
        // Block only when there is nothing to do; otherwise just take what is queued.
        let busy = loaded && (want.is_some() || (hovered && !grid.is_empty()));
        let mut cmds: Vec<ThumbCmd> = std::mem::take(&mut pending);
        if !busy && cmds.is_empty() {
            match rx.recv_timeout(Duration::from_millis(60)) {
                Ok(c) => cmds.push(c),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
        }
        while let Ok(c) = rx.try_recv() {
            cmds.push(c);
        }
        for c in cmds {
            match c {
                ThumbCmd::Quit => return,
                ThumbCmd::Load(url, headers) => {
                    gen = THUMB_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed).wrapping_add(1);
                    loaded = false;
                    hovered = false;
                    grid_filled = false;
                    dur = 0.0;
                    want = None;
                    grid.clear();
                    tries.clear();
                    for (_, p) in made.iter() {
                        let _ = std::fs::remove_file(p);
                    }
                    made.clear();
                    // Same escaping as the main player: a comma inside a value would split the header list.
                    let _ = m.set("http-header-fields", &crate::mpv::header_list(&headers));
                    if let Err(e) = m.cmd(&["loadfile", &url]) {
                        crate::debug::log("thumbs", &format!("loadfile failed: {}", e));
                    }
                }
                // Only the last hovered time matters: earlier ones are dropped, a known slot answers at once.
                ThumbCmd::Seek(t) => {
                    if !hovered {
                        hovered = true;
                        crate::debug::log("thumbs", &format!("first request at {:.0}s (file loaded: {})", t, loaded));
                    }
                    let k = slot_of(t, slot_len);
                    match made.get(&k) {
                        Some(p) => {
                            send(&msg, Msg::Thumb(k as f64 * slot_len, p.clone()));
                            want = None;
                        }
                        None => want = Some(t),
                    }
                }
            }
        }
        for ev in m.events() {
            match ev {
                Evt::FileLoaded => {
                    loaded = true;
                    dur = m.get_f64("duration").unwrap_or(0.0);
                    // About 300 slots over the video, between 2 and 10 seconds each.
                    slot_len = if dur > 0.0 { (dur / 300.0).clamp(2.0, 10.0) } else { 5.0 };
                    crate::debug::log("thumbs", &format!("the preview player opened the video ({:.0}s, slot {:.1}s)", dur, slot_len));
                }
                Evt::EndFile { reason: 4, error } => {
                    loaded = false;
                    crate::debug::log("thumbs", &format!("the preview player could not open the video: {}", error));
                }
                _ => {}
            }
        }
        if !loaded {
            // A request made before the file opened stays in `want` and is served as soon as it opens.
            continue;
        }
        if hovered && !grid_filled && dur > 0.0 {
            grid_filled = true;
            // Coarse to fine: the 8 eighths first, then the halves between them, down to 120 points.
            const N: usize = 120;
            let mut seen = vec![false; N + 1];
            for stride in [64usize, 32, 16, 8, 4, 2, 1] {
                for i in (0..=N).step_by(stride) {
                    if !seen[i] {
                        seen[i] = true;
                        grid.push_back(dur * i as f64 / N as f64);
                    }
                }
            }
        }
        let asked = want.is_some();
        let t = match want.take() {
            Some(t) => t,
            None if hovered => match grid.pop_front() {
                Some(t) => t,
                None => continue,
            },
            None => continue,
        };
        let k = slot_of(t, slot_len);
        if made.contains_key(&k) {
            continue;
        }
        let at = (k as f64 * slot_len).min((dur - 0.5).max(0.0));
        let _ = m.cmd(&["seek", &format!("{:.2}", at), "absolute+keyframes"]);
        // A part of the video not downloaded yet (a torrent streams only around what was watched, a slow server)
        // takes longer than one already in the cache: the point the viewer points at is waited for up to 20 s, as
        // long as the pointer stays on it; a point of the background grid gets 4 s and is tried again later.
        let limit = Duration::from_millis(if asked { 20_000 } else { 4_000 });
        let started = Instant::now();
        let mut restarted = false;
        let mut moved_on = false;
        while started.elapsed() < limit && !restarted && !moved_on {
            for ev in m.events() {
                if let Evt::PlaybackRestart = ev {
                    restarted = true;
                }
            }
            while let Ok(c) = rx.try_recv() {
                match c {
                    // the pointer moved to another slot: that one now (a slot already made answers at once)
                    ThumbCmd::Seek(t2) if slot_of(t2, slot_len) != k => {
                        let k2 = slot_of(t2, slot_len);
                        if let Some(p) = made.get(&k2) {
                            send(&msg, Msg::Thumb(k2 as f64 * slot_len, p.clone()));
                        } else if asked {
                            want = Some(t2);
                            moved_on = true;
                        } else {
                            // the grid gives way to what the viewer points at
                            want = Some(t2);
                            moved_on = true;
                        }
                    }
                    ThumbCmd::Seek(_) => {}
                    other => {
                        pending.push(other);
                        moved_on = true;
                    }
                }
            }
            if !restarted && !moved_on {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        if !restarted {
            // No new frame: a picture now would be the old frame filed under the wrong time. Ask again later.
            if !moved_on {
                crate::debug::log("thumbs", &format!("seek to {:.0}s gave no frame in {} s", at, limit.as_secs()));
            }
            if !asked {
                let n = tries.entry(k).or_insert(0);
                *n += 1;
                if *n < 3 {
                    grid.push_back(t);
                }
            }
            continue;
        }
        let path = out_dir.join(format!("g{}_s{}.jpg", gen, k));
        match m.cmd(&["screenshot-to-file", &path.to_string_lossy(), "video"]) {
            Ok(()) if path.exists() => {
                if made.is_empty() {
                    crate::debug::log("thumbs", &format!("first preview picture made at {:.0}s: {}", at, path.display()));
                }
                made.insert(k, path.clone());
                send(&msg, Msg::Thumb(k as f64 * slot_len, path));
            }
            Ok(()) => crate::debug::log("thumbs", &format!("picture at {:.0}s: mpv said ok but wrote no file ({})", at, path.display())),
            Err(e) => crate::debug::log("thumbs", &format!("picture at {:.0}s failed: {}", at, e)),
        }
    }
}


/// A downloaded picture on disk: as WebP (pages and covers take far less space), as it came when it cannot be converted.
pub(crate) fn keep_picture(file: &Path, bytes: &[u8]) {
    #[cfg(feature = "webp")]
    {
        crate::webpstore::save(file, bytes);
    }
    #[cfg(not(feature = "webp"))]
    {
        if let Some(p) = file.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let _ = std::fs::write(file, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_ids() {
        assert_eq!(parse_episode_id("tt0944947:2:5"), (Some(2), Some(5)));
        assert_eq!(parse_episode_id("kitsu:42:7"), (None, Some(7)));
        assert_eq!(parse_episode_id("tt123"), (None, None));
    }
}

/// A home row as kept on disk (src/readcache.rs, "home" store).
fn row_to_json(r: &Row) -> crate::json::Json {
    use crate::json::Json;
    Json::obj(vec![
        ("title", r.title.clone().into()),
        ("addon", r.addon.clone().into()),
        ("kind", r.kind.clone().into()),
        ("items", Json::Arr(r.items.iter().map(|m| m.to_json()).collect())),
    ])
}

fn row_from_json(j: &crate::json::Json) -> Option<Row> {
    let items: Vec<MetaItem> = j.get("items").items().iter().map(MetaItem::from_json).filter(|m| !m.id.is_empty()).collect();
    if items.is_empty() {
        return None;
    }
    Some(Row { title: j.get("title").str(), addon: j.get("addon").str(), kind: j.get("kind").str(), items })
}

impl Core {
    /// The language of the interface, 2 letters ("pt", "en"…): the one picked under "UI language", else the main
    /// language of "Languages I know", else the system's. See `General::ui_lang`.
    pub fn ui_lang(&self) -> String {
        self.settings.general.ui_lang(&self.system_lang)
    }
}

/// The IMDb id of an episode ("tt123:2:4") to ask subtitle addons with, when the id being played is another
/// catalog's ("kitsu:…", "mal:…") and the metadata knows the IMDb id. None when the id already is an IMDb one.
pub fn imdb_episode_id(sid: &str, imdb: &str, kind: &str, season: Option<i64>, episode: Option<i64>) -> Option<String> {
    let imdb = imdb.split(':').next().unwrap_or("").trim();
    if sid.starts_with("tt") || !imdb.starts_with("tt") {
        return None;
    }
    if kind == "movie" {
        return Some(imdb.to_string());
    }
    Some(format!("{}:{}:{}", imdb, season.unwrap_or(1), episode?))
}

#[cfg(test)]
mod imdb_sid_tests {
    use super::imdb_episode_id;

    #[test]
    fn anime_ids_get_the_imdb_episode() {
        assert_eq!(imdb_episode_id("kitsu:46474:4", "tt30217403", "series", Some(2), Some(4)).as_deref(), Some("tt30217403:2:4"));
        assert_eq!(imdb_episode_id("tt30217403:2:4", "tt30217403", "series", Some(2), Some(4)), None);
        assert_eq!(imdb_episode_id("kitsu:1", "", "series", Some(1), Some(1)), None);
        assert_eq!(imdb_episode_id("mal:1", "tt1", "movie", None, None).as_deref(), Some("tt1"));
    }
}
