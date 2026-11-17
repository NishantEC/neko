#!/bin/bash
# The identity pass's committed snapshots: synthetic data only, headless (docs/knowledge/rune-identity.md).
#
#   apps/neko-rune/scripts/snap-identity.sh [out-dir]     (default docs/design/rune)
#
# Writes identity-setup-workspaces-dark.png (Setup inside the content region: the sessions column and the Neko column stay)
# and identity-neko-strip-dark.png (the Neko column folded to its 44 point strip: spark over the count).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="${1:-$repo/docs/design/rune}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
unset NEKO_DATA_DIR
cd "$here"
cargo build --quiet
for scene in setup-workspaces neko-strip; do
  "$CARGO_TARGET_DIR/debug/neko-rune" --snap "$out/identity-$scene-dark.png" --scene "$scene" --theme dark --size 1360x840 --scale 1 >/dev/null 2>&1
  echo "identity-$scene"
done
