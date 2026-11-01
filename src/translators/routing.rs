//! Which translator serves each place of the app that translates (Preferences > Translators). Part of `translate`.
//!
//! Every place asks `for_use(place, settings, http)`. A place left on "auto" keeps the choice it always had: the
//! subtitles use the service of Preferences > Subtitles, the readers the same one, or the free chain without Google.
use super::*;
use crate::settings::Settings;

/// A translator that can be chosen.
pub struct Info {
    pub id: &'static str,
    pub name: &'static str,
    /// Needs nothing from the user (no key, no account).
    pub free: bool,
    /// One line about it, for the preferences (English; the interface translates it).
    pub note: &'static str,
}

/// Every translator, in the order the preferences list them. "auto" is not a translator of its own: it is the
/// choice each place had before (see `for_use`).
pub const CATALOGUE: &[Info] = &[
    Info { id: "free", name: "Free chain (automatic)", free: true, note: "LibreTranslate servers, then Microsoft Edge, then Google, Yandex, Lingva and MyMemory, the first that answers" },
    Info { id: "libretranslate", name: "LibreTranslate", free: true, note: "Your own server, or the public ones (often busy)" },
    Info { id: "edge", name: "Microsoft Edge (free)", free: true, note: "The translator built into Edge: fast, many lines per request, no key" },
    Info { id: "google", name: "Google (free web)", free: true, note: "Good quality, one line per request; limits heavy use" },
    Info { id: "yandex", name: "Yandex (free)", free: true, note: "Several lines per request, slowed down to avoid its limit" },
    Info { id: "lingva", name: "Lingva (free)", free: true, note: "Google's translation through volunteer servers" },
    Info { id: "mymemory", name: "MyMemory (free)", free: true, note: "A daily quota of characters; slow for long texts" },
    Info { id: "azure", name: "Microsoft Translator (Azure)", free: false, note: "Your Azure key: 2 million characters a month free, fast and steady" },
    Info { id: "claude", name: "Claude (Anthropic)", free: false, note: "A language model: natural dialogue, uses the context; paid per use" },
    Info { id: "gemini", name: "Gemini (Google)", free: false, note: "A language model with a free tier; paid beyond it" },
    Info { id: "openai", name: "GPT (OpenAI)", free: false, note: "A language model (or any OpenAI-compatible server); paid per use" },
    Info { id: "ollama", name: "Local AI (Ollama · qwen2.5:3b)", free: true, note: "Runs on this computer: free, private and offline, but slow without a good graphics card; downloaded the first time (about 3 GB)" },
];

/// The places of the app that translate: (id, name, what it is). The names are English; the interface translates them.
pub const USES: &[(&str, &str, &str)] = &[
    ("subtitles", "Automatic subtitles", "Subtitles found and translated while the video loads"),
    ("forced", "Forced translation", "Translate a subtitle from the player's Subtitles menu"),
    ("readers_images", "Images reader", "Speech bubbles of manga pages and the chosen text"),
    ("readers_text", "Text reader", "Chapters of novels"),
    ("bubbles", "Bubble editor", "The Translate button of the speech bubble editor"),
    ("study_cards", "Study cards", "The translation of a card made from a page"),
    ("other", "Other", "Any other translation"),
];

/// Whether a translator has what it needs (a key for the paid ones).
pub fn configured(id: &str, s: &Settings) -> bool {
    let sb = &s.subtitles;
    match id {
        "azure" => !s.translate.azure_key.trim().is_empty(),
        "claude" => !sb.claude_key.trim().is_empty(),
        "gemini" => !sb.gemini_key.trim().is_empty(),
        "openai" => !sb.openai_key.trim().is_empty(),
        // the local AI: desktop only
        "ollama" => crate::platform::DESKTOP_AI,
        _ => CATALOGUE.iter().any(|i| i.id == id),
    }
}

pub fn name_of(id: &str) -> String {
    CATALOGUE.iter().find(|i| i.id == id).map(|i| i.name.to_string()).unwrap_or_else(|| id.to_string())
}

/// The translator `id` of the catalogue, or why it cannot work yet.
pub fn build<'a>(id: &str, s: &Settings, http: &'a dyn Http) -> Result<Box<dyn Translator + 'a>, String> {
    let sb = &s.subtitles;
    let tc = &s.translate;
    if !configured(id, s) {
        return Err(if CATALOGUE.iter().any(|i| i.id == id) {
            format!("{} needs a key. Add it in Preferences > Translators.", name_of(id))
        } else {
            format!("Unknown translator \"{}\".", id)
        });
    }
    Ok(match id {
        "free" => Box::new(Automatic::new(http, &sb.libre_url, &sb.libre_key)),
        "libretranslate" => Box::new(Automatic::new(http, &sb.libre_url, &sb.libre_key).libre_only()),
        "edge" => Box::new(Edge { http }),
        "google" => Box::new(GoogleWeb { http }),
        "yandex" => Box::new(Yandex { http }),
        "lingva" => Box::new(Lingva { http }),
        "mymemory" => Box::new(MyMemory { http }),
        "azure" => Box::new(Azure::new(http, &tc.azure_key, &tc.azure_region, &tc.azure_endpoint)),
        "claude" => Box::new(Claude { http, key: sb.claude_key.trim().to_string(), model: sb.claude_model.clone() }),
        "gemini" => Box::new(Gemini { http, key: sb.gemini_key.clone(), model: sb.gemini_model.clone() }),
        "ollama" => super::local_ai(sb, http, &local_model(s)).ok_or_else(|| "No translator is set up. Choose one in Preferences, Subtitles.".to_string())?,
        _ => Box::new(OpenAi { http, key: sb.openai_key.clone(), model: sb.openai_model.clone(), base: sb.openai_base.clone() }),
    })
}

/// The translator for a place of the app (`USES`). "auto" keeps the behaviour of before: the subtitles take the
/// service of Preferences > Subtitles, the readers and the rest that service too, or the free chain without
/// Google when it is LibreTranslate. The automatic subtitles stay off while "Find and translate subtitles" is off.
pub fn for_use<'a>(use_id: &str, s: &Settings, http: &'a dyn Http) -> Result<Box<dyn Translator + 'a>, String> {
    if use_id == "subtitles" && !s.subtitles.enabled {
        return Err("Subtitle translation is turned off.".to_string());
    }
    let mut choice = s.translate.choice(use_id);
    // the local AI is desktop only: on Android and iOS a place set to it takes the automatic choice
    if choice == "ollama" && !crate::platform::DESKTOP_AI {
        choice = "auto".to_string();
    }
    if choice != "auto" {
        return build(&choice, s, http);
    }
    if s.subtitles.provider == "ollama" && crate::platform::DESKTOP_AI {
        if let Some(t) = super::local_ai(&s.subtitles, http, &local_model(s)) {
            return Ok(t);
        }
    }
    match use_id {
        "subtitles" | "forced" => {
            let mut sub = s.subtitles.clone();
            sub.enabled = true;
            from_settings(&sub, http).ok_or_else(|| "No translator is set up. Choose one in Preferences, Subtitles.".to_string())
        }
        _ => Ok(for_readers(&s.subtitles, http)),
    }
}

/// The model of the local AI: the one chosen for Ollama in Conversation, else `llm::OLLAMA_MODEL`. One model for
/// both, so the local AI is not set up twice.
pub fn local_model(s: &Settings) -> String {
    crate::llm::pick("ollama", &s.subtitles, &s.general.kaiwa_ollama).model
}

/// Whether the place `use_id` translates with the local AI (chosen for it, or "auto" with the local AI in
/// Preferences > Subtitles): the controller then gets it ready and waits for it before translating.
pub fn uses_local_ai(use_id: &str, s: &Settings) -> bool {
    if !crate::platform::DESKTOP_AI || (use_id == "subtitles" && !s.subtitles.enabled) {
        return false;
    }
    match s.translate.choice(use_id).as_str() {
        "ollama" => true,
        "auto" => s.subtitles.provider == "ollama",
        _ => false,
    }
}

/// The name of the folder of saved translations of the readers: on "auto" the one used before (the service of
/// Preferences > Subtitles), so what was translated earlier is still found; otherwise the translator's own.
pub fn cache_key(use_id: &str, s: &Settings) -> String {
    match s.translate.choice(use_id).as_str() {
        "auto" => s.subtitles.provider.clone(),
        other => other.to_string(),
    }
}

/// What "auto" means for a place, in words, for the preferences.
pub fn auto_name(use_id: &str, s: &Settings) -> String {
    let p = s.subtitles.provider.as_str();
    let service = match p {
        "claude" | "gemini" | "openai" | "ollama" => name_of(p),
        _ => String::new(),
    };
    match use_id {
        _ if !service.is_empty() => service,
        "subtitles" | "forced" => "LibreTranslate → Edge → Google, Yandex, Lingva, MyMemory".to_string(),
        _ => "LibreTranslate → Edge → Yandex, Lingva, MyMemory".to_string(),
    }
}

/// The sentence the preferences translate to compare the translators.
pub const SAMPLE: &str = "Where are you going? I'll wait for you at the station.";

/// Translates `text` once with `tr` and says how long it took (milliseconds).
pub fn timed_sample(tr: &dyn Translator, text: &str, source: &str, target: &str) -> Result<(String, u64), String> {
    let t0 = std::time::Instant::now();
    let v = tr.translate(&[text.to_string()], source, target)?;
    let ms = t0.elapsed().as_millis() as u64;
    let out = v.into_iter().next().unwrap_or_default();
    if out.trim().is_empty() {
        return Err("The translator returned nothing.".to_string());
    }
    Ok((out, ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    fn with(pairs: &[(&str, &str)]) -> Settings {
        let mut s = Settings::default();
        s.translate.per_use = pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        s
    }

    #[test]
    fn auto_keeps_the_old_choice_of_each_place() {
        let f = Fake::new();
        let mut s = Settings::default();
        assert!(for_use("subtitles", &s, &f).unwrap().name().starts_with("LibreTranslate"));
        assert!(for_use("readers_images", &s, &f).unwrap().name().starts_with("LibreTranslate"));
        s.subtitles.provider = "claude".into();
        s.subtitles.claude_key = "k".into();
        assert_eq!(for_use("forced", &s, &f).unwrap().name(), "Claude");
        assert_eq!(for_use("readers_text", &s, &f).unwrap().name(), "Claude");
        assert_eq!(cache_key("readers_text", &s), "claude");
        // the switch of the automatic subtitles still decides for them, not for the forced translation
        s.subtitles.enabled = false;
        assert!(for_use("subtitles", &s, &f).is_err());
        assert!(for_use("forced", &s, &f).is_ok());
    }

    #[test]
    fn each_place_can_have_its_own_translator() {
        let f = Fake::new();
        let mut s = with(&[("subtitles", "edge"), ("readers_images", "azure"), ("bubbles", "google"), ("study_cards", "auto")]);
        assert_eq!(for_use("subtitles", &s, &f).unwrap().name(), "Microsoft Edge (free)");
        assert_eq!(for_use("bubbles", &s, &f).unwrap().name(), "Google (free web)");
        // Azure without a key: a clear error instead of another translator in silence
        assert!(for_use("readers_images", &s, &f).err().unwrap().contains("needs a key"));
        s.translate.azure_key = "K".into();
        assert_eq!(for_use("readers_images", &s, &f).unwrap().name(), "Microsoft Translator");
        assert_eq!(cache_key("readers_images", &s), "azure");
        assert!(for_use("study_cards", &s, &f).unwrap().name().starts_with("LibreTranslate"));
        assert!(for_use("readers_text", &s, &f).unwrap().name().starts_with("LibreTranslate"));
        assert!(build("nope", &s, &f).is_err());
    }

    #[test]
    fn every_catalogue_entry_builds_when_configured() {
        let f = Fake::new();
        let mut s = Settings::default();
        s.translate.azure_key = "a".into();
        s.subtitles.claude_key = "c".into();
        s.subtitles.gemini_key = "g".into();
        s.subtitles.openai_key = "o".into();
        for i in CATALOGUE {
            // the local AI exists on desktop only (Android and iOS refuse it as not set up)
            assert_eq!(build(i.id, &s, &f).is_ok(), i.id != "ollama" || crate::platform::DESKTOP_AI, "{}", i.id);
        }
        assert!(USES.iter().all(|(u, _, _)| for_use(u, &s, &f).is_ok()));
    }

    /// Android and iOS: a place or the subtitles set to the local AI get a cloud translator instead.
    #[test]
    #[cfg(any(target_os = "android", target_os = "ios"))]
    fn the_local_ai_falls_back_to_the_cloud_on_phones() {
        let f = Fake::new();
        let mut s = with(&[("bubbles", "ollama")]);
        s.subtitles.provider = "ollama".into();
        s.subtitles.gemini_key = "g".into();
        assert!(!configured("ollama", &s) && !uses_local_ai("bubbles", &s));
        assert!(for_use("bubbles", &s, &f).unwrap().name().contains("Gemini"));
        assert!(for_use("forced", &s, &f).unwrap().name().contains("Gemini"));
    }

    #[test]
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn the_local_ai_can_translate_every_place() {
        let f = Fake::new();
        let mut s = with(&[("bubbles", "ollama")]);
        assert!(configured("ollama", &s));
        assert!(uses_local_ai("bubbles", &s) && !uses_local_ai("readers_text", &s));
        assert!(for_use("bubbles", &s, &f).unwrap().name().starts_with("Local AI"));
        s.subtitles.provider = "ollama".into();
        assert!(USES.iter().all(|(u, _, _)| uses_local_ai(u, &s)));
        assert!(for_use("readers_images", &s, &f).unwrap().name().contains(crate::llm::OLLAMA_MODEL));
        assert_eq!(cache_key("readers_text", &s), "ollama");
        assert_eq!(auto_name("forced", &s), name_of("ollama"));
        s.general.kaiwa_ollama = "llama3.2:3b".into();
        assert_eq!(for_use("forced", &s, &f).unwrap().name(), "Local AI (llama3.2:3b)");
        s.subtitles.enabled = false;
        assert!(!uses_local_ai("subtitles", &s) && for_use("subtitles", &s, &f).is_err());
    }

    #[test]
    fn the_routed_translator_is_the_one_that_answers() {
        let f = Fake::new().route("microsofttranslator.com/translate", 200, r#"[{"translations":[{"text":"Aonde você vai?"}]}]"#);
        let mut s = with(&[("forced", "azure")]);
        s.translate.azure_key = "K".into();
        s.translate.azure_region = "brazilsouth".into();
        let tr = for_use("forced", &s, &f).unwrap();
        let (text, _ms) = timed_sample(tr.as_ref(), "Where are you going?", "en", "pt").unwrap();
        assert_eq!(text, "Aonde você vai?");
        assert!(f.requests()[0].headers.iter().any(|(k, v)| k == "Ocp-Apim-Subscription-Region" && v == "brazilsouth"));
    }

    #[test]
    fn the_choices_survive_the_settings_file() {
        let mut s = with(&[("subtitles", "azure"), ("readers_text", "edge")]);
        s.translate.azure_key = "K".into();
        s.translate.azure_region = "westeurope".into();
        let back = Settings::from_json(&Json::parse(&s.to_json().dump()).unwrap());
        assert_eq!(back.translate, s.translate);
        assert_eq!(back.translate.choice("readers_text"), "edge");
        assert_eq!(back.translate.choice("bubbles"), "auto");
    }
}
