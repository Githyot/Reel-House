//! The notification center: new episodes of the anime and series in the library and favorites.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::json::Json;

/// "NEW" on a cover: an episode came out that the user has not opened the title page for yet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notice {
    pub id: String,
    pub title: String,
    pub poster: Option<String>,
    /// "S01E05"
    pub label: String,
    pub episode_title: String,
    pub at: u64,
    /// "" for an episode, "manga" or "novel" for new chapters of a series of the Images or Text mode,
    /// "banner" for a pop-up of the Study mode kept in the bell (title, body in `label`).
    pub kind: String,
    /// Banners: the kind of the pop-up (streak, goal, pomodoro…), the mood of the macaw and the command (JSON text)
    /// sent when it is clicked ("" = none).
    pub banner_kind: String,
    pub mood: String,
    pub action: String,
}

/// The newest episode that is already out on `today` ("2026-10-11"): (season, episode, title). Specials do not count.
pub fn latest_released(meta: &MetaItem, today: &str) -> Option<(i64, i64, String)> {
    meta.videos
        .iter()
        // `get`, not `[..10]`: the date comes from the addon and may hold any bytes
        .filter(|v| v.released.get(..10).map(|d| d <= today).unwrap_or(false))
        .filter_map(|v| Some((v.season.unwrap_or(1), v.episode?, v.title.clone())))
        .filter(|(s, _, _)| *s >= 1)
        .max_by_key(|(s, e, _)| (*s, *e))
}

fn key_of(s: i64, e: i64) -> String {
    format!("{}:{}", s, e)
}

fn parse_key(k: &str) -> Option<(i64, i64)> {
    let (a, b) = k.split_once(':')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// What a check found out about one title.
pub type Found = (String, String, Option<String>, Option<(i64, i64, String)>);

pub fn load_notices(dir: &Path) -> (HashMap<String, String>, Vec<Notice>) {
    let Ok(text) = std::fs::read_to_string(dir.join("notices.json")) else { return Default::default() };
    let Ok(j) = Json::parse(&text) else { return Default::default() };
    let seen = j.get("seen").entries().iter().map(|(k, v)| (k.clone(), v.str())).collect();
    let list = j
        .get("notices")
        .items()
        .iter()
        .map(|n| Notice {
            id: n.get("id").str(),
            title: n.get("title").str(),
            poster: Some(n.get("poster").str()).filter(|p| !p.is_empty()),
            label: n.get("label").str(),
            episode_title: n.get("episode_title").str(),
            at: n.get("at").as_u64().unwrap_or(0),
            kind: n.get("kind").str(),
            banner_kind: n.get("banner_kind").str(),
            mood: n.get("mood").str(),
            action: n.get("action").str(),
        })
        .filter(|n| !n.id.is_empty())
        .collect();
    (seen, list)
}

impl Core {
    pub(super) fn save_notices(&self) {
        let seen = Json::obj(self.seen_latest.iter().map(|(k, v)| (k.as_str(), v.as_str().into())).collect());
        let list = Json::Arr(
            self.notices
                .iter()
                .map(|n| {
                    Json::obj(vec![
                        ("id", n.id.as_str().into()),
                        ("title", n.title.as_str().into()),
                        ("poster", n.poster.clone().unwrap_or_default().into()),
                        ("label", n.label.as_str().into()),
                        ("episode_title", n.episode_title.as_str().into()),
                        ("at", (n.at as f64).into()),
                        ("kind", n.kind.as_str().into()),
                        ("banner_kind", n.banner_kind.as_str().into()),
                        ("mood", n.mood.as_str().into()),
                        ("action", n.action.as_str().into()),
                    ])
                })
                .collect(),
        );
        let _ = std::fs::write(self.profile_dir.join("notices.json"), Json::obj(vec![("seen", seen), ("notices", list)]).pretty());
    }

    pub fn has_new(&self, id: &str) -> bool {
        self.notices.iter().any(|n| n.id == id)
    }

    /// The title page was opened (or the notice was dismissed): the NEW mark goes away.
    pub fn clear_notice(&mut self, id: &str) {
        if let Some(i) = self.notices.iter().position(|n| n.id == id) {
            self.notices.remove(i);
            self.save_notices();
            self.events.push(Event::Notices);
        }
    }

    /// A pop-up of the Study mode (or the Pomodoro) was shown: it is kept in the bell too, newest first, so a
    /// reminder that went by while the person looked away can still be read and clicked. At most 30 are kept; the
    /// same text again within a minute (the core and the page reporting one banner twice) is kept once.
    pub fn note_banner(&mut self, kind: &str, title: &str, body: &str, mood: &str, action: &Json) {
        if title.trim().is_empty() && body.trim().is_empty() {
            return;
        }
        let now = crate::util::now_unix();
        if self.notices.iter().any(|n| n.kind == "banner" && n.title == title && n.label == body && now.saturating_sub(n.at) < 60) {
            return;
        }
        // unique even for two banners in the same second
        let mut id = format!("banner:{}", now);
        let mut k = 1;
        while self.notices.iter().any(|n| n.id == id) {
            id = format!("banner:{}-{}", now, k);
            k += 1;
        }
        let action = match action {
            Json::Obj(_) => action.dump(),
            _ => String::new(),
        };
        self.notices.insert(
            0,
            Notice { id, title: title.to_string(), label: body.to_string(), at: now, kind: "banner".to_string(), banner_kind: kind.to_string(), mood: mood.to_string(), action, ..Default::default() },
        );
        let mut seen = 0;
        self.notices.retain(|n| {
            if n.kind != "banner" {
                return true;
            }
            seen += 1;
            seen <= 30
        });
        self.save_notices();
        self.events.push(Event::Notices);
    }

    pub fn clear_all_notices(&mut self) {
        if !self.notices.is_empty() {
            self.notices.clear();
            self.save_notices();
            self.events.push(Event::Notices);
        }
    }

    /// Called about once a minute: checks again every six hours.
    pub fn notify_tick(&mut self) {
        let due = self.new_checked.map(|t| t.elapsed().as_secs() > 6 * 3600).unwrap_or(true);
        if due {
            self.check_new_episodes();
            // The series of the Images and Text modes are looked at on the same schedule (new chapters).
            self.manga_refresh_library();
            self.novel_refresh_library();
        }
    }

    /// New chapters of a series of the library (`kind` is "manga" or "novel"): `added` came out since the last look.
    pub fn note_new_chapters(&mut self, kind: &str, series_id: &str, title: &str, cover: &str, added: u32) {
        let id = format!("{}:{}", kind, series_id);
        let more = self.notices.iter().find(|n| n.id == id).and_then(|n| n.label.trim_start_matches('+').parse::<u32>().ok()).unwrap_or(0);
        self.notices.retain(|n| n.id != id);
        self.notices.insert(
            0,
            Notice { id, title: title.to_string(), poster: Some(cover.to_string()).filter(|c| !c.is_empty()), label: format!("+{}", added + more), episode_title: String::new(), at: crate::util::now_unix(), kind: kind.to_string(), ..Default::default() },
        );
        self.notices.truncate(60);
        self.save_notices();
        self.events.push(Event::Notices);
    }

    /// Looks at every series in the library and favorites for episodes that came out since the last look.
    pub fn check_new_episodes(&mut self) {
        if self.checking_new {
            return;
        }
        let mut shows: Vec<(String, String)> = Vec::new();
        for m in self.library.iter().chain(self.favorites.iter()) {
            if (m.kind == "series" || m.kind == "anime") && !shows.iter().any(|(id, _)| *id == m.id) {
                shows.push((m.id.clone(), m.kind.clone()));
            }
        }
        self.new_checked = Some(Instant::now());
        if shows.is_empty() {
            return;
        }
        shows.truncate(80);
        self.checking_new = true;
        let entries = self.settings.addons.clone();
        self.spawn(move |tx, http| {
            let (y, mo, d, _, _, _) = crate::util::civil(crate::util::now_unix());
            let today = format!("{:04}-{:02}-{:02}", y, mo, d);
            let out = std::sync::Mutex::new(Vec::<Found>::new());
            let queue = std::sync::Mutex::new(shows);
            std::thread::scope(|sc| {
                for _ in 0..5 {
                    sc.spawn(|| loop {
                        let Some((id, kind)) = queue.lock().ok().and_then(|mut q| q.pop()) else { break };
                        let mut meta = addons::fetch_meta(&*http, &entries, &kind, &id);
                        if meta.as_ref().map(|m| m.videos.is_empty()).unwrap_or(true) && id.starts_with("tt") {
                            let url = format!("https://v3-cinemeta.strem.io/meta/series/{}.json", id);
                            if let Ok(j) = crate::http::get_json(&*http, &url, 20) {
                                let m = MetaItem::from_json(j.get("meta"));
                                if !m.name.is_empty() {
                                    meta = Some(m);
                                }
                            }
                        }
                        // A title that did not answer says nothing: it is left out, not counted as "no episodes".
                        let Some(meta) = meta else { continue };
                        if meta.videos.is_empty() {
                            continue;
                        }
                        if let Ok(mut o) = out.lock() {
                            o.push((id, meta.name.clone(), meta.poster.clone(), latest_released(&meta, &today)));
                        }
                    });
                }
            });
            send(tx, Msg::NewEps(out.into_inner().unwrap_or_default()));
        });
    }

    pub(super) fn on_new_eps(&mut self, found: Vec<Found>) {
        self.checking_new = false;
        let mut changed = false;
        let now = crate::util::now_unix();
        for (id, title, poster, latest) in found {
            let Some((s, e, ep_title)) = latest else { continue };
            let key = key_of(s, e);
            match self.seen_latest.get(&id).and_then(|k| parse_key(k)) {
                // The first look only learns where things are.
                None => {
                    self.seen_latest.insert(id, key);
                    changed = true;
                }
                Some(old) if (s, e) > old => {
                    self.seen_latest.insert(id.clone(), key);
                    self.notices.retain(|n| n.id != id);
                    self.notices.insert(0, Notice { id, title, poster, label: format!("S{:02}E{:02}", s, e), episode_title: ep_title, at: now, kind: String::new(), ..Default::default() });
                    changed = true;
                }
                Some(_) => {}
            }
        }
        self.notices.truncate(60);
        if changed {
            self.save_notices();
        }
        self.events.push(Event::Notices);
    }

    /// A library or favorites title by id, to open it from a notice.
    pub fn saved_title(&self, id: &str) -> Option<MetaItem> {
        self.library.iter().chain(self.favorites.iter()).find(|m| m.id == id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Video;

    fn video(s: i64, e: i64, date: &str) -> Video {
        Video { id: format!("x:{}:{}", s, e), season: Some(s), episode: Some(e), released: date.to_string(), ..Default::default() }
    }

    #[test]
    fn newest_released_episode() {
        let mut m = MetaItem::default();
        m.videos = vec![video(1, 1, "2026-01-01T00:00:00.000Z"), video(1, 2, "2026-10-11T00:00:00.000Z"), video(1, 3, "2026-10-18T00:00:00.000Z"), video(0, 9, "2026-10-01"), video(2, 1, "")];
        let (s, e, _) = latest_released(&m, "2026-10-11").unwrap();
        assert_eq!((s, e), (1, 2));
        // a date with a multi-byte character across byte 10 used to panic
        m.videos.push(video(1, 9, "2026-10-1é"));
        assert_eq!(latest_released(&m, "2026-10-11").map(|(s, e, _)| (s, e)), Some((1, 2)));
        assert!(latest_released(&m, "2025-01-01").is_none());
    }

    /// A banner kept in the bell comes back with its kind, mood and click command; old files without them still load.
    #[test]
    fn banner_notices_load() {
        let dir = std::env::temp_dir().join(format!("rh_notices_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("notices.json"),
            r#"{"seen":{},"notices":[{"id":"banner:5","title":"Streak","label":"Keep it","at":5,"kind":"banner","banner_kind":"streak","mood":"cheer","action":"{\"cmd\":\"set_mode\",\"mode\":\"study\"}"},{"id":"tt1","title":"Show","label":"S01E02","at":4,"kind":""}]}"#,
        )
        .unwrap();
        let (_, list) = load_notices(&dir);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].kind, "banner");
        assert_eq!(list[0].banner_kind, "streak");
        assert_eq!(list[0].mood, "cheer");
        assert_eq!(Json::parse(&list[0].action).unwrap().get("mode").str(), "study");
        assert_eq!(list[1].mood, "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keys_round_trip() {
        assert_eq!(parse_key(&key_of(2, 13)), Some((2, 13)));
        assert_eq!(parse_key("nope"), None);
    }
}
