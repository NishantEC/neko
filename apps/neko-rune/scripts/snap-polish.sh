#!/bin/bash
# The polish pass's committed snapshots: synthetic demo data only, headless, no daemon, no window.
#
#   apps/neko-rune/scripts/snap-polish.sh [out-dir]     (default docs/design/rune)
#
# Writes polish-<scene>-<light|dark>.png at 1360x840, 1x: Home (greeting, status line, composer, the art), the sessions
# column beside a session, Neko's column with the Activity line open, Routines and Settings.
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
for scene in home live activity routines settings; do
  for theme in light dark; do
    "$bin" --snap "$out/polish-$scene-$theme.png" --scene "$scene" --theme "$theme" --size 1360x840 --scale 1 >/dev/null 2>&1
  done
  echo "polish-$scene"
done
