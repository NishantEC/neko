#!/bin/zsh
# Build a fresh, signed Neko.app and replace both the bundle and Neko-owned
# local state. Accessibility approval is deliberately left to macOS.
set -euo pipefail

if [[ -z "${NEKO_CODESIGN_IDENTITY:-}" ]]; then
  print -u2 'Set NEKO_CODESIGN_IDENTITY to an Apple Development signing identity.'
  exit 2
fi

# Stop before building or touching an existing install when macOS cannot use
# the private key. `codesign` otherwise reports only errSecInternalComponent
# after a full release build, which is too late to explain the real remedy.
login_keychain="${HOME}/Library/Keychains/login.keychain-db"
if [[ -f "$login_keychain" ]] && ! security show-keychain-info "$login_keychain" >/dev/null 2>&1; then
  print -u2 'Your login keychain is locked. Unlock it in Keychain Access, then rerun this installer.'
  exit 3
fi

repo_root=${0:A:h:h}
build_directory="$repo_root/target/release"
application='/Applications/Neko.app'
support_directory="${HOME}/Library/Application Support"
data_directory="${support_directory}/neko"
staging_directory=$(mktemp -d "${TMPDIR:-/tmp}/neko-install.XXXXXX")
bundle="$staging_directory/Neko.app"
bundle_contents="$bundle/Contents"
bundle_macos="$bundle_contents/MacOS"

cleanup() {
  rm -rf -- "$staging_directory"
}
trap cleanup EXIT

[[ "$data_directory" == "${support_directory}/neko" ]] || {
  print -u2 'Refusing to clear an unexpected Neko data path.'
  exit 2
}
[[ "$application" == '/Applications/Neko.app' ]] || {
  print -u2 'Refusing to replace an unexpected application path.'
  exit 2
}

cd "$repo_root"
./scripts/setup-gpui-patch.sh
cargo build --release -p neko -p neko-daemon

mkdir -p "$bundle_macos"
cp "$repo_root/scripts/Neko-Info.plist" "$bundle_contents/Info.plist"
cp "$build_directory/neko" "$bundle_macos/neko"
cp "$build_directory/neko-daemon" "$bundle_macos/neko-daemon"
codesign --force --sign "$NEKO_CODESIGN_IDENTITY" --identifier 'dev.neko.launcher.daemon' --timestamp=none "$bundle_macos/neko-daemon"
codesign --force --sign "$NEKO_CODESIGN_IDENTITY" --identifier 'dev.neko.launcher' --timestamp=none "$bundle_macos/neko"
codesign --force --sign "$NEKO_CODESIGN_IDENTITY" --identifier 'dev.neko.launcher' --timestamp=none "$bundle"
codesign --verify --deep --strict "$bundle"

# Stop only the installed Neko processes before removing their exact files.
pkill -f '^/Applications/Neko\.app/Contents/MacOS/neko(-daemon)?( |$)' 2>/dev/null || true
for _ in {1..20}; do
  pgrep -f '^/Applications/Neko\.app/Contents/MacOS/neko(-daemon)?( |$)' >/dev/null || break
  sleep 0.1
done
if pgrep -f '^/Applications/Neko\.app/Contents/MacOS/neko(-daemon)?( |$)' >/dev/null; then
  print -u2 'Neko did not exit; refusing to replace its state.'
  exit 1
fi

# These service names are owned solely by Neko. Repeating the delete removes
# every matching account without exposing account names or secret contents.
for service in app.neko.mcp app.neko.mcp.oauth app.neko.linear; do
  while security delete-generic-password -s "$service" >/dev/null 2>&1; do :; done
done
rm -rf -- "$data_directory"
defaults delete dev.neko.launcher >/dev/null 2>&1 || true
rm -rf -- "$application"
ditto "$bundle" "$application"

print 'Installed a fresh /Applications/Neko.app with empty Neko state.'
print 'Accessibility approval was preserved; launch Neko to begin setup.'
