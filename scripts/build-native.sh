#!/bin/bash
# Build a self-contained native client without modifying the installed app or user data.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
. "$repo_root/scripts/lib/sign.sh"
configuration="${NEKO_NATIVE_CONFIGURATION:-release}"
# Cargo's output folder: $CARGO_TARGET_DIR when set (the updater builds into the
# user's cache folder, not inside its clone), else ./target.
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
case "$target_dir" in /*) ;; *) target_dir="$repo_root/$target_dir" ;; esac
if [[ "$configuration" == debug ]]; then
  cargo build -p neko-daemon --bin neko-daemon
else
  cargo build --release -p neko-daemon --bin neko-daemon
fi
swift build --package-path native/NekoKit -c "$configuration"
bundle="$target_dir/native/Neko.app"
bundle_id=dev.neko.launcher
if [[ "${NEKO_NATIVE_PREVIEW:-0}" == 1 ]]; then
  bundle="$target_dir/native/Neko Native Preview.app"
  bundle_id=dev.neko.native-preview
fi
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp scripts/Neko-Info.plist "$bundle/Contents/Info.plist"
cp "native/NekoKit/.build/$configuration/NekoNative" "$bundle/Contents/MacOS/neko"
cp "$target_dir/$configuration/neko-daemon" "$bundle/Contents/MacOS/neko-daemon"
cp packaging/icon/neko.icns "$bundle/Contents/Resources/neko.icns"
# The web UI the daemon serves (Contents/Resources/web). Built from web/
# when its sources are present; an older build is replaced, never mixed.
if [[ -f web/package.json ]]; then
  [[ -d web/node_modules ]] || pnpm --dir web install --frozen-lockfile
  pnpm --dir web build
  rm -rf "$bundle/Contents/Resources/web"
  cp -R web/dist "$bundle/Contents/Resources/web"
fi
# The Metal shader only decorates the Design lab; the app runs without it
# (GemShaders.library is nil). Command Line Tools alone have no Metal compiler,
# so a Mac without full Xcode builds without it rather than failing.
if xcrun -sdk macosx --find metal >/dev/null 2>&1; then
  shader_work="$(mktemp -d)"
  xcrun -sdk macosx metal -c native/Shaders/Gem.metal -o "$shader_work/Gem.air"
  xcrun -sdk macosx metallib "$shader_work/Gem.air" -o "$bundle/Contents/Resources/Neko.metallib"
else
  printf 'No Metal compiler (it ships with Xcode, not the Command Line Tools); building without the optional shader.\n' >&2
fi
/usr/libexec/PlistBuddy -c 'Add :CFBundleIconFile string neko.icns' "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :LSMinimumSystemVersion string 14.0' "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $bundle_id" "$bundle/Contents/Info.plist"
# Lets Settings → About check GitHub for newer commits and rebuild from this checkout.
if commit="$(git -C "$repo_root" rev-parse HEAD 2>/dev/null)"; then
  /usr/libexec/PlistBuddy -c "Add :NekoGitCommit string $commit" "$bundle/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Add :NekoSourcePath string $repo_root" "$bundle/Contents/Info.plist"
fi
neko_codesign_bundle "$bundle" "$bundle_id"
printf 'Native build: %s\n' "$bundle"
