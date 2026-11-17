#!/bin/bash
# Side-by-side pairs of the web UI and the Rune app on a COPY of the owner's real data, for the polish pass
# (docs/design/rune/polish-spec.md §0). Pictures land in /tmp/neko-polish and are NEVER committed: the repo is public.
#
#   apps/neko-rune/scripts/polish-pairs.sh
#
# 1. Copies the live database (read-only, `sqlite3 .backup`) to /tmp/neko-polish-data and the served web build to
#    /tmp/neko-polish-data-web. The live data folder is never written to and nothing is ever sent to it.
# 2. Runs the isolated `web_harness` daemon on the copy (it refuses any data folder outside /tmp), on port 4761.
# 3. Takes the Rune half headless (`--snap`, 1440x900 at 2x, no window, no focus) against that daemon.
# 4. Takes the web half with the shared headless Chrome (`~/.agents/bin/shared-chrome`, never a Chrome of its own).
# 5. Stops the harness it started, by pid.
#
# Needs: sqlite3, node with `playwright-core` (PLAYWRIGHT_CORE=<dir containing node_modules/playwright-core>),
# the daemon's `web_harness` binary (HARNESS=..., else ~/Library/Caches/neko/target/debug/web_harness).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
data=/tmp/neko-polish-data
web=/tmp/neko-polish-data-web
out=/tmp/neko-polish
port=4761
harness="${HARNESS:-$HOME/Library/Caches/neko/target/debug/web_harness}"
live="$HOME/Library/Application Support/neko"
theme="${THEME:-dark}"

if command -v "$HOME/.agents/bin/machine-ok" >/dev/null; then
  case "$("$HOME/.agents/bin/machine-ok" 2>&1 | head -n 1)" in WAIT*) echo "machine says wait" >&2; exit 3;; esac
fi
mkdir -p "$data" "$out"
rm -f "$data"/neko.db*
sqlite3 -readonly "$live/neko.db" ".backup $data/neko.db"
# The web build the installed app serves: its swapped-in `web-live`, else the build of its own clone of the published
# repository, else the one inside the installed app. A copy, so nothing of the live folder is ever written.
rm -rf "$web"
for candidate in "$live/web-live" "$live/source/web/dist" "/Applications/Neko.app/Contents/Resources/web"; do
  if [[ -f "$candidate/index.html" ]]; then cp -RL "$candidate" "$web"; break; fi
done
[[ -f "$web/index.html" ]] || { echo "no web build found: the web half of the pairs needs one (web-live or source/web/dist)" >&2; exit 1; }
web_env=(); [[ -d "$web" ]] && web_env=(NEKO_WEB_DIR="$web")

( cd "$data" && exec env -i PATH="$PATH" HOME="$data" NEKO_DATA_DIR="$data" NEKO_WEB_PORT="$port" ${web_env[@]+"${web_env[@]}"} "$harness" >"$data.log" 2>&1 ) &
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
snap home
snap session-plan NEKO_RUNE_OPEN_TASK="${PLAN_TITLE:-Optimize flow screens for mobile views}"
panel_title="${PANEL_TITLE-}"
[[ -n "$panel_title" ]] || panel_title="Send Ria's cards as images and read member files on WhatsApp"
snap session-panel NEKO_RUNE_OPEN_TASK="$panel_title" NEKO_RUNE_TAB=terminal
snap neko-column
snap routines NEKO_RUNE_VIEW=inbox
snap settings NEKO_RUNE_VIEW=settings
snap settings-permissions NEKO_RUNE_VIEW=settings NEKO_RUNE_SECTION=permissions
# Rune-only extras, for what the pairs' tickets do not show: the Plan and Info tabs, the header menu, the other new sections.
snap session-plan-tab NEKO_RUNE_OPEN_TASK="${PLAN_TAB_TITLE:-Let web checkout choose its payment gateway}" NEKO_RUNE_TAB=plan
snap session-info-pr NEKO_RUNE_OPEN_TASK="${INFO_TITLE:-Triage interrupted message stream errors in unified messenger on Firefox}" NEKO_RUNE_TAB=info
snap session-menu NEKO_RUNE_OPEN_TASK="${PLAN_TITLE:-Optimize flow screens for mobile views}" NEKO_RUNE_MENU=1
for section in appearance diagnostics rulebook; do snap "settings-$section" NEKO_RUNE_VIEW=settings NEKO_RUNE_SECTION="$section"; done
# Parity batch A (2026-11-16): the new screens on the same data. The ones with a web page are paired below (All sessions, Calendar,
# Inbox); the rest are Rune-only because the web reaches them by a click or a key, not an address.
snap ba-all NEKO_RUNE_PREVIEW=all
snap ba-month NEKO_RUNE_PREVIEW=month
snap ba-closed NEKO_RUNE_PREVIEW=closed
snap ba-palette NEKO_RUNE_PREVIEW=palette
snap ba-shortcuts NEKO_RUNE_PREVIEW=shortcuts
snap ba-model NEKO_RUNE_OPEN_TASK="${PLAN_TITLE:-Optimize flow screens for mobile views}" NEKO_RUNE_PREVIEW=model
snap ba-slash NEKO_RUNE_PREVIEW=slash
snap ba-scope NEKO_RUNE_PREVIEW=scope
snap ba-cadence NEKO_RUNE_PREVIEW=cadence
snap ba-menu NEKO_RUNE_OPEN_TASK="${PLAN_TITLE:-Optimize flow screens for mobile views}" NEKO_RUNE_MENU=1
# Parity batch C (2026-11-17): the Setup sections, each paired with the web page of the same name (Skills is a tab of the web's Tools page).
for section in workspaces watching tools skills memory profiles schedules; do snap "bc-$section" NEKO_RUNE_VIEW=setup NEKO_RUNE_SETUP="$section"; done

# The web half: the same screens at the same size and scale (routes need the task ids, in PLAN_ID and PANEL_ID).
"$HOME/.agents/bin/shared-chrome" acquire neko-polish >/dev/null
token="$(cat "$data/web-token")"
SHOTS="[[\"home\",\"/\"],[\"session-plan\",\"/tasks?task=${PLAN_ID:-}&view=full\"],[\"session-panel\",\"/tasks?task=${PANEL_ID:-}&view=full\"],[\"neko-column\",\"/\"],[\"routines\",\"/inbox\",\"Quiet\"],[\"settings\",\"/settings\"],[\"settings-permissions\",\"/settings?section=permissions\"],[\"ba-all\",\"/tasks\"],[\"ba-month\",\"/calendar\"],[\"ba-closed\",\"/inbox\"],[\"bc-workspaces\",\"/workspaces\"],[\"bc-watching\",\"/watching\"],[\"bc-tools\",\"/tools\"],[\"bc-skills\",\"/tools\"],[\"bc-memory\",\"/memory\"],[\"bc-profiles\",\"/profiles\"],[\"bc-schedules\",\"/schedules\"]]" \
  PORT="$port" TOKEN="$token" OUT="$out" THEME="$theme" PLAYWRIGHT_CORE="${PLAYWRIGHT_CORE:?set PLAYWRIGHT_CORE}" node "$here/scripts/polish-web.mjs"
"$HOME/.agents/bin/shared-chrome" release neko-polish >/dev/null
echo "pairs are in $out (not committed)"
