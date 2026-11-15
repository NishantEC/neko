#!/bin/bash
# usage: timed-build.sh <label> <cargo args...>   Prints wall seconds for the build.
set -u
cd "$(dirname "$0")"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"
label="$1"; shift
start=$(date +%s)
cargo build "$@" 2>&1 | tail -2
end=$(date +%s)
echo "BUILD $label: $((end - start)) s (cargo build $*)"
