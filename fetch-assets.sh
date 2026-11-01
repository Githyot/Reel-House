#!/usr/bin/env bash
# Linux and macOS counterpart of fetch-assets.ps1: downloads what build.rs builds into the core, into assets/:
#   assets/rqbit             the torrent engine used when Real-Debrid is off (the latest ikatson/rqbit release)
#   assets/shaders/*.glsl    the Anime4K upscaling shaders (bloc97/Anime4K)
# libmpv is not embedded here: Linux uses the system's (a package dependency), macOS ships it in the app bundle.
# Both parts are optional: a failed download only leaves the feature out. Needs gh (GH_TOKEN) and unzip/tar.
# Usage: bash fetch-assets.sh linux|macos
set -u
os="${1:-linux}"
root="$(cd "$(dirname "$0")" && pwd)"
assets="$root/assets"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$assets/shaders"

echo "rqbit..."
if [ "$os" = macos ]; then
  want='(osx|macos|darwin|apple)'
  avoid='desktop|\.dmg$|\.pkg$|sha|sig|\.txt$|x86_64|amd64|intel'
else
  want='linux.*(x86_64|amd64|x64)|(x86_64|amd64).*linux'
  avoid='desktop|\.deb$|\.rpm$|appimage|sha|sig|\.txt$|arm|aarch64|musl.*static.*gnu'
fi
names="$(gh release view -R ikatson/rqbit --json assets -q '.assets[].name' 2>/dev/null | grep -Ei "$want" | grep -viE "$avoid")"
echo "rqbit candidates: $(echo $names)"
pick="$(echo "$names" | head -n 1)"
if [ -n "$pick" ] && gh release download -R ikatson/rqbit -p "$pick" -D "$tmp" --clobber; then
  f="$tmp/$pick"
  case "$pick" in
    *.zip) mkdir -p "$tmp/rq" && unzip -oq "$f" -d "$tmp/rq" && f="$(find "$tmp/rq" -type f -name 'rqbit*' | head -n 1)" ;;
    *.tar.gz|*.tgz) mkdir -p "$tmp/rq" && tar -xzf "$f" -C "$tmp/rq" && f="$(find "$tmp/rq" -type f -name 'rqbit*' | head -n 1)" ;;
  esac
  if [ -n "$f" ] && [ -f "$f" ]; then
    cp -f "$f" "$assets/rqbit" && chmod +x "$assets/rqbit" && echo "rqbit ok ($pick)"
  else
    echo "WARNING: no rqbit program inside $pick"
  fi
else
  echo "WARNING: rqbit not downloaded: the program will use Real-Debrid only"
fi

echo "Anime4K shaders..."
if gh release download -R bloc97/Anime4K -p "*.zip" -D "$tmp/a4k" --clobber; then
  for z in "$tmp"/a4k/*.zip; do unzip -oq "$z" -d "$tmp/a4kx" || true; done
  find "$tmp/a4kx" -type f -name 'Anime4K_*.glsl' -exec cp -f {} "$assets/shaders/" \;
else
  echo "WARNING: Anime4K shaders not downloaded: Settings can download them later"
fi
echo "Shaders: $(find "$assets/shaders" -name '*.glsl' | wc -l)"
exit 0
