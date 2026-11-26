#!/bin/bash
# Quits the development copy of Neko if it is open, builds the current source, and opens it again with the same data.
#
#   scripts/show.sh          build, quit the open copy, open the new one
#   scripts/show.sh --quit   only quit the open copy
#
# The data folder is kept between runs, so tasks made while trying a build are still there in the next one. It is a
# development folder of its own ($NEKO_DEV_DATA, default "~/Library/Application Support/neko-dev"): an installed Neko and its
# data are never touched. Only the processes this script started are stopped, by the PIDs it wrote down.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
target="${NEKO_TARGET:-$HOME/Library/Caches/neko/target}"
data="${NEKO_DEV_DATA:-$HOME/Library/Application Support/neko-dev}"
pid_file="$data/show.pid"

alive() { [[ -n "${1:-}" ]] && kill -0 "$1" 2>/dev/null; }

quit_open_copy() {
  local app="" daemon=""
  [[ -f "$pid_file" ]] && app="$(cat "$pid_file" 2>/dev/null || true)"
  if [[ -f "$data/daemon.json" ]]; then
    daemon="$(sed -E 's/.*"pid":([0-9]+).*/\1/' "$data/daemon.json" 2>/dev/null || true)"
  fi
  if alive "$app"; then
    kill -TERM "$app" 2>/dev/null || true
    for _ in $(seq 1 50); do alive "$app" || break; sleep 0.1; done
  fi
  # The daemon belongs to this data folder alone; stop it so the new build starts its own.
  if alive "$daemon"; then
    kill -TERM "$daemon" 2>/dev/null || true
    for _ in $(seq 1 50); do alive "$daemon" || break; sleep 0.1; done
  fi
  rm -f "$pid_file"
}

if [[ "${1:-}" == "--quit" ]]; then
  quit_open_copy
  exit 0
fi

mkdir -p "$data"
CARGO_TARGET_DIR="$target/core" cargo build --manifest-path "$root/Cargo.toml" -p neko-daemon
CARGO_TARGET_DIR="$target/app" cargo build --manifest-path "$root/apps/neko/Cargo.toml"
quit_open_copy
NEKO_DATA_DIR="$data" NEKO_DAEMON_BIN="$target/core/debug/neko-daemon" RUNE_PREFS_DIR="$data/prefs" \
  nohup "$target/app/debug/neko" >"$data/app.log" 2>&1 &
echo $! >"$pid_file"
echo "Neko is open (data: $data)"
