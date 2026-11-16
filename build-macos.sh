#!/usr/bin/env bash
# Build Retypr for macOS: universal binary (arm64 + x86_64) -> Retypr.app -> .dmg
#
# Usage:  ./build-macos.sh [--no-dmg] [--run]
#   --no-dmg   stop after creating dist/macos/Retypr.app
#   --run      open the DMG (or the app with --no-dmg) when finished
#
# Environment (all optional):
#   CODESIGN_IDENTITY   signing identity, e.g. "Developer ID Application: Name (TEAMID)".
#                       Default "-" = ad-hoc signature (works locally; Gatekeeper warns on other Macs).
#   NOTARY_PROFILE      notarytool keychain profile (xcrun notarytool store-credentials ...).
#                       With a Developer ID identity this notarizes and staples the DMG.
#   LSUIELEMENT         "true" (default): background app, no Dock icon. "false": show in the Dock.
#   MACOSX_DEPLOYMENT_TARGET   minimum macOS version (default 11.0).
#
# Needs a Mac: Xcode Command Line Tools (xcode-select --install) and Rust (https://rustup.rs).
# Optional: `brew install create-dmg` for a nicer DMG window (plain hdiutil is used otherwise).
set -euo pipefail
cd "$(dirname "$0")"

APP_NAME="Retypr"
BIN_NAME="Retypr"
BUNDLE_ID="com.retypr.app"
IDENTITY="${CODESIGN_IDENTITY:--}"
LSUI="${LSUIELEMENT:-true}"
case "$LSUI" in true|false) ;; *) LSUI="true" ;; esac
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"

MAKE_DMG=1
RUN_AFTER=0
for a in "$@"; do
  case "$a" in
    --no-dmg) MAKE_DMG=0 ;;
    --run)    RUN_AFTER=1 ;;
    -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
    *) echo "[ERROR] unknown argument: $a"; exit 1 ;;
  esac
done

info()  { echo "[INFO] $*"; }
fail()  { echo "[ERROR] $*" >&2; exit 1; }

# ---------- checks ----------
[ "$(uname)" = "Darwin" ] || fail "this script must be run on macOS (it needs lipo, iconutil, codesign, hdiutil)."
for tool in cargo rustup lipo codesign hdiutil iconutil sips shasum; do
  command -v "$tool" >/dev/null 2>&1 || fail "'$tool' not found. Install Rust (https://rustup.rs) and Xcode Command Line Tools (xcode-select --install)."
done

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/version *= *"([^"]+)".*/\1/')"
[ -n "$VERSION" ] || fail "could not read the version from Cargo.toml"
info "Retypr $VERSION | identity: $([ "$IDENTITY" = "-" ] && echo 'ad-hoc' || echo "$IDENTITY") | min macOS $MACOSX_DEPLOYMENT_TARGET"

# ---------- make sure cargo notices changed sources ----------
# (cargo compares file timestamps; files unpacked from a zip can look older than the last build)
mkdir -p target
SIG="$( { find src assets build.rs Cargo.toml Cargo.lock tools -type f 2>/dev/null || true; } | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256 | cut -d' ' -f1 )"
MARKER="target/.retypr-sources.sig"
OLD_SIG=""
[ -f "$MARKER" ] && OLD_SIG="$(cat "$MARKER")"
if [ "$SIG" != "$OLD_SIG" ]; then
  find src assets -type f -exec touch {} +
  touch build.rs Cargo.toml 2>/dev/null || true
  echo "$SIG" > "$MARKER"
  info "Sources changed since the last build - timestamps refreshed, cargo will rebuild."
fi

# ---------- build ----------
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
info "Building for Apple Silicon (aarch64)..."
cargo build --release --target aarch64-apple-darwin
info "Building for Intel (x86_64)..."
cargo build --release --target x86_64-apple-darwin

OUT="dist/macos"
mkdir -p "$OUT"
UNIVERSAL="$OUT/$BIN_NAME"
lipo -create \
  "target/aarch64-apple-darwin/release/$BIN_NAME" \
  "target/x86_64-apple-darwin/release/$BIN_NAME" \
  -output "$UNIVERSAL"
info "Universal binary: $(lipo -archs "$UNIVERSAL")"

# ---------- .app bundle ----------
APP="$OUT/$APP_NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$UNIVERSAL" "$APP/Contents/MacOS/$BIN_NAME"
chmod +x "$APP/Contents/MacOS/$BIN_NAME"

# default sounds ship inside the bundle; Retypr copies them to its data folder on first run
if [ -d assets/sounds ]; then
  mkdir -p "$APP/Contents/Resources/sounds"
  cp assets/sounds/* "$APP/Contents/Resources/sounds/"
fi

# icon: assets/retypr-1024.png -> AppIcon.icns (python3 tools/make_icon.py regenerates the PNG)
ICON_SRC="assets/retypr-1024.png"
if [ -f "$ICON_SRC" ]; then
  ICONSET="$(mktemp -d)/AppIcon.iconset"
  mkdir -p "$ICONSET"
  mkicon() { sips -z "$1" "$1" "$ICON_SRC" --out "$ICONSET/$2" >/dev/null; }
  mkicon 16  icon_16x16.png
  mkicon 32  icon_16x16@2x.png
  mkicon 32  icon_32x32.png
  mkicon 64  icon_32x32@2x.png
  mkicon 128 icon_128x128.png
  mkicon 256 icon_128x128@2x.png
  mkicon 256 icon_256x256.png
  mkicon 512 icon_256x256@2x.png
  mkicon 512 icon_512x512.png
  cp "$ICON_SRC" "$ICONSET/icon_512x512@2x.png"
  iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
else
  echo "[WARN] $ICON_SRC not found - the app will have the default icon."
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>$BIN_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
  <key>LSUIElement</key><$LSUI/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>Retypr</string>
</dict>
</plist>
PLIST

# ---------- sign the app ----------
if [ "$IDENTITY" = "-" ]; then
  codesign --force --deep --sign - "$APP"
else
  # hardened runtime + secure timestamp are required for notarization
  codesign --force --deep --options runtime --timestamp --sign "$IDENTITY" "$APP"
fi
codesign --verify --deep --strict "$APP"
info "App bundle: $APP"

if [ "$MAKE_DMG" = "0" ]; then
  [ "$RUN_AFTER" = "1" ] && open "$APP"
  exit 0
fi

# ---------- DMG ----------
DMG="dist/$APP_NAME-$VERSION-macos-universal.dmg"
mkdir -p dist
rm -f "$DMG"
STAGE="$(mktemp -d)/$APP_NAME"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
[ -f packaging/macos/README.txt ] && cp packaging/macos/README.txt "$STAGE/READ ME FIRST.txt"

if command -v create-dmg >/dev/null 2>&1; then
  info "Creating DMG with create-dmg..."
  create-dmg \
    --volname "$APP_NAME" \
    --window-size 600 400 \
    --icon-size 96 \
    --icon "$APP_NAME.app" 160 190 \
    --app-drop-link 440 190 \
    --hide-extension "$APP_NAME.app" \
    "$DMG" "$STAGE" || true
fi
if [ ! -f "$DMG" ]; then
  info "Creating DMG with hdiutil..."
  ln -s /Applications "$STAGE/Applications"
  hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO -fs HFS+ "$DMG" >/dev/null
fi
[ -f "$DMG" ] || fail "DMG was not created."

if [ "$IDENTITY" != "-" ]; then
  codesign --force --timestamp --sign "$IDENTITY" "$DMG"
  if [ -n "${NOTARY_PROFILE:-}" ]; then
    info "Notarizing (this can take a few minutes)..."
    xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$DMG"
  else
    echo "[NOTE] Signed but not notarized. Set NOTARY_PROFILE to notarize so Gatekeeper accepts it everywhere."
  fi
else
  echo "[NOTE] Ad-hoc signature: on another Mac, open the app with right click -> Open (see 'READ ME FIRST.txt' in the DMG)."
fi

hdiutil verify "$DMG" >/dev/null && info "DMG verified."
echo
echo "[OK] $DMG  ($(du -h "$DMG" | cut -f1))"
echo "     sha256: $(shasum -a 256 "$DMG" | cut -d' ' -f1)"
[ "$RUN_AFTER" = "1" ] && open "$DMG"
exit 0
