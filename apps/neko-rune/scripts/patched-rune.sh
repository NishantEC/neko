#!/bin/bash
# Local fixes to Rune, applied by the build. Rune takes no pull requests, so a fix we need lives
# here as a patch in apps/neko-rune/patches/NNNN-name.patch (a `git diff` made in a Rune checkout
# at the pinned rev), and is reported upstream as an issue.
#
#   scripts/patched-rune.sh --ensure     make `.cargo/config.toml` match patches/ (no patches: remove it)
#   scripts/patched-rune.sh --dry-run    show what --ensure would write
#
# With patches: it makes a clean checkout of Rune at RUNE_REV under
# `$CARGO_TARGET_DIR/../rune-patched/<rev>/`, applies each patch in order, and writes a
# `.cargo/config.toml` (git-ignored) that points every crate of Rune's workspace at it, so the
# app and Rune's own crates still agree on one copy. Without patches the build uses Rune straight
# from git and there is no config file. A patch that no longer applies stops the build: re-make it
# against the new rev (scripts/bump-rune.sh says when a bump needs that).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
repo="https://github.com/anomalyco/rune"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
rev="$(head -n 1 RUNE_REV | tr -d '[:space:]')"
shopt -s nullglob
patches=(patches/*.patch)
dry=0; [[ "${1:-}" == "--dry-run" ]] && dry=1

if (( ${#patches[@]} == 0 )); then
  (( dry )) && { echo "no patches: the build uses Rune from git, no .cargo/config.toml"; exit 0; }
  rm -f .cargo/config.toml; rmdir .cargo 2>/dev/null || true
  exit 0
fi

checkout="$(dirname "$CARGO_TARGET_DIR")/rune-patched/$rev"
stamp="$checkout/.patched-with"
want="$(cat "${patches[@]}" | shasum | cut -d' ' -f1)"
if (( dry )); then
  echo "would use $checkout with: ${patches[*]}"
else
  if [[ ! -f "$stamp" || "$(cat "$stamp")" != "$want" ]]; then
    rm -rf "$checkout"; mkdir -p "$(dirname "$checkout")"
    git clone --quiet --no-checkout "$repo" "$checkout"
    git -C "$checkout" checkout --quiet "$rev"
    for p in "${patches[@]}"; do
      git -C "$checkout" apply --whitespace=nowarn "$here/$p" || { echo "patch $p does not apply to Rune $rev" >&2; exit 1; }
    done
    echo "$want" > "$stamp"
  fi
fi

# Every crate of Rune's workspace is patched to the checkout, plus the vendored crates of its
# own patch table (winit, wgpu-hal, ...), so there is one copy of each.
config=$'[patch."'"$repo"$'"]\n'
for c in "$checkout"/crates/*/Cargo.toml; do
  name="$(basename "$(dirname "$c")")"
  config+="$name = { path = \"$checkout/crates/$name\" }"$'\n'
done
config+=$'\n[patch.crates-io]\n'
for v in "$checkout"/vendor/*/Cargo.toml; do
  name="$(basename "$(dirname "$v")")"
  grep -Eq "^$name = " Cargo.toml && config+="$name = { path = \"$checkout/vendor/$name\" }"$'\n'
done
if (( dry )); then printf '%s' "$config"; else mkdir -p .cargo; printf '%s' "$config" > .cargo/config.toml; fi
