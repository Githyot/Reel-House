# Study while reading: the player's tricks in the Text and Images modes

Written 2026-10-14. The player already studies with the user while they watch (`src/studyplay.rs`,
`src/wordmix.rs`, `src/mining.rs`): question cards about known lines, parts without subtitles, learned words mixed
into subtitles in a known language, and cards mined from single words. This page says how the same ideas work in the
readers (novels in Text mode, manga in Images mode), and which core functions they share.

The same settings decide in all three places (Preferences > Study, `StudySettings.qml`): the main switch
`general.study_in_player`, then `study_quiz_cards`, `study_quiz_listen`, `study_wordmix` (+ `study_wordmix_rate`,
`study_wordmix_reading`), `study_mining` (+ `study_mining_max`), `study_mascot_cues`. "Known" always means the
knowledge state (`knowledge.md`, docs/learn-contract.md §2).

## 1. Word mixing in novel paragraphs (Text mode)

When the chapter is written in a language the user knows (not the study language), one or two words of some
paragraphs are written in the study language instead ("Eu quero 食べる agora").

- Language of the text: the novel's language (plugin / source language), else a guess from a sample.
- Which paragraphs: `wordmix::picked(paragraph, study_wordmix_rate)` — decided from the text itself, so a paragraph
  is mixed the same way every time the chapter is opened. Split long paragraphs into sentences first and mix per
  sentence (`wordmix::max_for` gives 1 word, 2 in sentences of six words or more).
- Which words: only words recalled with strength ≥ `wordmix::MIX_MIN_STRENGTH` (0.4); whole words or phrases of the
  translation, case kept, grammar words never.
- Drawing: the reader gets `(text, spans)`; draw each span in the accent (or underlined), its reading small after it
  when `study_wordmix_reading` and it has one, and the original word in a tooltip / on long press. Tapping a mixed
  word may flip it back to the original for a moment (no penalty) and records nothing.
- Recording: seeing a mixed word is not an answer; do not call `Knowledge::record`. (A tap on "I didn't know it" may
  record a wrong answer — optional.)
- The table of learned words is built once per language pair and kept (`Core::wordmix_swap`); call
  `Core::wordmix_forget()` after a Learn lesson or a review session so the next chapter uses the new words.

## 2. Known-sentence quizzes between pages (Text mode)

When the chapter is written in the study language, the player's "card" question becomes a quiz card between pages:

- Every N paragraphs (N from `study_quiz_rate` with `studyplay::gap_for`-like spacing: rate 100 → about every 3
  pages, rate 1 → about every 20), pick the paragraph's sentence whose words are all known
  (`Knowledge::line_known(lang, sentence)`, 3–12 words, not a heading).
- Card kinds, the same as `StudyQuiz.qml`: choose the meaning (translations of other sentences of the chapter as
  distractors, the sentence's own translation from the reader's translator), put the words in order
  (`Knowledge::tokens`), the meaning of one word (`WordRow::tr`).
- The answer goes to `Knowledge::record_line` / `record` and `knowledge.save()`, exactly like `study_quiz_answer`.
- "Parts without subtitles" have no reading equivalent; the closest is a listening card: play the sentence with the
  reader's text-to-speech (if any) and ask to type it. Only when `study_quiz_listen` is on and a voice exists.
- The macaw (`study_mascot_cues`): it pulls the quiz card in between the pages (pose `pull`) and pushes it away
  (`push`), the same as over the video (`Core.mascotCue` for anything outside the card).

## 3. Bubble-text mining in manga (Images mode)

The Images reader already reads the text of a page (OCR blocks, `crate::ocr::Block`) and makes picture cards
(`study_add_picture`, deck "Seu Reels").

- Mining: when the manga is in the study language and a bubble holds a single word (`mining::isolated_word`, e.g.
  "ありがとう！", "うるさい"), offer a card: front = the crop of the bubble (the picture card path), back = the word,
  its reading (knowledge row or course pack) and its translation in the main known language. At most
  `study_mining_max` per chapter, never the same word twice in a chapter (tag the notes like `mining.rs` does).
  Automatic only when `study_mining` is on; otherwise a "make a card" button on the bubble.
- Quiz: a bubble whose text is all known (`readstudy::known_texts` / `Knowledge::line_known`) can show a small "do
  you understand this?" chip; tapping it opens the same quiz card as Text mode.
- Word mixing: when the manga is translated into a known language (the reader's translation overlay), the
  translated bubble text goes through `wordmix_swap` like a subtitle line, so learned words appear in the bubbles.

## 4. Shared core API

Module `crate::controller::wordmix` (pure functions, no `Core` needed):

```rust
pub const MIX_MIN_STRENGTH: f64;               // 0.4: how well a word must be known to be mixed in
pub struct Span { pub start: usize, pub end: usize,          // char offsets in the NEW text
                  pub word: String, pub original: String, pub reading: String }
pub struct Lexicon { pub from: String, pub to: String, .. }  // 3-letter codes
impl Lexicon {
    pub fn build(k: &Knowledge, pack: Option<&learn::Pack>, study3: &str, from3: &str,
                 min_strength: f64, all_pack: bool, now: i64) -> Lexicon;
    pub fn lookup(&self, known_word: &str) -> Option<&Entry>;  // Entry { target, reading, strength }
    pub fn add(&mut self, translation: &str, target: &str, reading: &str, strength: f64);
}
pub fn swap_line(lex: &Lexicon, text: &str, max: usize) -> (String, Vec<Span>);
pub fn max_for(text: &str) -> usize;           // 1, or 2 for six words or more
pub fn picked(text: &str, rate: u32) -> bool;   // the same answer for the same text
pub fn pieces(text: &str, spans: &[Span]) -> Vec<(String, Option<Span>)>;  // for drawing
pub fn words(text: &str) -> Vec<String>;
```

On `Core` (uses the knowledge state, the course pack and the settings, caches the table):

```rust
pub fn wordmix_swap(&mut self, text: &str, from_lang: &str) -> (String, Vec<Span>);  // 2- or 3-letter code
pub fn wordmix_forget(&mut self);
```

Module `crate::controller::mining` (pure parts): `isolated_word(text) -> Option<String>`,
`select(lines, gap, avoid) -> Vec<Candidate>`, `gate(audio, sub, study, known)`, `add_mined(col, &Mined, now)`
(note type `MINED`, deck "Seu Reels", one word per episode / chapter tag).

`src/readstudy.rs` (readers) has its own small `swap_line` adapter; it can now call `Core::wordmix_swap` (or
`wordmix::swap_line` with a `Lexicon` it keeps) and turn the spans into its `Piece`s with `wordmix::pieces`.
