//! Updates that download only what changed (updater.rs uses it).
//!
//! The Windows and Linux releases carry a .zip of the program folder, and an APK is a zip too. In a zip each file is
//! compressed alone and the central directory at the end lists every file with its CRC-32, its size and where its
//! bytes start. GitHub serves parts of a release file (HTTP Range), so:
//! - Program folder (Windows, Linux): `plan_folder` reads the central directory, compares each file with the one
//!   installed (size and CRC-32), and `fetch_folder` downloads and unpacks only the files that differ into a staging
//!   folder, which is then copied over the program folder.
//! - Android: an APK must be installed whole and its signature covers every byte, so `rebuild_zip` puts the new APK
//!   together byte for byte: files that did not change are copied (raw, still compressed) from the APK installed now,
//!   the rest is downloaded, and the result is checked against the SHA-256 GitHub publishes for the file. When
//!   anything does not match, the caller downloads the whole file instead.
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Reads `len` bytes of the remote file from `from` (a Range request).
pub type Fetch<'a> = dyn Fn(u64, u64) -> Result<Vec<u8>, String> + 'a;

/// One file of a zip, from its central directory.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub version_needed: u16,
    pub time: u16,
    pub date: u16,
    pub crc: u32,
    pub csize: u64,
    pub usize: u64,
    /// Where the local header of the file starts.
    pub offset: u64,
    /// Unix permissions stored by the zip tool (0 when none).
    pub mode: u32,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

/// The central directory of a zip: its files (in the order of their bytes) and where the directory starts.
#[derive(Clone, Debug)]
pub struct Directory {
    pub entries: Vec<Entry>,
    /// Where the central directory starts (the end of the last file's bytes, or of the APK signing block).
    pub cd_offset: u64,
    pub size: u64,
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}
fn u64_at(b: &[u8], i: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[i..i + 8]);
    u64::from_le_bytes(a)
}

/// Reads the central directory of a zip of `size` bytes.
pub fn read_directory(fetch: &Fetch, size: u64) -> Result<Directory, String> {
    let bad = |w: &str| format!("The update file is not a zip this version understands ({}).", w);
    if size < 22 {
        return Err(bad("too small"));
    }
    // the end record is in the last 64 KiB + 22 bytes (its comment is at most 65535 bytes)
    let tail_len = size.min(65_557 + 20);
    let tail = fetch(size - tail_len, tail_len)?;
    let at = (0..=tail.len().saturating_sub(22)).rev().find(|&i| u32_at(&tail, i) == 0x0605_4b50).ok_or_else(|| bad("no end record"))?;
    let mut count = u16_at(&tail, at + 10) as u64;
    let mut cd_size = u32_at(&tail, at + 12) as u64;
    let mut cd_offset = u32_at(&tail, at + 16) as u64;
    if count == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_offset == 0xFFFF_FFFF {
        // zip64: the locator sits right before the end record
        if at < 20 || u32_at(&tail, at - 20) != 0x0706_4b50 {
            return Err(bad("zip64 without locator"));
        }
        let rec_at = u64_at(&tail, at - 20 + 8);
        let rec = fetch(rec_at, 56)?;
        if rec.len() < 56 || u32_at(&rec, 0) != 0x0606_4b50 {
            return Err(bad("zip64 record"));
        }
        count = u64_at(&rec, 32);
        cd_size = u64_at(&rec, 40);
        cd_offset = u64_at(&rec, 48);
    }
    if cd_offset + cd_size > size {
        return Err(bad("directory out of the file"));
    }
    let cd = fetch(cd_offset, cd_size)?;
    if (cd.len() as u64) < cd_size {
        return Err(bad("short directory"));
    }
    let mut entries = Vec::with_capacity(count as usize);
    let mut i = 0usize;
    while i + 46 <= cd.len() && u32_at(&cd, i) == 0x0201_4b50 {
        let n = u16_at(&cd, i + 28) as usize;
        let x = u16_at(&cd, i + 30) as usize;
        let c = u16_at(&cd, i + 32) as usize;
        if i + 46 + n + x + c > cd.len() {
            return Err(bad("cut entry"));
        }
        let name = String::from_utf8_lossy(&cd[i + 46..i + 46 + n]).replace('\\', "/");
        let mut e = Entry {
            name,
            version_needed: u16_at(&cd, i + 6),
            flags: u16_at(&cd, i + 8),
            method: u16_at(&cd, i + 10),
            time: u16_at(&cd, i + 12),
            date: u16_at(&cd, i + 14),
            crc: u32_at(&cd, i + 16),
            csize: u32_at(&cd, i + 20) as u64,
            usize: u32_at(&cd, i + 24) as u64,
            offset: u32_at(&cd, i + 42) as u64,
            mode: if u16_at(&cd, i + 4) >> 8 == 3 { (u32_at(&cd, i + 38) >> 16) & 0o7777 } else { 0 },
        };
        // zip64 sizes and offset, in that order, for the fields that are 0xFFFFFFFF
        let extra = &cd[i + 46 + n..i + 46 + n + x];
        let mut j = 0;
        while j + 4 <= extra.len() {
            let (id, len) = (u16_at(extra, j), u16_at(extra, j + 2) as usize);
            if j + 4 + len > extra.len() {
                break;
            }
            if id == 1 {
                let mut k = j + 4;
                for f in [&mut e.usize, &mut e.csize, &mut e.offset] {
                    if *f == 0xFFFF_FFFF && k + 8 <= j + 4 + len {
                        *f = u64_at(extra, k);
                        k += 8;
                    }
                }
            }
            j += 4 + len;
        }
        entries.push(e);
        i += 46 + n + x + c;
    }
    if entries.len() as u64 != count {
        return Err(bad("entry count"));
    }
    entries.sort_by_key(|e| e.offset);
    Ok(Directory { entries, cd_offset, size })
}

/// The CRC-32 and size of a file on disk (None when it cannot be read).
pub fn file_crc(path: &Path) -> Option<(u32, u64)> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut crc = flate2::Crc::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
    }
    Some((crc.sum(), crc.amount() as u64))
}

/// Where each file's bytes end: the next file's header, or the central directory (or APK signing block) for the last.
fn ends(dir: &Directory) -> Vec<u64> {
    let mut v: Vec<u64> = dir.entries.iter().skip(1).map(|e| e.offset).collect();
    v.push(dir.cd_offset);
    v
}

/// Byte ranges to download, with ranges closer than `gap` merged (fewer requests for a few extra bytes).
fn merge(mut ranges: Vec<(u64, u64)>, gap: u64) -> Vec<(u64, u64)> {
    ranges.sort();
    let mut out: Vec<(u64, u64)> = vec![];
    for (a, b) in ranges {
        match out.last_mut() {
            Some(l) if a <= l.1 + gap => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// What a folder update needs.
#[derive(Clone, Debug)]
pub struct FolderPlan {
    /// The files that differ (index into the directory), with where their bytes end.
    pub changed: Vec<(usize, u64)>,
    /// Bytes to download (after merging close ranges).
    pub bytes: u64,
    pub ranges: Vec<(u64, u64)>,
    /// The folder inside the zip that matches the program folder ("" or "ReelHouse/").
    pub prefix: String,
}

/// Compares the zip with the program folder `install`. `exe_rel` is the program inside the folder, as a zip path
/// ("reelhouse-qt.exe", "bin/reelhouse-qt"): it finds the zip's own top folder.
pub fn plan_folder(dir: &Directory, install: &Path, exe_rel: &str) -> Result<FolderPlan, String> {
    let exe = dir.entries.iter().find(|e| e.name == exe_rel || e.name.ends_with(&format!("/{}", exe_rel))).ok_or_else(|| format!("The update has no {}.", exe_rel))?;
    let prefix = exe.name[..exe.name.len() - exe_rel.len()].to_string();
    let ends = ends(dir);
    let mut changed = vec![];
    for (i, e) in dir.entries.iter().enumerate() {
        if e.is_dir() || !e.name.starts_with(&prefix) {
            continue;
        }
        let rel = &e.name[prefix.len()..];
        if rel.is_empty() || rel.split('/').any(|p| p == ".." || p.contains(':')) {
            continue;
        }
        let same = match std::fs::metadata(install.join(rel)) {
            Ok(m) if m.len() == e.usize => file_crc(&install.join(rel)).map(|(c, _)| c == e.crc).unwrap_or(false),
            _ => false,
        };
        if !same {
            changed.push((i, ends[i]));
        }
    }
    // close ranges merged; a wider gap when there are many, so one update stays a few hundred requests at most
    let wanted: Vec<(u64, u64)> = changed.iter().map(|&(i, end)| (dir.entries[i].offset, end)).collect();
    let mut gap = 64 * 1024;
    let mut ranges = merge(wanted.clone(), gap);
    while ranges.len() > 200 {
        gap *= 4;
        ranges = merge(wanted.clone(), gap);
    }
    let bytes = ranges.iter().map(|r| r.1 - r.0).sum();
    Ok(FolderPlan { changed, bytes, ranges, prefix })
}

/// Unpacks one file from its local header + data (`raw` starts at the header).
fn unpack(e: &Entry, raw: &[u8]) -> Result<Vec<u8>, String> {
    let bad = || format!("The update's copy of {} is damaged.", e.name);
    if raw.len() < 30 || u32_at(raw, 0) != 0x0403_4b50 {
        return Err(bad());
    }
    let start = 30 + u16_at(raw, 26) as usize + u16_at(raw, 28) as usize;
    let end = start + e.csize as usize;
    if end > raw.len() {
        return Err(bad());
    }
    let data = &raw[start..end];
    let out = match e.method {
        0 => data.to_vec(),
        8 => {
            let mut v = Vec::with_capacity(e.usize as usize);
            flate2::read::DeflateDecoder::new(data).read_to_end(&mut v).map_err(|_| bad())?;
            v
        }
        m => return Err(format!("{} uses a compression this version cannot unpack ({}).", e.name, m)),
    };
    let mut crc = flate2::Crc::new();
    crc.update(&out);
    if crc.sum() != e.crc || out.len() as u64 != e.usize {
        return Err(bad());
    }
    Ok(out)
}

/// Downloads the changed files of `plan` into `stage` (same layout as the program folder). `progress(done, total)`.
pub fn fetch_folder(fetch: &Fetch, dir: &Directory, plan: &FolderPlan, stage: &Path, install: Option<&Path>, progress: &dyn Fn(u64, u64), go_on: &dyn Fn() -> bool) -> Result<usize, String> {
    let _ = std::fs::remove_dir_all(stage);
    std::fs::create_dir_all(stage).map_err(|e| format!("Could not make {}: {}", stage.display(), e))?;
    let mut done = 0u64;
    let mut written = 0usize;
    for &(a, b) in &plan.ranges {
        if !go_on() {
            return Err("The download was cancelled.".into());
        }
        let chunk = fetch(a, b - a)?;
        if (chunk.len() as u64) < b - a {
            return Err("GitHub sent less than asked for. Try again.".into());
        }
        for &(i, end) in plan.changed.iter().filter(|(i, end)| dir.entries[*i].offset >= a && *end <= b) {
            let e = &dir.entries[i];
            let raw = &chunk[(e.offset - a) as usize..(end - a) as usize];
            let data = unpack(e, raw)?;
            let dest = stage.join(&e.name[plan.prefix.len()..]);
            if let Some(d) = dest.parent() {
                std::fs::create_dir_all(d).map_err(|er| format!("Could not make {}: {}", d.display(), er))?;
            }
            std::fs::write(&dest, &data).map_err(|er| format!("Could not write {}: {}", dest.display(), er))?;
            #[cfg(unix)]
            {
                // programs stay programs: the zip's permissions, else those of the installed file
                use std::os::unix::fs::PermissionsExt;
                let rel = &e.name[plan.prefix.len()..];
                let mode = if e.mode & 0o777 != 0 { Some(e.mode & 0o777) } else { install.and_then(|i| std::fs::metadata(i.join(rel)).ok()).map(|m| m.permissions().mode() & 0o777) };
                if let Some(m) = mode {
                    let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(m | 0o644));
                }
            }
            written += 1;
        }
        done += b - a;
        progress(done, plan.bytes);
    }
    Ok(written)
}

/// Reads part of a local file.
fn read_at(f: &mut std::fs::File, from: u64, len: u64) -> Result<Vec<u8>, String> {
    use std::io::Seek;
    f.seek(std::io::SeekFrom::Start(from)).map_err(|e| e.to_string())?;
    let mut v = vec![0u8; len as usize];
    f.read_exact(&mut v).map_err(|e| e.to_string())?;
    Ok(v)
}

/// The SHA-256 of a file, as lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = sha2::Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{:02x}", b)).collect())
}

/// What a rebuild needs: how many bytes come from the network.
#[derive(Clone, Debug)]
pub struct RebuildPlan {
    pub ranges: Vec<(u64, u64)>,
    pub bytes: u64,
    /// Files copied from the old zip: new index -> old entry.
    pub reuse: HashMap<usize, Entry>,
}

/// Plans putting the new zip (`new`) together from the old one (`old`): a file whose name, CRC, sizes and
/// compression are the same is copied from the old zip; everything else, and the bytes after the last file (APK
/// signing block, central directory), is downloaded.
pub fn plan_rebuild(new: &Directory, old: &Directory) -> RebuildPlan {
    let by_name: HashMap<&str, &Entry> = old.entries.iter().map(|e| (e.name.as_str(), e)).collect();
    let ends = ends(new);
    let mut reuse = HashMap::new();
    let mut ranges = vec![];
    for (i, e) in new.entries.iter().enumerate() {
        let same = by_name.get(e.name.as_str()).filter(|o| o.crc == e.crc && o.csize == e.csize && o.usize == e.usize && o.method == e.method && o.flags & 8 == e.flags & 8);
        match same {
            // tiny files cost more as a header guess than as bytes: download them with their neighbours
            Some(o) if e.csize >= 4096 => {
                reuse.insert(i, (*o).clone());
            }
            _ => ranges.push((e.offset, ends[i])),
        }
    }
    let first = new.entries.first().map(|e| e.offset).unwrap_or(new.cd_offset);
    if first > 0 {
        ranges.push((0, first));
    }
    ranges.push((new.cd_offset, new.size));
    let ranges = merge(ranges, 32 * 1024);
    // a reused file that falls inside a downloaded range is simply downloaded
    reuse.retain(|i, _| !ranges.iter().any(|r| new.entries[*i].offset >= r.0 && ends[*i] <= r.1));
    let bytes = ranges.iter().map(|r| r.1 - r.0).sum();
    RebuildPlan { ranges, bytes, reuse }
}

/// Writes the new zip to `dest`: downloaded ranges as they are, reused files as a local header (made from the new
/// central directory, with the old header's extra field when it has the length the new layout leaves for it, else
/// zeros) followed by the old raw bytes. The caller checks the SHA-256 afterwards.
pub fn rebuild_zip(fetch: &Fetch, new: &Directory, old_path: &Path, plan: &RebuildPlan, dest: &Path, progress: &dyn Fn(u64, u64), go_on: &dyn Fn() -> bool) -> Result<(), String> {
    let ends = ends(new);
    let mut old = std::fs::File::open(old_path).map_err(|e| format!("Could not read {}: {}", old_path.display(), e))?;
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("Could not make {}: {}", d.display(), e))?;
    }
    let mut out = std::io::BufWriter::new(std::fs::File::create(dest).map_err(|e| format!("Could not write {}: {}", dest.display(), e))?);
    // everything in file order: downloaded ranges and reused files
    let mut parts: Vec<(u64, u64, Option<usize>)> = plan.ranges.iter().map(|r| (r.0, r.1, None)).collect();
    for i in plan.reuse.keys() {
        parts.push((new.entries[*i].offset, ends[*i], Some(*i)));
    }
    parts.sort_by_key(|p| p.0);
    let mut pos = 0u64;
    let mut done = 0u64;
    for (a, b, reused) in parts {
        if !go_on() {
            return Err("The download was cancelled.".into());
        }
        if a != pos {
            return Err("The update could not be put together (a gap in the file).".into());
        }
        match reused {
            None => {
                let chunk = fetch(a, b - a)?;
                if (chunk.len() as u64) != b - a {
                    return Err("GitHub sent less than asked for. Try again.".into());
                }
                out.write_all(&chunk).map_err(|e| e.to_string())?;
                done += b - a;
                progress(done, plan.bytes);
            }
            Some(i) => {
                let e = &new.entries[i];
                let o = &plan.reuse[&i];
                let descriptor = if e.flags & 8 != 0 { 16 } else { 0 };
                let room = (b - a).checked_sub(30 + e.name.len() as u64 + e.csize + descriptor).ok_or("The update could not be put together (sizes).")?;
                let oh = read_at(&mut old, o.offset, 30)?;
                if u32_at(&oh, 0) != 0x0403_4b50 {
                    return Err("The installed copy is damaged.".into());
                }
                let (on, ox) = (u16_at(&oh, 26) as u64, u16_at(&oh, 28) as u64);
                let extra = if ox == room { read_at(&mut old, o.offset + 30 + on, ox)? } else { vec![0u8; room as usize] };
                let mut h = Vec::with_capacity(30 + e.name.len() + extra.len());
                h.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
                h.extend_from_slice(&e.version_needed.to_le_bytes());
                h.extend_from_slice(&e.flags.to_le_bytes());
                h.extend_from_slice(&e.method.to_le_bytes());
                h.extend_from_slice(&e.time.to_le_bytes());
                h.extend_from_slice(&e.date.to_le_bytes());
                let (crc, cs, us) = if e.flags & 8 != 0 { (0, 0, 0) } else { (e.crc, e.csize as u32, e.usize as u32) };
                h.extend_from_slice(&crc.to_le_bytes());
                h.extend_from_slice(&cs.to_le_bytes());
                h.extend_from_slice(&us.to_le_bytes());
                h.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
                h.extend_from_slice(&(room as u16).to_le_bytes());
                h.extend_from_slice(e.name.as_bytes());
                h.extend_from_slice(&extra);
                out.write_all(&h).map_err(|er| er.to_string())?;
                let data = read_at(&mut old, o.offset + 30 + on + ox, e.csize + descriptor)?;
                out.write_all(&data).map_err(|er| er.to_string())?;
            }
        }
        pos = b;
    }
    out.flush().map_err(|e| e.to_string())?;
    if pos != new.size {
        return Err("The update could not be put together (short file).".into());
    }
    Ok(())
}

/// Reads a local zip's central directory (the APK installed now).
pub fn read_local_directory(path: &Path) -> Result<Directory, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("Could not read {}: {}", path.display(), e))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let f = std::cell::RefCell::new(&mut f);
    read_directory(&|a, n| read_at(&mut f.borrow_mut(), a, n), size)
}

/// The APK of this app on Android: the runtime maps it, so it shows in /proc/self/maps.
pub fn installed_apk() -> Option<PathBuf> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps.lines().filter_map(|l| l.split_whitespace().nth(5)).find(|p| p.ends_with("/base.apk") && p.contains("org.reelhouse.app")).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny zip writer for the tests: (name, data, deflate?, extra length of the local header).
    fn zip(files: &[(&str, &[u8], bool, usize)]) -> Vec<u8> {
        let mut out = vec![];
        let mut cd = vec![];
        for (name, data, deflate, xl) in files {
            let mut c = flate2::Crc::new();
            c.update(data);
            let comp = if *deflate {
                let mut e = flate2::write::DeflateEncoder::new(vec![], flate2::Compression::default());
                e.write_all(data).unwrap();
                e.finish().unwrap()
            } else {
                data.to_vec()
            };
            let off = out.len() as u32;
            let m: u16 = if *deflate { 8 } else { 0 };
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&m.to_le_bytes());
            out.extend_from_slice(&[0, 0, 0x21, 0]);
            out.extend_from_slice(&c.sum().to_le_bytes());
            out.extend_from_slice(&(comp.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&(*xl as u16).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend(std::iter::repeat_n(0u8, *xl));
            out.extend_from_slice(&comp);
            cd.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&m.to_le_bytes());
            cd.extend_from_slice(&[0, 0, 0x21, 0]);
            cd.extend_from_slice(&c.sum().to_le_bytes());
            cd.extend_from_slice(&(comp.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0u8; 8]);
            cd.extend_from_slice(&[0u8; 4]);
            cd.extend_from_slice(&off.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_off = out.len() as u32;
        out.extend_from_slice(&cd);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn fetcher(z: &[u8]) -> impl Fn(u64, u64) -> Result<Vec<u8>, String> + '_ {
        move |a, n| Ok(z[a as usize..(a + n) as usize].to_vec())
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rh-delta-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn folder_downloads_only_changed_files() {
        let big = vec![7u8; 200_000];
        let z = zip(&[("ReelHouse/reelhouse-qt.exe", b"new exe", true, 0), ("ReelHouse/lib/a.dll", &big, true, 0), ("ReelHouse/lib/", b"", false, 0), ("ReelHouse/new.txt", b"hello", false, 0)]);
        let f = fetcher(&z);
        let dir = read_directory(&f, z.len() as u64).unwrap();
        assert_eq!(dir.entries.len(), 4);
        let install = tmp("install");
        std::fs::create_dir_all(install.join("lib")).unwrap();
        std::fs::write(install.join("reelhouse-qt.exe"), b"old exe").unwrap();
        std::fs::write(install.join("lib/a.dll"), &big).unwrap();
        let plan = plan_folder(&dir, &install, "reelhouse-qt.exe").unwrap();
        assert_eq!(plan.prefix, "ReelHouse/");
        let names: Vec<&str> = plan.changed.iter().map(|(i, _)| dir.entries[*i].name.as_str()).collect();
        assert_eq!(names, vec!["ReelHouse/reelhouse-qt.exe", "ReelHouse/new.txt"]);
        assert!(plan.bytes < 2000, "{}", plan.bytes);
        let stage = install.join("..").join(format!("rh-stage-{}", std::process::id()));
        let n = fetch_folder(&f, &dir, &plan, &stage, Some(&install), &|_, _| {}, &|| true).unwrap();
        assert_eq!(n, 2);
        assert_eq!(std::fs::read(stage.join("reelhouse-qt.exe")).unwrap(), b"new exe");
        assert_eq!(std::fs::read(stage.join("new.txt")).unwrap(), b"hello");
        assert!(!stage.join("lib/a.dll").exists());
        let _ = std::fs::remove_dir_all(&stage);
        let _ = std::fs::remove_dir_all(&install);
    }

    #[test]
    fn rebuild_is_byte_identical() {
        let lib_old: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let lib_new: Vec<u8> = (0..300_000u32).map(|i| (i % 253) as u8).collect();
        let res: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 256) as u8).collect();
        let old = zip(&[("AndroidManifest.xml", b"m1", true, 0), ("assets/res.bin", &res, false, 3), ("lib/arm64-v8a/libapp.so", &lib_old, true, 0)]);
        let new = zip(&[("AndroidManifest.xml", b"m2", true, 0), ("lib/arm64-v8a/libapp.so", &lib_new, true, 0), ("assets/res.bin", &res, false, 1)]);
        let d = tmp("apk");
        let old_path = d.join("base.apk");
        std::fs::write(&old_path, &old).unwrap();
        let f = fetcher(&new);
        let nd = read_directory(&f, new.len() as u64).unwrap();
        let od = read_local_directory(&old_path).unwrap();
        let plan = plan_rebuild(&nd, &od);
        assert_eq!(plan.reuse.len(), 1);
        assert!(plan.bytes < new.len() as u64 - 90_000);
        let dest = d.join("new.apk");
        rebuild_zip(&f, &nd, &old_path, &plan, &dest, &|_, _| {}, &|| true).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), new);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sha256_known() {
        let d = tmp("sha");
        std::fs::write(d.join("x"), b"abc").unwrap();
        assert_eq!(sha256_file(&d.join("x")).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_dir_all(&d);
    }
}
