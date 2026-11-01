# Study mode (flashcards)

The fourth mode (Video, Images, Text, **Study**). It is a native, Anki-compatible flashcard system, not the Anki program itself
(Anki is Python + a Rust backend + web screens; none of that can live inside the Qt app).

- Core: `src/sqlite.rs` (read-only SQLite reader), `src/study.rs` (decks, notes, cards, scheduler, templates, cloze, `.apkg` import),
  `src/controller_study.rs` (session and commands). The collection is `<data>/study/collection.json`, media in `<data>/study/media/`.
- Scheduler: the classic Anki one. Learning steps 1 min and 10 min, relearning 10 min, graduating interval 1 day (4 days on Easy), ease 250 %,
  Hard x1.2 (-15 %), Good x ease, Easy x ease x1.3 (+15 %), Again = lapse (-20 % ease). Day starts at 04:00. Limits: 20 new and 200 reviews per day per deck.
- Import: `.apkg` made by Anki with "Support older Anki versions" ticked (`collection.anki21` / `.anki2` with JSON note types). The newest
  format (`collection.anki21b`, zstd + protobuf) is refused with a message that says how to export again. Scheduling data, media and the
  deck tree (`A::B`) are kept; suspended cards are kept out of the queue. Sound tags are dropped (no audio yet).
- Bridge: state `study` (decks with counts, session with the rendered card), commands `study_open/close/show/answer/create_deck/delete_deck/import`.
- UI: `StudyPage.qml` (deck table, review with Again/Hard/Good/Easy, keys Space/Enter/1-4/Esc). The mode switch has four entries in every theme
  navigation (header Mode menu, `SidebarNav`, `RailNav`, `BottomNav`). Everything uses the theme tokens, so it follows GNOME, Apple, Windows and Android.
- Add cards (`StudyAdd.qml`: Basic, Basic and reversed, Cloze; one card per cloze number), card browser with search and editing (`StudyBrowse.qml`), statistics (`StudyStats.qml`: today, 30-day retention, cards by kind, reviews per day, forecast for 14 days; history is kept per day for all decks).
- Not yet: tags editing, custom note types, FSRS, audio, undo, the newest `.apkg` format, export.
- Test aids: `REELHOUSE_TEST_CMDS='[{"cmd":"set_mode","mode":"study"},...]'` sends commands one per second after start.


## Anki checklist
See `docs/anki-checklist.md` — it lists every Anki feature and what is done. QT40 added undo, suspend, bury, flag, mark, reset and set-due.

## Clips from the player (Editor)
In the player bar, **Editor** (scissors): mark the start and the end (Now, ±0.5 s, Preview) and **Save to the Anki deck**.
The card goes to the deck `Reelhouse::Clips` (note type "Reelhouse clip") with: the subtitle of that time (read from the subtitle file the player shows; for a subtitle inside the video, the line on screen), the words with kanji and their **hiragana** and **katakana** readings (Jisho API), each kanji with meaning, on/kun readings, JLPT level and strokes (trimmed davidluzgouveia/kanji-data in `assets/kanji-min.json`), meaning and a short description from the Japanese Wikipedia, a picture of the first frame and the clip itself (cut by libmpv's encoder into the Study media folder; H.264/AAC 480p). In review the picture is shown and "▶ Play the clip" opens the video in the system player.
Limits: words are found from runs of kanji (no morphological analyser), the clip needs a libmpv with the libx264 encoder (otherwise the card has the text only), the video is not played inside the card.

## Flash cards in the subtitle
While a video plays, each new subtitle (read from the player, so it comes up together with the subtitle) is searched for words with kanji that are in your Study decks: the cards of the Editor give their words with reading and meaning; other notes give their first field (up to 16 characters, with kanji) and the second as meaning. A small card at the bottom right shows the word, its reading and the translation for 5 s. The same word is not shown again within 45 s. Preferences → Subtitles → Study turns it off.
Limits: matching is by text (a word is found when it appears as written, so conjugated verbs only match in their written form); cloze notes are not used.

## Trim bar of the Editor

`TrimTimeline.qml`: two round handles (start, end) on the bar, times down to the millisecond, playhead and time marks. The bar zooms by the length of the part: 10 s or more, the whole video (zoom 0); under 10 s, 30 s on the bar (zoom 1); under 5 s, 15 s (zoom 2); under 2 s, 6 s (zoom 3). While a handle is dragged the bar only zooms in, around the handle; it zooms out again when the handle is released. A handle held near an edge pans the view. Click or drag on the bar to seek; arrow keys move the last handle by one pixel of time (Shift: ten). The player keeps `clipStart` / `clipEnd`; the component gets `startTime`, `endTime`, `position`, `duration` and sends `startEdited`, `endEdited`, `seekRequested`. Test: `REELHOUSE_TEST_OPEN=player:editor:<start>:<end>`.
