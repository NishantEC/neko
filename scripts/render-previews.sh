#!/bin/bash
# Render the native shell components offscreen, in light and dark, to PNGs.
# No window is shown and nothing takes keyboard focus, so it is safe to run
# while someone uses this Mac. Usage: scripts/render-previews.sh [output-dir]
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-${TMPDIR:-/tmp}/neko-previews}"
rm -rf "$out"; mkdir -p "$out"
NEKO_RENDER_DIR="$out" swift test --package-path "$repo_root/native/NekoKit" --filter RenderPreviewsTests --quiet
ls -1 "$out"/*.png
