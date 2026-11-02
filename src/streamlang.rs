//! The audio languages a stream's name says it carries, for the flags shown beside each stream (HP, 2026-11-02:
//! "bandeiras ao lado do nome ... que correspondam às faixas de áudio de cada país"). Only what the text says is
//! read: words such as "Dublado", "Latino", "ITA", a "PT-BR" tag, or the flag emojis some stream addons print.
//! A language named as a subtitle ("ENG SUB", "Legendado PT-BR") is not an audio track and is left out.
//! "Dual Áudio"/"Multi" only say there is more than one language: that is `multi`.

/// One audio language: a short code ("pt-BR", "en", "ja", "es-419"…) and the country whose flag stands for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioLang {
    pub lang: String,
    pub flag: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioLangs {
    pub langs: Vec<AudioLang>,
    /// The name says "dual audio" / "multi audio".
    pub multi: bool,
}

/// Words (whole tokens, lower case) that name an audio language: (word, language, flag).
const WORDS: &[(&str, &str, &str)] = &[
    ("dublado", "pt-BR", "br"),
    ("dublada", "pt-BR", "br"),
    ("dublagem", "pt-BR", "br"),
    ("ptbr", "pt-BR", "br"),
    ("portugues", "pt-BR", "br"),
    ("português", "pt-BR", "br"),
    ("portuguese", "pt-BR", "br"),
    ("nacional", "pt-BR", "br"),
    ("brazilian", "pt-BR", "br"),
    ("brasileiro", "pt-BR", "br"),
    ("portugal", "pt-PT", "pt"),
    ("english", "en", "us"),
    ("eng", "en", "us"),
    ("inglês", "en", "us"),
    ("ingles", "en", "us"),
    ("japanese", "ja", "jp"),
    ("jpn", "ja", "jp"),
    ("japonês", "ja", "jp"),
    ("japones", "ja", "jp"),
    ("日本語", "ja", "jp"),
    ("latino", "es-419", "mx"),
    ("latam", "es-419", "mx"),
    ("castellano", "es", "es"),
    ("spanish", "es", "es"),
    ("español", "es", "es"),
    ("espanol", "es", "es"),
    ("espanhol", "es", "es"),
    ("spa", "es", "es"),
    ("french", "fr", "fr"),
    ("truefrench", "fr", "fr"),
    ("français", "fr", "fr"),
    ("francais", "fr", "fr"),
    ("vff", "fr", "fr"),
    ("vfq", "fr", "fr"),
    ("vf", "fr", "fr"),
    ("vf2", "fr", "fr"),
    ("german", "de", "de"),
    ("deutsch", "de", "de"),
    ("ger", "de", "de"),
    ("italian", "it", "it"),
    ("italiano", "it", "it"),
    ("ita", "it", "it"),
    ("russian", "ru", "ru"),
    ("rus", "ru", "ru"),
    ("русский", "ru", "ru"),
    ("korean", "ko", "kr"),
    ("kor", "ko", "kr"),
    ("chinese", "zh", "cn"),
    ("mandarin", "zh", "cn"),
    ("cantonese", "zh", "cn"),
    ("hindi", "hi", "in"),
    ("hin", "hi", "in"),
    ("turkish", "tr", "tr"),
    ("türkçe", "tr", "tr"),
    ("polish", "pl", "pl"),
    ("polski", "pl", "pl"),
    ("pldub", "pl", "pl"),
    ("lektor", "pl", "pl"),
    ("ukrainian", "uk", "ua"),
    ("ukr", "uk", "ua"),
    ("dutch", "nl", "nl"),
    ("arabic", "ar", "sa"),
    ("thai", "th", "th"),
];

/// Words beside a language that make it a subtitle, not an audio track.
const SUB_WORDS: &[&str] = &[
    "sub", "subs", "subbed", "subtitle", "subtitles", "subtitled", "legenda", "legendas", "legendado", "legendada", "srt", "vost", "vostfr", "sottotitoli",
    "subtitulado", "subtitulos", "esub", "esubs", "msubs", "multisub", "multisubs",
];

/// The language a flag emoji's country stands for (for telling two flags of one language apart).
fn country_lang(cc: &str) -> &'static str {
    match cc {
        "br" => "pt-BR",
        "pt" => "pt-PT",
        "gb" | "us" | "au" | "ca" | "ie" | "nz" => "en",
        "jp" => "ja",
        "es" => "es",
        "mx" | "ar" | "co" | "cl" | "pe" | "ve" => "es-419",
        "fr" => "fr",
        "de" | "at" => "de",
        "it" => "it",
        "ru" => "ru",
        "kr" => "ko",
        "cn" | "tw" | "hk" => "zh",
        "in" => "hi",
        "tr" => "tr",
        "pl" => "pl",
        "ua" => "uk",
        "nl" => "nl",
        "sa" | "ae" | "eg" => "ar",
        "th" => "th",
        _ => "",
    }
}

fn is_sub(w: Option<&String>) -> bool {
    w.is_some_and(|w| SUB_WORDS.contains(&w.as_str()))
}

/// Reads the audio languages of a stream from its name, description and file name (any text, any case).
pub fn audio_langs(text: &str) -> AudioLangs {
    let mut out = AudioLangs::default();
    // flag emojis, in the order they are written (pairs of regional indicator letters)
    let mut found: Vec<(usize, String, String)> = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let ri = |c: char| (0x1F1E6..=0x1F1FF).contains(&(c as u32));
    let mut i = 0;
    while i + 1 < chars.len() {
        let (pos, a) = chars[i];
        let b = chars[i + 1].1;
        if ri(a) && ri(b) {
            let cc: String = [a, b].iter().map(|c| char::from_u32(*c as u32 - 0x1F1E6 + 'a' as u32).unwrap_or('?')).collect();
            let lang = country_lang(&cc);
            // a country the app has no language for keeps its own code as the language
            let lang = if lang.is_empty() { cc.clone() } else { lang.to_string() };
            found.push((pos, lang, cc));
            i += 2;
        } else {
            i += 1;
        }
    }
    // words, with where they start (so emojis and words keep the order of the text)
    let lower = text.to_lowercase();
    let mut toks: Vec<(usize, String)> = Vec::new();
    let mut start: Option<usize> = None;
    for (p, c) in lower.char_indices() {
        if c.is_alphanumeric() {
            if start.is_none() {
                start = Some(p);
            }
        } else if let Some(s) = start.take() {
            toks.push((s, lower[s..p].to_string()));
        }
    }
    if let Some(s) = start {
        toks.push((s, lower[s..].to_string()));
    }
    let words: Vec<String> = toks.iter().map(|(_, w)| w.clone()).collect();
    let latino = words.iter().any(|w| w == "latino" || w == "latam");
    for (k, (pos, w)) in toks.iter().enumerate() {
        let prev = if k > 0 { words.get(k - 1) } else { None };
        let next = words.get(k + 1);
        // "PT-BR" / "PT BR" is two tokens
        let hit: Option<(&'static str, &'static str)> = if w == "pt" && next.is_some_and(|n| n == "br") {
            if is_sub(prev) || is_sub(words.get(k + 2)) { None } else { Some(("pt-BR", "br")) }
        } else if w == "dual" || w == "multi" || w == "multiaudio" || w == "dualaudio" {
            if !is_sub(next) {
                out.multi = true;
            }
            None
        } else {
            WORDS.iter().find(|(word, _, _)| word == w).and_then(|(_, l, f)| {
                // "Español Latino" is the Latin American dub, not Castilian
                let castilian_word = latino && *l == "es" && (w == "español" || w == "espanol" || w == "spanish" || w == "spa");
                if is_sub(prev) || is_sub(next) || castilian_word {
                    None
                } else {
                    Some((*l, *f))
                }
            })
        };
        if let Some((l, f)) = hit {
            found.push((*pos, l.to_string(), f.to_string()));
        }
    }
    // "Dual Áudio" is how Brazilian releases say they carry the Portuguese dub beside the original
    if out.multi && words.iter().any(|w| w == "áudio") {
        found.push((usize::MAX, "pt-BR".to_string(), "br".to_string()));
    }
    found.sort_by_key(|f| f.0);
    for (_, lang, flag) in found {
        if !out.langs.iter().any(|x| x.lang == lang) {
            out.langs.push(AudioLang { lang, flag });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(t: &str) -> Vec<String> {
        audio_langs(t).langs.into_iter().map(|l| l.flag).collect()
    }

    #[test]
    fn reads_dub_words() {
        assert_eq!(flags("DAN DA DAN T02 EP04 🇧🇷 Dublado"), vec!["br"]);
        assert_eq!(flags("Pokemon S06E03 1080p DUBLADO"), vec!["br"]);
        assert_eq!(flags("Show.S01E01.1080p.WEB-DL.PT-BR.x264"), vec!["br"]);
        assert_eq!(flags("Serie 1080p Latino"), vec!["mx"]);
        assert_eq!(flags("Serie Español Latino"), vec!["mx"]);
        assert_eq!(flags("Serie Castellano"), vec!["es"]);
        assert_eq!(flags("Film.2024.iTA.ENG.1080p"), vec!["it", "us"]);
        assert_eq!(flags("Film TRUEFRENCH 1080p"), vec!["fr"]);
    }

    #[test]
    fn reads_flag_emojis_in_order() {
        assert_eq!(flags("Multi Audio / 🇬🇧 / 🇯🇵 / 🇮🇹"), vec!["gb", "jp", "it"]);
        assert!(audio_langs("Multi Audio / 🇬🇧 / 🇯🇵").multi);
        // one language once, even when both its word and its flag are there
        assert_eq!(flags("🇧🇷 Dublado PT-BR"), vec!["br"]);
    }

    #[test]
    fn subtitles_are_not_audio() {
        assert!(flags("[SubsPlease] Dandadan - 16 (720p) ENG SUB").is_empty());
        assert!(flags("Serie Legendado PT-BR").is_empty());
        assert!(flags("Legendado").is_empty());
        assert!(!audio_langs("Show 1080p Multi-Subs").multi);
        assert!(flags("Film VOSTFR 1080p").is_empty());
    }

    #[test]
    fn dual_audio() {
        let a = audio_langs("Dandadan 1080p Dual Áudio");
        assert!(a.multi);
        assert_eq!(a.langs.iter().map(|l| l.flag.as_str()).collect::<Vec<_>>(), vec!["br"]);
        let b = audio_langs("Dandadan 1080p Dual Audio");
        assert!(b.multi);
        assert!(b.langs.is_empty());
    }

    #[test]
    fn plain_names_say_nothing() {
        assert!(flags("1080p · Webrip 66/100 TrueSpec Verified hevc 10-bit").is_empty());
        assert!(flags("Dandadan / Dan Da Dan [S02] (2025) WEB-DL 1080p x264 AAC").is_empty());
    }
}
