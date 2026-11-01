//! A copy of the backup online: Google Drive, OneDrive, iCloud Drive or GitHub.
//!
//! Two ways to reach a cloud:
//! - **Sync folder**: the cloud's desktop program (OneDrive, Google Drive for desktop, iCloud for Windows) keeps a
//!   folder in step with the cloud. The backup is copied into `<that folder>/Reelhouse/` and the program uploads it.
//!   Nothing to sign in to here. iCloud Drive only works this way: Apple offers no public iCloud Drive API that a
//!   Windows program could call.
//! - **Account**: OAuth 2.0 "device code" sign-in (a short code typed at microsoft.com/devicelogin or
//!   google.com/device), then the files are uploaded with the Microsoft Graph / Google Drive APIs. Reelhouse has no
//!   registered app of its own, so the user pastes the client ID of a free app they register (the window says how).
//!   OneDrive keeps the files in the app's own folder (`Apps/<app name>`, scope Files.ReadWrite.AppFolder); Google
//!   Drive in a "Reelhouse" folder the app created (scope drive.file: the app sees only its own files).
//! - **GitHub**: a personal access token and a private repository, each backup a release (`cloud_github.rs`).
use crate::http::{Http, Request};
use crate::json::Json;
use crate::util::form_encode;
use std::path::{Path, PathBuf};

#[path = "cloud_github.rs"]
pub mod github;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Google,
    OneDrive,
    ICloud,
    GitHub,
}

impl Provider {
    pub fn parse(s: &str) -> Option<Provider> {
        match s {
            "gdrive" | "google" => Some(Provider::Google),
            "onedrive" => Some(Provider::OneDrive),
            "icloud" => Some(Provider::ICloud),
            "github" => Some(Provider::GitHub),
            _ => None,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Provider::Google => "gdrive",
            Provider::OneDrive => "onedrive",
            Provider::ICloud => "icloud",
            Provider::GitHub => "github",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Provider::Google => "Google Drive",
            Provider::OneDrive => "OneDrive",
            Provider::ICloud => "iCloud Drive",
            Provider::GitHub => "GitHub",
        }
    }
    /// iCloud Drive is reached only through its sync folder.
    pub fn has_account(self) -> bool {
        self != Provider::ICloud
    }
    /// GitHub has no sync folder: only the token.
    pub fn has_folder(self) -> bool {
        self != Provider::GitHub
    }
}

/// The folder made inside the sync folder (and on Google Drive).
pub const FOLDER: &str = "Reelhouse";

/// GitHub is reached through `github` (a token and a repository), not the device sign-in and the drive APIs here.
const GITHUB_BY_TOKEN: &str = "GitHub is reached with a token: paste it in the Backup settings.";

// ----- settings (kept in settings.json, under "backup" → "cloud") -----

#[derive(Clone, Debug, PartialEq)]
pub struct CloudSettings {
    /// Keep a copy of every backup online.
    pub enabled: bool,
    /// "gdrive", "onedrive", "icloud" or "github"
    pub provider: String,
    /// "folder" (the sync folder of the desktop program) or "account" (sign in with a code)
    pub method: String,
    /// The sync folder chosen by hand; empty means the one found on this computer.
    pub folder: String,
    /// How many copies are kept online (the oldest are deleted).
    pub keep: u32,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub google_refresh: String,
    pub google_account: String,
    pub ms_client_id: String,
    pub ms_refresh: String,
    pub ms_account: String,
    /// GitHub: the personal access token, the repository ("owner/name", or a name the token's account owns) and the
    /// account's login.
    pub github_token: String,
    pub github_repo: String,
    pub github_account: String,
    /// The last upload: when, its file name and size, and the error when it failed.
    pub last_upload: u64,
    pub last_name: String,
    pub last_size: u64,
    pub last_error: String,
}

impl Default for CloudSettings {
    fn default() -> Self {
        CloudSettings {
            enabled: false,
            provider: "gdrive".into(),
            method: "folder".into(),
            folder: String::new(),
            keep: 5,
            google_client_id: String::new(),
            google_client_secret: String::new(),
            google_refresh: String::new(),
            google_account: String::new(),
            ms_client_id: String::new(),
            ms_refresh: String::new(),
            ms_account: String::new(),
            github_token: String::new(),
            github_repo: String::new(),
            github_account: String::new(),
            last_upload: 0,
            last_name: String::new(),
            last_size: 0,
            last_error: String::new(),
        }
    }
}

impl CloudSettings {
    pub fn from_json(j: &Json) -> CloudSettings {
        let d = CloudSettings::default();
        let s = |k: &str, def: &str| match j.get(k) {
            Json::Str(v) => v.clone(),
            _ => def.to_string(),
        };
        let n = |k: &str, def: f64| j.get(k).as_f64().unwrap_or(def);
        let provider = Provider::parse(&s("provider", &d.provider)).unwrap_or(Provider::Google);
        let method = s("method", &d.method);
        CloudSettings {
            enabled: j.get("enabled").as_bool().unwrap_or(false),
            provider: provider.id().to_string(),
            // iCloud has only the sync folder, GitHub only the token
            method: if !provider.has_folder() || (method == "account" && provider.has_account()) { "account".into() } else { "folder".into() },
            folder: s("folder", ""),
            keep: (n("keep", 5.0) as u32).clamp(1, 50),
            google_client_id: s("google_client_id", "").trim().to_string(),
            google_client_secret: s("google_client_secret", "").trim().to_string(),
            google_refresh: s("google_refresh", ""),
            google_account: s("google_account", ""),
            ms_client_id: s("ms_client_id", "").trim().to_string(),
            ms_refresh: s("ms_refresh", ""),
            ms_account: s("ms_account", ""),
            github_token: s("github_token", "").trim().to_string(),
            github_repo: s("github_repo", "").trim().to_string(),
            github_account: s("github_account", ""),
            last_upload: n("last_upload", 0.0) as u64,
            last_name: s("last_name", ""),
            last_size: n("last_size", 0.0) as u64,
            last_error: s("last_error", ""),
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("enabled", self.enabled.into()),
            ("provider", self.provider.clone().into()),
            ("method", self.method.clone().into()),
            ("folder", self.folder.clone().into()),
            ("keep", self.keep.into()),
            ("google_client_id", self.google_client_id.clone().into()),
            ("google_client_secret", self.google_client_secret.clone().into()),
            ("google_refresh", self.google_refresh.clone().into()),
            ("google_account", self.google_account.clone().into()),
            ("ms_client_id", self.ms_client_id.clone().into()),
            ("ms_refresh", self.ms_refresh.clone().into()),
            ("ms_account", self.ms_account.clone().into()),
            ("github_token", self.github_token.clone().into()),
            ("github_repo", self.github_repo.clone().into()),
            ("github_account", self.github_account.clone().into()),
            ("last_upload", (self.last_upload as f64).into()),
            ("last_name", self.last_name.clone().into()),
            ("last_size", (self.last_size as f64).into()),
            ("last_error", self.last_error.clone().into()),
        ])
    }

    pub fn provider(&self) -> Provider {
        Provider::parse(&self.provider).unwrap_or(Provider::Google)
    }

    pub fn by_account(&self) -> bool {
        self.method == "account" && self.provider().has_account()
    }

    /// The app (client ID, secret) and the saved sign-in of the provider.
    pub fn app(&self, p: Provider) -> OAuthApp {
        match p {
            Provider::OneDrive => OAuthApp { provider: p, client_id: self.ms_client_id.clone(), client_secret: String::new() },
            _ => OAuthApp { provider: p, client_id: self.google_client_id.clone(), client_secret: self.google_client_secret.clone() },
        }
    }

    pub fn refresh_token(&self, p: Provider) -> &str {
        match p {
            Provider::OneDrive => &self.ms_refresh,
            Provider::Google => &self.google_refresh,
            Provider::ICloud => "",
            // the token itself: it does not expire by refresh
            Provider::GitHub => &self.github_token,
        }
    }

    pub fn account(&self, p: Provider) -> &str {
        match p {
            Provider::OneDrive => &self.ms_account,
            Provider::Google => &self.google_account,
            Provider::ICloud => "",
            Provider::GitHub => &self.github_account,
        }
    }

    /// Saves a sign-in (or forgets it, with empty strings).
    pub fn set_login(&mut self, p: Provider, refresh: &str, account: &str) {
        match p {
            Provider::OneDrive => {
                self.ms_refresh = refresh.to_string();
                self.ms_account = account.to_string();
            }
            Provider::Google => {
                self.google_refresh = refresh.to_string();
                self.google_account = account.to_string();
            }
            Provider::GitHub => {
                self.github_token = refresh.to_string();
                self.github_account = account.to_string();
            }
            Provider::ICloud => {}
        }
    }
}

// ----- sync folders of the desktop programs -----

/// Where the desktop program of `p` keeps its folder, most likely first. `env` reads an environment variable and `os`
/// is "windows", "macos" or "linux" (both given so the tests can pretend to be any computer).
pub fn sync_folder_candidates(p: Provider, env: &dyn Fn(&str) -> Option<String>, os: &str) -> Vec<PathBuf> {
    let home = if os == "windows" { env("USERPROFILE").or_else(|| env("HOME")) } else { env("HOME") }.filter(|h| !h.is_empty()).map(PathBuf::from);
    let mut v: Vec<PathBuf> = Vec::new();
    // macOS puts the File Provider folders of Google Drive and OneDrive in ~/Library/CloudStorage/<Name>-<account>
    let cloud_storage = |prefix: &str, inner: &[&str]| -> Vec<PathBuf> {
        let Some(h) = &home else { return vec![] };
        let Ok(rd) = std::fs::read_dir(h.join("Library").join("CloudStorage")) else { return vec![] };
        let mut out = Vec::new();
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(prefix) {
                if inner.is_empty() {
                    out.push(e.path());
                }
                for i in inner {
                    out.push(e.path().join(i));
                }
            }
        }
        out.sort();
        out
    };
    // "My Drive" in the languages people here use (Google names the folder in the account's language)
    const MY_DRIVE: &[&str] = &["My Drive", "Meu Drive", "Mi unidad", "Mon Drive", "Meine Ablage", "Il mio Drive"];
    match p {
        Provider::OneDrive => {
            for k in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
                if let Some(x) = env(k).filter(|x| !x.is_empty()) {
                    v.push(PathBuf::from(x));
                }
            }
            if let Some(h) = &home {
                v.push(h.join("OneDrive"));
            }
            if os == "macos" {
                v.extend(cloud_storage("OneDrive", &[]));
            }
        }
        Provider::Google => {
            if os == "windows" {
                // Google Drive for desktop shows the cloud as a drive letter (G: by default) holding "My Drive"
                for letter in 'D'..='Z' {
                    for m in MY_DRIVE {
                        v.push(PathBuf::from(format!("{}:\\{}", letter, m)));
                    }
                }
            }
            if os == "macos" {
                v.extend(cloud_storage("GoogleDrive", MY_DRIVE));
            }
            if let Some(h) = &home {
                // "mirror" mode of Drive for desktop, and the old Backup and Sync
                for m in MY_DRIVE {
                    v.push(h.join(m));
                }
                v.push(h.join("Google Drive").join("My Drive"));
                v.push(h.join("Google Drive"));
                v.push(h.join("GoogleDrive"));
            }
        }
        Provider::ICloud => {
            if let Some(h) = &home {
                if os == "macos" {
                    v.push(h.join("Library").join("Mobile Documents").join("com~apple~CloudDocs"));
                }
                v.push(h.join("iCloudDrive"));
                v.push(h.join("iCloud Drive"));
            }
        }
        Provider::GitHub => {}
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for x in v {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

/// The sync folders of `p` that exist (see `sync_folder_candidates`).
pub fn detect_sync_folders(p: Provider, env: &dyn Fn(&str) -> Option<String>, os: &str) -> Vec<PathBuf> {
    // A drive letter is looked at once (its root), not once per "My Drive" name: an absent network drive can take
    // a while to answer.
    let mut roots: Vec<(String, bool)> = Vec::new();
    sync_folder_candidates(p, env, os)
        .into_iter()
        .filter(|d| {
            let s = d.to_string_lossy();
            let b = s.as_bytes();
            if b.len() > 3 && b[1] == b':' && b[2] == b'\\' {
                let root = s[..3].to_string();
                let ok = match roots.iter().find(|(r, _)| *r == root) {
                    Some((_, ok)) => *ok,
                    None => {
                        let ok = Path::new(&root).is_dir();
                        roots.push((root, ok));
                        ok
                    }
                };
                if !ok {
                    return false;
                }
            }
            d.is_dir()
        })
        .collect()
}

/// The same on this computer.
pub fn detect(p: Provider) -> Vec<PathBuf> {
    detect_sync_folders(p, &|k| std::env::var(k).ok(), std::env::consts::OS)
}

/// A backup kept online (a file in the sync folder, or on the cloud).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Remote {
    /// The file's path (sync folder) or the cloud's id for it.
    pub id: String,
    pub name: String,
    pub size: u64,
    /// Unix time it was last changed.
    pub modified: u64,
}

impl Remote {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![("id", self.id.clone().into()), ("name", self.name.clone().into()), ("size", (self.size as f64).into()), ("modified", (self.modified as f64).into())])
    }
}

/// Our backups first by name: the names hold the date, newest first.
fn sort_newest_first(v: &mut [Remote]) {
    v.sort_by(|a, b| b.name.cmp(&a.name).then(b.modified.cmp(&a.modified)));
}

fn ours(name: &str) -> bool {
    name.starts_with(crate::backup::PREFIX) && crate::backup::is_backup_name(name)
}

/// The backups in `<folder>/Reelhouse`.
pub fn folder_list(folder: &Path) -> Vec<Remote> {
    let Ok(rd) = std::fs::read_dir(folder.join(FOLDER)) else { return vec![] };
    let mut v: Vec<Remote> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let m = e.metadata().ok()?;
            (m.is_file() && ours(&name)).then(|| Remote {
                id: e.path().display().to_string(),
                name,
                size: m.len(),
                modified: m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0),
            })
        })
        .collect();
    sort_newest_first(&mut v);
    v
}

/// Copies a backup into `<folder>/Reelhouse/` (through a ".part" name, so the desktop program never uploads half a
/// file) and deletes the oldest copies there beyond `keep`.
pub fn folder_upload(src: &Path, folder: &Path, keep: usize) -> Result<Remote, String> {
    if !folder.is_dir() {
        return Err(format!("The sync folder {} was not found. Is the cloud's desktop program installed and signed in?", folder.display()));
    }
    let dir = folder.join(FOLDER);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {}", dir.display(), e))?;
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("The backup has no file name.")?;
    let part = dir.join(format!("{}.part", name));
    let dest = dir.join(&name);
    let size = std::fs::copy(src, &part).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("Could not copy the backup to {}: {}", dir.display(), e)
    })?;
    std::fs::rename(&part, &dest).map_err(|e| format!("Could not copy the backup to {}: {}", dir.display(), e))?;
    let list = folder_list(folder);
    for old in list.iter().skip(keep.max(1)) {
        let _ = std::fs::remove_file(&old.id);
    }
    Ok(Remote { id: dest.display().to_string(), name, size, modified: crate::util::now_unix() })
}

// ----- OAuth 2.0 device code sign-in -----

#[derive(Clone, Debug, PartialEq)]
pub struct OAuthApp {
    pub provider: Provider,
    pub client_id: String,
    /// Google asks for it even in the device flow ("TVs and Limited Input devices" client); Microsoft does not.
    pub client_secret: String,
}

const MS_DEVICE: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/devicecode";
const MS_TOKEN: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
const MS_SCOPE: &str = "Files.ReadWrite.AppFolder User.Read offline_access";
const GOOGLE_DEVICE: &str = "https://oauth2.googleapis.com/device/code";
const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const GRAPH: &str = "https://graph.microsoft.com/v1.0";
const DRIVE: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3";

/// Where the user registers the free app whose client ID is pasted here.
pub fn register_url(p: Provider) -> &'static str {
    match p {
        Provider::OneDrive => "https://entra.microsoft.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade",
        Provider::GitHub => github::TOKEN_URL,
        _ => "https://console.cloud.google.com/apis/credentials",
    }
}

/// The code to type on the provider's page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verify_url: String,
    /// Seconds the code stays valid, and between two checks.
    pub expires_in: u64,
    pub interval: u64,
}

/// The answer to the device code request (Microsoft says `verification_uri`, Google `verification_url`).
pub fn parse_device_code(status: u16, j: &Json) -> Result<DeviceCode, String> {
    if !(200..300).contains(&status) || !j.get("error").str().is_empty() {
        return Err(oauth_error(j, status));
    }
    let url = [j.get("verification_uri").str(), j.get("verification_url").str()].into_iter().find(|u| !u.is_empty()).unwrap_or_default();
    let d = DeviceCode {
        device_code: j.get("device_code").str(),
        user_code: j.get("user_code").str(),
        verify_url: url,
        expires_in: j.get("expires_in").as_f64().unwrap_or(900.0) as u64,
        interval: (j.get("interval").as_f64().unwrap_or(5.0) as u64).clamp(1, 60),
    };
    if d.device_code.is_empty() || d.user_code.is_empty() || d.verify_url.is_empty() {
        return Err("The sign-in service gave no code.".into());
    }
    Ok(d)
}

/// A readable reason from an OAuth error answer.
fn oauth_error(j: &Json, status: u16) -> String {
    let e = j.get("error");
    // Google's API errors are {"error": {"message": …}}, OAuth ones {"error": "invalid_client", "error_description": …}
    let code = if let Json::Obj(_) = e { e.get("status").str() } else { e.str() };
    let desc = [j.get("error_description").str(), e.get("message").str()].into_iter().find(|x| !x.is_empty()).unwrap_or_default();
    let first = desc.lines().next().unwrap_or("").to_string();
    match code.as_str() {
        "invalid_client" | "unauthorized_client" => format!("The client ID was refused ({}). Check it, and that the app allows this sign-in.", if first.is_empty() { code.clone() } else { first }),
        "invalid_grant" => "The sign-in expired or was revoked. Sign in again.".to_string(),
        "" if first.is_empty() => format!("The service answered {}.", status),
        _ if first.is_empty() => code,
        _ => first,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tokens {
    pub access: String,
    /// Empty when the answer kept the old one (Google does not send a new refresh token on a refresh).
    pub refresh: String,
    pub expires_in: u64,
}

/// One check of the device sign-in.
#[derive(Clone, Debug, PartialEq)]
pub enum Poll {
    /// The user has not typed the code yet.
    Pending,
    /// Checking too often: wait longer.
    SlowDown,
    Done(Tokens),
    Failed(String),
}

/// The token endpoint's answer, for the device sign-in and for a refresh.
pub fn parse_token(status: u16, j: &Json) -> Poll {
    let err = j.get("error").str();
    match err.as_str() {
        "authorization_pending" => return Poll::Pending,
        "slow_down" => return Poll::SlowDown,
        "expired_token" | "code_expired" => return Poll::Failed("The code expired before it was typed. Start the sign-in again.".into()),
        "access_denied" | "authorization_declined" => return Poll::Failed("The access was refused on the sign-in page.".into()),
        _ => {}
    }
    if !(200..300).contains(&status) || !err.is_empty() {
        return Poll::Failed(oauth_error(j, status));
    }
    let t = Tokens { access: j.get("access_token").str(), refresh: j.get("refresh_token").str(), expires_in: j.get("expires_in").as_f64().unwrap_or(3600.0) as u64 };
    if t.access.is_empty() {
        return Poll::Failed("The sign-in service gave no access token.".into());
    }
    Poll::Done(t)
}

fn need_app(app: &OAuthApp) -> Result<(), String> {
    if app.client_id.trim().is_empty() {
        return Err(format!("Paste the client ID of your {} app first.", app.provider.name()));
    }
    if app.provider == Provider::Google && app.client_secret.trim().is_empty() {
        return Err("Google needs the client secret of the app as well.".into());
    }
    Ok(())
}

fn json_of(r: &crate::http::Response) -> Json {
    r.json().unwrap_or(Json::Null)
}

/// Asks for a code to type on the provider's page.
pub fn device_start(http: &dyn Http, app: &OAuthApp) -> Result<DeviceCode, String> {
    need_app(app)?;
    let (url, scope) = match app.provider {
        Provider::OneDrive => (MS_DEVICE, MS_SCOPE),
        Provider::Google => (GOOGLE_DEVICE, GOOGLE_SCOPE),
        Provider::ICloud => return Err("iCloud Drive has no sign-in here: use its sync folder.".into()),
        Provider::GitHub => return Err(GITHUB_BY_TOKEN.into()),
    };
    let r = http.send(&Request::post(url).form(&[("client_id", app.client_id.trim()), ("scope", scope)]).timeout(30))?;
    parse_device_code(r.status, &json_of(&r))
}

/// One check of the sign-in started with `device_code`.
pub fn device_poll(http: &dyn Http, app: &OAuthApp, device_code: &str) -> Poll {
    let grant = "urn:ietf:params:oauth:grant-type:device_code";
    let req = match app.provider {
        Provider::OneDrive => Request::post(MS_TOKEN).form(&[("client_id", app.client_id.trim()), ("grant_type", grant), ("device_code", device_code)]),
        _ => Request::post(GOOGLE_TOKEN).form(&[("client_id", app.client_id.trim()), ("client_secret", app.client_secret.trim()), ("grant_type", grant), ("device_code", device_code)]),
    };
    match http.send(&req.timeout(30)) {
        Ok(r) => parse_token(r.status, &json_of(&r)),
        // the network failing once is not the end of the sign-in
        Err(_) => Poll::Pending,
    }
}

/// A new access token from the saved refresh token.
pub fn refresh(http: &dyn Http, app: &OAuthApp, refresh_token: &str) -> Result<Tokens, String> {
    need_app(app)?;
    if refresh_token.is_empty() {
        return Err(format!("Sign in to {} first.", app.provider.name()));
    }
    let req = match app.provider {
        Provider::OneDrive => Request::post(MS_TOKEN).form(&[("client_id", app.client_id.trim()), ("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("scope", MS_SCOPE)]),
        _ => Request::post(GOOGLE_TOKEN).form(&[("client_id", app.client_id.trim()), ("client_secret", app.client_secret.trim()), ("grant_type", "refresh_token"), ("refresh_token", refresh_token)]),
    };
    let r = http.send(&req.timeout(30))?;
    match parse_token(r.status, &json_of(&r)) {
        Poll::Done(t) => Ok(t),
        Poll::Failed(e) => Err(e),
        _ => Err("The sign-in service did not answer as expected.".into()),
    }
}

/// The name of the signed-in account ("Name <mail>"), for the window.
pub fn account_name(http: &dyn Http, p: Provider, token: &str) -> String {
    let url = match p {
        Provider::OneDrive => format!("{}/me?$select=displayName,userPrincipalName,mail", GRAPH),
        _ => format!("{}/about?fields=user(displayName,emailAddress)", DRIVE),
    };
    let Ok(r) = http.send(&Request::get(&url).header("Authorization", &format!("Bearer {}", token)).timeout(20)) else { return String::new() };
    if !r.ok() {
        return String::new();
    }
    let j = json_of(&r);
    let (name, mail) = match p {
        Provider::OneDrive => (j.get("displayName").str(), [j.get("mail").str(), j.get("userPrincipalName").str()].into_iter().find(|x| !x.is_empty()).unwrap_or_default()),
        _ => (j.get("user").get("displayName").str(), j.get("user").get("emailAddress").str()),
    };
    match (name, mail) {
        (n, m) if !n.is_empty() && !m.is_empty() => format!("{} <{}>", n, m),
        (n, m) => if n.is_empty() { m } else { n },
    }
}

// ----- files on the cloud -----

/// Pieces of an upload: 10 MiB, a multiple of both 320 KiB (OneDrive) and 256 KiB (Google Drive).
const CHUNK: u64 = 10 * 1024 * 1024;

fn bearer(token: &str) -> String {
    format!("Bearer {}", token)
}

fn api_error(r: &crate::http::Response, what: &str) -> String {
    let j = json_of(r);
    let msg = [j.get("error").get("message").str(), j.get("error_description").str(), j.get("error").str()].into_iter().find(|x| !x.is_empty()).unwrap_or_default();
    match r.status {
        401 => format!("{}: the sign-in is no longer valid. Sign in again.", what),
        403 if msg.is_empty() => format!("{}: access refused.", what),
        507 => format!("{}: the cloud is full.", what),
        _ if msg.is_empty() => format!("{}: the service answered {}.", what, r.status),
        _ => format!("{}: {}", what, msg),
    }
}

/// "2026-10-14T21:30:05.123Z" → Unix time (0 when it cannot be read).
pub fn parse_time(s: &str) -> u64 {
    let n = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (Some(y), Some(mo), Some(d)) = (n(0, 4), n(5, 7), n(8, 10)) else { return 0 };
    let (h, mi, se) = (n(11, 13).unwrap_or(0), n(14, 16).unwrap_or(0), n(17, 19).unwrap_or(0));
    // days from civil (Howard Hinnant), the inverse of util::civil
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    (days * 86_400 + h * 3600 + mi * 60 + se).max(0) as u64
}

/// Reads bytes `from..from+len` of a file.
fn read_piece(f: &mut std::fs::File, from: u64, len: u64) -> Result<Vec<u8>, String> {
    use std::io::{Read, Seek, SeekFrom};
    f.seek(SeekFrom::Start(from)).map_err(|e| e.to_string())?;
    let mut b = vec![0u8; len as usize];
    f.read_exact(&mut b).map_err(|e| format!("Could not read the backup: {}", e))?;
    Ok(b)
}

fn put_piece(url: &str, auth: Option<&str>, data: Vec<u8>, from: u64, total: u64) -> Request {
    let mut req = Request { method: "PUT", url: url.to_string(), headers: vec![], body: Some(("application/octet-stream".into(), Vec::new())), timeout_secs: 600 };
    if let Some(t) = auth {
        req = req.header("Authorization", &bearer(t));
    }
    let end = from + data.len() as u64;
    if total > 0 {
        req = req.header("Content-Range", &format!("bytes {}-{}/{}", from, end - 1, total));
    }
    req.body = Some(("application/octet-stream".into(), data));
    req
}

/// The JSON of an uploaded file → `Remote`.
fn remote_of(p: Provider, j: &Json) -> Remote {
    match p {
        Provider::OneDrive => Remote { id: j.get("id").str(), name: j.get("name").str(), size: j.get("size").as_f64().unwrap_or(0.0) as u64, modified: parse_time(&j.get("lastModifiedDateTime").str()) },
        // Google gives the size as a string
        _ => Remote { id: j.get("id").str(), name: j.get("name").str(), size: j.get("size").str().parse().ok().or_else(|| j.get("size").as_f64().map(|x| x as u64)).unwrap_or(0), modified: parse_time(&j.get("modifiedTime").str()) },
    }
}

/// The id of the "Reelhouse" folder on Google Drive, made the first time.
fn google_folder(http: &dyn Http, token: &str) -> Result<String, String> {
    let q = format!("name = '{}' and mimeType = 'application/vnd.google-apps.folder' and trashed = false", FOLDER);
    let r = http.send(&Request::get(&format!("{}/files?q={}&fields=files(id,name)&spaces=drive", DRIVE, form_encode(&q))).header("Authorization", &bearer(token)).timeout(30))?;
    if !r.ok() {
        return Err(api_error(&r, "Google Drive"));
    }
    if let Some(id) = json_of(&r).get("files").items().first().map(|f| f.get("id").str()).filter(|x| !x.is_empty()) {
        return Ok(id);
    }
    let body = Json::obj(vec![("name", FOLDER.into()), ("mimeType", "application/vnd.google-apps.folder".into())]).dump();
    let r = http.send(&Request::post(&format!("{}/files?fields=id", DRIVE)).header("Authorization", &bearer(token)).json_body(body).timeout(30))?;
    if !r.ok() {
        return Err(api_error(&r, "Google Drive"));
    }
    let id = json_of(&r).get("id").str();
    if id.is_empty() {
        return Err("Google Drive did not make the Reelhouse folder.".into());
    }
    Ok(id)
}

/// Uploads a backup file (in pieces, so a big one never sits in memory whole).
pub fn upload(http: &dyn Http, p: Provider, token: &str, file: &Path) -> Result<Remote, String> {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("The backup has no file name.")?;
    let total = std::fs::metadata(file).map_err(|e| format!("Could not read the backup: {}", e))?.len();
    let mut f = std::fs::File::open(file).map_err(|e| format!("Could not read the backup: {}", e))?;
    let what = p.name();
    // 1. an upload session: one address the pieces go to
    let session = match p {
        Provider::OneDrive => {
            let body = Json::obj(vec![("item", Json::obj(vec![("@microsoft.graph.conflictBehavior", "replace".into()), ("name", name.clone().into())]))]).dump();
            let url = format!("{}/me/drive/special/approot:/{}:/createUploadSession", GRAPH, crate::util::url_encode(&name));
            let r = http.send(&Request::post(&url).header("Authorization", &bearer(token)).json_body(body).timeout(30))?;
            if !r.ok() {
                return Err(api_error(&r, what));
            }
            json_of(&r).get("uploadUrl").str()
        }
        Provider::Google => {
            let folder = google_folder(http, token)?;
            let body = Json::obj(vec![("name", name.clone().into()), ("parents", Json::Arr(vec![folder.into()]))]).dump();
            let url = format!("{}/files?uploadType=resumable&fields=id,name,size,modifiedTime", DRIVE_UPLOAD);
            let r = http.send(&Request::post(&url).header("Authorization", &bearer(token)).header("X-Upload-Content-Type", "application/octet-stream").header("X-Upload-Content-Length", &total.to_string()).json_body(body).timeout(30))?;
            if !r.ok() {
                return Err(api_error(&r, what));
            }
            r.location.clone()
        }
        Provider::ICloud => return Err("iCloud Drive is reached through its sync folder only.".into()),
        Provider::GitHub => return Err(GITHUB_BY_TOKEN.into()),
    };
    if session.is_empty() {
        return Err(format!("{} did not start the upload.", what));
    }
    // 2. the pieces; the last answer describes the file
    let mut from = 0u64;
    loop {
        let len = CHUNK.min(total - from);
        let piece = read_piece(&mut f, from, len)?;
        // OneDrive's upload address is signed already: it must be sent without the token
        let auth = if p == Provider::Google { Some(token) } else { None };
        let mut tries = 0;
        let r = loop {
            match http.send(&put_piece(&session, auth, piece.clone(), from, total)) {
                Ok(r) if r.status >= 500 && tries < 3 => tries += 1,
                Ok(r) => break r,
                Err(_) if tries < 3 => tries += 1,
                Err(e) => return Err(format!("{}: the upload was cut off ({}).", what, e)),
            }
            std::thread::sleep(std::time::Duration::from_secs(2 << tries));
        };
        from += len;
        match r.status {
            200 | 201 => return Ok(remote_of(p, &json_of(&r))),
            // more pieces wanted (OneDrive 202, Google 308)
            202 | 308 if from < total => continue,
            202 | 308 => return Err(format!("{} did not finish the upload.", what)),
            _ => return Err(api_error(&r, what)),
        }
    }
}

/// The backups on the cloud, newest first.
pub fn list(http: &dyn Http, p: Provider, token: &str) -> Result<Vec<Remote>, String> {
    let (url, key) = match p {
        Provider::OneDrive => (format!("{}/me/drive/special/approot/children?$select=id,name,size,lastModifiedDateTime&$top=200", GRAPH), "value"),
        Provider::Google => {
            let folder = google_folder(http, token)?;
            let q = format!("'{}' in parents and trashed = false", folder);
            (format!("{}/files?q={}&fields=files(id,name,size,modifiedTime)&pageSize=200", DRIVE, form_encode(&q)), "files")
        }
        Provider::ICloud => return Err("iCloud Drive is reached through its sync folder only.".into()),
        Provider::GitHub => return Err(GITHUB_BY_TOKEN.into()),
    };
    let r = http.send(&Request::get(&url).header("Authorization", &bearer(token)).timeout(30))?;
    if !r.ok() {
        return Err(api_error(&r, p.name()));
    }
    let mut v: Vec<Remote> = json_of(&r).get(key).items().iter().map(|j| remote_of(p, j)).filter(|x| ours(&x.name)).collect();
    sort_newest_first(&mut v);
    Ok(v)
}

pub fn delete(http: &dyn Http, p: Provider, token: &str, id: &str) -> Result<(), String> {
    let url = match p {
        Provider::OneDrive => format!("{}/me/drive/items/{}", GRAPH, id),
        _ => format!("{}/files/{}", DRIVE, id),
    };
    let req = Request { method: "DELETE", url, headers: vec![("Authorization".into(), bearer(token))], body: None, timeout_secs: 30 };
    let r = http.send(&req)?;
    if r.ok() || r.status == 404 {
        Ok(())
    } else {
        Err(api_error(&r, p.name()))
    }
}

/// Deletes the oldest backups on the cloud beyond `keep`.
pub fn prune(http: &dyn Http, p: Provider, token: &str, keep: usize) -> Result<(), String> {
    for old in list(http, p, token)?.iter().skip(keep.max(1)) {
        delete(http, p, token, &old.id)?;
    }
    Ok(())
}

/// Downloads a backup into `dest`.
pub fn download(http: &dyn Http, p: Provider, token: &str, remote: &Remote, dest: &Path) -> Result<(), String> {
    match p {
        Provider::OneDrive => {
            // a signed address that needs no token, read straight to the file
            let r = http.send(&Request::get(&format!("{}/me/drive/items/{}?select=id,@microsoft.graph.downloadUrl", GRAPH, remote.id)).header("Authorization", &bearer(token)).timeout(30))?;
            if !r.ok() {
                return Err(api_error(&r, p.name()));
            }
            let url = json_of(&r).get("@microsoft.graph.downloadUrl").str();
            if url.is_empty() {
                return Err("OneDrive gave no address to download the backup.".into());
            }
            http.download(&url, dest, &|_, _| {})
        }
        Provider::Google => {
            // in pieces with Range, so the answer size limit of the HTTP layer never cuts a big backup
            use std::io::Write;
            let mut out = std::fs::File::create(dest).map_err(|e| format!("Could not write {}: {}", dest.display(), e))?;
            let url = format!("{}/files/{}?alt=media", DRIVE, remote.id);
            let piece = 32 * 1024 * 1024u64;
            let mut from = 0u64;
            loop {
                let r = http.send(&Request::get(&url).header("Authorization", &bearer(token)).header("Range", &format!("bytes={}-{}", from, from + piece - 1)).timeout(600))?;
                if !r.ok() {
                    // asking past the end of a file whose size was a multiple of the piece
                    if r.status == 416 && from > 0 {
                        break;
                    }
                    return Err(api_error(&r, p.name()));
                }
                out.write_all(&r.body).map_err(|e| e.to_string())?;
                from += r.body.len() as u64;
                if r.status == 200 || (r.body.len() as u64) < piece || (remote.size > 0 && from >= remote.size) {
                    break;
                }
            }
            Ok(())
        }
        Provider::ICloud => Err("iCloud Drive is reached through its sync folder only.".into()),
        Provider::GitHub => Err(GITHUB_BY_TOKEN.into()),
    }
}

// ----- what the controller keeps while the app runs -----

/// A sign-in waiting for the code to be typed.
#[derive(Clone, Debug, Default)]
pub struct Pending {
    pub provider: String,
    pub code: DeviceCode,
    pub started: u64,
}

/// Results of the cloud work done in the background (`Msg::Cloud`).
pub enum Done {
    Code(Provider, Result<DeviceCode, String>),
    /// Signed in: the tokens and the account name.
    SignedIn(Provider, Result<(Tokens, String), String>),
    /// An upload (with tokens when they were refreshed on the way).
    Uploaded(Result<Remote, String>, Option<(Provider, Tokens)>),
    Listed(Result<Vec<Remote>, String>, Option<(Provider, Tokens)>),
    /// A backup downloaded into this file, to be restored.
    Fetched(Result<PathBuf, String>, Option<(Provider, Tokens)>),
    /// GitHub connected: (token, login, "owner/name").
    GitHub(Result<(String, String, String), String>),
}

#[derive(Default)]
pub struct Live {
    /// What runs now: "", "upload", "signin", "list", "fetch".
    pub busy: String,
    pub status: String,
    pub pending: Option<Pending>,
    /// Set to stop a sign-in that waits for the code.
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub remote: Vec<Remote>,
    pub listed: bool,
    /// The provider and method `remote` was read for ("gdrive/account"): another choice hides the old list.
    pub listed_for: String,
    /// Access token in memory: (provider id, token, valid until).
    pub access: Option<(String, String, u64)>,
    /// "Upload now": the backup being made is uploaded even with the online copy off.
    pub upload_next: bool,
    /// Sync folders found on this computer, per provider (looked up once; "Detect again" looks again).
    pub detected: std::sync::Mutex<Option<Detected>>,
}

/// Sync folders found, per provider.
pub type Detected = Vec<(Provider, Vec<PathBuf>)>;

impl Live {
    pub fn detected(&self, p: Provider) -> Vec<PathBuf> {
        let mut g = self.detected.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_none() {
            *g = Some([Provider::Google, Provider::OneDrive, Provider::ICloud].iter().map(|p| (*p, detect(*p))).collect());
        }
        g.as_ref().and_then(|v| v.iter().find(|(q, _)| *q == p).map(|(_, f)| f.clone())).unwrap_or_default()
    }

    pub fn redetect(&self) {
        *self.detected.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The access token in memory, when it is for `p` and still good for a minute.
    pub fn token(&self, p: Provider) -> Option<String> {
        self.access.as_ref().filter(|(id, _, until)| id == p.id() && *until > crate::util::now_unix() + 60).map(|(_, t, _)| t.clone())
    }
}

/// An access token: the one given, or a new one from the refresh token (returned so it can be kept).
pub fn access(http: &dyn Http, app: &OAuthApp, cached: Option<String>, refresh_token: &str) -> Result<(String, Option<(Provider, Tokens)>), String> {
    if let Some(t) = cached {
        return Ok((t, None));
    }
    // a GitHub token is used as it is
    if app.provider == Provider::GitHub {
        return Ok((refresh_token.to_string(), None));
    }
    let t = refresh(http, app, refresh_token)?;
    Ok((t.access.clone(), Some((app.provider, t))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("reelhouse-cloud-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sync_folders_are_found() {
        let home = tmp("home");
        std::fs::create_dir_all(home.join("OneDrive")).unwrap();
        std::fs::create_dir_all(home.join("iCloudDrive")).unwrap();
        std::fs::create_dir_all(home.join("Meu Drive")).unwrap();
        let business = home.join("OneDrive - Acme");
        std::fs::create_dir_all(&business).unwrap();
        let h = home.display().to_string();
        let b = business.display().to_string();
        let env = move |k: &str| match k {
            "USERPROFILE" => Some(h.clone()),
            "OneDriveCommercial" => Some(b.clone()),
            _ => None,
        };
        let od = detect_sync_folders(Provider::OneDrive, &env, "windows");
        assert_eq!(od, vec![business.clone(), home.join("OneDrive")]);
        assert_eq!(detect_sync_folders(Provider::ICloud, &env, "windows"), vec![home.join("iCloudDrive")]);
        assert_eq!(detect_sync_folders(Provider::Google, &env, "windows"), vec![home.join("Meu Drive")]);
        // Google Drive for desktop's drive letter is looked at on Windows
        assert!(sync_folder_candidates(Provider::Google, &env, "windows").contains(&PathBuf::from("G:\\My Drive")));
        // macOS: CloudStorage and the iCloud folder
        let mac = tmp("mac");
        std::fs::create_dir_all(mac.join("Library/CloudStorage/GoogleDrive-me@gmail.com/My Drive")).unwrap();
        std::fs::create_dir_all(mac.join("Library/Mobile Documents/com~apple~CloudDocs")).unwrap();
        let m = mac.display().to_string();
        let menv = move |k: &str| (k == "HOME").then(|| m.clone());
        assert_eq!(detect_sync_folders(Provider::Google, &menv, "macos"), vec![mac.join("Library/CloudStorage/GoogleDrive-me@gmail.com/My Drive")]);
        assert_eq!(detect_sync_folders(Provider::ICloud, &menv, "macos"), vec![mac.join("Library/Mobile Documents/com~apple~CloudDocs")]);
        assert!(detect_sync_folders(Provider::OneDrive, &menv, "macos").is_empty());
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&mac);
    }

    #[test]
    fn sync_folder_upload_keeps_the_newest() {
        let d = tmp("sync");
        let cloud = d.join("OneDrive");
        std::fs::create_dir_all(&cloud).unwrap();
        for i in 1..=4 {
            let f = d.join(format!("reelhouse-backup-2026101{}-1200.rhb", i));
            std::fs::write(&f, vec![i as u8; 10]).unwrap();
            let r = folder_upload(&f, &cloud, 2).unwrap();
            assert_eq!(r.size, 10);
        }
        let left: Vec<String> = folder_list(&cloud).into_iter().map(|r| r.name).collect();
        assert_eq!(left, vec!["reelhouse-backup-20261014-1200.rhb", "reelhouse-backup-20261013-1200.rhb"]);
        assert!(!cloud.join(FOLDER).join("reelhouse-backup-20261014-1200.rhb.part").exists());
        assert!(folder_upload(&d.join("x.rhb"), &d.join("missing"), 2).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn device_codes_are_read() {
        let ms = Json::parse(r#"{"user_code":"F7XQ2PLM","device_code":"DAQABAAEAAA","verification_uri":"https://microsoft.com/devicelogin","expires_in":900,"interval":5,"message":"To sign in, use a web browser..."}"#).unwrap();
        let d = parse_device_code(200, &ms).unwrap();
        assert_eq!((d.user_code.as_str(), d.verify_url.as_str(), d.expires_in, d.interval), ("F7XQ2PLM", "https://microsoft.com/devicelogin", 900, 5));
        let g = Json::parse(r#"{"device_code":"AH-1Ng","user_code":"GQVQ-JKEC","expires_in":1800,"interval":5,"verification_url":"https://www.google.com/device"}"#).unwrap();
        assert_eq!(parse_device_code(200, &g).unwrap().verify_url, "https://www.google.com/device");
        let bad = Json::parse(r#"{"error":"invalid_client","error_description":"AADSTS700016: Application with identifier 'x' was not found."}"#).unwrap();
        let e = parse_device_code(400, &bad).unwrap_err();
        assert!(e.contains("client ID") && e.contains("AADSTS700016"), "{}", e);
    }

    #[test]
    fn token_answers_are_read() {
        let j = |s: &str| Json::parse(s).unwrap();
        assert_eq!(parse_token(400, &j(r#"{"error":"authorization_pending"}"#)), Poll::Pending);
        assert_eq!(parse_token(428, &j(r#"{"error":"authorization_pending","error_description":"..."}"#)), Poll::Pending);
        assert_eq!(parse_token(400, &j(r#"{"error":"slow_down"}"#)), Poll::SlowDown);
        assert!(matches!(parse_token(400, &j(r#"{"error":"expired_token"}"#)), Poll::Failed(_)));
        assert!(matches!(parse_token(400, &j(r#"{"error":"authorization_declined"}"#)), Poll::Failed(e) if e.contains("refused")));
        assert!(matches!(parse_token(403, &j(r#"{"error":"access_denied"}"#)), Poll::Failed(e) if e.contains("refused")));
        assert!(matches!(parse_token(400, &j(r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#)), Poll::Failed(e) if e.contains("Sign in again")));
        let ok = parse_token(200, &j(r#"{"token_type":"Bearer","scope":"Files.ReadWrite.AppFolder","expires_in":3599,"access_token":"EwB4A","refresh_token":"M.C1"}"#));
        assert_eq!(ok, Poll::Done(Tokens { access: "EwB4A".into(), refresh: "M.C1".into(), expires_in: 3599 }));
        // Google's refresh answer has no new refresh token
        assert!(matches!(parse_token(200, &j(r#"{"access_token":"ya29.a0","expires_in":3599,"scope":"https://www.googleapis.com/auth/drive.file","token_type":"Bearer"}"#)), Poll::Done(t) if t.refresh.is_empty()));
    }

    #[test]
    fn sign_in_requests() {
        let http = Fake::new().route("POST https://login.microsoftonline.com/common/oauth2/v2.0/devicecode", 200, r#"{"user_code":"ABC","device_code":"dc","verification_uri":"https://microsoft.com/devicelogin","expires_in":900,"interval":5}"#).route("POST https://login.microsoftonline.com/common/oauth2/v2.0/token", 400, r#"{"error":"authorization_pending"}"#).route("POST https://login.microsoftonline.com/common/oauth2/v2.0/token", 200, r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#);
        let app = OAuthApp { provider: Provider::OneDrive, client_id: "cid".into(), client_secret: String::new() };
        let d = device_start(&http, &app).unwrap();
        assert_eq!(d.user_code, "ABC");
        assert_eq!(device_poll(&http, &app, &d.device_code), Poll::Pending);
        assert!(matches!(device_poll(&http, &app, &d.device_code), Poll::Done(t) if t.refresh == "rt"));
        let reqs = http.requests();
        let body = String::from_utf8_lossy(&reqs[0].body.as_ref().unwrap().1).into_owned();
        assert!(body.contains("client_id=cid") && body.contains("offline_access") && body.contains("Files.ReadWrite.AppFolder"), "{}", body);
        let poll = String::from_utf8_lossy(&reqs[1].body.as_ref().unwrap().1).into_owned();
        assert!(poll.contains("device_code=dc") && poll.contains("grant-type%3Adevice_code"), "{}", poll);
        // Google needs the secret
        let g = OAuthApp { provider: Provider::Google, client_id: "x".into(), client_secret: String::new() };
        assert!(device_start(&http, &g).unwrap_err().contains("secret"));
    }

    #[test]
    fn times_and_settings() {
        assert_eq!(parse_time("1970-01-02T00:00:01Z"), 86_401);
        assert_eq!(parse_time("2026-10-14T21:30:05.123Z"), 1_792_013_405);
        assert_eq!(crate::util::civil(parse_time("2024-02-29T12:00:00Z")), (2024, 2, 29, 12, 0, 0));
        assert_eq!(parse_time("bad"), 0);
        let mut s = CloudSettings { enabled: true, provider: "onedrive".into(), method: "account".into(), ms_client_id: "id".into(), ..CloudSettings::default() };
        s.set_login(Provider::OneDrive, "r", "Me <me@x>");
        let back = CloudSettings::from_json(&s.to_json());
        assert_eq!(back, s);
        // iCloud has only the folder method
        let ic = CloudSettings::from_json(&Json::parse(r#"{"provider":"icloud","method":"account"}"#).unwrap());
        assert_eq!((ic.method.as_str(), ic.by_account()), ("folder", false));
        // GitHub has only the token, kept like a sign-in
        let mut gh = CloudSettings::from_json(&Json::parse(r#"{"provider":"github","method":"folder","github_repo":" hp/rb "}"#).unwrap());
        assert_eq!((gh.provider(), gh.by_account(), gh.github_repo.as_str()), (Provider::GitHub, true, "hp/rb"));
        gh.set_login(Provider::GitHub, "ghp_x", "hp");
        assert_eq!((gh.refresh_token(Provider::GitHub), gh.account(Provider::GitHub)), ("ghp_x", "hp"));
        assert_eq!(CloudSettings::from_json(&gh.to_json()), gh);
        // its token is used as it is, never refreshed
        let app = gh.app(Provider::GitHub);
        let none = Fake::new();
        assert_eq!(access(&none, &OAuthApp { provider: Provider::GitHub, ..app }, None, "ghp_x").unwrap(), ("ghp_x".to_string(), None));
        assert!(none.requests().is_empty());
    }

    #[test]
    fn remote_lists_are_read() {
        let http = Fake::new().route("GET https://graph.microsoft.com/v1.0/me/drive/special/approot/children", 200, r#"{"value":[{"id":"1","name":"reelhouse-backup-20261001-0900.rhb","size":2048,"lastModifiedDateTime":"2026-10-01T09:00:10Z"},{"id":"2","name":"notes.txt","size":3},{"id":"3","name":"reelhouse-backup-20261014-0900.rhb","size":4096,"lastModifiedDateTime":"2026-10-14T09:00:10Z"}]}"#);
        let v = list(&http, Provider::OneDrive, "t").unwrap();
        assert_eq!(v.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["3", "1"]);
        assert_eq!(v[0].size, 4096);
        let g = Fake::new().route("GET https://www.googleapis.com/drive/v3/files?q=name", 200, r#"{"files":[{"id":"F","name":"Reelhouse"}]}"#).route("GET https://www.googleapis.com/drive/v3/files?q=%27F%27", 200, r#"{"files":[{"id":"a","name":"reelhouse-backup-20261014-0900.rhb","size":"123","modifiedTime":"2026-10-14T09:00:10.000Z"}]}"#);
        let v = list(&g, Provider::Google, "t").unwrap();
        assert_eq!((v.len(), v[0].size), (1, 123));
    }
}
