//! Stream objects returned by addons: parsing, ranking, quality grouping and magnet links.
use crate::json::Json;
use crate::util::form_encode;

pub const DEFAULT_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.tiny-vps.com:6969/announce",
    "udp://explodie.org:6969/announce",
    "udp://tracker.moeking.me:6969/announce",
    "udp://opentracker.io:6969/announce",
    "https://tracker.tamersunion.org:443/announce",
];

#[derive(Clone, Debug, Default)]
pub struct Stream {
    pub addon: String,
    pub name: String,
    pub desc: String,
    pub url: Option<String>,
    pub info_hash: Option<String>,
    pub file_idx: Option<usize>,
    pub trackers: Vec<String>,
    pub filename: Option<String>,
    pub headers: Vec<(String, String)>,
    pub yt_id: Option<String>,
    pub res: u32,
    pub seeders: u32,
    pub size_mb: u32,
    /// The size is the whole torrent (a season pack), not the one episode.
    pub pack: bool,
    /// The whole torrent, when `size_mb` was narrowed to the one episode file (0 when unknown).
    pub pack_mb: u32,
    /// `size_mb` is the episode file's own size (the addon's videoSize, or read from the torrent's file list).
    pub size_exact: bool,
    /// Reading the file list for the episode's size: 0 not asked, 1 reading, 2 could not.
    pub size_lookup: u8,
    pub score: u32,
    /// Path of a copy on this computer (a torrent kept with "Keep downloaded").
    pub local: Option<String>,
    /// The addon's `behaviorHints.bingeGroup`: streams of the same release share it from one episode to the next.
    pub binge_group: Option<String>,
}

impl Stream {
    pub fn from_json(j: &Json, addon: &str) -> Stream {
        let name = j.get("name").str();
        let mut desc = j.get("description").str();
        if desc.is_empty() {
            desc = j.get("title").str();
        }
        let text = format!("{}\n{}", name, desc);
        let hints = j.get("behaviorHints");
        let headers = hints
            .path("proxyHeaders.request")
            .entries()
            .iter()
            .map(|(k, v)| (k.clone(), v.str()))
            .collect();
        let trackers = j
            .get("sources")
            .strings()
            .into_iter()
            .filter_map(|s| {
                if let Some(t) = s.strip_prefix("tracker:") {
                    Some(t.to_string())
                } else if s.starts_with("udp://") || s.starts_with("http://") || s.starts_with("https://") {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();
        let nz = |s: String| if s.is_empty() { None } else { Some(s) };
        let file_idx = j.get("fileIdx").as_u64().map(|n| n as usize);
        // Addons that know the file report its size in bytes: that is the size of the episode itself.
        let video_bytes = hints.get("videoSize").as_u64().unwrap_or(0);
        let text_mb = parse_size_mb(&text);
        // A size is the episode file's own only when the addon also names the file (fileIdx): an addon's size text
        // then matches the file. Addons that send no fileIdx give the whole torrent's videoSize (checked on a
        // real network, 2026-11-01: Pokémon S06E03 "10.0 GB" was a 40-episode season). Those are read from the file list.
        let size_mb = if video_bytes > 0 { ((video_bytes / (1024 * 1024)) as u32).max(1) } else { text_mb };
        let size_exact = file_idx.is_some() && size_mb > 0;
        let pack = !size_exact && looks_like_pack(&text);
        let res = parse_res(&text);
        Stream {
            addon: addon.to_string(),
            url: nz(j.get("url").str()),
            info_hash: nz(j.get("infoHash").str().to_lowercase()),
            file_idx,
            trackers,
            filename: nz(hints.get("filename").str()),
            headers,
            yt_id: nz(j.get("ytId").str()),
            res,
            seeders: parse_seeders(&text),
            size_mb,
            pack,
            pack_mb: 0,
            size_exact,
            size_lookup: 0,
            score: parse_score(&text),
            local: None,
            binge_group: nz(hints.get("bingeGroup").str()),
            name,
            desc,
        }
    }

    pub fn is_torrent(&self) -> bool {
        self.url.is_none() && self.local.is_none() && self.info_hash.is_some()
    }

    pub fn is_playable(&self) -> bool {
        self.local.is_some() || self.url.is_some() || self.info_hash.is_some() || self.yt_id.is_some()
    }

    /// The audio languages the stream's name and file name say it carries (for the flags in the list).
    pub fn audio_langs(&self) -> crate::streamlang::AudioLangs {
        crate::streamlang::audio_langs(&format!("{}\n{}\n{}", self.name, self.desc, self.filename.as_deref().unwrap_or("")))
    }

    /// First line of the addon's name, e.g. "4K · Bluray".
    pub fn headline(&self) -> String {
        let first = self.name.lines().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("");
        if first.is_empty() { res_label(self.res) } else { first.to_string() }
    }

    /// Remaining lines (name continuation plus description), each on its own line.
    pub fn detail_lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        for (i, l) in self.name.lines().enumerate() {
            if i > 0 && !l.trim().is_empty() {
                lines.push(l.trim().to_string());
            }
        }
        for l in self.desc.lines() {
            if !l.trim().is_empty() {
                lines.push(l.trim().to_string());
            }
        }
        lines
    }

    /// The size shown in the list: the episode's own. A whole pack's size is never shown as it (HP, 2026-11-01: "porque
    /// ainda ta aparecendo PACK"); it stays empty until the file list gives the episode's size.
    pub fn size_text(&self) -> String {
        if self.size_mb == 0 || (self.pack && !self.size_exact) {
            String::new()
        } else {
            self.size_plain()
        }
    }

    /// "looking" while the file list is read for the episode's size, "failed" when it could not be, else "".
    pub fn size_state(&self) -> &'static str {
        match self.size_lookup {
            1 => "looking",
            2 => "failed",
            _ => "",
        }
    }

    /// Worth reading the torrent's file list for: a torrent whose size may be a whole pack's.
    pub fn needs_file_size(&self) -> bool {
        !self.size_exact && self.info_hash.is_some() && self.local.is_none() && self.url.is_none()
    }

    /// The whole torrent ("12.4 GB") when the size shown is only the episode's file, else "".
    pub fn pack_text(&self) -> String {
        if self.pack_mb == 0 { String::new() } else { mb_text(self.pack_mb) }
    }

    /// The episode file's own size replaces the pack's (read from the torrent's file list).
    pub fn set_file_size(&mut self, file_bytes: u64, total_bytes: u64) {
        if file_bytes == 0 {
            return;
        }
        let total = ((total_bytes / (1024 * 1024)) as u32).max(self.size_mb);
        self.size_mb = ((file_bytes / (1024 * 1024)) as u32).max(1);
        self.pack_mb = if total > self.size_mb { total } else { 0 };
        self.pack = false;
        self.size_exact = true;
        self.size_lookup = 0;
    }

    fn size_plain(&self) -> String {
        if self.size_mb == 0 { String::new() } else { mb_text(self.size_mb) }
    }
}

pub fn res_label(res: u32) -> String {
    match res {
        0 => "Unknown".to_string(),
        2160 => "4K".to_string(),
        n => format!("{}p", n),
    }
}

fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).map(|t| t.to_string()).collect()
}

pub fn parse_res(text: &str) -> u32 {
    for t in tokens(text) {
        match t.as_str() {
            "2160p" | "2160" | "4k" | "uhd" => return 2160,
            "1440p" | "1440" | "2k" => return 1440,
            "1080p" | "1080" | "fhd" | "1080i" => return 1080,
            "720p" | "720" | "hd720" => return 720,
            "576p" => return 576,
            "480p" | "480" => return 480,
            "360p" => return 360,
            _ => {}
        }
    }
    0
}

fn number_after(lower: &str, marker: &str) -> Option<u32> {
    let idx = lower.find(marker)?;
    let rest = &lower[idx + marker.len()..];
    let digits: String = rest
        .chars()
        .skip_while(|c| c.is_whitespace() || *c == ':' || *c == '=')
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

pub fn parse_seeders(text: &str) -> u32 {
    let lower = text.to_lowercase();
    for m in ["\u{1F464}", "seeders", "seeder", "seeds", "seed", "peers"] {
        if let Some(n) = number_after(&lower, m) {
            return n;
        }
    }
    0
}

fn mb_text(mb: u32) -> String {
    if mb >= 1024 { format!("{:.1} GB", mb as f64 / 1024.0) } else { format!("{} MB", mb) }
}

/// The most one episode weighs at a height (a 4K remux episode is about 20 GB, a 1080p one about 10 GB).
pub fn episode_max_mb(res: u32) -> u32 {
    if res >= 2160 { 32 * 1024 } else { 16 * 1024 }
}

/// For an episode: a stream bigger than any single episode at its quality is a whole season or series, whatever the
/// addon says (HP: a 77 GB "1080p Bluray" was ranked first for one episode). It is labelled and ranked as a pack.
/// Not for films, where a remux is that big.
pub fn mark_big_packs(streams: &mut [Stream]) {
    for s in streams.iter_mut() {
        // a torrent whose size is not known to be the episode file's may be a whole season: its size is not shown
        // (it is read from the file list) and it ranks below the ones that name the episode's file
        if s.info_hash.is_some() && !s.size_exact && s.local.is_none() {
            s.pack = true;
        }
        if s.size_mb > episode_max_mb(s.res) {
            // too big for one episode even when the addon calls it the file's size: read the file list to know
            s.pack = true;
            s.size_exact = false;
        }
    }
}

/// Does the release name say it holds a whole season or series (batch, complete, "01-12", "S01-S03")?
pub fn looks_like_pack(text: &str) -> bool {
    let lower = text.to_lowercase();
    let toks = tokens(&lower);
    if toks.iter().any(|t| matches!(t.as_str(), "batch" | "complete" | "collection" | "pack" | "seasons" | "completa" | "completo")) {
        return true;
    }
    // Ranges: "01-12", "s01-s03", "1-24".
    let b = lower.as_bytes();
    for i in 1..b.len().saturating_sub(1) {
        if b[i] == b'-' || b[i] == b'~' {
            let left = lower[..i].trim_end_matches(' ');
            let right = lower[i + 1..].trim_start_matches(' ');
            let l: String = left.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
            let r: String = right.trim_start_matches(|c| c == 's' || c == 'e').chars().take_while(|c| c.is_ascii_digit()).collect();
            let tail_ok = right.trim_start_matches(|c| c == 's' || c == 'e').chars().nth(r.len()).map(|c| !c.is_ascii_digit()).unwrap_or(true);
            if let (Ok(a), Ok(z)) = (l.parse::<u32>(), r.parse::<u32>()) {
                let year_like = a > 1900 || z > 1900;
                let res_like = matches!(z, 480 | 720 | 1080 | 2160) || matches!(a, 480 | 720 | 1080 | 2160);
                if tail_ok && z > a && z - a >= 2 && !year_like && !res_like && l.len() <= 3 && r.len() <= 3 {
                    return true;
                }
            }
        }
    }
    false
}

pub fn parse_score(text: &str) -> u32 {
    if let Some(idx) = text.find("/100") {
        let digits: String = text[..idx].chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
        return digits.parse().unwrap_or(0);
    }
    0
}

pub fn parse_size_mb(text: &str) -> u32 {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.').to_string())
        .collect();
    let unit_mb = |u: &str| -> Option<f64> {
        match u {
            "tb" | "tib" => Some(1024.0 * 1024.0),
            "gb" | "gib" => Some(1024.0),
            "mb" | "mib" => Some(1.0),
            _ => None,
        }
    };
    for (i, w) in words.iter().enumerate() {
        let lw = w.to_lowercase();
        let split = lw.find(|c: char| c.is_alphabetic()).unwrap_or(lw.len());
        let (num, unit) = lw.split_at(split);
        if !num.is_empty() {
            if let (Ok(n), Some(f)) = (num.parse::<f64>(), unit_mb(unit)) {
                return (n * f) as u32;
            }
            if unit.is_empty() {
                if let (Ok(n), Some(next)) = (num.parse::<f64>(), words.get(i + 1)) {
                    if let Some(f) = unit_mb(&next.to_lowercase()) {
                        return (n * f) as u32;
                    }
                }
            }
        }
    }
    0
}

/// Best first. Streams at or under `preferred` height come first (highest of those),
/// then larger ones, then unknown. Ties go to score, seeders and direct links.
pub fn rank(streams: &mut [Stream], preferred: u32) {
    // within a quality, a stream that can play (peers or a link) with the most languages (flags) comes first
    // (HP, 2026-11-02: "dar sempre preferência para o torrent que tiver o maior número de bandeiras")
    streams.sort_by_cached_key(|s| {
        let band: i64 = if s.res == 0 { 0 } else if s.res <= preferred { 2 } else { 1 };
        let res_key: i64 = if s.res == 0 || s.res <= preferred { s.res as i64 } else { -(s.res as i64) };
        let alive = (s.seeders > 0 || s.url.is_some() || s.local.is_some()) as i64;
        std::cmp::Reverse((band, res_key, !s.pack as i64, alive, lang_count(s) as i64, s.score as i64, s.seeders as i64, s.url.is_some() as i64))
    });
}

/// How many languages (flags) a stream's name says it carries; "Dual"/"Multi" alone counts as two.
pub fn lang_count(s: &Stream) -> usize {
    let a = crate::streamlang::audio_langs(&format!("{} {} {}", s.name, s.desc, s.filename.as_deref().unwrap_or("")));
    if a.multi {
        a.langs.len().max(2)
    } else {
        a.langs.len()
    }
}

/// The key that names one stream: its info hash, else its link.
pub fn key_of(s: &Stream) -> String {
    s.info_hash.clone().or_else(|| s.url.clone()).or_else(|| s.local.clone()).unwrap_or_default()
}

/// A user rule for choosing a stream by itself, e.g. `1080p.*(web|bluray)` (case does not matter). None when empty
/// or not a valid pattern, so a typo never stops playback.
pub fn autoplay_rule(pattern: &str) -> Option<regex::Regex> {
    let p = pattern.trim();
    if p.is_empty() {
        return None;
    }
    regex::RegexBuilder::new(p).case_insensitive(true).size_limit(1 << 20).build().ok()
}

fn rule_text(s: &Stream) -> String {
    format!("{}\n{}\n{}", s.name, s.desc, s.filename.clone().unwrap_or_default())
}

/// Which stream of a ranked list plays by itself (idea from NuvioMobile, GPL-3.0, StreamAutoPlaySelector.kt; written
/// anew here). In order:
/// 1. `same` (the stream that last played this very video, e.g. when resuming) when it is still listed;
/// 2. only streams that match the user's rule, when there is a rule and something matches it;
/// 3. among those, the release of `reference` (the stream of the previous episode): the same addon `bingeGroup`,
///    else the closest release family (group, quality, source, codec, language);
/// 4. else the best ranked one.
pub fn autoplay_pick(list: &[Stream], reference: Option<&Stream>, same: Option<&str>, rule: Option<&regex::Regex>) -> usize {
    if list.is_empty() {
        return 0;
    }
    if let Some(key) = same.filter(|k| !k.is_empty()) {
        if let Some(i) = list.iter().position(|s| s.is_playable() && key_of(s) == key) {
            return i;
        }
    }
    let mut cands: Vec<usize> = (0..list.len()).filter(|&i| list[i].is_playable()).collect();
    if let Some(re) = rule {
        let matching: Vec<usize> = cands.iter().copied().filter(|&i| re.is_match(&rule_text(&list[i]))).collect();
        if !matching.is_empty() {
            cands = matching;
        }
    }
    if let Some(r) = reference {
        if let Some(g) = r.binge_group.as_deref().filter(|g| !g.is_empty()) {
            if let Some(&i) = cands.iter().find(|&&i| list[i].binge_group.as_deref() == Some(g)) {
                return i;
            }
        }
        let best = cands
            .iter()
            .map(|&i| (i, crate::downloads::family_score(r, &list[i])))
            .filter(|(_, sc)| *sc >= 40)
            .max_by_key(|&(i, sc)| (sc, std::cmp::Reverse(i)));
        if let Some((i, _)) = best {
            return i;
        }
    }
    cands.first().copied().unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq)]
pub struct QualityOption {
    pub res: u32,
    pub label: String,
    pub stream_idx: usize,
    pub count: usize,
}

/// One entry per resolution, pointing at the best-ranked stream of that resolution.
/// `streams` must already be ranked, so the first stream seen for a resolution is its best.
pub fn quality_options(streams: &[Stream]) -> Vec<QualityOption> {
    let mut out: Vec<QualityOption> = Vec::new();
    for (i, s) in streams.iter().enumerate() {
        if !s.is_playable() {
            continue;
        }
        match out.iter_mut().find(|q| q.res == s.res) {
            Some(q) => q.count += 1,
            None => out.push(QualityOption { res: s.res, label: res_label(s.res), stream_idx: i, count: 1 }),
        }
    }
    out.sort_by_key(|q| std::cmp::Reverse(if q.res == 0 { 1 } else { q.res }));
    out
}

pub fn magnet(s: &Stream) -> Option<String> {
    let hash = s.info_hash.as_ref()?;
    let mut m = format!("magnet:?xt=urn:btih:{}", hash);
    if let Some(dn) = &s.filename {
        m.push_str(&format!("&dn={}", form_encode(dn)));
    }
    let mut seen: Vec<&str> = Vec::new();
    for t in s.trackers.iter().map(|t| t.as_str()).chain(DEFAULT_TRACKERS.iter().copied()) {
        if !seen.contains(&t) {
            seen.push(t);
            m.push_str(&format!("&tr={}", form_encode(t)));
        }
    }
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(name: &str, desc: &str, hash: Option<&str>) -> Stream {
        let mut j = Json::obj(vec![("name", name.into()), ("description", desc.into())]);
        if let Some(h) = hash {
            j.set("infoHash", h.into());
        }
        Stream::from_json(&j, "T")
    }

    #[test]
    fn parses_addon_style_text() {
        let st = s("4K · Bluray", "89/100 · \u{1F464} 17\nx265 · DV\natmos · 5.1 · MULTI\n8.0 GB", Some("ABC"));
        assert_eq!((st.res, st.seeders, st.score, st.size_mb), (2160, 17, 89, 8192));
        assert_eq!(st.info_hash.as_deref(), Some("abc"));
        assert_eq!(st.headline(), "4K · Bluray");
        assert_eq!(st.size_text(), "8.0 GB");
        assert!(st.detail_lines().iter().any(|l| l.starts_with("x265")));
    }

    #[test]
    fn parses_common_title_formats() {
        assert_eq!(parse_res("Movie.2023.1080p.WEB-DL.x264"), 1080);
        assert_eq!(parse_res("Show S01E01 720p"), 720);
        assert_eq!(parse_res("something x264k"), 0);
        assert_eq!(parse_seeders("Seeds: 42"), 42);
        assert_eq!(parse_seeders("seeders 7 leechers 3"), 7);
        assert_eq!(parse_size_mb("size 700 MB"), 700);
        assert_eq!(parse_size_mb("\u{1F4BE} 1.5GB"), 1536);
        assert_eq!(parse_size_mb("nothing"), 0);
    }

    #[test]
    fn autoplay_keeps_the_binge_group_then_the_rule() {
        let mut a = s("1080p", "[SubsPlease] Show - 05 (1080p)", Some("a"));
        let mut b = s("720p", "[Erai] Show - 05 (720p) WEB", Some("b"));
        let mut c = s("1080p", "[Erai] Show - 05 (1080p) WEB", Some("c"));
        a.binge_group = Some("subsplease-1080".into());
        b.binge_group = Some("erai-720".into());
        c.binge_group = Some("erai-1080".into());
        let list = vec![a.clone(), b.clone(), c.clone()];
        // no reference, no rule: the best ranked
        assert_eq!(autoplay_pick(&list, None, None, None), 0);
        // the previous episode came from erai-720: the same group again
        let mut prev = s("720p", "[Erai] Show - 04 (720p) WEB", Some("z"));
        prev.binge_group = Some("erai-720".into());
        assert_eq!(autoplay_pick(&list, Some(&prev), None, None), 1);
        // a rule narrows the choice first
        let rule = autoplay_rule("1080p.*web");
        assert_eq!(autoplay_pick(&list, None, None, rule.as_ref()), 2);
        // a rule nothing matches is ignored
        let none = autoplay_rule("2160p");
        assert_eq!(autoplay_pick(&list, None, None, none.as_ref()), 0);
        // a broken pattern is no rule at all
        assert!(autoplay_rule("(").is_none());
        // the stream that played this video before wins over everything
        assert_eq!(autoplay_pick(&list, Some(&prev), Some("c"), rule.as_ref()), 2);
        assert_eq!(autoplay_pick(&list, None, Some("gone"), None), 0);
    }

    #[test]
    fn autoplay_follows_the_release_family_without_binge_group() {
        let list = vec![
            s("1080p", "[SubsPlease] Show - 05 (1080p)", Some("a")),
            s("720p", "[Erai-raws] Show - 05 [720p][Multiple Subtitle]", Some("b")),
        ];
        let prev = s("720p", "[Erai-raws] Show - 04 [720p][Multiple Subtitle]", Some("z"));
        assert_eq!(autoplay_pick(&list, Some(&prev), None, None), 1);
        let j = Json::parse(r#"{"name":"1080p","infoHash":"aa","behaviorHints":{"bingeGroup":"x|1080"}}"#).unwrap();
        assert_eq!(Stream::from_json(&j, "T").binge_group.as_deref(), Some("x|1080"));
    }

    #[test]
    fn ranks_preferred_height_first() {
        let mut v = vec![
            s("4K", "score 80/100 \u{1F464} 5", Some("a")),
            s("720p", "", Some("b")),
            s("1080p", "70/100 \u{1F464} 2", Some("c")),
            s("1080p", "90/100 \u{1F464} 1", Some("d")),
            s("mystery", "", Some("e")),
        ];
        rank(&mut v, 1080);
        let order: Vec<_> = v.iter().map(|x| x.info_hash.clone().unwrap()).collect();
        assert_eq!(order, vec!["d", "c", "b", "a", "e"]);
    }

    #[test]
    fn most_languages_first_within_a_quality() {
        let mut v = vec![
            s("1080p", "Show S02E05 \u{1F464} 240 \u{1F1F7}\u{1F1FA} \u{1F1FA}\u{1F1F8}", Some("two")),
            s("1080p", "Show S02E05 \u{1F464} 300", Some("none")),
            s("1080p", "Show S02E05 \u{1F464} 114 \u{1F1EC}\u{1F1E7} \u{1F1F7}\u{1F1FA} \u{1F1EE}\u{1F1F9} \u{1F1F5}\u{1F1F9}", Some("four")),
            s("1080p", "Show S02E05 DUAL", Some("dead")),
        ];
        for x in v.iter_mut() {
            x.seeders = if x.info_hash.as_deref() == Some("dead") { 0 } else { 5 };
        }
        rank(&mut v, 1080);
        let order: Vec<_> = v.iter().map(|x| x.info_hash.clone().unwrap()).collect();
        assert_eq!(order, vec!["four", "two", "none", "dead"]);
    }

    #[test]
    fn quality_options_group_by_resolution() {
        let mut v = vec![s("1080p", "90/100", Some("a")), s("4K", "", Some("b")), s("1080p", "80/100", Some("c")), s("720p", "", Some("d"))];
        rank(&mut v, 2160);
        let q = quality_options(&v);
        assert_eq!(q.iter().map(|o| (o.label.as_str(), o.count)).collect::<Vec<_>>(), vec![("4K", 1), ("1080p", 2), ("720p", 1)]);
        assert_eq!(v[q[1].stream_idx].info_hash.as_deref(), Some("a"));
    }

    #[test]
    fn magnet_has_filename_and_unique_trackers() {
        let mut st = s("x", "", Some("abc"));
        st.filename = Some("a b.mkv".into());
        st.trackers = vec![DEFAULT_TRACKERS[0].to_string(), "udp://own:1".to_string()];
        let m = magnet(&st).unwrap();
        assert!(m.starts_with("magnet:?xt=urn:btih:abc&dn=a%20b.mkv&tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce"));
        assert_eq!(m.matches("opentrackr").count(), 1);
        assert!(m.contains("udp%3A%2F%2Fown%3A1"));
        assert!(magnet(&s("x", "", None)).is_none());
    }

    #[test]
    fn direct_links_and_headers() {
        let j = Json::parse(r#"{"url":"http://v/x.mp4","name":"1080p","behaviorHints":{"proxyHeaders":{"request":{"Referer":"http://r"}}}}"#).unwrap();
        let st = Stream::from_json(&j, "A");
        assert!(!st.is_torrent() && st.is_playable());
        assert_eq!(st.headers, vec![("Referer".to_string(), "http://r".to_string())]);
    }

    #[test]
    fn packs_and_sizes() {
        assert!(looks_like_pack("[SubsPlease] Show (01-12) (1080p) [Batch]"));
        assert!(looks_like_pack("Show S01-S03 Complete"));
        assert!(looks_like_pack("Show 01 ~ 24 1080p"));
        assert!(!looks_like_pack("[SubsPlease] Show - 05 (1080p) [ABC123]"));
        assert!(!looks_like_pack("Show - S01E05 - 1080p x264 2020-2021"));
        assert!(!looks_like_pack("Show 720p-1080p"));
        // An addon that names the file: the file is named (fileIdx), its size text is the file's
        let j = crate::json::Json::parse(r#"{"title":"Show S01E05 1080p\n415 MB","infoHash":"aa","fileIdx":339}"#).unwrap();
        let st = Stream::from_json(&j, "x");
        assert!(st.size_exact && !st.pack && !st.needs_file_size());
        assert_eq!(st.size_text(), "415 MB");
        // An addon with no fileIdx: no fileIdx, its videoSize is the whole season's; in an episode list it is never shown as such
        let j = crate::json::Json::parse(r#"{"name":"1080p","infoHash":"aa","behaviorHints":{"videoSize":10737418240}}"#).unwrap();
        let mut list = vec![Stream::from_json(&j, "x")];
        mark_big_packs(&mut list);
        let st = list[0].clone();
        assert!(st.pack && !st.size_exact);
        // a pack's size is never shown as the episode's: nothing until the file list gives it
        assert_eq!(st.size_text(), "");
        assert!(st.needs_file_size());
        let mut known = st.clone();
        known.set_file_size(1_200 * 1024 * 1024, 14 * 1024 * 1024 * 1024);
        assert_eq!(known.size_text(), "1.2 GB");
        assert_eq!(known.pack_text(), "14.0 GB");
        assert!(!known.pack && !known.needs_file_size());
        let j = crate::json::Json::parse(r#"{"title":"Show S01E05 1080p 14.2 GB","infoHash":"aa","fileIdx":4,"behaviorHints":{"videoSize":1468006400}}"#).unwrap();
        let st = Stream::from_json(&j, "x");
        assert!(!st.pack);
        let mut big = vec![st.clone()];
        big[0].size_mb = 77 * 1024;
        mark_big_packs(&mut big);
        assert!(big[0].pack && big[0].size_text().is_empty() && big[0].needs_file_size());
        assert_eq!(st.size_text(), "1.4 GB");
    }
}
