#!/bin/bash
# Builds Neko from source and opens it against a throwaway data folder, for development.
#
#   scripts/run.sh          debug build, a new empty data folder, removed afterwards
#   scripts/run.sh --keep   keep the data folder (its path is printed)
#
# The app starts the daemon in that folder; when the app quits, this script stops that daemon (by the PID it wrote) and removes
# the folder. The real data folder and the installed app's window settings are never touched.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
keep=no
[[ "${1:-}" == "--keep" ]] && keep=yes
target="${NEKO_TARGET:-$HOME/Library/Caches/neko/target}"
CARGO_TARGET_DIR="$target/core" cargo build --manifest-path "$root/Cargo.toml" -p neko-daemon
CARGO_TARGET_DIR="$target/app" cargo build --manifest-path "$root/apps/neko/Cargo.toml"
data="$(mktemp -d "${TMPDIR:-/tmp}/neko-dev.XXXXXX")"
echo "Data folder: $data"
stop_daemon() {
  if [[ -f "$data/daemon.json" ]]; then
    pid="$(sed -E 's/.*"pid":([0-9]+).*/\1/' "$data/daemon.json")"
    [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  fi
  if [[ "$keep" == no ]]; then rm -rf "$data"; else echo "Kept: $data"; fi
}
trap stop_daemon EXIT
NEKO_DATA_DIR="$data" NEKO_DAEMON_BIN="$target/core/debug/neko-daemon" RUNE_PREFS_DIR="$data/prefs" "$target/app/debug/neko"
