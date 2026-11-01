//! The three modes of the app (Video, Images, Text) and the source catalogs of the last two.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::extensions::{self, Ext};

pub const MODES: [&str; 5] = ["video", "images", "text", "study", "research"];

impl Core {
    pub fn mode(&self) -> &str {
        if MODES.contains(&self.settings.general.mode.as_str()) { &self.settings.general.mode } else { "video" }
    }

    pub fn set_mode(&mut self, mode: &str) {
        if !MODES.contains(&mode) || self.settings.general.mode == mode {
            return;
        }
        self.settings.general.mode = mode.to_string();
        // leaving Study: Conversation lets go of the local AI and Whisper (they come back when it is opened again)
        if mode != "study" {
            self.kaiwa_release();
        }
        self.save_settings();
        self.events.push(Event::Mode);
    }

    /// Reads the extension list of the repository of a mode ("images" or "text").
    pub fn load_extensions(&mut self, mode: &str) {
        let repo = match mode {
            "images" => self.settings.general.images_repo.clone(),
            "text" => self.settings.general.text_repo.clone(),
            _ => return,
        };
        if repo.trim().is_empty() {
            self.ext_status.insert(mode.to_string(), super::controller_novels::NO_REPO.to_string());
            self.events.push(Event::Extensions);
            return;
        }
        if !self.ext_loading.insert(mode.to_string()) {
            return;
        }
        self.ext_status.insert(mode.to_string(), "Loading…".to_string());
        self.events.push(Event::Extensions);
        let mode = mode.to_string();
        self.spawn(move |tx, http| {
            send(tx, Msg::Extensions(mode, extensions::fetch_index(&*http, &repo)));
        });
    }

    pub(super) fn on_extensions(&mut self, mode: String, r: Result<Vec<Ext>, String>) {
        self.ext_loading.remove(&mode);
        match r {
            Ok(list) => {
                self.ext_status.insert(mode.clone(), format!("{} sources found", list.len()));
                self.ext_lists.insert(mode, list);
            }
            Err(e) => {
                self.ext_status.insert(mode, e);
            }
        }
        self.events.push(Event::Extensions);
    }
}
