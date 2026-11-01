//! Marking many episodes at once, filling in the library cards, and opening a card from a list.
//! This file is part of `controller` (it needs the private parts of `Core`).
use super::*;

/// Which episodes a mark applies to.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkScope {
    /// Every episode before this one (of its season).
    Before(String),
    /// Every episode after this one (of its season).
    After(String),
    /// The season on screen.
    Season,
    /// Every episode of every season.
    All,
}

/// The ids an episode mark reaches. Pure, so it is tested.
pub fn scope_ids(meta: &MetaItem, season: i64, scope: &MarkScope) -> Vec<String> {
    match scope {
        MarkScope::Season => meta.episodes(season).iter().map(|v| v.id.clone()).collect(),
        MarkScope::All => meta.seasons().into_iter().flat_map(|s| meta.episodes(s).into_iter().map(|v| v.id.clone()).collect::<Vec<_>>()).collect(),
        MarkScope::Before(id) | MarkScope::After(id) => {
            let eps = meta.episodes(season);
            let Some(at) = eps.iter().position(|v| &v.id == id) else { return vec![] };
            let before = matches!(scope, MarkScope::Before(_));
            eps.iter().enumerate().filter(|(i, _)| if before { *i < at } else { *i > at }).map(|(_, v)| v.id.clone()).collect()
        }
    }
}

/// Does a card already carry what the library page shows?
fn complete(m: &MetaItem) -> bool {
    m.poster.is_some() && !m.description.is_empty() && !m.rating.is_empty()
}

/// Looks a title up in the catalog: the best match for the name, with its year when known.
fn search_catalog(http: &dyn Http, name: &str, year: Option<i64>, prefer_movie: bool) -> Option<MetaItem> {
    let want: String = name.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect();
    if want.is_empty() {
        return None;
    }
    let kinds = if prefer_movie { ["movie", "series"] } else { ["series", "movie"] };
    let mut fallback: Option<MetaItem> = None;
    for kind in kinds {
        let url = format!("https://v3-cinemeta.strem.io/catalog/{}/top/search={}.json", kind, crate::util::url_encode(name));
        let Ok(j) = crate::http::get_json(http, &url, 15) else { continue };
        for m in j.get("metas").items() {
            let mut item = MetaItem::from_json(m);
            if item.id.is_empty() || item.name.is_empty() {
                continue;
            }
            if item.kind.is_empty() {
                item.kind = kind.to_string();
            }
            let got: String = item.name.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect();
            let same_year = year.map(|y| item.release_info.contains(&y.to_string())).unwrap_or(true);
            if got == want && same_year {
                return Some(item);
            }
            if fallback.is_none() && (got == want || got.contains(&want) || want.contains(&got)) {
                fallback = Some(item);
            }
        }
        if fallback.is_some() {
            break;
        }
    }
    fallback
}

impl Core {
    /// Marks the episodes of `scope` as watched (or not watched) in one go.
    pub fn mark(&mut self, scope: MarkScope, watched: bool) {
        let Some(d) = self.detail.as_ref() else { return };
        let ids = scope_ids(&d.meta, d.season, &scope);
        if ids.is_empty() {
            return;
        }
        let n = ids.len();
        let mut progress_changed = false;
        for id in ids {
            if watched {
                self.watched.insert(id.clone());
                self.ext_off.remove(&id);
                if let Some(i) = self.progress.iter().position(|p| p.video_id == id) {
                    self.progress.remove(i);
                    progress_changed = true;
                }
            } else {
                self.watched.remove(&id);
                // A mark that comes from the MyAnimeList list is switched off for this session.
                if self.external_watched.contains(&id) {
                    self.ext_off.insert(id);
                }
            }
        }
        self.save_watched();
        if progress_changed {
            self.save_progress_file();
            self.events.push(Event::Progress);
        }
        self.events.push(Event::Detail);
        self.toast(&if watched { format!("{} episodes marked as watched", n) } else { format!("{} episodes marked as not watched", n) });
    }

    /// Fills in cover, synopsis and rating of the titles in the library and favorites that lack them.
    pub fn fill_library(&mut self) {
        if self.library_filling {
            return;
        }
        let todo: Vec<MetaItem> = self.library.iter().chain(self.favorites.iter()).filter(|m| !complete(m)).cloned().collect();
        if todo.is_empty() {
            return;
        }
        self.library_filling = true;
        let entries = self.settings.addons.clone();
        self.spawn(move |tx, http| {
            let mut out = Vec::new();
            for m in todo {
                let mut got = addons::fetch_meta(&*http, &entries, &m.kind, &m.id);
                if got.as_ref().map(complete) != Some(true) && !m.id.starts_with("mal:") || got.is_none() {
                    let year: Option<i64> = m.release_info.split(|c: char| !c.is_ascii_digit()).find(|s| s.len() == 4).and_then(|s| s.parse().ok());
                    if let Some(found) = search_catalog(&*http, &m.name, year, m.kind == "movie") {
                        match got.as_mut() {
                            Some(g) => g.fill_from(&found),
                            None => got = Some(found),
                        }
                    }
                }
                if let Some(mut g) = got {
                    g.id = m.id.clone();
                    out.push(g);
                }
            }
            send(tx, Msg::LibraryFill(out));
        });
    }

    pub(super) fn on_library_fill(&mut self, found: Vec<MetaItem>) {
        self.library_filling = false;
        let mut changed = false;
        for f in &found {
            for list in [&mut self.library, &mut self.favorites] {
                if let Some(m) = list.iter_mut().find(|m| m.id == f.id) {
                    let before = (m.poster.clone(), m.description.clone(), m.rating.clone());
                    if m.poster.is_none() {
                        m.poster = f.poster.clone();
                    }
                    if m.description.is_empty() {
                        m.description = f.description.clone();
                    }
                    if m.rating.is_empty() {
                        m.rating = f.rating.clone();
                    }
                    if m.genres.is_empty() {
                        m.genres = f.genres.clone();
                    }
                    if m.release_info.is_empty() {
                        m.release_info = f.release_info.clone();
                    }
                    changed |= before != (m.poster.clone(), m.description.clone(), m.rating.clone());
                }
            }
        }
        if changed {
            let _ = settings::save_library(&self.profile_dir, &self.library);
            let _ = settings::save_list(&self.profile_dir, "favorites.json", &self.favorites);
            self.events.push(Event::Library);
        }
    }

    /// Opens a card from a list (similar anime, categories). Anime of MyAnimeList are looked up in the
    /// catalog first, so the page has episodes and streams; when nothing is found the card opens as it is.
    pub fn open_card(&mut self, card: &mal::Card) {
        let fallback = card.to_meta();
        if !card.key.is_empty() {
            self.events.push(Event::OpenItem(fallback));
            return;
        }
        self.card_gen += 1;
        let gen = self.card_gen;
        let (title, year) = (card.title.clone(), if card.year > 0 { Some(card.year) } else { None });
        let movie = card.kind.eq_ignore_ascii_case("movie");
        // The title page opens at once with what the card already shows (its picture, name, year); the catalogue search
        // runs behind it and the page is filled in when the real title is found (on_card_open).
        self.open_detail(fallback.clone());
        self.spawn(move |tx, http| {
            let mut found = search_catalog(&*http, &title, year, movie);
            if found.is_none() {
                // The same title without the year, then the first words (subtitles after ":" are often missing).
                found = search_catalog(&*http, &title, None, movie).or_else(|| {
                    let head = title.split(':').next().unwrap_or("").trim().to_string();
                    if head.is_empty() || head == title { None } else { search_catalog(&*http, &head, year, movie) }
                });
            }
            let item = found.unwrap_or(fallback);
            send(tx, Msg::CardOpen(gen, Box::new(item)));
        });
    }

    pub(super) fn on_card_open(&mut self, gen: u64, item: MetaItem) {
        if gen == self.card_gen {
            self.events.push(Event::OpenItem(item));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Video;

    fn show() -> MetaItem {
        let mut m = MetaItem::default();
        for (s, e) in [(1, 1), (1, 2), (1, 3), (2, 1), (2, 2)] {
            m.videos.push(Video { id: format!("x:{}:{}", s, e), season: Some(s), episode: Some(e), ..Default::default() });
        }
        m
    }

    #[test]
    fn ranges() {
        let m = show();
        assert_eq!(scope_ids(&m, 1, &MarkScope::Before("x:1:3".into())), vec!["x:1:1", "x:1:2"]);
        assert_eq!(scope_ids(&m, 1, &MarkScope::After("x:1:1".into())), vec!["x:1:2", "x:1:3"]);
        assert_eq!(scope_ids(&m, 2, &MarkScope::Season).len(), 2);
        assert_eq!(scope_ids(&m, 1, &MarkScope::All).len(), 5);
        assert!(scope_ids(&m, 1, &MarkScope::Before("nope".into())).is_empty());
        assert!(scope_ids(&m, 1, &MarkScope::Before("x:1:1".into())).is_empty());
    }
}
