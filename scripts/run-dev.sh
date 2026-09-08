#!/bin/zsh
# Build and run the development client with a stable macOS identity.
#
# Accessibility grants for an ad-hoc Mach-O are tied to its cdhash, which
# changes on every Rust rebuild. A real Apple Development signature gives
# System Settings a durable identity instead. The identity is intentionally
# supplied by the developer's keychain rather than committed to this repo.
set -euo pipefail

if [[ -z "${NEKO_CODESIGN_IDENTITY:-}" ]]; then
  print -u2 'Set NEKO_CODESIGN_IDENTITY to an Apple Development signing identity.'
  print -u2 'Find one with: security find-identity -v -p codesigning'
  exit 2
fi

repo_root=${0:A:h:h}
client_binary="$repo_root/target/debug/neko"

cd "$repo_root"
cargo build -p neko -p neko-daemon
codesign --force --sign "$NEKO_CODESIGN_IDENTITY" --identifier 'dev.neko.launcher' --timestamp=none "$client_binary"
exec "$client_binary"
