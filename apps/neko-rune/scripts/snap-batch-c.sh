#!/bin/bash
# Parity batch C's committed snapshots: the Setup sections on synthetic demo data only, headless, no daemon, no window.
#
#   apps/neko-rune/scripts/snap-batch-c.sh [out-dir]     (default docs/design/rune)
#
# Writes batch-c-<scene>-<light|dark>.png at 1360x840, 1x, for every `setup-*` scene (src/scenes_setup.rs): Workspaces (closed, open,
# its dialog), Watching, Tools (the list, a connection's detail, a signed-out one, adding a connection), Skills with a diff proposal,
# Memory, Profiles, Schedules and a coral confirmation. Pictures of the person's own data are never committed (the repo is public):
# see `polish-pairs.sh`, whose `bc-*` pairs go to /tmp/neko-polish.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="${1:-$repo/docs/design/rune}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
cd "$here"
cargo build --quiet
bin="$CARGO_TARGET_DIR/debug/neko-rune"
mkdir -p "$out"
for scene in $("$bin" --list-scenes | grep '^setup-'); do
  for theme in light dark; do
    "$bin" --snap "$out/batch-c-${scene#setup-}-$theme.png" --scene "$scene" --theme "$theme" --size 1360x840 --scale 1 >/dev/null 2>&1
  done
  echo "batch-c-${scene#setup-}"
done
