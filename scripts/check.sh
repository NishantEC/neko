#!/bin/bash
# The one gate before code reaches main or /Applications.
#
#   scripts/check.sh           everything: Rust lints and tests, Swift tests
#   scripts/check.sh --quick   compile only (the pre-commit hook)
#   scripts/check.sh --rune    also the Rune client (apps/neko-rune), even on a cold cache
#   scripts/check.sh --no-rune never the Rune client
#
# The Rune client is its own Cargo workspace (apps/neko-rune, docs/decisions/0002). Its check is
# about 25 s with a warm cache and about 3 minutes cold (Rune and wgpu compile once), so a full
# check runs it, as advice that never blocks, only when the cache from an earlier build exists;
# with --rune it runs always and blocks.
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
rune=auto
for option in "$@"; do
  case "$option" in
    --quick) mode=quick ;;
    --rune) rune=yes ;;
    --no-rune) rune=no ;;
    --help) sed -n '2,12p' "$0"; exit 0 ;;
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
# The optional GPUI client is its own workspace and needs the patched fork;
# it is checked only on a Mac that has run scripts/setup-gpui-patch.sh.
if [[ -d "$repo_root/.gpui-fork-patched" ]]; then
  step gpui-client cargo check --manifest-path crates/neko/Cargo.toml --target-dir target --quiet
fi

# The Rune client: warm cache or asked for, never in the quick (pre-commit) pass.
rune_cache="$HOME/Library/Caches/neko/rune-target"
if [[ "$mode" == full && "$rune" != no && -d "$repo_root/apps/neko-rune" ]]; then
  if [[ "$rune" == yes || -d "$rune_cache/debug/deps" ]]; then
    # Always its own cache, whatever target dir the daemon's steps were given.
    step rune-client env CARGO_TARGET_DIR="$rune_cache" "$repo_root/apps/neko-rune/scripts/check.sh"
    # Without --rune it is advisory: a Rune problem is shown but never blocks the daemon's gate
    # (the installer runs this gate too).
    if [[ "$rune" != yes && "${failed[*]:-}" == *rune-client* ]]; then
      kept=(); for f in ${failed[@]+"${failed[@]}"}; do [[ "$f" == rune-client ]] || kept+=("$f"); done
      failed=(${kept[@]+"${kept[@]}"})
      printf '        (advisory: pass --rune to make the Rune client block)\n'
    fi
  fi
fi

if (( ${#failed[@]} )); then
  printf 'Failed: %s\n' "${failed[*]}"
  exit 1
fi
printf 'All checks passed.\n'
