# Which Mac app Neko installs: the Swift app (`swift`, the default) or the Rune
# app (`rune`). The choice is a file, `<data dir>/app-shell`, holding one of those
# words; `NEKO_SHELL` overrides it for a single run. A missing file, an empty
# one or anything else means `swift`. The Rust twin is
# `neko_core::self_update::app_shell`; keep them in step.
#
#   . "$repo_root/scripts/lib/shell.sh"
#   shell="$(neko_app_shell)"
#
# Not run on its own. `neko_app_shell [data dir]` prints `swift` or `rune`.
neko_app_shell() {
  local data_dir="${1:-${NEKO_DATA_DIR:-$HOME/Library/Application Support/neko}}" word
  for word in "${NEKO_SHELL:-}" "$(head -c 64 "$data_dir/app-shell" 2>/dev/null || true)"; do
    word="$(printf '%s' "$word" | tr -d '[:space:]' | tr '[:upper:]' '[:lower:]')"
    case "$word" in
      swift|rune) printf '%s\n' "$word"; return 0 ;;
    esac
  done
  printf 'swift\n'
}
