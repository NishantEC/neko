#!/bin/bash
# Luau plugins' committed snapshots: the example pack's card and view over made-up data, a card that stopped, more cards than fit, the
# status rows in Plugins and in a pack's page. Synthetic data only, headless, no daemon, no window.
#
#   apps/neko-rune/scripts/snap-luau.sh [out-dir]     (default docs/design/rune)
#
# Writes luau-<scene>-<light|dark>.png at 1360x840, 1x, for every `luau-*` scene (src/scenes_luau.rs). The example's own files
# (packs/hello-luau/ui/*.luau) are the ones drawn.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="${1:-$repo/docs/design/rune}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
export CARGO_INCREMENTAL=0
cd "$here"
cargo build --quiet
bin="$CARGO_TARGET_DIR/debug/neko-rune"
mkdir -p "$out"
for scene in $("$bin" --list-scenes | grep '^luau-'); do
  for theme in light dark; do
    "$bin" --snap "$out/${scene}-$theme.png" --scene "$scene" --theme "$theme" --size 1360x840 --scale 1 >/dev/null 2>&1
  done
  echo "$scene"
done
