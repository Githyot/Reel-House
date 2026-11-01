//! Android and iOS: Study > Conversation (the tutor, `controller_kaiwa.rs`) and the local AI are desktop-only, so this
//! stands in for them with the same names. The commands answer with a toast instead of doing anything; the interface
//! does not show Conversation there (StudyPage and the navigation bars) nor the local AI (Preferences).
use super::*;

fn unavailable() -> String {
    format!("Conversation and the local AI are not available on {}: they run on Windows, Linux and macOS.", crate::platform::pretty_os())
}

/// The messages of the desktop version; only `Ollama` exists here (and nothing sends it).
pub enum KaiwaMsg {
    Ollama,
}

#[derive(Default)]
pub struct KaiwaState {}

impl Core {
    fn kaiwa_refuse(&mut self) {
        self.toast(&unavailable());
        self.events.push(Event::Study);
    }

    pub fn kaiwa_start(&mut self, _j: &Json) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_send(&mut self, _text: &str) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_retry(&mut self) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_hint(&mut self) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_cancel(&mut self) {}
    pub fn kaiwa_release(&mut self) {}
    pub fn kaiwa_status_json(&self) -> Json {
        Json::obj(vec![("stage", Json::Str(String::new()))])
    }
    pub fn kaiwa_word(&mut self, _word: &str, _sentence: &str) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_card(&mut self, _j: &Json) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_end(&mut self) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_open(&mut self, _sid: i64) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_delete(&mut self, _sid: i64) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_listen(&mut self, _path: &str) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_speak(&mut self, _text: &str) {
        self.kaiwa_refuse()
    }

    pub(super) fn on_kaiwa(&mut self, msg: KaiwaMsg) {
        match msg {
            KaiwaMsg::Ollama => self.events.push(Event::Study),
        }
    }

    pub fn ollama_setup(&mut self) {
        self.kaiwa_refuse()
    }
    pub fn ollama_status(&mut self) {
        self.events.push(Event::Study);
    }
    pub fn ollama_stop(&mut self) {
        self.ollama.stop();
    }

    /// The local AI for the interface: always unavailable here.
    pub fn ollama_json(&self) -> Json {
        self.ollama.status().to_json()
    }

    // Conversation by voice (Gemini Live) is desktop-only too
    pub fn kaiwa_live_tick(&mut self) {}
    pub fn kaiwa_live_shutdown(&mut self) {}
    pub fn kaiwa_live_talk(&mut self, _down: bool) {}
    pub fn kaiwa_live_text(&mut self, _text: &str) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_live_call(&mut self) {
        self.kaiwa_refuse()
    }
    pub fn kaiwa_live_stop(&mut self) {}
    /// (another profile is in use: nothing of Conversation is kept here)
    pub(super) fn kaiwa_leave_profile(&mut self) {}
    pub(super) fn kaiwa_forget(&mut self) {}
    pub fn kaiwa_live_opts(&mut self, _j: &Json) {}
    pub fn kaiwa_annotate(&mut self) {}
    pub fn kaiwa_live_json(&self) -> Json {
        Json::obj(vec![("running", Json::Bool(false))])
    }

    /// Conversation for the interface: `available: false` and why.
    pub fn kaiwa_json(&self) -> Json {
        Json::obj(vec![("available", Json::Bool(false)), ("error", Json::Str(unavailable()))])
    }
}
