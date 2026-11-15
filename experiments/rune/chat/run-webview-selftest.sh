#!/bin/bash
# Run one webview self-test (child | panel | under, optionally "hide") with a 45 s hard limit,
# then keep its screenshots under /tmp/rune-spike-shots/<label>/.
set -u
label="$1"; shift
BIN="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}/release/webview"
rm -f /tmp/rune-spike-shots/0*.png
perl -e 'alarm 45; exec @ARGV' "$BIN" "$@" selftest 2>&1
mkdir -p "/tmp/rune-spike-shots/$label"
cp /tmp/rune-spike-shots/0*.png "/tmp/rune-spike-shots/$label/" 2>/dev/null
