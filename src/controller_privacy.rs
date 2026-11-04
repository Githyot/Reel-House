//! IP protection inside the core (src/privacy.rs does the tunnels): the commands of Preferences > General >
//! IP protection, the address check, and what changes when the protection changes (the torrent engine and the
//! Suwayomi server start again with the new network options; a tunnel that dropped stops the torrent).
//! Part of `controller`.
use super::*;

/// What the engine and Suwayomi were started with ("mode:port"); when it moves they are started again.
static SIG: Mutex<String> = Mutex::new(String::new());

fn sig() -> String {
    format!("{}:{}", crate::privacy::mode(), crate::privacy::proxy_port())
}

impl Core {
    /// `privacy_set`: {mode?, proxy_url?, wg_conf?, wg_name?, torrent_over_tor?} over the current choice.
    pub fn privacy_set(&mut self, j: &Json) {
        let mut cfg = crate::privacy::cfg();
        if let Some(m) = j.get("mode").as_str() {
            cfg.mode = m.to_string();
        }
        if let Some(u) = j.get("proxy_url").as_str() {
            cfg.proxy_url = u.trim().to_string();
        }
        if let Some(w) = j.get("wg_conf").as_str() {
            cfg.wg_conf = w.to_string();
        }
        if let Some(n) = j.get("wg_name").as_str() {
            cfg.wg_name = n.to_string();
        }
        if let Some(b) = j.get("torrent_over_tor").as_bool() {
            cfg.torrent_over_tor = b;
        }
        let mode = cfg.mode.clone();
        match crate::privacy::apply(cfg) {
            Ok(()) => {
                let text = match mode.as_str() {
                    "off" => "IP protection off: the app connects straight again",
                    "tor" => "IP protection: connecting to Tor (the first time can take a minute)",
                    "system" => "IP protection: using the system VPN; nothing goes out while it is off",
                    _ => "IP protection: connecting the tunnel",
                };
                self.events.push(Event::Toast(text.to_string()));
            }
            Err(e) => self.events.push(Event::Toast(e)),
        }
        self.on_privacy_change();
    }

    /// `privacy_wg_file`: {path, copied}: reads a WireGuard file the user picked and uses it. `copied`: the window
    /// copied an Android document into the app's data to read it; that copy (it holds a private key) is removed.
    pub fn privacy_wg_file(&mut self, path: &str, copied: bool) {
        let path = path.strip_prefix("file://").unwrap_or(path);
        let read = std::fs::read_to_string(path);
        if copied {
            let _ = std::fs::remove_file(path);
        }
        match read {
            Ok(text) if text.len() < 64 * 1024 => {
                let name = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                self.privacy_set(&Json::obj(vec![("mode", "wireguard".into()), ("wg_conf", text.into()), ("wg_name", name.into())]));
            }
            Ok(_) => self.events.push(Event::Toast("That file is too big to be a WireGuard file.".to_string())),
            Err(e) => self.events.push(Event::Toast(format!("Could not read the file ({}).", e))),
        }
    }

    /// `privacy_warp`: makes a free Cloudflare WARP tunnel (registration, no account) and uses it as the WireGuard
    /// file. Only on the user's press: Cloudflare is a company and sees the traffic, the window says so.
    pub fn privacy_warp(&mut self) {
        self.events.push(Event::Toast("Creating a Cloudflare WARP tunnel…".to_string()));
        self.spawn(move |tx, http| {
            let r = crate::privacy::warp_register(&*http);
            send(tx, Msg::PrivacyWarp(r));
        });
    }

    /// The answer of `privacy_warp`.
    pub fn on_privacy_warp(&mut self, r: Result<String, String>) {
        match r {
            Ok(conf) => self.privacy_set(&Json::obj(vec![("mode", "wireguard".into()), ("wg_conf", conf.into()), ("wg_name", "Cloudflare WARP".into())])),
            Err(e) => self.events.push(Event::Toast(e)),
        }
    }

    /// `privacy_check`: asks a public service which address the internet sees now (through the tunnel when on).
    pub fn privacy_check(&mut self) {
        self.spawn(move |_tx, http| {
            let r = crate::http::get_json(&*http, "https://api.ipify.org?format=json", 30).map(|j| j.get("ip").str()).and_then(|ip| if ip.is_empty() { Err("no answer".to_string()) } else { Ok(ip) });
            crate::privacy::set_exit_ip(r);
        });
    }

    /// The protection changed or its tunnel dropped (the bridge calls this when `privacy::generation` moves).
    pub fn on_privacy_change(&mut self) {
        let now = sig();
        let moved = {
            let mut g = SIG.lock().unwrap_or_else(|p| p.into_inner());
            let moved = *g != now;
            *g = now;
            moved
        };
        let dropped = crate::privacy::take_dropped();
        if moved || dropped {
            // the engine started with the old network options (or the tunnel is gone): stop it; the next video
            // starts a new one with the current options
            let had = self.engine.lock().map(|mut g| g.take().is_some()).unwrap_or(false);
            if had && dropped && crate::privacy::active() {
                self.events.push(Event::Toast("IP protection dropped: the torrent was stopped so your address is not shown. It comes back with the tunnel.".to_string()));
            }
        }
        if moved && self.suwayomi_host.is_some() {
            self.suwayomi_restart();
        }
    }
}
