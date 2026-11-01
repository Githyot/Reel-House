//! A conversation with a language model, for the tutor of the Study mode (Conversation) and the trivia of the X-Ray:
//! Claude, Gemini, any OpenAI-compatible server (OpenAI, OpenRouter) or Ollama on this computer. The keys are the
//! ones of the translators (Preferences > Subtitles); Ollama needs none. Ollama (and Conversation) are desktop only:
//! on Android and iOS "ollama" means the automatic choice, and without any key `missing` asks for one.
use crate::http::{Http, Request};
use crate::json::Json;

/// Which model answers, and how to reach it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cfg {
    /// "claude", "gemini", "openai" or "ollama"; empty when no translator has a key on Android/iOS (see `missing`)
    pub provider: String,
    pub key: String,
    pub model: String,
    /// OpenAI-compatible servers only (empty: api.openai.com)
    pub base: String,
}

pub const OLLAMA_BASE: &str = "http://localhost:11434/v1";
/// The model of the local AI (about 1.9 GB), pulled by `ollama_local` when Conversation first needs it.
pub const OLLAMA_MODEL: &str = "qwen2.5:3b-instruct";

/// The model for the tutor: the provider chosen (`choice`: "auto", "claude", "gemini", "openai", "ollama"); "auto" takes
/// the first translator that has a key, then Ollama. `ollama_model` empty means `OLLAMA_MODEL`. On Android and iOS
/// (no local AI) "ollama" is "auto", and "auto" without any key gives an empty `Cfg` whose `missing` asks for one.
pub fn pick(choice: &str, s: &crate::settings::Subtitles, ollama_model: &str) -> Cfg {
    let claude = || Cfg { provider: "claude".into(), key: s.claude_key.trim().into(), model: s.claude_model.trim().into(), base: String::new() };
    let gemini = || Cfg { provider: "gemini".into(), key: s.gemini_key.trim().into(), model: s.gemini_model.trim().into(), base: String::new() };
    let openai = || Cfg { provider: "openai".into(), key: s.openai_key.trim().into(), model: s.openai_model.trim().into(), base: s.openai_base.trim().into() };
    let ollama = || Cfg { provider: "ollama".into(), key: String::new(), model: if ollama_model.trim().is_empty() { OLLAMA_MODEL.into() } else { ollama_model.trim().into() }, base: OLLAMA_BASE.into() };
    match choice {
        "claude" => claude(),
        "gemini" => gemini(),
        "openai" => openai(),
        "ollama" if crate::platform::DESKTOP_AI => ollama(),
        _ => {
            if !s.claude_key.trim().is_empty() {
                claude()
            } else if !s.gemini_key.trim().is_empty() {
                gemini()
            } else if !s.openai_key.trim().is_empty() {
                openai()
            } else if crate::platform::DESKTOP_AI {
                ollama()
            } else {
                Cfg::default()
            }
        }
    }
}

impl Cfg {
    /// The name shown in the interface ("Claude", "Ollama (qwen2.5:3b-instruct)").
    pub fn label(&self) -> String {
        match self.provider.as_str() {
            "claude" => "Claude".into(),
            "gemini" => "Gemini".into(),
            "openai" => "GPT (OpenAI)".into(),
            "ollama" => format!("Ollama ({})", self.model),
            _ => "a language model".into(),
        }
    }

    /// Why this model cannot answer (no key), or None.
    pub fn missing(&self) -> Option<String> {
        if self.provider.is_empty() {
            return Some("Add a key for Claude, Gemini or OpenAI in Preferences > Subtitles (translators).".into());
        }
        if self.provider != "ollama" && self.key.is_empty() {
            if !crate::platform::DESKTOP_AI {
                return Some(format!("Add a key for {} in Preferences > Subtitles (translators).", self.label()));
            }
            return Some(format!("Add a key for {} in Preferences > Subtitles (translators), or pick Ollama.", self.label()));
        }
        None
    }
}

/// One turn of the conversation: "user" or "assistant".
#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    pub role: String,
    pub text: String,
}

impl Turn {
    pub fn new(role: &str, text: &str) -> Turn {
        Turn { role: role.into(), text: text.into() }
    }
}

/// Asks the model: `system` is the instruction, `turns` the conversation (it must end with the user's turn).
/// `json` asks for a JSON object only (the providers that can enforce it do). Returns the text of the answer.
pub fn chat(http: &dyn Http, cfg: &Cfg, system: &str, turns: &[Turn], json: bool) -> Result<String, String> {
    if let Some(m) = cfg.missing() {
        return Err(m);
    }
    // a conversation must start with the user (Claude and Gemini refuse otherwise)
    let mut turns: Vec<Turn> = turns.to_vec();
    if turns.first().map(|t| t.role != "user").unwrap_or(true) {
        turns.insert(0, Turn::new("user", "(start)"));
    }
    match cfg.provider.as_str() {
        "claude" => {
            let model = if cfg.model.is_empty() { "claude-sonnet-5-5" } else { cfg.model.as_str() };
            let msgs: Vec<Json> = turns.iter().map(|t| Json::obj(vec![("role", t.role.as_str().into()), ("content", t.text.as_str().into())])).collect();
            let body = Json::obj(vec![("model", model.into()), ("max_tokens", 2000i64.into()), ("system", system.into()), ("messages", Json::Arr(msgs))]);
            let r = http.send(&Request::post("https://api.anthropic.com/v1/messages").header("x-api-key", &cfg.key).header("anthropic-version", "2023-06-01").json_body(body.dump()).timeout(120))?;
            if r.status == 401 {
                return Err("The Claude API key was rejected. Check it in Preferences > Subtitles.".into());
            }
            if !r.ok() {
                return Err(format!("Claude answered {}: {}", r.status, r.text().chars().take(160).collect::<String>()));
            }
            Ok(r.json()?.get("content").items().iter().map(|b| b.get("text").str()).collect())
        }
        "gemini" => {
            let model = if cfg.model.is_empty() { "gemini-2.5-flash" } else { cfg.model.as_str() };
            let contents: Vec<Json> = turns
                .iter()
                .map(|t| Json::obj(vec![("role", if t.role == "assistant" { "model" } else { "user" }.into()), ("parts", Json::Arr(vec![Json::obj(vec![("text", t.text.as_str().into())])]))]))
                .collect();
            let mut gen = vec![("temperature", 0.7f64.into())];
            if json {
                gen.push(("responseMimeType", "application/json".into()));
            }
            let body = Json::obj(vec![
                ("systemInstruction", Json::obj(vec![("parts", Json::Arr(vec![Json::obj(vec![("text", system.into())])]))])),
                ("contents", Json::Arr(contents)),
                ("generationConfig", Json::obj(gen)),
            ]);
            let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", model);
            let r = http.send(&Request::post(&url).header("x-goog-api-key", &cfg.key).json_body(body.dump()).timeout(120))?;
            if r.status == 401 || r.status == 403 || (r.status == 400 && r.text().contains("API key")) {
                return Err("The Gemini API key was rejected. Check it in Preferences > Subtitles.".into());
            }
            if !r.ok() {
                return Err(format!("Gemini answered {}: {}", r.status, r.text().chars().take(160).collect::<String>()));
            }
            Ok(r.json()?.path("candidates.0.content.parts").items().iter().map(|p| p.get("text").str()).collect())
        }
        _ => {
            let base = if cfg.base.is_empty() { "https://api.openai.com/v1" } else { cfg.base.as_str() };
            let model = if cfg.model.is_empty() { "gpt-4o-mini" } else { cfg.model.as_str() };
            let mut msgs = vec![Json::obj(vec![("role", "system".into()), ("content", system.into())])];
            msgs.extend(turns.iter().map(|t| Json::obj(vec![("role", t.role.as_str().into()), ("content", t.text.as_str().into())])));
            let mut body = vec![("model", model.into()), ("temperature", 0.7f64.into()), ("messages", Json::Arr(msgs))];
            if json {
                body.push(("response_format", Json::obj(vec![("type", "json_object".into())])));
            }
            let mut req = Request::post(&format!("{}/chat/completions", base.trim_end_matches('/'))).json_body(Json::obj(body).dump()).timeout(if cfg.provider == "ollama" { 600 } else { 180 });
            if !cfg.key.is_empty() {
                req = req.header("Authorization", &format!("Bearer {}", cfg.key));
            }
            let r = match http.send(&req) {
                Ok(r) => r,
                Err(e) if cfg.provider == "ollama" => return Err(format!("The local AI (Ollama) is not answering ({}). Use \"Download and turn on the local AI\" in Conversation, then try again.", e)),
                Err(e) => return Err(e),
            };
            if r.status == 401 {
                return Err("The OpenAI API key was rejected. Check it in Preferences > Subtitles.".into());
            }
            if r.status == 404 && cfg.provider == "ollama" {
                return Err(format!("Ollama does not have the model {} yet. Use \"Download and turn on the local AI\" in Conversation.", cfg.model));
            }
            if !r.ok() {
                return Err(format!("{} answered {}: {}", cfg.label(), r.status, r.text().chars().take(160).collect::<String>()));
            }
            Ok(r.json()?.path("choices.0.message.content").str())
        }
    }
}

/// Where a free Gemini key is made (Google AI Studio): it serves the translators, the X-Ray, the transcription of
/// the microphone and the spoken Conversation (Gemini Live).
pub const GEMINI_KEY_URL: &str = "https://aistudio.google.com/apikey";

/// The Gemini key of the translators (Preferences > Accounts > Gemini), whatever the tutor is.
pub fn gemini_key(s: &crate::settings::Subtitles) -> Option<String> {
    let k = s.gemini_key.trim();
    if k.is_empty() {
        None
    } else {
        Some(k.to_string())
    }
}

/// Gemini for text (reports and readings of the spoken Conversation), with the model of the translators.
pub fn gemini_text(s: &crate::settings::Subtitles) -> Option<Cfg> {
    gemini_key(s).map(|key| Cfg { provider: "gemini".into(), key, model: s.gemini_model.trim().into(), base: String::new() })
}

/// Who turns the learner's voice into text: Gemini or an OpenAI-compatible server with Whisper (Claude and Ollama do
/// not take audio). The tutor's own provider first when it can, then any translator key.
pub fn transcriber(tutor: &Cfg, s: &crate::settings::Subtitles) -> Option<Cfg> {
    if (tutor.provider == "gemini" || tutor.provider == "openai") && !tutor.key.is_empty() {
        return Some(tutor.clone());
    }
    if !s.gemini_key.trim().is_empty() {
        return Some(Cfg { provider: "gemini".into(), key: s.gemini_key.trim().into(), model: s.gemini_model.trim().into(), base: String::new() });
    }
    if !s.openai_key.trim().is_empty() {
        return Some(Cfg { provider: "openai".into(), key: s.openai_key.trim().into(), model: String::new(), base: s.openai_base.trim().into() });
    }
    None
}

/// The cloud model that may hear the learner for the setting `voice_input` ("auto", "gemini", "openai", "local"):
/// "auto" is `transcriber`, "gemini" / "openai" that provider when its key is set, "local" none (Whisper on this
/// computer only: see `voice::stt_plan`, which adds the local engine and the fallbacks).
pub fn cloud_transcriber(choice: &str, tutor: &Cfg, s: &crate::settings::Subtitles) -> Option<Cfg> {
    match choice {
        "local" => None,
        "gemini" if !s.gemini_key.trim().is_empty() => Some(Cfg { provider: "gemini".into(), key: s.gemini_key.trim().into(), model: s.gemini_model.trim().into(), base: String::new() }),
        "openai" if !s.openai_key.trim().is_empty() => Some(Cfg { provider: "openai".into(), key: s.openai_key.trim().into(), model: String::new(), base: s.openai_base.trim().into() }),
        "gemini" | "openai" => None,
        _ => transcriber(tutor, s),
    }
}

/// What was said in a recording (`wav`, the bytes of a WAV file) in the language `lang` (two letters). The learner's
/// mistakes are kept: the check of the message needs them.
pub fn transcribe(http: &dyn Http, cfg: &Cfg, wav: &[u8], lang: &str) -> Result<String, String> {
    let name = lang_english(lang);
    if cfg.provider == "gemini" {
        let model = if cfg.model.is_empty() { "gemini-2.5-flash" } else { cfg.model.as_str() };
        let prompt = format!(
            "Transcribe this recording exactly as spoken. The speaker is learning {} and usually speaks it, but if they speak another language, \
             write what they said in that language. Keep their mistakes, do not correct or translate. The voice may be quiet or far from the \
             microphone: write every word you can make out. Write it in the normal script of the language spoken. Answer with the transcription \
             only, without quotes or notes; only when there is no voice at all, answer with nothing.",
            name
        );
        let body = Json::obj(vec![
            (
                "contents",
                Json::Arr(vec![Json::obj(vec![
                    ("role", "user".into()),
                    ("parts", Json::Arr(vec![Json::obj(vec![("inline_data", Json::obj(vec![("mime_type", "audio/wav".into()), ("data", crate::suwayomi::base64(wav).into())]))]), Json::obj(vec![("text", prompt.into())])])),
                ])]),
            ),
            ("generationConfig", Json::obj(vec![("temperature", 0.0f64.into())])),
        ]);
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", model);
        let r = http.send(&Request::post(&url).header("x-goog-api-key", &cfg.key).json_body(body.dump()).timeout(120))?;
        if !r.ok() {
            return Err(format!("Gemini could not transcribe ({}): {}", r.status, r.text().chars().take(160).collect::<String>()));
        }
        let t: String = r.json()?.path("candidates.0.content.parts").items().iter().map(|p| p.get("text").str()).collect();
        return Ok(t.trim().to_string());
    }
    // OpenAI's /audio/transcriptions (Whisper), as multipart form data
    let base = if cfg.base.is_empty() { "https://api.openai.com/v1" } else { cfg.base.as_str() };
    let mut req = Request::post(&format!("{}/audio/transcriptions", base.trim_end_matches('/'))).header("Authorization", &format!("Bearer {}", cfg.key)).timeout(120);
    req.body = Some(crate::voice::multipart(&[("model", "whisper-1"), ("language", lang), ("response_format", "json")], "file", "voice.wav", "audio/wav", wav));
    let r = http.send(&req)?;
    if !r.ok() {
        return Err(format!("Whisper could not transcribe ({}): {}", r.status, r.text().chars().take(160).collect::<String>()));
    }
    Ok(r.json()?.get("text").str().trim().to_string())
}

/// The English name of a language, for the instructions.
pub fn lang_english(code: &str) -> String {
    match crate::util::lang_base(code).as_str() {
        "ja" => "Japanese".into(),
        "en" => "English".into(),
        "pt" => "Portuguese".into(),
        "es" => "Spanish".into(),
        "fr" => "French".into(),
        "de" => "German".into(),
        "it" => "Italian".into(),
        "ko" => "Korean".into(),
        "zh" => "Chinese".into(),
        "ru" => "Russian".into(),
        other => crate::util::lang_name(other),
    }
}

/// The JSON object in a model's answer, even when it is wrapped in prose or a code fence.
pub fn json_of(text: &str) -> Option<Json> {
    let (a, b) = (text.find('{')?, text.rfind('}')?);
    if b <= a {
        return None;
    }
    Json::parse(&text[a..=b]).ok().filter(|j| matches!(j, Json::Obj(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn auto_picks_the_first_key() {
        let mut s = crate::settings::Settings::default().subtitles;
        s.openai_key = "o".into();
        assert_eq!(pick("auto", &s, "").provider, "openai");
        s.gemini_key = "g".into();
        assert_eq!(pick("auto", &s, "").provider, "gemini");
        s.claude_key = "c".into();
        assert_eq!(pick("auto", &s, "").provider, "claude");
        s.openai_key.clear();
        assert!(pick("openai", &s, "").missing().unwrap().contains("GPT (OpenAI)"));
        assert!(Cfg::default().missing().unwrap().contains("Claude, Gemini or OpenAI"));
        let http = Fake::new();
        assert!(chat(&http, &Cfg::default(), "sys", &[Turn::new("user", "hi")], false).is_err());
    }

    /// Android and iOS: no local AI, so "ollama" is the automatic choice and no key means asking for one.
    #[test]
    #[cfg(any(target_os = "android", target_os = "ios"))]
    fn without_the_local_ai_auto_asks_for_a_key() {
        let s = crate::settings::Settings::default().subtitles;
        assert!(pick("auto", &s, "").provider.is_empty());
        assert!(pick("ollama", &s, "").missing().is_some());
    }

    #[test]
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn auto_picks_a_key_then_ollama() {
        let mut s = crate::settings::Settings::default().subtitles;
        assert_eq!(pick("auto", &s, "").provider, "ollama");
        assert_eq!(pick("auto", &s, "").model, OLLAMA_MODEL);
        s.gemini_key = "g".into();
        assert_eq!(pick("auto", &s, "").provider, "gemini");
        s.claude_key = "c".into();
        assert_eq!(pick("auto", &s, "").provider, "claude");
        assert_eq!(pick("ollama", &s, "llama3.2").model, "llama3.2");
        assert!(pick("openai", &s, "").missing().is_some());
    }

    #[test]
    fn voice_goes_to_gemini_or_whisper() {
        let mut s = crate::settings::Settings::default().subtitles;
        let tutor = pick("claude", &s, "");
        assert!(transcriber(&tutor, &s).is_none());
        s.openai_key = "o".into();
        let w = transcriber(&tutor, &s).unwrap();
        assert_eq!(w.provider, "openai");
        let http = Fake::new().route("api.openai.com/v1/audio/transcriptions", 200, r#"{"text":" こんにちは "}"#);
        assert_eq!(transcribe(&http, &w, b"RIFF....", "ja").unwrap(), "こんにちは");
        s.gemini_key = "g".into();
        assert_eq!(transcriber(&tutor, &s).unwrap().provider, "gemini");
        // the local AI as tutor: the microphone still works through the Gemini key
        let local = Cfg { provider: "ollama".into(), key: String::new(), model: OLLAMA_MODEL.into(), base: OLLAMA_BASE.into() };
        assert_eq!(transcriber(&local, &s).unwrap().provider, "gemini");
        assert_eq!(gemini_key(&s).as_deref(), Some("g"));
        assert_eq!(gemini_text(&s).unwrap().provider, "gemini");
        let g = Fake::new().route("generativelanguage.googleapis.com", 200, r#"{"candidates":[{"content":{"parts":[{"text":"ola tudo bem"}]}}]}"#);
        assert_eq!(transcribe(&g, &transcriber(&tutor, &s).unwrap(), b"RIFF", "pt").unwrap(), "ola tudo bem");
        // the OpenAI request carries the language and the file
        let body = http.requests()[0].body.clone().unwrap();
        assert!(body.0.starts_with("multipart/form-data"));
        assert!(String::from_utf8_lossy(&body.1).contains("name=\"language\"\r\n\r\nja\r\n"));
    }

    #[test]
    fn the_voice_input_choice_picks_the_cloud_model() {
        let mut s = crate::settings::Settings::default().subtitles;
        let tutor = pick("claude", &s, "");
        assert!(cloud_transcriber("auto", &tutor, &s).is_none());
        assert!(cloud_transcriber("gemini", &tutor, &s).is_none(), "no Gemini key");
        s.gemini_key = "g".into();
        s.openai_key = "o".into();
        assert_eq!(cloud_transcriber("auto", &tutor, &s).unwrap().provider, "gemini");
        assert_eq!(cloud_transcriber("openai", &tutor, &s).unwrap().provider, "openai");
        assert_eq!(cloud_transcriber("gemini", &tutor, &s).unwrap().key, "g");
        assert!(cloud_transcriber("local", &tutor, &s).is_none(), "this computer only");
    }

    #[test]
    fn chat_reads_each_provider() {
        let http = Fake::new()
            .route("POST https://api.anthropic.com/v1/messages", 200, r#"{"content":[{"type":"text","text":"こんにちは"}]}"#)
            .route("generativelanguage.googleapis.com", 200, r#"{"candidates":[{"content":{"parts":[{"text":"はい"}]}}]}"#)
            .route("api.openai.com/v1/chat/completions", 200, r#"{"choices":[{"message":{"content":"ok"}}]}"#)
            .route("localhost:11434/v1/chat/completions", 200, r#"{"choices":[{"message":{"content":"{\"reply\":\"はい\"}"}}]}"#);
        let c = Cfg { provider: "claude".into(), key: "k".into(), ..Cfg::default() };
        assert_eq!(chat(&http, &c, "sys", &[Turn::new("user", "hi")], false).unwrap(), "こんにちは");
        let g = Cfg { provider: "gemini".into(), key: "k".into(), ..Cfg::default() };
        assert_eq!(chat(&http, &g, "sys", &[Turn::new("assistant", "yo")], true).unwrap(), "はい");
        let oa = Cfg { provider: "openai".into(), key: "k".into(), ..Cfg::default() };
        assert_eq!(chat(&http, &oa, "sys", &[Turn::new("user", "hi")], true).unwrap(), "ok");
        assert!(http.requests().iter().any(|r| r.headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer k")));
        let o = Cfg { provider: "ollama".into(), key: String::new(), model: OLLAMA_MODEL.into(), base: OLLAMA_BASE.into() };
        let t = chat(&http, &o, "sys", &[Turn::new("user", "hi")], true).unwrap();
        assert_eq!(json_of(&t).unwrap().get("reply").str(), "はい");
        assert_eq!(json_of("Sure! ```json\n{\"a\":1}\n```").unwrap().get("a").as_i64(), Some(1));
        assert!(json_of("no json").is_none());
    }
}
