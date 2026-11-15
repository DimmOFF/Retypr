#!/usr/bin/env bash
# Сборка под macOS: universal binary (Intel + Apple Silicon).
# Использование: ./build-macos.sh [debug]
# Требуется: Xcode Command Line Tools (xcode-select --install) и Rust (https://rustup.rs)
set -euo pipefail
cd "$(dirname "$0")"

PROFILE=release; FLAGS="--release"
[ "${1:-}" = "debug" ] && { PROFILE=debug; FLAGS=""; }

command -v cargo >/dev/null || { echo "[ERROR] cargo не найден: https://rustup.rs"; exit 1; }
xcode-select -p >/dev/null 2>&1 || { echo "[ERROR] Установите: xcode-select --install"; exit 1; }

rustup target add aarch64-apple-darwin x86_64-apple-darwin

echo "[INFO] Сборка retypr ($PROFILE) для macOS (arm64 + x86_64)..."
cargo build $FLAGS --target aarch64-apple-darwin
cargo build $FLAGS --target x86_64-apple-darwin

OUT="dist/macos-universal"
mkdir -p "$OUT"
lipo -create \
  "target/aarch64-apple-darwin/$PROFILE/retypr" \
  "target/x86_64-apple-darwin/$PROFILE/retypr" \
  -output "$OUT/retypr"
cp README.md "$OUT/"

# Ad-hoc подпись, чтобы macOS не ругалась на неподписанный бинарник
codesign --force --sign - "$OUT/retypr" || true

echo "[OK] Готово: $OUT/retypr"
echo "[HINT] Выдайте разрешения: System Settings → Privacy & Security → Accessibility и Input Monitoring."
