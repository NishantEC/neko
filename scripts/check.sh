#!/bin/bash
# The gate: nothing reaches main or /Applications unless this passes.
#
#   scripts/check.sh          clippy's always-a-defect lints and every test, in both workspaces, and the privacy check
#   scripts/check.sh --quick  compile only, both workspaces (the pre-commit hook)
#
# Two Cargo workspaces: the daemon's (repository root) and the app's (apps/neko, its own because of Rune's patch table).
# Build output goes to $NEKO_TARGET/core and $NEKO_TARGET/app (default ~/Library/Caches/neko/target).
# A failed step's output is kept in $TMPDIR/neko-check/<step>.log. No formatter runs here on purpose.
set -uo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
mode=full
for option in "$@"; do
  case "$option" in
    --quick) mode=quick ;;
    --help) sed -n '2,9p' "$0"; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$option" >&2; exit 2 ;;
  esac
done

# Keep the machine cool: where the shared check exists, wait (up to ten minutes) until it says OK.
if [[ -x "$HOME/.agents/bin/machine-ok" ]]; then
  waited=0
  until verdict="$("$HOME/.agents/bin/machine-ok" 2>/dev/null | head -n 1)"; [[ "$verdict" == OK* ]]; do
    if (( waited >= 600 )); then printf 'The machine is still busy (%s); building anyway.\n' "$verdict"; break; fi
    (( waited == 0 )) && printf 'Waiting for the machine to cool (%s)…\n' "$verdict"
    sleep 30; waited=$((waited + 30))
  done
fi

target="${NEKO_TARGET:-$HOME/Library/Caches/neko/target}"
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

# Lints that are always a defect in committed code. The wider clippy set is advice, shown but never blocking.
lints=(-D unused_must_use -D clippy::correctness -D clippy::suspicious -D clippy::dbg_macro -D clippy::todo -D clippy::unimplemented)

core() { CARGO_TARGET_DIR="$target/core" cargo "$@"; }
app() { (cd "$root/apps/neko" && CARGO_TARGET_DIR="$target/app" NEKO_DAEMON_BIN="$target/core/debug/neko-daemon" cargo "$@"); }

printf 'neko check (%s)\n' "$mode"
if [[ "$mode" == quick ]]; then
  step core-check core check --workspace --all-targets --quiet
  step app-check app check --all-targets --quiet
else
  step core-clippy core clippy --workspace --all-targets --quiet -- "${lints[@]}"
  step core-test core test --workspace --quiet
  # The app's end-to-end test starts this binary.
  step daemon-build core build --quiet -p neko-daemon
  step app-clippy app clippy --all-targets --quiet -- "${lints[@]}"
  step app-test app test --quiet
  step privacy bash "$root/scripts/privacy-check.sh"
fi

if (( ${#failed[@]} )); then
  printf 'Failed: %s\n' "${failed[*]}"
  exit 1
fi
printf 'All checks passed.\n'
