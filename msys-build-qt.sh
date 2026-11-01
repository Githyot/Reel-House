#!/usr/bin/env bash
# Runs inside MSYS2 MINGW64 (started by build-qt.ps1) from the project folder. Output also goes to build-qt.log.
set -e
LOG="build-qt.log"
: > "$LOG"
exec > >(tee -a "$LOG") 2>&1
# The build log is copied to qt/build/debug/ (the folder of every log) when the script ends, with or without errors.
ROOT="$(pwd)"
trap 'sleep 1; mkdir -p "$ROOT/qt/build/debug" 2>/dev/null; cp -f "$ROOT/$LOG" "$ROOT/qt/build/debug/build-qt.log" 2>/dev/null || true' EXIT
echo "== Reelhouse Qt build, $(date)"
pacman -S --needed --noconfirm mingw-w64-x86_64-qt6-base mingw-w64-x86_64-qt6-declarative mingw-w64-x86_64-qt6-tools \
  mingw-w64-x86_64-qt6-imageformats mingw-w64-x86_64-qt6-svg mingw-w64-x86_64-qt6-multimedia mingw-w64-x86_64-qt6-speech mingw-w64-x86_64-rust mingw-w64-x86_64-cmake \
  mingw-w64-x86_64-ninja mingw-w64-x86_64-toolchain mingw-w64-x86_64-pkgconf \
  mingw-w64-x86_64-libjpeg-turbo mingw-w64-x86_64-libwebp mingw-w64-x86_64-libpng mingw-w64-x86_64-libtiff mingw-w64-x86_64-giflib
rustc --version; cargo --version; pacman -Q mingw-w64-x86_64-qt6-base
cd qt
# Research mode draws web pages with Chromium: CEF (the Chromium Embedded Framework, BSD licence), its "minimal"
# Windows build from the official CEF build server, downloaded once into qt/third_party/cef/ (include/ for the
# build, Release/ and Resources/ to ship). The version is fixed (and its SHA-1 checked) so a new release cannot
# break the build; the C code is written for this version's API (CEF_API_VERSION 15400 in qt/CMakeLists.txt).
# Without it the app still builds and Research mode opens pages in the system browser.
# Another copy of the same archive can be given with CEF_ARCHIVE=/path/to/cef_binary_..._windows64_minimal.tar.bz2.
CEF_VERSION="154.0.34+g14c5a08+chromium-154.0.8037.98"
CEF_SHA1="be54bbaf3222191dec59ffd9c9ec7e8a7f44a26d"
CEF_NAME="cef_binary_${CEF_VERSION}_windows64_minimal"
CEF="$ROOT/qt/third_party/cef"
if [ ! -f "$CEF/include/capi/cef_app_capi.h" ] || [ ! -f "$CEF/Release/libcef.dll" ] || [ "$(cat "$CEF/VERSION.reelhouse" 2>/dev/null)" != "$CEF_VERSION" ]; then
  echo "== getting Chromium (CEF $CEF_VERSION, about 165 MB)"
  pacman -S --needed --noconfirm tar bzip2 >/dev/null 2>&1 || true
  mkdir -p "$ROOT/qt/third_party"
  ARCHIVE="$ROOT/qt/third_party/$CEF_NAME.tar.bz2"
  if [ -n "$CEF_ARCHIVE" ] && [ -f "$CEF_ARCHIVE" ]; then cp -f "$CEF_ARCHIVE" "$ARCHIVE"; fi
  if [ ! -f "$ARCHIVE" ] || [ "$(sha1sum "$ARCHIVE" | cut -d' ' -f1)" != "$CEF_SHA1" ]; then
    rm -f "$ARCHIVE"
    # the "+" of the version is written %2B in the address
    URL="https://cef-builds.spotifycdn.com/$(printf '%s' "$CEF_NAME" | sed 's/+/%2B/g').tar.bz2"
    curl -fL --retry 3 --connect-timeout 30 -o "$ARCHIVE" "$URL" || echo "== WARNING: $URL could not be downloaded"
  fi
  if [ -f "$ARCHIVE" ] && [ "$(sha1sum "$ARCHIVE" | cut -d' ' -f1)" = "$CEF_SHA1" ]; then
    rm -rf "$CEF" "$ROOT/qt/third_party/$CEF_NAME"
    if tar -xjf "$ARCHIVE" -C "$ROOT/qt/third_party"; then
      mv "$ROOT/qt/third_party/$CEF_NAME" "$CEF" && echo "$CEF_VERSION" > "$CEF/VERSION.reelhouse"
      # only what the build and the program need (the samples' sources and CMake files stay out)
      rm -rf "$CEF/libcef_dll" "$CEF/cmake" "$CEF/bazel" "$CEF/Release/bootstrap.exe" "$CEF/Release/bootstrapc.exe"
      rm -f "$ARCHIVE"
      echo "== Chromium ready: $CEF"
    else
      echo "== WARNING: the CEF archive could not be unpacked: Research mode will open pages in the system browser"
    fi
  else
    echo "== WARNING: Chromium (CEF) could not be downloaded (or its checksum is wrong): Research mode will open pages in the system browser"
    rm -f "$ARCHIVE"
  fi
fi
# The old Edge WebView2 SDK of earlier builds is not used any more.
rm -rf "$ROOT/qt/third_party/webview2"
# build-qt.ps1 may build through a drive letter (a folder name with accents, like "Área de Trabalho", breaks
# qmlimportscanner): a build folder configured from the other path is reset so CMake does not refuse it.
if [ -f build/CMakeCache.txt ]; then
  HERE="$(cygpath -m "$(pwd)" 2>/dev/null || pwd)"
  WAS="$(sed -n 's/^CMAKE_HOME_DIRECTORY:INTERNAL=//p' build/CMakeCache.txt)"
  if [ -n "$WAS" ] && [ "${WAS,,}" != "${HERE,,}" ]; then
    echo "== the source moved ($WAS -> $HERE): resetting the CMake cache"
    rm -rf build/CMakeCache.txt build/CMakeFiles
  fi
fi
if ! cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release; then echo "== CMAKE FAILED"; exit 1; fi
if ! cmake --build build; then
  echo "== BUILD FAILED. The errors, in short:"
  grep -n -E "^(error|.*error:)" -A6 "../$LOG" | grep -v "^--$" | head -80 || true
  exit 1
fi
echo "== bundling DLLs (windeployqt6)"
cd build
windeployqt6 --qmldir ../qml --no-translations reelhouse-qt.exe || true
# The system voices (Qt TextToSpeech: LocalTts.qml): the QML module and the SAPI engine, copied by hand when
# windeployqt6 left them out (the DLL itself comes with the objdump pass below).
QT6SHARE=/mingw64/share/qt6
if [ -d "$QT6SHARE/plugins/texttospeech" ]; then
  mkdir -p texttospeech && cp -n "$QT6SHARE"/plugins/texttospeech/qtexttospeech_sapi*.dll texttospeech/ 2>/dev/null || true
  [ -d qml/QtTextToSpeech ] || { mkdir -p qml && cp -r "$QT6SHARE/qml/QtTextToSpeech" qml/; }
  [ -f Qt6TextToSpeech.dll ] || cp -n /mingw64/bin/Qt6TextToSpeech.dll . 2>/dev/null || true
fi
for need in texttospeech/qtexttospeech_sapi.dll qml/QtTextToSpeech/qmldir Qt6TextToSpeech.dll; do
  [ -f "$need" ] || echo "== WARNING: $need is missing: no system voices (install mingw-w64-x86_64-qt6-speech)"
done
for d in libgcc_s_seh-1.dll libstdc++-6.dll libwinpthread-1.dll; do cp -n "$(dirname "$(which gcc)")/$d" . 2>/dev/null || true; done
# Every DLL that the program and the Qt plugins (pictures, platform, TLS…) need from MSYS2, found with objdump (it is
# more reliable than ldd for plugins) and copied next to the program, again and again until nothing new shows up.
export PATH="/mingw64/bin:$PATH"
for round in 1 2 3 4 5 6; do
  added=0
  for f in *.dll */*.dll */*/*.dll; do
    [ -f "$f" ] || continue
    for dep in $(objdump -p "$f" 2>/dev/null | awk '/DLL Name:/ {print $3}'); do
      if [ ! -f "$dep" ] && [ -f "/mingw64/bin/$dep" ]; then cp -n "/mingw64/bin/$dep" . && added=1 && echo "  + $dep (needed by $f)"; fi
    done
  done
  [ "$added" = 0 ] && break
done
# Research mode: Chromium's files go next to the program (libcef.dll and the DLLs it loads, its V8 snapshot, the
# .pak resources, icudtl.dat, and the locales of the app's languages); reelhouse-cef-helper.exe is built here by CMake.
rm -f WebView2Loader.dll
if [ -f "$CEF/Release/libcef.dll" ] && [ -f reelhouse-cef-helper.exe ]; then
  for f in "$CEF"/Release/*.dll "$CEF"/Release/*.bin "$CEF"/Release/*.json; do [ -f "$f" ] && cp -f "$f" .; done
  cp -f "$CEF"/Resources/*.pak "$CEF"/Resources/icudtl.dat .
  rm -rf locales && mkdir -p locales
  for l in en-US pt-BR pt-PT es es-419 ja; do cp -f "$CEF"/Resources/locales/"$l"*.pak locales/ 2>/dev/null || true; done
  echo "== Chromium (CEF) copied: $(du -sh libcef.dll | cut -f1) libcef.dll, $(ls locales | wc -l) locale files"
else
  echo "== WARNING: Chromium (CEF) or reelhouse-cef-helper.exe missing: Research mode will open pages in the system browser"
fi
for need in imageformats/qjpeg.dll imageformats/qwebp.dll imageformats/qgif.dll; do
  [ -f "$need" ] || echo "== WARNING: $need is missing: pictures of that type will not show"
done
# The Learn course packs (assets/learn/<lang>.json) go next to the program, the first place the core looks.
if ls "$ROOT/assets/learn/"*.json >/dev/null 2>&1; then
  mkdir -p learn && cp -f "$ROOT/assets/learn/"*.json learn/ && echo "== Learn courses: $(ls learn | tr '\n' ' ')"
else
  echo "== WARNING: assets/learn has no course packs: the Learn area will ask for them"
fi
# The kana sounds of the Characters view (assets/kana/<romaji>.ogg, credits in CREDITS.md) go to kana/.
if ls "$ROOT/assets/kana/"*.ogg >/dev/null 2>&1; then
  mkdir -p kana && cp -f "$ROOT/assets/kana/"*.ogg "$ROOT/assets/kana/CREDITS.md" kana/ 2>/dev/null; echo "== Kana sounds: $(ls kana/*.ogg | wc -l)"
else
  echo "== NOTE: assets/kana has no sounds: the Characters view of Learn stays silent (assets/kana/fetch_kana.py gets them)"
fi
mkdir -p debug
echo "== done: qt/build/reelhouse-qt.exe, $(date)"
