//! Research mode (the built-in browser): the commands of the Research page and of the player's quick search, and the
//! state they read. The lists themselves live in `crate::browser`.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::browser::{self, DlItem, Visit};

impl Core {
    /// Runs a `browser_*` command from the window. An error is shown to the user by the caller.
    pub fn browser_cmd(&mut self, cmd: &str, j: &Json) -> Result<(), String> {
        self.browser.ensure_loaded();
        let b = &mut self.browser;
        match cmd {
            // A page finished loading (Research tabs: kind "page"; quick search: "quick"; picture search: "lens").
            "browser_visit" => {
                let kind = match j.get("kind").str().as_str() {
                    k @ ("quick" | "lens") => k.to_string(),
                    _ => "page".to_string(),
                };
                if b.history.add(Visit { time: util::now_unix(), title: j.get("title").str(), url: j.get("url").str(), kind }) {
                    b.save_history();
                }
            }
            "browser_title" => {
                if b.history.set_title(&j.get("url").str(), &j.get("title").str()) {
                    b.save_history();
                }
            }
            "browser_history_query" => b.query = j.get("q").str(),
            "browser_history_remove" => {
                if b.history.remove(j.get("time").as_u64().unwrap_or(0), &j.get("url").str()) {
                    b.save_history();
                }
            }
            // `since` in Unix seconds; 0 or missing clears everything.
            "browser_history_clear" => {
                b.history.clear_since(j.get("since").as_u64().unwrap_or(0));
                b.save_history();
            }
            "browser_dl" => {
                let d = DlItem {
                    id: j.get("id").str(),
                    url: j.get("url").str(),
                    path: j.get("path").str(),
                    received: j.get("received").as_f64().unwrap_or(0.0).max(0.0) as u64,
                    total: j.get("total").as_f64().unwrap_or(0.0).max(0.0) as u64,
                    state: j.get("state").str(),
                    time: util::now_unix(),
                };
                if d.id.is_empty() {
                    return Err("a download without an id".into());
                }
                if b.downloads.update(d) {
                    b.save_downloads();
                }
            }
            "browser_dl_remove" => {
                if b.downloads.remove(&j.get("id").str()) {
                    b.save_downloads();
                }
            }
            "browser_dl_clear" => {
                b.downloads.clear_finished();
                b.save_downloads();
            }
            "browser_set" => {
                let v = j.get("value");
                match j.get("key").str().as_str() {
                    "engine" => {
                        let e = v.str();
                        if !browser::ENGINES.iter().any(|x| x.0 == e) {
                            return Err(format!("unknown search engine: {}", e));
                        }
                        b.prefs.engine = e;
                    }
                    "restore_tabs" => b.prefs.restore_tabs = v.as_bool().unwrap_or(true),
                    "pause_quick" => b.prefs.pause_quick = v.as_bool().unwrap_or(true),
                    "quick_w" => b.prefs.quick_w = v.as_f64().unwrap_or(520.0).clamp(280.0, 4000.0) as u32,
                    "quick_h" => b.prefs.quick_h = v.as_f64().unwrap_or(420.0).clamp(200.0, 4000.0) as u32,
                    "adblock" => {
                        b.prefs.adblock = v.as_bool().unwrap_or(true);
                        b.apply_adblock(false);
                    }
                    other => return Err(format!("unknown browser setting: {}", other)),
                }
                b.save_prefs();
            }
            // The ad blocker on (`on` true) or off for the site of `url`.
            "browser_adblock_site" => {
                if b.adblock_site(&j.get("url").str(), j.get("on").as_bool().unwrap_or(true)) {
                    b.apply_adblock(false);
                    b.save_prefs();
                }
            }
            // Downloads the ad blocker's lists again now.
            "browser_adblock_update" => b.apply_adblock(true),
            "browser_pin" => {
                b.prefs.pin(&j.get("url").str(), &j.get("title").str(), j.get("on").as_bool().unwrap_or(true));
                b.save_prefs();
            }
            // The tabs open now (to open them again next time).
            "browser_tabs" => {
                b.prefs.tabs = j
                    .get("tabs")
                    .items()
                    .iter()
                    .map(|t| browser::Page { title: t.get("title").str(), url: t.get("url").str() })
                    .filter(|p| !p.url.is_empty() && !p.url.starts_with("about:"))
                    .take(60)
                    .collect();
                b.save_prefs();
            }
            // What the address field means: read the answer with state("browser:address").
            "browser_address" => b.address = browser::address_for(&j.get("text").str(), &b.prefs.engine),
            // The picture on screen for Google Lens: read the file with state("browser:frame").
            "browser_grab_frame" => {
                self.browser.frame = self.grab_frame();
                if let Err(e) = &self.browser.frame {
                    return Err(e.clone());
                }
            }
            // The same, off the window's thread (the quick search): mpv's screenshot converts and encodes the frame at
            // the video's own size, which takes from tens of ms to over a second (4K, HDR, a busy stream), and the
            // window froze for that long. The answer comes as the "lens_frame" event.
            "browser_grab_frame_async" => {
                let m = self.mpv.clone().ok_or_else(|| "Nothing is playing.".to_string())?;
                self.browser.frame_token += 1;
                let token = self.browser.frame_token;
                let tx = self.tx.clone();
                std::thread::Builder::new()
                    .name("lens-frame".into())
                    .spawn(move || {
                        let t0 = Instant::now();
                        let path = std::env::temp_dir().join("reelhouse-quick-lens.jpg");
                        let r = grab_frame_to(&m, &path);
                        send(&tx, Msg::LensFrame(token, r, t0.elapsed().as_millis() as u64));
                    })
                    .map_err(|e| format!("The picture could not be taken: {}", e))?;
            }
            other => return Err(format!("unknown command: {}", other)),
        }
        Ok(())
    }

    /// The answer of `browser_grab_frame_async` (only the newest request is told to the window).
    pub(super) fn on_lens_frame(&mut self, token: u64, r: Result<String, String>, ms: u64) {
        crate::debug::log("lens", &format!("screenshot {} ms ({})", ms, if r.is_ok() { "ok" } else { "failed" }));
        if token != self.browser.frame_token {
            return;
        }
        self.browser.frame = r.clone();
        self.events.push(Event::LensFrame(token, r, ms));
    }

    /// Saves the video frame on screen (without subtitles) as a JPEG in the temporary folder.
    fn grab_frame(&self) -> Result<String, String> {
        let m = self.mpv.as_ref().ok_or_else(|| "Nothing is playing.".to_string())?;
        grab_frame_to(m, &std::env::temp_dir().join("reelhouse-lens.jpg"))
    }

    /// The state of Research mode, in parts so the window reads only what changed:
    /// "browser" (preferences, pins, saved tabs), "browser:history" (the history filtered by `browser_history_query`,
    /// newest first, at most 400), "browser:downloads" (newest first), "browser:address" and "browser:frame" (the
    /// answers of `browser_address` and `browser_grab_frame`).
    pub fn browser_json(&mut self, name: &str) -> Json {
        self.browser.ensure_loaded();
        let b = &self.browser;
        let page = |p: &browser::Page| Json::obj(vec![("title", Json::Str(p.title.clone())), ("url", Json::Str(p.url.clone()))]);
        match name {
            "browser:address" => Json::obj(vec![("address", Json::Str(b.address.clone()))]),
            "browser:frame" => match &b.frame {
                Ok(p) => Json::obj(vec![("frame", Json::Str(p.clone())), ("error", Json::Str(String::new()))]),
                Err(e) => Json::obj(vec![("frame", Json::Str(String::new())), ("error", Json::Str(e.clone()))]),
            },
            "browser:history" => Json::obj(vec![
                ("query", Json::Str(b.query.clone())),
                ("count", Json::Num(b.history.items.len() as f64)),
                ("items", Json::Arr(b.history.search(&b.query, 400).iter().map(|v| v.to_json()).collect())),
            ]),
            "browser:downloads" => Json::obj(vec![("items", Json::Arr(b.downloads.items.iter().rev().map(DlItem::to_json).collect()))]),
            _ => Json::obj(vec![
                ("engine", Json::Str(b.prefs.engine.clone())),
                ("engines", Json::Arr(browser::ENGINES.iter().map(|e| Json::obj(vec![("id", Json::Str(e.0.into())), ("label", Json::Str(e.1.into()))])).collect())),
                ("home", Json::Str(browser::home_url(&b.prefs.engine))),
                ("pins", Json::Arr(b.prefs.pins.iter().map(page).collect())),
                ("tabs", Json::Arr(b.prefs.tabs.iter().map(page).collect())),
                ("restoreTabs", Json::Bool(b.prefs.restore_tabs)),
                ("pauseQuick", Json::Bool(b.prefs.pause_quick)),
                ("quickW", Json::Num(b.prefs.quick_w as f64)),
                ("quickH", Json::Num(b.prefs.quick_h as f64)),
                ("adblock", Json::Bool(b.prefs.adblock)),
                ("adblockAllow", Json::Arr(b.prefs.adblock_allow.iter().map(|h| Json::Str(h.clone())).collect())),
                ("adblockRules", Json::Num(crate::adblock::status().0 as f64)),
                ("adblockUpdated", Json::Num(crate::adblock::status().1 as f64)),
                ("adblockProblem", Json::Str(crate::adblock::status().2)),
                ("adblockBusy", Json::Bool(crate::adblock::status().3)),
                // Chromium's profile (cookies, logins, site data) sits next to these lists, in its own cef/ folder;
                // the backup leaves its caches out.
                ("profileDir", Json::Str(b.dir.to_string_lossy().into_owned())),
            ]),
        }
    }
}

/// Saves the video frame on screen (without subtitles) as a JPEG at `path`. Any thread (mpv's API is thread safe).
fn grab_frame_to(m: &Mpv, path: &std::path::Path) -> Result<String, String> {
    let _ = std::fs::remove_file(path);
    // "video": the picture of the video itself, at its own size, without the subtitles and the controls.
    m.cmd(&["screenshot-to-file", &path.to_string_lossy(), "video"]).map_err(|e| format!("The picture could not be taken: {}", e))?;
    if !path.exists() {
        return Err("The picture could not be taken.".into());
    }
    Ok(path.to_string_lossy().into_owned())
}
