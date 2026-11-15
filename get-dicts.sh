#!/usr/bin/env bash
# Download frequency dictionaries (en/ru/uk, 50k words each) used for automatic layout detection.
# Source: hermitdave/FrequencyWords (CC-BY-SA 4.0, OpenSubtitles data). Files stay on your machine.
set -e
if [ "$(uname)" = "Darwin" ]; then
  DIR="$HOME/Library/Application Support/retypr/dict"
else
  DIR="${XDG_CONFIG_HOME:-$HOME/.config}/retypr/dict"
fi
BASE="https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018"
mkdir -p "$DIR"
echo "[INFO] Folder: $DIR"
curl -fL "$BASE/en/en_50k.txt" -o "$DIR/en.txt"
curl -fL "$BASE/ru/ru_50k.txt" -o "$DIR/ru.txt"
curl -fL "$BASE/uk/uk_50k.txt" -o "$DIR/uk.txt" || echo "[WARN] uk.txt not downloaded - Ukrainian will use heuristics only."
[ -f "$DIR/exceptions.txt" ] || printf '# Words that must never be auto-converted, one per line (lowercase).\n# Example: retypr\n' > "$DIR/exceptions.txt"
echo "[OK] Done. Restart Retypr to load the dictionaries."
