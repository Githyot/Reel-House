//! Local profiles in the core (the list itself is `profiles.rs`): the state the window shows, and switching, adding,
//! renaming, pictures and removing. Switching first writes what the profile in use keeps in memory and closes what
//! shows its things (readers, a review), then reads every list of the other profile from its own folder.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::profiles::{self, Store};

/// The temporary pictures the window prepares (`CoreBridge::squareAvatar`): only these are deleted after use.
fn prepared_picture(p: &Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("reelhouse-avatar-") && n.ends_with(".png")).unwrap_or(false)
}

impl Core {
    /// State "profiles": {active, presets, max, list: [{id, name, preset, photo, main, active}]}. `photo` is the path
    /// of the profile's own picture ("" for a ready-made one, `preset` then says which); `name` is "" for a main
    /// profile never named (the window shows "Main profile").
    pub fn profiles_json(&self) -> Json {
        let list = self
            .profiles
            .list
            .iter()
            .map(|p| {
                let photo = self.profiles.photo_path(&self.data_dir, &p.id).map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                Json::obj(vec![
                    ("id", p.id.as_str().into()),
                    ("name", p.name.as_str().into()),
                    ("preset", (p.preset() as f64).into()),
                    ("photo", Json::Str(photo)),
                    ("main", Json::Bool(p.id == profiles::MAIN)),
                    ("active", Json::Bool(p.id == self.profiles.active)),
                ])
            })
            .collect();
        Json::obj(vec![
            ("active", self.profiles.active.as_str().into()),
            ("presets", (profiles::PRESETS as f64).into()),
            ("max", (profiles::MAX_PROFILES as f64).into()),
            ("list", Json::Arr(list)),
        ])
    }

    /// Commands "profile_switch" {id}, "profile_add" {name, preset?, photo?}, "profile_rename" {id, name},
    /// "profile_avatar" {id, preset | photo} and "profile_delete" {id}. `photo` is a picture the window prepared
    /// (a 256 x 256 PNG made by `Core.squareAvatar`).
    pub fn profile_cmd(&mut self, cmd: &str, j: &Json) -> Result<(), String> {
        let id = j.get("id").str();
        let photo = j.get("photo").str();
        let preset = j.get("preset").as_u64();
        let r = match cmd {
            "profile_switch" => self.profile_switch(&id),
            "profile_add" => self.profile_add(&j.get("name").str(), preset, &photo).map(|_| ()),
            "profile_rename" => self.profile_rename(&id, &j.get("name").str()),
            "profile_avatar" => self.profile_avatar(&id, preset, &photo),
            "profile_delete" => self.profile_delete(&id),
            other => Err(format!("unknown command: {}", other)),
        };
        if let Err(e) = &r {
            crate::debug::log("profiles", &format!("{} failed: {}", cmd, e));
        }
        // the window shows the list again, whatever happened
        self.events.push(Event::Profiles);
        r
    }

    /// Writes `profiles.json`; when that fails, the list goes back to what it was (`before`), so the window never
    /// shows a change that would be gone at the next start.
    fn profiles_save(&mut self, before: Store) -> Result<(), String> {
        match self.profiles.save(&self.data_dir) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.profiles = before;
                Err(format!("The profiles could not be saved: {}", e))
            }
        }
    }

    /// Uses another profile: the library, favourites, history, study and books shown become that person's.
    /// Not while a video plays (its position would be kept in the wrong history).
    pub fn profile_switch(&mut self, id: &str) -> Result<(), String> {
        if self.profiles.get(id).is_none() {
            return Err("That profile does not exist.".into());
        }
        if id == self.profiles.active {
            return Ok(());
        }
        if self.session.is_some() {
            return Err("Close the video first.".into());
        }
        let before = self.profiles.clone();
        self.profiles.set_active(id)?;
        self.profiles_save(before)?;
        self.profile_leave();
        self.profile_dir = self.profiles.active_dir(&self.data_dir);
        let _ = std::fs::create_dir_all(&self.profile_dir);
        self.profile_reload();
        crate::debug::log("profiles", &format!("now using profile {} ({})", id, self.profile_dir.display()));
        Ok(())
    }

    /// Before another profile comes in: what the one in use keeps in memory is written to its folder, and the places
    /// that show its things (the readers, a study review) are closed.
    fn profile_leave(&mut self) {
        self.manga_close_reader();
        self.manga_close();
        self.novel_close_reader();
        self.novel_close();
        // (closing a review saves the collection)
        self.study_close();
        if self.study_dirty > 0 {
            self.save_study();
        }
        self.time_flush();
        self.save_progress_file();
        self.save_manga();
        self.save_novels();
        self.save_notices();
        // Conversation: a call hangs up and the sessions are written (desktop; nothing on Android and iOS)
        self.kaiwa_leave_profile();
    }

    /// Reads again everything that belongs to the profile in use, from `profile_dir` (after a switch, or a restored
    /// backup), and tells the window to show it.
    pub(super) fn profile_reload(&mut self) {
        let d = self.profile_dir.clone();
        self.library = settings::load_library(&d);
        self.favorites = settings::load_list(&d, "favorites.json");
        self.watched = load_watched(&d);
        self.progress = load_progress(&d);
        self.ep_resume = controller_resume::load_resume(&d);
        self.track_restore = None;
        self.manga = crate::manga::State::load(&d);
        self.events.push(Event::MangaLibrary);
        self.novels = crate::manga::State::load_file(&d, controller_novels::SHELF);
        self.events.push(Event::NovelLibrary);
        // Study: the collection, knowledge.md, Learn's progress, the study time and the reminders
        self.study = Core::load_study(&d);
        self.jlab = crate::jlab::Jlab::load(&d);
        self.study_dirty = 0;
        self.study_session = None;
        self.study_browse = None;
        self.study_stats = None;
        self.hint_cache = None;
        self.hint_text.clear();
        self.flash = None;
        self.wordmix_readers = None;
        self.knowledge = crate::knowledge::Knowledge::load_from(&crate::study::dir(&d).join("knowledge.md"));
        self.time_profile_changed();
        self.learn_profile_changed();
        self.nudges = Default::default();
        // Conversation (desktop): the sessions, mistakes and words are read from this profile's kaiwa.json
        self.kaiwa_forget();
        self.events.push(Event::Study);
        // the bell: new episodes of this person's library, and their study pop-ups
        let (seen, notices) = controller_notify::load_notices(&d);
        self.seen_latest = seen;
        self.notices = notices;
        self.events.push(Event::Notices);
        // Research mode: the history, downloads, pins and tabs (Chromium's own folder stays shared)
        self.browser.set_lists(d.join("browser"));
        // the calendar and the "Because you watched…" rows are made from the library and what was watched
        self.calendar_gen += 1;
        self.calendar_loading = false;
        let had_calendar = self.calendar_at.take().is_some() || !self.calendar.is_empty();
        self.calendar.clear();
        if had_calendar {
            self.load_calendar();
        }
        self.load_taste();
        for e in [Event::Profiles, Event::Library, Event::Progress, Event::Home, Event::Calendar, Event::Detail] {
            self.events.push(e);
        }
    }

    /// A new profile, with its own (empty) folder; the one in use stays. Returns its id.
    pub fn profile_add(&mut self, name: &str, preset: Option<u64>, photo: &str) -> Result<String, String> {
        let before = self.profiles.clone();
        let avatar = preset.map(|n| format!("preset:{}", n)).unwrap_or_default();
        let id = self.profiles.add(name, &avatar, util::now_unix())?;
        // a photo that cannot be used leaves the ready-made picture, and says why (the profile is made all the same,
        // so a second "Save" does not make a second one)
        let photo_err = if photo.is_empty() { None } else { self.profile_photo(&id, photo).err() };
        self.profiles_save(before)?;
        let _ = std::fs::create_dir_all(profiles::data_dir_of(&self.data_dir, &id));
        crate::debug::log("profiles", &format!("added profile {}", id));
        if let Some(e) = photo_err {
            self.toast(&e);
        }
        Ok(id)
    }

    pub fn profile_rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let before = self.profiles.clone();
        self.profiles.rename(id, name)?;
        self.profiles_save(before)
    }

    /// Gives a profile one of the ready-made pictures (`preset`) or a photo the window prepared (`photo`).
    pub fn profile_avatar(&mut self, id: &str, preset: Option<u64>, photo: &str) -> Result<(), String> {
        let before = self.profiles.clone();
        let old = if !photo.is_empty() {
            self.profile_photo(id, photo)?
        } else {
            let n = preset.ok_or_else(|| "Choose a picture.".to_string())?;
            self.profiles.set_avatar(id, &format!("preset:{}", n))?
        };
        self.profiles_save(before)?;
        // the photo it had before goes only once the new picture is kept
        if let Some(o) = old {
            profiles::remove_photo(&self.data_dir, id, &o);
        }
        Ok(())
    }

    /// Puts a picture the window prepared into the profile's folder and makes it its avatar. Returns the photo it had
    /// before (for the caller to delete once the list is saved). The prepared file was temporary: it goes either way.
    fn profile_photo(&mut self, id: &str, src: &str) -> Result<Option<String>, String> {
        if self.profiles.get(id).is_none() {
            return Err("That profile does not exist.".into());
        }
        let src = PathBuf::from(src);
        let r = profiles::keep_photo(&self.data_dir, id, &src, util::now_unix()).and_then(|avatar| {
            self.profiles.set_avatar(id, &avatar).inspect_err(|_| profiles::remove_photo(&self.data_dir, id, avatar.trim_start_matches("file:")))
        });
        if prepared_picture(&src) {
            let _ = std::fs::remove_file(&src);
        }
        r
    }

    /// Removes a profile and deletes its folder (its library, favourites, history, study and pictures). Never the main
    /// profile nor the last one; when it is the one in use, the main profile is used first.
    pub fn profile_delete(&mut self, id: &str) -> Result<(), String> {
        // the rules first, on a copy: nothing changes when it cannot be removed
        self.profiles.clone().remove(id)?;
        if self.profiles.active == id {
            self.profile_switch(profiles::MAIN)?;
        }
        let before = self.profiles.clone();
        self.profiles.remove(id)?;
        self.profiles_save(before)?;
        crate::debug::log("profiles", &format!("removed profile {}", id));
        profiles::delete_folder(&self.data_dir, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> MetaItem {
        MetaItem::from_json(&Json::obj(vec![("id", id.into()), ("type", "movie".into()), ("name", id.into())]))
    }

    /// Two people on one computer: each sees only their own library, favourites and history; the main profile keeps
    /// reading the files at the top of the data folder (nothing is moved), and a new profile writes in its own folder.
    #[test]
    fn switching_keeps_each_profile_apart() {
        let dir = std::env::temp_dir().join(format!("rh_profiles_core_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // data from before profiles existed
        settings::save_library(&dir, &[item("tt-old")]).unwrap();
        std::fs::write(dir.join("watched.json"), "[\"tt-old:1:1\"]").unwrap();
        let mut c = Core::new_in(dir.clone());
        assert_eq!(c.profile_dir, dir);
        assert_eq!(c.library.len(), 1);
        assert!(c.watched.contains("tt-old:1:1"));

        let id = c.profile_add("Bia", Some(3), "").unwrap();
        assert_eq!(c.profiles.active, profiles::MAIN, "adding does not switch");
        c.profile_switch(&id).unwrap();
        assert_eq!(c.profile_dir, dir.join("profiles").join(&id));
        assert!(c.library.is_empty() && c.watched.is_empty() && c.favorites.is_empty(), "a new profile starts empty");
        c.library.push(item("tt-new"));
        let _ = settings::save_library(&c.profile_dir, &c.library);
        c.watched.insert("tt-new:1:2".into());
        c.save_watched();
        assert!(dir.join("profiles").join(&id).join("library.json").is_file());

        // back to the main profile: its data is untouched
        c.profile_switch(profiles::MAIN).unwrap();
        assert_eq!(c.library.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["tt-old"]);
        assert!(c.watched.contains("tt-old:1:1") && !c.watched.contains("tt-new:1:2"));

        // the profile in use is remembered for the next start
        c.profile_switch(&id).unwrap();
        let again = Core::new_in(dir.clone());
        assert_eq!(again.profiles.active, id);
        assert_eq!(again.library.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["tt-new"]);
        drop(again);

        // deleting the profile in use goes back to the main one and removes its folder
        assert!(c.profile_delete(profiles::MAIN).is_err());
        c.profile_delete(&id).unwrap();
        assert_eq!(c.profiles.active, profiles::MAIN);
        assert_eq!(c.profile_dir, dir);
        assert!(!dir.join("profiles").join(&id).exists());
        assert!(dir.join("library.json").is_file(), "the main profile's files stay");
        let s = c.profiles_json();
        assert_eq!(s.get("list").items().len(), 1);
        assert_eq!(s.get("active").str(), profiles::MAIN);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
