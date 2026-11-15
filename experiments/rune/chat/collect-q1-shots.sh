#!/bin/bash
# Downscale the question 1 screenshots from /tmp/rune-spike-shots into shots/.
set -eu
cd "$(dirname "$0")"
S=/tmp/rune-spike-shots
mkdir -p shots
while IFS=: read -r src dst; do
  sips -Z 1000 "$S/$src.png" --out "shots/$dst.png" >/dev/null
done <<'EOF'
child-final/01-composited:q1-child-composited
child-final/02-dropdown-open:q1-child-dropdown-hidden-behind-web
child-final/04-scrolled:q1-child-scrolled-clipped
child-final/05-resized:q1-child-resized
child-hide/02-dropdown-open:q1-child-hide-dropdown-above
panel/04-scrolled:q1-panel-overflows-window
under/02-dropdown-open:q1-under-web-not-visible
EOF
ls -la shots
