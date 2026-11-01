//! "Trivia" of the X-Ray for the episode playing: the question "Tell me trivia about <title> that happened in episode
//! <n>" asked, in the language of the interface, first to Google (its AI Overview, when the page has it in plain
//! HTML), then to the app's language model (Preferences > Conversation: Claude, Gemini, OpenAI or Ollama). Gemini
//! answers with Google Search grounding, so its facts come from Google too. The answer is kept on disk per title,
//! season, episode and language: the question is asked once.
use crate::http::{Http, Request};
use crate::json::Json;
use crate::llm;

/// What is asked: the title playing, its season and episode (None for a film) and the language of the interface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    pub title: String,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    /// two letters: "pt", "en", "es"...
    pub lang: String,
}

/// The facts found and who gave them ("Google (IA)", "Gemini + Google", "Claude"...).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trivia {
    pub items: Vec<String>,
    pub source: String,
    pub question: String,
}

impl Trivia {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("items", Json::Arr(self.items.iter().map(|s| Json::Str(s.clone())).collect())),
            ("source", Json::Str(self.source.clone())),
            ("question", Json::Str(self.question.clone())),
        ])
    }
    fn from_json(j: &Json) -> Option<Trivia> {
        let t = Trivia { items: j.get("items").strings(), source: j.get("source").str(), question: j.get("question").str() };
        (!t.source.is_empty()).then_some(t)
    }
}

/// The question, as the owner asked it, in the language of the interface. The season is named after the title when
/// it is past the first ("Frieren (temporada 2)"); a film (no episode) is asked about as a whole.
pub fn question(a: &Ask) -> String {
    let title = match a.season.filter(|s| *s > 1) {
        Some(s) => match a.lang.as_str() {
            "pt" => format!("{} (temporada {})", a.title, s),
            "es" => format!("{} (temporada {})", a.title, s),
            _ => format!("{} (season {})", a.title, s),
        },
        None => a.title.clone(),
    };
    match (a.lang.as_str(), a.episode.filter(|e| *e > 0)) {
        ("pt", Some(e)) => format!("Me conte curiosidades sobre {} que ocorreram no episódio {}", title, e),
        ("pt", None) => format!("Me conte curiosidades sobre {}", title),
        ("es", Some(e)) => format!("Cuéntame curiosidades sobre {} que ocurrieron en el episodio {}", title, e),
        ("es", None) => format!("Cuéntame curiosidades sobre {}", title),
        (_, Some(e)) => format!("Tell me trivia about {} that happened in episode {}", title, e),
        (_, None) => format!("Tell me trivia about {}", title),
    }
}

fn lang_name(l: &str) -> &'static str {
    match l {
        "pt" => "Brazilian Portuguese",
        "es" => "Spanish",
        "ja" => "Japanese",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        _ => "English",
    }
}

/// The answer of a model that does not know the episode.
const UNSURE: &str = "UNSURE";

fn system_prompt(lang: &str) -> String {
    format!(
        "You answer questions about films, series and anime for a viewer who is watching. Answer in {lang}. \
         Give 5 to 8 short trivia items (one or two sentences each) about exactly the episode asked: production facts, \
         differences from the source material, references, music, voice actors, reception, events of the episode. \
         Write one item per line, each starting with \"- \". No title, no introduction, no closing sentence, no markdown \
         other than the dashes. Do not invent anything: if you do not really know this specific episode, answer only \
         the word {UNSURE}.",
        lang = lang_name(lang)
    )
}

/// The items of an answer: its lines that start with a dash, a bullet or a number ("1." / "1)"), or, when it has
/// none, its non-empty lines. Markdown bold and the citation marks ("[1]") are taken out.
pub fn bullets(text: &str) -> Vec<String> {
    let clean = |s: &str| -> String {
        let mut s = s.replace("**", "").replace("__", "");
        // [1], [2, 3] citation marks of the grounded answers
        while let Some(a) = s.find('[') {
            let Some(b) = s[a..].find(']').map(|b| a + b) else { break };
            if s[a + 1..b].chars().all(|c| c.is_ascii_digit() || c == ',' || c == ' ') && b > a + 1 {
                s.replace_range(a..=b, "");
            } else {
                break;
            }
        }
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let mut marked = vec![];
    let mut plain = vec![];
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
        let rest = if let Some(r) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("• ")).or_else(|| line.strip_prefix("– ")) {
            Some(r)
        } else if digits > 0 && digits < 3 && (line[digits..].starts_with(". ") || line[digits..].starts_with(") ")) {
            Some(&line[digits + 2..])
        } else {
            None
        };
        match rest {
            Some(r) => marked.push(clean(r)),
            None => plain.push(clean(line)),
        }
    }
    let list = if marked.is_empty() { plain } else { marked };
    list.into_iter().filter(|s| s.chars().count() >= 12 && !s.eq_ignore_ascii_case(UNSURE)).take(10).collect()
}

/// The answer only says the model does not know the episode.
fn is_unsure(text: &str) -> bool {
    let t = text.trim().trim_matches(|c: char| c == '.' || c == '"' || c == '*');
    t.eq_ignore_ascii_case(UNSURE) || (t.len() < 40 && t.to_uppercase().contains(UNSURE))
}

// ---------------------------------------------------------------- Google's AI Overview

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// The headings Google puts above its AI Overview, in the languages of the interface.
const OVERVIEW_HEADS: [&str; 4] = ["Visão geral criada por IA", "AI Overview", "Vista creada con IA", "Resumen de IA"];
/// What closes it: the warning under it, or the link to learn more.
const OVERVIEW_ENDS: [&str; 8] = [
    "As respostas de IA podem",
    "As respostas da IA podem",
    "AI responses may include mistakes",
    "Las respuestas de la IA pueden",
    "Generative AI is experimental",
    "A IA generativa é experimental",
    "Saiba mais",
    "Learn more",
];

/// The text of a page, a line for each block (paragraph, list item, heading), without scripts and styles.
fn page_lines(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 4);
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let lower: String = rest.chars().take(10).collect::<String>().to_ascii_lowercase();
        // skip the inside of <script> and <style>
        let skip_to = if lower.starts_with("<script") { Some("</script>") } else if lower.starts_with("<style") { Some("</style>") } else { None };
        if let Some(end) = skip_to {
            rest = rest.to_ascii_lowercase().find(end).map(|j| &rest[j + end.len()..]).unwrap_or("");
            continue;
        }
        let Some(j) = rest.find('>') else { break };
        let tag = rest[1..j].trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_ascii_lowercase();
        if matches!(tag.as_str(), "div" | "p" | "li" | "br" | "h1" | "h2" | "h3" | "h4" | "ul" | "ol" | "tr") {
            out.push('\n');
        }
        if tag == "li" && !rest[1..].starts_with('/') {
            out.push_str("- ");
        }
        rest = &rest[j + 1..];
    }
    out.push_str(rest);
    let out = out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    out.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty() && l != "-").collect::<Vec<_>>().join("\n")
}

/// The items of the AI Overview of a Google results page, when the page has it as text (Google usually builds it
/// with JavaScript, or answers a page that asks for JavaScript or for consent: then None).
pub fn ai_overview(html: &str) -> Option<Vec<String>> {
    let text = page_lines(html);
    let (at, head) = OVERVIEW_HEADS.iter().filter_map(|h| text.find(h).map(|i| (i, *h))).min_by_key(|x| x.0)?;
    let body = &text[at + head.len()..];
    let end = OVERVIEW_ENDS.iter().filter_map(|e| body.find(e)).min().unwrap_or(body.len().min(4000));
    let body = &body[..end];
    let items = bullets(body);
    let total: usize = items.iter().map(|s| s.len()).sum();
    (items.len() >= 2 && total >= 120).then_some(items)
}

/// Asks Google and reads its AI Overview. Errors and pages without it are None (the model answers then).
pub fn ask_google(http: &dyn Http, q: &str, lang: &str) -> Option<Vec<String>> {
    let hl = match lang {
        "pt" => "pt-BR",
        "es" => "es",
        l if l.len() == 2 => l,
        _ => "en",
    };
    let url = format!("https://www.google.com/search?q={}&hl={}", crate::util::url_encode(q), hl);
    let req = Request::get(&url)
        .header("User-Agent", BROWSER_UA)
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Accept-Language", &format!("{},en;q=0.5", hl))
        .timeout(10);
    let r = http.send(&req).ok().filter(|r| r.ok())?;
    let found = ai_overview(&r.text());
    crate::debug::log("xray", &format!("trivia: Google answered {} bytes, AI Overview {}", r.body.len(), if found.is_some() { "found" } else { "absent" }));
    found
}

// ---------------------------------------------------------------- the language model

/// Gemini with Google Search grounding: the answer is built from Google's results.
fn gemini_grounded(http: &dyn Http, cfg: &llm::Cfg, system: &str, q: &str) -> Result<String, String> {
    let model = if cfg.model.is_empty() { "gemini-2.5-flash" } else { cfg.model.as_str() };
    let body = Json::obj(vec![
        ("systemInstruction", Json::obj(vec![("parts", Json::Arr(vec![Json::obj(vec![("text", system.into())])]))])),
        ("contents", Json::Arr(vec![Json::obj(vec![("role", "user".into()), ("parts", Json::Arr(vec![Json::obj(vec![("text", q.into())])]))])])),
        ("tools", Json::Arr(vec![Json::obj(vec![("google_search", Json::obj(vec![]))])])),
        ("generationConfig", Json::obj(vec![("temperature", 0.3f64.into())])),
    ]);
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", model);
    let r = http.send(&Request::post(&url).header("x-goog-api-key", &cfg.key).json_body(body.dump()).timeout(120))?;
    if !r.ok() {
        return Err(format!("Gemini answered {}: {}", r.status, r.text().chars().take(160).collect::<String>()));
    }
    Ok(r.json()?.path("candidates.0.content.parts").items().iter().map(|p| p.get("text").str()).collect())
}

/// Asks the language model; Gemini searches Google first. An answer that says it does not know is an error.
pub fn ask_model(http: &dyn Http, cfg: &llm::Cfg, q: &str, lang: &str) -> Result<Trivia, String> {
    if let Some(m) = cfg.missing() {
        return Err(m);
    }
    let system = system_prompt(lang);
    let mut source = cfg.label();
    let mut text = String::new();
    if cfg.provider == "gemini" {
        match gemini_grounded(http, cfg, &system, q) {
            Ok(t) if !t.trim().is_empty() => {
                text = t;
                source = "Gemini + Google".into();
            }
            Ok(_) => {}
            Err(e) => crate::debug::log("xray", &format!("trivia: grounded Gemini failed ({}), asking without search", e)),
        }
    }
    if text.trim().is_empty() {
        text = llm::chat(http, cfg, &system, &[llm::Turn::new("user", q)], false)?;
    }
    if is_unsure(&text) {
        // a small model on this computer rarely knows an episode: say what helps (the page translates these)
        if cfg.provider == "ollama" {
            return Err(LOCAL_UNSURE.into());
        }
        return Err(format!("{} does not know this episode well enough to tell trivia about it.", source));
    }
    let items = bullets(&text);
    if items.is_empty() {
        if cfg.provider == "ollama" {
            return Err(LOCAL_UNSURE.into());
        }
        return Err(format!("{} gave no trivia for this episode.", source));
    }
    Ok(Trivia { items, source, question: q.to_string() })
}

/// What the page shows when the AI on this computer does not know the episode.
pub const LOCAL_UNSURE: &str = "The AI on this computer does not know this episode. With a free Gemini key (Preferences > Addons & accounts), the trivia comes from Google Search.";

/// The models asked, in order: Gemini first whenever its key is set (it searches Google, so it knows recent
/// episodes), then the AI of Preferences > Conversation when it is another one.
pub fn models(tutor: &llm::Cfg, s: &crate::settings::Subtitles) -> Vec<llm::Cfg> {
    let mut out = vec![];
    if !s.gemini_key.trim().is_empty() {
        out.push(llm::Cfg { provider: "gemini".into(), key: s.gemini_key.trim().into(), model: s.gemini_model.trim().into(), base: String::new() });
    }
    if out.iter().all(|c| c.provider != tutor.provider) {
        out.push(tutor.clone());
    }
    out
}

// ---------------------------------------------------------------- the whole lookup, with the cache

fn cache_file(dir: &std::path::Path, a: &Ask) -> std::path::PathBuf {
    let key = format!("{}|{}|{}|{}", a.title.trim().to_lowercase(), a.season.unwrap_or(0), a.episode.unwrap_or(0), a.lang);
    dir.join(format!("{}.json", crate::util::fnv_hex(&key)))
}

/// The trivia of the episode: the cache (`cache_dir`), else Google's AI Overview, else the models in order (`models`;
/// the first error is told when none knows). Only answers with items are kept.
pub fn fetch(http: &dyn Http, cfgs: &[llm::Cfg], a: &Ask, cache_dir: Option<&std::path::Path>) -> Result<Trivia, String> {
    if a.title.trim().is_empty() {
        return Err("The title of this video is not known.".into());
    }
    if let Some(dir) = cache_dir {
        if let Some(t) = std::fs::read_to_string(cache_file(dir, a)).ok().and_then(|s| Json::parse(&s).ok()).and_then(|j| Trivia::from_json(&j)) {
            return Ok(t);
        }
    }
    let q = question(a);
    let t = match ask_google(http, &q, &a.lang) {
        Some(items) => Trivia { items, source: "Google (IA)".into(), question: q.clone() },
        None => {
            let mut first: Option<String> = None;
            let mut found = None;
            for cfg in cfgs {
                match ask_model(http, cfg, &q, &a.lang) {
                    Ok(t) => {
                        found = Some(t);
                        break;
                    }
                    Err(e) => {
                        crate::debug::log("xray", &format!("trivia: {} gave none ({})", cfg.label(), e));
                        // the local model's "does not know" says what helps, so it wins over a missing key
                        if first.is_none() || e == LOCAL_UNSURE {
                            first = Some(e);
                        }
                    }
                }
            }
            match found {
                Some(t) => t,
                None => return Err(first.unwrap_or_else(|| "No trivia was found for this episode.".into())),
            }
        }
    };
    if let Some(dir) = cache_dir {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(cache_file(dir, a), t.to_json().dump());
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    fn frieren() -> Ask {
        Ask { title: "Frieren".into(), season: Some(1), episode: Some(5), lang: "pt".into() }
    }

    #[test]
    fn the_question_in_the_language_of_the_interface() {
        assert_eq!(question(&frieren()), "Me conte curiosidades sobre Frieren que ocorreram no episódio 5");
        let s2 = Ask { season: Some(2), lang: "en".into(), ..frieren() };
        assert_eq!(question(&s2), "Tell me trivia about Frieren (season 2) that happened in episode 5");
        let film = Ask { title: "Akira".into(), lang: "es".into(), ..Ask::default() };
        assert_eq!(question(&film), "Cuéntame curiosidades sobre Akira");
    }

    #[test]
    fn bullets_of_an_answer() {
        let t = "Aqui estão:\n- **Primeiro** fato do episódio com detalhe [1].\n* Segundo fato do episódio, sobre a música [2, 3]\n1. Terceiro fato numerado aqui\n- curto";
        assert_eq!(bullets(t), vec!["Primeiro fato do episódio com detalhe .", "Segundo fato do episódio, sobre a música", "Terceiro fato numerado aqui"]);
        assert_eq!(bullets("Uma frase só, sem marcadores, mas longa."), vec!["Uma frase só, sem marcadores, mas longa."]);
        assert!(is_unsure("UNSURE."));
        assert!(!is_unsure("- UNSURE about the dub, but the episode aired in 2023 and drew praise."));
    }

    #[test]
    fn google_overview_read_from_html_when_present() {
        let html = r#"<html><head><style>.a{}</style><script>var x="AI Overview";</script></head><body>
            <div><h2>Visão geral criada por IA</h2><div>No episódio 5 de Frieren:</div><ul>
            <li>O episódio adapta os capítulos 12 e 13 do mangá, com cenas extras.</li>
            <li>A luta contra Qual foi animada por uma equipe convidada do estúdio Madhouse.</li>
            <li>A trilha &quot;Zoltraak&quot; de Evan Call estreia neste episódio.</li></ul>
            <div>As respostas de IA podem incluir erros.</div></div><div>outros resultados</div></body></html>"#;
        let items = ai_overview(html).unwrap();
        assert_eq!(items.len(), 3);
        assert!(items[2].contains("\"Zoltraak\""));
        // the page Google gives to scripts: no overview
        assert!(ai_overview(r#"<html><body><noscript>Ative o JavaScript</noscript><a href="/httpservice/retry/enablejs">clique aqui</a></body></html>"#).is_none());
    }

    #[test]
    fn falls_back_to_gemini_with_google_search_and_caches() {
        let dir = std::env::temp_dir().join(format!("rh-trivia-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let http = Fake::new()
            .route("www.google.com/search", 200, "<html><body>enablejs</body></html>")
            .route("generativelanguage.googleapis.com", 200, r#"{"candidates":[{"content":{"parts":[{"text":"- O episódio adapta os capítulos 12 e 13 do mangá.\n- A trilha de Evan Call ganhou destaque aqui."}]}}]}"#);
        let cfg = llm::Cfg { provider: "gemini".into(), key: "g".into(), ..llm::Cfg::default() };
        let t = fetch(&http, &[cfg.clone()], &frieren(), Some(&dir)).unwrap();
        assert_eq!(t.source, "Gemini + Google");
        assert_eq!(t.items.len(), 2);
        let sent = http.log.lock().unwrap();
        let gem = sent.iter().find(|r| r.url.contains("generativelanguage")).unwrap();
        let body = String::from_utf8_lossy(&gem.body.as_ref().unwrap().1).to_string();
        assert!(body.contains("google_search") && body.contains("Frieren"));
        drop(sent);
        // the second time comes from the disk, without asking anyone
        let none = Fake::new();
        assert_eq!(fetch(&none, &[cfg], &frieren(), Some(&dir)).unwrap(), t);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_model_that_does_not_know_says_so() {
        let http = Fake::new().route("api.anthropic.com", 200, r#"{"content":[{"type":"text","text":"UNSURE"}]}"#);
        let cfg = llm::Cfg { provider: "claude".into(), key: "k".into(), ..llm::Cfg::default() };
        let e = ask_model(&http, &cfg, "q", "pt").unwrap_err();
        assert!(e.contains("does not know"));
        let ok = Fake::new().route("api.anthropic.com", 200, r#"{"content":[{"type":"text","text":"- Fato um do episódio bem conhecido.\n- Fato dois do episódio bem conhecido."}]}"#);
        assert_eq!(ask_model(&ok, &cfg, "q", "pt").unwrap().source, "Claude");
    }

    #[test]
    fn gemini_is_asked_before_the_local_model() {
        let mut s = crate::settings::Settings::default().subtitles;
        let ollama = llm::Cfg { provider: "ollama".into(), model: "qwen2.5:3b-instruct".into(), base: "http://127.0.0.1:11434".into(), ..llm::Cfg::default() };
        assert_eq!(models(&ollama, &s), vec![ollama.clone()]);
        s.gemini_key = "g".into();
        let m = models(&ollama, &s);
        assert_eq!(m.iter().map(|c| c.provider.as_str()).collect::<Vec<_>>(), vec!["gemini", "ollama"]);
        // the tutor is Gemini already: asked once
        assert_eq!(models(&m[0], &s).len(), 1);
        // only the small local model, and it does not know: the page says what helps
        let ollama = llm::Cfg { base: llm::OLLAMA_BASE.into(), ..ollama };
        let http = Fake::new().route("www.google.com/search", 200, "<html></html>").route("/v1/chat/completions", 200, r#"{"choices":[{"message":{"content":"UNSURE"}}]}"#);
        let e = fetch(&http, &[ollama], &frieren(), None).unwrap_err();
        assert_eq!(e, LOCAL_UNSURE);
    }
}
