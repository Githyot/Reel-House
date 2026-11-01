//! Review texts come with a little markup (AniList: `img550(url)`, `#####(heading)`, `[text](url)`, `__bold__`).
//! This turns them into plain paragraphs and pictures the interface can show.

#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    Image(String),
}

fn is_image_url(u: &str) -> bool {
    let l = u.to_lowercase();
    let path = l.split(['?', '#']).next().unwrap_or("");
    l.starts_with("http") && (path.ends_with(".png") || path.ends_with(".jpg") || path.ends_with(".jpeg") || path.ends_with(".gif") || path.ends_with(".webp") || l.contains("i.imgur.com/"))
}

/// Finds `name(url)` forms such as `img550(...)`, `img(...)`, `Img220(...)`. Returns (start, end, url).
fn find_img_call(s: &str) -> Option<(usize, usize, String)> {
    let lower = s.to_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("img") {
        let start = from + rel;
        let mut i = start + 3;
        let b = lower.as_bytes();
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        // Not the end of another word ("aimg(").
        let word_start = start == 0 || !lower.as_bytes()[start - 1].is_ascii_alphanumeric();
        if word_start && i < b.len() && b[i] == b'(' {
            if let Some(close) = s[i + 1..].find(')') {
                let url = s[i + 1..i + 1 + close].trim().to_string();
                return Some((start, i + 1 + close + 1, url));
            }
        }
        from = start + 3;
    }
    None
}

/// `![alt](url)` and `[![alt](url)](link)`: returns (start, end, url) of the first image.
fn find_md_image(s: &str) -> Option<(usize, usize, String)> {
    let start = s.find("![")?;
    let mid = s[start..].find("](")? + start;
    let close = s[mid + 2..].find(')')? + mid + 2;
    let mut end = close + 1;
    // `[![a](img)](link)`: take the outer link too.
    if start > 0 && s.as_bytes()[start - 1] == b'[' && s[end..].starts_with("](") {
        if let Some(c2) = s[end + 2..].find(')') {
            end = end + 2 + c2 + 1;
            return Some((start - 1, end, s[mid + 2..close].trim().to_string()));
        }
    }
    Some((start, end, s[mid + 2..close].trim().to_string()))
}

/// Markdown links `[text](url)` become `text`.
fn strip_links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find('[') {
        let after = &rest[a + 1..];
        if let Some(b) = after.find("](") {
            if !after[..b].contains('\n') {
                if let Some(c) = after[b + 2..].find(')') {
                    out.push_str(&rest[..a]);
                    out.push_str(&after[..b]);
                    rest = &after[b + 2 + c + 1..];
                    continue;
                }
            }
        }
        out.push_str(&rest[..a + 1]);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// One line without markdown decorations.
fn clean_line(line: &str) -> String {
    let t = line.trim();
    // `#####(text)`: a heading marker with the text in brackets.
    let hashes = t.chars().take_while(|c| *c == '#').count();
    let mut t = if hashes > 0 { t[hashes..].trim_start() } else { t }.to_string();
    if hashes > 0 && t.starts_with('(') && t.ends_with(')') {
        t = t[1..t.len() - 1].to_string();
    }
    let mut t = strip_links(&t);
    for m in ["__", "**", "~~~", "~~", "~!", "!~"] {
        t = t.replace(m, "");
    }
    // Single `*word*` and `_word_` emphasis.
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if *c == '*' {
            continue;
        }
        if *c == '_' {
            let prev_alnum = i > 0 && chars[i - 1].is_alphanumeric();
            let next_alnum = i + 1 < chars.len() && chars[i + 1].is_alphanumeric();
            if !(prev_alnum && next_alnum) {
                continue;
            }
        }
        out.push(*c);
    }
    out
}

/// Splits a review into paragraphs of text and pictures, in reading order.
pub fn pieces(raw: &str) -> Vec<Piece> {
    // Pictures are found first, on the raw text (html tags would hide `<img src>`).
    let mut out: Vec<Piece> = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut Vec<Piece>| {
        let plain = crate::anilist::strip_html(text);
        let mut lines: Vec<String> = Vec::new();
        for l in plain.lines() {
            let c = clean_line(l);
            // Bare image addresses on their own line are pictures too.
            lines.push(c);
        }
        // Paragraphs: blank lines separate them.
        let mut para = String::new();
        for l in lines {
            if l.trim().is_empty() {
                if !para.trim().is_empty() {
                    out.push(Piece::Text(para.trim().to_string()));
                }
                para.clear();
            } else {
                if !para.is_empty() {
                    para.push(' ');
                }
                para.push_str(l.trim());
            }
        }
        if !para.trim().is_empty() {
            out.push(Piece::Text(para.trim().to_string()));
        }
        text.clear();
    };
    let mut rest = raw.replace("\r\n", "\n");
    loop {
        let a = find_img_call(&rest);
        let b = find_md_image(&rest);
        let c = rest.find("<img").and_then(|st| {
            let end = rest[st..].find('>')? + st + 1;
            let tag = &rest[st..end];
            let src = tag.split("src=").nth(1)?;
            let q = src.chars().next()?;
            let url = src[1..].split(q).next()?.to_string();
            Some((st, end, url))
        });
        let first = [a, b, c].into_iter().flatten().min_by_key(|x| x.0);
        match first {
            Some((s, e, url)) => {
                text.push_str(&rest[..s]);
                flush(&mut text, &mut out);
                if is_image_url(&url) {
                    out.push(Piece::Image(url));
                }
                rest = rest[e..].to_string();
            }
            None => {
                text.push_str(&rest);
                flush(&mut text, &mut out);
                break;
            }
        }
    }
    // A line that is only an image address.
    out.into_iter()
        .map(|p| match p {
            Piece::Text(t) if is_image_url(t.trim()) && !t.trim().contains(' ') => Piece::Image(t.trim().to_string()),
            other => other,
        })
        .collect()
}

/// The text of a review without pictures (for the folded card).
pub fn plain(raw: &str) -> String {
    pieces(raw).into_iter().filter_map(|p| if let Piece::Text(t) = p { Some(t) } else { None }).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_and_text_in_order() {
        let raw = "Intro line.\n\nimg550(https://i.imgur.com/5DeBOxn.png)\n\n#####(The first episode commits a sin, in *many* ways.)\n\nSee [the site](https://x.y/z) now.\nimg(https://i.imgur.com/abc.jpg)";
        let p = pieces(raw);
        assert_eq!(
            p,
            vec![
                Piece::Text("Intro line.".into()),
                Piece::Image("https://i.imgur.com/5DeBOxn.png".into()),
                Piece::Text("The first episode commits a sin, in many ways.".into()),
                Piece::Text("See the site now.".into()),
                Piece::Image("https://i.imgur.com/abc.jpg".into()),
            ]
        );
    }

    #[test]
    fn markdown_and_html_pictures() {
        let p = pieces("A ![alt](https://a.b/c.png) B <img src=\"https://a.b/d.gif\"> C");
        assert_eq!(p.len(), 5);
        assert_eq!(p[1], Piece::Image("https://a.b/c.png".into()));
        assert_eq!(p[3], Piece::Image("https://a.b/d.gif".into()));
        assert_eq!(pieces("[![x](https://a.b/e.webp)](https://link)"), vec![Piece::Image("https://a.b/e.webp".into())]);
    }

    #[test]
    fn not_a_picture() {
        assert_eq!(pieces("a word(not an image) and aimg(x) here"), vec![Piece::Text("a word(not an image) and aimg(x) here".into())]);
        assert_eq!(pieces("img(javascript:alert)"), vec![]);
        assert_eq!(plain("one\n\nimg(https://i.imgur.com/a.png)\n\ntwo"), "one\n\ntwo");
    }

    #[test]
    fn emphasis_marks_go() {
        assert_eq!(pieces("__bold__ and **strong** and ~!spoiler!~ with snake_case"), vec![Piece::Text("bold and strong and spoiler with snake_case".into())]);
    }
}
