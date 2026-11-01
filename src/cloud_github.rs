//! GitHub as the online copy of the backup: a private repository of the user's ("reelhouse-backup", made the first
//! time), where each backup is a release holding the backup file as its one asset. Release assets take files up to
//! 2 GB, far more than the 100 MB the contents API allows, and a release is deleted with its file when it gets old.
//!
//! It is reached with a personal access token (PAT) pasted in the Backup settings: a fine-grained token with
//! "Contents: read and write" on that repository (plus "Administration: read and write" when Reelhouse should create
//! the repository), or a classic token with the `repo` scope. The token is checked once (`connect`), then kept in
//! the settings like the cloud sign-ins (`backup.cloud`), which a restored backup never replaces.
use super::{parse_time, sort_newest_first, Remote};
use crate::http::{Http, Request, Response};
use crate::json::Json;
use std::path::Path;

pub const API: &str = "https://api.github.com";
pub const UPLOADS: &str = "https://uploads.github.com";
/// The repository made when the user names none.
pub const DEFAULT_REPO: &str = "reelhouse-backup";
/// Where a token is made (fine-grained tokens).
pub const TOKEN_URL: &str = "https://github.com/settings/personal-access-tokens/new";
/// GitHub refuses release files from 2 GiB up.
const MAX_ASSET: u64 = 2 * 1024 * 1024 * 1024 - 1;

fn req(method: &'static str, url: &str, token: &str) -> Request {
    Request {
        method,
        url: url.to_string(),
        headers: vec![
            ("Authorization".into(), format!("Bearer {}", token.trim())),
            ("Accept".into(), "application/vnd.github+json".into()),
            ("X-GitHub-Api-Version".into(), "2022-11-28".into()),
        ],
        body: None,
        timeout_secs: 30,
    }
}

fn json_of(r: &Response) -> Json {
    Json::parse(&r.text()).unwrap_or(Json::Null)
}

/// An answer of GitHub that is not a success, in words.
fn error(r: &Response, what: &str) -> String {
    let msg = json_of(r).get("message").str();
    match r.status {
        401 => "GitHub did not accept the token (wrong, expired or revoked). Make a new one and connect again.".to_string(),
        403 if msg.to_lowercase().contains("rate limit") => "GitHub: too many requests for now. Try again in a while.".to_string(),
        403 | 404 if msg.to_lowercase().contains("not accessible") => {
            format!("{}: the token has no permission for this. Give it \"Contents: read and write\" on the repository (or the \"repo\" scope).", what)
        }
        _ if msg.is_empty() => format!("{}: GitHub answered {}.", what, r.status),
        _ => format!("{}: {}", what, msg),
    }
}

/// "owner/name" from what was typed: a name (the token's account owns it), "owner/name", or the repository's
/// address. Empty means the default name.
pub fn repo_name(typed: &str, login: &str) -> Result<String, String> {
    let mut t = typed.trim().trim_end_matches('/').to_string();
    for p in ["https://github.com/", "http://github.com/", "github.com/"] {
        if let Some(rest) = t.strip_prefix(p) {
            t = rest.to_string();
        }
    }
    let t = t.trim_end_matches(".git");
    let full = if t.is_empty() {
        format!("{}/{}", login, DEFAULT_REPO)
    } else if t.contains('/') {
        t.to_string()
    } else {
        format!("{}/{}", login, t)
    };
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    match full.split_once('/') {
        Some((o, n)) if ok(o) && ok(n) && !n.contains('/') && n != "." && n != ".." => Ok(full),
        _ => Err(format!("\"{}\" is not a repository name. Use a name such as {} or owner/name.", typed.trim(), DEFAULT_REPO)),
    }
}

/// Checks the token, finds the repository (made private when it does not exist yet) and makes sure it has a first
/// commit, which releases need. Returns (the account's login, "owner/name").
pub fn connect(http: &dyn Http, token: &str, typed_repo: &str) -> Result<(String, String), String> {
    if token.trim().is_empty() {
        return Err("Paste a GitHub token first.".into());
    }
    let r = http.send(&req("GET", &format!("{}/user", API), token))?;
    if !r.ok() {
        return Err(error(&r, "GitHub"));
    }
    let login = json_of(&r).get("login").str();
    if login.is_empty() {
        return Err("GitHub did not say whose token this is.".into());
    }
    let repo = repo_name(typed_repo, &login)?;
    let (owner, name) = repo.split_once('/').unwrap_or((&login, DEFAULT_REPO));
    let r = http.send(&req("GET", &format!("{}/repos/{}", API, repo), token))?;
    if r.status == 404 {
        if !owner.eq_ignore_ascii_case(&login) {
            return Err(format!("The repository {} was not found, or the token cannot see it.", repo));
        }
        let body = Json::obj(vec![
            ("name", name.into()),
            ("private", true.into()),
            ("auto_init", true.into()),
            ("description", "Reelhouse backups".into()),
        ])
        .dump();
        let c = http.send(&req("POST", &format!("{}/user/repos", API), token).json_body(body))?;
        if !c.ok() {
            return Err(format!(
                "{} Or create a private repository named {} on GitHub yourself and connect again.",
                error(&c, &format!("Could not create {}", repo)),
                name
            ));
        }
    } else if !r.ok() {
        return Err(error(&r, &repo));
    } else {
        let j = json_of(&r);
        if j.get("private").as_bool() == Some(false) {
            return Err(format!("{} is public. A backup holds your personal data: make the repository private on GitHub, or use another name.", repo));
        }
        if j.get("permissions").get("push").as_bool() == Some(false) {
            return Err(format!("The token can read {} but not write to it. Give it \"Contents: read and write\".", repo));
        }
    }
    first_commit(http, token, &repo)?;
    Ok((login, repo))
}

/// An empty repository gets a README, so releases have a commit to point at.
fn first_commit(http: &dyn Http, token: &str, repo: &str) -> Result<(), String> {
    let r = http.send(&req("GET", &format!("{}/repos/{}/commits?per_page=1", API, repo), token))?;
    if r.status != 409 {
        return Ok(());
    }
    let text = "# Reelhouse backups\n\nEach release holds one backup file made by Reelhouse (Preferences > Backup).\n";
    let body = Json::obj(vec![("message", "Reelhouse backups".into()), ("content", crate::suwayomi::base64(text.as_bytes()).into())]).dump();
    let w = http.send(&req("PUT", &format!("{}/repos/{}/contents/README.md", API, repo), token).json_body(body))?;
    if w.ok() {
        Ok(())
    } else {
        Err(error(&w, &format!("Could not write the first file of {}", repo)))
    }
}

/// The release tag of a backup file: its name without the extension.
pub fn tag_of(name: &str) -> String {
    let n = name.strip_suffix(crate::backup::EXT).or_else(|| name.strip_suffix(crate::backup::LEGACY_EXT)).unwrap_or(name);
    n.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' }).collect()
}

/// A release file → `Remote`; its id keeps the release, the file and the tag ("12:34:tag"), all needed to delete it.
fn remote_of(release: &Json, asset: &Json) -> Remote {
    let t = asset.get("updated_at").str();
    Remote {
        id: format!("{}:{}:{}", release.get("id").as_u64().unwrap_or(0), asset.get("id").as_u64().unwrap_or(0), release.get("tag_name").str()),
        name: asset.get("name").str(),
        size: asset.get("size").as_u64().unwrap_or(0),
        modified: parse_time(if t.is_empty() { release.get("created_at").str() } else { t }.as_str()),
    }
}

fn parts(id: &str) -> Option<(u64, u64, String)> {
    let mut it = id.splitn(3, ':');
    let rel = it.next()?.parse().ok()?;
    let asset = it.next()?.parse().ok()?;
    Some((rel, asset, it.next().unwrap_or("").to_string()))
}

/// Uploads a backup as a new release ("reelhouse-backup-20261015-1130") with the file attached.
pub fn upload(http: &dyn Http, token: &str, repo: &str, file: &Path) -> Result<Remote, String> {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("The backup has no file name.")?;
    let size = std::fs::metadata(file).map_err(|e| format!("Could not read the backup: {}", e))?.len();
    if size > MAX_ASSET {
        return Err("GitHub takes files up to 2 GB. Turn off \"Include study media\" in the backup options and try again.".into());
    }
    let tag = tag_of(&name);
    let body = Json::obj(vec![
        ("tag_name", tag.as_str().into()),
        ("name", tag.as_str().into()),
        ("body", "Reelhouse backup".into()),
        ("make_latest", "false".into()),
    ])
    .dump();
    let r = http.send(&req("POST", &format!("{}/repos/{}/releases", API, repo), token).json_body(body))?;
    let release = if r.status == 422 {
        // a release of that name already exists (an upload that failed before): the file goes into it again
        let g = http.send(&req("GET", &format!("{}/repos/{}/releases/tags/{}", API, repo, crate::util::url_encode(&tag)), token))?;
        if !g.ok() {
            return Err(error(&r, "GitHub"));
        }
        let j = json_of(&g);
        for a in j.get("assets").items().iter().filter(|a| a.get("name").str() == name) {
            let _ = http.send(&req("DELETE", &format!("{}/repos/{}/releases/assets/{}", API, repo, a.get("id").as_u64().unwrap_or(0)), token));
        }
        j
    } else if r.ok() {
        json_of(&r)
    } else {
        return Err(error(&r, "GitHub"));
    };
    let rel_id = release.get("id").as_u64().unwrap_or(0);
    if rel_id == 0 {
        return Err("GitHub did not make the release for the backup.".into());
    }
    let data = std::fs::read(file).map_err(|e| format!("Could not read the backup: {}", e))?;
    let mut up = req("POST", &format!("{}/repos/{}/releases/{}/assets?name={}", UPLOADS, repo, rel_id, crate::util::url_encode(&name)), token);
    up.body = Some(("application/octet-stream".into(), data));
    up.timeout_secs = 3600;
    let fail = |e: String| {
        // an empty release is not left behind
        let _ = delete(http, token, repo, &format!("{}:0:{}", rel_id, tag));
        e
    };
    let a = http.send(&up).map_err(|e| fail(format!("GitHub: the upload was cut off ({}).", e)))?;
    if !a.ok() {
        return Err(fail(error(&a, "GitHub")));
    }
    Ok(remote_of(&release, &json_of(&a)))
}

/// The backups in the repository, newest first.
pub fn list(http: &dyn Http, token: &str, repo: &str) -> Result<Vec<Remote>, String> {
    let r = http.send(&req("GET", &format!("{}/repos/{}/releases?per_page=100", API, repo), token))?;
    if !r.ok() {
        return Err(error(&r, "GitHub"));
    }
    let mut v: Vec<Remote> = vec![];
    for rel in json_of(&r).items() {
        for a in rel.get("assets").items() {
            let x = remote_of(rel, a);
            if super::ours(&x.name) {
                v.push(x);
            }
        }
    }
    sort_newest_first(&mut v);
    Ok(v)
}

/// Deletes a backup: its release and the release's tag.
pub fn delete(http: &dyn Http, token: &str, repo: &str, id: &str) -> Result<(), String> {
    let (rel, _, tag) = parts(id).ok_or("That is not a backup on GitHub.")?;
    let r = http.send(&req("DELETE", &format!("{}/repos/{}/releases/{}", API, repo, rel), token))?;
    if !r.ok() && r.status != 404 {
        return Err(error(&r, "GitHub"));
    }
    if !tag.is_empty() {
        // the tag stays when this fails; it costs nothing
        let _ = http.send(&req("DELETE", &format!("{}/repos/{}/git/refs/tags/{}", API, repo, crate::util::url_encode(&tag)), token));
    }
    Ok(())
}

/// Deletes the oldest backups beyond `keep`.
pub fn prune(http: &dyn Http, token: &str, repo: &str, keep: usize) -> Result<(), String> {
    for old in list(http, token, repo)?.iter().skip(keep.max(1)) {
        delete(http, token, repo, &old.id)?;
    }
    Ok(())
}

/// Downloads a backup into `dest`, in pieces (Range), so the answer size limit of the HTTP layer never cuts a big
/// one. GitHub answers with a redirect to its file store; the token is not sent there.
pub fn download(http: &dyn Http, token: &str, repo: &str, remote: &Remote, dest: &Path) -> Result<(), String> {
    use std::io::Write;
    let (_, asset, _) = parts(&remote.id).ok_or("That is not a backup on GitHub.")?;
    let url = format!("{}/repos/{}/releases/assets/{}", API, repo, asset);
    let mut out = std::fs::File::create(dest).map_err(|e| format!("Could not write {}: {}", dest.display(), e))?;
    let piece = 32 * 1024 * 1024u64;
    let mut from = 0u64;
    loop {
        let mut q = req("GET", &url, token);
        q.headers.retain(|(k, _)| k != "Accept");
        q = q.header("Accept", "application/octet-stream").header("Range", &format!("bytes={}-{}", from, from + piece - 1)).timeout(600);
        let r = http.send(&q)?;
        if !r.ok() {
            if r.status == 416 && from > 0 {
                break;
            }
            return Err(error(&r, "GitHub"));
        }
        out.write_all(&r.body).map_err(|e| e.to_string())?;
        from += r.body.len() as u64;
        if r.status == 200 || r.body.is_empty() || (r.body.len() as u64) < piece || (remote.size > 0 && from >= remote.size) {
            break;
        }
    }
    if remote.size > 0 && from < remote.size {
        return Err("The download from GitHub ended early. Try again.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn repository_names() {
        assert_eq!(repo_name("", "hp").unwrap(), "hp/reelhouse-backup");
        assert_eq!(repo_name(" meus-dados ", "hp").unwrap(), "hp/meus-dados");
        assert_eq!(repo_name("Githyot/backup", "hp").unwrap(), "Githyot/backup");
        assert_eq!(repo_name("https://github.com/hp/rb.git", "x").unwrap(), "hp/rb");
        assert!(repo_name("a/b/c", "hp").is_err());
        assert!(repo_name("nome com espaço", "hp").is_err());
        assert!(repo_name("hp/..", "hp").is_err());
        assert_eq!(tag_of("reelhouse-backup-20261015-1130.rhb"), "reelhouse-backup-20261015-1130");
    }

    #[test]
    fn connect_makes_a_private_repository_and_refuses_a_public_one() {
        let http = Fake::new()
            .route("GET https://api.github.com/user", 200, r#"{"login":"hp"}"#)
            .route("GET https://api.github.com/repos/hp/reelhouse-backup/commits", 200, "[]")
            .route("GET https://api.github.com/repos/hp/reelhouse-backup", 404, r#"{"message":"Not Found"}"#)
            .route("POST https://api.github.com/user/repos", 201, r#"{"full_name":"hp/reelhouse-backup","private":true}"#);
        assert_eq!(connect(&http, "tok", "").unwrap(), ("hp".to_string(), "hp/reelhouse-backup".to_string()));
        let reqs = http.requests();
        let make = reqs.iter().find(|r| r.url.ends_with("/user/repos")).unwrap();
        let body = String::from_utf8_lossy(&make.body.as_ref().unwrap().1).into_owned();
        assert!(body.contains("\"private\":true") && body.contains("\"auto_init\":true"), "{}", body);
        assert!(reqs[0].headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer tok"));

        let public = Fake::new()
            .route("GET https://api.github.com/user", 200, r#"{"login":"hp"}"#)
            .route("GET https://api.github.com/repos/hp/open", 200, r#"{"private":false,"permissions":{"push":true}}"#);
        assert!(connect(&public, "tok", "open").unwrap_err().contains("public"));

        let bad = Fake::new().route("GET https://api.github.com/user", 401, r#"{"message":"Bad credentials"}"#);
        assert!(connect(&bad, "x", "").unwrap_err().contains("token"));
        assert!(connect(&bad, " ", "").is_err());

        // an empty repository gets its first commit
        let empty = Fake::new()
            .route("GET https://api.github.com/user", 200, r#"{"login":"hp"}"#)
            .route("GET https://api.github.com/repos/hp/rb/commits", 409, r#"{"message":"Git Repository is empty."}"#)
            .route("GET https://api.github.com/repos/hp/rb", 200, r#"{"private":true,"permissions":{"push":true}}"#)
            .route("PUT https://api.github.com/repos/hp/rb/contents/README.md", 201, "{}");
        assert!(connect(&empty, "tok", "rb").is_ok());
        assert!(empty.requests().iter().any(|r| r.method == "PUT"));
    }

    #[test]
    fn upload_list_prune_and_download() {
        let d = std::env::temp_dir().join(format!("reelhouse-gh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("reelhouse-backup-20261015-1130.rhb");
        std::fs::write(&f, b"PK backup").unwrap();
        let http = Fake::new()
            .route("POST https://api.github.com/repos/hp/rb/releases", 201, r#"{"id":7,"tag_name":"reelhouse-backup-20261015-1130","created_at":"2026-10-15T11:30:00Z"}"#)
            .route("POST https://uploads.github.com/repos/hp/rb/releases/7/assets?name=reelhouse-backup-20261015-1130.rhb", 201, r#"{"id":70,"name":"reelhouse-backup-20261015-1130.rhb","size":9,"updated_at":"2026-10-15T11:31:00Z"}"#)
            .route(
                "GET https://api.github.com/repos/hp/rb/releases?",
                200,
                r#"[{"id":7,"tag_name":"reelhouse-backup-20261015-1130","assets":[{"id":70,"name":"reelhouse-backup-20261015-1130.rhb","size":9,"updated_at":"2026-10-15T11:31:00Z"}]},
                    {"id":5,"tag_name":"reelhouse-backup-20261014-0900","assets":[{"id":50,"name":"reelhouse-backup-20261014-0900.rhb","size":4,"updated_at":"2026-10-14T09:01:00Z"}]},
                    {"id":3,"tag_name":"other","assets":[{"id":30,"name":"notes.txt","size":1}]}]"#,
            )
            .route("DELETE https://api.github.com/repos/hp/rb/releases/5", 204, "")
            .route("DELETE https://api.github.com/repos/hp/rb/git/refs/tags/reelhouse-backup-20261014-0900", 204, "")
            .route("GET https://api.github.com/repos/hp/rb/releases/assets/70", 206, "PK backup");
        let up = upload(&http, "tok", "hp/rb", &f).unwrap();
        assert_eq!((up.id.as_str(), up.size), ("7:70:reelhouse-backup-20261015-1130", 9));
        let sent = http.requests().into_iter().find(|r| r.url.starts_with(UPLOADS)).unwrap();
        assert_eq!(sent.body.as_ref().unwrap().1, b"PK backup".to_vec());

        let v = list(&http, "tok", "hp/rb").unwrap();
        assert_eq!(v.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["reelhouse-backup-20261015-1130.rhb", "reelhouse-backup-20261014-0900.rhb"]);
        prune(&http, "tok", "hp/rb", 1).unwrap();
        assert!(http.requests().iter().any(|r| r.method == "DELETE" && r.url.ends_with("/releases/5")));
        assert!(!http.requests().iter().any(|r| r.method == "DELETE" && r.url.ends_with("/releases/7")));

        let dest = d.join("down.rhb");
        download(&http, "tok", "hp/rb", &v[0], &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"PK backup".to_vec());
        let get = http.requests().into_iter().rev().find(|r| r.url.ends_with("/assets/70")).unwrap();
        assert!(get.headers.iter().any(|(k, v)| k == "Accept" && v == "application/octet-stream"));
        assert!(!get.headers.iter().any(|(k, v)| k == "Accept" && v.contains("vnd.github")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failed_upload_leaves_no_empty_release() {
        let d = std::env::temp_dir().join(format!("reelhouse-gh2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("reelhouse-backup-20261015-1200.rhb");
        std::fs::write(&f, b"x").unwrap();
        let http = Fake::new()
            .route("POST https://api.github.com/repos/hp/rb/releases", 201, r#"{"id":9,"tag_name":"reelhouse-backup-20261015-1200"}"#)
            .route("POST https://uploads.github.com/", 403, r#"{"message":"Resource not accessible by personal access token"}"#)
            .route("DELETE https://api.github.com/repos/hp/rb/releases/9", 204, "");
        let e = upload(&http, "tok", "hp/rb", &f).unwrap_err();
        assert!(e.contains("permission"), "{}", e);
        assert!(http.requests().iter().any(|r| r.method == "DELETE" && r.url.ends_with("/releases/9")));
        let _ = std::fs::remove_dir_all(&d);
    }
}
