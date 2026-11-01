//! App extensions (see `appext`): loading them and firing their hooks. Part of `controller`.
use super::*;
use crate::appext::{self, AppExt};

impl Core {
    pub fn extensions_reload(&mut self) {
        self.app_exts = appext::scan(&self.data_dir);
        let ok = self.app_exts.iter().filter(|x| x.error.is_empty()).count();
        crate::debug::log("ext", &format!("{} extensions ({} usable) in {}", self.app_exts.len(), ok, appext::dir(&self.data_dir).display()));
        for x in self.app_exts.iter().filter(|x| !x.error.is_empty()) {
            crate::debug::log("ext", &format!("{}: {}", x.id, x.error));
        }
    }

    pub fn extension_enabled(&self, x: &AppExt) -> bool {
        x.error.is_empty() && !self.settings.general.disabled_plugins.iter().any(|d| d == &x.id)
    }

    pub fn extension_toggle(&mut self, id: &str, on: bool) {
        self.settings.general.disabled_plugins.retain(|d| d != id);
        if !on {
            self.settings.general.disabled_plugins.push(id.to_string());
        }
        self.save_settings();
    }

    pub fn extensions_folder(&self) -> PathBuf {
        appext::dir(&self.data_dir)
    }

    /// Runs `hook` of every extension that has it, each in its own thread; answers come back as `Msg::ExtReply`.
    pub fn fire_hook(&self, hook: &str, payload: Json) {
        for x in self.app_exts.iter().filter(|x| self.extension_enabled(x) && x.hooks.iter().any(|h| h == hook)) {
            let (id, code, hook, args) = (x.id.clone(), x.code.clone(), hook.to_string(), Json::Arr(vec![payload.clone()]));
            self.spawn(move |tx, http| {
                let r = crate::jsrt::run(http, &id, &code, &hook, &args);
                send(tx, Msg::ExtReply(id, hook, r));
            });
        }
    }

    pub(super) fn on_ext_reply(&mut self, id: String, hook: String, r: Result<Json, String>) {
        match r {
            Ok(j) => {
                let rep = appext::reply(&j);
                if let Some(t) = rep.toast {
                    self.toast(&t);
                }
                if let Some(u) = rep.open_url {
                    self.events.push(Event::OpenUrl(u));
                }
            }
            Err(e) => crate::debug::log("ext", &format!("{} {} failed: {}", id, hook, e)),
        }
    }
}
