#!/usr/bin/env bash
# Builds the Android app (arm64-v8a): the Rust core with cargo-ndk, then the Qt app and the APK with Qt for Android.
# Runs on Linux or macOS. Output: qt/build-android/android-build/build/outputs/apk/debug/android-build-debug.apk
# (debug-signed: installs on a phone with "unknown sources" allowed; a store or F-Droid build needs a release key).
#
# Needs (see docs/mobile-build.md):
#   ANDROID_SDK_ROOT   the Android SDK (platforms;android-34, build-tools;34.0.0, platform-tools)
#   ANDROID_NDK_ROOT   the NDK (r26 or r27); or ANDROID_SDK_ROOT/ndk/<version> is used
#   QT_ANDROID         Qt for Android, e.g. ~/Qt/6.8.3/android_arm64_v8a
#   QT_HOST            the desktop Qt of the same version, e.g. ~/Qt/6.8.3/gcc_64 (macOS: macos)
#   JAVA_HOME          a JDK 17
#   rustup, cargo, cmake 3.21+, ninja, libclang (bindgen for QuickJS: apt install libclang-dev)
# Options: REELHOUSE_ABI=arm64-v8a (default) | x86_64 (emulator); BUILD_TYPE=Release (default) | Debug;
#          SKIP_RUST=1 reuses the core built before.
set -euo pipefail
cd "$(dirname "$0")"
ROOT="$(pwd)"
LOG="build-android.log"
: > "$LOG"
exec > >(tee -a "$LOG") 2>&1
echo "== Reelhouse Android build, $(date)"

ABI="${REELHOUSE_ABI:-arm64-v8a}"
BUILD_TYPE="${BUILD_TYPE:-Release}"
MIN_SDK=28
case "$ABI" in
  arm64-v8a) TRIPLE=aarch64-linux-android ;;
  x86_64) TRIPLE=x86_64-linux-android ;;
  armeabi-v7a) TRIPLE=armv7-linux-androideabi ;;
  x86) TRIPLE=i686-linux-android ;;
  *) echo "unknown ABI $ABI"; exit 1 ;;
esac

: "${ANDROID_SDK_ROOT:?set ANDROID_SDK_ROOT to the Android SDK}"
if [ -z "${ANDROID_NDK_ROOT:-}" ]; then
  ANDROID_NDK_ROOT="$(ls -d "$ANDROID_SDK_ROOT"/ndk/* 2>/dev/null | sort -V | tail -1 || true)"
fi
: "${ANDROID_NDK_ROOT:?set ANDROID_NDK_ROOT to the NDK (or install one under ANDROID_SDK_ROOT/ndk)}"
: "${QT_ANDROID:?set QT_ANDROID to the Qt for Android kit, e.g. ~/Qt/6.8.3/android_arm64_v8a}"
: "${QT_HOST:?set QT_HOST to the desktop Qt of the same version, e.g. ~/Qt/6.8.3/gcc_64}"
export ANDROID_NDK_HOME="$ANDROID_NDK_ROOT" ANDROID_HOME="$ANDROID_SDK_ROOT"
echo "SDK $ANDROID_SDK_ROOT"; echo "NDK $ANDROID_NDK_ROOT"; echo "Qt  $QT_ANDROID (host $QT_HOST)"; echo "ABI $ABI, $BUILD_TYPE"

# ---- the Rust core (qtbridge/) for Android: cargo-ndk picks the NDK's clang and sysroot ----
RUST_LIB="$ROOT/target-android/$TRIPLE/release/libreelhouse_qtbridge.a"
if [ "${SKIP_RUST:-0}" != 1 ]; then
  command -v cargo-ndk >/dev/null 2>&1 || cargo install cargo-ndk --locked
  rustup target add "$TRIPLE"
  # `mobile`: no helper programs (Suwayomi, upscaler, yt-dlp, Ollama); `js-bindgen`: QuickJS bindings made here;
  # `torrent-embed`: the torrent engine (librqbit) runs inside the app
  cargo ndk -t "$ABI" --platform "$MIN_SDK" build --release \
    --manifest-path qtbridge/Cargo.toml --target-dir "$ROOT/target-android" \
    --no-default-features --features "mobile,js-bindgen,torrent-embed"
fi
[ -f "$RUST_LIB" ] || { echo "== the Rust core is missing: $RUST_LIB"; exit 1; }
echo "== Rust core: $(du -h "$RUST_LIB" | cut -f1)"

# ---- libmpv for Android (prebuilt, see docs/mobile-build.md): qt/third_party/mpv-android/<abi>/*.so ----
# When none is there, take libmpv and the ffmpeg libraries from the latest mpv-android release (the official mpv app,
# whose universal APK carries them for every ABI). Left out: libplayer.so (its Java glue). Its libc++_shared.so is
# kept apart (CXX_DIR): mpv-android builds with a newer NDK than Qt, and libmpv needs that newer C++ library (an older
# one makes the app close at start: "cannot locate symbol ...__from_chars_floating_point..."). It replaces Qt's in the
# APK below; the newer libc++ also serves code built with the older NDK.
MPV_DIR="$ROOT/qt/third_party/mpv-android/$ABI"
CXX_DIR="$ROOT/qt/third_party/mpv-android/cxx-$ABI"
if ! ls "$MPV_DIR/"*.so >/dev/null 2>&1 || [ ! -f "$CXX_DIR/libc++_shared.so" ]; then
  MPV_TMP="$(mktemp -d)"
  # with the workflow's token when there is one (the anonymous GitHub API is rate limited on shared runners), tried
  # three times
  GH_AUTH=(); [ -n "${GITHUB_TOKEN:-}" ] && GH_AUTH=(-H "Authorization: Bearer $GITHUB_TOKEN")
  MPV_URL="$(curl -fsSL --retry 3 --retry-delay 5 --retry-all-errors "${GH_AUTH[@]}" https://api.github.com/repos/mpv-android/mpv-android/releases/latest \
    | python3 -c 'import json,sys; a=[x["browser_download_url"] for x in json.load(sys.stdin)["assets"] if x["name"].endswith(".apk")]; u=[x for x in a if "universal" in x and "api" not in x.rsplit("/",1)[1]] or [x for x in a if "universal" in x] or a; print(u[0] if u else "")' || true)"
  if [ -n "$MPV_URL" ] && curl -fsSL --retry 3 --retry-delay 5 --retry-all-errors -o "$MPV_TMP/mpv.apk" "$MPV_URL" \
     && unzip -q -o "$MPV_TMP/mpv.apk" "lib/$ABI/*.so" -d "$MPV_TMP"; then
    mkdir -p "$MPV_DIR"
    find "$MPV_TMP/lib/$ABI" -name '*.so' ! -name libplayer.so ! -name 'libc++_shared.so' -exec cp -f {} "$MPV_DIR/" \;
    mkdir -p "$CXX_DIR"
    cp -f "$MPV_TMP/lib/$ABI/libc++_shared.so" "$CXX_DIR/" 2>/dev/null || true
    echo "== libmpv from $MPV_URL: $(ls "$MPV_DIR" | tr '\n' ' ')"
  else
    echo "== WARNING: could not fetch libmpv from mpv-android"
  fi
  rm -rf "$MPV_TMP"
fi
if ! ls "$MPV_DIR/"*.so >/dev/null 2>&1; then
  echo "== WARNING: no libmpv in qt/third_party/mpv-android/$ABI/: the APK will play no video"
  # a release must never ship an APK that plays nothing (v0.8.3's first arm64 APK did)
  if [ -n "${REELHOUSE_RELEASE:-}" ]; then echo "== stopping: a release build needs libmpv"; exit 1; fi
fi

# ---- OpenSSL for Qt's https (posters and covers): KDAB's prebuilt libraries, fetched once ----
if [ ! -f "$ROOT/qt/third_party/android_openssl/android_openssl.cmake" ]; then
  git clone --depth 1 https://github.com/KDAB/android_openssl.git "$ROOT/qt/third_party/android_openssl" \
    || echo "== WARNING: could not fetch android_openssl: pictures over https will not load in the APK"
fi

# ---- the Qt app and the APK ----
BUILD="$ROOT/qt/build-android"
"$QT_ANDROID/bin/qt-cmake" -S qt -B "$BUILD" -G Ninja \
  -DCMAKE_BUILD_TYPE="$BUILD_TYPE" \
  -DQT_HOST_PATH="$QT_HOST" \
  -DANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" -DANDROID_NDK_ROOT="$ANDROID_NDK_ROOT" \
  -DANDROID_ABI="$ABI" -DANDROID_PLATFORM="android-$MIN_SDK" \
  -DREELHOUSE_BUILD_RUST=OFF -DREELHOUSE_RUST_LIB="$RUST_LIB"
cmake --build "$BUILD"
# androiddeployqt: packs the Qt libraries, the QML modules, the extra libs and the manifest (signed below)
cmake --build "$BUILD" --target apk
APK="$(find "$BUILD/android-build" -name '*.apk' | head -1 || true)"
if [ -z "$APK" ]; then echo "== APK not found under $BUILD/android-build"; exit 1; fi
mkdir -p dist
# a Release build comes out unsigned, and Android refuses an unsigned APK ("package appears to be invalid"): align
# it and sign it (v1+v2) with the debug key, made once in ~/.android and kept, so later APKs install as updates
BT="$(ls -d "$ANDROID_SDK_ROOT"/build-tools/*/ 2>/dev/null | sort -V | tail -1)"
if [ -z "$BT" ] || [ ! -x "$BT/apksigner" ]; then echo "== apksigner not found under $ANDROID_SDK_ROOT/build-tools"; exit 1; fi
KS="$HOME/.android/debug.keystore"
# The same key on every machine: the workflow passes the project's own keystore (secret ANDROID_KEYSTORE_B64, alias
# androiddebugkey, password android). Without it a shared runner makes a NEW key each run, every release gets another
# signature, and Android refuses to install it over the previous one ("App not installed"): the phone keeps the old app.
if [ -n "${ANDROID_KEYSTORE_B64:-}" ]; then
  mkdir -p "$HOME/.android"
  printf '%s' "$ANDROID_KEYSTORE_B64" | base64 -d > "$KS"
  echo "== signing with the project's keystore"
elif [ -n "${GITHUB_ACTIONS:-}" ]; then
  echo "::warning::ANDROID_KEYSTORE_B64 is not set: this APK gets a one-off key and will NOT install over an older ReelHouse"
fi
if [ ! -f "$KS" ]; then
  mkdir -p "$HOME/.android"
  keytool -genkeypair -v -keystore "$KS" -storepass android -alias androiddebugkey -keypass android \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
fi
# libmpv's newer libc++_shared.so in place of the one androiddeployqt packed (see CXX_DIR above)
if [ -f "$CXX_DIR/libc++_shared.so" ] && ls "$MPV_DIR/"*.so >/dev/null 2>&1; then
  python3 - "$APK" "lib/$ABI/libc++_shared.so" "$CXX_DIR/libc++_shared.so" <<'PY'
import os, shutil, sys, zipfile
apk, entry, new = sys.argv[1:4]
tmp = apk + ".tmp"
with zipfile.ZipFile(apk) as zin, zipfile.ZipFile(tmp, "w") as zout:
    for i in zin.infolist():
        if i.filename.startswith("META-INF/") and i.filename.upper().endswith((".SF", ".RSA", ".EC", ".DSA", "MANIFEST.MF")):
            continue   # the old signature: signed again below
        data = open(new, "rb").read() if i.filename == entry else zin.read(i.filename)
        zout.writestr(i, data, compress_type=i.compress_type)
shutil.move(tmp, apk)
print("== libc++_shared.so from mpv-android put in the APK")
PY
fi
"$BT/zipalign" -p -f 4 "$APK" "dist/ReelHouse-android-$ABI.apk"
"$BT/apksigner" sign --ks "$KS" --ks-pass pass:android --ks-key-alias androiddebugkey --key-pass pass:android \
  "dist/ReelHouse-android-$ABI.apk"
"$BT/apksigner" verify --verbose "dist/ReelHouse-android-$ABI.apk" | head -5
echo "== done: dist/ReelHouse-android-$ABI.apk ($(du -h "dist/ReelHouse-android-$ABI.apk" | cut -f1)), $(date)"
