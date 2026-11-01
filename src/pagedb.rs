//! The translation database of a chapter, one Markdown file per chapter: what people corrected by hand.
//! Each page keeps its speech bubbles (box, original text, translation with simple formatting, reviewed or not) and
//! the edits of the picture (brush, eraser, bucket, crop) as a `reelhouse-edit` code block, replayed when the page is
//! read on any computer. The file stays readable by people (headings, quotes, inline HTML for bold, underline…).
//!
//! ````text
//! # Series — Chapter 12
//! <!-- reelhouse-translation 1 -->
//! - series: mangadex:abc
//! - chapter: c123
//! - language: pt
//!
//! ## Page 3
//!
//! ### Bubble 1
//! - box: 0.1200 0.3400 0.2000 0.1000
//! - ocr: 4
//! - reviewed: yes
//! - font: Lora
//! - size: 120
//! - color: #d32f2f
//! - highlight: pill
//!
//! > 何だと？
//!
//! <b>O quê?</b>
//!
//! ```reelhouse-edit
//! [{"t":"stroke","tool":"brush","c":"#ffffff","a":1,"s":0.02,"p":[0.1,0.2,0.11,0.21]}]
//! ```
//! ````
use crate::json::Json;
use std::path::{Path, PathBuf};

pub const MAGIC: &str = "<!-- reelhouse-translation 1 -->";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bubble {
    /// index of the OCR block it corrects; -1 for a box the user drew
    pub ocr: i64,
    /// normalised to the page: 0..1
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub original: String,
    /// the translation, with <b> <i> <u> <s> <mark> <span style="color"> only
    pub html: String,
    pub reviewed: bool,
    /// removed by the user (an OCR block that is not a bubble)
    pub deleted: bool,
    /// the look of the whole bubble, so another computer draws it the same: font family ("" = the reader's own),
    /// size in percent of the size fitted to the box (0 = not set, read as 100), text colour ("" = the default dark
    /// ink) and how highlighted words are drawn ("pill" round ends, "plain" square; "" = pill)
    pub font: String,
    pub size: u32,
    pub color: String,
    pub highlight: String,
}

impl Bubble {
    /// Makes the look fields safe whatever wrote them (QML, a file from another computer): a short one-line font
    /// name, a size the reader can draw, a #rgb/#rrggbb colour and a known highlight style; anything else is unset.
    pub fn clean_look(&mut self) {
        self.font = one_line(&self.font).chars().filter(|c| !c.is_control()).take(80).collect();
        if self.size != 0 {
            self.size = self.size.clamp(30, 400);
        }
        self.color = hex_color(&self.color).unwrap_or_default();
        if self.highlight != "pill" && self.highlight != "plain" {
            self.highlight.clear();
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    /// 1-based, as people count pages
    pub number: usize,
    pub bubbles: Vec<Bubble>,
    /// the edit operations of the picture, a JSON array (see `qt/src/imageprovider.cpp`, EditedPage)
    pub ops: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chapter {
    pub series_id: String,
    pub series_title: String,
    pub chapter_id: String,
    pub chapter_label: String,
    pub language: String,
    pub pages: Vec<Page>,
}

pub fn dir(data: &Path) -> PathBuf {
    data.join("translations")
}

/// Where the file of a chapter lives: readable folder and file names, made safe for every system.
pub fn file_for(data: &Path, series_id: &str, series_title: &str, chapter_id: &str, chapter_label: &str) -> PathBuf {
    fn clean(s: &str, max: usize) -> String {
        let t: String = s.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '.' { c } else { '_' }).collect();
        t.trim().chars().take(max).collect()
    }
    let series = format!("{} [{}]", clean(series_title, 60), crate::util::fnv_hex(series_id).chars().take(8).collect::<String>());
    let chapter = format!("{} [{}].md", clean(chapter_label, 40), crate::util::fnv_hex(chapter_id).chars().take(8).collect::<String>());
    dir(data).join(series).join(chapter)
}

impl Chapter {
    pub fn page_mut(&mut self, number: usize) -> &mut Page {
        if let Some(i) = self.pages.iter().position(|p| p.number == number) {
            return &mut self.pages[i];
        }
        self.pages.push(Page { number, ..Page::default() });
        self.pages.sort_by_key(|p| p.number);
        let i = self.pages.iter().position(|p| p.number == number).unwrap_or(0);
        &mut self.pages[i]
    }

    pub fn page(&self, number: usize) -> Option<&Page> {
        self.pages.iter().find(|p| p.number == number)
    }

    pub fn is_empty(&self) -> bool {
        self.pages.iter().all(|p| p.bubbles.is_empty() && ops_empty(&p.ops))
    }

    pub fn to_markdown(&self) -> String {
        let mut o = String::new();
        o.push_str(&format!("# {} — {}\n{}\n", one_line(&self.series_title), one_line(&self.chapter_label), MAGIC));
        o.push_str(&format!("- series: {}\n- series-title: {}\n- chapter: {}\n- chapter-label: {}\n- language: {}\n", one_line(&self.series_id), one_line(&self.series_title), one_line(&self.chapter_id), one_line(&self.chapter_label), one_line(&self.language)));
        for p in &self.pages {
            if p.bubbles.is_empty() && ops_empty(&p.ops) {
                continue;
            }
            o.push_str(&format!("\n## Page {}\n", p.number));
            for (i, b) in p.bubbles.iter().enumerate() {
                o.push_str(&format!("\n### Bubble {}\n", i + 1));
                o.push_str(&format!("- box: {:.4} {:.4} {:.4} {:.4}\n", b.x, b.y, b.w, b.h));
                o.push_str(&format!("- ocr: {}\n", b.ocr));
                o.push_str(&format!("- reviewed: {}\n", if b.reviewed { "yes" } else { "no" }));
                if b.deleted {
                    o.push_str("- deleted: yes\n");
                }
                if !b.font.trim().is_empty() {
                    o.push_str(&format!("- font: {}\n", one_line(&b.font)));
                }
                if b.size != 0 && b.size != 100 {
                    o.push_str(&format!("- size: {}\n", b.size));
                }
                if !b.color.is_empty() {
                    o.push_str(&format!("- color: {}\n", b.color));
                }
                if !b.highlight.is_empty() {
                    o.push_str(&format!("- highlight: {}\n", b.highlight));
                }
                if !b.original.trim().is_empty() {
                    o.push('\n');
                    for l in b.original.trim().lines() {
                        o.push_str(&format!("> {}\n", l));
                    }
                }
                if !b.html.trim().is_empty() {
                    o.push('\n');
                    // a line of the translation never starts a heading, a list or a fence by accident
                    for l in b.html.trim().replace("<br>", "\n").lines() {
                        let l = l.trim_end();
                        if l.starts_with('#') || l.starts_with("- ") || l.starts_with('>') || l.starts_with("```") {
                            o.push('\\');
                        }
                        o.push_str(l);
                        o.push_str("  \n");
                    }
                }
            }
            if !ops_empty(&p.ops) {
                o.push_str("\n```reelhouse-edit\n");
                o.push_str(p.ops.trim());
                o.push_str("\n```\n");
            }
        }
        o
    }

    pub fn from_markdown(md: &str) -> Result<Chapter, String> {
        if !md.contains(MAGIC) {
            return Err("This file is not a Reelhouse translation (the marker line is missing).".into());
        }
        let mut c = Chapter::default();
        let mut page: Option<usize> = None;
        let mut bubble: Option<Bubble> = None;
        let mut in_ops = false;
        let mut ops = String::new();
        let mut text: Vec<String> = Vec::new();
        let mut orig: Vec<String> = Vec::new();
        fn finish(c: &mut Chapter, page: Option<usize>, b: &mut Option<Bubble>, text: &mut Vec<String>, orig: &mut Vec<String>) {
            if let (Some(p), Some(mut x)) = (page, b.take()) {
                x.original = orig.join("\n");
                x.html = text.join("<br>");
                c.page_mut(p).bubbles.push(x);
            }
            text.clear();
            orig.clear();
        }
        for raw in md.lines() {
            let line = raw.trim_end();
            if in_ops {
                if line.trim() == "```" {
                    in_ops = false;
                    if let Some(p) = page {
                        c.page_mut(p).ops = ops.trim().to_string();
                    }
                    ops.clear();
                } else {
                    ops.push_str(line);
                    ops.push('\n');
                }
                continue;
            }
            if line.trim() == "```reelhouse-edit" {
                finish(&mut c, page, &mut bubble, &mut text, &mut orig);
                in_ops = true;
                continue;
            }
            if let Some(n) = line.strip_prefix("## Page ") {
                finish(&mut c, page, &mut bubble, &mut text, &mut orig);
                page = n.trim().parse().ok();
                if let Some(p) = page {
                    c.page_mut(p);
                }
                continue;
            }
            if line.starts_with("### Bubble") {
                finish(&mut c, page, &mut bubble, &mut text, &mut orig);
                bubble = Some(Bubble { ocr: -1, ..Bubble::default() });
                continue;
            }
            if line.starts_with("# ") || line.trim() == MAGIC {
                continue;
            }
            if let Some(kv) = line.strip_prefix("- ") {
                let (k, v) = kv.split_once(':').map(|(a, b)| (a.trim(), b.trim())).unwrap_or((kv, ""));
                match (&mut bubble, k) {
                    (Some(b), "box") => {
                        let n: Vec<f64> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                        if n.len() == 4 {
                            (b.x, b.y, b.w, b.h) = (n[0], n[1], n[2], n[3]);
                        }
                        continue;
                    }
                    (Some(b), "ocr") => {
                        b.ocr = v.parse().unwrap_or(-1);
                        continue;
                    }
                    (Some(b), "reviewed") => {
                        b.reviewed = v == "yes";
                        continue;
                    }
                    (Some(b), "deleted") => {
                        b.deleted = v == "yes";
                        continue;
                    }
                    (Some(b), "font") => {
                        b.font = v.to_string();
                        b.clean_look();
                        continue;
                    }
                    (Some(b), "size") => {
                        b.size = v.trim_end_matches('%').trim().parse().unwrap_or(0);
                        b.clean_look();
                        continue;
                    }
                    (Some(b), "color") => {
                        b.color = v.to_string();
                        b.clean_look();
                        continue;
                    }
                    (Some(b), "highlight") => {
                        b.highlight = v.to_ascii_lowercase();
                        b.clean_look();
                        continue;
                    }
                    (None, "series") => {
                        c.series_id = v.to_string();
                        continue;
                    }
                    (None, "series-title") => {
                        c.series_title = v.to_string();
                        continue;
                    }
                    (None, "chapter") => {
                        c.chapter_id = v.to_string();
                        continue;
                    }
                    (None, "chapter-label") => {
                        c.chapter_label = v.to_string();
                        continue;
                    }
                    (None, "language") => {
                        c.language = v.to_string();
                        continue;
                    }
                    _ => {}
                }
            }
            if bubble.is_some() {
                if let Some(q) = line.strip_prefix("> ").or_else(|| if line == ">" { Some("") } else { None }) {
                    orig.push(q.to_string());
                } else if !line.trim().is_empty() {
                    let l = line.strip_suffix("  ").unwrap_or(line);
                    text.push(l.strip_prefix('\\').unwrap_or(l).to_string());
                }
            }
        }
        finish(&mut c, page, &mut bubble, &mut text, &mut orig);
        Ok(c)
    }

    pub fn to_json(&self) -> Json {
        Json::Arr(
            self.pages
                .iter()
                .map(|p| {
                    Json::obj(vec![
                        ("page", Json::Num(p.number as f64)),
                        ("ops", Json::Str(p.ops.clone())),
                        (
                            "bubbles",
                            Json::Arr(
                                p.bubbles
                                    .iter()
                                    .map(|b| {
                                        Json::obj(vec![
                                            ("ocr", Json::Num(b.ocr as f64)),
                                            ("x", Json::Num(b.x)),
                                            ("y", Json::Num(b.y)),
                                            ("w", Json::Num(b.w)),
                                            ("h", Json::Num(b.h)),
                                            ("original", Json::Str(b.original.clone())),
                                            ("html", Json::Str(b.html.clone())),
                                            ("reviewed", Json::Bool(b.reviewed)),
                                            ("deleted", Json::Bool(b.deleted)),
                                            ("font", Json::Str(b.font.clone())),
                                            ("size", Json::Num(if b.size == 0 { 100.0 } else { b.size as f64 })),
                                            ("color", Json::Str(b.color.clone())),
                                            ("highlight", Json::Str(b.highlight.clone())),
                                        ])
                                    })
                                    .collect(),
                            ),
                        ),
                    ])
                })
                .collect(),
        )
    }
}

pub fn ops_empty(ops: &str) -> bool {
    let t = ops.trim();
    t.is_empty() || t == "[]"
}

fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ").trim().to_string()
}

/// Only the formatting the bubbles use survives: b, i, u, s, br, mark and span with a colour; everything else is text.
pub fn clean_html(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(p) = rest.find('<') {
        out.push_str(&rest[..p]);
        let Some(e) = rest[p..].find('>') else {
            out.push_str(&rest[p..].replace('<', "&lt;"));
            rest = "";
            break;
        };
        let tag = &rest[p + 1..p + e];
        let low = tag.to_ascii_lowercase();
        let name = low.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        let keep = match name {
            "b" | "i" | "u" | "s" | "br" | "mark" => {
                if low.starts_with('/') {
                    format!("</{}>", name)
                } else if name == "br" {
                    "<br>".to_string()
                } else if name == "mark" {
                    match style_color(&low, "background") {
                        Some(c) => format!("<mark style=\"background:{}\">", c),
                        None => "<mark>".to_string(),
                    }
                } else {
                    format!("<{}>", name)
                }
            }
            "span" => {
                if low.starts_with('/') {
                    "</span>".to_string()
                } else {
                    match style_color(&low, "color") {
                        Some(c) => format!("<span style=\"color:{}\">", c),
                        None => "<span>".to_string(),
                    }
                }
            }
            _ => String::new(),
        };
        out.push_str(&keep);
        rest = &rest[p + e + 1..];
    }
    out.push_str(rest);
    out
}

fn style_color(tag: &str, prop: &str) -> Option<String> {
    let i = tag.find(&format!("{}:", prop))?;
    let v: String = tag[i + prop.len() + 1..].trim_start().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '#').collect();
    hex_color(&v)
}

/// "#rgb" or "#rrggbb" (lower case), else None.
fn hex_color(v: &str) -> Option<String> {
    let v = v.trim();
    let digits = v.strip_prefix('#')?;
    if (digits.len() == 6 || digits.len() == 3) && digits.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(v.to_ascii_lowercase())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_round_trip() {
        let mut c = Chapter { series_id: "md:abc".into(), series_title: "Berserk".into(), chapter_id: "c1".into(), chapter_label: "Chapter 1".into(), language: "pt".into(), pages: vec![] };
        c.page_mut(3).bubbles.push(Bubble { ocr: 2, x: 0.1, y: 0.2, w: 0.3, h: 0.1, original: "何だと？\n二行".into(), html: "<b>O quê?</b><br># não é título".into(), reviewed: true, deleted: false, ..Bubble::default() });
        c.page_mut(3).ops = r##"[{"t":"fill","x":0.5,"y":0.5,"c":"#ffffff","a":1,"tol":40}]"##.into();
        let md = c.to_markdown();
        let back = Chapter::from_markdown(&md).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn bubble_look_round_trip() {
        let mut c = Chapter { series_id: "md:abc".into(), series_title: "Berserk".into(), chapter_id: "c1".into(), chapter_label: "Chapter 1".into(), language: "pt".into(), pages: vec![] };
        let styled = Bubble {
            ocr: -1,
            x: 0.5,
            y: 0.5,
            w: 0.2,
            h: 0.1,
            html: "<mark style=\"background:#fff176\">Olá</mark>".into(),
            font: "PT Serif".into(),
            size: 130,
            color: "#d32f2f".into(),
            highlight: "plain".into(),
            ..Bubble::default()
        };
        c.page_mut(1).bubbles.push(styled.clone());
        // a bubble without a look writes no look lines (old files stay as they were)
        c.page_mut(1).bubbles.push(Bubble { ocr: 0, html: "oi".into(), ..Bubble::default() });
        let md = c.to_markdown();
        assert!(md.contains("- font: PT Serif\n- size: 130\n- color: #d32f2f\n- highlight: plain\n"));
        assert_eq!(md.matches("- font:").count(), 1);
        let back = Chapter::from_markdown(&md).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.pages[0].bubbles[0], styled);
    }

    #[test]
    fn bubble_look_is_cleaned() {
        let md = format!("# A — B\n{}\n- series: s\n- chapter: c\n\n## Page 1\n\n### Bubble 1\n- box: 0 0 0.1 0.1\n- ocr: -1\n- reviewed: no\n- font: Comic Neue\n- size: 9000\n- color: red; evil\n- highlight: Wavy\n\noi  \n", MAGIC);
        let c = Chapter::from_markdown(&md).unwrap();
        let b = &c.pages[0].bubbles[0];
        assert_eq!(b.font, "Comic Neue");
        assert_eq!(b.size, 400);
        assert_eq!(b.color, "");
        assert_eq!(b.highlight, "");
        assert_eq!(b.html, "oi");
        let mut x = Bubble { font: "A\nB".into(), size: 5, color: "#ABCDEF".into(), highlight: "pill".into(), ..Bubble::default() };
        x.clean_look();
        assert_eq!((x.font.as_str(), x.size, x.color.as_str(), x.highlight.as_str()), ("A B", 30, "#abcdef", "pill"));
        // the JSON for the reader always has a size
        let j = Chapter { pages: vec![Page { number: 1, bubbles: vec![Bubble::default()], ops: String::new() }], ..Chapter::default() }.to_json();
        assert_eq!(j.path("0.bubbles.0.size").as_f64(), Some(100.0));
    }

    #[test]
    fn html_is_cleaned() {
        assert_eq!(clean_html("<p style=\"x\"><b>a</b><script>x</script><span style=\"color:#ff0000; font-size:3px\">r</span></p>"), "<b>a</b>x<span style=\"color:#ff0000\">r</span>");
    }
}
