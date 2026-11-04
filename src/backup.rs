//! The backup: everything the app keeps about the user in one file, `reelhouse-backup-YYYYMMDD-HHMM.rhb`.
//!
//! The file is a plain ZIP (any unzip program opens it) with:
//! - `manifest.json`: what made it, when, and what is inside (sections, number of files, sizes);
//! - `data/<path>`: each file of the data folder, under its path there (settings, library, progress, addons, the
//!   study collection and its media, knowledge.md, learn.json, time.json, browser history, page translations,
//!   reader edits, books, extensions, karaoke delays (in the settings), covers and chosen pictures, and the cached
//!   pictures the library uses).
//!
//! Caches, thumbnails, downloaded videos, tools and logs are left out: they are big and come back by themselves.
//! Text is compressed with deflate (`gzip.rs`); pictures and videos, already compressed, are stored as they are.
//! Old backups (`.proto.gz`, protobuf + gzip, see `docs/backup.proto`) are still read.
use crate::gzip;
use crate::json::Json;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Version of the layout. A newer number than this means a newer app made it.
pub const FORMAT: u32 = 2;
pub const EXT: &str = ".rhb";
/// The first backups: protobuf, gzip compressed.
pub const LEGACY_EXT: &str = ".proto.gz";
/// Every backup file name starts with this (the automatic ones are pruned by it).
pub const PREFIX: &str = "reelhouse-backup-";
/// Copies made just before a restore, in case the restored one was the wrong one.
pub const BEFORE_RESTORE: &str = "reelhouse-before-restore-";

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Relative to the data folder, always with `/` as separator.
    pub path: String,
    pub data: Vec<u8>,
}

/// What to put in a backup.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// The pictures, sounds and clips of the study cards (can be big).
    pub media: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { media: true }
    }
}

/// What a backup holds, as said by its manifest.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Manifest {
    pub format: u32,
    pub app_version: String,
    pub created: u64,
    pub media: bool,
    pub files: usize,
    /// Size of the files before compression.
    pub bytes: u64,
    /// (section, files, bytes), e.g. ("study", 12, 40960).
    pub sections: Vec<(String, usize, u64)>,
}

impl Manifest {
    pub fn to_json(&self) -> Json {
        let (y, mo, d, h, mi, s) = crate::util::civil(self.created);
        Json::obj(vec![
            ("app", "Reelhouse".into()),
            ("format", self.format.into()),
            ("app_version", self.app_version.clone().into()),
            ("created", (self.created as f64).into()),
            ("created_utc", format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, mi, s).into()),
            ("os", std::env::consts::OS.into()),
            ("study_media", self.media.into()),
            ("files", (self.files as f64).into()),
            ("bytes", (self.bytes as f64).into()),
            (
                "sections",
                Json::Arr(self.sections.iter().map(|(n, f, b)| Json::obj(vec![("name", n.clone().into()), ("files", (*f as f64).into()), ("bytes", (*b as f64).into())])).collect()),
            ),
        ])
    }

    pub fn from_json(j: &Json) -> Result<Manifest, String> {
        if j.get("app").str() != "Reelhouse" {
            return Err("This is not a Reelhouse backup.".to_string());
        }
        let num = |k: &str| j.get(k).as_f64().unwrap_or(0.0);
        Ok(Manifest {
            format: num("format") as u32,
            app_version: j.get("app_version").str(),
            created: num("created") as u64,
            media: j.get("study_media").as_bool().unwrap_or(true),
            files: num("files") as usize,
            bytes: num("bytes") as u64,
            sections: j.get("sections").items().iter().map(|s| (s.get("name").str(), s.get("files").as_f64().unwrap_or(0.0) as usize, s.get("bytes").as_f64().unwrap_or(0.0) as u64)).collect(),
        })
    }
}

/// What writing a backup gave.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub files: usize,
    /// Size of the backup file.
    pub size: u64,
}

// ----- the old format (protobuf), still read -----

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7F) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_bytes(out: &mut Vec<u8>, field: u32, data: &[u8]) {
    put_varint(out, ((field as u64) << 3) | 2);
    put_varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

fn put_num(out: &mut Vec<u8>, field: u32, v: u64) {
    put_varint(out, (field as u64) << 3);
    put_varint(out, v);
}

struct Cur<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn varint(&mut self) -> Result<u64, String> {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let b = *self.d.get(self.p).ok_or("The backup file is cut short.")?;
            self.p += 1;
            v |= ((b & 0x7F) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
            if shift > 63 {
                return Err("The backup file is damaged.".to_string());
            }
        }
    }
    fn bytes(&mut self) -> Result<&'a [u8], String> {
        let n = self.varint()? as usize;
        if self.p.checked_add(n).map(|e| e > self.d.len()).unwrap_or(true) {
            return Err("The backup file is cut short.".to_string());
        }
        let s = &self.d[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn done(&self) -> bool {
        self.p >= self.d.len()
    }
}

/// Skips a field this version does not know, so newer backups still open.
fn skip(c: &mut Cur, wire: u64) -> Result<(), String> {
    match wire {
        0 => c.varint().map(|_| ()),
        1 => {
            c.p += 8;
            Ok(())
        }
        2 => c.bytes().map(|_| ()),
        5 => {
            c.p += 4;
            Ok(())
        }
        _ => Err("The backup file is damaged.".to_string()),
    }
}

/// The old protobuf layout (format 1); only written by the tests now.
pub fn encode(entries: &[Entry], created: u64) -> Vec<u8> {
    let mut out = Vec::new();
    put_num(&mut out, 1, 1);
    put_bytes(&mut out, 2, b"Reelhouse");
    put_num(&mut out, 3, created);
    for e in entries {
        let mut f = Vec::with_capacity(e.data.len() + e.path.len() + 16);
        put_bytes(&mut f, 1, e.path.as_bytes());
        put_bytes(&mut f, 2, &e.data);
        put_bytes(&mut out, 4, &f);
    }
    out
}

pub fn decode(bytes: &[u8]) -> Result<(u64, Vec<Entry>), String> {
    let mut c = Cur { d: bytes, p: 0 };
    let (mut created, mut format, mut app) = (0u64, 0u64, false);
    let mut entries = Vec::new();
    while !c.done() {
        let tag = c.varint()?;
        match (tag >> 3, tag & 7) {
            (1, 0) => format = c.varint()?,
            (2, 2) => app = c.bytes()? == b"Reelhouse",
            (3, 0) => created = c.varint()?,
            (4, 2) => {
                let inner = c.bytes()?;
                let mut f = Cur { d: inner, p: 0 };
                let (mut path, mut data) = (String::new(), Vec::new());
                while !f.done() {
                    let t = f.varint()?;
                    match (t >> 3, t & 7) {
                        (1, 2) => path = String::from_utf8_lossy(f.bytes()?).into_owned(),
                        (2, 2) => data = f.bytes()?.to_vec(),
                        (_, w) => skip(&mut f, w)?,
                    }
                }
                entries.push(Entry { path, data });
            }
            (_, w) => skip(&mut c, w)?,
        }
    }
    if !app || format == 0 {
        return Err("This is not a Reelhouse backup.".to_string());
    }
    if format > 1 {
        return Err("This backup was made by a newer version of Reelhouse.".to_string());
    }
    Ok((created, entries))
}

pub fn pack(entries: &[Entry], created: u64) -> Vec<u8> {
    gzip::gzip(&encode(entries, created))
}

pub fn unpack(bytes: &[u8]) -> Result<(u64, Vec<Entry>), String> {
    // Also accept a plain (not compressed) protobuf file.
    if bytes.len() > 2 && bytes[0] == 0x1F && bytes[1] == 0x8B {
        decode(&gzip::gunzip(bytes)?)
    } else {
        decode(bytes)
    }
}

// ----- ZIP, written and read here (deflate from gzip.rs), so no library is needed -----

/// DOS date and time of a Unix time, as ZIP keeps them.
fn dos_time(secs: u64) -> (u16, u16) {
    let (y, mo, d, h, mi, s) = crate::util::civil(secs.max(315_532_800)); // ZIP starts in 1980
    let time = ((h as u16) << 11) | ((mi as u16) << 5) | ((s as u16) / 2);
    let date = (((y - 1980).clamp(0, 127) as u16) << 9) | ((mo as u16) << 5) | d as u16;
    (time, date)
}

/// Files already compressed: deflate would only spend time on them.
fn compressed_already(name: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "webp" | "jpg" | "jpeg" | "png" | "gif" | "avif" | "mp4" | "webm" | "mkv" | "m4a" | "mp3" | "ogg" | "opus" | "aac" | "flac" | "gz" | "zip" | "epub" | "apkg" | "zst" | "bz2" | "xz" | "7z" | "rhb" | "woff" | "woff2"
    )
}

/// Writes a ZIP file one entry at a time (only the current file is in memory).
pub struct ZipWriter<W: Write> {
    w: W,
    pos: u64,
    central: Vec<u8>,
    count: usize,
}

const TOO_BIG: &str = "The backup would be larger than 4 GB. Turn off \"Study media\" in the backup options and try again.";

impl<W: Write> ZipWriter<W> {
    pub fn new(w: W) -> ZipWriter<W> {
        ZipWriter { w, pos: 0, central: Vec::new(), count: 0 }
    }

    pub fn add(&mut self, name: &str, data: &[u8], mtime: u64) -> Result<(), String> {
        let crc = gzip::crc32(data);
        let packed = if compressed_already(name) || data.len() < 64 { None } else { Some(gzip::deflate(data)).filter(|d| d.len() < data.len()) };
        let (method, body): (u16, &[u8]) = match &packed {
            Some(d) => (8, d),
            None => (0, data),
        };
        if self.pos > u32::MAX as u64 || data.len() as u64 > u32::MAX as u64 || self.count >= 65_535 {
            return Err(TOO_BIG.to_string());
        }
        let (time, date) = dos_time(mtime);
        let n = name.as_bytes();
        let mut h = Vec::with_capacity(30 + n.len());
        h.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        h.extend_from_slice(&20u16.to_le_bytes()); // version needed
        h.extend_from_slice(&0x0800u16.to_le_bytes()); // names are UTF-8
        h.extend_from_slice(&method.to_le_bytes());
        h.extend_from_slice(&time.to_le_bytes());
        h.extend_from_slice(&date.to_le_bytes());
        h.extend_from_slice(&crc.to_le_bytes());
        h.extend_from_slice(&(body.len() as u32).to_le_bytes());
        h.extend_from_slice(&(data.len() as u32).to_le_bytes());
        h.extend_from_slice(&(n.len() as u16).to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(n);
        let c = &mut self.central;
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes()); // made by
        c.extend_from_slice(&20u16.to_le_bytes()); // needed
        c.extend_from_slice(&0x0800u16.to_le_bytes());
        c.extend_from_slice(&method.to_le_bytes());
        c.extend_from_slice(&time.to_le_bytes());
        c.extend_from_slice(&date.to_le_bytes());
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&(body.len() as u32).to_le_bytes());
        c.extend_from_slice(&(data.len() as u32).to_le_bytes());
        c.extend_from_slice(&(n.len() as u16).to_le_bytes());
        c.extend_from_slice(&[0u8; 8]); // extra, comment, disk, internal attributes
        c.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        c.extend_from_slice(&(self.pos as u32).to_le_bytes());
        c.extend_from_slice(n);
        self.w.write_all(&h).map_err(|e| e.to_string())?;
        self.w.write_all(body).map_err(|e| e.to_string())?;
        self.pos += (h.len() + body.len()) as u64;
        self.count += 1;
        if self.pos > u32::MAX as u64 {
            return Err(TOO_BIG.to_string());
        }
        Ok(())
    }

    /// Writes the directory at the end and gives back the writer.
    pub fn finish(mut self) -> Result<W, String> {
        let mut e = Vec::with_capacity(22);
        e.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        e.extend_from_slice(&[0u8; 4]); // disk numbers
        e.extend_from_slice(&(self.count as u16).to_le_bytes());
        e.extend_from_slice(&(self.count as u16).to_le_bytes());
        e.extend_from_slice(&(self.central.len() as u32).to_le_bytes());
        e.extend_from_slice(&(self.pos as u32).to_le_bytes());
        e.extend_from_slice(&0u16.to_le_bytes());
        self.w.write_all(&self.central).map_err(|e| e.to_string())?;
        self.w.write_all(&e).map_err(|e| e.to_string())?;
        self.w.flush().map_err(|e| e.to_string())?;
        Ok(self.w)
    }
}

struct ZipItem {
    name: String,
    method: u16,
    crc: u32,
    csize: u64,
    size: u64,
    offset: u64,
}

/// Reads a ZIP file from disk entry by entry (a backup with study media can be big: it is never read whole).
pub struct ZipReader {
    f: fs::File,
    items: Vec<ZipItem>,
}

fn le16(d: &[u8], p: usize) -> u64 {
    d.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u64).unwrap_or(0)
}

fn le32(d: &[u8], p: usize) -> u64 {
    d.get(p..p + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64).unwrap_or(0)
}

impl ZipReader {
    pub fn open(path: &Path) -> Result<ZipReader, String> {
        let bad = || "The backup file is damaged or is not a Reelhouse backup.".to_string();
        let mut f = fs::File::open(path).map_err(|e| format!("Could not read {}: {}", path.display(), e))?;
        let len = f.metadata().map_err(|e| e.to_string())?.len();
        if len < 22 {
            return Err(bad());
        }
        // the end record is in the last 64 KB (it can have a comment after it)
        let tail_len = len.min(22 + 65_535);
        f.seek(SeekFrom::Start(len - tail_len)).map_err(|e| e.to_string())?;
        let mut tail = vec![0u8; tail_len as usize];
        f.read_exact(&mut tail).map_err(|e| e.to_string())?;
        let eocd = (0..=tail.len() - 22).rev().find(|&i| tail[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).ok_or_else(bad)?;
        let count = le16(&tail, eocd + 10) as usize;
        let (cd_size, cd_off) = (le32(&tail, eocd + 12), le32(&tail, eocd + 16));
        if cd_off + cd_size > len {
            return Err(bad());
        }
        f.seek(SeekFrom::Start(cd_off)).map_err(|e| e.to_string())?;
        let mut cd = vec![0u8; cd_size as usize];
        f.read_exact(&mut cd).map_err(|_| bad())?;
        let mut items = Vec::with_capacity(count);
        let mut p = 0usize;
        for _ in 0..count {
            if cd.get(p..p + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
                return Err(bad());
            }
            let (n, x, c) = (le16(&cd, p + 28) as usize, le16(&cd, p + 30) as usize, le16(&cd, p + 32) as usize);
            let name = cd.get(p + 46..p + 46 + n).map(|b| String::from_utf8_lossy(b).into_owned()).ok_or_else(bad)?;
            items.push(ZipItem { name, method: le16(&cd, p + 10) as u16, crc: le32(&cd, p + 16) as u32, csize: le32(&cd, p + 20), size: le32(&cd, p + 24), offset: le32(&cd, p + 42) });
            p += 46 + n + x + c;
        }
        Ok(ZipReader { f, items })
    }

    pub fn names(&self) -> Vec<String> {
        self.items.iter().map(|i| i.name.clone()).collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.items.iter().any(|i| i.name == name)
    }

    /// The contents of one entry, checked against its CRC.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>, String> {
        let i = self.items.iter().position(|i| i.name == name).ok_or_else(|| format!("{} is not in the backup", name))?;
        self.read_at(i)
    }

    fn read_at(&mut self, i: usize) -> Result<Vec<u8>, String> {
        let it = &self.items[i];
        let damaged = || format!("The backup file is damaged ({}).", it.name);
        let mut h = [0u8; 30];
        self.f.seek(SeekFrom::Start(it.offset)).map_err(|e| e.to_string())?;
        self.f.read_exact(&mut h).map_err(|_| damaged())?;
        if h[0..4] != [0x50, 0x4b, 0x03, 0x04] {
            return Err(damaged());
        }
        let skip = le16(&h, 26) + le16(&h, 28);
        self.f.seek(SeekFrom::Current(skip as i64)).map_err(|e| e.to_string())?;
        let mut raw = vec![0u8; it.csize as usize];
        self.f.read_exact(&mut raw).map_err(|_| damaged())?;
        let data = match it.method {
            0 => raw,
            8 => gzip::inflate(&raw).map_err(|_| damaged())?,
            m => return Err(format!("{}: compression {} is not supported", it.name, m)),
        };
        if data.len() as u64 != it.size || gzip::crc32(&data) != it.crc {
            return Err(damaged());
        }
        Ok(data)
    }
}

/// The file starts like a ZIP.
fn is_zip(path: &Path) -> bool {
    let mut b = [0u8; 4];
    fs::File::open(path).and_then(|mut f| f.read_exact(&mut b)).map(|_| b == [0x50, 0x4b, 0x03, 0x04]).unwrap_or(false)
}

// ----- which files belong in a backup -----

/// A path from a backup is only used when it stays inside the data folder.
pub fn safe_rel(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') || path.contains(':') || path.contains('\0') {
        return false;
    }
    !path.split(['/', '\\']).any(|p| p == ".." || p.is_empty())
}

/// Folders of the data folder that are not part of a backup: caches, downloads, programs and logs. They are big and
/// come back by themselves (`imgcache` is added apart: only the pictures the library uses).
const SKIP_DIRS: &[&str] = &[
    "torrents", "runtime", "shaders", "thumbs", "backups", "subtitles", "imgcache", "mal-cache", "detail-cache", "ocr", "upscaled", "upscaler", "tools",
    "debug", "autocards", "suwayomi", "learn", "cache", "logs", "tmp", "temp",
    // IP protection: Tor's directory cache and state (src/privacy.rs)
    "tor", "vpn",
];
/// Folders skipped wherever they are (a web browser profile keeps its caches in these).
const SKIP_ANYWHERE: &[&str] = &["cache", "caches", "code cache", "gpucache", "dawncache", "grshadercache", "shadercache", "service worker", "blob_storage", "thumbs", "thumbnails", "tmp", "temp"];
/// Files of the top of the data folder that are kept: the app's own data (settings, lists, notes).
const TOP_EXTS: &[&str] = &["json", "md", "txt", "csv", "tsv", "toml", "html", "db", "sqlite", "sqlite3"];
/// Downloaded videos are never part of a backup (the clips of study cards are, under `study/`).
const VIDEO_EXTS: &[&str] = &["mkv", "mp4", "avi", "mov", "m4v", "ts", "webm", "wmv", "flv"];
/// One file larger than this is left out (it would make the backup too big to upload).
const MAX_FILE: u64 = 1024 * 1024 * 1024;

/// File name of a cached picture for an address.
pub fn image_file(url: &str) -> String {
    format!("imgcache/{}", crate::util::fnv_hex(url))
}

/// A file never kept: half-written files, logs, locks and other backups.
fn skip_file(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    // IP protection's choice holds a WireGuard private key or a proxy password: never in a backup (online copies)
    if l == "privacy.json" {
        return true;
    }
    [".tmp", ".part", ".log", ".pid", ".lock", EXT, LEGACY_EXT, ".crdownload"].iter().any(|e| l.ends_with(e)) || l.starts_with("crash-") || l.starts_with('~')
}

fn ext_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// A file to put in the backup: its path in the data folder and on disk.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub rel: String,
    pub path: PathBuf,
    pub size: u64,
    pub mtime: u64,
}

/// The media of the study cards: the main profile's (`study/media`) or another profile's (`profiles/<id>/study/media`,
/// see `profiles.rs`); left out when the backup is made without "Study media".
fn is_study_media(r: &str) -> bool {
    r == "study/media" || (r.starts_with("profiles/") && r.ends_with("/study/media") && r.matches('/').count() == 3)
}

fn walk(base: &Path, rel: &str, opts: Options, out: &mut Vec<Item>) {
    let Ok(rd) = fs::read_dir(base.join(rel)) else { return };
    let mut list: Vec<_> = rd.flatten().collect();
    list.sort_by_key(|e| e.file_name());
    for e in list {
        let name = e.file_name().to_string_lossy().into_owned();
        let r = if rel.is_empty() { name.clone() } else { format!("{}/{}", rel, name) };
        let lower = name.to_ascii_lowercase();
        // symbolic links are not followed: one could point out of the data folder, or at itself
        let Ok(m) = fs::symlink_metadata(e.path()) else { continue };
        if m.is_dir() {
            let skip = (rel.is_empty() && SKIP_DIRS.contains(&lower.as_str())) || SKIP_ANYWHERE.contains(&lower.as_str()) || lower.starts_with('.') || (!opts.media && is_study_media(&r))
                // the Chromium profile of Research mode: caches and encrypted cookies, useless on another computer
                || r == "browser/cef";
            if !skip {
                walk(base, &r, opts, out);
            }
        } else if m.is_file() {
            let ext = ext_of(&name);
            let keep = !skip_file(&name)
                && (!rel.is_empty() || TOP_EXTS.contains(&ext.as_str()))
                && (!VIDEO_EXTS.contains(&ext.as_str()) || r.starts_with("study/"))
                && m.len() <= MAX_FILE;
            if keep {
                let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                out.push(Item { rel: r, path: e.path(), size: m.len(), mtime });
            }
        }
    }
}

/// Everything to save: the data folder (without caches) and the cached pictures for `urls`.
pub fn plan(dir: &Path, urls: &[String], opts: Options) -> Vec<Item> {
    let mut out = Vec::new();
    walk(dir, "", opts, &mut out);
    let mut seen: Vec<String> = Vec::new();
    for u in urls {
        let f = image_file(u);
        if seen.contains(&f) {
            continue;
        }
        let p = dir.join(&f);
        if let Ok(m) = fs::metadata(&p) {
            seen.push(f.clone());
            out.push(Item { rel: f, path: p, size: m.len(), mtime: 0 });
        }
    }
    out
}

/// The section a file belongs to, for the manifest and the window ("study", "browser", "lists"…).
pub fn section_of(rel: &str) -> String {
    if rel == "settings.json" {
        return "settings".into();
    }
    if rel.starts_with("study/media/") || (rel.starts_with("profiles/") && rel.split('/').nth(2) == Some("study") && rel.split('/').nth(3) == Some("media")) {
        return "study media".into();
    }
    match rel.split_once('/') {
        Some(("imgcache", _)) => "pictures".into(),
        Some(("edits", _)) => "reader edits".into(),
        Some(("plugins", _)) => "text plugins".into(),
        Some(("forced", _)) => "translated subtitles".into(),
        Some((top, _)) => top.to_string(),
        None => "lists".into(),
    }
}

/// The settings as they go into a backup, without the sign-ins of the online copy (the GitHub token, the Google and
/// Microsoft refresh tokens, the Google client secret). A restore keeps this computer's backup settings anyway
/// (`controller_backup.rs`), so in the file they would only be secrets travelling where people copy or share it.
pub fn scrub_settings(data: &[u8]) -> Vec<u8> {
    let Ok(mut j) = Json::parse(&String::from_utf8_lossy(data)) else { return data.to_vec() };
    let mut b = j.get("backup").clone();
    let mut c = b.get("cloud").clone();
    if c.is_null() {
        return data.to_vec();
    }
    for k in ["github_token", "google_refresh", "ms_refresh", "google_client_secret"] {
        if !c.get(k).is_null() {
            c.set(k, Json::Str(String::new()));
        }
    }
    b.set("cloud", c);
    j.set("backup", b);
    j.pretty().into_bytes()
}

/// Writes a backup of `dir` to `dest` (through a temporary file, so a cut-off write never leaves half a backup).
pub fn write_backup(dir: &Path, dest: &Path, urls: &[String], now: u64, opts: Options) -> Result<Summary, String> {
    let items = plan(dir, urls, opts);
    if let Some(p) = dest.parent() {
        fs::create_dir_all(p).map_err(|e| format!("Could not create {}: {}", p.display(), e))?;
    }
    let tmp = dest.with_extension("tmp");
    let r = (|| -> Result<Summary, String> {
        let file = fs::File::create(&tmp).map_err(|e| format!("Could not write {}: {}", dest.display(), e))?;
        let mut z = ZipWriter::new(std::io::BufWriter::new(file));
        let mut man = Manifest { format: FORMAT, app_version: env!("CARGO_PKG_VERSION").to_string(), created: now, media: opts.media, ..Manifest::default() };
        for it in &items {
            // a file that went away or is locked by another program is left out, not the whole backup
            let Ok(data) = fs::read(&it.path) else { continue };
            let data = if it.rel == "settings.json" { scrub_settings(&data) } else { data };
            z.add(&format!("data/{}", it.rel), &data, if it.mtime > 0 { it.mtime } else { now })?;
            man.files += 1;
            man.bytes += data.len() as u64;
            let sec = section_of(&it.rel);
            match man.sections.iter_mut().find(|s| s.0 == sec) {
                Some(s) => {
                    s.1 += 1;
                    s.2 += data.len() as u64;
                }
                None => man.sections.push((sec, 1, data.len() as u64)),
            }
        }
        z.add("manifest.json", man.to_json().pretty().as_bytes(), now)?;
        z.finish()?;
        Ok(Summary { files: man.files, size: fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) })
    })();
    match r {
        Ok(s) => {
            fs::rename(&tmp, dest).map_err(|e| format!("Could not write {}: {}", dest.display(), e))?;
            Ok(s)
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// What a backup file holds (its manifest); an old `.proto.gz` gives a manifest made from its files.
pub fn read_manifest(src: &Path) -> Result<Manifest, String> {
    if is_zip(src) {
        let mut z = ZipReader::open(src)?;
        let raw = z.read("manifest.json").map_err(|_| "This is not a Reelhouse backup.".to_string())?;
        let j = Json::parse(&String::from_utf8_lossy(&raw)).map_err(|_| "The backup file is damaged.".to_string())?;
        Manifest::from_json(&j)
    } else {
        let bytes = fs::read(src).map_err(|e| format!("Could not read {}: {}", src.display(), e))?;
        let (created, entries) = unpack(&bytes)?;
        Ok(Manifest { format: 1, created, media: false, files: entries.len(), bytes: entries.iter().map(|e| e.data.len() as u64).sum(), ..Manifest::default() })
    }
}

/// Puts the files of a backup back into the data folder. Returns how many files were restored.
/// The files are first unpacked (and checked) in a folder of their own, then moved in place, so a damaged backup
/// changes nothing.
pub fn restore(dir: &Path, src: &Path) -> Result<usize, String> {
    let stage = dir.join("backups").join(format!(".restoring-{}", std::process::id()));
    let _ = fs::remove_dir_all(&stage);
    let r = unpack_to(&stage, src).and_then(|rels| {
        let mut n = 0;
        for rel in &rels {
            let target = dir.join(rel);
            if let Some(p) = target.parent() {
                fs::create_dir_all(p).map_err(|x| format!("Could not create {}: {}", p.display(), x))?;
            }
            // a rename replaces the old file in one step; across disks it fails, and the file is copied instead
            let from = stage.join(rel);
            if fs::rename(&from, &target).is_err() {
                fs::copy(&from, &target).map_err(|x| format!("Could not write {}: {}", target.display(), x))?;
            }
            n += 1;
        }
        Ok(n)
    });
    let _ = fs::remove_dir_all(&stage);
    r
}

/// Unpacks the data files of a backup into `stage`; gives their relative paths.
fn unpack_to(stage: &Path, src: &Path) -> Result<Vec<String>, String> {
    let mut rels = Vec::new();
    let mut put = |rel: &str, data: &[u8]| -> Result<(), String> {
        let t = stage.join(rel);
        if let Some(p) = t.parent() {
            fs::create_dir_all(p).map_err(|x| x.to_string())?;
        }
        fs::write(&t, data).map_err(|x| format!("Could not write {}: {}", t.display(), x))?;
        rels.push(rel.to_string());
        Ok(())
    };
    if is_zip(src) {
        let mut z = ZipReader::open(src)?;
        let raw = z.read("manifest.json").map_err(|_| "This is not a Reelhouse backup.".to_string())?;
        let man = Manifest::from_json(&Json::parse(&String::from_utf8_lossy(&raw)).map_err(|_| "The backup file is damaged.".to_string())?)?;
        if man.format > FORMAT {
            return Err("This backup was made by a newer version of Reelhouse.".to_string());
        }
        if !z.has("data/settings.json") {
            return Err("This backup has no settings in it.".to_string());
        }
        for (i, name) in z.names().into_iter().enumerate() {
            let Some(rel) = name.strip_prefix("data/") else { continue };
            if !safe_rel(rel) || rel.ends_with('/') {
                continue;
            }
            let data = z.read_at(i)?;
            put(rel, &data)?;
        }
    } else {
        let bytes = fs::read(src).map_err(|e| format!("Could not read {}: {}", src.display(), e))?;
        let (_, entries) = unpack(&bytes)?;
        if !entries.iter().any(|e| e.path == "settings.json") {
            return Err("This backup has no settings in it.".to_string());
        }
        for e in &entries {
            if safe_rel(&e.path) {
                put(&e.path.replace('\\', "/"), &e.data)?;
            }
        }
    }
    Ok(rels)
}

/// Where automatic backups go: the chosen folder, or `backups` in the data folder.
pub fn backup_dir(data_dir: &Path, folder: &str) -> PathBuf {
    if folder.trim().is_empty() {
        data_dir.join("backups")
    } else {
        PathBuf::from(folder.trim())
    }
}

/// `reelhouse-backup-20261014-2130.rhb` (the time is UTC).
pub fn file_name(now: u64, prefix: &str) -> String {
    let (y, mo, d, h, mi, _) = crate::util::civil(now);
    format!("{}{:04}{:02}{:02}-{:02}{:02}{}", prefix, y, mo, d, h, mi, EXT)
}

/// A name in `dir` not used yet: two backups in the same minute get "-2", "-3"…
pub fn free_name(dir: &Path, now: u64, prefix: &str) -> PathBuf {
    let first = dir.join(file_name(now, prefix));
    if !first.exists() {
        return first;
    }
    let stem = file_name(now, prefix).trim_end_matches(EXT).to_string();
    (2..1000).map(|i| dir.join(format!("{}-{}{}", stem, i, EXT))).find(|p| !p.exists()).unwrap_or(first)
}

/// A backup file of ours (new or old layout).
pub fn is_backup_name(name: &str) -> bool {
    name.ends_with(EXT) || name.ends_with(LEGACY_EXT)
}

/// Keeps the newest `keep` backups starting with one of `prefixes` in `dir` and deletes the older ones. The names
/// hold the date, so sorting them sorts by age (the old "reelhouse-auto-" ones sort first, and they are older).
pub fn prune(dir: &Path, prefixes: &[&str], keep: usize) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut files: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| prefixes.iter().any(|p| n.starts_with(p)) && is_backup_name(n)).collect();
    files.sort();
    let extra = files.len().saturating_sub(keep.max(1));
    for n in files.into_iter().take(extra) {
        let _ = fs::remove_file(dir.join(n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("reelhouse-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn protobuf_round_trip() {
        let e = vec![Entry { path: "settings.json".into(), data: b"{\"a\":1}".to_vec() }, Entry { path: "art/x.jpg".into(), data: [0u8, 255, 7, 128].repeat(400) }];
        let (created, back) = unpack(&pack(&e, 1_700_000_000)).unwrap();
        assert_eq!(created, 1_700_000_000);
        assert_eq!(back, e);
        // The plain form works as well.
        assert_eq!(unpack(&encode(&e, 5)).unwrap().1, e);
    }

    #[test]
    fn unknown_fields_are_skipped_and_foreign_files_refused() {
        let mut raw = encode(&[], 1);
        put_num(&mut raw, 9, 77);
        put_bytes(&mut raw, 10, b"future");
        assert!(decode(&raw).is_ok());
        assert!(decode(b"\x08\x01").is_err());
        assert!(unpack(b"hello").is_err());
        assert!(decode(&[0x22, 0x50]).is_err());
    }

    #[test]
    fn paths_must_stay_inside_the_folder() {
        assert!(safe_rel("settings.json"));
        assert!(safe_rel("art/a.png"));
        assert!(!safe_rel("../x"));
        assert!(!safe_rel("art/../../x"));
        assert!(!safe_rel("/etc/passwd"));
        assert!(!safe_rel("C:/x"));
        assert!(!safe_rel(""));
    }

    #[test]
    fn zip_round_trip() {
        let mut z = ZipWriter::new(Vec::new());
        let text = "hello hello hello hello hello hello hello hello hello hello".repeat(50);
        z.add("a.txt", text.as_bytes(), 1_700_000_000).unwrap();
        z.add("p.webp", &[1u8, 2, 3, 4], 0).unwrap();
        z.add("empty.json", b"", 0).unwrap();
        let bytes = z.finish().unwrap();
        // text is compressed, the picture stored
        assert!(bytes.len() < text.len());
        let d = tmp("zip");
        let p = d.join("t.zip");
        fs::write(&p, &bytes).unwrap();
        let mut r = ZipReader::open(&p).unwrap();
        assert_eq!(r.names(), vec!["a.txt", "p.webp", "empty.json"]);
        assert_eq!(r.read("a.txt").unwrap(), text.as_bytes());
        assert_eq!(r.read("p.webp").unwrap(), vec![1, 2, 3, 4]);
        assert_eq!(r.read("empty.json").unwrap(), Vec::<u8>::new());
        // the EPUB reader (a separate ZIP reader) opens our files too
        let e = crate::epub::Zip::open(&bytes).unwrap();
        assert_eq!(e.read("a.txt").unwrap(), text.as_bytes());
        // a damaged file is refused, not restored half
        let mut bad = bytes.clone();
        // a byte inside the compressed text (after the 30-byte header and the name)
        bad[30 + 5 + 4] ^= 0xFF;
        fs::write(&p, &bad).unwrap();
        assert!(ZipReader::open(&p).and_then(|mut r| r.read("a.txt")).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    /// A data folder like the app's: settings, lists, the study collection and media, a browser history, page
    /// translations, reader edits, extensions, and the caches that must stay out.
    fn fake_data(a: &Path) -> String {
        let w = |p: &str, d: &[u8]| {
            let f = a.join(p);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, d).unwrap();
        };
        w("settings.json", b"{\"x\":1}");
        w("watched.json", b"[\"tt1:1:1\"]");
        w("manga.json", b"{}");
        w("junk.bin", b"nope");
        w("crash-rust.txt", b"log");
        w("art/p.jpg", &[1, 2, 3]);
        w("study/collection.json", b"{\"cards\":[]}");
        w("study/knowledge.md", b"# Knowledge\n");
        w("study/learn.json", b"{}");
        w("study/time.json", b"{}");
        w("study/media/clip.mp4", &[7u8; 50]);
        w("browser/history.json", b"[]");
        w("browser/Cache/data_0", &[0u8; 30]);
        w("translations/Show [abcd1234]/Ch 1 [00000000].md", b"| a | b |");
        w("edits/0123.html", b"<p>x</p>");
        w("extensions/x/manifest.json", b"{}");
        w("torrents/big.mkv", &[9u8; 100]);
        w("mal-cache/1.json", b"{}");
        w("tools/yt-dlp", b"bin");
        w("covers/a.webp", &[4u8; 8]);
        w("downloads-folder/movie.mkv", &[9u8; 10]);
        w("settings.json.tmp", b"half");
        let url = "https://img/x.jpg".to_string();
        w(&image_file(&url), &[5u8; 10]);
        w(&image_file("https://img/other.jpg"), &[6u8; 10]);
        url
    }

    /// The other profiles' folders (`profiles/<id>/…`) and the list of profiles go into a backup like the rest;
    /// their study media only with "Study media".
    #[test]
    fn plan_keeps_the_profiles() {
        let a = tmp("profiles");
        for (p, d) in [("profiles.json", &b"{}"[..]), ("profiles/p1/library.json", b"[]"), ("profiles/p1/study/collection.json", b"{}"), ("profiles/p1/study/media/a.mp3", b"x"), ("profiles/main/avatar-1.png", b"png")] {
            let f = a.join(p);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, d).unwrap();
        }
        let all: Vec<String> = plan(&a, &[], Options { media: true }).into_iter().map(|i| i.rel).collect();
        for want in ["profiles.json", "profiles/p1/library.json", "profiles/p1/study/collection.json", "profiles/p1/study/media/a.mp3", "profiles/main/avatar-1.png"] {
            assert!(all.contains(&want.to_string()), "{} missing from {:?}", want, all);
        }
        let no_media: Vec<String> = plan(&a, &[], Options { media: false }).into_iter().map(|i| i.rel).collect();
        assert!(!no_media.contains(&"profiles/p1/study/media/a.mp3".to_string()) && no_media.contains(&"profiles/p1/study/collection.json".to_string()));
        assert_eq!(section_of("profiles/p1/study/media/a.mp3"), "study media");
        assert_eq!(section_of("profiles/p1/library.json"), "profiles");
        let _ = fs::remove_dir_all(&a);
    }

    #[test]
    fn plan_keeps_user_data_and_leaves_caches_out() {
        let a = tmp("plan");
        let url = fake_data(&a);
        let rels: Vec<String> = plan(&a, &[url.clone()], Options { media: true }).into_iter().map(|i| i.rel).collect();
        for want in ["settings.json", "watched.json", "manga.json", "art/p.jpg", "study/collection.json", "study/knowledge.md", "study/learn.json", "study/time.json", "study/media/clip.mp4", "browser/history.json", "translations/Show [abcd1234]/Ch 1 [00000000].md", "edits/0123.html", "extensions/x/manifest.json", "covers/a.webp"] {
            assert!(rels.contains(&want.to_string()), "{} missing from {:?}", want, rels);
        }
        assert!(rels.contains(&image_file(&url)));
        for not in ["junk.bin", "crash-rust.txt", "browser/Cache/data_0", "torrents/big.mkv", "mal-cache/1.json", "tools/yt-dlp", "downloads-folder/movie.mkv", "settings.json.tmp", &image_file("https://img/other.jpg")] {
            assert!(!rels.contains(&not.to_string()), "{} should be left out", not);
        }
        let no_media: Vec<String> = plan(&a, &[], Options { media: false }).into_iter().map(|i| i.rel).collect();
        assert!(!no_media.iter().any(|r| r.starts_with("study/media")) && no_media.contains(&"study/collection.json".to_string()));
        let _ = fs::remove_dir_all(&a);
    }

    #[test]
    fn backups_leave_the_online_sign_ins_out() {
        let a = tmp("scrub");
        fs::write(a.join("settings.json"), r#"{"general":{"gemini_key":"k"},"backup":{"keep":3,"cloud":{"github_token":"ghp_secret","github_repo":"hp/rb","ms_refresh":"M.R","google_refresh":"","google_client_secret":"s"}}}"#).unwrap();
        fs::create_dir_all(a.join("study")).unwrap();
        fs::write(a.join("study/learn.json"), "{\"xp_total\":120}").unwrap();
        let dest = a.join("out").join(file_name(1_760_000_000, PREFIX));
        write_backup(&a, &dest, &[], 1_760_000_000, Options::default()).unwrap();
        let mut z = ZipReader::open(&dest).unwrap();
        let set = String::from_utf8(z.read("data/settings.json").unwrap()).unwrap();
        assert!(!set.contains("ghp_secret") && !set.contains("M.R") && !set.contains("\"s\""), "{}", set);
        // everything else stays: the keys the user typed, the repository, the study progress
        assert!(set.contains("hp/rb") && set.contains("gemini_key"), "{}", set);
        assert_eq!(z.read("data/study/learn.json").unwrap(), b"{\"xp_total\":120}".to_vec());
        // settings without an online copy are copied byte for byte
        assert_eq!(scrub_settings(b"{\"x\":1}"), b"{\"x\":1}".to_vec());
        let _ = fs::remove_dir_all(&a);
    }

    #[test]
    fn backs_up_and_restores_a_folder() {
        let a = tmp("a");
        let url = fake_data(&a);
        let dest = a.join("backups").join(file_name(1_760_000_000, PREFIX));
        let s = write_backup(&a, &dest, &[url.clone()], 1_760_000_000, Options::default()).unwrap();
        assert_eq!(s.files, 15);
        assert!(s.size > 0 && dest.exists() && !dest.with_extension("tmp").exists());
        let man = read_manifest(&dest).unwrap();
        assert_eq!((man.format, man.created, man.files, man.media), (FORMAT, 1_760_000_000, 15, true));
        assert!(man.sections.iter().any(|(n, f, _)| n == "study" && *f == 4));
        assert!(man.sections.iter().any(|(n, f, _)| n == "study media" && *f == 1));
        assert!(man.sections.iter().any(|(n, _, _)| n == "browser"));
        let b = tmp("b");
        fs::write(b.join("settings.json"), "{\"old\":true}").unwrap();
        assert_eq!(restore(&b, &dest).unwrap(), 15);
        assert_eq!(fs::read_to_string(b.join("settings.json")).unwrap(), "{\"x\":1}");
        assert_eq!(fs::read_to_string(b.join("watched.json")).unwrap(), "[\"tt1:1:1\"]");
        assert_eq!(fs::read(b.join("study/media/clip.mp4")).unwrap(), vec![7u8; 50]);
        assert_eq!(fs::read_to_string(b.join("translations/Show [abcd1234]/Ch 1 [00000000].md")).unwrap(), "| a | b |");
        assert_eq!(fs::read(b.join(image_file(&url))).unwrap(), vec![5u8; 10]);
        assert!(!b.join("torrents").exists() && !b.join("junk.bin").exists());
        // the folder used to unpack is gone
        assert!(fs::read_dir(b.join("backups")).map(|r| r.count()).unwrap_or(0) == 0);
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn old_backups_still_restore_and_bad_ones_change_nothing() {
        let a = tmp("legacy");
        let old = a.join("old.proto.gz");
        fs::write(&old, pack(&[Entry { path: "settings.json".into(), data: b"{\"v\":1}".to_vec() }, Entry { path: "../evil".into(), data: b"x".to_vec() }], 5)).unwrap();
        assert_eq!(read_manifest(&old).unwrap().files, 2);
        let b = tmp("legacy-b");
        assert_eq!(restore(&b, &old).unwrap(), 1);
        assert_eq!(fs::read_to_string(b.join("settings.json")).unwrap(), "{\"v\":1}");
        // a ZIP that is not ours
        let mut z = ZipWriter::new(Vec::new());
        z.add("data/settings.json", b"{}", 0).unwrap();
        fs::write(a.join("foreign.rhb"), z.finish().unwrap()).unwrap();
        assert!(restore(&b, &a.join("foreign.rhb")).is_err());
        assert_eq!(fs::read_to_string(b.join("settings.json")).unwrap(), "{\"v\":1}");
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn names_and_pruning() {
        assert_eq!(file_name(0, PREFIX), "reelhouse-backup-19700101-0000.rhb");
        let d = tmp("prune");
        for n in 1..=5u64 {
            fs::write(d.join(file_name(n * 86_400, PREFIX)), "x").unwrap();
        }
        fs::write(d.join("reelhouse-auto-19691231-000000.proto.gz"), "x").unwrap();
        fs::write(d.join("manual.rhb"), "x").unwrap();
        prune(&d, &[PREFIX, "reelhouse-auto-"], 2);
        let mut left: Vec<String> = fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, vec!["manual.rhb", "reelhouse-backup-19700105-0000.rhb", "reelhouse-backup-19700106-0000.rhb"]);
        // two in the same minute
        let p = free_name(&d, 5 * 86_400, PREFIX);
        assert!(p.ends_with("reelhouse-backup-19700106-0000-2.rhb"));
        let _ = fs::remove_dir_all(&d);
    }
}
