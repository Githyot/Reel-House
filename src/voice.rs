//! The voice of the Study mode (Conversation, and anything else that listens or speaks): who turns the learner's
//! voice into text, who reads a text aloud, and the small pieces both need. Decisions only, no I/O: the controller
//! (controller_voice.rs) runs them, `whisper_local` is the program on this computer.
//!
//! Hearing (speech to text), setting `voice_input`:
//! - "auto": the cloud model of a key (Gemini, then OpenAI's Whisper) when there is one, else Whisper on this
//!   computer. A cloud call that fails (no network, quota, a bad key) falls back to the local Whisper when it is
//!   already installed (a failed call never starts a large download by itself).
//! - "gemini" / "openai": that cloud model (the local Whisper still catches its failures when installed).
//! - "local": only Whisper on this computer (private, offline after the one-time download).
//!
//! Speaking (text to speech), setting `voice_output`: "auto" is Google's online voice and, when it cannot be reached,
//! a voice of the system (Qt TextToSpeech: SAPI on Windows, speech-dispatcher on Linux); "google" or "local" only.
use crate::json::Json;
use crate::llm::Cfg;

/// One way to turn the voice into text.
#[derive(Clone, Debug, PartialEq)]
pub enum Stt {
    /// Gemini or an OpenAI-compatible Whisper, through `llm::transcribe`
    Cloud(Cfg),
    /// whisper.cpp on this computer, with this model (`whisper_local::MODELS`)
    Local(String),
}

impl Stt {
    /// The name in the logs ("gemini", "openai-whisper", "local-whisper(large-v3-turbo)").
    pub fn label(&self) -> String {
        match self {
            Stt::Cloud(c) if c.provider == "gemini" => "gemini".into(),
            Stt::Cloud(_) => "openai-whisper".into(),
            Stt::Local(m) => format!("local-whisper({})", m),
        }
    }

    /// The name shown to the learner.
    pub fn pretty(&self) -> String {
        match self {
            Stt::Cloud(c) if c.provider == "gemini" => "Gemini".into(),
            Stt::Cloud(_) => "Whisper (OpenAI)".into(),
            Stt::Local(_) => "Whisper (this computer)".into(),
        }
    }
}

/// The engines to try, in order (an empty list: the voice cannot be heard, see `hear_missing`).
/// `cloud` is the cloud model the choice allows (`llm::cloud_transcriber`), `local_ok` whether this system can run
/// Whisper (desktop), `installed` whether it is already downloaded, `model` the local model for the language.
pub fn stt_plan(choice: &str, cloud: Option<Cfg>, local_ok: bool, installed: bool, model: &str) -> Vec<Stt> {
    let local = || Stt::Local(model.to_string());
    let mut out = vec![];
    match choice {
        "local" => {
            if local_ok {
                out.push(local());
            }
        }
        _ => {
            match cloud {
                Some(c) => {
                    out.push(Stt::Cloud(c));
                    // the fallback only when nothing has to be downloaded first
                    if local_ok && installed {
                        out.push(local());
                    }
                }
                // "auto" without a key: this computer; "gemini"/"openai" without their key: nothing
                None if choice != "gemini" && choice != "openai" && local_ok => out.push(local()),
                None => {}
            }
        }
    }
    out
}

/// Why the voice cannot be heard with this choice (shown instead of the microphone's error).
pub fn hear_missing(choice: &str, local_ok: bool) -> String {
    match choice {
        "gemini" => "Speaking with Gemini needs its key (Preferences > Subtitles). Or choose \"This computer\" for the voice input.".into(),
        "openai" => "Speaking with OpenAI needs its key (Preferences > Subtitles). Or choose \"This computer\" for the voice input.".into(),
        _ if !local_ok => "Speaking needs a Gemini or OpenAI key (Preferences > Subtitles).".into(),
        _ => String::new(),
    }
}

/// How to read a text aloud.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tts {
    /// Google's online voice (an MP3 per piece, played by the Study mode's sound player)
    Google,
    /// a voice of the system, spoken by the page (Qt TextToSpeech)
    Local,
}

pub fn tts_plan(choice: &str) -> Vec<Tts> {
    match choice {
        "google" => vec![Tts::Google],
        "local" => vec![Tts::Local],
        _ => vec![Tts::Google, Tts::Local],
    }
}

/// Whether the system has a voice for `lang`, from what the page reported (`{available, engine, voices: [{name,
/// locale}]}`): None while the page has not said (it will try), Some(false) when it has none.
pub fn has_system_voice(local: &Json, lang: &str) -> Option<bool> {
    let available = local.get("available").as_bool()?;
    if !available {
        return Some(false);
    }
    let base = crate::util::lang_base(lang);
    Some(local.get("voices").items().iter().any(|v| crate::util::lang_base(&v.get("locale").str()) == base))
}

/// A sentence to try a voice with.
pub fn sample_sentence(lang: &str) -> &'static str {
    match crate::util::lang_base(lang).as_str() {
        "ja" => "こんにちは。今日はいい天気ですね。",
        "zh" => "你好。今天天气很好。",
        "ko" => "안녕하세요. 오늘 날씨가 좋네요.",
        "pt" => "Olá! Hoje o dia está bonito.",
        "es" => "¡Hola! Hoy hace un día bonito.",
        "fr" => "Bonjour ! Il fait beau aujourd'hui.",
        "de" => "Hallo! Heute ist schönes Wetter.",
        "it" => "Ciao! Oggi è una bella giornata.",
        _ => "Hello! It is a nice day today.",
    }
}

/// Google's voice for one piece of text (at most about 200 characters).
pub fn google_tts_url(lang: &str, piece: &str) -> String {
    format!("https://translate.google.com/translate_tts?ie=UTF-8&client=tw-ob&tl={}&q={}", crate::util::lang_base(lang), crate::util::url_encode(piece))
}

/// A text cut into pieces short enough for the voice (at sentence ends, then at spaces or anywhere).
pub fn speech_pieces(text: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        let end = matches!(c, '。' | '！' | '？' | '.' | '!' | '?' | '\n');
        if (end && cur.chars().count() > max / 3) || cur.chars().count() >= max {
            out.push(cur.trim().to_string());
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out.into_iter().filter(|p| !p.is_empty()).collect()
}

fn cjk(lang: &str) -> bool {
    matches!(crate::util::lang_base(lang).as_str(), "ja" | "zh" | "ko" | "yue")
}

/// A short line in the language with its normal punctuation: Whisper copies the style of the text before it, so
/// this gets "。" and "、" (and kana / kanji instead of romaji) for Japanese, "¿" for Spanish, and so on.
fn style_seed(lang: &str) -> &'static str {
    match crate::util::lang_base(lang).as_str() {
        "ja" => "はい、そうです。わかりました。",
        "zh" => "你好。是的，我明白了。",
        "ko" => "네, 그렇습니다. 알겠습니다.",
        "pt" => "Olá, tudo bem? Sim, entendi.",
        "es" => "Hola, ¿qué tal? Sí, entiendo.",
        "fr" => "Bonjour, ça va ? Oui, je comprends.",
        "de" => "Hallo, wie geht's? Ja, ich verstehe.",
        "it" => "Ciao, come stai? Sì, ho capito.",
        "ru" => "Привет, как дела? Да, понимаю.",
        _ => "Hello, how are you? Yes, I see.",
    }
}

/// The initial prompt for Whisper: the style line, the words and phrases the learner is likely to use (hints,
/// words learned) and the tutor's last lines (the scene's own vocabulary), the most recent last. Whisper keeps only
/// the end of a long prompt (about 220 tokens), so it is cut from the front: CJK text to 150 characters, others 400.
pub fn stt_prompt(lang: &str, tutor_lines: &[String], phrases: &[String], words: &[String]) -> String {
    let cjk = cjk(lang);
    let (sep, max) = if cjk { ("、", 150usize) } else { (", ", 400usize) };
    let mut parts: Vec<String> = vec![style_seed(lang).to_string()];
    let words: Vec<&str> = words.iter().map(|w| w.trim()).filter(|w| !w.is_empty()).take(8).collect();
    if !words.is_empty() {
        let mut w = words.join(sep);
        w.push_str(if cjk { "。" } else { "." });
        parts.push(w);
    }
    for p in phrases.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).take(3) {
        parts.push(p.to_string());
    }
    for l in tutor_lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        parts.push(l.to_string());
    }
    let all = parts.join(if cjk { "" } else { " " }).replace('\n', " ");
    let n = all.chars().count();
    if n <= max {
        return all;
    }
    // keep the end, starting at a sentence when one begins close by
    let tail: String = all.chars().skip(n - max).collect();
    let cut = tail.char_indices().take(40).find(|(_, c)| matches!(c, '。' | '.' | '!' | '?' | '！' | '？')).map(|(i, c)| i + c.len_utf8());
    match cut {
        Some(i) if i < tail.len() => tail[i..].trim_start().to_string(),
        _ => tail,
    }
}

/// What Whisper (and other models) write on silence or noise: never something the learner said.
const HALLUCINATIONS: &[&str] = &[
    "ご視聴ありがとうございました",
    "ご清聴ありがとうございました",
    "最後までご視聴いただきありがとうございました",
    "チャンネル登録よろしくお願いします",
    "チャンネル登録お願いします",
    "thankyouforwatching",
    "thanksforwatching",
    "pleasesubscribe",
    "subtitlesbytheamaraorgcommunity",
    "obrigadoporassistir",
    "legendaspelacomunidadeamaraorg",
    "graciasporver",
    "subtítulosrealizadosporlacomunidaddeamaraorg",
    "untertitelimauftragdeszdf",
];

fn is_cjk_char(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0xAC00..=0xD7AF | 0x3000..=0x303F)
}

/// The text of a transcription made clean: notes such as "[音楽]" / "(拍手)" / "♪" gone, spaces Whisper puts between
/// Japanese words gone, and the usual silence hallucinations ("ご視聴ありがとうございました") dropped. "" when nothing
/// real was said.
pub fn clean_transcript(text: &str, lang: &str) -> String {
    // the bracketed notes (short ones only: a learner never speaks in brackets)
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let close = match c {
            '[' => Some(']'),
            '(' => Some(')'),
            '（' => Some('）'),
            '【' => Some('】'),
            '<' => Some('>'),
            _ => None,
        };
        if let Some(close) = close {
            if let Some(len) = chars[i + 1..].iter().take(24).position(|x| *x == close) {
                i += len + 2;
                continue;
            }
        }
        if c != '♪' && c != '♫' && c != '*' {
            out.push(c);
        }
        i += 1;
    }
    let mut t = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if cjk(lang) {
        // "今日 は いい 天気" → "今日はいい天気": a space only stays between two non-CJK characters
        let cs: Vec<char> = t.chars().collect();
        let mut s = String::new();
        for (k, c) in cs.iter().enumerate() {
            if *c == ' ' {
                let a = k.checked_sub(1).map(|j| cs[j]).unwrap_or(' ');
                let b = cs.get(k + 1).copied().unwrap_or(' ');
                if is_cjk_char(a) || is_cjk_char(b) {
                    continue;
                }
            }
            s.push(*c);
        }
        t = s;
    }
    let bare: String = t.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect();
    if bare.is_empty() || HALLUCINATIONS.iter().any(|h| bare == *h) {
        return String::new();
    }
    t.trim().to_string()
}

/// The readings of the kanji of a text, for the hover box of the page (KanaHover.js `tip`): `{reading:{},
/// meaning:{}, kanji:{"食": "eat, food · ショク · た.べる"}}`.
pub fn kanji_dict(text: &str) -> Json {
    let mut kanji: Vec<(String, Json)> = vec![];
    for c in text.chars() {
        if kanji.iter().any(|(k, _)| k.starts_with(c)) {
            continue;
        }
        if let Some(k) = crate::kanji::info(c) {
            let mut parts: Vec<String> = vec![];
            if !k.meanings.is_empty() {
                parts.push(k.meanings.iter().take(3).cloned().collect::<Vec<_>>().join(", "));
            }
            if !k.on.is_empty() {
                parts.push(k.on.iter().take(3).cloned().collect::<Vec<_>>().join("、"));
            }
            if !k.kun.is_empty() {
                parts.push(k.kun.iter().take(3).cloned().collect::<Vec<_>>().join("、"));
            }
            kanji.push((c.to_string(), Json::Str(parts.join(" · "))));
        }
        if kanji.len() >= 40 {
            break;
        }
    }
    Json::Obj(vec![("reading".into(), Json::obj(vec![])), ("meaning".into(), Json::obj(vec![])), ("kanji".into(), Json::Obj(kanji))])
}

// ---------------------------------------------------------------- Whisper on this computer: models and requests

/// A ggml model of whisper.cpp. `only`: the one language it knows ("" = all).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WhisperModel {
    pub id: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    /// size of the download (MiB)
    pub mib: u32,
    pub label: &'static str,
    pub note: &'static str,
    pub only: &'static str,
}

/// The models offered. Sizes from huggingface.co/ggerganov/whisper.cpp and kotoba-tech/kotoba-whisper-v2.0-ggml
/// (checked 2026-10-15). Japanese character error rates (CommonVoice 8 / JSUT, lower is better) published with
/// kotoba-whisper: small 15.1 / 14.2, medium 11.5 / 10.0, large-v3 8.5 / 7.1, kotoba-whisper-v2.0 9.2 / 8.4;
/// large-v3-turbo is reported at about the level of large-v2 for Japanese (9.7 / 8.2). The turbo model has the
/// encoder of large-v3 and 4 decoder layers: on a CPU it is about half as fast as small and faster than medium,
/// which it beats on accuracy, so medium is not offered.
pub const WHISPER_MODELS: &[WhisperModel] = &[
    WhisperModel {
        id: "turbo",
        file: "ggml-large-v3-turbo-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        mib: 547,
        label: "Accurate (large-v3-turbo)",
        note: "Best for Japanese, Chinese and Korean and for accents. A few seconds per sentence on a normal CPU.",
        only: "",
    },
    WhisperModel {
        id: "small",
        file: "ggml-small-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        mib: 181,
        label: "Fast (small)",
        note: "Good for Portuguese, Spanish, English, French… Weak for Japanese.",
        only: "",
    },
    WhisperModel {
        id: "turbo-q8",
        file: "ggml-large-v3-turbo-q8_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q8_0.bin",
        mib: 834,
        label: "Most accurate (large-v3-turbo q8)",
        note: "A little more accurate than Accurate, a little slower, more memory.",
        only: "",
    },
    WhisperModel {
        id: "kotoba",
        file: "ggml-kotoba-whisper-v2.0-q5_0.bin",
        url: "https://huggingface.co/kotoba-tech/kotoba-whisper-v2.0-ggml/resolve/main/ggml-kotoba-whisper-v2.0-q5_0.bin",
        mib: 513,
        label: "Japanese only (kotoba-whisper)",
        note: "Trained on native Japanese TV speech: very good for natives, often without 。、. Japanese only.",
        only: "ja",
    },
    WhisperModel {
        id: "base",
        file: "ggml-base-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
        mib: 57,
        label: "Very light (base)",
        note: "For old computers. Many mistakes, above all in Japanese.",
        only: "",
    },
];

/// Silero VAD for whisper.cpp (whisper-server --vad-model): cuts the silence and noise out before Whisper reads,
/// which stops it from inventing text on quiet parts. 885 kB.
pub const WHISPER_VAD_FILE: &str = "ggml-silero-v6.2.0.bin";
pub const WHISPER_VAD_URL: &str = "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin";

/// The model for `choice` ("auto" or a model id) and the language studied: "auto" is large-v3-turbo for Japanese,
/// Chinese and Korean (small makes too many mistakes there, above all with a learner's accent) and small for the
/// others; a model made for one language falls back to "auto" for the other languages.
pub fn whisper_model(choice: &str, lang: &str) -> &'static WhisperModel {
    let base = crate::util::lang_base(lang);
    let auto = || WHISPER_MODELS.iter().find(|m| m.id == if cjk(&base) { "turbo" } else { "small" }).unwrap_or(&WHISPER_MODELS[0]);
    match WHISPER_MODELS.iter().find(|m| m.id == choice) {
        Some(m) if m.only.is_empty() || m.only == base => m,
        _ => auto(),
    }
}

/// The request to whisper-server's /inference: the language always given (a short learner sentence is often taken
/// for another language when Whisper guesses), the prompt (`stt_prompt`), beam search, no timestamps, the
/// non-speech tokens suppressed, and the VAD with a wider pad (the first sound of a hesitant word is kept) when
/// the server has the VAD model.
pub fn whisper_request(root: &str, wav: &[u8], lang: &str, prompt: &str, vad: bool) -> crate::http::Request {
    let lang = crate::util::lang_base(lang);
    let lang = if lang.is_empty() { "auto".to_string() } else { lang };
    let mut fields: Vec<(&str, &str)> = vec![
        ("language", lang.as_str()),
        ("response_format", "json"),
        ("temperature", "0.0"),
        ("temperature_inc", "0.2"),
        ("beam_size", "5"),
        ("no_timestamps", "true"),
        ("suppress_nst", "true"),
    ];
    if !prompt.trim().is_empty() {
        fields.push(("prompt", prompt.trim()));
    }
    if vad {
        fields.push(("vad", "true"));
        fields.push(("vad_speech_pad_ms", "200"));
    }
    let (ct, body) = multipart(&fields, "file", "voice.wav", "audio/wav", wav);
    let mut r = crate::http::Request::post(&format!("{}/inference", root.trim_end_matches('/'))).timeout(240);
    r.body = Some((ct, body));
    r
}

/// What whisper-server heard (the raw text; see `clean_transcript`).
pub fn whisper_transcribe(http: &dyn crate::http::Http, root: &str, wav: &[u8], lang: &str, prompt: &str, vad: bool) -> Result<String, String> {
    let r = http.send(&whisper_request(root, wav, lang, prompt, vad)).map_err(|e| format!("Whisper on this computer is not answering ({}).", e))?;
    if !r.ok() {
        return Err(format!("Whisper on this computer answered {}: {}", r.status, r.text().chars().take(160).collect::<String>()));
    }
    let j = r.json().map_err(|_| format!("Whisper on this computer answered: {}", r.text().chars().take(160).collect::<String>()))?;
    let err = j.get("error").str();
    if !err.is_empty() {
        return Err(format!("Whisper on this computer: {}", err));
    }
    Ok(j.get("text").str().trim().to_string())
}

/// A multipart/form-data body: the text fields, then one file. Returns (content type, body).
pub fn multipart(fields: &[(&str, &str)], file_field: &str, file_name: &str, file_type: &str, file: &[u8]) -> (String, Vec<u8>) {
    let boundary = "----reelhouse-voice-7f3a9c";
    let mut body: Vec<u8> = Vec::new();
    for (k, v) in fields {
        body.extend_from_slice(format!("--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n", boundary, k, v).as_bytes());
    }
    body.extend_from_slice(format!("--{}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n", boundary, file_field, file_name, file_type).as_bytes());
    body.extend_from_slice(file);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    (format!("multipart/form-data; boundary={}", boundary), body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gemini() -> Cfg {
        Cfg { provider: "gemini".into(), key: "g".into(), model: String::new(), base: String::new() }
    }

    #[test]
    fn auto_hears_with_a_key_then_this_computer() {
        // a key: the cloud first; the local Whisper catches its failures once it is installed
        assert_eq!(stt_plan("auto", Some(gemini()), true, false, "turbo"), vec![Stt::Cloud(gemini())]);
        assert_eq!(stt_plan("auto", Some(gemini()), true, true, "turbo"), vec![Stt::Cloud(gemini()), Stt::Local("turbo".into())]);
        // no key: this computer (the first use downloads it)
        assert_eq!(stt_plan("auto", None, true, false, "turbo"), vec![Stt::Local("turbo".into())]);
        // a phone: only the cloud
        assert_eq!(stt_plan("auto", None, false, false, "turbo"), vec![]);
        assert!(hear_missing("auto", false).contains("Gemini or OpenAI"));
        assert_eq!(hear_missing("auto", true), "");
    }

    #[test]
    fn a_chosen_engine_is_kept() {
        assert_eq!(stt_plan("local", Some(gemini()), true, false, "small"), vec![Stt::Local("small".into())]);
        assert_eq!(stt_plan("local", Some(gemini()), false, true, "small"), vec![]);
        assert_eq!(stt_plan("gemini", None, true, true, "small"), vec![], "Gemini chosen without its key");
        assert!(hear_missing("gemini", true).contains("Gemini"));
        assert_eq!(stt_plan("openai", Some(gemini()), true, true, "small").len(), 2);
        assert_eq!(Stt::Local("small".into()).label(), "local-whisper(small)");
        assert_eq!(Stt::Cloud(gemini()).label(), "gemini");
        assert_eq!(Stt::Cloud(Cfg { provider: "openai".into(), ..gemini() }).pretty(), "Whisper (OpenAI)");
    }

    #[test]
    fn speaking_prefers_google_then_the_system() {
        assert_eq!(tts_plan("auto"), vec![Tts::Google, Tts::Local]);
        assert_eq!(tts_plan(""), vec![Tts::Google, Tts::Local]);
        assert_eq!(tts_plan("local"), vec![Tts::Local]);
        assert_eq!(tts_plan("google"), vec![Tts::Google]);
        let u = google_tts_url("ja-JP", "こんにちは 世界");
        assert!(u.starts_with("https://translate.google.com/translate_tts?") && u.contains("tl=ja&") && !u.contains(' '));
        // the system voices the page reported
        assert_eq!(has_system_voice(&Json::Null, "ja"), None, "not reported yet");
        let none = Json::parse(r#"{"available":false}"#).unwrap();
        assert_eq!(has_system_voice(&none, "ja"), Some(false));
        let win = Json::parse(r#"{"available":true,"engine":"sapi","voices":[{"name":"Microsoft Zira Desktop","locale":"en_US"},{"name":"Microsoft Haruka Desktop","locale":"ja_JP"}]}"#).unwrap();
        assert_eq!(has_system_voice(&win, "ja"), Some(true));
        assert_eq!(has_system_voice(&win, "pt-BR"), Some(false));
        assert!(sample_sentence("ja").ends_with('。'));
    }

    #[test]
    fn speech_is_cut_at_sentences() {
        let p = speech_pieces("こんにちは。今日はいい天気ですね！散歩に行きませんか？", 30);
        assert!(p.len() >= 2 && p.iter().all(|x| x.chars().count() <= 30));
        assert_eq!(p.concat(), "こんにちは。今日はいい天気ですね！散歩に行きませんか？");
        assert_eq!(speech_pieces("short", 180), vec!["short".to_string()]);
    }

    #[test]
    fn the_prompt_carries_the_scene_and_japanese_punctuation() {
        let tutor = vec!["いらっしゃいませ。ご注文はお決まりですか？".to_string()];
        let p = stt_prompt("ja", &tutor, &["ラーメンを一つください。".into()], &["醤油".into(), " ".into(), "餃子".into()]);
        assert!(p.starts_with("はい、そうです。"), "{}", p);
        assert!(p.contains("醤油、餃子。"));
        assert!(p.ends_with("ご注文はお決まりですか？"), "the tutor's last line comes last: {}", p);
        assert!(!p.contains(' '));
        // too long: the start goes, the last line stays whole
        let long: Vec<String> = (0..12).map(|i| format!("これは{}番目の長い文です。", i)).collect();
        let p = stt_prompt("ja", &long, &[], &[]);
        assert!(p.chars().count() <= 150);
        assert!(p.ends_with("これは11番目の長い文です。"));
        assert!(p.starts_with("これは"), "cut at a sentence: {}", p);
        let es = stt_prompt("es", &["¿Qué quieres comer?".into()], &[], &["paella".into()]);
        assert_eq!(es, "Hola, ¿qué tal? Sí, entiendo. paella. ¿Qué quieres comer?");
    }

    #[test]
    fn transcripts_are_cleaned() {
        assert_eq!(clean_transcript(" 今日 は いい 天気 ですね。 ", "ja"), "今日はいい天気ですね。");
        assert_eq!(clean_transcript("[音楽] ラーメン を ください (笑)", "ja"), "ラーメンをください");
        assert_eq!(clean_transcript("ご視聴ありがとうございました。", "ja"), "");
        assert_eq!(clean_transcript("♪～", "ja"), "");
        assert_eq!(clean_transcript("Thank you for watching!", "en"), "");
        assert_eq!(clean_transcript(" Eu  quero um café (por favor ", "pt"), "Eu quero um café (por favor");
        assert_eq!(clean_transcript("Obrigado por assistir.", "pt"), "");
        assert_eq!(clean_transcript("Bonjour, je voudrais un café.", "fr"), "Bonjour, je voudrais un café.");
        // things a learner really says stay, even short ones
        assert_eq!(clean_transcript("おやすみなさい。", "ja"), "おやすみなさい。");
        assert_eq!(clean_transcript("ご視聴ありがとうございました、と言いました。", "ja"), "ご視聴ありがとうございました、と言いました。");
        // a mixed line keeps the space between Latin words
        assert_eq!(clean_transcript("iPhone を 買いました New York で", "ja"), "iPhoneを買いましたNew Yorkで");
    }

    #[test]
    fn kanji_readings_for_the_hover_box() {
        let d = kanji_dict("水を飲みます");
        let k = d.get("kanji");
        assert!(matches!(d.get("reading"), Json::Obj(_)));
        if crate::kanji::info('水').is_some() {
            assert!(!k.get("水").str().is_empty());
            assert!(!k.get("飲").str().is_empty());
        }
        assert!(k.get("を").str().is_empty());
    }

    #[test]
    fn japanese_gets_the_accurate_model() {
        assert_eq!(whisper_model("auto", "ja").id, "turbo");
        assert_eq!(whisper_model("", "ja-JP").id, "turbo");
        assert_eq!(whisper_model("auto", "ko").id, "turbo");
        assert_eq!(whisper_model("auto", "pt").id, "small");
        assert_eq!(whisper_model("auto", "").id, "small");
        assert_eq!(whisper_model("kotoba", "ja").id, "kotoba");
        assert_eq!(whisper_model("kotoba", "es").id, "small", "kotoba only knows Japanese");
        assert_eq!(whisper_model("base", "ja").id, "base");
        assert_eq!(whisper_model("nonsense", "zh").id, "turbo");
        for m in WHISPER_MODELS {
            assert!(m.url.starts_with("https://huggingface.co/") && m.url.ends_with(m.file), "{}", m.id);
        }
        assert!(WHISPER_VAD_URL.ends_with(WHISPER_VAD_FILE));
    }

    #[test]
    fn the_local_request_names_the_language_and_prompt() {
        let r = whisper_request("http://127.0.0.1:8178/", b"RIFF", "ja-JP", " はい。 ", true);
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "http://127.0.0.1:8178/inference");
        let (ct, body) = r.body.clone().unwrap();
        assert!(ct.starts_with("multipart/form-data"));
        let b = String::from_utf8_lossy(&body);
        for f in ["name=\"language\"\r\n\r\nja\r\n", "name=\"prompt\"\r\n\r\nはい。\r\n", "name=\"response_format\"\r\n\r\njson\r\n", "name=\"vad\"\r\n\r\ntrue\r\n", "name=\"beam_size\"\r\n\r\n5\r\n"] {
            assert!(b.contains(f), "missing {:?}", f);
        }
        let plain = whisper_request("http://x", b"RIFF", "", "", false);
        let b = String::from_utf8_lossy(&plain.body.as_ref().unwrap().1).to_string();
        assert!(b.contains("name=\"language\"\r\n\r\nauto\r\n") && !b.contains("name=\"prompt\"") && !b.contains("name=\"vad\""));
        let http = crate::http::fake::Fake::new()
            .route("POST http://127.0.0.1:8178/inference", 200, r#"{"text":" ラーメンをください。\n"}"#)
            .route("POST http://127.0.0.1:9/inference", 500, r#"{"error":"failed to read audio"}"#);
        assert_eq!(whisper_transcribe(&http, "http://127.0.0.1:8178", b"RIFF", "ja", "", false).unwrap(), "ラーメンをください。");
        assert!(whisper_transcribe(&http, "http://127.0.0.1:9", b"RIFF", "ja", "", false).unwrap_err().contains("500"));
    }

    #[test]
    fn multipart_has_the_fields_and_the_file() {
        let (ct, body) = multipart(&[("language", "ja"), ("prompt", "はい。")], "file", "voice.wav", "audio/wav", b"RIFFdata");
        assert!(ct.starts_with("multipart/form-data; boundary="));
        let b = String::from_utf8_lossy(&body);
        assert!(b.contains("name=\"language\"\r\n\r\nja\r\n"));
        assert!(b.contains("name=\"prompt\"\r\n\r\nはい。\r\n"));
        assert!(b.contains("filename=\"voice.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFFdata\r\n"));
        assert!(b.trim_end().ends_with("--"));
    }
}
