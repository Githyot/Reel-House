//! Anime Skip (anime-skip.com): community timestamps for openings, endings, recaps and previews.
//! It talks to their public GraphQL API. A client ID from the Anime Skip account page is required.
use crate::http::{Http, Request};
use crate::json::Json;
use crate::util::md5_hex;

pub const ENDPOINT: &str = "https://api.anime-skip.com/graphql";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Intro,
    Credits,
    Recap,
    Preview,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub start: f64,
    /// Where the next timestamp begins. None for the last one.
    pub end: Option<f64>,
    pub kind: Kind,
    pub name: String,
}

impl Segment {
    pub fn skippable(&self) -> bool {
        self.kind != Kind::Other
    }
    pub fn label(&self) -> &'static str {
        match self.kind {
            Kind::Intro => "Skip Opening",
            Kind::Credits => "Skip Ending",
            Kind::Recap => "Skip Recap",
            Kind::Preview => "Skip Preview",
            Kind::Other => "Skip",
        }
    }
}

pub fn kind_of(type_name: &str) -> Kind {
    let n = type_name.to_lowercase();
    let words: Vec<&str> = n.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |w: &str| words.contains(&w);
    if n.contains("intro") || n.contains("opening") || n.contains("branding") || has("op") || has("ncop") {
        Kind::Intro
    } else if n.contains("credit") || n.contains("ending") || n.contains("outro") || has("ed") || has("nced") {
        Kind::Credits
    } else if n.contains("recap") || n.contains("previously") {
        Kind::Recap
    } else if n.contains("preview") || n.contains("next episode") {
        Kind::Preview
    } else {
        Kind::Other
    }
}

/// Turns raw timestamps (any order) into segments that last until the next timestamp.
pub fn segments_from(timestamps: &[(f64, String)]) -> Vec<Segment> {
    let mut v: Vec<(f64, String)> = timestamps.to_vec();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    (0..v.len())
        .map(|i| Segment { start: v[i].0, end: v.get(i + 1).map(|n| n.0), kind: kind_of(&v[i].1), name: v[i].1.clone() })
        .collect()
}

/// The skippable segment that contains `pos`, if any.
pub fn segment_at(segs: &[Segment], pos: f64, duration: f64) -> Option<&Segment> {
    segs.iter().find(|s| s.skippable() && pos >= s.start - 0.25 && pos < s.end.unwrap_or(if duration > 0.0 { duration } else { f64::MAX }) - 1.0)
}

/// Where to jump to leave the segment.
pub fn skip_target(seg: &Segment, duration: f64) -> f64 {
    match seg.end {
        Some(e) => e,
        None if duration > 0.0 => (duration - 0.5).max(seg.start),
        None => seg.start + 90.0,
    }
}

pub struct Client<'a> {
    pub http: &'a dyn Http,
    pub client_id: String,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Login {
    pub token: String,
    pub refresh: String,
}

impl<'a> Client<'a> {
    fn query(&self, q: &str, vars: Json) -> Result<Json, String> {
        if self.client_id.trim().is_empty() {
            return Err("Anime Skip needs a client ID. Create one at anime-skip.com/account and paste it in Settings.".to_string());
        }
        let body = Json::obj(vec![("query", q.into()), ("variables", vars)]);
        let mut req = Request::post(ENDPOINT).header("X-Client-ID", self.client_id.trim()).json_body(body.dump()).timeout(25);
        if !self.token.is_empty() {
            req = req.header("Authorization", &format!("Bearer {}", self.token));
        }
        let r = self.http.send(&req)?;
        if r.status == 429 {
            return Err("Anime Skip is rate limiting this client ID. Use your own client ID in Settings.".to_string());
        }
        let j = r.json().map_err(|_| format!("Anime Skip answered {} with something unreadable.", r.status))?;
        if let Some(first) = j.get("errors").items().first() {
            return Err(first.get("message").str());
        }
        if !r.ok() {
            return Err(format!("Anime Skip answered {}.", r.status));
        }
        Ok(j.get("data").clone())
    }

    pub fn login(&self, email: &str, password: &str) -> Result<Login, String> {
        let q = "query Login($u: String!, $p: String!) { login(usernameEmail: $u, passwordHash: $p) { authToken refreshToken } }";
        let d = self
            .query(q, Json::obj(vec![("u", email.trim().into()), ("p", md5_hex(password.as_bytes()).into())]))
            .map_err(|e| if e.to_lowercase().contains("password") || e.to_lowercase().contains("credential") { "Wrong email or password.".to_string() } else { e })?;
        let token = d.path("login.authToken").str();
        if token.is_empty() {
            return Err("Anime Skip did not return a login token.".to_string());
        }
        Ok(Login { token, refresh: d.path("login.refreshToken").str() })
    }

    pub fn refresh(&self, refresh_token: &str) -> Result<Login, String> {
        let q = "query Refresh($t: String!) { loginRefresh(refreshToken: $t) { authToken refreshToken } }";
        let d = self.query(q, Json::obj(vec![("t", refresh_token.into())]))?;
        let token = d.path("loginRefresh.authToken").str();
        if token.is_empty() {
            return Err("The saved Anime Skip login expired. Log in again in Settings.".to_string());
        }
        Ok(Login { token, refresh: d.path("loginRefresh.refreshToken").str() })
    }

    fn find_show(&self, name: &str) -> Result<Option<String>, String> {
        let q = "query S($s: String!) { searchShows(search: $s, limit: 6) { id name originalName } }";
        let d = self.query(q, Json::obj(vec![("s", name.into())]))?;
        let shows = d.get("searchShows").items();
        let want = name.trim().to_lowercase();
        let exact = shows.iter().find(|s| s.get("name").str().to_lowercase() == want || s.get("originalName").str().to_lowercase() == want);
        Ok(exact.or_else(|| shows.first()).map(|s| s.get("id").str()).filter(|i| !i.is_empty()))
    }

    /// Segments for one episode, found by show name, season and episode number.
    pub fn segments(&self, show: &str, season: Option<i64>, episode: i64, video_secs: f64) -> Result<Vec<Segment>, String> {
        let Some(show_id) = self.find_show(show)? else { return Ok(vec![]) };
        let q = "query E($id: ID!) { findEpisodesByShowId(showId: $id) { id season number absoluteNumber baseDuration timestamps { at type { name } } } }";
        let d = self.query(q, Json::obj(vec![("id", show_id.into())]))?;
        let eps = d.get("findEpisodesByShowId").items();
        let num = |e: &Json, k: &str| e.get(k).str().parse::<i64>().ok().or_else(|| e.get(k).as_f64().map(|f| f as i64));
        let by_season = eps.iter().find(|e| num(e, "number") == Some(episode) && (season.is_none() || num(e, "season").is_none() || num(e, "season") == season));
        let ep = by_season.or_else(|| eps.iter().find(|e| num(e, "absoluteNumber") == Some(episode)));
        let Some(ep) = ep else { return Ok(vec![]) };
        // Times belong to one release. If its length is far from ours the times would land in the wrong place.
        if let Some(base) = ep.get("baseDuration").as_f64().filter(|b| *b > 60.0) {
            if video_secs > 60.0 && (base - video_secs).abs() > 6.0 {
                return Ok(vec![]);
            }
        }
        let stamps: Vec<(f64, String)> = ep.get("timestamps").items().iter().map(|t| (t.get("at").as_f64().unwrap_or(0.0), t.path("type.name").str())).collect();
        Ok(segments_from(&stamps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    fn client(f: &Fake) -> Client<'_> {
        Client { http: f, client_id: "cid".into(), token: String::new() }
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of("Intro"), Kind::Intro);
        assert_eq!(kind_of("Mixed Intro"), Kind::Intro);
        assert_eq!(kind_of("New Credits"), Kind::Credits);
        assert_eq!(kind_of("Recap"), Kind::Recap);
        assert_eq!(kind_of("Canon"), Kind::Other);
        assert_eq!(kind_of("Title Card"), Kind::Other);
    }

    #[test]
    fn segments_and_skipping() {
        let segs = segments_from(&[(90.0, "Canon".into()), (0.0, "Recap".into()), (30.0, "Intro".into()), (1300.0, "Credits".into())]);
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[0].kind, Kind::Recap);
        assert_eq!(segs[0].end, Some(30.0));
        let s = segment_at(&segs, 45.0, 1420.0).unwrap();
        assert_eq!(s.label(), "Skip Opening");
        assert_eq!(skip_target(s, 1420.0), 90.0);
        assert!(segment_at(&segs, 200.0, 1420.0).is_none());
        let c = segment_at(&segs, 1310.0, 1420.0).unwrap();
        assert_eq!(c.label(), "Skip Ending");
        assert_eq!(skip_target(c, 1420.0), 1419.5);
    }

    #[test]
    fn login_sends_md5_and_client_id() {
        let f = Fake::new().route("POST https://api.anime-skip.com/graphql", 200, r#"{"data":{"login":{"authToken":"T","refreshToken":"R"}}}"#);
        let l = client(&f).login(" me@x.com ", "abc").unwrap();
        assert_eq!(l, Login { token: "T".into(), refresh: "R".into() });
        let req = &f.requests()[0];
        assert!(req.headers.iter().any(|(k, v)| k == "X-Client-ID" && v == "cid"));
        let body = String::from_utf8(req.body.clone().unwrap().1).unwrap();
        assert!(body.contains("900150983cd24fb0d6963f7d28e17f72") && body.contains("me@x.com"));
    }

    #[test]
    fn login_error_is_friendly() {
        let f = Fake::new().route("graphql", 200, r#"{"errors":[{"message":"Invalid password"}]}"#);
        assert_eq!(client(&f).login("a", "b").unwrap_err(), "Wrong email or password.");
    }

    #[test]
    fn needs_client_id() {
        let f = Fake::new();
        let c = Client { http: &f, client_id: String::new(), token: String::new() };
        assert!(c.segments("x", None, 1, 0.0).unwrap_err().contains("client ID"));
    }

    #[test]
    fn finds_episode_timestamps() {
        let f = Fake::new()
            .route("graphql", 200, r#"{"data":{"searchShows":[{"id":"s0","name":"Other","originalName":""},{"id":"s1","name":"Frieren","originalName":"Sousou no Frieren"}]}}"#)
            .route(
                "graphql",
                200,
                r#"{"data":{"findEpisodesByShowId":[
                {"id":"e1","season":"1","number":"1","absoluteNumber":"1","timestamps":[{"at":0,"type":{"name":"Canon"}}]},
                {"id":"e2","season":"1","number":"2","absoluteNumber":"2","timestamps":[{"at":12.5,"type":{"name":"Intro"}},{"at":101.5,"type":{"name":"Canon"}}]}]}}"#,
            );
        let segs = client(&f).segments("Frieren", Some(1), 2, 0.0).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0], Segment { start: 12.5, end: Some(101.5), kind: Kind::Intro, name: "Intro".into() });
    }

    #[test]
    fn unknown_show_gives_no_segments() {
        let f = Fake::new().route("graphql", 200, r#"{"data":{"searchShows":[]}}"#);
        assert!(client(&f).segments("Nope", None, 1, 0.0).unwrap().is_empty());
    }
}
