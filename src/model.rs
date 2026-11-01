//! Titles, episodes and catalog rows as the app uses them.
use crate::json::Json;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Video {
    pub id: String,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub title: String,
    pub thumbnail: Option<String>,
    pub released: String,
    /// Length of the episode in minutes, when the addon says.
    pub runtime_min: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetaItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub poster: Option<String>,
    pub background: Option<String>,
    pub logo: Option<String>,
    pub description: String,
    pub release_info: String,
    pub runtime: String,
    pub rating: String,
    pub genres: Vec<String>,
    pub cast: Vec<String>,
    pub videos: Vec<Video>,
    /// MyAnimeList and IMDb ids when the addon knows them (AniSync does).
    pub mal_id: Option<u64>,
    pub imdb_id: String,
}

/// "24 min", "1h 45min" or "45" as minutes.
pub fn parse_minutes(text: &str) -> Option<u32> {
    let t = text.to_lowercase();
    let nums: Vec<(usize, u32, usize)> = {
        let b = t.as_bytes();
        let mut v = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_digit() {
                let st = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if let Ok(n) = t[st..i].parse::<u32>() {
                    v.push((st, n, i));
                }
            } else {
                i += 1;
            }
        }
        v
    };
    let first = nums.first()?;
    let after: String = t[first.2..].trim_start().chars().take(1).collect();
    if after == "h" {
        let mins = nums.get(1).map(|n| n.1).unwrap_or(0);
        return Some(first.1 * 60 + mins);
    }
    if first.1 == 0 { None } else { Some(first.1) }
}

/// Minutes left in a season, the whole season, and the whole series.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeasonTimes {
    pub remaining: u64,
    pub season: u64,
    pub series: u64,
}

fn opt(j: &Json) -> Option<String> {
    let s = j.str();
    if s.is_empty() { None } else { Some(s) }
}

impl MetaItem {
    pub fn from_json(j: &Json) -> MetaItem {
        let mut name = j.get("name").str();
        if name.is_empty() {
            name = j.get("title").str();
        }
        let mut release = j.get("releaseInfo").str();
        if release.is_empty() {
            release = j.get("year").str();
        }
        let videos = j
            .get("videos")
            .items()
            .iter()
            .map(|v| {
                let mut title = v.get("title").str();
                if title.is_empty() {
                    title = v.get("name").str();
                }
                let ep = v.get("episode").as_i64().or_else(|| v.get("number").as_i64());
                Video {
                    id: v.get("id").str(),
                    season: v.get("season").as_i64(),
                    episode: ep,
                    title,
                    thumbnail: opt(v.get("thumbnail")),
                    released: v.get("released").str(),
                    runtime_min: parse_minutes(&v.get("runtime").str()),
                }
            })
            .filter(|v| !v.id.is_empty())
            .collect();
        MetaItem {
            id: j.get("id").str(),
            kind: j.get("type").str(),
            name,
            poster: opt(j.get("poster")),
            background: opt(j.get("background")),
            logo: opt(j.get("logo")),
            description: j.get("description").str(),
            release_info: release,
            runtime: j.get("runtime").str(),
            rating: j.get("imdbRating").str(),
            genres: j.get("genres").strings(),
            cast: j.get("cast").strings(),
            videos,
            mal_id: j.get("mal_id").as_u64(),
            imdb_id: j.get("imdb_id").str(),
        }
    }

    /// Compact form for the library file (episodes are fetched again when opened).
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj(vec![
            ("id", self.id.clone().into()),
            ("type", self.kind.clone().into()),
            ("name", self.name.clone().into()),
            ("releaseInfo", self.release_info.clone().into()),
        ]);
        if let Some(p) = &self.poster {
            o.set("poster", p.clone().into());
        }
        if let Some(p) = &self.background {
            o.set("background", p.clone().into());
        }
        if let Some(p) = &self.logo {
            o.set("logo", p.clone().into());
        }
        if let Some(m) = self.mal_id {
            o.set("mal_id", (m as f64).into());
        }
        if !self.imdb_id.is_empty() {
            o.set("imdb_id", self.imdb_id.clone().into());
        }
        o
    }

    /// The whole title in the form `from_json` reads back: what the page shows (description, episodes, cast...). Kept
    /// on disk for the titles of the library and the favourites, so their pages open without loading again.
    pub fn to_full_json(&self) -> Json {
        let mut o = self.to_json();
        o.set("description", self.description.clone().into());
        if !self.runtime.is_empty() {
            o.set("runtime", self.runtime.clone().into());
        }
        if !self.rating.is_empty() {
            o.set("imdbRating", self.rating.clone().into());
        }
        o.set("genres", Json::Arr(self.genres.iter().map(|g| g.clone().into()).collect()));
        o.set("cast", Json::Arr(self.cast.iter().map(|g| g.clone().into()).collect()));
        let videos = self
            .videos
            .iter()
            .map(|v| {
                let mut e = Json::obj(vec![("id", v.id.clone().into()), ("title", v.title.clone().into()), ("released", v.released.clone().into())]);
                if let Some(s) = v.season {
                    e.set("season", (s as f64).into());
                }
                if let Some(n) = v.episode {
                    e.set("episode", (n as f64).into());
                }
                if let Some(t) = &v.thumbnail {
                    e.set("thumbnail", t.clone().into());
                }
                if let Some(m) = v.runtime_min {
                    e.set("runtime", format!("{} min", m).into());
                }
                e
            })
            .collect();
        o.set("videos", Json::Arr(videos));
        o
    }

    /// Length of one episode: its own, else the series runtime.
    fn episode_minutes(&self, v: &Video) -> Option<u64> {
        v.runtime_min.or_else(|| parse_minutes(&self.runtime)).map(|m| m as u64)
    }

    /// True when at least one episode length is known.
    fn has_runtimes(&self) -> bool {
        self.videos.iter().any(|v| v.episode.is_some() && self.episode_minutes(v).is_some())
    }

    /// Time left, season length and series length. `watched` tells which episode ids were watched.
    /// None when no episode length is known.
    pub fn season_times(&self, season: i64, watched: &dyn Fn(&str) -> bool) -> Option<SeasonTimes> {
        let mut t = SeasonTimes { remaining: 0, season: 0, series: 0 };
        let mut known = false;
        // Catalogs often leave the length out: 24 minutes is the usual length of an anime episode.
        let guess = !self.has_runtimes();
        for v in self.videos.iter().filter(|v| v.episode.is_some()) {
            let sn = v.season.unwrap_or(1);
            if sn == 0 && season != 0 {
                continue;
            }
            let Some(m) = self.episode_minutes(v).or(if guess { Some(24) } else { None }) else { continue };
            known = true;
            t.series += m;
            if sn == season {
                t.season += m;
                if !watched(&v.id) {
                    t.remaining += m;
                }
            }
        }
        if known { Some(t) } else { None }
    }

    pub fn is_series(&self) -> bool {
        self.kind == "series" || self.videos.iter().any(|v| v.episode.is_some())
    }

    pub fn seasons(&self) -> Vec<i64> {
        let mut s: Vec<i64> = self.videos.iter().filter(|v| v.episode.is_some()).map(|v| v.season.unwrap_or(1)).collect();
        s.sort();
        s.dedup();
        s
    }

    pub fn episodes(&self, season: i64) -> Vec<&Video> {
        let mut e: Vec<&Video> = self.videos.iter().filter(|v| v.episode.is_some() && v.season.unwrap_or(1) == season).collect();
        e.sort_by_key(|v| v.episode.unwrap_or(0));
        e
    }

    /// Fills gaps from `other` (used to merge catalog data with richer metadata).
    pub fn fill_from(&mut self, other: &MetaItem) {
        macro_rules! fill {
            ($f:ident) => {
                if self.$f.is_none() {
                    self.$f = other.$f.clone();
                }
            };
        }
        fill!(poster);
        fill!(background);
        fill!(logo);
        if self.description.is_empty() {
            self.description = other.description.clone();
        }
        if self.name.is_empty() {
            self.name = other.name.clone();
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Row {
    pub title: String,
    pub addon: String,
    pub kind: String,
    pub items: Vec<MetaItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cinemeta_style_meta() {
        let j = Json::parse(
            r#"{"id":"tt1","type":"series","name":"Show","poster":"p","background":"b","logo":"l","imdbRating":"8.1",
            "releaseInfo":"2023-","genres":["Animation"],"cast":["A","B"],
            "videos":[{"id":"tt1:1:2","season":1,"episode":2,"title":"Two"},{"id":"tt1:1:1","season":1,"episode":1,"name":"One"},
            {"id":"tt1:0:1","season":0,"episode":1,"title":"Special"}]}"#,
        )
        .unwrap();
        let m = MetaItem::from_json(&j);
        assert_eq!(m.name, "Show");
        assert_eq!(m.background.as_deref(), Some("b"));
        assert_eq!(m.rating, "8.1");
        assert!(m.is_series());
        assert_eq!(m.seasons(), vec![0, 1]);
        assert_eq!(m.episodes(1).iter().map(|v| v.title.as_str()).collect::<Vec<_>>(), vec!["One", "Two"]);
    }

    #[test]
    fn reads_minutes() {
        assert_eq!(parse_minutes("24 min"), Some(24));
        assert_eq!(parse_minutes("1h 45min"), Some(105));
        assert_eq!(parse_minutes("2h"), Some(120));
        assert_eq!(parse_minutes("45"), Some(45));
        assert_eq!(parse_minutes(""), None);
        assert_eq!(parse_minutes("unknown"), None);
    }

    #[test]
    fn counts_time_left_in_a_season() {
        let mut m = MetaItem { runtime: "24 min".into(), ..Default::default() };
        for (s, e) in [(1, 1), (1, 2), (1, 3), (2, 1), (2, 2), (0, 1)] {
            m.videos.push(Video { id: format!("x:{}:{}", s, e), season: Some(s), episode: Some(e), ..Default::default() });
        }
        m.videos[3].runtime_min = Some(30);
        let t = m.season_times(1, &|id| id == "x:1:1").unwrap();
        assert_eq!(t, SeasonTimes { remaining: 48, season: 72, series: 72 + 30 + 24 });
        let t = m.season_times(2, &|_| true).unwrap();
        assert_eq!((t.remaining, t.season), (0, 54));
        assert!(MetaItem::default().season_times(1, &|_| false).is_none());
    }

    #[test]
    fn library_form_keeps_art() {
        let m = MetaItem { id: "tt2".into(), kind: "movie".into(), name: "M".into(), poster: Some("p".into()), ..Default::default() };
        let back = MetaItem::from_json(&m.to_json());
        assert_eq!(back.poster.as_deref(), Some("p"));
        assert_eq!(back.id, "tt2");
    }

    #[test]
    fn fill_from_only_fills_gaps() {
        let mut a = MetaItem { name: "A".into(), poster: Some("keep".into()), ..Default::default() };
        let b = MetaItem { poster: Some("other".into()), background: Some("bg".into()), ..Default::default() };
        a.fill_from(&b);
        assert_eq!(a.poster.as_deref(), Some("keep"));
        assert_eq!(a.background.as_deref(), Some("bg"));
    }
}
