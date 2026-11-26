#!/bin/bash
# Runs every QA scenario for the current commit with the fake CLI (headless), then writes one summary.md (docs/knowledge/qa.md).
#
#   scripts/qa/all.sh [--window] [--real]
#
#   --window  also runs the scenarios that have a window form, in a real window
#   --real    also runs the scenarios for the real CLI (real model calls, the cheapest model)
#
# The summary goes to $NEKO_QA_OUT/<UTC time>-all/summary.md, with a link to each run's folder. Exit code non-zero on any failure.
set -uo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
window=0 real=0
for a in "$@"; do
  case "$a" in
    --window) window=1 ;;
    --real) real=1 ;;
    --help|-h) sed -n '2,9p' "$0"; exit 0 ;;
    *) printf 'Unknown argument: %s\n' "$a" >&2; exit 2 ;;
  esac
done
out_root="${NEKO_QA_OUT:-/tmp/neko-qa}"
mkdir -p "$out_root" || exit 2
out_root="$(cd "$out_root" && pwd -P)"
export NEKO_QA_OUT="$out_root"
summary_dir="$out_root/$(date -u +%Y%m%dT%H%M%SZ)-all"
mkdir -p "$summary_dir" || exit 2
list="$summary_dir/runs.txt"
: > "$list"

# One build for all of them.
target="${NEKO_TARGET:-$HOME/Library/Caches/neko/target}"
CARGO_TARGET_DIR="$target/core" cargo build --quiet --manifest-path "$root/Cargo.toml" -p neko-daemon -p neko-core --bins || exit 2
(cd "$root/apps/neko" && CARGO_TARGET_DIR="$target/app" cargo build --quiet --bin neko --example neko-qa) || exit 2

one() {
  local log
  log="$(mktemp /tmp/neko-qa-all.XXXXXX)"
  "$root/scripts/qa/run.sh" "$@" --no-build | tee "$log"
  sed -n 's/^Result: //p' "$log" >> "$list"
  rm -f "$log"
}

for f in "$root"/apps/neko/qa/*.json; do
  clis="$(python3 -c 'import json,sys; print(" ".join(json.load(open(sys.argv[1])).get("cli", ["fake"])))' "$f")"
  has_window="$(python3 -c 'import json,sys; print(1 if json.load(open(sys.argv[1])).get("window") else 0)' "$f")"
  if [[ " $clis " == *" fake "* ]]; then
    one "$f"
    (( window && has_window )) && one "$f" --window
  fi
  if (( real )) && [[ " $clis " == *" real "* ]]; then
    one "$f" --cli real
  fi
done

python3 - "$list" "$summary_dir/summary.md" <<'PY'
import json, os, sys
runs = [l.strip() for l in open(sys.argv[1]) if l.strip()]
rows, passed, failed, checks_ok, checks_all, cost = [], 0, 0, 0, 0, 0.0
commit = "unknown"
for d in runs:
    try:
        r = json.load(open(os.path.join(d, "result.json")))
    except Exception:
        rows.append(f"| {os.path.basename(d)} | ? | ? | **rig error** (no result.json) | | {d} |")
        failed += 1
        continue
    commit = r.get("commit", commit)
    ex = r.get("expectations", [])
    ok = sum(1 for e in ex if e["result"] == "pass")
    checks_ok += ok
    checks_all += len(ex)
    first_fail = next((e for e in ex if e["result"] == "fail"), None)
    why = ""
    if first_fail:
        why = f"step {first_fail['step']}: {first_fail['expect']} ({first_fail.get('picture') or ''})"
    elif r.get("notes"):
        why = "; ".join(r["notes"])
    u = r.get("usage") or {}
    if u.get("cost_usd"):
        cost += u["cost_usd"]
    if r["passed"]:
        passed += 1
    else:
        failed += 1
    rows.append(f"| {r['scenario']} | {r['mode']} | {r['cli']} | {'pass' if r['passed'] else '**FAIL**'} {ok}/{len(ex)} | {why.replace('|', '/')} | {d} |")
md = [f"# Neko QA, commit `{commit}`", "", f"{passed} passed, {failed} failed; {checks_ok} of {checks_all} expectations held."]
if cost:
    md.append(f"Real runs reported ${cost:.4f} in total (as Claude Code reports it).")
md += ["", "| Scenario | Mode | CLI | Result | First failure | Folder |", "| --- | --- | --- | --- | --- | --- |"] + rows + [""]
open(sys.argv[2], "w").write("\n".join(md))
print("\n".join(md))
sys.exit(1 if failed else 0)
PY
code=$?
printf 'Summary: %s\n' "$summary_dir/summary.md"
exit $code
