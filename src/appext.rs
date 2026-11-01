//! Extensions of the app itself (not the reading sources): small JavaScript modules in `<data>/extensions/<id>/`,
//! run by the same QuickJS engine as the Text mode plugins (fetch, cheerio, dayjs… are there).
//!
//! `manifest.json`: `{"id", "name", "version", "description", "main": "main.js", "hooks": ["onPlay", …]}`.
//! `main.js` is a CommonJS module that exports one function per hook. Each gets one object (what happened) and may
//! return (or resolve to) `{"toast": "text"}` and/or `{"openUrl": "https://…"}`.
use crate::json::Json;
use std::path::{Path, PathBuf};

/// The hooks the app fires, with what each receives (the fields sent, as `fire_hook` is called for each).
/// `season` / `episode` are null for a film; `position` / `duration` are seconds; `ease` is 1 (Again) to 4 (Easy);
/// `autocard` is true when the clip was made by Autocards (absent for a clip saved from the player).
pub const HOOKS: &[(&str, &str)] = &[
    ("onStart", "{version}"),
    ("onTitleOpened", "{id, name, kind}"),
    ("onPlay", "{title, kind, id, season, episode}"),
    ("onPlayEnd", "{title, position, duration}"),
    ("onClipSaved", "{text, autocard?}"),
    ("onStudyAnswer", "{deck, ease}"),
];

#[derive(Clone, Debug, Default)]
pub struct AppExt {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub hooks: Vec<String>,
    pub dir: PathBuf,
    pub code: String,
    /// why it could not be loaded (then it never runs)
    pub error: String,
}

pub fn dir(data: &Path) -> PathBuf {
    data.join("extensions")
}

fn safe_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Every folder with a manifest, sorted by name. Broken ones are listed with their error.
pub fn scan(data: &Path) -> Vec<AppExt> {
    let root = dir(data);
    let _ = std::fs::create_dir_all(&root);
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&root) else { return out };
    for e in rd.flatten() {
        let d = e.path();
        if !d.is_dir() {
            continue;
        }
        let folder = d.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        let mut x = AppExt { id: folder.clone(), name: folder.clone(), dir: d.clone(), ..AppExt::default() };
        let m = match std::fs::read_to_string(d.join("manifest.json")).map_err(|e| e.to_string()).and_then(|t| Json::parse(&t)) {
            Ok(m) => m,
            Err(err) => {
                x.error = format!("manifest.json: {}", err);
                out.push(x);
                continue;
            }
        };
        let id = m.get("id").str();
        if !id.is_empty() {
            x.id = id;
        }
        if !safe_id(&x.id) {
            x.error = "the id may only have letters, digits, '-', '_' and '.'".into();
        }
        x.name = if m.get("name").str().is_empty() { x.id.clone() } else { m.get("name").str() };
        x.version = m.get("version").str();
        x.description = m.get("description").str();
        x.hooks = m.get("hooks").items().iter().map(|h| h.str()).filter(|h| HOOKS.iter().any(|(k, _)| k == h)).collect();
        let main = if m.get("main").str().is_empty() { "main.js".to_string() } else { m.get("main").str() };
        if !crate::util::safe_media_name(&main) {
            x.error = "main must be a file inside the extension folder".into();
        } else if x.error.is_empty() {
            match std::fs::read_to_string(d.join(&main)) {
                Ok(code) => x.code = code,
                Err(e) => x.error = format!("{}: {}", main, e),
            }
        }
        out.push(x);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// What a hook asked the app to do.
#[derive(Debug, Default, PartialEq)]
pub struct Reply {
    pub toast: Option<String>,
    pub open_url: Option<String>,
}

pub fn reply(j: &Json) -> Reply {
    let t = j.get("toast").str();
    let u = j.get("openUrl").str();
    Reply {
        toast: if t.is_empty() { None } else { Some(t.chars().take(300).collect()) },
        open_url: if u.starts_with("https://") || u.starts_with("http://") { Some(u) } else { None },
    }
}
