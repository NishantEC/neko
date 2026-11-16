#!/bin/bash
# Headless snapshots and element-tree dumps of the shell: no window, no focus, no daemon.
#
#   apps/neko-rune/scripts/snap.sh [out-dir] [scene ...]     (default /tmp/neko-rune-snaps, every scene)
#
# NEKO_RUNE_SIZE (1280x800) and NEKO_RUNE_SCALE (2) change the picture; docs/design/rune uses 1360x840 at 1.
# Writes <scene>-light.png / <scene>-dark.png (2x) and <scene>-dark.tree.json. The same renderer
# as the window, a fixed virtual clock, so a snapshot is a fact about the layout, not a screenshot
# of whatever the Mac was doing. Debug builds always have these flags; release needs the
# `tooling` feature, which this crate turns on.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-/tmp/neko-rune-snaps}"
shift || true
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
cd "$here"
cargo build --quiet
bin="$CARGO_TARGET_DIR/debug/neko-rune"
mkdir -p "$out"
scenes=("$@")
if [ ${#scenes[@]} -eq 0 ]; then
  scenes=($("$bin" --list-scenes))
fi
for scene in "${scenes[@]}"; do
  for theme in light dark; do
    "$bin" --snap "$out/$scene-$theme.png" --scene "$scene" --theme "$theme" --size "${NEKO_RUNE_SIZE:-1280x800}" --scale "${NEKO_RUNE_SCALE:-2}" >/dev/null 2>&1
  done
  "$bin" --tree "$out/$scene-dark.tree.json" --scene "$scene" --theme dark --size 1280x800 >/dev/null 2>&1
  echo "$scene"
done
echo "wrote $out"
