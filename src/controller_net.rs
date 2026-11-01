//! The offline state inside the core: the user's "Offline mode" switch and what is done when the connection comes
//! back (src/connectivity.rs watches the network; this file is part of `controller`).
use super::*;

impl Core {
    /// The "Offline mode" switch: kept in the settings, the requests to the internet stop at once.
    pub fn set_offline_mode(&mut self, on: bool) {
        if self.settings.general.offline_mode == on {
            return;
        }
        self.settings.general.offline_mode = on;
        crate::connectivity::set_forced(on);
        self.save_settings();
        self.events.push(Event::Toast(if on { "Offline mode: only what is on this computer is used" } else { "Offline mode off" }.to_string()));
        if !on {
            self.on_net_change(crate::connectivity::online());
        }
    }

    /// The connection came back (or went): the pages that could not load are asked again.
    pub fn on_net_change(&mut self, online: bool) {
        if !online {
            return;
        }
        // the home rows that failed offline
        if !self.home_loading && (self.rows.is_empty() || !self.home_errors.is_empty()) {
            self.load_home();
        }
    }
}
