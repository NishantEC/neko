#!/bin/bash
# Builds Neko.app: the app and the daemon beside it, signed ad hoc, with the version and commit in Info.plist.
#
#   scripts/build-app.sh [OUT_DIR]   (default: $NEKO_TARGET/bundle; prints the bundle's path)
#
# Nothing is installed: scripts/install.sh does that.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
target="${NEKO_TARGET:-$HOME/Library/Caches/neko/target}"
out="${1:-$target/bundle}"
version="$(sed -nE 's/^version = "([^"]+)"/\1/p' "$root/apps/neko/Cargo.toml" | head -n 1)"
commit="$(git -C "$root" rev-parse HEAD 2>/dev/null || echo unknown)"
if [[ -n "$(git -C "$root" status --porcelain 2>/dev/null)" ]]; then commit="$commit-dirty"; fi

CARGO_TARGET_DIR="$target/core" cargo build --release --manifest-path "$root/Cargo.toml" -p neko-daemon
CARGO_TARGET_DIR="$target/app" cargo build --release --manifest-path "$root/apps/neko/Cargo.toml"

app="$out/Neko.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$target/app/release/neko" "$app/Contents/MacOS/neko"
cp "$target/core/release/neko-daemon" "$app/Contents/MacOS/neko-daemon"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.neko.app</string>
  <key>CFBundleName</key><string>Neko</string>
  <key>CFBundleDisplayName</key><string>Neko</string>
  <key>CFBundleExecutable</key><string>neko</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>NekoGitCommit</key><string>$commit</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSHumanReadableCopyright</key><string>Copyright © 2026 The Neko authors. Released under the MIT License.</string>
</dict>
</plist>
PLIST
cp "$root/LICENSE" "$root/NOTICE" "$app/Contents/Resources/"
cp -R "$root/THIRD_PARTY_LICENSES" "$app/Contents/Resources/"
plutil -lint "$app/Contents/Info.plist" >/dev/null
# Ad hoc: the daemon first, then the bundle around it.
codesign --force --sign - --timestamp=none "$app/Contents/MacOS/neko-daemon"
codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict "$app"
echo "$app"
