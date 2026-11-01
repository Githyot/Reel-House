//! Karaoke for the openings and endings of anime, like the old fansubs had. The songs come from the Karaoke
//! Mugen database (kara.moe, ODbL data): each one has an ASS file whose times start at 0 at the beginning of the
//! clip. We find the songs of a series through its MyAnimeList id (or its name), pick the one whose length
//! matches the opening/ending of the episode, and move its times to where that part starts.
use crate::animeskip::Kind;
use crate::http::{get_json, Http, Request};
use crate::json::Json;
use crate::util::url_encode;

/// The public API of kara.moe (the Karaoke Mugen server). Checked against the live site: `GET karas/search?filter=text&size=n`
/// answers `{infos, i18n, avatars, content:[…]}`.
pub const API: &str = "https://kara.moe/api/";
/// Where the lyrics files (ASS) are downloaded from.
const LYRICS: &[&str] = &["https://kara.moe/downloads/lyrics/"];

#[derive(Clone, Debug, PartialEq)]
pub struct Kara {
    pub kid: String,
    pub title: String,
    pub kind: Kind,
    pub order: u32,
    /// Seconds.
    pub duration: f64,
    /// File name of the lyrics.
    pub lyrics: String,
    /// Names of the series the song belongs to.
    pub series: Vec<String>,
    /// MyAnimeList ids of those series.
    pub mal: Vec<u64>,
}

/// A song with its lyrics, ready to be placed in an episode.
#[derive(Clone, Debug)]
pub struct Song {
    pub kind: Kind,
    pub label: String,
    pub duration: f64,
    pub order: u32,
    pub ass: String,
}

fn kind_of(j: &Json) -> Option<Kind> {
    for t in j.get("songtypes").items() {
        let n = t.get("name").str().to_uppercase();
        let n = if n.is_empty() { t.str().to_uppercase() } else { n };
        match n.as_str() {
            "OP" | "OPENING" => return Some(Kind::Intro),
            "ED" | "ENDING" => return Some(Kind::Credits),
            _ => {}
        }
    }
    None
}

fn id_of(v: &Json) -> Option<u64> {
    v.as_f64().or_else(|| v.as_str().and_then(|x| x.trim().parse::<f64>().ok())).map(|f| f as u64)
}

fn title_of(k: &Json) -> String {
    let titles = k.get("titles");
    for key in [k.get("titles_default_language").str(), "eng".to_string(), "qro".to_string(), "jpn".to_string()] {
        let t = titles.get(&key).str();
        if !t.is_empty() {
            return t;
        }
    }
    titles.entries().first().map(|(_, v)| v.str()).unwrap_or_default()
}

/// Reads the answer of a search. Songs that are not an opening or an ending, or that have no lyrics, are left out.
pub fn parse_search(j: &Json) -> Vec<Kara> {
    let mut out = Vec::new();
    for k in j.get("content").items() {
        let Some(kind) = kind_of(k) else { continue };
        let kid = k.get("kid").str();
        let infos = k.get("lyrics_infos").items();
        let lyrics = infos.iter().find(|i| i.get("default").as_bool().unwrap_or(false)).or_else(|| infos.first()).map(|i| i.get("filename").str()).filter(|f| !f.is_empty()).unwrap_or_else(|| k.get("subfile").str());
        if kid.is_empty() || lyrics.is_empty() {
            continue;
        }
        let mut series = Vec::new();
        let mut mal: Vec<u64> = k.get("myanimelist_ids").items().iter().filter_map(id_of).collect();
        for s in k.get("series").items() {
            let name = s.get("name").str();
            if !name.is_empty() {
                series.push(name);
            }
            if let Some(id) = id_of(&s.path("external_database_ids.myanimelist")) {
                if !mal.contains(&id) {
                    mal.push(id);
                }
            }
        }
        out.push(Kara { kid, title: title_of(k), kind, order: k.get("songorder").as_f64().unwrap_or(0.0) as u32, duration: k.get("duration").as_f64().unwrap_or(0.0), lyrics, series, mal });
    }
    out
}

/// The text search of the site (it also looks at the other names of a series, so "Witch Hat Atelier" finds
/// "Tongari Bôshi no Atelier").
pub fn search(http: &dyn Http, text: &str) -> Result<Vec<Kara>, String> {
    let j = get_json(http, &format!("{}karas/search?filter={}&size=120", API, url_encode(text)), 20)?;
    Ok(parse_search(&j))
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The openings and endings of a series. `mal` are the MyAnimeList ids that are good for this episode (the first one
/// that has songs wins); `names` are the names to search for, best first. Without an id, a song counts when the name
/// of its series is the name searched for.
pub fn find(http: &dyn Http, mal: &[u64], names: &[String]) -> Result<Vec<Kara>, String> {
    let mut seen: Vec<String> = Vec::new();
    let mut last_err = None;
    let mut all: Vec<Kara> = Vec::new();
    for n in names.iter().filter(|n| !n.trim().is_empty()).take(4) {
        let key = norm(n);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key.clone());
        match search(http, n) {
            Ok(v) => {
                for k in v {
                    if !all.iter().any(|a| a.kid == k.kid) {
                        all.push(k);
                    }
                }
                let hit = if mal.is_empty() { all.iter().any(|k| k.series.iter().any(|s| norm(s) == key)) } else { mal.iter().any(|id| all.iter().any(|k| k.mal.contains(id))) };
                if hit {
                    break;
                }
            }
            Err(e) => last_err = Some(e),
        }
    }
    if all.is_empty() {
        return last_err.map_or(Ok(vec![]), Err);
    }
    for id in mal {
        let v: Vec<Kara> = all.iter().filter(|k| k.mal.contains(id)).cloned().collect();
        if !v.is_empty() {
            return Ok(v);
        }
    }
    if mal.is_empty() {
        let keys: Vec<String> = seen;
        return Ok(all.into_iter().filter(|k| k.series.iter().any(|s| keys.contains(&norm(s)))).collect());
    }
    Ok(vec![])
}

/// Downloads the lyrics (an ASS file) of one song.
pub fn fetch_ass(http: &dyn Http, k: &Kara) -> Result<String, String> {
    let mut last = "no lyrics file".to_string();
    for b in LYRICS {
        let u = format!("{}{}", b, url_encode(&k.lyrics));
        match http.send(&Request::get(&u).timeout(20)) {
            Ok(r) if r.ok() => {
                let t = String::from_utf8_lossy(&r.body).trim_start_matches('\u{feff}').to_string();
                if t.contains("[Events]") {
                    return Ok(t);
                }
                last = format!("{} did not return an ASS file", u.split('/').nth(2).unwrap_or(&u));
            }
            Ok(r) => last = format!("{} answered {}", u.split('/').nth(2).unwrap_or(&u), r.status),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Looks the songs up and downloads their lyrics (at most 10 of them).
pub fn songs_for(http: &dyn Http, mal: &[u64], names: &[String]) -> Result<Vec<Song>, String> {
    let mut karas = find(http, mal, names)?;
    karas.sort_by_key(|k| (k.kind as u8, k.order));
    let mut out = Vec::new();
    let mut last_err = String::new();
    for k in karas.iter().take(10) {
        match fetch_ass(http, k) {
            Ok(ass) => {
                let tag = if k.kind == Kind::Intro { "OP" } else { "ED" };
                let n = if k.order > 0 { format!(" {}", k.order) } else { String::new() };
                out.push(Song { kind: k.kind, label: format!("Karaoke {}{} - {}", tag, n, k.title), duration: k.duration, order: k.order, ass });
            }
            Err(e) => last_err = e,
        }
    }
    if out.is_empty() && !last_err.is_empty() {
        return Err(last_err);
    }
    Ok(out)
}

/// The song that fits a part of the episode that lasts `len` seconds (0 when unknown): the closest length wins
/// if it is within 20 s, and the first song wins when the length is unknown.
pub fn pick<'a>(songs: &'a [Song], kind: Kind, len: f64) -> Option<&'a Song> {
    let mut list: Vec<&Song> = songs.iter().filter(|s| s.kind == kind).collect();
    if list.is_empty() {
        return None;
    }
    list.sort_by_key(|s| s.order);
    if len <= 0.0 {
        return list.first().copied();
    }
    let best = list.iter().copied().min_by(|a, b| (a.duration - len).abs().partial_cmp(&(b.duration - len).abs()).unwrap_or(std::cmp::Ordering::Equal))?;
    if best.duration > 0.0 && (best.duration - len).abs() > 20.0 {
        None
    } else {
        Some(best)
    }
}

fn parse_time(t: &str) -> Option<f64> {
    let mut p = t.trim().split(':');
    let h: f64 = p.next()?.parse().ok()?;
    let m: f64 = p.next()?.parse().ok()?;
    let s: f64 = p.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

fn fmt_time(t: f64) -> String {
    let cs = (t.max(0.0) * 100.0).round() as u64;
    format!("{}:{:02}:{:02}.{:02}", cs / 360_000, cs / 6000 % 60, cs / 100 % 60, cs % 100)
}

/// Moves the lyrics to the top of the screen (see `restyle`).
pub fn restyle_top(ass: &str) -> String {
    restyle(ass, true, 100, 0)
}

/// The look of the lyrics: `top` turns bottom-aligned styles into top-aligned ones, `size` (percent) scales the font
/// of every style, and a `margin` above 0 sets the distance from the edge (pixels of the script). Lines that carry
/// their own position (`\pos`) are left where they are.
pub fn restyle(ass: &str, top: bool, size: u32, margin: u32) -> String {
    let mut out = String::with_capacity(ass.len());
    let (mut align_at, mut size_at, mut margin_at): (Option<usize>, Option<usize>, Option<usize>) = (None, None, None);
    let up = |a: &str| match a.trim() {
        "1" => "7",
        "2" => "8",
        "3" => "9",
        o => o,
    }
    .to_string();
    for line in ass.lines() {
        if let Some(f) = line.strip_prefix("Format:") {
            let cols: Vec<&str> = f.split(',').map(|c| c.trim()).collect();
            if cols.first() == Some(&"Name") {
                let at = |n: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(n));
                (align_at, size_at, margin_at) = (at("Alignment"), at("Fontsize"), at("MarginV"));
            }
            out.push_str(line);
        } else if let Some(rest) = line.strip_prefix("Style:").filter(|_| align_at.is_some()) {
            let mut f: Vec<String> = rest.split(',').map(|c| c.to_string()).collect();
            if top {
                if let Some(a) = align_at.and_then(|i| f.get_mut(i)) {
                    *a = up(a);
                }
            }
            if size != 100 {
                if let Some(a) = size_at.and_then(|i| f.get_mut(i)) {
                    if let Ok(v) = a.trim().parse::<f64>() {
                        *a = format!("{:.0}", (v * size as f64 / 100.0).max(1.0));
                    }
                }
            }
            if margin > 0 {
                if let Some(a) = margin_at.and_then(|i| f.get_mut(i)) {
                    *a = margin.to_string();
                }
            }
            out.push_str("Style:");
            out.push_str(&f.join(","));
        } else if top && line.starts_with("Dialogue:") && !line.contains("\\pos") {
            out.push_str(&line.replace("\\an1", "\\an7").replace("\\an2", "\\an8").replace("\\an3", "\\an9"));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Paints the lyrics in one colour: the filled part gets it, the part still to be sung a paler one. Colour overrides
/// written inside the lines are dropped, so the choice shows everywhere.
pub fn recolor(ass: &str, (r, g, b): (u8, u8, u8)) -> String {
    let ass_col = |r: u8, g: u8, b: u8| format!("&H00{:02X}{:02X}{:02X}", b, g, r);
    let pale = |c: u8| (c as f64 + (255.0 - c as f64) * 0.55).round() as u8;
    let (primary, secondary) = (ass_col(r, g, b), ass_col(pale(r), pale(g), pale(b)));
    let mut out = String::with_capacity(ass.len());
    let (mut p_at, mut s_at): (Option<usize>, Option<usize>) = (None, None);
    for line in ass.lines() {
        if let Some(f) = line.strip_prefix("Format:") {
            let cols: Vec<&str> = f.split(',').map(|c| c.trim()).collect();
            if cols.first() == Some(&"Name") {
                let at = |n: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(n));
                (p_at, s_at) = (at("PrimaryColour"), at("SecondaryColour"));
            }
            out.push_str(line);
        } else if let Some(rest) = line.strip_prefix("Style:").filter(|_| p_at.is_some()) {
            let mut f: Vec<String> = rest.split(',').map(|c| c.to_string()).collect();
            if let Some(a) = p_at.and_then(|i| f.get_mut(i)) {
                *a = primary.clone();
            }
            if let Some(a) = s_at.and_then(|i| f.get_mut(i)) {
                *a = secondary.clone();
            }
            out.push_str("Style:");
            out.push_str(&f.join(","));
        } else if line.starts_with("Dialogue:") {
            out.push_str(&strip_colour_tags(line));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Removes `\c`, `\1c` and `\2c` overrides (colour changes inside the text of a line).
fn strip_colour_tags(line: &str) -> String {
    let b = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            let mut j = i + 1;
            if j < b.len() && (b[j] == b'1' || b[j] == b'2') {
                j += 1;
            }
            if j < b.len() && b[j] == b'c' && j + 1 < b.len() && b[j + 1] == b'&' {
                // To the closing "&" of the value.
                if let Some(end) = line[j + 2..].find('&') {
                    i = j + 2 + end + 1;
                    continue;
                }
            }
        }
        // Copy one character.
        let ch = line[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// What the user chose for the look of the lyrics, on top of the file's own (None keeps the file's value).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub font: Option<String>,
    pub bold: Option<bool>,
    pub outline: Option<f64>,
    pub shadow: Option<f64>,
    /// Colour of the part still to be sung.
    pub unfilled: Option<(u8, u8, u8)>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    /// Colour of the border around the letters and of the shadow (or of the box when `boxed`).
    pub outline_color: Option<(u8, u8, u8)>,
    pub back_color: Option<(u8, u8, u8)>,
    /// Width and height of the letters, percent.
    pub scale_x: Option<u32>,
    pub scale_y: Option<u32>,
    /// Space between the letters, pixels of the script.
    pub spacing: Option<f64>,
    /// Where the lyrics sit, 1 to 9 as on the numeric keypad (7 8 9 on top, 1 2 3 at the bottom).
    pub align: Option<u8>,
    pub margin_l: Option<u32>,
    pub margin_r: Option<u32>,
    /// true: an opaque box behind the letters (BorderStyle 3); false: border and shadow (BorderStyle 1).
    pub boxed: Option<bool>,
}

/// Writes the chosen look into every style of the file, by the names of its `Format:` columns (the same way ASS
/// editors such as Aegisub write them). Only the fields that are set change.
pub fn set_styles(ass: &str, look: &Look) -> String {
    if *look == Look::default() {
        return ass.to_string();
    }
    let ass_col = |r: u8, g: u8, b: u8| format!("&H00{:02X}{:02X}{:02X}", b, g, r);
    let mut out = String::with_capacity(ass.len());
    let mut cols: Option<Vec<String>> = None;
    for line in ass.lines() {
        if let Some(f) = line.strip_prefix("Format:") {
            let c: Vec<String> = f.split(',').map(|c| c.trim().to_string()).collect();
            if c.first().map(|x| x.as_str()) == Some("Name") {
                cols = Some(c);
            }
            out.push_str(line);
        } else if let (Some(rest), Some(cols)) = (line.strip_prefix("Style:"), cols.as_ref()) {
            let mut f: Vec<String> = rest.split(',').map(|c| c.to_string()).collect();
            let at = |n: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(n));
            let mut put = |name: &str, v: String| {
                if let Some(a) = at(name).and_then(|i| f.get_mut(i)) {
                    *a = v;
                }
            };
            if let Some(font) = &look.font {
                put("Fontname", font.clone());
            }
            if let Some(b) = look.bold {
                put("Bold", if b { "-1".to_string() } else { "0".to_string() });
            }
            if let Some(o) = look.outline {
                put("Outline", format!("{}", o));
            }
            if let Some(sh) = look.shadow {
                put("Shadow", format!("{}", sh));
            }
            if let Some((r, g, b)) = look.unfilled {
                put("SecondaryColour", ass_col(r, g, b));
            }
            if let Some(v) = look.italic {
                put("Italic", if v { "-1" } else { "0" }.to_string());
            }
            if let Some(v) = look.underline {
                put("Underline", if v { "-1" } else { "0" }.to_string());
            }
            if let Some((r, g, b)) = look.outline_color {
                put("OutlineColour", ass_col(r, g, b));
            }
            if let Some((r, g, b)) = look.back_color {
                put("BackColour", ass_col(r, g, b));
            }
            if let Some(v) = look.scale_x {
                put("ScaleX", v.to_string());
            }
            if let Some(v) = look.scale_y {
                put("ScaleY", v.to_string());
            }
            if let Some(v) = look.spacing {
                put("Spacing", format!("{}", v));
            }
            if let Some(v) = look.align {
                put("Alignment", v.to_string());
            }
            if let Some(v) = look.margin_l {
                put("MarginL", v.to_string());
            }
            if let Some(v) = look.margin_r {
                put("MarginR", v.to_string());
            }
            if let Some(v) = look.boxed {
                put("BorderStyle", if v { "3" } else { "1" }.to_string());
            }
            out.push_str("Style:");
            out.push_str(&f.join(","));
        } else if look.align.is_some() && line.starts_with("Dialogue:") && !line.contains("\\pos") {
            // The chosen place wins over the \anN tags written inside the lines.
            out.push_str(&strip_an(line));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Removes the `\anN` alignment tags of a line.
fn strip_an(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if line[i..].starts_with("\\an") && b.get(i + 3).map_or(false, |c| c.is_ascii_digit()) && !b.get(i + 4).map_or(false, |c| c.is_ascii_digit()) {
            i += 4;
        } else {
            let ch = line[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Moves every line of an ASS file `by` seconds later.
pub fn shift_ass(ass: &str, by: f64) -> String {
    let mut out = String::with_capacity(ass.len() + 64);
    for line in ass.lines() {
        let kind = if line.starts_with("Dialogue:") {
            Some("Dialogue:")
        } else if line.starts_with("Comment:") {
            Some("Comment:")
        } else {
            None
        };
        match kind {
            Some(k) => {
                let rest = &line[k.len()..];
                let mut f: Vec<&str> = rest.splitn(10, ',').collect();
                if f.len() == 10 {
                    if let (Some(a), Some(b)) = (parse_time(f[1]), parse_time(f[2])) {
                        let (a, b) = (fmt_time(a + by), fmt_time(b + by));
                        f[1] = &a;
                        f[2] = &b;
                        out.push_str(k);
                        out.push_str(&f.join(","));
                        out.push('\n');
                        continue;
                    }
                }
                out.push_str(line);
                out.push('\n');
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    const SEARCH: &str = r#"{"infos":{"count":3},"content":[
      {"kid":"k1","titles":{"jpn":"風のアンセム","qro":"Kaze no Anthem","eng":"Wind's Anthem"},"titles_default_language":"qro","songtypes":[{"name":"OP"}],"songorder":null,"duration":89,
       "lyrics_infos":[{"default":true,"version":"Default","filename":"a.ass"}],"myanimelist_ids":["51553"],
       "series":[{"name":"Tongari Bôshi no Atelier","external_database_ids":{"kitsu":46043,"anilist":147105,"myanimelist":51553}}]},
      {"kid":"k2","titles":{"qro":"Tada Utsukushii Noroi"},"titles_default_language":"qro","songtypes":[{"name":"ED"}],"songorder":1,"duration":89,
       "lyrics_infos":[{"default":true,"filename":"b.ass"}],"myanimelist_ids":["51553"],
       "series":[{"name":"Tongari Bôshi no Atelier","external_database_ids":{"myanimelist":51553}}]},
      {"kid":"k3","titles":{"eng":"AMV"},"songtypes":[{"name":"AMV"}],"duration":100,"lyrics_infos":[{"filename":"c.ass"}],"series":[]}]}"#;

    const ASS: &str = "[Script Info]\nTitle: x\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.50,0:00:03.00,Default,,0,0,0,,Hello, world\nComment: 0,0:01:00.00,0:01:02.00,Default,,0,0,0,,note\n";

    #[test]
    fn look_writes_the_chosen_style_fields() {
        let ass = "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, Bold, Outline, Shadow\nStyle: Default,Arial,40,&H0000A5FF,&H00FFFFFF,0,2,1\n";
        let look = Look { font: Some("Noto Sans".into()), bold: Some(true), outline: Some(3.0), shadow: Some(0.0), unfilled: Some((0x10, 0x20, 0x30)), ..Default::default() };
        let t = set_styles(ass, &look);
        assert!(t.contains("Style: Default,Noto Sans,40,&H0000A5FF,&H00302010,-1,3,0") || t.contains("Style:Default,Noto Sans,40,&H0000A5FF,&H00302010,-1,3,0"));
        assert_eq!(set_styles(ass, &Look::default()), ass);
    }

    #[test]
    fn reads_openings_and_endings_only() {
        let k = parse_search(&Json::parse(SEARCH).unwrap());
        assert_eq!(k.len(), 2);
        assert_eq!(k[0].kind, Kind::Intro);
        assert_eq!(k[0].title, "Kaze no Anthem");
        assert_eq!(k[0].lyrics, "a.ass");
        assert_eq!(k[1].kind, Kind::Credits);
        assert_eq!(k[0].mal, vec![51553]);
        assert_eq!(k[1].order, 1);
    }

    #[test]
    fn shifts_times_and_keeps_commas_in_text() {
        let s = shift_ass(ASS, 62.0);
        assert!(s.contains("Dialogue: 0,0:01:03.50,0:01:05.00,Default,,0,0,0,,Hello, world"));
        assert!(s.contains("Comment: 0,0:02:02.00,0:02:04.00"));
        assert!(s.contains("[Script Info]"));
    }

    #[test]
    fn lyrics_go_to_the_top() {
        let ass = "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, Alignment, MarginL\nStyle: Default,Arial,40,&H00FFFFFF,2,10\nStyle: Up,Arial,40,&H00FFFFFF,8,10\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\an2}hi\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\pos(5,5)\\an2}fixed\n";
        let t = restyle_top(ass);
        assert!(t.contains("Style: Default,Arial,40,&H00FFFFFF,8,10"));
        assert!(t.contains("Style: Up,Arial,40,&H00FFFFFF,8,10"));
        assert!(t.contains("{\\an8}hi"));
        assert!(t.contains("{\\pos(5,5)\\an2}fixed"));
    }

    #[test]
    fn size_and_margin_of_the_lyrics() {
        let ass = "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, Alignment, MarginV\nStyle: Default,Arial,40,&H00FFFFFF,2,10\n";
        let t = restyle(ass, false, 150, 60);
        assert!(t.contains("Style: Default,Arial,60,&H00FFFFFF,2,60"));
        assert_eq!(restyle(ass, false, 100, 0).trim_end(), ass.trim_end());
    }

    #[test]
    fn lyrics_take_the_chosen_colour() {
        let ass = "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, Alignment\nStyle: Default,Arial,40,&H0000A5FF,&H00FFFFFF,2\n[Events]\nFormat: Layer, Start, End, Style, Name, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,{\\1c&H0000A5FF&\\k20}hi {\\c&H123456&}you\n";
        let t = recolor(ass, (0x35, 0x84, 0xe4));
        assert!(t.contains("Style: Default,Arial,40,&H00E48435,"));
        assert!(!t.contains("0000A5FF&"));
        assert!(t.contains("{\\k20}hi {}you"));
    }

    #[test]
    fn shifting_never_goes_below_zero() {
        assert!(shift_ass(ASS, -5.0).contains("Dialogue: 0,0:00:00.00,0:00:00.00"));
    }

    #[test]
    fn finds_by_mal_id_through_any_name_and_downloads_lyrics() {
        let h = Fake::new().route("GET https://kara.moe/api/karas/search?filter=Witch", 200, SEARCH).route("downloads/lyrics/", 200, ASS);
        let songs = songs_for(&h, &[51553], &["Witch Hat Atelier".to_string()]).unwrap();
        assert_eq!(songs.len(), 2);
        assert!(songs[0].label.starts_with("Karaoke OP - Kaze no Anthem"));
        assert!(songs[1].label.starts_with("Karaoke ED 1"));
    }

    #[test]
    fn tries_the_next_name_and_never_takes_another_series() {
        let h = Fake::new().route("filter=Tongari", 200, SEARCH).route("filter=Witch", 200, r#"{"content":[]}"#);
        let names = vec!["Witch Hat Atelier".to_string(), "Tongari Bôshi no Atelier".to_string()];
        assert_eq!(find(&h, &[51553], &names).unwrap().len(), 2);
        // Another MyAnimeList id: nothing of this series is used.
        assert!(find(&h, &[999], &names).unwrap().is_empty());
        // No id known: the series name has to be the one searched for.
        assert_eq!(find(&h, &[], &["Tongari Bôshi no Atelier".to_string()]).unwrap().len(), 2);
        assert!(find(&h, &[], &["Witch Hat Atelier".to_string()]).unwrap().is_empty());
    }

    #[test]
    fn picks_by_length() {
        let mk = |kind, order, d| Song { kind, label: String::new(), duration: d, order, ass: String::new() };
        let songs = vec![mk(Kind::Intro, 1, 90.0), mk(Kind::Intro, 2, 120.0), mk(Kind::Credits, 1, 90.0)];
        assert_eq!(pick(&songs, Kind::Intro, 118.0).unwrap().order, 2);
        assert_eq!(pick(&songs, Kind::Intro, 0.0).unwrap().order, 1);
        assert!(pick(&songs, Kind::Intro, 40.0).is_none());
        assert_eq!(pick(&songs, Kind::Credits, 85.0).unwrap().kind, Kind::Credits);
    }

    #[test]
    fn missing_lyrics_is_an_error() {
        let h = Fake::new().route("karas/search", 200, SEARCH);
        assert!(songs_for(&h, &[51553], &["x".to_string()]).is_err());
    }

    #[test]
    fn a_server_error_is_reported() {
        let h = Fake::new().route("karas/search", 500, "oops");
        assert!(find(&h, &[1], &["x".to_string()]).is_err());
    }
}

#[cfg(test)]
mod look_tests {
    use super::*;

    const ASS: &str = "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, ScaleX, ScaleY, Spacing, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,100,100,0,1,2,1,2,10,10,20\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\an2}la la\nDialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,{\\an2\\pos(5,5)}fixed\n";

    #[test]
    fn writes_every_field_of_the_style() {
        let look = Look {
            italic: Some(true),
            underline: Some(true),
            outline_color: Some((1, 2, 3)),
            back_color: Some((4, 5, 6)),
            scale_x: Some(120),
            scale_y: Some(80),
            spacing: Some(2.5),
            align: Some(8),
            margin_l: Some(33),
            margin_r: Some(44),
            boxed: Some(true),
            ..Default::default()
        };
        let out = set_styles(ASS, &look);
        let st = out.lines().find(|l| l.starts_with("Style:")).unwrap();
        assert_eq!(st, "Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00030201,&H00060504,0,-1,-1,120,80,2.5,3,2,1,8,33,44,20");
        // The place chosen replaces \an inside the lines, except where the line has its own \pos.
        assert!(out.contains(",,{}la la"));
        assert!(out.contains("{\\an2\\pos(5,5)}fixed"));
    }

    #[test]
    fn strip_an_keeps_other_tags() {
        assert_eq!(strip_an("a{\\an7\\b1}b\\and"), "a{\\b1}b\\and");
    }
}
