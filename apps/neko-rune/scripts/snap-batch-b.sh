#!/bin/bash
# Parity batch B's committed snapshots: synthetic demo data only, headless, no daemon, no window.
#
#   apps/neko-rune/scripts/snap-batch-b.sh [out-dir]     (default docs/design/rune)
#
# Writes batch-b-<scene>-<light|dark>.png at 1360x840, 1x, for every `bb-*` scene (src/scenes_batch_b.rs): the Board (resting, a stopped
# task carried over Running, a running task carried over Needs you), the Tree (open, folded), Pull requests (the list, Close with a
# reason, Merge all ready, the overflow), the Details tab and the `/` popover in the session's reply box. Pictures of the person's own
# data are never committed (the repo is public): see `polish-pairs-b.sh`.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="${1:-$repo/docs/design/rune}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
cd "$here"
cargo build --quiet
bin="$CARGO_TARGET_DIR/debug/neko-rune"
mkdir -p "$out"
for scene in $("$bin" --list-scenes | grep '^bb-'); do
  for theme in light dark; do
    "$bin" --snap "$out/batch-b-${scene#bb-}-$theme.png" --scene "$scene" --theme "$theme" --size 1360x840 --scale 1 >/dev/null 2>&1
  done
  echo "batch-b-${scene#bb-}"
done
