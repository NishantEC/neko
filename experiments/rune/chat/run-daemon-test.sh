#!/bin/bash
# Question 3: start the isolated test daemon (web_harness, data under /tmp, port 4749), seed it with
# the made-up snapshot web-verify uses, run the chat headless against it, and stop the daemon.
# Never touches the installed app or its data directory.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
target="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
data=/tmp/neko-rune-spike-data
port=4749
rm -rf "$data"; mkdir -p "$data"
node --input-type=module -e "
import { webSeed } from '$repo/scripts/fixtures/web-seed.mjs';
import { writeFileSync } from 'node:fs';
writeFileSync('$data-seed.json', JSON.stringify(webSeed()));
"
env -i PATH="$PATH" HOME="$data" NEKO_DATA_DIR="$data" NEKO_WEB_PORT=$port NEKO_WEB_SEED="$data-seed.json" \
  "$target/debug/web_harness" > "$data.log" 2>&1 &
pid=$!
for _ in $(seq 1 50); do nc -z 127.0.0.1 $port 2>/dev/null && break; sleep 0.2; done
echo "daemon up on $port (pid $pid)"
"${CHAT_BIN:-/tmp/chat-tooling}" daemon="$data/neko.sock" --snap "${1:-$here/shots/q3-real-daemon-tasks.png}" --size 1180x760 --scale 1 2>&1 | grep -E "daemon|frames"
# stop only the harness this script started
if ps -p $pid -o command= | grep -q web_harness; then kill $pid; fi
