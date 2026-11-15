#!/bin/bash
# Real-window bench of the chat app, built with the perf profile (non-incremental, no tooling).
# usage: run-bench.sh <name> <app flags...>   e.g. run-bench.sh bench bench name=bench
#        run-bench.sh leak-clean leak fast=12 name=leak-clean
# Writes results/<name>.json (the app does) and prints the JSON. Hard limit 600 s.
set -u
cd "$(dirname "$0")"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
BIN="$CARGO_TARGET_DIR/perf/chat"
[ -x "$BIN" ] || cargo build --profile perf --bin chat
perl -e 'alarm 600; exec @ARGV' "$BIN" "$@"
