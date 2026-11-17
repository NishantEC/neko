#!/bin/bash
# The isolated daemon the identity pass reads from (docs/knowledge/rune-identity.md).
#
#   identity-harness.sh start    copy the live database (sqlite3 .backup, read-only on the live side), build the web UI of this
#                                checkout, start `web_harness` on the copy (port 4771), print the data folder
#   identity-harness.sh stop     stop the harness by pid
#
# Real-data folders stay in /tmp/neko-rune-identity and are never committed. The harness refuses a data folder outside /tmp.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
base=/tmp/neko-rune-identity
data=$base/data
web=$base/web-dist
port=4771
harness="${HARNESS:-$HOME/Library/Caches/neko/target/debug/web_harness}"
live="$HOME/Library/Application Support/neko"

case "${1:-}" in
  start)
    mkdir -p "$data"
    if [[ -f $base/harness.pid ]] && ps -p "$(cat $base/harness.pid)" -o command= 2>/dev/null | grep -q web_harness; then echo "$data"; exit 0; fi
    rm -f "$data"/neko.db* "$data"/neko.sock
    sqlite3 -readonly "$live/neko.db" ".backup $data/neko.db"
    # The web UI of THIS checkout (embed mode included), built into /tmp; node_modules come from the main checkout when this one has none.
    if [[ ! -e $root/web/node_modules && -d "$HOME/Documents/neko/web/node_modules" ]]; then ln -s "$HOME/Documents/neko/web/node_modules" "$root/web/node_modules"; fi
    if [[ "${REBUILD_WEB:-1}" == 1 || ! -f $web/index.html ]]; then (cd "$root/web" && node_modules/.bin/vite build --outDir "$web" --emptyOutDir >"$base/web-build.log" 2>&1); fi
    # `;` not `&&`: only the one command goes to the background, so `$!` is the harness itself (not a lingering subshell that holds the
    # caller's `$(...)` pipe open and makes `identity.sh` hang), and `stop` finds it by name.
    ( cd "$data"; nohup env -i PATH="$PATH" HOME="$data" NEKO_DATA_DIR="$data" NEKO_WEB_PORT="$port" NEKO_WEB_DIR="$web" "$harness" >"$base/harness.log" 2>&1 </dev/null & echo $! >"$base/harness.pid" )
    for _ in $(seq 1 60); do [[ -S "$data/neko.sock" ]] && break; sleep 0.2; done
    [[ -S "$data/neko.sock" ]] || { echo "the harness did not start: see $base/harness.log" >&2; exit 1; }
    echo "$data"
    ;;
  stop)
    if [[ -f $base/harness.pid ]]; then
      pid="$(cat $base/harness.pid)"
      if ps -p "$pid" -o command= 2>/dev/null | grep -q web_harness; then kill -TERM "$pid" 2>/dev/null || true; fi
      rm -f $base/harness.pid
    fi
    ;;
  *) echo "usage: $0 start|stop" >&2; exit 2;;
esac
