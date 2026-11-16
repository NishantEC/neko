#!/bin/bash
# Headless snapshots of the shell with live rows from an isolated test daemon.
#
#   apps/neko-rune/scripts/snap-harness.sh [out-dir]     (default docs/design/rune)
#
# Starts the daemon's verification harness (`web_harness`: a real `server` over an isolated data
# folder under /tmp, seeded with the made-up workbench of scripts/fixtures/web-seed.mjs, no agents,
# no clipboard; never the person's own Neko), asks it for the workbench through the Rune client's
# own link (`neko-rune probe json`, read-only), and renders the `harness` scene in light and dark
# without opening a window. Then stops the harness it started, by its own pid.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="${1:-$repo/docs/design/rune}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"

if command -v "$HOME/.agents/bin/machine-ok" >/dev/null; then
  verdict="$("$HOME/.agents/bin/machine-ok" 2>&1 | head -n 1)"
  case "$verdict" in WAIT*) echo "machine says: $verdict. Try again later." >&2; exit 3;; esac
fi

# The harness is the daemon's own workspace (target/ beside the repo), the client is Rune's.
harness="$repo/target/debug/web_harness"
[[ -x "$harness" ]] || (cd "$repo" && env -u CARGO_TARGET_DIR cargo build --quiet -p neko-daemon --bin web_harness)
(cd "$here" && cargo build --quiet)
bin="$CARGO_TARGET_DIR/debug/neko-rune"

data="/tmp/neko-rune-harness-data-$$"
port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
seed="$data-seed.json"
web="$data-web"
rm -rf "$data" "$web"; mkdir -p "$data" "$web" "$out"
echo '<html><script src="/assets/x.js"></script></html>' > "$web/index.html"
(cd "$repo" && node --input-type=module -e 'import { webSeed } from "./scripts/fixtures/web-seed.mjs"; import fs from "node:fs"; fs.writeFileSync(process.argv[1], JSON.stringify(webSeed()));' "$seed")

( cd "$data" && exec env -i PATH="$PATH" HOME="$data" NEKO_DATA_DIR="$data" NEKO_WEB_PORT="$port" NEKO_WEB_DIR="$web" NEKO_WEB_SEED="$seed" "$harness" >"$data.log" 2>&1 ) &
pid=$!
cleanup() {
  # By pid, and only after confirming it is the harness this script started.
  if ps -p "$pid" -o command= 2>/dev/null | grep -q web_harness; then kill -TERM "$pid" 2>/dev/null || true; fi
  rm -rf "$data" "$web" "$seed" "$data.log" "$data-summary.json"
}
trap cleanup EXIT

for _ in $(seq 1 50); do [[ -S "$data/neko.sock" ]] && break; sleep 0.2; done
[[ -S "$data/neko.sock" ]] || { echo "the harness did not start: see $data.log" >&2; exit 1; }

summary="$data-summary.json"
NEKO_DATA_DIR="$data" NEKO_WEB_PORT="$port" "$bin" probe json > "$summary"
echo "workbench from the harness daemon: $(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(len(d["tasks"]), "tasks")' "$summary")"
for theme in light dark; do
  NEKO_RUNE_SUMMARY="$summary" "$bin" --snap "$out/shell-$theme.png" --scene harness --theme "$theme" --size 1280x800 --scale 2 >/dev/null 2>&1
  echo "wrote $out/shell-$theme.png"
done
