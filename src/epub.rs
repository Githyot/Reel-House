//! Books from files for the Text mode: `.epub` (a zip with an OPF package), and plain text or HTML files.
//! A book is unpacked into `<data>/books/<id>/` once; `book.json` keeps what the shelf needs (title, author,
//! cover, the chapters in reading order). The chapters are read from the disk, so a book needs no network at all.
//! Standard library only (the zip reader uses the inflate of `gzip`).
use crate::gzip;
use crate::json::Json;
use crate::manga::{Chapter, Series};
use crate::novels;
use std::path::{Path, PathBuf};

/// The pseudo plugin that owns the books made from files: their ids are `novel:file:<book>` and `novel:file:<book>/<n>`.
pub const PLUGIN: &str = "file";

// ----- zip -----

struct Entry {
    name: String,
    method: usize,
    csize: usize,
    offset: usize,
}

pub struct Zip<'a> {
    data: &'a [u8],
    entries: Vec<Entry>,
}

fn u16le(d: &[u8], p: usize) -> usize {
    d.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).unwrap_or(0)
}

fn u32le(d: &[u8], p: usize) -> usize {
    d.get(p..p + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).unwrap_or(0)
}

impl<'a> Zip<'a> {
    pub fn open(data: &'a [u8]) -> Result<Zip<'a>, String> {
        let bad = || "This file is not a valid EPUB (zip) file.".to_string();
        if data.len() < 22 {
            return Err(bad());
        }
        // The end-of-central-directory record is in the last 64 KB (it can have a comment after it).
        let from = data.len().saturating_sub(22 + 65_535);
        let eocd = (from..=data.len() - 22).rev().find(|&i| data[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).ok_or_else(bad)?;
        let count = u16le(data, eocd + 10);
        let mut p = u32le(data, eocd + 16);
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if data.get(p..p + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
                break;
            }
            let (method, csize) = (u16le(data, p + 10), u32le(data, p + 20));
            let (n, x, c) = (u16le(data, p + 28), u16le(data, p + 30), u16le(data, p + 32));
            let offset = u32le(data, p + 42);
            let name = data.get(p + 46..p + 46 + n).map(|b| String::from_utf8_lossy(b).to_string()).ok_or_else(bad)?;
            entries.push(Entry { name, method, csize, offset });
            p += 46 + n + x + c;
        }
        if entries.is_empty() {
            return Err(bad());
        }
        Ok(Zip { data, entries })
    }

    pub fn names(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.name.clone()).collect()
    }

    fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name).or_else(|| self.entries.iter().find(|e| e.name.eq_ignore_ascii_case(name)))
    }

    fn data_of(&self, e: &Entry) -> Result<Vec<u8>, String> {
        let d = self.data;
        if d.get(e.offset..e.offset + 4) != Some(&[0x50, 0x4b, 0x03, 0x04]) {
            return Err(format!("{}: damaged entry", e.name));
        }
        let start = e.offset + 30 + u16le(d, e.offset + 26) + u16le(d, e.offset + 28);
        let raw = d.get(start..start + e.csize).ok_or_else(|| format!("{}: damaged entry", e.name))?;
        match e.method {
            0 => Ok(raw.to_vec()),
            8 => gzip::inflate(raw),
            m => Err(format!("{}: compression {} is not supported", e.name, m)),
        }
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        let e = self.find(name).ok_or_else(|| format!("{} is not in the file", name))?;
        self.data_of(e)
    }

    pub fn read_text(&self, name: &str) -> Result<String, String> {
        Ok(text_of(&self.read(name)?))
    }
}

/// Text of a file: UTF-8, or UTF-16 with a byte order mark; anything else is read as UTF-8 with the bad bytes replaced.
fn text_of(b: &[u8]) -> String {
    if b.starts_with(&[0xff, 0xfe]) || b.starts_with(&[0xfe, 0xff]) {
        let le = b[0] == 0xff;
        let u: Vec<u16> = b[2..].chunks(2).filter(|c| c.len() == 2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return String::from_utf16_lossy(&u);
    }
    let b = b.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(b);
    String::from_utf8_lossy(b).to_string()
}

// ----- a small XML reader (enough for OPF, NCX, XHTML navigation) -----

struct Tag {
    /// Without the prefix, in lower case ("dc:title" is "title").
    name: String,
    attrs: Vec<(String, String)>,
    close: bool,
    /// Where the text after the tag begins.
    end: usize,
}

impl Tag {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, v)| (k == name || k.ends_with(&format!(":{}", name))) && !v.is_empty()).map(|(_, v)| v.as_str())
    }
}

fn tags(xml: &str) -> Vec<Tag> {
    let mut out = Vec::new();
    let b = xml.as_bytes();
    let mut i = 0;
    while let Some(lt) = xml[i..].find('<') {
        let start = i + lt;
        if xml[start..].starts_with("<!--") {
            i = xml[start..].find("-->").map(|e| start + e + 3).unwrap_or(xml.len());
            continue;
        }
        // The end of the tag, with quotes respected.
        let mut j = start + 1;
        let mut quote = 0u8;
        while j < b.len() {
            let c = b[j];
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
            } else if c == b'"' || c == b'\'' {
                quote = c;
            } else if c == b'>' {
                break;
            }
            j += 1;
        }
        if j >= b.len() {
            break;
        }
        let inner = &xml[start + 1..j];
        i = j + 1;
        if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        }
        let close = inner.starts_with('/');
        let body = inner.trim_start_matches('/');
        let full: String = body.chars().take_while(|c| !c.is_whitespace() && *c != '/').collect::<String>().to_lowercase();
        let name = full.rsplit(':').next().unwrap_or("").to_string();
        out.push(Tag { name, attrs: if close { vec![] } else { novels::attrs(body) }, close, end: i });
    }
    out
}

/// The text from `from` up to the closing tag named `name` (other tags in between are dropped).
fn inner_text(xml: &str, from: usize, name: &str) -> String {
    let mut out = String::new();
    let mut rest = &xml[from.min(xml.len())..];
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let Some(gt) = rest[lt..].find('>') else { break };
        let tag = &rest[lt + 1..lt + gt];
        if let Some(c) = tag.strip_prefix('/') {
            if c.trim().to_lowercase().rsplit(':').next() == Some(name) {
                return novels::decode(&out).split_whitespace().collect::<Vec<_>>().join(" ");
            }
        }
        rest = &rest[lt + gt + 1..];
    }
    novels::decode(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn dir_of(path: &str) -> String {
    path.rfind('/').map(|i| path[..i].to_string()).unwrap_or_default()
}

/// A path inside the book, relative to the folder of the file that mentions it.
fn rel_to(dir: &str, href: &str) -> String {
    let h = novels::decode(href);
    let h = h.split('#').next().unwrap_or("");
    let decoded = percent_decode(h);
    novels::join_path(dir, &decoded).trim_start_matches('/').to_string()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
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

// ----- a book -----

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    /// Path of the cover inside the book's folder ("" when there is none).
    pub cover: String,
    /// Title and file (inside the book's folder) of each chapter, in reading order.
    pub chapters: Vec<(String, String)>,
}

impl Book {
    fn to_json(&self) -> Json {
        Json::obj(vec![
            ("id", self.id.as_str().into()),
            ("title", self.title.as_str().into()),
            ("author", self.author.as_str().into()),
            ("description", self.description.as_str().into()),
            ("cover", self.cover.as_str().into()),
            ("chapters", Json::Arr(self.chapters.iter().map(|(t, f)| Json::obj(vec![("title", t.as_str().into()), ("file", f.as_str().into())])).collect())),
        ])
    }

    fn from_json(j: &Json) -> Option<Book> {
        let id = j.get("id").str();
        if id.is_empty() {
            return None;
        }
        Some(Book {
            id,
            title: j.get("title").str(),
            author: j.get("author").str(),
            description: j.get("description").str(),
            cover: j.get("cover").str(),
            chapters: j.get("chapters").items().iter().map(|c| (c.get("title").str(), c.get("file").str())).collect(),
        })
    }
}

/// The id of a series of a book, and of the chapter `n` of it.
pub fn series_id(book: &str) -> String {
    novels::series_id(PLUGIN, book)
}

pub fn chapter_id(book: &str, n: usize) -> String {
    novels::series_id(PLUGIN, &format!("{}/{}", book, n))
}

/// The book and the chapter number inside an id of a chapter (`None` for the id of the series).
pub fn split_chapter(path: &str) -> (&str, Option<usize>) {
    match path.rsplit_once('/') {
        Some((b, n)) => match n.parse::<usize>() {
            Ok(n) => (b, Some(n)),
            Err(_) => (path, None),
        },
        None => (path, None),
    }
}

/// Where the books are kept.
pub fn books_dir(data: &Path) -> PathBuf {
    data.join("books")
}

fn safe_name(name: &str) -> Option<String> {
    let n = name.replace('\\', "/");
    if n.ends_with('/') || n.starts_with('/') || n.split('/').any(|p| p == ".." || p.contains(':')) {
        return None;
    }
    Some(n)
}

/// The series and chapters of a book on the shelf.
pub fn load(books: &Path, book: &str) -> Result<(Series, Vec<Chapter>), String> {
    let dir = books.join(book);
    let text = std::fs::read_to_string(dir.join("book.json")).map_err(|_| "This book is no longer on the disk. Add it again from its file.".to_string())?;
    let b = Book::from_json(&Json::parse(&text)?).ok_or_else(|| "The book's data is damaged.".to_string())?;
    let cover = if b.cover.is_empty() { String::new() } else { dir.join(&b.cover).to_string_lossy().replace('\\', "/") };
    let series = Series {
        id: series_id(&b.id),
        title: b.title.clone(),
        cover,
        description: b.description.clone(),
        authors: if b.author.is_empty() { vec![] } else { vec![b.author.clone()] },
        status: "File".to_string(),
        chapter_count: b.chapters.len() as u32,
        ..Default::default()
    };
    let chapters = b
        .chapters
        .iter()
        .enumerate()
        .map(|(i, (t, _))| Chapter { id: chapter_id(&b.id, i), title: t.clone(), number: (i + 1).to_string(), volume: String::new(), lang: String::new(), pages: 0, published: String::new(), group: String::new() })
        .collect();
    Ok((series, chapters))
}

/// The page of a chapter as HTML, with a `<base>` that tells where its pictures are.
pub fn chapter_html(books: &Path, book: &str, n: usize) -> Result<String, String> {
    let dir = books.join(book);
    let text = std::fs::read_to_string(dir.join("book.json")).map_err(|_| "This book is no longer on the disk.".to_string())?;
    let b = Book::from_json(&Json::parse(&text)?).ok_or_else(|| "The book's data is damaged.".to_string())?;
    let (_, file) = b.chapters.get(n).ok_or_else(|| "This chapter is not in the book.".to_string())?;
    let path = dir.join(file);
    let html = text_of(&std::fs::read(&path).map_err(|e| format!("Could not read the chapter: {}", e))?);
    let folder = path.parent().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let base = if folder.starts_with('/') { format!("file://{}/", folder) } else { format!("file:///{}/", folder) };
    Ok(format!("<base href=\"{}\">{}", base, html))
}

/// A new, empty document of the person's own (written in the editor of the reader).
pub fn create(books: &Path, title: &str) -> Result<Book, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Give the document a name.".to_string());
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let id = format!("d{}", crate::util::fnv_hex(&format!("{}|{}", title, nanos)));
    let dest = books.join(&id);
    std::fs::create_dir_all(&dest).map_err(|e| format!("Could not make the document's folder: {}", e))?;
    std::fs::write(dest.join("ch1.html"), "").map_err(|e| format!("Could not save the document: {}", e))?;
    let book = Book { id, title: title.to_string(), author: String::new(), description: String::new(), cover: String::new(), chapters: vec![(String::new(), "ch1.html".to_string())] };
    save(books, &book)?;
    Ok(book)
}

/// Adds an empty chapter at the end of a book that is on the shelf.
pub fn add_chapter(books: &Path, book: &str, title: &str) -> Result<Book, String> {
    let mut b = existing(books, book).ok_or_else(|| "This book is no longer on the disk.".to_string())?;
    let mut n = b.chapters.len() + 1;
    let mut file = format!("ch{}.html", n);
    while books.join(book).join(&file).exists() {
        n += 1;
        file = format!("ch{}.html", n);
    }
    std::fs::write(books.join(book).join(&file), "").map_err(|e| format!("Could not save the chapter: {}", e))?;
    b.chapters.push((title.trim().to_string(), file));
    save(books, &b)?;
    Ok(b)
}

/// Replaces the page of a chapter with new HTML (after an edit).
pub fn save_chapter(books: &Path, book: &str, n: usize, html: &str) -> Result<(), String> {
    let b = existing(books, book).ok_or_else(|| "This book is no longer on the disk.".to_string())?;
    let (_, file) = b.chapters.get(n).ok_or_else(|| "This chapter is not in the book.".to_string())?;
    std::fs::write(books.join(book).join(file), html).map_err(|e| format!("Could not save the chapter: {}", e))
}

/// Imports a file: an EPUB, or a text or HTML file. Returns the book (already on the disk); a file imported twice gives the same book.
pub fn import(file_name: &str, bytes: &[u8], books: &Path) -> Result<Book, String> {
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "epub" => import_epub(bytes, books),
        "txt" | "md" | "text" => import_text(file_name, bytes, books, false),
        "html" | "htm" | "xhtml" => import_text(file_name, bytes, books, true),
        _ => Err("Only .epub, .txt and .html files can be added.".to_string()),
    }
}

fn book_id(seed: &str, len: usize) -> String {
    format!("b{}", crate::util::fnv_hex(&format!("{}|{}", seed, len)))
}

fn existing(books: &Path, id: &str) -> Option<Book> {
    let t = std::fs::read_to_string(books.join(id).join("book.json")).ok()?;
    Book::from_json(&Json::parse(&t).ok()?)
}

fn save(books: &Path, b: &Book) -> Result<(), String> {
    std::fs::write(books.join(&b.id).join("book.json"), b.to_json().pretty()).map_err(|e| format!("Could not save the book: {}", e))
}

fn import_epub(bytes: &[u8], books: &Path) -> Result<Book, String> {
    let zip = Zip::open(bytes)?;
    let container = zip.read_text("META-INF/container.xml").map_err(|_| "This EPUB has no META-INF/container.xml.".to_string())?;
    let opf_path = tags(&container).iter().find(|t| t.name == "rootfile" && !t.close).and_then(|t| t.attr("full-path").map(|s| s.to_string())).ok_or_else(|| "This EPUB does not say where its package file is.".to_string())?;
    let opf = zip.read_text(&opf_path)?;
    let root = dir_of(&opf_path);
    let tg = tags(&opf);

    let first = |name: &str| tg.iter().find(|t| t.name == name && !t.close).map(|t| inner_text(&opf, t.end, name)).unwrap_or_default();
    let mut title = first("title");
    let author = tg.iter().filter(|t| t.name == "creator" && !t.close).map(|t| inner_text(&opf, t.end, "creator")).filter(|a| !a.is_empty()).collect::<Vec<_>>().join(", ");
    let description = first("description");
    if title.is_empty() {
        title = "Untitled book".to_string();
    }

    // The manifest: id -> (file, type, properties).
    struct Item {
        id: String,
        href: String,
        kind: String,
        props: String,
    }
    let items: Vec<Item> = tg
        .iter()
        .filter(|t| t.name == "item" && !t.close)
        .filter_map(|t| Some(Item { id: t.attr("id")?.to_string(), href: rel_to(&root, t.attr("href")?), kind: t.attr("media-type").unwrap_or("").to_lowercase(), props: t.attr("properties").unwrap_or("").to_string() }))
        .collect();
    let by_id = |id: &str| items.iter().find(|i| i.id == id);

    let id = book_id(&format!("{}|{}", title, author), bytes.len());
    if let Some(b) = existing(books, &id) {
        return Ok(b);
    }
    let dest = books.join(&id);

    // The cover.
    let cover_id = tg.iter().find(|t| t.name == "meta" && t.attr("name") == Some("cover")).and_then(|t| t.attr("content").map(|s| s.to_string()));
    let cover = items
        .iter()
        .find(|i| i.props.split_whitespace().any(|p| p == "cover-image"))
        .or_else(|| cover_id.as_deref().and_then(by_id))
        .map(|i| i.href.clone())
        .unwrap_or_default();

    // Titles of the chapters: the navigation document (EPUB 3) or the NCX (EPUB 2).
    let mut titles: Vec<(String, String)> = Vec::new();
    if let Some(nav) = items.iter().find(|i| i.props.split_whitespace().any(|p| p == "nav")) {
        if let Ok(x) = zip.read_text(&nav.href) {
            let nd = dir_of(&nav.href);
            for t in tags(&x).iter().filter(|t| t.name == "a" && !t.close) {
                if let Some(h) = t.attr("href") {
                    titles.push((rel_to(&nd, h), inner_text(&x, t.end, "a")));
                }
            }
        }
    }
    if titles.is_empty() {
        let ncx = items.iter().find(|i| i.kind.contains("dtbncx")).or_else(|| tg.iter().find(|t| t.name == "spine").and_then(|t| t.attr("toc")).and_then(by_id));
        if let Some(n) = ncx {
            if let Ok(x) = zip.read_text(&n.href) {
                let nd = dir_of(&n.href);
                let tt = tags(&x);
                let mut label = String::new();
                for t in tt.iter() {
                    if t.name == "text" && !t.close {
                        label = inner_text(&x, t.end, "text");
                    } else if t.name == "content" && !t.close {
                        if let Some(s) = t.attr("src") {
                            titles.push((rel_to(&nd, s), std::mem::take(&mut label)));
                        }
                    }
                }
            }
        }
    }

    // Unpack (without fonts, which the reader does not use).
    std::fs::create_dir_all(&dest).map_err(|e| format!("Could not make the book's folder: {}", e))?;
    let result = (|| -> Result<Book, String> {
        for name in zip.names() {
            let Some(rel) = safe_name(&name) else { continue };
            let low = rel.to_lowercase();
            if [".ttf", ".otf", ".woff", ".woff2"].iter().any(|e| low.ends_with(e)) {
                continue;
            }
            let Ok(data) = zip.read(&name) else { continue };
            let path = dest.join(&rel);
            if let Some(p) = path.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, data).map_err(|e| format!("Could not unpack the book: {}", e))?;
        }
        // The chapters: the reading order of the package.
        let mut chapters: Vec<(String, String)> = Vec::new();
        for r in tg.iter().filter(|t| t.name == "itemref" && !t.close) {
            if r.attr("linear") == Some("no") {
                continue;
            }
            let Some(item) = r.attr("idref").and_then(by_id) else { continue };
            if !(item.kind.contains("html") || item.kind.contains("xml") && !item.kind.contains("ncx")) {
                continue;
            }
            let Ok(html) = std::fs::read(dest.join(&item.href)).map(|b| text_of(&b)) else { continue };
            let blocks = novels::blocks(&html);
            if blocks.is_empty() {
                continue;
            }
            let mut name = titles.iter().find(|(f, t)| *f == item.href && !t.is_empty()).map(|(_, t)| t.clone()).unwrap_or_default();
            if name.is_empty() {
                name = blocks.iter().find(|b| b.heading).map(|b| b.text.clone()).unwrap_or_default();
            }
            chapters.push((name, item.href.clone()));
        }
        if chapters.is_empty() {
            return Err("This EPUB has no chapters with text.".to_string());
        }
        let book = Book { id: id.clone(), title, author, description, cover, chapters };
        save(books, &book)?;
        Ok(book)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&dest);
    }
    result
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A chapter heading in a plain text: "Chapter 3", "Capítulo 12 - Name", "Prologue", "第12章".
fn is_heading(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() || l.chars().count() > 80 {
        return false;
    }
    let low = l.to_lowercase();
    let words = ["chapter ", "capítulo ", "capitulo ", "cap. ", "cap ", "prologue", "prólogo", "epilogue", "epílogo", "part ", "parte ", "book "];
    words.iter().any(|w| low.starts_with(w) && (w.ends_with(' ') && low[w.len()..].chars().next().map(|c| c.is_ascii_digit() || "ivxlc".contains(c)).unwrap_or(false) || !w.ends_with(' ')))
        || (l.starts_with('第') && (l.contains('章') || l.contains('話') || l.contains('话')))
}

fn import_text(file_name: &str, bytes: &[u8], books: &Path, html: bool) -> Result<Book, String> {
    let text = text_of(bytes);
    if text.trim().is_empty() {
        return Err("The file is empty.".to_string());
    }
    let stem = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = stem.rsplit_once('.').map(|(a, _)| a).unwrap_or(stem).to_string();
    let id = book_id(&stem, bytes.len());
    if let Some(b) = existing(books, &id) {
        return Ok(b);
    }
    let dest = books.join(&id);
    std::fs::create_dir_all(&dest).map_err(|e| format!("Could not make the book's folder: {}", e))?;
    let mut chapters: Vec<(String, String)> = Vec::new();
    let mut write = |n: usize, title: &str, body: &str| -> Result<(), String> {
        let file = format!("ch{}.html", n);
        std::fs::write(dest.join(&file), body).map_err(|e| format!("Could not save the book: {}", e))?;
        chapters.push((title.to_string(), file));
        Ok(())
    };
    let r = (|| -> Result<(), String> {
        if html {
            return write(1, "", &text);
        }
        // Parts start at each heading; a text without any is one chapter.
        let mut parts: Vec<(String, Vec<String>)> = vec![(String::new(), vec![])];
        for line in text.lines() {
            if is_heading(line) {
                parts.push((line.trim().to_string(), vec![]));
            } else if !line.trim().is_empty() {
                parts.last_mut().map(|p| p.1.push(line.trim().to_string()));
            }
        }
        parts.retain(|(t, l)| !l.is_empty() || !t.is_empty());
        if parts.is_empty() {
            return Err("The file has no text.".to_string());
        }
        for (i, (title, lines)) in parts.iter().enumerate() {
            let mut body = String::new();
            if !title.is_empty() {
                body.push_str(&format!("<h2>{}</h2>", esc(title)));
            }
            for l in lines {
                body.push_str(&format!("<p>{}</p>", esc(l)));
            }
            write(i + 1, title, &body)?;
        }
        Ok(())
    })();
    if let Err(e) = r {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(e);
    }
    let book = Book { id, title: stem, author: String::new(), description: String::new(), cover: String::new(), chapters };
    save(books, &book)?;
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zip with stored (not compressed) entries.
    fn make_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut central: Vec<u8> = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            let crc = gzip::crc32(data);
            out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_at = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_at.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    fn sample() -> Vec<u8> {
        let container = br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        let opf = br#"<?xml version="1.0"?><package xmlns:dc="x"><metadata><dc:title>My Light Novel</dc:title><dc:creator>A. Writer</dc:creator><dc:description>A &amp; B</dc:description><meta name="cover" content="cov"/></metadata>
<manifest><item id="cov" href="images/cover.jpg" media-type="image/jpeg"/><item id="n" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="text/c1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="text/c%202.xhtml" media-type="application/xhtml+xml"/><item id="empty" href="text/e.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="empty"/><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#;
        let nav = br#"<html><body><nav epub:type="toc"><ol><li><a href="text/c1.xhtml">Chapter <b>One</b></a></li><li><a href="text/c%202.xhtml#x">The Return</a></li></ol></nav></body></html>"#;
        let c1 = br#"<html><body><h1>Ignored heading</h1><p>Hello.</p><p><img src="../images/map.png" alt="Map"/></p></body></html>"#;
        let c2 = br#"<html><body><p>Second chapter text.</p></body></html>"#;
        make_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", container),
            ("OEBPS/content.opf", opf),
            ("OEBPS/nav.xhtml", nav),
            ("OEBPS/text/c1.xhtml", c1),
            ("OEBPS/text/c 2.xhtml", c2),
            ("OEBPS/text/e.xhtml", b"<html><body></body></html>"),
            ("OEBPS/images/cover.jpg", b"JPEG"),
            ("OEBPS/images/map.png", b"PNG"),
        ])
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("reelhouse-epub-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_a_zip() {
        let z = make_zip(&[("a.txt", b"hello"), ("dir/b.txt", b"world")]);
        let zip = Zip::open(&z).unwrap();
        assert_eq!(zip.read_text("a.txt").unwrap(), "hello");
        assert_eq!(zip.read_text("DIR/B.TXT").unwrap(), "world");
        assert!(zip.read("nope").is_err());
        assert!(Zip::open(b"not a zip at all, really not").is_err());
    }

    #[test]
    fn imports_an_epub() {
        let books = temp("epub");
        let data = sample();
        let b = import("story.epub", &data, &books).unwrap();
        assert_eq!((b.title.as_str(), b.author.as_str(), b.description.as_str()), ("My Light Novel", "A. Writer", "A & B"));
        assert_eq!(b.cover, "OEBPS/images/cover.jpg");
        // The empty page is left out; the titles come from the navigation, or from the first heading.
        assert_eq!(b.chapters, vec![("Chapter One".to_string(), "OEBPS/text/c1.xhtml".to_string()), ("The Return".to_string(), "OEBPS/text/c 2.xhtml".to_string())]);
        // The same file again is the same book.
        assert_eq!(import("story.epub", &data, &books).unwrap().id, b.id);

        let (series, chapters) = load(&books, &b.id).unwrap();
        assert_eq!(series.title, "My Light Novel");
        assert!(series.cover.ends_with("OEBPS/images/cover.jpg"));
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[1].id, format!("novel:file:{}/1", b.id));
        assert_eq!(split_chapter(&format!("{}/1", b.id)), (b.id.as_str(), Some(1)));
        assert_eq!(split_chapter(&b.id), (b.id.as_str(), None));

        let html = chapter_html(&books, &b.id, 0).unwrap();
        let blocks = novels::blocks(&html);
        assert_eq!(blocks[1].text, "Hello.");
        let img = blocks[2].image.clone().unwrap();
        assert!(img.ends_with("OEBPS/images/map.png"), "{}", img);
        assert!(std::path::Path::new(&img.replace('/', std::path::MAIN_SEPARATOR_STR)).is_file());
        let _ = std::fs::remove_dir_all(&books);
    }

    #[test]
    fn imports_text_files() {
        let books = temp("txt");
        let t = "Prologue\nIt began.\n\nChapter 1: Start\nOne.\nTwo & <three>.\n\nChapter 2\nMore.";
        let b = import("My Story.txt", t.as_bytes(), &books).unwrap();
        assert_eq!(b.title, "My Story");
        assert_eq!(b.chapters.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), vec!["Prologue", "Chapter 1: Start", "Chapter 2"]);
        let html = chapter_html(&books, &b.id, 1).unwrap();
        let blocks = novels::blocks(&html);
        assert_eq!(blocks.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(), vec!["Chapter 1: Start", "One.", "Two & <three>."]);
        assert!(import("a.pdf", b"x", &books).is_err());
        assert!(import("empty.txt", b"  \n", &books).is_err());
        let one = import("page.html", b"<p>Just a page.</p>", &books).unwrap();
        assert_eq!(one.chapters.len(), 1);
        let _ = std::fs::remove_dir_all(&books);
    }

    #[test]
    fn own_documents() {
        let books = temp("own");
        assert!(create(&books, "  ").is_err());
        let b = create(&books, "My notes").unwrap();
        assert_eq!(load(&books, &b.id).unwrap().1.len(), 1);
        let b = add_chapter(&books, &b.id, "Second").unwrap();
        assert_eq!(b.chapters.len(), 2);
        save_chapter(&books, &b.id, 1, "<p>Written here.</p>").unwrap();
        let blocks = novels::blocks(&chapter_html(&books, &b.id, 1).unwrap());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "Written here.");
        assert!(save_chapter(&books, &b.id, 9, "x").is_err());
        let _ = std::fs::remove_dir_all(&books);
    }

    #[test]
    fn xml_pieces() {
        let x = r#"<a:b c="1>2" d='x'>text <i>in</i> side</a:b><x/>"#;
        let t = tags(x);
        assert_eq!(t[0].name, "b");
        assert_eq!(t[0].attr("c"), Some("1>2"));
        assert_eq!(inner_text(x, t[0].end, "b"), "text in side");
        assert_eq!(rel_to("OEBPS/text", "../images/a%20b.png#f"), "OEBPS/images/a b.png");
        assert!(safe_name("../x").is_none() && safe_name("/x").is_none() && safe_name("a/b.txt").is_some());
    }
}
