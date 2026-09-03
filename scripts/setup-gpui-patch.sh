#!/usr/bin/env bash
set -euo pipefail

# Populates the local, patched copy of the `wingleeio/zed` gpui fork that
# this repo's root `Cargo.toml` `[patch."https://github.com/wingleeio/zed"]`
# section points `gpui`/`gpui_platform` at. Run this once before the first
# `cargo build`/`cargo test`/`cargo clippy` in a fresh checkout, and again
# any time the pinned rev in `crates/neko/Cargo.toml` changes or
# `.gpui-fork-patched/` (inside this repo, gitignored) is deleted.
#
# Why this exists at all, and what it fixes: `AGENTS.md`'s "Summon latency"
# section and `patches/gpui-0001-*.patch`'s own header. Short version: the
# pinned fork rev has a real ~30ms warm-summon regression (an inactive-
# window frame throttle with no exemption for a pending `on_next_frame`
# callback) that isn't fixed in any published gpui release; this script
# applies a small, local patch on top of the exact pinned rev rather than
# waiting on an upstream fix.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORK_URL="https://github.com/wingleeio/zed"
REV="$(grep -m1 'wingleeio/zed.*rev = "' "$REPO_ROOT/crates/neko/Cargo.toml" | sed -n 's/.*rev = "\([0-9a-f]*\)".*/\1/p')"

if [ -z "$REV" ]; then
  echo "setup-gpui-patch: could not find the pinned wingleeio/zed rev in crates/neko/Cargo.toml" >&2
  exit 1
fi

# **Inside the repo, not under $HOME.** Cargo's `[patch]` section takes a
# path but expands neither `~` nor environment variables, so an absolute
# path there is one machine's path — it broke `cargo build` for every clone
# but this author's. A path relative to the workspace root works for
# everybody; the directory is gitignored.
DEST="$REPO_ROOT/.gpui-fork-patched"
REV_MARKER="$DEST/.neko-patched-rev"

if [ -f "$REV_MARKER" ] && [ "$(cat "$REV_MARKER")" = "$REV" ]; then
  echo "setup-gpui-patch: $DEST already patched at $REV, nothing to do"
  exit 0
fi

echo "setup-gpui-patch: populating $DEST at rev $REV"
rm -rf "$DEST"
mkdir -p "$(dirname "$DEST")"

# Reuse cargo's own git checkout if it already fetched this exact rev (the
# normal case once neko has been built at least once) rather than paying
# for a second network fetch of the same large monorepo.
CARGO_CHECKOUT="$(find "$HOME/.cargo/git/checkouts" -maxdepth 2 -type d -name "${REV:0:7}*" 2>/dev/null | head -1)"
if [ -n "$CARGO_CHECKOUT" ] && [ -d "$CARGO_CHECKOUT/crates/gpui" ]; then
  echo "setup-gpui-patch: reusing cargo's own checkout at $CARGO_CHECKOUT"
  cp -R "$CARGO_CHECKOUT/." "$DEST"
else
  echo "setup-gpui-patch: no existing cargo checkout found for this rev, cloning fresh"
  git clone "$FORK_URL" "$DEST"
  (cd "$DEST" && git checkout "$REV")
fi

for p in "$REPO_ROOT"/patches/gpui-*.patch; do
  [ -e "$p" ] || continue
  echo "setup-gpui-patch: applying $(basename "$p")"
  (cd "$DEST" && git apply "$p")
done

echo "$REV" > "$REV_MARKER"
echo "setup-gpui-patch: done — gpui/gpui_platform at $DEST/crates/{gpui,gpui_platform}"
