# Learn (Duolingo-style) — shared contracts between the parts

Written 2026-10-14 so the data builder, the Learn engine, the player quizzes and the preferences agree. Change a
contract here first, then the code.

Language codes: ISO 639-3 as Tatoeba uses them: `jpn`, `eng`, `por` (Brazilian Portuguese), `spa`.
The app's settings mostly use 2-letter codes (`ja`, `en`, `pt`, `es`); convert with one helper
(`crate::learn::lang3("pt") == "por"`, `lang2("por") == "pt"`).

## 1. Course packs (data builder → Learn engine)

Files: `assets/learn/<lang3>.json` in the repository, one per language being learned (`jpn.json`, `eng.json`,
`por.json`, `spa.json`). Copied next to the program as `learn/` by the build script (`msys-build-qt.sh`).
The core looks for them, in order: `<exe dir>/learn/`, `<data dir>/learn/`, `<repo>/assets/learn/` (development).
UTF-8 JSON, no comments. Keep each file under ~6 MB.

```json
{
  "lang": "jpn",
  "version": 1,
  "credits": "Sentences and audio: Tatoeba (tatoeba.org), CC BY 2.0 FR …; words: JMdict (EDRDG), CC BY-SA 4.0 …",
  "words": [
    { "w": "食べる", "r": "たべる", "rank": 312, "pos": "verb",
      "tr": { "eng": "to eat", "por": "comer", "spa": "comer" } }
  ],
  "sentences": [
    { "id": 4707, "t": "私は学生です。", "r": "わたしはがくせいです。",
      "tok": ["私", "は", "学生", "です", "。"],
      "tr": { "eng": "I am a student.", "por": "Eu sou estudante.", "spa": "Soy estudiante." },
      "audio": 12345 }
  ],
  "units": [
    { "id": 1, "title": { "eng": "Basics 1", "por": "Básico 1", "spa": "Básico 1", "jpn": "きほん 1" },
      "lessons": [ { "id": "1-1", "words": ["私", "は", "学生", "です"], "sentences": [4707, 2032] } ] }
  ]
}
```

- `w` is the dictionary form; `r` the reading (kana) for Japanese, omitted for other languages.
- `tok` holds the tokens of the sentence as they appear in `t`; joining them gives `t` back for Japanese, and with
  single spaces (punctuation attached) for the other languages. Punctuation tokens are kept.
- `tr` holds the translations into the other three languages when known (a missing key means none). A word may
  lack translations into some languages.
- `lem` (optional, same length as `tok`) holds the dictionary form of each token (`"急い"` → `"急ぐ"`, `""` for
  punctuation). The engine matches lesson words and hints by it, and teaches the knowledge tokeniser these forms.
- `audio` is the Tatoeba audio id (play `https://tatoeba.org/audio/download/<id>`), or absent.
- `units` are ordered from easiest; each lesson introduces up to ~6 new `words` and lists sentences that use only
  words introduced so far (plus particles/punctuation).

## 2. Knowledge state (Learn engine owns it; the player and Study read it)

File: `<data dir>/study/knowledge.md` — readable Markdown, also parsed back by the app (keep the table layout).

```markdown
# Current knowledge
- Study language: jpn
- Translation language: por
- Known languages: por, eng, spa
- Updated: 2026-10-14T10:00:00Z

## Words — jpn
| word | reading | seen | right | wrong | strength | last seen | por | eng | spa |
|---|---|---|---|---|---|---|---|---|---|
| 食べる | たべる | 12 | 9 | 3 | 0.82 | 2026-10-14 | comer | to eat | comer |

## Characters — jpn
| character | romaji | seen | right | wrong | strength | last seen |
|---|---|---|---|---|---|---|
| か | ka | 4 | 3 | 1 | 0.91 | 2026-10-14 |
| カ | ka | 2 | 2 | 0 | 0.97 | 2026-10-14 |

## Lessons — jpn
| unit | lesson | title | completed | plays | xp | accuracy | words |
|---|---|---|---|---|---|---|---|
| 1 | 1-1 | Basics 1 · 1 | 2026-10-14 | 2 | 30 | 92% | 私, は, 学生, です |
| 1 | legendary-1 | Basics 1 · Legendary | 2026-10-14 | 1 | 50 | 100% |  |
| Hiragana | kana-h-2 | か き く け こ | 2026-10-14 | 1 | 15 | 85% | か, き, く, け, こ |

## Flashcards
| id | kind | deck | note | where | front | back |
|---|---|---|---|---|---|---|
| a:1700000000 | anki | Japanese V2K | 1700000000 | study/collection | 食べる | to eat |
| clip:9876 | clip | Reelhouse::Clips | 9876 | study/media/clip_9876.mp4 | ご飯を食べる | comer arroz |
```

Section order: header, `Words — <lang>` (study language first), `Characters — <lang>`, `Lessons — <lang>`, `Flashcards`.
Readers find columns by their names, so a column added later does not break them; an unknown `##` section is skipped.

- Every word the user saw (a lesson, a review, a flashcard) is a row; the translations columns follow the known
  languages (one column each).
- `strength` is 0..1 (half-life regression style recall probability at the last update).
- Every challenge of a Learn lesson records the words it asked about (`Challenge::words`): the word of a select card,
  each pair of a match, and the words of the sentence of word-bank, translation, fill-in and listening cards. When a
  lesson ends, the words it introduces get a row even if no card asked them (seen 0), and the file is written at once.
- `Characters — jpn`: the kana practised in the Characters view (hiragana and katakana are separate rows); same HLR
  strength as the words. Kept apart from the words so the tokeniser and `knows` never see them.
- `Lessons — <lang>`: one row per lesson finished in Learn (path lessons `1-1`, legendary rounds `legendary-<unit>`,
  characters lessons `kana-<h|k>-<n>`), updated when played again: `completed` is the last time, `plays` how many
  times, `xp` the XP earned in it over every play, `accuracy` of the last play, `words` what it introduces.
- The Flashcards table is the index the player uses to find "a matching card" for a subtitle line.

### Rust API (module `src/knowledge.rs`, field `Core::knowledge`)

```rust
pub struct Knowledge { /* … */ }
impl Knowledge {
    pub fn load() -> Knowledge;                       // reads knowledge.md (empty if missing)
    pub fn save(&self);                               // writes knowledge.md
    pub fn study_lang(&self) -> String;               // "jpn"
    pub fn tokens(&self, lang: &str, text: &str) -> Vec<String>;   // words of a line (no punctuation)
    pub fn knows(&self, lang: &str, word: &str) -> bool;
    pub fn line_known(&self, lang: &str, text: &str) -> bool;      // every word of the line is known (≥1 word)
    pub fn record(&mut self, lang: &str, word: &str, correct: bool);
    pub fn record_line(&mut self, lang: &str, text: &str, correct: bool);
    pub fn card_for_line(&self, text: &str) -> Option<CardRef>;    // best flashcard for this text
    pub fn record_char_at(&mut self, lang: &str, ch: &str, romaji: &str, correct: bool, now: i64);
    pub fn char_row(&self, lang: &str, ch: &str) -> Option<&WordRow>;
    pub fn record_lesson(&mut self, lang: &str, row: LessonRow);   // adds or updates the row of row.id
    pub fn introduce(&mut self, lang: &str, word: &str);           // a row with no answers yet
}
pub struct CardRef { pub id: String, pub kind: String, pub deck: String, pub note: i64, pub location: String, pub front: String, pub back: String }
```

Japanese tokenising at run time has no morphological analyser: `tokens` does a longest match against the known
words, the course words and a small built-in list of particles/endings; unknown runs of kana/kanji become one token.

## 3. Player quiz events (player integration)

- Before playing, the core compares every subtitle line in the study language with the knowledge state and sends
  `player.studyMarks: [{t, end, kind: "card" | "listen"}]` (shown on the seek bar).
- `kind: "card"`: after the line ends, pause and show a Duolingo-style card (the line's audio, choose/type the
  translation, or the matching Anki card); answer → right/wrong → play on. Records with `record_line`.
- `kind: "listen"`: a notice "The next part plays without subtitles", subtitles hidden for the part, pause at its
  end, type what was heard, check, play on.
- Commands: `study_quiz_answer {id, answer}`, `study_quiz_skip {id}`; setting `general.study_in_player` (bool) and
  `general.study_quiz_rate` (0..100, how often).

## 4. Languages the user knows (preferences)

- `general.known_langs: Vec<String>` (2-letter, ordered by preference, first = main) and
  `general.study_lang: String` (2-letter, the language being learned, may be empty).
- Subtitles: if the video offers subtitles in a known language, load the first in preference order; if none, load
  the best available and translate it automatically to the main language.

## 5. The Learn area (engine ↔ window)

- Progress in `<data dir>/study/learn.json` (XP per day, streak, freezes, hearts, gems, goal, quests, achievements,
  finished lessons per course, recent mistakes). Course = `general.study_lang` (else the one chosen in Learn);
  translation language = first of `general.known_langs` the pack has (else the app language).
- State `"learn"` (`Core.state("learn")`, refreshed on the `study` event): course, units with lessons
  (`state`: done | current | locked), xp/streak/hearts/gems/goal, quests, achievements, calendar, weeks, and
  `session` (status question | right | wrong | no_hearts | done, the current `challenge` — its `answer` only after
  checking — `verdict`, `result`).
- Commands: `learn_open {tz}`, `learn_start {lesson}` (a path lesson `1-1`, a legendary round `legendary-<unit id>` of
  a finished unit, a characters lesson `kana-<h|k>-<n>`), `learn_answer {answer, missed}`, `learn_continue`,
  `learn_quit`, `learn_practice {mode: weak | mistakes | kana-h | kana-k}`, `learn_set_goal {xp}`,
  `learn_buy {item: refill | freeze | boost}`, `learn_chest` (the daily chest, once the goal of the day is reached),
  `learn_set_course {lang}`, `learn_play {url, slow}` (a Tatoeba address, or `kana:<romaji>` for a file of
  `kana/`). Every command may carry `tz` (minutes east of UTC).
- Session kinds: `lesson`, `practice`, `mistakes`, `legendary` (loses hearts like a lesson, 40 XP + 10 when perfect),
  `kana` (challenge kinds `kana_romaji`, `romaji_kana`, `kana_listen`, `match`, `kana_trace`; the window judges the
  tracing and sends `ok` or `no`).
- Gems (src/learn.rs constants, shown by the shop's "How to get gems" from `gem_rules`): +5 per lesson or practice,
  +3 more with no mistakes, daily quests 10–20 each, achievements +20, streak milestones 7 / 30 / 100 days
  (+25 / +50 / +100), the daily chest 5–20 (one in four also gives an XP boost). Shop: refill 60, freeze 50,
  XP boost 80 (double XP for 15 min, stacks).
- More state keys: `gem_rules`, `chest_ready`, `chest_opened`, `boost_left` (seconds), `price_boost`, units `level`
  (0 / 1 finished / 2 legendary) and `legendary`, session `best_combo` and `boost`, result messages `streak:N`,
  `milestone:N`, `goal`, `chest`, `boost`, `achievement:<id>`; `kana` (Japanese only): `hira` / `kata` charts
  (sections gojuon / dakuten / yoon, rows of cells `{c, r, s, seen, snd}` or null), `lessons_h` / `lessons_k`
  (`{id, n, chars, state: done | current | todo}`), `learned_h`, `learned_k`, `total`, `sounds`.
- Banners (docs/mascot-contract.md) sent by the core when a lesson ends: `streak` (extended, or a milestone), `goal`
  (daily goal reached; and the daily chest when opened), `achievement`.
- Kana sounds: `assets/kana/<romaji>.ogg` (credits and licences in `assets/kana/CREDITS.md`), copied next to the
  program as `kana/` by `msys-build-qt.sh`; looked for in `<exe dir>/kana/`, `<repo>/assets/kana/`, `<data dir>/kana/`.
- Setting `general.learn_unlimited_hearts` (bool).
- The core reads the course of the study language at start (`Core::learn_ensure`), so `knowledge.tokens` knows its
  words (and their written forms) for the player too.
