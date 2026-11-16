#!/bin/bash
# The Rune client's gate: build, clippy, every test (unit, shell, read-only, headless snapshots).
#
#   apps/neko-rune/scripts/check.sh
#
# Warm, on this Mac, about a minute (cold, with the dependency cache empty, about three: Rune
# and wgpu compile once into $CARGO_TARGET_DIR). It is its own Cargo workspace and never touches
# the daemon's `target/`. The repo's `scripts/check.sh --rune` runs this one.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"

# Same courtesy as the repo's gate: this Mac runs many agents, so wait for the load to drop.
load() { /usr/sbin/sysctl -n vm.loadavg | awk '{print int($2)}'; }
waited=0
while (( $(load) >= 20 && waited < 300 )); do
  (( waited == 0 )) && printf 'Load average %s; waiting for it to drop below 20…\n' "$(load)"
  sleep 15; waited=$((waited + 15))
done

# Local fixes to Rune (patches/*.patch), when there are any, come from a patched checkout.
"$here/scripts/patched-rune.sh" --ensure || exit 1

logs="${TMPDIR:-/tmp}/neko-rune-check"
mkdir -p "$logs"
failed=()
step() {
  local name="$1"; shift
  local started=$SECONDS
  if "$@" >"$logs/$name.log" 2>&1; then
    printf '  ok    %-14s %3ss\n' "$name" $((SECONDS - started))
  else
    printf '  FAIL  %-14s %3ss  (%s)\n' "$name" $((SECONDS - started)) "$logs/$name.log"
    tail -n 25 "$logs/$name.log" | sed 's/^/        /'
    failed+=("$name")
  fi
}

printf 'neko-rune check (Rune %s)\n' "$(head -n 1 RUNE_REV | cut -c1-9)"
step build cargo build --all-targets --quiet
step clippy cargo clippy --all-targets --quiet -- -D warnings
step test cargo test --quiet
step pin "$here/scripts/bump-rune.sh" --verify

if (( ${#failed[@]} )); then
  printf 'Failed: %s\n' "${failed[*]}"
  exit 1
fi
printf 'All Rune checks passed.\n'
