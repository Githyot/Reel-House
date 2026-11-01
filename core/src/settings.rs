use crate::addons::InstalledAddon;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct General {
    /// Empty means the mpv that ships inside the app.
    pub mpv_path: String,
    pub hwdec: String,
    pub buffer_seconds: u32,
    /// "ask" shows the stream list first, "best" plays the top match.
    pub auto_pick: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Accounts {
    pub real_debrid_token: String,
    pub tmdb_api_key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SubtitleSettings {
    pub enabled: bool,
    /// Empty means the system language.
    pub target_lang: String,
    pub preferred_source_langs: Vec<String>,
    /// "libretranslate", "anthropic" or "none".
    pub provider: String,
    pub libretranslate_url: String,
    pub libretranslate_api_key: String,
    pub anthropic_api_key: String,
    pub anthropic_model: String,
    pub max_wait_seconds: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Upscaling {
    pub enabled: bool,
    pub mode: String,
    pub quality: String,
    pub clamp_highlights: bool,
    pub max_source_height: u32,
    pub gpu_hq_profile: bool,
    pub scaler: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Settings {
    pub general: General,
    pub accounts: Accounts,
    pub subtitles: SubtitleSettings,
    pub upscaling: Upscaling,
    pub addons: Vec<InstalledAddon>,
    pub first_run_done: bool,
}

impl Default for General {
    fn default() -> Self {
        Self { mpv_path: String::new(), hwdec: "auto-safe".into(), buffer_seconds: 15, auto_pick: "ask".into() }
    }
}

impl Default for SubtitleSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            target_lang: String::new(),
            preferred_source_langs: vec!["en".into()],
            provider: "libretranslate".into(),
            libretranslate_url: "http://localhost:5000".into(),
            libretranslate_api_key: String::new(),
            anthropic_api_key: String::new(),
            anthropic_model: "claude-haiku-5-5".into(),
            max_wait_seconds: 90,
        }
    }
}

impl Default for Upscaling {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "A".into(),
            quality: "HQ".into(),
            clamp_highlights: true,
            max_source_height: 1080,
            gpu_hq_profile: true,
            scaler: "ewa_lanczossharp".into(),
        }
    }
}

/// Merges `patch` into `base`: objects merge key by key, everything else is replaced.
pub fn merge_json(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                merge_json(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

pub struct SettingsStore {
    path: PathBuf,
    inner: RwLock<Settings>,
}

impl SettingsStore {
    pub fn load(dir: &Path) -> Arc<Self> {
        let path = dir.join("settings.json");
        let settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Arc::new(Self { path, inner: RwLock::new(settings) })
    }

    pub fn get(&self) -> Settings {
        self.inner.read().unwrap().clone()
    }

    pub fn set(&self, s: Settings) -> Result<Settings> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(&s)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // The file can hold API tokens, so keep it private to the user.
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        *self.inner.write().unwrap() = s.clone();
        Ok(s)
    }

    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings> {
        let mut s = self.get();
        f(&mut s);
        self.set(s)
    }

    /// Applies a partial JSON object from the UI, for example `{"upscaling":{"mode":"C"}}`.
    pub fn patch(&self, patch: Value) -> Result<Settings> {
        let mut v = serde_json::to_value(self.get())?;
        merge_json(&mut v, &patch);
        // Keep the addon list under the control of the addon manager.
        if let Some(p) = patch.as_object() {
            if p.contains_key("addons") {
                v["addons"] = serde_json::to_value(self.get().addons)?;
            }
        }
        self.set(serde_json::from_value(v)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_merges_nested_values_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        store.patch(serde_json::json!({"upscaling": {"mode": "C+A"}, "accounts": {"real_debrid_token": "abc"}})).unwrap();
        let s = store.get();
        assert_eq!(s.upscaling.mode, "C+A");
        assert_eq!(s.upscaling.quality, "HQ");
        assert_eq!(s.accounts.real_debrid_token, "abc");
        assert_eq!(SettingsStore::load(dir.path()).get().upscaling.mode, "C+A");
    }

    #[test]
    fn defaults_ask_which_stream_and_leave_upscaling_off() {
        let s = Settings::default();
        assert_eq!(s.general.auto_pick, "ask");
        assert!(!s.upscaling.enabled);
        assert!(s.addons.is_empty());
    }

    #[test]
    fn unknown_or_missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"general":{"hwdec":"no"}}"#).unwrap();
        assert_eq!(s.general.hwdec, "no");
        assert_eq!(s.general.buffer_seconds, 15);
    }
}
