#!/bin/bash
# Build a self-contained native client without modifying the installed app or user data.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
configuration="${NEKO_NATIVE_CONFIGURATION:-release}"
if [[ "$configuration" == debug ]]; then
  cargo build -p neko-daemon --bin neko-daemon
else
  cargo build --release -p neko-daemon --bin neko-daemon
fi
swift build --package-path native/NekoKit -c "$configuration"
bundle="$repo_root/target/native/Neko.app"
bundle_id=dev.neko.launcher
if [[ "${NEKO_NATIVE_PREVIEW:-0}" == 1 ]]; then
  bundle="$repo_root/target/native/Neko Native Preview.app"
  bundle_id=dev.neko.native-preview
fi
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp scripts/Neko-Info.plist "$bundle/Contents/Info.plist"
cp "native/NekoKit/.build/$configuration/NekoNative" "$bundle/Contents/MacOS/neko"
cp "target/$configuration/neko-daemon" "$bundle/Contents/MacOS/neko-daemon"
cp packaging/icon/neko.icns "$bundle/Contents/Resources/neko.icns"
shader_work="$(mktemp -d)"
xcrun -sdk macosx metal -c native/Shaders/Gem.metal -o "$shader_work/Gem.air"
xcrun -sdk macosx metallib "$shader_work/Gem.air" -o "$bundle/Contents/Resources/Neko.metallib"
/usr/libexec/PlistBuddy -c 'Add :CFBundleIconFile string neko.icns' "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :LSMinimumSystemVersion string 14.0' "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $bundle_id" "$bundle/Contents/Info.plist"
# Lets Settings → About check GitHub for newer commits and rebuild from this checkout.
if commit="$(git -C "$repo_root" rev-parse HEAD 2>/dev/null)"; then
  /usr/libexec/PlistBuddy -c "Add :NekoGitCommit string $commit" "$bundle/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Add :NekoSourcePath string $repo_root" "$bundle/Contents/Info.plist"
fi
identity="${NEKO_CODESIGN_IDENTITY:--}"
codesign --force --sign "$identity" --identifier "$bundle_id.daemon" --timestamp=none "$bundle/Contents/MacOS/neko-daemon"
codesign --force --sign "$identity" --identifier "$bundle_id" --timestamp=none "$bundle/Contents/MacOS/neko"
codesign --force --sign "$identity" --timestamp=none "$bundle"
codesign --verify --deep --strict "$bundle"
printf 'Native build: %s\n' "$bundle"
