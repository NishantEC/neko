#!/bin/bash
# Parity batch D's pairs (2026-11-17): the web UI and the Rune app side by side on a COPY of the owner's real data, for Settings (the rest),
# Usage, the first-run setup and Plugins. Pictures land in /tmp/neko-polish as bd-<screen>-{web,rune}.png and are NEVER committed (the repo is public).
#
#   PLAYWRIGHT_CORE=<dir with node_modules/playwright-core> apps/neko-rune/scripts/polish-pairs-d.sh
#
# The same steps as `polish-pairs-b.sh`: copy the live database read-only, run the isolated `web_harness` on the copy (port 4763), take the
# Rune half headless (`--snap`, 1440x900 at 2x) and the web half with the shared headless Chrome. The live data folder is never written to.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
data=/tmp/neko-polish-data
web=/tmp/neko-polish-data-web
out=/tmp/neko-polish
port=4763
harness="${HARNESS:-$CARGO_TARGET_DIR/debug/web_harness}"
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

( cd "$data" && exec env -i PATH="$PATH" HOME="$data" NEKO_DATA_DIR="$data" NEKO_WEB_PORT="$port" NEKO_WEB_DIR="$web" "$harness" >"$data.log" 2>&1 ) &
pid=$!
trap 'ps -p "$pid" -o command= 2>/dev/null | grep -q web_harness && kill -TERM "$pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do [[ -S "$data/neko.sock" ]] && break; sleep 0.2; done
[[ -S "$data/neko.sock" ]] || { echo "the harness did not start: see $data.log" >&2; exit 1; }

(cd "$here" && cargo build --quiet)
bin="$CARGO_TARGET_DIR/debug/neko-rune"
export NEKO_RUNE_SOCKET="$data/neko.sock" NEKO_DATA_DIR="$data" NEKO_WEB_PORT="$port" NEKO_RUNE_ACCOUNT=1
snap() { # name [ENV=value ...]
  local name="$1"; shift
  env "$@" "$bin" --snap "$out/$name-rune.png" --scene default --theme "$theme" --size 1440x900 --scale 2 >/dev/null 2>&1
  echo "rune $name"
}
snap bd-appearance NEKO_RUNE_VIEW=settings NEKO_RUNE_SECTION=appearance
snap bd-general NEKO_RUNE_VIEW=settings NEKO_RUNE_SECTION=general
snap bd-ai NEKO_RUNE_VIEW=settings NEKO_RUNE_SECTION=ai
snap bd-rulebook NEKO_RUNE_VIEW=settings NEKO_RUNE_SECTION=rulebook
snap bd-quick-panel NEKO_RUNE_VIEW=settings NEKO_RUNE_SECTION=quick-panel
snap bd-usage NEKO_RUNE_VIEW=usage
snap bd-plugins NEKO_RUNE_VIEW=tools
snap bd-onboarding NEKO_RUNE_PREVIEW=onboarding NEKO_RUNE_STEP=agents

"$HOME/.agents/bin/shared-chrome" acquire neko-polish-d >/dev/null
token="$(cat "$data/web-token")"
SHOTS='[["bd-appearance","/settings?section=appearance"],["bd-general","/settings?section=general"],["bd-ai","/settings?section=ai"],["bd-rulebook","/settings?section=rulebook"],["bd-quick-panel","/settings?section=search"],["bd-usage","/usage"],["bd-plugins","/customisations"],["bd-onboarding","/setup"]]' \
  PORT="$port" TOKEN="$token" OUT="$out" THEME="$theme" PLAYWRIGHT_CORE="${PLAYWRIGHT_CORE:?set PLAYWRIGHT_CORE}" node "$here/scripts/polish-web.mjs"
"$HOME/.agents/bin/shared-chrome" release neko-polish-d >/dev/null
echo "pairs are in $out (not committed)"
