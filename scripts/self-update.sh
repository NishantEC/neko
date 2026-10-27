#!/bin/bash
# Neko reinstalling itself after a landed change (see AGENTS.md, "Neko updates
# itself"). The daemon starts this in its own process group and does not wait:
# the install below stops that daemon, and this script outlives it.
#
#   bash scripts/self-update.sh --checkout <repo> --data-dir <dir> --commit <sha> [--web-port <port>]
#
# 1. refuses unless the checkout is exactly <sha> with nothing uncommitted;
# 2. waits for ~/.agents/bin/machine-ok, then runs scripts/install-native.sh
#    (its checks run first; cargo uses two jobs);
# 3. relaunches /Applications/Neko.app and checks it is healthy: the daemon
#    answers Ping, the snapshot loads, the web UI answers 200, and the app is
#    the new commit;
# 4. unhealthy: restores the newest /Applications/.neko-native-backup.* and
#    relaunches that; healthy: keeps exactly one backup;
# 5. writes <data dir>/self-update/result.json, which the (new) daemon reads.
#
# Safety-critical: changes to this file wait for the owner's click.
set -uo pipefail

checkout='' data_dir='' commit='' web_port=4747
while (( $# )); do
  case "$1" in
    --checkout) checkout="$2"; shift 2 ;;
    --data-dir) data_dir="$2"; shift 2 ;;
    --commit) commit="$2"; shift 2 ;;
    --web-port) web_port="$2"; shift 2 ;;
    *) printf 'Unknown option: %s\n' "$1" >&2; exit 2 ;;
  esac
done
[[ -n "$checkout" && -n "$data_dir" && -n "$commit" ]] || { printf 'Usage: self-update.sh --checkout DIR --data-dir DIR --commit SHA [--web-port PORT]\n' >&2; exit 2; }

application='/Applications/Neko.app'
state_dir="$data_dir/self-update"
result="$state_dir/result.json"
lock="$state_dir/lock"
mkdir -p "$state_dir"
# The daemon runs this without a login shell, so nvm's node and pnpm are not on PATH:
# take the newest installed nvm node (pnpm lives beside it), as ship-web.sh does.
nvm_bin="$(/bin/ls -d "$HOME"/.nvm/versions/node/*/bin 2>/dev/null | /usr/bin/sort -V | /usr/bin/tail -1)"
export PATH="${nvm_bin:+$nvm_bin:}/opt/homebrew/bin:/usr/local/bin:$HOME/.cargo/bin:$HOME/Library/pnpm:/usr/bin:/bin:/usr/sbin:/sbin:${PATH:-}"
export CARGO_BUILD_JOBS=2 TURBO_DAEMON=false

log() { printf '%s %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*"; }
json_text() { printf '%s' "$1" | tr '\n\r\t' '   ' | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }
report() {
  printf '{"commit":"%s","status":"%s","reason":"%s"}\n' "$(json_text "$commit")" "$1" "$(json_text "$2")" > "$result.tmp" && mv "$result.tmp" "$result"
  log "result: $1 $2"
}

mkdir "$lock" 2>/dev/null || { log 'another self-update is running'; exit 1; }
trap 'rmdir "$lock" 2>/dev/null' EXIT

is_installed_executable() {
  case "$1" in
    "$application/Contents/MacOS/neko"|"$application/Contents/MacOS/neko-daemon") return 0 ;;
    *) return 1 ;;
  esac
}
installed_pids() {
  /bin/ps -axo pid=,comm= | while read -r process_id executable; do
    if is_installed_executable "$executable"; then printf '%s\n' "$process_id"; fi
  done
}
stop_installed() {
  local pid attempt
  for pid in $(installed_pids); do /bin/kill -TERM "$pid" 2>/dev/null || true; done
  for ((attempt=0; attempt<50; attempt++)); do [[ -z "$(installed_pids)" ]] && return 0; sleep 0.1; done
  for pid in $(installed_pids); do /bin/kill -KILL "$pid" 2>/dev/null || true; done
}
relaunch() { /usr/bin/open -g "$application" || true; }
installed_commit() { /usr/libexec/PlistBuddy -c 'Print :NekoGitCommit' "$application/Contents/Info.plist" 2>/dev/null || true; }

# The daemon answers Ping, its snapshot loads and the web UI answers 200.
healthy_once() {
  if [[ "$web_port" == 0 ]]; then
    [[ -n "$(installed_pids)" ]]
    return
  fi
  local token base
  token="$(cat "$data_dir/web-token" 2>/dev/null)" || return 1
  base="http://127.0.0.1:$web_port"
  /usr/bin/curl -sf -m 10 -H "Authorization: Bearer $token" -H 'Content-Type: application/json' -d '"Ping"' "$base/api/request" | /usr/bin/grep -q 'Pong' || return 1
  /usr/bin/curl -sf -m 20 -H "Authorization: Bearer $token" -H 'Content-Type: application/json' -d '{"Workbench":"Snapshot"}' "$base/api/request" | /usr/bin/grep -q '"Workbench"' || return 1
  [[ "$(/usr/bin/curl -s -o /dev/null -m 10 -w '%{http_code}' -H "Authorization: Bearer $token" "$base/")" == 200 ]]
}
wait_healthy() {
  local attempt
  for ((attempt=0; attempt<60; attempt++)); do
    healthy_once && return 0
    sleep 2
  done
  return 1
}

restore_backup() {
  local reason="$1" newest
  newest="$(/bin/ls -1dt /Applications/.neko-native-backup.*/previous-Neko.app 2>/dev/null | head -1)"
  if [[ -z "$newest" || ! -d "$newest" ]]; then
    report failed "$reason, and there was no backup to restore"
    relaunch
    exit 1
  fi
  log "restoring $newest"
  stop_installed
  if [[ -e "$application" ]]; then mv "$application" "$(dirname "$newest")/failed-Neko.app" || true; fi
  mv "$newest" "$application" || { report failed "$reason; restoring the backup failed too"; exit 1; }
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$application" || true
  relaunch
  if wait_healthy; then
    report rolled_back "$reason"
  else
    report rolled_back "$reason (the restored app has not answered yet)"
  fi
  exit 1
}

cd "$checkout" || { report failed "the checkout $checkout is missing"; exit 1; }
log "self-update to $commit from $checkout"
[[ "$(git rev-parse HEAD 2>/dev/null)" == "$commit" ]] || { report failed "the checkout is no longer at the commit to install"; exit 1; }
[[ -z "$(git status --porcelain --untracked-files=no 2>/dev/null)" ]] || { report failed "the checkout has uncommitted changes"; exit 1; }
if [[ -x "$HOME/.agents/bin/machine-ok" ]]; then
  "$HOME/.agents/bin/machine-ok" --wait 1800 || { report failed "the Mac stayed too busy to build"; exit 1; }
fi

if ! bash scripts/install-native.sh; then
  # Checks or build failed: nothing was replaced. A failure after the old app
  # was stopped is undone by install-native.sh itself; make sure it runs again.
  [[ -n "$(installed_pids)" ]] || relaunch
  report failed "install-native.sh failed (its checks, the build, or the install; see self-update/install.log)"
  exit 1
fi

relaunch
if ! wait_healthy; then
  restore_backup "the new version did not become healthy within two minutes"
fi
now="$(installed_commit)"
if [[ "$now" != "$commit" ]]; then
  restore_backup "the installed app reports commit ${now:-none}, not $commit"
fi

# Healthy: keep exactly one backup (the app this replaced).
index=0
while IFS= read -r entry; do
  index=$((index + 1))
  (( index > 1 )) || continue
  [[ -d "$entry" && ! -L "$entry" && "$(basename "$entry")" == .neko-native-backup.* ]] || continue
  rm -rf -- "$entry"
done < <(/bin/ls -1dt /Applications/.neko-native-backup.* 2>/dev/null || true)
report updated ''
