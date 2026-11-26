#!/bin/bash
# Runs one QA scenario against the real app and a real daemon, records it and writes a verdict (docs/knowledge/qa.md).
#
#   scripts/qa/run.sh SCENARIO [--cli fake|real] [--window] [--no-build]
#
# SCENARIO is a name from apps/neko/qa/ (first-task) or a path to a scenario file. Results go to
# $NEKO_QA_OUT/<UTC time>-<scenario>[-window][-real]/ (default /tmp/neko-qa), never inside the repository.
# --cli real runs the installed `claude` with your own login: real model calls (the cheapest model, NEKO_QA_MODEL, default haiku).
# Exit code: 0 passed, 1 a check failed, 2 the rig could not run it.
set -uo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
scenario="" cli=fake window=0 build=1
for a in "$@"; do
  case "$a" in
    --cli=*) cli="${a#--cli=}" ;;
    --cli) cli="__next" ;;
    --window) window=1 ;;
    --no-build) build=0 ;;
    --help|-h) sed -n '2,10p' "$0"; exit 0 ;;
    *) if [[ "$cli" == "__next" ]]; then cli="$a"; elif [[ -z "$scenario" ]]; then scenario="$a"; else printf 'Unknown argument: %s\n' "$a" >&2; exit 2; fi ;;
  esac
done
[[ "$cli" == fake || "$cli" == real ]] || { printf -- '--cli is fake or real\n' >&2; exit 2; }
[[ -n "$scenario" ]] || { sed -n '2,10p' "$0"; exit 2; }
file="$scenario"
[[ -f "$file" ]] || file="$root/apps/neko/qa/$scenario.json"
[[ -f "$file" ]] || { printf 'No scenario %s (looked in apps/neko/qa/)\n' "$scenario" >&2; exit 2; }
name="$(basename "$file" .json)"

# Results never go in the repository.
out_root="${NEKO_QA_OUT:-/tmp/neko-qa}"
mkdir -p "$out_root" || exit 2
out_real="$(cd "$out_root" && pwd -P)"
root_real="$(cd "$root" && pwd -P)"
case "$out_real/" in
  "$root_real/"*) printf 'Refusing to write results inside the repository (%s). Set NEKO_QA_OUT elsewhere.\n' "$out_real" >&2; exit 2 ;;
esac

# Keep the machine cool: wait (up to ten minutes) until the shared check says OK.
if [[ -x "$HOME/.agents/bin/machine-ok" ]]; then
  waited=0
  until verdict="$("$HOME/.agents/bin/machine-ok" 2>/dev/null | head -n 1)"; [[ "$verdict" == OK* ]]; do
    if (( waited >= 600 )); then printf 'The machine is still busy (%s); running anyway.\n' "$verdict"; break; fi
    (( waited == 0 )) && printf 'Waiting for the machine to cool (%s)…\n' "$verdict"
    sleep 30; waited=$((waited + 30))
  done
fi

target="${NEKO_TARGET:-$HOME/Library/Caches/neko/target}"
if (( build )); then
  CARGO_TARGET_DIR="$target/core" cargo build --quiet --manifest-path "$root/Cargo.toml" -p neko-daemon -p neko-core --bins || exit 2
  (cd "$root/apps/neko" && CARGO_TARGET_DIR="$target/app" cargo build --quiet --bin neko --example neko-qa) || exit 2
fi
driver="$target/app/debug/examples/neko-qa"
daemon="$target/core/debug/neko-daemon"
fake="$target/core/debug/neko-fake-claude"
app="$target/app/debug/neko"
for b in "$driver" "$daemon" "$fake" "$app"; do [[ -x "$b" ]] || { printf 'Missing %s (build first)\n' "$b" >&2; exit 2; }; done

commit="$(git -C "$root" rev-parse --short HEAD 2>/dev/null || echo unknown)"
[[ -n "$(git -C "$root" status --porcelain 2>/dev/null)" ]] && commit="$commit-dirty"
suffix=""
(( window )) && suffix="$suffix-window"
[[ "$cli" == real ]] && suffix="$suffix-real"
out="$out_real/$(date -u +%Y%m%dT%H%M%SZ)-$name$suffix"
mkdir -p "$out" || exit 2
work="$(mktemp -d /tmp/neko-qa-work.XXXXXX)" || exit 2

args=(--scenario "$file" --out "$out" --work "$work" --daemon "$daemon" --fake "$fake" --cli "$cli" --commit "$commit")
(( window )) && args+=(--window --app "$app")
NEKO_QA_SCRIPTS="$root/scripts/qa" "$driver" "${args[@]}"
code=$?

# Nothing this run started may be left: every PID it recorded, and anything still running from its folder.
leaks=()
if [[ -f "$out/.pids" ]]; then
  while read -r pid what; do
    [[ -n "$pid" ]] || continue
    if kill -0 "$pid" 2>/dev/null; then
      leaks+=("$pid $what")
      kill -KILL "$pid" 2>/dev/null
    fi
  done < "$out/.pids"
fi
while read -r pid cmd; do
  [[ -n "$pid" ]] || continue
  leaks+=("$pid ${cmd%% *}")
  kill -KILL "$pid" 2>/dev/null
done < <(ps -axo pid=,command= | awk -v w="$work" 'index($0, w) && !/awk/ {print $1, $2}')
rm -f "$out/.pids"
rm -rf "$work"
[[ -e "$work" ]] && leaks+=("folder $work")
if (( ${#leaks[@]} )); then
  printf 'LEFT BEHIND (now stopped): %s\n' "${leaks[*]}"
  python3 - "$out/result.json" "${leaks[@]}" <<'PY'
import json, sys
path, leaks = sys.argv[1], sys.argv[2:]
try:
    r = json.load(open(path))
except Exception:
    sys.exit(0)
r["passed"] = False
r.setdefault("notes", []).append("Left behind after the run (stopped by run.sh): " + ", ".join(leaks))
json.dump(r, open(path, "w"), indent=2)
PY
  (( code == 0 )) && code=1
fi
printf 'Result: %s\n' "$out"
exit $code
