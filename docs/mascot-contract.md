# Mascot "Arara" and in-app banners — shared contract (2026-10-14)

## Mascot (QML)
`qt/qml/Mascot.qml` — the blue macaw (arara-azul) drawn from SVGs in `qt/mascot/arara-<mood>.svg` (qrc `:/mascot/…`).

```qml
Mascot {
    mood: "idle"      // idle | happy | cheer | sad | tired | sleep | think | wave | surprised
    size: 120         // height in px; width follows the drawing
    say: ""           // optional speech bubble text (empty = no bubble)
    bubbleSide: "right"  // right | left | top
    animate: true     // gentle idle bob / blink / wing flap per mood
}
```

## Banners (phone-style pop-up at the top of the window)
- Rust: `Core::banner(&mut self, kind: &str, title: &str, body: &str, mood: &str, action: Option<Json>)` sends
  `Event::Banner` → bridge event `{"e":"banner", kind, title, body, mood, action}`.
- C++: `CoreBridge` emits `banner(QVariantMap b)`; QML may also call `Core.showBanner({kind, title, body, mood, action})`
  (Q_INVOKABLE, same path, for banners born in QML such as the Pomodoro timer).
- `action` is a command object sent with `Core.cmd(action)` when the banner is clicked (e.g. `{cmd:"set_mode", mode:"study"}`).
- `kind`: `streak` | `goal` | `quest` | `review` | `pomodoro` | `lesson` | `achievement` | `info`.
- Shown by `qt/qml/BannerHost.qml` (in Main.qml, above everything, also over the player): slides down from the top
  centre, mascot on the left, title + body, auto-hides after ~5 s, queue of banners, click = action, swipe up / × to close.
- Settings: `general.banners` (bool, default true), `general.banner_quiet_from`/`_to` (hours, quiet time).
- The core schedules study reminders (streak at risk in the evening, daily goal not met, reviews due, quests nearly
  done, inactivity "the macaw is waiting") with a cooldown so the same kind is not repeated within a few hours; the
  messages vary (a pool per kind, picked so recent ones are not repeated), in the UI language.

## Additions (2026-10-14 17:33)
- New moods/poses (tools/mascot.py): `fly` (wings spread, flying), `shh` (wing tip on the beak, eyes sideways — "no subtitles"), `peek` (magnifier in front of one eye — "watching the subtitle"), `pull` (leaning back, holding the top edge of a card / banner with the feet or wings), `push` (leaning forward, wings pushing).
- Global cues: `Core.mascotCue({cue, corner, ms, text})` (Q_INVOKABLE that emits `mascotCue(QVariantMap)`), shown by `qt/qml/MascotStage.qml` hosted in Main.qml above pages and the player:
  - `cue: "shh"` → appears at `corner` (default bottom-right of the video), shh pose, stays `ms` (or until `Core.mascotCue({cue:"hide"})`).
  - `cue: "peek"` → small, discreet, only the magnifier + eye visible from the corner edge for `ms`.
  - `cue: "cheer" | "wave" | "happy" | "sad" | "think"` → pops in at the corner with an optional speech bubble `text`.
  - `cue: "hide"` → flies away.
- Banners: the mascot drops in from the top (fly pose), grabs the banner and pulls it down into view, then flies off upward when the banner leaves (BannerHost).
- Flashcards over the video (StudyQuiz): the mascot pulls the card in from the side (pull pose) and pushes it out after the answer (push pose) — StudyQuiz embeds Mascot itself.
- In Study mode the mascot is always present (corner of the Study pages); elsewhere only at opportune moments (cues).

## As built (2026-10-14, mascot agent)
- Files: `qt/mascot/arara-{fly,shh,peek,pull,push}.svg` + `-blink` for each, `-b` (second frame) for fly / pull / push;
  WebP copies in `qt/mascot/webp/`. `Mascot.qml` knows the new moods; fly flaps all the time (120 ms), pull/push tug
  (300 ms). `Mascot { mirror: true }` faces the other way (bubble unaffected).
- Pull pose geometry: the wing tips hold a horizontal edge ~22–26 % down the picture, centred; push faces right (mirror for left).
- `Core.mascotCue(map)` is the Q_INVOKABLE; the signal is `mascotCued(QVariantMap)` (a signal cannot share the name).
  `ms` defaults: shh 0 (stays until hide), peek 4500, the moods 3500. `corner`: bottom-right | bottom-left | top-right
  | top-left. A peek leaves by slipping back out of the edge; the others fly away. Click on the bird = hide.
  MascotStage is in Main.qml at z 9 (above pages and the player, below the night filter / dialogs), `bottomInset` 110 in the player.
- BannerHost: `calm: true` = no courier, plain slide (no app-wide reduced-motion setting exists yet).
- Study game state: `Core.study.game` = {xpToday, goal, streak, streakToday, boost, combo, bestCombo, xp, answered,
  correct, accuracy, fire}. Each right review answer = 1 XP (2 XP from 10 in a row), Again = 0; it goes through
  `learn::Progress::add_xp` (day total, daily goal, XP quest, streak, boost); undo takes it back (`remove_xp`).

## Funny lines (2026-10-15)
- `src/mascot_lines.rs`: 230 Duolingo-style lines, each `(en, pt-BR, es)` in one const table per moment ("{n}" = a number).
- Banner moments (notify_study.rs): `streak`, `goal`, `review`, `quest`, `idle`, `greet_*` — the lines are extra bodies
  of those pools (`pool_len` = own pairs + lines; the title comes from the pool), same no-repeat log (`study/nudges.json`).
- Window moments, asked by QML with `Core.state("mascot:" + Core.uiLang + ":" + moment)` → `{text}` (random, never the
  last one of that moment; `""` for an unknown moment): `review_right` / `review_wrong` (StudyReview, now and then),
  `lesson_finish` / `lesson_perfect` (LearnLesson finish screen), `learn_cheer` (LearnPage), `study_hello` (StudyPage,
  no decks), `buddy_done` / `buddy_empty` / `tap` (StudyBuddy; clicking the bird), `chapter_done` (manga / novel reader),
  `quiz_right` / `quiz_wrong` (ReaderQuizCard). Each QML file has a small `quip(m)` helper; the old `tr()` texts stay as fallbacks.
