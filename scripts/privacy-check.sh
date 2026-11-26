#!/bin/bash
# Fails when the repository would publish something personal or work-related.
#
#   scripts/privacy-check.sh            scan every tracked text file
#   scripts/privacy-check.sh --staged   scan only the lines a commit adds (the pre-commit hook)
#
# Two layers:
#  1. Generic: a real home folder (/Users/<name>/ other than placeholders) and
#     an email address that is not a placeholder.
#  2. Private: one extended regex per line from ~/.config/neko/private-patterns.txt
#     (override with NEKO_PRIVATE_PATTERNS). That file is the owner's list of names,
#     employers, clients, handles and trackers. It lives outside the repo and must
#     never be committed.
# Placeholders to write instead: /Users/example, Acme, Client A, ENG-123, Asha,
# alex, acme-api. See docs/knowledge/keeping-neko-public.md.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

mode=tree
[ "${1:-}" = "--staged" ] && mode=staged

# Lines to inspect, as "path:text". Lock files and binaries are skipped.
lines() {
  { if [ "$mode" = staged ]; then
    git diff --cached -U0 --diff-filter=ACMR --no-color -- . ':!*.lock' ':!pnpm-lock.yaml' \
      | awk '/^\+\+\+ b\//{f=substr($0,7);next} /^\+/ && !/^\+\+\+/{print f":"substr($0,2)}'
  else
    git grep -I -n -E '' -- . ':!*.lock' ':!pnpm-lock.yaml' | sed -E 's/^([^:]+):[0-9]+:/\1:/'
  fi; } | grep -v 'privacy-ok'
}

fail=0
report() { # label, regex, extra filter regex to drop
  local out
  out=$(lines | grep -E -- "$2" | grep -v -E -- "${3:-^$}" | head -20)
  if [ -n "$out" ]; then
    echo "privacy-check: $1" >&2
    echo "$out" | cut -c1-200 >&2
    fail=1
  fi
}

# Home folders other than placeholders.
report "a real home folder" '/Users/[A-Za-z0-9._-]+' \
  '/Users/(example|x|a|n|me|you|user|name|test|testuser|someone|shared|Shared|\$|<|\{|\*)|/Users/\.|/Users/[A-Za-z]*\)'

# Email addresses other than placeholders and git remotes.
report "an email address" '[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+\.[A-Za-z]{2,}' \
  '@example\.|\.invalid|noreply|git@github|@localhost|@types/|@[a-z-]+/|@[0-9]|token@|user@host|user@|you@|name@'

# The owner's private list.
patterns="${NEKO_PRIVATE_PATTERNS:-$HOME/.config/neko/private-patterns.txt}"
if [ -f "$patterns" ]; then
  grep -v -E '^\s*(#|$)' "$patterns" > /tmp/neko-private-patterns.$$ || true
  if [ -s /tmp/neko-private-patterns.$$ ]; then
    out=$(lines | grep -i -E -f /tmp/neko-private-patterns.$$ | head -20)
    if [ -n "$out" ]; then
      echo "privacy-check: matches your private pattern list" >&2
      echo "$out" | cut -c1-200 >&2
      fail=1
    fi
  fi
  rm -f /tmp/neko-private-patterns.$$
fi

if [ "$fail" -ne 0 ]; then
  echo "privacy-check: failed. Replace with placeholders (docs/knowledge/keeping-neko-public.md)." >&2
  exit 1
fi
[ "$mode" = tree ] && echo "privacy-check: clean"
exit 0
