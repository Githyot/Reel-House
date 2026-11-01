//! Where the program runs, and what that place allows. The same core serves Windows, Linux, macOS, Android and
//! iOS; the differences are collected here so the rest of the code asks one question ("may I start a helper
//! program?") instead of testing the operating system in many places.
//!
//! `reelhouse_mobile` is a cfg that build.rs sets for the Android and iOS targets and for the `mobile` cargo feature.
//! A phone or tablet cannot start the rqbit torrent engine, the Suwayomi server (Java), the Real-ESRGAN upscaler,
//! yt-dlp or a local Ollama: on those systems the functions that would answer with [`local_processes`]'s error, and
//! the interface reads the same facts through [`caps`] (`Platform` in QML) to hide the buttons.

use crate::json::Json;

/// True on Android and iOS (and when the `mobile` feature is on).
pub const MOBILE: bool = cfg!(reelhouse_mobile);
pub const ANDROID: bool = cfg!(target_os = "android");
pub const IOS: bool = cfg!(target_os = "ios");
/// Study > Conversation (the tutor) and the local AI (Ollama): Windows, Linux and macOS only. Android and iOS build
/// stand-ins (ollama_local_mobile.rs, controller_kaiwa_mobile.rs) and the "ollama" translator falls back to the cloud.
pub const DESKTOP_AI: bool = cfg!(not(any(target_os = "android", target_os = "ios")));

/// "windows", "linux", "macos", "android", "ios" or the target's own name.
pub fn os() -> &'static str {
    std::env::consts::OS
}

/// Whether this build may start helper programs (the torrent engine, Suwayomi, the upscaler, yt-dlp, Ollama).
pub fn local_processes_allowed() -> bool {
    !MOBILE
}

/// Ok on desktop; on a phone or tablet, the message the user sees instead of a failed start.
pub fn local_processes() -> Result<(), String> {
    needs_program("This feature needs a program that cannot run on phones and tablets.")
}

/// Ok on desktop; on a phone or tablet, `why`: what the feature needs and what to do instead (each feature says it
/// its own way, so the user is not told about Real-Debrid on a screen that has nothing to do with it).
pub fn needs_program(why: &str) -> Result<(), String> {
    if local_processes_allowed() {
        Ok(())
    } else {
        Err(why.to_string())
    }
}

/// The name shown in messages.
pub fn pretty_os() -> &'static str {
    match os() {
        "windows" => "Windows",
        "linux" => "Linux",
        "macos" => "macOS",
        "android" => "Android",
        "ios" => "iOS",
        other => other,
    }
}

/// What the interface needs to know, as JSON (`Platform` in QML reads it through the bridge):
/// `{os, mobile, android, ios, desktop, localProcesses, torrentEngine, suwayomiLocal, upscaler, themeSongs,
///   localAi, conversation, browser, fileDialogs, windowFrame}`.
pub fn caps() -> Json {
    let local = local_processes_allowed();
    Json::obj(vec![
        ("os", Json::Str(os().to_string())),
        ("mobile", Json::Bool(MOBILE)),
        ("android", Json::Bool(ANDROID)),
        ("ios", Json::Bool(IOS)),
        ("desktop", Json::Bool(!MOBILE)),
        // helper programs started by the core
        ("localProcesses", Json::Bool(local)),
        // the rqbit program on desktop, the engine inside the app on Android (feature `torrent-embed`)
        ("torrentEngine", Json::Bool(local || cfg!(feature = "torrent-embed"))),
        ("suwayomiLocal", Json::Bool(local)),
        ("upscaler", Json::Bool(local)),
        ("themeSongs", Json::Bool(local)),
        ("localAi", Json::Bool(local && DESKTOP_AI)),
        ("conversation", Json::Bool(DESKTOP_AI)),
        // Chromium (CEF) for Research mode: desktop only; the phone opens pages in the system browser
        ("browser", Json::Bool(!MOBILE)),
        // native open / save dialogs with a path the core can read
        ("fileDialogs", Json::Bool(!MOBILE)),
        // the app draws its own window frame (title bar buttons) on desktop; the phone has none
        ("windowFrame", Json::Bool(!MOBILE)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_follow_the_build() {
        let c = caps();
        assert_eq!(c.get("mobile").as_bool(), Some(MOBILE));
        assert_eq!(c.get("localProcesses").as_bool(), Some(!MOBILE));
        assert_eq!(c.get("os").as_str(), Some(std::env::consts::OS));
        if MOBILE {
            assert!(local_processes().is_err());
        } else {
            assert!(local_processes().is_ok());
        }
    }
}
