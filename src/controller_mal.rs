//! MyAnimeList on the title page, genre pages, the user's list, favorites and the MyAnimeList login.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;
use crate::anilist;
use crate::mal::{self, Card, Character, Info, ListEntry, MalData, MyStatus, Review, Staff};

/// One piece of MyAnimeList data arriving for the open title page.
pub enum MalPart {
    Id(Option<u64>),
    Info(Info),
    Recs(Vec<Card>),
    Reviews(Vec<Review>),
    Characters(Vec<Character>),
    Staff(Vec<Staff>),
    My(Option<MyStatus>),
    /// Where the page data comes from when it is not MyAnimeList ("tmdb").
    Source(String),
    Error(String),
    Done,
}

/// Everything loaded for one MyAnimeList id, kept for the session so switching seasons is instant.
#[derive(Clone, Default)]
pub struct MalBundle {
    pub info: Option<Info>,
    pub recs: Vec<Card>,
    pub reviews: Vec<Review>,
    pub characters: Vec<Character>,
    pub staff: Vec<Staff>,
}

/// A page of titles with one genre.
pub struct GenrePage {
    pub gen: u64,
    pub id: u64,
    /// "anime" (MyAnimeList / AniList categories), or "movie" / "series" (Cinemeta genres).
    pub kind: String,
    pub name: String,
    pub items: Vec<Card>,
    pub page: u32,
    pub more: bool,
    pub loading: bool,
    pub error: String,
}

/// A working access token: refreshed when it is about to expire. The second value is set when it was refreshed.
pub fn fresh_token(http: &dyn Http, acc: &settings::MalAccount) -> Result<(String, Option<mal::Tokens>), String> {
    if acc.access_token.is_empty() && acc.refresh_token.is_empty() {
        return Err("Log in to MyAnimeList in Accounts first.".to_string());
    }
    if !acc.access_token.is_empty() && acc.expires_at > util::now_unix() + 120 {
        return Ok((acc.access_token.clone(), None));
    }
    if acc.refresh_token.is_empty() || acc.client_id.is_empty() {
        return Err("Your MyAnimeList login expired. Log in again in Accounts.".to_string());
    }
    let t = mal::refresh(http, &acc.client_id, &acc.client_secret, &acc.refresh_token)?;
    Ok((t.access.clone(), Some(t)))
}

impl Core {
    // ----- favorites -----

    pub fn is_favorite(&self, id: &str) -> bool {
        self.favorites.iter().any(|m| m.id == id)
    }

    pub fn toggle_favorite(&mut self, m: &MetaItem) {
        if let Some(i) = self.favorites.iter().position(|x| x.id == m.id) {
            self.favorites.remove(i);
        } else {
            let mut item = m.clone();
            item.videos.clear();
            item.poster = self.poster_for(m);
            self.favorites.insert(0, item);
        }
        let _ = settings::save_list(&self.profile_dir, "favorites.json", &self.favorites);
        self.keep_open_info();
        self.events.push(Event::Home);
        self.events.push(Event::Detail);
        if let Some(d) = self.detail.as_ref().filter(|d| d.meta.id == m.id && !d.loading && !d.meta.videos.is_empty()) {
            if self.favorites.iter().any(|x| x.id == m.id) {
                crate::settings::save_detail(&self.data_dir, &d.meta);
            }
        }
    }

    // ----- watched marks from the user's list -----

    /// Watched according to the app, or to the MyAnimeList list (unless the user took the mark off).
    pub fn is_watched(&self, id: &str) -> bool {
        self.watched.contains(id) || (self.external_watched.contains(id) && !self.ext_off.contains(id))
    }

    pub fn watched_externally(&self, id: &str) -> bool {
        self.external_watched.contains(id) && !self.watched.contains(id) && !self.ext_off.contains(id)
    }

    pub(super) fn refresh_external_watched(&mut self) {
        let Some(d) = self.detail.as_ref() else {
            self.external_watched.clear();
            return;
        };
        let mut set = mal::watched_ids(&d.meta, &self.mal_list);
        // The entry of the season on screen decides that season, when the list has it.
        if let Some(id) = d.mal.id {
            if let Some(e) = self.mal_list.iter().find(|e| e.mal_id == id) {
                let eps = d.meta.episodes(d.season);
                for v in &eps {
                    set.remove(&v.id);
                }
                let w = mal::watched_count(e) as i64;
                for v in eps {
                    if v.episode.map(|n| n <= w).unwrap_or(false) {
                        set.insert(v.id.clone());
                    }
                }
            }
        }
        if set != self.external_watched {
            self.external_watched = set;
            self.events.push(Event::Detail);
        }
    }

    // ----- the title page -----

    /// Looks the open title up on MyAnimeList. Runs when the page opens and when the season changes.
    pub(super) fn start_mal_lookup(&mut self) {
        let Some(d) = self.detail.as_mut() else { return };
        let (gen, mut meta, season) = (d.gen, d.meta.clone(), d.season);
        meta.videos.clear();
        d.mal = MalData { loading: true, started: Some(std::time::Instant::now()), ..Default::default() };
        self.events.push(Event::Mal);
        let cache = self.mal_cache.clone();
        // Saved titles (library or favourites) keep their MyAnimeList page on disk.
        let saved_dir = self.is_saved(&meta.id).then(|| self.data_dir.clone());
        let acc = self.settings.mal.clone();
        let tmdb_key = self.settings.metadata.tmdb_key.clone();
        let lang = self.ui_lang();
        let rkey = resolve_key(&meta.id, season);
        self.spawn(move |tx, http| {
            // A saved title remembers what it was matched with: no lookup of the id on the web, and a movie/series page of
            // TMDB is read from the disk too.
            let kept = saved_dir.as_ref().and_then(|dir| kept_id(dir, &rkey));
            if kept == Some(0) {
                let imdb = if meta.imdb_id.starts_with("tt") { meta.imdb_id.clone() } else { meta.id.split(':').next().unwrap_or("").to_string() };
                if let Some(b) = saved_dir.as_ref().and_then(|dir| load_tmdb(dir, &imdb, &meta.kind, &lang)) {
                    send(tx, Msg::Mal(gen, MalPart::Source("tmdb".into())));
                    if let Some(i) = b.info {
                        send(tx, Msg::Mal(gen, MalPart::Info(i)));
                    }
                    send(tx, Msg::Mal(gen, MalPart::Recs(b.recs)));
                    send(tx, Msg::Mal(gen, MalPart::Reviews(b.reviews)));
                    send(tx, Msg::Mal(gen, MalPart::Characters(b.characters)));
                    send(tx, Msg::Mal(gen, MalPart::Staff(b.staff)));
                    send(tx, Msg::Mal(gen, MalPart::Done));
                    return;
                }
            }
            let resolved = match kept {
                Some(i) if i > 0 => Ok(Some(i)),
                _ => mal::resolve_id(&*http, &meta, season),
            };
            if let (Ok(r), Some(dir)) = (&resolved, saved_dir.as_ref()) {
                keep_id(dir, &rkey, r.unwrap_or(0));
            }
            let id = match resolved {
                Ok(Some(i)) => i,
                Ok(None) => {
                    // Not on MyAnimeList (so not an anime): movies and series come from TMDB, when there is a key.
                    let imdb = if meta.imdb_id.starts_with("tt") { meta.imdb_id.clone() } else { meta.id.split(':').next().unwrap_or("").to_string() };
                    if !tmdb_key.trim().is_empty() && imdb.starts_with("tt") {
                        match crate::tmdb::full(&*http, &tmdb_key, &imdb, &meta.kind, &lang) {
                            Ok(Some(b)) => {
                                if let Some(dir) = saved_dir.as_ref() {
                                    let mb = MalBundle { info: Some(b.info.clone()), recs: b.recs.clone(), reviews: b.reviews.clone(), characters: b.characters.clone(), staff: b.staff.clone() };
                                    save_tmdb(dir, &imdb, &meta.kind, &lang, &mb);
                                }
                                send(tx, Msg::Mal(gen, MalPart::Source("tmdb".into())));
                                send(tx, Msg::Mal(gen, MalPart::Info(b.info)));
                                send(tx, Msg::Mal(gen, MalPart::Recs(b.recs)));
                                send(tx, Msg::Mal(gen, MalPart::Reviews(b.reviews)));
                                send(tx, Msg::Mal(gen, MalPart::Characters(b.characters)));
                                send(tx, Msg::Mal(gen, MalPart::Staff(b.staff)));
                                send(tx, Msg::Mal(gen, MalPart::Done));
                                return;
                            }
                            Ok(None) => {}
                            Err(e) => {
                                send(tx, Msg::Mal(gen, MalPart::Error(e)));
                                return;
                            }
                        }
                    }
                    send(tx, Msg::Mal(gen, MalPart::Id(None)));
                    return;
                }
                Err(e) => {
                    send(tx, Msg::Mal(gen, MalPart::Error(e)));
                    return;
                }
            };
            send(tx, Msg::Mal(gen, MalPart::Id(Some(id))));
            if !acc.access_token.is_empty() || !acc.refresh_token.is_empty() {
                if let Ok((tok, newt)) = fresh_token(&*http, &acc) {
                    if let Some(t) = newt {
                        send(tx, Msg::MalTokens(t));
                    }
                    let mine = mal::Api { http: &*http, token: tok }.my_status(id).ok().flatten();
                    send(tx, Msg::Mal(gen, MalPart::My(mine)));
                }
            }
            let cached = cache.lock().ok().and_then(|c| c.get(&id).cloned());
            if let Some(b) = cached {
                if let Some(i) = b.info {
                    send(tx, Msg::Mal(gen, MalPart::Info(i)));
                }
                send(tx, Msg::Mal(gen, MalPart::Recs(b.recs)));
                send(tx, Msg::Mal(gen, MalPart::Reviews(b.reviews)));
                send(tx, Msg::Mal(gen, MalPart::Characters(b.characters)));
                send(tx, Msg::Mal(gen, MalPart::Staff(b.staff)));
                send(tx, Msg::Mal(gen, MalPart::Done));
                return;
            }
            if let Some(b) = saved_dir.as_ref().and_then(|dir| load_bundle(dir, id)) {
                if let Some(i) = b.info {
                    send(tx, Msg::Mal(gen, MalPart::Info(i)));
                }
                send(tx, Msg::Mal(gen, MalPart::Recs(b.recs)));
                send(tx, Msg::Mal(gen, MalPart::Reviews(b.reviews)));
                send(tx, Msg::Mal(gen, MalPart::Characters(b.characters)));
                send(tx, Msg::Mal(gen, MalPart::Staff(b.staff)));
                send(tx, Msg::Mal(gen, MalPart::Done));
                return;
            }
            let jk = mal::Jikan { http: &*http };
            let mut bundle = MalBundle::default();
            let mut via_jikan = !mal::jikan_down();
            if via_jikan {
                match jk.info(id) {
                    Ok(i) => {
                        bundle.info = Some(i.clone());
                        send(tx, Msg::Mal(gen, MalPart::Info(i)));
                    }
                    // Whatever went wrong with Jikan (down, slow, busy, odd answer): AniList has the same page.
                    Err(_) => via_jikan = false,
                }
            }
            if via_jikan {
                if let Ok(v) = jk.recommendations(id) {
                    bundle.recs = v.clone();
                    send(tx, Msg::Mal(gen, MalPart::Recs(v)));
                }
                if let Ok(v) = jk.reviews(id) {
                    bundle.reviews = v.clone();
                    send(tx, Msg::Mal(gen, MalPart::Reviews(v)));
                }
                if let Ok(v) = jk.characters(id) {
                    bundle.characters = v.clone();
                    send(tx, Msg::Mal(gen, MalPart::Characters(v)));
                }
                if let Ok(v) = jk.staff(id) {
                    bundle.staff = v.clone();
                    send(tx, Msg::Mal(gen, MalPart::Staff(v)));
                }
            } else {
                match anilist::full(&*http, id) {
                    Ok(f) => {
                        if let Some(i) = f.info {
                            bundle.info = Some(i.clone());
                            send(tx, Msg::Mal(gen, MalPart::Info(i)));
                        }
                        bundle.recs = f.recs.clone();
                        send(tx, Msg::Mal(gen, MalPart::Recs(f.recs)));
                        bundle.reviews = f.reviews.clone();
                        send(tx, Msg::Mal(gen, MalPart::Reviews(f.reviews)));
                        bundle.characters = f.characters.clone();
                        send(tx, Msg::Mal(gen, MalPart::Characters(f.characters)));
                        bundle.staff = f.staff.clone();
                        send(tx, Msg::Mal(gen, MalPart::Staff(f.staff)));
                    }
                    Err(e) => {
                        send(tx, Msg::Mal(gen, MalPart::Error(e)));
                        return;
                    }
                }
            }
            if let Some(dir) = saved_dir.as_ref() {
                save_bundle(dir, id, &bundle);
            }
            if let Ok(mut c) = cache.lock() {
                c.insert(id, bundle);
            }
            send(tx, Msg::Mal(gen, MalPart::Done));
        });
    }

    /// "Try again" on the Info page.
    pub fn retry_mal(&mut self) {
        self.start_mal_lookup();
    }

    pub(super) fn on_mal(&mut self, gen: u64, part: MalPart) {
        let mut found_id = false;
        {
            let Some(d) = self.detail.as_mut().filter(|d| d.gen == gen) else { return };
            match part {
                MalPart::Id(None) => {
                    d.mal.loading = false;
                    d.mal.not_found = true;
                }
                MalPart::Id(Some(i)) => {
                    d.mal.id = Some(i);
                    found_id = true;
                }
                MalPart::Info(i) => d.mal.info = Some(i),
                MalPart::Recs(v) => d.mal.recs = v,
                MalPart::Reviews(v) => d.mal.reviews = v,
                MalPart::Characters(v) => d.mal.characters = v,
                MalPart::Staff(v) => d.mal.staff = v,
                MalPart::My(m) => d.mal.my = m,
                MalPart::Source(s) => d.mal.source = s,
                MalPart::Error(e) => {
                    d.mal.error = e;
                    d.mal.loading = false;
                }
                MalPart::Done => d.mal.loading = false,
            }
        }
        if found_id {
            self.refresh_external_watched();
        }
        self.events.push(Event::Mal);
    }

    // ----- editing the user's entry -----

    /// Saves status, score (0 = none) and watched episodes of the open title on MyAnimeList.
    pub fn set_my_status(&mut self, status: &str, score: u32, watched: u32) {
        let Some(id) = self.detail.as_ref().and_then(|d| d.mal.id) else { return };
        let acc = self.settings.mal.clone();
        let s = MyStatus { status: status.to_string(), score: score.min(10), watched };
        // Completed: the episodes of the season on screen count as watched here too.
        if status == "completed" {
            self.mark(MarkScope::Season, true);
        }
        self.spawn(move |tx, http| {
            let r = fresh_token(&*http, &acc).and_then(|(tok, newt)| {
                if let Some(t) = newt {
                    send(tx, Msg::MalTokens(t));
                }
                mal::Api { http: &*http, token: tok }.set_status(id, &s).map(Some)
            });
            send(tx, Msg::MalMy(id, r));
        });
    }

    /// Takes the title off the user's list.
    pub fn remove_my_status(&mut self) {
        let Some(id) = self.detail.as_ref().and_then(|d| d.mal.id) else { return };
        let acc = self.settings.mal.clone();
        self.spawn(move |tx, http| {
            let r = fresh_token(&*http, &acc).and_then(|(tok, newt)| {
                if let Some(t) = newt {
                    send(tx, Msg::MalTokens(t));
                }
                mal::Api { http: &*http, token: tok }.remove(id).map(|_| None)
            });
            send(tx, Msg::MalMy(id, r));
        });
    }

    pub(super) fn on_mal_my(&mut self, id: u64, r: Result<Option<MyStatus>, String>) {
        match r {
            Err(e) => self.toast(&e),
            Ok(my) => {
                if let Some(d) = self.detail.as_mut().filter(|d| d.mal.id == Some(id)) {
                    d.mal.my = my.clone();
                }
                match &my {
                    Some(s) => match self.mal_list.iter_mut().find(|e| e.mal_id == id) {
                        Some(e) => {
                            e.status = s.status.clone();
                            e.progress = s.watched;
                            e.score = s.score;
                        }
                        None => self.mal_list.push(ListEntry { mal_id: id, status: s.status.clone(), progress: s.watched, score: s.score, ..Default::default() }),
                    },
                    None => self.mal_list.retain(|e| e.mal_id != id),
                }
                self.refresh_external_watched();
                self.toast(if my.is_some() { "Saved on MyAnimeList." } else { "Removed from your MyAnimeList." });
                self.events.push(Event::Mal);
            }
        }
    }

    // ----- the list and the login -----

    /// Reads the user's list from AniSync (when installed) and from MyAnimeList (when logged in).
    pub fn refresh_mal_list(&mut self) {
        let anisync = self.settings.addons.iter().find(|a| a.enabled && a.url.contains("anisync")).map(|a| a.url.clone());
        let acc = self.settings.mal.clone();
        let logged = !acc.access_token.is_empty() || !acc.refresh_token.is_empty();
        if anisync.is_none() && !logged {
            return;
        }
        self.spawn(move |tx, http| {
            let mut errors = Vec::new();
            let (mut a, mut o) = (Vec::new(), Vec::new());
            if let Some(u) = anisync {
                let (l, e) = mal::fetch_anisync(&*http, &u);
                a = l;
                errors.extend(e);
            }
            if logged {
                match fresh_token(&*http, &acc) {
                    Ok((tok, newt)) => {
                        if let Some(t) = newt {
                            send(tx, Msg::MalTokens(t));
                        }
                        match (mal::Api { http: &*http, token: tok }).list() {
                            Ok(l) => o = l,
                            Err(e) => errors.push(e),
                        }
                    }
                    Err(e) => errors.push(e),
                }
            }
            send(tx, Msg::MalList(mal::merge_lists(a, o), errors));
        });
    }

    pub(super) fn on_mal_list(&mut self, list: Vec<ListEntry>, errors: Vec<String>) {
        // A failed read must not wipe a list that was read before.
        if !list.is_empty() || errors.is_empty() {
            self.mal_list = list;
        }
        self.mal_list_info = if errors.is_empty() { format!("{} titles on your MyAnimeList.", self.mal_list.len()) } else { errors.join(" · ") };
        self.refresh_external_watched();
        self.events.push(Event::Info(MsgKind::Mal, self.mal_list_info.clone()));
    }

    pub fn mal_logged_in(&self) -> bool {
        !self.settings.mal.access_token.is_empty() || !self.settings.mal.refresh_token.is_empty()
    }

    /// Opens MyAnimeList in the browser and waits for the answer on localhost.
    pub fn mal_login(&mut self) {
        let id = self.settings.mal.client_id.trim().to_string();
        if id.is_empty() {
            self.events.push(Event::Info(MsgKind::Mal, "Paste your Client ID first (steps above).".to_string()));
            return;
        }
        if !mal::valid_client_id(&id) {
            self.events.push(Event::Info(
                MsgKind::Mal,
                "That is not a Client ID. It is a code of 32 letters and numbers shown on myanimelist.net/apiconfig after you create an app (not your username or password).".to_string(),
            ));
            return;
        }
        self.settings.mal.client_id = id.clone();
        self.settings.mal.client_secret = self.settings.mal.client_secret.trim().to_string();
        self.save_settings();
        self.mal_cancel.store(true, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        self.mal_cancel = cancel.clone();
        let verifier = mal::new_verifier();
        let state: String = mal::new_verifier().chars().take(20).collect();
        let redirect = mal::effective_redirect(&self.settings.mal.redirect);
        let local = mal::is_local_redirect(&redirect);
        let url = mal::auth_url(&id, &verifier, &state, &redirect);
        self.mal_pending = Some((verifier.clone(), state.clone()));
        self.events.push(Event::OpenUrl(url));
        self.events.push(Event::Info(
            MsgKind::Mal,
            if local {
                "Allow Reelhouse in the browser. If the browser asks for a username and password, the Redirect URL of your app on myanimelist.net/apiconfig does not list the address shown above: add it there. If the browser does not come back here, copy the address of its last page and paste it below.".to_string()
            } else {
                "Allow Reelhouse in the browser. It will end on your own redirect page: copy the full address of that page and paste it below.".to_string()
            },
        ));
        if !local {
            return;
        }
        let secret = self.settings.mal.client_secret.clone();
        let redirect2 = redirect.clone();
        self.spawn(move |tx, http| {
            let r = (|| -> Result<(mal::Tokens, String), String> {
                let code = mal::wait_for_code(&state, 300, &cancel)?;
                let t = mal::exchange(&*http, &id, &secret, &redirect2, &code, &verifier)?;
                let name = mal::Api { http: &*http, token: t.access.clone() }.me().unwrap_or_default();
                Ok((t, name))
            })();
            // A cancelled wait (because the address was pasted instead) must not report an error.
            if cancel.load(Ordering::SeqCst) && r.is_err() {
                return;
            }
            send(tx, Msg::MalLogin(r));
        });
    }

    /// The browser could not come back to the app: the person pastes the address it ended on.
    pub fn mal_finish_with(&mut self, pasted: &str) {
        let Some((verifier, state)) = self.mal_pending.clone() else {
            self.events.push(Event::Info(MsgKind::Mal, "Press Log in first, then paste the address.".to_string()));
            return;
        };
        let cb = mal::parse_callback(pasted);
        if let Some(e) = cb.error {
            self.events.push(Event::Info(MsgKind::Mal, format!("MyAnimeList refused the login ({}).", e)));
            return;
        }
        let Some(code) = cb.code else {
            self.events.push(Event::Info(MsgKind::Mal, "That address has no login code. Copy the whole address of the page, it starts with http://localhost:51789/callback?code=".to_string()));
            return;
        };
        if cb.state.as_deref().map(|s| s != state).unwrap_or(false) {
            self.events.push(Event::Info(MsgKind::Mal, "That address belongs to an older login. Press Log in again and use the new one.".to_string()));
            return;
        }
        // Stop waiting on localhost: this answer is the one that counts.
        self.mal_cancel.store(true, Ordering::SeqCst);
        self.mal_pending = None;
        let (id, secret) = (self.settings.mal.client_id.clone(), self.settings.mal.client_secret.clone());
        let redirect = mal::effective_redirect(&self.settings.mal.redirect);
        self.events.push(Event::Info(MsgKind::Mal, "Finishing the login…".to_string()));
        self.spawn(move |tx, http| {
            let r = (|| -> Result<(mal::Tokens, String), String> {
                let t = mal::exchange(&*http, &id, &secret, &redirect, &code, &verifier)?;
                let name = mal::Api { http: &*http, token: t.access.clone() }.me().unwrap_or_default();
                Ok((t, name))
            })();
            send(tx, Msg::MalLogin(r));
        });
    }

    pub fn mal_cancel_login(&mut self) {
        self.mal_cancel.store(true, Ordering::SeqCst);
    }

    pub fn mal_logout(&mut self) {
        self.settings.mal.access_token.clear();
        self.settings.mal.refresh_token.clear();
        self.settings.mal.expires_at = 0;
        self.settings.mal.user.clear();
        self.save_settings();
        self.events.push(Event::Info(MsgKind::Mal, "Logged out.".to_string()));
        if let Some(d) = self.detail.as_mut() {
            d.mal.my = None;
        }
        self.refresh_mal_list();
    }

    pub(super) fn on_mal_login(&mut self, r: Result<(mal::Tokens, String), String>) {
        self.mal_pending = None;
        match r {
            Ok((t, name)) => {
                // Bring the app back to the front: the browser is on top after the login.
                self.events.push(Event::Raise);
                let m = &mut self.settings.mal;
                m.access_token = t.access;
                m.refresh_token = t.refresh;
                m.expires_at = util::now_unix() + t.expires_in;
                m.user = name.clone();
                self.save_settings();
                let who = if name.is_empty() { "your account".to_string() } else { name };
                self.events.push(Event::Info(MsgKind::Mal, format!("Logged in as {}.", who)));
                self.refresh_mal_list();
                if self.detail.is_some() {
                    self.start_mal_lookup();
                }
            }
            Err(e) => self.events.push(Event::Info(MsgKind::Mal, e)),
        }
    }

    pub(super) fn on_mal_tokens(&mut self, t: mal::Tokens) {
        let m = &mut self.settings.mal;
        m.access_token = t.access;
        if !t.refresh.is_empty() {
            m.refresh_token = t.refresh;
        }
        m.expires_at = util::now_unix() + t.expires_in;
        self.save_settings();
    }

    // ----- "because you watched" -----

    /// Rows of titles that people who liked what you watch also liked (AniList), for the Discover page.
    pub fn load_taste(&mut self) {
        let animeish = |m: &MetaItem| {
            m.mal_id.is_some()
                || m.id.starts_with("kitsu:")
                || m.id.starts_with("mal:")
                || m.kind == "anime"
                || m.genres.iter().any(|g| g.eq_ignore_ascii_case("animation") || g.eq_ignore_ascii_case("anime"))
        };
        let mut recent: Vec<&Progress> = self.progress.iter().collect();
        recent.sort_by(|a, b| b.updated.cmp(&a.updated));
        let mut picks: Vec<MetaItem> = Vec::new();
        let sources = recent.iter().map(|p| &p.meta).chain(self.library.iter()).chain(self.favorites.iter());
        for m in sources {
            if animeish(m) && !picks.iter().any(|x| x.id == m.id) {
                picks.push(m.clone());
            }
            if picks.len() >= 4 {
                break;
            }
        }
        self.taste_gen += 1;
        let gen = self.taste_gen;
        if picks.is_empty() {
            self.taste.clear();
            return;
        }
        self.spawn(move |tx, http| {
            let mut rows = Vec::new();
            for m in picks {
                let year = m.release_info.chars().skip_while(|c| !c.is_ascii_digit()).take(4).collect::<String>().parse::<i64>().ok();
                if let Ok((found, cards)) = anilist::recommendations_for(&*http, &m.name, year, 1) {
                    let own = found.map(|f| f.id);
                    let items: Vec<MetaItem> = cards.iter().filter(|c| Some(c.id) != own).map(|c| c.to_meta()).collect();
                    if items.len() >= 3 {
                        rows.push(Row { title: format!("Because you watched {}", m.name), addon: "AniList".to_string(), kind: "series".to_string(), items });
                    }
                }
            }
            send(tx, Msg::Taste(gen, rows));
        });
    }

    pub(super) fn on_taste(&mut self, gen: u64, rows: Vec<Row>) {
        if gen != self.taste_gen {
            return;
        }
        self.taste = rows.into_iter().map(Arc::new).collect();
        self.events.push(Event::Home);
    }

    // ----- genre pages -----

    pub fn open_genre(&mut self, id: u64, name: &str, kind: &str) {
        self.genre_gen += 1;
        self.genre = Some(GenrePage { gen: self.genre_gen, id, kind: kind.to_string(), name: name.to_string(), items: vec![], page: 0, more: false, loading: false, error: String::new() });
        self.events.push(Event::Genre);
        self.genre_more();
    }

    /// Loads the next page of the open category.
    pub fn genre_more(&mut self) {
        let Some(g) = self.genre.as_mut() else { return };
        if g.loading || (g.page > 0 && !g.more) {
            return;
        }
        g.loading = true;
        g.error.clear();
        let (gen, id, page, kind, name, have) = (g.gen, g.id, g.page + 1, g.kind.clone(), g.name.clone(), g.items.len());
        self.events.push(Event::Genre);
        self.spawn(move |tx, http| {
            let r = match kind.as_str() {
                "movie" | "series" => mal::cinemeta_genre(&*http, &kind, &name, have),
                _ => {
                    let jikan = if id != 0 && !mal::jikan_down() { Some(mal::Jikan { http: &*http }.by_genre(id, page)) } else { None };
                    match jikan {
                        Some(Ok(r)) => Ok(r),
                        // Jikan is down, or the category has no MyAnimeList id: AniList knows it by name.
                        _ => anilist::by_genre(&*http, &name, page),
                    }
                }
            };
            send(tx, Msg::Genre(gen, page, r));
        });
    }

    pub(super) fn on_genre(&mut self, gen: u64, page: u32, r: Result<(Vec<Card>, bool), String>) {
        let Some(g) = self.genre.as_mut().filter(|g| g.gen == gen) else { return };
        g.loading = false;
        match r {
            Ok((cards, more)) => {
                g.page = page;
                g.more = more;
                for c in cards {
                    if !g.items.iter().any(|x| x.id == c.id && x.key == c.key) {
                        g.items.push(c);
                    }
                }
            }
            Err(e) => g.error = e,
        }
        self.events.push(Event::Genre);
    }

    pub fn close_genre(&mut self) {
        self.genre = None;
    }

    pub fn toggle_watched_external(&mut self, id: &str) -> bool {
        // Used by `toggle_watched` for marks that come from the list.
        if self.watched_externally(id) {
            self.ext_off.insert(id.to_string());
            true
        } else {
            self.ext_off.remove(id);
            false
        }
    }
}

// ----- the MyAnimeList page of a saved title, kept on disk (<data>/mal-cache/<id>.json) -----

fn mal_file(dir: &std::path::Path, id: u64) -> std::path::PathBuf {
    dir.join("mal-cache").join(format!("{id}.json"))
}

fn resolve_key(meta_id: &str, season: i64) -> String {
    format!("{}|{}", meta_id, season)
}

fn resolve_file(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("mal-cache").join("resolved.json")
}

/// What a saved title was matched with on MyAnimeList: its id, or 0 when it is not there.
fn kept_id(dir: &std::path::Path, key: &str) -> Option<u64> {
    let t = std::fs::read_to_string(resolve_file(dir)).ok()?;
    crate::json::Json::parse(&t).ok()?.get(key).as_u64()
}

fn keep_id(dir: &std::path::Path, key: &str, id: u64) {
    let f = resolve_file(dir);
    let mut j = std::fs::read_to_string(&f).ok().and_then(|t| crate::json::Json::parse(&t).ok()).filter(|j| matches!(j, crate::json::Json::Obj(_))).unwrap_or_else(|| crate::json::Json::obj(vec![]));
    if j.get(key).as_u64() == Some(id) {
        return;
    }
    j.set(key, crate::json::Json::Num(id as f64));
    if let Some(parent) = f.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(f, j.dump());
}

fn tmdb_file(dir: &std::path::Path, imdb: &str, kind: &str, lang: &str) -> std::path::PathBuf {
    dir.join("mal-cache").join(format!("tmdb-{}-{}-{}.json", imdb, kind, lang))
}

fn save_tmdb(dir: &std::path::Path, imdb: &str, kind: &str, lang: &str, b: &MalBundle) {
    let f = tmdb_file(dir, imdb, kind, lang);
    if let Some(parent) = f.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(f, bundle_json(b).pretty());
}

fn load_tmdb(dir: &std::path::Path, imdb: &str, kind: &str, lang: &str) -> Option<MalBundle> {
    let t = std::fs::read_to_string(tmdb_file(dir, imdb, kind, lang)).ok()?;
    bundle_from(&crate::json::Json::parse(&t).ok()?)
}

impl Core {
    /// The title on screen was just added to the library or the favourites: its page is kept now, so the next time it opens at once.
    pub fn keep_open_info(&self) {
        let Some(d) = self.detail.as_ref() else { return };
        if d.mal.loading || d.mal.info.is_none() || !self.is_saved(&d.meta.id) {
            return;
        }
        let b = MalBundle { info: d.mal.info.clone(), recs: d.mal.recs.clone(), reviews: d.mal.reviews.clone(), characters: d.mal.characters.clone(), staff: d.mal.staff.clone() };
        let key = resolve_key(&d.meta.id, d.season);
        match (d.mal.id, d.mal.source.as_str()) {
            (Some(id), _) => {
                keep_id(&self.data_dir, &key, id);
                save_bundle(&self.data_dir, id, &b);
            }
            (None, "tmdb") => {
                let imdb = if d.meta.imdb_id.starts_with("tt") { d.meta.imdb_id.clone() } else { d.meta.id.split(':').next().unwrap_or("").to_string() };
                keep_id(&self.data_dir, &key, 0);
                save_tmdb(&self.data_dir, &imdb, &d.meta.kind, &self.ui_lang(), &b);
            }
            _ => {}
        }
    }
}

/// The page is written once a fresh lookup of a saved title is done.
fn save_bundle(dir: &std::path::Path, id: u64, b: &MalBundle) {
    let f = mal_file(dir, id);
    if let Some(parent) = f.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(f, bundle_json(b).pretty());
}

/// The kept page of a title, if there is one.
fn load_bundle(dir: &std::path::Path, id: u64) -> Option<MalBundle> {
    let t = std::fs::read_to_string(mal_file(dir, id)).ok()?;
    bundle_from(&crate::json::Json::parse(&t).ok()?)
}

use crate::json::Json;

fn num(x: f64) -> Json {
    Json::Num(x)
}
fn text(s: &str) -> Json {
    Json::Str(s.to_string())
}
fn strs(v: &[String]) -> Json {
    Json::Arr(v.iter().map(|s| text(s)).collect())
}
fn named_json(v: &[mal::Named]) -> Json {
    Json::Arr(v.iter().map(|n| Json::obj(vec![("id", num(n.id as f64)), ("name", text(&n.name))])).collect())
}
fn pairs_json(v: &[(String, String)]) -> Json {
    Json::Arr(v.iter().map(|(a, b)| Json::obj(vec![("a", text(a)), ("b", text(b))])).collect())
}
fn info_json(i: &Info) -> Json {
    Json::obj(vec![
        ("provider", text(&i.provider)),
        ("id", num(i.id as f64)),
        ("url", text(&i.url)),
        ("title", text(&i.title)),
        ("title_en", text(&i.title_en)),
        ("title_ja", text(&i.title_ja)),
        ("synonyms", strs(&i.synonyms)),
        ("image", text(&i.image)),
        ("trailer", text(&i.trailer)),
        ("kind", text(&i.kind)),
        ("source", text(&i.source)),
        ("episodes", i.episodes.map(|e| num(e as f64)).unwrap_or(Json::Null)),
        ("status", text(&i.status)),
        ("aired", text(&i.aired)),
        ("premiered", text(&i.premiered)),
        ("broadcast", text(&i.broadcast)),
        ("duration", text(&i.duration)),
        ("rating", text(&i.rating)),
        ("score", num(i.score)),
        ("scored_by", num(i.scored_by as f64)),
        ("rank", num(i.rank as f64)),
        ("popularity", num(i.popularity as f64)),
        ("members", num(i.members as f64)),
        ("favorites", num(i.favorites as f64)),
        ("synopsis", text(&i.synopsis)),
        ("background", text(&i.background)),
        ("producers", named_json(&i.producers)),
        ("licensors", named_json(&i.licensors)),
        ("studios", named_json(&i.studios)),
        ("genres", named_json(&i.genres)),
        ("themes", named_json(&i.themes)),
        ("demographics", named_json(&i.demographics)),
        ("openings", strs(&i.openings)),
        ("endings", strs(&i.endings)),
        ("streaming", pairs_json(&i.streaming)),
        ("external", pairs_json(&i.external)),
        ("facts", pairs_json(&i.facts)),
        (
            "relations",
            Json::Arr(
                i.relations
                    .iter()
                    .map(|r| {
                        Json::obj(vec![
                            ("kind", text(&r.kind)),
                            ("entries", Json::Arr(r.entries.iter().map(|e| Json::obj(vec![("id", num(e.id as f64)), ("kind", text(&e.kind)), ("name", text(&e.name))])).collect())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}
fn cards_json(v: &[Card]) -> Json {
    Json::Arr(
        v.iter()
            .map(|c| {
                Json::obj(vec![
                    ("id", num(c.id as f64)),
                    ("title", text(&c.title)),
                    ("image", text(&c.image)),
                    ("score", num(c.score)),
                    ("kind", text(&c.kind)),
                    ("year", num(c.year as f64)),
                    ("votes", num(c.votes as f64)),
                    ("key", text(&c.key)),
                    ("synopsis", text(&c.synopsis)),
                    ("genres", strs(&c.genres)),
                ])
            })
            .collect(),
    )
}
fn bundle_json(b: &MalBundle) -> Json {
    Json::obj(vec![
        ("info", b.info.as_ref().map(info_json).unwrap_or(Json::Null)),
        ("recs", cards_json(&b.recs)),
        (
            "reviews",
            Json::Arr(
                b.reviews
                    .iter()
                    .map(|r| {
                        Json::obj(vec![
                            ("user", text(&r.user)),
                            ("avatar", text(&r.avatar)),
                            ("date", text(&r.date)),
                            ("tag", text(&r.tag)),
                            ("score", num(r.score as f64)),
                            ("text", text(&r.text)),
                            ("spoiler", Json::Bool(r.spoiler)),
                            ("url", text(&r.url)),
                            ("reactions", num(r.reactions as f64)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "characters",
            Json::Arr(
                b.characters
                    .iter()
                    .map(|c| {
                        Json::obj(vec![
                            ("id", num(c.id as f64)),
                            ("name", text(&c.name)),
                            ("role", text(&c.role)),
                            ("image", text(&c.image)),
                            ("actor", text(&c.actor)),
                            ("actor_lang", text(&c.actor_lang)),
                            ("actor_image", text(&c.actor_image)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("staff", Json::Arr(b.staff.iter().map(|s| Json::obj(vec![("name", text(&s.name)), ("image", text(&s.image)), ("positions", text(&s.positions))])).collect())),
    ])
}

/// Reads a page written by `bundle_json`. Missing parts come back empty.
fn bundle_from(j: &Json) -> Option<MalBundle> {
    fn arr(j: &Json) -> &[Json] {
        match j {
            Json::Arr(v) => v,
            _ => &[],
        }
    }
    fn named(j: &Json) -> Vec<mal::Named> {
        arr(j).iter().map(|n| mal::Named { id: n.get("id").as_u64().unwrap_or(0), name: n.get("name").str() }).collect()
    }
    fn pairs(j: &Json) -> Vec<(String, String)> {
        arr(j).iter().map(|p| (p.get("a").str(), p.get("b").str())).collect()
    }
    let info = match j.get("info") {
        Json::Obj(_) => {
            let g = |k: &str| j.get("info").get(k).str();
            let n = |k: &str| j.get("info").get(k).as_u64().unwrap_or(0);
            let f = |k: &str| j.get("info").get(k).as_f64().unwrap_or(0.0);
            Some(Info {
                provider: g("provider"),
                id: n("id"),
                url: g("url"),
                title: g("title"),
                title_en: g("title_en"),
                title_ja: g("title_ja"),
                synonyms: j.get("info").get("synonyms").strings(),
                image: g("image"),
                trailer: g("trailer"),
                kind: g("kind"),
                source: g("source"),
                episodes: j.get("info").get("episodes").as_u64().map(|e| e as u32),
                status: g("status"),
                aired: g("aired"),
                premiered: g("premiered"),
                broadcast: g("broadcast"),
                duration: g("duration"),
                rating: g("rating"),
                score: f("score"),
                scored_by: n("scored_by"),
                rank: n("rank"),
                popularity: n("popularity"),
                members: n("members"),
                favorites: n("favorites"),
                synopsis: g("synopsis"),
                background: g("background"),
                producers: named(j.get("info").get("producers")),
                licensors: named(j.get("info").get("licensors")),
                studios: named(j.get("info").get("studios")),
                genres: named(j.get("info").get("genres")),
                themes: named(j.get("info").get("themes")),
                demographics: named(j.get("info").get("demographics")),
                openings: j.get("info").get("openings").strings(),
                endings: j.get("info").get("endings").strings(),
                streaming: pairs(j.get("info").get("streaming")),
                external: pairs(j.get("info").get("external")),
                facts: pairs(j.get("info").get("facts")),
                relations: arr(j.get("info").get("relations"))
                    .iter()
                    .map(|r| mal::Relation {
                        kind: r.get("kind").str(),
                        entries: arr(r.get("entries"))
                            .iter()
                            .map(|e| mal::Related { id: e.get("id").as_u64().unwrap_or(0), kind: e.get("kind").str(), name: e.get("name").str() })
                            .collect(),
                    })
                    .collect(),
            })
        }
        _ => None,
    };
    let recs = arr(j.get("recs"))
        .iter()
        .map(|c| Card {
            id: c.get("id").as_u64().unwrap_or(0),
            title: c.get("title").str(),
            image: c.get("image").str(),
            score: c.get("score").as_f64().unwrap_or(0.0),
            kind: c.get("kind").str(),
            year: c.get("year").as_i64().unwrap_or(0),
            votes: c.get("votes").as_u64().unwrap_or(0),
            key: c.get("key").str(),
            synopsis: c.get("synopsis").str(),
            genres: c.get("genres").strings(),
        })
        .collect();
    let reviews = arr(j.get("reviews"))
        .iter()
        .map(|r| Review {
            user: r.get("user").str(),
            avatar: r.get("avatar").str(),
            date: r.get("date").str(),
            tag: r.get("tag").str(),
            score: r.get("score").as_u64().unwrap_or(0) as u32,
            text: r.get("text").str(),
            spoiler: r.get("spoiler").as_bool().unwrap_or(false),
            url: r.get("url").str(),
            reactions: r.get("reactions").as_u64().unwrap_or(0) as u32,
        })
        .collect();
    let characters = arr(j.get("characters"))
        .iter()
        .map(|c| Character {
            id: c.get("id").as_u64().unwrap_or(0),
            name: c.get("name").str(),
            role: c.get("role").str(),
            image: c.get("image").str(),
            actor: c.get("actor").str(),
            actor_lang: c.get("actor_lang").str(),
            actor_image: c.get("actor_image").str(),
        })
        .collect();
    let staff = arr(j.get("staff"))
        .iter()
        .map(|s| Staff { name: s.get("name").str(), image: s.get("image").str(), positions: s.get("positions").str() })
        .collect();
    Some(MalBundle { info, recs, reviews, characters, staff })
}
