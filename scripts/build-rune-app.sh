#!/bin/bash
# Build the Rune client as the Mac app, `Neko.app`, the way build-native.sh builds the Swift one. It does not touch
# the installed app or any user data. Installed by scripts/install-native.sh when the Mac app is set to Rune
# (docs/knowledge/rune-as-the-app.md).
#
#   bash scripts/build-rune-app.sh      builds $CARGO_TARGET_DIR (default ./target)/native-rune/Neko.app
#
# The layout is the Swift bundle's: Contents/MacOS/neko is the Rune client (it is `--app` by itself when it runs
# from here), Contents/MacOS/neko-daemon the daemon built exactly as build-native.sh builds it, Contents/Resources has the
# web UI and the icon, and Info.plist is scripts/Neko-Info.plist plus the `neko://` scheme. Same bundle id and the same
# signing as the Swift bundle (scripts/lib/sign.sh), so macOS permissions granted to one carry over to the other.
#
# Rune is built only into its own cache folder ($HOME/Library/Caches/neko/rune-target), without incremental
# compilation, so a build never grows the daemon's target folder or the repository.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
. "$repo_root/scripts/lib/sign.sh"
# The daemon's Cargo output folder, as build-native.sh has it.
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
case "$target_dir" in /*) ;; *) target_dir="$repo_root/$target_dir" ;; esac
rune_target="$HOME/Library/Caches/neko/rune-target"
cargo build --release -p neko-daemon --bin neko-daemon
# Local fixes to Rune, when there are any, come from a patched checkout (apps/neko-rune/scripts/patched-rune.sh).
CARGO_TARGET_DIR="$rune_target" "$repo_root/apps/neko-rune/scripts/patched-rune.sh" --ensure
(cd apps/neko-rune && CARGO_TARGET_DIR="$rune_target" CARGO_INCREMENTAL=0 cargo build --release --bin neko-rune)
bundle="$target_dir/native-rune/Neko.app"
bundle_id=dev.neko.launcher
rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp scripts/Neko-Info.plist "$bundle/Contents/Info.plist"
cp "$rune_target/release/neko-rune" "$bundle/Contents/MacOS/neko"
cp "$target_dir/release/neko-daemon" "$bundle/Contents/MacOS/neko-daemon"
cp packaging/icon/neko.icns "$bundle/Contents/Resources/neko.icns"
# The web UI the daemon serves (Contents/Resources/web), built from web/ as build-native.sh does.
if [[ -f web/package.json ]]; then
  [[ -d web/node_modules ]] || pnpm --dir web install --frozen-lockfile
  pnpm --dir web build
  rm -rf "$bundle/Contents/Resources/web"
  cp -R web/dist "$bundle/Contents/Resources/web"
fi
plist="$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleIconFile string neko.icns' "$plist"
/usr/libexec/PlistBuddy -c 'Add :LSMinimumSystemVersion string 14.0' "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $bundle_id" "$plist"
# `neko://` links open this app (the Rune client routes them; docs/knowledge/rune-as-the-app.md).
/usr/libexec/PlistBuddy -c 'Add :CFBundleURLTypes array' "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleURLTypes:0 dict' "$plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleURLTypes:0:CFBundleURLName string $bundle_id" "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleURLTypes:0:CFBundleURLSchemes array' "$plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleURLTypes:0:CFBundleURLSchemes:0 string neko' "$plist"
# Which app this is: the daemon compares it with the choice in <data dir>/app-shell (a different choice means a reinstall).
/usr/libexec/PlistBuddy -c 'Add :NekoShell string rune' "$plist"
# Lets Settings → About check GitHub for newer commits and rebuild from this checkout.
if commit="$(git -C "$repo_root" rev-parse HEAD 2>/dev/null)"; then
  /usr/libexec/PlistBuddy -c "Add :NekoGitCommit string $commit" "$plist"
  /usr/libexec/PlistBuddy -c "Add :NekoSourcePath string $repo_root" "$plist"
fi
neko_codesign_bundle "$bundle" "$bundle_id"
printf 'Rune app build: %s\n' "$bundle"
