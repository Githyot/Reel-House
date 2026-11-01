//! Text recognition for the pages of the reader: the page goes to an OCR service (OCR.space or Google Cloud Vision) and
//! comes back as blocks of text with their place on the page, so the reader can let you select, highlight and
//! translate what is written in a speech bubble.
use crate::http::{Http, Request};
use crate::json::Json;

/// A piece of text and where it is on the page (all four numbers go from 0 to 1, from the top left corner).
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug)]
pub struct Cfg {
    /// "ocrspace" or "vision"
    pub provider: String,
    pub key: String,
    /// "jpn", "kor", "chs", "eng" or "auto"
    pub lang: String,
}

/// The languages the reader offers for the text of the pages (OCR.space codes).
pub const LANGS: [(&str, &str); 5] = [("jpn", "Japanese"), ("kor", "Korean"), ("chs", "Chinese"), ("eng", "English"), ("auto", "Automatic")];

/// The language for the translator ("auto" lets it find out).
pub fn source_code(lang: &str) -> &'static str {
    match lang {
        "jpn" => "ja",
        "kor" => "ko",
        "chs" | "cht" => "zh",
        "eng" => "en",
        _ => "auto",
    }
}

/// The largest picture the free service takes.
pub const OCRSPACE_LIMIT: usize = 1_000_000;

pub fn recognize(http: &dyn Http, cfg: &Cfg, jpeg: &[u8], width: u32, height: u32) -> Result<Vec<Block>, String> {
    if width == 0 || height == 0 {
        return Err("The page has no size.".to_string());
    }
    let b64 = crate::suwayomi::base64(jpeg);
    if cfg.provider == "vision" {
        if cfg.key.trim().is_empty() {
            return Err("Google Cloud Vision needs an API key (Preferences > Images).".to_string());
        }
        let hint = match source_code(&cfg.lang) {
            "auto" => Vec::new(),
            c => vec![c.to_string()],
        };
        let body = Json::obj(vec![(
            "requests",
            Json::Arr(vec![Json::obj(vec![
                ("image", Json::obj(vec![("content", b64.into())])),
                ("features", Json::Arr(vec![Json::obj(vec![("type", "DOCUMENT_TEXT_DETECTION".into())])])),
                ("imageContext", Json::obj(vec![("languageHints", hint.into())])),
            ])]),
        )]);
        let url = format!("https://vision.googleapis.com/v1/images:annotate?key={}", cfg.key.trim());
        let r = http.send(&Request::post(&url).json_body(body.dump()).timeout(90))?;
        if !r.ok() {
            let j = r.json().unwrap_or(Json::Null);
            let m = j.path("error.message").str();
            return Err(format!("Google Cloud Vision answered {}: {}", r.status, if m.is_empty() { r.text().chars().take(120).collect() } else { m }));
        }
        return parse_vision(&r.json()?);
    }
    if jpeg.len() > OCRSPACE_LIMIT && cfg.key.trim().is_empty() {
        return Err("The page is too big for the free OCR.space service.".to_string());
    }
    let key = if cfg.key.trim().is_empty() { "helloworld" } else { cfg.key.trim() };
    let lang = if cfg.lang.is_empty() { "jpn" } else { cfg.lang.as_str() };
    // Engine 2 reads Latin text better and finds the language by itself.
    let engine = if matches!(lang, "eng" | "auto") { "2" } else { "1" };
    let data = format!("data:image/jpeg;base64,{}", b64);
    let r = http.send(
        &Request::post("https://api.ocr.space/parse/image")
            .form(&[("apikey", key), ("language", lang), ("isOverlayRequired", "true"), ("OCREngine", engine), ("scale", "true"), ("detectOrientation", "true"), ("base64Image", &data)])
            .timeout(90),
    )?;
    if r.status == 429 {
        return Err("The shared key of OCR.space allows only about 30 pages an hour and it is used up. Get a free key at ocr.space/ocrapi/freekey and paste it in Preferences > Images > Text of the pages.".to_string());
    }
    if !r.ok() {
        return Err(format!("OCR.space answered {}.", r.status));
    }
    parse_ocrspace(&r.json()?, width, height)
}

/// One line of text of OCR.space, in pixels.
/// A slice of a long page (a webtoon strip is read in pieces, or the text would be too small to read).
/// `y0` and `y1` are where the slice starts and ends in the page, as a fraction of its height.
pub struct Tile {
    pub jpeg: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub y0: f64,
    pub y1: f64,
}

/// Reads every slice and puts the blocks together in page coordinates. A block seen twice in the overlap of two slices is kept once.
pub fn recognize_tiles(http: &dyn Http, cfg: &Cfg, tiles: &[Tile]) -> Result<Vec<Block>, String> {
    let mut all: Vec<Block> = Vec::new();
    let mut first_err: Option<String> = None;
    let mut worked = false;
    for (k, t) in tiles.iter().enumerate() {
        if k > 0 {
            std::thread::sleep(std::time::Duration::from_millis(350));
        }
        match recognize(http, cfg, &t.jpeg, t.w, t.h) {
            Ok(v) => {
                worked = true;
                let span = t.y1 - t.y0;
                for b in v {
                    let mapped = Block { text: b.text, x: b.x, y: t.y0 + b.y * span, w: b.w, h: b.h * span };
                    match all.iter().position(|o| overlap(o, &mapped) > 0.4) {
                        Some(i) => {
                            if mapped.text.chars().count() > all[i].text.chars().count() {
                                all[i] = mapped;
                            }
                        }
                        None => all.push(mapped),
                    }
                }
            }
            Err(e) => first_err = first_err.or(Some(e)),
        }
    }
    if !worked {
        return Err(first_err.unwrap_or_else(|| "The page has no size.".to_string()));
    }
    sort_blocks(&mut all);
    Ok(all)
}

/// How much of the smaller block lies inside the other (0 to 1).
fn overlap(a: &Block, b: &Block) -> f64 {
    let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    if w <= 0.0 || h <= 0.0 {
        return 0.0;
    }
    w * h / (a.w * a.h).min(b.w * b.h).max(1e-9)
}

#[derive(Clone, Debug)]
struct Line {
    text: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

pub fn parse_ocrspace(j: &Json, width: u32, height: u32) -> Result<Vec<Block>, String> {
    if j.get("IsErroredOnProcessing").as_bool().unwrap_or(false) {
        let m = match j.get("ErrorMessage") {
            Json::Arr(v) => v.iter().map(|x| x.str()).collect::<Vec<_>>().join(" "),
            o => o.str(),
        };
        return Err(format!("OCR.space: {}", if m.is_empty() { "the page could not be read".to_string() } else { m }));
    }
    let mut lines: Vec<Line> = Vec::new();
    for res in j.get("ParsedResults").items() {
        for l in res.path("TextOverlay.Lines").items() {
            let words = l.get("Words").items();
            if words.is_empty() {
                continue;
            }
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, 0.0f64, 0.0f64);
            for w in words {
                let (x, y) = (w.get("Left").as_f64().unwrap_or(0.0), w.get("Top").as_f64().unwrap_or(0.0));
                let (ww, hh) = (w.get("Width").as_f64().unwrap_or(0.0), w.get("Height").as_f64().unwrap_or(0.0));
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + ww);
                y1 = y1.max(y + hh);
            }
            let text = l.get("LineText").str();
            if !text.trim().is_empty() && x1 > x0 && y1 > y0 {
                lines.push(Line { text: text.trim().to_string(), x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
            }
        }
    }
    Ok(group(lines, width as f64, height as f64))
}

fn cjk(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x9FFF | 0xAC00..=0xD7AF | 0xFF00..=0xFFEF))
}

/// Joins the lines that belong to one speech bubble (they touch once each is grown a little) and puts them in reading order.
fn group(lines: Vec<Line>, pw: f64, ph: f64) -> Vec<Block> {
    let n = lines.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut Vec<usize>, i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            r = p[r];
        }
        let mut c = i;
        while p[c] != r {
            let next = p[c];
            p[c] = r;
            c = next;
        }
        r
    }
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (&lines[i], &lines[j]);
            let m = 0.8 * a.w.min(a.h).max(b.w.min(b.h)).max(1.0);
            let apart_x = a.x > b.x + b.w + m || b.x > a.x + a.w + m;
            let apart_y = a.y > b.y + b.h + m || b.y > a.y + a.h + m;
            if !apart_x && !apart_y {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        match roots.iter().position(|x| *x == r) {
            Some(k) => groups[k].push(i),
            None => {
                roots.push(r);
                groups.push(vec![i]);
            }
        }
    }
    let mut out: Vec<Block> = Vec::new();
    for g in groups {
        let mut ls: Vec<&Line> = g.iter().map(|i| &lines[*i]).collect();
        let vertical = ls.iter().filter(|l| l.h > l.w * 1.5).count() * 2 > ls.len();
        if vertical {
            // Columns are read from the right to the left.
            ls.sort_by(|a, b| (b.x + b.w / 2.0).partial_cmp(&(a.x + a.w / 2.0)).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            ls.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal).then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)));
        }
        let joined: Vec<&str> = ls.iter().map(|l| l.text.as_str()).collect();
        let text = if cjk(&joined.join("")) { joined.join("") } else { joined.join(" ") };
        let x0 = ls.iter().map(|l| l.x).fold(f64::MAX, f64::min);
        let y0 = ls.iter().map(|l| l.y).fold(f64::MAX, f64::min);
        let x1 = ls.iter().map(|l| l.x + l.w).fold(0.0, f64::max);
        let y1 = ls.iter().map(|l| l.y + l.h).fold(0.0, f64::max);
        out.push(Block { text, x: x0 / pw, y: y0 / ph, w: (x1 - x0) / pw, h: (y1 - y0) / ph });
    }
    sort_blocks(&mut out);
    out
}

/// Reading order of the page: top to bottom, and right to left within a row (manga).
fn sort_blocks(v: &mut [Block]) {
    v.sort_by(|a, b| {
        let same_row = (a.y - b.y).abs() < 0.08;
        if same_row {
            (b.x + b.w).partial_cmp(&(a.x + a.w)).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal)
        }
    });
}

pub fn parse_vision(j: &Json) -> Result<Vec<Block>, String> {
    let r = j.get("responses").at(0);
    let err = r.path("error.message").str();
    if !err.is_empty() {
        return Err(format!("Google Cloud Vision: {}", err));
    }
    let page = r.path("fullTextAnnotation.pages").at(0);
    let (pw, ph) = (page.get("width").as_f64().unwrap_or(0.0), page.get("height").as_f64().unwrap_or(0.0));
    if pw <= 0.0 || ph <= 0.0 {
        return Ok(Vec::new());
    }
    let mut out: Vec<Block> = Vec::new();
    for b in page.get("blocks").items() {
        let mut text = String::new();
        for p in b.get("paragraphs").items() {
            for w in p.get("words").items() {
                for s in w.get("symbols").items() {
                    text.push_str(&s.get("text").str());
                    match s.path("property.detectedBreak.type").str().as_str() {
                        "SPACE" | "SURE_SPACE" => text.push(' '),
                        "EOL_SURE_SPACE" | "LINE_BREAK" => text.push('\n'),
                        _ => {}
                    }
                }
            }
            text.push('\n');
        }
        let text = tidy(&text);
        let vs = b.path("boundingBox.vertices").items();
        if text.is_empty() || vs.is_empty() {
            continue;
        }
        let xs: Vec<f64> = vs.iter().map(|v| v.get("x").as_f64().unwrap_or(0.0)).collect();
        let ys: Vec<f64> = vs.iter().map(|v| v.get("y").as_f64().unwrap_or(0.0)).collect();
        let (x0, x1) = (xs.iter().cloned().fold(f64::MAX, f64::min), xs.iter().cloned().fold(0.0, f64::max));
        let (y0, y1) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(0.0, f64::max));
        out.push(Block { text, x: x0 / pw, y: y0 / ph, w: (x1 - x0) / pw, h: (y1 - y0) / ph });
    }
    sort_blocks(&mut out);
    Ok(out)
}

/// Line breaks inside a bubble are not breaks of the sentence: they become a space (nothing at all between Japanese characters).
fn tidy(s: &str) -> String {
    let cj = cjk(s);
    let mut out = String::new();
    for l in s.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        if !out.is_empty() && !cj {
            out.push(' ');
        }
        out.push_str(l);
    }
    out
}

/// The block under a point of the page (both from 0 to 1), the nearest one when the point is just outside.
pub fn hit(blocks: &[Block], x: f64, y: f64) -> Option<usize> {
    let pad = 0.012;
    let mut best: Option<(usize, f64)> = None;
    for (i, b) in blocks.iter().enumerate() {
        let (dx, dy) = ((b.x - x).max(x - (b.x + b.w)).max(0.0), (b.y - y).max(y - (b.y + b.h)).max(0.0));
        let d = (dx * dx + dy * dy).sqrt();
        if d <= pad && best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn tiles_are_put_back_in_page_coordinates() {
        let line = format!("{{\"LineText\":\"Hi\",\"Words\":[{}]}}", "{\"WordText\":\"Hi\",\"Left\":100,\"Top\":800,\"Width\":100,\"Height\":50}");
        let body = format!("{{\"ParsedResults\":[{{\"TextOverlay\":{{\"Lines\":[{}]}}}}],\"IsErroredOnProcessing\":false}}", line);
        let http = Fake::new().route("api.ocr.space", 200, &body);
        let cfg = Cfg { provider: "ocrspace".into(), key: String::new(), lang: "eng".into() };
        let tiles = vec![Tile { jpeg: vec![1], w: 1000, h: 1000, y0: 0.0, y1: 0.55 }, Tile { jpeg: vec![1], w: 1000, h: 1000, y0: 0.45, y1: 1.0 }];
        let v = recognize_tiles(&http, &cfg, &tiles).unwrap();
        assert_eq!(v.len(), 2);
        assert!((v[0].y - 0.44).abs() < 1e-9 && (v[1].y - 0.89).abs() < 1e-9);
        // The same block seen in the overlap of two slices is kept once.
        let both = vec![Tile { jpeg: vec![1], w: 1000, h: 1000, y0: 0.0, y1: 1.0 }, Tile { jpeg: vec![1], w: 1000, h: 1000, y0: 0.0, y1: 1.0 }];
        assert_eq!(recognize_tiles(&http, &cfg, &both).unwrap().len(), 1);
    }

    fn word(t: &str, l: f64, tp: f64, w: f64, h: f64) -> String {
        format!("{{\"WordText\":\"{}\",\"Left\":{},\"Top\":{},\"Width\":{},\"Height\":{}}}", t, l, tp, w, h)
    }

    #[test]
    fn two_columns_of_one_bubble_and_another_bubble() {
        // A vertical bubble of two columns (read right to left) and a horizontal line far away.
        let c1 = format!("{{\"LineText\":\"あい\",\"Words\":[{}]}}", word("あい", 140.0, 100.0, 30.0, 120.0));
        let c2 = format!("{{\"LineText\":\"うえ\",\"Words\":[{}]}}", word("うえ", 100.0, 100.0, 30.0, 120.0));
        let far = format!("{{\"LineText\":\"Hello\",\"Words\":[{}]}}", word("Hello", 400.0, 700.0, 100.0, 30.0));
        let body = format!("{{\"ParsedResults\":[{{\"TextOverlay\":{{\"Lines\":[{},{},{}]}}}}],\"IsErroredOnProcessing\":false}}", c2, c1, far);
        let b = parse_ocrspace(&Json::parse(&body).unwrap(), 1000, 1000).unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].text, "あいうえ");
        assert!((b[0].x - 0.1).abs() < 1e-9 && (b[0].w - 0.07).abs() < 1e-9);
        assert_eq!(b[1].text, "Hello");
    }

    #[test]
    fn service_errors_are_reported() {
        let j = Json::parse("{\"IsErroredOnProcessing\":true,\"ErrorMessage\":[\"File size exceeds the maximum\"]}").unwrap();
        assert!(parse_ocrspace(&j, 10, 10).unwrap_err().contains("File size"));
    }

    #[test]
    fn vision_blocks() {
        let sym = |t: &str, br: &str| format!("{{\"text\":\"{}\",\"property\":{{\"detectedBreak\":{{\"type\":\"{}\"}}}}}}", t, br);
        let body = format!(
            "{{\"responses\":[{{\"fullTextAnnotation\":{{\"pages\":[{{\"width\":200,\"height\":400,\"blocks\":[{{\"boundingBox\":{{\"vertices\":[{{\"x\":20,\"y\":40}},{{\"x\":100,\"y\":40}},{{\"x\":100,\"y\":80}},{{\"x\":20,\"y\":80}}]}},\"paragraphs\":[{{\"words\":[{{\"symbols\":[{},{}]}},{{\"symbols\":[{}]}}]}}]}}]}}]}}}}]}}",
            sym("す", ""), sym("み", "LINE_BREAK"), sym("ま", "")
        );
        let b = parse_vision(&Json::parse(&body).unwrap()).unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, "すみま");
        assert!((b[0].x - 0.1).abs() < 1e-9 && (b[0].w - 0.4).abs() < 1e-9);
        assert!((b[0].y - 0.1).abs() < 1e-9 && (b[0].h - 0.1).abs() < 1e-9);
    }

    #[test]
    fn finds_the_block_under_the_pointer() {
        let v = vec![Block { text: "a".into(), x: 0.1, y: 0.1, w: 0.2, h: 0.2 }, Block { text: "b".into(), x: 0.6, y: 0.6, w: 0.2, h: 0.2 }];
        assert_eq!(hit(&v, 0.15, 0.15), Some(0));
        assert_eq!(hit(&v, 0.7, 0.7), Some(1));
        assert_eq!(hit(&v, 0.45, 0.45), None);
        assert_eq!(hit(&v, 0.305, 0.2), Some(0));
    }

    #[test]
    fn asks_the_service() {
        let http = Fake::new().route("POST https://api.ocr.space/parse/image", 200, "{\"ParsedResults\":[],\"IsErroredOnProcessing\":false}");
        let cfg = Cfg { provider: "ocrspace".into(), key: String::new(), lang: "jpn".into() };
        assert_eq!(recognize(&http, &cfg, b"abc", 10, 10).unwrap(), vec![]);
        let sent = http.requests();
        let body = String::from_utf8_lossy(&sent[0].body.clone().unwrap().1).to_string();
        assert!(body.contains("apikey=helloworld") && body.contains("language=jpn") && body.contains("YWJj"));
        let v = Cfg { provider: "vision".into(), key: String::new(), lang: "jpn".into() };
        assert!(recognize(&http, &v, b"abc", 10, 10).unwrap_err().contains("API key"));
    }
}
