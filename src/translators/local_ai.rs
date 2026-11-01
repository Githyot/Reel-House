//! The local AI (Ollama on this computer, the model of `llm::OLLAMA_MODEL`) as a translator. Part of `translate`.
//!
//! A 3B model on a CPU is slow and loses its way with long inputs, so the texts go in small numbered batches
//! ("1. text"), one request at a time in the whole app, and the reply is read back by its numbers. A batch whose
//! reply does not line up comes back shorter, and `translate_range` splits it until it does. The controller gets
//! the local AI ready (and waits for it) before the first call: see `controller_translators::local_ai_gate`.
use super::Translator;
use crate::http::Http;
use crate::llm;
use std::sync::Mutex;

/// The most texts and characters sent in one request.
pub const LOCAL_MAX_ITEMS: usize = 10;
pub const LOCAL_MAX_CHARS: usize = 900;

/// One request at a time to the local model, from every translation of the app.
static LOCAL_TURN: Mutex<()> = Mutex::new(());

pub struct LocalAi<'a> {
    pub http: &'a dyn Http,
    /// The Ollama model ("qwen2.5:3b-instruct").
    pub model: String,
}

impl<'a> LocalAi<'a> {
    pub fn new(http: &'a dyn Http, model: &str) -> LocalAi<'a> {
        let model = if model.trim().is_empty() { llm::OLLAMA_MODEL } else { model.trim() };
        LocalAi { http, model: model.to_string() }
    }

    fn cfg(&self) -> llm::Cfg {
        llm::Cfg { provider: "ollama".into(), key: String::new(), model: self.model.clone(), base: llm::OLLAMA_BASE.into() }
    }

    /// One batch (no empty texts): Some(translations) when the reply lines up, else what could be read.
    fn batch(&self, texts: &[String], source: &str, target: &str, hint: &str) -> Result<Vec<String>, String> {
        let system = local_prompt(source, target, hint, texts.len());
        let user = numbered(texts);
        let reply = {
            let _turn = LOCAL_TURN.lock().unwrap_or_else(|e| e.into_inner());
            llm::chat(self.http, &self.cfg(), &system, &[llm::Turn::new("user", &user)], false)?
        };
        Ok(parse_numbered(&reply, texts.len()))
    }
}

impl Translator for LocalAi<'_> {
    fn name(&self) -> String {
        format!("Local AI ({})", self.model)
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        self.translate_hinted(texts, source, target, "")
    }

    fn translate_hinted(&self, texts: &[String], source: &str, target: &str, hint: &str) -> Result<Vec<String>, String> {
        // the empty texts stay as they are and are not sent
        let idx: Vec<usize> = (0..texts.len()).filter(|&i| !texts[i].trim().is_empty()).collect();
        let mut out: Vec<String> = texts.to_vec();
        let mut start = 0;
        while start < idx.len() {
            let mut end = start;
            let mut chars = 0;
            while end < idx.len() && end - start < LOCAL_MAX_ITEMS && (end == start || chars + texts[idx[end]].chars().count() <= LOCAL_MAX_CHARS) {
                chars += texts[idx[end]].chars().count();
                end += 1;
            }
            let part: Vec<String> = idx[start..end].iter().map(|&i| texts[i].clone()).collect();
            let got = self.batch(&part, source, target, hint)?;
            if got.len() != part.len() {
                // shorter than asked: the caller splits the batch and asks again
                return Ok(got);
            }
            for (k, t) in got.into_iter().enumerate() {
                out[idx[start + k]] = t;
            }
            start = end;
        }
        Ok(out)
    }
}

/// The instruction: short and strict, because a small model follows few rules.
pub fn local_prompt(source: &str, target: &str, hint: &str, n: usize) -> String {
    let to = crate::llm::lang_english(target);
    let from = if source.is_empty() || source == "auto" { String::new() } else { format!(" from {}", crate::llm::lang_english(source)) };
    let what = match hint {
        "manga" => "speech bubbles of one manga page, in reading order (keep them short; names and honorifics stay)",
        "novel" => "paragraphs of a novel (fluent prose; names stay the same)",
        _ => "subtitle lines of a video (keep tags such as <i>, music symbols and speaker dashes)",
    };
    format!(
        "You are a translator. Translate the numbered {what}{from} into {to}.\n\
         Rules: reply with exactly {n} numbered lines, \"1. \" to \"{n}. \", one for each input line, in the same order. \
         Each line holds only the translation of the line with the same number. Keep \"<br>\" where it appears. \
         Never merge, skip or explain lines. No notes, no quotes around the lines, nothing else."
    )
}

/// "1. text" per entry; a line break inside an entry becomes "<br>" so each entry stays one line.
pub fn numbered(texts: &[String]) -> String {
    texts.iter().enumerate().map(|(i, t)| format!("{}. {}", i + 1, t.trim().replace("\r\n", "\n").replace('\n', "<br>"))).collect::<Vec<_>>().join("\n")
}

/// Reads "N. text" lines back (also "N)" and "N:"); lines without a number continue the one before. Returns the
/// `n` translations in order, or fewer when a number is missing (the reply did not line up).
pub fn parse_numbered(reply: &str, n: usize) -> Vec<String> {
    let mut got: Vec<Option<String>> = vec![None; n];
    let mut last: Option<usize> = None;
    for line in reply.lines() {
        let l = line.trim().trim_start_matches(['*', '#', '-']).trim();
        if l.is_empty() || l.starts_with("```") {
            continue;
        }
        let digits: String = l.chars().take_while(|c| c.is_ascii_digit()).collect();
        let rest = &l[digits.len()..];
        let numbered = !digits.is_empty() && (rest.starts_with('.') || rest.starts_with(')') || rest.starts_with(':'));
        if numbered {
            let k: usize = digits.parse().unwrap_or(0);
            if k >= 1 && k <= n && got[k - 1].is_none() {
                got[k - 1] = Some(rest[1..].trim().trim_start_matches('*').trim().to_string());
                last = Some(k - 1);
                continue;
            }
            last = None;
        } else if let Some(i) = last {
            if let Some(t) = got[i].as_mut() {
                t.push(' ');
                t.push_str(l);
            }
        }
    }
    let fix = |t: String| t.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n");
    if got.iter().all(|g| g.is_some()) {
        got.into_iter().map(|g| fix(g.unwrap_or_default())).collect()
    } else {
        got.into_iter().take_while(|g| g.is_some()).map(|g| fix(g.unwrap_or_default())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn numbered_lines_go_and_come_back_in_order() {
        let texts = vec!["Hello".to_string(), "Two\nlines".to_string()];
        assert_eq!(numbered(&texts), "1. Hello\n2. Two<br>lines");
        let back = parse_numbered("Here you go:\n1. Olá\n2) Duas<br>linhas\n", 2);
        assert_eq!(back, vec!["Olá".to_string(), "Duas\nlinhas".to_string()]);
        // a missing number: the reply is shorter, so the batch is split by the caller
        assert_eq!(parse_numbered("1. a\n3. c", 3).len(), 1);
        // a wrapped line continues the one before
        assert_eq!(parse_numbered("1. a long\nsentence\n2. b", 2)[0], "a long sentence");
    }

    #[test]
    fn local_ai_translates_through_ollama() {
        let f = Fake::new().route("11434/v1/chat/completions", 200, r#"{"choices":[{"message":{"content":"1. Aonde você vai?\n2. Eu espero."}}]}"#);
        let tr = LocalAi::new(&f, "");
        let texts = vec!["Where are you going?".to_string(), String::new(), "I'll wait.".to_string()];
        let out = tr.translate(&texts, "en", "pt").unwrap();
        assert_eq!(out, vec!["Aonde você vai?".to_string(), String::new(), "Eu espero.".to_string()]);
        let body = f.requests()[0].body.as_ref().map(|(_, b)| String::from_utf8_lossy(b).to_string()).unwrap_or_default();
        assert!(body.contains(llm::OLLAMA_MODEL) && body.contains("Portuguese"));
    }
}
