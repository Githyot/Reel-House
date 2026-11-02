# Reelhouse

A Stremio-style media center: a Rust core (`src/`) with a **Qt 6 / Qt Quick** interface (`qt/`, through the C bridge in `qtbridge/`) and four themes (GNOME, Windows, Android, Apple). Video plays through a built-in **mpv** (libmpv) renderer.

## Features
- Discover page (Cinemeta + Netflix catalog addon), Library, search, hero banner, detail page with TMDB artwork
- Stremio addons (add by URL in Preferences > Addons; AniSync and SubSource: paste your personal install link)
- Playback **with or without Real-Debrid**: without it, torrents stream through the rqbit engine (downloaded automatically on first use)
- Player controls in Qt Quick (no mpv OSC): seek bar with thumbnail previews, Skip 90 s button, quality/audio/subtitle menus
- Anime4K upscaling (tab "Upscalling")
- Subtitles: translation via LibreTranslate, Claude, Gemini, GPT/OpenAI-compatible; full style editor (font, colors, outline, shadow, underline, background, opacity)
- Anime Skip intro/ending skip button, with login in Preferences > Accounts (you need your own Anime Skip client ID)

- Player extras: playback speed, statistics (peers, speed, completion, info hash), subtitle delay and position, next-episode card with auto-play, Stremio-style loading screen with the show logo
- Platforms menu (Netflix, HBO Max, Anime, Cinemeta…) to filter Discover
- Artwork button on every title page: pick your own cover/background or hide them (for NSFW covers)
- Unwatched episode pictures are blurred (can be turned off)
- Preferences: size slider 50–200 %, UI language (English, Português, Español), rebindable shortcuts
- Game controller support (A play/pause, B back, X skip, Y fullscreen, D-pad seek/volume), with the model shown when it connects
- Mouse back/forward buttons

## Build on Windows
1. Install MSYS2 (https://www.msys2.org) and the GitHub CLI (`gh auth login`).
2. In PowerShell: `Set-ExecutionPolicy -Scope Process Bypass; .\build-qt.ps1`
3. Result: `qt\build\reelhouse-qt.exe`. Logs: `qt\build\debug\`.

The release build uses no LTO, 2 codegen units and 2 parallel jobs (`Cargo.toml`, `.cargo/config.toml`): thin LTO of the
static library ran rustc-LLVM out of memory.

## Notes
- Pictures kept on disk (pages and covers) are stored as WebP (`src/webpstore.rs`); upscaled pages too.
- Autocards (learnjapanese.moe/autocards): Ctrl+M in the player sends the picture, sentence audio and context of the
  line on screen to the newest card of the mining deck through AnkiConnect (`src/autocards.rs`).
- App extensions: `<data>/extensions/<id>/manifest.json` + `main.js` (`src/appext.rs`, example in `examples/extensions`).
- Licensed GPL-3.0 (embeds libmpv).

## Added in this version
- **Continue watching** row on the start page (play mark, progress bar, ⋮ menu: Play / Details / Dismiss). Progress is saved in `progress.json`.
- **Calendar** tab: release days of the series in your library (month grid plus a "Coming up" list).
- Episodes: WATCHED badge, mark as watched / not watched button, unwatched ones dimmed and blurred.
- Loading screen shows time waited, peers, speed and progress of the torrent.
- Player: larger icon buttons, pill Skip button, volume 0–150 % with gradient, 3-column subtitle menu (languages, variants, settings), episode sidebar, next-episode card with picture, next episode preloaded in the last 5 minutes, small seek preview, click the end time to show time left, Video scale (Fit / Crop / Stretch).

## MyAnimeList login

MyAnimeList only accepts a login when the address it sends you back to is registered in your own API app. Set it up once:

1. Open <https://myanimelist.net/apiconfig> and choose **Create ID**. App Type: **other**.
2. In **App Redirect URL** write `http://localhost:51789/callback` (the app shows the exact address in Preferences → MyAnimeList, with a copy button). The field accepts several addresses, one per line; keep the ones you already have and add this one on its own line. Save.
3. Copy the **Client ID** (and the **Client Secret**, if the app shows one) into Preferences → MyAnimeList and press Log in.
4. If your browser does not come back to the app, copy the address it ended on and paste it into "Browser did not come back? Paste its address here".

A username and password box from the browser, or an "invalid redirect_uri" message, means the address in step 2 is missing or different from the one in the app. You can also keep another redirect address: type it in the field "Keep another redirect URL" and paste the final address as in step 4.

## Modes and new episodes

- **Mode** (header, next to Categories): Video is the whole app as it was. Images (manga and comics, in the style of Mihon) and Text (light and web novels, in the style of mobile novel readers) have their own page and their own Preferences pages; Preferences only shows the pages of the mode you are in. Everything is saved in the same settings and in the same backup.
- Source repositories: the app ships with none. Add the ones you want in Preferences → Images (extension repositories in the Tachiyomi / Mihon format) and Preferences → Text (a list of JavaScript plugins).
- **Notifications** (bell in the header): the app looks for new episodes of the series in the library and favorites a little after it starts and every six hours (or press Check now). A yellow NEW pill appears on the cover until you open that title.

## Skip openings and endings
When a video starts, Reelhouse looks for skip times in this order and shows them as yellow strokes on the seek bar:
1. Chapters stored inside the video file (Opening, Ending, Recap…). Open Statistics to see how many chapters the file has and where the skip times came from.
2. AniSkip (crowdsourced, anime, by MyAnimeList id and episode length).
3. IntroDB (crowdsourced, by IMDb id).
4. Anime Skip (needs a client ID; ignored when its release length differs from your file).

## Pictures
Posters that fail to load are retried from other sizes, Cinemeta, TMDB and OMDb (add keys in Preferences → Metadata). The cover/background window shows the size and shape of the pictures and fits your own images automatically or with a crop frame.

## Images mode (manga and comics)

Choose **Mode → Images**. The layout has a **Library** (with categories and unread counters),
**Explore** (popular, latest and search) and **History**. Open a title to see its chapters, add it to the library
and read it. Titles come from MangaDex and from the extensions below; when a title has a
MyAnimeList link, finishing a chapter updates your MyAnimeList manga list. Everything is kept in
`manga.json` in the data folder, so it travels inside the backup.

**The reader** has four modes: right to left, left to right, vertical (page by page, scrolling inside a tall page) and
webtoon (one continuous strip that only keeps the pictures near the screen in memory). Zoom with the buttons,
`+` `-` `0`, `Ctrl` + wheel or a pinch; fit the page to the screen, width, height or its original size; click the sides
of the page (or the arrows on the screen) to turn it, the middle to show or hide the bars; a slider jumps to any page;
`F` is full screen, `M` hides the bars, `[` `]` change chapter, `Home` `End` go to the first or last page; drag a zoomed
page to move it. Webtoon can scroll by itself (options button in the header). Background, tap zones and the
on-screen buttons are options too. The chapter language is chosen from a list in Preferences → Images.

### Tachiyomi / Mihon extensions (Suwayomi)

Extension repositories in that format hold Android packages, so the app runs [Suwayomi-Server](https://github.com/Suwayomi/Suwayomi-Server)
as the engine and only draws the result. You do not install or start anything:

1. The first time the Images mode opens, the app downloads Suwayomi (about 330 MB, it carries its own Java) into the
   data folder and starts it hidden on a free local port. It stops with the app, and a few minutes after you leave the
   Images mode, so the other modes use no memory for it.
2. The app comes with no repository: add the ones you want in Preferences → Images → **Repositories**.
3. Preferences → Images lists every extension they offer, with a language filter and a search box; *Install* adds one,
   *Install all* adds everything shown (all of a language, say).
4. In **Explore** pick the source in the drop-down next to the search box.

To use a server you run yourself, turn off *Run it inside the app* and give its address.

## Text mode (light and web novels)

**Mode → Text.** Same layout as Images: **Library**, **Explore** (popular, latest, search) and **History**; open a title for its
chapters and read in the app (text size with `A-`/`A+`, `[` and `]` for the previous and next chapter). Reading position,
read marks and the library are kept in `novels.json` (inside the backup). When MyAnimeList is connected, finishing a chapter updates
the manga/novel list; the MyAnimeList entry is found by title (AniList, then Jikan) when the source does not give it.

Sources are **JavaScript plugins** in the format that manga and light novel readers use. The app comes with none. They run inside the app, in a small
embedded JavaScript engine (QuickJS) with its own cheerio, fetch and dayjs; no Node.js or Android is needed.
Paste the address of a plugin list in Preferences → Text; it then lists the plugins: press *Install* on the ones you want, then choose one in Explore.

**Own books.** *Add from file* (or drop files on the library) imports `.epub`, `.txt`, `.md` and `.html`; *New document* starts an
empty book you write yourself, and the series menu adds chapters. Pictures inside chapters (light/web novels, EPUB) are shown in the
reader. The pencil in the reader opens an editor for the chapter (`# heading`, one line per paragraph, `[[image: address | text]]`);
for a site's chapter the original can be restored. **Aa** sets size, line height, indent, word distance, column width, font,
alignment, background, text colour and a red light over the whole app (indent and word distance are made with spaces).
Source lists in Preferences (Images and Text) can be filtered by language and 18+, and keep their place when you install.

Plugins for sites behind a captcha or a login in a browser (Cloudflare) may fail: the app has no browser to solve it.
The build needs a C compiler (MSYS2's toolchain, which `build-windows.ps1` installs) for the JavaScript engine.

## Karaoke on openings and endings

For anime, Reelhouse looks up the opening and ending songs of the series in the [Karaoke Mugen](https://kara.moe) database (`kara.moe/api/karas/search`, matched by MyAnimeList id; the series is identified through AniList). The player's **Subtitles menu → Karaoke** says what was found for the episode and can search again. While the player is inside an opening or ending (times from AniSkip, IntroDB, Anime Skip or the file's chapters) it shows the song's karaoke lyrics and then brings your subtitle back. It has its own Preferences page (**Karaoke**): on/off, **delay** (default −2 s: the lyrics usually come about 2 s late, so they are shown that much earlier), top/bottom, size and distance from the edge; changes apply at once while the lyrics are on screen. **Statistics → Karaoke** says what was found. The lyrics are ODbL data from Karaoke Mugen.

## Repositories of the Images mode

Preferences → Images → **Repositories** lists the extension repositories given to the built-in Suwayomi. Paste the address of a repository (a GitHub page like `github.com/user/repo`, a `…/tree/repo` page or the link of its `index.min.json`) and press Enter or the tick to add it; the trash button removes one.

## Debug report

Preferences → General → **Diagnostics** → *Debug report*. While it is on, the app writes `debug/reelhouse-debug.log` in its data folder: every 5 seconds the processor use and memory of the app and the worst tick of the window (and which part it was), every part of the window that took more than 12 ms, every network request (keys and tokens hidden; the answers of Karaoke Mugen, Suwayomi and any error are included), and what the player, the skip times, the karaoke and Suwayomi do. Turn it on, repeat the problem for a minute, turn it off and send the file (the previous run is kept as `reelhouse-debug.old.log`).

## Images mode: layout, upscaling and text of the pages

In the Images mode, the **Library** has a list of categories on the side (the bookmark button; right-click a category to remove it) and a search; **Explore** starts with the list of **servers** (pin the ones you use, groups by language) and opens the titles of one; **History** is grouped by day, with a play button that opens the chapter; the page of a title has **Info**, **Chapters** and **Categories**.

**Upscaling.** The button in the reader's bar makes the pages sharper with an AI model. It runs on the graphics card through a Vulkan engine ([Real-ESRGAN ncnn Vulkan](https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan): NVIDIA, AMD and Intel, no Python or CUDA), which the app downloads the first time (about 40 MB). Each page is done once and kept on the disk; the next pages are done while you read. The official MangaJaNai models are PyTorch/ONNX files for CUDA and cannot be run inside the app, but the engine takes any NCNN model: put the `.param` and `.bin` files (a MangaJaNai model converted with [chaiNNer](https://github.com/chaiNNer-org/chaiNNer), for example) in the `models` folder next to the engine and choose it in the reader's options. Preferences > Images has the engine, tile size and cache size.

**Text of the pages.** The other button of the bar reads the page (OCR.space, free, or Google Cloud Vision with your own key, much better for vertical Japanese) and draws a box over each speech bubble. Click a box to select it, right-click to **translate**, **highlight** or **copy**; the translation shows in a sheet at the bottom with the original, editable. The translator and the target language are those of the subtitles (Libretranslate on the web works without setup).

## Credits

Parts of the stream choice and subtitle handling follow [NuvioMobile](https://github.com/NuvioMedia/NuvioMobile) (GPL-3.0, NuvioMedia and contributors): the filter for hearing-impaired notes in `src/subtitles.rs` (`strip_sdh`) is ported from its `SubtitleSdhFilter.kt` and modified (2026-11); the binge-group and rule-based autoplay in `src/streams.rs` and `src/streammemory.rs`, the subtitle sync in `src/subsync.rs` (by speech instead of an embedded track), the brightness / volume slides of the phone player and the P2P notice are written anew from its ideas.
