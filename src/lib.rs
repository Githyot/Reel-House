//! Reelhouse core. Everything here uses only the standard library, so it builds and tests anywhere.
//! The window, the mpv bridge and networking live in the modules gated behind the `app` feature.
pub mod addons;
pub mod addon_kind;
pub mod adblock;
pub mod browser;
pub mod anilist;
pub mod animeskip;
pub mod anime4k;
pub mod audiosync;
pub mod artwork;
pub mod backup;
pub mod cloud;
pub mod connectivity;
pub mod debrid;
pub mod debug;
pub mod downloads;
pub mod epub;
pub mod extensions;
pub mod gzip;
pub mod http;
pub mod i18n;
pub mod json;
pub mod jlab;
pub mod kanji;
pub mod karaoke;
pub mod knowledge;
pub mod learn;
pub mod mangaup;
pub mod mascot_lines;
pub mod ocr;
pub mod mal;
pub mod tmdb;
pub mod malmatch;
pub mod manga;
pub mod novels;
pub mod readcache;
pub mod readstudy;
pub mod jsrt;
pub mod appext;
pub mod pagedb;
#[cfg(feature = "webp")]
pub mod webpstore;
pub mod exthost;
pub mod suwayomi;
pub mod suwayomi_host;
pub mod model;
pub mod orbs;
pub mod outline;
pub mod palette;
pub mod platform;
/// IP protection ("VPN inside the app"): proxy, WireGuard and Tor through a local gateway.
#[cfg(feature = "privacy")]
pub mod privacy;
#[cfg(not(feature = "privacy"))]
#[path = "privacy_off.rs"]
pub mod privacy;
pub mod profiles;
pub mod richtext;
pub mod settings;
pub mod sitepass;
pub mod sqlite;
pub mod study;
pub mod skipdb;
pub mod streamlang;
pub mod streams;
pub mod streammemory;
pub mod subsync;
pub mod theme;
pub mod substyle;
pub mod subtitles;
pub mod torrent;
#[cfg(feature = "torrent-embed")]
pub mod torrent_embed;
pub mod translate;
pub mod updater;
pub mod uisounds;
pub mod util;

// The program itself. Needs the `app` feature (libloading, ureq).
#[cfg(feature = "app")]
pub mod controller;
#[cfg(feature = "app")]
pub mod mpv;
#[cfg(feature = "app")]
pub mod mkvsubs;
#[cfg(feature = "app")]
pub mod net;
#[cfg(feature = "app")]
pub mod ankiweb;
#[cfg(feature = "app")]
pub mod autocards;
#[cfg(feature = "app")]
pub mod themesong;
#[cfg(feature = "app")]
pub mod xray;
pub mod xray_trivia;
#[cfg(feature = "app")]
pub mod llm;
// Study > Conversation (kaiwa) and the local AI (Ollama): desktop only. Android and iOS get a stand-in with the same
// names that never starts anything (see platform::DESKTOP_AI).
#[cfg(all(feature = "app", not(any(target_os = "android", target_os = "ios"))))]
pub mod ollama_local;
#[cfg(all(feature = "app", any(target_os = "android", target_os = "ios")))]
#[path = "ollama_local_mobile.rs"]
pub mod ollama_local;
#[cfg(all(feature = "app", not(any(target_os = "android", target_os = "ios"))))]
pub mod kaiwa;
// The voice of the Study mode: which engine hears and speaks (voice.rs), the microphone's detector (vad.rs) and
// Whisper on this computer (whisper_local.rs; desktop only, a stand-in on Android and iOS like ollama_local).
pub mod vad;
#[cfg(feature = "app")]
pub mod voice;
#[cfg(all(feature = "app", not(any(target_os = "android", target_os = "ios"))))]
pub mod whisper_local;
#[cfg(all(feature = "app", any(target_os = "android", target_os = "ios")))]
#[path = "whisper_local_mobile.rs"]
pub mod whisper_local;
// Conversation by voice: the Gemini Live client (a WebSocket worker and the sound hub the Qt side feeds) and the
// Kaiwa layer that steers it (scene, level, goals, corrections). Desktop only, like the rest of Conversation.
#[cfg(all(feature = "app", not(any(target_os = "android", target_os = "ios"))))]
pub mod live;
#[cfg(all(feature = "app", not(any(target_os = "android", target_os = "ios"))))]
pub mod kaiwa_live;
#[cfg(feature = "app")]
pub mod runtime;
