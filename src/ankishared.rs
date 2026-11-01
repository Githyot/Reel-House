//! AnkiWeb's shared decks (https://ankiweb.net/shared/decks), read the way the site itself reads them.
//!
//! The site is a SvelteKit page that calls a few `/svc/shared/…` services; their answers are protobuf messages
//! (no JSON), so the few fields used here are decoded by hand:
//! - `GET /svc/shared/list-decks?search=<words>`: every deck matching all the words (up to ~2500 rows, no paging and
//!   no order of note: the site sorts and the app pages them). An empty search gives nothing; words under 3 letters
//!   are ignored by the server.
//! - `GET /svc/shared/item-info?sharedId=<id>`: the page of one deck (description, reviews, sample notes, the key
//!   of the download).
//! - `GET /svc/shared/download-deck/<id>?t=<download key>`: the `.apkg`. No login is needed.
//! - Pictures and sounds of the sample notes: `https://ankiweb.net/shared/mpreview/<id>/<file>?<last update>`.
//!
//! The site's categories are fixed searches ("japanese", "anatomy"…), kept here as the same two groups.
//! Requests are page-sized and only made when the user asks (search, open a deck, add it).
use crate::http::{Http, Request};
use std::path::{Path, PathBuf};

pub const SITE: &str = "https://ankiweb.net";

/// Decks uploaded before this moment had a plain-text description (the site shows them with pre-wrap).
const HTML_DESCRIPTIONS_SINCE: i64 = 1_691_900_427;

/// One deck in a list of results.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeckRow {
    pub id: u32,
    pub title: String,
    pub up: u32,
    pub down: u32,
    /// Last change, unix seconds.
    pub mtime: i64,
    pub notes: u32,
    pub audio: u32,
    pub images: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Review {
    pub time: i64,
    pub up: bool,
    pub body: String,
    /// The author's answer, when there is one.
    pub reply: String,
}

/// A sample note: its fields, (name, value) with the value in Anki's HTML.
pub type Sample = Vec<(String, String)>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeckInfo {
    pub id: u32,
    pub title: String,
    pub tags: Vec<String>,
    /// Size of the package in bytes.
    pub size: u64,
    pub updated: i64,
    /// As AnkiWeb has it (HTML for recent decks, plain text for old ones): see `description_html`.
    pub description: String,
    pub up: u32,
    pub down: u32,
    pub notes: u32,
    pub audio: u32,
    pub images: u32,
    pub samples: Vec<Sample>,
    pub download_key: String,
    pub reviews: Vec<Review>,
    /// How many items the same person shared.
    pub by_author: u32,
    pub original_name: String,
    pub too_new_for_rating: bool,
}

/// A category of the site: a fixed search.
pub struct Category {
    pub key: &'static str,
    pub label: &'static str,
}

pub struct CategoryGroup {
    pub label: &'static str,
    /// Icon name of the interface (Icon.qml).
    pub icon: &'static str,
    pub items: &'static [Category],
}

/// The categories of the site's front page, in its order and grouping.
pub fn categories() -> &'static [CategoryGroup] {
    const LANGS: &[Category] = &[
        Category { key: "arabic", label: "Arabic" },
        Category { key: "chinese", label: "Chinese" },
        Category { key: "english", label: "English" },
        Category { key: "french", label: "French" },
        Category { key: "german", label: "German" },
        Category { key: "hebrew", label: "Hebrew" },
        Category { key: "japanese", label: "Japanese" },
        Category { key: "korean", label: "Korean" },
        Category { key: "russian", label: "Russian" },
        Category { key: "spanish", label: "Spanish" },
    ];
    const SCIENCES: &[Category] = &[
        Category { key: "anatomy", label: "Anatomy" },
        Category { key: "biology", label: "Biology" },
        Category { key: "chemistry", label: "Chemistry" },
        Category { key: "geography", label: "Geography" },
        Category { key: "history", label: "History" },
        Category { key: "law", label: "Law" },
        Category { key: "math", label: "Math" },
        Category { key: "music", label: "Music" },
        Category { key: "pathology", label: "Pathology" },
        Category { key: "physics", label: "Physics" },
    ];
    const GROUPS: &[CategoryGroup] = &[
        CategoryGroup { label: "Languages", icon: "translate", items: LANGS },
        CategoryGroup { label: "Art, sciences and trivia", icon: "study", items: SCIENCES },
    ];
    GROUPS
}

pub fn category_label(key: &str) -> Option<&'static str> {
    categories().iter().flat_map(|g| g.items.iter()).find(|c| c.key == key).map(|c| c.label)
}

/// The words sent to the server: the category's word and what was typed.
pub fn search_words(query: &str, cat: &str) -> String {
    let q = query.split_whitespace().collect::<Vec<_>>().join(" ");
    match (cat.trim(), q.as_str()) {
        ("", q) => q.to_string(),
        (c, "") => c.to_string(),
        (c, q) => format!("{} {}", c, q),
    }
}

/// The site does not search for one or two plain letters / digits (it would match everything).
pub fn too_short(words: &str) -> bool {
    let compact: Vec<char> = words.chars().filter(|c| *c != ' ').collect();
    compact.len() < 3 && compact.iter().all(|c| c.is_ascii_alphanumeric())
}

// ---------- protobuf, by hand ----------

enum Val<'a> {
    Int(u64),
    Bytes(&'a [u8]),
    Skip,
}

fn varint(b: &[u8], i: &mut usize) -> Result<u64, String> {
    let mut out = 0u64;
    for shift in (0..64).step_by(7) {
        let c = *b.get(*i).ok_or("AnkiWeb sent a shortened answer.")?;
        *i += 1;
        out |= ((c & 0x7f) as u64) << shift;
        if c < 0x80 {
            return Ok(out);
        }
    }
    Err("AnkiWeb sent an answer that could not be read.".into())
}

/// The fields of one message, in order (repeated fields appear several times).
fn fields(b: &[u8]) -> Result<Vec<(u32, Val<'_>)>, String> {
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let key = varint(b, &mut i)?;
        let (no, wire) = ((key >> 3) as u32, key & 7);
        let v = match wire {
            0 => Val::Int(varint(b, &mut i)?),
            2 => {
                let len = varint(b, &mut i)? as usize;
                let end = i.checked_add(len).filter(|e| *e <= b.len()).ok_or("AnkiWeb sent a shortened answer.")?;
                let s = &b[i..end];
                i = end;
                Val::Bytes(s)
            }
            1 => {
                i += 8;
                Val::Skip
            }
            5 => {
                i += 4;
                Val::Skip
            }
            _ => return Err("AnkiWeb sent an answer that could not be read.".into()),
        };
        if i > b.len() {
            return Err("AnkiWeb sent a shortened answer.".into());
        }
        out.push((no, v));
    }
    Ok(out)
}

fn int(v: &Val) -> u64 {
    if let Val::Int(n) = v { *n } else { 0 }
}

fn text(v: &Val) -> String {
    if let Val::Bytes(b) = v { String::from_utf8_lossy(b).into_owned() } else { String::new() }
}

fn bytes<'a>(v: &Val<'a>) -> &'a [u8] {
    if let Val::Bytes(b) = v { b } else { &[] }
}

/// The answer of `list-decks`.
pub fn parse_list(b: &[u8]) -> Result<Vec<DeckRow>, String> {
    let mut rows = vec![];
    for (no, v) in fields(b)? {
        if no != 1 {
            continue;
        }
        let mut r = DeckRow::default();
        for (n, f) in fields(bytes(&v))? {
            match n {
                1 => r.id = int(&f) as u32,
                2 => r.title = text(&f),
                3 => r.up = int(&f) as u32,
                4 => r.down = int(&f) as u32,
                5 => r.mtime = int(&f) as i64,
                6 => r.notes = int(&f) as u32,
                7 => r.audio = int(&f) as u32,
                8 => r.images = int(&f) as u32,
                _ => {}
            }
        }
        if r.id != 0 {
            rows.push(r);
        }
    }
    Ok(rows)
}

fn parse_review(b: &[u8]) -> Result<Review, String> {
    let mut r = Review::default();
    for (n, f) in fields(b)? {
        match n {
            1 => r.time = int(&f) as i64,
            2 => r.up = int(&f) != 0,
            4 => r.body = text(&f),
            5 => r.reply = text(&f),
            _ => {}
        }
    }
    Ok(r)
}

/// The answer of `item-info`. A missing deck, a hidden one or an add-on is an error.
pub fn parse_info(id: u32, b: &[u8]) -> Result<DeckInfo, String> {
    let mut avail: Option<&[u8]> = None;
    for (no, v) in fields(b)? {
        match no {
            1 => avail = Some(bytes(&v)),
            2 => return Err("This deck is no longer shared on AnkiWeb.".into()),
            3 => return Err("AnkiWeb does not show this deck.".into()),
            _ => {}
        }
    }
    let avail = avail.ok_or("This deck is no longer shared on AnkiWeb.")?;
    let mut d = DeckInfo { id, ..Default::default() };
    let mut is_deck = false;
    for (n, f) in fields(avail)? {
        match n {
            1 => d.reviews.push(parse_review(bytes(&f))?),
            5 => d.title = text(&f),
            6 => d.tags = text(&f).split_whitespace().map(|s| s.to_string()).collect(),
            7 => d.size = int(&f),
            8 => d.updated = int(&f) as i64,
            9 => d.description = text(&f),
            10 => {
                is_deck = true;
                for (dn, df) in fields(bytes(&f))? {
                    match dn {
                        1 => d.notes = int(&df) as u32,
                        2 => d.audio = int(&df) as u32,
                        3 => d.images = int(&df) as u32,
                        4 => {
                            let mut sample = vec![];
                            for (sn, sf) in fields(bytes(&df))? {
                                if sn != 1 {
                                    continue;
                                }
                                let (mut name, mut value) = (String::new(), String::new());
                                for (kn, kf) in fields(bytes(&sf))? {
                                    match kn {
                                        1 => name = text(&kf),
                                        2 => value = text(&kf),
                                        _ => {}
                                    }
                                }
                                sample.push((name, value));
                            }
                            d.samples.push(sample);
                        }
                        5 => d.download_key = text(&df),
                        _ => {}
                    }
                }
            }
            11 => return Err("This is an Anki add-on, not a deck.".into()),
            13 => d.by_author = int(&f) as u32,
            18 => d.up = int(&f) as u32,
            19 => d.down = int(&f) as u32,
            20 => d.original_name = text(&f),
            21 => d.too_new_for_rating = int(&f) != 0,
            _ => {}
        }
    }
    if !is_deck {
        return Err("This is not a deck.".into());
    }
    Ok(d)
}

// ---------- the services ----------

fn get(http: &dyn Http, url: &str) -> Result<Vec<u8>, String> {
    let r = http.send(&Request::get(url).header("Accept", "application/octet-stream").timeout(40)).map_err(|e| {
        crate::debug::log("ankishared", &format!("{}: {}", url, e));
        "AnkiWeb could not be reached. Check the connection and try again.".to_string()
    })?;
    match r.status {
        200 => Ok(r.body),
        404 => Err("This deck is no longer shared on AnkiWeb.".into()),
        429 | 503 => Err("AnkiWeb is busy. Try again in a moment.".into()),
        s => {
            crate::debug::log("ankishared", &format!("{} answered {}", url, s));
            Err(format!("AnkiWeb answered {}", s))
        }
    }
}

/// Every deck matching all the words (the server's order).
pub fn list(http: &dyn Http, words: &str) -> Result<Vec<DeckRow>, String> {
    if words.trim().is_empty() {
        return Ok(vec![]);
    }
    let url = format!("{}/svc/shared/list-decks?search={}", SITE, crate::util::form_encode(words.trim()));
    parse_list(&get(http, &url)?)
}

pub fn deck_info(http: &dyn Http, id: u32) -> Result<DeckInfo, String> {
    let url = format!("{}/svc/shared/item-info?sharedId={}", SITE, id);
    parse_info(id, &get(http, &url)?)
}

/// The folder the packages are downloaded into before they are imported.
pub fn download_dir() -> PathBuf {
    std::env::temp_dir().join("reelhouse-shared")
}

/// Downloads the deck's package into `dir` and gives its path. `progress(done, total)` is called now and then.
pub fn download(http: &dyn Http, info: &DeckInfo, dir: &Path, progress: &dyn Fn(u64, Option<u64>)) -> Result<PathBuf, String> {
    if info.download_key.is_empty() {
        return Err("AnkiWeb did not offer this deck for download.".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the download folder: {}", e))?;
    let path = dir.join(format!("{}.apkg", info.id));
    let url = format!("{}/svc/shared/download-deck/{}?t={}", SITE, info.id, crate::util::form_encode(&info.download_key));
    if let Err(e) = http.download(&url, &path, progress) {
        let _ = std::fs::remove_file(&path);
        crate::debug::log("ankishared", &format!("download {}: {}", info.id, e));
        return Err(format!("The download failed: {}", e));
    }
    // an .apkg is a zip file; anything else is an error page
    let mut head = [0u8; 2];
    let ok = std::fs::File::open(&path).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head)).is_ok() && &head == b"PK";
    if !ok {
        let _ = std::fs::remove_file(&path);
        return Err("AnkiWeb did not send a deck package.".into());
    }
    Ok(path)
}

// ---------- what the interface shows ----------

/// The site's "rating" order: the lower bound of the Wilson interval of the thumbs up (95 %).
pub fn wilson(up: u32, total: u32) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let (z, n) = (1.96f64, total as f64);
    let p = up as f64 / n;
    (p + z * z / (2.0 * n) - z * ((p * (1.0 - p) + z * z / (4.0 * n)) / n).sqrt()) / (1.0 + z * z / n)
}

/// The orders of the site's table: rating (default), modified, notes, audio, images, title.
pub fn sort_rows(rows: &mut [DeckRow], sort: &str) {
    match sort {
        "title" => rows.sort_by_key(|r| r.title.to_lowercase()),
        "modified" => rows.sort_by_key(|r| std::cmp::Reverse(r.mtime)),
        "notes" => rows.sort_by_key(|r| std::cmp::Reverse(r.notes)),
        "audio" => rows.sort_by_key(|r| std::cmp::Reverse(r.audio)),
        "images" => rows.sort_by_key(|r| std::cmp::Reverse(r.images)),
        _ => rows.sort_by(|a, b| {
            let (wa, wb) = (wilson(a.up, a.up + a.down), wilson(b.up, b.up + b.down));
            if wa > 0.0 || wb > 0.0 {
                wb.partial_cmp(&wa).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                a.down.cmp(&b.down)
            }
        }),
    }
}

/// The address of a picture or sound of a sample note.
pub fn preview_url(id: u32, file: &str, updated: i64) -> String {
    format!("{}/shared/mpreview/{}/{}?{}", SITE, id, crate::util::url_encode(file), updated)
}

/// `[image:1.jpg]` and `[sound:0.mp3]` references of a sample field: (kind, file).
pub fn media_refs(value: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut rest = value;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else { break };
        let inner = &after[..end];
        if let Some((kind, file)) = inner.split_once(':') {
            if (kind == "image" || kind == "sound") && !file.is_empty() && !file.contains('[') {
                out.push((kind.to_string(), file.to_string()));
            }
        }
        rest = &after[end + 1..];
    }
    out
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let semi = tail[..tail.len().min(12)].find(';');
        let ent = semi.map(|j| &tail[1..j]).unwrap_or("");
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match (ch, semi) {
            (Some(c), Some(j)) => {
                out.push(c);
                rest = &tail[j + 1..];
            }
            _ => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A field of a sample note as plain text: tags and media references out, line breaks kept as spaces.
pub fn plain(value: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in value.chars() {
        match c {
            '<' => {
                inside = true;
                out.push(' ');
            }
            '>' if inside => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    let mut out = decode_entities(&out);
    for (kind, file) in media_refs(&out.clone()) {
        out = out.replace(&format!("[{}:{}]", kind, file), " ");
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The description as rich text the interface can show safely: old plain-text descriptions are escaped with their
/// line breaks; HTML ones keep only simple formatting and web links (no pictures, scripts, styles or frames).
pub fn description_html(d: &DeckInfo) -> String {
    if d.updated < HTML_DESCRIPTIONS_SINCE && !d.description.contains("</p>") {
        return escape(d.description.trim()).replace("\r\n", "\n").replace('\n', "<br>");
    }
    sanitize_html(&d.description)
}

const KEEP: &[&str] = &[
    "p", "br", "b", "strong", "i", "em", "u", "s", "strike", "del", "ul", "ol", "li", "h1", "h2", "h3", "h4", "h5", "h6",
    "blockquote", "code", "pre", "hr", "a", "div", "span", "sub", "sup", "small", "big", "table", "tr", "td", "th",
    "thead", "tbody",
];
/// Their content is dropped too.
const DROP: &[&str] = &["script", "style", "iframe", "object", "embed", "svg", "math", "head", "title", "noscript", "template", "form", "select", "textarea", "video", "audio"];

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = lower[from..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let start = attrs.len() - rest.len() + 1;
        let raw = attrs[start..].trim_start();
        let v = match raw.chars().next() {
            Some(q @ ('"' | '\'')) => raw[1..].split(q).next().unwrap_or(""),
            _ => raw.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or(""),
        };
        return Some(decode_entities(v));
    }
    None
}

/// Keeps the tags of `KEEP` without their attributes (links keep an http(s) / mailto address), drops the tags and
/// content of `DROP`, keeps the text of everything else, and escapes stray `<` and `&`.
pub fn sanitize_html(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let b = src.as_bytes();
    let mut skip_until: Option<String> = None;
    while i < src.len() {
        let c = b[i];
        if c == b'<' {
            // a comment
            if src[i..].starts_with("<!--") {
                i = src[i..].find("-->").map(|j| i + j + 3).unwrap_or(src.len());
                continue;
            }
            let Some(end) = src[i..].find('>').map(|j| i + j) else {
                if skip_until.is_none() {
                    out.push_str("&lt;");
                }
                i += 1;
                continue;
            };
            let inner = &src[i + 1..end];
            let closing = inner.starts_with('/');
            let body = inner.trim_start_matches('/');
            let name: String = body.chars().take_while(|ch| ch.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
            // a doctype or processing instruction goes; a "<" that starts no tag ("1 < 2") is text
            if inner.starts_with('!') || inner.starts_with('?') {
                i = end + 1;
                continue;
            }
            if name.is_empty() {
                if skip_until.is_none() {
                    out.push_str("&lt;");
                }
                i += 1;
                continue;
            }
            i = end + 1;
            if let Some(stop) = &skip_until {
                if closing && name == *stop {
                    skip_until = None;
                }
                continue;
            }
            if DROP.contains(&name.as_str()) {
                if !closing && !body.trim_end().ends_with('/') {
                    skip_until = Some(name);
                }
                continue;
            }
            if !KEEP.contains(&name.as_str()) {
                continue;
            }
            if closing {
                out.push_str(&format!("</{}>", name));
            } else if name == "a" {
                let href = attr_value(&body[1..], "href").unwrap_or_default();
                let h = href.trim().to_ascii_lowercase();
                if h.starts_with("https://") || h.starts_with("http://") || h.starts_with("mailto:") {
                    out.push_str(&format!("<a href=\"{}\">", escape(href.trim())));
                } else {
                    out.push_str("<a>");
                }
            } else {
                out.push_str(&format!("<{}>", name));
            }
            continue;
        }
        let ch = src[i..].chars().next().unwrap_or(' ');
        if skip_until.is_none() {
            if ch == '&' {
                // a well-formed entity stays, a lone & is escaped
                let tail = &src[i + 1..];
                let ent_len = tail.find(';').filter(|j| *j > 0 && *j <= 10 && tail[..*j].chars().all(|x| x.is_ascii_alphanumeric() || x == '#'));
                if ent_len.is_some() {
                    out.push('&');
                } else {
                    out.push_str("&amp;");
                }
            } else if ch == '>' {
                out.push_str("&gt;");
            } else {
                out.push(ch);
            }
        }
        i += ch.len_utf8();
    }
    out.trim().to_string()
}

/// "4.3 MB"
pub fn size_text(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB", b / 1024.0 / 1024.0 / 1024.0)
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB", b / 1024.0 / 1024.0)
    } else if b >= 1024.0 {
        format!("{} KB", (b / 1024.0).round() as u64)
    } else {
        format!("{} B", bytes)
    }
}

/// "2026-07-08"
pub fn date_text(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    let (y, m, d, ..) = crate::util::civil(secs as u64);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    // Small answers of the real service (2026-10-14), cut down: the first 5 rows of "japanese", and the page of
    // deck 114060567 with two of its reviews.
    const LIST: &[u8] = include_bytes!("testdata/ankishared_list.bin");
    const INFO: &[u8] = include_bytes!("testdata/ankishared_info.bin");

    #[test]
    fn reads_the_list() {
        let rows = parse_list(LIST).unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].id, 911122782);
        assert_eq!(rows[0].title, "Japanese course based on Tae Kim's grammar guide & anime");
        assert_eq!((rows[0].up, rows[0].down), (1568, 29));
        assert!(rows[0].mtime > 1_500_000_000);
        assert!(rows.iter().any(|r| r.id == 114060567 && r.audio > 0 && r.images > 0));
        assert_eq!(parse_list(&[]).unwrap(), vec![]);
        assert!(parse_list(&[0x0a, 0x10, 0x08]).is_err());
    }

    #[test]
    fn reads_a_deck_page() {
        let d = parse_info(114060567, INFO).unwrap();
        assert_eq!(d.title, "Japanese Core 2000 Step 01 Listening Sentence Vocab + Images");
        assert_eq!(d.tags, vec!["Japanese", "Core", "Step", "Listening", "Image"]);
        assert_eq!(d.size, 22873766);
        assert_eq!(d.updated, 1449014400);
        assert_eq!((d.notes, d.audio, d.images), (400, 405, 235));
        assert_eq!((d.up, d.down), (515, 17));
        assert_eq!(d.reviews.len(), 2);
        assert!(d.reviews[0].up && !d.reviews[1].up);
        assert_eq!(d.samples.len(), 3);
        assert_eq!(d.samples[0][0], ("Expression".to_string(), "大学に行ってもっと勉強したいです。".to_string()));
        assert!(d.download_key.starts_with("eyJ"));
        // the old plain-text description keeps its lines
        assert!(description_html(&d).starts_with("This is the 1st step of Japanese Core 2000<br>"));
    }

    #[test]
    fn missing_and_addon_pages_are_errors() {
        assert!(parse_info(1, &[0x10, 0x01]).unwrap_err().contains("no longer shared"));
        assert!(parse_info(1, &[0x18, 0x01]).is_err());
        // available { addon {} }
        assert!(parse_info(1, &[0x0a, 0x02, 0x5a, 0x00]).unwrap_err().contains("add-on"));
    }

    #[test]
    fn search_words_and_short_queries() {
        assert_eq!(search_words("  core   2000 ", "japanese"), "japanese core 2000");
        assert_eq!(search_words("", "anatomy"), "anatomy");
        assert_eq!(search_words("kanji", ""), "kanji");
        assert!(too_short("n5"));
        assert!(too_short(" a b "));
        assert!(!too_short("漢字"));
        assert!(!too_short("kanji"));
        assert_eq!(category_label("japanese"), Some("Japanese"));
    }

    #[test]
    fn sorts_like_the_site() {
        let mut rows = vec![
            DeckRow { id: 1, title: "b".into(), up: 3, down: 0, mtime: 10, notes: 5, ..Default::default() },
            DeckRow { id: 2, title: "A".into(), up: 300, down: 10, mtime: 30, notes: 1, ..Default::default() },
            DeckRow { id: 3, title: "c".into(), up: 0, down: 2, mtime: 20, notes: 9, ..Default::default() },
            DeckRow { id: 4, title: "d".into(), up: 0, down: 0, mtime: 5, notes: 2, ..Default::default() },
        ];
        sort_rows(&mut rows, "rating");
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![2, 1, 4, 3]);
        sort_rows(&mut rows, "title");
        assert_eq!(rows[0].id, 2);
        sort_rows(&mut rows, "modified");
        assert_eq!(rows[0].id, 2);
        sort_rows(&mut rows, "notes");
        assert_eq!(rows[0].id, 3);
        assert!(wilson(10, 10) > wilson(1, 1));
        assert_eq!(wilson(0, 0), 0.0);
    }

    #[test]
    fn sample_fields_and_media() {
        let v = "<div>犬&nbsp;<b>dog</b></div>[sound:0.mp3]<br>[image:1.jpg]";
        assert_eq!(plain(v), "犬 dog");
        assert_eq!(media_refs(v), vec![("sound".into(), "0.mp3".into()), ("image".into(), "1.jpg".into())]);
        assert_eq!(preview_url(5, "a b.jpg", 7), "https://ankiweb.net/shared/mpreview/5/a%20b.jpg?7");
    }

    #[test]
    fn sanitises_descriptions() {
        let html = r#"<p>Made for <a href="https://apps.ankiweb.net/" rel="noopener">Anki</a> &amp; you.</p><script>alert(1)</script><img src="x" onerror="y"><a href="javascript:evil()">bad</a> 1 < 2 & <b onclick="z">bold</b><style>p{}</style>"#;
        let s = sanitize_html(html);
        assert_eq!(s, r#"<p>Made for <a href="https://apps.ankiweb.net/">Anki</a> &amp; you.</p><a>bad</a> 1 &lt; 2 &amp; <b>bold</b>"#);
        let d = DeckInfo { description: "line 1\n<line 2>".into(), updated: 1_000, ..Default::default() };
        assert_eq!(description_html(&d), "line 1<br>&lt;line 2&gt;");
    }

    #[test]
    fn lists_and_downloads_through_http() {
        let http = Fake::new()
            .route("GET https://ankiweb.net/svc/shared/list-decks?search=japanese%20core", 200, "")
            .route("download-deck/7?t=k.e%2Fy", 200, "PK\u{3}\u{4}rest");
        assert_eq!(list(&http, "japanese core").unwrap(), vec![]);
        assert_eq!(list(&http, "  ").unwrap(), vec![]);
        let dir = std::env::temp_dir().join(format!("rh-shared-test-{}", std::process::id()));
        let info = DeckInfo { id: 7, download_key: "k.e/y".into(), ..Default::default() };
        let p = download(&http, &info, &dir, &|_, _| {}).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"PK\x03\x04rest");
        let bad = DeckInfo { id: 8, download_key: "x".into(), ..Default::default() };
        let http2 = Fake::new().route("download-deck/8", 200, "<html>login</html>");
        assert!(download(&http2, &bad, &dir, &|_, _| {}).is_err());
        assert!(!dir.join("8.apkg").exists());
        assert!(download(&http2, &DeckInfo::default(), &dir, &|_, _| {}).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(Fake::new().route("item-info", 500, "x").send(&Request::get("https://ankiweb.net/svc/shared/item-info?sharedId=1")).is_ok());
        assert!(deck_info(&Fake::new().route("item-info", 500, "x"), 1).unwrap_err().contains("500"));
    }

    /// Against the real site (a search and one deck page; nothing is downloaded):
    /// `cargo test --lib --features devproxy ankishared::tests::live -- --ignored`
    #[test]
    #[ignore]
    fn live() {
        let http = crate::net::UreqHttp::new();
        let rows = list(&http, "japanese kanji").unwrap();
        assert!(rows.len() > 10, "{} rows", rows.len());
        let d = deck_info(&http, rows[0].id).unwrap();
        assert!(!d.title.is_empty() && !d.download_key.is_empty());
        println!("{} rows; first: {} ({} notes, {})", rows.len(), d.title, d.notes, size_text(d.size));
    }

    #[test]
    fn sizes_and_dates() {
        assert_eq!(size_text(4_498_266), "4.3 MB");
        assert_eq!(size_text(900), "900 B");
        assert_eq!(date_text(1449014400), "2015-12-02");
        assert_eq!(date_text(0), "");
    }
}
