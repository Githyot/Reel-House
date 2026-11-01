# Duolingo research for Reelhouse Study mode

Written 2026-10-14. This is research material for later work on the **Learn** part of Study mode (Japanese, English,
Brazilian Portuguese and Spanish). The data contracts are in [`learn-contract.md`](learn-contract.md). Change a contract
there first, then the code.

## Resumo (PT-BR)

- **O que é:** um levantamento das funções públicas do Duolingo (trilha, tipos de exercício, XP, ofensiva, vidas/energia,
  gemas, ligas, missões, notificações, repetição espaçada) com regras e números de fontes públicas (blog, artigos científicos, wiki
  da comunidade). Para cada função, como fazer no Reelhouse (núcleo Rust + QML) e se dá para fazer **agora** ou **depois**.
- **github.com/duolingo:** 12 repositórios públicos. Os úteis para nós são o `halflife-regression` (algoritmo MIT, dados
  CC BY-NC 4.0) e os dados públicos das tarefas SLAM 2018 e STAPLE 2020 (traduções aceitas en→pt e en→ja, CC BY-NC 4.0). Isso
  serve num app pessoal e não comercial, mas impede uso comercial.
- **sanidhyy/duolingo-clone (MIT):** Next.js + Postgres. O modelo (cursos → unidades → lições → desafios → opções), o fluxo
  da lição (Verificar → certo/errado → Continuar, vidas, 10 pontos por desafio, tela final) e as regras de missão, loja e ranking
  são fáceis de portar para Rust/QML. **Não** copiar os sons, as vozes, o mascote nem as ilustrações: a origem deles não está
  documentada e o mascote imita a identidade visual do Duolingo.
- **morluto/rea:** é um MCP de engenharia reversa. Não vamos usá-lo no Duolingo (termos de uso e direitos autorais).
- **Bases de dados abertas:** Tatoeba (frases, traduções, áudio com licença por gravação), JMdict/KANJIDIC2 (CC BY-SA 4.0),
  KanjiVG (ordem dos traços), Wiktionary via Kaikki, listas de frequência, CEFR-J, listas JLPT, Common Voice, vozes Piper.
  O pipeline recomendado gera `assets/learn/<lang3>.json` no formato do contrato.
- **Plano por fases:** 1) trilha + 6 tipos de exercício + XP/meta diária/ofensiva + revisão de erros + HLR;
  2) missões, gemas/loja, conquistas, mascote com notificações, lendário, teste de nivelamento;
  3) aba de caracteres japoneses (traçado KanjiVG), fala, "Histórias" a partir de legendas de anime;
  4) ligas e funções sociais (precisam de servidor) e conversa com IA (chave do próprio usuário).

---

## 0. Scope and boundaries

- We **reproduce functions and game design**: rules, flows and numbers that Duolingo has described publicly. We do not copy
  Duolingo's course content (sentences, translations, curriculum), audio, character art, the owl, its name or its green
  look, and we do not decompile or probe Duolingo's apps, APIs or servers. The [Duolingo Terms](https://www.duolingo.com/terms)
  forbid reverse engineering and scraping. Game mechanics are not copyrightable. The content and the brand are protected.
- We may use Duolingo's **published open-source code and datasets** under their licences. Most datasets are
  **CC BY-NC 4.0**. That is acceptable while Reelhouse stays a personal, non-commercial app with attribution. If the app ever
  becomes commercial, drop the NC data (see §2).
- How reliable each source is:
  - **[official]**: Duolingo blog, research papers or help pages.
  - **[community]**: the fan wiki or third-party teardowns. These numbers change with A/B tests.
  - **[observed]**: widely known app behaviour with no official write-up.

---

## 1. Duolingo feature catalogue → Reelhouse

Legend for the last column: **NOW** means it can be built with what the app has today: the Rust core, QML, libmpv, the
`knowledge.md` state and the course packs. **LATER** means it needs new dependencies, a server, models or a lot of content.

### 1.1 Learning path

| Feature | What it does / public rules | Reelhouse implementation | When |
|---|---|---|---|
| **Path** | Since Nov 2022 the home screen is one linear path of circles ("levels"), grouped in **units** with a goal heading such as "get directions". Practice, stories and review are placed along the path, mixing new material with spaced review ([official: home-screen redesign](https://blog.duolingo.com/new-duolingo-home-screen-design)). | `LearnPath.qml`: a vertical `ListView` of units. Each unit has a header card and lesson nodes in a zig-zag. The clone uses an 8-node cycle with `right = indent*40px`, see §3. Node states are completed, current and locked. The data comes from `units[].lessons[]` in the course pack plus a progress file. | NOW |
| **Sections** | Units are grouped in sections. Tapping the section label shows the CEFR level and the grammar and vocabulary covered ([official](https://blog.duolingo.com/how-to-review-lessons-on-duolingo)). | Add an optional `section` to units in the pack, for example by JLPT level for jpn or by CEFR-J for eng. Show a section picker on the path header. | NOW (if the builder emits it) |
| **Lessons** | A lesson is a short session of about 10–15 exercises. Units move from easy exercises (recognise a word) to harder ones (type it) ([official: Duolingo 101](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)). | Generate the session at run time from the lesson's `words` and `sentences`. Early nodes of a unit use recognition exercises and later nodes use production (type, listen & type). See §1.2. | NOW |
| **Legendary** | After a unit is finished, a harder "Legendary" challenge for that unit (formerly per skill, five crown levels plus Legendary). Status now applies to the whole unit ([official](https://blog.duolingo.com/new-duolingo-home-screen-design)). Free users pay gems and lose the attempt after too many mistakes [community]. | `legendary: true` on a finished unit. The run is a production-only session (type, listen & type, no word bank) with **no hints** and a 3-mistake limit. The unit gets a gold frame. Optional gem cost (§1.5). | LATER (phase 2) |
| **Personalized practice** | Practice sessions built from content due for review and from your mistakes ([official](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)). Super's Practice Hub adds mistakes review, listening, speaking and vocabulary sets. | "Practice" button: choose the 6–10 words with the lowest HLR recall `p` from `knowledge.md` (§1.8) and build sentences around them. | NOW |
| **Mistakes review** | Reviews the exercises you got wrong. In the Energy system, reviewing mistakes at the end of a lesson costs no energy ([official](https://blog.duolingo.com/duolingo-energy/)). | Keep a per-session `mistakes[]`. Re-ask the wrong items at the end of the lesson (Duolingo does this [observed]) and keep the last 50 in `<data>/study/learn-progress.json` for a "Mistakes" practice. | NOW |
| **Words list** | A list of the words learnt, with audio and strength. In the Practice Hub for Super users [community]. | `LearnWords.qml`: a table from `knowledge.md` words with reading, translations and a strength bar. Click to play TTS or audio. Already half there: the knowledge table has all the columns. | NOW |
| **Guidebook** | Tips and key phrases per unit (the old "Tips") behind a button on the unit header ([official](https://blog.duolingo.com/new-duolingo-home-screen-design)). | Optional `tips` per unit (Markdown) in the pack. Most open data has no grammar notes, so we write these ourselves or leave them out at first. | LATER |
| **Duolingo Score** | A 0–160 score that rises with course progress and maps to CEFR. For example 30–59 = A2, 60–79 = early B1, 130–160 = C1/C2. Unlocked after Unit 1 ([official, Oct 2024](https://blog.duolingo.com/duolingo-score/)). | Our own "level" from units finished × the pack's level tags. Show "about A1/A2" or "about N5/N4". Keep it honest: these are estimates. | LATER |
| **Placement test** | At onboarding you can start from scratch or take a placement test that skips units. Placement cannot be undone ([official](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)). | Adaptive test: ask 2 items per unit, starting at unit 1 and doubling the step while answers are right (binary search). Stop after 2 misses. Mark the passed units completed. Also "jump here" on any unit: pass a 10-item unit test to skip it (the old "test out", 50 XP [community]). | LATER (phase 2) |
| **Chests on the path** | Reward chests between nodes (gems, XP boost) [observed]. | A node of kind `chest` every ~5 lessons gives gems (§1.5). | LATER |

### 1.2 Exercise types

Names from the [community exercise list](https://duolingo.fandom.com/wiki/Exercise), the
[Duolingo 101 post](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo) and the
[DuoRadio post](https://blog.duolingo.com/duoradio-listening-practice). The data column refers to the course-pack fields in
`learn-contract.md`.

| Exercise | What the learner does | Data we need | Answer check | When |
|---|---|---|---|---|
| **Select the image** ("Which one of these is X?") | 3–4 picture cards, pick the word. | A word plus a picture. Open data has few pictures. Options: emoji for concrete nouns (Unicode CLDR names give the mapping), or images the user adds. | Exact option id. | LATER (emoji version NOW for ~150 nouns) |
| **Select the meaning / Read and respond** | Read a sentence (or a highlighted word) and pick its meaning from 3 options. | `sentences[].tr` plus 2 distractor translations from the same unit. | Option id. | NOW |
| **Translate with word bank** ("Write this in X") | Tap tiles in order. Extra distractor tiles, about 4 depending on the language [community]. | `tok` of the target sentence plus distractor tokens from lesson words of the same POS. | Joined tiles equal any accepted answer (see "Answer checking" below). | NOW |
| **Type the translation** | Free typing, both directions. | `tr` (several accepted answers when Tatoeba has several linked translations; optional STAPLE answers, §2). | Normalised and tolerant, see below. | NOW |
| **Listen & tap** ("Tap what you hear") | Audio plays (normal or slow, the turtle button). Build the sentence from tiles. | Tatoeba `audio` id (licensed recordings only, §5) or TTS. | Tile sequence. | NOW (TTS needs §5.9) |
| **Listen & type** ("Type what you hear") | Same audio, typed. | Same. | Same normaliser, in the target language. | NOW |
| **What do you hear?** | Pick the transcription you heard from 3 options. | Audio plus distractor sentences. | Option id. | NOW |
| **Match pairs** ("Tap the pairs") | 4–5 pairs: target word ↔ translation (or audio ↔ word, or kana ↔ romaji). | `words[].tr`. | Each pair. A wrong pair flashes red, and that counts as one mistake. | NOW |
| **Fill in the blank / Select the missing word / Complete the translation** | A sentence with a gap. Pick (3 options) or type the missing token. | `tok`: remove one content token. Distractors are same-POS words. | Token equality (kana/kanji variants accepted for jpn). | NOW |
| **Complete the chat** | A short two-turn dialogue. Pick or type the reply. | Pairs of sentences linked as question → answer. Tatoeba has no dialogue structure. Use sentences ending in "?" plus a hand-written answer set, or subtitle pairs from videos (consecutive lines). | Option id. | LATER |
| **Speak** ("Speak this sentence") | Read aloud. Speech recognition scores it. Can be turned off ("Can't speak now") [observed]. | The sentence text. | ASR transcript vs. text, token overlap ≥ 0.8. Needs whisper.cpp (MIT) or Vosk (Apache-2.0) models for ja/en/pt/es. | LATER (phase 3) |
| **Character exercises (ja)** | Match kana ↔ sound, trace strokes, write freehand, build kanji from components ([official](https://blog.duolingo.com/learning-to-read-japanese-characters)). | Kana table (built-in), KanjiVG strokes, KANJIDIC2 readings and meanings, RADKFILE components. | Tracing: compare the user's stroke against the KanjiVG path (stroke count, start and end within a tolerance, direction). Matching: option id. | NOW for kana matching. Tracing is LATER (phase 3). |
| **Stories** | Short illustrated dialogues with audio. Comprehension questions are mixed in. Placed in the path and replayable ([official](https://blog.duolingo.com/new-duolingo-home-screen-design)). | Our version: **"Scenes"** made from the user's own anime or video subtitles (Reelhouse's advantage). A run of 8–15 consecutive subtitle lines, with a comprehension or fill-in question every 3 lines, using the video clip as the illustration. See §6. | Option id. | LATER (phase 3) |
| **DuoRadio** | Short podcast-like episodes hosted by a character. Matching, true/false and "listen and select 3 of 5 words" exercises during the episode. Early episodes mix English in ([official](https://blog.duolingo.com/duoradio-listening-practice)). They scaled it from 300 to 15,000+ episodes with LLM scripts seeded by curriculum sentences, plus TTS ([official](https://blog.duolingo.com/scaling-duoradio/)). | **"Reel Radio"**: a TTS dialogue built from lesson sentences, or audio-only playback of a saved video clip with listen-and-select questions. | Option id. | LATER |
| **Adventures** | A 3D mini-game: walk around a scene, talk to characters, choose replies. Worth 30 XP. Launched Sept 2024 for fr/es learners ([official](https://blog.duolingo.com/adventures)). | Out of scope for a desktop media app. A 2D "choose your reply" scene could reuse Complete the chat. | LATER (maybe never) |
| **Video Call / Roleplay / Explain My Answer (Max)** | GPT-4 based. A video call with Lily that remembers past calls and gives a transcript. Written roleplays with feedback. A grammar explanation of your answer. ja gets Video Call only ([official](https://blog.duolingo.com/duolingo-max)). | Optional, with a **user-provided** LLM API key (like the Real-Debrid key setting). "Explain my answer" is the cheapest one to do: send the prompt, the user answer and the accepted answer. Roleplay is a chat page. No video avatar. | LATER (phase 4) |

**Answer checking for typed answers.** Duolingo accepts typos and missing accents and shows a note such as "You have a
typo" or "Pay attention to the accents" [observed]. Plan for `learn.rs`:

1. NFKC-normalise, lowercase and trim. Collapse whitespace. Strip punctuation (Unicode category `P*`, the same rule as the
   STAPLE scorer's `strip_punctuation`).
2. For jpn, fold katakana to hiragana and accept a kanji word written in kana when its JMdict reading matches.
3. Compare against every accepted answer. An accepted answer is any linked translation of the sentence, plus our own
   variants (contractions such as "I'm"/"I am"; pt "você"/"tu").
4. If the only difference is accents (á/a, ñ/n, ç/c), count it **correct** with an accent note.
5. If one word of length ≥ 4 differs by Damerau-Levenshtein distance 1, count it **correct** with a typo note.
6. Otherwise it is wrong. Show the closest accepted answer and highlight the differing tokens.

### 1.3 Japanese "Characters" tab

[Official](https://blog.duolingo.com/learning-to-read-japanese-characters):

- Three charts: hiragana (46), katakana and kanji.
- A "Learn the Characters" button starts lessons, and the characters light up as they are learnt.
- Hiragana comes first, katakana early in Section 1 and kanji later in Section 1, spread out.
- A kanji appears in the kanji tab once it shows up in a course word. Tapping it lists the learnt words that use it.
- Romaji can be turned on with "Show pronunciation".

Reelhouse: `LearnKana.qml` is a grid of 46+46 cells (plus dakuten and yōon rows). The fill colour shows strength from
`knowledge.md`, where a kana counts as a word with kind `kana`. The kanji grid is built from the kanji that appear in learnt
words (KANJIDIC2 gives meanings in en/pt/es, see §5.2). Tracing uses KanjiVG SVG paths drawn with QML `Shape`/`PathSvg`.
The user draws on a `Canvas`. NOW for the charts and matching drills. LATER for tracing.

### 1.4 Gamification

| Mechanic | Public rules and numbers | Reelhouse implementation | When |
|---|---|---|---|
| **XP** | 20 XP per lesson, plus up to 5 combo XP. Practice 10 + 0–5. Unit test-out 50. Stories 14–28. Legendary ≈ 40 [community: [fandom XP](https://duolingo.fandom.com/wiki/XP)]. Timed challenges and time-of-day bonuses exist ([official](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)). The clone gives 10 points per challenge (§3). | `xp_for(lesson) = 10 + combo_bonus(0..5) + (perfect ? 5 : 0)`. Practice gives half. XP is logged per day in `learn-progress.json` (`xp_by_day`). | NOW |
| **Daily goal** | Chosen at onboarding. Historically 10/20/30/50 XP (Casual/Regular/Serious/Intense) [community]. Newer onboarding asks minutes per day [observed]. | Setting `learn.daily_goal_xp` (10/20/30/50). A ring on the Study header fills up, with a "Goal reached" chime. | NOW |
| **Streak** | Days in a row with at least one lesson. Learners at 7+ days are 3.6× more likely to finish the course. A milestone animation gave +1.7% day-7 retention ([official](https://blog.duolingo.com/how-duolingo-streak-builds-habit)). | `streak` and `last_day` in `learn-progress.json`. The day starts at 04:00 like the Anki scheduler in `study.rs`. Any lesson, practice or Anki review session counts (configurable). | NOW |
| **Streak Freeze** | Protects one missed day. Up to **2 equipped**. "Refill streak freezes" costs 400 gems in the official screenshot. +0.38% DAU from allowing 2 ([official](https://blog.duolingo.com/how-duolingo-streak-builds-habit)). Freezes are used automatically, and the day shows a snowflake. Freezes also come from milestones, quests and chests [community: [teardown](https://duolingo.deconstructoroffun.com/mechanics/streaks)]. | `freezes: 0..2`. On the first open after missed days, use one freeze per missed day while any remain, otherwise reset. The calendar view shows snowflakes. | NOW |
| **Streak repair / Earn back** | If freezes ran out, a short set of lessons within a time window restores the streak [community]. | When the streak broke yesterday: "Do 3 lessons today to restore it". | LATER |
| **Streak wager** | Bet gems on keeping a 7-day streak to win more gems ([official, KDD paper](https://research.duolingo.com/papers/yancey.kdd20.pdf)). | Shop item "Wager 50 gems → 100 if you keep 7 days". | LATER |
| **Perfect Streak / Streak Society / milestones** | Milestones at 7, 30, 100 and 365 days get special animations and chests. Streak Society tiers raise the freeze cap to 5. Perfect Streak means no freeze used, shown with a gold halo [community: [teardown](https://duolingo.deconstructoroffun.com/mechanics/streaks)]. | Milestone toast plus a mascot animation at 7/30/100/365. A gold flame when no freeze was used in the last 7 days. | LATER |
| **Friend Streaks** | Two friends keep a shared streak. Up to 5 [community]. | Needs accounts and sync. Out of scope unless sync exists. | LATER (needs server) |
| **Hearts** | 5 hearts. You lose 1 per mistake. At 0 you practise to earn one back or pay gems. Unlimited with Super ([official](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)). They refill fully in about a day ([official](https://blog.duolingo.com/duolingo-energy/)). | Optional, **off by default**: no business reason to block a learner. Setting `learn.hearts` (off/5). If on: −1 per mistake (not in practice), +1 per correct practice answer, full refill after 24 h or for gems. | NOW (as an option) |
| **Energy** (replacing hearts, test started July 2025) | Each exercise costs energy. Mistakes cost more. Correct-answer runs, perfect lessons and chests give energy back. Mistakes review is free. A full recharge takes about a day ([official](https://blog.duolingo.com/duolingo-energy/)). Community numbers: start 25, −1 per correct and −1–2 per wrong answer, +1 per hour, +1–5 for answer streaks, 750 gems for +25 [community: [fandom](https://duolingo.fandom.com/wiki/Energy)]. | Not worth copying. It is a monetisation lever. Mention as an optional "challenge mode" only. | — |
| **Gems and shop** | A currency earned from quests, chests, leagues and achievements. It buys streak freezes, timer boosts and heart refills ([official](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)). League top-3 get about 60 gems. League repair costs 2,000 [community]. | `gems` in progress. Earn: +5 per lesson, +10 per daily quest, +30 per monthly badge, chest 10–30. Shop: freeze 200, heart refill 100 (if hearts are on), XP boost (2× for 15 min) 100, legendary attempt 100. All local, no real money. | LATER (phase 2) |
| **Leagues** | 10 tiers: Bronze, Silver, Gold, Sapphire, Ruby, Emerald, Amethyst, Pearl, Obsidian, Diamond. Groups of 30. A week ends on Sunday. Matched by study habits and time zone, across all courses. Top 10 in Diamond can enter a 3-round tournament. Profiles can opt out ([official](https://blog.duolingo.com/duolingo-leagues-leaderboards/)). Promotion counts: Bronze 20, Silver 15, Gold 10, Sapphire–Pearl 7, Obsidian 5, Diamond 0 [community: [teardown](https://duolingo.deconstructoroffun.com/mechanics/leagues)]. | Real leagues need a server and other users. Offline options: (a) "**Personal league**": compete with your own previous weeks (ghost rows). (b) "**Practice league**": 29 clearly labelled bots whose weekly XP is drawn from a log-normal around your 4-week average, so it stays winnable. Never pretend bots are people. (c) Later: friend leagues via the backup or sync file. | LATER (phase 4) |
| **Daily quests** | 3 per day, e.g. "Earn 30 XP", "Get 10 in a row", "Perfect 2 lessons". Quest points count towards a **monthly challenge** badge, e.g. complete 50 (or 100) quests in a month [community: [Duoplanet](https://newsletter.duoplanet.com/posts/how-many-quests-duolingo)]. | `quests_today[]` drawn each day from a template table (§3 lists the clone's XP-threshold quests), with a chest when all 3 are done. Monthly badge: complete N quests → a badge in a "Trophies" page. | LATER (phase 2) |
| **Friends Quests** | Weekly pairing with a friend. A shared goal over 5 days, nudges, gifts, a big chest ([official](https://blog.duolingo.com/friends-quests/)). | Needs a server. | LATER (needs server) |
| **Achievements / badges** | Tiered badges for streak length, XP, words learnt, perfect lessons and league wins [observed]. | `achievements.rs`-style table: id, metric, tiers (e.g. streak 3/7/14/30/60/100/365; XP 100…50k; words 50…5k; perfect lessons 1…500). Check after each session. | LATER (phase 2) |
| **XP boost** | 2× XP for 15 min, e.g. after finishing a unit ([community: fandom XP](https://duolingo.fandom.com/wiki/XP)). Friends can gift one ([official](https://blog.duolingo.com/friends-quests/)). | `xp_boost_until` timestamp. A chest or shop item grants it. | LATER |
| **Combo / "N in a row" / perfect lesson** | A banner after several correct answers in a row, with up to 5 combo XP. A "Perfect lesson" banner when there were no mistakes. Energy gives bonus energy for perfect lessons ([official](https://blog.duolingo.com/duolingo-energy/)). | A combo counter in the lesson header with a flame and a chime at 5/10/15. The finish screen shows XP, accuracy, time and "Perfect!". | NOW |
| **Timed challenges** | Timed XP rushes such as match-pairs races ([official mention](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)). | "Match Rush": 90 s of match-pairs from learnt words, +1 XP per pair. | LATER |

### 1.5 Notifications and the mascot

- **Official facts.** Practice reminders are picked from a pool of hand-written templates by a bandit algorithm, the
  *Recovering Difference Softmax*:
  - Reward = the user finishes a lesson **within 2 hours** of the reminder.
  - The score of a template is its reward lift relative to other templates eligible for the same user (the "sleeping" arms).
  - A **recency penalty** demotes recently used templates (novelty effect): score − γ·0.5^(days since last use / h), with
    **γ = 0.017 and h = 15 days** in production.
  - Arms are chosen by softmax.
  - Result: **+0.5% DAU and +2% new-user retention**.
  - Templates have eligibility rules, such as "3+ day streak" or "has a streak wager".

  Sources: [Yancey & Settles, KDD 2020](https://research.duolingo.com/papers/yancey.kdd20.pdf) and
  [blog](https://blog.duolingo.com/hi-its-duo-the-ai-behind-the-meme/). The dataset (~200 M notifications) is CC BY-NC 4.0
  (§2).
- **Community teardown** ([notifications](https://duolingo.deconstructoroffun.com/mechanics/notifications)):
  - No fixed broadcast times. A routine reminder lands near your habit time, e.g. 30 min before yesterday's practice time.
  - "Save" pushes come before something is lost (streak, league, reward).
  - At most **2 pushes per day**.
  - Each character has a tone: Duo is friendly and personal, Lily sarcastic, Oscar dramatic.
  - Settings include Smart scheduling, Streak Saver, and Early Bird / Night Owl rewards ([official](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)).
- **Duo's behaviour** [observed]: Duo cheers on correct answers and milestones, looks sad when you are wrong or break a
  streak, and is "guilt-trip" persistent in reminders. The meme persona is part of their marketing.

**Reelhouse.**

- **Mascot.** An **original** mascot. Not an owl and not Duolingo green, so it does not look like a knock-off. For example a
  small film-reel or cat character drawn by us, or assembled from CC0 parts. It has 5 poses (idle, happy, sad, sleeping,
  celebrating) as SVGs, animated with QML transforms and `SequentialAnimation`. No Rive or Lottie dependency is needed.
  It speaks short lines from a per-language template table, translated through `Core.tr` like all UI strings.
- **Reminder delivery.**
  - **In-app:** the existing notification centre (`src/controller_notify.rs`) gets a "Study" notice: streak at risk, daily
    quests, cards due.
  - **System toasts:** while Reelhouse runs in the background, use the Rust crate
    [`notify-rust`](https://crates.io/crates/notify-rust) (MIT/Apache-2.0, WinRT toasts on Windows, D-Bus on Linux). The Qt
    app does not link QtWidgets, so `QSystemTrayIcon` is not available without a new module.
  - **When the app is closed:** that needs a Windows scheduled task or a small autostart helper. LATER.
- **Template choice, a simplified bandit.**
  - Use the KDD recency penalty (γ = 0.017, h = 15 days) plus a per-template success rate with a Beta(1,1) prior.
    Reward = a lesson finished within 2 h.
  - Softmax with τ ≈ 0.01.
  - With one user the data is tiny, so the recency rotation is what matters.
  - Limits: at most 2 per day and quiet hours 22:00–08:00.
  - Remind 30 min before the median practice time of the last 7 days, and send a "save" reminder at 20:00 if the
    streak is at risk.
  - After 7 days without practice, back off to one reminder every 3 days. After 30 days, stop. This rule is ours, not
    Duolingo's.

### 1.6 Onboarding and placement

[Official, Duolingo 101](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo) and
[community teardown](https://duolingo.deconstructoroffun.com/mechanics/streaks):

- Choose the language.
- Choose the reason and the daily goal.
- Turn on reminders.
- Choose "start from scratch" or "find my level" (placement test).
- Make a streak commitment (7/14/30 days) before the sign-up screen.

Reelhouse: a 4-step `LearnOnboarding.qml`.

- Study language is `general.study_lang` from learn-contract §4.
- Known languages come from `general.known_langs`. This picks the translation language.
- Daily goal.
- Reminders on or off.
- Start at unit 1 or take the placement test.

NOW for the steps. The placement test is phase 2.

### 1.7 Spaced repetition (HLR) and Birdbrain

**Half-life regression** ([Settles & Meeder, ACL 2016](https://research.duolingo.com/papers/settles.acl16.pdf), code in
[duolingo/halflife-regression](https://github.com/duolingo/halflife-regression), MIT):

- Recall probability `p = 2^(−Δ/h)`. Δ = days since the word was last practised. h = half-life in days.
- `h = 2^(Θ·x)`. Features `x`: `√(1+right)`, `√(1+wrong)`, `bias = 1`, and optionally one indicator per lexeme tag.
- h is clipped to **[15 min, 274 days]** and p to [0.0001, 0.9999] (`MIN_HALF_LIFE`, `MAX_HALF_LIFE`, `pclip` in
  `experiment.py`).
- Trained by SGD:
  - The loss is (p − p̂)² + α·(h − ĥ)² + λ‖Θ‖².
  - Defaults: `lrate = 0.001`, `hlwt (α) = 0.01`, `l2wt (λ) = 0.1`, `sigma = 1`.
  - Learning rate per feature = lrate / √(1 + count).
- In production HLR gave **+12% daily engagement**. The variant without lexeme features (HLR −lex) did best in retention.

Reelhouse (`src/knowledge.rs`, column `strength`, see learn-contract §2):

```rust
// p = 2^(-Δ/h), h = 2^(w_r*sqrt(1+right) + w_w*sqrt(1+wrong) + w_b)
// Weights: start from hand-set values (w_r = 1.0, w_w = -0.6, w_b = 0.0 → a new word ≈ 1–2 days)
// and refit them locally from the user's own history once there are a few hundred reviews (same SGD as experiment.py).
fn recall(right: u32, wrong: u32, days: f64, w: &[f64; 3]) -> f64 {
    let x = [((1 + right) as f64).sqrt(), ((1 + wrong) as f64).sqrt(), 1.0];
    let h = 2f64.powf(w.iter().zip(x).map(|(a, b)| a * b).sum::<f64>()).clamp(15.0 / 1440.0, 274.0);
    2f64.powf(-days / h).clamp(1e-4, 0.9999)
}
```

The starting weights are our own guess, not published values. They could instead be fitted once on the
Duolingo 13M-trace dataset (CC BY-NC) with `experiment.py -l` (no lexemes). Ship only the 3 numbers and credit the
dataset.

**Birdbrain** ([official blog, 2020](https://blog.duolingo.com/learning-how-to-help-you-learn-introducing-birdbrain);
[IEEE Spectrum, 2023](https://spectrum.ieee.org/duolingo)):

- It predicts P(correct) for each learner × exercise.
- V1 was an IRT-style logistic model with an Elo-like update after every exercise. Exercise difficulty = the sum of feature
  difficulties (exercise type, words). A wrong answer lowers the learner's ability, raises the item's difficulty, and
  surprising results move them more.
- V2 (May 2022) is an LSTM with a 40-dimensional learner state, updated within minutes.
- The session generator uses it to keep lessons challenging but not overwhelming. Scale: ~1 B exercises per day.

Reelhouse "Birdbrain-lite" (NOW, in `learn.rs`):

- Keep the learner ability θ (one per study language) and a difficulty β per exercise type and per word. Both are stored
  in the progress file.
- P(correct) = σ(θ − β_type − mean β_words).
- After each answer, with y = 1 if right and 0 if wrong:
  - θ += k·(y − p)
  - β_type −= k·(y − p)
  - each β_word −= k·(y − p)/n
  - k = 0.1
- When building a session, prefer candidates with P in **0.70–0.85**, mixing in easier ones after 2 misses in a row.
  These thresholds are our choice. Duolingo has not published a target rate.

### 1.8 Sound, haptics and visual feedback

Public descriptions [observed, and the clone in §3]:

- A distinct bright chime for correct.
- A low buzz or thud for wrong.
- A fanfare on lesson complete. Confetti, the mascot jumping, and the XP counting up.
- A streak flame animation the first time each day.
- Haptics on mobile.
- Sound and haptics can each be turned off in settings ([official](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)).
- Characters have their own voices. They were recorded by actors and turned into TTS voices in 5 languages
  ([official](https://blog.duolingo.com/character-voices)).

The bottom feedback banner turns green ("Nicely done!") or red with the correct answer, and the main button changes
Check → Continue (§3).

Reelhouse:

- **Original** sounds only. Either CC0 packs such as [Kenney "Interface Sounds"](https://kenney.nl/assets/interface-sounds),
  or tones we synthesise ourselves (a short major-third chime, a soft low thud, a 1-second arpeggio) saved as small WAV
  files in `assets/sounds/`.
- Play them through a second, audio-only libmpv handle, which is already embedded, or a tiny Rust audio crate. The app does
  not link QtMultimedia.
- Desktop haptics: none. Instead use a small shake animation on wrong, a scale "pop" on correct, and confetti with
  `ParticleSystem`, which is in QtQuick.Particles (part of QtQuick, no new link).
- Settings: `learn.sounds` and `learn.motion` (reduce motion).

### 1.9 Accessibility

Public settings: listening exercises on/off and a "Can't listen now" button, speaking off, sound and haptics off,
motivational messages off ([official](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)).
Disabling is temporary, about 1 hour, from the in-lesson button [observed]. Screen-reader support is partial, judging by
community reports such as [AppleVis](https://applevis.com/comment/150456).

Reelhouse, all NOW:

- **Full keyboard flow.** The clone already does this: digits 1–9 pick options and Enter checks or continues. Match pairs
  works with 1–5 / 6–0. Tiles have arrow and Enter focus.
- **Accessible names.** `Accessible.name` and `Accessible.role` on every tile and option.
- **Not colour alone.** ✓/✗ icons and the text "Correct" or "Wrong" go with the green and red.
- **Theme tokens.** Use the existing theme tokens so the GNOME, Windows, Apple and Android themes and dark mode work.
- **Text.** Font scaling follows `Theme.type`. Furigana and romaji are toggles (`learn.show_reading`, `learn.show_romaji`).
- **Audio.** Slow audio (turtle) is mpv `speed=0.75`. "Can't listen now" and "Can't speak now" skip that type for 1 hour.
- **Motion.** Reduce motion turns off confetti and shake.

---

## 2. github.com/duolingo: public repositories

The API listing (`api.github.com/orgs/duolingo/repos`) was blocked from this sandbox. The org page
([github.com/orgs/duolingo/repositories](https://github.com/orgs/duolingo/repositories)) lists **12 public repos**. The
licences of the useful ones were checked from the cloned repos.

| Repo | Licence | What it is | Useful for us |
|---|---|---|---|
| [halflife-regression](https://github.com/duolingo/halflife-regression) | **MIT** (code). Data **CC BY-NC 4.0**. | HLR spaced-repetition model (`experiment.py`), R evaluation, ACL 2016 paper PDF, `lexeme_reference.txt` (POS and morphology tags). The 13M learning traces are on [Dataverse doi:10.7910/DVN/N8XJME](https://dataverse.harvard.edu/dataset.xhtml?persistentId=doi:10.7910/DVN/N8XJME) (`settles.acl16.learning_traces.13m.csv.gz`, 379 MB). | **Yes.** Port the formula (§1.7). Optionally fit our 3 weights on the data (non-commercial). |
| [duolingo-sharedtask-2020](https://github.com/duolingo/duolingo-sharedtask-2020) | No licence file (scripts). Data **CC BY-NC 4.0**. | **STAPLE 2020**: English prompts with *all accepted translations*, weighted by how often learners typed them. Pairs **en→pt, en→ja**, en→hu, en→ko, en→vi. Train: 4,000 prompts for pt and 2,500 for ja, with ~856 k accepted ja translations. Dev and test have 500 each ([task site](https://sharedtask.duolingo.com/)). Data on [Dataverse doi:10.7910/DVN/38OJR6](https://dataverse.harvard.edu/dataset.xhtml?persistentId=doi:10.7910/DVN/38OJR6) (`staple-2020-full-data.tar.gz`, 17.7 MB). File format: blocks of `prompt_<md5>\|<english>` followed by `<translation>\|<weight>` lines, blank-line separated (see `utils.py`). | **Yes, carefully.** Gives many accepted pt and ja answers for English prompts, which is perfect for "translate en→pt/ja" answer checking. But these are Duolingo course sentences under CC BY-NC. Keep them as an **optional, separate** pack (`assets/learn/extra/staple-en-pt.json`), credit Duolingo, non-commercial only. Downloading needs a Dataverse **guestbook form** (a browser click), not plain curl. |
| (data only) SLAM 2018 | Data **CC BY-NC 4.0** | Second Language Acquisition Modeling: ~7 M words of learner answers with per-token right/wrong labels for en_es, es_en and fr_en ([task](https://sharedtask.duolingo.com/2018.html), [Dataverse doi:10.7910/DVN/8SWHNO](https://dataverse.harvard.edu/dataset.xhtml?persistentId=doi:10.7910/DVN/8SWHNO)). | Low. It could tune which en/es words are hard (β priors). No pt or ja. |
| (data only) Notification bandit 2020 | Data **CC BY-NC 4.0** | ~200 M practice reminders over 35 days ([Dataverse doi:10.7910/DVN/23ZWVI](https://dataverse.harvard.edu/dataset.xhtml?persistentId=doi:10.7910/DVN/23ZWVI), ~9 GB). | No. The paper is enough. |
| [whosaidit](https://github.com/duolingo/whosaidit) | Data **CC BY-NC 4.0** | 3,600 labelled sentences (en/es/it/ko/zh) for speaker-attribute classification (2026). | No. |
| [metasearch](https://github.com/duolingo/metasearch) | Apache-2.0 | Search aggregator for Slack, Docs and GitHub. | No. |
| [pre-commit-hooks](https://github.com/duolingo/pre-commit-hooks) | Apache-2.0 | Lint and format hooks. | No. |
| [slack-ai-agent](https://github.com/duolingo/slack-ai-agent), [slack-mcp](https://github.com/duolingo/slack-mcp) | Apache-2.0 | Slack bot / Slack MCP server. | No. |
| [minject](https://github.com/duolingo/minject) | Apache-2.0 | Python dependency injection. | No. |
| [pulldozer](https://github.com/duolingo/pulldozer), [splinter](https://github.com/duolingo/splinter) | Apache-2.0 | Batch repo edits / pattern linter. | No. |
| [rtl-viewpager](https://github.com/duolingo/rtl-viewpager) (archived), [rollbar-ios](https://github.com/duolingo/rollbar-ios) (archived) | Apache-2.0 / MIT | Android RTL pager, iOS crash reporting. | No. |

Papers worth reading are on [research.duolingo.com](https://research.duolingo.com/): HLR (ACL 2016), the notification
bandit (KDD 2020), STAPLE (ACL 2020 WNGT), SLAM (NAACL 2018 BEA), and "Jump-starting item parameters" (EMNLP 2021, cold-start
difficulty for new items).

> **Licence summary.** MIT code can be copied with the notice. All Duolingo **datasets are CC BY-NC 4.0**. They are allowed in
> Reelhouse while it is personal and non-commercial, with credit ("Contains data © Duolingo, Inc., CC BY-NC 4.0") on an
> About/Sources screen. Do not bundle them in anything sold.

---

## 3. sanidhyy/duolingo-clone (Next.js), analysis

Repository: [sanidhyy/duolingo-clone](https://github.com/sanidhyy/duolingo-clone). **MIT**, © 2024 Sanidhya Kumar Verma. Brand
name in the app: "Lingo". Read at commit `833c9a5` (2026-10-09).

**Stack:** Next.js 16 (App Router, server actions), React 19, Drizzle ORM on Neon Postgres, Clerk auth, Stripe subscription
($20 "unlimited hearts"), react-admin for the admin, zustand for the modals, Tailwind and shadcn UI, react-use (audio, keys),
react-confetti.

### 3.1 Data model (`db/schema.ts`)

```
courses(id, title, image_src)
units(id, title, description, course_id→courses, order)
lessons(id, title, unit_id→units, order)
challenges(id, lesson_id→lessons, type ENUM('SELECT','ASSIST'), question, order)
challenge_options(id, challenge_id→challenges, text, correct bool, image_src?, audio_src?)
challenge_progress(id, user_id, challenge_id→challenges, completed bool)   UNIQUE(user_id, challenge_id)
user_progress(user_id PK, user_name, user_image_src, active_course_id→courses, hearts int default 5, points int default 0)
user_subscription(id, user_id unique, stripe_customer_id, stripe_subscription_id, stripe_price_id, stripe_current_period_end)
```

- Only **two exercise types**:
  - `SELECT`: "Which one of these is 'the man'?", answered with picture cards with audio.
  - `ASSIST`: "Select the correct meaning", where the question is shown in a speech bubble next to the mascot and the
    options are text rows.
- Content is static rows, inserted by `scripts/prod.ts`: 1 course (Spanish), 2 units × 4 lessons × N challenges.
- Constants (`constants.ts`): `MAX_HEARTS = 5`, `POINTS_TO_REFILL = 10`, and `QUESTS` = earn 20/50/100/250/500/1000 XP.
  These are cumulative points, not daily.

### 3.2 Lesson flow (`app/lesson/quiz.tsx`, `actions/*.ts`)

1. Load the active lesson. The active lesson is the **first lesson that has any challenge without completed progress**.
   Earlier lessons are completed and later ones locked (`db/queries.ts → getCourseProgress`, `learn/unit.tsx`).
2. Resume at the first uncompleted challenge. The progress bar equals completed / total.
3. Select an option (click or keys **1..n**) and press **Check** (Enter).
   - **Correct:** play `correct.wav` and server-side `upsertChallengeProgress` (+10 points). The footer turns green
     ("Nicely done!"), the button becomes **Next**, the bar advances.
   - **Wrong:** play `incorrect.wav` and `reduceHearts` (−1 heart). The footer turns red ("Try again."), the button becomes
     **Retry**, and the same challenge is retried. There is no "show the answer" step.
   - **0 hearts:** the hearts modal ("You ran out of hearts", go to the shop).
4. **Practice mode** = replaying a completed lesson. You cannot lose hearts, and each correct answer gives +1 heart (max 5)
   and +10 points (`isPractice`). A modal explains this at the start.
5. **Finish screen:** `finish.mp3` and confetti. Two result cards: points = challenges × 10, and hearts left (∞ if
   subscribed). Buttons: "Practice again" and "Continue".
6. Exit (X) opens a confirm modal ("Wait, don't go!").

### 3.3 Other pages

- **Learn** (`(main)/learn`): units with a banner (title, description, "Continue"). Lesson buttons in an 8-step zig-zag:
  `cycleIndex = i % 8`, indent 0,1,2,1,0,−1,−2,−1, `right = indent × 40px`. Icons: Check for done, Crown for the last
  lesson, Star otherwise. The current lesson has a circular progress ring and a bouncing "Start" bubble.
- **Leaderboard:** the top 10 users by total points, all-time and global. No leagues.
- **Quests:** a progress bar per XP threshold (`points / quest.value`).
- **Shop:** refill hearts for 10 points, or subscribe (Stripe) for unlimited hearts.
- **Sidebar and user progress:** course flag, hearts and points, with a promo card.
- **Admin** (`app/admin`, react-admin over `app/api/*` REST routes): CRUD for courses, units, lessons, challenges and
  options. Access is by `CLERK_ADMIN_IDS`.

### 3.4 What to port to Reelhouse and how

| Clone part | Port? | How in Reelhouse |
|---|---|---|
| Schema (course → unit → lesson → challenge → option) | **Concept yes, tables no.** | Our course pack (`learn-contract.md` §1) holds words, sentences and units. **Challenges are generated** per session in Rust (`learn.rs`), not stored, so the same sentence gives several exercise types. Progress goes in `<data>/study/learn-progress.json`: completed lessons, xp_by_day, streak, freezes, gems, quests, achievements, θ/β. Knowledge per word stays in `knowledge.md`. |
| Lesson state machine (none → selected → check → correct/wrong → next; finish) | **Yes** | Rust owns it: a `LearnSession { items, index, status, hearts, combo, mistakes }`. The `learn` state JSON goes through `rh_state`. Commands `learn_start {lesson}`, `learn_answer {answer}`, `learn_continue`, `learn_quit`. QML renders `status` and sends commands. Improve on the clone: on wrong, **show the correct answer** and re-queue the item at the end (Duolingo behaviour) instead of a blind retry. |
| Hearts, practice regains hearts, refill | **Optional** | As in §1.4, off by default. |
| 10 points per challenge, finish summary | **Yes** | XP rules in §1.4. The finish card shows XP, accuracy, time, combo and words learnt. |
| Keyboard shortcuts 1–9 / Enter | **Yes** | QML `Keys.onPressed` in the lesson page. Esc = quit dialog. |
| Zig-zag path layout | **Yes** | The same 8-step pattern in QML. Locked, current and completed styles come from Theme tokens, not the clone's colours. |
| Quests (cumulative XP thresholds) | **Partially** | Use daily quests plus a monthly badge instead (§1.4). The thresholds can be achievements. |
| Leaderboard top 10 | **No** (no other users) | See the leagues options in §1.4. |
| Shop / Stripe / Clerk / admin | **No** | No payments or accounts. The "admin" is our offline course builder (§5.10) plus Study's existing card editor. |
| Audio, images, mascot | **No, do not copy** | See below. |

**Assets that must NOT be copied** (`public/`):

- `mascot.svg`, `mascot_sad.svg`, `mascot_bad.svg`: a green-gradient character (`#59B15C`…`#85DFAF`) that imitates
  Duolingo's look. Its origin is not documented.
- `correct.wav`, `incorrect.wav`, `finish.mp3`: the README credits Freesound, but there is no per-file licence.
- `es_*.mp3`: Spanish voices generated with ElevenLabs (README credit). They are bound to ElevenLabs' terms.
- Character SVGs (`man`, `woman`, `boy`, `girl`, `robot`, `zombie`), `hero.svg`, `learn.svg`, `shop.svg` and the other
  illustrations: the provenance is unclear. The README credits Kenney and Flagpack generally.

The MIT licence covers the code, not necessarily these files. Reelhouse uses its own mascot and sounds (§1.5, §1.8) and
Lucide icons, which the app already has.

---

## 4. morluto/rea

[REA ("Reverse Engineer Anything")](https://github.com/morluto/rea) (MIT, 2026) is an MCP server plus CLI. It gives a coding
agent reverse-engineering tools for native binaries (through Hopper, Ghidra or IDA), JavaScript and Electron apps, .NET
assemblies, Android packages (JADX) and websites, and explains how a feature works with evidence. We will **not** use it on
Duolingo:

- Duolingo's Terms of Service forbid reverse engineering, decompiling and scraping their apps and services.
- Their course content, audio and art are copyrighted.
- Everything we need is already public: the blog, the papers and the open datasets.

It could be useful on **our own** code, for example to inspect a Reelhouse release build, the libmpv DLL we ship, or a
crash. Even then a normal debugger is usually enough. Not planned.

---

## 5. Open databases for ja / en / pt-BR / es

Sizes and counts were measured on 2026-10-14 unless marked otherwise.

| # | Source | Licence | Size / counts | Gives us | Download |
|---|---|---|---|---|---|
| 5.1 | **Tatoeba** | Sentences **CC BY 2.0 FR**, part also CC0. **Audio: per recording**. An empty licence field means *no reuse outside Tatoeba* ([downloads page](https://tatoeba.org/en/downloads)). | Sentences: **jpn 248,924**, **eng 2,038,719**, **por 445,215**, **spa 442,544**. Links: jpn-eng 280,723, jpn-spa 39,780, **jpn-por only 4,925**, eng-por 302,280, eng-spa 283,852, por-spa 77,793. Audio rows: jpn 6,420 (**5,111 with no licence**, 1,282 CC BY-NC 4.0, 27 CC BY 4.0), eng 851,623 (mostly CC BY-NC-ND 3.0), por 20,957 (12,316 none, 8,638 CC BY-NC), spa 119,524 (mostly CC BY-NC-ND 3.0). Updated every Saturday 06:30 UTC. | Example sentences with translations (`sentences[].tr`), audio ids (`audio`), Japanese readings (`transcriptions`, script "Hrkt", which gives furigana for `r`), Tanaka B-lines (`jpn_indices`: sentence → JMdict words), and native-speaker info (`user_languages`: self-rated level 5 = native, a quality filter). | `https://downloads.tatoeba.org/exports/per_language/<lang>/<lang>_sentences.tsv.bz2`, `…/<a>/<a>-<b>_links.tsv.bz2`, `…/<lang>_sentences_with_audio.tsv.bz2`, `exports/transcriptions.tar.bz2`, `exports/jpn_indices.tar.bz2`, `exports/user_languages.tar.bz2`. Audio: `https://tatoeba.org/audio/download/<audio id>`. |
| 5.2 | **JMdict** (EDRDG) | **CC BY-SA 4.0** for the Japanese and English parts. Other-language glosses are © their compilers under the same terms ([EDRDG licence](https://www.edrdg.org/edrdg/licence.html)). Apps must show the acknowledgement on an About/Sources screen and keep the data reasonably current. | 218,909 entries. Glosses: 440 k eng, **68 k spa**, ger, dut, hun, rus, fre, slv, swe. **No Portuguese.** `JMdict.gz` 22 MB, `JMdict_e.gz` 10.6 MB. | Dictionary form, readings, POS, frequency/priority tags (`news1`, `ichi1`, `spec1`, `nfXX`), eng and spa meanings for `words[].tr`. | `http://ftp.edrdg.org/pub/Nihongo/JMdict.gz`. JSON: [scriptin/jmdict-simplified releases](https://github.com/scriptin/jmdict-simplified/releases/latest) (`jmdict-eng`, `jmdict-all`; data CC BY-SA 4.0, weekly). |
| 5.3 | **KANJIDIC2** (EDRDG) | **CC BY-SA 4.0** (SKIP and Pinyin have extra conditions, §8 of the licence). | 13,108 kanji. Meanings in en, **es (8,658), pt (6,963)**, fr. 1.5 MB gz. | Kanji meanings in all 3 of our translation languages, on/kun readings, stroke count, grade, old JLPT level, frequency. | `http://ftp.edrdg.org/pub/Nihongo/kanjidic2.xml.gz`. **RADKFILE/KRADFILE** (EDRDG licence) give kanji components for "build the kanji". |
| 5.4 | **KanjiVG** | **CC BY-SA 3.0**, © Ulrich Apel ([README](https://github.com/KanjiVG/kanjivg)). | ~6.7 k kanji plus kana, one SVG per character with ordered strokes. | Stroke order and paths for tracing exercises and animation (kana included). | [github.com/KanjiVG/kanjivg/releases](https://github.com/KanjiVG/kanjivg/releases) (`kanjivg-<date>-main.zip`). |
| 5.5 | **JLPT word lists** | No official lists since 2010. Community lists by Jonathan Waller ([tanos.co.uk](http://www.tanos.co.uk/jlpt/)) are **CC BY**. [stephenmk/yomitan-jlpt-vocab](https://github.com/stephenmk/yomitan-jlpt-vocab) maps them to JMdict ids (CC BY). [jamsinclair/open-anki-jlpt-decks](https://github.com/jamsinclair/open-anki-jlpt-decks) is **MIT**, with CSVs for N5–N1. | N5 ≈ 700 … N1 ≈ 3,400 words. | Ordering of Japanese words into units and sections (N5 first). | `https://raw.githubusercontent.com/jamsinclair/open-anki-jlpt-decks/main/src/n5.csv` (n4…n1). |
| 5.6 | **Wiktionary via [Kaikki.org](https://kaikki.org/dictionary/)** (wiktextract) | Wiktionary text is **CC BY-SA 4.0** (and GFDL). | English-Wiktionary per-language JSONL: Japanese 385 MB, Portuguese 577 MB, Spanish 1.05 GB, English 3.3 GB (Oct 2026). Native-wiktionary extracts also exist: [pt](https://kaikki.org/ptwiktionary/), [es](https://kaikki.org/eswiktionary/), [ja](https://kaikki.org/jawiktionary/). | Senses, inflected `forms` (→ lemmatising pt/es/en), and **`translations` sections** of English entries (the eng↔por↔spa↔jpn word links, including the **jpn→por** that JMdict lacks), plus IPA. | `https://kaikki.org/dictionary/<Language>/kaikki.org-dictionary-<Language>.jsonl`. |
| 5.7 | **Frequency lists** | [hermitdave/FrequencyWords](https://github.com/hermitdave/FrequencyWords) (OpenSubtitles 2018): content **CC BY-SA 4.0**, code MIT. [wordfreq](https://github.com/rspeer/wordfreq): data CC BY-SA 4.0, frozen since 2024 (SUNSET.md), and the author asks not to strip it into CSVs without attribution. | hermitdave `en_50k`, `pt_br_50k`, `es_50k` (~650 KB each). `ja_full` exists but is weak (subtitle tokenisation). | `rank` for eng/por/spa words, subtitle-flavoured, which suits a video app. For jpn, rank by JMdict priority tags, JLPT level and our own Tatoeba token counts. | `https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/{en,pt_br,es}/{lang}_50k.txt`. |
| 5.8 | **CEFR-J** (English) | Vocabulary and grammar profiles: free for research and commercial use **with citation**, © Tono Lab, TUFS. Octanove C1/C2 list: **CC BY-SA 4.0** ([olp-en-cefrj](https://github.com/openlanguageprofiles/olp-en-cefrj)). | ~7.8 k headwords A1–B2, plus C1/C2. | CEFR level per English word → English units and sections. pt and es have no equally open CEFR list. Use frequency there. | `https://raw.githubusercontent.com/openlanguageprofiles/olp-en-cefrj/master/cefrj-vocabulary-profile-1.5.csv`. |
| 5.9a | **Common Voice** (Mozilla) | **CC0** audio and text. | Thousands of hours for en/es/pt and hundreds for ja (it varies by release). Now distributed through [Mozilla Data Collective](https://mozilladatacollective.com/), where sign-in and accepting terms are needed ([announcement](https://discourse.mozilla.org/t/common-voice-dataset-segments-now-available-on-mdc/149060)). | Native-speaker recordings of short read sentences, freely reusable. A pool for "listen" exercises when Tatoeba has no licensed audio. Quality varies, so use only clips with up-votes. | Via MDC (manual download, large). LATER. |
| 5.9b | **Leipzig Corpora Collection** | Downloadable corpora are **CC BY** ([catalogue entry](https://sprogressource.digst.govcloud.dk/dataset/leipzig-corpora-collection)). Other Wortschatz data is CC BY-NC. | 10 k … 1 M+ sentence corpora per language (news, web, Wikipedia), each with a word-frequency file. | Extra monolingual sentences and frequencies (no translations). Backup source for `rank`. | [wortschatz.uni-leipzig.de/en/download](https://wortschatz.uni-leipzig.de/en/download) (bot-protected, browser download). |
| 5.9c | **Anki shared decks** | Vary. Many have no stated licence. "Core 2k/6k" decks derive from commercial iKnow content. [Kaishi 1.5k](https://github.com/donkuri/Kaishi) has no licence file. [Open Anki JLPT](https://github.com/jamsinclair/open-anki-jlpt-decks) is MIT. | — | **Do not bundle.** Users can already import their own `.apkg` into Study for personal use. Learn can pick up those words through `knowledge.md` flashcards. | AnkiWeb / GitHub releases. |
| 5.9d | **STAPLE 2020** (Duolingo) | CC BY-NC 4.0 | See §2. | Accepted translations en→pt, en→ja. | Dataverse (guestbook). |

### 5.10 Text-to-speech options for a desktop app

| Option | Languages | Licence / risk | Notes |
|---|---|---|---|
| **Windows built-in voices** (SAPI 5 and OneCore/WinRT `Windows.Media.SpeechSynthesis`) | ja (Haruka/Ayumi/Ichiro), pt-BR (Maria/Daniel), es-ES/es-MX (Helena/Laura/Pablo/Sabina/Raul), en (David/Zira/Mark…) once the language's speech pack is installed (Settings → Time & language → Speech). | Part of Windows. Free to use from apps. | **Best default on Windows.** Use the Rust crate [`tts`](https://crates.io/crates/tts) (MIT, WinRT/SAPI on Windows, speech-dispatcher on Linux), or Qt's **QtTextToSpeech** module (engines `winrt` and `sapi`; msys2 package `mingw-w64-ucrt-x86_64-qt6-speech`; a new link dependency, so `CMakeLists.txt` would change). mpv cannot synthesise speech: TTS must produce a WAV that mpv plays, or speak directly. The "Natural" Narrator voices are not officially exposed to SAPI. |
| **Edge "Read aloud" online voices** (as used by the [`edge-tts`](https://github.com/rany2/edge-tts) Python tool) | Excellent neural voices for all 4 languages. | **Unofficial, undocumented endpoint**, not licensed for third-party apps. Microsoft can block or change it at any time. Grey area under the Microsoft Services Agreement. | Do not build on it. At most an opt-in "experimental" setting, off by default. |
| **Piper** (offline neural TTS) | Voices on [rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices): **pt_BR** faber/cadu/jeff (dataset **CC0**), **es_ES** davefx (CC0), es_MX claude (Apache-2.0), es_AR daniela (CC BY-SA 4.0), **en_US** ljspeech (public domain), lessac (Blizzard licence, non-commercial), ryan (CC BY-NC-SA 4.0), **ja_JP** hi_fi_captain (dataset **CC BY-NC-SA 4.0**). About 60–115 MB per voice (`.onnx`). | Engine: the old [rhasspy/piper](https://github.com/rhasspy/piper) was MIT (archived; Windows binaries in its releases). Development moved to [OHF-Voice/piper1-gpl](https://github.com/OHF-Voice/piper1-gpl), which is **GPL-3.0**. Run it as a **separate process** (`piper.exe --model x.onnx --output_file out.wav`) so no GPL code is linked into Reelhouse. | Good offline quality for pt/es/en. Japanese has only one non-commercial voice. Optional download in Preferences, not bundled (size). |

### 5.11 Recommended data pipeline for the course packs

The goal is `assets/learn/{jpn,eng,por,spa}.json` exactly as in [`learn-contract.md`](learn-contract.md) §1, each under
~6 MB.

The builder is an **offline developer tool**, not part of the app, for example `tools/learn-build/` (a Rust binary in the
workspace or a Python script). It runs on the dev machine, caches downloads in `~/.cache/reelhouse-learn/` and is re-run
when Tatoeba or JMdict update.

1. **Download** (§5 URLs):
   - Tatoeba per-language sentences for jpn/eng/por/spa.
   - All 6 pair link files.
   - `*_sentences_with_audio`, `transcriptions`, `jpn_indices`, `user_languages`.
   - jmdict-simplified (`jmdict-all` for spa glosses), KANJIDIC2.
   - JLPT CSVs, CEFR-J CSV, hermitdave 50k lists.
   - Kaikki English-Wiktionary JSONL for Japanese, Portuguese and Spanish, plus a filtered pass over the English one (only
     entries whose `translations` include pt, es or ja).
2. **Clean sentences:**
   - Keep sentences of 3–12 tokens.
   - Drop sentences whose owner is not a self-declared native speaker (`user_languages` level 5), where that information
     exists.
   - Drop duplicates (case and punctuation folded) and sentences with digits, URLs or names (capitalised tokens not in
     the dictionary).
3. **Tokenise and lemmatise:**
   - **jpn:** [sudachi.rs](https://github.com/WorksApplications/sudachi.rs) (Apache-2.0) with SudachiDict (Apache-2.0), or
     [Lindera](https://github.com/lindera/lindera) (MIT) with IPADIC. Take surface `tok` and dictionary form → `w`, and
     check it against JMdict. Use `jpn_indices` (Tanaka B-lines) to confirm the word senses.
   - **eng/por/spa:** a regex tokeniser that keeps punctuation tokens, as the contract requires. Lemmas come from the
     Wiktionary `forms` tables (inflected form → lemma).
4. **Words and translations (`words[]`):**
   - jpn→eng and jpn→spa from JMdict.
   - **jpn→por** from Wiktionary translations (English entry ↔ Japanese and Portuguese), then KANJIDIC2 pt meanings for
     single-kanji words. Leave the key **missing** when unsure (the contract allows it).
   - eng/por/spa between each other from Wiktionary translations.
   - `r` comes from the JMdict reading matching Sudachi's reading. `pos` is mapped to `noun/verb/adj/adv/particle/other`.
5. **Sentence translations (`sentences[].tr`):** direct Tatoeba links first, then **one-hop pivot through English**,
   marked with lower confidence. This matters for jpn-por, which has only 4.9 k direct links: jpn→eng→por gives far more.
   Store only the first translation per language in `tr`. If we want several accepted answers, add an optional
   `alt: {por: [..]}` field to the contract first.
6. **Audio:** set `audio` only when the recording's licence is non-empty (CC BY / BY-SA / BY-NC / BY-NC-ND). Unmodified
   playback with credit satisfies ND and NC in a non-commercial app. Leave it out otherwise, so the app falls back to TTS.
   Record the licence counts in `credits`.
7. **Ranking (`rank`):**
   - jpn: JLPT level (N5 = 1) first, then JMdict priority (`news1`/`ichi1`/`spec1`, `nf01`…`nf48`), then our Tatoeba
     token count.
   - eng: CEFR-J level, then the hermitdave rank.
   - por/spa: hermitdave rank (pt_br list for por).
8. **Units and lessons** (contract rule: ≤ ~6 new words per lesson, and sentences use only introduced words plus
   particles and punctuation):
   - Walk words by `rank`. For each candidate lesson take the next 4–6 words. Pick up to 8 sentences whose content tokens
     are all introduced, preferring sentences that use the new words.
   - If a word has no usable sentence yet, delay it.
   - Group 5 lessons per unit and give units section tags (JLPT N5/N4…, CEFR A1/A2…).
   - Unit titles: "Basics 1…" or the dominant topic, in the 4 UI languages.
   - **jpn extra:** prepend kana units (hiragana then katakana) generated from a built-in table. No sentences there, only
     character matching.
9. **Size check:**
   - Aim for 3,000–4,000 words and 8,000–12,000 sentences per pack. Measured row sizes put that at ~3–5 MB.
   - Trim the tail units if the pack is over 6 MB.
   - Validate: every lesson sentence id exists, `tok` joins back to `t`, and there are no empty `tr` objects.
10. **Credits string** (shown in Study → About/Sources, as EDRDG requires a separate screen):

    > Sentences and audio: Tatoeba (tatoeba.org), CC BY 2.0 FR. Audio by its contributors under the licence shown per
    > recording. Words: JMdict and KANJIDIC2 (EDRDG), CC BY-SA 4.0. Wiktionary via Kaikki.org, CC BY-SA 4.0. JLPT lists:
    > J. Waller (tanos.co.uk), CC BY. CEFR-J Wordlist 1.5 (Tono Lab, TUFS). Frequencies: FrequencyWords (H. Dave) /
    > OpenSubtitles 2018, CC BY-SA 4.0.

    The resulting packs are a CC BY-SA 4.0 adaptation. Say so in `assets/learn/LICENSE.md`.
11. **Optional extras** (separate files, off by default):
    - `assets/learn/extra/staple-en-por.json` and `…-jpn.json` (STAPLE, CC BY-NC) for richer answer checking on
      English → pt/ja prompts.
    - `assets/learn/kanjivg/` (only the kanji used by the pack, ~2 MB) for tracing.

---

## 6. Phased implementation plan

What exists today: Study mode with an Anki-compatible SRS (`src/study.rs`), `.apkg` import, clips from the player, the
`knowledge.md` contract (`src/knowledge.rs`), the notification centre and a `src/learn.rs` stub. The player-quiz
contract is in learn-contract §3.

**Phase 1: Learn MVP.** Everything here is NOW. Suggested owner files: `src/learn.rs`, `src/controller_learn.rs`, and
`qt/qml/Learn*.qml`.

1. Course-pack builder (§5.11) and the first 4 packs, kana units included.
2. `learn.rs`:
   - Load the pack (search order from the contract).
   - Session generator: 12 items, 60% new and 40% review by HLR `p`, Birdbrain-lite target 0.70–0.85.
   - Answer checker (§1.2).
3. Exercises:
   - Select meaning, word-bank translate, type translation, match pairs, fill in the blank.
   - Listen & tap and listen & type, using licensed Tatoeba audio through a second libmpv handle, else Windows TTS.
4. Lesson UI:
   - Check → feedback banner → Continue, re-queue on wrong, combo counter.
   - Finish screen with XP, accuracy and time.
   - Keyboard flow. Own sounds.
5. Path UI with units, zig-zag nodes, locked/current/done states and a "Practice" (personalized) button.
6. Progress file: XP per day, daily goal ring, streak with 04:00 day start, 2 freezes (granted at 7/14/30-day milestones
   while there is no shop).
7. `knowledge.md` updates per answered word (`record`), so the player quizzes and the Anki side see the same knowledge.
8. Words list page. Kana chart (recognition drills).
9. Settings (each needs field, default, load and save, as with `study_hints`):
   - `learn.daily_goal_xp`, `learn.hearts`, `learn.sounds`, `learn.motion`, `learn.show_reading`, `learn.show_romaji`.

**Phase 2: motivation layer.**

- Daily quests (3/day) with a chest, a monthly badge, and gems with a local shop (freeze, XP boost, heart refill,
  legendary entry).
- Achievements page. Perfect lesson and combo XP. Unit chests on the path.
- **Mascot** (original, 5 poses) on the path, in lessons and on the finish screen.
- **Reminders:** in-app notices plus `notify-rust` toasts, template rotation with the KDD recency penalty, max 2 per day,
  quiet hours, habit-time scheduling, streak-saver at 20:00.
- Legendary runs. Placement test and "jump to unit" test. Mistakes practice.

**Phase 3: Japanese and media-powered content.**

- Characters tab: hiragana and katakana grids with strength, the kanji grid from learnt words, KanjiVG stroke animation
  and tracing.
- **Scenes** (the Stories equivalent) from the user's own subtitles and saved clips. This is Reelhouse's unique angle and
  connects to the player quiz marks (learn-contract §3).
- **Reel Radio** (DuoRadio equivalent) from TTS dialogues or clip audio.
- Speak exercises with an optional whisper.cpp or Vosk model download. Piper voices as an optional download.

**Phase 4: social and AI (needs a server or user keys).**

- Leagues: personal or practice-bot league first. Real leagues and friend quests only if Reelhouse ever gets a sync
  service.
- "Explain my answer" and roleplay chat via a user-provided LLM API key.
- Optional STAPLE answer packs. Common Voice audio pool.

**Not planned:** the Energy system (a monetisation lever), Adventures (a 3D game), real-money purchases, copying any
Duolingo content or assets.

---

## Sources

Official Duolingo:

- [Duolingo 101](https://blog.duolingo.com/duolingo-101-how-to-learn-a-language-on-duolingo)
- [Home-screen path redesign](https://blog.duolingo.com/new-duolingo-home-screen-design)
- [Review lessons](https://blog.duolingo.com/how-to-review-lessons-on-duolingo)
- [Duolingo Score](https://blog.duolingo.com/duolingo-score/)
- [Energy](https://blog.duolingo.com/duolingo-energy/)
- [Leagues](https://blog.duolingo.com/duolingo-leagues-leaderboards/)
- [Streak](https://blog.duolingo.com/how-duolingo-streak-builds-habit)
- [Friends Quests](https://blog.duolingo.com/friends-quests/)
- [Birdbrain](https://blog.duolingo.com/learning-how-to-help-you-learn-introducing-birdbrain)
- [Notifications AI](https://blog.duolingo.com/hi-its-duo-the-ai-behind-the-meme/)
- [Japanese characters](https://blog.duolingo.com/learning-to-read-japanese-characters)
- [DuoRadio](https://blog.duolingo.com/duoradio-listening-practice)
- [Scaling DuoRadio](https://blog.duolingo.com/scaling-duoradio/)
- [Adventures](https://blog.duolingo.com/adventures)
- [Duolingo Max](https://blog.duolingo.com/duolingo-max)
- [Character voices](https://blog.duolingo.com/character-voices)
- [research.duolingo.com](https://research.duolingo.com/)
- [HLR paper](https://research.duolingo.com/papers/settles.acl16.pdf)
- [Bandit paper](https://research.duolingo.com/papers/yancey.kdd20.pdf)
- [STAPLE](https://sharedtask.duolingo.com/)
- [SLAM](https://sharedtask.duolingo.com/2018.html)

Press:

- [IEEE Spectrum on Birdbrain](https://spectrum.ieee.org/duolingo)

Community:

- [Duolingo wiki: Exercise](https://duolingo.fandom.com/wiki/Exercise)
- [Duolingo wiki: XP](https://duolingo.fandom.com/wiki/XP)
- [Duolingo wiki: Energy](https://duolingo.fandom.com/wiki/Energy)
- [Deconstructor of Fun teardown](https://duolingo.deconstructoroffun.com/mechanics/leagues)
- [Duoplanet on quests](https://newsletter.duoplanet.com/posts/how-many-quests-duolingo)

Code and data:

- [sanidhyy/duolingo-clone](https://github.com/sanidhyy/duolingo-clone)
- [morluto/rea](https://github.com/morluto/rea)
- [Tatoeba downloads](https://tatoeba.org/en/downloads)
- [EDRDG licence](https://www.edrdg.org/edrdg/licence.html)
- [jmdict-simplified](https://github.com/scriptin/jmdict-simplified)
- [KanjiVG](https://github.com/KanjiVG/kanjivg)
- [Kaikki](https://kaikki.org/dictionary/)
- [FrequencyWords](https://github.com/hermitdave/FrequencyWords)
- [wordfreq](https://github.com/rspeer/wordfreq)
- [CEFR-J (OLP)](https://github.com/openlanguageprofiles/olp-en-cefrj)
- [Open Anki JLPT](https://github.com/jamsinclair/open-anki-jlpt-decks)
- [yomitan-jlpt-vocab](https://github.com/stephenmk/yomitan-jlpt-vocab)
- [Piper voices](https://huggingface.co/rhasspy/piper-voices)
- [piper1-gpl](https://github.com/OHF-Voice/piper1-gpl)
- [Mozilla Data Collective](https://mozilladatacollective.com/)
