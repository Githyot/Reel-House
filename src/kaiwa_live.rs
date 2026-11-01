//! The Kaiwa layer over Gemini Live (`live`): what turns a free voice chat into a study conversation. Desktop only.
//!
//! * `live_system`: the instruction of the spoken tutor, built from the same scene, level and memory as the written
//!   one (`kaiwa::tutor_body`), plus how to treat a learner's voice, the numbered goals and when to use each tool.
//! * `tools`: the functions Gemini calls instead of talking about the lesson: `report_correction`, `mark_goal_done`,
//!   `give_hint`, `end_scene`. `apply_tool` applies them to the session (the corrections go under the learner's line
//!   like the written checks, the goals are ticked, the score grows) and gives the answer sent back to Gemini.
//! * `Coach`: watches the transcripts after each turn and writes a short note ("[Kaiwa] …") when the tutor leaves the
//!   language or the level, the learner keeps speaking their own language, or a goal is stalled; notes go up as
//!   `clientContent` the model reads but does not answer aloud.
use crate::json::Json;
use crate::kaiwa::{self, Ctx, Goal, Session};
use crate::llm::lang_english;

/// Every note of the app starts with this (the instruction tells the model what it means).
pub const NOTE: &str = "[Kaiwa]";

/// The BCP-47 code of a language for speech ("ja" -> "ja-JP").
pub fn bcp47(lang: &str) -> String {
    let base = crate::util::lang_base(lang);
    match base.as_str() {
        "ja" => "ja-JP",
        "en" => "en-US",
        "pt" => "pt-BR",
        "es" => "es-ES",
        "fr" => "fr-FR",
        "de" => "de-DE",
        "it" => "it-IT",
        "ko" => "ko-KR",
        "zh" => "cmn-CN",
        "ru" => "ru-RU",
        "nl" => "nl-NL",
        "pl" => "pl-PL",
        "tr" => "tr-TR",
        "hi" => "hi-IN",
        "id" => "id-ID",
        "vi" => "vi-VN",
        "th" => "th-TH",
        _ => return base,
    }
    .to_string()
}

/// The voice: the one chosen, else Kore (clear and steady, a good model for a learner of Japanese) or Aoede.
pub fn voice_for(lang: &str, chosen: &str) -> String {
    let c = chosen.trim();
    if !c.is_empty() {
        return c.to_string();
    }
    if crate::util::lang_base(lang) == "ja" { "Kore" } else { "Aoede" }.to_string()
}

// ---------------------------------------------------------------- goals

/// A goal sentence of a scene cut into the steps it lists ("Order a dish and a drink, ask what is recommended, and
/// pay." -> three goals). Commas inside brackets stay; at most four steps.
pub fn split_goal(t: &str) -> Vec<String> {
    let mut parts: Vec<String> = vec![];
    let (mut cur, mut depth) = (String::new(), 0i32);
    for c in t.trim().trim_end_matches('.').chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if (c == ',' || c == ';') && depth <= 0 {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    parts.push(cur);
    let mut out: Vec<String> = parts
        .into_iter()
        .map(|p| {
            let p = p.trim();
            let p = p.strip_prefix("and ").or_else(|| p.strip_prefix("or ")).or_else(|| p.strip_prefix("then ")).unwrap_or(p).trim();
            let mut ch = p.chars();
            match ch.next() {
                Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
                None => String::new(),
            }
        })
        .filter(|p| !p.is_empty())
        .collect();
    if out.len() > 4 {
        let rest = out.split_off(3).join(", ");
        out.push(rest);
    }
    out
}

/// The goals of a new spoken session.
pub fn goals_for(mode: &str, scene_id: &str) -> Vec<Goal> {
    let texts: Vec<String> = match mode {
        "roleplay" => kaiwa::scene(scene_id).map(|s| split_goal(s.goal)).unwrap_or_default(),
        "lesson" => kaiwa::lesson(scene_id).map(|l| split_goal(l.goals)).unwrap_or_default(),
        "custom" => vec!["Open the scene in your role".into(), "Get what your role wants".into(), "Close the scene politely".into()],
        _ => vec!["Greet and say how you are".into(), "Talk about something you did or like".into(), "Ask the tutor a question".into()],
    };
    texts.into_iter().map(|text| Goal { text, done: false, at: 0 }).collect()
}

fn next_goal(goals: &[Goal]) -> Option<(usize, &str)> {
    goals.iter().enumerate().find(|(_, g)| !g.done).map(|(i, g)| (i + 1, g.text.as_str()))
}

// ---------------------------------------------------------------- what Gemini is told

/// The instruction of the spoken tutor.
pub fn live_system(c: &Ctx, goals: &[Goal]) -> String {
    let lang = lang_english(&c.lang);
    let ui = lang_english(&c.ui);
    let ja = crate::util::lang_base(&c.lang) == "ja";
    let mut p = kaiwa::tutor_body(c, true);
    p.push(String::new());
    p.push("SPOKEN, IN REAL TIME: you hear the learner's voice and answer with your voice. One to three short spoken sentences per turn, then let them talk. No emoji, lists, brackets or stage directions.".into());
    p.push(format!(
        "LANGUAGE: speak {} only, slowly and clearly, at the learner's level{}. Use {} only for a very short help when the learner is completely lost, then go back to {}.",
        lang,
        if ja { ", in standard Japanese (標準語), with polite です/ます forms unless the scene is between friends" } else { "" },
        ui,
        lang
    ));
    p.push(format!(
        "THE LEARNER'S VOICE: they are a {} learner at level {}, not a native speaker. Their accent and pronunciation are not native, and they pause to think in the middle of a sentence. Understand imperfect pronunciation generously and guess the intended words from the scene. Never comment on their accent. Only when you really cannot understand, ask them kindly to say it again, slowly.",
        lang, c.level
    ));
    p.push("CORRECTIONS: never stop the conversation or break your sentence to correct. When the learner makes a real mistake (grammar, a wrong word, a particle, a conjugation, politeness, or a pronunciation slip that changes the word), call report_correction quietly, then answer naturally using the correct form. One call per mistake; let small hesitations go.".into());
    if goals.is_empty() {
        p.push("ENDING: when the learner wants to stop, say goodbye in one short sentence and call end_scene.".into());
    } else {
        p.push("GOALS of this conversation, for the learner to reach one at a time:".into());
        for (i, g) in goals.iter().enumerate() {
            p.push(format!("{}. {}{}", i + 1, g.text, if g.done { " (already reached)" } else { "" }));
        }
        p.push("- Guide the talk so the learner can reach them one after the other; never read them out as a list. When the learner has done one, call mark_goal_done with its number.".into());
        p.push("- When all are reached, or the learner wants to stop, close warmly in your role in one or two sentences and call end_scene.".into());
    }
    p.push("HINTS: when the learner is stuck, silent for a while, or asks for help, call give_hint with one sentence they could say, and encourage them in a few words.".into());
    p.push(format!("APP NOTES: text that begins with \"{}\" comes from the study app, not from the learner. Follow it silently: never read it aloud and never mention it.", NOTE));
    p.join("\n")
}

fn prop(kind: &str, desc: &str) -> Json {
    Json::obj(vec![("type", Json::Str(kind.into())), ("description", Json::Str(desc.into()))])
}

fn function(name: &str, desc: &str, props: Vec<(&str, Json)>, required: &[&str]) -> Json {
    Json::obj(vec![
        ("name", Json::Str(name.into())),
        ("description", Json::Str(desc.into())),
        (
            "parameters",
            Json::obj(vec![
                ("type", Json::Str("OBJECT".into())),
                ("properties", Json::Obj(props.into_iter().map(|(k, v)| (k.to_string(), v)).collect())),
                ("required", Json::Arr(required.iter().map(|r| Json::Str(r.to_string())).collect())),
            ]),
        ),
    ])
}

/// The function declarations of a spoken session.
pub fn tools(c: &Ctx) -> Json {
    let lang = lang_english(&c.lang);
    let ui = lang_english(&c.ui);
    let ja = crate::util::lang_base(&c.lang) == "ja";
    Json::Arr(vec![
        function(
            "report_correction",
            "Record a real mistake the learner just made, so the app shows it under their words. Call it quietly, without pausing the conversation, and never read the correction aloud. Not for accent or small hesitations.",
            vec![
                ("original", prop("STRING", &format!("the wrong piece as the learner said it, in {}", lang))),
                ("corrected", prop("STRING", &format!("the right form, in {}", lang))),
                ("explanation", prop("STRING", &format!("one short sentence in {} saying why", ui))),
                ("kind", prop("STRING", "one of: grammar, particle, verb, word, order, politeness, pronunciation, other")),
            ],
            &["original", "corrected", "explanation"],
        ),
        function(
            "mark_goal_done",
            "Mark a goal of the conversation as reached, once the learner has done it.",
            vec![("goal", prop("INTEGER", "the number of the goal (1, 2, 3…)")), ("evidence", prop("STRING", "what the learner said that reached it"))],
            &["goal"],
        ),
        function(
            "give_hint",
            "Show the learner one sentence they could say next, when they are stuck, silent or ask for help.",
            vec![
                ("text", prop("STRING", &format!("the sentence, in {}, at the learner's level", lang))),
                ("reading", prop("STRING", if ja { "the same sentence written in hiragana only" } else { "empty" })),
                ("translation", prop("STRING", &format!("the sentence in {}", ui))),
            ],
            &["text", "translation"],
        ),
        function(
            "end_scene",
            "End the conversation when its goals are reached or the learner wants to stop. Say a short goodbye in your role first.",
            vec![("success", prop("BOOLEAN", "whether the learner reached the goals")), ("summary", prop("STRING", &format!("one or two encouraging sentences in {} on how it went", ui)))],
            &["success", "summary"],
        ),
    ])
}

/// The note that opens the call (the tutor speaks first).
pub fn opening(mode: &str) -> String {
    match mode {
        "roleplay" | "custom" => format!("{} The call starts now. Open the scene in your role with a natural first line, at the learner's level.", NOTE),
        "lesson" => format!("{} The call starts now. Greet the learner and start the lesson with its first small step.", NOTE),
        _ => format!("{} The call starts now. Greet the learner and start the conversation with one easy question.", NOTE),
    }
}

/// The learner pressed for a hint.
pub fn hint_note() -> String {
    format!("{} The learner asks for a hint: call give_hint with one sentence they could say now, at their level, and encourage them in a few words.", NOTE)
}

/// The connection came back as a new session (the old one could not be resumed): the last lines, to go on.
pub fn recap_note(s: &Session) -> String {
    let from = s.lines.len().saturating_sub(8);
    let lines: Vec<String> = s.lines[from..].iter().map(|l| format!("{}: {}", if l.role == "assistant" { "Tutor" } else { "Learner" }, l.text)).collect();
    format!("{} The connection dropped and came back. Go on with the same conversation from where it was, without greeting again. The last lines were:\n{}", NOTE, lines.join("\n"))
}

/// The learner calls an older session again: the last lines, and a short hello before going on.
pub fn again_note(s: &Session) -> String {
    let from = s.lines.len().saturating_sub(8);
    let lines: Vec<String> = s.lines[from..].iter().map(|l| format!("{}: {}", if l.role == "assistant" { "Tutor" } else { "Learner" }, l.text)).collect();
    format!("{} The learner calls again to go on with this conversation. Say hello again in a few words and go on from where it was. The last lines were:\n{}", NOTE, lines.join("\n"))
}

// ---------------------------------------------------------------- the tools, applied

/// What a tool call changed, for the controller (the session itself is already changed by `apply_tool`).
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    None,
    /// {wrong, right, why, kind}
    Correction(Json),
    /// the goal (0-based) just reached; `all`: it was the last one
    Goal { index: usize, all: bool },
    /// {text, reading, translation}
    Hint(Json),
    End { success: bool, summary: String },
}

/// Points: a goal 10, all goals 5 more, each turn the learner speaks 1.
pub const GOAL_POINTS: i64 = 10;
pub const ALL_GOALS_BONUS: i64 = 5;

fn numbered_left(goals: &[Goal]) -> Vec<Json> {
    goals.iter().enumerate().filter(|(_, g)| !g.done).map(|(i, g)| Json::Str(format!("{}. {}", i + 1, g.text))).collect()
}

/// A correction goes under the learner's last line (its `check`, as the written checks are kept). False when the
/// learner has said nothing yet.
pub fn attach_correction(s: &mut Session, err: &Json) -> bool {
    let Some(l) = s.lines.iter_mut().rev().find(|l| l.role == "user") else { return false };
    let mut check = if matches!(l.check, Json::Obj(_)) && l.check.get("error").is_null() { l.check.clone() } else { Json::obj(vec![]) };
    let mut errs = check.get("errors").items().to_vec();
    if errs.len() < 5 && !errs.contains(err) {
        errs.push(err.clone());
    }
    check.set("ok", Json::Bool(false));
    check.set("errors", Json::Arr(errs));
    check.set("source", Json::Str("live".into()));
    l.check = check;
    true
}

/// Applies a tool call to the session. Returns the answer for Gemini and what changed.
pub fn apply_tool(s: &mut Session, name: &str, args: &Json, now: i64) -> (Json, Effect) {
    let err = |t: String| (Json::obj(vec![("error", Json::Str(t))]), Effect::None);
    match name {
        "report_correction" => {
            let (wrong, right) = (args.get("original").str().trim().to_string(), args.get("corrected").str().trim().to_string());
            if wrong.is_empty() || right.is_empty() {
                return err("original and corrected are both needed".into());
            }
            if wrong == right {
                return (Json::obj(vec![("result", Json::Str("nothing to correct".into()))]), Effect::None);
            }
            let kind = args.get("kind").str().trim().to_lowercase();
            let e = Json::obj(vec![
                ("wrong", Json::Str(wrong)),
                ("right", Json::Str(right)),
                ("why", Json::Str(args.get("explanation").str().trim().to_string())),
                ("kind", Json::Str(if kind.is_empty() { "other".into() } else { kind })),
            ]);
            attach_correction(s, &e);
            (Json::obj(vec![("result", Json::Str("saved: keep talking naturally".into()))]), Effect::Correction(e))
        }
        "mark_goal_done" => {
            let n = args.get("goal").as_i64().unwrap_or(0);
            if n < 1 || n as usize > s.goals.len() {
                return err(format!("there is no goal {}; the goals are 1 to {}", n, s.goals.len()));
            }
            let i = n as usize - 1;
            if s.goals[i].done {
                return (Json::obj(vec![("result", Json::Str("already reached".into())), ("remaining", Json::Arr(numbered_left(&s.goals)))]), Effect::None);
            }
            s.goals[i].done = true;
            s.goals[i].at = now;
            s.score += GOAL_POINTS;
            let left = numbered_left(&s.goals);
            if left.is_empty() {
                s.score += ALL_GOALS_BONUS;
                (Json::obj(vec![("result", Json::Str("all goals reached: close warmly in your role, then call end_scene".into()))]), Effect::Goal { index: i, all: true })
            } else {
                let next = left[0].str();
                (Json::obj(vec![("result", Json::Str(format!("ok; next, guide the learner toward goal {}", next))), ("remaining", Json::Arr(left))]), Effect::Goal { index: i, all: false })
            }
        }
        "give_hint" => {
            let text = args.get("text").str().trim().to_string();
            if text.is_empty() {
                return err("text is needed".into());
            }
            let h = Json::obj(vec![("text", Json::Str(text)), ("reading", Json::Str(args.get("reading").str().trim().to_string())), ("translation", Json::Str(args.get("translation").str().trim().to_string()))]);
            (Json::obj(vec![("result", Json::Str("shown on the learner's screen".into()))]), Effect::Hint(h))
        }
        "end_scene" => {
            let success = args.get("success").as_bool().unwrap_or(true);
            let summary = args.get("summary").str().trim().to_string();
            (Json::obj(vec![("result", Json::Str("ok: say goodbye in one short sentence".into()))]), Effect::End { success, summary })
        }
        other => err(format!("unknown function {}", other)),
    }
}

// ---------------------------------------------------------------- reading the transcripts

#[derive(Clone, Copy, PartialEq, Debug)]
enum Script {
    Japanese,
    Han,
    Hangul,
    Cyrillic,
    Arabic,
    Devanagari,
    Thai,
    Greek,
    Hebrew,
}

fn script_of(lang: &str) -> Option<Script> {
    Some(match crate::util::lang_base(lang).as_str() {
        "ja" => Script::Japanese,
        "zh" => Script::Han,
        "ko" => Script::Hangul,
        "ru" | "uk" | "bg" | "sr" | "mk" | "be" => Script::Cyrillic,
        "ar" | "fa" | "ur" => Script::Arabic,
        "hi" | "mr" | "ne" => Script::Devanagari,
        "th" => Script::Thai,
        "el" => Script::Greek,
        "he" => Script::Hebrew,
        _ => return None,
    })
}

fn in_script(c: char, s: Script) -> bool {
    let u = c as u32;
    let han = (0x3400..=0x9FFF).contains(&u) || (0xF900..=0xFAFF).contains(&u);
    match s {
        Script::Japanese => han || (0x3040..=0x30FF).contains(&u) || (0xFF66..=0xFF9F).contains(&u),
        Script::Han => han,
        Script::Hangul => (0xAC00..=0xD7AF).contains(&u) || (0x1100..=0x11FF).contains(&u) || (0x3130..=0x318F).contains(&u),
        Script::Cyrillic => (0x0400..=0x04FF).contains(&u),
        Script::Arabic => (0x0600..=0x06FF).contains(&u),
        Script::Devanagari => (0x0900..=0x097F).contains(&u),
        Script::Thai => (0x0E00..=0x0E7F).contains(&u),
        Script::Greek => (0x0370..=0x03FF).contains(&u),
        Script::Hebrew => (0x0590..=0x05FF).contains(&u),
    }
}

/// Common short words that tell Latin-script languages apart (only words the others do not share).
fn common_words(lang: &str) -> &'static [&'static str] {
    match crate::util::lang_base(lang).as_str() {
        "en" => &["the", "and", "is", "are", "you", "what", "i", "it", "to", "of", "have", "this", "that", "my", "your", "with", "for", "not", "can", "do"],
        "pt" => &["você", "não", "é", "muito", "também", "eu", "meu", "minha", "isso", "um", "uma", "os", "obrigado", "obrigada", "sim", "mas", "tudo", "bem", "então", "aqui"],
        "es" => &["usted", "tú", "muy", "yo", "eso", "un", "una", "los", "las", "gracias", "sí", "el", "y", "pero", "bien", "aquí", "qué", "cómo", "hola", "ahora"],
        "fr" => &["le", "les", "est", "vous", "tu", "je", "ne", "pas", "une", "du", "et", "mais", "très", "avec", "pour", "oui", "bonjour", "merci", "c'est", "ça"],
        "de" => &["der", "die", "das", "ist", "und", "sie", "du", "ich", "nicht", "ein", "eine", "mit", "für", "auch", "aber", "sehr", "ja", "danke", "wie", "was"],
        "it" => &["il", "è", "che", "non", "di", "io", "mio", "con", "per", "ma", "molto", "anche", "grazie", "ciao", "sono", "questo", "come", "cosa", "bene", "gli"],
        _ => &[],
    }
}

fn share_in(text: &str, s: Script) -> Option<f64> {
    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() < 6 {
        return None;
    }
    Some(letters.iter().filter(|c| in_script(**c, s)).count() as f64 / letters.len() as f64)
}

/// Whether a text is in `lang` (Some(true)), clearly in another language such as `other` (Some(false)), or too short
/// or unclear to tell (None).
pub fn speaks(text: &str, lang: &str, other: &str) -> Option<bool> {
    if let Some(sc) = script_of(lang) {
        let share = share_in(text, sc)?;
        return if share >= 0.5 {
            Some(true)
        } else if share < 0.2 {
            Some(false)
        } else {
            None
        };
    }
    // a Latin-script language: against another script first, then by its common words
    if let Some(sc) = script_of(other) {
        if share_in(text, sc)? >= 0.5 {
            return Some(false);
        }
    }
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !(c.is_alphanumeric() || c == '\'')).filter(|w| !w.is_empty()).collect();
    let hits = |l: &str| words.iter().filter(|w| common_words(l).contains(w)).count();
    let (a, b) = (hits(lang), if crate::util::lang_base(other) == crate::util::lang_base(lang) { 0 } else { hits(other) });
    if a + b < 2 {
        None
    } else if b > a * 2 {
        Some(false)
    } else if a >= b {
        Some(true)
    } else {
        None
    }
}

/// How long a turn is, in words (Japanese and Chinese have no spaces: about 2.5 characters a word).
pub fn words(text: &str, lang: &str) -> usize {
    match script_of(lang) {
        Some(Script::Japanese) | Some(Script::Han) => {
            let n = text.chars().filter(|c| in_script(*c, Script::Japanese) || c.is_alphanumeric()).count();
            (n as f64 / 2.5).round() as usize
        }
        _ => text.split_whitespace().count(),
    }
}

/// The longest turn of the tutor that suits a level (None: no limit).
pub fn max_words(level: &str) -> Option<usize> {
    match level {
        "N5" | "A1" => Some(20),
        "N4" | "A2" => Some(28),
        "N3" | "B1" => Some(42),
        _ => None,
    }
}

/// Watches the transcripts and steers the tutor with short notes.
#[derive(Clone, Debug, Default)]
pub struct Coach {
    lang: String,
    ui: String,
    level: String,
    turn: u32,
    since_goal: u32,
    long_in_row: u32,
    ui_in_row: u32,
    last_note: Option<u32>,
    pub notes: u32,
}

impl Coach {
    pub fn new(lang: &str, ui: &str, level: &str) -> Coach {
        Coach { lang: lang.into(), ui: ui.into(), level: level.into(), ..Coach::default() }
    }

    pub fn goal_reached(&mut self) {
        self.since_goal = 0;
    }

    fn note(&mut self, text: String) -> Option<String> {
        self.last_note = Some(self.turn);
        self.notes += 1;
        Some(format!("{} {}", NOTE, text))
    }

    /// After a turn of the tutor (`learner`: what the learner said before it, `tutor`: what the tutor said): a note to
    /// steer it, or None. At most one note every two turns, except when the tutor left the language.
    pub fn after_turn(&mut self, learner: &str, tutor: &str, goals: &[Goal]) -> Option<String> {
        self.turn += 1;
        let spoke = !learner.trim().is_empty();
        if spoke {
            self.since_goal += 1;
        }
        let lang = lang_english(&self.lang);
        let ui = lang_english(&self.ui);
        if speaks(tutor, &self.lang, &self.ui) == Some(false) {
            return self.note(format!("You answered in another language. Speak {} only, at level {}; use {} only for a few words of help.", lang, self.level, ui));
        }
        match max_words(&self.level) {
            Some(max) if words(tutor, &self.lang) > max => self.long_in_row += 1,
            _ => self.long_in_row = 0,
        }
        if spoke && crate::util::lang_base(&self.ui) != crate::util::lang_base(&self.lang) {
            if speaks(learner, &self.lang, &self.ui) == Some(false) {
                self.ui_in_row += 1;
            } else {
                self.ui_in_row = 0;
            }
        }
        if let Some(t) = self.last_note {
            if self.turn < t + 2 {
                return None;
            }
        }
        if self.long_in_row >= 2 {
            self.long_in_row = 0;
            return self.note(format!("Your turns are too long for level {}: one or two short sentences with simple words, then one easy question.", self.level));
        }
        if self.ui_in_row >= 2 {
            self.ui_in_row = 0;
            return self.note(format!("The learner keeps speaking {}. Give them the {} sentence for what they want to say, slowly, and let them repeat it.", ui, lang));
        }
        if self.since_goal >= 4 {
            if let Some((n, text)) = next_goal(goals) {
                self.since_goal = 0;
                return self.note(format!("The conversation is drifting. Bring it back gently, in your role, so the learner can reach goal {}: {}.", n, text));
            }
        }
        None
    }

    /// The learner has been quiet for a while after the tutor spoke.
    pub fn idle_note(&mut self, goals: &[Goal]) -> String {
        self.notes += 1;
        self.last_note = Some(self.turn);
        match next_goal(goals) {
            Some((n, text)) => format!("{} The learner has been quiet for a while. Help them toward goal {} ({}): ask an easy question, or call give_hint with a sentence they could say.", NOTE, n, text),
            None => format!("{} The learner has been quiet for a while. Ask an easy question, or call give_hint with a sentence they could say.", NOTE),
        }
    }
}

/// Reading in kana, romaji and translation of a line of a spoken session (the written tutor gives them in its JSON
/// answer; the spoken one only speaks).
pub fn annotate_prompt(c: &Ctx, text: &str) -> String {
    let ui = lang_english(&c.ui);
    if crate::util::lang_base(&c.lang) == "ja" {
        format!(
            "For this Japanese sentence, answer with a JSON object only: {{\"reading\": \"the sentence written entirely in hiragana\", \"romaji\": \"the sentence in Hepburn romaji\", \"translation\": \"the sentence in {}\"}}. Keep any mistakes of the sentence as they are. Sentence: {:?}",
            ui, text
        )
    } else {
        format!("Translate this {} sentence into {}. Answer with a JSON object only: {{\"translation\": \"…\"}}. Sentence: {:?}", lang_english(&c.lang), ui, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kaiwa::Line;

    fn ctx(mode: &str) -> Ctx {
        Ctx { lang: "ja".into(), ui: "pt".into(), level: "N5".into(), hour: 20, mode: mode.into(), scene_id: "noodles".into(), ..Ctx::default() }
    }

    fn session(goals: &[&str]) -> Session {
        Session {
            id: 1,
            mode: "roleplay".into(),
            live: true,
            goals: goals.iter().map(|g| Goal { text: g.to_string(), ..Goal::default() }).collect(),
            lines: vec![Line { role: "assistant".into(), text: "いらっしゃいませ".into(), ..Line::default() }, Line { role: "user".into(), text: "ラーメンを一つください".into(), ..Line::default() }],
            ..Session::default()
        }
    }

    #[test]
    fn scene_goals_are_cut_into_steps() {
        assert_eq!(split_goal("Order a dish and a drink, ask what is recommended, and pay."), vec!["Order a dish and a drink", "Ask what is recommended", "Pay"]);
        assert_eq!(split_goal("Order a drink with a change (size, milk, sugar) and choose a cake to eat in or take away."), vec!["Order a drink with a change (size, milk, sugar) and choose a cake to eat in or take away"]);
        assert_eq!(split_goal("a, b, c, d, e, f").len(), 4);
        for s in kaiwa::SCENES {
            let g = goals_for("roleplay", s.id);
            assert!(!g.is_empty() && g.len() <= 4 && g.iter().all(|g| !g.text.is_empty() && !g.text.starts_with("And ")), "{}", s.id);
        }
        for l in kaiwa::LESSONS {
            assert!(!goals_for("lesson", l.id).is_empty());
        }
        assert_eq!(goals_for("call", "").len(), 3);
    }

    #[test]
    fn the_spoken_tutor_knows_the_learner_the_goals_and_the_tools() {
        let goals = goals_for("roleplay", "noodles");
        let s = live_system(&ctx("roleplay"), &goals);
        assert!(s.contains("Japanese") && s.contains("N5") && s.contains("noodle shop"));
        assert!(s.contains("not a native speaker") && s.contains("pronunciation generously"));
        assert!(s.contains("report_correction") && s.contains("mark_goal_done") && s.contains("give_hint") && s.contains("end_scene"));
        assert!(s.contains("1. Order a dish and a drink") && s.contains("3. Pay"));
        assert!(s.contains("[Kaiwa]") && s.contains("標準語"));
        // spoken: no JSON answer format
        assert!(!s.contains("ANSWER FORMAT") && !s.contains("\"reading\""));
        let t = tools(&ctx("roleplay"));
        let names: Vec<String> = t.items().iter().map(|f| f.get("name").str()).collect();
        assert_eq!(names, vec!["report_correction", "mark_goal_done", "give_hint", "end_scene"]);
        assert_eq!(t.at(0).path("parameters.type").str(), "OBJECT");
        assert!(t.at(0).path("parameters.properties.explanation.description").str().contains("Portuguese"));
        assert_eq!(t.at(1).path("parameters.properties.goal.type").str(), "INTEGER");
        assert!(t.at(2).path("parameters.properties.reading.description").str().contains("hiragana"));
        assert_eq!(bcp47("ja"), "ja-JP");
        assert_eq!(bcp47("pt-BR"), "pt-BR");
        assert_eq!(voice_for("ja", ""), "Kore");
        assert_eq!(voice_for("en", "Puck"), "Puck");
        assert!(opening("roleplay").starts_with(NOTE));
    }

    #[test]
    fn tool_calls_change_the_session() {
        let mut s = session(&["Order", "Ask", "Pay"]);
        // a correction goes under the learner's last line
        let (r, e) = apply_tool(&mut s, "report_correction", &Json::parse(r#"{"original":"ラーメンを一つください","corrected":"ラーメンを一つお願いします","explanation":"mais educado","kind":"politeness"}"#).unwrap(), 5);
        assert_eq!(r.get("result").str(), "saved: keep talking naturally");
        assert!(matches!(e, Effect::Correction(_)));
        let chk = &s.lines[1].check;
        assert_eq!(chk.get("ok").as_bool(), Some(false));
        assert_eq!(chk.path("errors.0.right").str(), "ラーメンを一つお願いします");
        assert_eq!(chk.path("errors.0.kind").str(), "politeness");
        // the same correction twice is kept once
        apply_tool(&mut s, "report_correction", &Json::parse(r#"{"original":"ラーメンを一つください","corrected":"ラーメンを一つお願いします","explanation":"mais educado","kind":"politeness"}"#).unwrap(), 5);
        assert_eq!(s.lines[1].check.get("errors").items().len(), 1);
        assert!(apply_tool(&mut s, "report_correction", &Json::parse(r#"{"original":"","corrected":"x"}"#).unwrap(), 5).0.get("error").str().contains("needed"));
        // goals
        let (r, e) = apply_tool(&mut s, "mark_goal_done", &Json::parse(r#"{"goal":1}"#).unwrap(), 6);
        assert_eq!(e, Effect::Goal { index: 0, all: false });
        assert!(r.get("result").str().contains("2. Ask"));
        assert!(s.goals[0].done && s.goals[0].at == 6 && s.score == GOAL_POINTS);
        assert_eq!(apply_tool(&mut s, "mark_goal_done", &Json::parse(r#"{"goal":1}"#).unwrap(), 7).0.get("result").str(), "already reached");
        assert!(apply_tool(&mut s, "mark_goal_done", &Json::parse(r#"{"goal":9}"#).unwrap(), 7).0.get("error").str().contains("1 to 3"));
        apply_tool(&mut s, "mark_goal_done", &Json::parse(r#"{"goal":"2"}"#).unwrap(), 8);
        let (r, e) = apply_tool(&mut s, "mark_goal_done", &Json::parse(r#"{"goal":3}"#).unwrap(), 9);
        assert_eq!(e, Effect::Goal { index: 2, all: true });
        assert!(r.get("result").str().contains("end_scene"));
        assert_eq!(s.score, 3 * GOAL_POINTS + ALL_GOALS_BONUS);
        // hint and end
        let (_, e) = apply_tool(&mut s, "give_hint", &Json::parse(r#"{"text":"お会計お願いします","reading":"おかいけいおねがいします","translation":"A conta, por favor"}"#).unwrap(), 10);
        assert!(matches!(e, Effect::Hint(h) if h.get("reading").str() == "おかいけいおねがいします"));
        let (_, e) = apply_tool(&mut s, "end_scene", &Json::parse(r#"{"success":true,"summary":"Muito bem!"}"#).unwrap(), 11);
        assert_eq!(e, Effect::End { success: true, summary: "Muito bem!".into() });
        assert!(apply_tool(&mut s, "fly", &Json::Null, 1).0.get("error").str().contains("unknown"));
        // the session keeps it all on disk
        let store = kaiwa::Store { sessions: vec![s.clone()], ..kaiwa::Store::default() };
        let back = kaiwa::Store::from_json(&store.to_json()).sessions.remove(0);
        assert_eq!(back.goals, s.goals);
        assert_eq!((back.score, back.live), (s.score, true));
    }

    #[test]
    fn languages_are_told_apart() {
        assert_eq!(speaks("こんにちは、今日は何を食べますか", "ja", "pt"), Some(true));
        assert_eq!(speaks("Olá, tudo bem? O que você quer comer?", "ja", "pt"), Some(false));
        assert_eq!(speaks("hai", "ja", "pt"), None);
        assert_eq!(speaks("Hello, what do you want to eat today?", "en", "pt"), Some(true));
        assert_eq!(speaks("Eu não sei, você pode repetir isso?", "en", "pt"), Some(false));
        assert_eq!(speaks("Yo no sé, ¿puedes repetir eso, por favor? Muy bien", "es", "pt"), Some(true));
        assert_eq!(speaks("これは何ですか、教えてください", "en", "ja"), Some(false));
        assert_eq!(words("今日は何を食べましたか", "ja"), 4);
        assert_eq!(words("I went to the park", "en"), 5);
    }

    #[test]
    fn the_coach_steers_drift_level_and_stalled_goals() {
        let goals = goals_for("roleplay", "noodles");
        let mut c = Coach::new("ja", "pt", "N5");
        // a short Japanese turn: nothing to say
        assert_eq!(c.after_turn("ラーメンください", "はい、ラーメンですね。お飲み物は？", &goals), None);
        // the tutor switched to Portuguese: at once
        let n = c.after_turn("えっと", "Claro! Você quer uma bebida com o lámen? Temos chá e suco.", &goals).unwrap();
        assert!(n.starts_with(NOTE) && n.contains("Speak Japanese only"));
        // too long twice in a row (and not right after a note)
        let long = "いらっしゃいませ、今日はとても暑いですね、冷たいお茶とビールとジュースがありますが、どれにしますか、それとも温かいスープもありますよ、おすすめは特製の味噌ラーメンです";
        assert_eq!(c.after_turn("はい", long, &goals), None);
        let n = c.after_turn("はい", long, &goals).unwrap();
        assert!(n.contains("too long for level N5"));
        // the learner keeps speaking Portuguese
        let mut c = Coach::new("ja", "pt", "N5");
        assert_eq!(c.after_turn("Eu não sei o que pedir aqui", "だいじょうぶですよ。", &goals), None);
        let n = c.after_turn("Não entendi, você pode repetir?", "もう一度言いますね。", &goals).unwrap();
        assert!(n.contains("keeps speaking Portuguese"));
        // four turns of the learner without a goal
        let mut c = Coach::new("ja", "pt", "N5");
        for _ in 0..3 {
            assert_eq!(c.after_turn("はい", "そうですね。", &goals), None);
        }
        let n = c.after_turn("はい", "そうですね。", &goals).unwrap();
        assert!(n.contains("goal 1") && n.contains("Order a dish"));
        c.goal_reached();
        assert_eq!(c.after_turn("はい", "そうですね。", &goals), None);
        assert!(c.idle_note(&goals).contains("quiet"));
        assert_eq!(c.notes, 2);
    }

    #[test]
    fn notes_and_prompts() {
        let s = session(&["Order"]);
        let r = recap_note(&s);
        assert!(r.starts_with(NOTE) && r.contains("Learner: ラーメンを一つください") && r.contains("without greeting"));
        assert!(hint_note().contains("give_hint"));
        let a = annotate_prompt(&ctx("call"), "ラーメン");
        assert!(a.contains("hiragana") && a.contains("romaji") && a.contains("Portuguese"));
        let mut e = ctx("call");
        e.lang = "en".into();
        assert!(!annotate_prompt(&e, "hi").contains("romaji"));
    }
}
