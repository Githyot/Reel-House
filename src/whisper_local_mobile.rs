//! Android and iOS: Whisper on this computer (`whisper_local.rs`) is desktop only, like the local AI, so this stands
//! in for it with the same names. Nothing is found, downloaded or started; the voice plan never picks it there
//! (`voice::stt_plan` with `local_ok` false), so this only keeps the shared code compiling.
use crate::http::Http;
use crate::json::Json;
use crate::voice::WhisperModel;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn unavailable() -> String {
    format!("Whisper on this device is not available on {}: it runs on Windows, Linux and macOS.", crate::platform::pretty_os())
}

/// The same fields as on desktop (never filled here).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub phase: String,
    pub message: String,
    pub done: u64,
    pub total: u64,
    pub model: String,
    pub busy: bool,
    pub ours: bool,
    pub root: String,
    pub vad: bool,
    pub program: String,
}

pub fn health(_http: &dyn Http, _root: &str) -> Option<bool> {
    None
}

impl Status {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("phase", Json::Str("error".into())),
            ("message", Json::Str(unavailable())),
            ("done", Json::Num(0.0)),
            ("total", Json::Num(0.0)),
            ("percent", Json::Num(0.0)),
            ("model", Json::Str(String::new())),
            ("busy", Json::Bool(false)),
            ("ready", Json::Bool(false)),
            ("ours", Json::Bool(false)),
            ("vad", Json::Bool(false)),
            ("program", Json::Str(String::new())),
            ("canDownload", Json::Bool(false)),
            ("available", Json::Bool(false)),
        ])
    }
}

pub fn home(data_dir: &Path) -> PathBuf {
    data_dir.join("whisper")
}

pub fn installed(_data_dir: &Path, _m: &WhisperModel) -> bool {
    false
}

/// The handle kept by the core: never ready on a phone or tablet.
#[derive(Clone, Default)]
pub struct Local;

impl Local {
    pub fn status(&self) -> Status {
        Status { phase: "error".into(), message: unavailable(), ..Status::default() }
    }
    pub fn begin(&self, _id: &str) -> bool {
        false
    }
    pub fn invalidate(&self) {}
    pub fn wait_ready(&self, _id: &str, _timeout: Duration) -> Result<(String, bool), String> {
        Err(unavailable())
    }
    pub fn run(&self, _http: &dyn Http, _data_dir: &Path, _notify: &dyn Fn()) {}
    pub fn stop(&self) {}
}
