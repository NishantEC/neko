#!/bin/bash
# Build a self-contained native client without modifying the installed app or user data.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
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
# macOS remembers permissions against the signature. An ad-hoc signature is a
# per-build fingerprint, so every rebuild looked like a new app and asked again.
# A real certificate keeps the same identity across builds, so permissions stick.
identity="${NEKO_CODESIGN_IDENTITY:-}"
if [[ -z "$identity" ]]; then
  identities="$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/^ *[0-9]*) \([0-9A-F]\{40\}\) "\(Apple Development:.*\)"$/\1 \2/p')"
  # Prefer whichever certificate signed the installed app, so its permissions carry over.
  installed="$(codesign -dvv /Applications/Neko.app 2>&1 | sed -n 's/^Authority=\(Apple Development:.*\)$/\1/p' | head -1 || true)"
  if [[ -n "$installed" ]]; then identity="$(printf '%s\n' "$identities" | awk -v name="$installed" 'substr($0, 42) == name { print $1; exit }')"; fi
  if [[ -z "$identity" ]]; then identity="$(printf '%s\n' "$identities" | head -1 | cut -d' ' -f1)"; fi
  if [[ -z "$identity" ]]; then
    identity=-
    printf 'No Apple Development certificate found; signing ad-hoc. macOS will ask for permissions again after each rebuild.\n' >&2
  fi
fi
printf 'Signing with %s\n' "$identity"
codesign --force --sign "$identity" --identifier "$bundle_id.daemon" --timestamp=none "$bundle/Contents/MacOS/neko-daemon"
codesign --force --sign "$identity" --identifier "$bundle_id" --timestamp=none "$bundle/Contents/MacOS/neko"
codesign --force --sign "$identity" --timestamp=none "$bundle"
codesign --verify --deep --strict "$bundle"
printf 'Native build: %s\n' "$bundle"
