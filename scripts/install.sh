#!/bin/bash
# Runs the gate, builds Neko.app and puts it in /Applications. Nothing here updates itself.
#
#   scripts/install.sh                 the gate, then build and install
#   scripts/install.sh --skip-checks   build and install without the gate (say why when you use it)
#
# An installed Neko that is running is not stopped: quit it first, then run this again.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
if [[ "${1:-}" != "--skip-checks" ]]; then
  "$root/scripts/check.sh"
fi
if pgrep -f '/Applications/Neko.app/Contents/MacOS/neko$' >/dev/null; then
  echo "Neko is running. Quit it (Neko ▸ Quit Neko) and run this again." >&2
  exit 1
fi
app="$("$root/scripts/build-app.sh" | tail -n 1)"
rm -rf /Applications/Neko.app
ditto "$app" /Applications/Neko.app
echo "Installed /Applications/Neko.app. Open it from Launchpad, Spotlight or: open -a Neko"
