//! Android and iOS: the local AI (Ollama) is a desktop-only feature, so this stands in for `ollama_local.rs` with
//! the same names. Nothing is found, downloaded or started; every status says why and the gate never opens. The
//! controller never asks for it there (`platform::DESKTOP_AI`), so this only keeps the shared code compiling.
use crate::http::Http;
use crate::json::Json;
use std::path::Path;
use std::time::Duration;

/// Why the local AI is missing on this system.
pub fn unavailable() -> String {
    format!("The local AI (Ollama) is not available on {}: it runs on Windows, Linux and macOS.", crate::platform::pretty_os())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub phase: String,
    pub message: String,
    pub model: String,
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
            ("canDownload", Json::Bool(false)),
            ("available", Json::Bool(false)),
        ])
    }
}

/// The handle kept by the core: never ready on a phone or tablet.
#[derive(Clone, Default)]
pub struct Local;

impl Local {
    pub fn status(&self) -> Status {
        Status { phase: "error".into(), message: unavailable(), model: String::new() }
    }

    /// Never starts a setup here.
    pub fn begin(&self, _model: &str) -> bool {
        false
    }

    pub fn wait_ready(&self, _model: &str, _timeout: Duration) -> Result<(), String> {
        Err(unavailable())
    }

    pub fn run(&self, _http: &dyn Http, _data_dir: &Path, _notify: &dyn Fn()) {}

    pub fn stop(&self) {}
}
