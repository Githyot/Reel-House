//! AnkiWeb, with the sync protocol of the Anki clients (version 11): every request is a zstd-compressed JSON body
//! sent to `sync/<method>` with an `anki-sync` header; answers come back compressed the same way.
//! Supported here: login (host key), the collection's metadata, and a full download of the collection, which is then
//! read like an `.apkg` collection. The password is only sent once; the host key AnkiWeb gives back is what is kept.
use crate::http::{Http, Request};
use crate::json::Json;
use std::io::Read;

pub const ENDPOINT: &str = "https://sync.ankiweb.net/";
const CLIENT: &str = "anki,2.1.66 (reelhouse),win:10";
const SYNC_VERSION: u32 = 11;
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

pub struct Meta {
    pub modified: i64,
    pub user: String,
    pub message: String,
    pub empty: bool,
}

fn compress(data: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(data, ruzstd::encoding::CompressionLevel::Fastest)
}

/// Plain bytes stay as they are; one or more zstd frames are decoded one after the other.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 4 || data[..4] != ZSTD_MAGIC {
        return Ok(data.to_vec());
    }
    let mut rest: &[u8] = data;
    let mut out = Vec::new();
    while rest.len() >= 4 && rest[..4] == ZSTD_MAGIC {
        let mut dec = ruzstd::decoding::StreamingDecoder::new(&mut rest).map_err(|e| format!("AnkiWeb sent data that could not be read ({})", e))?;
        dec.read_to_end(&mut out).map_err(|e| format!("AnkiWeb sent data that could not be read ({})", e))?;
    }
    Ok(out)
}

fn session() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}", t & 0xffff_ffff)
}

/// One call. AnkiWeb may move the account to another server with a redirect: it is followed (method and body kept).
fn call(http: &dyn Http, method: &str, hkey: &str, body: &str, timeout: u64) -> Result<Vec<u8>, String> {
    let header = Json::obj(vec![
        ("v", Json::Num(SYNC_VERSION as f64)),
        ("k", Json::Str(hkey.to_string())),
        ("c", Json::Str(CLIENT.to_string())),
        ("s", Json::Str(session())),
    ])
    .dump();
    let payload = compress(body.as_bytes());
    let mut url = format!("{}sync/{}", ENDPOINT, method);
    for _ in 0..4 {
        let mut req = Request::post(&url).header("anki-sync", &header).timeout(timeout);
        req.body = Some(("application/octet-stream".to_string(), payload.clone()));
        let r = http.send(&req)?;
        match r.status {
            200 => return decompress(&r.body),
            301 | 302 | 307 | 308 if !r.location.is_empty() => {
                url = if r.location.starts_with("http") { r.location.clone() } else { format!("{}{}", ENDPOINT.trim_end_matches('/'), r.location) };
                if !url.contains("/sync/") {
                    url = format!("{}/sync/{}", url.trim_end_matches('/'), method);
                }
            }
            403 => return Err("AnkiWeb did not accept the user name or password.".into()),
            401 => return Err("The AnkiWeb login expired. Log in again.".into()),
            400..=599 => {
                let text = String::from_utf8_lossy(&decompress(&r.body).unwrap_or_default()).trim().to_string();
                return Err(if text.is_empty() { format!("AnkiWeb answered {}", r.status) } else { format!("AnkiWeb: {}", text) });
            }
            _ => return Err(format!("AnkiWeb answered {}", r.status)),
        }
    }
    Err("AnkiWeb kept redirecting the request.".into())
}

/// Logs in and gives back the host key (kept instead of the password).
pub fn login(http: &dyn Http, user: &str, pass: &str) -> Result<String, String> {
    let body = Json::obj(vec![("u", Json::Str(user.to_string())), ("p", Json::Str(pass.to_string()))]).dump();
    let out = call(http, "hostKey", "", &body, 30)?;
    let j = Json::parse(&String::from_utf8_lossy(&out)).map_err(|_| "AnkiWeb gave an answer that could not be read.".to_string())?;
    let key = j.get("key").str();
    if key.is_empty() {
        return Err("AnkiWeb did not give a login key.".into());
    }
    Ok(key)
}

/// What AnkiWeb knows of the collection: last change, the account name, a message for the user.
pub fn meta(http: &dyn Http, hkey: &str) -> Result<Meta, String> {
    let body = Json::obj(vec![("v", Json::Num(SYNC_VERSION as f64)), ("cv", Json::Str(CLIENT.to_string()))]).dump();
    let out = call(http, "meta", hkey, &body, 30)?;
    let j = Json::parse(&String::from_utf8_lossy(&out)).map_err(|_| "AnkiWeb gave an answer that could not be read.".to_string())?;
    if j.get("cont").as_bool() == Some(false) {
        return Err(format!("AnkiWeb: {}", j.get("msg").str()));
    }
    Ok(Meta { modified: j.get("mod").as_i64().unwrap_or(0), user: j.get("uname").str(), message: j.get("msg").str(), empty: j.get("empty").as_bool().unwrap_or(false) })
}

/// The whole collection (an SQLite file in the schema of `.apkg` packages).
pub fn download(http: &dyn Http, hkey: &str) -> Result<Vec<u8>, String> {
    let out = call(http, "download", hkey, "{}", 900)?;
    if out.len() < 16 || &out[..16] != b"SQLite format 3\0" {
        let text = String::from_utf8_lossy(&out[..out.len().min(200)]).to_string();
        return Err(format!("AnkiWeb did not send a collection ({})", text.trim()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zstd_round_trip() {
        let data = b"{\"key\":\"abc\"}".repeat(20);
        let c = compress(&data);
        assert_eq!(&c[..4], &ZSTD_MAGIC);
        assert_eq!(decompress(&c).unwrap(), data);
        assert_eq!(decompress(b"plain").unwrap(), b"plain");
    }
}
