#!/bin/bash
# Install the SwiftUI/AppKit client. Default: preserve all Neko user state.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
application='/Applications/Neko.app'
data_directory="$HOME/Library/Application Support/neko"
bundle="$repo_root/target/native/Neko.app"
dry_run=0
clean_data=0
skip_build=0
for option in "$@"; do
  case "$option" in
    --dry-run) dry_run=1 ;;
    --clean-data) clean_data=1 ;;
    --skip-build) skip_build=1 ;;
    --help) printf '%s\n' 'Usage: bash scripts/install-native.sh [--dry-run] [--skip-build] [--clean-data]' 'Default preserves data and credentials. --clean-data moves only Neko data to a recoverable backup.'; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$option" >&2; exit 2 ;;
  esac
done
[[ ! -L "$application" && ! -L "$data_directory" ]] || { printf 'Refusing symlinked installation or data targets.\n' >&2; exit 2; }
is_installed_executable() {
  case "$1" in
    '/Applications/Neko.app/Contents/MacOS/neko'|'/Applications/Neko.app/Contents/MacOS/neko-daemon'|/Applications/.neko-native-backup.*/previous-Neko.app/Contents/MacOS/neko|/Applications/.neko-native-backup.*/previous-Neko.app/Contents/MacOS/neko-daemon) return 0 ;;
    *) return 1 ;;
  esac
}
installed_pids() {
  /bin/ps -axo pid=,comm= | while read -r process_id executable; do
    if is_installed_executable "$executable"; then printf '%s\n' "$process_id"; fi
  done
}
validate_bundle() {
  local candidate="$1"
  [[ -x "$candidate/Contents/MacOS/neko" && -x "$candidate/Contents/MacOS/neko-daemon" ]]
  [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$candidate/Contents/Info.plist")" == dev.neko.launcher ]]
  /usr/bin/codesign --verify --deep --strict "$candidate"
  # Reject a validly signed legacy GPUI bundle accidentally used as input.
  /usr/bin/otool -L "$candidate/Contents/MacOS/neko" | /usr/bin/grep -q '/SwiftUI.framework/'
}
if (( dry_run )); then
  printf 'Dry run: no build, process signals, file moves, or installation.\n'
  printf 'Source: %s\nTarget: %s\n' "$bundle" "$application"
  if (( skip_build )); then validate_bundle "$bundle"; printf 'Existing signed native source bundle verified.\n'
  else printf 'Would run scripts/build-native.sh and verify the resulting signed SwiftUI bundle.\n'; fi
  printf 'Would stop only installed Neko executables, including installer backups; matching PIDs:\n'
  installed_pids
  printf 'Would back up the existing app in a unique /Applications/.neko-native-backup.* directory.\n'
  printf 'Would unregister the build output and installer backups, then register /Applications/Neko.app with Launch Services.\n'
  if (( clean_data )); then printf 'Would move %s into a sibling recoverable backup after checking its socket is unused.\n' "$data_directory"
  else printf 'User data remains in place.\n'; fi
  printf 'Keychain credentials and macOS preferences remain unchanged.\n'
  exit 0
fi
if (( ! skip_build )); then NEKO_NATIVE_PREVIEW=0 bash "$repo_root/scripts/build-native.sh"; fi
validate_bundle "$bundle" || { printf 'Native bundle validation failed; installation untouched.\n' >&2; exit 1; }
[[ -w /Applications ]] || { printf '/Applications is not writable; no installation changes made.\n' >&2; exit 1; }
backup_root="$(mktemp -d /Applications/.neko-native-backup.XXXXXX)"
staged="$backup_root/new-Neko.app"
/usr/bin/ditto "$bundle" "$staged"
validate_bundle "$staged" || { printf 'Staged bundle failed verification; retained at %s\n' "$staged" >&2; exit 1; }
while read -r process_id; do
  [[ -n "$process_id" ]] || continue
  executable="$(/bin/ps -p "$process_id" -o comm= || true)"
  if is_installed_executable "$executable"; then /bin/kill -TERM "$process_id" 2>/dev/null || true; fi
done < <(installed_pids)
for ((attempt=0; attempt<50; attempt++)); do
  [[ -z "$(installed_pids)" ]] && break
  sleep 0.1
done
[[ -z "$(installed_pids)" ]] || { printf 'Installed Neko did not exit. No app/data replacement performed; staged bundle retained at %s\n' "$staged" >&2; exit 1; }
data_backup=''
app_moved=0
new_installed=0
rollback() {
  result=$?
  if (( result != 0 )); then
    if (( new_installed )); then mv "$application" "$backup_root/failed-install.app" || true; fi
    if (( app_moved )); then mv "$backup_root/previous-Neko.app" "$application" || true; fi
    if [[ -n "$data_backup" && -d "$data_backup/neko" && ! -e "$data_directory" ]]; then mv "$data_backup/neko" "$data_directory" || true; fi
    printf 'Installation failed. Recovery files: %s %s\n' "$backup_root" "$data_backup" >&2
  fi
}
trap rollback EXIT
if (( clean_data )) && [[ -e "$data_directory" ]]; then
  if [[ -S "$data_directory/neko.sock" ]] && [[ -n "$(/usr/sbin/lsof -t "$data_directory/neko.sock" 2>/dev/null || true)" ]]; then
    printf 'Another process holds the Neko data socket; refusing data replacement.\n' >&2; exit 1
  fi
  data_backup="$(mktemp -d "$HOME/Library/Application Support/.neko-native-data-backup.XXXXXX")"
  mv "$data_directory" "$data_backup/neko"
fi
if [[ -e "$application" ]]; then mv "$application" "$backup_root/previous-Neko.app"; app_moved=1; fi
mv "$staged" "$application"
new_installed=1
validate_bundle "$application"
# Launch Services can retain the moved bundle's identity and reopen a backup.
# Keep recovery bundles on disk, but make the installed app the launch target.
launch_services='/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister'
"$launch_services" -u "$bundle" || true
for previous_bundle in /Applications/.neko-native-backup.*/previous-Neko.app; do
  [[ -d "$previous_bundle" ]] || continue
  "$launch_services" -u "$previous_bundle" || true
done
"$launch_services" -f "$application"
printf 'Installed native /Applications/Neko.app. Previous bundle backup: %s\n' "$backup_root"
if [[ -n "$data_backup" ]]; then printf 'Previous Neko data backup: %s/neko\n' "$data_backup"; fi
printf 'Credentials and preferences preserved. Launch /Applications/Neko.app when ready.\n'
