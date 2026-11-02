#!/usr/bin/env bash
# Runs inside the emulator job of android-ext-test.yml. Prints counts and status codes only (no names or addresses),
# since the log of this public repository is public.
set -u
PKG_APP=org.reelhouse.app
redact() { sed -E 's#https?://[^ "]+#<url>#g' | cut -c1-220; }
fail() { echo "::error::$1"; adb logcat -d -s ReelHouseExt:V AndroidRuntime:E | redact | tail -60; exit 1; }

adb install -r -g ReelHouse-android-x86_64.apk >/dev/null || fail "app install failed"
adb logcat -c
adb shell monkey -p $PKG_APP -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
port=""
for i in $(seq 1 90); do
  port=$(adb logcat -d -s ReelHouseExt:I | grep -oE 'Extension host on 127\.0\.0\.1:[0-9]+' | tail -1 | grep -oE '[0-9]+$')
  [ -n "$port" ] && break
  sleep 2
done
[ -n "$port" ] || fail "the extension host did not start"
echo "host started on port $port after ~$((i*2)) s"
adb forward tcp:18080 tcp:$port >/dev/null
H=http://127.0.0.1:18080

# the APK is not debuggable, so copy the extension in as root (google_apis images allow adb root)
adb root >/dev/null; adb wait-for-device
home=/data/user/0/$PKG_APP
uid=$(adb shell stat -c %u $home | tr -d '\r')
[ -n "$uid" ] || fail "app folder not found"
dir="$home/files/extensions"
adb push ext.apk /data/local/tmp/ext.apk >/dev/null
adb shell "mkdir -p $dir/downloads && cp /data/local/tmp/ext.apk $dir/downloads/ext.apk && chown -R $uid:$uid $home/files && restorecon -R $home/files" || fail "copy failed"
adb forward tcp:18080 tcp:$port >/dev/null

api() { curl -sS -m 150 -X POST -H 'Content-Type: application/json' -d "$1" $H/api; }
check() { local r="$1" what="$2"; if echo "$r" | jq -e 'has("error")' >/dev/null 2>&1; then echo "$what: error: $(echo "$r" | jq -r .error | redact)"; return 1; fi; echo "$r" | jq -e . >/dev/null 2>&1 || { echo "$what: not JSON: $(echo "$r" | head -c 200 | redact)"; return 1; }; }

r=$(api "{\"op\":\"install\",\"apk\":\"$dir/downloads/ext.apk\"}"); check "$r" install || fail install
echo "install: ok, $(echo "$r" | jq '.sources | length') source(s), lib version $(echo "$r" | jq -r .version | cut -d. -f1-2)"
r=$(api '{"op":"installed"}'); check "$r" installed || fail installed
echo "installed: $(echo "$r" | jq '.extensions | length') extension(s)"
r=$(api '{"op":"sources"}'); check "$r" sources || fail sources
src=$(echo "$r" | jq -r '.sources[0].id'); echo "sources: $(echo "$r" | jq '.sources | length')"

ok=1
r=$(api "{\"op\":\"popular\",\"source\":\"$src\",\"page\":1}")
if check "$r" popular; then
  echo "popular: $(echo "$r" | jq '.mangas | length') series, hasNext=$(echo "$r" | jq .hasNext)"
  murl=$(echo "$r" | jq -r '.mangas[0].url'); thumb=$(echo "$r" | jq -r '.mangas[0].thumbnail')
else ok=0; fi
r=$(api "{\"op\":\"search\",\"source\":\"$src\",\"page\":1,\"query\":\"a\"}")
check "$r" search && echo "search: $(echo "$r" | jq '.mangas | length') series" || ok=0
if [ -n "${murl:-}" ] && [ "$murl" != null ]; then
  q=$(jq -cn --arg s "$src" --arg u "$murl" '{op:"details",source:$s,url:$u}')
  r=$(api "$q"); check "$r" details && echo "details: ok, status $(echo "$r" | jq .status), description $(echo "$r" | jq -r .description | wc -c) chars" || ok=0
  r=$(api "$(echo "$q" | jq -c '.op="chapters"')")
  if check "$r" chapters; then
    echo "chapters: $(echo "$r" | jq '.chapters | length')"
    curl_ch=$(echo "$r" | jq -r '.chapters[0].url')
    r=$(api "$(jq -cn --arg s "$src" --arg u "$curl_ch" '{op:"pages",source:$s,url:$u}')")
    if check "$r" pages; then
      echo "pages: $(echo "$r" | jq '.pages | length')"
      img=$(echo "$r" | jq -r '.pages[0]')
      img=${img/127.0.0.1:$port/127.0.0.1:18080}
      echo "page picture: $(curl -sS -m 150 -o page.bin -w '%{http_code} %{content_type}' "$img") $(stat -c %s page.bin 2>/dev/null) bytes"
    else ok=0; fi
  else ok=0; fi
fi
if [ -n "${thumb:-}" ] && [ "$thumb" != null ] && [ "$thumb" != "" ]; then
  thumb=${thumb/127.0.0.1:$port/127.0.0.1:18080}
  echo "cover: $(curl -sS -m 150 -o cover.bin -w '%{http_code} %{content_type}' "$thumb") $(stat -c %s cover.bin 2>/dev/null) bytes"
fi
echo "--- host log (addresses removed) ---"
adb logcat -d -s ReelHouseExt:V AndroidRuntime:E | redact | tail -40
[ $ok = 1 ] || exit 1
