#!/usr/bin/env bash
# Сборка под Linux (X11).
# Использование: ./build.sh [debug] [--install-deps]
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
  echo "[INFO] Установка системных зависимостей..."
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
    echo "[WARN] Неизвестный пакетный менеджер, поставьте зависимости вручную (см. README)."
  fi
}

[ "$INSTALL_DEPS" = 1 ] && install_deps

command -v cargo >/dev/null || { echo "[ERROR] cargo не найден: https://rustup.rs"; exit 1; }

echo "[INFO] Сборка retypr ($PROFILE) для Linux..."
cargo build $FLAGS

OUT="dist/linux-$(uname -m)"
mkdir -p "$OUT"
cp "target/$PROFILE/retypr" "$OUT/"
cp README.md "$OUT/"
echo "[OK] Готово: $OUT/retypr"
echo "[HINT] Для переключения раскладки нужен xkb-switch (X11). Для Wayland/evdev потребуется группа 'input'."
