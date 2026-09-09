#!/bin/zsh
# Build and run the development client with a stable macOS identity.
#
# Accessibility grants apply to an app bundle, not a bare Unix executable.
# The bundle built here has a stable identifier and Apple Development
# signature, so System Settings can retain its authorization across rebuilds.
# The identity is intentionally supplied by the developer's keychain rather
# than committed to this repo.
set -euo pipefail

if [[ -z "${NEKO_CODESIGN_IDENTITY:-}" ]]; then
  print -u2 'Set NEKO_CODESIGN_IDENTITY to an Apple Development signing identity.'
  print -u2 'Find one with: security find-identity -v -p codesigning'
  exit 2
fi

repo_root=${0:A:h:h}
build_directory="$repo_root/target/debug"
bundle="$build_directory/Neko.app"
bundle_contents="$bundle/Contents"
bundle_macos="$bundle_contents/MacOS"
client_binary="$bundle_macos/neko"
daemon_binary="$bundle_macos/neko-daemon"

cd "$repo_root"
cargo build -p neko -p neko-daemon
mkdir -p "$bundle_macos"
cp "$repo_root/scripts/Neko-Info.plist" "$bundle_contents/Info.plist"
cp "$build_directory/neko" "$client_binary"
cp "$build_directory/neko-daemon" "$daemon_binary"

# The client launches its daemon from the sibling binary, so both must travel
# inside the app bundle and carry a valid development signature.
codesign --force --sign "$NEKO_CODESIGN_IDENTITY" --identifier 'dev.neko.launcher.daemon' --timestamp=none "$daemon_binary"
codesign --force --sign "$NEKO_CODESIGN_IDENTITY" --identifier 'dev.neko.launcher' --timestamp=none "$client_binary"
codesign --force --sign "$NEKO_CODESIGN_IDENTITY" --identifier 'dev.neko.launcher' --timestamp=none "$bundle"

# Execute through LaunchServices rather than invoking Contents/MacOS/neko
# directly. Accessibility authorization is attached to the application process
# that LaunchServices creates for this bundle, not to an arbitrary child Mach-O.
exec open -W "$bundle"
