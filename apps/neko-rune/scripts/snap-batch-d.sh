#!/bin/bash
# Parity batch D's committed snapshots: Settings (the rest), Usage, the first-run setup and Plugins on synthetic demo data only, headless, no daemon, no window.
#
#   apps/neko-rune/scripts/snap-batch-d.sh [out-dir]     (default docs/design/rune)
#
# Writes batch-d-<scene>-<light|dark>.png at 1360x840, 1x, for every `bd-*` scene (src/scenes_batch_d.rs). Pictures of the person's own data
# are never committed (the repo is public): see `polish-pairs-d.sh`, whose `bd-*` pairs go to /tmp/neko-polish.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="${1:-$repo/docs/design/rune}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
cd "$here"
cargo build --quiet
bin="$CARGO_TARGET_DIR/debug/neko-rune"
mkdir -p "$out"
for scene in $("$bin" --list-scenes | grep '^bd-'); do
  for theme in light dark; do
    "$bin" --snap "$out/batch-d-${scene#bd-}-$theme.png" --scene "$scene" --theme "$theme" --size 1360x840 --scale 1 >/dev/null 2>&1
  done
  echo "batch-d-${scene#bd-}"
done
