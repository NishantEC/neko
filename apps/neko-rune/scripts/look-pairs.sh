#!/bin/bash
# Measures the look against the approved prototype (docs/knowledge/rune-look-2026-11.md): renders the `look-*` scenes headless at the
# prototype's window size (1440x900, 1x), dark and light, and prints the share of differing pixels per pair. Pictures go to /tmp/neko-look.
#
#   apps/neko-rune/scripts/look-pairs.sh            (needs pngjs and pixelmatch in /tmp/neko-look/tools, see look-diff.mjs)
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
proto="${NEKO_PROTOTYPE:-$HOME/Documents/neko-design/prototype-2026-11-21/shots}"
out=/tmp/neko-look
mkdir -p "$out"
(cd "$here" && cargo build --quiet)
bin="$CARGO_TARGET_DIR/debug/neko-rune"
for theme in dark light; do
  for pair in "home:look-home:home-0" "cat:look-cat:cat-miso-0"; do
    IFS=: read -r name scene shot <<<"$pair"
    "$bin" --snap "$out/$name-$theme.png" --scene "$scene" --theme "$theme" --size 1440x900 --scale 1 >/dev/null 2>&1
    full=$(node "$here/scripts/look-diff.mjs" "$out/$name-$theme.png" "$proto/$shot-$theme.png" "$out/$name-$theme")
    side=$(node "$here/scripts/look-diff.mjs" "$out/$name-$theme.png" "$proto/$shot-$theme.png" "$out/$name-$theme-sidebar" 0,0,248,900)
    echo "$name $theme  full $(echo "$full" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pct"])')%  sidebar $(echo "$side" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pct"])')%"
  done
done
