#!/bin/bash
# The one gate before code reaches main or /Applications.
#
#   scripts/check.sh           everything: Rust lints and tests, Swift tests
#   scripts/check.sh --quick   compile only (the pre-commit hook)
#
# Prints one line per step and exits non-zero if any step failed. Output of a
# failed step is kept in $TMPDIR/neko-check/<step>.log.
#
# No formatter runs here on purpose: this codebase keeps long lines, and
# `cargo fmt --check` reports ~900 differences that are style, not defects.
set -uo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
mode=full
for option in "$@"; do
  case "$option" in
    --quick) mode=quick ;;
    --help) sed -n '2,10p' "$0"; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$option" >&2; exit 2 ;;
  esac
done

# This Mac runs many agents at once (~/.agents/RESOURCES.md): wait for load
# to drop below 20 before heavy work, for at most five minutes.
load() { /usr/sbin/sysctl -n vm.loadavg | awk '{print int($2)}'; }
waited=0
while (( $(load) >= 20 && waited < 300 )); do
  (( waited == 0 )) && printf 'Load average %s; waiting for it to drop below 20…\n' "$(load)"
  sleep 15; waited=$((waited + 15))
done

logs="${TMPDIR:-/tmp}/neko-check"
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

# Lints that are always a defect in committed code. The wider clippy set is
# advisory here; promote a lint to this list once the tree is clean of it.
deny_lints=(-D clippy::dbg_macro -D clippy::todo -D clippy::unimplemented -D clippy::print_literal)

printf 'neko check (%s)\n' "$mode"
if [[ "$mode" == quick ]]; then
  step rust-check cargo check --workspace --all-targets --quiet
  step swift-build swift build --package-path native/NekoKit --quiet
else
  step rust-clippy cargo clippy --workspace --all-targets --quiet -- "${deny_lints[@]}"
  step rust-test cargo test --workspace --quiet
  step swift-test swift test --package-path native/NekoKit --quiet
fi

if (( ${#failed[@]} )); then
  printf 'Failed: %s\n' "${failed[*]}"
  exit 1
fi
printf 'All checks passed.\n'
