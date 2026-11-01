//! The interface sounds of controller mode (UiSounds.qml): the navigation blip, the select sound and the background
//! music. No recordings ship with the app; each slot plays a file of the person's own, kept in `<data>/sounds/` as
//! `nav.<ext>`, `select.<ext>` and `music.<ext>`, and saved in the settings as `general.sound_<slot>` relative to the
//! data folder ("sounds/music.mp3").
//!
//! - [`adopt`]: at start, an empty slot (or one whose file is gone) takes the file of that name found in
//!   `<data>/sounds/`, so copying `nav.mp3`, `select.mp3` and `music.mp3` there is enough.
//! - [`import_pack`]: Preferences > Controller > Sounds, "Import sound pack…": a `.zip` or a folder holding files
//!   named `nav.*`, `select.*`, `music.*` (anywhere inside) is copied into `<data>/sounds/`.
//!
//! Standard library only (the zip is read by `epub::Zip`, stored or deflated entries).
use crate::epub::Zip;
use crate::settings::General;
use std::path::{Path, PathBuf};

/// The slots, in the order the settings list them.
pub const SLOTS: [&str; 3] = ["nav", "select", "music"];

/// The audio formats Qt Multimedia plays here (FFmpeg backend).
const EXTS: [&str; 9] = ["mp3", "ogg", "wav", "flac", "m4a", "opus", "aac", "wma", "oga"];

fn is_audio(ext: &str) -> bool {
    EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

/// The slot a file name stands for: "Music.MP3" -> ("music", "mp3"); None for anything else.
fn slot_of(file_name: &str) -> Option<(&'static str, String)> {
    let (stem, ext) = file_name.rsplit_once('.')?;
    if !is_audio(ext) {
        return None;
    }
    let stem = stem.trim().to_ascii_lowercase();
    let slot = SLOTS.iter().find(|s| **s == stem)?;
    Some((slot, ext.to_ascii_lowercase()))
}

fn sounds_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("sounds")
}

/// The file of a slot in `<data>/sounds/` ("sounds/music.mp3"), if there is one.
pub fn found(data_dir: &Path, slot: &str) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(sounds_dir(data_dir)).ok()?.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
    names.sort();
    names.into_iter().find(|n| slot_of(n).map(|(s, _)| s == slot).unwrap_or(false)).map(|n| format!("sounds/{}", n))
}

fn get<'a>(g: &'a mut General, slot: &str) -> &'a mut String {
    match slot {
        "nav" => &mut g.sound_nav,
        "select" => &mut g.sound_select,
        _ => &mut g.sound_music,
    }
}

/// Fills the empty slots (and those whose file is missing) with `<data>/sounds/<slot>.*`. Returns what changed as
/// (slot, new value), so the caller saves the settings and logs it.
pub fn adopt(g: &mut General, data_dir: &Path) -> Vec<(&'static str, String)> {
    let mut changed = Vec::new();
    for slot in SLOTS {
        let cur = get(g, slot).clone();
        let gone = !cur.is_empty() && !data_dir.join(&cur).is_file();
        if !cur.is_empty() && !gone {
            continue;
        }
        if let Some(rel) = found(data_dir, slot) {
            if rel != cur {
                *get(g, slot) = rel.clone();
                changed.push((slot, rel));
            }
        }
    }
    changed
}

/// Removes the old files of a slot, so `music.mp3` does not stay beside a new `music.ogg`.
fn clear_slot(dir: &Path, slot: &str) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if slot_of(&e.file_name().to_string_lossy()).map(|(s, _)| s == slot).unwrap_or(false) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Writes one slot's bytes as `<data>/sounds/<slot>.<ext>`; returns "sounds/<slot>.<ext>".
fn store(data_dir: &Path, slot: &str, ext: &str, bytes: &[u8]) -> Result<String, String> {
    let dir = sounds_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not make {}: {}", dir.display(), e))?;
    clear_slot(&dir, slot);
    let name = format!("{}.{}", slot, ext);
    std::fs::write(dir.join(&name), bytes).map_err(|e| format!("Could not write {}: {}", name, e))?;
    Ok(format!("sounds/{}", name))
}

/// One file picked for a slot (Preferences > Controller > Sounds, "Choose…"): copied as `<data>/sounds/<slot>.<ext>`.
/// Read before the old file is removed, so picking the file already in the sounds folder keeps it.
pub fn set_file(data_dir: &Path, slot: &str, src: &Path) -> Result<String, String> {
    let bytes = std::fs::read(src).map_err(|e| format!("Could not read {}: {}", src.display(), e))?;
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    store(data_dir, slot, if ext.is_empty() { "wav" } else { &ext }, &bytes)
}

/// Removes the files of a slot ("Clear").
pub fn clear(data_dir: &Path, slot: &str) {
    clear_slot(&sounds_dir(data_dir), slot);
}

/// The files of a folder (and its subfolders, four levels down) that name a slot; the first of each slot wins.
fn from_folder(dir: &Path, depth: u32, out: &mut Vec<(&'static str, String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries.iter().filter(|p| p.is_file()) {
        if let Some((slot, ext)) = p.file_name().and_then(|n| slot_of(&n.to_string_lossy())) {
            if !out.iter().any(|(s, _, _)| *s == slot) {
                out.push((slot, ext, p.clone()));
            }
        }
    }
    if depth > 0 {
        for p in entries.iter().filter(|p| p.is_dir()) {
            from_folder(p, depth - 1, out);
        }
    }
}

/// Imports a sound pack: a `.zip` file or a folder with `nav.*`, `select.*` and/or `music.*` in it. The files go to
/// `<data>/sounds/`; returns the slots set as (slot, "sounds/<slot>.<ext>"), at least one, or an error to show.
pub fn import_pack(src: &Path, data_dir: &Path) -> Result<Vec<(&'static str, String)>, String> {
    let none = || "No nav, select or music audio file was found there (names like nav.mp3, select.mp3, music.mp3).".to_string();
    let mut done = Vec::new();
    if src.is_dir() {
        let mut files = Vec::new();
        from_folder(src, 4, &mut files);
        if files.is_empty() {
            return Err(none());
        }
        for (slot, ext, path) in files {
            // the pack may be <data>/sounds itself: read before the old files are removed
            let bytes = std::fs::read(&path).map_err(|e| format!("Could not read {}: {}", path.display(), e))?;
            done.push((slot, store(data_dir, slot, &ext, &bytes)?));
        }
        return Ok(done);
    }
    let data = std::fs::read(src).map_err(|e| format!("Could not read {}: {}", src.display(), e))?;
    if let Some((slot, ext)) = src.file_name().and_then(|n| slot_of(&n.to_string_lossy())) {
        // a single file named like a slot
        done.push((slot, store(data_dir, slot, &ext, &data)?));
        return Ok(done);
    }
    let zip = Zip::open(&data).map_err(|_| "This file is not a zip file or a folder.".to_string())?;
    let mut names = zip.names();
    names.sort();
    for name in names {
        // folders inside the zip are fine; macOS's resource copies (__MACOSX/._music.mp3) are not sounds
        if name.ends_with('/') || name.contains("__MACOSX") {
            continue;
        }
        let base = name.rsplit('/').next().unwrap_or(&name);
        if base.starts_with("._") {
            continue;
        }
        let Some((slot, ext)) = slot_of(base) else { continue };
        if done.iter().any(|(s, _)| *s == slot) {
            continue;
        }
        let bytes = zip.read(&name)?;
        if bytes.is_empty() {
            continue;
        }
        done.push((slot, store(data_dir, slot, &ext, &bytes)?));
    }
    if done.is_empty() {
        return Err(none());
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rh-uisounds-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A zip with stored (uncompressed) entries, like the one HP gets.
    fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        fn crc(d: &[u8]) -> u32 {
            let mut c = 0xffff_ffffu32;
            for &b in d {
                c ^= b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
                }
            }
            !c
        }
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for (name, data) in files {
            let off = out.len() as u32;
            let (c, n, l) = (crc(data), name.len() as u16, data.len() as u32);
            out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&c.to_le_bytes());
            out.extend_from_slice(&l.to_le_bytes());
            out.extend_from_slice(&l.to_le_bytes());
            out.extend_from_slice(&n.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&c.to_le_bytes());
            central.extend_from_slice(&l.to_le_bytes());
            central.extend_from_slice(&l.to_le_bytes());
            central.extend_from_slice(&n.to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&off.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let (cd_off, cd_len, count) = (out.len() as u32, central.len() as u32, files.len() as u16);
        out.extend_from_slice(&central);
        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_len.to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn names() {
        assert_eq!(slot_of("Music.MP3"), Some(("music", "mp3".to_string())));
        assert_eq!(slot_of("nav.ogg"), Some(("nav", "ogg".to_string())));
        assert_eq!(slot_of("nav.txt"), None);
        assert_eq!(slot_of("navigation.mp3"), None);
    }

    #[test]
    fn zip_pack_and_adopt() {
        let data = tmp("zip");
        let z = stored_zip(&[("pack/music.mp3", b"MUSIC"), ("pack/nav.mp3", b"NAV"), ("__MACOSX/pack/._select.mp3", b"junk"), ("pack/select.mp3", b"SEL"), ("readme.txt", b"hi")]);
        let zp = data.join("pack.zip");
        std::fs::write(&zp, z).unwrap();
        let got = import_pack(&zp, &data).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(std::fs::read(data.join("sounds/select.mp3")).unwrap(), b"SEL");
        assert_eq!(std::fs::read(data.join("sounds/music.mp3")).unwrap(), b"MUSIC");
        // a new music in another format replaces the old file
        std::fs::write(data.join("m.zip"), stored_zip(&[("music.ogg", b"OGG")])).unwrap();
        assert_eq!(import_pack(&data.join("m.zip"), &data).unwrap(), vec![("music", "sounds/music.ogg".to_string())]);
        assert!(!data.join("sounds/music.mp3").exists());
        // adopt: empty and missing slots take the files found
        let mut g = crate::settings::Settings::default().general;
        g.sound_select = "sounds/gone.wav".into();
        let ch = adopt(&mut g, &data);
        assert_eq!(ch.len(), 3);
        assert_eq!(g.sound_nav, "sounds/nav.mp3");
        assert_eq!(g.sound_select, "sounds/select.mp3");
        assert_eq!(g.sound_music, "sounds/music.ogg");
        assert!(adopt(&mut g, &data).is_empty());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn folder_pack_and_errors() {
        let data = tmp("dir");
        let src = data.join("src/inner");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("NAV.wav"), b"N").unwrap();
        let got = import_pack(&data.join("src"), &data).unwrap();
        assert_eq!(got, vec![("nav", "sounds/nav.wav".to_string())]);
        // the sounds folder itself can be imported again
        assert_eq!(import_pack(&data.join("sounds"), &data).unwrap().len(), 1);
        assert_eq!(std::fs::read(data.join("sounds/nav.wav")).unwrap(), b"N");
        std::fs::write(data.join("x.zip"), b"not a zip").unwrap();
        assert!(import_pack(&data.join("x.zip"), &data).is_err());
        std::fs::create_dir_all(data.join("empty")).unwrap();
        assert!(import_pack(&data.join("empty"), &data).is_err());
        let _ = std::fs::remove_dir_all(&data);
    }

    /// The pack HP gets (entregas/sons-ps2.zip), when this machine has it. It is never part of the repository.
    #[test]
    fn hp_pack_if_present() {
        let zp = Path::new("/mnt/project-files/entregas/sons-ps2.zip");
        if !zp.is_file() {
            return;
        }
        let data = tmp("hp");
        let got = import_pack(zp, &data).unwrap();
        let mut slots: Vec<&str> = got.iter().map(|(s, _)| *s).collect();
        slots.sort();
        assert_eq!(slots, vec!["music", "nav", "select"]);
        assert!(std::fs::metadata(data.join("sounds/music.mp3")).unwrap().len() > 1_000_000);
        let _ = std::fs::remove_dir_all(&data);
    }
}
