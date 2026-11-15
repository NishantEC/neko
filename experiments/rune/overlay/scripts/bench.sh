#!/bin/bash
# Measurement runs for REPORT.md. Waits for ~/.agents/bin/machine-ok to say OK before each run.
# The panel run takes keyboard focus while the panel is up (launcher_panel is a key-taking panel),
# so it only runs when the keyboard and mouse have been idle for IDLE_MIN seconds (default 180).
# Usage: bench.sh <outdir>
set -u
OUT=${1:?outdir}; mkdir -p "$OUT"
HERE=$(cd "$(dirname "$0")" && pwd)
IDLE_MIN=${IDLE_MIN:-180}
ok() { until ~/.agents/bin/machine-ok 2>/dev/null | head -1 | grep -q '^OK'; do sleep 15; done; }
idle() { ioreg -c IOHIDSystem | awk '/HIDIdleTime/ {printf "%d", $NF/1000000000; exit}'; }

for n in 1 2 3; do
  ok; STARTUP=8 "$HERE/live.sh" 80 "$OUT/overlay-run$n.txt" footprint "bench 20 overlay" sleep:24 footprint
done

# Memory with the pointer animating, then settled.
ok; STARTUP=8 "$HERE/live.sh" 80 "$OUT/memory-pointer.txt" footprint demo sleep:3 footprint sleep:12 footprint

# Launcher panel (key-taking), only while the person is away.
for n in 1 2 3; do
  ok
  if [ "$(idle)" -lt "$IDLE_MIN" ]; then echo "skip panel run $n: input idle $(idle)s < $IDLE_MIN" | tee -a "$OUT/panel-skipped.txt"; continue; fi
  STARTUP=8 "$HERE/live.sh" 80 "$OUT/panel-run$n.txt" footprint "bench 20 panel" sleep:26 footprint
done
echo done > "$OUT/DONE"
