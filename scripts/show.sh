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
# A real bundle, so notifications, the Dock badge and neko:// links work in what the person sees. It is the development bundle:
# its own name and bundle id, the debug build, never installed.
app="$("$root/scripts/build-app.sh" --dev | tail -n 1)"
exe="$app/Contents/MacOS/neko"
quit_open_copy
/usr/bin/open -n -g "$app" --env "NEKO_DATA_DIR=$data" --env "RUNE_PREFS_DIR=$data/prefs"
# `open` does not say which process it started: find it by its exact executable path (read only; nothing is signalled here).
pid=""
for _ in $(seq 1 50); do
  pid="$(/bin/ps -axo pid=,comm= | awk -v exe="$exe" '$2 == exe { print $1 }' | tail -n 1)"
  [[ -n "$pid" ]] && break
  sleep 0.1
done
if [[ -z "$pid" ]]; then echo "Neko did not start" >&2; exit 1; fi
echo "$pid" >"$pid_file"
echo "Neko is open (data: $data)"
