//! Conversation (会話) in the Study mode: a tutor who talks with the learner in the language they study, at their
//! level. Free chat, scenes to act (a ramen shop, a job interview…), scenes the learner writes, short guided lessons
//! and reading stories with questions. Each message of the learner is checked (corrections explained in the language
//! of the interface, mistakes remembered), a hint gives three replies to choose from, a click on a word explains it
//! and can make a flashcard, and the end of a session writes a report. The model is the one of `llm` (Claude, Gemini,
//! OpenAI-compatible or Ollama on this computer).
//!
//! Written for Reelhouse after the features of Kaiwa (github.com/yeshsanchez/Kaiwa); the scenes, lessons and
//! instructions here are our own, and work for any language studied (Japanese also gets kana readings and romaji).
use crate::json::Json;
use crate::llm::Turn;

// ---------------------------------------------------------------- scenes and lessons

/// A scene to act: where it happens, who the tutor plays, who the learner plays, and what the learner should get done.
#[derive(Clone, Copy, Debug)]
pub struct Scene {
    pub id: &'static str,
    pub icon: &'static str,
    pub cat: &'static str,
    pub title: &'static str,
    pub setting: &'static str,
    pub ai_role: &'static str,
    pub user_role: &'static str,
    pub goal: &'static str,
}

const fn sc(id: &'static str, icon: &'static str, cat: &'static str, title: &'static str, setting: &'static str, ai_role: &'static str, user_role: &'static str, goal: &'static str) -> Scene {
    Scene { id, icon, cat, title, setting, ai_role, user_role, goal }
}

pub const SCENES: &[Scene] = &[
    sc("noodles", "🍜", "Food & drink", "Noodle shop", "A small busy noodle shop at lunchtime, with a counter and a menu on the wall.", "the cook who also takes the orders", "a hungry customer", "Order a dish and a drink, ask what is recommended, and pay."),
    sc("cafe", "☕", "Food & drink", "Café", "A cosy café with cakes in a glass case.", "the barista", "a customer", "Order a drink with a change (size, milk, sugar) and choose a cake to eat in or take away."),
    sc("bar", "🏮", "Food & drink", "Evening bar with friends", "A lively little bar after work; small dishes to share.", "a friendly coworker", "a coworker who just joined the team", "Order together, make small talk about work and hobbies, and suggest meeting again."),
    sc("restaurant", "🍽️", "Food & drink", "Restaurant with an allergy", "A family restaurant; the menu has photos.", "the waiter", "a customer with a food allergy", "Explain the allergy, ask what is safe to eat, order, and ask for the bill."),
    sc("market", "🍎", "Shopping", "Market stall", "An open-air market with fruit and vegetables.", "the stall keeper, chatty and proud of the produce", "a shopper", "Ask prices, buy a few things in the right amounts, and ask how to prepare one of them."),
    sc("clothes", "👕", "Shopping", "Clothes shop", "A clothes shop in a mall.", "the shop assistant", "a shopper", "Ask for another size or colour, try something on, and decide whether to buy it."),
    sc("electronics", "📱", "Shopping", "Phone shop", "An electronics store.", "the salesperson", "someone who needs a new phone", "Explain what you need, compare two options, and ask about the price and the guarantee."),
    sc("return", "🧾", "Shopping", "Returning an item", "The service desk of a department store.", "the clerk at the desk", "a customer with a broken product and the receipt", "Explain the problem and get a refund or an exchange."),
    sc("station", "🚉", "Travel", "Train ticket", "The ticket office of a big train station.", "the station clerk", "a traveller", "Buy a ticket to another city, ask the time, the platform and whether a seat can be reserved."),
    sc("directions", "🗺️", "Travel", "Asking the way", "A street corner in an unfamiliar neighbourhood.", "a kind passer-by who lives nearby", "a lost visitor", "Find out how to get to a place, how long it takes, and repeat the directions back."),
    sc("hotel", "🏨", "Travel", "Hotel check-in", "The front desk of a traditional inn.", "the receptionist", "a guest with a reservation", "Check in, ask about breakfast and the bath times, and ask for a recommendation nearby."),
    sc("airport", "✈️", "Travel", "Airport check-in", "An airline counter at the airport.", "the airline staff", "a passenger", "Check in, choose a seat, ask about baggage, and find out the gate."),
    sc("taxi", "🚕", "Travel", "Taxi ride", "Inside a taxi.", "the taxi driver, who likes to chat", "a passenger", "Say where to go, chat a little on the way, and pay."),
    sc("police", "👮", "Emergencies", "Lost wallet", "A small neighbourhood police box.", "the police officer", "someone who lost a wallet", "Describe what was lost, where and when, and leave contact details."),
    sc("doctor", "🩺", "Emergencies", "At the doctor", "A clinic consultation room.", "the doctor", "a patient who feels sick", "Describe the symptoms and since when, understand the advice, and ask about the medicine."),
    sc("pharmacy", "💊", "Emergencies", "Pharmacy", "A pharmacy counter.", "the pharmacist", "a customer with a headache and a cold", "Explain the symptoms, get a suitable medicine, and understand how to take it."),
    sc("neighbour", "🏠", "Daily life", "New neighbour", "The hallway of an apartment building.", "the neighbour next door", "someone who just moved in", "Introduce yourself, give a small gift, and ask about the rubbish days."),
    sc("bank", "🏦", "Daily life", "Opening an account", "A bank branch.", "the bank clerk", "a new customer", "Open an account: say what you need, answer the questions, and ask about a card."),
    sc("post", "📮", "Daily life", "Post office", "A post office counter.", "the post office clerk", "a customer with a parcel", "Send a parcel abroad, choose how fast, and ask the price."),
    sc("haircut", "💇", "Daily life", "Hair salon", "A hair salon.", "the hairdresser", "a customer", "Explain how you want your hair cut, and chat during the cut."),
    sc("interview", "💼", "Work & school", "Job interview", "An office meeting room.", "the interviewer, polite and formal", "a candidate", "Introduce yourself formally, talk about your experience and strengths, and ask a question at the end."),
    sc("office", "🖥️", "Work & school", "First day at work", "An office, first morning.", "the team leader", "a new employee", "Introduce yourself to the team, ask what to do first, and ask for help politely."),
    sc("class", "🏫", "Work & school", "Talking to a teacher", "A classroom after the lesson.", "the teacher", "a student", "Ask about something you did not understand and ask for an extension on homework."),
    sc("phone", "📞", "Work & school", "Booking by phone", "A phone call to a clinic.", "the receptionist on the phone", "a caller", "Book an appointment: give your name, find a time that works for both, and confirm it."),
    sc("party", "🎉", "Social", "Party small talk", "A friend's birthday party.", "a guest you have never met", "another guest", "Introduce yourself, find something in common, and exchange contacts."),
    sc("date", "🌸", "Social", "Making weekend plans", "Messages with a friend on a Friday.", "a close friend", "the learner, as themself", "Decide together what to do this weekend, where and when to meet."),
    sc("hobby", "🎮", "Social", "Club of a hobby", "The first meeting of a hobby club (games, anime, music).", "the club leader", "a newcomer", "Say what you like, why, and ask how the club works."),
    sc("festival", "🎆", "Social", "Summer festival", "A summer festival with food stalls and fireworks.", "a friend wearing festival clothes", "a friend", "Choose stalls to visit, try a game, and find a good place for the fireworks."),
];

/// A short lesson taught through the conversation, one step at a time.
#[derive(Clone, Copy, Debug)]
pub struct Lesson {
    pub id: &'static str,
    pub icon: &'static str,
    pub title: &'static str,
    pub goals: &'static str,
}

pub const LESSONS: &[Lesson] = &[
    Lesson { id: "l-greet", icon: "👋", title: "Greetings", goals: "Greet at different times of the day, say goodbye, thank and apologise." },
    Lesson { id: "l-self", icon: "🙋", title: "Introducing yourself", goals: "Say your name, where you are from, what you do, and say it is nice to meet someone." },
    Lesson { id: "l-numbers", icon: "🔢", title: "Numbers and prices", goals: "Count, ask how much something costs and understand the answer." },
    Lesson { id: "l-time", icon: "⏰", title: "Time and dates", goals: "Tell the time, the days of the week and dates; ask when something happens." },
    Lesson { id: "l-likes", icon: "❤️", title: "Likes and dislikes", goals: "Say what you like, love and dislike, and ask others about it." },
    Lesson { id: "l-past", icon: "📅", title: "Talking about the past", goals: "Tell what you did yesterday or last weekend with the past tense." },
    Lesson { id: "l-plans", icon: "🗓️", title: "Plans and invitations", goals: "Invite someone, accept or refuse politely, and suggest another time." },
    Lesson { id: "l-requests", icon: "🙏", title: "Polite requests", goals: "Ask someone to do something and ask for permission, politely." },
    Lesson { id: "l-describe", icon: "🖼️", title: "Describing things and people", goals: "Describe looks, character and things with adjectives; compare two things." },
    Lesson { id: "l-formal", icon: "🎩", title: "Formal speech", goals: "Switch between casual and formal speech (for Japanese, polite and honorific forms), and know when to use each." },
];

pub fn scene(id: &str) -> Option<&'static Scene> {
    SCENES.iter().find(|s| s.id == id)
}

pub fn lesson(id: &str) -> Option<&'static Lesson> {
    LESSONS.iter().find(|l| l.id == id)
}

// ---------------------------------------------------------------- levels and languages

/// The levels of a language: JLPT for Japanese, CEFR for the others.
pub fn levels(lang: &str) -> &'static [&'static str] {
    if lang == "ja" {
        &["N5", "N4", "N3", "N2", "N1"]
    } else {
        &["A1", "A2", "B1", "B2", "C1", "C2"]
    }
}

/// The level saved, or the first one of the language when it belongs to another scale.
pub fn level_for(lang: &str, saved: &str) -> String {
    let ls = levels(lang);
    if ls.contains(&saved) {
        saved.to_string()
    } else {
        ls[0].to_string()
    }
}

fn level_guide(level: &str) -> &'static str {
    match level {
        "N5" | "A1" => "Absolute beginner: very short, simple sentences (about 5-10 words), the most common everyday words, present and past, polite forms. No idioms.",
        "N4" | "A2" => "Beginner: simple sentences with basic connectors, common words only, one idea per sentence.",
        "N3" | "B1" => "Intermediate: natural everyday language with compound sentences; avoid rare words, literary forms and heavy formal speech.",
        "N2" | "B2" => "Upper intermediate: speak naturally as natives do every day, casual forms and common idioms included; now and then a slightly harder word.",
        "C1" => "Advanced: fully natural speech with nuance and idioms; simplify only when asked.",
        _ => "Near native: speak exactly as a native speaker would, with nuance, idioms and register; never simplify.",
    }
}

/// The English name of a language, for the instructions (it lives in `llm`, shared with the X-Ray and the
/// translators, which also exist where Conversation does not).
pub use crate::llm::lang_english;

// ---------------------------------------------------------------- what the tutor is told

/// Everything the instruction of the tutor depends on.
#[derive(Clone, Debug, Default)]
pub struct Ctx {
    /// the language studied ("ja") and the language of the interface ("pt")
    pub lang: String,
    pub ui: String,
    pub level: String,
    pub name: String,
    pub interests: String,
    /// local hour, for greetings that fit the time of day
    pub hour: u32,
    pub mode: String,
    pub scene_id: String,
    /// a scene the learner wrote (mode "custom"): setting, the tutor's role, the learner's role
    pub custom: [String; 3],
    /// story mode: the topic, and for Japanese the script ("hiragana", "katakana", "normal", "stretch")
    pub topic: String,
    pub script: String,
    pub mistakes: Vec<(String, String)>,
    pub words: Vec<String>,
}

fn time_of_day(h: u32) -> &'static str {
    match h {
        5..=10 => "morning",
        11..=16 => "afternoon",
        17..=21 => "evening",
        _ => "late at night",
    }
}

/// The shape every answer of the tutor has: the reply and what the learner's toggles show under it.
fn reply_format(c: &Ctx) -> String {
    let ui = lang_english(&c.ui);
    if c.lang == "ja" {
        format!(
            "ANSWER FORMAT: a JSON object only, no other text: {{\"reply\": \"your message in Japanese\", \"reading\": \"the same message written entirely in hiragana\", \"romaji\": \"the same message in Hepburn romaji\", \"translation\": \"the same message in {}\"}}.",
            ui
        )
    } else {
        format!("ANSWER FORMAT: a JSON object only, no other text: {{\"reply\": \"your message in {}\", \"translation\": \"the same message in {}\"}}.", lang_english(&c.lang), ui)
    }
}

/// Sessions spoken aloud: the tutor's voice always reads its turns.
pub fn spoken(mode: &str) -> bool {
    mode == "call"
}

/// The instruction of the tutor for a session.
pub fn tutor_system(c: &Ctx) -> String {
    let mut p = tutor_body(c, false);
    p.push(String::new());
    p.push(reply_format(c));
    p.join("\n")
}

/// The instruction without the answer format, shared by the written tutor and the spoken one (Gemini Live,
/// `kaiwa_live::live_system`): who the tutor is, the level, the rules and the mode (scene, lesson, call…).
pub fn tutor_body(c: &Ctx, voice: bool) -> Vec<String> {
    let lang = lang_english(&c.lang);
    let mut p: Vec<String> = vec![
        format!("You are the conversation partner and tutor of a person learning {} in a study app. You are warm, patient, curious and encouraging.", lang),
        format!("The learner is called {}.", if c.name.trim().is_empty() { "the learner" } else { c.name.trim() }),
        format!("It is {} for the learner now: a greeting must fit that. Greet only in your first {}; after that just keep the conversation going.", time_of_day(c.hour), if voice { "turn" } else { "message" }),
        format!("LEARNER'S LEVEL ({}): {}", c.level, level_guide(&c.level)),
        String::new(),
        "RULES:".into(),
        format!("- {} {}. Keep each turn short: one to three sentences, usually ending with an easy question that keeps the talk going.", if voice { "Speak" } else { "Write in" }, lang),
        "- Stay at the learner's level all the time. No long paragraphs, no lists.".into(),
        "- When the learner makes a mistake, do not lecture: use the correct form naturally in your answer, then carry on. (Corrections are shown to them separately.)".into(),
        format!("- If the learner {} in another language, give them a simple {} sentence they could have said, then continue.", if voice { "speaks" } else { "writes" }, lang),
        "- If they seem stuck, offer an easier question or a choice between two answers.".into(),
        "- Answer what they actually said; be interested in them. At most one short reaction word per turn, and vary it.".into(),
        "- Sound like a real person in everyday life, not a textbook. Never say you are an AI.".into(),
    ];
    if !c.interests.trim().is_empty() {
        p.push(format!("- The learner likes: {}. Bring examples from there when it fits.", c.interests.trim()));
    }
    p.push(String::new());
    match c.mode.as_str() {
        "roleplay" | "custom" => {
            let (setting, ai, user, goal) = match (c.mode.as_str(), scene(&c.scene_id)) {
                ("roleplay", Some(s)) => (s.setting.to_string(), s.ai_role.to_string(), s.user_role.to_string(), s.goal.to_string()),
                _ => (c.custom[0].clone(), c.custom[1].clone(), c.custom[2].clone(), "Play the scene the learner described.".to_string()),
            };
            p.push("ROLE-PLAY: act the scene and stay in it.".into());
            p.push(format!("- Place and situation: {}", setting));
            p.push(format!("- You are: {}", ai));
            p.push(format!("- The learner is: {}", user));
            p.push(format!("- What the learner should manage to do: {}", goal));
            p.push("- Open the scene with a natural first line for your role. Keep the language at the learner's level.".into());
            p.push("- If the learner wanders off, answer briefly in character and bring the scene back to its goal. When the goal is reached, close the scene warmly in character.".into());
        }
        "lesson" => {
            let l = lesson(&c.scene_id);
            p.push("GUIDED LESSON through conversation, one small step per turn:".into());
            p.push(format!("- Topic: {}", l.map(|l| l.title).unwrap_or("conversation")));
            p.push(format!("- By the end the learner can: {}", l.map(|l| l.goals).unwrap_or("")));
            p.push(format!("- Pick the {} words and patterns that fit this topic and the learner's level.", lang));
            p.push("- Steps: introduce the topic in one sentence; show one useful sentence with a tiny example; ask the learner to try it; react and move to the next item; after a few items, a short role-play using everything; end with a one-line summary of what they can now say.".into());
            p.push("- Never give the whole lesson at once. Wait for the learner at every step.".into());
        }
        "story" => {
            p.push("READING STORY, then questions:".into());
            p.push("- Exception to the short-turn rule: your first message is a short story with a title on the first line, and nothing else but one closing line inviting the learner to say when they have read it. No greeting, no translation, no word list.".into());
            p.push(if c.topic.trim().is_empty() { "- Choose a pleasant topic, ideally close to what the learner likes.".to_string() } else { format!("- Topic of the story: {}", c.topic.trim()) });
            p.push("- Length: beginners 5-7 short sentences; intermediate 8-12; advanced 12-15 richer sentences.".into());
            if c.lang == "ja" {
                p.push(match c.script.as_str() {
                    "hiragana" => "- Write the whole story in hiragana only: no kanji and no katakana at all; avoid loanwords.".to_string(),
                    "katakana" => "- Fill the story with loanwords written in katakana (food, travel, technology, music): this is katakana practice.".to_string(),
                    "stretch" => "- Use kanji a little above the learner's level (useful ones they should learn next); grammar and words stay at their level.".to_string(),
                    _ => "- Use kanji normally for the learner's level.".to_string(),
                });
            }
            p.push("- After the learner says they read it, ask three questions about the story, ONE per message, each answerable from the text. A question message holds only the question: no answer, no hint, no translation in brackets.".into());
            p.push("- After each answer: one short sentence saying if it was right (give the right answer only when it was wrong), then the next question on a new line. After the third: congratulate, point out two or three useful words of the story, and offer to talk about it.".into());
        }
        "call" => {
            if voice {
                p.push("VOICE CALL: you are on the phone with the learner.".into());
            } else {
                p.push("VOICE CALL: you are on the phone with the learner; everything you write is read aloud by a voice.".into());
            }
            p.push("- Talk like a friend on the phone: very short turns, one or two short sentences, then one simple question.".into());
            p.push("- Plain spoken words only: no emoji, no lists, no brackets, no stage directions; they would be read aloud.".into());
            if !voice {
                p.push("- The learner's words come from speech recognition and may be wrong in places: guess what they meant, or kindly ask them to say it again.".into());
            }
            p.push("- With no topic, ask about their day, their plans or what they like.".into());
        }
        _ => {
            p.push("FREE CONVERSATION: talk about whatever the learner wants. With no topic, ask about their day, their plans or what they like.".into());
        }
    }
    if !c.mistakes.is_empty() {
        let m: Vec<String> = c.mistakes.iter().take(5).map(|(w, r)| format!("\"{}\" → \"{}\"", w, r)).collect();
        p.push(String::new());
        p.push(format!("REMEMBER (do not mention it): the learner recently got these wrong: {}. Give them natural chances to use these forms again.", m.join("; ")));
    }
    if !c.words.is_empty() {
        p.push(format!("REMEMBER (do not mention it): the learner recently learned: {}. Reuse them naturally.", c.words.iter().take(10).cloned().collect::<Vec<_>>().join(", ")));
    }
    p
}

/// The check of one message of the learner.
pub fn check_prompt(c: &Ctx, text: &str) -> String {
    let lang = lang_english(&c.lang);
    let ui = lang_english(&c.ui);
    format!(
        "You check messages written by a {lang} learner (level {level}) in a conversation. Message: {text:?}\n\n\
         Answer with a JSON object only:\n\
         {{\"ok\": true or false, \"corrected\": \"the whole message as a native would write it (the same message when it was right)\", \
         \"errors\": [{{\"wrong\": \"the wrong piece\", \"right\": \"the right piece\", \"why\": \"one short sentence in {ui}\", \"kind\": \"grammar|particle|verb|word|order|politeness|spelling|other\"}}], \
         \"praise\": \"one short encouraging sentence in {ui} about what they did well\"}}\n\n\
         Rules: flag only real mistakes, not style; casual speech is fine when it is consistent; at most 3 errors, the most important first. \
         Writing a word in a harder script that is correct (for Japanese, a right kanji above the level) is never a mistake: praise it instead. \
         If the message is not in {lang}, set ok to true, no errors, and use praise to gently invite them to try it in {lang}. \
         \"why\" and \"praise\" are written in {ui}; quote {lang} pieces inside them when needed.",
        lang = lang,
        level = c.level,
        text = text,
        ui = ui
    )
}

fn transcript(lines: &[Line], last: usize) -> String {
    let from = lines.len().saturating_sub(last);
    lines[from..].iter().map(|l| format!("{}: {}", if l.role == "assistant" { "Tutor" } else { "Learner" }, l.text)).collect::<Vec<_>>().join("\n")
}

/// Three replies the learner could send next.
pub fn hint_prompt(c: &Ctx, lines: &[Line]) -> String {
    format!(
        "A {lang} learner (level {level}: {guide}) is stuck in this conversation; they speak next.\n\n{t}\n\n\
         Suggest three different natural replies at their level: one simple and safe, one that asks something back, one a little more expressive. \
         Answer with a JSON object only: {{\"hints\": [{{\"text\": \"the reply in {lang}\", \"reading\": \"{reading}\", \"translation\": \"in {ui}\"}}]}}",
        lang = lang_english(&c.lang),
        level = c.level,
        guide = level_guide(&c.level),
        t = transcript(lines, 6),
        reading = if c.lang == "ja" { "the reply in hiragana" } else { "" },
        ui = lang_english(&c.ui)
    )
}

/// A word of the conversation, explained.
pub fn word_prompt(c: &Ctx, word: &str, sentence: &str) -> String {
    format!(
        "You are a {lang}–{ui} dictionary for learners. Explain the {lang} word {word:?} as it is used in: {sentence:?}\n\
         Answer with a JSON object only: {{\"word\": \"the dictionary form\", \"reading\": \"{reading}\", \"meaning\": \"short meanings in {ui}, comma separated\", \
         \"note\": \"one short note on use or nuance in {ui}, or empty\", \"example\": \"one new simple example sentence in {lang}\", \"example_tr\": \"the example in {ui}\"}}",
        lang = lang_english(&c.lang),
        ui = lang_english(&c.ui),
        word = word,
        sentence = sentence,
        reading = if c.lang == "ja" { "the reading in hiragana" } else { "the pronunciation, only if it is not obvious, else empty" }
    )
}

/// The report at the end of a session.
pub fn report_prompt(c: &Ctx, s: &Session) -> String {
    let fixes: Vec<String> = s
        .lines
        .iter()
        .filter_map(|l| {
            let e = l.check.get("errors").items();
            if e.is_empty() {
                None
            } else {
                Some(e.iter().map(|x| format!("\"{}\" → \"{}\"", x.get("wrong").str(), x.get("right").str())).collect::<Vec<_>>().join("; "))
            }
        })
        .collect();
    let goals = if s.goals.is_empty() {
        String::new()
    } else {
        format!("\n\nGoals of the scene: {}", s.goals.iter().map(|g| format!("{} ({})", g.text, if g.done { "reached" } else { "not reached" })).collect::<Vec<_>>().join("; "))
    };
    format!(
        "You are a {lang} tutor writing a short report for a learner (level {level}) after a conversation.\n\nConversation:\n{t}\n\nCorrections:\n{f}{goals}\n\n\
         Answer with a JSON object only: {{\"summary\": \"two or three encouraging sentences in {ui}\", \"strengths\": [\"short point in {ui}\"], \
         \"improve\": [\"short, concrete point in {ui}\"], \"words\": [{{\"word\": \"a useful {lang} word from the conversation\", \"reading\": \"{reading}\", \"meaning\": \"in {ui}\"}}]}}. \
         At most 3 strengths, 3 points to improve and 4 words.",
        lang = lang_english(&c.lang),
        level = c.level,
        t = transcript(&s.lines, 40),
        f = if fixes.is_empty() { "(none)".to_string() } else { fixes.join("\n") },
        goals = goals,
        ui = lang_english(&c.ui),
        reading = if c.lang == "ja" { "in hiragana" } else { "" }
    )
}

/// The turns sent to the model: the conversation so far, the tutor's turns as the plain reply.
pub fn turns(s: &Session) -> Vec<Turn> {
    let from = s.lines.len().saturating_sub(24);
    s.lines[from..].iter().map(|l| Turn::new(if l.role == "assistant" { "assistant" } else { "user" }, &l.text)).collect()
}

/// The keys small local models use for their message when they do not follow the format exactly.
const REPLY_KEYS: &[&str] = &["reply", "message", "response", "answer", "text", "content", "tutor", "assistant"];

/// The value of `"key": "…"` in text that is not whole JSON (a model cut short, or one that wrote text around it).
fn loose_field(t: &str, key: &str) -> Option<String> {
    let at = t.find(&format!("\"{}\"", key))? + key.len() + 2;
    let rest = t[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut esc = false;
    for c in rest.chars() {
        match (esc, c) {
            (true, 'n') => out.push('\n'),
            (true, c) => out.push(c),
            (false, '\\') => {
                esc = true;
                continue;
            }
            (false, '"') => return Some(out),
            (false, c) => out.push(c),
        }
        esc = false;
    }
    // cut short: what came is still the message
    Some(out)
}

/// A line of the tutor from the model's answer: a JSON object (its reply under any of the usual keys), or plain text
/// when the model ignored the format. None when the answer holds no message at all: empty, "{}" (what a small model
/// such as qwen2.5:3b gives under Ollama's JSON mode when it is lost), or an object without any text.
pub fn reply_of(answer: &str, now: i64) -> Option<Line> {
    let t = answer.trim();
    let mk = |text: String, j: Option<&Json>| Line {
        role: "assistant".into(),
        text: text.trim().to_string(),
        reading: j.map(|j| j.get("reading").str().trim().to_string()).unwrap_or_default(),
        romaji: j.map(|j| j.get("romaji").str().trim().to_string()).unwrap_or_default(),
        translation: j.map(|j| j.get("translation").str().trim().to_string()).unwrap_or_default(),
        check: Json::Null,
        at: now,
    };
    if let Some(j) = crate::llm::json_of(t) {
        let mut text = REPLY_KEYS.iter().map(|k| j.get(k).str()).find(|s| !s.trim().is_empty()).unwrap_or_default();
        if text.trim().is_empty() {
            // any other text in it, but not the reading, romaji or translation of a missing reply
            if let Json::Obj(o) = &j {
                text = o.iter().filter(|(k, _)| !matches!(k.as_str(), "reading" | "romaji" | "translation")).find_map(|(_, v)| v.as_str().filter(|s| !s.trim().is_empty()).map(String::from)).unwrap_or_default();
            }
        }
        return if text.trim().is_empty() { None } else { Some(mk(text, Some(&j))) };
    }
    // not whole JSON: the reply inside broken JSON, or the plain text without code fences
    if t.starts_with('{') || t.starts_with("```") {
        if let Some(r) = REPLY_KEYS.iter().find_map(|k| loose_field(t, k)).filter(|r| !r.trim().is_empty()) {
            return Some(mk(r, None));
        }
    }
    let plain = t.trim_matches('`').trim();
    let plain = plain.strip_prefix("json").map(str::trim).unwrap_or(plain);
    if plain.chars().all(|c| matches!(c, '{' | '}' | '[' | ']' | '"' | ':' | ',') || c.is_whitespace()) {
        return None;
    }
    Some(mk(plain.to_string(), None))
}

/// A line of the tutor from the model's answer (see `reply_of`); an answer without a message gives an empty line.
pub fn tutor_line(answer: &str, now: i64) -> Line {
    reply_of(answer, now).unwrap_or(Line { role: "assistant".into(), at: now, ..Line::default() })
}

/// Said to the model when its first answer had no message: once more, in plain text.
pub const PLAIN_AGAIN: &str = "IMPORTANT: your last answer was empty. Answer now with your next message as plain text only: no JSON, no braces, no quotes, just what you say to the learner.";

/// The tutor's errors as the learner sees them: one sentence of the interface (translated by the page) and what to do.
pub const ERR_EMPTY: &str = "The tutor answered with nothing twice. Press \"Try again\"; if it keeps happening, choose another model for the tutor (Conversation > Tutor), or use Gemini with a free key.";
pub const ERR_TIMEOUT: &str = "The tutor took too long to answer. A local model can be slow on this computer: press \"Try again\", choose a smaller model, or use Gemini with a free key.";
pub const ERR_OLLAMA_DOWN: &str = "The local AI (Ollama) is not answering. Use \"Download and turn on the local AI\" in Conversation, then try again.";
pub const ERR_OLLAMA_MODEL: &str = "Ollama does not have this model yet. Use \"Download and turn on the local AI\" in Conversation.";

/// The sentence shown for an error of the tutor's call (`raw`, the error itself, goes to the detail line).
pub fn explain_error(raw: &str) -> &'static str {
    let low = raw.to_lowercase();
    if low.contains("timed out") || low.contains("timeout") || low.contains("deadline") {
        ERR_TIMEOUT
    } else if raw.starts_with("The local AI (Ollama) is not answering") {
        ERR_OLLAMA_DOWN
    } else if raw.starts_with("Ollama does not have the model") {
        ERR_OLLAMA_MODEL
    } else {
        ""
    }
}

// ---------------------------------------------------------------- what is kept

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    /// "user" or "assistant"
    pub role: String,
    pub text: String,
    pub reading: String,
    pub romaji: String,
    pub translation: String,
    /// the check of a learner's line (`check_prompt`), Null until it comes
    pub check: Json,
    pub at: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub id: i64,
    /// "chat", "call", "roleplay", "custom", "lesson", "story"
    pub mode: String,
    pub scene: String,
    pub title: String,
    pub custom: [String; 3],
    pub topic: String,
    pub script: String,
    pub lang: String,
    pub level: String,
    pub started: i64,
    pub ended: i64,
    pub lines: Vec<Line>,
    /// `report_prompt`'s answer, Null until the session ends
    pub report: Json,
    /// spoken with Gemini Live (`kaiwa_live`): the goals of the scene the tutor marks as reached, and the points won
    pub live: bool,
    pub goals: Vec<Goal>,
    pub score: i64,
}

/// One thing the learner should manage in a spoken scene (see `kaiwa_live::goals_for`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Goal {
    pub text: String,
    pub done: bool,
    pub at: i64,
}

impl Session {
    /// Minutes between the first and the last line (at least 1 once the learner wrote something).
    pub fn minutes(&self) -> i64 {
        let (Some(a), Some(b)) = (self.lines.first(), self.lines.last()) else { return 0 };
        let m = (b.at - a.at) / 60;
        if self.lines.iter().any(|l| l.role == "user") {
            m.max(1)
        } else {
            m
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mistake {
    pub wrong: String,
    pub right: String,
    pub why: String,
    pub kind: String,
    pub lang: String,
    pub at: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Store {
    pub sessions: Vec<Session>,
    pub mistakes: Vec<Mistake>,
    /// words looked up or learned (word, meaning, language, time)
    pub words: Vec<(String, String, String, i64)>,
}

fn sj(s: &str) -> Json {
    Json::Str(s.to_string())
}

impl Line {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("role", sj(&self.role)),
            ("text", sj(&self.text)),
            ("reading", sj(&self.reading)),
            ("romaji", sj(&self.romaji)),
            ("translation", sj(&self.translation)),
            ("check", self.check.clone()),
            ("at", Json::Num(self.at as f64)),
        ])
    }
    fn from_json(j: &Json) -> Line {
        Line {
            role: j.get("role").str(),
            text: j.get("text").str(),
            reading: j.get("reading").str(),
            romaji: j.get("romaji").str(),
            translation: j.get("translation").str(),
            check: j.get("check").clone(),
            at: j.get("at").as_i64().unwrap_or(0),
        }
    }
}

impl Session {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("id", Json::Num(self.id as f64)),
            ("mode", sj(&self.mode)),
            ("scene", sj(&self.scene)),
            ("title", sj(&self.title)),
            ("custom", Json::Arr(self.custom.iter().map(|x| sj(x)).collect())),
            ("topic", sj(&self.topic)),
            ("script", sj(&self.script)),
            ("lang", sj(&self.lang)),
            ("level", sj(&self.level)),
            ("started", Json::Num(self.started as f64)),
            ("ended", Json::Num(self.ended as f64)),
            ("lines", Json::Arr(self.lines.iter().map(|l| l.to_json()).collect())),
            ("report", self.report.clone()),
            ("live", Json::Bool(self.live)),
            ("goals", Json::Arr(self.goals.iter().map(|g| Json::obj(vec![("text", sj(&g.text)), ("done", Json::Bool(g.done)), ("at", Json::Num(g.at as f64))])).collect())),
            ("score", Json::Num(self.score as f64)),
        ])
    }
    fn from_json(j: &Json) -> Session {
        let c = j.get("custom").strings();
        let at = |i: usize| c.get(i).cloned().unwrap_or_default();
        Session {
            id: j.get("id").as_i64().unwrap_or(0),
            mode: j.get("mode").str(),
            scene: j.get("scene").str(),
            title: j.get("title").str(),
            custom: [at(0), at(1), at(2)],
            topic: j.get("topic").str(),
            script: j.get("script").str(),
            lang: j.get("lang").str(),
            level: j.get("level").str(),
            started: j.get("started").as_i64().unwrap_or(0),
            ended: j.get("ended").as_i64().unwrap_or(0),
            lines: j.get("lines").items().iter().map(Line::from_json).collect(),
            report: j.get("report").clone(),
            live: j.get("live").as_bool().unwrap_or(false),
            goals: j.get("goals").items().iter().map(|g| Goal { text: g.get("text").str(), done: g.get("done").as_bool().unwrap_or(false), at: g.get("at").as_i64().unwrap_or(0) }).collect(),
            score: j.get("score").as_i64().unwrap_or(0),
        }
    }
}

impl Store {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("sessions", Json::Arr(self.sessions.iter().map(|s| s.to_json()).collect())),
            (
                "mistakes",
                Json::Arr(
                    self.mistakes
                        .iter()
                        .map(|m| Json::obj(vec![("wrong", sj(&m.wrong)), ("right", sj(&m.right)), ("why", sj(&m.why)), ("kind", sj(&m.kind)), ("lang", sj(&m.lang)), ("at", Json::Num(m.at as f64))]))
                        .collect(),
                ),
            ),
            ("words", Json::Arr(self.words.iter().map(|(w, m, l, t)| Json::obj(vec![("word", sj(w)), ("meaning", sj(m)), ("lang", sj(l)), ("at", Json::Num(*t as f64))])).collect())),
        ])
    }

    pub fn from_json(j: &Json) -> Store {
        Store {
            sessions: j.get("sessions").items().iter().map(Session::from_json).collect(),
            mistakes: j
                .get("mistakes")
                .items()
                .iter()
                .map(|m| Mistake { wrong: m.get("wrong").str(), right: m.get("right").str(), why: m.get("why").str(), kind: m.get("kind").str(), lang: m.get("lang").str(), at: m.get("at").as_i64().unwrap_or(0) })
                .collect(),
            words: j.get("words").items().iter().map(|w| (w.get("word").str(), w.get("meaning").str(), w.get("lang").str(), w.get("at").as_i64().unwrap_or(0))).collect(),
        }
    }

    pub fn load(path: &std::path::Path) -> Store {
        std::fs::read_to_string(path).ok().and_then(|t| Json::parse(&t).ok()).map(|j| Store::from_json(&j)).unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, self.to_json().dump()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    /// The mistakes of a check, remembered (the newest first, 200 at most).
    pub fn remember(&mut self, check: &Json, lang: &str, now: i64) {
        for e in check.get("errors").items() {
            let (wrong, right) = (e.get("wrong").str(), e.get("right").str());
            if wrong.is_empty() || wrong == right {
                continue;
            }
            self.mistakes.retain(|m| !(m.wrong == wrong && m.lang == lang));
            self.mistakes.insert(0, Mistake { wrong, right, why: e.get("why").str(), kind: e.get("kind").str(), lang: lang.into(), at: now });
        }
        self.mistakes.truncate(200);
    }

    pub fn learned(&mut self, word: &str, meaning: &str, lang: &str, now: i64) {
        if word.trim().is_empty() {
            return;
        }
        self.words.retain(|w| !(w.0 == word && w.2 == lang));
        self.words.insert(0, (word.to_string(), meaning.to_string(), lang.to_string(), now));
        self.words.truncate(300);
    }

    /// The progress of a language: sessions, minutes, messages written, days in a row with a conversation (`tz` in
    /// minutes east of UTC), and the kinds of mistakes, most frequent first.
    pub fn progress(&self, lang: &str, now: i64, tz: i64) -> Json {
        let mine: Vec<&Session> = self.sessions.iter().filter(|s| s.lang == lang).collect();
        let minutes: i64 = mine.iter().map(|s| s.minutes()).sum();
        let written = mine.iter().map(|s| s.lines.iter().filter(|l| l.role == "user").count()).sum::<usize>();
        let day = |t: i64| (t + tz * 60).div_euclid(86_400);
        let mut days: Vec<i64> = mine.iter().filter(|s| s.lines.iter().any(|l| l.role == "user")).map(|s| day(s.started)).collect();
        days.sort();
        days.dedup();
        let mut streak = 0;
        let mut d = day(now);
        if !days.contains(&d) {
            d -= 1;
        }
        while days.contains(&d) {
            streak += 1;
            d -= 1;
        }
        let mut kinds: Vec<(String, usize)> = vec![];
        for m in self.mistakes.iter().filter(|m| m.lang == lang) {
            let k = if m.kind.is_empty() { "other".to_string() } else { m.kind.clone() };
            match kinds.iter_mut().find(|x| x.0 == k) {
                Some(x) => x.1 += 1,
                None => kinds.push((k, 1)),
            }
        }
        kinds.sort_by(|a, b| b.1.cmp(&a.1));
        Json::obj(vec![
            ("sessions", Json::Num(mine.len() as f64)),
            ("minutes", Json::Num(minutes as f64)),
            ("written", Json::Num(written as f64)),
            ("streak", Json::Num(streak as f64)),
            ("kinds", Json::Arr(kinds.into_iter().map(|(k, n)| Json::obj(vec![("kind", Json::Str(k)), ("count", Json::Num(n as f64))])).collect())),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(mode: &str) -> Ctx {
        Ctx { lang: "ja".into(), ui: "pt".into(), level: "N5".into(), hour: 20, mode: mode.into(), scene_id: "noodles".into(), ..Ctx::default() }
    }

    #[test]
    fn scenes_and_lessons_are_complete() {
        assert!(SCENES.len() >= 24 && LESSONS.len() == 10);
        let mut ids: Vec<&str> = SCENES.iter().map(|s| s.id).chain(LESSONS.iter().map(|l| l.id)).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n, "ids are unique");
        assert!(SCENES.iter().all(|s| !s.setting.is_empty() && !s.ai_role.is_empty() && !s.goal.is_empty()));
    }

    #[test]
    fn levels_follow_the_language() {
        assert_eq!(level_for("ja", "B1"), "N5");
        assert_eq!(level_for("en", "B1"), "B1");
        assert_eq!(level_for("es", "N3"), "A1");
    }

    #[test]
    fn the_tutor_is_told_the_scene_level_time_and_format() {
        let s = tutor_system(&ctx("roleplay"));
        assert!(s.contains("Japanese") && s.contains("N5") && s.contains("evening"));
        assert!(s.contains("noodle shop") && s.contains("the cook"));
        assert!(s.contains("\"reading\"") && s.contains("Portuguese"));
        let mut c = ctx("chat");
        c.lang = "en".into();
        c.level = "B2".into();
        c.mistakes = vec![("I goed".into(), "I went".into())];
        let e = tutor_system(&c);
        assert!(e.contains("FREE CONVERSATION") && e.contains("I goed") && !e.contains("\"romaji\""));
        let call = tutor_system(&ctx("call"));
        assert!(call.contains("VOICE CALL") && call.contains("speech recognition"));
        let st = tutor_system(&Ctx { script: "hiragana".into(), ..ctx("story") });
        assert!(st.contains("hiragana only"));
    }

    #[test]
    fn empty_or_odd_answers_of_small_models() {
        // what qwen2.5:3b gives under Ollama's JSON mode when it is lost: no message, never a "{}" bubble
        for bad in ["{}", "  {}\n", "", "```json\n{}\n```", "{\"reply\": \"\"}", "[]", "{\"translation\": \"Olá\"}"] {
            assert!(reply_of(bad, 1).is_none(), "{:?}", bad);
            assert_eq!(tutor_line(bad, 1).text, "");
        }
        // other keys, broken JSON, plain text
        assert_eq!(reply_of("{\"message\": \"こんにちは\"}", 1).unwrap().text, "こんにちは");
        assert_eq!(reply_of("{\"resposta\": \"Oi!\"}", 1).unwrap().text, "Oi!");
        assert_eq!(reply_of("{\"reply\": \"元気？\\nうん\", \"reading\": \"げん", 1).unwrap().text, "元気？\nうん");
        assert_eq!(reply_of("こんにちは！元気？", 1).unwrap().text, "こんにちは！元気？");
        assert_eq!(explain_error("operation timed out"), ERR_TIMEOUT);
        assert_eq!(explain_error("The local AI (Ollama) is not answering (refused)."), ERR_OLLAMA_DOWN);
        assert_eq!(explain_error("Gemini answered 500"), "");
    }

    #[test]
    fn tutor_lines_from_json_or_plain_text() {
        let l = tutor_line("```json\n{\"reply\":\"ラーメンですね。\",\"reading\":\"らーめんですね。\",\"romaji\":\"raamen desu ne.\",\"translation\":\"Lámen, né.\"}\n```", 5);
        assert_eq!((l.text.as_str(), l.reading.as_str(), l.translation.as_str(), l.at), ("ラーメンですね。", "らーめんですね。", "Lámen, né.", 5));
        let p = tutor_line("いらっしゃいませ！", 6);
        assert_eq!(p.text, "いらっしゃいませ！");
        assert!(p.translation.is_empty());
    }

    #[test]
    fn store_keeps_sessions_mistakes_and_progress() {
        let day = 86_400;
        let mut st = Store::default();
        for (i, start) in [10 * day + 100, 11 * day + 100, 12 * day + 100].iter().enumerate() {
            st.sessions.push(Session {
                id: i as i64,
                lang: "ja".into(),
                started: *start,
                lines: vec![Line { role: "assistant".into(), at: *start, ..Line::default() }, Line { role: "user".into(), at: start + 300, ..Line::default() }],
                ..Session::default()
            });
        }
        let check = Json::parse(r#"{"ok":false,"errors":[{"wrong":"は","right":"が","why":"...","kind":"particle"},{"wrong":"x","right":"x"}]}"#).unwrap();
        st.remember(&check, "ja", 1);
        st.remember(&check, "ja", 2);
        assert_eq!(st.mistakes.len(), 1);
        st.learned("ラーメン", "lámen", "ja", 3);
        let back = Store::from_json(&Json::parse(&st.to_json().dump()).unwrap());
        assert_eq!(back, st);
        let p = st.progress("ja", 12 * day + 500, 0);
        assert_eq!(p.get("sessions").as_i64(), Some(3));
        assert_eq!(p.get("minutes").as_i64(), Some(15));
        assert_eq!(p.get("streak").as_i64(), Some(3));
        assert_eq!(p.get("kinds").items()[0].get("kind").str(), "particle");
        // a day without a conversation breaks the streak
        assert_eq!(st.progress("ja", 14 * day, 0).get("streak").as_i64(), Some(0));
    }
}
