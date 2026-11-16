#!/bin/bash
# Parity batch A's committed snapshots: synthetic demo data only, headless, no daemon, no window.
#
#   apps/neko-rune/scripts/snap-batch-a.sh [out-dir]     (default docs/design/rune)
#
# Writes batch-a-<scene>-<light|dark>.png at 1360x840, 1x, for every `ba-*` scene (src/scenes_batch_a.rs): the sky badge, the
# model chip's popover, the task menu, the confirm dialog, Stop all, All sessions, the / and @ popovers, files attached, the drop
# overlay, the queue tray, the palette, the shortcut overlay, the right-click menu on selected text, the workspace switcher,
# Closed recently, the cadence editor, the Month, an event's popover and the toasts.
# Pictures of the person's own data are never committed (the repo is public): see `polish-pairs.sh`.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="${1:-$repo/docs/design/rune}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
cd "$here"
cargo build --quiet
bin="$CARGO_TARGET_DIR/debug/neko-rune"
mkdir -p "$out"
for scene in $("$bin" --list-scenes | grep '^ba-'); do
  for theme in light dark; do
    "$bin" --snap "$out/batch-a-${scene#ba-}-$theme.png" --scene "$scene" --theme "$theme" --size 1360x840 --scale 1 >/dev/null 2>&1
  done
  echo "batch-a-${scene#ba-}"
done
