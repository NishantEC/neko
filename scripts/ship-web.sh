#!/bin/bash
# Ship a web-only change to the running Neko without reinstalling the app
# (see docs/knowledge/neko-updates-itself.md, "Shipping a web change by hand").
#
#   bash scripts/ship-web.sh             build web/ at HEAD and make it live
#   bash scripts/ship-web.sh --rollback  go back to the previous live build
#   bash scripts/ship-web.sh --force     ship even though code outside web/
#                                        changed since the installed app
#
# Same mechanism as the daemon's own updater: the build goes to
# <data>/web-builds/<short commit>, `web-live` (a symlink the daemon serves
# from, per request) is swapped in one rename and the old target is kept as
# `web-live.prev`. The daemon adopts the build on its next tick (≤10 s),
# records it in Settings → Neko updates and tells open pages to reload; a
# newer app installed later outranks it.
#
# It refuses when anything other than web/ or docs changed since the
# installed app (`NekoGitCommit`): that UI might need a daemon the app does
# not have yet. Run scripts/install-native.sh for those. Nothing here touches
# the safety-critical paths (`self_update::SENSITIVE`); those still wait for
# the owner's "Apply update".
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
data="${NEKO_DATA_DIR:-$HOME/Library/Application Support/neko}"
app="${NEKO_APP:-/Applications/Neko.app}"
port="${NEKO_WEB_PORT:-4747}"
mode=ship force=0
for arg in "$@"; do
  case "$arg" in
    --rollback) mode=rollback ;;
    --force) force=1 ;;
    -h|--help) sed -n '2,21p' "$0"; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$arg" >&2; exit 2 ;;
  esac
done
export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$HOME/Library/pnpm:/opt/homebrew/bin:/usr/local/bin:$PATH"

started=$(date +%s)
live="$data/web-live" prev="$data/web-live.prev" builds="$data/web-builds"
token_file="$data/web-token"

served_script() {
  # The script the running app serves at / right now.
  [[ -f "$token_file" ]] || return 0
  curl -fsS --max-time 5 -H "Cookie: neko_token=$(cat "$token_file")" "http://127.0.0.1:$port/" 2>/dev/null \
    | grep -o 'src="/assets/[^"]*\.js"' | head -1 | sed 's/^src="//; s/"$//' || true
}

swap_link() { # swap_link <link> <target>: atomic (rename over the old link)
  local tmp="$1.tmp-$$"
  rm -f "$tmp"
  ln -s "$2" "$tmp"
  mv -fh "$tmp" "$1"
}

if [[ $mode == rollback ]]; then
  if [[ ! -L "$prev" ]]; then
    printf 'No previous build to go back to. Removing web-live serves the app'"'"'s own UI: rm "%s"\n' "$live" >&2
    exit 1
  fi
  swap_link "$live" "$(readlink "$prev")"
  rm -f "$prev"
  printf 'Rolled back to %s. Served now: %s\n' "$(basename "$(readlink "$live")")" "$(served_script)"
  exit 0
fi

cd "$repo"
head=$(git rev-parse --verify HEAD)
short=${head:0:8}
if [[ -n "$(git status --porcelain --untracked-files=no -- web)" ]]; then
  echo 'web/ has uncommitted changes. Commit them first: the build is labelled with HEAD.' >&2
  exit 1
fi

installed=$(/usr/libexec/PlistBuddy -c 'Print :NekoGitCommit' "$app/Contents/Info.plist" 2>/dev/null || true)
if [[ -z "$installed" ]] || ! git merge-base --is-ancestor "$installed" "$head" 2>/dev/null; then
  if (( ! force )); then
    printf 'The installed app (%s) is not an ancestor of HEAD; install it with scripts/install-native.sh, or pass --force.\n' "${installed:-unknown}" >&2
    exit 1
  fi
else
  other=$(git diff --name-only --no-renames "$installed" "$head" | grep -v '^web/' | grep -v '\.md$' || true)
  if [[ -n "$other" ]] && (( ! force )); then
    printf 'Changed since the installed app (%s), outside web/:\n%s\n' "${installed:0:8}" "$other" | head -20 >&2
    printf 'The UI may need that code. Install with scripts/install-native.sh, or pass --force if it does not.\n' >&2
    exit 1
  fi
fi

if [[ "$(readlink "$live" 2>/dev/null || true)" == "web-builds/$short" ]]; then
  printf 'Already live: %s. Served: %s\n' "$short" "$(served_script)"
  exit 0
fi

[[ -x "$HOME/.agents/bin/machine-ok" ]] && "$HOME/.agents/bin/machine-ok" --wait

cd "$repo/web"
if [[ ! -d node_modules ]] || [[ pnpm-lock.yaml -nt node_modules/.modules.yaml ]]; then
  pnpm install --frozen-lockfile --prefer-offline
fi
stage="$data/web-live.ship"
trap 'rm -rf "$stage"' EXIT
rm -rf "$stage"
pnpm run build --outDir "$stage" --emptyOutDir >"$data/ship-web.log" 2>&1 || {
  tail -20 "$data/ship-web.log" >&2
  echo 'The build failed; nothing was changed.' >&2
  exit 1
}

# Smoke check, as the daemon does: index.html loads a script and every
# /assets/… file it names exists and is not empty.
grep -q 'src="/assets/[^"]*\.js"' "$stage/index.html" || { echo 'index.html loads no script; nothing was changed.' >&2; exit 1; }
for asset in $(grep -o '\(src\|href\)="/assets/[^"]*"' "$stage/index.html" | sed 's/^[a-z]*="\///; s/"$//'); do
  [[ -s "$stage/$asset" ]] || { printf 'index.html names /%s, which the build lacks; nothing was changed.\n' "$asset" >&2; exit 1; }
done

mkdir -p "$builds"
rm -rf "${builds:?}/$short"
mv "$stage" "$builds/$short"
old=$(readlink "$live" 2>/dev/null || true)
swap_link "$live" "web-builds/$short"
if [[ -n "$old" && "$old" != "web-builds/$short" ]]; then
  swap_link "$prev" "$old"
fi
# Keep only what web-live and web-live.prev point at.
keep_live=$(basename "$(readlink "$live")") keep_prev=$(basename "$(readlink "$prev" 2>/dev/null || echo none)")
for build in "$builds"/*; do
  name=$(basename "$build")
  [[ "$name" == "$keep_live" || "$name" == "$keep_prev" ]] || rm -rf "$build"
done

expected=$(grep -o 'src="/assets/[^"]*\.js"' "$builds/$short/index.html" | head -1 | sed 's/^src="//; s/"$//')
served=$(served_script)
printf 'Shipped %s in %ss. Built: %s. Served: %s.\n' "$short" "$(( $(date +%s) - started ))" "$expected" "${served:-the app is not running}"
if [[ -n "$served" && "$served" != "$expected" ]]; then
  echo 'The app serves a different page; is it running from another data folder?' >&2
  exit 1
fi
echo 'Open pages reload within ~10 s (the daemon adopts the build). Undo: bash scripts/ship-web.sh --rollback'
