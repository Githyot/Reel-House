# Mihon source API (adapted)

The Kotlin files under `eu/kanade/tachiyomi/`, `tachiyomi/core/common/util/lang/` and `mihon/core/common/extensions/`
in this folder come from **Mihon** (<https://github.com/mihonapp/mihon>, commit `e588e043`), licensed under the
Apache License, Version 2.0 (<https://www.apache.org/licenses/LICENSE-2.0>). Mihon itself continues Tachiyomi.

They keep their original package names and public API, because Mihon/Tachiyomi extension APKs are compiled against
them and look them up by name at run time.

Changes made for ReelHouse:

- `source/model/FilterList.kt`: the Compose `@Stable` annotation is removed (no Compose in this app).
- `network/NetworkHelper.kt`: built without Mihon's dependency injection (Metro) and preferences; no DNS over HTTPS,
  no HTTP logging, a fixed default user agent, and no Cloudflare/WebView interceptor.
- `network/AndroidCookieJar.kt`: falls back to an in-memory cookie store on devices without a WebView.
- `network/JavaScriptEngine.kt`: dependency-injection annotations removed.
- `AppInfo.kt`: version values set by the host at start instead of a BuildConfig; the image MIME list is inlined.
- Every file carries a short header naming its origin.

The other files are copied unchanged. ReelHouse's own extension host is in `org/reelhouse/ext/`.
