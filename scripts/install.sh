#!/bin/bash
# One command from a fresh clone to a running Neko:
#
#   bash scripts/install.sh
#
# It checks what a build needs, prints the exact command for anything missing
# (and stops there, changing nothing), then builds Neko.app, installs it in
# /Applications and opens it. The first launch walks you through setup.
#
#   --check        only check the prerequisites
#   --build-only   build target/native/Neko.app but do not install or open it
#   --no-open      install but do not open
#
# The test suites are not run here (scripts/check.sh does that, for people
# changing Neko). Your data is never touched: it lives in
# ~/Library/Application Support/neko and survives reinstalls.
set -uo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root" || exit 1

check_only=0; build_only=0; open_app=1
for option in "$@"; do
  case "$option" in
    --check) check_only=1 ;;
    --build-only) build_only=1 ;;
    --no-open) open_app=0 ;;
    --help|-h) sed -n '2,16p' "$0"; exit 0 ;;
    *) printf 'Unknown option: %s (try --help)\n' "$option" >&2; exit 2 ;;
  esac
done

bold=$'\033[1m'; dim=$'\033[2m'; plain=$'\033[0m'
[[ -t 1 ]] || { bold=''; dim=''; plain=''; }
missing=0
say() { printf '%s\n' "$*"; }
ok() { printf '  ok       %s\n' "$*"; }
optional() { printf '  skipped  %s\n' "$*"; }
need() {
  # need "what" "why" "command..."
  missing=$((missing + 1))
  printf '  MISSING  %s\n' "$1"
  shift
  while (( $# )); do printf '           %s\n' "$1"; shift; done
}

# Rust's own installer puts cargo here; a fresh terminal has not read it yet.
[[ -f "${HOME:-/nonexistent}/.cargo/env" ]] && . "${HOME:-/nonexistent}/.cargo/env"

say "${bold}Neko: checking this Mac${plain}"

# --- macOS ---------------------------------------------------------------
if [[ "$(uname -s)" != Darwin ]]; then
  say "Neko is a macOS app; this is $(uname -s)."; exit 1
fi
major="$(sw_vers -productVersion | cut -d. -f1)"
if (( major >= 14 )); then ok "macOS $(sw_vers -productVersion)"
else need "macOS 14 or newer (this is $(sw_vers -productVersion))" "Update in System Settings > General > Software Update."; fi

# --- Xcode Command Line Tools / Swift ---------------------------------------
if xcode-select -p >/dev/null 2>&1 && command -v swift >/dev/null 2>&1 && xcrun --find clang >/dev/null 2>&1; then
  ok "Swift $(swift --version 2>&1 | sed -n 's/.*Swift version \([0-9.]*\).*/\1/p' | head -1) (Xcode or its Command Line Tools)"
else
  need "Xcode Command Line Tools (the Swift compiler)" "Run:  xcode-select --install" "then wait for the installer to finish and run this again."
fi

# --- Rust ---------------------------------------------------------------------
if command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
  rust_version="$(rustc --version | sed -n 's/^rustc \([0-9]*\.[0-9]*\).*/\1/p')"
  rust_minor="${rust_version#*.}"
  if [[ "${rust_version%%.*}" -gt 1 ]] || (( rust_minor >= 88 )); then ok "Rust $rust_version"
  else need "Rust 1.88 or newer (this is $rust_version)" "Run:  rustup update stable"; fi
else
  need "Rust" "Run:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" "then open a new Terminal window (or run:  . \"\$HOME/.cargo/env\") and run this again."
fi

# --- Node and pnpm ----------------------------------------------------------
if command -v node >/dev/null 2>&1; then
  node_major="$(node --version | sed -n 's/^v\([0-9]*\).*/\1/p')"
  if (( node_major >= 20 )); then ok "Node $(node --version)"
  else need "Node 20 or newer (this is $(node --version))" "Run:  brew install node   (or download it from https://nodejs.org)"; fi
else
  need "Node" "Run:  brew install node   (or download it from https://nodejs.org)" "Homebrew itself: https://brew.sh"
fi
if command -v pnpm >/dev/null 2>&1; then ok "pnpm $(pnpm --version)"
elif command -v corepack >/dev/null 2>&1; then
  need "pnpm" "Run:  corepack enable pnpm" "then run this again."
else
  need "pnpm" "Run:  npm install -g pnpm" "then run this again."
fi

# --- Optional -------------------------------------------------------------------
if command -v gh >/dev/null 2>&1; then
  if gh auth status >/dev/null 2>&1; then ok "gh (GitHub CLI), signed in: Neko can open pull requests"
  else optional "gh is installed but not signed in. Neko can open pull requests once you run:  gh auth login"; fi
else
  optional "gh (GitHub CLI). Only needed for pull requests:  brew install gh && gh auth login"
fi
agents=''
for agent in claude codex opencode; do command -v "$agent" >/dev/null 2>&1 && agents="$agents $agent"; done
if [[ -n "$agents" ]]; then ok "agent CLIs found:$agents (setup shows which are signed in)"
else optional "no agent CLI yet. Neko runs the ones you install (Claude Code, Codex, OpenCode); setup shows how."; fi

if (( missing )); then
  say ""
  say "${bold}$missing thing(s) to install first.${plain} Run the commands above, then:  bash scripts/install.sh"
  exit 1
fi
say ""
if (( check_only )); then say "Everything needed is here."; exit 0; fi

# --- Build ----------------------------------------------------------------------
say "${bold}Building Neko${plain} ${dim}(the first build takes 5-15 minutes; later ones are faster)${plain}"
if (( build_only )); then
  NEKO_NATIVE_PREVIEW=0 bash "$repo_root/scripts/build-native.sh" || { say "The build failed. The messages above say where."; exit 1; }
  say ""
  say "Built $repo_root/target/native/Neko.app (not installed: --build-only)."
  exit 0
fi
bash "$repo_root/scripts/install-native.sh" --skip-checks || { say "The install failed; nothing in /Applications was changed unless it says so above."; exit 1; }

say ""
say "${bold}Neko is installed.${plain}"
if (( open_app )); then
  open /Applications/Neko.app && say "Opened. Setup starts on first launch; it takes about two minutes and every step can be skipped."
else
  say "Open it with:  open /Applications/Neko.app"
fi
say "Update later with:  git pull && bash scripts/install.sh"
