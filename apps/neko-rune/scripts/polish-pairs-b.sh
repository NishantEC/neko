#!/bin/bash
# Parity batch B's pairs (2026-11-17): the web UI and the Rune app side by side on a COPY of the owner's real data, for Board, Tree,
# Pull requests and Details. Pictures land in /tmp/neko-polish as bb-<screen>-{web,rune}.png and are NEVER committed (the repo is public).
#
#   PLAYWRIGHT_CORE=<dir with node_modules/playwright-core> apps/neko-rune/scripts/polish-pairs-b.sh
#
# The same steps as `polish-pairs.sh`, for the four screens: copy the live database read-only, run the isolated `web_harness` on the
# copy (port 4762), take the Rune half headless (`--snap`, 1440x900 at 2x) and the web half with the shared headless Chrome. The live data
# folder is never written to and nothing is ever sent to it.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
data=/tmp/neko-polish-data
web=/tmp/neko-polish-data-web
out=/tmp/neko-polish
port=4762
harness="${HARNESS:-$HOME/Library/Caches/neko/target/debug/web_harness}"
live="$HOME/Library/Application Support/neko"
theme="${THEME:-dark}"

if command -v "$HOME/.agents/bin/machine-ok" >/dev/null; then
  case "$("$HOME/.agents/bin/machine-ok" 2>&1 | head -n 1)" in WAIT*) echo "machine says wait" >&2; exit 3;; esac
fi
mkdir -p "$data" "$out"
rm -f "$data"/neko.db*
sqlite3 -readonly "$live/neko.db" ".backup $data/neko.db"
rm -rf "$web"
for candidate in "$live/web-live" "$live/source/web/dist" "/Applications/Neko.app/Contents/Resources/web"; do
  if [[ -f "$candidate/index.html" ]]; then cp -RL "$candidate" "$web"; break; fi
done
[[ -f "$web/index.html" ]] || { echo "no web build found" >&2; exit 1; }

# A task with a pull request and the most events: the Details pair (its Info has the pull request, its log is long).
pick="from workbench_tasks where json_extract(body,'\$.pull_request.url') is not null order by json_array_length(json_extract(body,'\$.events')) desc limit 1"
detail_id="$(sqlite3 -readonly "$data/neko.db" "select id $pick")"
detail_title="$(sqlite3 -readonly "$data/neko.db" "select replace(coalesce(json_extract(body,'\$.display_title'), json_extract(body,'\$.title')), char(10), ' ') $pick")"
echo "details pair: $detail_id"

( cd "$data" && exec env -i PATH="$PATH" HOME="$data" NEKO_DATA_DIR="$data" NEKO_WEB_PORT="$port" NEKO_WEB_DIR="$web" "$harness" >"$data.log" 2>&1 ) &
pid=$!
trap 'ps -p "$pid" -o command= 2>/dev/null | grep -q web_harness && kill -TERM "$pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do [[ -S "$data/neko.sock" ]] && break; sleep 0.2; done
[[ -S "$data/neko.sock" ]] || { echo "the harness did not start: see $data.log" >&2; exit 1; }

(cd "$here" && cargo build --quiet)
bin="$CARGO_TARGET_DIR/debug/neko-rune"
export NEKO_RUNE_SOCKET="$data/neko.sock" NEKO_DATA_DIR="$data" NEKO_RUNE_ACCOUNT=1
snap() { # name [ENV=value ...]
  local name="$1"; shift
  env "$@" "$bin" --snap "$out/$name-rune.png" --scene default --theme "$theme" --size 1440x900 --scale 2 >/dev/null 2>&1
  echo "rune $name"
}
snap bb-board NEKO_RUNE_PREVIEW=board
snap bb-tree NEKO_RUNE_PREVIEW=tree
snap bb-prs NEKO_RUNE_PREVIEW=prs
snap bb-details NEKO_RUNE_OPEN_TASK="$detail_title" NEKO_RUNE_TAB=details

"$HOME/.agents/bin/shared-chrome" acquire neko-polish-b >/dev/null
token="$(cat "$data/web-token")"
SHOTS="[[\"bb-board\",\"/tasks\",\"\",{\"neko.work.layout\":\"board\"}],[\"bb-tree\",\"/tasks\",\"\",{\"neko.work.layout\":\"tree\"}],[\"bb-prs\",\"/prs\"],[\"bb-details\",\"/tasks?task=${detail_id}&view=full\"]]" \
  PORT="$port" TOKEN="$token" OUT="$out" THEME="$theme" PLAYWRIGHT_CORE="${PLAYWRIGHT_CORE:?set PLAYWRIGHT_CORE}" node "$here/scripts/polish-web.mjs"
"$HOME/.agents/bin/shared-chrome" release neko-polish-b >/dev/null
echo "pairs are in $out (not committed)"
