#!/bin/bash
# Visual identity of the Rune client against the web UI, on a COPY of the owner's real data (docs/knowledge/rune-identity.md).
# Run by hand; pictures land in /tmp/neko-rune-identity and are NEVER committed (the repo is public).
#
#   apps/neko-rune/scripts/identity.sh            every pair
#   ONLY=setup-workspaces,usage apps/neko-rune/scripts/identity.sh
#   STRUCTURE=1 apps/neko-rune/scripts/identity.sh   also run tests/structure.rs against the copy
#
# 1. `identity-harness.sh start`: `sqlite3 .backup` of the live database, the web UI of this checkout built into /tmp, the isolated
#    `web_harness` on the copy (it refuses any folder outside /tmp). The live data folder is never written.
# 2. Builds Rune (CARGO_TARGET_DIR under ~/Library/Caches, no incremental), after the disk and machine checks.
# 3. `identity.mjs`: Rune's content region headless, the web page in embed mode in the SHARED headless Chrome at the same size and
#    scale, pixelmatch (threshold 0.1, anti-aliasing ignored), REPORT.md with every pair, its percentage and its side-by-side picture.
# Pass threshold: 2.5% differing pixels per pair.
#
# Needs a scratch folder with `pngjs`, `pixelmatch` and `playwright-core` (pnpm add ... in /tmp/neko-rune-identity/tools).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
out=/tmp/neko-rune-identity
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}" CARGO_INCREMENTAL=0

free_gb="$(df -g / | awk 'NR==2 {print $4}')"
if (( free_gb < 20 )); then echo "only ${free_gb} GB free: stopping (the rule is 20 GB)" >&2; exit 4; fi
if [[ -x "$HOME/.agents/bin/machine-ok" ]]; then
  while case "$("$HOME/.agents/bin/machine-ok" 2>&1 | head -n 1)" in WAIT*) true;; *) false;; esac; do echo "machine says wait"; sleep 30; done
fi
if [[ ! -d $out/tools/node_modules/pixelmatch ]]; then
  mkdir -p $out/tools && (cd $out/tools && { [[ -f package.json ]] || pnpm init >/dev/null; } && pnpm add pixelmatch pngjs playwright-core >/dev/null)
fi

data="$("$here/scripts/identity-harness.sh" start)"
trap '"$here/scripts/identity-harness.sh" stop; "$HOME/.agents/bin/shared-chrome" release neko-rune-identity >/dev/null || true' EXIT
(cd "$here" && cargo build --quiet)
"$HOME/.agents/bin/shared-chrome" acquire neko-rune-identity >/dev/null
if [[ "${STRUCTURE:-0}" == 1 ]]; then
  (cd "$here" && NEKO_STRUCTURE_SOCKET="$data/neko.sock" cargo test --test structure --quiet)
fi
DATA="$data" OUT="$out" node "$here/scripts/identity.mjs"
