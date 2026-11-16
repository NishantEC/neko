#!/bin/bash
# Build a release and open "Neko (Rune)" against the running daemon, beside the installed app.
#
#   apps/neko-rune/scripts/run.sh                      the real daemon (the installed Neko)
#   apps/neko-rune/scripts/run.sh --data-dir DIR       a daemon whose data lives in DIR (a test one)
#   apps/neko-rune/scripts/run.sh --no-launch          build only
#
# P0 is read-only: the window listens to the daemon's Unix socket (the same snapshot the web UI
# holds) and never writes to it or to its database. Every page says "Coming in P1" and its button
# opens the same place in the web UI. The installed app and what it ships are untouched; this is a
# second, separate process, and quitting it (⌘Q) leaves Neko as it was.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
launch=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    --data-dir) export NEKO_DATA_DIR="$2"; shift 2;;
    --no-launch) launch=0; shift;;
    -h|--help) sed -n '2,12p' "$0"; exit 0;;
    *) echo "unknown option: $1" >&2; exit 2;;
  esac
done

if command -v "$HOME/.agents/bin/machine-ok" >/dev/null; then
  verdict="$("$HOME/.agents/bin/machine-ok" 2>&1 | head -n 1)"
  case "$verdict" in WAIT*) echo "machine says: $verdict. Try again later." >&2; exit 3;; esac
fi

"$here/scripts/patched-rune.sh" --ensure
cargo build --release --quiet
bin="$CARGO_TARGET_DIR/release/neko-rune"
echo "built $bin"
(( launch )) || exit 0

data="${NEKO_DATA_DIR:-$HOME/Library/Application Support/neko}"
if [[ ! -S "$data/neko.sock" ]]; then
  echo "note: no daemon socket at $data/neko.sock; the window will say Neko isn't running and keep trying."
fi
exec "$bin"
