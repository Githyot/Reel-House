//! The free services of the automatic chain, each on its own, so Preferences > Translators can pick one for a
//! place and compare them. Part of `translate` (they share its pauses, so a service that refused is left alone
//! by every translator of the app for a while).
use super::*;

/// Microsoft Edge's built-in translator: free, no key, many texts per request.
pub struct Edge<'a> {
    pub http: &'a dyn Http,
}

impl Translator for Edge<'_> {
    fn name(&self) -> String {
        "Microsoft Edge (free)".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        edge_translate(self.http, texts, source, target)
    }
}

/// Yandex's translator (the one of its Android app): several texts per request, slowed down to avoid its limit.
pub struct Yandex<'a> {
    pub http: &'a dyn Http,
}

impl Translator for Yandex<'_> {
    fn name(&self) -> String {
        "Yandex (free)".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        let mut out: Vec<String> = Vec::with_capacity(texts.len());
        let mut at = 0;
        while at < texts.len() {
            // up to 12 texts and 4500 characters per request, as the automatic chain does
            let mut end = at;
            let mut chars = 0;
            while end < texts.len() && end - at < 12 && (end == at || chars + texts[end].len() < 4500) {
                chars += texts[end].len();
                end += 1;
            }
            // it answers nothing for an empty text, so blanks travel as a space and come back as they were
            let chunk: Vec<String> = texts[at..end].iter().map(|t| if t.trim().is_empty() { " ".to_string() } else { t.clone() }).collect();
            if chunk.iter().all(|t| t.trim().is_empty()) {
                out.extend(texts[at..end].iter().cloned());
            } else {
                let got = yandex_batch(self.http, &chunk, source, target)?;
                out.extend(got.into_iter().zip(&texts[at..end]).map(|(t, o)| if o.trim().is_empty() { o.clone() } else { t }));
            }
            at = end;
        }
        Ok(out)
    }
}

/// Translates text by text with a service that takes one text per request. A text that fails keeps its original
/// (so subtitles stay in step); when nothing at all could be translated the first error is returned.
fn one_by_one(texts: &[String], f: &dyn Fn(&str) -> Result<String, String>) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(texts.len());
    let mut first_err: Option<String> = None;
    let (mut asked, mut got) = (0, 0);
    for t in texts {
        if t.trim().is_empty() {
            out.push(t.clone());
            continue;
        }
        asked += 1;
        match f(t) {
            Ok(x) if !x.is_empty() => {
                got += 1;
                out.push(x)
            }
            Ok(_) => out.push(t.clone()),
            Err(e) => {
                first_err.get_or_insert(e);
                out.push(t.clone());
                // nothing answered for the first texts: the rest would only wait for the same failures
                if got == 0 && asked >= 3 {
                    break;
                }
            }
        }
    }
    if asked > 0 && got == 0 {
        return Err(first_err.unwrap_or_else(|| "The web translation returned nothing.".to_string()));
    }
    out.extend(texts[out.len()..].iter().cloned());
    Ok(out)
}

fn source_or_auto(source: &str) -> &str {
    if source.is_empty() { "auto" } else { source }
}

/// Google Translate through its free widget endpoints (no key; it limits heavy use with 429).
pub struct GoogleWeb<'a> {
    pub http: &'a dyn Http,
}

impl Translator for GoogleWeb<'_> {
    fn name(&self) -> String {
        "Google (free web)".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        one_by_one(texts, &|t| google_text(self.http, t, source_or_auto(source), target))
    }
}

/// Lingva: free front ends of Google Translate run by volunteers.
pub struct Lingva<'a> {
    pub http: &'a dyn Http,
}

impl Translator for Lingva<'_> {
    fn name(&self) -> String {
        "Lingva (free)".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        one_by_one(texts, &|t| lingva_text(self.http, t, source_or_auto(source), target).map_err(|e| if e.is_empty() { "No Lingva server answered.".to_string() } else { e }))
    }
}

/// MyMemory: free, a daily quota of characters.
pub struct MyMemory<'a> {
    pub http: &'a dyn Http,
}

impl Translator for MyMemory<'_> {
    fn name(&self) -> String {
        "MyMemory (free)".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        one_by_one(texts, &|t| mymemory_text(self.http, t, source_or_auto(source), target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn google_web_line_by_line_keeps_blank_lines() {
        let f = Fake::new().route("translate_a/single", 200, r#"[[["Olá","Hello",null,null,1]],null,"en"]"#);
        let g = GoogleWeb { http: &f };
        assert_eq!(g.translate(&["Hello".into(), "".into(), "Hello".into()], "en", "pt").unwrap(), vec!["Olá", "", "Olá"]);
        assert_eq!(f.requests().len(), 2);
    }

    #[test]
    fn one_by_one_fails_only_when_nothing_was_translated() {
        let texts: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        assert!(one_by_one(&texts, &|_| Err("down".into())).unwrap_err().contains("down"));
        let half = one_by_one(&texts, &|t| if t == "b" { Err("x".into()) } else { Ok(t.to_uppercase()) }).unwrap();
        assert_eq!(half, vec!["A", "b", "C"]);
    }

    #[test]
    fn lingva_tries_the_next_host() {
        let f = Fake::new().route("lingva.ml", 500, "").route("lingva.thedaviddelta.com", 200, r#"{"translation":"Hola"}"#);
        assert_eq!(Lingva { http: &f }.translate(&["Hi".into()], "en", "es").unwrap(), vec!["Hola"]);
    }
}
