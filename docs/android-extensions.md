# Extensions on Android (no Suwayomi)

On Windows and Linux the Images mode runs extensions through Suwayomi-Server. On Android the app loads the
extension APKs itself, the way Mihon does, with a small host written in Kotlin (`qt/android/src/org/reelhouse/ext/`)
and a port of Mihon's source API (`qt/android/src/eu/kanade/tachiyomi/`, Apache-2.0, see `NOTICE-mihon.md` there).

The core (Rust) talks to that host over HTTP on 127.0.0.1, so the code that already speaks to Suwayomi keeps its
shape (`src/exthost.rs` mirrors `src/suwayomi.rs`).

## Starting

C++ (`main.cpp`, Android only) calls once, after the window exists:

    int ExtensionHost.start(Context context, String dir)

`dir` is a folder in the app's private data (`<appData>/extensions`). It returns the port of the local server (0 on
failure). C++ hands it to the core with the bridge command `{"cmd":"ext_host","port":N,"dir":dir}`.

## POST /api

Body and answer are JSON objects. Every answer is either the result or `{"error": "text"}` (HTTP 200 both ways).

| op | request fields | answer |
|---|---|---|
| `installed` | | `{"extensions": [Ext]}` |
| `install` | `apk` (absolute path of a downloaded APK) | `Ext` |
| `uninstall` | `pkg` | `{}` |
| `sources` | | `{"sources": [Src]}` |
| `popular`, `latest` | `source`, `page` (1-based) | `{"mangas": [Manga], "hasNext": bool}` |
| `search` | `source`, `page`, `query` | same |
| `details` | `source`, `url` | `Manga` |
| `chapters` | `source`, `url` | `{"chapters": [Chapter]}` |
| `pages` | `source`, `url` (chapter url) | `{"pages": [string]}`: addresses of `/image` on this server |

- `Ext`: `{"pkg", "name", "version", "code", "lang", "nsfw": bool, "sources": [Src]}`
- `Src`: `{"id": string (the 64-bit id as text), "name", "lang", "supportsLatest": bool, "pkg"}`
- `Manga`: `{"url", "title", "thumbnail", "author", "artist", "description", "genre", "status": int, "initialized": bool}`;
  `thumbnail` is already rewritten to `/image?source=..&url=..` on this server (covers often need the source's headers).
- `Chapter`: `{"url", "name", "date": ms since 1970 (0 if unknown), "number": float (-1 if unknown), "scanlator"}`

## GET /image?source=ID&url=URL[&page=1]

Fetches a picture through the source's own OkHttp client and headers (Referer, cookies, interceptors) and answers
with its bytes and content type. With `page=1`, `url` is a page url whose picture address still has to be asked of
the source (`getImageUrl`).

## Repositories

The core reads the repositories the user added (`index.min.json`), downloads an extension's APK to
`<dir>/downloads/<apk>`, and asks `install` with that path. The host copies it to `<dir>/installed/<pkg>.apk`,
reads its manifest meta-data (`tachiyomi.extension.class`, `tachiyomi.extension.nsfw`), and loads it with a
`DexClassLoader` whose parent is the app's class loader (which carries OkHttp, Jsoup, RxJava, Injekt, kotlinx).
Installed extensions are loaded again at `start`.

## Notes from the host implementation

- Text fields the source leaves empty (`thumbnail`, `author`, `artist`, `description`, `genre`, `scanlator`) are
  empty strings, never `null`. Mangas a source returns without a url are left out of `mangas`.
- The `/image` addresses the host writes may carry two more parameters, to be passed through untouched: `index`
  (the page's position; it marks a reader page, fetched through the source's `imageRequest`, while an address without
  it is a cover, fetched with the source's client and headers like Mihon's cover loader) and `ref` (the page url,
  which sources often send as the Referer of the picture).
- A failed `/image` answers a non-200 status (502 when the source or website failed) with the reason as plain text.
- `install` only accepts an APK inside `dir` (anything on the device can reach the local port, so the host does not
  load code from elsewhere). Extension library versions 1.4 to 1.6 are accepted, as in Mihon; signatures are not
  checked against the repository yet.
- Requests answer after the extensions installed earlier have finished loading (at most two minutes after `start`).
- A source call that takes longer than 90 s (120 s for a picture) answers `{"error": ...}` / 502.
