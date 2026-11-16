#!/usr/bin/env bash
# Build Retypr for Linux (X11; the Linux port is still experimental).
# Usage: ./build.sh [debug] [--install-deps]
set -euo pipefail
cd "$(dirname "$0")"

PROFILE=release; FLAGS="--release"; INSTALL_DEPS=0
for a in "$@"; do
  case "$a" in
    debug) PROFILE=debug; FLAGS="" ;;
    --install-deps) INSTALL_DEPS=1 ;;
  esac
done

install_deps() {
  echo "[INFO] Installing system dependencies..."
  if command -v apt-get >/dev/null; then
    sudo apt-get update
    sudo apt-get install -y build-essential pkg-config libx11-dev libxtst-dev libxi-dev \
      libxkbcommon-dev libxdo-dev libevdev-dev libasound2-dev xkb-switch
  elif command -v dnf >/dev/null; then
    sudo dnf install -y gcc pkgconf-pkg-config libX11-devel libXtst-devel libXi-devel \
      libxkbcommon-devel libxdo-devel libevdev-devel alsa-lib-devel
  elif command -v pacman >/dev/null; then
    sudo pacman -S --needed --noconfirm base-devel pkgconf libx11 libxtst libxi \
      libxkbcommon xdotool libevdev alsa-lib
  else
    echo "[WARN] Unknown package manager: install the X11, xdo, evdev and ALSA development packages manually."
  fi
}

[ "$INSTALL_DEPS" = 1 ] && install_deps

command -v cargo >/dev/null || { echo "[ERROR] cargo not found: https://rustup.rs"; exit 1; }

echo "[INFO] Building Retypr ($PROFILE) for Linux..."
cargo build $FLAGS

OUT="dist/linux-$(uname -m)"
mkdir -p "$OUT"
cp "target/$PROFILE/Retypr" "$OUT/"
cp README.md "$OUT/"
echo "[OK] Built: $OUT/Retypr"
echo "[HINT] Switching the layout needs xkb-switch (X11). Wayland/evdev would need membership of the 'input' group."
