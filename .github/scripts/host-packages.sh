#!/usr/bin/env bash
# Installs what the Linux and Android builds need from the system, on GitHub's Ubuntu machines (apt) and on the
# owner's own Fedora PC as a self-hosted runner (dnf). Usage: host-packages.sh linux|android
# Also installs Rust with rustup when cargo is missing (GitHub's machines already have it).
set -euo pipefail
what="${1:?linux or android}"

if command -v apt-get >/dev/null; then
  sudo apt-get update -q
  if [ "$what" = android ]; then
    sudo apt-get install -y -q libclang-dev ninja-build
  else
    # the build (OpenGL, xkbcommon headers for Qt's CMake files) and what linuxdeploy copies with Qt's plugins
    # (xcb platform, speech-dispatcher voice, PulseAudio sound, VA-API)
    sudo apt-get install -y -q ninja-build libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev libxkbcommon-x11-0 \
      libfontconfig1-dev libfreetype6-dev libdbus-1-dev libxcb-cursor0 libxcb-icccm4 libxcb-image0 libxcb-keysyms1 \
      libxcb-randr0 libxcb-render-util0 libxcb-shape0 libxcb-xinerama0 libxcb-xkb1 libxcb-sync1 libxcb-xfixes0 \
      libxcb-shm0 libxcb-util1 libspeechd2 libpulse0 libva2 libva-drm2 libva-x11-2 libvdpau1 libgtk-3-0 unzip patchelf
  fi
elif command -v dnf >/dev/null; then
  common=(git curl unzip tar gcc gcc-c++ make cmake ninja-build python3 python3-pip gh file)
  if [ "$what" = android ]; then
    pkgs=("${common[@]}" clang-devel)
  else
    # the same as the apt list above, by Fedora's names; dpkg gives dpkg-deb for the .deb checks
    pkgs=("${common[@]}" mesa-libGL-devel mesa-libEGL-devel libxkbcommon-devel libxkbcommon-x11 fontconfig-devel
      freetype-devel dbus-devel libxcb xcb-util xcb-util-cursor xcb-util-wm xcb-util-image xcb-util-keysyms
      xcb-util-renderutil speech-dispatcher-libs pulseaudio-libs libva libvdpau gtk3 patchelf dpkg)
  fi
  # a name a Fedora release lacks only skips that package (dnf5 and dnf4 spell it differently)
  sudo dnf install -y --skip-unavailable "${pkgs[@]}" 2>/dev/null || sudo dnf install -y --setopt=strict=0 "${pkgs[@]}"
else
  echo "::error::no apt-get or dnf on this runner"; exit 1
fi

if ! command -v cargo >/dev/null; then
  if [ -x "$HOME/.cargo/bin/cargo" ]; then :; else
    curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal
  fi
  echo "$HOME/.cargo/bin" >> "${GITHUB_PATH:-/dev/null}"
  export PATH="$HOME/.cargo/bin:$PATH"
fi
cargo --version
