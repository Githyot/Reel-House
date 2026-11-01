//! Local profiles: several people sharing one computer, each with their own library, favourites, history (watched,
//! continue watching, what was read), study (decks, cards, Learn, study time) and the books they added. A profile is
//! only a name and a picture: one of the pictures that come with the app, or a photo of the person's own.
//!
//! The list lives in `<data>/profiles.json`. The first profile ("main") keeps reading the files at the top of the data
//! folder, exactly where they were before profiles existed, so nothing moves and nothing can be lost. Every other
//! profile has its own folder, `<data>/profiles/<id>/`, holding the same file names. What is not personal (the
//! preferences, addons, keys, downloaded engines and caches) stays at the top and is shared by everyone.
//!
//! The pictures chosen from the computer are kept as `avatar-<time>.png` in `<data>/profiles/<id>/` (the main profile
//! too: its folder then holds only its pictures), so a backup keeps them with the rest of the data folder.
use crate::json::Json;
use std::path::{Path, PathBuf};

/// The list of profiles and which one is in use.
pub const FILE: &str = "profiles.json";
/// The folder of the profiles' own data.
pub const FOLDER: &str = "profiles";
/// The first profile: the data that was there before profiles existed.
pub const MAIN: &str = "main";
/// How many ready-made pictures the app draws (ProfileAvatar.qml): avatars "preset:0" .. "preset:11".
pub const PRESETS: u32 = 12;
/// More than this many people on one computer would make the list hard to use.
pub const MAX_PROFILES: usize = 12;
const MAX_NAME: usize = 40;
/// A picture the window made is 256 x 256 px; anything much bigger is not one of them.
const MAX_PHOTO: u64 = 8 * 1024 * 1024;
const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub id: String,
    /// As typed; "" for the main profile until it is named (the window shows a translated "Main profile").
    pub name: String,
    /// "preset:<n>" or "file:<name>.png" (a picture in the profile's own folder).
    pub avatar: String,
    /// When it was made (unix seconds; 0 for the main profile).
    pub created: u64,
}

impl Profile {
    fn main() -> Profile {
        Profile { id: MAIN.into(), name: String::new(), avatar: "preset:0".into(), created: 0 }
    }

    fn to_json(&self) -> Json {
        Json::obj(vec![
            ("id", self.id.as_str().into()),
            ("name", self.name.as_str().into()),
            ("avatar", self.avatar.as_str().into()),
            ("created", (self.created as f64).into()),
        ])
    }

    fn from_json(j: &Json) -> Profile {
        Profile { id: j.get("id").str(), name: clean_name(&j.get("name").str()), avatar: j.get("avatar").str(), created: j.get("created").as_u64().unwrap_or(0) }
    }

    /// The number of its ready-made picture; one picked from the id when it has a photo (or a broken value), so a
    /// photo that went missing still leaves a picture.
    pub fn preset(&self) -> u32 {
        self.avatar
            .strip_prefix("preset:")
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|n| *n < PRESETS)
            .unwrap_or_else(|| (crate::util::fnv_hex(&self.id).bytes().map(|b| b as u32).sum::<u32>()) % PRESETS)
    }

    /// The file name of its photo, when it has one.
    pub fn photo_name(&self) -> Option<&str> {
        self.avatar.strip_prefix("file:").filter(|n| safe_file(n))
    }
}

/// An id made by `Store::add` (or "main"): lower-case letters and digits only, so it is always a plain folder name.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 24 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// A picture file name of a profile folder: "avatar-….png", nothing that could leave the folder.
fn safe_file(name: &str) -> bool {
    name.starts_with("avatar-") && name.ends_with(".png") && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

/// "preset:<n>" with a picture that exists, or "file:<name>" of a picture file name.
pub fn valid_avatar(avatar: &str) -> bool {
    match (avatar.strip_prefix("preset:"), avatar.strip_prefix("file:")) {
        (Some(n), _) => n.parse::<u32>().map(|n| n < PRESETS).unwrap_or(false),
        (_, Some(f)) => safe_file(f),
        _ => false,
    }
}

/// A name as it is kept: no control characters, spaces collapsed, at most 40 characters.
pub fn clean_name(name: &str) -> String {
    let plain: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    plain.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_NAME).collect()
}

/// The folder of a profile's data: the data folder itself for the main profile, `<data>/profiles/<id>` for the others.
pub fn data_dir_of(data: &Path, id: &str) -> PathBuf {
    if id == MAIN {
        data.to_path_buf()
    } else {
        data.join(FOLDER).join(id)
    }
}

/// The folder of a profile's own pictures (for the main profile, a folder of its own beside the others').
pub fn home_of(data: &Path, id: &str) -> PathBuf {
    data.join(FOLDER).join(id)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Store {
    pub list: Vec<Profile>,
    pub active: String,
}

impl Default for Store {
    fn default() -> Self {
        Store { list: vec![Profile::main()], active: MAIN.into() }
    }
}

impl Store {
    /// Reads `profiles.json`. Without it (every install before profiles existed) there is one profile, the main one,
    /// over the data that is already there; nothing is written until something changes.
    pub fn load(data: &Path) -> Store {
        let mut s = std::fs::read_to_string(data.join(FILE))
            .ok()
            .and_then(|t| Json::parse(&t).ok())
            .map(|j| Store { list: j.get("profiles").items().iter().map(Profile::from_json).collect(), active: j.get("active").str() })
            .unwrap_or_default();
        s.repair();
        s
    }

    /// A file edited by hand or cut off half-way still gives a usable list: the main profile first, ids valid and
    /// unique, pictures that make sense, and an active profile that exists.
    fn repair(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        self.list.retain(|p| {
            let keep = valid_id(&p.id) && !seen.contains(&p.id);
            seen.push(p.id.clone());
            keep
        });
        match self.list.iter().position(|p| p.id == MAIN) {
            Some(0) => {}
            Some(i) => {
                let m = self.list.remove(i);
                self.list.insert(0, m);
            }
            None => self.list.insert(0, Profile::main()),
        }
        self.list.truncate(MAX_PROFILES);
        for p in self.list.iter_mut() {
            if !valid_avatar(&p.avatar) {
                p.avatar = format!("preset:{}", p.preset());
            }
        }
        if !self.list.iter().any(|p| p.id == self.active) {
            self.active = MAIN.into();
        }
    }

    /// Writes `profiles.json` through a temporary file, so a crash never leaves half a list.
    pub fn save(&self, data: &Path) -> Result<(), String> {
        std::fs::create_dir_all(data).map_err(|e| e.to_string())?;
        let j = Json::obj(vec![("active", self.active.as_str().into()), ("profiles", Json::Arr(self.list.iter().map(Profile::to_json).collect()))]);
        let tmp = data.join(format!("{}.tmp", FILE));
        std::fs::write(&tmp, j.pretty()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, data.join(FILE)).map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.list.iter().find(|p| p.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut Profile, String> {
        self.list.iter_mut().find(|p| p.id == id).ok_or_else(|| "That profile does not exist.".to_string())
    }

    /// The profile in use (the list always has the main one).
    pub fn active(&self) -> &Profile {
        self.get(&self.active).unwrap_or(&self.list[0])
    }

    /// The folder of the active profile's data.
    pub fn active_dir(&self, data: &Path) -> PathBuf {
        data_dir_of(data, &self.active().id)
    }

    /// The photo of a profile on disk, when it has one and the file is there.
    pub fn photo_path(&self, data: &Path, id: &str) -> Option<PathBuf> {
        let p = self.get(id)?;
        let f = home_of(data, &p.id).join(p.photo_name()?);
        f.is_file().then_some(f)
    }

    /// A new profile (not switched to). An empty name becomes "Profile <n>"; `avatar` "" gives the next ready-made
    /// picture, so people added one after the other look different. Returns its id.
    pub fn add(&mut self, name: &str, avatar: &str, now: u64) -> Result<String, String> {
        if self.list.len() >= MAX_PROFILES {
            return Err(format!("There can be at most {} profiles.", MAX_PROFILES));
        }
        if !avatar.is_empty() && !valid_avatar(avatar) {
            return Err("That picture cannot be used.".into());
        }
        let mut name = clean_name(name);
        if name.is_empty() {
            name = format!("Profile {}", self.list.len() + 1);
        }
        // the time in base 36 (short, and ordered by creation), then a counter if two are made in the same second
        let base = format!("p{}", base36(now));
        let mut id = base.clone();
        let mut n = 2;
        while self.get(&id).is_some() {
            id = format!("{}{}", base, n);
            n += 1;
        }
        let avatar = if avatar.is_empty() { format!("preset:{}", self.list.len() as u32 % PRESETS) } else { avatar.to_string() };
        self.list.push(Profile { id: id.clone(), name, avatar, created: now });
        Ok(id)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = clean_name(name);
        if name.is_empty() {
            return Err("Type a name for the profile.".into());
        }
        self.get_mut(id)?.name = name;
        Ok(())
    }

    /// Gives a profile another picture. Returns the photo it had before, when it had one (its file can go).
    pub fn set_avatar(&mut self, id: &str, avatar: &str) -> Result<Option<String>, String> {
        if !valid_avatar(avatar) {
            return Err("That picture cannot be used.".into());
        }
        let p = self.get_mut(id)?;
        let old = p.photo_name().map(|s| s.to_string());
        p.avatar = avatar.to_string();
        Ok(old.filter(|o| avatar != format!("file:{}", o)))
    }

    /// Takes a profile out of the list. The main profile stays (its data is the top of the data folder, mixed with
    /// what everyone shares), and so does the last one. When it was the one in use, the main profile is used again.
    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        if id == MAIN {
            return Err("The main profile cannot be removed.".into());
        }
        if self.list.len() <= 1 {
            return Err("The last profile cannot be removed.".into());
        }
        let i = self.list.iter().position(|p| p.id == id).ok_or_else(|| "That profile does not exist.".to_string())?;
        self.list.remove(i);
        if self.active == id {
            self.active = MAIN.into();
        }
        Ok(())
    }

    pub fn set_active(&mut self, id: &str) -> Result<(), String> {
        if self.get(id).is_none() {
            return Err("That profile does not exist.".into());
        }
        self.active = id.to_string();
        Ok(())
    }
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Copies a picture the window prepared (a 256 x 256 PNG, see `CoreBridge::squareAvatar`) into the profile's folder
/// and gives the avatar value for it ("file:avatar-<time>.png"). Only a PNG of a sensible size is taken.
pub fn keep_photo(data: &Path, id: &str, src: &Path, now: u64) -> Result<String, String> {
    if !valid_id(id) {
        return Err("That profile does not exist.".into());
    }
    let meta = std::fs::metadata(src).map_err(|_| "The picture could not be read.".to_string())?;
    if !meta.is_file() || meta.len() > MAX_PHOTO {
        return Err("That picture cannot be used.".into());
    }
    let bytes = std::fs::read(src).map_err(|_| "The picture could not be read.".to_string())?;
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err("That picture cannot be used.".into());
    }
    let dir = home_of(data, id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // a new name every time: the window caches pictures by their address, so a new photo must have a new one
    let mut name = format!("avatar-{}.png", now);
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("avatar-{}-{}.png", now, n);
        n += 1;
    }
    std::fs::write(dir.join(&name), &bytes).map_err(|e| e.to_string())?;
    Ok(format!("file:{}", name))
}

/// Removes a photo file of a profile (one that was replaced).
pub fn remove_photo(data: &Path, id: &str, name: &str) {
    if valid_id(id) && safe_file(name) {
        let _ = std::fs::remove_file(home_of(data, id).join(name));
    }
}

/// Deletes the whole folder of a profile that was removed from the list: its library, favourites, history, study and
/// pictures. Never the main profile (its data is the top of the data folder).
pub fn delete_folder(data: &Path, id: &str) -> Result<(), String> {
    if id == MAIN || !valid_id(id) {
        return Err("The main profile cannot be removed.".into());
    }
    let dir = home_of(data, id);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("The profile's folder could not be deleted: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rh_profiles_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// No file: one main profile over the data folder itself, and nothing is written just by reading.
    #[test]
    fn no_file_is_one_main_profile() {
        let d = temp("none");
        let s = Store::load(&d);
        assert_eq!(s.list.len(), 1);
        assert_eq!(s.active, MAIN);
        assert_eq!(s.active_dir(&d), d);
        assert!(!d.join(FILE).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn add_rename_switch_save_and_load() {
        let d = temp("cycle");
        let mut s = Store::load(&d);
        let a = s.add("  Ana \n Maria ", "", 1_700_000_000).unwrap();
        let b = s.add("", "preset:7", 1_700_000_000).unwrap();
        assert_ne!(a, b, "two profiles made in the same second still get their own ids");
        assert!(valid_id(&a) && valid_id(&b));
        assert_eq!(s.get(&a).unwrap().name, "Ana Maria");
        assert_eq!(s.get(&b).unwrap().name, "Profile 3");
        assert_eq!(s.get(&a).unwrap().avatar, "preset:1", "the next ready-made picture");
        assert!(s.rename(&a, "   ").is_err());
        s.rename(&a, "Ana").unwrap();
        s.set_active(&b).unwrap();
        assert!(s.set_active("nobody").is_err());
        assert_eq!(s.active_dir(&d), d.join("profiles").join(&b));
        s.save(&d).unwrap();
        let back = Store::load(&d);
        assert_eq!(back, s);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Never the main profile, never the last; removing the one in use goes back to the main profile.
    #[test]
    fn remove_rules() {
        let mut s = Store::default();
        assert!(s.remove(MAIN).is_err());
        let a = s.add("A", "", 5).unwrap();
        s.set_active(&a).unwrap();
        s.remove(&a).unwrap();
        assert_eq!(s.active, MAIN);
        assert!(s.remove(&a).is_err());
        assert_eq!(s.list.len(), 1);
    }

    #[test]
    fn limits_and_bad_values() {
        let mut s = Store::default();
        assert!(s.add("x", "preset:12", 1).is_err());
        assert!(s.add("x", "file:../../evil.png", 1).is_err());
        for i in 1..MAX_PROFILES {
            s.add(&format!("P{}", i), "", i as u64).unwrap();
        }
        assert!(s.add("one more", "", 99).is_err());
        assert_eq!(clean_name(&"x".repeat(100)).chars().count(), 40);
        assert!(!valid_id("../x") && !valid_id("A") && !valid_id(""));
        assert!(valid_avatar("file:avatar-12.png") && !valid_avatar("file:avatar-1/../x.png") && !valid_avatar("preset:x"));
    }

    /// A file edited by hand: the main profile comes back first, duplicates and bad ids go, a bad picture becomes a
    /// ready-made one and an active id that does not exist falls back to the main profile.
    #[test]
    fn broken_file_is_repaired() {
        let d = temp("repair");
        std::fs::write(
            d.join(FILE),
            r#"{"active":"gone","profiles":[{"id":"pb","name":"B","avatar":"bad"},{"id":"pb","name":"B2"},{"id":"../x","name":"X"},{"id":"main","name":"Me","avatar":"preset:3"}]}"#,
        )
        .unwrap();
        let s = Store::load(&d);
        assert_eq!(s.list.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["main", "pb"]);
        assert_eq!(s.list[0].name, "Me");
        assert!(valid_avatar(&s.list[1].avatar) && s.list[1].avatar.starts_with("preset:"));
        assert_eq!(s.active, MAIN);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A photo is kept only when it is a PNG; replacing it gives the old name back so its file can go, and deleting a
    /// profile removes its whole folder (never the data folder of the main one).
    #[test]
    fn photos_and_folders() {
        let d = temp("photos");
        let mut s = Store::default();
        let a = s.add("A", "", 10).unwrap();
        let png = d.join("in.png");
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&[0u8; 32]);
        std::fs::write(&png, &bytes).unwrap();
        let not_png = d.join("in.jpg");
        std::fs::write(&not_png, b"\xff\xd8\xff\xe0 not a png").unwrap();
        assert!(keep_photo(&d, &a, &not_png, 20).is_err());
        let v1 = keep_photo(&d, &a, &png, 20).unwrap();
        assert_eq!(v1, "file:avatar-20.png");
        assert_eq!(s.set_avatar(&a, &v1).unwrap(), None);
        assert_eq!(s.photo_path(&d, &a), Some(d.join("profiles").join(&a).join("avatar-20.png")));
        let v2 = keep_photo(&d, &a, &png, 20).unwrap();
        assert_eq!(v2, "file:avatar-20-2.png", "a second photo in the same second gets its own name");
        assert_eq!(s.set_avatar(&a, &v2).unwrap().as_deref(), Some("avatar-20.png"));
        remove_photo(&d, &a, "avatar-20.png");
        assert!(!d.join("profiles").join(&a).join("avatar-20.png").exists());
        assert_eq!(s.set_avatar(&a, "preset:4").unwrap().as_deref(), Some("avatar-20-2.png"));
        assert_eq!(s.photo_path(&d, &a), None);
        // the main profile's pictures live in profiles/main, not at the top of the data folder
        let m = keep_photo(&d, MAIN, &png, 30).unwrap();
        assert!(d.join("profiles").join("main").join(m.trim_start_matches("file:")).is_file());
        assert!(delete_folder(&d, MAIN).is_err());
        delete_folder(&d, &a).unwrap();
        assert!(!d.join("profiles").join(&a).exists());
        assert!(d.join("in.png").exists(), "nothing outside the profile's folder is touched");
        let _ = std::fs::remove_dir_all(&d);
    }
}
