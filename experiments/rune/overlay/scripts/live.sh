#!/bin/bash
# Runs the overlay app for a bounded time with stdin on a FIFO, sends it commands, and records
# window state and memory. Never shows the launcher panel unless a `show`/`toggle`/`bench panel`
# command is in the command list (the panel takes keys; run those only while the person is away).
#
# Usage: live.sh <seconds-max> <outfile> cmd [cmd...]      e.g. live.sh 30 out.txt status "640 380 Clicking Save"
# Commands starting with "sleep:" wait that many seconds; "winlist" and "footprint" record state.
set -u
BIN=${RUNE_OVERLAY_BIN:-${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}/release/rune-overlay}
WINLIST=${WINLIST:-/tmp/winlist}
MAX=$1; OUT=$2; shift 2
FIFO=$(mktemp -u "${TMPDIR:-/tmp}/rune-overlay-fifo.XXXXXX"); mkfifo "$FIFO"
sleep "$MAX" > "$FIFO" &       # keeps the write end open; also the hard time limit
HOLD=$!
: > "$OUT"
"$BIN" < "$FIFO" >> "$OUT" 2>&1 &   # append mode, so the script's own notes below are not overwritten
APP=$!
( sleep "$MAX"; kill "$APP" 2>/dev/null ) &
exec 3> "$FIFO"
sleep "${STARTUP:-3}"
for c in "$@"; do
  case "$c" in
    sleep:*) sleep "${c#sleep:}" ;;
    winlist) { echo "--- winlist $(date +%T)"; "$WINLIST" rune-overlay; } >> "$OUT" ;;
    footprint) { echo "--- footprint"; footprint -p "$APP" 2>&1 | head -12; ps -o rss=,vsz= -p "$APP"; } >> "$OUT" ;;
    shot:*)  # shot:<png>:<pointer|capsule> captures that one window only (never the whole screen)
      IFS=: read -r _ png which <<< "$c"
      id=$(grep -o "${which}_window=Some([0-9]*)" "$OUT" | tail -1 | grep -o '[0-9]*')
      [ -n "$id" ] && screencapture -x -o -l "$id" "$png" ;;
    *) echo "$c" >&3 ;;
  esac
done
echo quit >&3
sleep 0.5
kill "$APP" "$HOLD" 2>/dev/null
exec 3>&-
rm -f "$FIFO"
