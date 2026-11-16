#!/bin/bash
# Try a new Rune revision: pin it, build, run the headless tests, and say what broke.
#
#   apps/neko-rune/scripts/bump-rune.sh               try the newest `dev`
#   apps/neko-rune/scripts/bump-rune.sh <rev>         try that revision
#   apps/neko-rune/scripts/bump-rune.sh --keep [rev]  leave the new pin in place even if it fails
#   apps/neko-rune/scripts/bump-rune.sh --verify      only check that RUNE_REV and Cargo.toml agree
#
# Rune is pre-release: `dev` takes hundreds of commits a week, the build contract (its patch
# table, its feature names) has changed within one, and it takes no pull requests. So the rev is
# pinned, and bumped at most about weekly, on a green run of this script. What it does:
#   1. rewrites every `rev =` in Cargo.toml and the first line of RUNE_REV;
#   2. rewrites `[patch.crates-io]` from the table in Rune's own root Cargo.toml at that rev
#      (Cargo does not inherit a dependency's patches), and says what changed in it;
#   3. checks that the features this app uses (`tooling`, `markdown`) still exist;
#   4. builds, then runs scripts/check.sh's tests;
#   5. green: keeps the pin and notes the date in RUNE_REV. Red: puts the old pin back, prints
#      the first errors and the commits in between, and exits 1 (`--keep` skips the restore).
# Needs network (raw.githubusercontent.com and `git ls-remote`), and never runs two builds at once.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
repo="https://github.com/anomalyco/rune"
raw="https://raw.githubusercontent.com/anomalyco/rune"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"

pinned() { head -n 1 RUNE_REV | tr -d '[:space:]'; }

# --verify: the pin file, the dependency lines and the patch lines all name one rev.
if [[ "${1:-}" == "--verify" ]]; then
  want="$(pinned)"
  revs="$(grep -oE 'rev = "[0-9a-f]{40}"' Cargo.toml | sort -u)"
  if [[ "$(printf '%s\n' "$revs" | wc -l | tr -d ' ')" != 1 || "$revs" != "rev = \"$want\"" ]]; then
    echo "RUNE_REV ($want) and Cargo.toml disagree:" >&2; printf '%s\n' "$revs" >&2; exit 1
  fi
  echo "Rune pinned at $want"; exit 0
fi

keep=0
if [[ "${1:-}" == "--keep" ]]; then keep=1; shift; fi
old="$(pinned)"
new="${1:-}"
if [[ -z "$new" ]]; then
  new="$(git ls-remote "$repo" refs/heads/dev | cut -f1)"
  [[ -n "$new" ]] || { echo "could not read Rune's dev branch (offline?)" >&2; exit 1; }
fi
if [[ ! "$new" =~ ^[0-9a-f]{40}$ ]]; then echo "give a full 40-digit revision, not '$new'" >&2; exit 2; fi
if [[ "$new" == "$old" ]]; then echo "already at $old"; exit 0; fi

# This Mac runs many agents: do not start a heavy build on a hot machine.
if command -v "$HOME/.agents/bin/machine-ok" >/dev/null; then
  verdict="$("$HOME/.agents/bin/machine-ok" 2>&1 | head -n 1)"
  case "$verdict" in WAIT*|WARM*) echo "machine says: $verdict. Try again later." >&2; exit 3;; esac
fi

work="$(mktemp -d)"
cp Cargo.toml "$work/Cargo.toml"; cp RUNE_REV "$work/RUNE_REV"; [[ -f Cargo.lock ]] && cp Cargo.lock "$work/Cargo.lock"
restore() {
  cp "$work/Cargo.toml" Cargo.toml; cp "$work/RUNE_REV" RUNE_REV
  [[ -f "$work/Cargo.lock" ]] && cp "$work/Cargo.lock" Cargo.lock
}
fail() {
  echo; echo "BUMP FAILED at step: $1"
  if (( keep )); then echo "(--keep: the new pin stays; the old one is $old)"; else restore; echo "Restored $old."; fi
  echo "Commits in between: $repo/compare/${old:0:12}...${new:0:12}"
  rm -rf "$work"; exit 1
}

echo "Rune $old -> $new"

# 1 + 2. The pin and the patch table, from Rune's root Cargo.toml at the new rev.
upstream="$work/rune-root.toml"
curl -fsSL "$raw/$new/Cargo.toml" -o "$upstream" || fail "fetching Rune's Cargo.toml at $new"
python3 -I - "$upstream" Cargo.toml "$old" "$new" "$repo" <<'PY' || fail "rewriting the patch table"
import re, sys
upstream, manifest, old, new, repo = sys.argv[1:6]
up = open(upstream).read()
m = re.search(r'^\[patch\.crates-io\]\n(.*?)(?=^\[|\Z)', up, re.S | re.M)
if not m:
    sys.exit("Rune's Cargo.toml has no [patch.crates-io] table any more: read its README, 'Depending on Rune'")
names = re.findall(r'^([A-Za-z0-9_-]+)\s*=\s*\{[^}]*path\s*=', m.group(1), re.M)
text = open(manifest).read()
cur = re.search(r'^\[patch\.crates-io\]\n(.*?)(?=^\[|\Z)', text, re.S | re.M)
before = re.findall(r'^([A-Za-z0-9_-]+)\s*=', cur.group(1), re.M) if cur else []
table = "".join(f'{n} = {{ git = "{repo}", rev = "{new}" }}\n' for n in names)
head = text[:cur.start()] if cur else text.rstrip() + "\n\n"
tail = text[cur.end():] if cur else ""
text = head + "[patch.crates-io]\n" + table + tail
text = text.replace(old, new)
open(manifest, "w").write(text)
added, removed = sorted(set(names) - set(before)), sorted(set(before) - set(names))
print("patch table:", ", ".join(names))
if added: print("  NEW in Rune's table:", ", ".join(added))
if removed: print("  GONE from Rune's table:", ", ".join(removed))
PY
{ echo "$new"; tail -n +2 RUNE_REV; echo "# $(date +%Y-%m-%d): bumped from ${old:0:12} by scripts/bump-rune.sh"; } > RUNE_REV.new && mv RUNE_REV.new RUNE_REV

# 3. The features this app turns on.
for pair in "crates/rune:tooling" "crates/rune-kit:markdown"; do
  crate="${pair%%:*}"; feat="${pair##*:}"
  curl -fsSL "$raw/$new/$crate/Cargo.toml" -o "$work/crate.toml" || fail "fetching $crate/Cargo.toml"
  grep -Eq "^$feat[[:space:]]*=" "$work/crate.toml" || { echo "feature '$feat' is gone from $crate"; fail "feature $feat"; }
done

# 4. Build, then the tests (unit, shell, read-only, headless snapshots).
rm -f Cargo.lock.new
cargo update -p rune -p rune-kit --quiet 2>"$work/update.log" || { cat "$work/update.log" | tail -20; fail "cargo update"; }
cargo build --all-targets --quiet 2>"$work/build.log" || { grep -E "^(error|warning: unused)" -A6 "$work/build.log" | head -40; fail "build"; }
cargo test --quiet 2>"$work/test.log" >"$work/test.out" || { tail -30 "$work/test.out"; grep -E "^error" -A6 "$work/test.log" | head -20; fail "tests"; }

rm -rf "$work"
echo
echo "BUMP OK: Rune is now ${new:0:12}. Commit apps/neko-rune/{Cargo.toml,Cargo.lock,RUNE_REV}."
