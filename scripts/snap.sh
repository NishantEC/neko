#!/bin/bash
# Renders every scene headless, light and dark, wide and at the smallest window, into a folder (default /tmp/neko-s0).
# Placeholder data only (apps/neko/src/scenes.rs). Look at each picture before calling a change done.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-/tmp/neko-s0}"
target="${NEKO_TARGET:-$HOME/Library/Caches/neko/target}"
CARGO_TARGET_DIR="$target/app" cargo build --quiet --manifest-path "$root/apps/neko/Cargo.toml"
mkdir -p "$out"
bin="$target/app/debug/neko"
for scene in $("$bin" --list-scenes); do
  for theme in light dark; do
    "$bin" --snap "$out/$scene-$theme.png" --scene "$scene" --size 1200x800 --theme "$theme" --settle --chrome >/dev/null 2>&1
  done
  "$bin" --snap "$out/$scene-760.png" --scene "$scene" --size 760x560 --theme light --settle --chrome >/dev/null 2>&1
done
echo "Pictures in $out"
