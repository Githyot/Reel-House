//! The Text mode: plugin catalog, the shapes plugins answer with, and chapter text.
//! Plugins are the JavaScript ones of LNReader (IReader runs the same format); `jsrt` runs them.
use crate::http::Http;
use crate::json::Json;
use crate::manga::{Chapter, Series};

pub const PREFIX: &str = "novel";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub site: String,
    pub lang: String,
    pub version: String,
    pub url: String,
    pub icon: String,
}

pub fn parse_index(j: &Json) -> Vec<Plugin> {
    let clean = |s: String| s.chars().filter(|c| !matches!(*c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}')).collect::<String>().trim().to_string();
    let mut out: Vec<Plugin> = j
        .items()
        .iter()
        .filter_map(|p| {
            let (id, url) = (p.get("id").str(), p.get("url").str());
            if id.is_empty() || url.is_empty() {
                return None;
            }
            Some(Plugin { id, name: clean(p.get("name").str()), site: p.get("site").str(), lang: clean(p.get("lang").str()), version: p.get("version").str(), url, icon: p.get("iconUrl").str() })
        })
        .collect();
    out.sort_by(|a, b| a.lang.cmp(&b.lang).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

pub fn fetch_index(http: &dyn Http, repo: &str) -> Result<Vec<Plugin>, String> {
    let j = crate::http::get_json(http, repo.trim(), 30).map_err(|e| format!("Could not read the plugin list ({})", e))?;
    let list = parse_index(&j);
    if list.is_empty() {
        return Err("The list answered, but it has no plugins in the format this app knows.".to_string());
    }
    Ok(list)
}

/// Downloads the script of a plugin.
pub fn fetch_code(http: &dyn Http, p: &Plugin) -> Result<String, String> {
    let r = http.send(&crate::http::Request::get(&p.url).timeout(40))?;
    if !r.ok() {
        return Err(format!("{} answered {}", p.url.split('/').nth(2).unwrap_or("the server"), r.status));
    }
    let t = r.text();
    if t.trim().is_empty() {
        return Err("The plugin file is empty.".to_string());
    }
    Ok(t)
}

// ----- ids -----

/// "novel:<plugin>:<path>"
pub fn series_id(plugin: &str, path: &str) -> String {
    format!("{}:{}:{}", PREFIX, plugin, path)
}

pub fn is_novel(id: &str) -> bool {
    id.starts_with("novel:")
}

/// The plugin and the path inside an id.
pub fn split_id(id: &str) -> Option<(&str, &str)> {
    let rest = id.strip_prefix("novel:")?;
    let at = rest.find(':')?;
    Some((&rest[..at], &rest[at + 1..]))
}

// ----- what plugins answer -----

/// popularNovels / searchNovels: `[{name, path, cover}]`
pub fn parse_list(plugin: &str, j: &Json) -> Vec<Series> {
    let mut seen = std::collections::HashSet::new();
    j.items()
        .iter()
        .filter_map(|n| {
            let (name, path) = (n.get("name").str(), n.get("path").str());
            if name.is_empty() || path.is_empty() || !seen.insert(path.clone()) {
                return None;
            }
            Some(Series { id: series_id(plugin, &path), title: name.trim().to_string(), cover: n.get("cover").str(), ..Default::default() })
        })
        .collect()
}

fn split_list(s: &str) -> Vec<String> {
    s.split(|c| c == ',' || c == ';' || c == '\n').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

/// A chapter name like "Chapter 12" says nothing the number does not; longer names are kept.
fn chapter_title(name: &str, number: &str) -> String {
    let name = name.trim();
    let low = name.to_lowercase();
    let generic = ["chapter", "capítulo", "capitulo", "ch.", "ch ", "cap.", "cap ", "episode", "ep."];
    if generic.iter().any(|g| low.starts_with(g)) {
        let rest = name.trim_start_matches(|c: char| c.is_alphabetic() || c == '.' || c == ' ').trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
        let rest = rest.trim_start_matches(|c: char| matches!(c, ':' | '-' | '–' | '—' | '.' | ' ')).trim();
        let _ = number;
        return rest.to_string();
    }
    name.to_string()
}

pub fn parse_chapters(plugin: &str, j: &Json, offset: usize) -> Vec<Chapter> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    // Some sites list the newest chapter first: reading order is oldest first.
    let mut items: Vec<&Json> = j.items().iter().collect();
    let nums: Vec<f64> = items.iter().filter_map(|c| c.get("chapterNumber").as_f64().filter(|n| *n > 0.0)).collect();
    if nums.len() >= 2 && nums[0] > nums[nums.len() - 1] {
        items.reverse();
    }
    for (i, c) in items.into_iter().enumerate() {
        let path = c.get("path").str();
        if path.is_empty() || !seen.insert(path.clone()) {
            continue;
        }
        let num = c.get("chapterNumber").as_f64().filter(|n| *n > 0.0).map(crate::json::fmt_num).unwrap_or_else(|| (offset + i + 1).to_string());
        let name = c.get("name").str();
        out.push(Chapter {
            id: series_id(plugin, &path),
            title: chapter_title(&name, &num),
            number: num,
            volume: String::new(),
            lang: String::new(),
            pages: 0,
            published: c.get("releaseTime").str().chars().take(16).collect(),
            group: String::new(),
        });
    }
    out
}

/// parseNovel: the series with its details, its chapters and how many pages of chapters there are.
pub fn parse_novel(plugin: &str, j: &Json) -> Option<(Series, Vec<Chapter>, u32)> {
    let path = j.get("path").str();
    let name = j.get("name").str();
    if path.is_empty() || name.is_empty() {
        return None;
    }
    let mut authors = Vec::new();
    for k in ["author", "artist"] {
        let a = j.get(k).str();
        if !a.trim().is_empty() && !authors.contains(&a.trim().to_string()) {
            authors.push(a.trim().to_string());
        }
    }
    let s = Series {
        id: series_id(plugin, &path),
        title: name.trim().to_string(),
        cover: j.get("cover").str(),
        description: j.get("summary").str().trim().to_string(),
        status: j.get("status").str(),
        year: None,
        tags: if j.get("genres").as_str().is_some() { split_list(&j.get("genres").str()) } else { j.get("genres").strings() },
        authors,
        mal_id: None,
        categories: vec![],
        chapter_count: 0,
    };
    let chapters = parse_chapters(plugin, j.get("chapters"), 0);
    let pages = j.get("totalPages").as_u64().unwrap_or(0) as u32;
    Some((s, chapters, pages))
}

// ----- chapter text -----

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub heading: bool,
    /// The paragraph (for a picture, its description).
    pub text: String,
    /// A picture of the story: a web address, or the path of a file on the disk (books made from files).
    pub image: Option<String>,
}

/// The attributes of a tag (`tag` is what is between `<` and `>`), names in lower case, values decoded.
pub(crate) fn attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b: Vec<char> = tag.chars().collect();
    let mut i = 0;
    // The tag name.
    while i < b.len() && !b[i].is_whitespace() {
        i += 1;
    }
    while i < b.len() {
        while i < b.len() && (b[i].is_whitespace() || b[i] == '/') {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i] != '=' && !b[i].is_whitespace() && b[i] != '/' {
            i += 1;
        }
        let key: String = b[start..i].iter().collect::<String>().to_lowercase();
        if key.is_empty() {
            i += 1;
            continue;
        }
        while i < b.len() && b[i].is_whitespace() {
            i += 1;
        }
        let mut val = String::new();
        if i < b.len() && b[i] == '=' {
            i += 1;
            while i < b.len() && b[i].is_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == '"' || b[i] == '\'') {
                let q = b[i];
                i += 1;
                let s0 = i;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                val = b[s0..i].iter().collect();
                i += 1;
            } else {
                let s0 = i;
                while i < b.len() && !b[i].is_whitespace() {
                    i += 1;
                }
                val = b[s0..i].iter().collect();
            }
        }
        out.push((key, decode(&val)));
    }
    out
}

fn attr<'a>(list: &'a [(String, String)], names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|n| list.iter().find(|(k, v)| k == n && !v.trim().is_empty()).map(|(_, v)| v.trim()))
}

/// Percent-decoding of the file names inside a book.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Joins `rel` to a folder, handling `.` and `..` (always with `/`).
pub(crate) fn join_path(dir: &str, rel: &str) -> String {
    let rel = rel.split(['#', '?']).next().unwrap_or("");
    let mut parts: Vec<&str> = dir.split('/').collect();
    let absolute = rel.starts_with('/');
    if absolute {
        parts.clear();
        parts.push("");
    }
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if !(parts.is_empty() || parts.len() == 1 && parts[0].is_empty()) {
                    parts.pop();
                }
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Where a picture of a chapter is: the address as written, made complete with the base of the chapter.
pub(crate) fn resolve_image(base: &str, src: &str) -> String {
    let src = src.trim();
    if src.contains("://") {
        return src.to_string();
    }
    if let Some(rest) = src.strip_prefix("//") {
        return format!("https://{}", rest);
    }
    if let Some(dir) = base.strip_prefix("file://") {
        // "file:///C:/folder/" is the path "C:/folder/"; "file:///home/x/" is "/home/x/".
        let dir = dir.trim_end_matches('/');
        let dir = if dir.len() > 2 && dir.as_bytes()[0] == b'/' && dir.as_bytes()[2] == b':' { &dir[1..] } else { dir };
        return join_path(dir, &unescape(src));
    }
    if base.starts_with("http") {
        let origin_end = base.find("://").map(|i| base[i + 3..].find('/').map(|j| i + 3 + j).unwrap_or(base.len())).unwrap_or(base.len());
        if src.starts_with('/') {
            return format!("{}{}", &base[..origin_end], src);
        }
        let dir = base[..base.rfind('/').filter(|i| *i >= origin_end).unwrap_or(origin_end)].to_string();
        return format!("{}/{}", dir, src);
    }
    src.to_string()
}

pub(crate) fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "hellip" => Some('…'),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "lsquo" => Some('‘'),
            "rsquo" => Some('’'),
            "ldquo" => Some('“'),
            "rdquo" => Some('”'),
            "laquo" => Some('«'),
            "raquo" => Some('»'),
            _ => {
                if let Some(h) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    u32::from_str_radix(h, 16).ok().and_then(char::from_u32)
                } else if let Some(d) = name.strip_prefix('#') {
                    d.parse::<u32>().ok().and_then(char::from_u32)
                } else {
                    None
                }
            }
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text of a chapter as paragraphs. Plugins answer with HTML, or sometimes with plain text.
pub fn blocks(html: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut cur = String::new();
    let mut heading = false;
    // `<base href>` says where the relative pictures are.
    let mut base = String::new();
    let flush = |cur: &mut String, heading: &mut bool, out: &mut Vec<Block>| {
        let t: String = decode(cur).split_whitespace().collect::<Vec<_>>().join(" ");
        if !t.is_empty() {
            out.push(Block { heading: *heading, text: t, image: None });
        }
        cur.clear();
        *heading = false;
    };
    if !html.contains('<') {
        for line in html.lines() {
            cur.push_str(line);
            flush(&mut cur, &mut heading, &mut out);
        }
        return out;
    }
    let mut rest = html;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            cur.push_str(rest);
            break;
        };
        cur.push_str(&rest[..lt]);
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map(|e| &rest[e + 3..]).unwrap_or("");
            continue;
        }
        let Some(gt) = rest.find('>') else {
            cur.push_str(rest);
            break;
        };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
        match name.as_str() {
            "base" if !closing => {
                if let Some(h) = attr(&attrs(tag), &["href"]) {
                    base = h.to_string();
                }
            }
            "img" | "image" if !closing => {
                flush(&mut cur, &mut heading, &mut out);
                let a = attrs(tag);
                let src = attr(&a, &["src", "data-src", "data-lazy-src", "data-original", "href", "xlink:href"]);
                if let Some(src) = src.filter(|s| !s.starts_with("data:")) {
                    let alt = attr(&a, &["alt", "title"]).unwrap_or("").to_string();
                    out.push(Block { heading: false, text: alt, image: Some(resolve_image(&base, src)) });
                }
            }
            "script" | "style" if !closing => {
                let end = format!("</{}", name);
                let low = rest.to_lowercase();
                rest = low.find(&end).and_then(|e| rest[e..].find('>').map(|g| &rest[e + g + 1..])).unwrap_or("");
            }
            "p" | "div" | "br" | "li" | "tr" | "blockquote" | "hr" | "section" | "article" | "ul" | "ol" | "table" => flush(&mut cur, &mut heading, &mut out),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                flush(&mut cur, &mut heading, &mut out);
                heading = !closing;
            }
            _ => {}
        }
    }
    flush(&mut cur, &mut heading, &mut out);
    out
}


// ----- the editor: the text of a chapter as lines -----

/// The text of a chapter as the editor shows it: one paragraph per line with an empty line between them,
/// "# " in front of a heading and `[[image: address | description]]` for a picture.
pub fn to_markup(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|b| match &b.image {
            Some(src) if b.text.is_empty() => format!("[[image: {}]]", src),
            Some(src) => format!("[[image: {} | {}]]", src, b.text),
            None if b.heading => format!("# {}", b.text),
            None => b.text.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The other way: every line that is not empty is a block.
pub fn parse_markup(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if let Some(inner) = l.strip_prefix("[[image:").and_then(|r| r.strip_suffix("]]")) {
            let (src, alt) = inner.split_once('|').map(|(a, b)| (a.trim(), b.trim())).unwrap_or((inner.trim(), ""));
            if !src.is_empty() {
                out.push(Block { heading: false, text: alt.to_string(), image: Some(src.to_string()) });
                continue;
            }
        }
        match l.strip_prefix("# ") {
            Some(h) if !h.trim().is_empty() => out.push(Block { heading: true, text: h.trim().to_string(), image: None }),
            _ => out.push(Block { heading: false, text: l.to_string(), image: None }),
        }
    }
    out
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// HTML for blocks (what is saved after an edit; `blocks` reads it again).
pub fn to_html(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|b| match &b.image {
            Some(src) => format!("<p><img src=\"{}\" alt=\"{}\"/></p>", esc(src), esc(&b.text)),
            None if b.heading => format!("<h2>{}</h2>", esc(&b.text)),
            None => format!("<p>{}</p>", esc(&b.text)),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_and_ids() {
        let j = Json::parse(r#"[{"id":"zed","name":"Zed","site":"https://z","lang":"English","version":"1.0.0","url":"https://x/z.js","iconUrl":"i"},{"id":"ar","name":"‎Ar","lang":"‎العربية","url":"https://x/a.js"},{"name":"broken"}]"#).unwrap();
        let v = parse_index(&j);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "zed");
        assert_eq!(v[1].name, "Ar");
        assert_eq!(v[1].lang, "العربية");
        let id = series_id("boxnovel", "novel/foo:bar/");
        assert_eq!(split_id(&id), Some(("boxnovel", "novel/foo:bar/")));
        assert!(is_novel(&id) && !is_novel("suwayomi:1"));
    }

    #[test]
    fn plugin_answers() {
        let list = Json::parse(r#"[{"name":"Foo Novel","cover":"https://x/c.jpg","path":"novel/foo/"},{"name":"Bar","path":"novel/bar/"},{"name":"Dup","path":"novel/bar/"},{"name":"","path":"x"}]"#).unwrap();
        let s = parse_list("box", &list);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].id, "novel:box:novel/foo/");
        assert_eq!(s[0].cover, "https://x/c.jpg");

        let n = Json::parse(r#"{"path":"/novel/foo/","name":"Foo Novel","cover":"c","status":"Ongoing","genres":"Fantasy, Action","author":"A. Uthor","summary":" A story. ","chapters":[{"name":"Chapter 1","path":"c/1","releaseTime":"May 5, 2024","chapterNumber":1},{"name":"Chapter 2: The Return","path":"c/2"},{"name":"Prologue","path":"c/0"},{"name":"dup","path":"c/1"}],"totalPages":3}"#).unwrap();
        let (s, ch, pages) = parse_novel("box", &n).unwrap();
        assert_eq!(s.tags, vec!["Fantasy", "Action"]);
        assert_eq!(s.authors, vec!["A. Uthor"]);
        assert_eq!(s.description, "A story.");
        assert_eq!(pages, 3);
        assert_eq!(ch.len(), 3);
        assert_eq!((ch[0].number.as_str(), ch[0].title.as_str()), ("1", ""));
        assert_eq!((ch[1].number.as_str(), ch[1].title.as_str()), ("2", "The Return"));
        assert_eq!((ch[2].number.as_str(), ch[2].title.as_str()), ("3", "Prologue"));
        assert_eq!(ch[0].published, "May 5, 2024");
        assert!(parse_novel("box", &Json::parse("{}").unwrap()).is_none());
        let newest_first = Json::parse(r#"[{"name":"C3","path":"3","chapterNumber":3},{"name":"C2","path":"2","chapterNumber":2},{"name":"C1","path":"1","chapterNumber":1}]"#).unwrap();
        let ch = parse_chapters("box", &newest_first, 0);
        assert_eq!(ch.iter().map(|c| c.number.as_str()).collect::<Vec<_>>(), vec!["1", "2", "3"]);
    }

    #[test]
    fn text_of_a_chapter() {
        let b = blocks("<h2>Chapter 1</h2><p>Hello &amp; welcome,<br>friend.</p><script>var a='<p>x</p>';</script><div>  Second\n paragraph&hellip; </div><!-- hidden --><p></p>");
        assert_eq!(b.iter().map(|x| (x.heading, x.text.as_str())).collect::<Vec<_>>(), vec![(true, "Chapter 1"), (false, "Hello & welcome,"), (false, "friend."), (false, "Second paragraph…")]);
        let p = blocks("Line one\n\nLine two");
        assert_eq!(p.len(), 2);
        assert_eq!(decode("&#8217;&#x41;&unknown; &"), "’A&unknown; &");
    }

    #[test]
    fn the_editor_round_trip() {
        let src = "<h2>One</h2><p>Hello &amp; bye</p><p><img src=\"https://x/a.png\" alt=\"A | map\"/></p><img src=\"C:/b/c.jpg\">";
        let b = blocks(src);
        let m = to_markup(&b);
        assert!(m.starts_with("# One\n\nHello & bye\n\n[[image: https://x/a.png | A | map]]"), "{}", m);
        assert_eq!(parse_markup("# Two\n\n  plain line  \n[[image: C:/b/c.jpg]]\n#nospace\n\n[[image: ]]"), vec![
            Block { heading: true, text: "Two".into(), image: None },
            Block { heading: false, text: "plain line".into(), image: None },
            Block { heading: false, text: String::new(), image: Some("C:/b/c.jpg".into()) },
            Block { heading: false, text: "#nospace".into(), image: None },
            Block { heading: false, text: "[[image: ]]".into(), image: None },
        ]);
        // Saved as HTML and read again, the same blocks come back.
        let again = blocks(&to_html(&parse_markup("# T & <U>\n\nBody \"q\"\n[[image: https://x/y.png | alt]]")));
        assert_eq!(again.iter().map(|x| (x.heading, x.text.as_str(), x.image.as_deref())).collect::<Vec<_>>(), vec![(true, "T & <U>", None), (false, "Body \"q\"", None), (false, "alt", Some("https://x/y.png"))]);
    }

    #[test]
    fn pictures_of_a_chapter() {
        let b = blocks("<p>Before</p><p><img src=\"/img/a.png\" alt=\"A map\"></p><img data-src='b.jpg'><p>After</p><img src=\"data:image/png;base64,AAA\">");
        assert_eq!(b.len(), 4);
        assert_eq!(b[1].image.as_deref(), Some("/img/a.png"));
        assert_eq!(b[1].text, "A map");
        assert_eq!(b[2].image.as_deref(), Some("b.jpg"));
        assert!(b[0].image.is_none() && b[3].image.is_none());
        let f = blocks("<base href=\"file:///C:/books/x/OEBPS/text/\"><img src=\"../images/c%201.jpg\">");
        assert_eq!(f[0].image.as_deref(), Some("C:/books/x/OEBPS/images/c 1.jpg"));
        let g = blocks("<base href=\"file:///home/me/b/ch/\"><svg><image xlink:href=\"../cover.png\"/></svg>");
        assert_eq!(g[0].image.as_deref(), Some("/home/me/b/cover.png"));
        assert_eq!(resolve_image("https://site.org/novel/1/", "//cdn.site.org/x.png"), "https://cdn.site.org/x.png");
        assert_eq!(resolve_image("https://site.org/novel/1/ch2", "p.png"), "https://site.org/novel/1/p.png");
        assert_eq!(resolve_image("https://site.org/novel/1/ch2", "/p.png"), "https://site.org/p.png");
    }
}
